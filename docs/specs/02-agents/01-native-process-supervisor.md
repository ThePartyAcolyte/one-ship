# Native Process Supervisor

## Purpose

Define how ONE-SHIP orchestrates autonomous AI agents, persistent project services, deterministic tasks, and interactive shells natively inside `oneship-daemon` (`src-tauri/src/supervisor.rs`). The supervisor replaces external terminal multiplexers with an in-process asynchronous Tokio supervisor managing child lifecycles, circular output ring buffers, bidirectional stdin pipes, `/proc` attestation, and real-time telemetry events.

## Scope

- The `ProcessSupervisor` singleton and `ManagedProcess` registry inside `oneship-daemon`.
- Trimodal headless multi-turn agent protocols (`agy` stream-json, `opencode` ACP JSON-RPC 2.0, `claude` stream-json).
- Process group isolation, executable/shim resolution, kernel death signals, whole-tree teardown, and shutdown draining.
- Exact key scoping, bounded input queues, mid-turn `SIGINT` interruption, and dynamic re-arming.
- Persistent process ledger (`supervised_processes`), boot-time service adoption vs. task eviction, and pre-flight port conflict guards.
- Daemon operator commands, background state watcher (`agent-states-updated`, `running-services-updated`), frontend store integration, and permission governance.

Out of scope: conversation thread continuity and Brief assembly (see [Thread, Conversation, and Runner-Neutral Context Continuity](02-thread-conversation-and-continuity.md)), MCP dialogue tools (see [MCP Dialogue Channel and Chat Overlay](03-mcp-dialogue-and-chat-overlay.md)), and continuous profiler engine wrapping (see the profiling specifications).

## Model / Design

### 1. Supervision Pipeline & Singleton Architecture

`ProcessSupervisor` is owned exclusively by `oneship-daemon` as a process-wide singleton (`static SUPERVISOR: OnceLock<ProcessSupervisor>`, accessed via `get_supervisor()`). The desktop GUI supervises nothing: closing the desktop window leaves managed services and agents running, and the UI interacts with them solely through daemon RPC (`/api/call`) and SSE (`/api/events`).

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                      NATIVE PROCESS SUPERVISION PIPELINE                    │
│                                                                             │
│  [ ONE-SHIP Vue UI ] ◄───(SSE /api/events: agent-states-updated)── [Watcher]│
│         │                                                              ▲    │
│         ├─── Dispatches Daemon RPC (`/api/call`) ──────────────────────┤    │
│         │    (launch_agent, prompt_agent, stop_agent)                  │    │
│         │                                                              ▼    │
│         ▼                                             [ ProcessSupervisor ] │
│  [ Floating Chat Overlay ]                             ├── Agent Processes  │
│  [ Live Rolling Log Viewer ]                           ├── Service Child PIDs
│         │                                              ├── Circular Buffers │
│         └─── Stdin Pipe Injection / Output Polling ────┴── Stdin Pipes      │
└─────────────────────────────────────────────────────────────────────────────┘
```

```rust
pub struct ManagedProcess {
    pub pid: u32,
    pub run_id: Option<String>,
    pub target_type: Option<String>,
    pub status: Arc<RwLock<ProcessStatus>>,
    pub output_ring: Arc<RwLock<VecDeque<String>>>,
    pub stdin_tx: Option<mpsc::Sender<String>>,
    pub cancel_token: tokio_util::sync::CancellationToken,
    pub active_session_id: Arc<RwLock<Option<String>>>,
    pub pending_inputs: Arc<RwLock<VecDeque<String>>>,
    pub pending_preamble: Arc<Mutex<Option<String>>>,
    pub init_notify: Option<Arc<tokio::sync::Notify>>,
    pub stop_grace_period_ms: u64,
}

pub struct ProcessSupervisor {
    processes: Arc<RwLock<HashMap<String, ManagedProcess>>>,
    retained_buffers: Arc<RwLock<HashMap<String, VecDeque<String>>>>,
    event_tx: Arc<RwLock<Option<tokio::sync::broadcast::Sender<DaemonEvent>>>>,
    rearm_tx: mpsc::Sender<AgentRearmRequest>,
}
```

### 2. Child Process Spawning & Trimodal Headless Multi-Turn Protocols

Agents are spawned via `tokio::process::Command` with `current_dir(project_path)`. Spawning agents with batch prompt flags (`-p`, `--prompt`, `--batch`, `--print-timeout`) or raw interactive TTY requirements is prohibited. `stdin`, `stdout`, and `stderr` are kept open as `Stdio::piped()`, and an internal non-blocking channel sink (`stdin_tx`) coordinates handshakes, configuration, and prompt injection.

1. **Google Antigravity CLI (`agy`):**
   - Launched with `--input-format stream-json --output-format stream-json`.
   - User inputs are wrapped as NDJSON user events: `{"event":"user","message":{"role":"user","content":"<prompt>"}}`.
   - Output events (`init`, `step_update`, `result`) are decoded into the circular ring buffer and emitted as `agent-step-update` events.
2. **OpenCode CLI (`opencode` — DeepSeek & Multi-Provider):**
   - Launched in ACP server mode (`opencode acp`).
   - Handshake executes over JSON-RPC 2.0: `initialize` followed by `session/new` with project `cwd` and `mcpServers` pointing to `http://localhost:8765/mcp` (configured port).
   - Dynamic model and effort configurations are applied via `session/set_config_option`; prompts are dispatched via `session/prompt`.
   - Notifications (`agent_message_chunk`, `agent_thought_chunk`, `tool_call`, `tool_call_update`, `usage_update`) stream into the ring buffer and emit `agent-step-update` / `agent-usage-update` events.
   - Tool permissions and file mutation requests (`session/request_permission`) are intercepted asynchronously by the reader and answered according to `auto_approve`.
3. **Anthropic Claude Code CLI (`claude` — Generation 5.5):**
   - Launched with `--print --input-format stream-json --output-format stream-json --verbose --include-partial-messages --dangerously-skip-permissions`.
   - Dynamic model and reasoning effort are passed via `--model` and `--effort`; resumed sessions connect via `--resume <session_id>`.
   - Stdin injections wrap prompt text into Claude's stream-json envelope: `{"type":"user","session_id":"...","message":{"role":"user","content":[{"type":"text","text":"..."}]}}`.
   - Output events stream asynchronously:
     - `type: "system"` (`subtype: "init"`): extracts session UUID, initializes supervisor tracking, and records `claude_session_id`.
     - `type: "stream_event"`: decodes live streaming deltas for assistant text and thinking blocks, emitting `agent-step-update`.
     - `type: "assistant"`: captures complete messages and MCP tool executions (such as `post_agent_response`).
     - `type: "rate_limit_event"`: extracts live rolling quota metrics (`five_hour` and `seven_day` utilization and reset epochs) and writes them directly into SQLite `account_quotas`.
     - `type: "result"`: transitions status to `ProcessStatus::Idle`, records token usage metrics, runs protocol compliance verification, and triggers `dispatch_next_pending_input`.

**Shared stream & post-turn mechanics:**
- **Circular output ring buffer:** Decoded lines (`[INIT]`, text deltas, `[THINKING]`, `[TOOL]`, `[USAGE]`, `[RESULT]`) are appended to `output_ring` (`VecDeque<String>`, bounded by `supervisor.max_buffer_lines_count`, default 500 lines).
- **Protocol compliance guard:** If an agent concludes a turn without calling `post_agent_response`, the supervisor either fallback-ingests the buffered text or injects a corrective compliance prompt.
- **Post-turn retention:** The child process remains alive in `ProcessStatus::Idle` across turns, preserving LLM context, memory, and MCP sessions.

### 3. Process Isolation, Naming, Teardown & Shutdown Invariants

- **Invariants & diagnostics:** On process state changes, `debug_assert!` verifies that the process PID exists and matches registered supervisor state.
- **Process group isolation (`setpgid(0, 0)`):** Every child spawned via `spawn_service`, `spawn_task`, or `spawn_agent` executes `libc::setpgid(0, 0)` in `pre_exec`, isolating the child and all descendants (e.g., `npm` -> `node` -> `vite` / `esbuild`) into a process group whose PGID equals the child PID.
- **Dynamic process titles (`argv[0]` via `arg0`):** On Unix/Linux, processes are spawned with descriptive titles visible to `ps`, `htop`, `ss`, and `lsof`:
  - Agents: `oneship[<project_name>:<agent_id>]` (e.g., `oneship[odoo-mcp:bosmang]`)
  - Services: `oneship-svc[<project_name>:<service_id>]` (e.g., `oneship-svc[odoo-mcp:dev]`)
  - Tasks: `oneship-task[<project_name>:<task_id>]` (e.g., `oneship-task[odoo-mcp:test-e2e]`)
- **Shim dispatcher compatibility guard (`resolve_executable`):** Before spawning, executables are resolved through `resolve_executable`. When managed by a version manager shim (such as `mise`), the supervisor resolves the underlying binary (via `mise which` or non-shim `PATH` entries). If an executable remains a shim dispatcher whose dispatching relies on `argv[0]`, `arg0` overrides are omitted to avoid `"is not a valid shim"` errors.
- **Kernel-enforced death signal (`PR_SET_PDEATHSIG`):** `pre_exec` configures `libc::prctl(PR_SET_PDEATHSIG, SIGKILL)` so the Linux kernel kills spawned children if the daemon terminates unexpectedly.
- **Orderly whole-tree stop (services and tasks):**
  1. `stop_process` cancels `cancel_token`; the watcher sends `SIGINT` to the process group so the run (and any wrapping profiler engine) can flush and exit.
  2. The watcher waits up to `ManagedProcess.stop_grace_period_ms` (`supervisor.shutdown_grace_period_ms`, default `3000` ms, or `profiler.engine_shutdown_grace_period_ms`, default `15000` ms, when wrapped by a profiler engine), then sends `SIGKILL` to the group, reaps the child, and records termination with exit code (`128 + signal` for signal death) and status `stopped`.
  3. `stdout`/`stderr` readers stay open until the child is reaped so output flushed during teardown cannot trigger `SIGPIPE`.
  4. `stop_process` polls status every `supervisor.process_poll_step_ms` for the grace period plus two steps, killing the group itself if the watcher has not reaped it.
  5. An adopted service (not a direct child of the current daemon) receives the same `SIGINT`, liveness polling, and `SIGKILL`; its exit code is recorded as `NULL`.
  6. Agents retain immediate `SIGKILL` in their watcher on explicit stop.
  7. `db_record_process_termination` writes once only (`WHERE terminated_at_ms IS NULL`).
- **Global shutdown drain (`stop_all_processes`):** On daemon `SIGINT`/`SIGTERM`, `supervisor.stop_all_processes().await` stops all registered agents, dev servers, tasks, and tools concurrently. The drain lasts at most the longest grace period (15 s for profiler-wrapped runs), within the systemd unit's `TimeoutStopSec` (30 s). Stopped services are marked terminated and are not re-adopted on next boot.
- **Post-termination retention & state discovery:** When a child exits, its status transitions to `ProcessStatus::Completed { exit_code }` or `ProcessStatus::Failed { error }`. The supervisor atomically emits `agent-status-report` and `agent-step-update` (`state: "DONE"`), keeping `output_ring` retained for inspection. `compute_project_state` treats an agent as active only when `matches!(st, ProcessStatus::Working | ProcessStatus::Starting | ProcessStatus::Idle | ProcessStatus::Blocked)`. Ephemeral personas (`initializer`, `*-resolver`, `*-crafter`) are detected via `is_flag_resolution_agent`; `is_init_actively_running` includes `Working`, `Starting`, `Idle`, and `Blocked`.

### 4. Bidirectional Stdin, Exact Key Scoping & Idle Queue Dispatch

- **Canonical key scoping:** Process lookup in `send_input`, `read_terminal_output`, and `dispatch_next_pending_input` enforces exact canonical keys (`${project_path}:${agent_id}`, `${project_path}:svc:${service_id}`, `${project_path}:tsk:${task_id}`). Permissive suffix matching (`.ends_with(...)`) is prohibited.
- **Interactive stdin injection:** `send_terminal_input` validates that `processKey` belongs to `projectPath`, appends a newline, and sends to `stdin_tx: mpsc::Sender<String>`.
- **Bounded input queue:** While an agent is `Working`, `send_input` queues prompts in `pending_inputs` up to `supervisor.max_pending_inputs_count` (default `16`). A full queue returns an explicit error and restores an untaken Persona Preamble rather than dropping or growing unbounded.
- **Autonomous queue drain:** When stream readers (AGY, OpenCode, Claude) transition an agent to `ProcessStatus::Idle`, they immediately invoke `dispatch_next_pending_input(&proc_key)`.

### 5. Lock Poisoning Safeguards & Structured Tracing

- All `RwLock` accesses use the poison-recovering trait `RwLockSafe` (`.read_safe(context)` / `.write_safe(context)`), which logs a `WARN` and recovers via `PoisonError::into_inner` rather than panicking.
- Lifecycle transitions (spawn args, working directory, PID allocation, reader termination, EOF, exit codes) are logged via `crate::logger::log`.

### 6. Mid-Turn Cancellation, Interruption & Dynamic Re-Arming (`interrupt_agent`)

When an agent loops or goes off-track, `interrupt_agent` halts the active turn without destroying the conversation session:

1. **Process group `SIGINT`:** `kill(-(pid as i32), SIGINT)` terminates active child tool subprocesses (e.g., `bash` commands) while allowing the runner CLI (`agy` / `opencode` / `claude`) to flush in-memory state (e.g., `~/.gemini/antigravity-cli/conversations/<id>.db`) and exit with an interrupted status.
2. **Protocol compliance bypass:** The stream reader checks exit status and output lines for `"error":"interrupted"` and suppresses protocol compliance reminders during user interruptions.
3. **Background re-arm queue (`rearm_tx`):** Detecting `was_interrupted` in `child.wait()` enqueues an `AgentRearmRequest` on a bounded `tokio::sync::mpsc::channel`. The re-arm worker re-launches the CLI in `Idle` mode attached to the existing native session without re-injecting the Persona Preamble, and injects any attached `correctivePrompt` once `Idle` is reached.

### 7. Deterministic Project Scoping & Zero Agent Burden

- **Canonical namespacing (`format_agent_key`):** Every agent process is keyed as `<project_path>:<agent_id>`.
- **Deterministic `prompt_agent` dispatch:** `prompt_agent(project_path, target, prompt)` verifies `project_path` against SQLite (`list_projects()`), constructs `<project_path>:<target>`, and writes to `stdin_tx` (auto-launching the persona if stopped). Empty or unregistered paths fail immediately with `SupervisorError::ProjectRequired` or `SupervisorError::ProjectNotFound`.
- **Zero agent burden invariant:** Agents never pass or manage ONE-SHIP project IDs, process keys, or routing metadata; the supervisor scopes all dispatch and I/O deterministically.

### 8. Persistent Process Ledger, Boot-Time Adoption & Port Guards

1. **Persistent process ledger (`supervised_processes`):** Every service, task, and shell spawned via `spawn_service` or `spawn_task` is recorded in SQLite:
   - `run_id` (e.g., `svc_frontend_1728950400000`), `project_id` (FK), `target_id`, `target_type` (`service` | `task` | `shell`), `command`, `args_json`, `cwd`, `pid`, `pgid`, `env_identifier` (`ONESHIP_RUN_ID=<run_id>`), `status` (`working` | `stopped` | `completed` | `failed` | `orphaned_killed` | `crashed`), `spawned_at_ms`, `terminated_at_ms`, `exit_code`, `failure_reason`, `profiler_engine_id`, `profile_artifact_path`.
2. **Linux `/proc` attestation (`scan_proc_for_oneship_run_ids`):** To defeat PID recycling and handle launcher processes (`uv run`, `npm run`) whose worker children inherit the PGID, boot reconciliation performs a single-pass scan of `/proc/<pid>/environ` for `ONESHIP_RUN_ID=<run_id>`. Liveness is confirmed when the root PID matches, any living descendant carries `ONESHIP_RUN_ID`, or `libc::kill(-pgid, 0) == 0`.
3. **Startup reconciliation (`reconcile_orphan_processes`):** Executed solely by `oneship-daemon` at boot before serving RPC:
   - **Services (`target_type == "service"`):** Alive services are **adopted** into `ManagedProcess` (`ProcessStatus::Working`) with an asynchronous liveness watcher and `[SYSTEM] Service re-attached from previous session (PID <pid>, PGID <pgid>). Active monitoring resumed.` appended to the buffer.
   - **Tasks (`target_type == "task"`):** Killed via `SIGKILL` to `-pgid` and each living descendant PID; marked `'orphaned_killed'`.
   - **Dead records:** Marked `'crashed'` with reason `"Process exited before startup recovery"`.
4. **Pre-flight port conflict detection (`find_port_listener`):** When `spawn_service` receives a declared or inferred port, it inspects `/proc/net/tcp*` socket inodes (`port_inspect::find_port_listener`):
   - **Auto-adoption:** If the listener carries `ONESHIP_RUN_ID` matching `svc_<service_id>_*`, it is adopted directly without restart or error.
   - **Conflict guard:** If held by another process, `spawn_service` fails with `SupervisorError::PortOccupied` (occupant PID and command line), which `start_service` maps to the `port_occupied` start outcome.
   - **Strict inspection:** Unreadable `/proc/net/tcp*`, uninspectable foreign sockets, or unreadable PGID/environ are surfaced as explicit errors, never treated as a free port.
   - Users can invoke `release_port(port, target_pid)` to terminate the conflicting process group via `SIGKILL`.

```text
  [ Spawn Service / Task ]
            │
            ▼
      status: "working" (SQLite: db_record_process_spawn)
            │
            ├───────────────┬───────────────┬────────────────────────┐
            ▼               ▼               ▼                        ▼
   [ User Stop ]     [ Clean Exit ]  [ Task/Svc Err ]       [ Host Crash / SIGKILL ]
     "stopped"        "completed"       "failed"                     │
   (exit_code: 0)   (exit_code: 0)   (exit_code: >0)                 ▼
                                                            [ Restart ONE-SHIP ]
                                                            reconcile_orphan_processes()
                                                                     │
                                                    ┌────────────────┴────────────────┐
                                                    ▼                                 ▼
                                            [ Process Alive ]               [ Process Dead ]
                                                    │                                 │
                                            ┌───────┴───────┐                         ▼
                                            ▼               ▼                     "crashed"
                                      [ Service ]        [ Task ]
                                      *ADOPTED*      "orphaned_killed"
                                      (Working)     (kill -PGID SIGKILL)
```

### 9. Service Launch, State Watcher, Frontend Integration & Permissions

- **Service launch (`spawn_service`):** Port and profiler engine ID come from `ScriptEntry.port` and `ScriptEntry.profiler`; `rula.toml` is not re-read. Missing or failing profiler manifests return `SupervisorError::ProfilerNotFound` / `SupervisorError::ProfilerNotInstalled`. Port lookup, profiler discovery, and `check_command` run on the blocking pool. Commands containing spaces execute via `/bin/sh -c` with single-quoted arguments (`shell_command_line`); others execute directly. `running_runs()` lists `Working` services/tasks (excluding shells and agents); `get_service_pid` returns a PID only while `Working`. Persistent dev servers launch via `orchestrator::start_project_service`. Detached desktop editors run in a systemd scope via `desktop.rs`, never supervised.
- **Background state watcher (`spawn_agent_state_watcher`):** Runs every `supervisor.state_watcher_interval_ms` (default `2500` ms), reading SQLite and `rula.toml` on the blocking pool across all registered projects, calling `compute_project_state`, and emitting `agent-states-updated` and `running-services-updated` (`<project_id>:<service_id>`, shells excluded).
- **Frontend integration:** `dialogueStore.ts` keys conversations and live steps by `${projectPath}::${agentId.toLowerCase()}`; `workspaceLayoutStore.ts` handles `openFloatingChat` and `switchProjectContext`; `CardHeader.vue` renders `<Terminal :size="16" />` via `hasActiveProcesses`; `AgentTaskbar.vue` discovers active ephemeral and domain personas across `working`, `running`, `idle`, `starting`, and `blocked` states and provides launch/restart/stop/edit actions; `FloatingChatOverlay.vue` binds to `floatingChatProjectPath` / `activeProjectPath`.
- **Trimodal permission governance (`auto_approve` & `accept_edits`):**
  - `agy`: appends `--dangerously-skip-permissions` when `auto_approve: true`, and `--mode accept-edits` when `accept_edits: true` (if supported by runner schema).
  - `opencode`: intercepts ACP `session/request_permission` requests over stdout and replies `{"jsonrpc":"2.0","id":<id>,"result":{"outcome":"accepted"}}` when `auto_approve == true` or `"rejected"` when `false` (`accept_edits` omitted).
  - `claude`: appends `--dangerously-skip-permissions` when `auto_approve: true`, with MCP discovery via `.mcp.json`.
  - Cognitive governance remains mandatory regardless of process-level bypasses: destructive actions and file edits require a technical proposal and explicit approval via `post_agent_response` / `request_approval`.

## Contracts

### Daemon Commands (`api::API_COMMANDS` / `daemonCall`)

| Command | Arguments | Returns | Description |
| :--- | :--- | :--- | :--- |
| `launch_agent` | `projectPath`, `projectName`, `agentId`, `runner`, `model`, `effort`, `accountId`, `cleanContext` | `string` | Launches agent CLI process under supervisor |
| `stop_agent` | `projectPath`, `agentId` | `string` | Terminates agent process tree |
| `interrupt_agent` | `projectPath`, `agentId`, `correctivePrompt` | `string` | Signals `SIGINT` to halt active turn and re-arms agent in `Idle` |
| `restart_agent` | Same as `launch_agent` | `string` | Stops and respawns agent |
| `prompt_agent` | `projectPath`, `target`, `prompt` | `string` | Writes text to child stdin with deterministic project scoping |
| `read_terminal_output` | `projectPath`, `paneId`, `linesCount`, `filter` (`all` \| `errors_only`) | `string` | Reads terminal ring buffer with optional semantic error filtering |
| `send_terminal_input` | `projectPath`, `processKey`, `input` | `null` | Sends line input to child stdin; `processKey` must belong to `projectPath` |
| `launch_interactive_shell` | `projectId`, `shellId` (`shell_` prefix) | `number` | Spawns an interactive `$SHELL` session in the project directory |
| `list_interactive_shells` | `projectId` | `string[]` | IDs of the project's running shells, sorted by launch time |
| `get_project_agent_state` | `projectPath`, `projectName` | `ProjectAgentState` | Snapshot of all agents in project |
| `dispatch_git_instruction` | `projectId`, `instruction` | `string` | Dispatches VCS prompt to project's git agent |
| `release_port` | `port`, `targetPid` | `null` | Terminates conflicting port occupant and its process group via `SIGKILL` |

*Note:* Errors serialize as `SupervisorError` / `OrchestratorError` in `ApiErrorResponse`. The legacy `read_process_output` command was superseded by `read_terminal_output`.

### Daemon Events (`events::DAEMON_EVENT_TYPES`, SSE `/api/events`)

- `agent-states-updated`: Map of project paths to `ProjectAgentState` emitted by `spawn_agent_state_watcher`.
- `running-services-updated`: List of `<project_id>:<service_id>` entries for active `Working` services.
- `agent-step-update`: Live streaming text, thinking deltas, tool calls, and `state: "DONE"` transitions.
- `agent-status-report`: Agent lifecycle status updates.
- `agent-usage-update`: Token and cost telemetry updates from ACP `usage_update` and Claude `result`.
- `agent-error`: Scoped agent execution errors carrying `project_path`.

### Configuration Keys (`[plugins.supervisor]`, global-only unless noted)

- `supervisor.max_buffer_lines_count` (default `500`)
- `supervisor.max_pending_inputs_count` (default `16`)
- `supervisor.shutdown_grace_period_ms` (default `3000`)
- `supervisor.process_poll_step_ms`
- `supervisor.state_watcher_interval_ms` (default `2500`)
- `supervisor.shutdown_step_delay_ms`
- `supervisor.max_task_capture_lines_count`
- `supervisor.max_agent_response_bytes`
- `profiler.engine_shutdown_grace_period_ms` (default `15000`)

## Implementation Status

| Item | Status | Evidence |
| :--- | :--- | :--- |
| `ProcessSupervisor`, `ManagedProcess`, `SupervisorError`, `reconcile_orphan_processes`, `compute_project_state` | Implemented | `src-tauri/src/supervisor.rs`, `src-tauri/src/bin/daemon.rs` |
| Trimodal runners (`agy`, `opencode acp`, `claude` stream-json) & permission handling | Implemented | `src-tauri/src/supervisor.rs` |
| All 12 daemon commands in `API_COMMANDS` | Implemented | `src-tauri/src/api.rs` (`read_process_output` removed; `read_terminal_output` active) |
| Daemon SSE events & `spawn_agent_state_watcher` | Implemented | `src-tauri/src/events.rs`, `src-tauri/src/bin/daemon.rs` |
| Config keys in `get_all_plugin_schemas()` | Implemented | `src-tauri/src/config.rs` |
| Frontend taskbar, chat overlay & composite-key store | Implemented | `src/stores/dialogueStore.ts`, `src/stores/workspaceLayoutStore.ts`, `src/components/taskbar/AgentTaskbar.vue`, `src/components/dialogue/FloatingChatOverlay.vue`, `src/components/ProjectCard/CardHeader.vue` |

## Related Specs

- [Thread, Conversation, and Runner-Neutral Context Continuity](02-thread-conversation-and-continuity.md)
- [MCP Dialogue Channel and Chat Overlay](03-mcp-dialogue-and-chat-overlay.md)
- [Multi-Account Management](04-multi-account-management.md)
- [Project Registry, Scripts & Service Orchestration](../03-projects/01-project-registry-and-service-orchestration.md)
- [Configuration Schema and Cascade](../01-architecture/02-configuration-schema-and-cascade.md)

Supersedes legacy: `08_native_process_supervisor.md`
