# Project Settings and Overview Dashboard

## Purpose

Separates per-project configuration (`ProjectConfigPanel.vue`) from the Project Overview (`ProjectOverviewTab.vue`), transforming `overview` into a live, plugin-driven summary dashboard while giving per-project settings a permanent, non-hideable workspace tab (`settings`) and adding inline-editable project title and description metadata.

## Scope

In scope: the fixed `settings` tab and its surface-registry actions, editable `projects.name` and `projects.description` metadata, the 4-column `OverviewWidgetGrid.vue` dashboard, widget capability gates, and overview layout persistence.

Out of scope: global/project configuration schema cascade ([Configuration Schema and Cascade](../01-architecture/02-configuration-schema-and-cascade.md)), generic panel chrome rules ([Workspace Chrome and Panel Contracts](03-workspace-chrome-and-panel-contracts.md)), and the underlying UI state machine ([UI State Machine and Session Persistence](04-ui-state-machine-and-session-persistence.md)).

## Model / Design

### Fixed Project Settings Tab (`settings`)

1. **Position and immutability:**
   - Placed immediately to the **left of `overview`** in the project workspace tab strip.
   - **Fixed and non-movable:** Excluded from tab reordering and tab hiding (`hidden_tabs`), always accessible for every registered project.
   - **Minimalist icon-only trigger:** Rendered strictly with the gear icon (`Settings`) and an `AppTooltip` (`"Project Settings"`), with **no textual label**.
2. **Surface registry integration:**
   - Registered as top-level node `id: "settings"` (`SurfaceNodeKind::Tab`) in `UiSurfaceRegistry`.
   - Hosts `get_project_config_schema` and `set_project_config_value` backend surface actions so the Global Voice Controller can inspect and mutate `rula.toml` settings directly.
3. **Panel chrome:** `ProjectConfigPanel.vue` follows the workspace panel contract in [Workspace Chrome and Panel Contracts](03-workspace-chrome-and-panel-contracts.md): full-bleed content, a single top toolbar, no repeated panel title, and no bottom action footer. Unsaved edits are guarded on tab/project navigation via `src/utils/navigationGuard.ts`, and external `rula.toml` changes never overwrite dirty values.

### Project Title and Description Metadata

1. **Editable display title (`projects.name`):**
   - `projects.name` is a mutable display title (independent of primary key `id` and filesystem `path`), initialized from the directory basename and editable inline in the `project_summary` widget via `update_project_name`.
2. **Persistence (`projects.description`):**
   - Optional `TEXT` column on the SQLite `projects` table (`description: Option<String>`), surfaced in `ProjectRecord` / `ProjectMetadata` and `get_ui_state` / project summaries so voice and agent orchestrators can disambiguate projects by domain purpose (e.g. matching *"bank reconciliation"* to `l10n_cl_banking`).
3. **Inline UI rendering:**
   - Displayed directly beneath the project title in `OverviewSummaryWidget.vue`.
   - When unset or empty, renders `"No description"` in muted style.
   - Includes a pencil icon button (wrapped in `AppTooltip`) for inline on-the-fly editing persisted via `update_project_description`.

### Plugin-Driven Overview Dashboard (`overview`)

The `overview` tab (`ProjectOverviewTab.vue`) renders a full-bleed 4-column `OverviewWidgetGrid.vue` without a top toolbar. Pending initialization flags render as an aligned alert banner above the grid when present.

Each active subsystem/tool (plus the pinned `project_summary` widget) declares a summary widget card injected dynamically into `overview`:

| Subsystem / Plugin | Capability Gate | Summary Metrics Displayed |
| :--- | :--- | :--- |
| **Project Summary (`project_summary`)** | Always enabled (pinned, non-hideable) | Favorite star toggle, inline-editable project title and description, skill/remote/supervisor status icons, `ProjectStatusLeds`, Git branch and status metrics, and `Customize` / `Done` layout toggle |
| **Project Management (`project_mgmt`)** | `has_project_management_enabled` | Active tasks count, overdue tasks count, per-stage breakdown, milestone progress % and drift/health summary |
| **Forge (`forge` / GitHub)** | `has_git` (+ Forge remote) | Unread/flagged/tracked items count, open PRs and unread/new issues summary |
| **Semantic Knowledge (`knowledge`)** | Always present in overview payload | Indexed chunks (code, spec, entity), graph edges, auto-index flag, min score %, indexing state and last duration |
| **Designer (`desigma`)** | `has_desigma_enabled` | Design files count, editor running / recent snapshot status |
| **CAD (`solidcraft`)** | `has_solidcraft_enabled` | CAD files count, editor running / validation and metrics summary |
| **PCB (`pcbcraft`)** | `has_pcbcraft_enabled` | PCB board targets count, editor running / DRC and export status |
| **Profiler (`profiler`)** | `has_profiler_enabled` | Runs count, latest profiler run target, status, and duration |
| **Devlog / Journal (`journal`)** | Always enabled | Total entries count, latest `agent_session_journal` entry title, agent, and timestamp |

- **Voice controller surface action (intended):** The `overview` surface node is intended to expose an overview summary action returning the aggregated JSON payload for all active widgets.

## Contracts

### Daemon Commands (`src-tauri/src/api.rs`, `src/types/daemonApi.ts`)

| Command | Args -> Result | Notes |
| :--- | :--- | :--- |
| `update_project_name` | `UpdateProjectNameArgs { projectId, name } -> string` | Updates mutable `projects.name` display title. |
| `update_project_description` | `UpdateProjectDescriptionArgs { projectId, description } -> string \| null` | Updates or clears `projects.description`. |
| `get_project_overview_summary` | `ProjectIdArgs { projectId } -> ProjectOverviewSummary` | Gathers persisted layout plus `projectMgmt`, `forge`, `knowledge`, `journal`, `profiler`, `desigma`, `solidcraft`, and `pcbcraft` summaries. |
| `save_project_overview_layout` | `SaveProjectOverviewLayoutArgs { projectId, layout: OverviewWidgetLayoutItem[] } -> null` | Validates grid bounds (`w` in `1..=4`, `h` in `1..=3`, `x < 4`, non-empty `id`) and persists layout JSON to SQLite. |

### Surface Actions (`src-tauri/src/ui_state.rs`, `capabilities/settings.rs`)

- On node `settings`:
  - `get_project_config_schema`: returns the project configuration tree with every configurable plugin field, options, description, current value, and provenance.
  - `set_project_config_value`: updates a project configuration field in `rula.toml` (`plugin_id`, `field_key`, `value_json`).

### Overview Layout Grid Constraints (`src-tauri/src/overview.rs`)

- Grid columns: `OVERVIEW_GRID_COLS_COUNT = 4`.
- Max widget row height: `OVERVIEW_GRID_MAX_H_ROWS = 3`.
- Widget descriptor (`OverviewWidgetLayoutItem`): `{ id, x, y, w, h, visible }`.

## Implementation Status

Verified against the repository:

- Fixed `settings` tab (`ProjectConfigPanel.vue`), `get_project_config_schema` / `set_project_config_value` surface actions (`src-tauri/src/ui_state.rs`, `src-tauri/src/capabilities/settings.rs`), and navigation guard integration: **Implemented**.
- Editable `projects.name` and `projects.description` (`src-tauri/src/registry.rs`, `src-tauri/src/db.rs`, `OverviewSummaryWidget.vue`): **Implemented**.
- Overview dashboard (`ProjectOverviewTab.vue`, `OverviewWidgetGrid.vue`, `src-tauri/src/overview.rs`) with `get_project_overview_summary`, `save_project_overview_layout`, and 9 widget types (including `knowledge` in addition to the 8 listed in legacy prose): **Implemented**.
- Surface action `get_overview_summary` on the `overview` node in `src-tauri/src/ui_state.rs` (the `overview` node currently declares `actions: Vec::new()`): **Roadmap / Partial** (backend data command `get_project_overview_summary` exists, but is not yet exposed as a `UiSurfaceActionDef` on `overview`).

## Related Specs

- [Workspace Chrome and Panel Contracts](03-workspace-chrome-and-panel-contracts.md)
- [UI State Machine and Session Persistence](04-ui-state-machine-and-session-persistence.md)
- [Configuration Schema and Cascade](../01-architecture/02-configuration-schema-and-cascade.md)
- [Project Registry and Service Orchestration](../03-projects/01-project-registry-and-service-orchestration.md)

Supersedes legacy: `37_project_settings_tab_and_overview_dashboard.md`
