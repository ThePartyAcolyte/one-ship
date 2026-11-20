> [!WARNING]
> **LEGACY SPECIFICATION:** Superseded by [02-agents/03-mcp-dialogue-and-chat-overlay.md](../../specs/02-agents/03-mcp-dialogue-and-chat-overlay.md). Preserved for historical provenance only.

# Specification 18: MCP Dialogue Channel & Unified Agent Chat Overlay

---

## 1. Overview & Problem Statement

### 1.1 Context
In previous builds of ONE-SHIP, AI agents (running via Antigravity / `agy` in terminal panes) were interacted with either via direct terminal typing, platform skill injection, voice dictation, or raw terminal log inspection. When agents initialized, a heavy block of reference context files (`context_docs`) was eagerly injected into their initial system prompt, consuming excessive token quota and cluttering the initial session.

Furthermore, interaction with agents required keeping terminal windows visible. Users lacked a clean, dedicated graphical chat interface separating natural human dialogue (TTS/voice ready) from structured technical outputs (code, diffs, plans, tables).

### 1.2 Objectives
1. **MCP Dialogue Protocol:** Enable agents to stream conversational dialogue and technical artifacts separately via dedicated MCP tools (`post_agent_response` and `request_approval`).
2. **Lazy Context Loading:** When `mcp_dialogue_mode = true`, replace eager `context_docs` injection with a lightweight MCP interaction protocol, allowing agents to inspect files on-demand.
3. **Unified Chat Overlay UI (`AgentDialogueOverlay`):** A full-screen or side overlay featuring a 3-column brutalist layout:
   - **Column 1 (Contacts / Agent List):** All configured project agents grouped by project, displaying real-time lifecycle status (`unlaunched`, `idle`, `working`, `blocked`) and unread badges. Supports launching unlaunched agents directly from chat.
   - **Column 2 (Conversation Stream):** Spoken dialogue bubbles, user prompt input, and clickable artifact chips.
   - **Column 3 (Artifact Reader & Full-Screen Switcher):** Dedicated reader for technical artifacts with language highlighting, collapsible/resizable partition, and a full-overlay artifact picker.
4. **Detached Session & Interlock Detection:** Detect when an agent is active in a terminal session outside the chat overlay or running headless in the background daemon, offering explicit controls to focus terminal, attach, or safely recycle.
5. **Idle-without-MCP Compliance Engine:** Daemon-level monitor tracking `last_user_message_at` vs `last_mcp_response_at` and automatically nudging agents if they turn idle without invoking `post_agent_response`.

---

## 2. Architecture & Data Flow

```
┌────────────────────────────────────────────────────────────────────────┐
│                          ONE-SHIP ARCHITECTURE                      │
│                                                                        │
│  ┌──────────────────────┐              ┌────────────────────────────┐  │
│  │   Agent (in agy/pane)│              │     AgentDialogueOverlay   │  │
│  │                      │              │                            │  │
│  │ 1. Receives prompt   │              │ [Col 1: Agents / Contacts] │  │
│  │ 2. Reasons / tools   │              │ [Col 2: Chat & Input]      │  │
│  │ 3. Calls MCP Tool:   │              │ [Col 3: Artifact Reader]   │  │
│  │    post_agent_response              └──────────────▲─────────────┘  │
│  └──────────┬───────────┘                             │                │
│             │ JSON-RPC (HTTP)                         │ Tauri Event    │
│             ▼                                         │ (AgentResponse)│
│  ┌────────────────────────────────────────────────────┴─────────────┐  │
│  │  Standalone MCP Daemon / Axum Router (`src-tauri/src/mcp.rs`)     │  │
│  │  - `post_agent_response` (emits AgentResponse DaemonEvent)       │  │
│  │  - `request_approval` (emits ApprovalRequested + oneshot await)  │  │
│  │  - Updates `conversation.rs` tracker: mark_agent_responded()     │  │
│  └──────────────────────────────────┬───────────────────────────────┘  │
│                                     │                                  │
│                                     ▼                                  │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │  Conversation State Tracker (`src-tauri/src/conversation.rs`)    │  │
│  │  - Tracks last_user_message_at, last_mcp_response_at             │  │
│  │  - Guards compliance_prompted flag                               │  │
│  └──────────────────────────────────┬───────────────────────────────┘  │
│                                     │ Polled by Watcher                │
│                                     ▼                                  │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │  Agent Watcher Loop (`src-tauri/src/watcher.rs`)                 │  │
│  │  - Detects state transitions (working -> idle)                   │  │
│  │  - Injects compliance reminder prompt if agent failed to call MCP│  │
│  └──────────────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 3. MCP Tool Contracts

### 3.1 `post_agent_response`
- **Purpose:** Finalizes an agent turn by routing human-directed dialogue and optional technical artifacts to the UI.
- **Strict Project Attribution & Automatic Project Envelope:**
  - Every response is attributed to the managed project of the calling agent instance, resolved from the process-attested `CallerContext` (spec 12 §6): TCP peer socket → owning PID → `ONESHIP_AGENT_INSTANCE` → `agent_instances` registry.
  - **Zero Agent Burden Invariant:** The AI agent (LLM) never supplies its own `agent_id`, `project_uid`, `project_id`, or `project_path`; the tool schema does not accept them. All routing, namespacing, and identity resolution are handled exclusively by the One-Ship platform.
  - **Automatic Payload Enrichment:** The MCP handler automatically resolves `project_entry.id` and `project_entry.path` and injects `"project_id"` and `"project_path"` directly into the emitted `AgentResponse` Tauri event payload, allowing the frontend store to cache the binding without agent involvement.
  - If project context cannot be verified, the call is rejected immediately with JSON-RPC `isError: true`, preventing spurious database writes, orphaned chat turns, and UI crosstalk.
- **Dual-Path Artifact Resolution:**
  - `artifact` may be an inline string (code snippet, unified diff, markdown table, JSON schema) OR a filesystem path / Antigravity brain artifact reference (e.g. `path/to/file.md` or `.gemini/antigravity-cli/brain/<cid>/...`).
  - When a file path is detected, the MCP handler reads the file content automatically, infers type and language, and binds the content to the `AgentResponse` payload.
  - If no technical output was produced during the turn, `artifact` is set to `null` or omitted.
- **Arguments:**
  - `dialogue` (string, required): Primary conversational channel. Explanations of decisions, context, workflow roadmaps, and all interactive questions directed to the user. Markdown is fully supported (lists, formatting, inline code, and ```text ASCII flowcharts). Never leave dialogue as a brief redirection stub.
  - `artifact` (string, optional): Technical companion payload. Reserved strictly for complete files, git diffs, formal specifications, or complex tables. FORBIDDEN: Never put conversational questions, greetings, or scoping agendas in `artifact`.
  - `file_path` (string, optional): Project-relative path to a file on disk (`docs/*.md`, `scratchpad/*.md`, source files) to bind and preview as the turn's live artifact. Must resolve inside the project root.
  - `artifact_type` (enum, optional): `"markdown" | "plan" | "proposal" | "code" | "diff" | "json"`. Defaults to `"markdown"`.
  - `artifact_lang` (string, optional): Syntax highlight hint (e.g. `"rust"`, `"typescript"`, `"toml"`).
  - `approval` (object, optional): Interactive proposal or multi-option strategy selector rendered in the Chat Overlay for user review and selection.
    - `title` (string, required): One-line title of the proposed action or decision.
    - `description` (string, optional): Brief description or rationale.
    - `options` (array of strings, optional): Actionable choice buttons rendered in the chat stream (e.g. `["1. Purge inline comments", "2. Annotate with IDs", "3. Preserve as-is"]`). Clicking an option button immediately dispatches a preformed user prompt to the agent's interactive REPL.

### 3.2 `request_approval`
- **Purpose:** Synchronously blocks execution to request explicit user authorization for sensitive actions (file changes, architecture decisions, CLI commands).
- **Persistence & UI Recovery:**
  - When invoked, the pending approval is persisted to SQLite (`pending_approvals` table) associating `task_id`, `project_path`, `phase`, and `payload_markdown`. If the UI reloads, pending approvals remain actionable.
- **Arguments:**
  - `approval_type` (enum, required): `"technical_decision" | "file_edit" | "command" | "architectural_change"`.
  - `title` (string, required): Short summary.
  - `description` (string, required): Detailed context / impact in markdown.
  - `options` (array of strings, optional): Custom choices if non-binary.

---

## 4. UI Design System & Component Layout

*(Historical Note: The early Phase 2A–2D components `ChatFab.vue`, `AgentDialogueOverlay.vue`, `AgentList.vue`, `ConversationPanel.vue`, and `ArtifactPanel.vue` were superseded by the unified bottom taskbar `AgentTaskbar.vue`, `FloatingChatOverlay.vue`, and the integrated Explorer/Reader in `ProjectWorkspace.vue`).*

### 4.1 Docked & Floating Chat Overlay (`FloatingChatOverlay.vue`)

A dockable and floating chat window enabling ubiquitous interaction with any project agent across all workspace views:
1. **Header & Execution Toolbox Controls:**
   - Agent avatar with pixel portrait DNA (`AppAgentPortrait`) and role metadata.
   - Status badge (`unlaunched`, `idle`, `working`, `blocked`).
   - Runner, Model, Effort, and Account selector controls (`AppSelect` / `AppModelEffortSelect`) allowing on-the-fly execution configuration overrides.
   - Conversation search (`ConversationSearchModal.vue`), Context Transfer (`ContextTransferModal.vue`), and Clean Context (`RotateCcw`) triggers.
   - Docked vs. Floating layout mode toggle (`chatLayoutMode`) and Messages vs. Live Terminal stream view toggle (`chatViewMode`).
   - Minimize to bottom taskbar and close actions.
2. **Context Window, Token Usage & Quota Telemetry Bar:**
   - Real-time `CTX` usage indicator displaying current context percentage with a dynamic visual progress bar.
   - Token breakdown displaying total tokens, fresh input tokens, cache read tokens, and cache hit percentage (or USD cost for OpenCode/DeepSeek).
   - Rolling rate limit indicators: 5-hour and 7-day remaining quota percentages with reset timing tooltips.
   - Active process account tag wrapped in standard `<AppTooltip>`.

### 4.2 Per-Project UI Workspace State Machine (`workspaceLayoutStore.ts`)

To guarantee seamless developer continuity across multiple concurrent projects, workspace layout states are partitioned into a per-project state machine in `src/stores/workspaceLayoutStore.ts`:

1. **State Machine Contract (`ProjectUiWorkspaceState`):**
   ```ts
   export type WorkspaceTab =
     | 'overview' | 'project' | 'design' | 'cad' | 'pcb' | 'preview'
     | 'files' | 'devlog' | 'git' | 'terminal' | 'profiler' | 'rea'
     | 'skills' | 'electronics';
   ```
2. **Atomic Context Switch (`switchProjectContext`):**
   - **Outgoing Snapshot:** When switching away from `fromProjectId`, the store atomically captures the current active tab, selected document, explorer scope/directory, subtabs, and chat presence into `projectWorkspaceStates[fromProjectId]`.
   - **Incoming Restoration:** When switching into `toProjectId`, restores the previous tab, opened document, and chat window status, falling back to `'overview'` if the restored tab is disabled on the target project.
3. **Bottom Taskbar Centralization & Strict Ownership (`AgentTaskbar.vue`):**
   - **Centralized Presence:** All domain project specialists and ephemeral flag resolvers (`initializer`, `*-resolver`, `*-crafter`) are managed directly from `AgentTaskbar.vue` and `FloatingChatOverlay.vue`.
   - **Contextual Management:** Houses agent portraits (`AppAgentPortrait`), creation triggers (`+`), filter toggles (show/hide inactive agents), right-click context menus (`AppContextMenu`), and global account/quota controls (`AccountQuotaControls.vue`).
   - **Strict Project Scoping:** Enforces `isAgentBelongsToActiveProject(agentId)` on all candidate tabs. Foreign agent conversations from other projects never leak into the bottom taskbar.
   - **Canonical Storage Keys:** Inactive agent visibility preference is persisted using `oneship:visibility:taskbar_domain_agents:${projectId}`.

### 4.6 Listener Teardown Lifecycle & Memory Safety
- **Daemon Event & Tauri Unlisten Registry:** All frontend store listeners (`coreStore.initTauriListeners()`, `dialogueStore.initListener()`, `App.vue` window-level events) register their unsubscribe callbacks (from `onDaemonEvent` / `listenDaemonEvent` in `src/utils/daemonEvents.ts`, or Tauri `listen` for the events of commands still in the app) into managed arrays (`_unlisteners`, `_dialogueUnlisteners`).
- **Deterministic Teardown:** The stores provide explicit `teardownListeners()` methods invoked on component unmount and session disposal. SSE EventSource connections (`_daemonEventSource`) are closed deterministically to prevent zombie connections, memory bloat, and duplicate event handler execution across hot-reloads.

### 4.7 Prompt Input Row, Stop Deduplication & OpenDictate Voice Integration
1. **Dynamic Send / Stop Toggle:**
   - The user prompt input row features a single, consolidated execution button.
   - When the agent is active (`isWorking === true`), the button switches to `[■ Stop]` (empty input) or `[■ Stop & Send]` (if input contains text).
   - The redundant secondary Stop button inside the live action indicator bar was removed to prevent visual clutter and confusion.
   - **`isWorking` State Invariant:** The UI rigorously computes `isWorking` by checking that the agent is not in `'idle'`, `'completed'`, `'failed'`, or `'unlaunched'`. Resumed or initialized agents with idle supervisor states correctly display `Send` instead of a stuck `Stop` button.
2. **OpenDictate Live Speech Watermark & Click-to-Reserve:**
   - Clicking or focusing `AppTextarea` automatically reserves the dictation slot via `voiceStore.armReservation()`.
   - A non-interactive subtle watermark microphone icon sits inside the textarea (`ready` when armed, `rec` when recording).
   - If the user blurs the input before speaking, the reservation is released. Once speech begins, hands-free dictation streams into the textarea even if focus is lost.
3. **Multi-Agent Draft Centralization (`agentInputDrafts`):**
   - Unsent prompt drafts and live voice streams are keyed by `agentId` in `dialogueStore.agentInputDrafts`.
   - Navigating between different agents or switching workspace tabs preserves typed text and active voice streams without data loss.

### 4.8 Composite Key Architecture & Exact Project Scoping Invariant
1. **Composite Key Storage (`${projectPath}::${agentId.toLowerCase()}`):**
   - Active conversations, messages, and live step updates in `dialogueStore` are strictly keyed by `${projectPath}::${agentId.toLowerCase()}`.
   - Eliminates cross-project chat collisions and prevents agents with identical names (e.g. `bosmang`) in different projects from sharing state or receiving foreign turns.
2. **Elimination of Flat Maps & Ambiguous Heuristics:**
   - The flat `agentProjectMap` dictionary and permissive candidate fallbacks (`candidateProjects[0]`) have been completely purged in accordance with Rule 11.
   - Project path must be provided explicitly or derived unambiguously from `coreStore.activeProject.path`.
3. **No bare-name aliases (2026-10-09):**
   - Conversations, live steps (`activeSteps`), agent errors (`agentErrors`) and retry prompts are stored only under the composite key; the `conversations[agentId]` / `activeSteps[agentId]` aliases are removed, and `getConversation`, `getActiveStep`, `getAgentError` return `null` without a project.
   - `agent-error`, `agent-step-update`, `agent-status-report` and `AgentResponse` events always carry `project_path`; a consumer that receives one without it reports it in the status log instead of attributing it.
   - `read_terminal_output` rejects a bare agent name without `project_path`; the agent terminal pane passes the chat's project.
   - `agentProjectMap` is gone. `resolveAgentProjectPath(agentId)` answers only from explicit context: a composite key, the open floating chat's project, the project of the agent the user selected (`activeProjectPath`), or the active project when the agent belongs to it. It never scans other projects, initializers or the active conversation record, and returns `null` otherwise.
   - The launch guard (`isAgentLaunching`/`setAgentLaunching`) and `coreStore.agentOutcomes` (`getAgentOutcome`) are keyed per `(project, agent)`; the `agent-status-report` event (emitted directly by `report_task_status` and the initializer; the former global `last_agent_status.json` file and `get_last_agent_status` command are removed) carries `project_path`, and a report without it is not attributed (except from the project-less `system` agent).
   - `ConversationPanel.launchCurrentAgent` launches in the chat's project, not in the first project that has an agent with that name.
3. **Conversation Account Precedence:**
   - In `FloatingChatOverlay.vue`, the active account dropdown hydrates with priority: `selectedAccountId = conv?.account_id || info?.account_id || ''`.
   - The explicitly selected conversation account overrides transient runtime process telemetry, preventing account switching regressions.
4. **Zero Agent Burden:**
   - Multi-turn conversations remain completely agnostic of project IDs or filesystem paths on the agent side. Agents operate solely within their local workspace context while One-Ship handles all lifecycle, scoping, and UI isolation deterministically.

---

## 5. Implementation Status

- [x] **Phase 1 (Backend MCP & Lazy Prompting):**
  - Added `post_agent_response` and `request_approval` tools to `mcp.rs`.
  - Created `conversation.rs` tracker module and registered in `lib.rs`.
  - Added `mcp_dialogue_mode: bool` to the agent state and `rula.toml` parser.
  - Updated `build_agent_initial_prompt` to inject response protocol and skip eager context docs.
- [x] **Phase 2A (Core Frontend Store & Components):**
  - Created `dialogueStore.ts` with Pinia state, unread tracking, and `AgentResponse` listener.
  - Created `ChatFab.vue`, `AgentDialogueOverlay.vue`, `AgentList.vue`, `ConversationPanel.vue`, and `ArtifactPanel.vue`.
  - Mounted in `App.vue` and connected to `coreStore`.
- [x] **Phase 2B (Refinements & Orchestration Guard):**
  - Direct agent launch from `AgentList.vue` and `ConversationPanel.vue` for unlaunched agents.
  - Aligned Tauri IPC command parameters (`target`, `prompt`) and error notification in `dialogueStore.ts`.
  - Added standalone daemon SSE event forwarding in `coreStore.ts` and response deduplication.
  - Added backend auto-bootstrap in `prompt_agent` for unlaunched agents.
  - Enabled `mcp_dialogue_mode = true` in `rula.toml`.
- [x] **Phase 2C (Background Interaction, Quiet Handshake & Ergonomics):**
  - Eliminated imperative verification command triggers from agent persona prompts (descriptive stack only).
  - Enforced quiet startup handshake in `build_agent_initial_prompt` (immediate `post_agent_response` greeting, zero unprompted execution).
  - Enforced `should_auto_approve` for dialogue mode (passes `--dangerously-skip-permissions` to eliminate terminal keyboard stalls).
  - Disabled focus stealing on launch (`auto_focus = false` prevents the terminal from grabbing desktop focus).
  - Implemented `localStorage` chat history persistence and system boundary events (`role: 'system'`).
  - Resolved floating log overlap on chat textarea by hiding bottom ticker in dialogue mode and docking a multi-line wrapped log stream in `AgentList.vue`.
- [x] **Phase 2D (View Modes, Grouping & Partitioning):**
  - Group agents by project in `AgentList.vue` with dynamic model grouping, active-first view, and internal agent isolation.
  - Draggable split-pane divider (`AgentDialogueWorkspace.vue`) for conversation vs artifact reading width.
  - Initial workspace state without forced auto-selection of internal agents (`activeAgentId = null` empty state).
- [x] **Phase 2F (Floating Chat Overlay, Account Mismatch & In-Chat Telemetry):**
  - Implemented `FloatingChatOverlay.vue` with docked and floating modes.
  - Added real-time in-chat account mismatch banner with one-click `restart_agent` session reload.
  - Added brutalist Context Window, Token Breakdown, and 5h/7d Quota telemetry bar above chat stream.
- [x] **Phase 2G (Unified Live Thought Streaming, Transient Working Bubble & Smart Auto-Scroll):**
  - Replaced the top amber warning telemetry banner with a unified Transient Working & Thinking Bubble situated directly at the bottom of the conversation stream.
  - Aggregated real-time reasoning tokens in `supervisor.rs` for both OpenCode (`agent_thought_chunk`) and Antigravity (`step_update.thought`), preventing terminal log shredding and vertical cascade flooding.
  - Emitted `thought_text` over `agent-step-update` to power dynamic in-place thought rendering that resets/rewrites per thinking phase in multi-step tool turns.
  - Dissolves the transient bubble cleanly upon delivery of `post_agent_response` or turn termination (`state: 'DONE'`).
  - Added bidirectional smart pinned auto-scroll for both chat history and live terminal `<pre>` streams, preventing scroll hijacking when reading history and re-engaging auto-scroll when near bottom.
- [x] **Phase 2H (Multi-Runner Execution Toolbox, Clean Session Lifecycles & Context Handoff):**
  - In-chat execution toolbox (`AppSelect` controls for Runner, Model, and Account) permitting on-the-fly model switching and explicit account binding without global account pollution.
  - Removed legacy account mismatch reload banner; account selections are strictly user-directed.
  - Runner-aware telemetry bar dynamically adapting between Antigravity (5h/7d quotas, token cache breakdowns) and OpenCode/DeepSeek (usage token metrics, USD cost tracking, context limit ratios).
  - Cross-runner resumption guard preventing incompatible session restoration and auto-archiving foreign runner conversations.
  - Context & Minutes Handoff system (`ContextTransferModal.vue`) allowing structured context, recent turn highlights, and `agent_session_journal` minutes to be transferred to new or existing project agents.
- [x] **Phase 2I (Conversational Lifecycle Decoupling, Session Decision Card & Auto-Retry):**
  - Unlaunched/stopped agents render full conversation history with bottom Session Decision Card: `[ Resume Session ]` (preserves context without initial persona prompt injection) vs `[ New Clean Session ]` (archives previous session and injects persona directives).
  - Persona directives (`build_agent_initial_prompt`) are injected strictly when starting a brand new session (`clean_context == Some(true)` or zero prior history). Resumed sessions never receive initial persona prompt injection.
  - In-chat quota exhausted banner displays actionable recovery paths with optimal recommended account (`[REC]`), sorting viable accounts by `weekly_reset_epoch_seconds ASC`.
  - Failed prompts on quota error (429 / RESOURCE_EXHAUSTED) are captured in `pendingRetryPrompts` and auto-dispatched upon account switch/retry without duplicate chat bubbles.
- [x] **Phase 2J (Persistent Inline Approvals & Unobtrusive Auto-Resumed Context):**
  - Added `approval_payload` column and automated migration to `agent_chat_turns` table in SQLite.
  - Persisted structured approvals delivered via `post_agent_response` into SQLite, allowing pending approvals to rehydrate interactively across app closures and restarts.
  - Added `update_turn_approval_cmd` IPC command to persist user decisions (accepted, rejected, selected options) back to the database.
  - Replaced bulky session decision cards with streamlined auto-launch and auto-resume lifecycle: opening an agent seamlessly attaches to ongoing context.
  - Brutalist status badge reflects `conversation resumed` with clear, zero-clutter indicator, while retaining the minimal `RotateCcw` button for starting clean sessions.
- [x] **Phase 2K (Per-Project UI Workspace State Machine & Cross-Project Isolation):**
  - Partitioned workspace and dialogue UI states into `projectWorkspaceStates` map in `dialogueStore.ts`.
  - Implemented `switchProjectContext` routine in store to snapshot outgoing project context and restore incoming project context with capability fallbacks.
  - Bound `explorerPath` two-way to preserve directory navigation per project.
  - Guarded `ProjectGitSnapshotsTab` against sticky rendering on incompatible projects.
- [x] **Phase 2L (Mid-Turn Execution Interruption & Dynamic Dual Send/Stop Button):**
  - Transformed the Send button into a dynamic Send / Stop dual button in `FloatingChatOverlay.vue` and `ConversationPanel.vue` when `isWorking` is true.
  - Implemented one-step "Stop & Send Correction": typing a corrective prompt in the textarea while the agent is active transforms the button into `[■ Stop & Send]` (Enter key). Clicking or pressing Enter signals `SIGINT` to halt the active tool or turn, saves the user correction to SQLite, and immediately feeds the prompt to the re-armed agent in one step.
  - If the textarea is empty, the button acts as `[■ Stop]` (Esc key), signaling `SIGINT` and re-arming the agent in `Idle` without prompt injection.
  - Deduplicated Stop controls: removed the redundant secondary Stop button from the live action indicator bar.
  - Refined `isWorking` state machine to strictly guard against `idle`, `completed`, `failed`, and `unlaunched` states, eliminating false-positive stuck Stop buttons on resumed sessions.
- [x] **Phase 2M (Real-Time OpenDictate Speech Streaming & Input Ergonomics):**
  - Integrated OpenDictate (`org.kirulab.OpenDictate`) D-Bus signals (`SessionStarted`, `InterimText`, `SessionFinished`, `SessionCancelled`, `SessionReservationReleased`) via dedicated `voiceStore.ts`.
  - Added click-to-reserve and focus handlers to `AppTextarea.vue` (`reserve_voice_capture` with `hide_bubble: true`).
  - Added idle blur-to-cancel invariant (`isArmed && !isRecording`) while protecting active recordings (`isRecording === true`) from focus loss or window blur.
  - Added subtle non-interactive watermark microphone icon inside the textarea reflecting `ready` and `rec` states without layout shifts.
  - Centralized multi-agent draft inputs in `dialogueStore.agentInputDrafts`, preserving prompt drafts and streaming speech across agent switching.
- [x] **Phase 2N (Deterministic Agent Project Scoping & Zero-Burden Protocol):**
  - Eliminated blind `projects[0]` fallback loops in backend `prompt_agent` supervisor routing.
  - Prioritized `coreStore.activeProject` in frontend `resolveAgentProjectPath`, `getActiveProjectPath`, and `findProjectForAgent`.
  - Propagated explicit `projectPath` across all UI launch points (`AgentSection.vue`, `AgentTaskbar.vue`, `FloatingChatOverlay.vue`, `ConversationPanel.vue`, `AgentList.vue`, `RollingStatusLog.vue`, `App.vue`).
  - Automatically enriched outgoing `AgentResponse` MCP event payloads with `project_id` and `project_path`, maintaining continuous multi-turn project binding without burdening the agent model.
- [x] **Phase 2O (Centralized Agent Taskbar & Overview Purification):**
  - Consolidated agent lifecycle interactions (launch, edit persona, filter, avatar DNA rendering) exclusively into `AgentTaskbar.vue` and the Floating Chat Overlay.
  - Decommissioned the legacy internal agent list and the obsolete "Reset All" action from the Project Overview card.
  - Standardized local storage persistence to `oneship:visibility:taskbar_domain_agents:${projectId}`.
- [x] **Phase 2E (Turn-End Compliance Guard & Error Recovery Loop):**
  - Implemented directly at turn completion inside `src-tauri/src/supervisor.rs` across all three runners (`opencode`, `agy`, `claude`) via `conversation::should_inject_compliance_reminder`: when an agent finishes a turn without invoking `post_agent_response`, the supervisor either ingests the fallback response or injects a corrective compliance prompt.



