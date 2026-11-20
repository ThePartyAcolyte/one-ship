# Devlog: Visual UX, Layout Refinements & Agent Lifecycle Synchronization

**Date:** 2026-09-24  
**Topic:** Global Minimalist Scrollbars, Resizable Docked Chat Column, Collapsible File Explorer Sidebar, Inverted Layout Toggle Icons, and Instant Agent Launch & Taskbar Synchronization

---

## 1. Objectives & Context

The platform UI required visual and ergonomic polish across multiple ubiquitous interactions:
1. **Global Scrollbars:** Replaces intrusive default scrollbars with ultra-thin (3px), transparent-track brutalist scrollbars styled in the active accent tone (`var(--accent-color)`).
2. **Resizable Docked Chat:** Allows users to freely drag and resize the right docked chat column via a left-side resize splitter, persisting the width into `localStorage` (`kapawu:dialogue:docked_chat_width`).
3. **Collapsible File Explorer Sidebar:** Enables collapsing the left file tree column in the workspace `Files` tab via clean `PanelLeftClose` / `PanelLeft` buttons, expanding the document and artifact reader to full viewport width.
4. **Dock / Float Toggle Inversion:** Inverts the icons on the chat overlay header to accurately reflect the target action (showing `Layers` to float when docked, and `PanelRight` to dock when floating) with neutral action styling.
5. **Agent Taskbar Process-Linked Visibility:** Restricts bottom bar center tabs exclusively to agents with active supervisor processes (`isAgentProcessActive`) or currently open chat sessions. Closing via `X` kills the process and immediately removes the tab.
6. **Instant Launch & Chat from Overview List:** Clicking an agent in the dashboard/card agent section immediately opens the chat overlay, launches the agent if unlaunched, and optimistically sets its status to `starting` so it appears in the bottom bar instantly without awaiting the backend polling tick.

---

## 2. Key Changes & Architecture

### Backend & Store Synchronization
- **`src/stores/dialogueStore.ts`:**
  - Added `dockedChatWidthVw` state initialized with `localStorage` persistence (`16vw` to `60vw`, default `28vw`). Purged legacy pixel-based storage key `kapawu:dialogue:docked_chat_width`.
  - Added `setDockedChatWidthVw(vw: number)` action saving to `localStorage` (`kapawu:dialogue:docked_chat_width_vw`), clamped to `[16vw, 60vw]`.
  - Updated `closeAndKillAgent` to clear `activeAgentId` and optimistically set agent status to `unlaunched` in `coreStore`, ensuring immediate UI feedback before async `stop_agent` finishes.

### App Shell & Global Styles
- **`src/App.vue`:**
  - **Brutalist Minimalist Scrollbars:**
    - Enforced `border-radius: 0px !important` on all pseudo-elements (`::-webkit-scrollbar`, track, thumb, hover).
    - Locked `width: 3px !important; height: 3px !important;` on both idle and `:hover` to eliminate any GTK overlay expansion.
    - Set `border: none !important; outline: none !important; box-shadow: none !important;`.
    - Applied solid semi-transparent accent background: `color-mix(in srgb, var(--accent-color) 40%, transparent)` in idle, brightening to `color-mix(in srgb, var(--accent-color) 75%, transparent)` on hover.
    - Disabled buttons (`::-webkit-scrollbar-button`) and made corners transparent.
    - Synced `.scrollbar-thin` overrides to ensure consistent straight styling across all containers.
  - **Docked Chat Resizer with Native Pointer Capture:**
    - Replaced `@mousedown` with `@pointerdown` and `setPointerCapture(pointerId)` for deterministic mouse tracking in WebKitGTK / Wayland without event drop.
    - Added expanded grab zone (`before:absolute before:-inset-x-2 before:inset-y-0 before:z-50 before:cursor-col-resize`) providing a generous 17px hit target while keeping a 1px visible line.
    - Added global drag shield overlay (`fixed inset-0 z-[9999] cursor-col-resize select-none pointer-events-none`) during resize.
    - Styled docked column using `:style="{ width: `${dialogueStore.dockedChatWidthVw || 28}vw`, minWidth: '280px', maxWidth: '60vw' }"`.
    - Preserved double-click reset (`@dblclick="dialogueStore.setDockedChatWidthVw(28)"`).

### Workspace & Explorer Ergonomics
- **`src/components/workspace/ProjectWorkspace.vue`:**
  - Added `isExplorerCollapsed` ref with `localStorage` persistence.
  - Added `PanelLeftClose` button in the explorer toolbar header.
  - Added `PanelLeft` expand button in the Document Reader breadcrumb header.

### Floating & Docked Chat Overlay
- **`src/components/dialogue/FloatingChatOverlay.vue`:**
  - Updated dock/float toggle button to show `<Layers />` when docked (tooltip: "Float as Window") and `<PanelRight />` when floating (tooltip: "Dock as Right Column").
  - Removed persistent accent background, standardizing button to neutral tool styling.
  - Enforced `min-w-0 truncate` on header title and `shrink-0` on action buttons to prevent control occlusion at narrow widths.

### Agent Taskbar & Lifecycle
- **`src/components/taskbar/AgentTaskbar.vue`:**
  - Implemented `isAgentProcessActive(agentId)` evaluating live supervisor statuses (`working`, `running`, `idle`, `starting`, `blocked`).
  - Restricted `activeConversations` exclusively to live processes or currently open chat windows.
  - Enhanced `handleCloseTab` to close the chat, clear active agent, optimistically mark status as `unlaunched`, and invoke `stop_agent`.

### Overview / Card Launch Integration
- **`src/kopeng/agents/components/AgentSection.vue`:**
  - Implemented `handleLaunchAndOpenAgent` and unified `handleAgentClick` so clicking any agent in the list immediately opens the chat overlay, launches the process if not running, and sets status to `starting` optimistically.
  - Updated context menu "Launch" action to follow the same unified immediate flow.

---

## 3. Verification

- Executed `npm run build` (`vue-tsc --noEmit && vite build`), compiling all modules and Vue templates with zero type or syntax errors.
- Verified that resizing with native pointer capture tracks drag smoothly across the entire viewport.
- Verified scrollbar styles enforce flat straight edges (`border-radius: 0`), zero hover widening, and transparent borders.
- Verified absence of git commands and verified dedicated tool editing compliance.
