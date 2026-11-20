# Development Log: Log Viewer Cursor Pagination & Universal Backend Logging Instrumentation

**Date:** 2026-09-23  
**Topic:** Log Viewer Infinite Scrollback, Non-Blocking SQLite Logging Worker, and Repository-Wide Logging Instrumentation (from 138 to 939 Log Statements)

---

## 1. Summary of Changes

### 1.1 Non-Blocking SQLite Logging Engine (`src-tauri/src/logger.rs` & `src-tauri/src/db.rs`)
- **Problem:** Every `logger::log()` call previously acquired a synchronous mutex and performed a synchronous SQLite insert (`INSERT INTO system_logs`), risking disk I/O thrashing and lock contention when instrumenting hundreds of functions.
- **Resolution:**
  - Introduced a non-blocking `sync_channel::<QueuedLog>(4096)` in `src-tauri/src/logger.rs` with opportunistic draining.
  - Dedicated background thread (`kapawu-sqlite-logger`) batches writes and commits them within an explicit transaction (`tx.commit()`) via `db_insert_logs_batch` in `src-tauri/src/db.rs`.
  - Added poison-safe mutex locking (`if let Ok(_lock) = LOG_MUTEX.lock()`) for physical file appending to `kapawu.log`.

### 1.2 Cursor-Based Paging in Backend (`src-tauri/src/db.rs`)
- **Problem:** The system log query relied strictly on `LIMIT` and `OFFSET`, causing duplicated or skipped logs when new entries were inserted concurrently.
- **Resolution:**
  - Extended `db_get_system_logs` and Tauri command `get_system_logs` to support `before_id: Option<i64>` and `after_id: Option<i64>`.
  - Backward pagination (`before_id`) queries `id < ? ORDER BY id DESC LIMIT ?`.
  - Forward background polling (`after_id`) queries `id > ? ORDER BY id ASC LIMIT ?`.
  - Synchronized caller signatures in `src-tauri/src/mcp.rs` and `src-tauri/src/supervisor.rs`.

### 1.3 Infinite Backward Scroll & Memory Bounding in Log Viewer (`src/components/floating/RollingStatusLog.vue`)
- **Problem:** 
  1. The 3-second polling timer (`setInterval(fetchLogs, 3000)`) completely wiped out `dbLogs.value`, resetting the viewport to the latest 50 logs whenever the user was reading earlier history.
  2. The `DEBUG` log level was absent from `levelOptions`.
  3. No memory bounding existed for backward lazy-loading.
- **Resolution:**
  - Implemented `loadOlderLogs()` on `scrollTop <= 50` fetching with `beforeId: oldestId.value`, prepending older logs and maintaining exact visual scroll anchor (`scrollTop = newScrollHeight - oldScrollHeight`).
  - Implemented `pollNewLogs()` polling every 2.5s with `afterId: latestId.value`, appending new logs without clearing or moving the scroll position unless the user is already anchored at the bottom.
  - Added `DEBUG` to `levelOptions` with brutalist cyan styling (`text-cyan-400 opacity-70`).
  - Added an in-memory sliding buffer cap (`MAX_LOGS_IN_MEMORY = 1000`) preventing DOM bloat.

### 1.4 Universal Backend Logging Instrumentation (4 Flash Subagents)
Deconstructed into 4 parallel execution domains, instrumenting 24 files across the entire backend:
1. **Persistence Layer (`db.rs`, `duck.rs`, `registry.rs`):**
   - Expanded `db.rs` from 6 logs to **403 logs**, covering every query, mutation, snapshot, task execution, flag, and quota operation.
   - `duck.rs` instrumented from 2 to **43 logs**.
   - `registry.rs` instrumented from 2 to **19 logs**.
2. **Process Supervision & Telemetry (`supervisor.rs`, `watcher.rs`, `orchestrator.rs`, `telemetry.rs`):**
   - `supervisor.rs` expanded from 20 to **59 logs**, instrumenting process states, stream readers, agent turn dispatches, and poison-safe lock wrappers.
   - `watcher.rs`, `orchestrator.rs`, and `telemetry.rs` instrumented with lifecycle and tick metrics.
3. **Kopeng Domain Modules (`accounts.rs`, `initializer.rs`, `flag_resolver.rs`, `runner.rs`, `snapshots.rs`, `skill.rs`, `git.rs`):**
   - Comprehensive `DEBUG` on heuristics, prompts, file diffs, and hashing; `INFO` on logins, flag resolutions, and snapshot mutations.
4. **MCP Server & Visual Builder (`mcp.rs`, `visual_builder.rs`):**
   - `mcp.rs` expanded from 17 to **171 logs**, tracking caller `agent_id` across every JSON-RPC endpoint.
   - `visual_builder.rs` expanded from 14 to **87 logs**, instrumenting AST parsing, class/style mutations, and Dev Server lifecycle.

Total backend logging density increased from **138 to 939 log statements**.

### 1.5 Governance & Protocol Formalization (`AGENTS.md`)
- Added Section 12 (*Universal Logging & Diagnostic Observability Standards*) codifying:
  1. Mandatory universal coverage (Zero Silent Code Invariant).
  2. Strict 4-tier taxonomy (`DEBUG`, `INFO`, `WARN`, `ERROR`).
  3. Non-blocking asynchronous persistence invariant (bounded MPSC queue + SQLite batched transactions).
  4. Cursor-based paging and bounded infinite scrollback in log viewers.

---

## 2. Verification

- `cargo check --manifest-path src-tauri/Cargo.toml`: Finished in 0.36s (0 errors, 0 warnings).
- `cargo test --manifest-path src-tauri/Cargo.toml --lib`: 27/27 unit tests passed.
- `npm run build`: Compiled cleanly via `vue-tsc --noEmit && vite build` in 5.06s.
