# 2026-11-17 — Scripts Hub Chrome Cleanup

## Summary

Cleaned the Scripts workspace tab (`ProjectTerminalGrid.vue`): removed the duplicated rail title, dropped the host-terminal (Ghostty) launcher, kept History and Shell, moved the filter to the standardized `.ui-search-box` row, and aligned both pane headers with the `h-9 ui-surface-title` panel-bar contract. No daemon or behavior changes beyond the removed launcher.

## Changes Implemented

### Rail (`ProjectTerminalGrid.vue`)

- Removed the `Terminal` icon, `SCRIPTS & TERMINAL` label, and script-count badge.
- Header is now a title-less `ui-surface-title` block: `h-9` row with only `History` and `+ Shell` (`h-6 px-2 text-[9px]`), then a full-width `.ui-search-box` row (`h-6`, accent icon, `text-[10px]`, `X` clear button).
- Removed the host-terminal button and its frontend plumbing (`preferredTerminal`, `loadPreferredTerminal`, `openHostTerminal`, `isSpawningTerminal`, `ExternalLink` import). The `open_system_terminal` daemon command remains registered for other clients.
- Added the missing `X` icon import (also used by the port-conflict modal).

### Console header

- Normalized to `h-9 px-3 ui-surface-title` without wrapping: target dot/name/status/command on the left.
- `ALL`/`ERRORS`, `AUTO`, and `SIGINT` are `h-6 px-2 text-[9px]`; clear and copy use `.ui-icon-btn`.

## Specs Updated

- `01 §3.2` — header actions and filter placement, host launcher removal.
- `13 §3.13` — Scripts Hub chrome contract.

## Deferred

Git & Snapshots requires a functional phase, not only styling: staging → commit flow, consolidated snapshot/commit filters, default Git agent selection under **Project Settings → Git & VCS**, and a clear split between `Commit & Sync` and `Dispatch`. Electronics remains unreviewed.

## Verification

No build or runtime process was started (AGENTS.md §1.6). Pending manual check: title-free rail, filter box and clear button, History/Shell actions, console controls, and that no host-terminal action is reachable from the tab.
