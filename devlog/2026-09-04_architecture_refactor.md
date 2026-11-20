# Session Continuation Summary — Wang-Kapawu Architectural Audit & Remediation

**Date:** 2026-09-04
**Topic:** Core Architectural Decoupling (Daemon & Vue Frontend)

## Objective
The primary goal of this session was to execute a radical architectural refactor of Wang-Kapawu, moving away from a monolithic Tauri structure where the frontend (`App.vue`) and background workers were tightly coupled, towards a decoupled, scalable, and high-performance system.

## Actions Executed

### 1. Phase 3: Daemon Persistent Architecture
*   **Decoupled Backend Engine:** Created `src-tauri/src/bin/daemon.rs` as the authoritative background daemon.
*   **Database Concurrency:** Implemented `r2d2` connection pooling for SQLite in `db.rs` to eliminate locking bottlenecks across multiple asynchronous workers.
*   **Asynchronous Workers:** Refactored the core system loops (`telemetry`, `watcher`, `opendictate`, and `herdr`) to run purely as Tokio asynchronous tasks, completely independent of the Tauri `AppHandle`.
*   **Event Broadcast:** Introduced `tokio::sync::broadcast` and `CancellationToken` for graceful shutdown and real-time state emission (`DaemonEvent` with JSON payloads).
*   **WebSocket Endpoint:** Deployed a native `/ws` endpoint via Axum (listening on port `8765`), sharing the router with the existing `/mcp` endpoints.

### 2. Phase 4: Vue Frontend Decoupling
*   **Pinia State Management:** Integrated `pinia` to manage the application's global state, eliminating the 700-line reactivity bottleneck in `App.vue`.
*   **Native WebSocket Client:** Created `src/stores/coreStore.ts` to manage the WebSocket connection (`ws://127.0.0.1:8765/ws`), parse `DaemonEvent` payloads, and maintain a centralized state (projects, telemetry, dictation sessions, and agent outcomes).
*   **IPC Elimination:** Removed all `@tauri-apps/api/event` listeners from the Vue layer. The UI now renders strictly through computed properties synced with the Pinia store.

## Deferred Tasks & Technical Debt
As per the user's explicit directive ("van a ser cuatro. No le des tantas vueltas"), the refactoring session concluded at Phase 4. The planned Phase 5 (Backend Optimizations) was deferred.

The following items were registered in `TECHNICAL_DEBT.md` and `TODO.md`:
*   **O(N) Polling in Herder Watcher:** The watcher executes `herdr` CLI commands per project loop. This needs inversion to fetch global states once per cycle.
*   **Blocking Shell Calls:** Replace `std::process::Command` with `tokio::process::Command` in async worker loops to prevent thread starvation.
*   **Double-Fetch in Project Watcher:** Eliminate redundant file system checks that trigger an immediate re-fetch of projects.
*   **Command Injection:** Sanitize the `sh -c` execution wrapper in the orchestrator.

## Conclusion
The architectural mandate of absolute non-backward compatibility was strictly followed. The intermediary broken states were resolved, and both the `kapawu-daemon` binary and the decoupled Vue frontend compile and function harmoniously. The system is now primed for scalable multi-agent tracking and dynamic "Zone" UI injections.
