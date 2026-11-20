# Quota Observability and Token Economy

## Purpose

Provide real-time visibility into per-session token consumption, prompt cache efficiency, context window health, and multi-account rolling rate-limit quotas (`5h` and `7d`), while eliminating quota runway lag ("lazy reset" timers) through deterministic, PTY-isolated warmup probes.

## Scope

In scope: `src-tauri/src/kopeng/stats.rs`, `scripts/agy_statusline.py`, `~/.local/state/one-ship/agy-statusline.json`, Antigravity `transcript.jsonl` parsing, SQLite tables (`agent_session_metrics`, `account_quotas`), `get_agent_metrics` resolution rules, context warning policy, the Zero Startup Warmup Invariant, `libc::openpty` PTY probe isolation, cross-account attribution guards, local timezone formatting, daemon commands, SSE events, `[plugins.accounts]` config keys, and UI telemetry (`agentMetricsStore.ts`, `FloatingChatOverlay.vue`, `AppQuotaConcentric` / `AppQuotaBar` / `AppQuotaPie`).

Out of scope: OAuth keyring encryption and multi-provider account switching (see [Multi-Account Management](04-multi-account-management.md)), and Brief-based conversation reopening that replaced the legacy high-context resumption modal (see [Thread, Conversation, and Runner-Neutral Context Continuity](02-thread-conversation-and-continuity.md)).

## Model / Design

### 1. Motivation & Dual-Track Telemetry Architecture

Agentic workflows encounter two primary economic and throughput bottlenecks:
1. **Context bloat & token degradation:** As turns accumulate past the configured warning threshold (`accounts.context_warning_threshold_percent`, default `30.0%`), every prompt re-transmits large token volumes, increasing cost, latency, and cache misses.
2. **Quota runway lag (lazy reset):** Antigravity (`agy`) applies 5-hour and 7-day rolling quotas whose next reset countdown begins only on the first request made *after* expiration rather than at expiration time. Idle secondary accounts lose hours or days of rolling availability unless kickstarted when their reset boundary arrives.

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                     QUOTA OBSERVABILITY & TOKEN ECONOMY                     │
│                                                                             │
│  [ agy statusLine stdin ] ─► scripts/agy_statusline.py ─► agy-statusline.json│
│  [ transcript.jsonl ]     ──────────────────────────────► [ kopeng::stats ] │
│  [ claude stream-json ]   ─► rate_limit_event ──────────► [ SQLite Tables ] │
│  [ opencode ACP ]         ─► usage_update ──────────────► [ SSE /api/events]│
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
           ┌───────────────────────────┴───────────────────────────┐
           ▼                                                       ▼
┌──────────────────────────────────────┐  ┌──────────────────────────────────────┐
│  1. SESSION CONTEXT & METRICS        │  │  2. QUOTA RUNWAY & KICKSTART DAEMON  │
│  - `agent_session_metrics`           │  │  - `account_quotas`                  │
│  - Context % & cache hit %           │  │  - Zero Startup Warmup Invariant     │
│  - Threshold (default 30.0%)         │  │  - PTY probe (`libc::openpty`)       │
│  - `get_agent_metrics` read-only API │  │  - `KeyringRestoreGuard` RAII        │
└──────────────────────────────────────┘  └──────────────────────────────────────┘
```

### 2. Ingestion Channels

#### 2.1 Native Antigravity StatusLine Ingestion (`scripts/agy_statusline.py`)

Configured in `~/.gemini/antigravity-cli/settings.json`, `agy` pipes live JSON state on `stdin` when executing its interactive Bubbletea TUI loop:

```json
{
  "email": "developer@example.com",
  "plan_tier": "Google AI Pro",
  "model": {
    "id": "Gemini 3.8 Flash (High)",
    "display_name": "Gemini 3.8 Flash (High)"
  },
  "quota": {
    "gemini-5h": {
      "remaining_fraction": 0.7535,
      "reset_time": "2026-09-17T20:27:35Z",
      "reset_in_seconds": 14690
    },
    "gemini-weekly": {
      "remaining_fraction": 0.8045,
      "reset_time": "2026-09-23T03:46:37Z",
      "reset_in_seconds": 473032
    },
    "3p-5h": {
      "remaining_fraction": 0.0,
      "reset_time": "2026-09-17T20:25:46Z",
      "reset_in_seconds": 14581
    },
    "3p-weekly": {
      "remaining_fraction": 0.6625,
      "reset_time": "2026-09-24T13:41:36Z",
      "reset_in_seconds": 595131
    }
  },
  "conversation_id": "5c5aa59d-c3e7-456e-85f5-c3f8703d567d",
  "context_window": {
    "total_input_tokens": 112431,
    "total_output_tokens": 125732,
    "current_usage": {
      "input_tokens": 80870,
      "cache_read_input_tokens": 36721
    }
  }
}
```

The hook separates two tracks:
1. **Account-level quota runway (`account_quotas`):** Maps deterministically by `account_id`, else `email`, to `accounts.id`; observations naming no registered account are rejected (never attributed to `is_active = 1`). Captures 5h and weekly remaining percentages, countdowns, and reset epochs for both `gemini-pool` and `3p-pool` via `canonical_pool_identity`, and atomically writes `~/.local/state/one-ship/agy-statusline.json`.
2. **Conversation-level context & token metrics (`agent_session_metrics`):** Mapped by `conversation_id` against `agent_chat_turns` and accumulated from transcripts.

#### 2.2 Direct Transcript Log Ingestion (`transcript.jsonl`)

Reads `~/.gemini/antigravity-cli/brain/<conversation_id>/.system_generated/logs/transcript.jsonl`, summing `message.usage` / `usage` objects across assistant turns:

```json
{
  "type": "assistant",
  "message": {
    "usage": {
      "input_tokens": 120,
      "cache_read_input_tokens": 850,
      "cache_creation_input_tokens": 0,
      "output_tokens": 45
    }
  }
}
```

- `Total Input = sum(input_tokens)`
- `Total Read = sum(cache_read_input_tokens)`
- `Total Creation = sum(cache_creation_input_tokens)`
- `Cache Hit % = Total Read / (Total Input + Total Read + Total Creation) * 100`

#### 2.3 Claude Code & OpenCode Live Ingestion

- **Claude Code:** `supervisor.rs` intercepts `type: "rate_limit_event"` on `stream-json` stdout and updates `account_quotas` (`provider = 'claude'`, `model = 'claude'`) passively without PTY probes. Turn `result` events emit `agent-usage-update`.
- **OpenCode ACP:** `usage_update` notifications emit `agent-usage-update` streaming `tokens_used`, `max_context_tokens`, and `cost_dollars`.

### 3. Metrics Resolution (`get_agent_metrics`)

`get_agent_metrics(project_path, agent_id, conversation)` is read-only and scoped to one agent of one project:
- **Native session:** Resolved from `conversation` (`ConversationRef::Native`, or the newest `agy` session among turns of a `ConversationRef::OneShip` Conversation), else the agent's live session, else the newest session recorded in its turns.
- **Context %:** Uses the statusline-recorded percentage when positive; otherwise computes total transcript tokens divided by the model's `context_window_tokens_count` from `templates/runners/<runner>.toml`. Without either, no context percentage is reported (window sizes are never guessed from model names).
- **Quota windows:** Read from `account_quotas` for the account the agent's process was launched with; an agent without a recorded account shows no quota windows (the global active account is never substituted).
- **Persistence separation:** Session metrics are persisted only by `post_agent_response` (`record_session_metrics`, keyed by ONE-SHIP Conversation); quota rows are written only by statusline sync, `rate_limit_event`, and warmup probes.

### 4. Context Health Policy & Resumption

- **Threshold:** `accounts.context_warning_threshold_percent` (global only, default `30.0%`).
- **Health categorization:**
  - Optimal (`0.0% - 30.0%`): healthy accent/ok indicator, high cache efficiency.
  - Elevated (`30.1% - 50.0%`): warning state (`--status-warn-*`), moderate burn.
  - Critical bloat (`> 50.0%`): danger state (`--status-error-*`), candidate for context clean/compaction.
- **Resumption Guard (superseded by Reopen):** Reopening an older Conversation from `DevlogJournalView` seeds a new Native Session with a size-capped Brief (`supervisor.brief_max_bytes`) rather than resuming the old bloated Native Session; the legacy high-context resumption modal and `resume_agent_session_cmd` were removed.

### 5. Quota Anti-Lag Engine & Interactive PTY Probe Isolation

1. **Reset epoch calculation (`parse_countdown_to_epoch`):** Parses RFC 3339 ISO-8601 UTC strings (e.g., `"2026-09-25T09:15:59Z"`) via `chrono::DateTime::parse_from_rfc3339` first, falling back to relative durations (`"3h 20m"`, `"4d 12h"`, `"45m"`). Backend computes `five_hour_resets_at_local` and `weekly_resets_at_local` in `chrono::Local`.
2. **Zero Startup Warmup Invariant (`spawn_quota_watcher`):**
   - Consumes the 0th `tokio::time::interval` tick immediately upon spawn so daemon restarts perform strictly passive sync (`sync_quota_from_statusline_file`) and never fire startup probe storms.
   - Ticks every `accounts.stats_refresh_interval_seconds` (default `60` s) when `accounts.auto_warmup_enabled` is `true` and per-account `warmup_enabled == 1`.
   - **Depleted-only filter:** Only probes pools with `remaining_pct <= accounts.quota_depleted_threshold_percent` (default `5.0%`).
   - **Runtime boundary crossing:** Fires only when the reset boundary is crossed during runtime (`epoch <= now && epoch > last_check_epoch_seconds`, or `now >= epoch` without a ping since `epoch`).
   - **Mandatory probe cooldown:** `db_update_account_quota_ping` records `last_pinged_at` before/on probe, enforcing `accounts.quota_probe_cooldown_seconds` (default `900` s / 15 min) to prevent recursive probe loops.
3. **Native Linux PTY isolation (`probe_account_quota`):**
   - Because `agy` only invokes its `statusLine` hook (`scripts/agy_statusline.py`) when attached to a real TTY running its Bubbletea TUI (batch `-p` skips it), `probe_account_quota` allocates a pseudo-terminal pair via `libc::openpty`.
   - Acquires the shared `keyring_lock`, wraps active credentials in `KeyringRestoreGuard` (`Drop` restores original credentials on any exit path), writes target credentials, spawns a background drain task on `master_fd`, attaches `slave_fd` to `stdin`/`stdout`/`stderr`, and runs:
     ```bash
     agy -i hi --model <quota_probe_model> --effort <quota_probe_effort> --disable-slash-commands
     ```
     (default `gemini-3.8-flash` / `low` from `[plugins.accounts]`, validated against the `agy` runner manifest to avoid consuming `3p-pool` quota).
   - Polls `agy-statusline.json` every `quota_probe_poll_step_ms` (default `150` ms) up to `quota_probe_timeout_ms` (default `7000` ms) for an entry matching the target `email` or `account_id` with `timestamp >= probe_start_epoch - 1`, then kills the child, updates `account_quotas`, and emits `AccountQuotaUpdated`, `account-quotas-updated`, and `keyring-warmup-status`.
4. **Cross-account attribution guard:** When `target_account_id` is specified in `sync_quota_from_statusline_file`, payloads with mismatched `email`/`account_id` are logged at `WARN` and discarded. When `None`, resolution matches `account_id` or `email` against SQLite strictly; in `scripts/agy_statusline.py`, active-account fallback is restricted strictly to payloads lacking any email field.

### 6. UI Observability Integration (`FloatingChatOverlay.vue`)

- **Telemetry bar:** Displays `CTX XX.X%` meter (`--accent-color` < 60%, warning 60–84%, danger >= 85% using semantic theme variables), token count + cache hit % (`TOKENS: 42.1k (88% hit)` or USD cost for DeepSeek/OpenCode), `5H` / `7D` remaining badges with `<AppTooltip>` local reset clock + countdown (`Gemini 5h: 71% remaining • Resets at 06:15 local (2h 55m left)` or `Depleted`), and active process account handle.
- **Scoped warmup badge:** `keyring-warmup-status` drives the top-bar `⚡ WARMING UP` indicator scoped strictly to the probed account and pool.
- **Quota gauges:** `AppQuotaConcentric`, `AppQuotaBar`, `AppQuotaPie` (selectable via `ui.quota_gauge_style`).

## Contracts

### Database Tables (`oneship.db`)

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

### Rust Telemetry Types (`src-tauri/src/kopeng/stats.rs`)

```rust
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct UsageWindowStats {
    pub label: String,                // "5h", "7d"
    pub used_percent: f64,
    pub remaining_percent: f64,
    pub resets_at: Option<String>,
    pub reset_epoch_seconds: Option<i64>,
    pub severity: String,             // "normal", "warning", "danger"
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CacheUsageStats {
    pub fresh_input_tokens_count: u64,
    pub read_tokens_count: u64,
    pub creation_tokens_count: u64,
    pub hit_percent: f64,
    pub ttl_seconds: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AgentContextStats {
    pub used_percent: f64,
    pub remaining_percent: f64,
    pub cache: Option<CacheUsageStats>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AgentSessionMetrics {
    pub agent_key: Option<String>,
    pub conversation_id: Option<String>,
    pub provider: String,
    pub model: Option<String>,
    pub status: String,
    pub five_hour_window: Option<UsageWindowStats>,
    pub weekly_window: Option<UsageWindowStats>,
    pub context: Option<AgentContextStats>,
    pub source: String,               // "statusline", "transcript", "hybrid"
    pub captured_at_epoch_seconds: u64,
    pub tokens_used: Option<u64>,
    pub cost_dollars: Option<f64>,
    pub max_context_tokens: Option<u64>,
}
```

### Daemon Commands (`api::API_COMMANDS` / `daemonCall`)

| Command | Arguments | Returns | Description |
| :--- | :--- | :--- | :--- |
| `get_agent_metrics` | `{ projectPath, agentId, conversationId }` | `AgentSessionMetrics` | Read-only telemetry snapshot for one agent |
| `get_session_metrics` | `{ agyConversationId }` | `AgentSessionMetricsRecord \| null` | Persisted metrics for a native session ID |
| `get_account_quotas` | `{}` | `AccountQuotaRecord[]` | Quota status across all accounts, providers, and pools |
| `toggle_account_warmup` | `{ accountId, enabled }` | `null` | Enables/disables automated kickstart for an account |
| `kickstart_account_quota` | `{ accountId }` | `null` | Triggers immediate manual PTY quota probe |
| `get_context_threshold` | `{ projectPath? }` | `number` | Resolves effective `accounts.context_warning_threshold_percent` |

*Note:* Errors serialize as `StatsError` in `ApiErrorResponse`.

### Daemon Events (SSE `/api/events`)

- `AccountQuotaUpdated`
- `account-quotas-updated`
- `keyring-warmup-status`
- `agent-usage-update`

### Configuration Keys (`[plugins.accounts]`, global-only unless cascade-scoped)

- `accounts.auto_warmup_enabled` (default `true`)
- `accounts.context_warning_threshold_percent` (default `30.0`)
- `accounts.quota_alert_threshold_percent`
- `accounts.quota_danger_threshold_percent`
- `accounts.stats_refresh_interval_seconds` (default `60`)
- `accounts.quota_depleted_threshold_percent` (default `5.0`)
- `accounts.quota_probe_cooldown_seconds` (default `900`)
- `accounts.quota_probe_timeout_ms` (default `7000`)
- `accounts.quota_probe_poll_step_ms` (default `150`)
- `accounts.quota_probe_model` (default `"gemini-3.8-flash"`)
- `accounts.quota_probe_effort` (default `"low"`)

## Implementation Status

| Item | Status | Evidence |
| :--- | :--- | :--- |
| `agent_session_metrics` & `account_quotas` schemas | Implemented | `src-tauri/src/db.rs` |
| Statusline hook, transcript reader, `get_agent_metrics`, `StatsError` | Implemented | `src-tauri/src/kopeng/stats.rs`, `scripts/agy_statusline.py` |
| `spawn_quota_watcher`, `probe_account_quota` (`libc::openpty`), `KeyringRestoreGuard` | Implemented | `src-tauri/src/kopeng/stats.rs`, `src-tauri/src/bin/daemon.rs` |
| All 6 metrics/quota daemon commands & SSE events | Implemented | `src-tauri/src/api.rs`, `src-tauri/src/events.rs` |
| `[plugins.accounts]` schema keys | Implemented | `src-tauri/src/config.rs` |
| UI telemetry bar, quota gauges & tooltips | Implemented | `src/stores/agentMetricsStore.ts`, `src/components/dialogue/FloatingChatOverlay.vue`, `src/components/ui/AppQuota*.vue` |

## Related Specs

- [Multi-Account Management](04-multi-account-management.md)
- [Thread, Conversation, and Runner-Neutral Context Continuity](02-thread-conversation-and-continuity.md)
- [MCP Dialogue Channel and Chat Overlay](03-mcp-dialogue-and-chat-overlay.md)
- [Configuration Schema and Cascade](../01-architecture/02-configuration-schema-and-cascade.md)

Supersedes legacy: `19_agent_quota_and_context_observability.md`, `23_token_economy_and_quota_kickstart.md`
