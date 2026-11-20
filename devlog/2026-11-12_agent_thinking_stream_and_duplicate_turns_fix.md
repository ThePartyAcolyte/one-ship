# Devlog: Agent Live Thinking Stream & Duplicate Chat Turns Fix

**Date:** 2026-11-12  
**Topic:** Bugfix & Ergonomics — AGY / Multi-Runner Live Thinking Stream, Orphan Process Unblock & Zero Duplicate Chat Turns

---

## 1. Context & Problem Statement

During an interactive conversation with the `editorial` agent in project `Kirulab Website` (`conv-18-editorial-4b93cacd`), several issues were observed:
1. **Agent Hanging / Indeterminate State**: After the user asked whether generated files were located in the project repository or in the runner's internal memory/antigravity cache, the agent remained in a perpetual `Working` state. No response was delivered to the chat overlay or approval box.
2. **Missing Live Thinking / Reasoning Stream**: The user observed that despite the model executing reasoning cycles in the background, the transient thinking bubble situated above the action/tool badge in `FloatingChatOverlay` did not display the agent's real-time thoughts or reasoning text across cycles.
3. **Duplicate User Messages in Chat History**: Every user prompt sent via speech-to-text dictation or chat input was recorded twice in `agent_chat_turns` with identical timestamps (e.g. turns 610/611, 613/614, and 616/617).

---

## 2. Root Cause Analysis

### A. Runaway Background Search & Missed Turn Completion
- When asked to verify file locations, the `editorial` agent spawned:
  ```bash
  cd "/home/butcherwutcher/Projects/dev/Kirulab Website"; pwd; ls -la docs ...; find / -name '04_linea_editorial_v2.md' 2>/dev/null
  ```
- The recursive search `find /` attempted to scan the entire Linux root filesystem. After exceeding the synchronous timeout (8000ms), `agy` shifted the command into a background task (`task-54`, PID 3790473), which continued executing for >20 minutes.
- The model produced a text explanation (Step 55) confirming the files existed in `docs/`, but outputted plain text rather than calling the MCP tool `post_agent_response`.
- Because the background `find /` task was still running, `agy` never emitted the terminal `"result"` event. Consequently, the Process Supervisor's fallback mechanism (`persist_fallback_agent_response`) never triggered, keeping the process marked as `Working` and withholding the message from the user.

### B. Missing Live Thought Stream for AGY Runner
- In `src-tauri/src/supervisor.rs` (lines 3109–3151), the `is_agy` event handler exclusively expected `step_type: "thought"` or `"thinking"`.
- However, Antigravity (`agy`) in `--output-format stream-json` mode streams deltas under `step_type: "agent_response"` (`text_delta`).
- Because `accumulated_thought` was never populated during streaming in `agy`, `sup.emit_step_update` continuously sent `thought_text: None`. Consequently, the `<Brain /> Thinking` section of the transient bubble in `FloatingChatOverlay.vue` remained hidden.

### C. Race Condition Causing Duplicate Chat Turns
- In `src/utils/dialogueListeners.ts` (`sendUserMessageHelper`), sending a prompt dispatched two asynchronous daemon calls concurrently without awaiting:
  ```ts
  daemonCall('save_user_chat_turn', { ... });
  daemonCall('prompt_agent', { ... });
  ```
- Because `prompt_agent` in `supervisor.rs` also checks `is_already_saved` and inserts a turn if not found, firing both calls simultaneously meant that `prompt_agent` inspected SQLite before `save_user_chat_turn` committed, leading to duplicate records in `agent_chat_turns`.

---

## 3. Changes Implemented

### 1. Process Unblocking & Session Recovery
- Terminated runaway background search process PID 3790473.
- AGY immediately finalized the turn, and ONE-SHIP's supervisor fallback successfully ingested the model's 1446-character response into SQLite as turn 618.
- Cleaned up duplicate user turns (611, 614, 617) in SQLite.

### 2. Multi-Runner Unified Live Thinking Stream (`supervisor.rs`)
- In `src-tauri/src/supervisor.rs`:
  - Updated `is_agy` to accumulate `delta` into `accumulated_thought` during `agent_response` when active, and pass it to `sup.emit_step_update`.
  - Updated `is_agy` tool handling to pass `live_thought` (`accumulated_thought` or `accumulated_response`) during `tool` execution, preserving the reasoning context while the tool runs.
  - Cleared `accumulated_thought` upon `[TOOL:DONE]` so each subsequent thinking cycle starts fresh.
  - Mirrored this behavior across `is_opencode` and `is_claude` runners to ensure uniform behavior regardless of runner.

### 3. Elimination of Duplicate Chat Turns (`dialogueListeners.ts` & `dialogueStore.ts`)
- Converted `sendUserMessageHelper` to `async` and explicitly `await`ed `daemonCall('save_user_chat_turn', ...)`.
- Ensured `save_user_chat_turn` commits to SQLite before `daemonCall('prompt_agent', ...)` is dispatched, allowing `is_already_saved` in `prompt_agent` to properly deduplicate the turn.

---

## 4. Verification

- `npx vue-tsc --noEmit`: Clean pass (0 errors).
- Clean SQLite conversation sequence verified for `conv-18-editorial-4b93cacd`.
