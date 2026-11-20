# Devlog: Phase 4 — Production Omarchy Bar Widget & Runtime Status Snapshot (2026-11-20)

## Summary

Completed **Phase 4** of [Spec 41 (Linux Packaging & Host Capabilities)](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/41_linux_packaging_and_host_capabilities.md), cleanly decoupling the developer dev-harness Omarchy bar widget from the end-user production widget, and adding a lightweight atomic daemon runtime snapshot (`$XDG_RUNTIME_DIR/oneship-status.json`) so bar widgets can observe live orchestrator telemetry without calling the authenticated operator API.

## Changes Implemented

1. **Daemon Runtime Status Snapshot (`$XDG_RUNTIME_DIR/oneship-status.json`):**
   - Added [`RuntimeAgentSummary`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/supervisor.rs#L5248-L5256), [`DaemonRuntimeStatus`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/supervisor.rs#L5258-L5274), [`runtime_status_file_path`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/supervisor.rs#L5278-L5294), [`build_runtime_status_snapshot`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/supervisor.rs#L5296-L5363), [`write_runtime_status_snapshot`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/supervisor.rs#L5365-L5382), and `remove_runtime_status_snapshot` in `src-tauri/src/supervisor.rs`.
   - Wired `spawn_agent_state_watcher` in `src-tauri/src/supervisor.rs` to atomically write `$XDG_RUNTIME_DIR/oneship-status.json` (write `.json.tmp`, `0600` on Unix, atomic rename) on every watch tick with working/idle/blocked/error agent counts, running services count, pending approvals count, and capped active agent summaries (`MAX_RUNTIME_STATUS_AGENTS_COUNT = 16`).
   - Wired [`src-tauri/src/bin/daemon.rs`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/bin/daemon.rs) to emit an initial snapshot at daemon startup and invoke `remove_runtime_status_snapshot()` on graceful shutdown.
   - Added unit test [`runtime_status_snapshot_aggregates_active_agents_and_counts`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/supervisor.rs#L5679-L5750) in `src-tauri/src/supervisor.rs`.

2. **Developer Widget Isolation (`integrations/omarchy-bar-dev/`):**
   - Moved the `run_dev.sh`-coupled developer widget (`status`, `all`, `rebuild-daemon`, `stop`, `gui`) to a dedicated plugin ID `tech.kirulab.one-ship-dev` under [`integrations/omarchy-bar-dev/manifest.json`](file:///home/butcherwutcher/Projects/dev/one-ship/integrations/omarchy-bar-dev/manifest.json), [`integrations/omarchy-bar-dev/Widget.qml`](file:///home/butcherwutcher/Projects/dev/one-ship/integrations/omarchy-bar-dev/Widget.qml), and [`integrations/omarchy-bar-dev/ControlPanel.qml`](file:///home/butcherwutcher/Projects/dev/one-ship/integrations/omarchy-bar-dev/ControlPanel.qml).

3. **Production Omarchy Bar Widget (`integrations/omarchy-bar/`):**
   - Rebuilt `tech.kirulab.one-ship` ([`integrations/omarchy-bar/manifest.json`](file:///home/butcherwutcher/Projects/dev/one-ship/integrations/omarchy-bar/manifest.json), [`integrations/omarchy-bar/Widget.qml`](file:///home/butcherwutcher/Projects/dev/one-ship/integrations/omarchy-bar/Widget.qml), [`integrations/omarchy-bar/ControlPanel.qml`](file:///home/butcherwutcher/Projects/dev/one-ship/integrations/omarchy-bar/ControlPanel.qml)) as an end-user widget with zero dev-script dependency.
   - Polls `systemctl --user show oneship-daemon.service` + reads `$XDG_RUNTIME_DIR/oneship-status.json` to display active/working/blocked/error agent counts, pending approvals badge, running services count, and active agent list.
   - Provides user actions to start/stop/restart `oneship-daemon.service` and open/focus the desktop GUI (`one-ship-launch` / `one-ship`).

4. **Specs & Documentation Sync:**
   - Updated [`docs/specs/12_streamable_http_mcp_daemon.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/12_streamable_http_mcp_daemon.md) to document the `$XDG_RUNTIME_DIR/oneship-status.json` runtime telemetry contract.
   - Updated [`docs/specs/41_linux_packaging_and_host_capabilities.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/41_linux_packaging_and_host_capabilities.md), [`README.md`](file:///home/butcherwutcher/Projects/dev/one-ship/README.md), and [`TODO.md`](file:///home/butcherwutcher/Projects/dev/one-ship/TODO.md).

## Verification

### 1. Read-Only Rule Gate (`./scripts/verify_rules.sh`)
Zero new violations introduced across all modified files (2 pre-existing unrelated hard failures remain: Debt `#45` `bg-black/*` in legacy modals and `src-tauri/src/config.rs:1920` `migrate_toml_snapshots_to_vcs` returning `Result<Option<String>, String>`):

```text
=== ONE-SHIP Read-Only Rule Verification ===
[PASS] Zero TypeScript 'any' in src/
[WARN] Native title= attributes found (verify these are component props, not HTML attributes): ... (13 AppEmptyState/card props)
[WARN] Lock .unwrap()/.expect() found (must only exist inside #[cfg(test)]):
src-tauri/src/kopeng/forge/gitlab.rs:721:        let _guard = PATH_LOCK.lock().unwrap();
[PASS] Zero Tailwind palette classes (text-/bg-/border-{color}-*)
[FAIL] Zero raw bg-black/* or bg-[#...] outside ui.css (legacy Debt #45 files only)
[FAIL] Unstructured Result<_, String> found outside tests:
src-tauri/src/config.rs:1920:pub fn migrate_toml_snapshots_to_vcs(content: &str) -> Result<Option<String>, String> {
[PASS] Zero backdrop-blur-* in components/views
[PASS] Zero hardcoded /home/butcherwutcher paths
=== Summary: 2 hard failure(s) ===
```

### 2. Silent Fallback Check on Modified Rust Files

```text
$ sed -n '5248,5420p' src-tauri/src/supervisor.rs | grep -nE "unwrap_or|unwrap_or_default|\.ok\(\)|let _ =" || true
(0 matches)

$ grep -nE "unwrap_or|unwrap_or_default|\.ok\(\)|let _ =|catch\s*(\([^)]*\))?\s*\{\s*\}" src-tauri/src/bin/daemon.rs
78:            let _ = tokio::signal::ctrl_c().await;
91:    let listener = tokio::net::TcpListener::bind(addr).await.unwrap_or_else(|_| panic!("Failed to bind to port {}", port));
(both pre-existing lines in signal wait / fail-fast bind panic)
```

### 3. Daemon Build, Service Health & Live Runtime Status Snapshot

```text
$ journalctl --user -u oneship-dev-daemon-build -n 25 --no-pager
Nov 20 12:35:01 omarchy systemd[1259]: Starting [systemd-run] /home/butcherwutcher/Projects/dev/one-ship/scripts/run_dev.sh _build-daemon-inner...
Nov 20 12:35:02 omarchy run_dev.sh[1063649]:    Compiling one-ship v0.2.1 (/home/butcherwutcher/Projects/dev/one-ship/src-tauri)
...
Nov 20 12:35:29 omarchy run_dev.sh[1063649]:     Finished `dev` profile [unoptimized + debuginfo] target(s) in 27.47s
Nov 20 12:35:29 omarchy systemd[1259]: Finished [systemd-run] /home/butcherwutcher/Projects/dev/one-ship/scripts/run_dev.sh _build-daemon-inner.

$ ./scripts/run_dev.sh status
{"units":{"daemon":{"active_state":"active","sub_state":"running","result":"success","main_pid":1067062,"log_tail":null},"daemon_build":{"active_state":"inactive","sub_state":"dead","result":"success","main_pid":null,"log_tail":null},"daemon_watch":{"active_state":"active","sub_state":"running","result":"success","main_pid":3872353,"log_tail":null},"vite":{"active_state":"active","sub_state":"running","result":"success","main_pid":3867329,"log_tail":null},"gui":{"active_state":"active","sub_state":"running","result":"success","main_pid":1072135,"log_tail":null}},"daemon_listening_ports":[8765],"vite_listening_ports":[],"gui_window_pid":1072206,"build_progress":{"phase":"cargo_build","completed_count":1187,"total_count":1188,"percent":99,"current_item":"linking","started_at_seconds":1795188901,"last_duration_seconds":null},"gui_progress":{"phase":"running","completed_count":null,"total_count":null,"percent":100,"current_item":"one-ship","started_at_seconds":1795188962,"last_duration_seconds":null}}

$ ls -l "${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/oneship-status.json"
-rw------- 1 butcherwutcher butcherwutcher 281 Nov 20 12:36 /run/user/1000/oneship-status.json
{"daemon_pid":1067062,"mcp_port":8765,"updated_at_ms":1795188986502,"projects_count":8,"working_agents_count":0,"idle_agents_count":0,"blocked_agents_count":0,"error_agents_count":0,"active_agents_count":0,"running_services_count":0,"pending_approvals_count":0,"active_agents":[]}
```

## Pending Manual / Runtime Items

- Install and test `integrations/omarchy-bar` (`tech.kirulab.one-ship`) and `integrations/omarchy-bar-dev` (`tech.kirulab.one-ship-dev`) inside the live Omarchy Bar UI (`omarchy-shell` / `quickshell`).
- Untrack legacy portrait asset paths (`git rm -r --cached extras/Pixel-Portraits-Free extras/Pixel-Portraits-Addon public/portraits`) if previously staged/committed (requires manual user command).
- Proceed to **Phase 5**: Interactive `./install.sh` feature wizard + external companion repos (`SolidCraft`, `PCBCraft`, `DE-SIGMA`) packaging coordination after proposal approval.
