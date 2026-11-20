# 2026-11-01 — Audit Phase 4 (Backend Correctness) and Phase 5 Module 1 (`db.rs` Typed Errors)

Follow-up of `2026-11-01_full_audit_and_remediation_phases_1_3.md`. Order approved by the maintainer: 4 → 5 (one module per session) → 6 → 7.

## Phase 4 — Backend correctness
- **Conversation tracker (`conversation.rs`):** `mark_user_message_sent`, `mark_agent_responded` and `should_inject_compliance_reminder` take a mandatory `project_path: &str`; the bare-`agent_id` key fallback is gone (keys are always `<project_path>:<agent_id>`). Poison-safe `lock_state()` replaces five `lock().unwrap()`. The SQLite check runs outside the mutex and re-validates the state afterwards. `resolve_project_id` accepts only exact registered paths (name matching removed; all frontend callers pass paths). Dead `get_all_states` / `clear_agent_state` removed. Unit test added.
- **Blocking I/O in async:** ten Visual Builder commands run their synchronous bodies through `run_blocking` (`spawn_blocking`); `post_agent_response` artifact resolution moved to `resolve_post_artifact` on the blocking pool; greenfield scaffolding (`mkdir`, `git init`, registration) moved to `scaffold_greenfield_project` on the blocking pool; `tokio::fs` in `install_project_skill`, `toggle_project_rea_plugin`, the initializer uid read and profiler artifact writes.
- **Bounded input queue:** `pending_inputs` capped by `supervisor.max_pending_inputs_count` (default 16); a full queue rejects with an explicit error.
- **DuckDB:** `CHECKPOINT` after each prune; startup compaction (`COPY FROM DATABASE` into a fresh file) when the file exceeds `telemetry.duckdb_compaction_threshold_mbytes` (default 256). Unit test added.

## Phase 5 — module 1: `db.rs`
- New `DatabaseError` (`thiserror`): `PoolUninitialized`, `PoolAlreadyInitialized`, `Pool`, `Query { operation, source }`, `Serialization`, `NotFound`, `InvalidState`, `InvalidRequest`. Every `db_*` function returns it; Tauri commands convert at the IPC boundary (`map_err(String::from)`).
- `DbResultExt::db_context` now logs the failure at ERROR, replacing 300+ hand-written `map_err` closures; `db_context_unlogged` and `get_connection_unlogged` serve the logger's own sink to avoid feedback loops.
- Row readers share `*_from_row` helpers and `collect_rows` (first row error fails the query; the previous `if let Ok` / `flatten()` dropped rows silently). Filtered queries use `(?N IS NULL OR col = ?N)` instead of duplicated statements.
- Behavior fixes found during conversion: `db_set_active_account` no longer falls back to provider `google` for an unknown account and runs in one transaction; `db_switch_active_conversation` is transactional and fails on an unknown target; `db_resolve_init_flag` fails on an unknown flag (it updated nothing and still emitted the event); `db_insert_vb_snapshot` prunes inside its transaction; `init_database` replaces silent `let _ = ALTER TABLE` with `ensure_column` and fails on index creation errors; dead `db_insert_log` and the `registry.json` / `delegations.json` import shims removed; the duplicated `init-flag-resolved` event from the MCP tool removed.
- Dependent types: `CallerError::Database` and `BriefError::Database` wrap `DatabaseError`; `mcp.rs` topic switch propagates lookup errors instead of creating a duplicate conversation.

## Verification
- `cargo check --all-targets`: clean. `cargo clippy`: no warnings in `db.rs` (11 before). `cargo test --lib -- db::tests conversation::tests logger::tests duck::tests`: 12 passed.
- Runtime checks pending (maintainer-run): TODO "Audit 2026-11-01 follow-up".

## Specs updated
08 §2.4 (bounded input queue), 10 (new `supervisor.max_pending_inputs_count`, `telemetry.duckdb_compaction_threshold_mbytes`), 25 §3 (checkpoint and startup compaction).
