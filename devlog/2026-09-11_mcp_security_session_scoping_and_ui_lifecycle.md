# 2026-09-11 - MCP Security, Session Scoping, Terminal Interlocks & UI Window Lifecycle

## Context & Objectives
Following the comprehensive system audit of data persistence, UI lifecycle, and technical debt, this session executed **Phase 1** of the strategic roadmap:
1. **Scoped Active Presence Detection:** Prevent global system-wide `agy` process scanning from triggering false warnings during multi-account switches by scoping to the active project's `HERDR_PANE_ID` and working directory (`check_active_presence_sessions`).
2. **Terminal Interlocks & Focus Detection:** Propagate `is_terminal_focused` through `HerdrAgentState` in `src-tauri/src/kopeng/herdr.rs` by cross-referencing Herder snapshot `focused_pane_id` against agent panes.
3. **MCP Caller Provenance & Verification:** Guarantee that MCP endpoints (`post_agent_response`, `record_session_minutes`, `resume_agent_session`, `restore_snapshot`) only execute for verified agents and projects.
4. **UI Window Lifecycle & Background Server Resilience:** Intercept Tauri desktop window `CloseRequested` events via `on_window_event` to invoke `api.prevent_close()` and `window.hide()`, preserving background watchers, quota kickstarters, and SSE connections.

---

## Key Changes & Implementations

### 1. Scoped Active Session Presence (`src-tauri/src/kopeng/accounts.rs`)
- Updated `check_active_presence_sessions(project_path: Option<&str>)` to resolve project Herder state and filter `agy` processes by `HERDR_PANE_ID` or canonical working directory (`cwd`).
- Updated `check_active_agent_sessions_cmd(project_path: Option<String>)` signature to allow optional project scoping from Tauri IPC.

### 2. Terminal Interlock & Focus Observability (`src-tauri/src/kopeng/herdr.rs`)
- Added `pub is_terminal_focused: bool` to `HerdrAgentState`.
- Extended `compute_project_herdr_state` and `spawn_herdr_watcher` to accept `focused_pane_id: Option<&str>` from the Herder socket snapshot.
- Set `is_terminal_focused = true` when `pane_id == focused_pane_id`, enabling real-time detection of external GUI terminal interaction.

### 3. Tauri Window Event Lifecycle (`src-tauri/src/lib.rs`)
- Registered `.on_window_event` on `tauri::Builder` intercepting `WindowEvent::CloseRequested`.
- Invokes `api.prevent_close()` and `window.hide()`, allowing the desktop app to remain active as a local daemon service while preserving instant re-opening.

### 4. Technical Debt & Documentation Sync
- Resolved Technical Debt #13 (*Herder Detached Session & Terminal Interlock Detection*), #15 (*Agent Dialogue UI Quota & Context Observability*), and #18 (*MCP Caller Origin Validation & Process Provenance*).
- Synchronized active tasks in `TODO.md`.

---

## Verification
- `cargo check --manifest-path src-tauri/Cargo.toml`: Passed (0 errors, 0 warnings).
- `cargo check --bin kapawu-daemon --manifest-path src-tauri/Cargo.toml`: Passed (0 errors, 0 warnings).
- `npm run build` (`vue-tsc --noEmit && vite build`): Passed cleanly.
