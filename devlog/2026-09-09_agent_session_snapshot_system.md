# Devlog: 2026-09-09 — Agent Session Snapshot System (Spec 16)

## Context & Objectives
Implemented the Agent Session Snapshot System as defined in `docs/specs/16_agent_session_snapshot_system.md`. The system provides automatic, token-free, per-session VCS safety nets for autonomous agent interventions without altering the official Git commit history.

## Key Changes
1. **Database Schema & Persistence (`src-tauri/src/db.rs`):**
   - Added `snapshots` SQLite table with indexing on `project_id` and `conversation_id`.
   - Created CRUD queries: `db_insert_snapshot`, `db_list_snapshots`, `db_get_snapshot_by_id_or_ref`, `db_get_pre_snapshot_record`, `db_update_snapshot_files_changed`, `db_delete_snapshot`, `db_count_snapshots`, `db_get_oldest_snapshot_conversations`.

2. **Configuration Cascade & History Buffer (`src-tauri/src/config.rs`):**
   - Registered `snapshots` plugin schema with `max_per_project` setting (default: 50).
   - Added `resolve_snapshot_max_per_project` cascading helper (supporting `rula.toml` `[snapshots] max_per_project = N`).

3. **Core Engine (`src-tauri/src/kopeng/snapshots.rs` & `kopeng/mod.rs`):**
   - Implemented `create_snapshot` with atomic Git sequence (`git add -A` -> `git commit --allow-empty` -> `git update-ref` -> `git reset --soft HEAD~1` -> `git restore --staged .`).
   - Implemented `restore_snapshot` supporting both full repo rollback and surgical partial rollback by file list (`git restore --source=<ref> -- <files>`).
   - Implemented `compute_files_changed` (`git diff <pre> <post> --name-only`).
   - Configured GC policies (`gc.reflogExpireUnreachable "never"`, `gc.pruneExpire "never"`).
   - Implemented `prune_excess_snapshots` for state-history-based automatic pruning.

4. **MCP Server Integration (`src-tauri/src/mcp.rs`):**
   - Exposed `create_snapshot`, `list_snapshots`, and `restore_snapshot` MCP tools.

5. **Watcher Daemon (`src-tauri/src/watcher.rs` & `lib.rs`):**
   - Integrated presence locks detection (`~/.gemini/antigravity-cli/presence/*.lock`).
   - Auto-triggers PRE snapshot on active project change and POST snapshot upon session lock clearance.
   - Exposed Tauri invoke handlers: `create_project_snapshot`, `list_project_snapshots`, `restore_project_snapshot`, `delete_project_snapshot`.

## Verification
- Unit & integration tests (`kopeng::snapshots::tests`):
  - `test_gc_policy_configuration` (PASSED)
  - `test_partial_restore` (PASSED)
  - `test_diff_computation` (PASSED)
- Full binary compilation check (`cargo build --bins`): PASSED without warnings.
