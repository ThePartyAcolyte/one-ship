# Work Session Log: Herder Tab Reuse Invariant, Process Safety Guard & State Normalization

**Date:** 2026-09-03  
**Status:** Completed  
**Branch:** master  

---

## 1. Problem Statement & Root Cause
During agent launch, if a project workspace already existed with a single tab (e.g. Tab "1" running a user test server or dev command):
1. `launch_herdr_agent` evaluated `if ws_tabs.len() == 1` and unconditionally hijacked the tab, renaming it to the requested agent ID.
2. The orchestrator injected `cd` and `herdr agent start` directly into the pane while the user's dev server was executing in the foreground, contaminating standard input.
3. `herdr agent start` failed/rejected, but the tab retained the agent label.
4. `get_project_herdr_state` matched the tab by label and reported Herder's `agent_status: "unknown"` as active, causing the frontend UI to display `[IDLE]`.
5. Subsequent clicks were blocked by the guard `if agent.status != "unlaunched"`, locking the user out of starting the agent.

---

## 2. Architectural Solution & Changes

### A. 4-Point Tab Reuse Invariant (`launch_herdr_agent`)
- Confined tab reuse strictly to workspaces created *in the same invocation* (`is_brand_new_workspace == true`).
- A tab is reused only if:
  1. Workspace was freshly created in this call.
  2. Workspace has exactly 1 tab.
  3. Tab label is strictly the default initial label (`"1"`).
  4. Pane has no running foreground processes.
- For all other cases, existing user tabs are untouched, and dedicated tabs are created cleanly.
- If a dedicated tab for the agent already exists, it is reused directly.

### B. Foreground Process Safety Guard (`herdr pane process-info`)
- Implemented `get_pane_busy_process(pane_id)` querying `herdr pane process-info --pane <pane_id>`.
- Verified `foreground_processes` and `foreground_process_group_id` against `shell_pid`.
- If any command other than the base shell is active, agent launch immediately halts with an explicit descriptive error, protecting running dev servers, interactive tools, and builds.

### C. Agent Lifecycle Normalization (`get_project_herdr_state`)
- An agent is considered launched ONLY if Herder reports an active lifecycle (`"idle"`, `"working"`, `"blocked"`, `"done"`) or `p.agent` is present.
- If a tab exists but reports `agent_status: "unknown"`, the agent is normalized to `"unlaunched"`.
- Tab metadata (`tab_id`, `pane_id`) is preserved, allowing the UI to offer `+Launch` and bootstrap the agent inside the existing tab without opening duplicates.

### D. Git Agent Bootstrap in `dispatch_git_instruction`
- If the `"git"` tab exists but the agent inside is unlaunched, `dispatch_git_instruction` starts the agent process and verifies the pane is idle before prompting.

### E. UI Context Menu & Inactive Tab Polish (`HerderSection.vue`)
- Added `isAgentRunning(agent)` helper to strictly exclude `"unlaunched"` and `"unknown"`.
- When an unlaunched agent has an open tab (`tab_id`), the right-click context menu displays "Focus Tab" and "Close Tab" alongside "Launch <id>", providing direct control over orphan tabs.

---

## 3. Verification & Build
- Backend check (`cargo check`): 0 errors, 0 warnings.
- Frontend build (`npm run build`): 0 errors, built in 1.37s.
- Spec synchronization: Updated `docs/specs/08_herder_agent_orchestration.md`.
