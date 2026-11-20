> [!WARNING]
> **LEGACY SPECIFICATION:** Superseded by [03-projects/07-project-management-and-milestones.md](../../specs/03-projects/07-project-management-and-milestones.md). Preserved for historical provenance only.

# Spec 34: Project Management & Open Source Collaboration
**Document Version:** 0.2.5  
**Domain:** Domain-Agnostic Project Planning, Weighted Milestones & Tracking Modes, Kanban, Schedule & Gantt, Git Branch Switching, Open Source (`Fork ↔ Upstream`) Collaboration  
**Maintainer:** ONE-SHIP / Omarchy Development Team

---

## 1. Architectural Separation: `Project` vs. `Journal`

ONE-SHIP maintains a strict domain boundary between **Development Memory** (`Journal`, Spec 22) and **Project Management** (`Project`):

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

* **`Journal` (`devlog` tab)** remains focused on code-level execution: `project_todos` (implementation checklists for developers/agents), `agent_session_journal` (session minutes and decisions), and `project_technical_debt`.
* **`Project` (`project` tab)** is an opt-in workspace subsystem enabled per project via `[plugins.project_management] enabled = true` in `rula.toml` (or toggled from Overview). Small personal repositories do not incur planning UI overhead unless enabled.
* **Multidisciplinary Tasks (`project_tasks`):** Tasks represent real-world or software deliverables across domains (`code`, `hardware`, `procurement`, `design`, `docs`, `client`, `milestone`, `ops`), with optional scheduling (`start_epoch_seconds`, `due_epoch_seconds`) and optional GitHub linkage (`github_repo`, `github_issue_number`, `github_pr_number`, `branch_name`).
* **Explicit Relational Bridges (`project_task_links`):** A `project_task` can link zero or more `project_todos` (delegating code execution to agents) and `agent_session_journal` entries (recording technical completion evidence) without merging the underlying tables.

---

## 2. Database Schema (`oneship.db`)

### 2.1 `project_tasks`
* `id`: `INTEGER PRIMARY KEY AUTOINCREMENT`
* `project_id`: `INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE`
* `title`: `TEXT NOT NULL`
* `description`: `TEXT`
* `domain`: `TEXT NOT NULL DEFAULT 'code'` (`code`, `hardware`, `procurement`, `design`, `docs`, `client`, `milestone`, `ops`)
* `status`: `TEXT NOT NULL DEFAULT 'backlog'` (`backlog`, `planned`, `in_progress`, `review`, `done`, `blocked`)
* `priority`: `INTEGER NOT NULL DEFAULT 2` (`1`: high, `2`: medium, `3`: low)
* `milestone_id`: `INTEGER REFERENCES project_milestones(id) ON DELETE SET NULL` (nullable — links emergent tasks to a strategic milestone)
* `weight_points`: `INTEGER NOT NULL DEFAULT 1` (optional internal effort weight inside a milestone)
* `start_epoch_seconds`: `INTEGER` (nullable)
* `due_epoch_seconds`: `INTEGER` (nullable)
* `github_repo`: `TEXT` (nullable `owner/name`)
* `github_issue_number`: `INTEGER` (nullable)
* `github_pr_number`: `INTEGER` (nullable)
* `branch_name`: `TEXT` (nullable)
* `created_at_seconds`: `INTEGER NOT NULL`
* `updated_at_seconds`: `INTEGER NOT NULL`
* `completed_at_seconds`: `INTEGER` (nullable)

### 2.2 `project_task_subtasks`
* `id`: `INTEGER PRIMARY KEY AUTOINCREMENT`
* `task_id`: `INTEGER NOT NULL REFERENCES project_tasks(id) ON DELETE CASCADE`
* `title`: `TEXT NOT NULL`
* `is_completed`: `INTEGER NOT NULL DEFAULT 0`
* `sort_order`: `INTEGER NOT NULL DEFAULT 0`
* `linked_todo_id`: `INTEGER REFERENCES project_todos(id) ON DELETE SET NULL`
* `created_at_seconds`: `INTEGER NOT NULL`

### 2.3 `project_task_links`
* `id`: `INTEGER PRIMARY KEY AUTOINCREMENT`
* `task_id`: `INTEGER NOT NULL REFERENCES project_tasks(id) ON DELETE CASCADE`
* `link_type`: `TEXT NOT NULL CHECK(link_type IN ('todo', 'journal'))`
* `target_id`: `INTEGER NOT NULL`
* `created_at_seconds`: `INTEGER NOT NULL`
* `UNIQUE(task_id, link_type, target_id)`

### 2.4 `project_task_stages`
* `id`: `INTEGER PRIMARY KEY AUTOINCREMENT`
* `project_id`: `INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE`
* `slug`: `TEXT NOT NULL` (`UNIQUE(project_id, slug)`) — matches `project_tasks.status`
* `label`: `TEXT NOT NULL`
* `sort_order`: `INTEGER NOT NULL DEFAULT 0`
* `is_collapsed`: `INTEGER NOT NULL DEFAULT 0`
* `is_terminal`: `INTEGER NOT NULL DEFAULT 0` — controls `project_tasks.completed_at_seconds`
* `created_at_seconds`: `INTEGER NOT NULL`
* `updated_at_seconds`: `INTEGER NOT NULL`

### 2.5 `project_milestones`
* `id`: `INTEGER PRIMARY KEY AUTOINCREMENT`
* `project_id`: `INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE`
* `title`: `TEXT NOT NULL`
* `description`: `TEXT` (architectural/commercial acceptance criteria)
* `status`: `TEXT NOT NULL DEFAULT 'planned'` (`planned`, `active`, `completed`, `archived`)
* `weight_percent`: `REAL NOT NULL DEFAULT 0.0` (macro value weight; normalized across active/planned/completed milestones in `weighted` mode)
* `hill_position_percent`: `INTEGER NOT NULL DEFAULT 0` (`0..49` = Uphill / architectural uncertainty; `50..100` = Downhill / known execution)
* `appetite_days`: `INTEGER` (nullable — Shape Up timebox budget)
* `planned_start_epoch_seconds`: `INTEGER` (nullable — client/baseline start)
* `planned_due_epoch_seconds`: `INTEGER` (nullable — client/baseline deadline)
* `sort_order`: `INTEGER NOT NULL DEFAULT 0`
* `created_at_seconds`: `INTEGER NOT NULL`
* `updated_at_seconds`: `INTEGER NOT NULL`
* `completed_at_seconds`: `INTEGER` (nullable)

---

## 3. Git Branch Modal & Upstream Sync (Overview & Explorer)

Available on all Git-tracked projects regardless of whether `project_management` is enabled:
* `list_git_branches`: Lists local branches (`refs/heads`) and remote-tracking refs (`refs/remotes`) with `is_current`, `is_remote`, `remote_name`, and `upstream` tracking info.
* `switch_git_branch`: Runs `git switch <branch>` (or `git switch --track <remote_branch>` when checking out a branch from `origin`). Never force-discards or auto-stashes uncommitted changes; conflicts fail explicitly with `GitError::Command`.
* `create_git_branch`: Validates branch names via `git check-ref-format --branch` and runs either `git switch -c <name> [start_point]` (`switch = true`, default for manual branch creation) or `git branch <name> [start_point]` (`switch = false`, creating a branch without modifying `HEAD`).
* Rendered via a clickable branch badge directly under the project title in `CardHeader` (and a compact trigger in the Explorer sidebar of `ProjectWorkspace`), opening `GitBranchModal`:
  * **Local Branches** as the primary switching list.
  * **Origin Remote Branches** (collapsible secondary section, excluding local duplicates and strictly excluding third-party `upstream/*` refs from branch switching).
  * **Create Branch** form with optional base point (`Current HEAD` or `upstream/<default>` when an upstream exists).
* **Overview Git Actions (`ProjectOverviewTab`):** `Fetch Upstream` (`sync_upstream_fetch`) and `+ Add Upstream` (`configure_upstream_remote`) live in the Project Overview alongside Git metrics rather than in the Project Management tab.

---

## 4. Phase 1 & Phase 2 Scope

### Phase 1 (Implemented Foundation)
* Schema tables (`project_tasks`, `project_task_subtasks`, `project_task_links`) + transactional CRUD.
* `[plugins.project_management]` configuration in `config.rs`, `HydratedProject.has_project_management_enabled`, automatic injection/cleanup of `oneship-project-mgmt` skill on `toggle_project_management_plugin`, and Overview toggle.
* `link_task_relation` / `unlink_task_relation` daemon commands (`api.rs`).
* `GitBranchSelector` in Overview and Explorer.
* `Project` workspace tab with:
  * **Board (Kanban):** 6 status columns (`Backlog`, `Planned`, `In Progress`, `Review`, `Done`, `Blocked`), domain badges, priority, subtasks, and links to `project_todos` / `agent_session_journal`.
  * **Schedule:** Chronological list of dated tasks and milestones. The former undated backlog section is gone: undated work already lives in the board's base stage.
  * **Gantt:** Frontend chart over the same scheduled tasks with day-scaled bars, milestone diamonds, month header, weekend shading and a today marker.
* Agent MCP tools: `query_project_tasks` and `link_journal_to_task`.

### Phase 2 (Open Source `Fork ↔ Upstream` Collaboration — Implemented)
* **Remote Topology & Upstream Management (`kopeng::github`):**
  * `get_github_collaboration_context`: Resolves local `origin` / `upstream` remotes, detects fork & parent repo via `gh repo view --json isFork,parent,defaultBranchRef,viewerPermission`, computes ahead/behind commit counts against `upstream`, and selects collaboration target (`upstream` > `parent` > `origin`).
  * `configure_upstream_remote`: Adds or updates the `upstream` git remote and runs initial `git fetch upstream`.
  * `sync_upstream_fetch`: Runs `git fetch upstream` and refreshes ahead/behind counts.
* **Cached Read-Only GitHub Views (`github_cache_ttl_seconds`):**
  * `list_github_issues`, `get_github_issue`: Browse open/closed issues, filter by label (`good first issue`, `help wanted`, etc.) and search query, and inspect full body & comments.
  * `list_github_prs`, `get_github_pr`: Inspect PR state, draft flag, mergeability, CI check rollups (`passing`/`failing`/`pending`), reviews, diff metrics, and comments (`All PRs` vs `My PRs`).
  * `list_github_releases`: Inspect published tags and releases on the target repository.
* **Guided Take-Issue Workflow (`kopeng::project_mgmt::take_github_issue`):**
  * Creates a `project_tasks` entry bound to `github_repo` and `github_issue_number`, optionally creates a local branch (`git branch` via `switch = false` so the active working `HEAD` is never disrupted unexpectedly) or links an existing branch name, and optionally creates a linked `project_todos` checklist item inside `Journal`.
  * `GithubCollabPanel` uses silent `F5` refresh instead of a visible manual refresh button (per `TECHNICAL_DEBT.md` #27).
* **Agent Read-Only Radar & Safety-Gated Draft PRs/MRs (`mcp.rs` + `capabilities/forge.rs` + `ForgeCollabPanel.vue`):**
  * Provider-neutral MCP tools (`forge_get_context`, `forge_list_issues`, `forge_get_issue`, `forge_list_prs`, `forge_get_pr`, `forge_list_releases`, `forge_list_inbox`, `forge_mark_item_read`, `forge_set_item_flag`, `forge_get_unread_counts`) route through the Unified Capability Core (`domain = "forge"`) and frame external issue/PR/MR/inbox payloads with an explicit untrusted-data boundary against prompt injection.
  * `propose_forge_draft_pr` blocks via `await_user_approval` (`approval_type = "forge_draft_pr"`); only explicit `"Approved"` executes `create_forge_draft_pull_request` and returns the PR/MR URL for manual review and promotion.

---

## 5. Phase 3: Customizable Kanban Stages & Board UX Refinements (Implemented)

Different projects (hardware builds, client work, open-source contributions, small tools) follow different workflows and should not be locked into a static set of 6 stages.

### 5.1 Per-Project Stage Persistence (`project_task_stages`)
* Introduce a dedicated SQLite table `project_task_stages`:
  * `id`: `INTEGER PRIMARY KEY AUTOINCREMENT`
  * `project_id`: `INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE`
  * `slug`: `TEXT NOT NULL` (`UNIQUE(project_id, slug)`) — matches `project_tasks.status`
  * `label`: `TEXT NOT NULL`
  * `sort_order`: `INTEGER NOT NULL DEFAULT 0`
  * `is_collapsed`: `INTEGER NOT NULL DEFAULT 0` — per-column compact/collapsed state persisted per project
  * `is_terminal`: `INTEGER NOT NULL DEFAULT 0` — marks stages considered completed (`completed_at_seconds`)
  * `created_at_seconds`: `INTEGER NOT NULL`
  * `updated_at_seconds`: `INTEGER NOT NULL`
* Seed default stages (`Backlog`, `Planned`, `In Progress`, `Review`, `Done`, `Blocked`) when a project initializes or enables `project_management`, while migrating any existing tasks seamlessly.
* Remove hardcoded static stage assumptions in backend validation (`VALID_TASK_STATUSES`) and MCP schemas so stage validation checks the project's registered `project_task_stages` rows instead.

### 5.2 Full Column Management & Compact Mode
* **Create / Rename / Delete Stages:**
  * Inline or modal stage editor per project allowing users to add new stages, rename existing stage labels, mark terminal completion stages, and delete unused stages (requiring explicit target-stage reassignment if the deleted stage still holds tasks).
* **Drag-and-Drop Stage Reordering:**
  * Users can drag stage column headers horizontally to reorder the board (`sort_order` persisted transactionally in SQLite).
* **Collapsible / Compact Columns:**
  * Each column header exposes a collapse/compact toggle (`is_collapsed`) turning low-focus columns (e.g. `Backlog`, `Done`, `Blocked`) into narrow vertical strips with task count badge while keeping drag-drop dropzone capability intact.

### 5.3 Per-Stage Task Creation & Modal Form
* **Contextual `+` Inside Each Column:**
  * Remove the global `+` task button and inline top drawer from the `Board`/`Schedule` toolbar.
  * Place a `+` trigger directly in each Kanban stage column so creating a task automatically targets that stage.
* **Dedicated Task Creation Modal:**
  * Clicking `+` on a stage opens a focused modal dialog (`CreateProjectTaskModal.vue`) with generous spacing for title, domain, stage selector (pre-filled from the clicked column), priority, start/due dates, optional branch, and initial subtasks/description.

### 5.4 Reactive Board Toolbar Cleanup
* Remove the manual refresh button from the `Board`/`Schedule` toolbar: task creation, updates, deletions, subtask toggles, Journal links, and MCP agent updates synchronize reactively via `project-tasks-updated` SSE events and command responses.

### 5.5 Full Agent MCP Lifecycle Coverage (`mcp.rs` + `oneship-project-mgmt`)
* **`manage_project_stage`:** Full stage lifecycle per project (`action: "list" | "create" | "update" | "reorder" | "delete"` with `reassign_to_stage_id` guard on non-empty deletion).
* **`manage_project_task`:** Full task lifecycle (`action: "create" | "update" | "move" | "delete"`) validated dynamically against the project's `project_task_stages`.
* **`manage_task_subtask`:** Checklist management inside a task (`action: "create" | "toggle" | "delete"`).
* **`link_journal_to_task`:** Bidirectional relation management (`action: "link" | "unlink"`, `link_type: "journal" | "todo"`), complementing read-only `query_project_tasks`. Every MCP mutation verifies explicit project ownership and broadcasts `project-tasks-updated`.

---

## 6. Project Modes: Tasks | Forge, Schedule & Gantt (Frontend, Spec 13/35)

### 6.1 Two Modes Inside the Existing `project` Tab
* **`Tasks` (internal project management):** `Board`, `Schedule` and `Gantt` views.
* **`Forge` (external collaboration flow):** renders `ForgeCollabPanel` (issues, pull/merge requests, releases, inbox/triage). It is available on any git project; when no remote is configured (or git status is unavailable) it renders a link-repository empty state instead of mounting the Forge panel and failing. The mode shell is provider-agnostic and supports GitHub (`gh`) and GitLab (`glab`) via Spec 39 (`kopeng::forge`).
* No new first-level workspace tab is introduced. The mode tabs live inside the existing `Project` surface and are rendered when the project is a git repository (Forge has no meaning without one).

### 6.2 Surface Nodes & `project.last_tasks_node`
The daemon surface tree exposes real nodes for every view:

| UI | Surface path |
| :--- | :--- |
| `Tasks > Milestones` | `project/milestones` |
| `Tasks > Board` | `project/board` |
| `Tasks > Schedule` | `project/schedule` |
| `Tasks > Gantt` | `project/gantt` |
| `Forge` | `project/forge` (`issues`, `prs`, `releases`) |

`project.last_tasks_node` (`milestones | board | schedule | gantt`, default `board`) is persisted as an explicit `UiSlotDef` on the `project` node so switching between `Tasks` and `Forge` restores the user's last Tasks view across reloads and remains inspectable by the Global Voice Controller.

The Tasks content branches and the Gantt range banner are gated by `!isForgeActive`, so `project/forge` can never render a stale Board, Schedule or Gantt view.

### 6.3 Schedule
* Lists every task with `start_epoch_seconds` or `due_epoch_seconds`, sorted by due date (falling back to the start date).
* Milestones appear in their due-date position.
* Undated tasks are not repeated here; they remain in the board's base stage.

### 6.4 Gantt
* **Geometry:** positions derive from UTC day indexes (`floor(epoch_seconds / 86_400)`), independent of the local timezone. Date-only fields are written at 12:00 UTC, so this index is their calendar date. The today marker maps the operator's local calendar date to the same scale; overdue means "due day before the local today", not "past 12:00 UTC". `start_epoch_seconds`/`due_epoch_seconds` define the bar; milestones (`domain = "milestone"`) render as diamonds at their due date; a task with only one date renders a one-day bar.
* **Cross-Project Overlay Filter:** when other registered projects have Project Management enabled, a scope selector in the Gantt toolbar (`This Project` | `All PM Projects` | `+ <Project>`) overlays their dated tasks on the same timeline (with `[ProjectName]` tags and dashed bars) so cross-project schedule overlaps are visible at a glance.
* **Range:** the visible range spans every dated task plus today, padded by two days. A 20-year bound guards pathological dates.
* **Rendering:** the day grid and Saturday/Sunday shading are repeating CSS gradients, so the grid DOM grows with tasks and months, not with days. Month labels are absolutely positioned spans.
* **Date header:** a second header row renders one cell per day with the weekday abbreviation (`Sun`…`Sat`) and the day number at 16 px/day zoom and wider (only the number below that), highlighted for weekends and today. Weekends and today are highlighted, and the fixed task column uses a matching two-row header so the rows stay aligned.
* **Row labels:** a sticky left column lists priority (or `[ProjectName]` for overlaid projects), title and anchor date; the label column stays fixed during horizontal scrolling and both headers stay fixed during vertical scrolling.
* **Tools:** `Today` recenters the viewport on the current day; zoom adjusts the day width between 10 and 48 px. The domain filter, task search, and cross-project overlay scope apply to the Gantt chart.
* **Selection:** clicking a current-project bar, diamond or row selects the task and opens the existing `ProjectTaskDetailPanel`.
* **Empty state:** with no dated tasks, the view explains that a start or due date is required.

### 6.5 Panel Chrome
* A single `SubtabBar` row collapses mode and views: a fixed `AppSegmentedToggle` (`Tasks | Forge`, rendered only when the project has git) on the left, the active mode's views next to it, and that mode's tools on the right.
* Tasks renders `Board | Schedule | Gantt` with the domain filter, task search and the Gantt toolset. Forge renders `Issues | Pull Requests (or Merge Requests) | Releases` with unread counters plus the provider badge and repository topology; when the repository has no remote the row keeps only the mode toggle above the link-repository empty state.
* `ForgeCollabPanel` is content-only: it reads the `project/forge` view node for the active subtab and renders no bar of its own. Releases stay lazily fetched on first visit to that node.
* The bar follows Spec 13 §3.13 (fixed 36 px, semantic surfaces, tabs-only or tabs+tools).

---

## 7. Phase 4: Strategic Milestones, Tracking Modes & Milestone Inspector (Planned)

### 7.1 Decoupled Storage vs. Tracking Strategy (`tracking_mode`)
All projects share one uniform schema (`project_milestones`, `project_tasks`, `agent_session_journal`, `project_technical_debt`). Only the progress calculation and UI presentation adapt to `[plugins.project_management].tracking_mode` in `rula.toml` (default: `linear`):

| `tracking_mode` | Best For | Milestone Progress Formula | UI Focus |
| :--- | :--- | :--- | :--- |
| **`linear`** | Personal tools, OSS, emergent-first work | Unweighted / `weight_points` task rollup (`done_points / total_points`, where `done` = stage `is_terminal = 1`). | Flat milestone filter & progress bar; no percentage weights or baseline clutter. |
| **`shape_up`** | R&D, exploratory / uncertain architecture | `hill_position_percent` (`0..49` = Uphill / figuring out, `50..100` = Downhill / executing) + optional `appetite_days` timebox; task rollup shown alongside. | Compact Hill Chart (`Uphill` / `Downhill` draggable marker) + appetite countdown. |
| **`weighted`** | Commercial / client projects with committed phases | **Two-Tier Hybrid:** Macro `weight_percent` (normalized across non-archived milestones to 100% effective share) multiplied by each milestone's internal emergent `progress_ratio`. | **Baseline vs. Real** dual indicator + **Drift Compass**. |

### 7.2 Emergent Scope & Drift Compass (`weighted` Mode)
* **Macro Stability vs. Micro Discovery:** Adding newly discovered `project_tasks` to a milestone updates that milestone's internal `progress_ratio` without mutating its `weight_percent` or distorting sibling milestones.
* **Weight Normalization:** Milestones store relative `weight_percent` values; the backend computes `effective_weight_percent = weight_percent / SUM(active_milestones.weight_percent) * 100.0` (falling back to equal distribution if all weights are `0`).
* **Drift Compass Metrics (Computed Purely in `kopeng::project_mgmt`):**
  * `schedule_drift_days`: Difference between `planned_due_epoch_seconds` and the latest active child task `due_epoch_seconds` (or `now` if past due and not `completed`).
  * `uphill_risk_flag`: `true` when `hill_position_percent < 50` and elapsed time exceeds `50%` of the `[planned_start_epoch_seconds, planned_due_epoch_seconds]` window.
  * `open_tech_debt_count` & `critical_tech_debt_count`: Count of unresolved `project_technical_debt` rows linked to the milestone.
* *(Future Spec Hook)*: Commercial budgeting / cost-per-milestone extensions will build directly on `project_milestones.weight_percent` and `planned_start_epoch_seconds` / `planned_due_epoch_seconds` without altering the task layer. Token consumption scaffolding (`tokens_used_count` per milestone and `total_tokens_used_count` per project) is aggregated from `agent_session_journal.total_tokens` (direct and transitive) so runner token telemetry populates milestone budgets automatically once runner accounting is wired.

### 7.3 Milestone Detail & Aggregated History View (`MilestoneDetailPanel`)
Clicking any milestone (in the Milestones strip, Schedule, or Gantt) opens a dedicated detail drawer/panel aggregating all cross-subsystem evidence:
1. **Header & Strategy Controls:** Title, acceptance criteria, status, effective weight % (in `weighted`), Hill slider/appetite (in `shape_up`), and Drift Compass badges.
2. **Transitive Journal / Minutes History (`agent_session_journal`):**
   * Queries all session journal entries where `agent_session_journal.milestone_id = milestone.id` **OR** `agent_session_journal.id IN (SELECT target_id FROM project_task_links WHERE link_type = 'journal' AND task_id IN (milestone_tasks))`, deduplicated and sorted chronologically (`created_at DESC`).
   * Preserves direct `reopen_conversation` navigation from any minute entry.
3. **Aggregated TODOs Summary (`project_todos`):**
   * Counts and lists distinct `project_todos` linked across the milestone's tasks (via `project_task_links` with `link_type = 'todo'` and `project_task_subtasks.linked_todo_id`), showing `completed_todos_count / total_todos_count`.
4. **Technical Debt Ledger (`project_technical_debt`):**
   * Lists open and resolved technical debt entries attributed to `milestone_id`, surfaced as Milestone Health.
5. **Tasks Breakdown:**
   * Per-stage (`project_task_stages`) and per-nature (`domain`) counts, plus quick navigation to filter the Board/Schedule/Gantt to that milestone.

### 7.4 Phased Implementation Plan
1. **Phase 4A — Backend, SQLite & MCP (`src-tauri/`):**
   * Add `project_milestones` table + `milestone_id` / `weight_points` on `project_tasks` + `milestone_id` on `agent_session_journal` and `project_technical_debt` (idempotent migrations in `db.rs`).
   * Add `tracking_mode` (`linear` | `shape_up` | `weighted`) to `ProjectManagementConfig` (`config.rs`).
   * Implement pure calculation & Drift Compass module in `kopeng::project_mgmt` + transactional CRUD and aggregated `get_milestone_detail` query (returning transitive journal minutes, todo counts, tech debt, and task breakdown).
   * Register daemon commands in `api::API_COMMANDS` / `api::dispatch` (`src-tauri/src/api.rs`) and `ts-rs` exports (`./scripts/run_dev.sh export-bindings`).
   * Add `manage_project_milestone` MCP tool and extend `manage_project_task`, `query_project_tasks`, `record_session_minutes`, and `record_technical_debt` with optional `milestone_id` (`src-tauri/src/mcp.rs` + `templates/skills/oneship-project-mgmt/`).
2. **Phase 4B — Frontend UI (`src/`):**
   * Mirror daemon commands in `src/types/daemonApi.ts` and extend `projectMgmtStore` (bounded, reactive via SSE `project-milestones-updated` / `project-tasks-updated`).
   * Add Milestone strip/header in `Project` tab (adapting cleanly to `linear`, `shape_up`, `weighted` per Spec 13), milestone selector in `CreateProjectTaskModal.vue` / `ProjectTaskDetailPanel.vue`, and `MilestoneDetailPanel.vue` for aggregated minutes, todos, and tech debt.
   * Update `Schedule` and `Gantt` to group/display milestone phases and baseline windows alongside child tasks.
3. **Phase 4C — Onboarding & Overview (`Spec 26` & `Spec 37`):**
   * Add `projectNature` / `tracking_mode` selector to `AddProjectModal.vue` and `create_greenfield_project`, and update `greenfield_initializer.md` to scaffold initial milestones before tasks.
   * Surface milestone completion % and Drift/Health summary in the `ProjectOverviewTab` Project Management widget.
