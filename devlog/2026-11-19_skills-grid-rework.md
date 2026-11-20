# 2026-11-19 — Skills Panel Dual View (Grid Default) & Spec 13 Rework

## Summary
Reworked `ProjectSkillsTab.vue` to conform to Spec 13 (§3.13, §4.1, §4.3, §4.5, §4.10) with a unified `SubtabBar` inline search, dual Grid/List view mode (Grid default), compact square cards, and normalized Explore & `SKILL.md` inspector layouts.

## Changes
- **Frontend (`src/components/workspace/ProjectSkillsTab.vue`):**
  - Moved search into `SubtabBar` `#center` (borderless inline filter for `Installed`, unified SkillsMP search + automatic `http(s)://` URL detection with inline skill-name input and `Install` trigger for `Explore`).
  - Removed the separate URL ingestion card from `Explore` in favor of the unified `#center` bar.
  - Added dual view toggle (`LayoutGrid` / `List` `.ui-icon-btn` controls in `#right`, defaulting to `grid` and persisted in `localStorage`).
  - Replaced long vertical cards with a responsive 1–4 column grid of compact square cards (`min-h-[168px]`, `ui-surface-card`) in Grid mode and compact horizontal rows in List mode across both `Installed` and `Explore`.
  - Standardized `Explore` URL ingestion card, quick-suggestion horizontal strip, `AppEmptyState` placeholders, semantic CSS tokens (removed `bg-white/5`, `border-white/5`, `bg-[var(--ui-bg-card)]/20`), and `h-9 ui-surface-title` header on the `SKILL.md` inspector pane.
- **Specification (`docs/specs/13_design_system_and_ui_components.md`):**
  - Added the `Skills panel (ProjectSkillsTab.vue)` contract to §3.13.

## Verification
- Targeted Spec 13 §11.3 grep audit on `ProjectSkillsTab.vue`: 0 hits.
- `./scripts/run_dev.sh status`: `vite`, `gui`, `daemon`, and `daemon_watch` active.
