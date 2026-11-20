# 2026-11-17 — Project Configuration Panel: Full-Bleed Layout, Disk Watcher & Unsaved-Change Guard

## Summary

Continued the workspace visual normalization with the project configuration panel (`ProjectConfigPanel.vue`). The panel moved from a floating card inside a padded tab wrapper to a full-bleed master-detail surface with a single top toolbar, no duplicated title/file badge/category header, and no bottom action footer. `rula.toml` now reloads automatically from the existing daemon `projects-updated` event without overwriting dirty edits. A frontend-only navigation guard confirms unsaved changes before leaving the settings tab or switching project, and the collapsed project rail now exposes only the drawer toggle.

## Changes Implemented

### 1. Collapsed Project Rail (`src/components/workspace/ProjectVerticalRail.vue`)

- Removed the `+` **Track New Project** button and the archived-projects visibility toggle from the closed vertical rail. The rail now only contains the drawer toggle (plus the project list).
- Both actions remain in the open drawer: archived filter in the header, `Track New Project` in the footer.
- No behavior or event changes; `archivedCount`, `showArchived`, and the `addProject` emit are still used by the drawer.

### 2. Project Configuration Panel Geometry (`src/components/workspace/ProjectConfigPanel.vue`, `ProjectWorkspace.vue`)

- The `settings` tab wrapper no longer applies `p-4` or an outer scroll container; it is a `flex flex-1 min-h-0 overflow-hidden` host.
- The panel root no longer renders a border/background card. It is `flex flex-col h-full min-h-0 w-full`.
- Removed the title block (`Project Configuration`), the `rula.toml` badge and the manual reload button.
- Removed the bottom `Discard Changes` / `Save to rula.toml` footer.
- Added one top toolbar (`h-9`, `.ui-search-box` style):
  - accent `Search` icon and parameter search;
  - live match counter;
  - `Discard` and `Save` actions (`h-6 px-2.5 text-[9px]`, text labels, no tooltips).
- The category sidebar is full height (`w-56`, `ui-surface-list`) with an internal `flex-1 overflow-y-auto` list; its background continues below short lists. The content column fills the remaining space and scrolls independently.
- Removed the `Categories` heading from the sidebar and the duplicated category name/status from the content pane. Category quick actions remain in a title-less `justify-end` action row.
- Parameter cards use `ui-surface-card`, `p-3`, 11px labels, 10px descriptions and 9px default hints, matching the Settings modal.
- Removed the now-unused indexing-status display (`get_indexing_status` polling) and the `Index Knowledge` quick action from the panel; the semantic indexing entry point is deferred to the Overview dashboard as Technical Debt #40. The semantic category fields and toggle remain in Project Config.
- Project settings entry points (`ProjectCard` / `ProjectOverviewTab` gear actions through `App.vue`) now switch to the `settings` tab instead of forcing `overview` and scrolling a removed `project-configuration` anchor; the rail/drawer already used `navigateToProjectSettings`.

### 3. Automatic `rula.toml` Reload (`ProjectConfigPanel.vue`)

- The daemon project watcher already polls project `rula.toml`/`.git/HEAD` mtimes every `watcher_debounce_ms` (default 1500 ms) and emits `projects-updated` (`src-tauri/src/watcher.rs`). No Rust change was needed.
- The panel subscribes with `onDaemonEvent('projects-updated')`, filters its project id, debounces 350 ms, re-reads `get_scoped_project_config`, and compares the normalized values with the loaded snapshot.
- No local edits: the disk version is applied silently, with a transient `rula.toml reloaded from disk` status bar.
- Local edits present: nothing is overwritten; a warning banner appears:
  `rula.toml changed on disk. Discard loads the disk version; Save overwrites it with the in-memory version.`
- Invalid/unreadable external files keep the current values and show the error banner; the existing `[! CONFIG]` flag remains the resolution path.
- The manual reload button is gone. `Discard` re-reads the file and clears the warning; `Save` writes the in-memory version.

### 4. Unsaved-Change Navigation Guard (frontend only)

- Added `src/utils/navigationGuard.ts`, a module-level guard registry with `registerNavigationGuard` / `runNavigationGuard`.
- `coreStore.setActiveProject` and `workspaceLayoutStore.setWorkspaceTab` consult the guard before changing state (with a `bypassNavigationGuard` option for confirmed navigation).
- `workspaceLayoutStore.navigateTo` and `applyDaemonSnapshot` pre-check the guard before mutating project/tab state; a blocked snapshot is not partially applied.
- `ProjectConfigPanel` computes `isDirty` from a JSON snapshot of the loaded values. While dirty it registers the guard; leaving the tab or switching project opens a blocking modal with:
  - `Save & Continue`
  - `Discard & Continue`
  - `Cancel`
- Saving resumes the pending project/tab navigation after a successful write; a failed save/discard closes the modal and keeps the user in the panel with the error banner.
- Sidebar category toggles persist pending field edits (`saveSettings`) before their daemon toggle/reload, except `semantic`/`snapshots`, whose own toggle path saves the whole form; a subsystem switch therefore cannot silently discard unsaved fields.
- No persistence or daemon changes were introduced; the guard is UI-scoped and unregisters on unmount or when the panel becomes clean.

### 5. Living Documentation

- `docs/specs/13_design_system_and_ui_components.md` bumped to **1.3.0**: added §3.13 **Workspace Panel Contract** (no duplicated titles, single top toolbar, full-bleed geometry, no floating card shell, no redundant category headers, unsaved-change guard).
- `docs/specs/10_configuration_schema_and_cascade.md` bumped to **1.1.0** and §4.1 rewritten for the settings tab, full-bleed layout, watcher auto-reload and navigation guard.
- `docs/specs/37_project_settings_tab_and_overview_dashboard.md` bumped to **0.2.2** and cross-referenced the panel contract.
- `docs/specs/35_ui_state_machine_and_session_persistence.md` bumped to **0.2.2** with the frontend unsaved-change guard invariant.

## Validation Required

No build, test, runtime process or dev server was started by the agent (AGENTS.md §1.6). The self-managed watcher rebuilds the frontend.

1. Rail: with the drawer closed, only the drawer toggle is visible; archived filter and `Track New Project` remain in the open drawer.
2. Entry points: the project card and overview gear actions open the `settings` tab for the selected project (no overview scroll anchor).
3. Panel geometry: the settings tab has no outer padding/border; the top toolbar spans the panel; the category sidebar reaches the bottom even with few categories; the content fills the remaining height.
4. Manual actions: edit a field, verify `Save`/`Discard` enablement, save, reopen the tab, discard local edits, and confirm the search/counter filter both categories and fields.
5. Navigation guard: edit a field and click another tab, press a tab shortcut, click another project in the rail/drawer, and use a panel quick action. Each attempt must show the confirmation modal; `Cancel` stays, `Discard & Continue` reloads and leaves, `Save & Continue` writes and leaves.
6. Disk watcher: with the panel clean, edit `rula.toml` outside the app and confirm the panel reloads with the transient status bar. With a local edit pending, change `rula.toml` and confirm the warning banner appears and the local value is not overwritten. Use `Discard` to take the disk version and `Save` to overwrite it.
7. Regression: project switching from the rail, workspace tab switching, Omnisearch navigation and the voice/surface `navigateTo` path must not corrupt the active project when the guard blocks a switch.

## Pending

- Runtime confirmation of the above (manual UI checks).
- `--accent-rgb` remains the hardcoded shadow fallback (Technical Debt #39).
- Semantic knowledge indexing UI is deferred to the Overview dashboard (Technical Debt #40).
- The blur/scrollbar runtime checks from the two previous devlogs remain open in `TODO.md`.
