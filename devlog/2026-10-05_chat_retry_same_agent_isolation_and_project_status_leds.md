# Development Log: Chat Retry, Same-Agent Project Isolation, and Minimalist Status LEDs

**Date:** 2026-10-05  
**Topic:** Direct prompt retry on chat error card, same-name agent project isolation across workspace transitions, and minimalist LED status indicators in the vertical project rail and drawer.

---

## 1. Problem Statement & Root Cause Analysis

### A. Missing Direct Retry Option in Chat Error Card
* **Symptom:** When prompt dispatch or agent execution encountered an error (such as network disconnection or transient quota exhaustion), the error banner in `FloatingChatOverlay.vue` only displayed buttons to switch accounts, switch to a Flash model, or restart the agent.
* **Root Cause:** While `dialogueStore.pendingRetryPrompts` held the failed prompt text, there was no single-click "Retry Message" action on the card to immediately re-dispatch the prompt to the supervisor.

### B. Chat Window Leakage & Same-Name Agent Collision (`bossman`)
* **Symptom:** When projects shared identical agent IDs (e.g. `bossman` existing in both Project A and Project B), switching active projects kept the chat overlay open pointing to the previous project's conversation, instead of isolating chats or loading the active project's agent turns.
* **Root Cause:**
  1. `dialogueStore.resolveAgentProjectPath(agentId)` prioritized `this.agentProjectMap[lower]` over `coreStore.activeProject`. Because `agentProjectMap` was a global flat map (`agentId -> projectPath`), once `bossman` was opened in Project A, any query for `bossman` permanently returned Project A's directory.
  2. `isAgentBelongsToActiveProject(agentId)` in `AgentTaskbar.vue` only verified string name matching against `st.agents`. Because Project B also possessed an agent named `bossman`, it evaluated to `true` for a chat originally opened in Project A.
  3. `dialogueStore` lacked explicit tracking of `floatingChatProjectPath`, causing workspace context switches to restore `floatingChatAgentId` without reloading the continuous chat turns for the incoming project.

### C. Missing Visual Status Indicators on the Left Rail
* **Symptom:** The left project vertical rail (`ProjectVerticalRail.vue`) and drawer cards displayed only project initials and static text, lacking quick glanceability into background operations (running background services, agents awaiting human approvals, unread responses, or active execution turns).

---

## 2. Architecture & Implementation Details

### A. Direct Prompt Retry Engine (`dialogueStore.ts` & `FloatingChatOverlay.vue`)
* **Store Action:** Added `retryLastAgentPrompt(agentId: string, explicitProjectPath?: string)` and `getLastUserPrompt(agentId: string)` in `src/stores/dialogueStore.ts`.
  * Extracts the failed prompt from `pendingRetryPrompts[agentId]` or the last user turn.
  * Clears the active agent error.
  * Re-dispatches to the native supervisor via `invoke('prompt_agent', { projectPath, target, prompt })`. If the agent process stopped or failed, the Rust supervisor automatically restarts it before injecting the prompt into `stdin`.
* **Chat Overlay Recovery UI:** Added `Retry Message` button with `RotateCcw` icon in `FloatingChatOverlay.vue`.

### B. Robust Cross-Project Agent Isolation (`dialogueStore.ts`, `AgentTaskbar.vue`, `FloatingChatOverlay.vue`)
* **Canonical Path Tracking:**
  * Added `floatingChatProjectPath: string | null` to `ProjectUiWorkspaceState` and `dialogueStore` state.
  * Recorded in `openFloatingChat` and cleared in `closeFloatingChat`.
* **Precedence Inversion in `resolveAgentProjectPath`:**
  * If `floatingChatAgentId === agentId` and `floatingChatProjectPath` is set, prioritizes `floatingChatProjectPath`.
  * If `coreStore.activeProject` contains the agent, it strictly returns `coreStore.activeProject.path`, overriding any stale global map.
* **Workspace Transition Synchronization:**
  * In `switchProjectContext(fromProjectId, toProjectId, toProject)`:
    * Validates whether `saved.floatingChatAgentId` actually belongs to `toProjectId`.
    * Restores `floatingChatProjectPath` bound strictly to `toProject.path`.
    * Automatically triggers `loadActiveConversation(targetPath, restoreAgentId)` to fetch the correct project turns from SQLite.
* **Taskbar Project Guard:**
  * In `AgentTaskbar.vue`, updated `isAgentBelongsToActiveProject` and active tab candidate filtering to verify that `dialogueStore.floatingChatProjectPath === activeProject.value?.path`.
* **Active Project Watcher:**
  * In `FloatingChatOverlay.vue`, bound `activeProjectPath` to `floatingChatProjectPath` and added a watcher that reloads the active conversation whenever the project context transitions.

### C. Minimalist Hardware-Rack LED Status Indicators (`ProjectStatusLeds.vue`)
* **Reusable Component:** Created `src/components/workspace/ProjectStatusLeds.vue`:
  * Supports horizontal and vertical orientations with brutalist micro-sockets (`w-1.5 h-1.5 rounded-full`).
  * **LED 1 (Agent / Chat):**
    * *Amber pulse*: Agent requires approval (`! APPROVAL`).
    * *Rose pulse*: Agent execution error (`ERROR`).
    * *Emerald ping*: Agent actively running turn (`WORKING`).
    * *Cyan solid*: Unread agent messages (`UNREAD`).
  * **LED 2 (Terminal / Services):**
    * *Emerald glow*: Background service or task running (`RUNNING`).
  * Full hover diagnostics via `<AppTooltip>`.
* **Integrated Touchpoints:**
  * Compact vertical rail strip (`ProjectVerticalRail.vue`): Rendered vertically between initials badge and rotated name.
  * Slide-over drawer (`ProjectVerticalRail.vue`): Rendered horizontally alongside the project title.
  * Project overview card header (`CardHeader.vue`): Rendered next to the project header.

### D. Compact Execution Config Modal & Tooltip Remediation (`AppExecutionConfigPicker.vue`, `FloatingChatOverlay.vue`)
* **Problem:** The three separate selects for Runner, Model & Effort, and Account consumed over 400px of width in `FloatingChatOverlay.vue`, colliding with lifecycle buttons (`RotateCcw`, `Brain`, `ArrowRightLeft`) in docked mode. Furthermore, button tooltips clipped against the upper header boundaries because of default upward positioning.
* **Component Architecture (`AppExecutionConfigPicker.vue`):**
  * Created a consolidated trigger button rendering the concatenated state string (`[Runner] / [Model] / [Account]`) in a single line with `truncate`.
  * **In-Message Viewport Modal:** Rather than an unconstrained screen modal or a floating dropdown popover, the configuration wizard teleports into `#chat-messages-modal-mount` (`<div class="relative flex-1 flex flex-col min-h-0 overflow-hidden">`), which encloses exclusively the chat messages and terminal viewport.
  * **Blurred Backdrop Overlay:** Rendered with `absolute inset-0 z-30 bg-black/75 backdrop-blur-sm flex items-center justify-center p-3`, obscuring and blurring the message area while leaving the window header and input tray fully interactive and visible.
  * **Stepped 3-Phase Progression:**
    1. *Step 1: Runner* (Select agent runtime supervisor).
    2. *Step 2: Model & Reasoning Effort* (Select foundational model and reasoning effort level).
    3. *Step 3: Account* (Select OAuth account / profile).
  * **Instant Transition & No Unnecessary Controls:** Selecting any item immediately progresses to the next step without requiring Next/Back buttons. Completing Step 3 applies the configuration and closes the modal.
  * **Dismissal:** Clicking anywhere on the blurred backdrop (`@click.self`) or pressing `Escape` cancels and dismisses the modal without applying incomplete choices.
* **Tooltip Remediation:**
  * Assigned explicit `position="bottom"` to the three lifecycle action buttons in `FloatingChatOverlay.vue` (`Restart Agent Session`, `Review / Refresh Memories`, `Toggle Compact History`), ensuring tooltips render cleanly downwards into the chat area without header clipping.

---

## 3. Verification & Validation

1. **TypeScript TypeCheck (`npx vue-tsc --noEmit`):**
   * Passed with 0 errors.
2. **Production Vite Build (`npm run build`):**
   * Built cleanly in 4.36s with zero compilation issues.
3. **Rust Backend Check (`cargo check`):**
   * Passed with 0 errors in 0.60s.

