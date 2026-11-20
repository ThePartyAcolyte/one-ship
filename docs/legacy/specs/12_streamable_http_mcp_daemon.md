> [!WARNING]
> **LEGACY SPECIFICATION:** Superseded by [01-architecture/04-streamable-http-mcp-daemon.md](../../specs/01-architecture/04-streamable-http-mcp-daemon.md) and [01-architecture/05-daemon-operator-api-and-lifecycle.md](../../specs/01-architecture/05-daemon-operator-api-and-lifecycle.md). Preserved for historical provenance only.

# Streamable HTTP MCP Daemon & Multi-Agent Delegation Architecture

## 1. Overview & Motivation
Historically, ONE-SHIP provided its Model Context Protocol (MCP) interface by spawning a CLI child process using standard input/output (`--mcp` via `stdio`). While suitable for early prototypes, this approach exhibited severe operational shortcomings:
1. **Zombie Processes & Stale Code:** When developing or updating ONE-SHIP, active terminal sessions holding an open child process continue executing stale binaries loaded into RAM.
2. **Local Machine Contention:** Multiple agents running simultaneously would spawn independent subprocesses without a shared in-memory state or centralized connection pool.
3. **Context Bloat:** Exposing exhaustive sets of domain tools overwhelms LLM context windows and increases latency.

Inspired by the Streamable HTTP transport and dynamic tool lazy-loading of `odoo-mcp`, ONE-SHIP migrated definitively to a **local Streamable HTTP Daemon** conforming to modern MCP standards, with **Zero Backward Compatibility** and **Zero Dead Code**.

---

## 2. Server Topology & Transport

```
+------------------------------------------------------------------------------------------------+
|                             ONE-SHIP STREAMABLE HTTP TOPOLOGY                                  |
+------------------------------------------------------------------------------------------------+

 [Clients: Antigravity IDE / Subagents / External LLMs]
         |
         | HTTP POST /mcp (JSON-RPC 2.0)
         v
+------------------------------------------------------------------------------------------------+
| Axum Asynchronous HTTP Daemon (http://127.0.0.1:<mcp_port>/mcp)                                |
|   - Bound to dynamic port (default: 8765, configurable via mcp.mcp_daemon_port)                 |
|   - Stateless request handling: no hanging persistent TCP connections                          |
|   - CORS restricted to the ONE-SHIP webview origins & HTTP request tracing                     |
+------------------------------------------------------------------------------------------------+
         |
         v
+------------------------------------------------------------------------------------------------+
| JSON-RPC 2.0 Dispatcher (src-tauri/src/mcp.rs)                                                 |
|   - initialize: returns protocol "2024-11-05", capabilities, serverInfo "one-ship v0.2.4"      |
|   - notifications/initialized: returns status ok                                               |
|   - tools/list: partitioned tools (Meta, Core, Domain, Telemetry, Flags)                       |
|   - tools/call: executes requested tool and updates state                                      |
+------------------------------------------------------------------------------------------------+
         |                                                       |
         v                                                       v
+-------------------------------+                       +-------------------------------+
| Delegation Ledger (SQLite)    |                       | Native Process Supervisor     |
|   - ~/.config/one-ship/       |                       |   - supervisor::              |
|     oneship.db                |                       |     prompt_agent              |
|   - db_insert_delegation      |                       |   - Real-time event streaming |
|   - Task correlation & audit  |                       |     and status callbacks      |
+-------------------------------+                       +-------------------------------+
```

### 2.1 Configuration
Because Antigravity CLI and external agents do not load MCP server configurations from arbitrary project root directories, ONE-SHIP's Streamable HTTP MCP server is registered globally in `~/.gemini/config/mcp_config.json`:
```json
{
  "mcpServers": {
    "one-ship": {
      "serverUrl": "http://localhost:8765/mcp"
    }
  }
}
```
Because the server is declared globally, strict process-origin scoping and project attribution guards are mandatory to distinguish internal managed agents from external development assistants.

---

## 3. Tool Partitioning & Dynamic Capabilities

To prevent context bloat, tools are partitioned into functional domains:
1. **Core & Orchestration:**
   - `discover_project`: Returns unified project discovery manifest (metadata, git branch, specialist agents, persistent services, and finite tasks with options schemas). Auto-resolves to caller's project context.
   - `list_projects`: Lists all registered projects in ONE-SHIP with configured agent personas.
   - `query_project_agents`: Queries configured agent personas, boundary prompts, and supervisor statuses for a project.
   - `delegate_agent_task`: Asynchronously assigns a task from an originator agent to a local or cross-project target specialist.
   - `report_task_status`: Notifies ONE-SHIP of task completion, errors, or obstacles. Automatically triggers closed-loop callback to the originator and dispatches next queued turns.
   - `query_workspace_state`: Returns live telemetry on active supervisor processes.
   - `inspect_terminal_output`: Live terminal inspection of any companion agent or service pane with optional semantic error filtering (`filter: "errors_only"`).
   - `query_project_audit`: Queries the chronological timeline of inter-agent interactions, instructions, and outcomes.

2. **Deterministic Script & Task Lifecycle:**
   - `start_project_task`: Starts a declared finite task (`[scripts.task]`) in the background and returns its execution ID (`exec_id`). `options` holds values for the task's declared options (omitted ones take their default, undeclared keys are rejected). With `wait: true` it awaits completion up to `timeout_ms` (default `mcp.task_wait_timeout_ms`, 30000) and returns exit code and logs; on timeout the task continues and the agent polls `query_project_task`. An ID that is not a declared task is an error that points persistent services to `manage_project_script`.
   - `query_project_task`: Queries execution lifecycle, duration, exit code, and live terminal buffers for an `exec_id`.
   - `cancel_project_task`: Cancels running task executions under the supervisor.
   - `manage_project_script`: Starts, stops, or restarts persistent background services (`[scripts.persistent]`) through the same `orchestrator` functions as the UI; launch options take their `rula.toml` defaults, and a restart is a start (it replaces the running instance). A port held by another process is reported as an error asking the agent to have the user release it. Only declared services can be managed (never a user's shell); an ID that is not a declared service is an error that points finite tasks to `start_project_task`.
   - `list_project_scripts`: Lists configured persistent services and finite tasks with live statuses.

3. **Dialogue & HITL Governance:**
   - `post_agent_response`: Emits dialogue text for TTS/speech playback and structured markdown artifacts. Drains pending input queues upon turn completion.
   - `request_approval`: Prompts the developer in the Chat Overlay for explicit confirmation before executing privileged actions.

4. **Project Initialization & Lifecycle Flags:**
   - `raise_initialization_flag`: Emits a pending project initialization flag (`docs`, `todos`, `tech_debt`, `profiler`) during Phase 1 reconnaissance.
   - `resolve_initialization_flag`: Resolves an initialization flag of the caller's own project during Phase 2 on-demand resolution sessions. A `removal` flag is rejected; it closes only through `complete_project_removal`.
   - `complete_project_initialization` (`initializer` only, no arguments): Closes the caller's project initialization as the agent's final action. It requires a valid `rula.toml` declaring `[project].id` and an initializer session journal entry (the Genesis milestone). If the `project_initializations` record was missing or unsynchronized, it auto-synthesizes the state instead of failing. On success it records `completed`, removes `oneship-initializer`, emits `agent-status-report` / `agent-step-update` and stops the agent. If prerequisites are missing (e.g. `rula.toml` or milestone), it returns a prescriptive error indicating the missing requirement and the exact MCP tool to satisfy it. A response from the initializer only marks the proposal as presented (`PROPOSAL_PRESENTED_PHASE`); the existence of `rula.toml` no longer closes the initialization.
   - `complete_project_removal` (`removal-resolver` only): Closes an aborted project removal (Spec 01 §1). Takes a `summary` of the fixed failures, shows a Delete approval and blocks until the user responds. Approved: the full removal is retried with the original `purge_traces`; success deletes the project and stops the agent, failure returns the new report. Rejected: the `removal` flag is dismissed, the project stays registered and the agent is stopped. Other responses are feedback returned to the agent.

5. **Journaling & State Management:**
   - `record_session_minutes`: Persists structured session devlogs, decisions, and milestones into SQLite.
   - `query_session_journal`: Queries historical session minutes, decisions, files touched, and technical debt for context continuity.
   - `manage_todo_item`: Queries, adds, completes, and updates tasks in `project_todos`.
   - `record_technical_debt`: Triage and recording of architectural debt.
   - `create_snapshot` / `list_snapshots` / `restore_snapshot`: Pre-refactor Git checkpoints and rollbacks.

6. **Telemetry & Profiling (Spec 25):**
   - `list_profiler_runs`, `get_run_profile`, `compare_profiler_runs` (Spec 25 §5). Profiling is continuous and bound to every run (no on-demand session tool).

---

## 4. Multi-Agent Delegation & Closed-Loop Callback (Local & Cross-Project)

HTTP is a stateless protocol. When a subordinate agent calls `report_task_status`, it sends an isolated HTTP request. To preserve context across agents and projects, the daemon uses an asynchronous relational ledger in SQLite (`oneship.db`):

1. **Zero-Burden Origin Attribution:**
   - The daemon resolves the calling process from the TCP connection and reads its `ONESHIP_AGENT_INSTANCE` identity (see §6). The originator agent, project and conversation come from the resolved `CallerContext`.
   - The LLM never supplies self-referential IDs. Targets in other projects are always addressed by explicit project `id`.
2. **Local and Cross-Project Delegation (`delegate_agent_task`):**
   - Allocates a unique `task_id` (e.g. `task-1788412247`).
   - If `target_project` is specified, routes to that project; otherwise defaults to caller's project.
   - If target agent status is `Working`, the supervisor enqueues the turn in a non-disruptive FIFO buffer (`pending_inputs`) rather than colliding with an active turn.
   - Persists delegation in `agent_delegations` with `originator_project` and `target_project`.
   - Inserts natural language record into `audit_journal` (`action_type = "delegation"`).
   - Prompts target agent in target project directory.
3. **Execution & Asynchronous Callback (`report_task_status`):**
   - Target agent completes work and calls `report_task_status`.
   - Looks up matching pending delegation in SQLite.
   - Routes callback notice to `originator_project` and prompts `originator_agent`.
   - Emits `agent-status-report` and updates `audit_journal`.
   - Automatically drains and dispatches the next queued turn from `pending_inputs` if present.
4. **Turn Conclusion & Queue Draining (`post_agent_response`):**
   - When an agent completes its turn via `post_agent_response`, the supervisor checks `pending_inputs` and dispatches the next turn automatically.
   - `post_agent_response` never reports a message as delivered unless its chat turn was stored: if the active Conversation cannot be resolved or the turn cannot be inserted, the tool returns `isError` ("NOT delivered; retry it") and logs `ERROR`.
5. **Failed Delegation Target:**
   - When the target agent's turn ends with `ERROR` (e.g. quota exhausted) or its process exits abnormally, `report_agent_failure` (supervisor) shows a `system` turn in the target's chat, marks the pending delegation for that exact target/project `failed` with the reason, and prompts the originator with a `[FAILED]` notice in the `report_task_status` format. The close is idempotent, so the turn error and the process exit of one incident notify the originator once.

---

## 5. Live Terminal Observability & Semantic Error Extraction

When an agent needs to inspect a companion agent or a failing service:
* `inspect_terminal_output(target, filter: "errors_only")`:
  * Reads the supervisor's bounded output ring buffer for the target process (agent, service, or task).
  * In `"errors_only"` mode, parses the terminal buffer for compiler errors, tracebacks, and fatal crash signatures.
  * Extracts only the relevant error window (3 lines before, 8 lines after), reducing LLM token consumption from ~1,500 tokens to <150 tokens.

---

## 6. Process-Attested Caller Identity (Zero Agent Burden)

> **Status: IMPLEMENTED in the backend, pending runtime verification.** Done: `agent_personas`/`agent_instances` registry, `ONESHIP_AGENT_INSTANCE` injection and termination in `spawn_agent`, `caller_identity::resolve_caller` with typed `CallerError`, `CallerContext` wired into `mcp_handler` and every tool handler, self-attribution parameters removed from tool schemas, prompts and skills, and the CWD-based cascade (`resolve_caller_cwd_from_peer_port`, `find_project_for_agent`, `resolve_project_with_fallback`, `infer_agent_id`) deleted. Not yet verified against live runners (`agy`, `claude`, `opencode`): see §6.8. Pending: removal of the remaining `projects[0]` defaults in the supervisor and frontend stores (§6.6).

### 6.1 Design Principles
1. **Runner-agnostic.** The MCP endpoint is global (`~/.gemini/config/mcp_config.json` for Antigravity, written once and shared by all agents), so identity MUST NOT depend on per-agent client configuration (URL, headers or env expansion in the client config). The only assumption is that ONE-SHIP spawns every managed agent process (`agy`, `claude`, `opencode`).
2. **Identity is attested by the kernel and the supervisor, never declared by the LLM.** No tool accepts the caller's own `project` or `agent_id` as input.
3. **Names are labels.** Agent display names are UI labels only. Internal identity is `agent_instance_id` (per process) and `agent_uid` (stable per persona within a project).
4. **Unresolvable means error.** Any failure to attribute the caller returns a typed error; there is no CWD, name-based, first-project or suffix-matching fallback (AGENTS.md rule 11).

### 6.2 Agent Instance Registry
At `spawn_agent`, before the child process starts, the supervisor generates `agent_instance_id` (UUID v4) and persists one row in SQLite table `agent_instances`:

| Column | Meaning |
|---|---|
| `instance_id` | UUID v4, primary key |
| `scope` | `project` or `system` (the project-less orchestrator agent `system`) |
| `project_id` | Owning project (FK `projects.id`); NULL iff `scope = system` |
| `agent_uid` | Stable persona identity (FK `agent_personas.agent_uid`); NULL iff `scope = system` |
| `runner` | `agy` / `claude` / `opencode` |
| `pid` | OS process id returned by spawn |
| `spawned_at_ms` | Spawn timestamp |
| `terminated_at_ms` | NULL while alive; set when the supervisor observes process exit |

Personas live in `agent_personas (agent_uid PK, project_id, display_name, created_at_ms, UNIQUE(project_id, display_name))`. `agent_uid` is created on first spawn of a `(project, display_name)` pair and never changes; the display name is a label only. The active conversation is NOT frozen at spawn: it is resolved at request time from the persona's active conversation, since topics switch during a session.

`spawn_agent` resolves identity **before** starting the process and fails if the project path is not registered. If registering the instance fails after spawn, the process is killed. On daemon boot (`oneship-daemon`, the sole supervisor owner), `close_stale_agent_instances()` closes every still-alive row in SQLite, because agent processes do not outlive the daemon unit.

The identity is injected into the child environment as `ONESHIP_AGENT_INSTANCE=<instance_id>` through the existing `cmd.envs(...)` path. Environment variables are inherited by child processes, so MCP clients running in subprocesses of the agent carry the same identity. Respawn creates a new `instance_id`; the previous row is closed.

The registry lives in SQLite owned by `oneship-daemon`, which both supervises agent processes and serves the MCP endpoints.

### 6.3 Request-Time Resolution Pipeline
For every `tools/call` request:
1. **Peer socket:** `axum::serve(..., into_make_service_with_connect_info::<SocketAddr>())` supplies the client's peer address. The MCP server port is shared by all clients; the **ephemeral source port** of each TCP connection is what distinguishes them.
2. **Socket → PID:** The daemon locates in `/proc/net/tcp{,6}` the socket whose full 4-tuple matches (`local = peer address`, `remote = 127.0.0.1:<mcp_port>`), takes its `inode`, and finds the owning `PID` by matching `socket:[<inode>]` in `/proc/<pid>/fd/*`. Matching on port suffix alone is prohibited.
3. **PID → instance:** Reads `ONESHIP_AGENT_INSTANCE` from `/proc/<pid>/environ`. If absent, the caller is unmanaged.
4. **Instance → context:** Looks up `agent_instances` by `instance_id`. The row must exist, have `terminated_at_ms IS NULL`, and the resolved `PID` must be the registered `pid` or one of its descendants (walk `ppid` upward, bounded by `identity_ancestor_walk_max_depth_count`).
5. **Build `CallerContext`** `{ instance_id, scope, project_id, project_path, agent_uid, agent_display_name, runner, pid }` and pass it to every tool handler. Implemented in `src/caller_identity.rs` (`resolve_caller`, blocking: invoke via `spawn_blocking`). The ancestor walk is bounded by `supervisor.identity_ancestor_walk_max_depth_count` (default 16).

A per-connection identity cache is deliberately deferred until the `/proc/*/fd` scan is measured as a bottleneck; if added, it must be bounded (`identity_cache_max_items_count`) and validated against PID start time to defeat PID/port reuse.

### 6.4 Typed Errors (`CallerError`, `thiserror`)
| Variant | Condition |
|---|---|
| `SocketUnresolved` | No socket row or owning PID found for the peer address |
| `ProcessGone` | The owning process exited before `/proc` could be read |
| `UnmanagedCaller` | Process has no `ONESHIP_AGENT_INSTANCE` (external CLI/pair-programming session) |
| `InstanceUnknown` | Identity not present in `agent_instances` |
| `InstanceTerminated` | Row exists but `terminated_at_ms` is set |
| `IdentityMismatch` | PID is neither the registered `pid` nor a descendant of it |

`UnmanagedCaller` preserves the former early scoping guard: external sessions (e.g. Antigravity or Claude Code pair-programming on the orchestrator itself) receive a JSON-RPC result with `isError: true` and a directive to render output in their own terminal. Zero state mutations, zero database insertions, zero daemon events. All other variants return a JSON-RPC error and log at `WARN` with `instance_id`, `pid` and peer port.

### 6.5 Tool Contract
- Tools derive project, agent and conversation exclusively from `CallerContext`. Parameters named `project`, `agent_id`, `originator_*` are removed from tool schemas for self-referential use.
- Cross-project operations (`delegate_agent_task`, `query_project_*`) take an explicit **target** project `id` and, when applicable, target `agent_uid`. Names are never used for resolution.
- `report_task_status` correlates by `task_id` against `agent_delegations`; the reporting agent is `CallerContext`, not a parameter.

### 6.6 Eliminated Mechanisms (deleted in the implementing cycle)
`resolve_caller_cwd_from_peer_port`, `find_project_for_agent`, `resolve_project_with_fallback`, CWD longest-prefix matching, persona scanning across projects, `ends_with(":{exec_id}")` process-key matching, composite-key parsing of `agent_id`, the `initializer` special case via `project_initializations`, and `projects[0]` defaults in the supervisor and frontend stores.

### 6.7 Tool Parameter Semantics (as implemented)
- `project` (optional on every project-scoped tool) is an explicit **target**, resolved strictly by exact `id`, absolute `path`, `name` (case-insensitive) or `rula.toml [project].id/uid`. No match is an error; several matches are an ambiguity error. Omitted or blank means the caller's own project; the `system` agent has none and must name a target.
- Process keys of task executions are built exactly from the execution record (`format_task_key(project_path, task_id, exec_id)`); suffix matching is gone.
- Target resolution reads `rula.toml` `[project].id/uid` of each registered project; an unreadable or invalid `rula.toml` is an error (`McpToolError::InvalidRula`), not a non-match. Helper failures are typed (`McpToolError`) and returned as the tool's error text.
- `post_agent_response` artifact paths (`file_path`, or artifact text read as a path) must be relative, made only of normal components, and resolve inside the project (symlinks included); `file_path` outside the project rejects the call, artifact text that is not such a path stays an ephemeral payload. Read and brain-recovery failures are returned, not ignored.
- `request_user_approval` and `request_approval` share one approval wait (`await_user_approval`); the pending-approval map lock recovers from poisoning.
- Tools that act as the caller (`post_agent_response`, `request_approval`, `request_user_approval`, `record_session_minutes`, `search_conversations`, `reopen_conversation`, `switch_conversation_topic`, `raise_initialization_flag`, `resolve_initialization_flag`, `complete_project_removal`, `complete_project_initialization`) operate on the caller's own project only.
- `query_system_logs` scopes to the target project (default: the caller's own); `project: "system"` selects orchestrator logs. An unscoped global query is no longer available.

### 6.8 Runtime Verification Checklist
1. Spawn one agent per runner; each must create an `agent_instances` row with a live `pid`.
2. `/proc/<pid>/environ` of that PID contains `ONESHIP_AGENT_INSTANCE=<instance_id>` (for `agy`, also check the MCP client subprocess if it differs from the spawned PID).
3. A `tools/call` from a managed agent returns a result; the same call from an external terminal returns the scoping rejection (`UnmanagedCaller`).
4. After the agent exits, `terminated_at_ms` is set and further calls with its identity fail with `InstanceTerminated`/`ProcessGone`.

### 6.9 Known Limits
- A single MCP client process hosting several managed agents cannot be told apart; not applicable to runners spawned one-process-per-agent.
- Internal sub-agents of one runner process share that process's identity by design.
- Linux-only (`/proc`), consistent with the single-machine target environment.

---

## 7. Server-Sent Events (SSE) Bus & Real-Time Telemetry

The daemon exposes an HTTP SSE stream at `GET /api/events` via `sse_events_handler` powered by a Tokio broadcast channel (`broadcast::Sender<DaemonEvent>`):

* **Event Types Broadcasted:**
  - `ApprovalRequested`: Interactive human-in-the-loop proposals requiring user action.
  - `AgentResponse`: Spoken dialogue and structured markdown artifacts from `post_agent_response`.
  - `agent-status-report`: Agent lifecycle status reports (`completed`, `failed`, `working`, `error`).
  - `agent-step-update`: Granular execution step progress (`state: "STEP"` / `"DONE"`).
  - `AccountQuotaUpdated`: Quota window telemetry and runway updates.

* **Build Identity:**
  - `GET /api/build` and `initialize.serverInfo.build_id` return `BUILD_ID` (epoch seconds of the `build.rs` run, which re-runs whenever `src/` changes). On every SSE connect `coreStore.checkDaemonBuild()` compares it with the app's `get_build_id_cmd`; a mismatch or a daemon without the endpoint is logged as `ERROR` in the status log. Reason: a long-running daemon on an older build reads a migrated database with stale SQL (observed 2026-10-09).
  - `init_database` refuses a database whose `PRAGMA user_version` is newer than the binary supports.

* **Sequence and resync:** every SSE event carries a per-connection sequence `id:`. When a client falls behind the broadcast buffer (`mcp.event_channel_capacity_count` events per subscriber, default 1024), the skipped events cannot be replayed: the daemon sends a `resync` event (`{"missed_count": n}`, logged at WARN) in their place, and `coreStore.resyncFromDaemon()` reloads the projects and pending approvals. Lag is never dropped silently.

* **One event bus:** the daemon runs every background loop (project and keyring watcher, host and service telemetry, continuous profiler, agent state watcher, quota watcher, OpenDictate D-Bus monitor, REA idle reaper, rearm worker) and the supervisor, so their events (`agent-states-updated`, `running-services-updated`, `agent-step-update`, `task-output-chunk`, `task-completed`, `agent-error`, `agent-usage-update`, `keyring-warmup-status`, `accounts-updated`, `opendictate_*`, `rea-queue-updated`, `projects-updated`, `project-files-changed`, `project-tasks-updated`, `init-flag-*`, …) exist only on `/api/events`. The app has no event channel and the frontend no Tauri `listen`.

* **Frontend Integration:**
  - The daemon URL comes from `mcp.mcp_daemon_port` (`get_daemon_port_cmd`, read once by `daemonClient.daemonBaseUrl()`); `coreStore.initDaemonSse()` owns the `/api/events` connection and binds each (re)connected `EventSource` to `src/utils/daemonEvents.ts`, where stores and components subscribe with `onDaemonEvent(name, handler)` / `listenDaemonEvent` and unsubscribe on teardown.
  - Pending approvals are hydrated only from SQLite (daemon command `get_pending_approvals`); there is no HTTP second source.

### 7.0 The Daemon Is a Required Subsystem

`oneship-daemon` is the only MCP server. The desktop app no longer starts an in-process MCP server when the daemon is absent: subsystems are not interchangeable, and a missing daemon is an error state, not a reason to substitute it. `coreStore.daemonStatus` is `connecting`, `connected`, `unreachable` or `build_mismatch`; the last two render a red indicator in the header (`ServerCrash`, detail in the tooltip) and an `ERROR` status-log line, while the SSE client keeps probing every 3 s until the daemon is back.

### 7.1 Operator API Guard (`/api/*`)

`/api/events`, `/api/approval-response`, `/api/build` and `/api/v1/call/<command>` (§7.2) serve only the ONE-SHIP app. `require_operator_caller` (route layer in `create_mcp_router`) applies three checks, all fail closed with HTTP 403 and a `WARN [OPERATOR API REJECTION]` log:

1. **Origin:** the request must carry an `Origin` in `TRUSTED_WEBVIEW_ORIGINS` (`http://localhost:1420`, the Vite `devUrl`; `tauri://localhost`, the bundled Linux webview). Browsers cannot forge `Origin`, so foreign web pages are rejected; clients without `Origin` (curl, scripts) are rejected too.
2. **Caller process:** the peer socket is resolved with `caller_identity::resolve_caller` (§6). Only `UnmanagedCaller` passes. A managed agent (or a child that inherited `ONESHIP_AGENT_INSTANCE`) is rejected even with a forged `Origin`, so an agent cannot approve its own pending approval. Any other resolution error rejects.
3. **App descendant:** the socket-holding process must be the ONE-SHIP app binary or descend from it (`caller_identity::require_app_descendant`: `/proc/<pid>/exe` compared with `one-ship` in the daemon's own directory, `operator_app_exe()`, walking at most `supervisor.identity_ancestor_walk_max_depth_count` parents; a binary replaced by a rebuild reads `<path> (deleted)` and matches; an ancestor whose `exe` is unreadable (`EACCES`: another user's process or one with capabilities, such as the compositor) is never the app, which runs as this user without capabilities, and the walk continues past it). The webview's requests come from WebKit's network process, a child of the app. This closes the bypass of check 2 where an agent strips `ONESHIP_AGENT_INSTANCE` (`env -u … curl -H 'Origin: tauri://localhost'`): its process still does not descend from the app.

Only the webview calls the operator API; the app's Rust side makes no request to the daemon. The daemon CORS layer allows only `TRUSTED_WEBVIEW_ORIGINS` (methods `GET`/`POST`, header `Content-Type`). `/mcp`, `/mcp/rea`, `/mcp/desigma`, and `/mcp/pcbcraft` keep per-tool process attestation and are unaffected, since MCP runners are not browsers.

### 7.2 Operator RPC API (`POST /api/v1/call/<command>`)

The daemon is the only supervisor and state owner; the app is an interface. Every domain command is served here (daemon ownership migration, completed 2026-11-06).

* **Request:** `POST /api/v1/call/<command>` with a JSON object of arguments (`{}` for none). Argument structs are owned by the domain module, `camelCase`, `deny_unknown_fields`.
* **Success:** HTTP 200 with the command's JSON result (`null` for none).
* **Failure:** `{"error": {"code": "<Enum>::<Variant>", "message": "…"}}` (`ApiErrorResponse`). Each domain error implements `api::ApiError` (`status()`, `code()`): 4xx for a request the caller can correct (400 invalid input, 404 not found, 409 conflict, 424 missing dependency), 502 for a failing external tool, 504 for a dev server that did not boot, 500 for a daemon failure. A wrapped error of another domain keeps its own status and code (`SupervisorError` inside `OrchestratorError` answers `SupervisorError::PortOccupied`). Failures are logged at WARN (4xx) or ERROR (5xx). RPC-layer errors are `ApiCallError`: `UnknownCommand` (404), `InvalidArguments` (400, also for a malformed body), `Serialize` (500), `Task` (500, the blocking-pool task of a synchronous command failed).
* **Execution:** `call!` awaits an async command; `blocking!` runs a synchronous one (SQLite, `/proc` or file reads) on the blocking pool (AGENTS.md §9.2).
* **Registry:** `api::API_COMMANDS` and `api::dispatch`. The frontend mirrors them in `src/types/daemonApi.ts` (`DaemonCommands`: args and result per command); the test `api_commands_match_the_frontend_map` fails when they diverge.
* **Types:** argument, result and error types derive `ts_rs::TS` with `#[ts(export)]`; `cargo test` writes them to `src/types/generated/` (`TS_RS_EXPORT_DIR` in `src-tauri/.cargo/config.toml`). Generated files are not edited by hand.
* **Client:** `src/utils/daemonClient.ts` `daemonCall(command, args)` rejects with `DaemonCallError` (status, code, message; `name` is the code) or `DaemonUnreachableError` (no connection, or a non-JSON answer). There is no fallback to Tauri IPC.
* **Commands:**
  - Stage 1: `run_diagnostics`, `install_missing_tools` (Spec 11 §1), `search_github_repos`, `search_gitlab_repos`, `link_project_remote`, `create_github_repo`, `create_gitlab_repo` (Spec 11 §3), `get_configured_agent_pair` (Spec 10 §1.2).
  - Stage 2 (every command that touches the supervisor, a live process or the keyring lock): services and tasks (`start_service`, `stop_service`, `release_port`, `run_project_task`); agents (`get_project_agent_state`, `launch_agent`, `restart_agent`, `stop_agent`, `interrupt_agent`, `prompt_agent`, `dispatch_git_instruction`); terminals (`read_process_output`, `read_terminal_output`, `send_terminal_input`, `launch_interactive_shell`, `list_interactive_shells`); personas (Spec 10); initialization and flag resolution (`initialize_project_agent`, `create_greenfield_project`, `launch_flag_resolution_agent`, `launch_project_removal_agent`); REA; Desigma; Conversations, Minutes, TODOs and technical debt (Spec 30); `get_pending_approvals`; accounts (Spec 17) and quota/metrics (Spec 19, 23).
  - Stage 3 (the remaining domains): projects (`get_projects`, `add_project`, `remove_project`, `toggle_project_favorite`, `toggle_project_archived`, `toggle_project_tab_visibility`), init flags (`get_project_init_flags`, `resolve_project_init_flag`), `migrate_project_deterministic`, `rollback_project_init`; task executions and logs (`get_task_executions`, `get_task_report`, `get_system_logs`, `get_system_log_sources`, `clear_system_logs`, `write_client_log`); configuration (`get_plugin_schemas`, `get_global_config`, `get_effective_project_config`, `get_scoped_project_config`, `save_global_config`, `save_project_rula_config`, `list_agent_runners`); desktop session (`get_omarchy_colors`, OpenDictate `reserve_voice_capture` / `cancel_voice_capture` / `get_opendictate_status`); git (Spec 09); skills (Spec 29); file tree, documents, comments and project files (Spec 20); snapshots (Spec 16); profiler and run history (`list_available_profilers`, `get_target_runs`, `get_run_profile`, `compare_runs`, Spec 25 §5; they replaced `get_continuous_profile`, `get_profiler_runs` and `compare_profiler_runs` in the profiler redesign). The commands that had a `_cmd` suffix lost it (`get_snapshot_diff`, `toggle_project_skill`, `create_document_comment`, …); `get_agent_runner_schema` and `open_system_terminal` were deleted (unused).
  - The full list is `api::API_COMMANDS`. These run in the daemon process, which must carry the user session environment (`DBUS_SESSION_BUS_ADDRESS` for OpenDictate, `WAYLAND_DISPLAY` for terminal and editor windows and `wl-paste`, a Polkit agent for `pkexec`, `gh` credentials, `SHELL` for interactive shells).
  - **Desktop windows** (`open_file_in_external_editor` through `xdg-open`, the `initiate_add_account` terminal) start through `desktop::desktop_command`: `systemd-run --user --scope`, a transient scope of their own. A window started inside the daemon's cgroup would be killed with the daemon unit (§7.3). `desktop::spawn_detached` reaps the process so it never stays a zombie of the daemon.
* **Tauri:** the app registers only `get_build_id_cmd` and `get_daemon_port_cmd`, which bootstrap the daemon connection, plus the dialog and URL opener plugins. It opens no database, validates no configuration, supervises nothing and logs to `oneship.log` only (`logger::use_file_sink_only`; its webview logs go through `write_client_log`).
* **Startup:** only the daemon reconciles orphan processes and closes stale agent instances; the app starts no process, loop or reconciliation. Processes outlive the app window and stop with the daemon (`stop_all_processes` on SIGINT/SIGTERM).

### 7.3 Interface Lifecycle & Development Launcher

* **Closing the window ends the interface process.** The app keeps no hidden window: every process and all state live in the daemon, so the interface is closed and relaunched freely while the daemon keeps running. Reopening reloads the webview.
* **Daemon shutdown:** on SIGINT/SIGTERM the daemon stops its supervised processes, then cancels its token. The token also ends every open `/api/events` stream (`take_until` in `sse_events_handler`); without that, axum's graceful shutdown would wait forever for the SSE connection of an open interface.
* **`scripts/run_dev.sh`** is the development launcher. Each component runs as a transient systemd user unit (`systemd-run --user`, project root, the caller's `PATH`), without a terminal; output goes to the journal. The user manager provides the session environment (`WAYLAND_DISPLAY`, `DBUS_SESSION_BUS_ADDRESS`, `HYPRLAND_INSTANCE_SIGNATURE`).

  | Unit | Runs | Notes |
  |---|---|---|
  | `oneship-dev-daemon` | `target/debug/oneship-daemon`, `RUST_LOG=debug` | `KillMode=mixed`, `TimeoutStopSec=30`: SIGTERM reaches only the daemon, which stops its supervised processes; whatever is left in its cgroup is SIGKILLed after the timeout. Supervised processes live in this cgroup and never outlive the unit. |
  | `oneship-dev-daemon-build` | `cargo build --no-default-features --bin oneship-daemon --bin one-ship` + `cargo test --lib export_bindings` (oneshot) | Single serialized builder for both binaries and `ts-rs` TypeScript bindings (`src/types/generated/`), preventing parallel `target/` lock contention. A failed build stays `failed` with its journal until the next build. |
  | `oneship-dev-daemon-watch` | `watchexec` on `src-tauri/src`, `src-tauri/Cargo.toml`, `src-tauri/build.rs` (`.rs`, `.toml`, debounce 500 ms, queued) | Runs `run_dev.sh rebuild-daemon` on every change: builds both binaries and `ts-rs` bindings first, restarts the daemon (and reloads `oneship-dev-gui` if it was open) only if the build succeeds, so a broken build keeps the running daemon and its agents. Requires `watchexec`; without it `run_dev.sh daemon` fails. |
  | `oneship-dev-vite` | `npm run dev` | Vite HMR for the frontend. |
  | `oneship-dev-gui` | `npm run tauri dev -- --no-watch` | Launches the interface against Vite and the prebuilt `one-ship` binary without spawning a competing Rust watcher; closing the window ends it. |

  | Subcommand | Effect |
  |---|---|
  | `all` (default) | Stop every unit, then start Vite, build both binaries + `ts-rs` bindings, start the daemon and its watcher, and start the interface. |
  | `daemon` | Build both binaries + `ts-rs` bindings, restart the daemon, start the watcher. |
  | `gui` | Focus the running interface (`hyprctl dispatch focuswindow pid:<pid>`), or ensure Vite, any in-flight build, and the daemon are ready before starting the interface unit. |
  | `stop daemon\|gui\|all` | `systemctl --user stop` on the units (daemon: watcher, build, daemon). |
  | `status` | One JSON line: `units.{daemon, daemon_build, daemon_watch, vite, gui}` with `active_state`, `sub_state`, `result`, `main_pid` and, for a failed unit, `log_tail` (last 15 journal lines of its last invocation); `daemon_listening_ports` and `vite_listening_ports` (from `ss`); `gui_window_pid` (the `one-ship` process); `build_progress` (phase, crate counts, percentage, current item, timestamps from `$XDG_RUNTIME_DIR/oneship-dev-build.json`). |
  | `logs <daemon\|build\|watch\|vite\|gui>` | Opens a terminal following the unit's journal (`journalctl --user -u <unit> -f`). |
  | `rebuild-daemon` | The watcher's action. |

* **Hot reload:** frontend edits reach the webview through Vite HMR; Rust edits are compiled serially by `watchexec` (`rebuild-daemon`), which builds both `oneship-daemon` and `one-ship` plus `ts-rs` bindings, restarts the daemon, and relaunches `oneship-dev-gui` if it was open. Running `tauri dev -- --no-watch` avoids duplicate Cargo builds and `target/` lock contention. Every daemon restart stops the agents and services it supervises.

* **Omarchy bar widget (`integrations/omarchy-bar/`, plugin id `tech.kirulab.one-ship`):**
  - Unified auto-adaptive Quickshell plugin that always reads the daemon's read-only `$XDG_RUNTIME_DIR/oneship-status.json` snapshot (working/idle/error agent counts, pending approvals, running services, and active agent list) without opening the operator API (§7.1).
  - **Development mode:** when symlinked from a checkout (`../../scripts/run_dev.sh` present and `oneship-dev-*` units active or `oneship-daemon.service` absent), automatically polls `run_dev.sh status` every `pollIntervalMs` (default 2000) to expose Cargo build progress, `watchexec`, Vite, and per-unit journal logs alongside live `oneship-status.json` telemetry.
  - **Production mode:** manages `oneship-daemon.service` via `systemctl --user` and launches/focuses `one-ship-launch`/`one-ship`.
  - **Editing the widget:** the shell does not reload a changed widget. Its file watcher covers `~/.config/omarchy/plugins/`, not the symlink target, and `omarchy-shell shell rescanPlugins` keeps the already loaded components. After editing `integrations/omarchy-bar/`, run `omarchy-restart-shell`; the dev units are systemd units and keep running. A widget older than `run_dev.sh status` fails silently in the bar, showing `TypeError` warnings in the Quickshell log (`/run/user/$UID/quickshell/by-id/<id>/log.log`).

### 7.4 Graphics Renderer Mode & Packaging Policy (Runtime IMPLEMENTED; installer/packaging PENDING)

The interface renders through WebKitGTK. `backdrop-filter` (Spec 13 §3.9) only works on the accelerated compositing path, so the renderer transport is a user-visible capability, not an implementation detail.

**Modes and environment mapping.** WebKitGTK reads these variables once per interface process:

| `[system] webkit_renderer_mode` | Environment set by `main()` | Transport set | Accelerated compositing | Backdrop blur |
|---|---|---|---|---|
| `dmabuf` (default) | none | `{SharedMemory, Hardware}` | yes | yes |
| `shared_memory` | `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1` | `{SharedMemory}` | yes | yes |
| `software` | `WEBKIT_DISABLE_DMABUF_RENDERER=1` | empty | no | no |

- `WEBKIT_DISABLE_DMABUF_RENDERER` is not a DMA-BUF-only switch in WebKitGTK 2.54.1: it empties `RendererBufferTransportMode` (`AcceleratedBackingStore::rendererBufferTransportMode()`) and disables accelerated compositing entirely. It is the last resort, never the default for NVIDIA hardware.
- A value of `0` in either variable is treated as unset by WebKitGTK.

**Resolution and restart semantics.**
- The transport set is latched in a process-wide `std::call_once`; changing the environment after the first backing store is created has no effect.
- `one-ship` resolves the mode in `main()` before `tauri::Builder` creates the webview, from the global-only `[system] webkit_renderer_mode` key in `config.toml`. This is a pre-boot typed read, not a daemon command: the webview must exist before the frontend can call the daemon.
- A change applies on the next interface start. Reloading the webview or reopening the window is insufficient. The daemon and its supervised processes are not affected.
- Settings exposes the selector with an explicit restart-required notice; the startup log records the resolved mode and its origin at INFO.

**`auto` resolution ladder.**
1. `dmabuf` on Intel/AMD with Mesa when `/dev/dri` is accessible and EGL exposes GBM or surfaceless plus `EXT_image_dma_buf_import`.
2. `shared_memory` when the NVIDIA proprietary driver is loaded (`/proc/driver/nvidia/version`), when the process cannot access `/dev/dri` (Flatpak/AppImage sandbox), or after a DMA-BUF startup failure.
3. `software` only when GTK cannot create a GL context (`Disabled hardware acceleration because GTK failed to initialize GL`), accepting the loss of backdrop blur and composited animations.

**Packaging constraints.**
- Installers can detect hardware and driver presence (`/proc/driver/nvidia/version`, `lspci -d 10de:`, `/sys/module/nvidia`), but that is not a reliable predictor: success depends on the active EGL vendor, driver version, compositor GPU, buffer modifiers, sandbox access to `/dev/dri`, and distro patches.
- Debian/Ubuntu ship `disable-nvidia-dmabuf.patch`, which empties the transport set on NVIDIA regardless of the environment; on those distros a packaged WebKitGTK may run without accelerated compositing (and therefore without backdrop blur) even with `dmabuf` selected. Arch does not carry that patch.
- The installer may only seed the initial `config.toml` value. The application must resolve at runtime and keep the manual override; the decision cannot be fixed at build time.
- The recommended packaged fallback is a launcher retry: start with `dmabuf`, and if the interface exits abnormally before the first frame, relaunch it once with `shared_memory` and record the demotion at WARN. The development launcher keeps the explicit `run_dev.sh stop gui` + `gui` flow instead.

