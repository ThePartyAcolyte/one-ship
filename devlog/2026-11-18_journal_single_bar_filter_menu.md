# 2026-11-18 — Journal Single Bar with Inline Search & Filter Menu

## Summary

Corrected the Journal header after review: the search no longer occupies a separate full-width row and the filter chips were replaced by an anchored dropdown menu. Tabs, inline search, filter trigger and actions now share one `h-9` `SubtabBar`.

## Changes Implemented

### `SubtabBar.vue`

- Added an optional `center` slot (`flex-1`, no border/surface) between the tabs and the right tools, so panels can integrate an inline control in the same bar without a second row.

### `DevlogJournalView.vue`

- Removed the standalone `.ui-search-box` row and the standardized chip row.
- The bar center now holds a borderless inline search (`Search` icon, transparent `AppInput`, clear button) that fills the remaining width.
- Added a `Filter` icon button at the end of the inline search with an accent state and dot when any filter is active. It opens an anchored dropdown (`ui-surface-card`, `w-56`, `max-h-80`) with contextual groups:
  - Devlogs: Agent.
  - Tasks: Status, Agent (when agents exist), Priority.
  - Debt: Status, Severity.
- Selecting an option applies it and closes the menu; outside `pointerdown`, tab switches and the document listener close it.
- Removed `filterChipClass` and all chip markup.

## Specs Updated

- `13 §3.13` — `SubtabBar` center zone and the Journal single-bar contract.

## Verification

No build or runtime process was started (AGENTS.md §1.6). Vite applied the HMR updates at 02:03 and reported no transform errors. If the running window still shows the previous layout, reload the webview (Ctrl+R) from the app; the agent does not restart the GUI.
