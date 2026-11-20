# Development Log: Granular Tool Observability & Real-Time Chat Step Indicator

**Date:** 2026-09-17  
**Topic:** Granular Tool Parameters in Terminal View, Live `agent-step-update` Event Emission, and Transient Chat Step Indicator  
**Build Version:** 0.2.1 (Pre-1.0 Baseline)

---

## 1. Problem Context & User Request

Following the migration to the Universal Headless Multi-Turn `stream-json` protocol:
1. **Terminal Log Opacity:** The terminal log viewer only showed generic step tags (`[STEP] tool`, `[STEP] agent_response`) without indicating which tool was invoked or with what arguments.
2. **Missing Chat Feedback:** The Chat Overlay did not show real-time feedback of what the agent was doing during multi-step turns (reading files, executing commands, calling MCP tools, thinking).
3. The user requested:
   - Provide detailed tool names and parameters in the terminal log view.
   - Display a single, live, transient status indicator in the Chat Overlay reflecting the agent's current active step, automatically updating on each step and disappearing upon turn completion.

---

## 2. Implemented Architecture & Code Changes

### 2.1 Backend Granular Step Decoding (`src-tauri/src/supervisor.rs`)
- In `spawn_agent`'s stdout stream reader:
  - Intercepts and parses `step_update` events for `step_type == "tool"`, `"agent_response"`, `"thought"`, and `"result"`.
  - Formats explicit terminal ring buffer logs:
    - `[TOOL] <tool_name> <parameters_json>` on `ACTIVE`.
    - `[TOOL:DONE] <tool_name> (<duration>s)` on `DONE`.
  - Translates tool calls into concise human-readable summaries:
    - `view_file` -> `Reading file: <filename>`
    - `write_to_file` -> `Writing file: <filename>`
    - `replace_file_content` -> `Editing file: <filename>`
    - `list_dir` -> `Inspecting directory: <path>`
    - `run_command` -> `Running: <command>`
    - `call_mcp_tool (post_agent_response)` -> `Delivering response to dialogue channel...`
    - `call_mcp_tool (raise_initialization_flag)` -> `Emitting initialization flag: <type>`
    - `thought` -> `Thinking...`
    - `agent_response` -> `Drafting response...`
  - Emits the real-time Tauri event `agent-step-update` with `{ agent_id, project_path, state, step_type, tool_name, action_summary, timestamp_ms }`.

### 2.2 Frontend Store & Live Chat Indicator (`src/stores/dialogueStore.ts` & `src/components/dialogue/ConversationPanel.vue`)
- **`dialogueStore.ts`:**
  - Added `AgentLiveStep` interface and `activeSteps: Record<string, AgentLiveStep | null>` in store state.
  - Added `agent-step-update` listener in `initListener`.
  - Cleared `activeSteps[agent_id]` on `handleAgentResponse` and on turn result.
- **`ConversationPanel.vue`:**
  - Added `currentLiveStep` computed property.
  - Updated the live working indicator banner directly above the composer to display `currentLiveStep.actionSummary` with animated brutalist badge styling (`animate-ping` dot and truncated description).

---

## 3. Verification & Compliance Matrix

| Check | Target | Result |
|---|---|---|
| Rust Backend Check | `cargo check --bins` | Passed (0 errors, 0 warnings) |
| Rust Unit Tests | `cargo test --lib` | Passed (23/23 tests green) |
| Frontend TypeScript & Bundle | `npm run build` | Passed (0 errors) |
| Live Step Telemetry | `agent-step-update` events emitted | Verified |
