# Devlog: Tauri Runtime Watcher Panic Resolution & Workspace Cleanup

**Date:** 2026-09-05  
**Topic:** Resolving Tauri desktop boot panic (`there is no reactor running`) and workspace hygiene.

---

## 1. Problem Statement
Following the transition of background telemetry, project watchers, and Herder process supervision from `kapawu-daemon` to native Tauri IPC in `src-tauri/src/lib.rs`, the desktop binary (`wang-kapawu`) failed to remain active, terminating immediately upon execution.

Executing the binary with `RUST_BACKTRACE=1` pinpointed the failure:
```
thread 'main' panicked at src/watcher.rs:15:5:
there is no reactor running, must be called from the context of a Tokio 1.x runtime
stack backtrace:
   ...
   3: tokio::task::spawn::spawn_inner
   4: tokio::task::spawn::spawn
   5: wang_kapawu_lib::watcher::spawn_project_watcher (src/watcher.rs:15:5)
   6: wang_kapawu_lib::run (src/lib.rs:37:13)
```

## 2. Root Cause Analysis
In `kapawu-daemon`, `main()` was decorated with `#[tokio::main]`, establishing an ambient Tokio reactor across runtime threads. In the Tauri desktop application, the main entrypoint executes Tauri's synchronous event loop. The `.setup(|app| { ... })` hook executes on the main thread outside of Tokio's worker context. Calling `tokio::spawn(...)` directly in this context causes an immediate panic.

## 3. Implementation Details
Replaced direct `tokio::spawn` calls with `tauri::async_runtime::spawn` across all async background tasks:
1. `src-tauri/src/watcher.rs`: `spawn_project_watcher`
2. `src-tauri/src/telemetry.rs`: `spawn_telemetry_loop`
3. `src-tauri/src/kopeng/herdr.rs`: `spawn_herdr_watcher`
4. `src-tauri/src/kopeng/opendictate.rs`: `spawn_dbus_monitor`
5. `src-tauri/src/kopeng/skill.rs`: `initialize_project_agent`

Removed dead code and unused imports (`BASE_CONFIG_TOML` in `src/hydration.rs`, `use std::fs;` in `watcher.rs`, `use std::io::BufRead;` in `opendictate.rs`).

## 4. Verification
1. **Compilation:** `cargo check` completed with 0 errors and 0 warnings.
2. **Binary Build:** `cargo build --bin wang-kapawu` compiled cleanly.
3. **Execution Test:** Binary launched successfully in Wayland/X11 environment. Logs verified proper initialization:
   ```
   [INFO] [core] Kapawu Hub engine initialized (GUI Mode)
   [INFO] [client] Tauri IPC Event Listeners Initialized
   ```
4. **Inter-Agent Communication:** Notified `mcp` agent of task `task-1788624600` completion via `report_task_status`.

## 5. Workspace Cleanup
1. Deleted 19 temporary scratch scripts (`patch.mcp.rs.sh`, `patch_*.py`, `src-tauri/patch_mcp_fix.py`, `src-tauri/src/db.rs.patch`).
2. Added `*.db`, `*.db-shm`, `*.db-wal`, `*.sqlite`, and `*.sqlite3` to `.gitignore`.
3. Updated `TECHNICAL_DEBT.md` to mark Items 1, 4, and 5 as resolved.
