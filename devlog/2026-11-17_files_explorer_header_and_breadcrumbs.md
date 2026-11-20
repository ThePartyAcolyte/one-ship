# 2026-11-17 — Files Explorer Header & Reader Breadcrumbs

## Summary

Standardized the Files workspace tab chrome. The explorer sidebar lost its redundant `EXPLORER` title and current-path label and now uses two flush rows on the panel surface: branch text plus collapse, then the search box with the filter trigger. The boxed branch selector became plain clickable text, the native filter checkboxes became `AppToggle` rows, the refresh button was removed in favor of `F5` while the Files tab is active, and the reader breadcrumbs now follow folder navigation when no file is open instead of printing `No file selected`.

## Changes Implemented

### 1. Explorer header (`ProjectWorkspace.vue`)

- Replaced the `p-2 bg-black/20` card with two rows on `ui-surface-title`:
  - Row 1 (`h-9`): plain branch text (`GitBranchSelector`) and the collapse action pinned right; no panel title, no path.
  - Row 2: `.ui-search-box` at full width with the filter trigger (`Filter` icon) to its right; active filters keep the accent border plus a corner dot.
- Search input raised from `text-[9px]` to `text-[10px]` in a `h-6` row; the clear button is symbol-only.
- Removed the refresh button; `F5` in `handleKeydown` calls `refreshExplorerFiles()` only while `workspaceTab === 'files'`, so Forge's own `F5` handler is unaffected.
- The filter popover keeps its anchored placement and uses `AppToggle` for `This Chat` and `Documentation (.md)`.

### 2. `GitBranchSelector.vue`

- Dropped the `compact` prop and the boxed button. The component is now plain text (branch icon, truncated bold name, chevron) that opens `GitBranchModal`; hover uses the accent color.

### 3. Reader breadcrumbs (`ProjectWorkspace.vue`)

- `breadcrumbSegments` uses the open document path when one exists and falls back to `explorerPath` otherwise; folder-only navigation renders every explored segment.
- The trailing segment is bold: the current folder when nothing is open, the open file title when a document is loaded. `No file selected` is gone.
- Separators render between segments instead of trailing after each one.
- The reader topbar is now `h-9` on `ui-surface-title` to match the panel bar geometry; the expand button (collapsed explorer) and the Read/Edit switch are unchanged.

## Specs Updated

- `13 §3.13` — Files tab header contract (two rows, plain branch text, search + filter trigger, `F5`, breadcrumb fallback).

## Verification

No build or runtime process was started (AGENTS.md §1.6). Pending manual check: branch text and modal, filter popover toggles, `F5` refresh with focus in the search box, breadcrumb while navigating folders without a file, breadcrumb after opening a file, collapsed/expanded sidebar.
