# Development Log: Agent Auto-Launch on Selection and Strict Last-Turn Approval Flow

**Date:** 2026-09-26  
**Topic:** Automatic Subprocess Activation on Chat Selection, Single-Flight Idempotent Resume, and Strict Last-Turn Approval Lifecycle  
**Version:** v0.2.4  

---

## 1. Context & Objectives

1. **Persistent Ghost `! APPROVAL` Chips:**
   - In resumed conversations (e.g. `frontend`), agent tabs in `AgentTaskbar.vue` continuously pulsed amber with a `! APPROVAL` badge even when the latest turn had already accepted a proposal or moved on.
   - Root cause: `AgentTaskbar.vue` used `c?.messages?.some(...)` over the entire conversation history. Because historical turn records retained `approval_payload.status = 'pending'` in SQLite, past answered proposals permanently flagged the agent tab as requiring approval.
2. **Unlaunched State & Dead Terminal on Agent Selection:**
   - Opening an agent from the chat bar, agent launcher popover, or workspace agent list opened the chat window but left the process `unlaunched`.
   - The user had to manually click a Play button in the header or type a prompt to start the agent runner.
   - Terminal stream and live agent state remained disconnected until manual initiation.

---

## 2. Technical Implementation Summary

### A. Strict Last-Turn Approval Invariant & Superseding Lifecycle
- **Taskbar Scoping (`AgentTaskbar.vue`):**
  - Introduced `checkConversationPendingApproval(conv)` helper: an approval is strictly actionable only if it belongs to the **last message** of the conversation and was emitted by an agent (`lastMsg?.role === 'agent' && lastMsg?.approval?.status === 'pending'`).
  - Applied to `activeResolvers` and `activeConversations`.
- **Dialogue Store Turn Superseding (`src/stores/dialogueStore.ts`):**
  - Expanded `ApprovalRequestData.status` union to include `'superseded'`.
  - In `loadActiveConversation`: when mapping loaded turns, any non-last turn with `approval.status === 'pending'` is automatically updated to `'superseded'`.
  - In `loadOlderMessages`: any historical turns prepended from continuous pagination are marked `'superseded'`.
  - In `sendUserMessage`: before appending a user turn, all prior pending approvals in that conversation are marked `'superseded'` and synced to SQLite via `updateTurnApproval(m.turnId, 'superseded')`.
  - In `AgentResponse`: upon arrival of a new agent message, prior approvals are marked `'superseded'`.
- **UI Approval Card Muting (`FloatingChatOverlay.vue` & `ConversationPanel.vue`):**
  - When `msg.approval.status === 'superseded'`, rendered with muted styling (`opacity-60`, `border-[var(--border-color)]`, `text-zinc-500`) and actionable buttons are hidden.

### B. Automatic Agent Launch on Selection
- **Native `ensureAgentRunning(agentId)` (`src/stores/dialogueStore.ts`):**
  - Guarded with `_launchingAgentIds` Set to ensure strict single-flight execution.
  - Automatically identifies whether the target is the global system agent or a project domain specialist.
  - Inspects active status (`working`, `running`, `idle`, `starting`, `blocked`). If unlaunched, completed, or failed:
    - Automatically invokes `launch_agent` with `cleanContext: false` (seamlessly resuming the existing conversation session).
    - Queries `get_project_agent_state_cmd` and updates `coreStore.agentStates`.
- **Integrated Invocation Points:**
  - `openFloatingChat(agentId)`: triggers `ensureAgentRunning(agentId)` on taskbar tab click or launcher selection.
  - `selectAgent(agentId)`: triggers `ensureAgentRunning(agentId)` when switching agents in workspace mode.
  - `FloatingChatOverlay.vue`: watch handler invokes `launchCurrentAgent()` if opened on an unlaunched agent.

---

## 3. Verification & Compliance

- **Rust Backend:** `cargo check --manifest-path src-tauri/Cargo.toml` completed cleanly with 0 errors.
- **Frontend App:** `npm run build` (`vue-tsc --noEmit && vite build`) passed with 0 errors.
- **Strict Git Ban Compliance:** No git commands executed.
- **Application Lifecycle Boundary:** No dev servers or background daemons restarted.
