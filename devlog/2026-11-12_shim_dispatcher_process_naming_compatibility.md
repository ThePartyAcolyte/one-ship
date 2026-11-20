# Devlog: Shim Dispatcher Compatibility & Process Naming Resolution

- **Date:** 2026-11-12
- **Topic:** Supervisor Executable Resolution & Shim Dispatcher Compatibility (`mise`/`asdf`)
- **Status:** Complete

---

## 1. Context & Observed Problem
During greenfield onboarding (e.g. creating project `Kirulab Website`), the initialization agent (`initializer` using `agy` runner) failed immediately at spawn time with exit code 1.
Inspection of the circular output buffer and chat turns revealed:
```text
[STDERR] mise ERROR oneship[Kirulab Website:initializer] is not a valid shim.
This likely means you uninstalled a tool and the shim does not point to anything.
Run `mise use <TOOL>` to reinstall the tool.
```

### Root Cause
1. **Shim Precedence in Daemon PATH**: In the daemon's runtime environment, `~/.local/share/mise/shims` precedes local binary directories (`~/.local/bin`). Consequently, bare command lookups like `agy` resolved to the `mise` shim (`~/.local/share/mise/shims/agy -> /usr/bin/mise`).
2. **Custom `argv[0]` via `arg0`**: The supervisor configured Unix process titles via `tokio::process::Command::arg0(format!("oneship[{}:{}]", project_name, agent_id))`.
3. **Mise Multicall Dispatch Failure**: Version manager shims (`mise`, `asdf`) are multicall dispatchers that inspect `argv[0]` to determine which managed tool to invoke. Overriding `argv[0]` with an application-specific title caused `mise` to attempt lookup of `oneship[...]` in its registry, causing an instant fatal abort.

---

## 2. Changes Implemented

### Backend (`src-tauri/src/supervisor.rs`)
1. **Executable Resolution (`resolve_executable`)**:
   - Implemented `resolve_executable(&str) -> ResolvedExecutable` and helper `resolve_via_mise_which(tool_name: &str) -> Option<PathBuf>`.
   - When a tool is managed by `mise` or masked by a shim, queries `mise which <tool>` to resolve the true underlying binary path (e.g. `~/.local/share/mise/installs/antigravity-cli/latest/agy` or `claude`).
   - For unmanaged binaries, scans `PATH` entries preferring non-shim executables (e.g. `/home/user/.local/bin/agy`) over shim directories (`/shims/`).
2. **Shim Dispatcher Guard (`is_known_shim_dispatcher`)**:
   - Detects whether an executable path is a shim dispatcher (resides in `/shims/` or canonicalizes to `mise`/`asdf`).
   - If an executable remains a shim dispatcher and cannot be resolved to an underlying binary, sets `supports_custom_arg0: false`.
   - In `spawn_agent`, `spawn_service`, and `spawn_task`, conditionally omits `cmd.arg0` when `supports_custom_arg0 == false`, safely preserving default `argv[0]` behavior.
3. **Quota Probe Resolution (`src-tauri/src/kopeng/stats.rs`)**:
   - Updated `QUOTA_PROBE_RUNNER` spawn to resolve through `resolve_executable`.

### Living Specifications (`docs/specs/08_native_process_supervisor.md`)
- Synchronized Section 2.3 with the new `resolve_executable` requirement and shim dispatcher compatibility invariant.

### Database State Cleanup
- Removed stuck `running` initialization record for `/home/butcherwutcher/Projects/dev/Kirulab Website` in `project_initializations`.

---

## 3. Verification & Results
- Added unit tests:
  - `supervisor::tests::test_is_known_shim_dispatcher`
  - `supervisor::tests::test_resolve_executable_standard_commands`
- Executed `cargo test --manifest-path src-tauri/Cargo.toml -- supervisor`:
  - 23 passed; 0 failed.
- Watchexec automatically rebuilt `oneship-daemon` and restarted `oneship-dev-daemon.service` cleanly.
