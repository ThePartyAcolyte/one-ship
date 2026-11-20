#!/usr/bin/env bash
# ONE-SHIP Interactive Linux Installer & Feature Wizard (Spec 41 Phase 5)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_REPOS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

PREFIX="${ONESHIP_PREFIX:-$HOME/.local}"
REPOS_DIR="${ONESHIP_REPOS_DIR:-$DEFAULT_REPOS_DIR}"
PROFILE=""
FEATURES_CSV=""
SKIP_DEPS=0
ASSUME_YES=0
DRY_RUN=0
UNINSTALL=0

# Feature flags (0/1)
FEAT_CORE=1
FEAT_REA=0
FEAT_ELECTRONICS=0
FEAT_DESIGN=0
FEAT_PCB=0
FEAT_CAD=0
FEAT_VOICE=0
FEAT_OMARCHY=0

# Detected environment globals
DISTRO_ID="unknown"
DISTRO_NAME="Linux"
PKG_MANAGER=""
AUR_HELPER=""
IS_OMARCHY=0
IS_HYPRLAND=0
SESSION_TYPE="${XDG_SESSION_TYPE:-unknown}"
ELEVATE_CMD=""

DESIGMA_REPO_URL="https://github.com/ThePartyAcolyte/desigma.git"
DESIGMA_BRANCH="oneship"
PCBCRAFT_REPO_URL="https://github.com/tangency-labs/pcbcraft.git"
PCBCRAFT_BRANCH="main"
OPENDICTATE_REPO_URL="https://github.com/ThePartyAcolyte/opendictate.git"
SOLIDCRAFT_RELEASES_URL="https://github.com/tangency-labs/solidcraft/releases/latest"
SOLIDCRAFT_RELEASES_API="https://api.github.com/repos/tangency-labs/solidcraft/releases/latest"

log_info()  { printf '[info]  %s\n' "$*"; }
log_ok()    { printf '[ok]    %s\n' "$*"; }
log_warn()  { printf '[warn]  %s\n' "$*" >&2; }
log_err()   { printf '[error] %s\n' "$*" >&2; }

run_cmd() {
  if (( DRY_RUN )); then
    printf '[dry-run]'
    printf ' %q' "$@"
    printf '\n'
    return 0
  fi
  "$@"
}

run_shell_dir() {
  local workdir="$1"
  shift
  if (( DRY_RUN )); then
    printf '[dry-run] (cd %q &&' "$workdir"
    printf ' %q' "$@"
    printf ')\n'
    return 0
  fi
  (cd "$workdir" && "$@")
}

ask_yes_no() {
  local prompt="$1"
  local default_ans="${2:-y}"
  if (( ASSUME_YES )); then
    [[ "$default_ans" == "y" || "$default_ans" == "Y" ]]
    return
  fi
  if (( DRY_RUN )) && [[ ! -t 0 ]]; then
    [[ "$default_ans" == "y" || "$default_ans" == "Y" ]]
    return
  fi
  local suffix="[Y/n]"
  [[ "$default_ans" == "n" || "$default_ans" == "N" ]] && suffix="[y/N]"
  local reply=""
  read -r -p "$prompt $suffix " reply || reply=""
  reply="${reply:-$default_ans}"
  [[ "$reply" =~ ^[Yy]$ ]]
}

usage() {
  cat <<'EOF'
ONE-SHIP Linux Installer & Feature Wizard (v0.2.1)

Usage:
  ./install.sh [options]

Options:
  --profile <minimal|standard|full>
      minimal  : Core IDE + daemon + systemd unit + desktop entry
      standard : Core + REA + Electronics (+udev) + Voice + Omarchy widget (if present)
      full     : Standard + DE-SIGMA (source/clone) + PCBCraft (source/clone) + SolidCraft (binary check/install)
  --features <list>
      Comma-separated optional features: rea,electronics,design,pcb,cad,voice,omarchy
      (core is always installed)
  --prefix <dir>
      Installation prefix for bin/ and share/ (default: ~/.local)
  --repos-dir <dir>
      Directory where companion source repos (desigma, pcbcraft, opendictate) live or are cloned
      (default: parent directory of this checkout)
  --skip-deps
      Do not install distribution packages via pacman/apt-get/dnf/zypper
  --yes
      Non-interactive mode: accept default confirmations automatically
  --dry-run
      Print planned actions without executing builds, package managers, clones, or file writes
  --uninstall
      Remove installed binaries, systemd user service, desktop file, templates, and widget
  -h, --help
      Show this help message
EOF
}

parse_args() {
  while (( $# > 0 )); do
    case "$1" in
      --profile)
        [[ $# -lt 2 ]] && { log_err "--profile requires a value"; exit 1; }
        PROFILE="$2"
        shift 2
        ;;
      --features)
        [[ $# -lt 2 ]] && { log_err "--features requires a comma-separated list"; exit 1; }
        FEATURES_CSV="$2"
        shift 2
        ;;
      --prefix)
        [[ $# -lt 2 ]] && { log_err "--prefix requires a directory path"; exit 1; }
        PREFIX="$2"
        shift 2
        ;;
      --repos-dir)
        [[ $# -lt 2 ]] && { log_err "--repos-dir requires a directory path"; exit 1; }
        REPOS_DIR="$2"
        shift 2
        ;;
      --skip-deps)
        SKIP_DEPS=1
        shift
        ;;
      --yes|-y)
        ASSUME_YES=1
        shift
        ;;
      --dry-run)
        DRY_RUN=1
        shift
        ;;
      --uninstall)
        UNINSTALL=1
        shift
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        log_err "Unknown option: $1"
        usage
        exit 1
        ;;
    esac
  done
}

detect_environment() {
  if [[ -r /etc/os-release ]]; then
    # shellcheck disable=SC1091
    source /etc/os-release
    DISTRO_ID="${ID:-unknown}"
    DISTRO_NAME="${PRETTY_NAME:-$DISTRO_ID}"
  fi

  for pm in pacman apt-get dnf zypper; do
    if command -v "$pm" >/dev/null 2>&1; then
      PKG_MANAGER="$pm"
      break
    fi
  done

  if command -v yay >/dev/null 2>&1; then
    AUR_HELPER="yay"
  elif command -v paru >/dev/null 2>&1; then
    AUR_HELPER="paru"
  fi

  if [[ -d "$HOME/.config/omarchy" ]] || command -v omarchy-restart-shell >/dev/null 2>&1; then
    IS_OMARCHY=1
  fi
  if [[ -n "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]] || command -v hyprctl >/dev/null 2>&1; then
    IS_HYPRLAND=1
  fi

  if command -v pkexec >/dev/null 2>&1; then
    ELEVATE_CMD="pkexec"
  elif command -v sudo >/dev/null 2>&1; then
    ELEVATE_CMD="sudo"
  fi
}

apply_profile_or_features() {
  if [[ -n "$PROFILE" ]]; then
    case "$PROFILE" in
      minimal)
        FEAT_REA=0; FEAT_ELECTRONICS=0; FEAT_DESIGN=0; FEAT_PCB=0; FEAT_CAD=0; FEAT_VOICE=0; FEAT_OMARCHY=0
        ;;
      standard)
        FEAT_REA=1; FEAT_ELECTRONICS=1; FEAT_DESIGN=0; FEAT_PCB=0; FEAT_CAD=0; FEAT_VOICE=1; FEAT_OMARCHY="$IS_OMARCHY"
        ;;
      full)
        FEAT_REA=1; FEAT_ELECTRONICS=1; FEAT_DESIGN=1; FEAT_PCB=1; FEAT_CAD=1; FEAT_VOICE=1; FEAT_OMARCHY="$IS_OMARCHY"
        ;;
      *)
        log_err "Unsupported profile '$PROFILE' (expected minimal|standard|full)"
        exit 1
        ;;
    esac
  fi

  if [[ -n "$FEATURES_CSV" ]]; then
    IFS=',' read -r -a tokens <<< "$FEATURES_CSV"
    for raw in "${tokens[@]}"; do
      local tok="${raw,,}"
      tok="${tok// /}"
      case "$tok" in
        core|"") ;;
        rea) FEAT_REA=1 ;;
        electronics) FEAT_ELECTRONICS=1 ;;
        design|desigma) FEAT_DESIGN=1 ;;
        pcb|pcbcraft) FEAT_PCB=1 ;;
        cad|solidcraft) FEAT_CAD=1 ;;
        voice|opendictate) FEAT_VOICE=1 ;;
        omarchy|omarchy-bar) FEAT_OMARCHY=1 ;;
        *)
          log_err "Unknown feature token '$tok' in --features"
          exit 1
          ;;
      esac
    done
  fi
}

interactive_wizard() {
  if (( ASSUME_YES )) || [[ -n "$PROFILE" || -n "$FEATURES_CSV" ]]; then
    return
  fi
  if [[ ! -t 0 ]]; then
    log_warn "Non-interactive stdin detected without --profile/--features; defaulting to 'minimal' core install."
    return
  fi

  echo ""
  echo "=== ONE-SHIP Interactive Feature Selection ==="
  echo "Core IDE + oneship-daemon + systemd user unit are always installed."
  echo ""
  ask_yes_no "Enable RE Lab (REA CLI + reverse engineering package hints)?" "n" && FEAT_REA=1 || FEAT_REA=0
  ask_yes_no "Enable Electronics Workbench (udev rules + ngspice / sigrok-cli)?" "n" && FEAT_ELECTRONICS=1 || FEAT_ELECTRONICS=0
  ask_yes_no "Enable Design Editor (DE-SIGMA 'oneship-designer': detect or clone+build)?" "n" && FEAT_DESIGN=1 || FEAT_DESIGN=0
  ask_yes_no "Enable EDA/PCB Workbench (PCBCraft: detect or clone+build)?" "n" && FEAT_PCB=1 || FEAT_PCB=0
  ask_yes_no "Enable 3D CAD Integration (SolidCraft prebuilt binary check/install)?" "n" && FEAT_CAD=1 || FEAT_CAD=0
  ask_yes_no "Enable Local Voice Dictation (OpenDictate check / optional clone)?" "n" && FEAT_VOICE=1 || FEAT_VOICE=0
  if (( IS_OMARCHY )); then
    ask_yes_no "Install Production Omarchy Top Bar Widget (tech.kirulab.one-ship)?" "y" && FEAT_OMARCHY=1 || FEAT_OMARCHY=0
  fi
}

install_system_packages() {
  local -a pkgs=("$@")
  (( ${#pkgs[@]} == 0 )) && return 0
  if (( SKIP_DEPS )); then
    log_info "Skipping system packages (--skip-deps): ${pkgs[*]}"
    return 0
  fi
  if [[ -z "$PKG_MANAGER" ]]; then
    log_warn "No supported package manager found (pacman/apt-get/dnf/zypper). Install manually: ${pkgs[*]}"
    return 0
  fi

  local -a cmd=()
  case "$PKG_MANAGER" in
    pacman)  cmd=(pacman -S --needed --noconfirm "${pkgs[@]}") ;;
    apt-get) cmd=(apt-get install -y "${pkgs[@]}") ;;
    dnf)     cmd=(dnf install -y "${pkgs[@]}") ;;
    zypper)  cmd=(zypper --non-interactive install "${pkgs[@]}") ;;
  esac

  if (( EUID != 0 )); then
    if [[ -n "$ELEVATE_CMD" ]]; then
      cmd=("$ELEVATE_CMD" "${cmd[@]}")
    else
      log_warn "Neither pkexec nor sudo available; run manually as root: ${cmd[*]}"
      return 0
    fi
  fi

  log_info "Installing system dependencies (${PKG_MANAGER}): ${pkgs[*]}"
  if ask_yes_no "Proceed installing packages (${pkgs[*]})?" "y"; then
    run_cmd "${cmd[@]}"
  else
    log_warn "Skipped package installation: ${pkgs[*]}"
  fi
}

install_base_toolchain_deps() {
  (( SKIP_DEPS )) && return 0
  case "$PKG_MANAGER" in
    pacman)
      install_system_packages base-devel pkgconf openssl webkit2gtk-4.1 libayatana-appindicator librsvg protobuf systemd-libs libusb git github-cli nodejs npm rust
      ;;
    apt-get)
      install_system_packages build-essential pkg-config libssl-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev protobuf-compiler libudev-dev libusb-1.0-0-dev curl wget file libxdo-dev git gh nodejs npm
      ;;
    dnf)
      install_system_packages @development-tools pkgconf-pkg-config openssl-devel webkit2gtk4.1-devel libayatana-appindicator-gtk3-devel librsvg2-devel protobuf-compiler systemd-devel libusb1-devel git gh nodejs npm
      ;;
    zypper)
      install_system_packages -t pattern devel_basis
      install_system_packages pkg-config libopenssl-devel webkit2gtk3-devel libayatana-appindicator3-devel librsvg-devel protobuf-devel libudev-devel libusb-1_0-devel git gh nodejs npm
      ;;
    *)
      log_warn "Unsupported package manager; ensure Rust, Node/npm, pkg-config, OpenSSL, protoc (protobuf-compiler), and WebKitGTK 4.1 headers are installed."
      ;;
  esac
}

ensure_rust_toolchain() {
  if command -v cargo >/dev/null 2>&1 && command -v rustc >/dev/null 2>&1; then
    return 0
  fi
  log_err "Rust toolchain (cargo/rustc) not found on PATH. Install rustup (https://rustup.rs) or system rust package first."
  (( DRY_RUN )) || exit 1
}

build_and_install_core() {
  local bin_dir="$PREFIX/bin"
  local templates_dest="$PREFIX/share/one-ship/templates"
  local data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
  local desktop_dir="$data_home/applications"
  local icons_dir="$data_home/icons/hicolor"
  local systemd_user_dir="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"

  install_base_toolchain_deps
  ensure_rust_toolchain

  if ! command -v protoc >/dev/null 2>&1; then
    log_err "Protobuf compiler ('protoc') not found on PATH (install 'protobuf-compiler' on Debian/Ubuntu/Fedora or 'protobuf' on Arch)."
    (( DRY_RUN )) || exit 1
  fi

  if ! command -v node >/dev/null 2>&1 || ! command -v npm >/dev/null 2>&1; then
    log_err "Node.js / npm not found on PATH."
    (( DRY_RUN )) || exit 1
  fi

  log_info "Checking optional portrait pack..."
  if [[ -n "${ONESHIP_PORTRAITS_SRC:-}" ]] || [[ -d "$SCRIPT_DIR/extras/Pixel-Portraits-Free" ]] || [[ -d "${XDG_DATA_HOME:-$HOME/.local/share}/one-ship/portraits-src" ]]; then
    run_shell_dir "$SCRIPT_DIR" node scripts/build_portrait_catalog.mjs
  else
    log_info "No local Pixel-Portraits pack found; MIT-clean procedural portraits will be active."
  fi

  if [[ ! -d "$SCRIPT_DIR/node_modules" ]]; then
    log_info "Installing frontend dependencies (npm ci)..."
    run_shell_dir "$SCRIPT_DIR" npm ci
  fi

  log_info "Building frontend bundle (npm run build)..."
  run_shell_dir "$SCRIPT_DIR" npm run build

  log_info "Building Rust release binaries (one-ship + oneship-daemon)..."
  run_cmd cargo build --release --manifest-path "$SCRIPT_DIR/src-tauri/Cargo.toml" --features tauri/custom-protocol --bin one-ship --bin oneship-daemon

  log_info "Installing binaries and templates into $PREFIX ..."
  run_cmd install -d "$bin_dir" "$templates_dest" "$desktop_dir" "$systemd_user_dir"
  run_cmd install -m 0755 "$SCRIPT_DIR/src-tauri/target/release/one-ship" "$bin_dir/one-ship"
  run_cmd install -m 0755 "$SCRIPT_DIR/src-tauri/target/release/oneship-daemon" "$bin_dir/oneship-daemon"
  run_cmd install -m 0755 "$SCRIPT_DIR/packaging/bin/one-ship-launch" "$bin_dir/one-ship-launch"

  if (( DRY_RUN )); then
    log_info "[dry-run] Would sync templates/ -> $templates_dest"
  else
    rm -rf "$templates_dest"
    mkdir -p "$templates_dest"
    cp -a "$SCRIPT_DIR/templates/." "$templates_dest/"
  fi

  if (( DRY_RUN )); then
    log_info "[dry-run] Would install $desktop_dir/one-ship.desktop (Exec=$bin_dir/one-ship-launch)"
    log_info "[dry-run] Would install hicolor icons -> $icons_dir/{32x32,128x128,256x256,512x512}/apps/one-ship.png"
    log_info "[dry-run] Would install $systemd_user_dir/oneship-daemon.service (ExecStart=$bin_dir/oneship-daemon)"
  else
    sed "s|^Exec=.*$|Exec=$bin_dir/one-ship-launch|" "$SCRIPT_DIR/packaging/one-ship.desktop" > "$desktop_dir/one-ship.desktop"
    install -d "$icons_dir/32x32/apps" "$icons_dir/128x128/apps" "$icons_dir/256x256/apps" "$icons_dir/512x512/apps"
    install -m 0644 "$SCRIPT_DIR/src-tauri/icons/32x32.png" "$icons_dir/32x32/apps/one-ship.png"
    install -m 0644 "$SCRIPT_DIR/src-tauri/icons/128x128.png" "$icons_dir/128x128/apps/one-ship.png"
    install -m 0644 "$SCRIPT_DIR/src-tauri/icons/128x128@2x.png" "$icons_dir/256x256/apps/one-ship.png"
    install -m 0644 "$SCRIPT_DIR/src-tauri/icons/icon.png" "$icons_dir/512x512/apps/one-ship.png"
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
      gtk-update-icon-cache -f -t "$icons_dir" >/dev/null 2>&1 || true
    fi
    if command -v update-desktop-database >/dev/null 2>&1; then
      update-desktop-database "$desktop_dir" >/dev/null 2>&1 || true
    fi
    sed "s|^ExecStart=.*$|ExecStart=$bin_dir/oneship-daemon|" "$SCRIPT_DIR/packaging/systemd/oneship-daemon.service" > "$systemd_user_dir/oneship-daemon.service"
  fi

  if command -v systemctl >/dev/null 2>&1; then
    run_cmd systemctl --user daemon-reload
    if ask_yes_no "Enable and (re)start oneship-daemon.service now?" "y"; then
      run_cmd systemctl --user enable oneship-daemon.service
      run_cmd systemctl --user restart oneship-daemon.service
    fi
  fi

  log_ok "Core installation complete."
}

detect_preferred_terminal() {
  local t=""
  for t in ghostty kitty alacritty foot wezterm gnome-terminal konsole xfce4-terminal x-terminal-emulator xterm; do
    if command -v "$t" >/dev/null 2>&1; then
      printf '%s' "$t"
      return 0
    fi
  done
  printf 'xterm'
}

detect_default_runner_pair() {
  if command -v agy >/dev/null 2>&1; then
    printf 'agy|gemini-3.8-flash|high'
  elif command -v claude >/dev/null 2>&1; then
    printf 'claude|claude-sonnet-5-5|medium'
  elif command -v opencode >/dev/null 2>&1; then
    printf 'opencode|deepseek/deepseek-flash|high'
  else
    printf 'agy|gemini-3.8-flash|high'
  fi
}

seed_initial_config() {
  local projects_root="${ONESHIP_PROJECTS_ROOT:-$HOME/Projects}"
  if (( ! ASSUME_YES )) && [[ -t 0 ]]; then
    local custom_root=""
    read -r -p "Default projects directory [$projects_root]: " custom_root || custom_root=""
    [[ -n "$custom_root" ]] && projects_root="$custom_root"
  fi
  if [[ ! -d "$projects_root" ]]; then
    run_cmd mkdir -p "$projects_root"
  fi

  if command -v git >/dev/null 2>&1; then
    local git_name git_email
    git_name="$(git config --global --get user.name 2>/dev/null || true)"
    git_email="$(git config --global --get user.email 2>/dev/null || true)"
    if [[ -z "$git_name" || -z "$git_email" ]] && (( ! ASSUME_YES )) && [[ -t 0 ]]; then
      if ask_yes_no "Global git user.name/user.email not set. Configure now?" "y"; then
        [[ -z "$git_name" ]] && read -r -p "Git user.name: " git_name || true
        [[ -z "$git_email" ]] && read -r -p "Git user.email: " git_email || true
        [[ -n "$git_name" ]] && run_cmd git config --global user.name "$git_name"
        [[ -n "$git_email" ]] && run_cmd git config --global user.email "$git_email"
      fi
    fi
  fi

  local cfg_dir="${XDG_CONFIG_HOME:-$HOME/.config}/one-ship"
  local cfg_file="$cfg_dir/config.toml"
  if [[ -f "$cfg_file" ]]; then
    log_info "Existing configuration preserved: $cfg_file"
    return 0
  fi

  local term
  term="$(detect_preferred_terminal)"
  local runner_triple
  runner_triple="$(detect_default_runner_pair)"
  IFS='|' read -r def_runner def_model def_effort <<< "$runner_triple"

  if (( DRY_RUN )); then
    log_info "[dry-run] Would create initial $cfg_file (preferred_terminal=$term, projects_root=$projects_root, runner=$def_runner, model=$def_model)"
    return 0
  fi

  mkdir -p "$cfg_dir"
  cat > "$cfg_file" <<EOF
# ONE-SHIP Global Configuration (generated by install.sh)

[system]
preferred_terminal = "$term"
default_projects_root_path = "$projects_root"
log_level = "INFO"
dbus_call_timeout_seconds = 2
webkit_renderer_mode = "auto"

[plugins.appearance]
body_font = "IBM Plex Mono"
font_scale = "100% (Normal)"
quota_gauge_style = "concentric"

[plugins.orchestrator]
init_runner = "$def_runner"
init_model = "$def_model"
init_effort = "$def_effort"
agent_runner = "$def_runner"
default_model = "$def_model"
default_effort = "$def_effort"
default_docstyle = "sdd"
auto_approve = false

[plugins.accounts]
auto_warmup_enabled = true
context_warning_threshold_percent = 30.0
quota_alert_threshold_percent = 15.0
quota_danger_threshold_percent = 5.0

[plugins.supervisor]
max_buffer_lines_count = 500
shutdown_grace_period_ms = 3000
state_watcher_interval_ms = 2500

[plugins.telemetry]
telemetry_interval_ms = 2000

[plugins.profiler]
sample_interval_ms = 500
engine_shutdown_grace_period_ms = 15000
process_breakdown_max_items_count = 20

[plugins.vcs]
git_runner = "$def_runner"
git_model = "$def_model"
git_effort = "$def_effort"
default_branch = "master"
github_repo_list_limit_count = 1000
gitlab_repo_list_limit_count = 100
snapshots_enabled = true
minutes_checkpoint_mode = "snapshot"
auto_commit_detail_level = "title_and_decisions"
auto_snapshot_on_file_change = true
max_per_project_count = 50
auto_snapshot_on_init = true
scratchpad_retention_days = 0
EOF
  log_ok "Seeded initial configuration: $cfg_file (runner=$def_runner, terminal=$term)"
}

install_feature_rea() {
  log_info "Configuring RE Lab (REA)..."
  case "$PKG_MANAGER" in
    pacman)  install_system_packages radare2 binutils binwalk squashfs-tools jadx ;;
    apt-get|dnf|zypper) install_system_packages radare2 binutils binwalk squashfs-tools ;;
  esac

  if command -v rea >/dev/null 2>&1; then
    log_ok "REA binary already installed: $(command -v rea)"
  elif command -v npm >/dev/null 2>&1; then
    if ask_yes_no "REA CLI ('rea') is missing. Install globally via 'npm install -g rea-agents@latest'?" "y"; then
      run_cmd npm install -g rea-agents@latest
    fi
  else
    log_warn "npm unavailable; install REA manually with: npm install -g rea-agents@latest"
  fi
}

install_feature_electronics() {
  log_info "Configuring Electronics Workbench & USB instrumentation udev rules..."
  case "$PKG_MANAGER" in
    pacman)  install_system_packages ngspice sigrok-cli ;;
    apt-get) install_system_packages ngspice sigrok-cli ;;
    dnf)     install_system_packages ngspice sigrok-cli ;;
    zypper)  install_system_packages ngspice sigrok-cli ;;
  esac

  local rules_src="$SCRIPT_DIR/packaging/udev/99-oneship-instruments.rules"
  local rules_dst="/etc/udev/rules.d/99-oneship-instruments.rules"
  if [[ -f "$rules_dst" ]]; then
    log_ok "udev rules already present: $rules_dst"
  elif ask_yes_no "Install USB oscilloscope/logic-analyzer udev rules to $rules_dst?" "y"; then
    if (( EUID == 0 )); then
      run_cmd install -m 0644 "$rules_src" "$rules_dst"
      run_cmd udevadm control --reload-rules
      run_cmd udevadm trigger
    elif [[ -n "$ELEVATE_CMD" ]]; then
      run_cmd "$ELEVATE_CMD" sh -c "install -m 0644 '$rules_src' '$rules_dst' && udevadm control --reload-rules && udevadm trigger"
    else
      log_warn "No pkexec/sudo found; copy $rules_src -> $rules_dst manually as root."
    fi
  fi
}

ensure_repo_cloned_or_prompt() {
  local label="$1"
  local dir="$2"
  local url="$3"
  local branch="$4"

  if [[ -d "$dir/.git" ]]; then
    log_ok "$label repository already present: $dir"
    return 0
  fi

  if ! command -v git >/dev/null 2>&1; then
    log_warn "git not installed; cannot clone $label ($url)"
    return 1
  fi

  if ask_yes_no "$label is not cloned at '$dir'. Clone from '$url' (branch '$branch')?" "y"; then
    run_cmd mkdir -p "$(dirname "$dir")"
    run_cmd git clone --branch "$branch" "$url" "$dir"
    return 0
  fi
  log_warn "Skipped cloning $label."
  return 1
}

install_feature_design() {
  log_info "Checking DE-SIGMA (oneship-designer & desigma-mcp)..."
  if command -v oneship-designer >/dev/null 2>&1 && command -v desigma-mcp >/dev/null 2>&1; then
    log_ok "DE-SIGMA binaries already installed: $(command -v oneship-designer), $(command -v desigma-mcp)"
    return 0
  fi

  local desigma_dir="${ONESHIP_DESIGMA_DIR:-$REPOS_DIR/desigma}"
  if ! ensure_repo_cloned_or_prompt "DE-SIGMA" "$desigma_dir" "$DESIGMA_REPO_URL" "$DESIGMA_BRANCH"; then
    return 0
  fi

  if ask_yes_no "Build & install DE-SIGMA ('oneship-designer', 'desigma-mcp') into $PREFIX/bin?" "y"; then
    run_cmd cargo build --release --manifest-path "$desigma_dir/Cargo.toml" -p desigma-app -p desigma-mcp
    run_cmd install -d "$PREFIX/bin"
    run_cmd install -m 0755 "$desigma_dir/target/release/oneship-designer" "$PREFIX/bin/oneship-designer"
    run_cmd install -m 0755 "$desigma_dir/target/release/desigma-mcp" "$PREFIX/bin/desigma-mcp"
    log_ok "DE-SIGMA installed into $PREFIX/bin"
  fi
}

install_feature_pcb() {
  log_info "Checking PCBCraft (pcbcraft & pcbcraft-cli)..."
  if command -v pcbcraft >/dev/null 2>&1 && command -v pcbcraft-cli >/dev/null 2>&1; then
    log_ok "PCBCraft binaries already installed: $(command -v pcbcraft), $(command -v pcbcraft-cli)"
    return 0
  fi

  local pcb_dir="${ONESHIP_PCBCRAFT_DIR:-$REPOS_DIR/pcbcraft}"
  if ! ensure_repo_cloned_or_prompt "PCBCraft" "$pcb_dir" "$PCBCRAFT_REPO_URL" "$PCBCRAFT_BRANCH"; then
    return 0
  fi

  if ask_yes_no "Build & install PCBCraft ('pcbcraft', 'pcbcraft-cli') with memory-bounded Cargo settings (Spec 33 §1.2)?" "y"; then
    # Bounded build flags prevent OOM on ~16 GiB workstations (Spec 33 §1.2)
    local -a cargo_cmd=(
      env
      CARGO_PROFILE_RELEASE_LTO=false
      CARGO_PROFILE_RELEASE_CODEGEN_UNITS=4
      cargo build --release -j 2 --manifest-path "$pcb_dir/Cargo.toml" -p pcbcraft -p pcbcraft-cli
    )
    if command -v systemd-run >/dev/null 2>&1; then
      run_cmd systemd-run --user --scope -p MemoryHigh=8G -p MemoryMax=10G "${cargo_cmd[@]}"
    else
      run_cmd "${cargo_cmd[@]}"
    fi
    run_cmd install -d "$PREFIX/bin"
    run_cmd install -m 0755 "$pcb_dir/target/release/pcbcraft" "$PREFIX/bin/pcbcraft"
    run_cmd install -m 0755 "$pcb_dir/target/release/pcbcraft-cli" "$PREFIX/bin/pcbcraft-cli"
    log_ok "PCBCraft installed into $PREFIX/bin"
  fi
}

install_feature_cad_solidcraft() {
  # SolidCraft is distributed as prebuilt binaries (never clone/build fork per LICENSE-BRAND).
  log_info "Checking SolidCraft (prebuilt binary package)..."
  if command -v solidcraft >/dev/null 2>&1 && command -v solidcraft-cli >/dev/null 2>&1; then
    log_ok "SolidCraft binaries already installed: $(command -v solidcraft), $(command -v solidcraft-cli)"
    return 0
  fi

  if [[ -n "$AUR_HELPER" ]] && [[ "$PKG_MANAGER" == "pacman" ]]; then
    if ask_yes_no "SolidCraft not found on PATH. Install prebuilt AUR package 'solidcraft-bin' using $AUR_HELPER?" "y"; then
      run_cmd "$AUR_HELPER" -S --needed solidcraft-bin
      return 0
    fi
  fi

  if command -v curl >/dev/null 2>&1 && command -v python3 >/dev/null 2>&1; then
    log_info "Checking official SolidCraft GitHub Releases for Linux binary artifacts..."
    local arch asset_url=""
    arch="$(uname -m)"
    if (( DRY_RUN )); then
      log_info "[dry-run] Would query $SOLIDCRAFT_RELEASES_API for '$arch' prebuilt release binary/archive"
      return 0
    fi
    asset_url="$(
      curl -fsSL "$SOLIDCRAFT_RELEASES_API" 2>/dev/null | python3 -c '
import json, sys
arch = sys.argv[1].lower()
data = json.load(sys.stdin)
assets = data.get("assets", [])
for a in assets:
    name = a.get("name", "").lower()
    if "linux" in name and (arch in name or (arch == "x86_64" and "amd64" in name)) and (name.endswith(".tar.gz") or name.endswith(".tar.xz") or name.endswith(".deb") or name.endswith(".rpm") or name.endswith(".appimage")):
        print(a.get("browser_download_url", ""))
        break
' "$arch" || true
    )"
    if [[ -n "$asset_url" ]]; then
      log_info "Found SolidCraft prebuilt release asset: $asset_url"
      log_info "Download and extract 'solidcraft' + 'solidcraft-cli' into '$PREFIX/bin', or install via your distro package format."
      return 0
    fi
  fi

  log_warn "SolidCraft is not cloned/compiled from source. Download prebuilt binaries from: $SOLIDCRAFT_RELEASES_URL"
}

install_feature_voice() {
  log_info "Checking OpenDictate voice integration..."
  if command -v opendictate >/dev/null 2>&1; then
    log_ok "OpenDictate already installed: $(command -v opendictate)"
    return 0
  fi

  local od_dir="${ONESHIP_OPENDICTATE_DIR:-$REPOS_DIR/opendictate}"
  if ensure_repo_cloned_or_prompt "OpenDictate" "$od_dir" "$OPENDICTATE_REPO_URL" "main"; then
    if [[ -x "$od_dir/install.sh" ]]; then
      if ask_yes_no "Run OpenDictate's installer ($od_dir/install.sh)?" "y"; then
        run_shell_dir "$od_dir" ./install.sh
      fi
    else
      log_info "OpenDictate cloned to $od_dir; follow its README to finish D-Bus service setup."
    fi
  fi
}

install_feature_omarchy() {
  log_info "Installing Production Omarchy Top Bar Widget (tech.kirulab.one-ship)..."
  local plugins_dir="${XDG_CONFIG_HOME:-$HOME/.config}/omarchy/plugins"
  local target="$plugins_dir/tech.kirulab.one-ship"
  run_cmd mkdir -p "$plugins_dir"
  if [[ -L "$target" || -e "$target" ]]; then
    if (( DRY_RUN )); then
      log_info "[dry-run] Would replace existing Omarchy plugin link at $target"
    else
      rm -rf "$target"
    fi
  fi
  run_cmd ln -s "$SCRIPT_DIR/integrations/omarchy-bar" "$target"
  if command -v omarchy-shell >/dev/null 2>&1; then
    run_cmd omarchy-shell shell rescanPlugins >/dev/null 2>&1 || true
  fi
  if command -v omarchy >/dev/null 2>&1; then
    run_cmd omarchy plugin enable tech.kirulab.one-ship || true
  fi
  log_ok "Omarchy plugin linked at $target (run 'omarchy-restart-shell' if not immediately visible)."
}

uninstall_oneship() {
  local bin_dir="$PREFIX/bin"
  local templates_dir="$PREFIX/share/one-ship"
  local data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
  local desktop_file="$data_home/applications/one-ship.desktop"
  local icons_dir="$data_home/icons/hicolor"
  local service_file="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/oneship-daemon.service"
  local omarchy_plugin="${XDG_CONFIG_HOME:-$HOME/.config}/omarchy/plugins/tech.kirulab.one-ship"

  log_info "Uninstalling ONE-SHIP from prefix $PREFIX..."
  if command -v systemctl >/dev/null 2>&1; then
    run_cmd systemctl --user disable --now oneship-daemon.service || true
  fi
  run_cmd rm -f "$bin_dir/one-ship" "$bin_dir/oneship-daemon" "$bin_dir/one-ship-launch" "$desktop_file" "$service_file" \
    "$icons_dir/32x32/apps/one-ship.png" "$icons_dir/128x128/apps/one-ship.png" \
    "$icons_dir/256x256/apps/one-ship.png" "$icons_dir/512x512/apps/one-ship.png"
  run_cmd rm -rf "$templates_dir"
  if [[ -L "$omarchy_plugin" ]]; then
    run_cmd rm -f "$omarchy_plugin"
  fi
  if command -v systemctl >/dev/null 2>&1; then
    run_cmd systemctl --user daemon-reload || true
  fi
  log_ok "Uninstall complete (user data/config in ~/.config/one-ship/ preserved)."
}

print_summary() {
  echo ""
  echo "=== ONE-SHIP Installation Plan Summary ==="
  echo "  Distro          : $DISTRO_NAME ($DISTRO_ID)"
  echo "  Package Manager : ${PKG_MANAGER:-none}"
  echo "  Session / WM    : $SESSION_TYPE (Hyprland=$IS_HYPRLAND, Omarchy=$IS_OMARCHY)"
  echo "  Install Prefix  : $PREFIX"
  echo "  Repos Directory : $REPOS_DIR"
  echo "  Features        : core=1 rea=$FEAT_REA electronics=$FEAT_ELECTRONICS design=$FEAT_DESIGN pcb=$FEAT_PCB cad=$FEAT_CAD voice=$FEAT_VOICE omarchy=$FEAT_OMARCHY"
  echo ""
}

main() {
  parse_args "$@"
  detect_environment

  if (( UNINSTALL )); then
    uninstall_oneship
    exit 0
  fi

  apply_profile_or_features
  interactive_wizard
  print_summary

  if ! ask_yes_no "Continue with installation?" "y"; then
    log_info "Installation cancelled by user."
    exit 0
  fi

  build_and_install_core
  seed_initial_config

  (( FEAT_REA ))         && install_feature_rea
  (( FEAT_ELECTRONICS )) && install_feature_electronics
  (( FEAT_DESIGN ))      && install_feature_design
  (( FEAT_PCB ))         && install_feature_pcb
  (( FEAT_CAD ))         && install_feature_cad_solidcraft
  (( FEAT_VOICE ))       && install_feature_voice
  (( FEAT_OMARCHY ))     && install_feature_omarchy

  if ! command -v agy >/dev/null 2>&1 && ! command -v claude >/dev/null 2>&1 && ! command -v opencode >/dev/null 2>&1; then
    echo ""
    log_warn "No AI agent runner detected on PATH (agy, claude, opencode)."
    log_info "Install at least one CLI runner before launching autonomous agents:"
    log_info "  - claude   : npm install -g @anthropic-ai/claude-code"
    log_info "  - opencode : curl -fsSL https://opencode.ai/install | bash"
  fi

  echo ""
  log_ok "ONE-SHIP installation finished. Ensure '$PREFIX/bin' is on your PATH and launch with 'one-ship-launch'."
}

main "$@"
