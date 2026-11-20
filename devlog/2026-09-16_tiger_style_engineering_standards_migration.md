# Devlog: 2026-09-16 — Tiger Style Engineering Standards Global Migration

## Summary of Changes

Executed a full-stack architectural standardization implementing **Tiger Style Engineering Principles** across documentation, runtime specifications, backend services, SQLite/DuckDB schemas, and Vue 3 frontend components.

### 1. Governance & Living Specifications
- **`AGENTS.md`**: Formalized Section 8 ("Tiger Style Engineering Standards: Performance, Safety & Contracts") codifying explicit unit suffixes, aggressive assertions (`debug_assert!`), bounded ring buffers, deterministic teardown loops, and zero unbounded dynamic growth.
- **`docs/specs/25_universal_telemetry_and_profiling_engine.md`**: Standardized DuckDB analytical tables (`process_samples`), in-vivo memory bounds (10k items), auto-pruning retention sweeps, and explicit units (`sampled_at_ms`, `cpu_usage_percent`, `rss_memory_bytes`, `duration_seconds`).
- **`docs/specs/08_native_process_supervisor.md`**: Updated with configurable ring buffer bounds (`max_buffer_lines_count`), `shutdown_grace_period_ms`, and `state_watcher_interval_ms`.
- **`docs/specs/23_token_economy_and_quota_kickstart.md`**: Standardized rolling quota ledger schemas (`five_hour_reset_epoch_seconds`, `weekly_reset_epoch_seconds`).

### 2. Centralized Persistent Configuration (`config.rs` & `config.toml`)
- Added `telemetry` and `supervisor` plugin schemas to `get_all_plugin_schemas()` exposing:
  - `telemetry_interval_ms` (default: 2000)
  - `profiler_sample_interval_ms` (default: 500)
  - `retention_window_hours` (default: 2)
  - `prune_interval_minutes` (default: 15)
  - `max_buffer_lines_count` (default: 500)
  - `shutdown_grace_period_ms` (default: 3000)
  - `state_watcher_interval_ms` (default: 2500)
- Implemented typed numeric resolvers: `resolve_effective_usize` and `resolve_effective_u64`.

### 3. DuckDB Analytical Engine & Telemetry (`duck.rs`, `telemetry.rs`, `profiler.rs`)
- Renamed columns, structs, and IPC payloads with explicit unit suffixes:
  - `ProcessSample`: `sampled_at_ms`, `cpu_usage_percent`, `rss_memory_bytes`, `rss_memory_mbytes`.
  - `ProcessHotspot` & `ContinuousProfileSummary`: `samples_count`, `total_samples_count`, `peak_memory_mbytes`, `avg_memory_mbytes`.
- Added preallocated batch vectors (`Vec::with_capacity(10)`), auto schema validation, and isolated in-memory DuckDB for unit tests (`init_duckdb_memory()`).

### 4. Native Process Supervisor (`supervisor.rs`)
- Standardized `ManagedProcess` with `started_at_ms: u64`.
- Replaced hardcoded ring buffer capacities with dynamic configuration readers (`get_supervisor_max_buffer_lines_count()`).
- Refactored `stop_process` into a bounded graceful polling loop (`poll_step_ms: 50`, `max_attempts_count = grace_period_ms / 50`) before forceful kill, with assertions `debug_assert!(proc.pid > 0)`.
- Standardized `read_output(&self, key: &str, max_lines_count: usize)`.

### 5. Token Economy, Quotas & Database Ledgers (`stats.rs`, `db.rs`, `mcp.rs`, `agy_statusline.py`)
- Standardized token structures:
  - `UsageWindowStats`: `reset_epoch_seconds`.
  - `CacheUsageStats`: `fresh_input_tokens_count`, `read_tokens_count`, `creation_tokens_count`.
  - `AgentSessionMetrics`: `captured_at_epoch_seconds`.
- Standardized SQLite tables (`agent_session_metrics` & `account_quotas`) and query functions with schema migrations for existing databases.
- Updated `scripts/agy_statusline.py` and `mcp.rs` tool handlers.

### 6. Frontend Stores & Components (`dialogueStore.ts`, `coreStore.ts`, `ConversationPanel.vue`)
- Synchronized TypeScript interfaces to consume the renamed IPC fields.
- Verified zero type errors with `vue-tsc --noEmit && vite build`.

---

## Verification
- `cargo test --lib`: 23 passed, 0 failed.
- `cargo check --bins`: 0 errors.
- `npm run build`: 0 errors, successful bundle generation.
