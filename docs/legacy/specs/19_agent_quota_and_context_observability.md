> [!WARNING]
> **LEGACY SPECIFICATION:** Superseded by [02-agents/05-quota-observability-and-token-economy.md](../../specs/02-agents/05-quota-observability-and-token-economy.md). Preserved for historical provenance only.

# Spec 19: Agent Quota, Token Usage, and Context Observability
**Document Version:** 0.4.0
**Domain:** Observability, Telemetry, Token Lifecycle, Quota Management
**Maintainer:** ONE-SHIP / Omarchy Development Team

---

## 1. Overview & Motivation

When AI agents (e.g., Antigravity `agy`, Claude Code, Codex) run within ONE-SHIP workspaces, understanding runway constraints in real time is vital for:
1. **Quota Management:** Preventing unexpected model lockouts across 5-hour rate limit windows and weekly quotas across multiple Google accounts.
2. **Context Window Health:** Proactively detecting context bloat (e.g. >75% window consumed) before truncation or latency degradation occurs, enabling timely auto-compaction and session cleanup.
3. **Cache Observability:** Monitoring prompt cache utilization (`cache_read_input_tokens` vs fresh `input_tokens`) to verify prompt caching efficiency across turns.

ONE-SHIP implements a unified metrics extraction layer that derives telemetry from two complementary sources:
- **Native StatusLine Ingestion Hook (`scripts/agy_statusline.py`):** Real-time JSON piped on stdin by `agy` on state changes, synchronously updating SQLite (`account_quotas` by authenticated email, and `agent_session_metrics` by conversation ID).
- **Antigravity Transcript Logs:** Detailed transactional metrics extracted directly from session transcripts (`transcript.jsonl`).

```
┌────────────────────────────────────────────────────────────────────────┐
│                      ANTIGRAVITY CLI (`agy`)                           │
│  Pipes periodic statusLine JSON on stdin upon state changes & turns    │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
          ┌─────────────────────────┴────────────────────────┐
          │                                                  │
          ▼                                                  ▼
┌─────────────────────────────────┐        ┌─────────────────────────────────┐
│   `scripts/agy_statusline.py`   │        │  `transcript.jsonl` Log Stream  │
│  Ingests stdin JSON & separates │        │  Accumulates turn token usage   │
│  Account Quota vs Conv Context  │        └─────────────────┬───────────────┘
└─────────────────┬───────────────┘                          │
                  │                                          │
                  ▼                                          ▼
┌─────────────────────────────────┐        ┌─────────────────────────────────┐
│        Direct SQLite Write      │        │   Direct Local Transcript Reader│
│  (`account_quotas` / metrics)   │        │     (`~/.gemini/.../logs/`)     │
└─────────────────┬───────────────┘        └─────────────────┬───────────────┘
                  │                                          │
                  └─────────────────┬────────────────────────┘
                                    │
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│               ONE-SHIP Rust Engine (`kopeng::stats`)                │
│  - Evaluates quota windows (5h, weekly) & reset times per account pool │
│  - Calculates context percentage & cache hit efficiency                │
│  - Exposes the daemon command `get_agent_metrics`                      │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Ingestion Channels

### 2.1 Native Antigravity StatusLine Ingestion Hook
When configured in `~/.gemini/antigravity-cli/settings.json`, Antigravity CLI pipes a live JSON state payload to `scripts/agy_statusline.py` via `stdin`:

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

The hook segregates data by scope into two decoupled tracks:
1. **Track 1: Account-Level Quota Runway (`account_quotas`):** Mapped deterministically by the observation's `account_id`, else its authenticated `email`, to `accounts.id`; an observation that names no registered account is rejected (never attributed to the active account). Simultaneously captures and updates rolling 5h and weekly remaining percentages, countdowns, and reset epochs for both Gemini (`Agy / gemini-pool`) and Third-Party (`Anthropic / 3p-pool`) model pools. Atomically writes the normalized snapshot to `~/.local/state/one-ship/agy-statusline.json`.
2. **Track 2: Conversation-Level Context & Token Observability (`agent_session_metrics`):** Scoped strictly to ONE-SHIP agent processes. Mapped by `conversation_id` against `agent_chat_turns` and accumulated directly from `~/.gemini/antigravity-cli/brain/<native_id>/.system_generated/logs/transcript.jsonl` (`input_tokens`, `cache_read_input_tokens`, `cache_creation_input_tokens`). Headless micro-prompts for account warmup probe only the agy pair `accounts.quota_probe_model` / `quota_probe_effort` (default `gemini-3.8-flash` / `low`) to avoid consuming Third-Party (Claude) quota.

### 2.2 Direct Transcript Log Ingestion
Antigravity stores immutable JSONL logs in:
`~/.gemini/antigravity-cli/brain/<conversation_id>/.system_generated/logs/transcript.jsonl`

Each assistant turn contains a `message.usage` or `usage` object:
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

The Rust engine accumulates token counts across all turns in the conversation:
- Total Input = sum(input_tokens)
- Total Read = sum(cache_read_input_tokens)
- Total Creation = sum(cache_creation_input_tokens)
- Cache Hit % = Total Read / (Total Input + Total Read + Total Creation) * 100

---

## 3. Rust Data Structures (`src-tauri/src/kopeng/stats.rs`)

```rust
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct UsageWindowStats {
    pub label: String,                // "5h", "7d"
    pub used_percent: f64,            // e.g. 15.0
    pub remaining_percent: f64,       // e.g. 85.0
    pub resets_at: Option<String>,     // ISO 8601 or countdown e.g. "3h 20m"
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

### 3.1 Metrics Resolution (as implemented)
`get_agent_metrics(project_path, agent_id, conversation)` is read-only and scoped to one agent of one project:
- **Native session:** the one named by `conversation` (`ConversationRef::Native`, or the newest agy session in the turns of a `ConversationRef::OneShip` Conversation), else the agent's live agy session, else the newest one recorded in its turns.
- **Context %:** the percentage recorded for the session by the statusline when positive, else total transcript tokens over the model's `context_window_tokens_count` from the runner manifest (`templates/runners/<runner>.toml`) of the agent's process. Without either, no context percentage is reported; the window is never guessed from the model name.
- **Quota windows:** read from `account_quotas` for the account the agent's process was launched with. An agent without a recorded account shows no quota windows; the active account is never substituted.
- Session metrics are persisted only by `post_agent_response` (`record_session_metrics`, keyed by the agent's ONE-SHIP Conversation). Quota rows are written only by the statusline sync and the warmup probe.

---

## 4. Daemon Commands & SSE Events (`kopeng/stats.rs`, Spec 12 §7.2)

1. `get_agent_metrics({ projectPath, agentId, conversationId })` → `AgentSessionMetrics` (`StatsError` on failure):
   Telemetry snapshot of one agent; `conversationId` is a ONE-SHIP Conversation ID. Runs on the blocking pool of the daemon.
2. `get_session_metrics({ agyConversationId })` → `Option<AgentSessionMetricsRecord>`:
   Reads persisted session metrics by native session ID.
3. `get_account_quotas({})` → `Vec<AccountQuotaRecord>`:
   Returns all persisted account quota windows across providers and model pools.
4. `toggle_account_warmup({ accountId, enabled })` → `()`:
   Enables or disables automatic 1-token quota kickstart probes for the specified account.
5. `kickstart_account_quota({ accountId })` → `()`:
   Triggers an immediate manual PTY quota probe (`probe_account_quota`).
6. `get_context_threshold({ projectPath })` → `f64`:
   Resolves the effective `accounts.context_warning_threshold_percent`.
7. **Live `agent-usage-update` SSE Event:** Emitted by `supervisor.rs` during OpenCode ACP (`usage_update`) and Claude Code (`result`) turns to stream live `tokens_used`, `max_context_tokens`, and `cost_dollars` into `agentMetricsStore.ts`.

---

## 5. UI Observability Integration (`FloatingChatOverlay.vue`)

1. **Telemetry Bar in Floating & Docked Chat:**
   - Positioned directly above the chat message stream and live terminal toggle.
   - **Context Gauge (`CTX XX.X%`):** Includes a visual horizontal meter with dynamic color coding:
     - < 60%: Standard accent (`bg-[var(--accent-color)]`)
     - 60% - 84%: Warning amber (`bg-amber-400`, `border-amber-500/80`)
     - >= 85%: Danger red (`bg-red-500`, `border-red-500/80`)
   - **Token Count & Cache Efficiency:** Renders total token footprint with cache hit % (e.g. `TOKENS: 42.1k (88% hit)`).
   - **Rate Limit Quota Badges:** 5-hour (`5H: XX%`) and 7-day (`7D: XX%`) remaining percentages with reset timing in `<AppTooltip>`.
   - **Account Indicator:** Displays the authenticated email handle under which the agent process was launched.
2. **Context Threshold Alerts & Auto-Compaction:**
   - When context reaches the configured threshold (Spec 23), displays prompts for clean checkpointing or session compaction.
