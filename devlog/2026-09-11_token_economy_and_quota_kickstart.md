# 2026-09-11: Token Economy, Session Context Bounds & Quota Kickstart System (Spec 23)

## Context & Motivation
In conversational workflows with Google Antigravity (`agy`), long-running sessions degrade in performance and token cost as context history accumulates. Above 30% context utilization, repeated retransmission increases turn latency and token consumption. Resuming high-context sessions without awareness compounds this overhead.
Furthermore, Google Antigravity rolling quota timers (5-hour and 7-day rolling windows) exhibit "runway lag" (the "lazy reset" phenomenon): the countdown timer for the next quota window does not begin when the previous window expires, but only upon the first subsequent request made. Idle secondary accounts lose rolling availability unless proactively kickstarted.

## Architectural Changes & Implementation

### 1. Relational Database Schema (`src-tauri/src/db.rs`)
- Created `agent_session_metrics` table tracking `agy_conversation_id`, `context_used_pct`, token counts (`fresh`, `cache_read`, `cache_creation`, `total`), cache hit rate, and calculated window reset epochs (`five_hour_reset_epoch`, `weekly_reset_epoch`).
- Created `account_quotas` table tracking per-account rolling quotas, remaining percentages, reset epochs, `last_pinged_at`, and `warmup_enabled`.
- Added columns `context_pct` and `total_tokens` to `agent_session_journal` with automatic SQLite schema migrations.

### 2. Relative Countdown Parser & Time Engine (`src-tauri/src/kopeng/stats.rs`)
- Implemented zero-dependency relative countdown parser (`parse_countdown_to_epoch`) handling durations like `"3h 20m"`, `"5d 14h"`, `"45m"`, and `"(30s)"`.
- Implemented `sync_metrics_to_db` to persist telemetry from Herder tokens and transcript logs directly into SQLite.

### 3. MCP Observability & Context Advisory (`src-tauri/src/mcp.rs`)
- Updated `record_session_minutes` to look up active metrics via `stats::get_agent_metrics`, populating `context_pct` and `total_tokens` on `SessionJournalRecord` and emitting them in `SessionMinutesRecorded`.
- Added an advisory notice in the MCP response text when `context_pct > 30.0%`, guiding agents to conclude or compact conversations when context thresholds are exceeded.

### 4. Quota Runway & Headless Kickstart Engine (`src-tauri/src/kopeng/stats.rs` & `lib.rs`)
- Implemented `probe_account_quota`: safely backs up active system keyring credentials, writes target account credentials, runs a 1-token headless probe (`agy -p "ping"`), immediately restores the active keyring token, and updates `last_pinged_at`.
- Implemented `spawn_quota_watcher`: background tokio task ticking every 60s, checking for expired rolling quota epochs across registered accounts with `warmup_enabled = true` and broadcasting `AccountQuotaUpdated`.
- Registered Tauri IPC commands: `get_session_metrics_cmd`, `get_account_quotas_cmd`, `toggle_account_warmup_cmd`, `kickstart_account_quota_cmd`, `get_context_threshold_cmd`, `set_context_threshold_cmd`.

### 5. Frontend UI & Resumption Guard (`src/`)
- **Pinia Dialogue Store (`src/stores/dialogueStore.ts`):** Added state and actions for metrics, quotas, and context threshold (`30.0%` default). Subscribed to `AccountQuotaUpdated` and `SessionMinutesRecorded`.
- **Live Context Badges (`src/components/dialogue/ConversationPanel.vue`):** Displays real-time `[Ctx: XX% / 30%]` badge with warning pulsing when exceeded, and `[Cache: XX%]` badge with hit statistics.
- **Devlog Journal Cards & Resumption Guard (`src/components/dialogue/DevlogJournalView.vue`):** Renders context usage badge per devlog entry. Intercepts resumption of sessions >30% context with a modal offering:
  - *Resume Anyway* (continues high-context session)
  - *Start Clean Session with Milestone Context* (creates fresh session pre-loaded with milestone summary)
- **Multi-Account Quota Runway (`src/components/AppHeader.vue`):** Shows 5h and 7d quota gauges and an interactive `● WARM` auto-kickstart toggle in the account dropdown, alongside a manual `⚡` kickstart probe button.

## Verification
- Backend unit tests passed cleanly (`cargo test` -> 11 passed, 0 failed).
- Backend compilation validated (`cargo check` -> 0 errors, 0.51s).
- Frontend type safety validated (`npx vue-tsc --noEmit` -> 0 errors).
- Production bundling verified (`npm run build` -> built in 2.99s).
- Runtime panic fix verified: `spawn_quota_watcher` uses `tauri::async_runtime::spawn` instead of raw `tokio::spawn` to run reliably from Tauri's synchronous `setup` hook.
- Process boundary respected: all runtime servers left intact under developer's `./scripts/run_dev.sh`.
