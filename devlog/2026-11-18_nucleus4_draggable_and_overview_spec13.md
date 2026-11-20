# 2026-11-18 — Nucleus 4b: Overview Spec 13 Alignment & `vue-draggable-plus` Adoption

## Summary
Aligned `ProjectOverviewTab.vue` and `OverviewWidgetGrid.vue` strictly with Spec 13 (§3.13 full-bleed geometry, single `h-9` toolbar, no duplicated title, no floating outer card shell) and migrated workspace tab reordering and Kanban stage/task drag-and-drop onto `vue-draggable-plus`.

## Changes
1. **Overview Spec 13 Alignment (`ProjectOverviewTab.vue`, `OverviewWidgetGrid.vue`):**
   - Removed `max-w-6xl mx-auto`, `p-4 sm:p-6`, `gap-6`, duplicated `Project Overview` heading, and the redundant `Subsystem Widgets` second toolbar.
   - Unified all header controls into a single `h-9 ui-surface-title` toolbar and rendered `OverviewWidgetGrid` full-bleed inside the `p-3` scroll body.
2. **Workspace Tab Reordering (`src/components/workspace/ProjectWorkspace.vue`):**
   - Replaced manual pointer-event drag state machine (~60 lines) with `<VueDraggable v-model="reorderableTabs">`, preserving the fixed `settings` tab and persisting per-project tab order.
3. **Kanban Stage & Task Drag-and-Drop (`src/components/workspace/ProjectManagementTab.vue`):**
   - Replaced manual stage pointer-dragging and HTML5 task drag-and-drop (~95 lines) with `<VueDraggable v-model="draggableStages" handle=".stage-drag-handle">` for stage columns and `<VueDraggable group="kanban-tasks" @add="...">` for cross-column task moves (`moveTaskStatus`).

## Verification
- Checked `./scripts/run_dev.sh status` (`vite` and `gui` `active (running)`).
