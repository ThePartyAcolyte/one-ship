# Devlog: 2026-09-10 — Quiet Startup Handshake, Headless Focus, Persona Prompt Hygiene & Chat Ergonomics

---

## 1. Context & Architectural Problem

Following initial validation of the chat overlay, several operational friction points were identified when launching agents in background chat mode:
1. **Imperative Prompt Triggers:** Initial persona generation previously included "Verification command: <cmd>" within `system_prompt`. When injected as the first user turn, LLM agents (such as `bosmang` in `fideiuris`) interpreted this as an immediate imperative task, proactively executing commands (e.g. `docker compose ps`) before the human developer had prompted them.
2. **Terminal Permission Stalls:** In projects with `auto_approve = false`, CLI runners (`agy`) paused on terminal standard input awaiting `y/n`, breaking background headless operation.
3. **Focus Stealing:** Launching an agent switched desktop focus to the Herder terminal window, contradicting the goal of an autonomous background chat interface.
4. **Lack of Live Feedback:** The chat stream gave no visual indication when an agent was bootstrapping or thinking.
5. **UI Element Collision:** The floating collapsed rolling status log bar (`fixed bottom-4`) overlapped and obscured the chat input textarea.
6. **Chat History Ephemerality:** In-memory chat conversations were wiped on page reloads and lacked clear visual boundaries across runner lifecycle recycles.

---

## 2. Solutions Implemented

### 2.1 Persona Prompt Hygiene & Initial Handshake (`skill.rs`, `rula_schema.md`, `fideiuris/rula.toml`, `herdr.rs`)
- **Removed Imperative Verification Commands:**
  - Updated project initialization rules in `src-tauri/src/kopeng/skill.rs` and `templates/skills/kapawu-platform/references/rula_schema.md`.
  - Persona `system_prompt` strings must contain purely descriptive stack context; verification commands belong strictly under `[scripts]`.
  - Cleaned up `/home/butcherwutcher/Projects/dev/fideiuris/rula.toml` to remove `Verification command:` strings from `bosmang`, `backend`, `frontend`, and `git`.
- **Enforced Quiet Startup Handshake in `build_agent_initial_prompt`:**
  - Instructs the agent that it is in background dialogue mode.
  - Prohibits proactive file inspection or command execution upon boot.
  - Commands the agent to immediately emit a brief readiness greeting via `post_agent_response` and remain idle awaiting user instructions.

### 2.2 Unattended Execution & Headless Launch (`herdr.rs`)
- **Automatic Skip-Permissions for Dialogue Mode:**
  - `bootstrap_agent_in_pane` now evaluates `let should_auto_approve = auto_approve || agent_info.mcp_dialogue_mode`. When in dialogue mode, `--dangerously-skip-permissions` is always passed to prevent terminal stdin blocks.
  - Governance remains enforced via the `request_approval` MCP tool rendering native GUI approval modals.
- **Disabled Herder Focus Stealing:**
  - `auto_focus` in `launch_herdr_agent` now defaults to `false`.
  - `create_workspace_sync` and `create_tab_sync` are executed without desktop window stealing.

### 2.3 Visual Feedback & Layout Ergonomics (`ConversationPanel.vue`, `AgentList.vue`, `App.vue`)
- **Live Thinking / Working / Bootstrapping Indicator:**
  - Added a reactive indicator with pulsing icon above `messagesEnd` whenever the agent is in `working` state or actively bootstrapping.
- **Resolved Bottom Overlap:**
  - `RollingStatusLog.vue`: accepts `hideCollapsed` prop and listens for `open-status-log` events.
  - `App.vue`: hides the floating collapsed ticker when `activeWorkspace === 'dialogue'`.
  - `AgentList.vue`: added a docked mini system log footer at the bottom of Column 1 with a clickable `<History>` icon to expand the log viewer modal.
- **Chat History Persistence & System Events (`dialogueStore.ts`):**
  - Conversations are automatically persisted to and hydrated from `localStorage`.
  - Added `role: 'system'` message type rendered as a subtle horizontal divider line (`// <message>`) to mark runner session recycles and context refreshes without losing chat history.

---

## 3. Technical Verification

- Backend compilation: `cargo check --manifest-path src-tauri/Cargo.toml` passed with 0 errors (0.41s).
- Frontend typechecking: `npx vue-tsc --noEmit` passed with 0 errors.
