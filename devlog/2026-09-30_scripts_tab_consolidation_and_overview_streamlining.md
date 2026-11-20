# Devlog: Scripts Tab Consolidation & Overview Streamlining

**Date:** 2026-09-30  
**Context:** Following the rebrand from `wang-kapawu` to `one-ship`, streamling workspace UX by consolidating scripts and deterministic tasks into the `Scripts` tab, removing duplicate cards from the overview dashboard, and integrating execution history and context menus.

---

## 1. Architectural Changes & Motivations

1. **Dashboard De-cluttering (Overview Card):**
   - The embedded `ScriptsSection` on the overview dashboard (`ProjectOverviewTab.vue` and `ProjectCard.vue`) duplicated script controls already available in the dedicated tab.
   - Removed the `project-card-body` `<ZoneOutlet />` container and its unused imports from `ProjectOverviewTab.vue` and `ProjectCard.vue`.
   - The Overview tab now focuses purely on top-level metrics, health, and initialization flags without redundant action containers.

2. **Workspace Tab Renaming:**
   - Renamed the sub-tab label from `Terminals` to `Scripts` in `src/components/workspace/ProjectWorkspace.vue`.
   - Internal tab routing key remains `'terminal'` to preserve existing keyboard navigation (`Alt+4`), state bindings, and window events (`switch-to-workspace-terminal`) without breaking changes.

3. **Execution History & Context Menus in `ProjectTerminalGrid.vue`:**
   - Added a dedicated `History` button in the rail topbar next to the native terminal launch button (`[preferredTerminal]`) and shell launcher (`[+ Shell]`), allowing developers to immediately open `TaskReportModal.vue` (`kapawu-open-history`).
   - Attached right-click context menus (`AppContextMenu`) to:
     - **Persistent Services:** Quick actions to `Focus Console`, `Profile Service (15s)`, `Restart Service`, `Stop Service` / `Start Service`, `Configure Options`, and `Open History`.
     - **Deterministic Tasks:** Quick actions to `Run Task`, `View Last Report`, `Configure Flags`, and `Open History`.

4. **Dead Code Elimination (Rule 4 Compliance):**
   - Unregistered `ServicesKopengPlugin` from `src/main.ts`.
   - Cleaned `src/kopeng/services/index.ts`.
   - Decommissioned `src/kopeng/services/components/ScriptsSection.vue` into an empty deprecated stub.

5. **Canonical one-ship Storage Convention:**
   - Standardized domain agent visibility preferences in `src/components/taskbar/AgentTaskbar.vue` to use `oneship:visibility:taskbar_domain_agents:${activeProject.value.id}` with backward-compatible reading.

---

## 2. Modified Files

- `src/components/workspace/ProjectWorkspace.vue`: Updated tab title from `Terminals` to `Scripts`.
- `src/components/workspace/ProjectOverviewTab.vue`: Removed `ZoneOutlet` for `project-card-body` and unused import.
- `src/components/ProjectCard/ProjectCard.vue`: Removed `ZoneOutlet` for `project-card-body` and unused import.
- `src/main.ts`: Removed `ServicesKopengPlugin` registration.
- `src/kopeng/services/index.ts`: De-registered `ScriptsSection`.
- `src/kopeng/services/components/ScriptsSection.vue`: Stubbed per Zero Dead Code policy.
- `src/components/workspace/ProjectTerminalGrid.vue`:
  - Added `History` button in rail header.
  - Added `AppContextMenu` component.
  - Attached `@contextmenu` listeners to persistent services and deterministic task cards.
- `src/components/taskbar/AgentTaskbar.vue`: Updated `localStorage` key to `oneship:` namespace.
- `docs/specs/01_service_orchestrator.md`: Bumped to v2.2.0, documented `Scripts` tab consolidation, `History` modal trigger, and right-click `AppContextMenu`.
- `docs/specs/06_plugin_architecture.md`: Documented decommissioning of `project-card-body` UI zone and `ServicesKopengPlugin`.
- `docs/specs/13_design_system_and_ui_components.md`: Added §3.6 for workspace tab homogenization, concise naming, and path clutter elimination.
- `docs/specs/18_mcp_dialogue_channel_and_chat_overlay.md`: Documented centralized `AgentTaskbar.vue` presence, removal of duplicate internal agents and Reset All button, and Phase 2O completion.

---

## 3. Verification

- Ran `npm run build` (`vue-tsc --noEmit && vite build`): Succeeded with **0 errors** in 21.28s.
- Verified absence of remaining `ZoneOutlet` dead bindings.
- Verified bidirectional synchronization of living specifications (`docs/specs/`).
