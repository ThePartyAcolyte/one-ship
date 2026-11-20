# Devlog: Contextual REA Visibility & In-Workspace Project Configuration Hub

**Date:** 2026-11-09  
**Topic:** Contextual RE Lab Tab Rendering, Overview Capabilities Dashboard & Native `rula.toml` Settings Hub  
**Author:** Antigravity (Pair Programming with Developer)

---

## 1. Context & Motivation

1. **Contextual RE Lab Visibility:**
   The Reverse Engineering Lab (`RE Lab`) tab button in `ProjectWorkspace.vue` was previously rendered unconditionally for all projects due to `v-if="activeProject?.has_rea_enabled"` being placed strictly on the inner status dot rather than the button element itself. This led to non-functional UI clutter on standard repositories without reverse-engineering workflows.
2. **Project Overview Ergonomics:**
   `ProjectOverviewTab.vue` previously offered minimal utility, presenting only the project header and initialization flags. Meanwhile, project configuration (`rula.toml`) was trapped inside a floating modal (`SettingsModal.vue`), obscuring the workspace and requiring modal navigation for simple configuration edits.

---

## 2. Changes Implemented

1. **Contextual Tab Visibility & Safe Auto-Fallback (`ProjectWorkspace.vue`):**
   - Transferred `v-if="activeProject?.has_rea_enabled"` directly to the tab button element.
   - Introduced a watcher observing `[activeProject.id, activeProject.has_rea_enabled]`: if the active tab is `'rea'` and the selected project does not have REA enabled, the workspace automatically falls back to `'overview'`.

2. **6-Subsystem Feature Matrix (`ProjectOverviewTab.vue`):**
   - Expanded the capabilities section into a complete 6-subsystem dashboard with direct feature toggles and live status badges:
     1. **Reverse Engineering Lab (REA):** Direct toggle backed by `toggle_project_rea_plugin`.
     2. **Visual Web Editor & Canvas:** Direct toggle backed by daemon command `toggle_project_web_builder`, updating `has_web_builder` in SQLite.
     3. **Continuous Profiling:** Direct toggle backed by daemon command `toggle_project_profiler_plugin`, updating `[plugins.profiler] enabled` in `rula.toml`.
     4. **Git Version Control & Snapshots:** Status badge indicating active branch/VCS tracking and actions for viewing diffs/snapshots or initializing git.
     5. **Platform Skills Catalog:** Live badge reflecting synchronized/outdated/uninstalled status with direct navigation and sync action.
     6. **Task & Service Supervisor:** Script count badge with navigation to inspect scripts and port allocations.
   - Unified watcher in `ProjectWorkspace.vue` observing `[id, has_rea_enabled, has_web_builder, has_profiler_enabled, has_git]` to guarantee automatic fallback to `'overview'` whenever an active conditional tab is disabled or missing in the active project.


3. **In-Workspace Project Configuration Hub (`ProjectConfigPanel.vue`):**
   - Modularized the master-detail configuration view into `src/components/workspace/ProjectConfigPanel.vue` and embedded it directly inside `ProjectOverviewTab.vue`.
   - Category sidebar on the left displays all plugins declaring project or cascade scope fields (`orchestrator`, `vcs`, `snapshots`, `rea`, `supervisor`, `telemetry`, `profiler`, `mcp`).
   - Integrated live parameter search, provenance badges (`[rula.toml]` vs `[default]`), runner catalog pair validation (`pairProblems`), and persistent saving via `save_project_rula_config`.

4. **Settings Navigation & Modal Scope Decoupling:**
   - Navigating to project settings (via `CardHeader.vue` or `ProjectVerticalRail.vue`) now activates the project and smoothly scrolls to `#project-configuration` in the Overview tab without opening a modal.
   - `SettingsModal.vue` is preserved strictly for Global Settings (`config.toml`), triggered from the main desktop header (`AppHeader.vue`).

5. **Visual Harmonization & Minimalist Toggles (`ProjectOverviewTab.vue`, `AppToggle.vue`):**
   - **Canonical Header Bars:** Standardized the top bar of the Project Header card and the Features & Subsystems card with the darker theme background (`bg-black/20 px-4 py-3 border-b border-[var(--border-color)]`), green accent title (`text-[var(--accent-color)]`), semantic icons (`FolderKanban`, `Layers`), and contextual badges.
   - **Inverted Subsystem Palette:** Refactored the 6 subsystem cards from sunken `bg-black/30` to elevated surface styling (`bg-[var(--surface-color)]/50 hover:bg-[var(--surface-color)]/70 hover:border-[var(--border-color)]/80 transition-all`), providing visual contrast against the translucent container.
   - **Minimalist Boolean Switches:** Updated `AppToggle.vue` so that when no label or description is passed, it renders directly as a clean switch button without an outer container box or padding. Removed redundant `"ON"`/`"OFF"` labels in `ProjectOverviewTab.vue`.

6. **Project Favorites (Starred Projects):**
   - **Database & Storage:** Added `is_favorite` (`INTEGER NOT NULL DEFAULT 0`) column to SQLite `projects` table via `ensure_column`.
   - **Backend RPC & Types:** Implemented `db_toggle_project_favorite` and exposed daemon command `toggle_project_favorite` with strongly typed `ToggleProjectFavoriteArgs` derived for TypeScript (`#[ts(export)]`).
   - **UI Integration & Star Sorting:**
     - Added minimal star selector button (`Star` icon) in the Project Header of `ProjectOverviewTab.vue`.
     - In `ProjectVerticalRail.vue`, added a micro amber star indicator on project items in the vertical rail, interactive star toggle in the slide-over drawer, and sorted project list with `is_favorite DESC, name ASC` so favorited repositories bubble to the top.

7. **Subsystem Sub-Tab Visibility Controls:**
   - **Database & Storage:** Added `hidden_tabs` (`TEXT NOT NULL DEFAULT '[]'`) column to SQLite `projects` table via `ensure_column`, storing hidden tab IDs.
   - **Backend RPC & Types:** Implemented `db_toggle_project_tab_visibility` and registered daemon command `toggle_project_tab_visibility` with `ToggleProjectTabVisibilityArgs`.
   - **Overview Dashboard Controls:** Added minimalist `AppToggle` switches to the Git, Skills, and Supervisor cards in `ProjectOverviewTab.vue`. When a tab is hidden, a `Tab Hidden` warning badge renders in place of normal status.
   - **Workspace Navigation & Auto-Fallback:** Updated `ProjectWorkspace.vue` tab headers to condition rendering on `!activeProject?.hidden_tabs?.includes(tabId)`. Extended the central workspace watcher to monitor `hidden_tabs`, immediately falling back to `'overview'` if the active tab is hidden.

8. **Bidirectional Spec Updates:**
   - Synchronized `docs/specs/26_rea_reverse_engineering_integration.md` (contextual RE Lab tab visibility and Overview capability toggle).
   - Synchronized `docs/specs/10_configuration_schema_and_cascade.md` (§4.1 In-Workspace Project Configuration Hub, subsystem visibility toggles, and favorite hierarchy).
   - Synchronized `docs/specs/12_streamable_http_mcp_daemon.md` (registered `toggle_project_favorite` and `toggle_project_tab_visibility` daemon commands).

---

## 3. Verification & Validation

- `cargo test --manifest-path src-tauri/Cargo.toml --lib`: 358/358 tests passed (0 failures, including `api_commands_match_the_frontend_map`).
- `npx vue-tsc --noEmit`: 0 errors.
- `npm run build`: Vite production bundle succeeded cleanly in 3.96s.
