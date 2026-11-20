# 2026-11-03 — Phase 5 Module 13 (`duck.rs` Typed Errors)

Follow-up of `2026-11-03_phase5_manifest_migration_errors.md`.

## `TelemetryError`
- Variants: `RoleNotSet`, `RoleConflict`, `PoolUninitialized`, `Pool`, `Query { operation, source }`, `ConfigDirUnavailable`, `Io`, `WalLeftOver`, `DaemonUnreachable`, `DaemonHttp`, `InvalidResponse`, `Task`; `Serialize`. All 8 `Result<_, String>` of the module are converted; `get_continuous_profile_cmd` (telemetry.rs) returns it directly.
- `DuckResultExt::duck_context` now maps to `TelemetryError` and logs at ERROR (like `db::DbResultExt`), replacing ~20 hand-written `map_err` + log blocks and the `duck_context_str` variant.

## Fixes
- `get_duckdb_path` fell back to `.` without a config directory and discarded the `create_dir_all` error.
- Summary and hotspot columns were read with `unwrap_or(0)` / `"unknown"`, so a read error showed as zero metrics; they are read strictly (the count is read first; with samples, no aggregate is NULL). The duration computation no longer special-cases a `0` start.
- The profiler loop discarded `prune_old_samples` errors (`let _ =`); they are logged.
- The compaction's cleanup of an unused compacted file logs its failure instead of discarding it; a missing database file skips compaction, other `stat` errors are logged.
- The legacy-schema shim (drop `process_samples` when `sampled_at_ms` is missing) is removed (AGENTS.md §1.4); the live database already has the current schema (checked on a copy).
- The schema SQL was duplicated between `init_duckdb` and the test-only `init_duckdb_memory`; both use `open_pool` + `PROCESS_SAMPLES_SCHEMA_SQL`.
- Dead code removed: `kopeng::errors::KopengError` (its only uses were the unused `duck_context` and an unused `From<DatabaseError>` in db.rs); `extract_semantic_errors` stays.
- `telemetry.rs` `manual_checked_ops` clippy warning fixed.

## Verification
`cargo check --all-targets` clean; `clippy` 24 warnings (none in the touched files); `cargo test --lib`: 67 passed (duck tests strengthened: duration, hotspot order, unknown run → `None`), same 2 pre-existing `kopeng::resolution` failures. Backend `Result<_, String>`: 70. Spec 25 §3.2 item 6 added. Not run in the app.
