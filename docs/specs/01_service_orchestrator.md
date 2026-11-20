# Spec 01: Project Scripts & Service Orchestration

**Document Version:** 2.4.0  
**Domain:** Persistent Services, Deterministic Tasks, Process Supervision & Terminal Hub  
**Status:** Implemented & Validated (Build 0.2.6)  
**Maintainer:** Bosmang (Lead Systems Architect & Engineer)

---

## 1. Configuration & Relational Persistence

*   **SQLite Relational Database (`oneship.db`):** The core daemon manages an embedded SQLite database (`~/.config/one-ship/oneship.db`) operating with Write-Ahead Logging (`PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;`).
*   **Data Models:**
    - `projects`: Tracks registered projects (`id`, `path`, `name`, `created_at`, `is_favorite`, `is_archived`, `hidden_tabs`). Registration (`add_project`) requires an absolute path to an existing directory and stores its canonical form, so lookups by path match exactly. Removal (`remove_project`) exports the project knowledge to Markdown (`TECHNICAL_DEBT.md`, `TODO.md`, `SESSION_JOURNAL.md` in `docs/` when it exists, else the root; overwritten, since the database is the source of truth while the project is in ONE-SHIP and these files are imported at onboarding; the whole journal is exported), optionally purges ONE-SHIP artifacts (restores the original `AGENTS.md`/`CLAUDE.md` from `.one-ship/backups/`, removes the managed MCP server entries from `.mcp.json`/`opencode.json`, every platform and specialized skill link, `rula.toml`, `.one-ship/`, declared persona/workflow documents and empty `.agents/` directories; any failed step is reported) and snapshot refs, then deletes the project, its dependent rows and its initialization state in one transaction; any failed step before the deletion aborts it and the project stays registered. The UI then shows the failure report and offers to cancel or to hand it to the `removal-resolver` agent (`launch_project_removal_agent`), recorded as a pending `removal` initialization flag (`metadata_json`: `purge_traces`, `failure_report`). The agent fixes what it can and calls `complete_project_removal` (Spec 12), which asks the user for a Delete approval and retries the removal. A registry read failure is an error, never an empty project list.
    - `system_logs`: Structured platform logging (`id`, `timestamp`, `level`, `source`, `origin`, `message`) for instant querying, metrics, and per-project/origin diagnostic introspection.
    - `supervised_processes`: Relational audit ledger for every managed process (persistent services, deterministic tasks, and interactive shells) tracking execution identity, lifecycle, and profiler artifacts (`run_id`, `project_id`, `target_id`, `target_type`, `command`, `args_json`, `cwd`, `pid`, `pgid`, `env_identifier`, `status`, `exit_code`, `spawned_at_ms`, `terminated_at_ms`, `failure_reason`, `profiler_engine_id`, `profile_artifact_path`).
    - `task_executions`: Records all CLI task executions and cognitive LLM task reports (`id`, `project_id`, `task_id`, `task_type`, `command`, `started_at`, `duration_ms`, `exit_code`, `status`, `summary`, `details_markdown`, `stdout_log`).
    - `project_initialization_flags`: Tracks project bootstrap and operational gaps (`flag_type` supporting `'docs'`, `'todos'`, `'tech_debt'`, `'profiler'`, `'invalid_config'`, and `'removal'`; `status IN ('pending', 'resolved', 'dismissed')`) for independent Phase 2 resolution.
*   **Local Project Config (`rula.toml`):** Within each project directory, fine-grained services, deterministic tasks, and options are defined declaratively:
    - `[scripts.persistent.<id>]`: Long-running background daemons, development servers, and services.
    - `[scripts.task.<id>]`: Deterministic run-to-completion tasks (build, test, lint, migrations).
    - `[scripts.*.<id>.options]`: Configurable boolean flags and selection parameters chosen in `ScriptLaunchModal`. The backend expands them (`hydration::expand_script_options`): a `boolean` option adds its `flag` when true, a `select` option adds its `flag` and a value from `choices`, and an option the launch request omits takes its declared `default`. Undeclared keys, wrong value types and values outside `choices` are rejected.

---

## 2. Process Lifecycle & Native Supervision

*   **Native Process Supervision (`ProcessSupervisor`):** Persistent services and transient tasks are spawned as supervised asynchronous child processes via Tokio in `src-tauri/src/supervisor.rs`.
*   **Key Namespace Isolation:**
    - **Persistent Services:** Keyed as `${project_path}:svc:${service_id}` (e.g. `/home/.../one-ship:svc:dev`).
    - **Transient Tasks:** Keyed as `${project_path}:tsk:${task_id}`. At most one execution of a given `task_id` may run concurrently per project (`SupervisorError::TaskAlreadyRunning`); `stdin` is piped while running and the output ring buffer is preserved in `retained_buffers` on completion so the Terminal Grid pane can inspect and interact with it directly.
    - **Interactive Shell:** Keyed as `${project_path}:svc:shell_${timestamp_ms}`. `launch_interactive_shell(project_id, shell_id)` requires the `shell_` prefix, so a shell never replaces a declared service. The supervisor is the source of truth for open shells (`list_interactive_shells(project_id)`): the terminal grid lists them again after a project or tab change.
    This strict namespace guarantees zero collisions between agent personas, background services, and user shells.
*   **Attested Identity Invariant (`ONESHIP_RUN_ID`):** Every spawned process is assigned a deterministic `run_id` and has `ONESHIP_RUN_ID=<run_id>` injected into its environment. This invariant guarantees that `/proc/<pid>/environ` can verify process ownership and identity without risk of PID recycling false-positives.
*   **Decoupled GUI Lifecycle:** The ONE-SHIP GUI acts as a reactive telemetry lens. The process supervisor runs only in `oneship-daemon` (Spec 12 §7.2) and maintains process lifecycles, circular output ring buffers (`VecDeque<String>`, default capacity: 500 lines), and stdin channels; services keep running when the app window closes.
*   **Service Commands (`orchestrator.rs`, daemon commands called with `daemonCall`):**
    - `start_service(project_id, service_id, options)`: starts `[scripts.persistent.<service_id>]` of a registered project. Command, arguments and working directory come only from `rula.toml`; an unreadable, invalid or missing declaration is an error. Starting a running service replaces it (also the MCP `restart` action). Returns `ServiceStartOutcome`: `{ kind: "started", pid }` or `{ kind: "port_occupied", port, pid, cmdline }` when the service's port is held by another process (Spec 08 §2.4); the UI then offers `release_port`.
    - `stop_service(project_id, service_id)`: stops a service or interactive shell; stopping one that is not running succeeds.
    - `release_port(port, target_pid)`: SIGKILLs the reported occupant and its process group, then polls until the port is free (`orchestrator.port_release_timeout_ms`, default 3000; step `orchestrator.port_release_poll_step_ms`, default 50). A port already free succeeds; a port now held by another PID is an error, and an occupant in ONE-SHIP's own process or process group is refused.
    - Errors are `OrchestratorError`, serialized as their message.
*   **Task Commands (`kopeng/runner.rs`):**
    - `run_project_task(project_id, task_id, options)`: runs `[scripts.task.<task_id>]` of a registered project to completion and returns its `task_executions` record. Like services, command, arguments and working directory come only from `rula.toml` and options are expanded by the backend with declared defaults.
    - The MCP `start_project_task` uses `runner::start_project_task`, which records the execution, runs it in the background and returns the execution ID plus a handle; `wait=true` awaits that handle up to `timeout_ms` (default `mcp.task_wait_timeout_ms`, 30000) and the task continues in the background on timeout.
    - Every recorded execution is closed: a process that fails to spawn is recorded as `failed` with no exit code (`exit_code` NULL). A failure to close the record is an error (logged at `ERROR` for background runs), never discarded. Status values are `running`, `completed`, `failed`.
    - Errors are `RunnerError`, serialized as their message.
*   **State Detection & Telemetry:** Running services are read from the supervisor's live process registry (`running_service_ids`: `Working` `svc` processes, shells excluded) and broadcast to the frontend via the `running-services-updated` daemon event (`/api/events`) (`<project_id>:<service_id>` entries).
*   **Command Injection & Graceful Interruption:**
    - Inputs and commands are dispatched directly down the process's standard input pipe via `send_terminal_input`.
    - Stopping a service performs graceful termination with bounded polling retry (`SIGTERM` followed by `SIGKILL` escalation if grace period expires).
*   **Boot-Time Startup Reconciliation Engine (`reconcile_orphan_processes`):**
    - Executed only in `main()` of `oneship-daemon` before serving requests (the GUI app supervises nothing, so a second reconciliation would treat the daemon's live processes as orphans).
    - Queries all rows in `supervised_processes` with active statuses (`'starting'`, `'working'`).
    - Reads `/proc/<pid>/environ` to verify whether the process survived a daemon restart or unexpected termination with a matching `ONESHIP_RUN_ID`.
    - **Live Persistent Services (`target_type == "service"`):** Re-adopted into the supervisor's in-memory process registry with a background exit monitor so running dev servers survive hot daemon restarts without interruption.
    - **Live Ephemeral Tasks (`target_type != "service"`):** Terminated across their entire process group (`kill(-pgid, SIGKILL)`), updated to `'orphaned_killed'` in SQLite, and logged in `retained_buffers` (`[SYSTEM] Orphaned task was terminated during startup recovery...`).
    - **Dead Processes:** If `/proc/<pid>` no longer exists or `ONESHIP_RUN_ID` does not match, updates database status to `'crashed'` and records the termination timestamp.

---

## 3. Project Scripts & Terminal Hub (`ProjectTerminalGrid.vue`)

The workspace's scripts tab (`workspaceTab === 'terminal'`) serves exclusively as the single authoritative **Project Scripts & Terminal Hub** (rendered with the user-facing tab label **`Scripts`** in `ProjectWorkspace.vue` while maintaining internal `'terminal'` routing for hotkeys and IPC events):

### 3.1 Overview De-cluttering & Single Source of Truth
*   **Decommissioned Overview Card Outlet:** The embedded `ScriptsSection` and the `project-card-body` `<ZoneOutlet />` have been decommissioned from `ProjectOverviewTab.vue` and `ProjectCard.vue`. The Project Overview acts purely as a high-level telemetry and status dashboard; all script executions, service monitoring, and task configurations are consolidated into the `Scripts` tab.

### 3.2 Left Explorer Rail (Scripts & Sessions)
*   **Sessions Section:**
    - `Interactive Shell`: Spawns or attaches to a managed interactive `/bin/bash -i` (or `$SHELL`) session running directly in `project.path`.
    - `PLATFORM: supervisor`: Diagnostic log view of the underlying platform daemon.
*   **Persistent Services Section:**
    - Visual indicators for service states (`RUNNING`, `STOPPED`, `TRANSITIONING`).
    - One-click actions: Start/Stop toggle, Graceful Restart, and Parameter Config Modal (`ScriptLaunchModal`). Every run is profiled continuously (Spec 25).
    - **Right-Click Context Menu (`AppContextMenu`):** Exposes `Focus Console`, `Performance Telemetry` (navigates to the `Profiling` workspace tab), `Restart Service`, `Stop Service` / `Start Service`, `Configure Options`, and `Open History`.
*   **Deterministic Tasks Section:**
    - Execution trigger (`RUN`), real-time execution spinner, last exit code badge (`EXIT 0` / `EXIT 1`), duration in milliseconds, and parameter modal.
    - **Right-Click Context Menu (`AppContextMenu`):** Exposes `Run Task`, `View Last Report`, `Configure Flags`, and `Open History`.
*   **Header Actions (title-less `ui-surface-title` block):**
    - `History`: Dedicated button opening the execution history and task reports modal (`TaskReportModal.vue`) via the `oneship-open-history` UI event.
    - `+ Shell`: Fast button to spawn or focus an interactive shell session.
    - Search filter: Full-width `.ui-search-box` row below the actions (`text-[10px]`, clear button), filtering scripts and tasks by name or command line in real time.
    - The former `Open in [Terminal]` host launcher was removed; interactive sessions live entirely inside the hub.

### 3.3 Right Console Viewport (Live Terminal & Stdin)
*   **Real-time Stream:** Continuously reads process logs via `read_terminal_output` and subscribes to live `task-output-chunk` and `task-completed` daemon SSE events (`/api/events`) with high-performance ANSI escape sequence rendering.
*   **ANSI Color Engine (`src/utils/ansi.ts`):** Parses SGR escape sequences into stylized HTML elements adhering to brutalist theme variables (16 standard/bright colors, bold, dim, underline, and 256-color palettes).
*   **Viewport Controls:**
    - `ALL` vs `ERRORS ONLY`: Semantic error extraction filtering out noise to show only failures and tracebacks.
    - `AUTO`: Autoscroll lock keeping the viewport anchored to the bottom as new lines arrive, automatically disengaging when the user manually scrolls up.
    - `Clear`: Flushes the local log view.
    - `Copy`: Copies formatted output to the system clipboard.
    - `SIGINT`: Terminates the active interactive shell (`closeShell`) or stops the active service (`stopService`).
*   **Bidirectional Stdin Bar:** Monospace command prompt located at the bottom of the console allowing users to send keyboard inputs or CLI commands directly to the running process or interactive shell, complete with command history (Up/Down arrow key navigation).
