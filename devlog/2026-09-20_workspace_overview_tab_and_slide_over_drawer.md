# Devlog: Workspace Overview Tab and Slide-Over Project Drawer

**Date:** 2026-09-20  
**Topic:** UI/UX Architecture Restructuring: Elimination of Redundant Dashboard, Introduction of Overview Workspace Tab, and Slide-Over Projects Drawer

---

## 1. Architectural Motivation

As the Wang-Kapawu interface evolved to incorporate deep active workspace management (file exploration, inline markdown editing, SQLite devlog journaling, active tasks, and split terminals), maintaining two parallel root views (`dashboard` with multi-project vertical cards and `workspace`) became redundant and introduced navigation friction.

All project-level capabilities previously displayed on the dashboard card (Git branch and status indicators, Platform Skill injection, process supervision telemetry, VCS dispatching, Safety Snapshots & diff rollback, and initialization flags resolution) naturally belong as the primary **Overview** tab of the selected project within the unified Workspace shell.

---

## 2. Key Changes Implemented

### 2.1 Unified Workspace Stage & Elimination of Dashboard View
- **`src/App.vue`:**
  - Removed dual-view conditional rendering (`currentView === 'dashboard' | 'workspace'`).
  - `ProjectWorkspace.vue` is now the sole root content viewport.
  - Updated global hotkeys (`Alt+1` -> Overview, `Alt+2` -> Chat, `Alt+3` -> Files & Docs, `Alt+4` -> Devlog & Tasks, `Ctrl+B` -> Toggle Projects Drawer).
  - Cleaned up obsolete imports and removed dead dashboard-specific references.

### 2.2 Dedicated `ProjectOverviewTab.vue`
- Created [`src/components/workspace/ProjectOverviewTab.vue`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/workspace/ProjectOverviewTab.vue) encapsulating:
  - Top Card Header: Git branch, symbolic metrics (`!modified`, `?untracked`, `+staged`, `↑ahead`, `↓behind`, `clean`), platform skill badge & injection modal, remote GitHub modal, supervisor process indicator, VCS voice/text modal trigger, snapshots & diff timeline, and options dropdown.
  - Active initialization flags banner with `AppInitBadge` resolution triggers.
  - Responsive 2-column plugin zone (`project-card-body`) hosting the Agent Orchestrator (`AgentSection.vue`) and Scripts & Services Orchestrator (`ScriptsSection.vue`).
  - Orchestration under construction hazard tape overlay with abort & rollback button.

### 2.3 Slide-Over Projects Drawer & Mouse-Wheel Scrollable Rail
- Enhanced [`src/components/workspace/ProjectVerticalRail.vue`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/workspace/ProjectVerticalRail.vue):
  - **Compact Rail (Leftmost):** Preserves rotated 90° project labels, 2-letter uppercase initials badge, and active bullet indicator. Vertically scrollable via mouse wheel without visible scrollbars (`overflow-y-auto scrollbar-none`).
  - **Slide-Over Drawer (Overlay on Top of Content):** Smoothly overlays from `left-11` on top of workspace content with a subtle backdrop overlay (without displacing or reflowing workspace content).
  - Includes real-time project search filter, project branch & status indicators, quick action buttons (Settings, Snapshots, Delete), and a `+ Track New Project` directory picker button.

### 2.4 State Management Synchronization
- **`src/stores/dialogueStore.ts`:**
  - Updated `activeWorkspaceTab` to `'overview' | 'files' | 'devlog' | 'snapshots' | 'terminal'` with `'overview'` as default.
  - Updated `NavigationTarget` and `setWorkspaceTab` to support `'snapshots'`.

### 2.5 Native Snapshots Workspace Tab & Modal Retirement
- Created [`src/components/workspace/ProjectSnapshotsTab.vue`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/workspace/ProjectSnapshotsTab.vue) integrating:
  - Timeline stream panel (`w-80`) with milestone, deploy, pre/post, and manual safety snapshots.
  - Manual safety snapshot trigger (`SNAP` with custom label).
  - Touched files checklist with individual file checkboxes, Select All / Deselect All, and surgical recovery (`RESTORE SELECTED`).
  - Full-width live unified `git diff` viewer comparing snapshot state against working tree/HEAD.
  - Full repository rollback action (`FULL ROLLBACK`).
- Retired `SnapshotTimelineModal.vue` modal overlay in accordance with the Zero Dead Code Policy.
- Wired all snapshot triggers from CardHeader, 3-dots project options, and projects drawer directly to the Snapshots tab.

---

## 3. Verification & Build
- `vue-tsc --noEmit` verified with 0 errors.
- `npm run build` bundled successfully with Vite in 11.0s.

