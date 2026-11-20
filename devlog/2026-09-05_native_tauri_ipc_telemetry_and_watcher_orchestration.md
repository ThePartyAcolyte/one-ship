# Devlog: Native Tauri IPC Telemetry & Watcher Orchestration

**Date:** 2026-09-05  
**Author:** Bosmang (Lead Systems Architect & Supervisor)  
**Topic:** Transition from Daemon WebSocket Broadcasting to In-App Native Tauri IPC Event Emission

---

## 1. Context & Architectural Objective

The `kapawu-daemon` (`src-tauri/src/bin/daemon.rs`) was refactored by the MCP specialist to serve exclusively as a Streamable HTTP Model Context Protocol (MCP) server on port `8765`. WebSocket broadcasting (`/ws`) from the daemon was eliminated.

To maintain continuous, reactive real-time telemetry (CPU, RAM, Swap, Disk, Network speed) and project/Herder state without relying on daemon sockets:
1. The background watchers (`spawn_project_watcher`, `spawn_telemetry_loop`, `spawn_herdr_watcher`) were transitioned directly into the **Tauri application lifecycle** (`src-tauri/src/lib.rs`).
2. Events are emitted directly to the webview using native Tauri IPC (`app.emit(...)`).
3. The frontend (`coreStore.ts`) transitions from WebSocket polling/reconnection to native Tauri `listen(...)` hooks.

---

## 2. Implementation Details

### A. Backend Watcher Spawning & IPC Forwarding (`src-tauri/src/lib.rs`)
In `tauri::Builder::setup(|app| { ... })`:
- Initialized a `tokio::sync::broadcast::channel::<DaemonEvent>(100)` and a `tokio_util::sync::CancellationToken`.
- Spawned an async event forwarding worker subscribing to the broadcast channel and emitting each event natively via `tauri::Emitter`:
  ```rust
  let mut event_rx = tx.subscribe();
  let app_handle = app.handle().clone();
  tauri::async_runtime::spawn(async move {
      use tauri::Emitter;
      while let Ok(evt) = event_rx.recv().await {
          let _ = app_handle.emit(&evt.event_type, evt.payload);
      }
  });
  ```
- Spawned core watchers directly in the Tauri async runtime:
  - `crate::watcher::spawn_project_watcher(tx.clone(), cancel_token.clone())` -> emits `projects-updated`
  - `crate::telemetry::spawn_telemetry_loop(tx.clone(), cancel_token.clone())` -> emits `system-telemetry`
  - `crate::kopeng::herdr::spawn_herdr_watcher(tx, cancel_token)` -> emits `herdr-states-updated`, `running-services-updated`, and `agent-status-report`
- Verified clean compilation with `cargo check`.

### B. Frontend IPC Listeners Integration (Frontend Specialist)
Delegated Task `task-1788624919` to the `frontend` agent (`w8:pZ`) via `delegate_agent_task`:
1. **`src/stores/coreStore.ts`:**
   - Removed WebSocket state (`ws: WebSocket | null`) and `connectWebSocket()`.
   - Imported `listen` from `@tauri-apps/api/event`.
   - Implemented `async initTauriListeners()` subscribing to:
     - `system-telemetry`
     - `projects-updated`
     - `running-services-updated`
     - `herdr-states-updated`
     - `agent-status-report`
2. **`src/App.vue`:**
   - Updated `onMounted()` to call `coreStore.initTauriListeners()`.
3. Verified frontend compilation with `npm run build` (`vue-tsc --noEmit && vite build`).
