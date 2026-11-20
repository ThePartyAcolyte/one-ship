# 2026-11-17 — Shell Geometry, Settings/Drawer Polish & Legacy Token Normalization

## Summary

Continued the visual-language unification started in `2026-11-17_visual-language-unification.md`. The application shell was re-laid out so the project rail reaches the bottom edge, the bottom agent taskbar starts where the tab bar starts, and the docked chat column reaches the bottom. The Settings modal and Projects drawer were brought onto the Omnisearch/System Logs visual language, and the undefined legacy token layer (`--surface-color` and siblings) was replaced globally with the Spec 13 surface/text tokens. The agent launcher popover, which appeared translucent, was moved into the workspace stage's stacking context and given a solid elevated surface.

## Root Causes

### Undefined legacy CSS tokens

`--surface-color` was referenced 176 times in 35 components but was never defined in `src/styles/ui.css`, `App.vue`, `index.html`, or any runtime theme injection. The same applied to `--card-bg` (13), `--bg-card` (4), `--text-color` (28), `--text-primary`, `--bg-primary`, and `--status-warning-*`.

An undefined `var()` reference invalidates the declaration at computed-value time, so `background-color` fell back to `transparent`. This is why:

- the bottom taskbar (`bg-[var(--surface-color)]/95`) and the agent launcher rows looked transparent;
- selected/hovered `AppSelect` and model-select rows had no visible highlight;
- many cards, modal headers and form panels lost their surface;
- `--text-color` silently inherited the parent color instead of failing loudly.

### Agent launcher stacking and surface

The popover used `bg-[var(--bg-color)]`, the exact same tone as the workspace, and its `z-50` lived inside the taskbar's `z-30` stacking context while the taskbar was a sibling of the workspace. The combination of the transparent taskbar token, a base-color popover over a base-color stage, and the old stacking position read as a translucent dropdown. The popover now lives inside the workspace stage and uses `ui-surface-title`, so it paints above the tab content with an opaque, elevated surface.

### Shell geometry

The taskbar was a global sibling below the whole main viewport, the rail stopped at the workspace bottom, and `pb-1` on the main viewport kept the docked chat 4px above the bottom. The tab bar mixed a 25px gear tab with 21px text tabs and 33px/36px bar heights.

## Changes Implemented

### 1. Application shell (`src/App.vue`, `src/components/workspace/ProjectWorkspace.vue`, `src/components/taskbar/AgentTaskbar.vue`)

- Removed `AgentTaskbar` from `App.vue` and mounted it inside the workspace stage column, after the tab content and before the stage closes. The taskbar now starts at the rail's right edge (same x as the tab bar) and ends at the workspace's right border.
- Removed `pb-1` from the main viewport; the docked chat and resizer now reach the bottom edge.
- The project rail (`ProjectVerticalRail`) is a normal stretched flex item of the full-height workspace, so it spans from the header to the bottom; its drawer panel (`top-0 bottom-0`) covers the full height.
- Tab bar and taskbar are both `h-9` rows with `px-3`; all inner controls are `h-6`:
  - text tabs `h-6 px-2.5`;
  - fixed gear settings tab `h-6 w-6 px-0`;
  - overflow button `h-6 w-6 px-0`;
  - agent launcher and agent tabs `h-6`;
  - account/quota controls were already `h-6`.
- The tab bar background changed from the undefined `--surface-color` to `ui-surface-title`; the taskbar uses the same solid surface.

### 2. Legacy token normalization (38 files)

Replaced every legacy reference with the Spec 13 tokens:

- `var(--surface-color)`, `var(--card-bg)`, `var(--bg-card)` → `var(--ui-bg-card)`;
- `var(--text-color)` → `var(--fg-color)`;
- `var(--bg-primary)` → `var(--ui-bg-base)`;
- `var(--status-warning-color|bg|border)` → `var(--status-warn-color|bg|border)`.

The mechanical pass covered the dialogue overlays, floating chat, log viewer, modals, project cards, taskbar/account controls, telemetry, shared UI controls (`AppSelect`, `AppModelEffortSelect`, `AppContextMenu`), Electronics/Logic/Oscilloscope views, GitHub panel, CAD/PCB/Design/Preview/Profiler/REA/Skills/Management/Task-detail/Terminal/Workspace components, and `AddProjectModal`. `grep` now returns zero references to the legacy names under `src/`.

### 3. Anchored popovers (Spec 13 §3.12)

Popover panels now use a solid elevated surface (`--ui-bg-title` / `ui-surface-title`) instead of the base window color:

- agent launcher (`AgentTaskbar.vue`), including `max-w-[calc(100vw-4rem)]`;
- Google account and DeepSeek wallet popovers (`AccountQuotaControls.vue`);
- header application menu (`AppHeader.vue`);
- `AppSelect` and `AppModelEffortSelect` dropdown panels;
- `AppContextMenu`;
- log-viewer “send to agent” dropdown (`RollingStatusLog.vue`);
- workspace tab-overflow menu (`ProjectWorkspace.vue`);
- project card header menu (`CardHeader.vue`).

### 4. Settings modal (`src/components/modals/SettingsModal.vue`)

- Modal container uses `ui-surface-base`; the ad-hoc `bg-black/30` sidebar, `bg-black/10` cards and `bg-black/20` footer are gone.
- Header is a single Omnisearch-style `.ui-search-box` row: `Settings` icon, title, `config.toml`/`rula.toml` badge, divider, accent `Search` icon, search input, live match counter, clear button, `ESC` key badge and close icon.
- `Escape` closes the modal from the window capture phase (nested controls included).
- Sidebar uses `ui-surface-list`; category buttons are `h-7 px-2.5 text-[10px]` with the accent left border.
- Content uses `p-4 gap-3`; parameter blocks use `ui-surface-card p-3`; labels are 11px, descriptions 10px and default hints 9px.
- Footer uses `.ui-modal-footer`; `Cancel`/`Save` are `h-6 px-2.5 text-[9px]`; the `Test` voice action is `h-8` and now uses `--ui-bg-base` / `--fg-color` instead of the undefined `--bg-primary` / `--text-primary`.

### 5. Projects drawer (`src/components/workspace/ProjectVerticalRail.vue`)

- Archived visibility button in the rail normalized from `w-8 h-7` to `w-8 h-8`.
- Header archived toggle and close button use `.ui-icon-btn`; close gains an `AppTooltip`.
- Drawer count/archived badges moved from `bg-white/5` to `bg-black/40`.
- Favorite stars use `--status-warn-color` (text and fill) instead of hardcoded `amber-400`.
- Search row spacing aligned to `py-2.5` with a 14px search icon.
- `Track New Project` footer fixed to `h-7`.

### 6. Living documentation

- `docs/specs/13_design_system_and_ui_components.md` bumped to **1.2.0**:
  - §1.2 declares the four `--ui-bg-*` tokens as the only valid surface tokens and prohibits the legacy names;
  - §3.6 adds tab-bar geometry (`h-9` row, `h-6` tabs/gear/overflow);
  - §3.7 adds the `h-6` modal-footer action invariant;
  - §3.10 defines the application shell geometry;
  - §3.11 defines the Settings modal contract;
  - §3.12 defines the anchored popover surface contract.
- `TECHNICAL_DEBT.md`: added **#38** (undefined legacy tokens, RESOLVED) and **#39** (`--accent-rgb` theme fallback, deferred).
- `TODO.md`: staged runtime checks for shell geometry, agent launcher opacity/stacking, Settings, drawer and token migration.

## Validation Required

No build, test, runtime process, or dev server was started by the agent (AGENTS.md §1.6). The self-managed watcher rebuilds the frontend.

Visual checks to run in the GUI:

1. Shell: rail to the bottom; agent taskbar starts at the tab bar's left edge and ends at the workspace's right border; docked chat touches the bottom; `h-9` bars with `h-6` controls; verify with chat docked, undocked and closed.
2. Agent launcher: open it upward from the taskbar and confirm the surface is opaque, the specialist list scrolls, and it paints above the tab content. Repeat for the Google/DeepSeek popovers and the header app menu.
3. Settings: open global settings, search and clear the query, verify the match counter, close with `ESC`, switch categories, check the parameter cards, `Test` voice, and `Save`/`Cancel`. Repeat in project scope.
4. Drawer: open with `Ctrl+B`, verify rail buttons, header icon buttons, favorite star color, search/`ESC`, archived badges, and the footer button height.
5. Token migration: confirm that previously invisible surfaces are now visible (bottom taskbar, chat composer, `AppSelect`/model select selected rows, Git/Profiler/Electronics cards, `AddProjectModal` panels) under the active Omarchy theme.

## Pending

- `--accent-rgb` remains a hardcoded fallback (Technical Debt #39); shadows currently use the teal fallback rather than the live accent.
- The blur/scrollbar runtime checks from the previous two devlogs remain pending and are tracked in `TODO.md`.
