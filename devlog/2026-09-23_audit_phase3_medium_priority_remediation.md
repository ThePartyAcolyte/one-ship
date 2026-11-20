# Development Log: Code Audit Phase 3 (Medium Priority Findings) Remediation

**Date:** 2026-09-23  
**Topic:** Medium Priority Remediation: Settings Schema Expansion, I/O Monitor Deduplication, Config Resolution Helper, Ergonomic DB Extension Traits, Stepwise Migration Logging, Onlook Client Bridge Composable, and Token Consolidation (M-01 to M-08)

---

## 1. Summary of Changes

Phase 3 of the comprehensive codebase audit addressed maintainability, user configurability, and code duplication across the Rust core supervisor, database layer, and visual builder tab.

### 1.1 M-01: Expose Hardcoded Values in Settings (`config.rs`, `db.rs`, `logger.rs`, `profiler.rs`, `stats.rs`, `daemon.rs`, `initializer.rs`, `flag_resolver.rs`, `fs_tree.rs`, `supervisor.rs`, `watcher.rs`, `mcp.rs`, `ProjectVisualBuilderTab.vue`)
- **Problem:** Multiple timing constants, limits, and paths were hardcoded as fixed constants instead of configurable settings exposed in `SettingsModal.vue`.
- **Resolution:**
  - Registered 14 new configuration fields with Tiger Style unit suffixes in `get_all_plugin_schemas()` in `src-tauri/src/config.rs`:
    - `general`: `log_max_size_bytes` (512,000), `log_max_rotations_count` (2), `fs_tree_max_depth_count` (12), `default_projects_root_path` ("/home/butcherwutcher/Projects/dev")
    - `orchestrator`: `agent_spawn_poll_step_ms` (250), `agent_log_tail_lines_count` (50)
    - `accounts`: `stats_refresh_interval_seconds` (60)
    - `supervisor`: `shutdown_step_delay_ms` (150), `process_poll_step_ms` (100), `watcher_debounce_ms` (1500)
    - `telemetry`: `db_connection_pool_max_count` (4)
    - `mcp`: `mcp_daemon_port` (8765), `approval_poll_delay_ms` (500), `approval_ttl_ms` (15000)
  - Updated all call sites in `db.rs`, `logger.rs`, `profiler.rs`, `stats.rs`, `daemon.rs`, `initializer.rs`, `flag_resolver.rs`, `fs_tree.rs`, `supervisor.rs`, `watcher.rs`, and `mcp.rs` to dynamically resolve values using `resolve_effective_usize` / `resolve_effective_u64`.
  - In `ProjectVisualBuilderTab.vue`, replaced hardcoded `50` snapshot cap with dynamic `maxUndoSnapshotsCount` populated from `get_effective_project_config` on mount.

### 1.2 M-02: Shared I/O Monitors in `supervisor.rs`
- **Problem:** Stdout and stderr Tokio reader loops were copy-pasted across `spawn_service`, `spawn_task`, and `spawn_agent`.
- **Resolution:**
  - Extracted shared helper functions `spawn_stream_reader` and `spawn_io_monitors` in `src-tauri/src/supervisor.rs`.
  - Replaced duplicate reader tasks in `spawn_service`, `spawn_task`, and `spawn_agent`, eliminating ~150 lines of duplicate async reader code while preserving output ring buffer bounded guarantees and NDJSON agent streaming.

### 1.3 M-03: Unified Agent Configuration Resolution (`src-tauri/src/kopeng/resolution.rs`)
- **Problem:** Fallback resolution logic for `runner`, `model`, and `effort` was copied between `initializer.rs` and `flag_resolver.rs`.
- **Resolution:**
  - Created `src-tauri/src/kopeng/resolution.rs` exposing `resolve_effective_agent_config(...)`.
  - Registered module in `src-tauri/src/kopeng/mod.rs` and updated both `initializer.rs` and `flag_resolver.rs` to use the unified function.

### 1.4 M-04: Ergonomic DB Extension Traits (`DbResultExt` in `db.rs` and `DuckResultExt` in `duck.rs`)
- **Problem:** Database operations repeated `.map_err(|e| format!("Failed to {operation}: {}", e))?` hundreds of times across database modules.
- **Resolution:**
  - Added `DatabaseOp` and `DuckDbOp` variants to `KopengError` in `src-tauri/src/kopeng/errors.rs`.
  - Implemented `DbResultExt<T>` on `rusqlite::Result<T>` in `src-tauri/src/db.rs` providing `.db_context(op)?` and `.db_context_str(op)?`.
  - Implemented `DuckResultExt<T>` on `duckdb::Result<T>` in `src-tauri/src/duck.rs` providing `.duck_context(op)?` and `.duck_context_str(op)?`.

### 1.5 M-05: Stepwise Migration Logging (`src-tauri/src/kopeng/migration.rs`)
- **Problem:** `migration.rs` logged an aggregate summary message at the conclusion of migration, with insufficient per-step progress logs.
- **Resolution:**
  - Added stepwise `DEBUG` and `INFO` log pairs for each migration step in `run_deterministic_migration`.
  - Added explicit per-step completion logs for platform skill sync, AGENTS.md managed block updates, and `rula.toml` version increments.

### 1.6 M-06: Onlook Client Bridge Composable (`src/composables/useOnlookClientBridge.ts`)
- **Problem:** Raw `iframeRef.value?.contentWindow?.postMessage(...)` calls were scattered throughout `ProjectVisualBuilderTab.vue`, mixing iframe transport details with component UI logic.
- **Resolution:**
  - Created `src/composables/useOnlookClientBridge.ts` exposing typed helper methods (`syncInspectMode`, `previewClasses`, `notifyHistoryRestored`, `previewSrc`, `refreshStyles`).
  - Refactored all 6 `contentWindow.postMessage` call sites in `ProjectVisualBuilderTab.vue` to use the composable.

### 1.7 M-07: Unregistered `run_project_task` Command (`lib.rs` & `runner.rs`)
- **Problem:** `run_project_task` in `kopeng/runner.rs` was marked with `#[tauri::command]` but missing from `tauri::generate_handler![...]` in `src-tauri/src/lib.rs`.
- **Resolution:**
  - Registered `kopeng::runner::run_project_task` in `generate_handler` in `src-tauri/src/lib.rs`.
  - Removed duplicate snapshots command registration in `lib.rs`.

### 1.8 M-08: Consolidated Active Tab Tokens in `ProjectVisualBuilderTab.vue`
- **Problem:** The active/inactive tab token styling string was copy-pasted dozens of times in the visual builder template.
- **Resolution:**
  - Created `getActiveTokenClasses(isActive: boolean)` helper in `ProjectVisualBuilderTab.vue`.
  - Replaced repetitive ternary template strings with the consolidated helper.

---

## 2. Verification & Status

1. **Rust Backend:**
   - `cargo check --manifest-path src-tauri/Cargo.toml` passed with 0 errors (3.43s).
2. **Frontend UI:**
   - `npm run build` (`vue-tsc --noEmit && vite build`) passed with 0 errors (3.60s).
3. **Status:** All 8 Medium Priority findings (M-01 through M-08) are 100% remediated and verified.
