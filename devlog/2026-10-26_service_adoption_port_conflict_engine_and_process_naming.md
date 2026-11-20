# Development Session: 2026-10-26 — Service Adoption, Port Conflict Engine & Process Naming

## Context & Objectives
- **Target Problem 1 (Orphan Process Port Collisions):**
  When restarting ONE-SHIP or recovering from an unexpected daemon termination, persistent services (such as FastMCP or Odoo running under Uvicorn) often remained bound to network ports (e.g. port 8000). Previous reconciliation routines checked only the primary PID, ignoring multi-process worker trees reparented to `init` while retaining identical process group IDs (`PGID`) and `ONESHIP_RUN_ID` tags. As a result, starting the service again failed with "port already in use".
- **Target Problem 2 (Re-Attachment vs Indiscriminate Termination):**
  Blindly killing all processes on boot disrupted developer workflow. Persistent services should be cleanly adopted and re-attached across restarts to preserve uptime, socket listeners, and retained buffers.
- **Target Problem 3 (Dynamic Port Inspection & Interactive Conflict Resolution):**
  Rather than letting the underlying toolchain crash with a cryptic port error, ONE-SHIP should inspect `/proc/net/tcp` pre-flight, auto-adopt identical service runs, or return a strongly-typed conflict error that the UI can interactively offer to terminate and replace on demand.
- **Target Problem 4 (Dynamic Process Naming / Observability):**
  All agent processes previously presented generic binary names (`oneship` or `agy`), making it impossible for OS process viewers (`htop`, `ps`, `ss`, `lsof`) and developer shell plugins (Oh-My-Zsh port managers) to differentiate which agent and project owned each port or process.

---

## Key Changes & Implementations

### 1. Dynamic Process Naming (`argv[0]`)
- **Native Process Supervisor (`src-tauri/src/supervisor.rs`):**
  - Configured Unix `tokio::process::Command::arg0` for all spawned processes:
    - **Agents:** `oneship[<project_name>:<agent_id>]`
    - **Services:** `oneship-svc[<project_name>:<service_id>]`
    - **Tasks:** `oneship-task[<project_name>:<task_id>]`
  - Fully decoupled from conversation storage; zero side-effects on SQLite conversation history, UUID mappings, or session resumption.

### 2. Boot-Time Service Adoption & Whole-Tree Liveness Checking
- **Module `src-tauri/src/port_inspect.rs`:**
  - Implemented `scan_proc_for_oneship_run_ids() -> HashMap<String, Vec<u32>>`: single-pass scan of `/proc` matching `ONESHIP_RUN_ID` from `/proc/<pid>/environ` in ~2ms.
  - Implemented `find_port_listener(target_port: u16) -> Option<PortOccupant>`: directly parses `/proc/net/tcp` and `/proc/net/tcp6`, maps socket inodes to process file descriptors, extracts PID, PGID, command line, and `ONESHIP_RUN_ID`.
  - Implemented `terminate_port_occupant(occupant, timeout_ms)`: gracefully signals `SIGTERM`, polls with bounded backoff, and escalates to `SIGKILL`.
- **Supervisor Reconciliation (`src-tauri/src/supervisor.rs`):**
  - Refactored `reconcile_orphan_processes()`:
    - Checks direct PID, PGID (`libc::kill(-pgid, 0)`), and child worker processes from the single-pass `/proc` scan.
    - **Persistent Services (`target_type == "service"`):** If alive, adopts the process into `supervisor.processes` with retained ring buffers, sets status to `ProcessStatus::Working`, and launches an asynchronous watcher loop (`tokio::spawn`) with cancellation token support.
    - **Ephemeral Tasks (`target_type == "task"`):** Terminated with `SIGKILL` to prevent detached background script leaks, marked `'orphaned_killed'`.
    - **Dead Processes:** Marked `'crashed'`.

### 3. Pre-Flight Port Conflict Guard & IPC Commands
- **Service Spawn Guard (`src-tauri/src/supervisor.rs`):**
  - Added `detect_script_port(rula_port, command, args)` to extract target ports from `rula.toml` or CLI arguments (`--port`, `-p`).
  - Prior to launching a service, checks `find_port_listener(port)`:
    - If occupied by the same service instance (`svc_<id>_*`), transparently auto-adopts the running process.
    - If occupied by an external or conflicting process, returns an explicit error: `"Port {port} is already occupied by PID {pid} ('{cmdline}'). Release the port or terminate the conflicting process before starting service '{service_id}'."`
  - Enforced process group cleanup (`kill_process_and_group(pid, SIGKILL)`) on `child.wait()` exit to eliminate orphan child processes.
- **Tauri IPC Handlers (`src-tauri/src/orchestrator.rs` & `src-tauri/src/lib.rs`):**
  - Registered `check_port_occupant(port: u16) -> Option<PortOccupant>`.
  - Registered `release_port(port: u16, target_pid: Option<u32>) -> Result<CommandResult, String>`.

### 4. Interactive UI Port Conflict Resolution
- **Component `src/components/workspace/ProjectTerminalGrid.vue`:**
  - Added `PortConflictInfo` reactive state and regex parsing on `startService` failure.
  - Rendered a brutalist modal on port conflict showing:
    - Service name and occupied port number.
    - Conflicting PID and full command line.
    - Semantic theme variables (`--status-warn-color`, `--status-error-color`, `--status-warn-border`).
  - Added **"Kill Process & Start"** action calling `invoke('release_port', ...)` and automatically retrying service launch upon successful termination.

### 5. Living Specifications Synchronization
- **Spec `docs/specs/08_native_process_supervisor.md`:**
  - Sidetracked and updated Section 2.3 with dynamic `arg0` process naming conventions.
  - Updated Section 2.8 with Boot-Time Service Adoption, Whole-Tree `/proc` Scan Attestation, Pre-Flight Port Guards, and updated Lifecycle State Machine diagram.
  - Updated Section 3 IPC command table with `check_port_occupant` and `release_port`.

---

## Verification & Compilation
- `cargo check --manifest-path src-tauri/Cargo.toml`: Finished cleanly with 0 errors.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib port_inspect`: 2 tests passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib supervisor::tests`: 7 tests passed (including CLI flag parsing and retained buffer continuity).
- `npx vue-tsc --noEmit`: Cleanly passed with 0 TypeScript/Vue errors.
