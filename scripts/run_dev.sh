#!/usr/bin/env bash
# ONE-SHIP development launcher (spec 12 §7.3). Each component runs as a transient systemd
# user unit, without a terminal; logs go to the journal.
#
#   run_dev.sh [all]                        restart every component
#   run_dev.sh daemon                       build, restart the daemon and its source watcher
#   run_dev.sh gui                          focus the interface, or launch it (and Vite)
#   run_dev.sh stop <daemon|gui|all>        stop components (the daemon stops its supervised processes first)
#   run_dev.sh status                       print the state of every unit as one JSON line
#   run_dev.sh logs <daemon|build|watch|vite|gui>   follow a unit's journal in a terminal
#   run_dev.sh rebuild-daemon               build; restart the daemon only if the build succeeds (watcher)
#   run_dev.sh export-bindings              run ts-rs export_bindings tests (when #[ts(export)] types change)
set -u

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SELF="$PROJECT_ROOT/scripts/run_dev.sh"

DAEMON_UNIT="oneship-dev-daemon"
BUILD_UNIT="oneship-dev-daemon-build"
WATCH_UNIT="oneship-dev-daemon-watch"
VITE_UNIT="oneship-dev-vite"
GUI_UNIT="oneship-dev-gui"

DAEMON_BIN="$PROJECT_ROOT/src-tauri/target/debug/oneship-daemon"
GUI_PROCESS_NAME="one-ship"
BUILD_PROGRESS_FILE="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/oneship-dev-build.json"
GUI_PROGRESS_FILE="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/oneship-dev-gui-build.json"

# The daemon stops its supervised processes one by one, each within `shutdown_grace_period_ms`;
# systemd sends SIGKILL to whatever is left in its cgroup after this timeout.
DAEMON_STOP_TIMEOUT_SECONDS=30
WATCH_DEBOUNCE_MS=500
FAILED_LOG_TAIL_LINES_COUNT=15

# Units run with the caller's PATH (cargo, the mise-managed node) in the project root.
run_unit() {
    local unit="$1"
    shift
    systemctl --user reset-failed "$unit" 2>/dev/null
    systemd-run --user --quiet --unit="$unit" \
        --working-directory="$PROJECT_ROOT" --setenv=PATH="$PATH" "$@"
}

unit_active() {
    systemctl --user is-active --quiet "$1"
}

stop_unit() {
    unit_active "$1" || return 0
    echo "Stopping $1..."
    systemctl --user stop "$1"
}

# Builds both Rust binaries and regenerates `ts-rs` TypeScript bindings in a single serialized
# oneshot unit so the daemon, interface, and `src/types/generated/` never contend on `target/` or
# drift out of sync. A failed build stays queryable (`failed`) until the next build.
build_daemon() {
    stop_unit "$BUILD_UNIT"
    echo "Building daemon and interface binary..."
    run_unit "$BUILD_UNIT" --wait --service-type=oneshot \
        --setenv=TS_RS_EXPORT_DIR="$PROJECT_ROOT/src/types/generated" \
        --setenv=ONESHIP_BUILD_PROGRESS_FILE="$BUILD_PROGRESS_FILE" \
        --setenv=CARGO_TERM_PROGRESS_WHEN=always \
        --setenv=CARGO_TERM_PROGRESS_WIDTH=40 \
        "$SELF" _build-daemon-inner
}

run_cargo_with_progress() {
    local pf="$1"
    local cmd="$2"
    local start_s
    start_s="$(date +%s)"
    export ONESHIP_PROGRESS_PF="$pf"
    export ONESHIP_PROGRESS_START="$start_s"

    write_progress() {
        local tmp="${pf}.tmp"
        printf "%s\n" "$1" > "$tmp" && mv -f "$tmp" "$pf"
    }
    finish_progress() {
        local rc=$?
        local dur=$(( $(date +%s) - start_s ))
        local phase="idle"
        (( rc != 0 )) && phase="failed"
        write_progress "{\"phase\":\"${phase}\",\"completed_count\":null,\"total_count\":null,\"percent\":null,\"current_item\":null,\"started_at_seconds\":${start_s},\"last_duration_seconds\":${dur}}"
        return $rc
    }
    trap finish_progress EXIT

    if command -v mold >/dev/null 2>&1; then
        export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-fuse-ld=mold"
    fi

    write_progress "{\"phase\":\"cargo_build\",\"completed_count\":0,\"total_count\":0,\"percent\":0,\"current_item\":\"preparing\",\"started_at_seconds\":${start_s},\"last_duration_seconds\":null}"
    set -o pipefail
    script -qefc "$cmd" /dev/null | perl -ne '
        BEGIN { $| = 1; $last = ""; $pf = $ENV{ONESHIP_PROGRESS_PF}; $st = $ENV{ONESHIP_PROGRESS_START}; }
        for my $seg (split /\r/) {
            my $clean = $seg;
            $clean =~ s/\e\[[0-9;?]*[A-Za-z]//g;
            if ($clean =~ /Building\s+\[[^\]]*\]\s+(\d+)\/(\d+)(?::\s*(.+))?/) {
                my ($done, $total, $item) = ($1 + 0, $2 + 0, defined($3) ? $3 : "linking");
                $item =~ s/^\s+|\s+$//g;
                $item =~ s/["\\]/_/g;
                $item = "linking" if $item eq "";
                my $pct = $total > 0 ? int(($done * 100) / $total) : 0;
                my $key = "$done/$total:$item";
                if ($key ne $last) {
                    $last = $key;
                    if (open my $fh, ">", "$pf.tmp") {
                        print $fh "{\"phase\":\"cargo_build\",\"completed_count\":$done,\"total_count\":$total,\"percent\":$pct,\"current_item\":\"$item\",\"started_at_seconds\":$st,\"last_duration_seconds\":null}\n";
                        close $fh;
                        rename "$pf.tmp", $pf;
                    }
                }
            } elsif ($clean =~ /Running\s+`target\//) {
                if (open my $fh, ">", "$pf.tmp") {
                    print $fh "{\"phase\":\"running\",\"completed_count\":null,\"total_count\":null,\"percent\":100,\"current_item\":\"one-ship\",\"started_at_seconds\":$st,\"last_duration_seconds\":null}\n";
                    close $fh;
                    rename "$pf.tmp", $pf;
                }
            }
            if ($clean =~ /\S/) {
                print "$clean\n";
            }
        }
    '
}

export_bindings() {
    if unit_active "$BUILD_UNIT"; then
        wait_for_build_unit || exit 1
    fi
    echo "Exporting ts-rs TypeScript bindings..."
    # The manifest lives in src-tauri, but Cargo only discovers src-tauri/.cargo/config.toml
    # when invoked from that directory. Pass the destination explicitly so the script works
    # from the repository root (and never falls back to src-tauri/bindings).
    TS_RS_EXPORT_DIR="$PROJECT_ROOT/src/types/generated" \
        cargo test --lib export_bindings --manifest-path "$PROJECT_ROOT/src-tauri/Cargo.toml"
}

wait_for_build_unit() {
    local max_wait_steps=600
    local step=0
    while unit_active "$BUILD_UNIT"; do
        if (( step >= max_wait_steps )); then
            echo "run_dev.sh: timed out waiting for $BUILD_UNIT" >&2
            return 1
        fi
        sleep 0.5
        step=$((step + 1))
    done
}

wait_for_unit_listening() {
    local unit="$1"
    local label="$2"
    local max_steps=60
    local step=0
    while (( step < max_steps )); do
        local pid
        pid="$(systemctl --user show "$unit" -p MainPID --value 2>/dev/null || echo 0)"
        if [[ ${pid:-0} -gt 0 ]] && [[ -n "$(listening_ports "$pid")" ]]; then
            return 0
        fi
        if ! unit_active "$unit"; then
            echo "run_dev.sh: $label exited before listening" >&2
            return 1
        fi
        sleep 0.25
        step=$((step + 1))
    done
    echo "run_dev.sh: timed out waiting for $label to listen" >&2
    return 1
}

# Restarts the daemon unit on the binary just built. KillMode=mixed sends SIGTERM to the
# daemon only, so it stops its supervised processes itself before the timeout.
restart_daemon_unit() {
    stop_unit "$DAEMON_UNIT"
    echo "Starting daemon..."
    run_unit "$DAEMON_UNIT" --property=KillMode=mixed \
        --property=TimeoutStopSec="$DAEMON_STOP_TIMEOUT_SECONDS" \
        --setenv=RUST_LOG=debug "$DAEMON_BIN"
    wait_for_unit_listening "$DAEMON_UNIT" "daemon" || true
}

start_watch() {
    unit_active "$WATCH_UNIT" && return 0
    echo "Starting daemon source watcher..."
    run_unit "$WATCH_UNIT" watchexec --postpone --on-busy-update=queue \
        --debounce="${WATCH_DEBOUNCE_MS}ms" \
        -w src-tauri/src -w src-tauri/Cargo.toml -w src-tauri/build.rs -e rs,toml \
        -n -- "$SELF" rebuild-daemon
}

require_watchexec() {
    command -v watchexec >/dev/null && return 0
    echo "run_dev.sh: watchexec is not installed (sudo pacman -S watchexec)" >&2
    exit 1
}

start_daemon() {
    require_watchexec
    build_daemon || { echo "run_dev.sh: daemon build failed; see: run_dev.sh logs build" >&2; exit 1; }
    restart_daemon_unit
    start_watch
}

stop_daemon() {
    stop_unit "$WATCH_UNIT"
    stop_unit "$BUILD_UNIT"
    stop_unit "$DAEMON_UNIT"
}

start_vite() {
    if unit_active "$VITE_UNIT"; then
        wait_for_unit_listening "$VITE_UNIT" "Vite" || true
        return 0
    fi
    echo "Starting Vite..."
    run_unit "$VITE_UNIT" npm run dev
    wait_for_unit_listening "$VITE_UNIT" "Vite" || true
}

start_gui() {
    echo "Starting interface..."
    # Accelerated compositing is mandatory for CSS backdrop-filter (Spec 13 §3.9) and is
    # supported by this machine's Intel Iris Xe + Mesa stack. WEBKIT_DISABLE_DMABUF_RENDERER
    # empties WebKitGTK's renderer transport set and forces the non-composited software path,
    # which silently disables every backdrop blur in the interface. If a DMA-BUF regression
    # appears, prefer WEBKIT_DMABUF_RENDERER_FORCE_SHM=1 (keeps compositing) over re-adding
    # the disable flag.
    run_unit "$GUI_UNIT" \
        --setenv=ONESHIP_GUI_PROGRESS_FILE="$GUI_PROGRESS_FILE" \
        "$SELF" _start-gui-inner
}

# Focuses a running interface; otherwise ensures daemon/vite are ready (without fighting an active
# Cargo build) and launches the interface.
open_gui() {
    local gui_pid
    gui_pid="$(pgrep -x "$GUI_PROCESS_NAME" | head -n 1)"
    if [[ -n $gui_pid ]]; then
        hyprctl dispatch focuswindow "pid:$gui_pid" >/dev/null
        return 0
    fi
    if unit_active "$GUI_UNIT"; then
        echo "Interface is still starting."
        return 0
    fi
    start_vite
    if unit_active "$BUILD_UNIT"; then
        wait_for_build_unit || exit 1
    fi
    if ! unit_active "$DAEMON_UNIT"; then
        start_daemon
    fi
    start_gui
}

stop_gui() {
    stop_unit "$GUI_UNIT"
}

# Space-separated TCP ports the given PID listens on.
listening_ports() {
    ss -ltnpH | grep -F "pid=$1," | awk '{ sub(/.*:/, "", $4); print $4 }' | sort -un | tr '\n' ' '
}

# One unit as JSON: state, result and main PID; a failed unit adds the tail of its last run's journal.
unit_json() {
    local unit="$1" active_state="" sub_state="" result="" main_pid="0" invocation_id="" log_tail=""
    local key value
    while IFS='=' read -r key value; do
        case "$key" in
            ActiveState) active_state="$value" ;;
            SubState) sub_state="$value" ;;
            Result) result="$value" ;;
            MainPID) main_pid="$value" ;;
            InvocationID) invocation_id="$value" ;;
        esac
    done < <(systemctl --user show "$unit" -p ActiveState -p SubState -p Result -p MainPID -p InvocationID)
    if [[ $active_state == "failed" && -n $invocation_id ]]; then
        log_tail="$(journalctl --user "_SYSTEMD_INVOCATION_ID=$invocation_id" -n "$FAILED_LOG_TAIL_LINES_COUNT" -o cat --no-pager 2>/dev/null)"
    fi
    jq -cn --arg active_state "$active_state" --arg sub_state "$sub_state" --arg result "$result" \
        --argjson main_pid "${main_pid:-0}" --arg log_tail "$log_tail" '{
            active_state: $active_state,
            sub_state: $sub_state,
            result: $result,
            main_pid: (if $main_pid > 0 then $main_pid else null end),
            log_tail: (if $log_tail == "" then null else ($log_tail | split("\n")) end)
        }'
}

print_status() {
    local daemon_pid vite_pid gui_pid ports="" vite_ports="" build_progress="null" gui_progress="null"
    daemon_pid="$(systemctl --user show "$DAEMON_UNIT" -p MainPID --value)"
    [[ ${daemon_pid:-0} -gt 0 ]] && ports="$(listening_ports "$daemon_pid")"
    vite_pid="$(systemctl --user show "$VITE_UNIT" -p MainPID --value)"
    [[ ${vite_pid:-0} -gt 0 ]] && vite_ports="$(listening_ports "$vite_pid")"
    gui_pid="$(pgrep -x "$GUI_PROCESS_NAME" | head -n 1)"
    if [[ -r $BUILD_PROGRESS_FILE ]]; then
        build_progress="$(jq -c . "$BUILD_PROGRESS_FILE" 2>/dev/null || echo null)"
    fi
    if [[ -r $GUI_PROGRESS_FILE ]]; then
        gui_progress="$(jq -c . "$GUI_PROGRESS_FILE" 2>/dev/null || echo null)"
    fi
    jq -cn \
        --argjson daemon "$(unit_json "$DAEMON_UNIT")" \
        --argjson daemon_build "$(unit_json "$BUILD_UNIT")" \
        --argjson daemon_watch "$(unit_json "$WATCH_UNIT")" \
        --argjson vite "$(unit_json "$VITE_UNIT")" \
        --argjson gui "$(unit_json "$GUI_UNIT")" \
        --argjson build_progress "$build_progress" \
        --argjson gui_progress "$gui_progress" \
        --arg ports "$ports" --arg vite_ports "$vite_ports" --arg gui_pid "$gui_pid" '{
            units: { daemon: $daemon, daemon_build: $daemon_build, daemon_watch: $daemon_watch, vite: $vite, gui: $gui },
            daemon_listening_ports: ($ports | split(" ") | map(select(. != "") | tonumber)),
            vite_listening_ports: ($vite_ports | split(" ") | map(select(. != "") | tonumber)),
            gui_window_pid: (if $gui_pid == "" then null else ($gui_pid | tonumber) end),
            build_progress: $build_progress,
            gui_progress: $gui_progress
        }'
}

follow_logs() {
    local unit
    case "${1:-}" in
        daemon) unit="$DAEMON_UNIT" ;;
        build) unit="$BUILD_UNIT" ;;
        watch) unit="$WATCH_UNIT" ;;
        vite) unit="$VITE_UNIT" ;;
        gui) unit="$GUI_UNIT" ;;
        *) usage ;;
    esac
    setsid -f ghostty --title="ONE-SHIP logs: $1" -e journalctl --user -u "$unit" -f -n 200 >/dev/null 2>&1
}

cleanup_target_artifacts() {
    local deps_dir="$PROJECT_ROOT/src-tauri/target/debug/deps"
    if [[ -d $deps_dir ]]; then
        find "$deps_dir" -maxdepth 1 -name '*.tmp*' -delete 2>/dev/null || true
    fi
    local target_dir="$PROJECT_ROOT/src-tauri/target"
    local stamp_file="$target_dir/.last_sweep"
    if command -v cargo-sweep >/dev/null 2>&1 && [[ -f "$target_dir/CACHEDIR.TAG" ]]; then
        if [[ ! -f $stamp_file ]] || [[ -n "$(find "$stamp_file" -mmin +1440 2>/dev/null)" ]]; then
            cargo sweep --time 7 "$PROJECT_ROOT/src-tauri" || true
            touch "$stamp_file" 2>/dev/null || true
        fi
    fi
}

usage() {
    sed -n '2,13p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//' >&2
    exit 2
}

case "${1:-all}" in
    all)
        stop_gui
        stop_daemon
        stop_unit "$VITE_UNIT"
        start_vite
        start_daemon
        start_gui
        ;;
    daemon)
        start_daemon
        ;;
    rebuild-daemon)
        build_daemon || { echo "Build failed; the running daemon and interface are kept." >&2; exit 1; }
        gui_was_active=0
        if unit_active "$GUI_UNIT" || [[ -n "$(pgrep -x "$GUI_PROCESS_NAME" | head -n 1)" ]]; then
            gui_was_active=1
        fi
        restart_daemon_unit
        if (( gui_was_active == 1 )); then
            stop_gui
            start_vite
            start_gui
        fi
        ;;
    export-bindings)
        export_bindings
        ;;
    _build-daemon-inner)
        run_cargo_with_progress "$BUILD_PROGRESS_FILE" "cargo build --no-default-features --bin oneship-daemon --bin one-ship --manifest-path src-tauri/Cargo.toml" &&
            cleanup_target_artifacts
        ;;
    _start-gui-inner)
        run_cargo_with_progress "$GUI_PROGRESS_FILE" "npm run tauri dev -- --no-watch"
        ;;
    gui)
        open_gui
        ;;
    stop)
        case "${2:-}" in
            daemon) stop_daemon ;;
            gui) stop_gui ;;
            all) stop_gui; stop_daemon; stop_unit "$VITE_UNIT" ;;
            *) usage ;;
        esac
        ;;
    status)
        print_status
        ;;
    logs)
        follow_logs "${2:-}"
        ;;
    *)
        usage
        ;;
esac
