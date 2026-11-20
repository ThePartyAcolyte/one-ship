# Development Session: 2026-10-14 — Orphan Process Reconciliation & State Ledger

## Context & Objectives
- **Target Problem:** In development workflows, persistent scripts (e.g. `npm run dev`, `vite`, `python`, background watchers) and tasks run as child processes under the Process Supervisor. While voluntary application exits properly drain and terminate processes, unexpected terminations (power loss, OS OOM killer, `SIGKILL`, or X11/Wayland compositor crashes) leave child processes orphaned.
- **Port Conflict Hazard (`EADDRINUSE`):** Because Linux does not deliver `PR_SET_PDEATHSIG` across arbitrary `fork()` generations to grandchild processes (such as `npm -> node -> vite`), orphaned services remain bound to TCP/UDP ports indefinitely. On relaunch, attempting to start the service results in silent failure or immediate `EADDRINUSE` collision.
- **North Star Roadmap:** Establish a universal application state ledger that tracks running processes, terminals, and agent sessions across unexpected terminations, moving toward a complete state restoration machine.

---

## Key Changes & Implementations

### 1. SQLite Process Persistence Ledger (`src-tauri/src/db.rs`)
- Created table `supervised_processes` with indexes (`idx_supervised_processes_lookup`, `idx_supervised_processes_pid`, `idx_supervised_processes_unclosed`):
  - Tracks: `run_id`, `project_id`, `target_id`, `target_type`, `command`, `args_json`, `cwd`, `pid`, `pgid`, `env_identifier`, `status`, `exit_code`, `spawned_at_ms`, `terminated_at_ms`, `failure_reason`.
  - Enforces explicit Tiger Style unit suffixes (`_ms`).
- Implemented lifecycle helpers:
  - `db_record_process_spawn`: Records active execution on launch.
  - `db_record_process_termination`: Records clean exits, user stops, and error statuses.
  - `db_list_unclosed_supervised_processes`: Retrieves lingering `status = 'working'` records on boot.
  - `db_get_project_supervised_processes`: Retrieves execution history per project.
  - Exposed `get_project_supervised_processes_cmd` Tauri command.

### 2. Linux Process Attestation & Boot-Time Reconciliation Engine (`src-tauri/src/supervisor.rs`)
- **Process Attestation Invariant (`ONESHIP_RUN_ID`):**
  - Child processes receive `ONESHIP_RUN_ID=<run_id>` in their environment on spawn.
  - Implemented `read_proc_environ_var(pid, key)` reading `/proc/<pid>/environ` on Linux.
  - Protects against Linux PID recycling hazards: unclosed processes are only terminated if their environment matches the recorded `run_id`.
- **Reconciliation Sweep (`reconcile_orphan_processes`):**
  - Sweeps unclosed process rows at startup.
  - If authenticated via `/proc/<pid>/environ`:
    - Dispatches whole-tree process group termination: `libc::kill(-(pid as i32), SIGKILL)` and `libc::kill(pid as i32, SIGKILL)` to immediately free network ports.
    - Transitions database record to `status = 'orphaned_killed'`.
    - Injects explanatory recovery notices into `retained_buffers` under both service keys (`{project}:svc:{id}`) and task keys (`{project}:tsk:{id}` and `{project}:tsk:{id}:0`).
  - If PID no longer exists or was recycled:
    - Transitions database record to `status = 'crashed'`.
  - Returns structured `ReconciliationSummary` and exposed `reconcile_orphan_processes_cmd`.
- **Supervisor Integration:**
  - Injected `ONESHIP_RUN_ID` in `spawn_service` and `spawn_task`.
  - Registered spawn and termination events across `spawn_service`, `spawn_task`, and `stop_process`.

### 3. Startup Wiring & Headless Daemon Alignment (`src-tauri/src/lib.rs` & `src-tauri/src/bin/daemon.rs`)
- In `lib.rs`: Invoked `reconcile_orphan_processes()` during application `setup()`, broadcasting `process-reconciliation-summary` via the daemon event channel.
- In `daemon.rs`: Executed `reconcile_orphan_processes()` alongside `close_stale_agent_instances()`.
- Registered `get_project_supervised_processes_cmd` and `reconcile_orphan_processes_cmd` in `tauri::generate_handler!`.

### 4. Frontend Types & Telemetry (`src/types/index.ts` & `src/stores/coreStore.ts`)
- Added TypeScript contracts: `SupervisedProcessRecord` and `ReconciliationSummary`.
- Registered `process-reconciliation-summary` listener in `coreStore.ts`, routing startup recovery counts and freed ports directly to `RollingStatusLog.vue`.

### 5. Living Specifications Synchronization
- Updated `docs/specs/01_service_orchestrator.md` to Build 0.2.3, detailing `supervised_processes` ledger, whole-tree teardown, and boot recovery.
- Updated `docs/specs/08_native_process_supervisor.md` to Build 0.2.3, adding Section 2.8 on process attestation, state transition lifecycle, and Tauri IPC commands.

---

## Verification & Compilation
- `cargo check`: Passed with 0 errors across all backend crates.
- `cargo test --lib supervisor::tests`: All 6 tests passed cleanly, validating `/proc` environment variable extraction and clean sweep logic.
- `npm run build` (`vue-tsc --noEmit && vite build`): Passed cleanly with zero TypeScript errors.
