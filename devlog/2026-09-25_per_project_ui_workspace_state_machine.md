# Development Log: Per-Project UI Workspace State Machine & Cross-Project Isolation

**Date:** 2026-09-25  
**Topic:** Per-Project UI Workspace State Machine, Tab & Document State Isolation, Visual Builder Stickiness Guard, and Taskbar Tab Filtering

---

## 1. Problem Diagnosis & Motivation

During multi-project workflows, several state leakages and UI synchronization anomalies occurred when switching active projects:

1. **Cross-Project Chat Leakage:**
   When an agent chat was active in Project A and the developer switched to Project B, the chat window remained open in the foreground over Project B. Furthermore, the taskbar tabs in Project B continued to render Project A's agent chat tab because `activeConversations` accepted the global `floatingChatAgentId` without verifying active project ownership.

2. **Visual Builder Tab & Component Stickiness:**
   If Project A had `workspaceTab = 'visual_builder'` and the developer switched to Project B (which lacks `has_web_builder`), the Visual Builder view remained stuck on screen displaying Project B's context under an incompatible tab, because conditional rendering only evaluated `workspaceTab === 'visual_builder'` without checking project capabilities.

3. **Open Document / File Editor Leakage:**
   When an active file or document was opened in Project A, switching to Project B retained `selectedDocument` pointing to Project A's file path, displaying foreign contents within Project B's file explorer and editor.

4. **Loss of Workspace Continuity:**
   Switching back and forth between projects reset or scrambled developer context instead of naturally resuming each project in the exact state it was left (active tab, opened document, and chat status).

---

## 2. Key Architecture & Implementation Details

### A. Per-Project UI State Machine (`src/stores/dialogueStore.ts`)
- **`ProjectUiWorkspaceState` Contract:**
  Defined a comprehensive state interface capturing:
  - `workspaceTab`: `'overview' | 'visual_builder' | 'files' | 'devlog' | 'git' | 'terminal'`
  - `selectedDocument`: `DocumentContent | null`
  - `activeArtifactId`: `string | null`
  - `explorerScope`: `'docs' | 'codebase'`
  - `explorerPath`: `string`
  - `readerMode`: `'artifact' | 'browse' | 'journal' | 'terminal'`
  - `journalSubTab`: `'devlog' | 'todos' | 'debt'`
  - `isFloatingChatOpen`: `boolean`
  - `floatingChatAgentId`: `string | null`
  - `isFloatingChatMinimized`: `boolean`
  - `activeAgentId`: `string | null`
- **State Map:** Added `projectWorkspaceStates: Record<number, ProjectUiWorkspaceState>` to `dialogueStore`.
- **Atomic Transition Routine (`switchProjectContext`):**
  - **Outgoing Snapshot:** Automatically saves the complete UI state for `fromProjectId`.
  - **Incoming Restoration:** If `toProjectId` has a previous state, validates `workspaceTab` against project capabilities (fallback to `'overview'` if project lacks `has_web_builder` or `has_git`), and restores the exact tabs, document, and chat state (whether open, minimized, or closed). If first entry, initializes clean default state.

### B. Store Coordination (`src/stores/coreStore.ts`, `src/stores/useProjectStore.ts`)
- **Centralized Switch Invocation:**
  Updated `coreStore.setActiveProject` to capture `previousProjectId` and trigger `dialogueStore.switchProjectContext(previousProjectId, projectId, p)`.
- **Deduplication:**
  Cleaned duplicate fetch calls in `useProjectStore.setActiveProject` to uphold the Anti-God-Store and Zero Dead Code principles.

### C. Capability Guards & Dynamic Explorer Path (`src/components/workspace/ProjectWorkspace.vue`)
- **Two-Way Explorer Path:** Bound `explorerPath` to `dialogueStore.explorerPath` as a computed property, preserving directory context across project switches.
- **Strict Tab Capability Guards:**
  - Added `v-else-if="workspaceTab === 'visual_builder' && activeProject?.has_web_builder"`.
  - Added `v-else-if="workspaceTab === 'git' && activeProject?.has_git"`.

### D. Strict Project Isolation in Taskbar (`src/components/taskbar/AgentTaskbar.vue`)
- **Ownership Verification (`isAgentBelongsToActiveProject`):**
  Ensures agents rendered in the bottom taskbar belong strictly to `activeProject` or represent the global `system` architect.
- **Active Conversations Filtering:**
  Precludes foreign agents from appearing in the taskbar tabs.
- **Visual State Reflection:**
  Styled the active chat tab with primary accent highlight only when expanded in the foreground (`!isFloatingChatMinimized`). When minimized, tab renders in clean neutral style, toggling smoothly between foreground and background upon clicking.

---

## 3. Verification & Validation

1. **TypeScript TypeCheck & Production Build (`npm run build`):**
   Executed `vue-tsc --noEmit && vite build`: compiled cleanly with 0 errors in 5.49s.
2. **Tab Transition Invariants:**
   Verified `has_web_builder` and `has_git` guards prevent sticky components on incompatible projects.
3. **Taskbar Isolation:**
   Verified candidate ID resolution discards cross-project foreign agents.
