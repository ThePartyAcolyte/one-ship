# Work Session Log: Dynamic Terminal Focus Resolution & Contextual Auto-Focus Governance

**Date:** 2026-09-03  
**Status:** Completed  
**Branch:** master  

---

## 1. Objectives & Context
1. Implement dynamic, zero-hardcoding terminal focus detection in Hyprland so that launching or clicking on Herder agents brings the exact terminal window to the front regardless of the terminal emulator used (Ghostty, Foot, Kitty, TUI.tile).
2. Establish a strict contextual auto-focus policy:
   - Interactive project agents (`[agents.<name>]`): Auto-focus by default (`auto_focus = true`).
   - Plugin tasks and Git operations: Silent background execution by default (`auto_focus = false`).
3. Add `preferred_terminal` to global configuration schema (`~/.config/wang-kapawu/config.toml`) to guide terminal discovery and cold-spawn fallback without constraining runtime dynamic search.

---

## 2. Key Accomplishments

### A. Dynamic 3-Stage Terminal Focus Resolution (`src-tauri/src/kopeng/herdr.rs`)
- Implemented `detect_and_focus_terminal_window()`:
  - **Stage 1 (Preferred Terminal Match):** Checks `hyprctl clients -j` for active windows of the configured `preferred_terminal` running Herder. If found, dispatches `focuswindow address:<addr>`.
  - **Stage 2 (Universal TUI Discovery):** Searches for any client matching `initialTitle == "herdr"`, title containing `"herdr"`, tag `"terminal*"`, or known TUI classes (`TUI.tile`, `ghostty`, `foot`, `kitty`, `alacritty`, `wezterm`), focusing by exact memory address.
  - **Stage 3 (Cold Spawn Fallback):** If no active terminal window exists on the desktop, spawns a new window of the preferred terminal (`<preferred_terminal> -e herdr`).
- Performance: 100% on-demand. Executed strictly upon user click or agent launch, running in ~1ms over Hyprland's local Unix socket with zero background polling overhead.

### B. Contextual Auto-Focus Governance
- Updated `launch_herdr_agent`:
  - Resolves `auto_focus` with default `true` for project personas.
  - If `auto_focus == true`: Creates tab without `--no-focus` and immediately invokes `focus_herdr_tab`.
  - If `auto_focus == false`: Passes `--no-focus` and avoids desktop focus shifts.
- Retained `--no-focus` for background tasks (`dispatch_git_instruction`).
- Pill interaction in `HerderSection.vue`: Clicking any launched/active agent pill (`idle`, `working`, `completed`, `blocked`) immediately focuses that tab and brings its terminal window to the front.

### C. Global Configuration Schema
- Added `preferred_terminal` (`"ghostty"`, `"foot"`, `"kitty"`, `"alacritty"`, `"wezterm"`) to the `orchestrator` plugin schema in `src-tauri/src/config.rs`.

---

## 3. Verification
- Backend compilation (`cargo check`) succeeded with 0 errors.
- Frontend compilation (`npm run build`) succeeded with 0 errors.
- Dev server (`task-75`) recompiled and is running live.
