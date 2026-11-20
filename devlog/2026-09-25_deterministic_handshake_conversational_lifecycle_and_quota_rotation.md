# Development Log: Deterministic Handshake, Conversational Lifecycle, and Model-Pool Quota Rotation

**Date:** 2026-09-25  
**Topic:** Deterministic Handshake, Unlaunched Conversational Lifecycle, Persona Prompt Decoupling, Model-Pool Quota Rotation, and In-Chat Auto-Retry

---

## 1. Problem Diagnosis & Motivation

During active developer testing with Antigravity and OpenCode runners across multiple accounts and models, four interrelated architectural and UX bottlenecks were identified:

1. **Keyring Swap Race Condition (The 250ms Sleep Flaw):**
   When launching an agent with a temporary target account, `supervisor.rs` previously swapped the keyring credentials, spawned the `agy` child process, slept an arbitrary `250ms`, and restored the original keyring token. Because the Antigravity Node/V8 CLI takes 3–5 seconds to boot and query the OS Secret Service, the token was restored before `agy` read it, causing `agy` to run under the exhausted account.
2. **Passive File Watcher Account Hijacking:**
   In `accounts.rs` (`check_and_register_keyring_account`), passive mtime file watcher scans on `~/.config/antigravity/default/credentials.json` called `save_account(&email, token, true)`. Whenever the keyring was touched, this passively flipped `is_active = true` in SQLite back to the exhausted account.
3. **Conversational Lifecycle & Persona Prompt Contamination:**
   Opening an unlaunched or stopped agent in the chat view did not provide clear options to resume previous discussions versus starting clean. Resuming conversations or launching an agent invariably injected verbose persona instructions (`"You are the frontend developer..."`), polluting the conversational history.
4. **Pool-Blind Quota Exhaustion & Turn Recovery:**
   Quota in Antigravity is partitioned into independent pools (`gemini-pool` vs `3p-pool` for Claude/Anthropic). An account exhausted for Claude can still run Gemini models with ample quota. However, account switching was blind to model pools and 7-day rolling reset cycles. Furthermore, when a turn failed due to quota (429 / `RESOURCE_EXHAUSTED`), the user had to manually copy, re-type, and resend their prompt.

---

## 2. Key Architecture & Implementation Details

### A. Deterministic Init Handshake
- **Synchronization Channel:** In `supervisor.rs` (`spawn_agent`), replaced the broken 250ms sleep with an event-driven `Arc<tokio::sync::Notify>` handshake.
- **Lock Holding:** When spawning `agy` with target credentials, the supervisor acquires the global `keyring_lock` and transfers it to a background worker awaiting `init_notify.notified()`.
- **Event-Driven Restoration:** The stdout reader monitors child output. Upon parsing `{"event":"init","conversation_id":"..."}`, it fires `init_notify.notify_waiters()`. The background worker confirms authentication in memory, restores original keyring credentials, and releases the lock. A 10-second timeout prevents indefinite lock starvation in case of child crash.
- **Session Mapping:** On `event == "init"`, `supervisor.rs` immediately maps the session ID into `agent_conversations.agy_conversation_id` in SQLite.
- **Non-Invasive Watcher:** Updated `accounts.rs` so passive mtime scans call `save_account(&email, token, is_new)`. Existing accounts never overwrite `is_active` in SQLite during passive background scans.

### B. Unlaunched Conversational Lifecycle & Persona Prompt Decoupling
- **Continuous History Rendering:** Unlaunched/stopped agents load and display their previous message history in `FloatingChatOverlay.vue`.
- **Session Decision Card:** Rendered at the bottom of the conversation stream when `isAgentUnlaunched` is true:
  - If previous turns exist: offers **[ Resume Session ]** (attaches to the existing session without injecting startup prompts) vs **[ New Clean Session ]** (archives existing session and injects persona directives).
  - If no turns exist: offers **[ Start Session ]** (initiates a fresh session with startup directives).
- **Auto-Launch on Input Send:** Typing a prompt while unlaunched and pressing Enter automatically launches the agent (resuming existing context if history exists, or starting clean if none) and delivers *only* the user's message.
- **Persona Prompt Guard:** In `launch_agent` (`supervisor.rs`), `build_agent_initial_prompt` is executed strictly when `clean_context == Some(true)` or when there is zero existing history/session. Resumed sessions never receive startup persona prompts.

### C. Model-Pool-Aware Account Rotation & Weekly Reset Priority
- **Pool Resolver:** Implemented `getRecommendedAccountForModel` in `dialogueStore.ts`. Identifies whether a model belongs to `gemini-pool` or `3p-pool` (Claude/Anthropic).
- **Weekly Reset Sorting:** Viable accounts ($>5\%$ 5h and $>0\%$ 7d) are sorted primarily by `weekly_reset_epoch_seconds ASC`. This prioritizes accounts closest to their 7-day reset date, ensuring expiring rolling quota cycles are consumed first without waste.
- **Annotated Account Dropdown:** Account dropdown labels in the chat toolbar dynamically display remaining quota and reset ETA for the currently selected model (e.g. `user@example.com [0% • resets 1h 35m]`).

### D. Seamless In-Chat Auto-Retry
- **Error Capture:** When `agent-error` fires in `dialogueStore.ts`, the store captures the latest user message into `pendingRetryPrompts[agentId]`.
- **Actionable Resolution Banner:** Displays the recommended account (`[REC]`) in `FloatingChatOverlay.vue`.
- **Seamless Re-Dispatch:** Clicking an alternative account or retry restarts the agent under the new configuration. In `updateAgentExecutionConfig`, the pending prompt is automatically dispatched to the agent via `invoke('prompt_agent', ...)`. Because the prompt is already displayed in the chat interface, it resumes immediately without creating duplicate message bubbles.

---

## 3. Verification & Validation

1. **Rust Compilation (`cargo check`):**
   - Verified zero compile errors and zero warnings across `src-tauri/src/db.rs`, `src-tauri/src/supervisor.rs`, and `src-tauri/src/kopeng/accounts.rs`.
2. **Frontend TypeCheck & Build (`vue-tsc && vite build`):**
   - Verified clean production build with 0 TypeScript and template errors across `src/stores/dialogueStore.ts` and `src/components/dialogue/FloatingChatOverlay.vue`.
3. **Spec Alignment:**
   - Synchronized `docs/specs/17_multi_account_management.md` with Section 15.
   - Synchronized `docs/specs/18_mcp_dialogue_channel_and_chat_overlay.md` with Phase 2I.
