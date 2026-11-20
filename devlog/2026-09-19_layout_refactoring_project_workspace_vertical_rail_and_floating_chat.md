# Devlog: Layout Refactoring, Project Workspace, Vertical Rail, and Floating Chat Overlay

**Date:** 2026-09-19  
**Topic:** Architectural decoupling of Agent Dialogue, Project Workspace Scoping, Vertical Rail Navigation, Multi-Terminal Split Grid, and Universal Bottom Taskbar.

---

## 1. Problem Statement & Motivation
Prior to this refactoring, the application coupled the project explorer, document reader, devlog/journal, and terminal output directly inside `AgentDialogueWorkspace` (the chat sidepanel). This architecture created several fundamental pain points:
- Inability to explore a project's files/documentation independently when no chat conversation was open.
- Heavy viewport contention where conversation bubbles consumed 50% of the horizontal screen real estate during long reading or document editing sessions.
- Bottom-left floating system status logs obstructing lower UI controls and chat elements.
- Terminal outputs being tied exclusively to single agent panes rather than multi-process observability across agents, running services, and build scripts.

## 2. Architectural Transformations

### A. Active Project Scoping in Pinia Stores
- **`coreStore.ts`**: Introduced `activeProjectId` with `localStorage` persistence, reactive `activeProject` resolution, and project-scoped terminal key selection (`selectedTerminalKeys`).
- **`dialogueStore.ts`**: Decoupled document fetching (`fetchProjectDocuments`, `fetchSessionJournals`, `saveProjectDocument`) from agent conversation state. Added reactive state for the floating ubiquitous chat overlay (`isFloatingChatOpen`, `floatingChatAgentId`, `isFloatingChatMinimized`).

### B. Project Dashboard Active Card Interaction
- **`ProjectCard.vue` & `CardHeader.vue`**:
  - Clicking on neutral space within a project card sets it as the active workspace (`coreStore.setActiveProject`).
  - Active cards render an accent contour (`border-2 border-[var(--accent-color)] shadow-[0_0_16px_rgba(...)]`) and a `[● ACTIVE WORKSPACE]` badge.
  - Added direct `[Workspace »]` action button for immediate navigation.

### C. Compact Vertical Project Rail
- **`ProjectVerticalRail.vue`**:
  - Positioned on the leftmost edge of `ProjectWorkspace`.
  - Displays tracked projects using rotated natural text (`[writing-mode:vertical-rl] rotate-180`) with two-letter acronym badges and active indicators.
  - Enables instant project switching without leaving the workspace.

### D. Multi-Terminal Split Grid Observability
- **`ProjectTerminalGrid.vue`**:
  - Scoped to the active project, discovering all running agents, supervised services, and platform supervisor streams.
  - Supports multi-selection with dynamic layout adjustment: 1 pane (100%), 2 panes (horizontal split), or up to 4 panes (2x2 grid).
  - Provides per-pane semantic error filtering (`errors_only` via `extract_semantic_errors`), auto-scroll, and buffer copy actions.

### E. Project Workspace (Files, Devlog, Terminals)
- **`ProjectWorkspace.vue`**:
  - Unifies the Vertical Rail, File Explorer, Markdown Reader & In-Place Editor, Devlog/Tasks (`DevlogJournalView`), and `ProjectTerminalGrid`.
  - Features breadcrumb navigation, live diff/conflict resolution, and document attachment to chat.

### F. Global Bottom Agent Taskbar & Floating Chat Overlay
- **`AgentTaskbar.vue`**:
  - Full-width fixed bottom bar (`h-9`) hosting the active project agent launcher popup, live conversation badges with unread counters, pending approval alerts (`[! APPROVAL]`), and live step telemetry.
- **`FloatingChatOverlay.vue`**:
  - Draggable/dockable floating modal accessible from any view.
  - Features quick jump to the agent's terminal stream in the workspace, artifact chips linking directly to the background reader, Wayland image paste, and interactive proposal approval cards.

### G. System Logs Relocation
- Integrated System Logs trigger into `AppHeader.vue`, removing the persistent bottom-left overlay and opening the full search/filter modal on demand.

---

## 3. Verification & Validation
- **TypeScript Compilation:** `npm run build` (`vue-tsc --noEmit && vite build`) passed with zero errors.
- **Backend Cargo Checks:** `cargo check` in `src-tauri` completed cleanly in 0.57s.
