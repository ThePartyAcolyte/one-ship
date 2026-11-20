# Development Log: Chat Close Lifecycle, Active Taskbar Synchronization, and Flag Resolver Scoping

**Date:** 2026-09-24  
**Topic:** Atomic termination of agent chat sessions, active taskbar process filtering, and initialization flag resolver persona scoping.  
**Build Version:** 0.2.1 (Pre-1.0 Baseline)

---

## 1. Problem Statement & Root Cause Analysis

### A. Zombie Active Indicators on Closed Chat Windows
When concluding an on-demand initialization flag resolution session (e.g., `profiler-crafter` or `docs-resolver`):
- The agent executed its concluding tool invocation (`resolve_initialization_flag`), leaving an active step in the store (`state: 'ACTIVE'`).
- Clicking the close button (`X`) on `FloatingChatOverlay` closed the modal and stopped the supervisor child process via `stop_agent`.
- However, `closeAndKillAgent` in `dialogueStore.ts` never cleared `this.activeSteps[agentId]`.
- In `AgentTaskbar.vue`, `isAgentProcessActive(agentId)` used a fallback to `dialogueStore.activeSteps[lower].state === 'ACTIVE'` when the agent was no longer in `st.agents`. Because the step state remained `'ACTIVE'`, `isAgentProcessActive` evaluated to `true`, retaining the tab in `activeConversations` and rendering a blinking green dot (`animate-ping`).
- Reopening the chat showed the status as `unlaunched` in the header, while the live telemetry banner below it reported an ongoing active step.
- Additionally, closing the tab directly from the bottom taskbar (`handleCloseTab`) did not clear `activeSteps` and duplicated teardown logic.

### B. Resolution Agents Leaking into Project Specialists
Flag resolution agents (`docs-resolver`, `techdebt-resolver`, etc.) lingered in the available project agents list even after their corresponding initialization flags had been resolved:
- In `estudiorc/rula.toml`, `[agents.docs-resolver]` and `[agents.techdebt-resolver]` were declared during previous runner testing.
- `compute_project_state` (Step 2) in `supervisor.rs` blindly added all entries in `[agents]` from `rula.toml` as permanent domain specialists (`is_internal: false`).
- Step 5 of `compute_project_state` iterated `self.processes` and added dynamic processes even if their status was `Completed` or `Failed`.
- In `AgentTaskbar.vue`, `isEphemeralOrInit` only checked `lower.endsWith('_resolver')` (with an underscore), ignoring canonical kebab-case resolvers (`docs-resolver`) and crafters (`profiler-crafter`).

---

## 2. Implemented Architecture & Solutions

### 2.1 Backend Supervisor & MCP Telemetry (`src-tauri/src/supervisor.rs` & `mcp.rs`)
- **Flag Resolver Scoping (`is_flag_resolution_agent`):**
  - Added `pub fn is_flag_resolution_agent(agent_id: &str) -> bool` matching `-resolver`, `_resolver`, `-crafter`, `_crafter` and canonical resolver IDs.
  - Step 2 in `compute_project_state` skips any agent matching `is_flag_resolution_agent`. Declared resolution configs in `rula.toml` now act strictly as configuration templates for runner/model overrides rather than creating phantom domain specialists.
  - Step 4 continues to dynamically discover and inject resolution agents only while their SQLite initialization flag is `pending` or `in_progress`.
- **Strict Active Process Discovery (Step 5):**
  - Enforced `matches!(st, ProcessStatus::Working | ProcessStatus::Starting | ProcessStatus::Idle | ProcessStatus::Blocked)` for any process discovered via `self.processes`, preventing terminated processes from appearing in `state.agents`.
- **Atomic Process Termination Events:**
  - `stop_process` now emits `agent-status-report` (`unlaunched`) and `agent-step-update` (`DONE`) when stopping an agent process.
  - In `spawn_agent`, the Tokio `child.wait()` block emits `agent-status-report` and `agent-step-update` (`DONE`) upon child exit (both for `Ok(code)` and `Err(e)`).
- **Flag Resolution Event Emission:**
  - `execute_resolve_initialization_flag` in `mcp.rs` emits `init-flag-resolved` and `agent-step-update` (`DONE`) when a flag is updated in SQLite.

### 2.2 Frontend Dialogue Store (`src/stores/dialogueStore.ts`)
- **Centralized `closeAndKillAgent(projectPath?, specificAgentId?)`:**
  - Immediately nullifies `this.activeSteps[targetAgentId.toLowerCase()]`.
  - Closes floating chat if open for that agent.
  - Optimistically updates `coreStore.agentStates`: removes ephemeral/resolution agents from `st.agents` if present, or marks permanent agents as `unlaunched`.
  - Dispatches `stop_agent` to the backend.
- **`init-flag-resolved` Listener:**
  - Automatically resets active steps for the resolving agent, refetches flags, and dispatches `kapawu-refresh-project`.
- **Safe Step Completion:**
  - `agent-step-update` clears `activeSteps` on any `state === 'DONE'` transition.

### 2.3 Frontend Taskbar UI (`src/components/taskbar/AgentTaskbar.vue`)
- **Expanded Ephemeral Classifier:**
  - `isEphemeralOrInit` now handles `-resolver`, `_resolver`, `-crafter`, and `_crafter`.
- **Canonical Process Activity Check:**
  - Removed stale `activeSteps` fallback in `isAgentProcessActive`. Process activity is derived strictly from canonical supervisor process state in `coreStore`.
- **Unified Tab Close:**
  - `handleCloseTab` delegates directly to `dialogueStore.closeAndKillAgent`.

### 2.4 Configuration Cleanup
- Removed residual `[agents.docs-resolver]` and `[agents.techdebt-resolver]` entries from `/home/butcherwutcher/Projects/dev/estudiorc/rula.toml`.

---

## 3. Verification & Compliance Matrix

| Check | Target | Result |
|---|---|---|
| Rust Backend Check | `cargo check` | Passed (0 errors, 0 warnings) |
| Rust Unit Tests | `cargo test kopeng` | Passed (22/22 unit tests green) |
| Frontend Typecheck & Build | `npm run build` (`vue-tsc --noEmit && vite build`) | Passed (0 errors, 0 warnings) |
| Living Specification | `docs/specs/08_native_process_supervisor.md` | Updated Section 2.3 |
