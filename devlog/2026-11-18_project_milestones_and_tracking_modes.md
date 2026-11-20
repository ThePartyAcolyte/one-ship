# Devlog: Strategic Milestones, Tracking Modes & Milestone Inspector (2026-11-18)

## Summary
Implemented Phase 4 of **Spec 34** (`project_milestones`), decoupling project storage from progress calculation strategies (`linear`, `shape_up`, `weighted`) and connecting milestones transversally across `Project`, `Journal` (`agent_session_journal`, `project_todos`, `project_technical_debt`), `Gantt`, `Overview`, and `Greenfield` onboarding.

## Changes
- **Specs:**
  - `docs/specs/34_project_management_and_oss_collaboration.md` (`v0.2.5`): Added `project_milestones` schema, `tracking_mode` strategies, Drift Compass metrics, `project/milestones` surface node, and `MilestoneDetailPanel` aggregated history specification.
  - `docs/specs/10_configuration_schema_and_cascade.md`, `22_session_lifecycle_and_devlog_graph.md`, `26_greenfield_project_creation_and_conversational_bootstrapping.md`, `37_project_settings_tab_and_overview_dashboard.md`: Synchronized `tracking_mode`, `milestone_id` attribution, greenfield nature selection, and overview summary.
- **Backend (`src-tauri/`):**
  - `db.rs`: Added `project_milestones` table, additive `milestone_id`/`weight_points` columns on `project_tasks`, `milestone_id` on `agent_session_journal` and `project_technical_debt`, CRUD helpers, `db_get_milestone_linked_records` (direct + transitive journal minutes, task/subtask-linked TODOs, and tech debt), and `db_sum_milestone_tokens` (token consumption scaffolding).
  - `kopeng/project_mgmt.rs`: Implemented `compute_milestone_summaries` (normalized `effective_weight_percent`, `schedule_drift_days`, `uphill_risk_flag`, tech debt, todo counts, and `tokens_used_count` / `total_tokens_used_count`) and daemon commands (`list_project_milestones`, `get_project_milestone_detail`, `create_project_milestone`, `update_project_milestone`, `delete_project_milestone`).
  - `api.rs`, `events.rs`, `ui_state.rs`, `config.rs`, `overview.rs`, `kopeng/initializer.rs`, `mcp.rs`, `templates/skills/oneship-project-mgmt/SKILL.md`, `templates/prompts/greenfield_initializer.md`.
- **Frontend (`src/`):**
  - `src/stores/milestoneStore.ts` + `src/stores/projectMgmtStore.ts`.
  - `src/components/workspace/ProjectMilestonesView.vue` (dedicated `Milestones` subtab before `Board`, with Shape Up SVG Hill Chart, Weighted normalized allocation bar, and project token counter).
  - `src/components/workspace/MilestoneDetailPanel.vue` (aggregated Tasks, direct/transitive Minutes with `reopenConversation`, TODOs, Tech Debt, and milestone token counter).
  - `src/components/modals/CreateProjectMilestoneModal.vue`, `CreateProjectTaskModal.vue`, `ProjectTaskDetailPanel.vue`, `ProjectManagementTab.vue` (milestone badges on Board/Schedule cards, optional Gantt `By Milestone` phase header rows & baseline bars), `OverviewWidgetGrid.vue`, and `AddProjectModal.vue`.

## Verification
- `./scripts/run_dev.sh export-bindings`: **395 passed, 0 failed**.
- Spec 13 §11.3 grep audit on new/modified Vue components: zero hardcoded palette colors, zero native `<select>` or `title="..."` attributes.
