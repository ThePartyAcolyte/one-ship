# Devlog: Chat Input Horizontal Scroll & Agent Taskbar State Synchronization

**Date:** 2026-11-12  
**Topic:** UI Fixes — Chat Textarea Overflow & Agent Taskbar Unread vs Working State Disconnection

---

## 1. Context & Problem Statement

Two UX issues were identified during interactive agent chat usage:
1. **Chat Textbox Horizontal Scroll**: The text input area in `FloatingChatOverlay` and `ConversationPanel` showed an undesirable horizontal scrollbar under certain conditions (e.g. typing long unbroken words, monospace characters, or window resize) due to flex container sizing and interaction with the audio/mic button overlay padding (`pr-8`).
2. **Agent Taskbar Badge Disconnection**:
   - The badge on the bottom `AgentTaskbar` displayed an unread count for the active agent whose chat conversation was currently open and focused in the UI.
   - The badge did not distinguish between unread messages and the agent's actual operational status ("working" vs "idle"), leading to cognitive disconnect where users saw "unread" badges even while reading live output.

---

## 2. Root Cause Analysis

1. **Textarea Horizontal Overflow**:
   - In `AppTextarea.vue`, `adjustHeight()` dynamically set `el.style.overflowY = 'hidden'` / `'auto'`. In modern browser rendering engines, setting `overflow-y` without an explicit `overflow-x` causes `overflow-x` to compute to `auto`.
   - The input wrapper had `relative flex-1` without `min-w-0`, allowing flex items to calculate minimum content widths that exceed the parent container.
   - Without explicit `overflow-x-hidden` and `break-words whitespace-pre-wrap`, lines could trigger horizontal overflow.

2. **Unread & Active State Desync**:
   - `workspaceLayoutStore.openFloatingChat(agentId, projectPath)` updated layout coordinates and flags, but did not synchronize `dialogueStore.isOpen = true` or reset `activeConversation.unread = 0`.
   - In `dialogueListeners.ts`, the incoming message handler evaluated:
     ```ts
     const isCurrentActive = dialogueStore.activeConversationId === conv.id;
     if (!dialogueStore.isOpen || !isCurrentActive) {
       conv.unread++;
     }
     ```
     Because `dialogueStore.isOpen` remained `false` (only `layoutStore.floatingChat.isOpen` was `true`), incoming messages continuously incremented `conv.unread` despite the chat being open on screen.
   - In `AgentTaskbar.vue`, tab badges only rendered `unread > 0`, showing no tooltip or visual indication of background work (`status === 'working'`) vs unread messages.

---

## 3. Changes Implemented

### Frontend Components & Layout
- **[AppTextarea.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/ui/AppTextarea.vue)**:
  - Enforced `el.style.overflowX = 'hidden'` in `adjustHeight()`.
  - Added `overflow-x-hidden break-words whitespace-pre-wrap` classes to the `<textarea>` element.
- **[FloatingChatOverlay.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/dialogue/FloatingChatOverlay.vue)** & **[ConversationPanel.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/dialogue/ConversationPanel.vue)**:
  - Added `min-w-0` to the relative input container to prevent flex item width blowouts.
  - Added `overflow-x-hidden break-words` to `AppTextarea` instances.

### Dialogue Store & Taskbar State Management
- **[dialogueStore.ts](file:///home/butcherwutcher/Projects/dev/one-ship/src/stores/dialogueStore.ts)**:
  - Added helper `isChatVisible(agentId: string, projectPath: string): boolean` that checks whether chat is open in either the floating overlay (not minimized) or conversation panel, matching the target agent and project.
- **[workspaceLayoutStore.ts](file:///home/butcherwutcher/Projects/dev/one-ship/src/stores/workspaceLayoutStore.ts)**:
  - Synchronized `dialogueStore.isOpen = true` on `openFloatingChat()`, and reset `conv.unread = 0`.
  - Set `dialogueStore.isOpen = false` on `closeFloatingChat()`.
- **[dialogueListeners.ts](file:///home/butcherwutcher/Projects/dev/one-ship/src/utils/dialogueListeners.ts)**:
  - Updated incoming message handlers to query `dialogueStore.isChatVisible(conv.agentId, conv.projectPath)`. If the chat is visible to the user, `conv.unread` is held at `0`.
- **[AgentTaskbar.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/taskbar/AgentTaskbar.vue)**:
  - Suppressed the unread counter badge when `isVisibleInChat(conv)` is true.
  - Added `isAgentProcessWorking(agentId)` helper checking `'working'` status in `coreStore.agentStates`.
  - Wrapped each taskbar tab in `<AppTooltip :text="getTabTooltip(conv)">` providing rich status context (Agent name, current working step or status, and unread count).
  - Maintained brutalist styling and full TypeScript compatibility with `activeStepSummary`.

---

## 4. Verification

- `npx vue-tsc --noEmit` executed with zero errors.
- `npm run build` executed successfully.
