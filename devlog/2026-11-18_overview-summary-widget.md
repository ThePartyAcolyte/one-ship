# 2026-11-18 — Overview Summary Widget & Editable Project Title

## Summary
Refactored the Project Overview dashboard (`ProjectOverviewTab.vue` + `OverviewWidgetGrid.vue`) to remove redundant chrome, align widths, and make the project summary card an adjustable grid widget with inline-editable title and favorite toggle.

## Changes
- **Backend (`src-tauri/src/db.rs`, `src-tauri/src/registry.rs`, `src-tauri/src/api.rs`):**
  - Added `update_project_name` daemon command (`UpdateProjectNameArgs`, `RegistryError::EmptyName`, `db_update_project_name`) to update `projects.name` safely while keeping `id` and `path` immutable.
  - Exported TypeScript bindings via `./scripts/run_dev.sh export-bindings` (`src/types/generated/UpdateProjectNameArgs.ts`) and registered `update_project_name` in `src/types/daemonApi.ts`.
- **Frontend (`OverviewSummaryWidget.vue`, `OverviewWidgetGrid.vue`, `ProjectOverviewTab.vue`, `ProjectWorkspace.vue`):**
  - Removed the `h-9` top toolbar from `ProjectOverviewTab.vue` (favorite, archive, path badge, git/upstream indicators, top Customize button).
  - Created `OverviewSummaryWidget.vue` and integrated it as a pinned (`project_summary`, default `2×2`, non-hideable) widget inside `OverviewWidgetGrid.vue`, eliminating the 8px horizontal margin mismatch with `GridLayout`.
  - Removed the redundant right-side VCS Action, Snapshots, and 3-dots Options menu from the summary card; moved `Customize` / `Done` into the `project_summary` widget header and `Reset` into the edit strip.
  - Placed the Favorite Star button (`Star`) to the left of the project title and added inline editing for `project.name` alongside `project.description`.
  - Aligned the `INITIALIZATION FLAGS` banner (`mx-[8px]`) above `GridLayout` so it never clips inside a compact widget.
- **Documentation (`docs/specs/37_project_settings_tab_and_overview_dashboard.md`):**
  - Updated Spec 37 §3 and §4 to reflect `update_project_name` and the `project_summary` widget.

## Verification
- `./scripts/run_dev.sh export-bindings` passed (395 tests ok).
- Targeted Spec 13 §11.3 grep audit on modified files: clean.
