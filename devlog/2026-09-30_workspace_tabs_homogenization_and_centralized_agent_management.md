# Devlog: Workspace Tabs Homogenization and Centralized Agent Management

**Date:** 2026-09-30  
**Topic:** UI/UX navigation streamlining, tab homogenization, overview scripts dedication, and centralized agent management in the taskbar.

---

## 1. Context and Problem Statement

1. **Workspace Tab Clutter & Inconsistent Styling:**
   - The sub-tab navigation header in `ProjectWorkspace.vue` displayed the project directory path on the right side, causing horizontal cramping and overflow on narrow viewports.
   - Tab names were wordy (`Files & Docs`, `Devlog & Tasks`) rather than clean, function-driven terms.
   - The `Visual Builder` tab used an ad-hoc hardcoded emerald color palette (`border-emerald-500`, `text-emerald-400`), violating the semantic theme token rules and visual homogeneity across workspace tabs.
2. **Overview Redundancy & Dead Code:**
   - The project overview tab (`ProjectOverviewTab.vue` and `ProjectCard.vue`) forced a two-column grid (`lg:grid-cols-2`) hosting `AgentSection.vue` alongside `ScriptsSection.vue`.
   - `AgentSection.vue` contained obsolete logic including internal agent displays and an unused `[RESET ALL]` button.
   - Splitting horizontal space squeezed `ScriptsSection.vue`, causing persistent services and deterministic tasks to render in constrained quarters.
3. **Fragmented Agent Management:**
   - Agent launching, persona addition (`+`), editing (`kapawu-edit-persona`), and right-click context options were trapped in the overview card rather than accessible from the universal bottom chat taskbar.

---

## 2. Technical Modifications

### 2.1 Workspace Header Homogenization and Tab Renaming
- **`src/components/workspace/ProjectWorkspace.vue`:**
  - Standardized `Visual Builder` tab styles to use dynamic theme variables (`border-[var(--accent-color)] bg-[var(--bg-color)] text-[var(--accent-color)]` when active; `border-transparent text-[var(--dim-color)] hover:border-[var(--border-color)] hover:text-[var(--fg-color)]` when inactive).
  - Renamed tab `Files & Docs` $\to$ `Explorer`.
  - Renamed tab `Devlog & Tasks` $\to$ `Journal`.
  - Removed the path indicator div (`activeProject?.path`) from the sub-tab bar header.
- **`src/components/workspace/ProjectGitSnapshotsTab.vue`:**
  - Synchronized reader tooltip from `'Open in Files & Docs Reader'` to `'Open in Explorer'`.

### 2.2 Overview Layout Dedication to Scripts
- **`src/kopeng/agents/index.ts` & `src/main.ts`:**
  - De-registered `AgentSection` from `project-card-body`.
  - Removed `AgentKopengPlugin` from `activePlugins` in `src/main.ts`.
  - Decommissioned `src/kopeng/agents/components/AgentSection.vue` per Rule 4 (Zero Dead Code Policy).
- **`src/components/workspace/ProjectOverviewTab.vue` & `src/components/ProjectCard/ProjectCard.vue`:**
  - Expanded `<ZoneOutlet zone-name="project-card-body" />` container from `grid-cols-1 lg:grid-cols-2` to `w-full`, giving `ScriptsSection` full screen width for persistent services and task lists.

### 2.3 Centralized Agent Management in `AgentTaskbar.vue`
- **`src/components/taskbar/AgentTaskbar.vue`:**
  - Integrated `AppAgentPortrait` (size 20px) on every agent item in the launcher popover and mini portrait (size 14px) on conversation tabs in the bottom bar.
  - Added minimalist `+` button in the specialists header dispatching `kapawu-add-persona`.
  - Added `Eye` / `EyeOff` active-only visibility toggle with `localStorage` persistence (`kapawu:visibility:taskbar_domain_agents:${projectId}`).
  - Wired left-click to launch unlaunched agents (`handleLaunchAndOpenAgent`) or open chat for running agents.
  - Implemented right-click `AppContextMenu` for all agents and conversation tabs:
    - Active: `Open Chat`, `Restart Agent`, `Stop Agent`.
    - Unlaunched: `Launch <agentId>`.
    - Persona: `Edit Persona` dispatching `kapawu-edit-persona`.

---

## 3. Verification

1. **Compilation:**
   - Ran `npm run build` (`vue-tsc --noEmit && vite build`): Built clean with exit code 0.
   - Ran `cargo check` in `src-tauri`: Verified zero backend warnings or regressions.
2. **Governance:**
   - Zero git commands executed (strictly observed Git Ban).
   - All modifications performed exclusively through dedicated file tools (`write_to_file`, `replace_file_content`).
