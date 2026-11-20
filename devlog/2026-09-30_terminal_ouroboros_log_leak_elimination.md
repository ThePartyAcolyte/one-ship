# Terminal Ouroboros Log Leak Elimination & Process Stream Isolation

**Date:** 2026-09-30  
**Topic:** Eradication of Self-Referencing System Log Feedback Loop in Terminal Output Readers

## 1. Problem Statement & Root Cause Analysis

When inspecting unlaunched processes, opening the project terminal grid, or launching background services/agents, the terminal view was bombarded with repetitive `[DEBUG]` entries at high frequency (every 150–300 ms):

```
[DEBUG] [system] db_get_system_logs: origin=None, level=None, search_query=Some("/home/...:svc:dev"), limit=300, offset=0
```

Once the process or agent fully initialized, the terminal cleared and began displaying standard process stdout/stderr.

### Forensic Findings (The Ouroboros Mechanism)
1. **Rogue Fallback in `read_terminal_output`:** In `src-tauri/src/supervisor.rs`, `read_terminal_output` checked `sup.read_output(&key, max_lines_count)`. When a process had not yet spawned or had emitted zero bytes to memory ring buffers, `out.is_empty()` triggered a fallback invoking `crate::db::db_get_system_logs(None, Some(key), ...)`.
2. **Self-Referential Diagnostic Logging:** Inside `src-tauri/src/db.rs`, `db_get_system_logs` logged its entry at `DEBUG` level including the verbatim `search_query` string in the message (`"db_get_system_logs: ... search_query=Some(...)"`).
3. **Database Contamination & Query Echo:** The asynchronous logger queued and committed this log into the `system_logs` SQLite table. Subsequently, the SQL query (`WHERE message LIKE '%key%' OR source LIKE '%key%'`) matched the log entry created by the query itself.
4. **Polling Feedback Cascade:** In `ProjectTerminalGrid.vue` and `FloatingChatOverlay.vue`, polling routines triggered `read_terminal_output` iteratively during startup transitions, multiplying the self-matching records and returning them as the terminal output stream.

## 2. Architectural Solution

In alignment with strict process boundary isolation and zero arbitrary fallback policies:

### Backend Process Supervisor (`src-tauri/src/supervisor.rs`)
- **Eliminated SQLite Fallback in `read_terminal_output`:** Removed lines querying `db_get_system_logs` when memory buffers are empty. `read_terminal_output` now strictly returns the memory-backed process stream (`Ok(out)`). Unlaunched or silent processes cleanly return empty strings.

### Database Telemetry Sanity (`src-tauri/src/db.rs`)
- **Sanitized Search Query Logging:** Updated `db_get_system_logs` to log `has_search_query: bool` instead of dumping the raw query string into SQLite, adhering to Tiger Style and telemetry observability invariants (preventing unbounded payloads and self-matching substring pollution).

### Frontend Verification
- Verified that `ProjectTerminalGrid.vue` and `FloatingChatOverlay.vue` cleanly handle empty outputs with native UI placeholders (`// No output logged yet for '...'` / `// Waiting for output stream...`).
