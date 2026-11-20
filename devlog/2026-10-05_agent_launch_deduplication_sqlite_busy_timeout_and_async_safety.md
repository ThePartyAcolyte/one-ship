# Development Log: Agent Launch Race Condition Elimination, SQLite Busy Timeout, and Tokio Keyring Async Safety

**Date:** 2026-10-05  
**Topic:** Resolving agent launch crashes (`[process_exit]: Agent process terminated abnormally`), SQLite `database is locked` concurrency errors, Tokio worker thread freezes from synchronous `.blocking_lock()`, and DuckDB process lock contention.

---

## 1. Problem Statement & Root Cause Analysis

### A. Spurious Agent Launch Crashes (`process_exit`)
* **Symptom:** When opening an agent in the floating chat overlay or clicking an agent from the taskbar / agent list, the agent immediately went into an error state (`[process_exit]: Agent process terminated abnormally`), and the application became unresponsive or sluggish.
* **Root Cause:**
  1. Multiple overlapping components were concurrently calling `launch_agent` for the same agent key within milliseconds:
     - `dialogueStore.openFloatingChat` called `selectAgent`, which triggered `ensureAgentRunning` (Launch 1).
     - `FloatingChatOverlay.vue` had a watcher on `[agentId, isFloatingChatOpen]` that simultaneously called `launchCurrentAgent` (Launch 2).
     - `AgentTaskbar.vue` had `handleLaunchAndOpenAgent` that called `openFloatingChat` and then directly executed `invoke('launch_agent')` (Launch 3).
  2. In the backend `supervisor.rs`, `launch_agent` checked if a child process existed; upon seeing the first spawned process from Launch 1, the concurrent Launch 2 called `stop_process(&key)` which sent a `SIGTERM` to the process.
  3. The supervisor's process monitor detected the `SIGTERM` exit as abnormal and dispatched `agent-error: process_exit` over Tauri events to the frontend, instantly presenting the user with an error card.

### B. SQLite `database is locked` Errors
* **Symptom:** Periodic errors in logs: `Sqlite(Error { code: DatabaseBusy, extended_code: 5 }) database is locked`.
* **Root Cause:** Rusqlite connections lacked a `busy_timeout` pragma (defaulting to 0ms). When background log flush routines (`kapawu-sqlite-logger`) held an exclusive write transaction, concurrent queries updating account quotas, session metrics, or token records failed immediately with SQLITE_BUSY.

### C. Tokio Thread Freezes from Keyring Mutex Contention
* **Symptom:** The frontend UI froze or lagged during agent startup.
* **Root Cause:** In `src-tauri/src/kopeng/accounts.rs`, functions such as `initiate_add_account` and `switch_agent_account` used `KEYRING_LOCK.blocking_lock()` inside Tokio runtime threads while executing `std::thread::sleep(Duration::from_millis(500))` polling loops. This blocked Tokio worker threads for up to 10 seconds, freezing the Tauri IPC dispatch channel.

### D. DuckDB Lock Contention with Daemon
* **Symptom:** If `oneship-daemon` was actively connected to `oneship_telemetry.duckdb`, Tauri's DuckDB initialization crashed or failed with an exclusive file lock error.

### E. Rapid Forensic Log Overwrites
* **Symptom:** `oneship.log` had a 1MB limit with only 1 rotation, overwriting diagnostic traces within minutes during active agent sessions.

---

## 2. Architecture & Implementation Details

### A. Backend Concurrency & Launch Deduplication (`src-tauri/src/supervisor.rs`)
* **Atomic Launch Tracker:** Introduced a global `LAUNCH_IN_PROGRESS: Lazy<Mutex<HashSet<String>>>` in `supervisor.rs`.
* **RAII `LaunchGuard`:** When `launch_agent` begins, it verifies whether the composite key (`<project_path>::<agent_id>`) is already launching. If so, it returns an early `Ok("Launch already in progress for this agent; ignoring duplicate request.")` without terminating or spawning duplicate processes. Upon completion (success or error), the guard releases the key from the set.
* **Keyring Lock Hold Reduction:** Reduced `KEYRING_LOCK` hold duration during supervisor launch from 10s to 3s with explicit step diagnostics.

### B. Frontend Cross-Component Launch Synchronization
* **Pinia Store (`src/stores/dialogueStore.ts`):**
  - Added `_launchingAgentIds = new Set<string>()`.
  - Added public helper methods `isAgentLaunching(agentId: string): boolean` and `setAgentLaunching(agentId: string, launching: boolean)`.
  - Removed duplicate `ensureAgentRunning` call in `openFloatingChat`.
* **Chat Overlay (`src/components/dialogue/FloatingChatOverlay.vue`):**
  - Updated watcher on `[agentId, isFloatingChatOpen]` to verify `!dialogueStore.isAgentLaunching(agentId)` before firing `launchCurrentAgent`.
  - Wrapped `launchCurrentAgent()` in `dialogueStore.setAgentLaunching` to prevent overlapping manual executions.
  - Guarded `handleContextTransfer` against re-launching an agent already being launched by `openFloatingChat`.
* **Taskbar (`src/components/taskbar/AgentTaskbar.vue`):**
  - In `handleLaunchAndOpenAgent`, added guard `if (dialogueStore.isAgentLaunching(agentId)) return;` before dispatching redundant `invoke('launch_agent')`.
* **Agent List & Conversation Panel (`AgentList.vue`, `ConversationPanel.vue`):**
  - Guarded manual play button clicks against triggering duplicate launches when an agent is already starting up.

### C. SQLite Busy Timeout Pragma (`src-tauri/src/db.rs`)
* Added `conn.execute_batch("PRAGMA busy_timeout = 5000;")` inside `SqliteConnectionManager::with_init`.
* Any concurrent write transaction now automatically retries for up to 5000ms rather than failing immediately with `DatabaseBusy`.

### D. Async Safety for Keyring Operations (`src-tauri/src/kopeng/accounts.rs`)
* Converted `initiate_add_account`, `switch_agent_account`, and their IPC command counterparts to asynchronous functions (`async fn`).
* Replaced `.blocking_lock()` on Tokio's `keyring_lock` with non-blocking `.lock().await`.
* Replaced synchronous `std::thread::sleep` with `tokio::time::sleep`.

### E. DuckDB Memory Fallback on Lock Conflict (`src-tauri/src/duck.rs`)
* Updated `init_duckdb` to catch database lock errors when opening `oneship_telemetry.duckdb`.
* If a lock conflict occurs with an external daemon process, logs a warning and cleanly falls back to an in-memory database (`init_duckdb_memory()`) instead of crashing the application.

### F. Expanded Log Buffer & Rotations (`src-tauri/src/logger.rs`)
* Increased `DEFAULT_MAX_LOG_SIZE_BYTES` from 1MB to 5MB (`5_242_880`).
* Increased `DEFAULT_MAX_ROTATIONS` from 1 to 5, preserving up to 25MB of forensic log history.

### G. Chat Execution Config Wizard Teleport Mount Resolution (`AppExecutionConfigPicker.vue`)
* **Problem:** In Vue 3, `<Teleport :to="mountTarget">` evaluated `document.querySelector('#chat-messages-modal-mount')` at the component's initial mount time. Because `AppExecutionConfigPicker` was in the upper toolbar while `#chat-messages-modal-mount` was rendered lower in `FloatingChatOverlay.vue`, the selector returned `null`, disabling the teleport anchor so the modal failed to appear when toggled.
* **Resolution:** Moved `v-if="isOpen"` directly onto `<Teleport v-if="isOpen" :to="resolvedMountTarget">`, ensuring Teleport is only evaluated when the modal is opened and the DOM target is guaranteed to exist. Added `isTargetInChat` check with a clean fallback to `body`.
* **Tooltip Standardization:** Wrapped the wizard trigger button and close buttons in `<AppTooltip>` and purged native HTML `title="..."` attributes.

---

## 3. Verification & Compliance

* **Backend Compilation:** `cargo check` in `src-tauri` executed with 0 errors.
* **Frontend Compilation:** `vue-tsc --noEmit && vite build` passed with exit code 0 and clean bundle outputs.
* **Governance Compliance:**
  - Zero `git` commands executed.
  - Zero application runtimes or dev servers spawned by the agent.
  - All file changes applied exclusively via dedicated tool interfaces (`write_to_file`, `replace_file_content`).
