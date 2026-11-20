# MCP Dialogue Channel and Chat Overlay

## Purpose

Define the structured MCP communication protocol (`post_agent_response`, `request_approval`) that separates spoken human dialogue from technical artifacts, the daemon-enforced turn-end compliance guard, and the unified frontend chat/taskbar interface (`FloatingChatOverlay.vue`, `AgentTaskbar.vue`, `dialogueStore.ts`, `workspaceLayoutStore.ts`) with exact per-project composite scoping and native Gemini dictation integration.

## Scope

In scope: lazy context loading (`mcp_dialogue_mode`), `post_agent_response` and `request_approval` MCP tool contracts, `conversation.rs` compliance tracking, `FloatingChatOverlay.vue` and `AgentTaskbar.vue` UI architecture, `workspaceLayoutStore.ts` per-project workspace state machine, SSE listener teardown, dual Send/Stop interruption ergonomics, native dictation target registration, and `${projectPath}::${agentId}` composite keys.

Out of scope: child process spawning and `stream-json`/ACP protocol parsing (see [Native Process Supervisor](01-native-process-supervisor.md)), Brief assembly and Reopen lifecycle (see [Thread, Conversation, and Runner-Neutral Context Continuity](02-thread-conversation-and-continuity.md)), and the native dictation engine itself (see [Native Gemini Dictation](../05-voice/02-dictation-and-agent-chat.md)).

## Model / Design

### 1. Architecture & Lazy Context Loading

When `mcp_dialogue_mode = true`, ONE-SHIP omits eager `context_docs` injection at startup and instructs agents to inspect files on demand and route all user-facing replies through dedicated MCP tools hosted by `oneship-daemon` (`src-tauri/src/mcp.rs`).

```text
┌────────────────────────────────────────────────────────────────────────┐
│                          ONE-SHIP ARCHITECTURE                         │
│                                                                        │
│  ┌──────────────────────┐              ┌────────────────────────────┐  │
│  │  Agent (supervised)  │              │    FloatingChatOverlay     │  │
│  │                      │              │                            │  │
│  │ 1. Receives prompt   │              │ [Header / Execution Config]│  │
│  │ 2. Reasons / tools   │              │ [Telemetry Bar]            │  │
│  │ 3. Calls MCP Tool:   │              │ [Dialogue & Artifact Chips]│  │
│  │    post_agent_response              └──────────────▲─────────────┘  │
│  └──────────┬───────────┘                             │                │
│             │ JSON-RPC (HTTP)                         │ SSE Event      │
│             ▼                                         │ (AgentResponse)│
│  ┌────────────────────────────────────────────────────┴─────────────┐  │
│  │  Standalone MCP Daemon / Axum Router (`src-tauri/src/mcp.rs`)    │  │
│  │  - `post_agent_response` (emits AgentResponse DaemonEvent)       │  │
│  │  - `request_approval` (emits ApprovalRequested + SQLite persist) │  │
│  │  - Updates `conversation.rs` tracker: mark_agent_responded()     │  │
│  └──────────────────────────────────┬───────────────────────────────┘  │
│                                     │                                  │
│                                     ▼                                  │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │  Conversation State Tracker (`src-tauri/src/conversation.rs`)    │  │
│  │  - Tracks last_user_message_at, last_mcp_response_at             │  │
│  │  - Guards compliance_prompted flag                               │  │
│  └──────────────────────────────────┬───────────────────────────────┘  │
│                                     │ Checked on Turn End              │
│                                     ▼                                  │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │  Supervisor Turn-End Guard (`src-tauri/src/supervisor.rs`)       │  │
│  │  - Detects turn end across opencode / agy / claude               │  │
│  │  - Ingests fallback text or injects compliance reminder prompt   │  │
│  └──────────────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────────┘
```

### 2. MCP Dialogue & Approval Tools

1. **Process-attested attribution & Zero Agent Burden Invariant:**
   - Every call resolves the calling agent and project deterministically from `CallerContext` (TCP peer socket -> owning PID -> `ONESHIP_AGENT_INSTANCE` -> `agent_instances` registry).
   - Tool schemas do not accept `agent_id`, `project_uid`, `project_id`, or `project_path`.
   - `post_agent_response` enriches the emitted `AgentResponse` event payload with `project_id` and `project_path`. If project context cannot be verified, the call fails immediately with JSON-RPC `isError: true`.
2. **Dual-path artifact resolution (`post_agent_response`):**
   - `artifact` may be inline text (code, unified diff, markdown table, JSON) or a path (`file_path` / Antigravity brain artifact path `.gemini/antigravity-cli/brain/<cid>/...`).
   - When a file path is provided (validated inside the project root), the MCP handler reads the content, infers `artifact_type` and `artifact_lang`, and attaches it to the `AgentResponse` payload.
   - Interactive inline proposals (`approval: { title, description?, options? }`) are persisted in `agent_chat_turns.approval_payload` so pending choices rehydrate across app restarts. Clicking an option button dispatches a preformed prompt to the agent's interactive REPL and persists the decision via `update_turn_approval`.
3. **Synchronous approval gates (`request_approval`):**
   - Blocks execution while awaiting explicit user authorization for `technical_decision`, `file_edit`, `command`, or `architectural_change`.
   - Persists pending approvals in SQLite (`pending_approvals`: `task_id`, `project_path`, `phase`, `payload_markdown`) so approvals remain actionable across UI reloads.
4. **Turn-end compliance guard (`conversation::should_inject_compliance_reminder`):**
   - Executed at turn completion inside `supervisor.rs` across `opencode`, `agy`, and `claude`.
   - Compares `last_user_message_at` vs. `last_mcp_response_at` guarded by `compliance_prompted`; if an agent turns idle without calling `post_agent_response` (and was not interrupted by the user), the supervisor either fallback-ingests buffered output or injects a corrective compliance prompt.

### 3. Chat Overlay, Taskbar & Per-Project Workspace UI

*(Historical note: early components `ChatFab.vue`, `AgentDialogueOverlay.vue`, `AgentList.vue`, `ConversationPanel.vue`, and `ArtifactPanel.vue` were retired and replaced by `AgentTaskbar.vue`, `FloatingChatOverlay.vue`, and the integrated workspace explorer/reader in `ProjectWorkspace.vue`.)*

1. **Docked & Floating Chat Overlay (`FloatingChatOverlay.vue`):**
   - **Header & execution toolbox:** `AppAgentPortrait` DNA avatar, status badge (`unlaunched`, `idle`, `working`, `blocked`), `AppSelect` / `AppModelEffortSelect` overrides for Runner, Model, Effort, and Account (`selectedAccountId` hydrates with priority `conv?.account_id || info?.account_id || ''`), `ConversationSearchModal.vue`, `ContextTransferModal.vue`, Clean Context (`RotateCcw`), Docked/Floating toggle (`chatLayoutMode`), Messages/Live Terminal toggle (`chatViewMode`), minimize, and close.
   - **Telemetry bar:** Real-time context usage meter (`CTX`), fresh vs. cached token breakdown (or USD cost for OpenCode/DeepSeek), 5-hour / 7-day quota remaining indicators with reset `<AppTooltip>`s, and active process account tag.
   - **Unified Transient Working & Thinking Bubble:** Aggregates real-time reasoning tokens in `supervisor.rs` (`agent_thought_chunk` for OpenCode, `step_update.thought` for Antigravity, streaming blocks for Claude) and emits `thought_text` over `agent-step-update`, rendering in-place at the bottom of the stream until `post_agent_response` or `state: 'DONE'`.
   - **Smart pinned auto-scroll:** Pinned to bottom during streaming; disengages automatically when scrolling up to inspect history.
2. **Centralized Bottom Taskbar (`AgentTaskbar.vue`):**
   - Houses all domain specialists and active ephemeral resolvers (`initializer`, `*-resolver`, `*-crafter` detected across `working`, `running`, `idle`, `starting`, and `blocked` via `isAgentProcessActive`).
   - Enforces `isAgentBelongsToActiveProject(agentId)` so foreign agents never leak across projects.
   - Provides `+` persona creation, inactive-agent filter toggle (persisted in `localStorage` under `oneship:visibility:taskbar_domain_agents:${projectId}`), right-click `AppContextMenu` ("Open Chat", "Restart Agent", "Stop Agent", "Edit Persona"), and `AccountQuotaControls.vue`.
3. **Per-Project UI Workspace State Machine (`workspaceLayoutStore.ts`):**
   - Partitions workspace UI state into `projectWorkspaceStates[projectId]` (`ProjectUiWorkspaceState`) covering `WorkspaceTab`:
     `'overview' | 'project' | 'design' | 'cad' | 'pcb' | 'preview' | 'files' | 'devlog' | 'git' | 'terminal' | 'profiler' | 'rea' | 'skills' | 'electronics'`.
   - `switchProjectContext(fromProjectId, toProjectId)` atomically snapshots the outgoing tab, selected document, `explorerPath`, subtabs, and chat state, and restores the incoming project's state (falling back to `'overview'` if the restored tab is disabled).

### 4. Listener Teardown, Prompt Input Ergonomics & Composite Scoping

1. **Listener teardown lifecycle:** `coreStore.initTauriListeners()`, `dialogueStore.initListener()`, and `App.vue` retain every unsubscribe handle from `onDaemonEvent` / `listenDaemonEvent` in `_unlisteners` / `_dialogueUnlisteners` and expose `teardownListeners()`, closing SSE `_daemonEventSource` connections deterministically on unmount/dispose.
2. **Dynamic Send / Stop toggle & interruption:**
   - Single consolidated execution button in the prompt row: when `isWorking === true`, displays `[■ Stop]` (`Esc`, signals `SIGINT` and re-arms `Idle`) or `[■ Stop & Send]` (`Enter`, signals `SIGINT`, persists user correction, and injects it to the re-armed agent).
   - `isWorking` is computed strictly by verifying status is not `'idle'`, `'completed'`, `'failed'`, or `'unlaunched'`.
3. **Native dictation ergonomics & multi-agent drafts:**
   - The chat microphone starts/stops native Gemini dictation for that prompt (`dictationStore.startDom('chat', ...)`) and the transcript is inserted at the caret. `Alt+V` inside the textarea uses the same path.
   - Auto-send is a global `[plugins.dictation].auto_send` setting toggled from the top-bar microphone; when enabled, the final transcript is submitted to the addressed agent.
   - Unsent drafts and live dictation streams are stored per agent in `dialogueStore.agentInputDrafts`.
4. **Composite key architecture & zero bare-name aliases:**
   - Conversations, `activeSteps`, `agentErrors`, and `pendingRetryPrompts` are keyed strictly by `${projectPath}::${agentId.toLowerCase()}`. `getConversation`, `getActiveStep`, and `getAgentError` return `null` without a project; bare-name aliases and `agentProjectMap` were removed.
   - `agent-error`, `agent-step-update`, `agent-status-report`, and `AgentResponse` events carry `project_path`; unattributed payloads (except from `system`) are logged rather than attributed to a project.
   - `resolveAgentProjectPath(agentId)` resolves only from explicit context (composite key, open floating chat project, selected `activeProjectPath`, or active project membership); never scans unrelated projects.
   - `isAgentLaunching` / `setAgentLaunching` and `coreStore.agentOutcomes` (`getAgentOutcome`) are keyed per `(project, agent)`.

## Contracts

### MCP Tools (`src-tauri/src/mcp.rs`)

- **`post_agent_response`**
  - `dialogue` (`string`, required): Primary human-directed conversation (markdown, lists, inline code, ```text ASCII diagrams). Never a brief stub redirecting to `artifact`.
  - `artifact` (`string`, optional): Technical payload (complete files, diffs, specs, complex tables). Never conversational questions or greetings.
  - `file_path` (`string`, optional): Project-relative path (`docs/*.md`, `scratchpad/*.md`, source files) validated within project root to bind as the turn's live artifact.
  - `artifact_type` (`enum`, optional): `"markdown" | "plan" | "proposal" | "code" | "diff" | "json"` (default `"markdown"`).
  - `artifact_lang` (`string`, optional): Syntax highlight language hint.
  - `approval` (`object`, optional): `{ title: string (required), description?: string, options?: string[] }`.
- **`request_approval`**
  - `approval_type` (`enum`, required): `"technical_decision" | "file_edit" | "command" | "architectural_change"`.
  - `title` (`string`, required): Short summary.
  - `description` (`string`, required): Detailed markdown impact/context.
  - `options` (`string[]`, optional): Custom choices if non-binary.

### Daemon Commands & Database Persistence

- `update_turn_approval({ turnId, status, feedback? })` (`api::API_COMMANDS` / `conversation::update_turn_approval`) -> updates `agent_chat_turns.approval_payload` via `db_update_turn_approval_status`.
- `get_pending_approvals({ projectPath? })` -> lists unresolved rows from `pending_approvals`.
- Storage keys: `oneship:visibility:taskbar_domain_agents:${projectId}` (`localStorage`).

### Daemon Events (SSE `/api/events`)

- `AgentResponse` (enriched with `project_id`, `project_path`)
- `ApprovalRequested`
- `agent-step-update` (includes `project_path`, `thought_text`, `state`)
- `agent-status-report` (includes `project_path`)
- `agent-error` (includes `project_path`)

## Implementation Status

| Item | Status | Evidence |
| :--- | :--- | :--- |
| `post_agent_response` & `request_approval` MCP tools + `CallerContext` attribution | Implemented | `src-tauri/src/mcp.rs` |
| Turn-end compliance tracker & reminder guard (`opencode`, `agy`, `claude`) | Implemented | `src-tauri/src/conversation.rs`, `src-tauri/src/supervisor.rs` |
| `update_turn_approval` & `get_pending_approvals` daemon commands + SQLite persistence | Implemented | `src-tauri/src/api.rs`, `src-tauri/src/db.rs` |
| `FloatingChatOverlay.vue`, `AgentTaskbar.vue`, `dialogueStore.ts`, `workspaceLayoutStore.ts` | Implemented | `src/components/dialogue/FloatingChatOverlay.vue`, `src/components/taskbar/AgentTaskbar.vue`, `src/stores/dialogueStore.ts`, `src/stores/workspaceLayoutStore.ts` |
| Composite key scoping & native chat dictation | Implemented | `src/stores/dialogueStore.ts`, `src/stores/dictationStore.ts`, `src/components/ui/AppTextarea.vue` |

## Related Specs

- [Native Process Supervisor](01-native-process-supervisor.md)
- [Thread, Conversation, and Runner-Neutral Context Continuity](02-thread-conversation-and-continuity.md)
- [Multi-Account Management](04-multi-account-management.md)
- [Quota Observability and Token Economy](05-quota-observability-and-token-economy.md)
- [Dictation and Agent Chat Integration](../05-voice/02-dictation-and-agent-chat.md)
- [Design System Foundations](../07-ui/01-design-system-foundations.md)

Supersedes legacy: `18_mcp_dialogue_channel_and_chat_overlay.md`
