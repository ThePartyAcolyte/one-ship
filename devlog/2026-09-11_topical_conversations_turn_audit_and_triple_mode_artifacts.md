# 2026-09-11: Discrete Topical Conversations, Complete Turn Audit Trail & Triple-Mode Artifact Viewer

## 1. Context & Motivation
Following the transition of artifacts into live, file-backed project scratchpads (`Spec 20`), the conversational layer required two crucial enhancements:
1. **Discrete Topical Conversations:** When exploring multiple disparate topics or bugs with an agent, stacking all turns into a single monolithic chat causes context degradation and token bloat. Agents and users need to partition sessions into discrete topics (`agent_conversations`) and switch between them cleanly.
2. **Complete Turn Audit Trail:** Auditing agent interactions requires persisting not only conversational dialogue and touched documents, but also an immutable snapshot of what the agent was seeing/executing in the terminal pane at that turn (`terminal_snapshot`).
3. **Multi-Tier Artifact Navigation & Live Terminal Mode:** In Column 3, users need to cleanly distinguish between active conversation scratchpads, permanent `docs/` (flagging those altered in this session with `[● MODIFIED]`), ephemeral in-memory artifacts (`[EPHEMERAL]`), and a live rolling Terminal Log.

---

## 2. Changes Delivered

### 2.1 Backend SQLite Persistence & Turn Audit Trail (`src-tauri/src/db.rs`, `src-tauri/src/mcp.rs`, `src-tauri/src/conversation.rs`)
- **Schema & Indexes:**
  - `agent_conversations`: `(id TEXT PRIMARY KEY, project_id INTEGER, agent_id TEXT, topic TEXT, status TEXT, created_at INTEGER, updated_at INTEGER, artifacts_touched TEXT)`.
  - `agent_chat_turns`: `(id INTEGER PRIMARY KEY AUTOINCREMENT, conversation_id TEXT, project_id INTEGER, agent_id TEXT, role TEXT, dialogue TEXT, artifact_type TEXT, artifact_lang TEXT, artifact_label TEXT, artifact_file_path TEXT, artifact_content TEXT, is_ephemeral INTEGER, terminal_snapshot TEXT, timestamp INTEGER)`.
  - High-performance SQLite indices on `(project_id, agent_id, updated_at DESC)` and `(conversation_id, id DESC)`.
- **MCP Response Hooking (`mcp.rs::execute_post_agent_response`):**
  - Resolves agent project ID, obtains/creates active conversation, detects ephemeral vs file-backed artifacts.
  - Samples latest 60 lines of Herder terminal output via `kopeng::herdr::get_terminal_output`.
  - Persists `ChatTurnRecord` in SQLite with terminal snapshot and touches `artifacts_touched` in `agent_conversations`.
- **MCP Topic Switching Primitive (`mcp.rs::execute_switch_conversation_topic`):**
  - Added `switch_conversation_topic` to `tools/list` and `tools/call`.
  - Enables agents or UI to switch active topics or create fresh ones.
  - Injects optional transition summary as a `system` turn.
  - Emits `DaemonEvent::TopicSwitched` for reactive frontend synchronization.

### 2.2 Frontend Pinia Store (`src/stores/dialogueStore.ts`)
- Added state: `activeConversationId`, `activeTopic`, `conversationsList`, `hasMoreHistory`, `isLoadingHistory`, `oldestLoadedTurnId`, `artifactsTouched`.
- Added actions:
  - `loadActiveConversation(projectPath, agentId)`: Hydrates active topic and latest 20 turns from SQLite.
  - `loadOlderMessages()`: Paginates earlier turns from SQLite using `before_id` cursor.
  - `switchTopic(projectPath, agentId, conversationId)`: Switches active session and re-hydrates messages.
  - `createTopic(projectPath, agentId, topic)`: Creates fresh topic in SQLite.
  - `sendUserMessage`: Persists user turns to SQLite via `save_user_chat_turn_cmd`.
- Listeners for `AgentResponse` and `TopicSwitched` daemon events.

### 2.3 Conversation Stream & Topic Switcher (`src/components/dialogue/ConversationPanel.vue`)
- Header: Integrated discrete topic selector dropdown with session counts and inline `+ New Topic` creation form.
- Stream: Added `[↑ Load older messages (+10)]` pagination button with loading spinner.
- Badges: Agent message chips now distinguish `[LIVE]` vs `[EPHEMERAL]` artifacts, and show a `[Log]` button for turns containing terminal snapshots.

### 2.4 Triple-Mode Artifact Panel & 3-Tier Selector (`src/components/dialogue/ArtifactPanel.vue`)
- Header Mode Switcher:
  - `[Live Artifact]`: Live disk-backed artifact linked to active conversation with 2.5s auto-polling.
  - `[Docs & Scratchpad]`: Explorer drawer for repository documents and scratchpads.
  - `[Terminal Log]`: Live streaming view of agent's Herder terminal buffer with `All` vs `Errors Only` filtering and quick copy.
- 3-Tier Categorized Artifact Picker:
  - **Tier 1 (This Conversation):** Scratchpads created or touched in current topic.
  - **Tier 2 (Project Docs):** Strictly `docs/` files, showing `[● MODIFIED]` badge if altered in this conversation.
  - **Tier 3 (Ephemeral Artifacts):** In-memory markdown tables, execution plans, and diffs with `[EPHEMERAL]` badge.

---

## 3. Verification & Compliance
- **Rust Backend:** `cargo check` passed with 0 errors and 0 warnings.
- **Frontend TypeScript:** `npx vue-tsc --noEmit` passed with 0 errors.
- **No Shim / Greenfield Policy:** Implemented directly on the SQLite database layer and Herder socket without legacy backward-compatibility wrappers.
- **Spec Synchronization:** Updated `docs/specs/20_live_artifacts_and_project_scratchpad.md`, `.agents/skills/kapawu-platform/references/mcp_primitives.md`, and `templates/skills/kapawu-platform/references/mcp_primitives.md`.
