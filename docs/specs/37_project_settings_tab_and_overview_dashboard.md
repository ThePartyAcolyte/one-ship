# Spec 37: Dedicated Project Settings Tab & Plugin-Driven Overview Dashboard
**Document Version:** 0.2.2  
**Domain:** Workspace Navigation, Fixed Settings Tab, Dynamic Overview Dashboard Widgets, Project Description Metadata  
**Maintainer:** ONE-SHIP / Omarchy Development Team

---

## 1. Architectural Purpose & Scope

Separates **Project Configuration** (`ProjectConfigPanel.vue`) from the **Project Overview** (`ProjectOverviewTab.vue`), transforming `overview` into a live, plugin-driven summary dashboard while giving per-project settings a dedicated, permanent workspace tab.

---

## 2. Fixed Project Settings Tab (`settings`)

1. **Position & Immutability:**
   - Placed immediately to the **left of `overview`** in the project workspace tab strip.
   - **Fixed and non-movable:** Excluded from tab reordering and tab hiding (`hidden_tabs`), always accessible for every registered project.
   - **Minimalist Icon-Only Trigger:** Rendered strictly with the gear icon (`Settings`) and an `AppTooltip` (`"Project Settings"`), with **no textual label** (`AGENTS.md §5.5`).
2. **Surface Registry Integration (Spec 35):**
   - Registered as top-level node `id: "settings"` (`SurfaceNodeKind::Tab`) in `UiSurfaceRegistry`.
   - Hosts `get_project_config_schema` and `set_project_config_value` backend surface actions so the Global Voice Controller (Spec 36) can inspect and mutate `rula.toml` settings directly.
3. **Panel Chrome:** `ProjectConfigPanel.vue` follows the workspace panel contract in Spec 13 §3.13: full-bleed content, a single top toolbar, no repeated panel title, and no bottom action footer. Unsaved edits are guarded on tab/project navigation and external `rula.toml` changes never overwrite dirty values.

---

## 3. Project Title & Description Metadata

1. **Editable Display Title (`projects.name`):**
   - `projects.name` is a mutable display title (independent of primary key `id` and filesystem `path`), initialized from the directory basename and editable inline in the `project_summary` widget via `update_project_name`.
2. **Persistence (`projects.description`):**
   - Optional `TEXT` column on SQLite `projects` table (`description: Option<String>`), surfaced in `ProjectRecord` / `ProjectMetadata` and `ui_get_state` project summaries so voice/agent orchestrators can disambiguate projects by domain purpose (e.g. matching *"bank reconciliation"* to `l10n_cl_banking`).
3. **Inline UI Rendering:**
   - Displayed directly beneath the project title in `OverviewSummaryWidget.vue`.
   - When unset or empty, renders `"No description"` in muted style.
   - Includes a pencil icon button (wrapped in `AppTooltip`) for inline on-the-fly editing persisted via `update_project_description`.

---

## 4. Plugin-Driven Overview Dashboard (`overview`)

The `overview` tab renders a full-bleed 4-column `OverviewWidgetGrid.vue` without a top toolbar. Pending initialization flags render as an aligned alert banner above the grid when present.

Each active subsystem/tool (plus the pinned `project_summary` widget) declares a summary widget card injected dynamically into `overview`:

| Subsystem / Plugin | Capability Gate | Summary Metrics Displayed |
| :--- | :--- | :--- |
| **Project Summary (`project_summary`)** | Always enabled (pinned, non-hideable) | Favorite star toggle, inline-editable project title & description, skill/remote/supervisor status icons, `ProjectStatusLeds`, Git branch & status metrics, and `Customize` / `Done` layout toggle |
| **Project Management** | `has_project_management_enabled` | Active tasks count, overdue tasks count, per-stage breakdown, milestone progress % & drift/health summary |
| **GitHub** | `has_git` + GitHub remote | Open PRs count, unread/new issues count (Spec 38) |
| **Designer (`desigma`)** | `has_desigma_enabled` | Design files count, recent snapshot status |
| **CAD (`solidcraft`)** | `has_solidcraft_enabled` | CAD files count, validation/metrics summary |
| **PCB (`pcbcraft`)** | `has_pcbcraft_enabled` | PCB board targets count, DRC/export status |
| **Profiler** | `has_profiler_enabled` | Latest profiler run duration/status summary |
| **Devlog / Journal** | Always enabled | Latest `agent_session_journal` entry title & timestamp |

- **Voice Controller Surface Action:** `overview` exposes `get_overview_summary` returning the aggregated JSON payload for all active widgets.
