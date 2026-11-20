# 2026-11-18 — Nucleus 2: Core UI Cleanup, AppTooltip, AppDatePicker & F5 Dispatcher

## Summary
Completed the core UI nucleus: viewport-aware `AppTooltip` repositioning (Debt #31), `color-mix()` replacement for `--accent-rgb` (Debt #39), custom brutalist `AppDatePicker` replacing native date inputs (`TODO.md` / Spec 39 §14), global `F5` context-refresh event dispatch (Debt #27 partial), and Spec 13 §11.3 token cleanup across shell and core workspace tabs.

## Changes
1. **Viewport-Aware `AppTooltip` (`src/components/ui/AppTooltip.vue`):**
   - Measures the teleported tooltip's `getBoundingClientRect()` after `nextTick()`, flips axis when overflowing viewport edges, and clamps `top`/`left` within an `8px` margin (`opacity-0` until positioned to prevent flicker).
2. **Custom `AppDatePicker` (`src/components/ui/AppDatePicker.vue`, `src/components/ui/index.ts`):**
   - Built a brutalist `YYYY-MM-DD` calendar popover using `Teleport`, `--ui-bg-title`/`--ui-bg-base`/`--accent-color` tokens, `Today`/`Clear` actions, and `Escape`/click-outside handling.
   - Replaced `<AppInput type="date" />` in `CreateProjectTaskModal.vue` and `ProjectTaskDetailPanel.vue`.
3. **Eliminated `--accent-rgb` Fallbacks:**
   - Replaced all 5 `rgba(var(--accent-rgb,0,255,200), ...)` glow expressions with `color-mix(in srgb, var(--accent-color) X%, transparent)` in `ProjectCard.vue`, `ProjectGitSnapshotsTab.vue`, `ProjectVerticalRail.vue`, and `ProjectStatusLeds.vue`.
4. **Spec 13 §11.3 Token & Tooltip Cleanup:**
   - Replaced `:title` with `<AppTooltip>` in `ProjectElectronicsTab.vue`.
   - Upgraded `text-[7px]` to `text-[8px]` in `AccountQuotaControls.vue`, `AgentTaskbar.vue`, `ProjectVerticalRail.vue`, and `ProjectManagementTab.vue`.
   - Replaced `amber-400` and `bg-black/*` with `--status-warn-color` and `.ui-surface-*` across `AppHeader.vue`, `AgentTaskbar.vue`, `ProjectVerticalRail.vue`, `ProjectOverviewTab.vue`, `ProjectSkillsTab.vue`, and `ProjectManagementTab.vue`.
5. **Global `F5` Dispatcher (`src/components/workspace/ProjectWorkspace.vue`):**
   - Intercepts `F5` across all workspace tabs (`preventDefault()`), refreshes `files`, and dispatches `oneship:context-refresh` for active tabs to adopt incrementally.
6. **Hygiene:**
   - Removed resolved Debt #31 and #39 from `TECHNICAL_DEBT.md` and updated #27; removed the completed `AppDatePicker` task from `TODO.md` and updated `docs/WORK_PLAN.md`.

## Verification
- Ran Spec 13 §11.3 targeted `rg` audit and `./scripts/run_dev.sh status` (read-only).
