# Development Log: Mid-Turn Agent Interruption & Dynamic Send/Stop Button

**Date:** 2026-09-25  
**Topic:** Mid-Turn Flow Cancellation, POSIX Process Group Signaling (SIGINT), Decoupled Re-Arm Queue, and One-Step "Stop & Send Correction" UX

---

## 1. Problem Diagnosis & Motivation

During active development sessions with autonomous agents (`agy` and `opencode`), agents occasionally enter unintended execution paths—such as reading irrelevant documentation, exploring unnecessary directories, or running long build commands that were not requested.

Previously, Wang-Kapawu lacked a mechanism to halt an active turn without either:
1. Closing the entire chat overlay and terminating the agent session (`closeAndKillAgent` / `stop_agent`), which discarded in-flight tool context, or
2. Waiting for the agent to complete the entire unwanted turn before sending a corrective prompt.

The developer required an intuitive, modern AI chat pattern:
- **Recycle the Send space:** While the agent is working (`isWorking`), the Send button transforms into a Stop / Dual-Action button with distinct warning styling.
- **One-Step "Stop & Send Correction":** Typing a corrective message while an agent is running and pressing Enter (or clicking the button) halts the runaway execution immediately and dispatches the corrective prompt in a single atomic step.
- **Pure Stop:** If the input textarea is empty, pressing Esc (or clicking Stop) halts execution and leaves the agent re-armed in `Idle` ready for input.
- **In-Bubble Action:** A small brutalist Stop button situated directly in the transient thinking/action bubble allows instant turn cancellation.

---

## 2. Key Architecture & Implementation Details

### A. Backend POSIX Process Group Signaling & Stream-JSON Handling (`src-tauri/src/supervisor.rs`)
- **Forensic Verification of `SIGINT`:** Verified that sending `SIGINT` to Antigravity CLI in `stream-json` mode triggers a clean tool abortion, preserves session memory in `~/.gemini/antigravity-cli/conversations/<id>.db`, and emits `{"status":"ERROR","error":"interrupted"}`.
- **Whole Process Group Interrupt (`kill_process_and_group(pid, libc::SIGINT)`):** Signaling `-(pid as i32)` terminates child tool subprocesses (e.g. bash commands spawned by `run_command`) while allowing the CLI runner to catch the signal and finalize its conversation state cleanly.
- **ManagedProcess State Flags:** Added `pub is_interrupted: Arc<AtomicBool>` and `pub pending_correction: Arc<RwLock<Option<String>>>` to `ManagedProcess`.
- **Protocol Guard Bypass:** In `stream-json` output processing, when status is `ERROR` or error contains `interrupted`, supervisor protocol compliance reminders are skipped.
- **Decoupled Background Re-Arm Queue (`rearm_tx`):** In `child.wait()`, when `was_interrupted` is detected, the supervisor sends an `AgentRearmRequest` through a bounded `mpsc::channel` worker. The background worker awaits a 50ms settling period and re-launches the agent in `Idle` attached to the ongoing session. If a corrective prompt was attached, it is injected directly into the agent's stdin pipe.

### B. Tauri IPC Command Registration (`src-tauri/src/supervisor.rs`, `src-tauri/src/lib.rs`)
- Exposed `interrupt_agent(project_path, agent_id, corrective_prompt)` as a native Tauri command.
- Registered `supervisor::interrupt_agent` in `generate_handler![...]` in `src-tauri/src/lib.rs`.

### C. Pinia Dialogue Store Integration (`src/stores/dialogueStore.ts`)
- Implemented `interruptAgent(projectPath: string, agentId: string, correctivePrompt?: string)`.
- If `correctivePrompt` is present, it is optimistically added to `conversations[agentId].messages`, saved to SQLite via `save_user_chat_turn_cmd`, and dispatched with the interrupt command.
- Immediately clears `activeSteps[agentId.toLowerCase()] = null` so the transient thinking bubble dissolves without delay.

### D. Dynamic Dual Send/Stop Button & In-Bubble Stop (`FloatingChatOverlay.vue`, `ConversationPanel.vue`)
- **Dynamic Dual Action Button:**
  - When `!isWorking`: Renders standard Send button (`Send` icon, accent colors, tooltip `Send message [Enter]`).
  - When `isWorking` & `inputText.trim()` has text: Renders Stop & Send button (`Square` filled icon, semantic error border/background, tooltip `Stop execution & send correction [Enter]`).
  - When `isWorking` & `inputText.trim()` is empty: Renders Stop button (`Square` filled icon, semantic error border, tooltip `Stop execution [Esc]`).
- **Keyboard Shortcuts:**
  - `Escape`: Halts the active turn immediately if `isWorking` is true.
  - `Enter` (without Shift): Triggers `handleMainAction()` (Send if idle, Stop & Send if working with text, Stop if working without text).
- **In-Bubble Stop Button:**
  - Added a compact brutalist `[■ Stop]` button directly inside Section 2 of the transient thinking/action bubble, providing an immediate inline stop target.

---

## 3. Verification & Validation

1. **Rust Backend Compilation (`cargo check`):**
   Compiled `one-ship v0.2.1` with 0 errors and 0 warnings in 0.21s.
2. **Frontend TypeCheck & Bundling (`npm run build`):**
   Passed `vue-tsc --noEmit` and Vite bundling with 0 errors in 3.69s.
3. **Living Documentation Synchronization:**
   Updated `docs/specs/08_native_process_supervisor.md` (Section 2.6 and IPC table) and `docs/specs/18_mcp_dialogue_channel_and_chat_overlay.md` (Phase 2L).
4. **Tokio Runtime Reactor Guard:**
   Resolved startup panic (`there is no reactor running`) by decoupling `spawn_rearm_worker()` from the synchronous `get_supervisor()` getter and invoking it explicitly in Tauri's `.setup()` and daemon startup. Passed `cargo check --all-targets` and `cargo build --bins` with 0 errors.
