# Devlog: Multi-Runner Chat Toolbox, Clean Session Lifecycles, and Context Handoff

**Date:** 2026-09-24  
**Topic:** Multi-Runner Chat Toolbox, Runner-Aware Telemetry, DeepSeek Process Isolation, Incompatible Session Guard, and Context Handoff

---

## 1. Problem Statement & Motivation
Prior to this release:
1. **Runner-Incompatible Resumption:** When switching an agent's configuration from OpenCode to Antigravity (e.g. `bosmang` previously configured with DeepSeek, subsequently reconfigured for Antigravity + Claude), resuming an active conversation loaded the foreign session ID from SQLite, crashing runner invocation (`--conversation` mismatch).
2. **Hardcoded Antigravity Telemetry in Chat:** The top telemetry bar in `FloatingChatOverlay.vue` assumed Google Antigravity quotas (5-hour and 7-day rate limit windows and cache tokens). When running OpenCode/DeepSeek agents, the telemetry was inaccurate or rendered zero/blank.
3. **Automatic Account Swapping Mismatch Banner:** An amber banner warned that an agent was running under a different account and proposed automatic reloads, whereas Antigravity local session resume operates independently of the active keyring account.
4. **Lack of In-Chat Agent Control:** Users could not change models, swap runners, switch accounts, clear context, or hand off conversation history and session minutes directly from the chat overlay without navigating out of the chat.

---

## 2. Architectural Changes & Key Implementations

### Backend (`src-tauri/`)
- **Database Schema Migration (`src-tauri/src/db.rs`):**
  - Added `runner TEXT NOT NULL DEFAULT 'agy'`, `model TEXT`, and `account_id TEXT` to the `agent_conversations` table.
  - Implemented cross-runner resumption guard in `db_get_active_agent_session`: validates `conv.runner == expected_runner`. If foreign, automatically archives the incompatible conversation and returns `None`, starting a clean session.
- **DeepSeek Key Environment Injection (`src-tauri/src/supervisor.rs`, `src-tauri/src/kopeng/accounts.rs`):**
  - Updated DeepSeek account management to bind real user emails in the canonical `accounts` SQLite table.
  - When spawning OpenCode, resolves the targeted or active DeepSeek account, decrypts its API key, and sets `DEEPSEEK_API_KEY` directly in the child process environment (`cmd.envs`), achieving clean process isolation.
- **OpenCode ACP Usage Metrics & Event Emission (`src-tauri/src/supervisor.rs`):**
  - Captured OpenCode ACP `usage_update` and prompt finish events.
  - Emitted `agent-usage-update` Tauri IPC event containing `{ agent_id, runner, model, tokens_used, cost_dollars, max_context_tokens }`.

### Frontend (`src/`)
- **Dialogue Store Integration (`src/stores/dialogueStore.ts`):**
  - Extended `AgentConversationRecord` and `AgentSessionMetrics` with runner, model, account, token usage, and cost fields.
  - Listened for `agent-usage-update` events to reactively populate session metrics.
  - Added `startCleanSession` and `updateAgentExecutionConfig` actions to allow clean restarts and parameter reconfiguration.
- **In-Chat Execution Toolbox (`src/components/dialogue/FloatingChatOverlay.vue`):**
  - Integrated a sub-header toolbar featuring:
    - **Runner Selector:** `AppSelect` toggling between `AGY` and `OPENCODE`.
    - **Model Selector:** Dynamically populated based on runner schema (`get_agent_runner_schema`).
    - **Account Selector:** Filtered by provider (Google accounts for Antigravity, DeepSeek accounts for OpenCode).
    - **Apply Button:** Appears when selection diverges from running agent, restarting agent with updated configuration.
    - **Clean Context Button:** Archives current conversation in SQLite and restarts agent cleanly.
    - **Handoff Button:** Opens `ContextTransferModal`.
  - Removed the amber account mismatch banner in accordance with user directives.
  - Adapted the telemetry bar to dynamically render OpenCode metrics (`tokens_used`, estimated USD cost, context ratio) and hide Google rate-limit quotas when running OpenCode.
- **Context & Minutes Handoff (`src/components/dialogue/ContextTransferModal.vue`):**
  - Created modal dialog allowing selection of target agent in the project.
  - Configurable handoff payload combining recent conversation turns and latest `agent_session_journal` minutes with optional user instructions.
  - Automatically switches floating chat focus to target agent and launches if unlaunched.

---

## 3. Verification & Compliance
- `cargo check`: Clean compilation with 0 warnings/errors.
- `cargo test kopeng`: 22 passed; 0 failed.
- `npm run build` (`vue-tsc --noEmit && vite build`): Succeeded cleanly.
- Strict Git ban preserved: Zero VCS commands executed.
- Dedicated file editing tools only (`replace_file_content`, `write_to_file`).
