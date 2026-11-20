# 2026-11-18 — Nucleus 4: Plugin-Driven Overview Dashboard & 4-Column Widget Grid (Spec 37)

## Summary
Installed `grid-layout-plus` and `vue-draggable-plus`, implemented the aggregated `get_project_overview_summary` + `save_project_overview_layout` daemon commands, and built the 4-column draggable/resizable widget dashboard in `ProjectOverviewTab.vue`.

## Changes
1. **Frontend Dependencies (`package.json`):**
   - Installed `grid-layout-plus` (2D 4-column drag/resize grid) and `vue-draggable-plus` (for 1D/Kanban/tab reordering).
2. **Backend Overview Aggregation & Persistence (`src-tauri/src/overview.rs`, `db.rs`, `api.rs`, `lib.rs`):**
   - Added `projects.overview_layout_json` column migration and `db_get_project_overview_layout` / `db_save_project_overview_layout` in `db.rs`.
   - Added `overview.rs` with `#[ts(export)]` DTOs (`ProjectOverviewSummary`, `OverviewWidgetLayoutItem`, `SaveProjectOverviewLayoutArgs`) and handlers aggregating Project Management, Forge Inbox, Semantic Knowledge, Devlog/Journal, Profiler, Designer, CAD, and PCB metrics.
   - Registered `get_project_overview_summary` and `save_project_overview_layout` in `src-tauri/src/api.rs` and `src/types/daemonApi.ts`.
3. **Overview Widget Grid UI (`src/components/workspace/OverviewWidgetGrid.vue`, `ProjectOverviewTab.vue`):**
   - Built a 4-column standardized `GridLayout` (`rowHeight: 116`, widths `1..4`, heights `1..3`, quick presets `1×1`, `2×1`, `2×2`, `4×1`).
   - Added `Customize` mode (visibility toggles, drag handle, resizer, `Reset` layout) and click-through navigation (`navigateToNode`) to each subsystem tab.
   - Reintroduced Semantic Knowledge status + `Index` trigger inside its Overview widget (resolving Technical Debt #40).
4. **Hygiene:**
   - Removed completed Spec 37 item from `TODO.md` and resolved Debt #40 from `TECHNICAL_DEBT.md`.

## Verification
- Running `./scripts/run_dev.sh export-bindings` to generate `ts-rs` bindings and verify `api_commands_match_the_frontend_map`.
