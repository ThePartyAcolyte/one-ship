# Devlog: Phase B — Post-Herder Architecture Refactor & Native StatusLine Ingestion

**Date:** 2026-09-16  
**Topic:** Redesigning Session Snapshots (`agent_key`), Purging Legacy Herder References, and Implementing Direct SQLite StatusLine Ingestion

---

## 1. Summary of Changes

### B1. Multi-Account Session Resume (`agent_key`)
- **Root Problem:** Post-migration from Herder to the native `ProcessSupervisor`, `AgySession.pane_id` extracted from `/proc/<pid>/environ` (`HERDR_PANE_ID=`) was permanently `None`. Consequently, `switch_account()` never saved running agent sessions to `pending_resume`, breaking session recovery upon account swapping.
- **Architectural Solution:**
  - Migrated SQLite table `pending_resume` from `pane_id TEXT PRIMARY KEY` to `agent_key TEXT PRIMARY KEY` (`"<project_path>::<agent_id>"`).
  - Implemented `ProcessSupervisor::get_agent_key_by_pid(pid)` for deterministic reverse-lookup of running OS PIDs to managed supervisor keys.
  - Updated `AgySession` struct: replaced `pane_id` with `agent_key`.
  - Refactored `switch_account()` to snapshot active sessions using `agent_key`, preventing premature table clearing.
  - Refactored `switch_pane_account` / `switch_pane_account_cmd` to `switch_agent_account` / `switch_agent_account_cmd`.
  - Purged obsolete `pane_id` and `tab_id` fields across frontend models (`SwitchConfirmationModal.vue`, `App.vue`, `AddPersonaModal.vue`, `dialogueStore.ts`).

### B2. StatusLine Legacy Purge (`stats.rs`)
- Removed dead fallback path `~/.local/state/herdr/plugins/herdr-agent-quota/agy-statusline.json` from `agy_statusline_file_path()`.
- Updated `AgentSessionMetrics` and `parse_pane_tokens()` to use `agent_key` and normalize `source: "statusline"`.
- Refactored `get_agent_metrics()` and `get_agent_metrics_cmd()` to take `agent_key` instead of `pane_id`.

### B3. Native StatusLine Direct Ingestion Hook (`scripts/agy_statusline.py`)
- Researched and verified Antigravity CLI's native `statusLine` command hook: `agy` pipes a live JSON payload to `stdin` on every turn and state transition, containing `email`, `plan_tier`, `model`, `quota` (windows), `conversation_id`, and `context_window`.
- Created executable `scripts/agy_statusline.py` implementing zero-latency direct SQLite ingestion with clean domain separation:
  - **Account Domain (`account_quotas`):** Extracts `email`, maps to `accounts.id`, canonicalizes model pool (`gemini-pool` vs `3p-pool`), and updates rolling rate limits (`five_hour_remaining_pct`, `weekly_remaining_pct`, reset epochs). Updates apply globally across all CLI sessions.
  - **Conversation Domain (`agent_session_metrics`):** Matches `conversation_id` against `agent_chat_turns` to update active session context utilization (`context_used_pct`), token counts (`fresh`, `cache_read`, `cache_create`, `total`), and cache hit efficiency.
  - **Terminal HUD:** Emits clean brutalist ANSI status string to `stdout` for terminal status bar display.

---

## 2. Verification & Validation

1. **Rust Backend Compilation & Tests:**
   - `cargo check`: 0 warnings, 0 errors.
   - `cargo test --lib`: 19/19 tests passed successfully.
2. **Frontend Type Check:**
   - `npx vue-tsc --noEmit`: 0 errors.
3. **End-to-End StatusLine Hook Test:**
   - Injected synthetic Antigravity payload into `scripts/agy_statusline.py`.
   - Verified immediate direct update in SQLite table `account_quotas` for `thepartyacolyte@gmail.com`.
