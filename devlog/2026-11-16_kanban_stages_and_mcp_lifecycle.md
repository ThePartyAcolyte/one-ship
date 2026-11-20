# Devlog: Customizable Kanban Stages, Pointer Drag-Reordering & Full Project Management MCP Lifecycle (2026-11-16)

## Summary
Implemented **Spec 34 Phase 3** (per-project customizable Kanban stages, collapsible columns, per-stage modal task creation, real-time `PointerEvent` column reordering, and removal of redundant top controls) and **full-lifecycle MCP tooling** so ONE-SHIP agents can manage stages, tasks, subtasks, and `Journal` evidence links end-to-end. Also aligned remaining `Spec 33 -> Spec 34` code annotations and refined `scripts/run_dev.sh` post-build GUI reload detection.

## Changes Implemented

### 1. Spec 34 Phase 3 — Per-Project Customizable Kanban Stages
- **Database (`src-tauri/src/db.rs`):**
  - Added `project_task_stages` table (`id`, `project_id`, `slug`, `label`, `sort_order`, `is_collapsed`, `is_terminal`, timestamps) and `idx_project_task_stages_proj_order`.
  - Added `ProjectTaskStageRecord` (`ts-rs`), lazy default stage seeding (`ensure_project_stages_on`), transactional stage CRUD (`db_list_project_stages`, `db_insert_project_stage`, `db_update_project_stage`, `db_reorder_project_stages`, `db_delete_project_stage` guarding last-stage deletion and requiring reassignment for non-empty stages), and ownership lookups (`db_get_project_stage`, `db_get_task_subtask_project_id`).
  - Updated `db_insert_project_task` and `db_update_project_task` to derive `completed_at_seconds` from each project's `stage.is_terminal` flag instead of hardcoded `"done"`.
- **Domain & Daemon RPCs (`src-tauri/src/kopeng/project_mgmt.rs`, `src-tauri/src/api.rs`, `src/types/daemonApi.ts`):**
  - Deleted static `VALID_TASK_STATUSES`; replaced status checks with per-project `db_project_stage_exists` validation.
  - Registered and mirrored `list_project_stages`, `create_project_stage`, `update_project_stage`, `reorder_project_stages`, and `delete_project_stage` (all broadcasting `project-tasks-updated` SSE events).
- **Frontend Stores & Modularity (`src/stores/projectMgmtStore.ts`, `src/stores/githubCollabStore.ts`):**
  - Split GitHub collaboration state/actions into dedicated `useGithubCollabStore` ([githubCollabStore.ts](file:///home/butcherwutcher/Projects/dev/one-ship/src/stores/githubCollabStore.ts)) and updated [GitBranchModal.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/modals/GitBranchModal.vue), [ProjectOverviewTab.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectOverviewTab.vue), and [GithubCollabPanel.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/GithubCollabPanel.vue).
  - Extended `useProjectMgmtStore` with dynamic stage CRUD, `fetchProjectBoard`, and `ProjectTaskStatus = string`.
- **Board UX (`src/components/workspace/ProjectManagementTab.vue`, `src/components/modals/CreateProjectTaskModal.vue`, `src/components/workspace/ProjectTaskDetailPanel.vue`):**
  - Replaced top global `+` task drawer and manual `Refresh` button with per-column `+` triggers opening [CreateProjectTaskModal.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/modals/CreateProjectTaskModal.vue).
  - Added collapsible column strips (`is_collapsed`) preserving task drag-drop zones, inline stage configuration (rename, `is_terminal` toggle, delete with destination reassignment), `+ Add Stage` builder, and real-time `PointerEvent` column reordering (`handleStagePointerDown` / `handleStagePointerMove` / `handleStagePointerUp`) matching [ProjectWorkspace.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectWorkspace.vue) tab reordering.
  - Updated `ProjectTaskDetailPanel.vue` and Timeline badges to resolve labels and terminal flags dynamically from `store.stages`.

### 2. Full Agent MCP Lifecycle & Skill Synchronization
- **MCP Tools (`src-tauri/src/mcp.rs`):**
  - Updated `query_project_tasks` to validate optional `status` dynamically against `db_project_stage_exists` without a static schema `enum`.
  - Extended `link_journal_to_task` with `action: "link" | "unlink"`.
  - Added `manage_project_stage` (`action: "list" | "create" | "update" | "reorder" | "delete"`), `manage_project_task` (`action: "create" | "update" | "move" | "delete"`), and `manage_task_subtask` (`action: "create" | "toggle" | "delete"`), all enforcing strict project ownership checks and broadcasting `project-tasks-updated`.
- **Platform Skill & Specs:**
  - Updated [templates/skills/oneship-project-mgmt/SKILL.md](file:///home/butcherwutcher/Projects/dev/one-ship/templates/skills/oneship-project-mgmt/SKILL.md) with full-lifecycle stage/task/subtask/evidence workflows.
  - Synchronized [docs/specs/34_project_management_and_oss_collaboration.md](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/34_project_management_and_oss_collaboration.md) (v0.2.3, including §2.4 `project_task_stages` and §5 Phase 3), [TODO.md](file:///home/butcherwutcher/Projects/dev/one-ship/TODO.md), and remaining `Spec 33 -> Spec 34` code comments.

### 3. Dev Launcher Post-Build GUI Reload
- **[scripts/run_dev.sh](file:///home/butcherwutcher/Projects/dev/one-ship/scripts/run_dev.sh):** Moved `gui_was_active` check after `build_daemon` inside `rebuild-daemon` so opening the interface while a Rust build is in progress still reloads `oneship-dev-gui` once the daemon restarts.

## Verification & Observations (`./scripts/run_dev.sh status`)
- **Frontend HMR & Shell Syntax:** `bash -n scripts/run_dev.sh` exited `0`; `oneship-dev-vite` applied HMR cleanly on all modified Vue components.
- **Daemon Build Note:** Per `AGENTS.md` §1.6, manual `cargo` commands were not invoked. Read-only diagnostics (`./scripts/run_dev.sh status`) showed `oneship-dev-daemon-build` failed on unrelated concurrent edits in `src/kopeng/snapshots.rs` (`lines 574, 579`: `.unwrap_or(...)` on `bool`/`u64`), keeping the prior `oneship-daemon` (`PID 362478`) running until `snapshots.rs` compiles cleanly.
