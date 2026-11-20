# Devlog: 2026-09-10 - Agent Quota, Context Observability & Pane-Scoped Account Switching

## Objectives
1. Reverse engineer and analyze the `herdr-agent-quota` plugin (`/home/butcherwutcher/herdr-agent-quota`) and understand its interaction with Antigravity (`agy`) and the Herder Unix socket (`herdr.sock`).
2. Implement backend telemetry and stats aggregation in Wang-Kapawu to extract remaining weekly quota, 5-hour rolling limits, context window percentage, and prompt cache hit efficiency.
3. Design and implement the backend foundation for window/pane-scoped temporary account switching (Spec 17 Phase 2), allowing an isolated conversation to run under a secondary account while preserving the system's global default keyring.
4. Formalize specifications in `docs/specs/` (Spec 19 and Spec 17 updates), record technical debt for deferred UI integration, and update active tasks.

---

## Key Changes & Architecture

### 1. Specification 19: Agent Quota, Token Usage, and Context Observability
- Authored [`docs/specs/19_agent_quota_and_context_observability.md`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/docs/specs/19_agent_quota_and_context_observability.md) covering:
  - Ingestion of Herder metadata tokens (`quota_provider`, `quota_model`, `quota_5h_*`, `quota_week_*`, `quota_context`, `quota_cache`, `quota_cache_ttl`).
  - Direct local parsing of Antigravity session transcripts (`transcript.jsonl`) to compute cumulative token counts (`input_tokens`, `cache_read_input_tokens`, `cache_creation_input_tokens`) and calculate cache hit ratios.
  - Rust data model contracts and Tauri IPC interfaces.

### 2. Specification 17 (Phase 2): Pane-Scoped Temporary Account Override
- Updated [`docs/specs/17_multi_account_management.md`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/docs/specs/17_multi_account_management.md) with Section 13 detailing the sequential atomic swap pattern:
  1. Capture and remember global default keyring credentials.
  2. Write target secondary account credentials to keyring via `secret-tool`.
  3. Send relaunch command `agy --conversation <conv_id>` to the specific Herder pane.
  4. Allow brief handshake window (250ms) for `agy` to read credentials into memory.
  5. Restore global default account into system keyring, keeping other panes and new sessions unaffected.

### 3. Backend Engine Implementation (`src-tauri/src/kopeng/stats.rs`)
- Implemented `parse_pane_tokens()` to extract and normalize rolling usage windows, severity classifications (`normal`, `warning`, `danger`), context %, and cache stats from Herder pane metadata.
- Implemented `parse_transcript_tokens()` to read and sum token usages across JSONL turns for precise cache efficiency auditing.
- Implemented `get_agent_metrics()` and `get_all_active_agent_metrics()` with fallback heuristics and hybrid telemetry enrichment.
- Exposed `get_agent_metrics_cmd` and `get_all_active_agent_metrics_cmd` in Tauri invoke handlers.

### 4. Pane Account Switch Command (`src-tauri/src/kopeng/accounts.rs`)
- Implemented `switch_pane_account()` and exposed `switch_pane_account_cmd` as a non-blocking asynchronous Tauri command (`tokio::task::spawn_blocking`).
- Enhanced `HerdrPane` and `HerdrAgent` in [`src-tauri/src/kopeng/herdr_socket.rs`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/kopeng/herdr_socket.rs) with `tokens` and `agent_session` deserialization.

### 5. Technical Debt & Deferred UI Integration
- Added Item 15 to [`TECHNICAL_DEBT.md`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/TECHNICAL_DEBT.md) detailing the upcoming UI components:
  - Header metric chips in [`ConversationPanel.vue`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/dialogue/ConversationPanel.vue) (Context Gauge %, Cache Efficiency %, 5h/7d Quotas).
  - Contextual window-scoped account dropdown selector.
  - Proactive auto-compaction and threshold alerts.
- Synchronized [`TODO.md`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/TODO.md).

---

## Verification & Testing
- Unit tests executed:
  - `kopeng::stats::tests::test_parse_pane_tokens` -> Passed.
  - `kopeng::accounts::tests::*` -> Passed (key derivation, JWT extraction, AES-GCM encryption/decryption roundtrip, active session scanning).
  - `kopeng::snapshots::tests::*` -> Passed.
  - `kopeng::herdr_socket::tests::*` -> Passed.
- Overall test suite: 10 passed; 0 failed.
