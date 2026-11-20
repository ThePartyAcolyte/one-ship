# Devlog: Fullscreen Log Viewer Copy-on-Select & Send-to-Agent Dispatch

**Date:** 2026-09-05  
**Topic:** Log Viewer Copy-on-Select & Send-to-Agent Dispatch  
**Status:** Completed

---

## 1. Context & Objectives
In the fullscreen system log viewer (`RollingStatusLog.vue`), manual selection required pressing `Ctrl+C` to copy, without visual feedback or any direct bridge to background agents in Herder.
The user requested:
1. **Option A (Copy-on-Select):** Automatically copy any selected log text to the system clipboard upon mouse release (`mouseup`).
2. **Option B (Send-to-Agent):** Enable sending the selected log snippet directly to active agents in Herder as debugging context or instructions.

## 2. Changes Implemented

### A. Backend IPC Command (`src-tauri/src/kopeng/herdr.rs` & `src-tauri/src/mcp.rs`)
- Made `resolve_herdr_target(target: &str) -> String` public in `src-tauri/src/mcp.rs` to allow resolving agent names (`bosmang`, `frontend`, etc.) to active Herder pane IDs.
- Implemented Tauri command `prompt_herdr_agent(target: String, prompt: String)`:
  - Resolves target to pane ID.
  - Executes `herdr agent prompt <target> <prompt>`.
  - Registered in `src-tauri/src/lib.rs`.

### B. Copy-on-Select Engine (`src/components/floating/RollingStatusLog.vue`)
- Added `@mouseup="handleLogSelection"` on `logContainer` with `select-text` styling.
- Upon non-empty text selection:
  - Writes to system clipboard via `navigator.clipboard.writeText(text)`.
  - Renders a subtle green header badge: `COPIED <N> CHARS` with auto-dismiss after 2.5s.
  - Ignores simple clicks and collapses to avoid clearing or corrupting the clipboard.

### C. Floating Action Pill & Agent Dropdown (`#log-selection-pill`)
- Positions a floating action pill near the selection coordinates:
  - Automatically calculates bounding rect with viewport boundary safety (flips above if near bottom border, shifts left if near right window edge).
  - Displays `[✓ COPIED]` confirmation.
  - Displays `[SEND TO AGENT ▾]` interactive button.
  - Dropdown lists all active agent personas across projects (`bosmang`, `frontend`, `mcp`, etc.) with role and status badges.
  - On selecting an agent, dispatches prompt: `[LOG SNIPPET from Wang-Kapawu Log Viewer]\n\`\`\`\n<text>\n\`\`\``.
  - Displays confirmation badge: `SENT TO <AGENT>` in the header and status log.

## 3. Verification
- `cargo check --manifest-path src-tauri/Cargo.toml` compiled cleanly with zero errors.
- `npm run build` (`vue-tsc --noEmit && vite build`) compiled cleanly with zero errors.
- Adheres to brutalist design principles and Rule 5 (boundary-safe floating elements).
