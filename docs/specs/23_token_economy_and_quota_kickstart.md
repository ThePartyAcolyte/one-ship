# Spec 23: Token Economy, Session Context Bounds & Quota Kickstart System
**Document Version:** 0.2.1  
**Domain:** Token Economy, Context Window Health, Multi-Account Quota Optimization, Observability  
**Maintainer:** Kapawu / Omarchy Development Team  

---

## 1. Overview & Motivation

In agentic pair-programming workflows using Google Antigravity (`agy`), two major economic and throughput bottlenecks exist:

1. **Context Bloat & Token Degradation Above 30%:**
   As sessions progress, conversational history and tool outputs accumulate in the agent's prompt context. Beyond **30% context utilization** (the user-configured threshold), every subsequent interaction re-transmits a huge volume of tokens, increasing cost, latency, and cache misses. When users resume older sessions, they often unwittingly resume heavily bloated sessions that waste significant token capacity.
2. **Quota Runway Lag (The "Lazy Reset" Problem):**
   Antigravity applies 5-hour and 7-day rolling quotas for models (such as Gemini 3.7 Flash, Gemini 2.5 Pro, and Claude 3.7 Sonnet). Crucially, **the reset timer for the next quota window does not begin when the previous window expires; it only begins on the first request made after the expiration**.
   - *Example:* If an account's 7-day weekly quota resets on Monday at 09:00, but the developer does not send a prompt until Wednesday at 09:00, the next 7-day countdown only begins on Wednesday. The developer has lost 48 hours of rolling quota availability.
   - In a multi-account environment, idle secondary accounts suffer extensive "runway lag" unless their reset windows are proactively kickstarted.

Spec 23 provides an automated, database-backed engine to:
- Persist per-session token metrics (`agent_session_metrics`) derived from runner usage events and transcript logs.
- Enforce a configurable context threshold (defaulting to **30.0%**) with UI indicators and a smart resumption guard.
- Maintain a multi-account quota ledger (`account_quotas`) tracking absolute reset timestamps (`reset_epoch`).
- Execute safe, automated, minimal-token headless pings (`agy -p "ping"`) the moment an account's quota resets, kickstarting the subsequent rolling window without developer intervention.

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                          KAPAWU TOKEN ECONOMY ENGINE                        │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
           ┌───────────────────────────┴───────────────────────────┐
           │                                                       │
           ▼                                                       ▼
┌──────────────────────────────────────┐  ┌──────────────────────────────────────┐
│  1. SESSION CONTEXT & METRICS STORE  │  │   2. ACCOUNT QUOTA RUNWAY & KICKSTART│
│                                      │  │                                      │
│  - Captures context % & cache tokens │  │  - Tracks 5h & 7d windows per account│
│  - SQLite: `agent_session_metrics`   │  │  - SQLite: `account_quotas`          │
│  - Configurable threshold (def 30%)  │  │  - Computes exact Unix `reset_epoch` │
│  - Smart Resumption Guard            │  │  - Headless probe (`agy -p "ping"`)  │
└──────────────────┬───────────────────┘  └──────────────────┬───────────────────┘
                   │                                         │
                   ▼                                         ▼
┌──────────────────────────────────────┐  ┌──────────────────────────────────────┐
│          FRONTEND OBSERVERS          │  │       BACKGROUND QUOTA WATCHER       │
│                                      │  │                                      │
│  - Dialogue header badge (Ctx/Cache) │  │  - Ticks every 60s in background     │
│  - Column 3 devlog context badges    │  │  - Checks `reset_epoch <= now`       │
│  - High-context warning modal        │  │  - Safe temporary keyring swap       │
│  - 4-Micro Pie Charts (Gemini/Others)│  │  - Restores active account token     │
└──────────────────────────────────────┘  └──────────────────────────────────────┘
```

---

## 2. Relational Database Schema

### 2.1 `agent_session_metrics` Table
Captures cumulative token telemetry for each Antigravity CLI session (`agy_conversation_id`):

```sql
CREATE TABLE IF NOT EXISTS agent_session_metrics (
    agy_conversation_id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES agent_conversations(id) ON DELETE CASCADE,
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL,
    provider TEXT NOT NULL DEFAULT 'Agy',
    model TEXT,
    context_used_pct REAL NOT NULL DEFAULT 0.0,
    fresh_tokens INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens INTEGER NOT NULL DEFAULT 0,
    cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    cache_hit_pct REAL NOT NULL DEFAULT 0.0,
    five_hour_used_pct REAL,
    five_hour_resets_at TEXT,
    five_hour_reset_epoch INTEGER,
    weekly_used_pct REAL,
    weekly_resets_at TEXT,
    weekly_reset_epoch INTEGER,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_session_metrics_conv ON agent_session_metrics(conversation_id);
CREATE INDEX IF NOT EXISTS idx_session_metrics_proj ON agent_session_metrics(project_id, agent_id);
```

### 2.2 `account_quotas` Table
Maintains multi-account rolling window quotas and kickstart state:

```sql
CREATE TABLE IF NOT EXISTS account_quotas (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    provider TEXT NOT NULL DEFAULT 'Agy',
    model TEXT NOT NULL DEFAULT 'default',
    five_hour_remaining_percent REAL,
    five_hour_resets_at TEXT,
    five_hour_reset_epoch_seconds INTEGER,
    weekly_remaining_percent REAL,
    weekly_resets_at TEXT,
    weekly_reset_epoch_seconds INTEGER,
    last_pinged_at_ms INTEGER,
    warmup_enabled INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'active',
    updated_at_ms INTEGER NOT NULL,
    UNIQUE(account_id, provider, model)
);

CREATE INDEX IF NOT EXISTS idx_account_quotas_epoch ON account_quotas(weekly_reset_epoch_seconds, five_hour_reset_epoch_seconds);
```

---

## 3. Context Health Policy & Smart Resumption Guard

### 3.1 Threshold Configuration
The context warning threshold and warmup toggle are configured under `[plugins.accounts]` in `config.toml` (global only), defaulting to **30.0%**:
```toml
[plugins.accounts]
context_warning_threshold_percent = 30.0
auto_warmup_enabled = true
```

### 3.2 Visual Health Categorization
- **Optimal (`0.0% - 30.0%`):** Color green / cyan. Fast cache reads, minimal turn overhead.
- **Elevated (`30.1% - 50.0%`):** Color amber. Moderate token burn.
- **Critical Bloat (`> 50.0%`):** Color red. High token cost per turn, slow responses, candidate for compaction.

### 3.3 Resumption Guard (superseded by Reopen)
Reopening an older Conversation from Column 3 (`DevlogJournalView`) no longer resumes the old runner session, so there is no inherited context bloat to warn about: `reopen_conversation` seeds a new Native Session with a size-capped Brief (`supervisor.brief_max_bytes`, Spec 30 §5). The former high-context modal and its "Start Clean Session with Milestone Context" branch were removed with `resume_agent_session_cmd` (Spec 30 Phase E).

---

## 4. Quota Anti-Lag Engine (The "Kickstart / Warmup" Daemon)

### 4.1 Reset Epoch Calculation
`kopeng::stats::parse_countdown_to_epoch` parses RFC 3339 ISO-8601 timestamps and human countdown strings into exact Unix epoch timestamps:
- `"3h 20m"` -> `now_unix + (3 * 3600 + 20 * 60)`
- `"4d 12h"` -> `now_unix + (4 * 86400 + 12 * 3600)`
- `"45m"` -> `now_unix + (45 * 60)`

### 4.2 Background Kickstart Loop (`spawn_quota_watcher`)
A dedicated background tokio task runs every `accounts.stats_refresh_interval_seconds` (default 60 seconds):
1. Queries `account_quotas` for rows where `warmup_enabled == 1`.
2. Evaluates if:
   - `weekly_reset_epoch_seconds <= now_unix && (last_pinged_at.is_none() || last_pinged_at < weekly_reset_epoch_seconds)` OR
   - `five_hour_reset_epoch_seconds <= now_unix && (last_pinged_at.is_none() || last_pinged_at < five_hour_reset_epoch_seconds)`
3. If true, executes the **Safe Keyring Swapping Probe** (`probe_account_quota` / `run_quota_probe`):
   - Backs up current active token from system keyring (`read_keyring_token`).
   - Writes the target account's token into keyring (`write_keyring_token`).
   - Allocates a Linux PTY (`libc::openpty`) and launches:
     ```bash
     agy -i hi --model <quota_probe_model> --effort <quota_probe_effort> --disable-slash-commands
     ```
   - `KeyringRestoreGuard` restores the active account's token on any exit path.
   - Polls `agy-statusline.json`, updates `account_quotas`, and emits `AccountQuotaUpdated` and `keyring-warmup-status` over `/api/events` SSE. Model pools are grouped via `canonical_pool_identity` (`gemini-pool`, `3p-pool`).

---

## 5. Daemon Commands (Spec 12 §7.2)

The quota watcher and the manual kickstart run in the daemon and share its keyring lock, so a scheduled probe and a manual one never swap the keyring at the same time. Errors are `StatsError` as `ApiErrorResponse`.

| Command | Arguments | Returns | Description |
| :--- | :--- | :--- | :--- |
| `get_account_quotas` | none | `AccountQuotaRecord[]` | Returns quota status for all accounts |
| `kickstart_account_quota` | `accountId` | `null` | Triggers an immediate manual probe; fails when the probe cannot run, times out or captures no quota |
| `toggle_account_warmup` | `accountId`, `enabled` | `null` | Enables or disables automated kickstart for an account |
| `get_session_metrics` | `agyConversationId` | `AgentSessionMetricsRecord \| null` | Returns metrics for a specific session |
| `get_context_threshold` | `projectPath` (nullable) | `number` | Effective `accounts.context_warning_threshold_percent` from the configuration cascade (Spec 10); edited in Settings. The UI has no default of its own. |

### 5.1 Warmup Configuration (`[plugins.accounts]`, global)
The watcher runs every `stats_refresh_interval_seconds` and probes only when `auto_warmup_enabled` is true, the per-account `warmup_enabled` is set, a pool is at or below `quota_depleted_threshold_percent` (default 5.0), its reset epoch has arrived, and `quota_probe_cooldown_seconds` (default 900) have passed since the last ping. A probe runs the agy pair `quota_probe_model` / `quota_probe_effort` (default `gemini-3.8-flash` / `low`, validated against the agy manifest; the model and effort flags come from the manifest) in a PTY and polls the statusline file every `quota_probe_poll_step_ms` (default 150) for at most `quota_probe_timeout_ms` (default 7000).

---

## 6. Verification Plan
- Unit tests in `src-tauri/src/kopeng/stats.rs` for relative time string parser.
- Integration tests in `src-tauri/src/db.rs` for `agent_session_metrics` and `account_quotas` CRUD.
- Automated `vue-tsc` and `cargo check` validation.
