# Streamable HTTP MCP Daemon

## Purpose

Replace legacy `stdio`-based MCP child processes with a single local Streamable HTTP Model Context Protocol (MCP) daemon (`oneship-daemon`) that eliminates zombie processes, centralizes SQLite and process-supervisor state, enforces kernel-attested caller identity without LLM self-reporting, and partitions tools to avoid context bloat.

## Scope

- Streamable HTTP MCP transport (`POST /mcp` and domain gateways) and global client registration (`~/.gemini/config/mcp_config.json`).
- Tool partitioning, parameter semantics, and dynamic capability exposure across core orchestration, scripts/tasks, dialogue/HITL, initialization/removal flags, journaling/snapshots, profiling, project management, forge, and skills.
- Stateless HTTP multi-agent delegation and closed-loop asynchronous callback via SQLite (`agent_delegations`, `pending_inputs`, `audit_journal`).
- Live terminal observability and semantic error extraction (`inspect_terminal_output`).
- Linux `/proc` process-attested caller identity (`caller_identity.rs`), `agent_instances` / `agent_personas` registry, typed `CallerError`, and runtime verification/limits.

Daemon SSE broadcasting, the operator RPC API (`/api/v1/call/<command>`), interface lifecycle, and WebKitGTK renderer policy are specified in [Daemon Operator API and Lifecycle](05-daemon-operator-api-and-lifecycle.md).

## Model / Design

### Motivation and migration away from `stdio`

Historically, ONE-SHIP exposed its MCP interface by spawning a CLI child process over `stdio` (`--mcp`). That model produced three operational failures:

1. **Zombie processes and stale code:** active terminal sessions holding open child processes kept executing stale binaries in RAM after ONE-SHIP updates.
2. **Local machine contention:** concurrent agents spawned independent subprocesses with no shared in-memory state or centralized connection pool.
3. **Context bloat:** exposing exhaustive domain tool sets without partitioning overwhelmed LLM context windows and increased latency.

ONE-SHIP migrated definitively to a local **Streamable HTTP Daemon** (`oneship-daemon`) with zero backward compatibility and zero dead code.

### Server topology and transport

```text
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
|   - initialize: returns protocol "2024-11-05", capabilities, serverInfo (version + build_id)   |
|   - notifications/initialized: returns status ok                                               |
|   - tools/list: partitioned tools (Core, Scripts, Dialogue, Flags, Journal, Telemetry, etc.)   |
|   - tools/call: resolves CallerContext, executes requested tool, and updates state             |
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

Additional domain MCP gateway endpoints are mounted on the same router (`src-tauri/src/mcp.rs`): `/mcp/rea`, `/mcp/desigma`, and `/mcp/pcbcraft`.

#### Global client registration

Because Antigravity CLI and external runners do not load MCP server definitions from arbitrary project root directories, ONE-SHIP's Streamable HTTP MCP server is registered globally in `~/.gemini/config/mcp_config.json` (synced by `config::sync_antigravity_global_mcp_config`, which fails on a corrupt file rather than overwriting it):

```json
{
  "mcpServers": {
    "one-ship": {
      "serverUrl": "http://localhost:8765/mcp"
    }
  }
}
```

Because the server is declared globally, strict process-origin scoping and project attribution guards are mandatory to separate internal managed agents from external development assistants.

### Tool partitioning and dynamic capabilities

To prevent context bloat, MCP tools exposed by `tools/list` are partitioned into functional domains:

1. **Core and orchestration:**
   - `discover_project`: returns a unified project discovery manifest (registered metadata, git branch/VCS status, specialist agents with role/boundaries/status, persistent services, and finite tasks with options schemas). Auto-resolves to the caller's project context when omitted.
   - `list_projects`: lists all registered projects in ONE-SHIP with configured agent personas.
   - `query_project_agents`: queries configured agent personas, boundary prompts, and supervisor statuses for a project.
   - `delegate_agent_task`: asynchronously assigns a task from an originator agent to a local or cross-project target specialist.
   - `report_task_status`: notifies ONE-SHIP of task completion, errors, or obstacles; triggers a closed-loop callback to the originator and dispatches the next queued turn.
   - `query_workspace_state`: returns live telemetry on active supervisor processes.
   - `inspect_terminal_output`: inspects live terminal output of any companion agent, service, or task pane with optional semantic error filtering (`filter: "errors_only"`).
   - `query_project_audit`: queries the chronological timeline of inter-agent interactions, instructions, and outcomes.
   - `query_system_logs`: queries system logs scoped to the target project (default: caller's own; `project: "system"` selects orchestrator logs; unscoped global queries are disallowed).

2. **Deterministic script and task lifecycle:**
   - `start_project_task`: starts a declared finite task (`[scripts.task]`) in the background and returns its execution ID (`exec_id`). `options` provides values for declared task options (omitted keys take defaults; undeclared keys are rejected). With `wait: true`, awaits completion up to `timeout_ms` (default `mcp.task_wait_timeout_ms`, `30000`) and returns exit code and logs; on timeout, the task continues running and the agent polls `query_project_task`. Supplying an ID that is not a declared task returns an error pointing persistent services to `manage_project_script`.
   - `query_project_task`: queries execution lifecycle, duration, exit code, and live terminal buffers for an `exec_id`.
   - `cancel_project_task`: cancels running task executions under the supervisor.
   - `manage_project_script`: starts, stops, or restarts persistent background services (`[scripts.persistent]`) via the same `orchestrator` functions used by the UI. Launch options use `rula.toml` defaults, and restart replaces the running instance. A port held by another process is reported as an error instructing the agent to ask the user to release it. Only declared services can be managed (never a user's shell); an ID that is not a declared service returns an error pointing finite tasks to `start_project_task`.
   - `list_project_scripts`: lists configured persistent services and finite tasks with live statuses.

3. **Dialogue and human-in-the-loop (HITL) governance:**
   - `post_agent_response`: emits dialogue text for TTS/speech playback and structured markdown artifacts (`file_path` or inline `artifact`). Drains pending input queues upon turn completion.
   - `request_approval` / `request_user_approval`: prompts the developer in the Chat Overlay for explicit confirmation before executing privileged actions, sharing a single wait mechanism (`await_user_approval`) with a poison-resilient pending-approval map lock.

4. **Project initialization and lifecycle flags:**
   - `raise_initialization_flag`: emits a pending project initialization flag (`docs`, `todos`, `tech_debt`, `profiler`) during Phase 1 reconnaissance.
   - `resolve_initialization_flag`: resolves an initialization flag of the caller's own project during Phase 2 on-demand resolution sessions. A `removal` flag is rejected here (it closes only through `complete_project_removal`).
   - `complete_project_initialization` (`initializer` only, no arguments): closes the caller's project initialization as the agent's final action. Requires a valid `rula.toml` declaring `[project].id` and an initializer session journal entry (the Genesis milestone). If the `project_initializations` record is missing or unsynchronized, it auto-synthesizes the state instead of failing. On success it records `completed`, removes `oneship-initializer`, emits `agent-status-report` / `agent-step-update`, and stops the agent. Missing prerequisites return a prescriptive error naming the requirement and exact MCP tool to satisfy it. A standard response from the initializer only marks `PROPOSAL_PRESENTED_PHASE`; the mere presence of `rula.toml` never closes initialization.
   - `complete_project_removal` (`removal-resolver` only): closes an aborted project removal. Takes a `summary` of fixed failures, displays a Delete approval prompt, and blocks until the user responds. Approved: retries full removal with the original `purge_traces`; success deletes the project and stops the agent, while failure returns the new report. Rejected: dismisses the `removal` flag, keeps the project registered, and stops the agent. Other responses are returned to the agent as feedback.

5. **Journaling, knowledge, conversations, TODOs, technical debt, and snapshots:**
   - `record_session_minutes`: persists structured session devlogs, decisions, and milestones into SQLite.
   - `query_session_journal`: queries historical session minutes, decisions, files touched, and technical debt for context continuity.
   - `search_conversations`, `reopen_conversation`, `switch_conversation_topic`: inspects, reopens, and manages agent conversation topics.
   - `search_knowledge`, `get_file_line_insights`: queries the semantic vector index and line-level file insights.
   - `list_document_comments`, `resolve_document_comment`: reads and resolves review comments on project documents.
   - `manage_todo_item`, `resolve_todo_item`: queries, adds, completes, and updates items in `project_todos`.
   - `record_technical_debt`, `resolve_technical_debt`: triages, records, and resolves architectural debt.
   - `create_snapshot`, `list_snapshots`, `restore_snapshot`: creates, inspects, and rolls back pre-refactor Git checkpoints.

6. **Telemetry and continuous profiling:**
   - `list_profiler_runs`, `get_run_profile`, `compare_profiler_runs`: inspects and compares continuous profiler captures bound to script/task runs (no on-demand start/stop profiling session tool exists).

7. **Project management, forge collaboration, and skills (implemented in `mcp.rs`):**
   - `query_project_tasks`, `link_journal_to_task`, `manage_project_stage`, `manage_project_task`, `manage_project_milestone`, `manage_task_subtask`.
   - `forge_get_context`, `forge_list_issues`, `forge_get_issue`, `forge_list_prs`, `forge_get_pr`, `forge_list_releases`, `forge_list_inbox`, `forge_mark_item_read`, `forge_set_item_flag`, `forge_get_unread_counts`, `propose_forge_draft_pr`.
   - `search_project_skills`, `manage_project_skill`.

### Multi-agent delegation and closed-loop callback

HTTP is stateless: when a subordinate agent calls `report_task_status`, it sends an isolated request. The daemon preserves inter-agent and cross-project context using an asynchronous relational ledger in SQLite (`oneship.db`):

1. **Zero-burden origin attribution:** the daemon resolves the calling process from its TCP socket and reads `ONESHIP_AGENT_INSTANCE`. Originator agent, project, and conversation come strictly from `CallerContext`. The LLM never supplies self-referential IDs; targets in other projects are addressed by explicit target project identifier.
2. **Local and cross-project delegation (`delegate_agent_task`):**
   - Allocates a unique `task_id` (e.g. `task-1788412247`).
   - Routes to `target_project` if specified, otherwise defaults to the caller's project.
   - If the target agent status is `Working`, enqueues the turn in a non-disruptive FIFO buffer (`pending_inputs`) instead of colliding with an active turn.
   - Persists delegation in `agent_delegations` with `originator_project` and `target_project`.
   - Inserts a natural-language entry into `audit_journal` (`action_type = "delegation"`).
   - Prompts the target agent in the target project directory.
3. **Execution and asynchronous callback (`report_task_status`):**
   - Target agent finishes work and calls `report_task_status`.
   - Looks up the matching pending delegation in SQLite.
   - Routes the callback notice to `originator_project` and prompts `originator_agent`.
   - Emits `agent-status-report` and updates `audit_journal`.
   - Automatically drains and dispatches the next queued turn from `pending_inputs` if present.
4. **Turn conclusion and queue draining (`post_agent_response`):**
   - When an agent concludes its turn via `post_agent_response`, the supervisor checks `pending_inputs` and dispatches the next queued turn automatically.
   - `post_agent_response` never reports delivery unless the chat turn was persisted: if the active `Conversation` cannot be resolved or the turn insert fails, it returns `isError` (`"NOT delivered; retry it"`) and logs `ERROR`.
5. **Failed delegation target:**
   - When a target agent's turn ends with `ERROR` (e.g., quota exhausted) or its process exits abnormally, the supervisor's `report_agent_failure` inserts a `system` turn in the target's chat, marks the pending delegation for that exact target/project `failed` with the reason, and prompts the originator with a `[FAILED]` notice in the `report_task_status` format. Closure is idempotent so turn error and process exit for a single incident notify the originator once.

### Live terminal observability and semantic error extraction

`inspect_terminal_output(target, filter: "errors_only")` reads the supervisor's bounded output ring buffer for a companion agent, persistent service, or task execution. In `"errors_only"` mode, it scans the buffer for compiler errors, tracebacks, and fatal crash signatures and extracts only the relevant error window (3 lines before, 8 lines after), reducing LLM token consumption from roughly 1,500 tokens to under 150 tokens.

### Process-attested caller identity

#### Design principles

1. **Runner-agnostic:** the MCP endpoint is global (`~/.gemini/config/mcp_config.json`), shared by all agents. Identity must not depend on per-agent client URLs, custom headers, or client-side env expansion. The sole assumption is that ONE-SHIP spawns every managed agent process (`agy`, `claude`, `opencode`).
2. **Kernel and supervisor attested:** identity is derived from OS process metadata and supervisor records, never declared by the LLM. No tool accepts the caller's own `project` or `agent_id` as input.
3. **Names are labels:** agent display names are UI labels only. Internal identity is `agent_instance_id` (per OS process) and `agent_uid` (stable per persona within a project).
4. **Unresolvable means error:** any failure to attribute the caller returns a typed `CallerError`; there is no CWD, name-based, first-project, or suffix-matching fallback.

#### Agent instance registry

At `spawn_agent`, before starting the child process, the supervisor generates a UUID v4 `agent_instance_id` and persists one row in SQLite table `agent_instances`:

| Column | Meaning |
|---|---|
| `instance_id` | UUID v4, primary key |
| `scope` | `project` or `system` (the project-less orchestrator agent `system`) |
| `project_id` | Owning project (FK `projects.id`); `NULL` iff `scope = system` |
| `agent_uid` | Stable persona identity (FK `agent_personas.agent_uid`); `NULL` iff `scope = system` |
| `runner` | `agy` / `claude` / `opencode` |
| `pid` | OS process ID returned by spawn |
| `spawned_at_ms` | Spawn timestamp |
| `terminated_at_ms` | `NULL` while alive; set when the supervisor observes process exit |

Personas reside in `agent_personas (agent_uid PK, project_id, display_name, created_at_ms, UNIQUE(project_id, display_name))`. `agent_uid` is created on first spawn of a `(project, display_name)` pair and never changes. The active conversation is not frozen at spawn; it is resolved at request time from the persona's active conversation because topics switch during a session.

`spawn_agent` resolves identity before starting the process and fails if the project path is unregistered. If registering the instance fails after spawn, the child process is killed. On `oneship-daemon` boot, `close_stale_agent_instances()` closes all still-alive rows in SQLite because managed agent processes do not outlive the daemon unit.

The identity is injected into the child environment as `ONESHIP_AGENT_INSTANCE=<instance_id>` via `cmd.envs(...)`. Child subprocesses (such as runner MCP helper processes) inherit this variable. Respawning an agent creates a new `instance_id` and closes the previous row.

#### Request-time resolution pipeline

For every `tools/call` request (executed off the async executor via `tokio::task::spawn_blocking` in `caller_identity::resolve_caller`):

1. **Peer socket:** `axum::serve(..., into_make_service_with_connect_info::<SocketAddr>())` supplies the client's IPv4 loopback peer address; the ephemeral source port distinguishes concurrent connections sharing `mcp_daemon_port`.
2. **Socket to PID:** locates the socket in `/proc/net/tcp` whose full 4-tuple matches (`local = peer address`, `remote = 127.0.0.1:<mcp_port>`), extracts its `inode`, and locates the owning `PID` by matching `socket:[<inode>]` across `/proc/<pid>/fd/*`. Matching on port suffix alone is prohibited.
3. **PID to instance:** reads `ONESHIP_AGENT_INSTANCE` from `/proc/<pid>/environ`. Absence classifies the process as `UnmanagedCaller`.
4. **Instance to context:** queries `agent_instances` by `instance_id`. The row must exist, have `terminated_at_ms IS NULL`, and the resolved `PID` must be the registered `pid` or one of its descendants (walking `ppid` upward, bounded by `supervisor.identity_ancestor_walk_max_depth_count`, default `16`).
5. **Context construction:** builds `CallerContext { instance_id, scope, project_id, project_path, agent_uid, agent_display_name, runner, pid }` and supplies it to every tool handler.

A per-connection identity cache is deferred until `/proc/*/fd` scanning is measured as a bottleneck; if introduced, it must be bounded (`identity_cache_max_items_count`) and validated against PID start time to defeat PID/port reuse.

#### Tool contract and parameter semantics

- Self-attribution parameters (`agent_id`, `originator_*`, or self-referential `project`) are absent from tool schemas.
- Cross-project tools (`delegate_agent_task`, `query_project_*`) accept explicit target `project` and, where applicable, target `agent_uid`.
- `report_task_status` correlates by `task_id` against `agent_delegations`; the reporting agent comes from `CallerContext`.
- Optional `project` on project-scoped tools is strictly an explicit **target**, resolved by exact `id`, absolute `path`, `name` (case-insensitive), or `rula.toml` `[project].id`/`uid`. Zero matches or multiple matches are errors; omitted or blank defaults to the caller's project (`system` has none and must specify a target).
- Task execution process keys are constructed strictly from the execution record (`format_task_key(project_path, task_id, exec_id)`); suffix matching is prohibited.
- Target resolution reads `rula.toml` `[project].id`/`uid` of each registered project; an unreadable or invalid `rula.toml` returns `McpToolError::InvalidRula` rather than a silent non-match.
- `post_agent_response` artifact paths (`file_path`, or artifact text interpreted as a path) must be relative, composed solely of normal path components, and resolve inside the project directory (including symlinks). A `file_path` outside the project rejects the call; non-path artifact text remains an ephemeral payload. File read and brain-recovery failures are returned rather than ignored.
- Caller-only tools (`post_agent_response`, `request_approval`, `request_user_approval`, `record_session_minutes`, `search_conversations`, `reopen_conversation`, `switch_conversation_topic`, `raise_initialization_flag`, `resolve_initialization_flag`, `complete_project_removal`, `complete_project_initialization`) operate exclusively on the caller's own project.

#### Eliminated legacy mechanisms

The following legacy mechanisms were permanently deleted: `resolve_caller_cwd_from_peer_port`, `find_project_for_agent`, `resolve_project_with_fallback`, CWD longest-prefix matching, persona scanning across projects, `ends_with(":{exec_id}")` process-key matching, composite-key parsing of `agent_id`, the `initializer` special case via `project_initializations`, and `projects[0]` defaults in the supervisor and frontend stores.

#### Runtime verification checklist and known limits

**Verification checklist:**
1. Spawn one agent per runner (`agy`, `claude`, `opencode`); confirm each creates an `agent_instances` row with a live `pid`.
2. Confirm `/proc/<pid>/environ` of that PID (and its MCP client subprocess if distinct) contains `ONESHIP_AGENT_INSTANCE=<instance_id>`.
3. Confirm `tools/call` from a managed agent succeeds, while the same call from an external terminal returns the `UnmanagedCaller` scoping rejection.
4. Confirm that after the agent exits, `terminated_at_ms` is populated and subsequent calls with its identity fail with `InstanceTerminated` or `ProcessGone`.

**Known limits:**
- A single shared MCP client process hosting multiple independent managed agents cannot be disambiguated (not applicable to one-process-per-agent runners).
- Internal sub-agents spawned inside a single runner process share their parent process's `CallerContext` by design.
- Linux-only (`/proc`), aligned with the single-machine target platform.

## Contracts

- **HTTP endpoints (`src-tauri/src/mcp.rs`):** `POST /mcp`, `POST /mcp/rea`, `POST /mcp/desigma`, `POST /mcp/pcbcraft`, `POST /api/v1/audit/semantic` (local diagnostic endpoint for semantic audit scripts).
- **JSON-RPC 2.0 methods:** `initialize` (`protocolVersion: "2024-11-05"`), `notifications/initialized`, `tools/list` (`meta.ttlMs` from `mcp.approval_ttl_ms`, `meta.cacheScope: "private"`), `tools/call`.
- **Environment & global client config:** `ONESHIP_AGENT_INSTANCE` (`caller_identity::AGENT_INSTANCE_ENV_KEY`); `~/.gemini/config/mcp_config.json`.
- **Config keys:** `mcp.mcp_daemon_port` (default `8765`), `mcp.task_wait_timeout_ms` (default `30000`), `mcp.approval_ttl_ms`, `supervisor.identity_ancestor_walk_max_depth_count` (default `16`).
- **SQLite tables (`oneship.db`):** `agent_instances`, `agent_personas`, `agent_delegations`, `pending_inputs`, `audit_journal`, `project_todos`, `agent_session_journal`.
- **Typed `CallerError` variants (`caller_identity.rs`):**

| Variant | Condition | Handling |
|---|---|---|
| `SocketUnresolved` | No socket row or owning PID found for the peer address | JSON-RPC `-32001`, `WARN` log |
| `UnsupportedPeerAddress` | Peer address is not IPv4 loopback | JSON-RPC `-32001`, `WARN` log |
| `ProcessGone` | Owning process exited before `/proc` could be read | JSON-RPC `-32001`, `WARN` log |
| `UnmanagedCaller` | Process lacks `ONESHIP_AGENT_INSTANCE` (external CLI/pair-programming session) | JSON-RPC result `isError: true` with terminal-output directive; zero DB/event side effects |
| `InstanceUnknown` | `instance_id` not present in `agent_instances` | JSON-RPC `-32001`, `WARN` log |
| `InstanceTerminated` | Row exists but `terminated_at_ms` is set | JSON-RPC `-32001`, `WARN` log |
| `IdentityMismatch` | Calling PID is neither the registered `pid` nor one of its descendants | JSON-RPC `-32001`, `WARN` log |
| `DanglingReference` | Project-scoped instance points to a missing project or persona | JSON-RPC `-32001`, `WARN` log |
| `ProcIo` | `/proc` read failed for a reason other than process exit | JSON-RPC `-32001`, `WARN` log |
| `NotFromApp` | Operator API caller does not descend from `one-ship` binary | HTTP 403 (`require_operator_caller`) |
| `PeerAddressMissing` | HTTP layer supplied no `ConnectInfo<SocketAddr>` | JSON-RPC `-32001`, `WARN` log |
| `ResolverTaskFailed` | Blocking `spawn_blocking` resolver task panicked or cancelled | JSON-RPC `-32001`, `WARN` log |
| `Database` | SQLite lookup failed (`DatabaseError`) | JSON-RPC `-32001`, `WARN` log |

## Implementation Status

| Area | Status | Notes |
|---|---|---|
| Axum Streamable HTTP router (`mcp.rs`) | Implemented | Verified `create_mcp_router`, `/mcp`, `/mcp/rea`, `/mcp/desigma`, `/mcp/pcbcraft`, `/api/v1/audit/semantic`. |
| JSON-RPC dispatcher & 59 MCP tools (`mcp.rs`) | Implemented | Verified `initialize` (`protocolVersion: "2024-11-05"`), `tools/list`, `tools/call`. |
| Kernel `/proc` caller attestation (`caller_identity.rs`) | Implemented (pending full multi-runner live verification) | Verified `CallerContext`, `CallerError` (13 variants), `resolve_caller`, `AGENT_INSTANCE_ENV_KEY`. |
| Multi-agent delegation ledger & failure callbacks | Implemented (per legacy source; individual supervisor paths not re-verified) | SQLite `agent_delegations`, `pending_inputs`, `agent_instances`, `agent_personas`. |

## Related Specs

- [Configuration Schema and Cascade](02-configuration-schema-and-cascade.md)
- [Platform Skill Injection](03-platform-skill-injection.md)
- [Daemon Operator API and Lifecycle](05-daemon-operator-api-and-lifecycle.md)
- [Project Versioning and Migration](06-project-versioning-and-migration.md)

Supersedes legacy: `12_streamable_http_mcp_daemon.md` (§1–§6)
