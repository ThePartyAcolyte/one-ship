# Spec 08: Native Process Supervisor & Autonomous Agent Orchestration

**Document Version:** 2.2.0  
**Domain:** Autonomous Agent Execution, Native Process Supervision & Daemon RPC  
**Status:** Implemented & Validated (Build 0.2.6)  
**Maintainer:** Bosmang (Lead Systems Architect & Engineer)

---

## 1. Overview & Architecture

ONE-SHIP orchestrates autonomous AI agents and persistent services directly via an internal, asynchronous **Tokio Process Supervisor** in the Rust daemon (`src-tauri/src/supervisor.rs`).

Previously, ONE-SHIP relied on an external terminal multiplexing daemon (Herder, Protocol 20). As part of the native architectural migration, ONE-SHIP is 100% self-contained: `oneship-daemon` manages child process lifecycles, circular output ring buffers, stdin prompt pipes, and real-time state telemetry natively without external multiplexer dependencies.

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

---

## 2. Process Supervisor Core Architecture (`src-tauri/src/supervisor.rs`)

### 2.1 The `ProcessSupervisor` Singleton
The process supervisor is owned exclusively by `oneship-daemon` as a process-wide singleton (`static SUPERVISOR: OnceLock<ProcessSupervisor>`, accessed via `get_supervisor()`).

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

### 2.2 Child Process Spawning & I/O Isolation (`stream-json` & `ACP` Protocols)
- **Command Construction:** Agents are spawned via `tokio::process::Command` inside the project working directory (`current_dir(project_path)`).
- **Universal Headless Multi-Turn Architecture:** Spawning agents with batch prompt flags (`-p`, `--prompt`, `--batch`, `--print-timeout`) or raw interactive TTY requirements is strictly prohibited. The supervisor natively supports trimodal headless multi-turn execution:
  1. **Google Antigravity (`agy`):** Launched with `--input-format stream-json --output-format stream-json`. User inputs are wrapped into NDJSON user events (`{"event":"user","message":{"role":"user","content":"<prompt>"}}`). Output events (`init`, `step_update`, `result`) are decoded and parsed into ring buffers and live `agent-step-update` events.
  2. **OpenCode CLI (`opencode` - DeepSeek & Multi-Provider):** Launched in ACP server mode (`opencode acp`). Handshake executes via JSON-RPC 2.0 (`initialize` $\rightarrow$ `session/new` with project `cwd` and `mcpServers` pointing to `http://localhost:8765/mcp`). Dynamic model and effort configurations are applied via `session/set_config_option`. Prompts are dispatched via `session/prompt`. Notifications (`agent_message_chunk`, `agent_thought_chunk`, `tool_call`, `tool_call_update`, `usage_update`) stream live into the supervisor ring buffers and emit unified `agent-step-update` events. Tool permissions and file mutation requests (`session/request_permission`) are handled automatically based on the agent's `auto_approve` setting.
  3. **Anthropic Claude Code CLI (`claude` - Generation 5.5):** Launched with `--print --input-format stream-json --output-format stream-json --verbose --include-partial-messages --dangerously-skip-permissions`. Dynamic model and reasoning effort configurations are passed via `--model` and `--effort`. Resumed sessions connect natively via `--resume <session_id>`. Stdin injections wrap raw prompt text into Claude's stream-json envelope (`{"type":"user","session_id":"...","message":{"role":"user","content":[{"type":"text","text":"..."}]}}`). Output events stream asynchronously:
     - `type: "system"` (`subtype: "init"`): Extracts session UUID, initializes supervisor conversation tracking, and records `claude_session_id`.
     - `type: "stream_event"`: Decodes live streaming deltas for assistant text and thinking blocks, publishing real-time `agent-step-update` events.
     - `type: "assistant"`: Captures complete messages and MCP tool executions (specifically `post_agent_response` and file inspection tools).
     - `type: "rate_limit_event"`: Extracts live rolling quota metrics (`five_hour` and `seven_day` utilization and reset epochs) and writes them directly into SQLite `account_quotas`.
     - `type: "result"`: Transitions agent status to `ProcessStatus::Idle`, records token usage metrics, executes protocol compliance verification, and triggers `dispatch_next_pending_input` to drain queued tasks.
- **Transparent Stdin Sink & Forwarding:** `stdin` is kept open as `Stdio::piped()`. An internal non-blocking channel sink coordinates protocol handshakes, model configuration, and interactive prompt injection asynchronously.
- **Asynchronous Output Stream Parsing:** `stdout` and `stderr` are piped (`Stdio::piped()`). Decoded lines (`[INIT]`, text deltas, `[THINKING]`, `[TOOL]`, `[USAGE]`, `[RESULT]`) enter the bounded circular ring buffer (`VecDeque<String>`, capacity 500 lines) for real-time terminal display.
- **Protocol Compliance Guard:** If an agent concludes a turn without invoking the mandatory MCP tool `post_agent_response`, the supervisor either fallback ingests the buffered text or injects a corrective compliance prompt.
- **Post-Turn Retention:** The child process remains alive in idle waiting state across multiple turns, preserving LLM conversation context, memory, and MCP tool sessions.

### 2.3 Process Retention, Orphan Prevention & Teardown Invariants
- **Invariants & Diagnostics:** On process state changes, `debug_assert!` verifies that the process PID exists and matches registered supervisor state.
- **Process Group Isolation (`setpgid(0, 0)`):** Every child process spawned via `spawn_service`, `spawn_task`, or `spawn_agent` executes `libc::setpgid(0, 0)` in `pre_exec`. This isolates the child and all its sub-processes (such as npm launching node, which in turn spawns vite and esbuild) into an autonomous process group whose PGID equals the child PID.
- **Dynamic Process Title & Naming (`argv[0]` via `arg0`):** On Unix/Linux, processes are spawned with descriptive process titles in `argv[0]`:
  - Agents: `oneship[<project_name>:<agent_id>]` (e.g. `oneship[odoo-mcp:bosmang]`, `oneship[estudiorc:core]`).
  - Services: `oneship-svc[<project_name>:<service_id>]` (e.g. `oneship-svc[odoo-mcp:dev]`, `oneship-svc[l10n_cl_banking:odoo_mcp]`).
  - Tasks: `oneship-task[<project_name>:<task_id>]` (e.g. `oneship-task[odoo-mcp:test-e2e]`).
  This allows host monitoring tools (`ps`, `htop`, `ss`, `lsof`, Oh-My-Zsh plugins) to identify the project and persona unambiguously without inspecting internal files.
  - **Shim Dispatcher Compatibility Guard (`resolve_executable`):** Before spawning, the supervisor resolves executables through `resolve_executable`. When a binary is managed by a version manager shim (such as `mise`), the supervisor resolves the underlying real executable (via `mise which` or non-shim `PATH` entries) to bypass shim dispatchers. If an executable remains a shim dispatcher whose command dispatching relies strictly on `argv[0]`, `arg0` overrides are safely omitted to prevent dispatch failures (`is not a valid shim`).
- **Kernel-Enforced Death Signal (`PR_SET_PDEATHSIG`):** In `pre_exec`, `libc::prctl(PR_SET_PDEATHSIG, SIGKILL)` is configured. If the parent daemon or desktop process unexpectedly dies or is terminated by `SIGKILL`, the Linux kernel automatically dispatches `SIGKILL` to all spawned child processes, preventing orphaned background processes.
- **Orderly Whole-Tree Stop (services and tasks):** When `stop_process` is executed:
  - Cancels the Tokio `cancel_token`; the run's watcher sends `SIGINT` to the process group, so the run (and a profiler engine wrapping it, Spec 25 §4.4) can flush and exit.
  - The watcher waits for the exit up to the run's grace period (`ManagedProcess.stop_grace_period_ms`: `supervisor.shutdown_grace_period_ms`, default 3000 ms, or `profiler.engine_shutdown_grace_period_ms`, default 15000 ms, for a run wrapped by an engine), then sends `SIGKILL` to the group, reaps the child and records the termination with the real exit code (`128 + signal` for a signal death) and status `stopped`.
  - The stdout/stderr readers stay open until the child is reaped, so output written while stopping cannot kill it with `SIGPIPE`.
  - `stop_process` polls the status every `supervisor.process_poll_step_ms` for the grace period plus two steps, then kills the group itself if the watcher has not reaped it.
  - An adopted service (not a child of this daemon) gets the same `SIGINT`, liveness polling and `SIGKILL`; its exit code is unknown (`NULL`).
  - Agents keep the immediate `SIGKILL` of their watcher.
  - A run's termination is recorded once: `db_record_process_termination` only updates a row whose `terminated_at_ms` is NULL.
- **Single Supervisor (Spec 12 §7.2):** the supervisor runs only in `oneship-daemon`. The desktop app supervises nothing: closing it leaves services and agents running, and its UI reaches them through daemon commands and `/api/events`.
- **Global Shutdown Drain (`stop_all_processes`):**
  - The daemon (`SIGINT` and `SIGTERM` signals) invokes `supervisor.stop_all_processes().await` before closing.
  - Automatically sweeps and terminates all registered agents, dev servers, tasks, and external tools.
  - The processes stop concurrently, so the drain lasts as long as the longest grace period (15 s for a run wrapped by a profiler engine), within the daemon unit's `TimeoutStopSec` (30 s in `scripts/run_dev.sh`).
  - Services stopped there are not adopted by the next daemon; adoption covers runs that survive a daemon crash.
- **Post-Termination Retention & Lifecycle Event Propagation:** When a managed child process terminates (either naturally, via error, or explicit stop):
  - Its status transitions to `ProcessStatus::Completed { exit_code }` or `ProcessStatus::Failed { error }`.
  - The supervisor atomically emits `agent-status-report` and `agent-step-update` with `state: "DONE"`, ensuring the frontend immediately disengages active indicators and resets step telemetries.
  - Its circular output buffer remains intact and queryable via `read_process_output`.
  - Dynamic discovery in `compute_project_state` strictly checks `matches!(st, ProcessStatus::Working | ProcessStatus::Starting | ProcessStatus::Idle | ProcessStatus::Blocked)`, preventing terminated or failed background processes from leaking into active project agent rosters while ensuring interactive multi-turn agents awaiting user input in `Idle` remain fully attached.
  - Ephemeral personas (`initializer`, and flag resolvers `*-resolver`, `*-crafter`) declared in `rula.toml` or SQLite are isolated via `is_flag_resolution_agent`. In `compute_project_state`, `is_init_actively_running` explicitly encompasses `Working`, `Starting`, `Idle`, and `Blocked` states, preventing ephemeral agents from being omitted from the roster while waiting for user interaction.
  - Removal from the internal registry occurs upon explicit user termination (`stop_agent`) or when a new process is spawned under the same key.


### 2.4 Bidirectional Stdin, Exact Key Resolution & Idle Queue Dispatch
- **Zero Permissive Fallbacks (Exact Key Scoping):** When dispatching commands or reading logs via `send_input`, `read_output`, and `dispatch_next_pending_input`, the supervisor strictly enforces exact canonical keys (`${project_path}:${agent_id}` or `${project_path}:svc:${service_id}`). Permissive suffix matching (`.ends_with(...)`) has been completely eliminated in accordance with Rule 11.
- **Interactive Stdin Injection:** The `send_terminal_input` command forwards line input to the process's bounded `stdin_tx: mpsc::Sender<String>` channel, automatically appending newlines and flushing to the child process's standard input pipe.
- **Bounded Input Queue:** While an agent is `Working`, `send_input` queues prompts in `pending_inputs` up to `supervisor.max_pending_inputs_count` (default 16). A full queue rejects the input with an explicit error (and restores an untaken Persona Preamble) instead of growing without bound (AGENTS.md §8.3).
- **Autonomous Queue Draining on Idle:** When an agent transitions to `ProcessStatus::Idle` (after completing an initial handshake or turn), stream readers (AGY and OpenCode) immediately invoke `sup.dispatch_next_pending_input(&proc_key).await`. This ensures tasks delegated while an agent was busy or starting are executed promptly without manual turn intervention.

### 2.5 Lock Poisoning Safeguards & Structured Telemetry
- **Poison-Safe Synchronization Contracts (`RwLockSafe`):** All `RwLock` accesses throughout `supervisor.rs` use the poison-recovering helper trait `RwLockSafe` (`.read_safe(context)` / `.write_safe(context)`), which logs a `WARN` and recovers the inner guard via `poisoned.into_inner()` rather than panicking (`AGENTS.md` §9.1).
- **Structured Tracing Guarantees:** Process lifecycle events (command arguments, working directory, child PID allocation, stdout/stderr reader termination, pipe EOF, and exit status code transitions) are logged through `crate::logger::log`, eliminating opaque failures and ensuring high-fidelity forensic debugging.

### 2.6 Mid-Turn Cancellation, Interruption & Dynamic Re-Arming (`interrupt_agent`)
- **Forensic `SIGINT` Handling:** When an agent goes off-track or enters an unintended execution loop (e.g. reading documents or running heavy commands unbidden), the user can interrupt the turn without destroying the agent session.
- **Process Group Signaling (`kill(-(pid as i32), SIGINT)`):** In Linux, signaling `SIGINT` to the process group PGID terminates active child tool subprocesses (such as child `bash` commands spawned by `run_command`) while allowing the runner CLI (`agy` / `opencode`) to catch the interrupt signal, flush in-memory state to disk (`~/.gemini/antigravity-cli/conversations/<id>.db`), and exit cleanly with an interrupted status.
- **Protocol Error Suppression:** The stream-json reader inspects exit status and output lines for `"error":"interrupted"`. Protocol compliance reminder prompts are bypassed during user interruptions.
- **Decoupled Background Re-Arm Queue (`rearm_tx`):** When `was_interrupted` is detected in `child.wait()`, the supervisor dispatches an `AgentRearmRequest` through a bounded `tokio::sync::mpsc::channel` worker. The worker re-launches the agent CLI in `Idle` mode attached to the existing conversation ID without persona re-injection. If a corrective prompt was attached, it is injected directly into `stdin_tx` once the agent reaches `Idle`.

### 2.7 Deterministic Project Scoping & Zero-Burden Supervision Protocol
- **Canonical Namespacing (`format_agent_key`):** Every managed agent process in the backend supervisor is keyed deterministically as `<project_path>:<agent_id>`. This strictly isolates agents sharing identical persona IDs across separate repositories (e.g. `bosmang` in `/repos/banking` vs `bosmang` in `/repos/estudiorc`).
- **Deterministic `prompt_agent` Dispatching:**
  - **Explicit Project Path:** `prompt_agent(project_path, target, prompt)` verifies that `project_path` matches a registered project in SQLite (`list_projects()`), constructs `<project_path>:<target>`, and routes directly to that child process's `stdin_tx` (auto-launching the persona first if not yet running).
  - **Required Project:** An empty or unregistered `project_path` fails immediately with `SupervisorError::ProjectRequired` or `SupervisorError::ProjectNotFound`; the supervisor never scans running processes or guesses the project.
- **Zero Agent Burden Invariant:** The AI agent (LLM) remains completely agnostic to platform-internal identifiers, paths, and routing mechanics. The agent must never be burdened with managing its own project IDs, passing metadata back and forth, or making routing decisions. ONE-SHIP owns the terminal, process lifecycle, supervisor process keys, working directories, and UI context, and handles 100% of the scoping deterministically.

### 2.8 Persistent Process Ledger, Boot-Time Service Adoption & Dynamic Port Guards

To ensure uninterrupted development workflows while preventing port conflicts (`EADDRINUSE`) and untracked zombie processes when the host system or ONE-SHIP restarts, ONE-SHIP implements a state-restoring supervision engine pairing persistent ledger state with native Linux `/proc` attestation:

1. **Persistent Process Ledger (`supervised_processes` Table):**
   - Every service, task, and shell spawned through `spawn_service` or `spawn_task` is immediately registered in the `supervised_processes` table in SQLite (`db.rs`).
   - Recorded fields include:
     - `run_id`: Globally unique execution identifier (e.g. `svc_frontend_1728950400000`).
     - `project_id`: Foreign key referencing the parent project.
     - `target_id`: ID of the service or task (e.g. `dev`, `build`, `shell`).
     - `target_type`: Categorical discriminator (`service`, `task`, `shell`).
     - `command`, `args_json`, `cwd`: Full command invocation context.
     - `pid`, `pgid`: Host process and process group identifiers.
     - `env_identifier`: Attestation token `ONESHIP_RUN_ID=<run_id>` injected into the child environment.
     - `status`: Current lifecycle state (`working`, `stopped`, `completed`, `failed`, `orphaned_killed`, `crashed`).
     - `spawned_at_ms`, `terminated_at_ms`: Epoch timestamps strictly adhering to Tiger Style unit suffixes.
     - `exit_code`, `failure_reason`: Termination diagnostics.
     - `profiler_engine_id`, `profile_artifact_path`: Optional flamegraph profiler metadata (Spec 25).

2. **Linux Process Attestation & Whole-Tree Scan (`scan_proc_for_oneship_run_ids`):**
   - Linux reuses process IDs (PIDs) dynamically. A raw PID check (`kill(pid, 0)`) risks terminating unrelated system processes if an old PID has been recycled.
   - Furthermore, modern development tools (such as `uv run`, `npm run`, or shell scripts) often launch descendant worker processes (e.g. `uvicorn`, `vite`, `python`) where the launcher PID exits while worker children continue running under the same Process Group (PGID).
   - **Invariant:** When evaluating unclosed records at boot, the supervisor executes a single-pass `/proc` environment scan mapping all active processes carrying `ONESHIP_RUN_ID=<run_id>`.
   - Process liveness is confirmed if:
     1. Direct root PID matches `/proc/<pid>/environ`.
     2. Any living descendant processes in `/proc` carry the matching `ONESHIP_RUN_ID`.
     3. The process group is active: `libc::kill(-pgid, 0) == 0`.

3. **Startup Pipeline: Service Adoption vs. Task Eviction (`reconcile_orphan_processes`):**
   - Executed exclusively by `oneship-daemon` (`src-tauri/src/bin/daemon.rs`) at startup before serving requests.
   - **Persistent Services (`target_type == "service"`):**
     - If verified alive, **the service is ADOPTED rather than killed**.
     - Re-instantiates `ManagedProcess` with status `ProcessStatus::Working`.
     - Spawns an asynchronous liveness watcher polling process health and managing teardown tokens.
     - Appends retained buffer explanation: `[SYSTEM] Service re-attached from previous session (PID <pid>, PGID <pgid>). Active monitoring resumed.`
     - Preserves running database connections, web servers (e.g. Odoo on 8069, Uvicorn on 8000), and dev servers without port collision or restart latency.
   - **Ephemeral Tasks (`target_type == "task"`):**
     - Terminated immediately via `libc::kill(-(pgid as i32), SIGKILL)` and `SIGKILL` to each living child PID.
     - Transitioned in SQLite to `'orphaned_killed'`.
   - **Stale Dead Records:**
     - Transitioned in SQLite to `'crashed'` with reason `"Process exited before startup recovery"`.

4. **Pre-Flight Port Conflict Detection & Auto-Adoption (`find_port_listener`):**
   - In `spawn_service`, the supervisor parses declared or inferred ports (`rula.toml` `port` or command arguments `--port <n>`).
   - If the port is in use, it inspects `/proc/net/tcp*` socket inodes and maps the exact listening PID (`port_inspect::find_port_listener`):
     - **Auto-Adoption:** If the listening process belongs to the same service (`ONESHIP_RUN_ID` matching `svc_<service_id>_*`), it is adopted immediately into supervisor memory without restarting or throwing errors.
     - **Conflict Guard:** If occupied by a different or external process, `spawn_service` fails with `SupervisorError::PortOccupied` (occupant PID and command line). `start_service` turns it into the `port_occupied` start outcome (Spec 01).
     - **Inspection failures are errors, never a free port:** an unreadable `/proc/net/tcp`, a listening socket whose owner cannot be inspected (another user's process), or an unreadable PGID / environment of the owner.
   - The user can invoke `release_port(port, target_pid)` to terminate the conflicting process on demand (Spec 01).

5. **Lifecycle State Transition Flow:**
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

---

## 3. Daemon Commands (Spec 12 §7.2)

Called through `daemonCall`; arguments are `camelCase` (generated `*Args` types), errors are `SupervisorError` / `OrchestratorError` as `ApiErrorResponse`.

| Command | Arguments | Returns | Description |
|---|---|---|---|
| `launch_agent` | `projectPath`, `projectName`, `agentId`, `runner`, `model`, `effort`, `accountId`, `cleanContext` | `string` | Launches agent CLI process under supervisor |
| `stop_agent` | `projectPath`, `agentId` | `string` | Terminates agent process tree with bounded grace period |
| `interrupt_agent` | `projectPath`, `agentId`, `correctivePrompt` | `string` | Signals `SIGINT` to halt active turn, clears transient steps, and re-arms agent in `Idle` with optional corrective prompt |
| `restart_agent` | as `launch_agent` | `string` | Stops and immediately respawns agent |
| `prompt_agent` | `projectPath`, `target`, `prompt` | `string` | Writes text directly to child process stdin with deterministic project scoping |
| `read_process_output` | `processKey`, `linesCount` | `string[]` | Reads terminal output from circular ring buffer |
| `read_terminal_output` | `projectPath`, `paneId`, `linesCount`, `filter` (`all` / `errors_only`) | `string` | Reads terminal stream with optional semantic error filtering |
| `send_terminal_input` | `projectPath`, `processKey`, `input` | `null` | Sends text input to child process stdin; `processKey` must be a canonical key of `projectPath` |
| `launch_interactive_shell` | `projectId`, `shellId` (`shell_` prefix) | `number` | Spawns an interactive `$SHELL` session in the project directory |
| `list_interactive_shells` | `projectId` | `string[]` | IDs of the project's running shells, sorted by launch time |
| `get_project_agent_state` | `projectPath`, `projectName` | `ProjectAgentState` | Returns snapshot of all agents in project |
| `dispatch_git_instruction` | `projectId`, `instruction` | `string` | Dispatches VCS prompt to project's git agent |
| `release_port` | `port`, `targetPid` | `null` | Forcefully terminates the reported port occupant and its process group via SIGKILL (Spec 01) |

The startup reconciliation (`reconcile_orphan_processes`) runs only inside the daemon and has no command. Detached desktop windows (such as external editors opened via `open_file_in_external_editor`) run in their own systemd scope via `desktop.rs`, never supervised (Spec 12 §7.2).

### 3.1 Service Launch (`spawn_service`)
- Port and profiler engine come from the caller's declared script (`ScriptEntry.port`, `ScriptEntry.profiler`, an engine id); `rula.toml` is not re-read. An engine without a valid manifest fails the start with `SupervisorError::ProfilerNotFound`, one whose `check_command` fails with `SupervisorError::ProfilerNotInstalled` (Spec 25 §4.4).
- The port occupant lookup (`/proc`), the profiler discovery and the `check_command` run on the blocking pool.
- `running_runs()` lists the services and tasks in `Working` state (not shells or agents) with their `run_id` and root PID, for the continuous profiler and the service telemetry. `get_service_pid` returns a PID only while the service is `Working`.
- A command containing spaces is shell syntax and runs through `/bin/sh -c` with every argument single-quoted (`shell_command_line`); any other command is executed directly with its arguments. `spawn_task` follows the same rule.
- Persistent dev servers start through `orchestrator::start_project_service`, the same path as the service grid.

---

## 4. Background State Watcher Loop (`spawn_agent_state_watcher`)

The background watcher runs in the daemon as an asynchronous Tokio loop with `supervisor.state_watcher_interval_ms` (2500); each tick reads SQLite and every `rula.toml` on the blocking pool:
1. Iterates through all registered projects.
2. Constructs the active agent state snapshot for each project (`compute_project_state`, the same as `get_project_agent_state`).
3. Emits the global `agent-states-updated` daemon event (`/api/events`) containing the map of project states.
4. Emits `running-services-updated` with the `<project_id>:<service_id>` entries of `Working` services from the live process registry (`running_service_ids`; shells excluded, no `rula.toml` reads).


---

## 5. Frontend UI & Store Integration

1. **`dialogueStore.ts` & `workspaceLayoutStore.ts` Composite Key Architecture:**
   - **Composite Key SSOT:** Conversations and live steps in `dialogueStore.ts` are keyed by `${projectPath}::${agentId.toLowerCase()}`, eliminating collisions between agents sharing names across projects (e.g. `bosmang` in `/repos/banking` vs `bosmang` in `/repos/estudiorc`).
   - **Composite Accessors:** State access occurs via `ensureConversation(agentId, projectPath)`, `getConversation(agentId, projectPath)`, and `getActiveStep(agentId, projectPath)`.
   - **Message & Step Dispatch:** `sendUserMessage`, `handleAgentResponse`, and `agent-step-update` listeners operate strictly under exact composite keys. Cross-project message leakage or step rendering is prohibited.
   - **`openFloatingChat` & `switchProjectContext` (`workspaceLayoutStore.ts`):** Manage active project layout transitions, docked vs. floating chat geometry, and preserve active ephemeral/resolution sessions matching `targetPath`.
2. **UI Dispatchers & Explicit Scope Propagation:**
   - All interactive controls (`AgentTaskbar.vue`, `FloatingChatOverlay.vue`, `RollingStatusLog.vue`, and `App.vue` global hotkeys) propagate `projectPath` explicitly into `openFloatingChat`, `selectAgent`, and message submission pipelines.
3. **`CardHeader.vue`:**
   - Displays native **Process Supervisor Indicator** (`<Terminal :size="16" />`).
   - Uses `hasActiveProcesses` computed property: displays green active border/text when agents are running, dim when idle.
4. **`AgentTaskbar.vue` (Unified Agent Launcher & Taskbar):**
   - Direct click on running agent triggers `layoutStore.openFloatingChat(agent.id, project.path)`.
   - Click on unlaunched agent invokes `launch_agent` followed by chat opening with explicit project path.
   - Right-click context menu provides: "Open Chat", "Restart Agent", "Stop Agent", and "Edit Persona".
   - **Ephemeral Multi-Turn Retention:** Active resolver and initializer agents are discovered via `isAgentProcessActive(ag.id)` covering `working`, `running`, `idle`, `starting`, and `blocked`, guaranteeing that ephemeral agents in `Idle` waiting for developer input remain visible in the taskbar.
5. **`FloatingChatOverlay.vue`:**
   - Bound to `floatingChatProjectPath` / `activeProjectPath`, ensuring multi-turn messaging, approval decisions, option selections, and context handoffs route deterministically to the active project.


---

## 6. Autonomous Execution & Governance Boundaries

### 6.1 Trimodal Runner Permission Protocols (`auto_approve` & ACP Governance)
In headless multi-turn workflows, child processes cannot wait on interactive stdin terminal confirmations. Depending on the runner engine, permission governance is applied either via process flags or protocol RPC interception:

1. **Google Antigravity CLI (`agy`):**
   - **Tool & Command Bypass:** When `auto_approve: true`, the supervisor injects the runner's bypass flag:
     ```bash
     agy --dangerously-skip-permissions ...
     ```
   - **File Edit Auto-Accept:** When `accept_edits: true` (and supported by the runner schema), the supervisor appends `--mode accept-edits`.
2. **OpenCode CLI (`opencode` - ACP Protocol):**
   - OpenCode does not utilize CLI bypass flags. Instead, when an agent attempts to execute tools (such as terminal commands or file modifications), the child process emits a JSON-RPC request to the supervisor over stdout:
     ```json
     {"jsonrpc":"2.0","id":1,"method":"session/request_permission","params":{"sessionId":"...","toolCall":{"title":"...","command":"..."}}}
     ```
   - The supervisor's asynchronous reader task inspects the agent's effective `auto_approve` configuration:
     - **If `auto_approve == true`:** Returns `{"jsonrpc":"2.0","id":<id>,"result":{"outcome":"accepted"}}`.
     - **If `auto_approve == false`:** Returns `{"jsonrpc":"2.0","id":<id>,"result":{"outcome":"rejected"}}`, strictly preventing unauthorized file edits or bash executions during headless turns.
   - Because file edits in OpenCode are tool calls governed directly by ACP permissions, runner schema flags such as `accept_edits` are omitted for OpenCode.
3. **Anthropic Claude Code CLI (`claude`):**
   - **Tool & Command Bypass:** When `auto_approve: true`, the supervisor appends the bypass flag:
     ```bash
     claude --dangerously-skip-permissions ...
     ```
   - **MCP Tool Discovery:** Automatically configured via `.mcp.json` provisioned in the project root pointing to the ONE-SHIP MCP daemon endpoint.

### 6.2 Cognitive Governance & Proposals
Regardless of process-level permission bypasses (`auto_approve: true`), agents remain bound by cognitive governance: all destructive actions, file modifications, and architecture changes require formulating a structured technical proposal and receiving explicit user approval via the Chat Overlay (`post_agent_response` / `request_approval`) before executing file mutations, adhering to the fundamental governance rules in `AGENTS.md`.
