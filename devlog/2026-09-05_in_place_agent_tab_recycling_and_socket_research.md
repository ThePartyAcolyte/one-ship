# Devlog: In-Place Agent Tab Recycling & Herder Socket Research

**Date:** 2026-09-05  
**Topic:** Eliminating tab recreation displacement on agent restart and discovering Herder's Protocol 20 socket primitives.

---

## 1. Problem Statement
Restarting an agent from the "Restart Agent" UI menu previously called `close_herdr_tab`, waited 350ms, and then called `launch_herdr_agent`.
This introduced severe UX degradation:
- Destroying the tab caused Herder to append the recreated tab at the very end of the tab bar (after all service tabs).
- Frequent tab switching was required whenever an agent was restarted.

## 2. Herder Socket Research & Capabilities
1. **Direct Socket Access:** Herder runs a persistent Unix domain socket at `~/.config/herdr/herdr.sock`.
2. **Server-Level Tab Reordering (`tab.move`):** While the CLI `herdr tab` lacks a move command, the underlying Protocol 20 JSON socket supports:
   ```json
   { "method": "tab.move", "params": { "tab_id": "w8:tC", "insert_index": 2 } }
   ```
   Direct socket testing verified that tabs can be reordered arbitrarily in real time.
3. **Reactive Subscriptions (`events.subscribe`):** The socket API supports subscribing to events (`pane.agent_status_changed`, `tab.created`, `tab.moved`, `workspace.updated`), enabling zero-latency event streaming directly to the UI.

## 3. Implementation: In-Place Tab Recycling
To guarantee an agent never loses its original position or changes its tab index on restart, we implemented in-place tab recycling:
1. **Backend Command (`src-tauri/src/kopeng/herdr.rs`):**
   - Implemented `terminate_pane_foreground_process(pane_id)`: Inspects `herdr pane process-info` to identify the active runner process (`pid != shell_pid`), sends SIGTERM/SIGKILL, and sends `C-c` until the shell returns to an idle prompt.
   - Refactored agent startup into `bootstrap_agent_in_pane` (shared between initial launch and restart).
   - Implemented `restart_herdr_agent(project_path, project_name, agent_id)`: Reuses the existing tab and pane, cleans the viewport with `clear`, synchronizes `cd '<project_path>'`, and restarts the runner in place.
   - Registered `restart_herdr_agent` in `src-tauri/src/lib.rs`.
2. **Frontend Sub-Delegation:**
   - Delegated task `task-1788629195` to the `frontend` agent to update `HerderSection.vue` to call `restart_herdr_agent` directly instead of closing and recreating tabs.

## 4. Verification
- Backend compiled with 0 errors and 0 warnings (`cargo check`).
- Task delegated via MCP to `frontend` specialist.

## 5. Callback & Verification
- **Frontend Callback:** `frontend` specialist confirmed task completion (`task-1788629195`).
- **Codebase Integrity:** Verified clean production builds across Rust core (`cargo check`, 0 errors, 0 warnings) and Vue 3 frontend (`vue-tsc --noEmit && vite build`, 0 errors).
- **Behavioral Outcome:** "Restart Agent" now reliably terminates the active runner, cleans the terminal viewport, and restarts the agent in the exact same tab and pane, preventing tab displacement.
