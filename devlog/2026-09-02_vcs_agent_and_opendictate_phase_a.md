# Devlog: VCS Agent Action & OpenDictate Voice Mode (Phase A)
**Date:** 2026-09-02  
**Topic:** Git Status Dual-Signification, OpenDictate Reserved Session Integration, and VCS Agent Action Sheet

## Overview
Replaced the ambiguous text-based git diff ("20 pending") with a structured `GitStatus` parser in `src-tauri/src/kopeng/git.rs` and dual-signification iconography in `src/App.vue`. Added OpenDictate Model 1 session reservation support and a pure minimalist, color-driven Voice Mode toggle in the global switches area.

## Key Changes
1. **Backend (`kopeng::git`, `hydration.rs`)**:
   - Implemented `GitStatus` (`branch`, `has_remote`, `is_clean`, `has_commits`, `staged`, `modified`, `untracked`, `ahead`, `behind`).
   - Replaced raw string counts with exact porcelain parsing.
2. **OpenDictate & Diagnostics (`kopeng::diagnostic`, `kopeng::opendictate`)**:
   - Added `/tmp/opendictate.socket` and D-Bus status checks to the system diagnostic suite.
   - Created `reserve_voice_capture` to dispatch Model 1 (`ReserveCaptureSession`) asynchronous capture requests with unique UUIDs.
3. **Frontend UI (`src/App.vue`)**:
   - Git Status display: Icon shows topology (`GitCompare` if remote tracking exists, `GitBranch` if local-only). Color shows health (green=clean, amber=dirty, grey=unborn). Compact symbolic badges (`! N`, `? N`, `+ N`, `↑ N`, `↓ N`).
   - Voice Mode Switch: Located in the bottom-right action area directly above the Diagnostic button. Minimalist, purely icon-and-color driven (`<Mic :size="20" />`), zero text.
   - VCS Action:
     - **Voice Mode ON:** Zero modals. Card's VCS button enters non-blocking listening state (`[LISTENING]`) while reserving an OpenDictate turn.
     - **Voice Mode OFF:** Opens a focused VCS Agent Action modal with prompt input and quick action shortcuts (`+ Auto-Commit`, `+ Commit & Push`).
     - Reaches message formulation stage ready for Herder background dispatch.
4. **Live D-Bus Signal Listener & Cyclic Logging**:
   - Implemented `src-tauri/src/logger.rs` with automatic file size rotation (500KB cap, max 3 files) in `~/.config/omarchy-project-tracker/logs/kapawu.log`.
   - Spawns background thread running `gdbus monitor --session --dest org.kirulab.OpenDictate` capturing `SessionCancelled` and `SessionFinished` signals in real time.
   - Replaced floating popup notifications with a subtle 5-line rolling FIFO terminal in the bottom-left corner.
   - Clicking a listening project's mic icon calls `ReleaseReservedSession` for manual cancellation directly from Kapawu.
5. **Split Action Presets & Watch Reload Fix**:
   - Fixed involuntary modal closure when saving `rula.toml` by ignoring `rula.toml`, markdown, and git directories in `vite.config.ts`.
   - Replaced mock action buttons with split buttons for each preset in `rula.toml`:
     - Left part (name): Inserts prompt into instruction textarea.
     - Right part (`»` / `ChevronsRight`): Performs instant dispatch directly to the status buffer and disk logs without extra clicks.

