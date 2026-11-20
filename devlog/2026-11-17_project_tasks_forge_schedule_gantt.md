# 2026-11-17 — Project Modes (Tasks | Forge), Schedule Cleanup & Gantt Chart

## Summary

Restructured the `Project` workspace tab into two internal modes, `Tasks` and `Forge`, without adding a first-level workspace tab or touching Rust. `Forge` is the external collaboration flow (issues, pull requests, releases) and is available on any git project; without a remote it shows a link-repository empty state instead of mounting the GitHub panel. Inside `Tasks`, the old `Timeline` view became `Schedule` (dated tasks only, no undated backlog) and a new frontend-only `Gantt` view renders the same scheduled data on a day grid. `SubtabBar` was restyled globally as a 36 px two-zone bar: tabs on the left, optional tools on the right, usable with or without tools.

## Changes Implemented

### 1. Global `SubtabBar` Restyle (`src/components/ui/SubtabBar.vue`)

- Fixed `h-9` row with `ui-surface-title` background and a bottom border.
- Left zone: tabs only. Buttons are `h-6 px-2.5`, `text-[9px]`, uppercase, accent border/fill when active, transparent border and hover surface when inactive. The zone scrolls only when the tabs cannot fit.
- Right zone: optional `right` slot behind a left divider. When the slot is empty the divider and zone are omitted, so a tabs-only bar keeps identical geometry.
- No wrapping: wide tool content scrolls horizontally inside the right zone.
- Updated the four consumers for the new height:
  - `ProjectManagementTab`: compact domain select (`[&>button]:h-6`), `h-6` search input, Gantt tools.
  - `GithubCollabPanel`: topology chips are now `flex-nowrap` + `whitespace-nowrap` so they scroll instead of stacking in the 36 px bar.
  - `ProjectSkillsTab`: right-side metrics are `whitespace-nowrap`.
  - `ProjectElectronicsTab`: unchanged content, inherits the new bar.

### 2. Project Modes (`ProjectManagementTab.vue`)

- Added a first bar with `Tasks | Forge` (`SubtabBar`, tabs only), rendered when the project has git; a non-git project keeps the view bar alone.
- **Forge availability:** `forgeModeAvailable = project.has_git`; `hasForgeRemote = Boolean(project.git_status?.has_remote)`. A git project without a remote (or with a git status error) shows a link-repository empty state instead of mounting the GitHub panel and failing. Non-git projects hide Forge.
- The empty state includes `Link Repository`, which emits `openGithub` through `ProjectWorkspace` to the existing GitHub Remote modal in `App.vue`.
- A persisted `project/github` node is redirected to `project/board` only when the project has no git (watcher on `activeSubView` + `forgeModeAvailable`, plus the project-path watcher).
- `Forge` embeds the existing `GithubCollabPanel` when a remote exists: `Issues | Pull Requests | Releases` and the origin/upstream/target topology.
- Fixed the mode/view crossover: while Forge is active, `tasksView` still returns the remembered Tasks view, so the Gantt branch matched before the Forge branch. The Board/Schedule/Gantt content branches and the Gantt range banner are now gated by `!isForgeActive`; the remembered view is used only to restore the mode when leaving Forge.
- Provider-agnostic shell: the mode is not named after GitHub. The current content is still the GitHub adapter; GitLab/other providers are Technical Debt #41.
- The last active Tasks view is restored when returning from Forge.

### 3. Surface Mapping Without Rust

The daemon registry only exposes `project/board`, `project/timeline` and `project/github`:

| UI | Surface path |
| :--- | :--- |
| `Tasks > Board` | `project/board` |
| `Tasks > Schedule` | `project/timeline` |
| `Tasks > Gantt` | `project/timeline` + local `showGantt` toggle |
| `Forge` | `project/github` |

No node was added to `ui_state.rs`. The Gantt selection is frontend-local, remembered per project in `localStorage` (`oneship:project:gantt_mode:<projectId>`) so it survives workspace tab switches and reloads; persisting it as a real surface node is Technical Debt #42.

### 4. Schedule Cleanup

- `Timeline` renamed to `Schedule`.
- Lists only tasks with `start_epoch_seconds` or `due_epoch_seconds`, sorted by due date (falling back to start date).
- Removed the `Undated Backlog` grid: undated work already lives in the board's base stage, so duplicating it was redundant.
- Added a proper empty state ("Add a start or due date...").
- Selecting an entry still opens `ProjectTaskDetailPanel`.

### 5. Gantt View (Frontend Only)

- Data: `start_epoch_seconds` / `due_epoch_seconds`; milestones (`domain = "milestone"`) are points at their due date; tasks with only one date render a one-day bar.
- Geometry: UTC day indexes (`floor(epoch_seconds / 86_400)`), so bars do not shift with the local timezone. Date-only fields are written at 12:00 UTC; the today marker maps the local calendar date to the same scale, so "overdue" is calendar-based (due day before local today) instead of instant-based against 12:00 UTC.
- Range: every dated task plus today, padded by two days, with a 20-year guard (`GANTT_MAX_SPAN_DAYS = 7_320`).
- Grid: day lines and Saturday/Sunday shading are repeating CSS gradients (`repeating-linear-gradient`), so the DOM grows with tasks and months, not with days. Month labels and separators are absolutely positioned.
- Date header: two rows. The first shows month/year; the second renders one cell per day with the weekday abbreviation (`Sun`…`Sat`) and day number at 16 px/day and wider (only the number below that), highlighted for weekends and today. The task label column uses a matching two-row header.
- Sticky chrome: the 240 px task-label column is `sticky left-0`; both headers are `sticky top-0`; the label column and headers remain visible while scrolling.
- Bars use semantic stage colors (`--status-ok-*`, `--status-error-*`, `--status-info-*`, card/border for pending). Wide bars (>= 110 px) show the truncated title.
- Milestones render as rotated accent diamonds at the due-day center.
- Today: accent vertical line across the header and rows.
- Tools in the tasks bar while Gantt is active:
  - `Today` recenters the scrollport on the current day.
  - `Zoom out` / `Zoom in` adjust the day width from 10 to 48 px.
  - Domain filter and task search apply identically to Board, Schedule and Gantt.
- Selection: clicking a bar, diamond, label or row selects the task and opens the existing detail inspector.
- Empty state explains that a start or due date is required.
- Range guard: if a task falls beyond the 20-year bound, a warning strip reports how many scheduled tasks are outside the chart instead of hiding them silently.
- Bounded layout: no unbounded day loop; months are capped by the same 20-year range.

### 6. Living Documentation

- `docs/specs/13_design_system_and_ui_components.md` → **1.3.1**: §3.13 gained the `SubtabBar` two-zone contract (tabs-only vs tabs+tools, fixed height, no wrapping).
- `docs/specs/34_project_management_and_oss_collaboration.md` → **0.2.4**: Phase 1 views updated (Schedule + Gantt, no undated backlog) and new §6 documenting the `Tasks | Forge` modes, surface mapping, Schedule and Gantt.
- `docs/specs/35_ui_state_machine_and_session_persistence.md` → **0.2.3**: §6.3 documents frontend-local presentations of an existing surface node (Gantt over `project/timeline`, Forge over `project/github`).
- `TECHNICAL_DEBT.md`: #41 Forge provider adaptation; #42 Gantt as a persisted surface node.
- `TODO.md`: runtime checks for the bar, modes, gating, Schedule, Gantt and regressions.

## Validation Required

No build, test, runtime process or dev server was started by the agent (AGENTS.md §1.6). The self-managed watcher rebuilds the frontend.

1. `SubtabBar`: open Project, GitHub, Skills and Electronics; confirm the 36 px height, tabs-only bars without a divider, tabs+tools bars with a divider, and horizontal scrolling instead of wrapping on narrow windows.
2. Modes: a project without a remote shows only `Tasks`; a project with a remote shows `Tasks | Forge`; a persisted `project/github` path on a local-only project lands on Board.
3. Schedule: only dated tasks/milestones appear, sorted by date; undated tasks are gone from this view; clicking one opens the detail panel.
4. Gantt: bars match dates, milestones are diamonds, the label column and headers stick while scrolling, `Today` centers the chart, zoom changes the scale, the domain filter/search apply, and clicking selects a task.
5. Empty state: a project with no dated tasks shows the guidance message instead of an empty grid.
6. Regressions: Forge → Tasks restores the last view; the detail inspector never renders over Forge; the board drag/drop and stage editor are unaffected.

## Pending

- Runtime confirmation of the above (manual UI checks).
- Forge still calls the GitHub adapter for every remote host (Technical Debt #41).
- External inbox flags/read state remain unimplemented by design (Spec 38).
- Gantt selection persists locally per project in `localStorage`, but remains invisible to the daemon surface registry (Technical Debt #42).
- Backend handoff for the real Schedule/Gantt/Forge nodes, provider abstraction and inbox is in `devlog/2026-11-17_project_forge_backend_handoff.md` and Spec 39.
