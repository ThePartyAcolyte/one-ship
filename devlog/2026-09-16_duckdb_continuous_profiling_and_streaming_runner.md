# Development Log: 2026-09-16 — DuckDB Continuous Profiling & Streaming Runner

## Context & Objectives
1. **Passive Continuous Profiling via DuckDB:** Move away from intrusive, synthetic 15-second manual profiling benchmarks. Implement continuous, high-frequency (500ms) process tree sampling during normal developer iteration (HMR, browser testing) storing time-series samples directly into an embedded DuckDB database (`kapawu_telemetry.duckdb`).
2. **Instant Performance Analysis:** Enable agents and developers to query in-vivo performance metrics (p50, p95 CPU via `QUANTILE_CONT`, peak RAM, top process hotspots) on the active run or recent window immediately in <10ms.
3. **Automated Bounded Retention:** Maintain a rolling 2-hour TTL with periodic pruning to ensure DuckDB remains lean, ephemeral, and memory-efficient.
4. **Streaming Script Runner (Tech Debt #11):** Migrate one-shot script/task executions in `runner.rs` from blocking `tokio::process::Command::output().await` to `ProcessSupervisor::spawn_task` with a 500-line ring buffer and real-time `task-output-chunk` event streaming.

---

## Architectural & Implementation Details

### 1. Embedded DuckDB Engine (`src-tauri/src/duck.rs`)
- Added `duckdb = { version = "1.10505.0", features = ["bundled", "r2d2"] }` to `src-tauri/Cargo.toml`.
- Implemented `init_duckdb()` managing connection pools and table schema:
  - Table: `process_samples (timestamp_ms, project_id, target_type, target_id, run_id, pid, process_name, cmdline, cpu_percent, memory_bytes, memory_mb)`.
  - Composite Index: `idx_samples_target_run ON process_samples (target_id, run_id, timestamp_ms)`.
- Batch insertion via DuckDB `Appender` API for zero-overhead writes during rapid 500ms sampling.
- Implemented `get_continuous_profile_summary` computing `AVG(cpu_percent)`, `QUANTILE_CONT(cpu_percent, 0.50)` (p50), `QUANTILE_CONT(cpu_percent, 0.95)` (p95), `MAX(memory_mb)`, and process-level hotspot ranking.
- Implemented `prune_old_samples` discarding samples older than configurable threshold.

### 2. High-Frequency Continuous Telemetry Loop (`src-tauri/src/telemetry.rs`)
- Spawned `spawn_continuous_profiler_loop` sampling all running services supervised by `ProcessSupervisor` every 500ms.
- Traverses full descendant process trees via `sysinfo`.
- Generates and writes batch rows to DuckDB.
- Executes automated retention pruning every 15 minutes (2-hour rolling window).
- Added `get_continuous_profile_cmd` Tauri IPC command for frontend observability.

### 3. Streaming Task Runner (`src-tauri/src/supervisor.rs` & `src-tauri/src/kopeng/runner.rs`)
- Extended `ProcessSupervisor` with:
  - `format_task_key(project_path, task_id, exec_id)`.
  - `spawn_task` running one-shot commands with a 500-line ring buffer.
  - Line-by-line streaming emitting `DaemonEvent` `"task-output-chunk"`:
    `{ project_id, task_id, exec_id, line, is_stderr }`.
  - Upon task completion, emits `"task-completed"` and returns structured `TaskRunResult`.
- Updated `runner.rs` to execute all project tasks through `ProcessSupervisor::spawn_task`, completely eliminating blocking `cmd.output().await`.

### 4. Continuous Profiler Integration (`src-tauri/src/kopeng/profiler.rs` & `src-tauri/src/mcp.rs`)
- Updated `execute_profiling_session`: when `duration_seconds` is omitted, checks DuckDB for active continuous profiling samples. If present, immediately computes the summary, generates the SVG visual artifact, stores the run record, and returns results in <10ms.
- Enhanced `query_profiler_hotspots_cmd` and MCP tool `query_profiler_hotspots` with automatic DuckDB fallback for continuous telemetry runs.

---

## Verification & Validation
- `cargo check --lib` & `cargo check --bins`: 0 errors.
- `cargo test --lib`: 21 passed (including `test_supervisor_spawn_task_streaming` and `test_duckdb_insert_and_summary_percentiles`).
- `npm run build`: 0 errors, production bundle compiled cleanly.
- Updated `docs/specs/25_universal_telemetry_and_profiling_engine.md` to v0.3.0.
- Marked Tech Debt #11 and #21 as resolved in `TECHNICAL_DEBT.md`.
