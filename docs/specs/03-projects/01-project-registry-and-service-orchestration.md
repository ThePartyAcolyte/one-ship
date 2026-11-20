# Project Registry, Scripts & Service Orchestration

## Purpose

Define how ONE-SHIP registers projects, persists project state in SQLite, declares scripts in `rula.toml`, supervises long-running services and run-to-completion tasks, recovers processes after daemon restarts, and presents them in the **Scripts** workspace tab (Project Scripts & Terminal Hub).

## Scope

In scope: relational persistence of projects and supervision data; project registration/removal; `rula.toml` script declarations and option expansion; `ProcessSupervisor` semantics; service and task commands; boot-time reconciliation; the Scripts tab UI.

Out of scope: profiler engines (see the profiling specs), the daemon API transport and MCP dialogue tools (see the platform specs), and the logging subsystem (see [System Logging & Status Terminal](03-system-logging-and-status-terminal.md)).

## Model / Design

### 1. Configuration & Relational Persistence

- **SQLite database (`oneship.db`):** the core daemon manages an embedded SQLite database at `~/.config/one-ship/oneship.db`, running with `PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;`.
- **Data models:**

| Table | Purpose and columns |
| :--- | :--- |
| `projects` | Registered projects: `id`, `path`, `name`, `created_at`, `is_favorite`, `is_archived`, `hidden_tabs`. |
| `system_logs` | Structured platform logging: `id`, `timestamp`, `level`, `source`, `origin`, `message`; for instant querying, metrics and per-project/origin diagnostics. |
| `supervised_processes` | Relational audit ledger for every managed process (persistent services, deterministic tasks, interactive shells): `run_id`, `project_id`, `target_id`, `target_type`, `command`, `args_json`, `cwd`, `pid`, `pgid`, `env_identifier`, `status`, `exit_code`, `spawned_at_ms`, `terminated_at_ms`, `failure_reason`, `profiler_engine_id`, `profile_artifact_path`. Tracks execution identity, lifecycle and profiler artifacts. |
| `task_executions` | All CLI task executions and cognitive LLM task reports: `id`, `project_id`, `task_id`, `task_type`, `command`, `started_at`, `duration_ms`, `exit_code`, `status`, `summary`, `details_markdown`, `stdout_log`. |
| `project_initialization_flags` | Bootstrap and operational gaps: `flag_type` in `'docs'`, `'todos'`, `'tech_debt'`, `'profiler'`, `'invalid_config'`, `'removal'`; `status IN ('pending', 'resolved', 'dismissed')`; resolved independently in Phase 2. |

### 2. Project Registration & Removal

- **Registration (`add_project`):** requires an absolute path to an existing directory and stores its canonical form, so lookups by path match exactly.
- **Removal (`remove_project`):**
  1. Exports project knowledge to Markdown: `TECHNICAL_DEBT.md`, `TODO.md`, `SESSION_JOURNAL.md`, written in `docs/` when it exists, else the project root. Files are overwritten: the database is the source of truth while the project is in ONE-SHIP, and these files are imported at onboarding. The whole journal is exported.
  2. Optionally purges ONE-SHIP artifacts: restores the original `AGENTS.md`/`CLAUDE.md` from `.one-ship/backups/`, removes the managed MCP server entries from `.mcp.json`/`opencode.json`, removes every platform and specialized skill link, `rula.toml`, `.one-ship/`, declared persona/workflow documents and empty `.agents/` directories. Any failed step is reported.
  3. Removes snapshot refs.
  4. Deletes the project, its dependent rows and its initialization state in **one transaction**. Any failed step before the deletion aborts it and the project stays registered.
- **Failure handling:** the UI shows the failure report and offers to cancel or to hand it to the `removal-resolver` agent (`launch_project_removal_agent`). The handoff is recorded as a pending `removal` initialization flag (`metadata_json`: `purge_traces`, `failure_report`). The agent fixes what it can and calls `complete_project_removal`, which asks the user for a *Delete* approval and retries the removal.
- **Registry reads:** a registry read failure is an error, never an empty project list.

### 3. Local Project Config (`rula.toml`)

Within each project directory, services, tasks and options are declared:

- `[scripts.persistent.<id>]`: long-running background daemons, development servers and services.
- `[scripts.task.<id>]`: deterministic run-to-completion tasks (build, test, lint, migrations).
- `[scripts.*.<id>.options]`: configurable boolean flags and selection parameters chosen in `ScriptLaunchModal`. The backend expands them (`hydration::expand_script_options`):
  - a `boolean` option adds its `flag` when true;
  - a `select` option adds its `flag` and a value from `choices`;
  - an option omitted by the launch request takes its declared `default`;
  - undeclared keys, wrong value types and values outside `choices` are rejected.

### 4. Process Lifecycle & Native Supervision

- **`ProcessSupervisor`** (`src-tauri/src/supervisor.rs`): persistent services and transient tasks are spawned as supervised asynchronous child processes via Tokio.
- **Key namespace isolation** (guarantees zero collisions between agent personas, background services and user shells):

| Kind | Key | Notes |
| :--- | :--- | :--- |
| Persistent service | `${project_path}:svc:${service_id}` | e.g. `/home/.../one-ship:svc:dev` |
| Transient task | `${project_path}:tsk:${task_id}` | At most one concurrent execution of a given `task_id` per project (`SupervisorError::TaskAlreadyRunning`). `stdin` is piped while running; the output ring buffer is preserved in `retained_buffers` on completion so the Terminal Grid can inspect and interact with it. |
| Interactive shell | `${project_path}:svc:shell_${timestamp_ms}` | `launch_interactive_shell(project_id, shell_id)` requires the `shell_` prefix, so a shell never replaces a declared service. The supervisor is the source of truth for open shells (`list_interactive_shells(project_id)`); the terminal grid lists them again after a project or tab change. |

- **Attested identity invariant (`ONESHIP_RUN_ID`):** every spawned process gets a deterministic `run_id` and `ONESHIP_RUN_ID=<run_id>` injected into its environment. `/proc/<pid>/environ` can then verify ownership and identity without PID-recycling false positives.
- **Decoupled GUI lifecycle:** the GUI is a reactive telemetry lens. The process supervisor runs only in `oneship-daemon` and maintains process lifecycles, circular output ring buffers (`VecDeque<String>`, default capacity 500 lines) and stdin channels; services keep running when the app window closes.
- **State detection & telemetry:** running services are read from the supervisor's live process registry (`running_service_ids`: `Working` `svc` processes, shells excluded) and broadcast via the `running-services-updated` daemon event (`/api/events`) with `<project_id>:<service_id>` entries.
- **Command injection & graceful interruption:** inputs are written to the process's stdin pipe via `send_terminal_input`. Stopping a service terminates gracefully with bounded polling retry: `SIGTERM`, then `SIGKILL` if the grace period expires.

### 5. Boot-Time Startup Reconciliation (`reconcile_orphan_processes`)

- Executed only in `main()` of `oneship-daemon`, before serving requests. The GUI app supervises nothing; a second reconciliation would treat the daemon's live processes as orphans.
- Queries all `supervised_processes` rows with active statuses (`'starting'`, `'working'`).
- Reads `/proc/<pid>/environ` to check whether the process survived with a matching `ONESHIP_RUN_ID`.
- **Live persistent services (`target_type == "service"`):** re-adopted into the supervisor's in-memory registry with a background exit monitor, so dev servers survive hot daemon restarts.
- **Live ephemeral tasks (`target_type != "service"`):** killed across their process group (`kill(-pgid, SIGKILL)`), status set to `'orphaned_killed'`, and a line logged to `retained_buffers` (`[SYSTEM] Orphaned task was terminated during startup recovery...`).
- **Dead processes:** if `/proc/<pid>` no longer exists or `ONESHIP_RUN_ID` mismatches, status becomes `'crashed'` and the termination timestamp is recorded.

### 6. Project Scripts & Terminal Hub (`ProjectTerminalGrid.vue`)

The scripts tab (`workspaceTab === 'terminal'`, user-facing label **`Scripts`** in `ProjectWorkspace.vue`; the internal `'terminal'` routing is kept for hotkeys and IPC events) is the single authoritative hub.

**Overview de-cluttering:** the embedded `ScriptsSection` and the `project-card-body` `<ZoneOutlet />` were removed from `ProjectOverviewTab.vue` and `ProjectCard.vue`. The Overview is a telemetry/status dashboard only; script execution, service monitoring and task configuration live in `Scripts`.

**Left explorer rail**

- *Sessions:* `Interactive Shell` spawns or attaches to a managed interactive `/bin/bash -i` (or `$SHELL`) in `project.path`; `PLATFORM: supervisor` is the diagnostic log view of the platform daemon.
- *Persistent Services:* state indicators (`RUNNING`, `STOPPED`, `TRANSITIONING`); Start/Stop toggle, Graceful Restart, parameter modal (`ScriptLaunchModal`); every run is profiled continuously. Context menu (`AppContextMenu`): `Focus Console`, `Performance Telemetry` (navigates to the `Profiling` tab), `Restart Service`, `Stop Service` / `Start Service`, `Configure Options`, `Open History`.
- *Deterministic Tasks:* `RUN` trigger, execution spinner, last exit code badge (`EXIT 0` / `EXIT 1`), duration in ms, parameter modal. Context menu: `Run Task`, `View Last Report`, `Configure Flags`, `Open History`.
- *Header actions (title-less `ui-surface-title` block):* `History` (opens `TaskReportModal.vue` via the `oneship-open-history` UI event), `+ Shell` (spawn/focus a shell), and a full-width `.ui-search-box` row (`text-[10px]`, clear button) filtering scripts/tasks by name or command line in real time. The former `Open in [Terminal]` host launcher was removed; interactive sessions live entirely inside the hub.

**Right console viewport**

- *Stream:* reads logs via `read_terminal_output` and subscribes to the `task-output-chunk` and `task-completed` daemon SSE events (`/api/events`).
- *ANSI engine (`src/utils/ansi.ts`):* parses SGR sequences into HTML adhering to theme variables (16 standard/bright colors, bold, dim, underline, 256-color palettes).
- *Controls:* `ALL` vs `ERRORS ONLY` (semantic error extraction); `AUTO` autoscroll lock (disengages when the user scrolls up); `Clear`; `Copy`; `SIGINT` (closes the active shell via `closeShell` or stops the active service via `stopService`).
- *Stdin bar:* monospace prompt sending input directly to the running process or shell, with Up/Down command history.

## Contracts

### Service commands (`orchestrator.rs`, daemon commands via `daemonCall`)

| Command | Behavior |
| :--- | :--- |
| `start_service(project_id, service_id, options)` | Starts `[scripts.persistent.<service_id>]`. Command, arguments and cwd come only from `rula.toml`; an unreadable, invalid or missing declaration is an error. Starting a running service replaces it (also MCP `restart`). Returns `ServiceStartOutcome`: `{ kind: "started", pid }` or `{ kind: "port_occupied", port, pid, cmdline }` when the port is held by another process; the UI then offers `release_port`. |
| `stop_service(project_id, service_id)` | Stops a service or interactive shell; stopping one that is not running succeeds. |
| `release_port(port, target_pid)` | SIGKILLs the reported occupant and its process group, then polls until the port is free (`orchestrator.port_release_timeout_ms`, default 3000; `orchestrator.port_release_poll_step_ms`, default 50). A free port succeeds; a port now held by another PID is an error; an occupant in ONE-SHIP's own process or process group is refused. |

Errors are `OrchestratorError`, serialized as their message.

### Task commands (`kopeng/runner.rs`)

- `run_project_task(project_id, task_id, options)`: runs `[scripts.task.<task_id>]` to completion and returns its `task_executions` record. Command, args and cwd come only from `rula.toml`; options are expanded with declared defaults.
- MCP `start_project_task` uses `runner::start_project_task`: records the execution, runs it in the background, returns the execution ID plus a handle. `wait=true` awaits the handle up to `timeout_ms` (default `mcp.task_wait_timeout_ms`, 30000); on timeout the task continues in the background.
- Every recorded execution is closed: a process that fails to spawn is recorded `failed` with `exit_code` NULL. A failure to close the record is an error (logged at `ERROR` for background runs), never discarded. Status values: `running`, `completed`, `failed`.
- Errors are `RunnerError`, serialized as their message.

### Other contracts

- Terminal I/O: `send_terminal_input`, `read_terminal_output`, `launch_interactive_shell`, `list_interactive_shells`.
- Events: `running-services-updated`, `task-output-chunk`, `task-completed` (SSE `/api/events`); UI event `oneship-open-history`.
- Removal: `remove_project`, `launch_project_removal_agent`, `complete_project_removal`.
- Config keys: `orchestrator.port_release_timeout_ms` (3000), `orchestrator.port_release_poll_step_ms` (50), `mcp.task_wait_timeout_ms` (30000); ring buffer capacity 500 lines.

## Implementation Status

| Area | Status |
| :--- | :--- |
| Supervisor, key namespaces, `ONESHIP_RUN_ID`, reconciliation (`supervisor.rs`, `bin/daemon.rs`) | Implemented |
| Service/port commands (`orchestrator.rs`), task runner (`kopeng/runner.rs`) | Implemented |
| Project removal with resolver handoff (`kopeng/flag_resolver.rs`, `api.rs`) | Implemented |
| Scripts tab UI (`ProjectTerminalGrid.vue`) | Implemented (not re-verified in detail) |

## Related Specs

- [System Logging & Status Terminal](03-system-logging-and-status-terminal.md)
- [Greenfield Project Creation](10-greenfield-project-creation.md)
- [Git Workspace, Branches & Snapshots Cockpit](02-git-workspace-and-branches.md)
- [Project Category Index](README.md)

Supersedes legacy: `01_service_orchestrator.md`
