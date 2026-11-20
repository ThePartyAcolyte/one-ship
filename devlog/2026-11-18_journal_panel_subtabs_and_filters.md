# 2026-11-18 — Journal Panel: SubtabBar, Shared Search & Standardized Filters

## Summary

Standardized the Journal workspace tab (`DevlogJournalView.vue`): the three record families now live in one `SubtabBar` with counts as badges and the action icons in its right slot; the per-tab filter headers were collapsed into one shared `.ui-search-box` plus a single non-wrapping filter-chip row. Cards, creation forms and empty states were re-tokenized.

## Changes Implemented

### Tabs and actions

- Replaced the custom boxed tab strip with `SubtabBar` (`Devlogs | Tasks | Tech Debt`), using `journalTabItems` with `devlogCount`, `pendingTasksCount` and `openDebtCount` as badges.
- `+` (Tasks/Debt) and refresh moved to the `right` slot as `.ui-icon-btn` controls with tooltips.

### Search and filters

- One `.ui-search-box` row (`h-6`, accent icon, `text-[10px]`, clear button) with a per-tab placeholder (`Filter devlogs…`, `Filter tasks…`, `Filter technical debt…`).
- One contextual chip row shared by the three tabs:
  - Devlogs: `Agent`.
  - Tasks: `Status`, `Agent` (only when agents exist), `Priority`.
  - Tech Debt: `Status`, `Severity`.
- Chips standardized to `h-6 px-2 border text-[9px] uppercase`, accent border/fill when active, horizontal scroll instead of wrapping (`filterChipClass`). The old `Devlog Agent Filter Bar`, `Filter and Form Header` and debt filter header blocks were removed.

### Surfaces and typography

- Journal cards, task items, debt cards, and the task/debt creation forms use `ui-surface-card`; `bg-black/*` surfaces removed from these areas.
- Body text raised to `text-[10px]`, metadata to `text-[8px]`; pills use `text-[8px] py-0.5`; `Reopen`/`Resolve` actions are `h-6 px-2 text-[9px]`.
- Forms use `!h-6` inputs and `compact` selects; Cancel/Save/Record actions are `h-7 text-[9px]`.
- Empty states remain inline but use 20 px icons, `text-[10px]` titles and the current tokens.

## Specs Updated

- `13 §3.13` — Journal panel chrome contract.
- `22 §3.1` — sub-tabs/search/filter facets are display-only.

## Verification

No build or runtime process was started (AGENTS.md §1.6). Vite applied the HMR updates without errors. Pending manual check: badge counts, per-tab filters combined with search, `+` forms, and horizontal chip scrolling on narrow widths.
