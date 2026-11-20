# Devlog: 2026-09-09 — MCP Dialogue Channel & Unified Agent Chat Overlay

---

## 1. Work Accomplished in this Session

### 1.1 MCP Protocol & Backend Infrastructure
- **New MCP Tools in `src-tauri/src/mcp.rs`:**
  - `post_agent_response`: Routes natural spoken language (TTS-ready) and optional structured artifacts (code, diffs, plans, tables, markdown) to the UI.
  - `request_approval`: Synchronous approval request with custom options and descriptions, using an async oneshot channel awaiting UI confirmation.
- **Conversation Tracker (`src-tauri/src/conversation.rs`):**
  - Created an in-memory tracker mapping `last_user_message_at`, `last_mcp_response_at`, and `compliance_prompted` flag to prevent spam reminders.
  - Exported helper functions `mark_user_message_sent`, `mark_agent_responded`, and `should_inject_compliance_reminder`.
  - Registered `pub mod conversation;` in `src-tauri/src/lib.rs`.
- **Lazy Context Loading & Mode Configuration in `src-tauri/src/kopeng/herdr.rs`:**
  - Added `mcp_dialogue_mode: bool` to `HerdrAgentState` and `ConfiguredAgent`.
  - Added parsing for `mcp_dialogue_mode` at both `[system]` and `[agents.<id>]` levels in `rula.toml`.
  - Refactored `build_agent_initial_prompt` so that when `mcp_dialogue_mode = true`, eager `[CONTEXT REFERENCE]` docs are omitted and replaced with the structured MCP response protocol.

### 1.2 Frontend Architecture & Components
- **Pinia Dialogue Store (`src/stores/dialogueStore.ts`):**
  - Tracks open state, active agent, per-agent conversations, unread counters, and session artifacts.
  - Subscribes to the `AgentResponse` Tauri event and updates active conversation models in real-time.
- **UI Components Created (`src/components/`):**
  - `components/floating/ChatFab.vue`: Floating action button in bottom-right with live unread badge.
  - `components/dialogue/AgentDialogueOverlay.vue`: Fullscreen modal container with escape key bindings.
  - `components/dialogue/AgentList.vue`: Left sidebar displaying agent contacts with role and lifecycle status dots.
  - `components/dialogue/ConversationPanel.vue`: Chat area with user/agent speech bubbles, time formatting, artifact chip triggers, and auto-scrolling.
  - `components/dialogue/ArtifactPanel.vue`: Full-panel artifact reader with language badges, formatted pre-blocks, and a transition modal picker overlay.
- **Integration in `App.vue` & `coreStore.ts`:**
  - Registered `AgentResponse` listener in `coreStore.initTauriListeners()`.
  - Rendered `ChatFab` in the floating controls bar and mounted `AgentDialogueOverlay` at root.
  - Verified compilation clean with `cargo check` and `vue-tsc --noEmit`.

---

## 2. Verified Technical Health
- Rust backend: `cargo check` finished with 0 errors.
- Frontend: `npx vue-tsc --noEmit` finished with 0 errors.

---

## 3. Pending Tasks for Future Sessions
1. Group agent contacts by project in `AgentList.vue` with activation trigger for unlaunched agents.
2. Implement draggable column resizer between conversation stream and artifact reader.
3. Wire `should_inject_compliance_reminder` into `spawn_herdr_watcher` loop to nudge agents when they transition to idle without calling `post_agent_response`.
4. Add detached session / Herder window interlock warning when agent is being operated outside the overlay.
