# Work Session Log: Deterministic Terminal Focus Resolution

**Date:** 2026-09-05  
**Status:** Completed  
**Domain:** Process Orchestration & Hyprland IPC  

---

## 1. Context & Problem Statement

When launching or restarting an agent via Kapawu, the auto-focus routine was intermittently focusing unrelated desktop applications on different workspaces (e.g. Zen Browser on workspace 11) instead of the active terminal emulator hosting Herder.

### Root Cause
1. **Unconstrained Substring Search:** `detect_and_focus_terminal_window()` checked `c.title.to_lowercase().contains("herdr")` across all Hyprland clients without first verifying that the client was a terminal emulator. A browser tab reading Herder documentation triggered an immediate match and focus shift.
2. **Class Mismatch in Omarchy:** In Omarchy, Ghostty is launched with `--class=TUI.tile -e herdr`. The initial check looked for `c.class == "ghostty"`, failing to match and dropping into the flawed fallback.

---

## 2. Implementation

Refactored `detect_and_focus_terminal_window()` in `src-tauri/src/kopeng/herdr.rs`:

1. **Strict Terminal Verification Guard (`is_terminal_client`):**
   - Explicitly rejects known non-terminal applications (`zen`, `firefox`, `chromium`, `google-chrome`, `brave`, `code`, `wang-kapawu`).
   - Validates terminal classes (`TUI.tile`, `ghostty`, `foot`, `kitty`, `alacritty`, `wezterm`, `xterm`, `org.omarchy.agent`, `org.omarchy.terminal`) and the `terminal*` Hyprland tag.
2. **Deterministic PID Discovery (`get_herdr_client_parent_pids`):**
   - Scans `/proc` for running `herdr` client instances (excluding daemon `server`).
   - Retrieves the parent PID (`PPid`) of the Herder client, which corresponds directly to the terminal emulator hosting it.
   - Matches against `c.pid` of the Hyprland client for an exact 1-to-1 match.
3. **Controlled Fallbacks:**
   - Fallback to verified terminal clients where `initial_title == "herdr"` or `title == "herdr"`.
   - Fallback to active `preferred_terminal` or `TUI.tile` terminal client.
   - Cold spawn fallback if no terminal window is present.

---

## 3. Verification

- `cargo check`: Clean (0 errors, 0 warnings).
- `npm run build`: Clean.
