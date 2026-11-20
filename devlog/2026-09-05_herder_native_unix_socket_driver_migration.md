# Work Session Log: Herder Native Unix Domain Socket Driver Migration

**Date:** 2026-09-05  
**Status:** Completed  
**Domain:** Process Orchestration, Terminal Multiplexing, Real-time IPC  

---

## 1. Context & Problem Statement

Wang-Kapawu previously interacted with the Herder terminal workspace manager exclusively via CLI subprocess spawning (`std::process::Command::new("herdr")`):
- Background watcher (`spawn_herdr_watcher`) spawned 3 separate CLI processes every 2,500 ms.
- Agent and service lifecycle actions spawned 3–6 sequential processes each.
- Incurred OS process scheduling and fork/exec overhead (~2–5 ms per call).
- State polling introduced up to 2.5s latency for agent state detection.
- Crucial server primitives like `tab.move` (arbitrary tab reordering) and `events.subscribe` (real-time push notifications) were inaccessible through the high-level CLI.

---

## 2. Protocol Analysis & Architectural Discoveries

Inspection of Herder's Protocol 20 source code (`herdrdev/herdr`) and live socket verification against `~/.config/herdr/herdr.sock` revealed key wire characteristics:
1. **Connection Model:** Standard RPC calls (`session.snapshot`, `tab.create`, `pane.send_input`, etc.) operate on a request-per-connection pattern where the server sends the JSON response line and closes the stream.
2. **Dedicated Event Stream:** `events.subscribe` holds an open, persistent streaming connection that emits newline-delimited event envelopes after an initial handshake acknowledgment (`{"type":"subscription_started"}`).
3. **Payload Structure:** `session.snapshot` returns `{ "type": "session_snapshot", "snapshot": { ... } }`. Agent prompt accepts `{ "target": str, "text": str }`. `pane.run` maps to `pane.send_input` with `{ "keys": ["Enter"] }`.

---

## 3. Implementation Summary

### 3.1 Native Socket Driver (`src-tauri/src/kopeng/herdr_socket.rs`)
- Implemented synchronous and asynchronous RPC dispatchers (`send_request_sync`, `send_request_async`) using atomic monotonic request IDs (`kw_<n>`).
- Built typed models for Herder Protocol 20 entities: `HerdrWorkspace`, `HerdrTab`, `HerdrPane`, `HerdrAgent`, `SessionSnapshot`, and `PaneProcessInfo`.
- High-level typed helpers:
  - `get_session_snapshot_sync` & `get_session_snapshot_async`
  - `list_workspaces_sync`, `create_workspace_sync`
  - `list_tabs_sync`, `create_tab_sync`, `rename_tab_sync`, `move_tab_sync`, `focus_tab_sync`, `close_tab_sync`
  - `list_panes_sync`, `get_pane_process_info_sync`
  - `send_keys_sync`, `send_input_sync`
  - `start_agent_sync`, `prompt_agent_sync`
- Unit test `test_live_herdr_snapshot` verifies live connectivity in 0.01s.

### 3.2 Full Codebase Caller Migration (Zero Dead Code)
- **`src-tauri/src/kopeng/herdr.rs`**:
  - Replaced CLI-based data structs with `HerdrSnapshot`, `HerdrTab`, `HerdrPane`.
  - Migrated `get_pane_id_for_tab`, `get_pane_busy_process`, `compute_project_herdr_state`, `get_project_herdr_state`, `dispatch_git_instruction`, `launch_herdr_agent`, `terminate_pane_foreground_process`, `bootstrap_agent_in_pane`, `restart_herdr_agent`, `focus_herdr_tab`, and `close_herdr_tab`.
  - Integrated `tab.move` tab reordering during agent launch to keep agent tabs visually grouped before service tabs.
  - Refactored `spawn_herdr_watcher` to stream real-time events via `events.subscribe` with automatic backoff reconnection and fall-through snapshot synchronization.
- **`src-tauri/src/orchestrator.rs`**:
  - Replaced all 11 `Command::new("herdr")` occurrences in `create_service_tab`, `start_service`, `stop_service`, `focus_service_tab`, and `get_running_services` with direct socket RPC.
  - Removed `std::process::Command` completely.
- **`src-tauri/src/kopeng/skill.rs`**:
  - Migrated supervised project initialization to `herdr_socket`. Removed unused `Command` import.
- **`src-tauri/src/mcp.rs`**:
  - Migrated `resolve_herdr_target`, `execute_report_task_status` callbacks, `delegate_agent_task`, and `execute_query_workspace_state` to typed socket calls. Removed unused `Command` import.

---

## 4. Verification & Results

- `cargo check`: Clean (0 errors, 0 warnings).
- `cargo test --lib -- kopeng::herdr_socket`: Passed (`test_live_herdr_snapshot` ok in 0.01s).
- `npm run build`: Clean (0 errors).
- Grep scan for `Command::new("herdr")`: 0 results in entire codebase.
