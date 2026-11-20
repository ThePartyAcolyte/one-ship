# Project Management & Milestones

## Purpose

Define ONE-SHIP's opt-in multidisciplinary **Project** management subsystem (`Milestones`, `Board`, `Schedule`, `Gantt`, and `Forge` mode), its relational SQLite persistence (`project_milestones`, `project_tasks`, `project_task_stages`, `project_task_subtasks`, `project_task_links`), customizable Kanban workflows, strategic tracking modes (`linear`, `shape_up`, `weighted`) with Drift Compass health metrics, and open-source `Fork ↔ Upstream` collaboration workflow.

## Scope

In scope:
- Domain separation between Development Memory (`Journal`) and Project Management (`Project`).
- SQLite schema for tasks, subtasks, relational bridges, customizable stages, and strategic milestones.
- Customizable Kanban board, chronological Schedule, UTC-indexed Gantt chart with cross-project overlay, and single-row `SubtabBar` chrome (`Tasks | Forge`).
- Guided `take_forge_issue` workflow, agent MCP task/stage/milestone tools, and approval-gated `propose_forge_draft_pr`.
- Tracking modes (`linear`, `shape_up`, `weighted`), Drift Compass metrics, and `MilestoneDetailPanel` transitive aggregation.

Out of scope:
- Git working tree, commit/diff cockpit, and branch commands ([Git Workspace, Branches & Snapshots Cockpit](02-git-workspace-and-branches.md)).
- Provider-neutral Forge adapters and GitLab API details ([Forge Provider Abstraction](05-forge-provider-abstraction.md)).
- Forge inbox persistence and triage flags ([Forge Inbox & Triage](06-forge-inbox-and-triage.md)).

## Model / Design

### 1. Architectural Separation: `Project` vs. `Journal`

ONE-SHIP enforces a strict boundary between **Development Memory** (`Journal` / `devlog` tab) and **Project Management** (`Project` / `project` tab):

```text
┌─────────────────────────────────────────────────────────────────────────┐
│ PROJECT MANAGEMENT (`Project` Tab — Opt-in per Project)                 │
│                                                                         │
│  project_milestones (weight_percent, hill_position_percent, appetite)   │
│        │                                                                │
│        ├──1:N──► project_tasks ──1:N──► project_task_subtasks           │
│        │               │                                                │
│        │               └──N:M──► project_task_links ──► project_todos   │
│        │                                            └──► session_journal│
│        ├──1:N──► agent_session_journal (milestone_id)                   │
│        └──1:N──► project_technical_debt (milestone_id)                  │
└─────────────────────────────────────────────────────────────────────────┘
```

- **`Journal` (`devlog` tab):** Code-level execution memory (`project_todos`, `agent_session_journal`, `project_technical_debt`).
- **`Project` (`project` tab):** Opt-in per project via `[plugins.project_management] enabled = true` in `rula.toml` (or toggled via `toggle_project_management_plugin` in Overview). Toggling injects or cleans up the `oneship-project-mgmt` skill and sets `HydratedProject.has_project_management_enabled`.
- **Multidisciplinary tasks (`project_tasks`):** Represent software or real-world deliverables across domains (`code`, `hardware`, `procurement`, `design`, `docs`, `client`, `milestone`, `ops`) with optional scheduling (`start_epoch_seconds`, `due_epoch_seconds`) and Forge/Git linkage (`github_repo`, `github_issue_number`, `github_pr_number`, `branch_name`).
- **Explicit relational bridges (`project_task_links`):** Links zero or more `project_todos` and `agent_session_journal` entries to a `project_task` without merging the underlying tables.

### 2. Database Schema (`oneship.db`)

#### `project_tasks`

| Column | Type / Constraints | Notes |
| :--- | :--- | :--- |
| `id` | `INTEGER PRIMARY KEY AUTOINCREMENT` | — |
| `project_id` | `INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE` | — |
| `title` | `TEXT NOT NULL` | — |
| `description` | `TEXT` | — |
| `domain` | `TEXT NOT NULL DEFAULT 'code'` | `code`, `hardware`, `procurement`, `design`, `docs`, `client`, `milestone`, `ops` |
| `status` | `TEXT NOT NULL DEFAULT 'backlog'` | Matches `project_task_stages.slug` for the project |
| `priority` | `INTEGER NOT NULL DEFAULT 2` | `1`: high, `2`: medium, `3`: low |
| `milestone_id` | `INTEGER REFERENCES project_milestones(id) ON DELETE SET NULL` | Nullable link to strategic milestone |
| `weight_points` | `INTEGER NOT NULL DEFAULT 1` | Internal effort weight inside a milestone |
| `start_epoch_seconds` | `INTEGER` | Nullable (date-only values stored at 12:00 UTC) |
| `due_epoch_seconds` | `INTEGER` | Nullable |
| `github_repo` | `TEXT` | Nullable `owner/name` |
| `github_issue_number` | `INTEGER` | Nullable |
| `github_pr_number` | `INTEGER` | Nullable |
| `branch_name` | `TEXT` | Nullable |
| `created_at_seconds` | `INTEGER NOT NULL` | — |
| `updated_at_seconds` | `INTEGER NOT NULL` | — |
| `completed_at_seconds` | `INTEGER` | Nullable; controlled by terminal stage `is_terminal` |

#### `project_task_subtasks`

- `id` (`INTEGER PRIMARY KEY AUTOINCREMENT`)
- `task_id` (`INTEGER NOT NULL REFERENCES project_tasks(id) ON DELETE CASCADE`)
- `title` (`TEXT NOT NULL`)
- `is_completed` (`INTEGER NOT NULL DEFAULT 0`)
- `sort_order` (`INTEGER NOT NULL DEFAULT 0`)
- `linked_todo_id` (`INTEGER REFERENCES project_todos(id) ON DELETE SET NULL`)
- `created_at_seconds` (`INTEGER NOT NULL`)

#### `project_task_links`

- `id` (`INTEGER PRIMARY KEY AUTOINCREMENT`)
- `task_id` (`INTEGER NOT NULL REFERENCES project_tasks(id) ON DELETE CASCADE`)
- `link_type` (`TEXT NOT NULL CHECK(link_type IN ('todo', 'journal'))`)
- `target_id` (`INTEGER NOT NULL`)
- `created_at_seconds` (`INTEGER NOT NULL`)
- `UNIQUE(task_id, link_type, target_id)`

#### `project_task_stages`

- `id` (`INTEGER PRIMARY KEY AUTOINCREMENT`)
- `project_id` (`INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE`)
- `slug` (`TEXT NOT NULL`, `UNIQUE(project_id, slug)`) — matches `project_tasks.status`
- `label` (`TEXT NOT NULL`)
- `sort_order` (`INTEGER NOT NULL DEFAULT 0`)
- `is_collapsed` (`INTEGER NOT NULL DEFAULT 0`) — persisted compact column toggle
- `is_terminal` (`INTEGER NOT NULL DEFAULT 0`) — governs `project_tasks.completed_at_seconds`
- `created_at_seconds` (`INTEGER NOT NULL`)
- `updated_at_seconds` (`INTEGER NOT NULL`)

#### `project_milestones`

- `id` (`INTEGER PRIMARY KEY AUTOINCREMENT`)
- `project_id` (`INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE`)
- `title` (`TEXT NOT NULL`)
- `description` (`TEXT`) — architectural/commercial acceptance criteria
- `status` (`TEXT NOT NULL DEFAULT 'planned'`) — `planned`, `active`, `completed`, `archived`
- `weight_percent` (`REAL NOT NULL DEFAULT 0.0`) — macro value weight normalized across non-archived milestones in `weighted` mode
- `hill_position_percent` (`INTEGER NOT NULL DEFAULT 0`) — `0..49` = Uphill (architectural uncertainty); `50..100` = Downhill (known execution)
- `appetite_days` (`INTEGER`) — nullable Shape Up timebox budget
- `planned_start_epoch_seconds` (`INTEGER`) — nullable baseline start
- `planned_due_epoch_seconds` (`INTEGER`) — nullable baseline deadline
- `sort_order` (`INTEGER NOT NULL DEFAULT 0`)
- `created_at_seconds` (`INTEGER NOT NULL`)
- `updated_at_seconds` (`INTEGER NOT NULL`)
- `completed_at_seconds` (`INTEGER`) — nullable

Cross-table columns: `agent_session_journal.milestone_id` and `project_technical_debt.milestone_id` (`INTEGER REFERENCES project_milestones(id) ON DELETE SET NULL`).

### 3. Customizable Kanban Stages & Board UX

- **Per-project stage seeding:** Initializing or enabling `project_management` seeds default stages (`Backlog`, `Planned`, `In Progress`, `Review`, `Done`, `Blocked`). Backend validation and MCP schemas validate dynamically against `project_task_stages` rather than static status lists.
- **Full column management:**
  - Create stages, rename labels, mark terminal completion stages (`is_terminal`), and delete unused stages (non-empty stage deletion requires an explicit `reassign_to_stage_id`).
  - Horizontal drag-and-drop column reordering persisted transactionally via `sort_order`.
  - Collapsible/compact columns (`is_collapsed`) rendering narrow vertical strips with task count badges while preserving drag-and-drop dropzone targets.
- **Per-stage creation & reactive sync:**
  - Each column header carries a `+` trigger opening `CreateProjectTaskModal.vue` pre-targeted to that stage (replacing the former global `+` drawer).
  - No manual refresh button on the Board/Schedule toolbar; mutations and agent MCP updates synchronize reactively via `project-tasks-updated` SSE events (and `F5` silent refresh in Forge).

### 4. Project Modes (`Tasks | Forge`), Schedule, Gantt & Panel Chrome

#### Surface Nodes & Gating

| UI View | Surface Path |
| :--- | :--- |
| `Tasks > Milestones` | `project/milestones` |
| `Tasks > Board` | `project/board` |
| `Tasks > Schedule` | `project/schedule` |
| `Tasks > Gantt` | `project/gantt` |
| `Forge` | `project/forge` (`issues`, `prs`, `releases`) |

- `project.last_tasks_node` (`milestones | board | schedule | gantt`, default `board`) preserves the last Tasks view across `Tasks ↔ Forge` switches and reloads.
- `Forge` mode appears inside `Project` whenever `has_git` is true; if no remote is configured, it displays a link-repository empty state instead of mounting `ForgeCollabPanel`.
- Tasks content and the Gantt range banner are gated by `!isForgeActive` so `project/forge` never renders stale Tasks views.

#### Panel Chrome (`SubtabBar`)

- A single `36 px` `SubtabBar` hosts a fixed `AppSegmentedToggle` (`Tasks | Forge`, shown when `has_git` is true) on the left, the active mode's views next to it, and mode-specific tools on the right.
- Tasks mode displays `Milestones | Board | Schedule | Gantt` with domain filter, task search, and Gantt controls.
- Forge mode displays `Issues | Pull Requests` (or `Merge Requests`) `| Releases` with unread counts, `ForgeTopologyTools.vue` (provider badge and topology), and content-only `ForgeCollabPanel.vue` (releases lazily fetched on first visit).

#### Schedule & Gantt Geometry

- **Schedule:** Chronological list of tasks with `start_epoch_seconds` or `due_epoch_seconds`, sorted by due date (falling back to start date), with milestones at their due-date position. Undated tasks remain on the Board only.
- **Gantt:**
  - **UTC day index:** Horizontal positions derive from `floor(epoch_seconds / 86_400)`, independent of local timezone. Date-only fields are stored at `12:00 UTC`. The today marker maps the operator's local calendar date onto the same scale; overdue means "due day before local today", not "past 12:00 UTC".
  - **Marks:** `start_epoch_seconds`..`due_epoch_seconds` defines bars; single-date tasks render 1-day bars; `domain = "milestone"` tasks render as diamonds at their due date.
  - **Cross-project overlay:** When other registered projects enable `project_management`, a scope selector (`This Project` | `All PM Projects` | `+ <Project>`) overlays their dated tasks with `[ProjectName]` tags and dashed bars.
  - **Range & grid:** Spans all dated items plus today, padded by 2 days, guarded by a 20-year bound. Day grid and weekend shading use repeating CSS gradients so DOM size scales with tasks and months rather than days.
  - **Headers & sticky column:** Two-row header renders month labels and daily cells (`Sun`..`Sat` + day number at `>= 16 px/day`, day number only below). A sticky left column lists priority (or `[ProjectName]`), title, and anchor date.
  - **Tools & selection:** `Today` recenter button, `10–48 px/day` zoom, domain filter, task search, and cross-project overlay scope. Clicking a current-project bar/diamond/row opens `ProjectTaskDetailPanel.vue`. With no dated tasks, an empty state explains that a start or due date is required.

### 5. Open Source Collaboration (`Fork ↔ Upstream`)

- **Branch lifecycle:** Shared across all Git projects via `GitBranchSelector.vue` and `GitBranchModal.vue` (`list_git_branches`, `switch_git_branch`, `create_git_branch`), with Overview `Fetch Upstream` (`sync_upstream_fetch`) and `+ Add Upstream` (`configure_upstream_remote`); see [Git Workspace, Branches & Snapshots Cockpit](02-git-workspace-and-branches.md).
- **Remote topology & cached reads:** `get_forge_context` resolves `origin` vs `upstream` / fork `parent`, computes ahead/behind counts, and targets `upstream` > `parent` > `origin`. Issue/PR/release lists and details use TTL caching (`vcs.github_cache_ttl_seconds` / Forge cache).
- **Guided `take_forge_issue`:** Creates a `project_tasks` entry bound to repo/issue number, optionally creates a local branch with `switch = false` (preserving the user's active `HEAD`) or links an existing branch, and optionally creates a linked `project_todos` entry in `Journal`.
- **Agent read-only radar & safety-gated draft PRs:**
  - Read-only Forge MCP tools (`forge_*`) frame external issue/PR/MR/inbox payloads inside an explicit untrusted-data boundary against prompt injection.
  - `propose_forge_draft_pr` blocks via `await_user_approval` (`approval_type = "forge_draft_pr"`); only explicit `"Approved"` executes `create_forge_draft_pull_request` and returns the draft PR/MR URL for human review.

### 6. Strategic Milestones, Tracking Modes & Milestone Inspector

#### Tracking Modes (`[plugins.project_management].tracking_mode`)

All projects share one schema; calculation and presentation adapt to `tracking_mode` in `rula.toml` (default: `linear`):

| `tracking_mode` | Best For | Milestone Progress Formula | UI Focus |
| :--- | :--- | :--- | :--- |
| **`linear`** | Personal tools, OSS, emergent-first work | `weight_points` task rollup (`done_points / total_points`, where `done` = stage `is_terminal = 1`) | Flat milestone filter & progress bar; no percentage weights or baseline clutter |
| **`shape_up`** | R&D, exploratory / uncertain architecture | `hill_position_percent` (`0..49` Uphill, `50..100` Downhill) + optional `appetite_days` timebox; task rollup alongside | Compact Hill Chart marker + appetite countdown |
| **`weighted`** | Commercial / client projects with committed phases | **Two-tier hybrid:** macro `weight_percent` (normalized across non-archived milestones to 100%) × internal emergent `progress_ratio` | Baseline vs. Real dual indicator + Drift Compass |

#### Emergent Scope & Drift Compass

- Adding newly discovered `project_tasks` to a milestone updates that milestone's internal `progress_ratio` without mutating `weight_percent` or distorting sibling milestones.
- `effective_weight_percent = weight_percent / SUM(non_archived_milestones.weight_percent) * 100.0` (falling back to equal distribution when all weights are `0`).
- **Drift Compass (`kopeng::project_mgmt`):**
  - `schedule_drift_days`: difference between `planned_due_epoch_seconds` and the latest active child task `due_epoch_seconds` (or current time if overdue and incomplete).
  - `uphill_risk_flag`: `true` when `hill_position_percent < 50` and elapsed time exceeds `50%` of `[planned_start_epoch_seconds, planned_due_epoch_seconds]`.
  - `open_tech_debt_count` and `critical_tech_debt_count`: unresolved `project_technical_debt` rows linked to `milestone_id`.
  - `tokens_used_count` (per milestone) and `total_tokens_used_count` (per project): aggregated from `agent_session_journal.total_tokens` (direct and transitive) as budget-accounting scaffolding.

#### `MilestoneDetailPanel.vue` Transitive Aggregation

Clicking a milestone opens `MilestoneDetailPanel.vue` (`get_project_milestone_detail`) aggregating:
1. **Header & Strategy Controls:** title, acceptance criteria, status, effective weight % (`weighted`), Hill slider/appetite (`shape_up`), and Drift Compass badges.
2. **Transitive Journal / Minutes History:** all `agent_session_journal` rows where `milestone_id = milestone.id` OR `id IN (SELECT target_id FROM project_task_links WHERE link_type = 'journal' AND task_id IN (milestone_tasks))`, deduplicated and sorted `created_at DESC`, with direct `reopen_conversation` navigation.
3. **Aggregated TODOs Summary:** distinct `project_todos` linked via `project_task_links` (`link_type = 'todo'`) and `project_task_subtasks.linked_todo_id` (`completed_todos_count / total_todos_count`).
4. **Technical Debt Ledger:** open and resolved `project_technical_debt` entries attributed to `milestone_id`.
5. **Tasks Breakdown:** per-stage and per-domain counts, plus one-click filtering of Board/Schedule/Gantt to the milestone.

## Contracts

### Daemon Commands (`api.rs` & `src/types/daemonApi.ts`)

- **Plugin & Milestones:** `toggle_project_management_plugin`, `list_project_milestones`, `get_project_milestone_detail`, `create_project_milestone`, `update_project_milestone`, `delete_project_milestone`
- **Stages:** `list_project_stages`, `create_project_stage`, `update_project_stage`, `reorder_project_stages`, `delete_project_stage`
- **Tasks, Subtasks & Links:** `list_project_tasks`, `create_project_task`, `update_project_task`, `delete_project_task`, `create_task_subtask`, `toggle_task_subtask`, `delete_task_subtask`, `link_task_relation`, `unlink_task_relation`
- **Issue Onboarding:** `take_forge_issue`

### Events & Config

- **SSE Events:** `project-tasks-updated`, `project-milestones-updated`
- **Project Config (`rula.toml`):** `[plugins.project_management].enabled` (bool), `[plugins.project_management].tracking_mode` (`"linear" | "shape_up" | "weighted"`)
- **Global / VCS Config:** `vcs.github_cache_ttl_seconds` (cache TTL for Forge queries)

### MCP Tools (`src-tauri/src/mcp.rs` & `templates/skills/oneship-project-mgmt/SKILL.md`)

- `query_project_tasks` (optional `milestone_id` filter)
- `manage_project_stage` (`list` | `create` | `update` | `reorder` | `delete` with `reassign_to_stage_id` guard)
- `manage_project_task` (`create` | `update` | `move` | `delete`, optional `milestone_id`, dynamic stage check)
- `manage_task_subtask` (`create` | `toggle` | `delete`)
- `link_journal_to_task` (`link` | `unlink`, `link_type: "journal" | "todo"`)
- `manage_project_milestone` (`list` | `get_detail` | `create` | `update` | `delete`)
- `record_session_minutes` and `record_technical_debt` accept optional `milestone_id`

Every MCP mutation verifies caller project ownership via `CallerContext` and emits `project-tasks-updated` or `project-milestones-updated`.

## Implementation Status

| Area | Status | Notes |
| :--- | :--- | :--- |
| `project_tasks`, `project_task_subtasks`, `project_task_links`, `project_task_stages`, `project_milestones` (`db.rs`) | Implemented | — |
| `tracking_mode` (`linear` / `shape_up` / `weighted`), Drift Compass, milestone CRUD & detail (`project_mgmt.rs`, `config.rs`) | Implemented | — |
| Customizable Kanban stages, Board, Schedule, Gantt & cross-project overlay (`ProjectManagementTab.vue`, `CreateProjectTaskModal.vue`, `ProjectTaskDetailPanel.vue`) | Implemented | — |
| Milestones UI (`ProjectMilestonesView.vue`, `MilestoneDetailPanel.vue`, `CreateProjectMilestoneModal.vue`, `milestoneStore.ts`) | Implemented | — |
| Greenfield `tracking_mode` selector (`AddProjectModal.vue`, `initializer.rs`) & Overview PM widget (`overview.rs`) | Implemented | — |
| MCP tools (`query_project_tasks`, `manage_project_*`, `link_journal_to_task`, `propose_forge_draft_pr`) | Implemented | — |
| Dedicated `AppDatePicker` replacing native date inputs | Roadmap | Task/milestone forms still use native date inputs |

## Related Specs

- [Git Workspace, Branches & Snapshots Cockpit](02-git-workspace-and-branches.md)
- [Forge Provider Abstraction](05-forge-provider-abstraction.md)
- [Forge Inbox & Triage](06-forge-inbox-and-triage.md)
- [Greenfield Project Creation](10-greenfield-project-creation.md)
- [Project Category Index](README.md)

Supersedes legacy: `34_project_management_and_oss_collaboration.md`
