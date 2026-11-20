# Devlog: Versioned Runner Specifications & Direct Argument Orchestration

**Date:** 2026-09-05  
**Author:** Bosmang (Lead Systems Architect & Supervisor)  
**Topic:** Versioned CLI Runner Specifications (`agy`), Model & Effort Parameter Matrix, and Herder Argument Passing

---

## 1. Context & Architectural Problem

Previously, agent parameters (such as `--model` or `--effort`) were passed through ad-hoc string concatenation or attempted via secondary `/model <value>` prompt injection into active PTY panes. This created several vulnerabilities:
1. **Fragile Startup Coordination:** Attempting to inject `/model` via `herdr agent prompt` after launch introduced race conditions with PTY readiness and TUI banner initialization.
2. **Unconstrained Model & Effort Capabilities:** Free-text input in the UI allowed invalid configurations (e.g. attempting to assign `medium` effort to `gemini-3.1-pro` which only supports `low` and `high`, or configuring effort on models with fixed reasoning like `claude-sonnet-4-6`).
3. **Hypothetical Raw Terminal Launch vs Herder Agent Registration:** An alternative proposal to bypass Herder's agent launcher using `herdr pane run "cd ... && agy ..."` was empirically evaluated. Probes revealed that raw terminal execution breaks Herder's agent catalog, causing subsequent MCP delegations and callbacks (`delegate_agent_task`, `report_task_status`) to fail immediately with `agent_not_found`.

---

## 2. Empirical Probes & Binary Verification

1. **Model & Effort Matrix:**
   Direct query to the local `agy` binary (`agy models` and `agy --help`) identified:
   - `gemini-3.8-flash`: supports `low`, `medium`, `high`.
   - `gemini-3.7-flash`: supports `low`, `medium`, `high`.
   - `gemini-3.6-flash`: supports `low`, `medium`, `high`.
   - `gemini-3.1-pro`: supports `low`, `high` (no `medium`).
   - `claude-sonnet-4-6`, `claude-opus-4-6-thinking`, `gpt-oss-120b-medium`: fixed thinking/effort (no `--effort` flag permitted).
   - Execution modes: `--mode accept-edits` auto-approves code modifications.
   - Permissions: `--dangerously-skip-permissions` auto-approves tool execution.

2. **Herder Argument Passing Probe:**
   Spawned an isolated test pane (`w8:p0`) and executed:
   ```bash
   herdr agent start testagent --kind agy --pane w8:p0 -- --model gemini-3.8-flash-low --effort low
   ```
   Herder 0.8.2 confirmed successful launch with exact `argv` passed:
   ```json
   "argv": ["agy", "--model", "gemini-3.8-flash-low", "--effort", "low"]
   ```
   The agent was fully registered in Herder's catalog with `agent_status: "idle"`.

---

## 3. Implementation Details

### A. Versioned Runner Specification (`templates/runners/agy.toml`)
Created specification version `0.2.1` adhering to the Pre-1.0 Build Number Convention:
- Flags mapping: `--model`, `--effort`, `--dangerously-skip-permissions`, `--mode accept-edits`.
- Exhaustive models array with explicit `effort_levels` and `default_effort`.

### B. Backend Engine & Persistence (`src-tauri/src/config.rs` & `src-tauri/src/kopeng/herdr.rs`)
1. **Schema Retrieval:**
   - Implemented `load_runner_schema` checking template paths with a resilient built-in fallback.
   - Exposed `get_agent_runner_schema(runner: String) -> Result<RunnerSchema, String>` to Tauri IPC.
2. **State & Input Types:**
   - Added `effort: Option<String>` and `accept_edits: Option<bool>` to `HerdrAgentState` and `AgentPersonaInput`.
   - Updated `compute_project_herdr_state` with a typed `ConfiguredAgent` struct parsing `effort` and `accept_edits` from `rula.toml`.
   - Updated `add_agent_persona` to persist `effort` and `accept_edits` to `[agents.<id>]` in `rula.toml`.
3. **Launch Pipeline (`launch_herdr_agent`):**
   - Assembles explicit CLI arguments:
     - `auto_approve` -> `--dangerously-skip-permissions`
     - `accept_edits` -> `--mode accept-edits`
     - `model` -> `--model <model>`
     - `effort` -> `--effort <effort>`
   - Passes all arguments cleanly via `herdr agent start ... -- <cli_args...>`.
   - Eliminated the transitional `/model` prompt injection workaround.

### C. Live Specification Synchronization
Updated `rula_schema.md` in both `templates/skills/kapawu-platform/` and `.agents/skills/kapawu-platform/` documenting the new `model`, `effort`, and `accept_edits` fields.

### D. Multi-Agent Delegation
Delegated frontend component integration for `AddPersonaModal.vue` to the `frontend` agent (`w8:pZ`) via `delegate_agent_task`.

### E. Frontend Component Integration (Frontend Specialist)
Completed Task `task-1788624040` integrating the versioned runner schema into `src/components/modals/AddPersonaModal.vue`:
1. **Schema Retrieval:** Added `loadRunnerSchema()` reactive query calling Tauri IPC `get_agent_runner_schema({ runner })` on mount and on runner selector changes.
2. **Model Selection:** Replaced the free-text `AppInput` with standardized brutalist `AppSelect`, dynamically mapping `runnerSchema.models` to selectable options with automatic custom model fallback preservation.
3. **Reasoning Effort Reactivity:** Added reasoning effort selector dynamically conditioned on `currentModelObj.effort_levels`. If configurable, renders `AppSelect` with available effort options (`Low`, `Medium`, `High`) and auto-syncs to `default_effort` or first valid tier; if fixed/unsupported (e.g. Claude or GPT-OSS), renders disabled input wrapped in `AppTooltip` (`"This model uses fixed reasoning effort (not configurable)."`).
4. **Auto-Accept File Edits Toggle:** Added standardized `AppToggle` for `--mode accept-edits` with clear subtitle description.
5. **State & Payload Synchronization:** Updated `HerderAgentState`, `emit('save')`, and initial value watchers to ensure `effort` and `accept_edits` propagate seamlessly to `add_agent_persona`.
6. **Build Verification:** Verified clean production compilation (`vue-tsc --noEmit && vite build`). Dispatched completion report to `bosmang` via `report_task_status`.

