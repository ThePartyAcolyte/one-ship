# Devlog: Snapshot Timeline & Surgical Rollback (Spec 16 Phase 2)
**Date:** 2026-09-11  
**Topic:** Spec 16 Phase 2 - Milestone-Only Snapshots, Transcript Scoping, Live Diff Viewer & Surgical Rollback UI  
**Author:** Antigravity / Pair Programming Session

---

## 1. Context & Architecture Goal
Prior to this session, snapshots relied on global filesystem state captures (`git add -A`), creating race conditions in concurrent multi-agent environments where one agent's dirty working tree could be inadvertently captured inside another agent's snapshot.

The goals for this implementation were:
1. **Milestone-Only Architecture:** Shift away from dual `PRE`/`POST` session snapshots towards Milestone/Deploy snapshots triggered at discrete progress points (e.g. devlog recording, deployments, or manual triggers).
2. **Transcript-Scoped Staging:** Extract exact file paths touched by each agent during its Antigravity session from its transcript tool calls (`replace_file_content`, `write_to_file`), staging only those files.
3. **Interactive UI Timeline & Diff Inspector:** Deliver `SnapshotTimelineModal.vue` allowing developers to inspect snapshot metadata, browse touched files, view unified diffs against `HEAD`, and perform selective surgical file restores or full rollbacks.

---

## 2. Changes Implemented

### Backend (`src-tauri/`)
- **`src-tauri/src/db.rs`:**
  - Removed restrictive `CHECK (label IN ('pre', 'post'))` constraint on the `snapshots` SQLite table to support arbitrary labels (`milestone:<id>`, `deploy:<id>`, `manual`).
- **`src-tauri/src/kopeng/snapshots.rs`:**
  - Implemented `extract_agent_touched_files(conversation_id, project_path)` parsing `transcript.jsonl` tool invocations.
  - Updated `create_snapshot()` to accept `explicit_files: Option<Vec<String>>` and automatically scope git staging.
  - Implemented `get_snapshot_diff(project_path, snapshot_ref, compare_with, file_path)` returning unified git diff output.
  - Exposed `create_project_snapshot`, `list_project_snapshots`, `restore_project_snapshot`, `delete_project_snapshot`, and `get_snapshot_diff_cmd` via Tauri commands.
- **`src-tauri/src/lib.rs`:**
  - Registered all 5 snapshot IPC handlers in `tauri::generate_handler![...]`.
- **`src-tauri/src/mcp.rs`:**
  - Updated `execute_create_snapshot` to support explicit file lists.
  - Hooked automatic milestone snapshot creation (`milestone:<devlog_id>`) into `execute_record_session_minutes`.
- **`src-tauri/src/watcher.rs`:**
  - Removed obsolete `PRE`/`POST` presence lock polling loops and cleaned up unused snapshot imports (Zero Dead Code).

### Frontend (`src/`)
- **`src/components/modals/SnapshotTimelineModal.vue`:**
  - Dual-panel brutalist modal interface.
  - Left pane: Milestone history stream with label badges, relative timestamps, agent authorship, commit IDs, and deletion actions.
  - Right pane: Overview header with snapshot ref, touched files checklist with "Select All", live unified diff viewer with syntax highlighting (`+` green, `-` red, `@@` sky), and actions for `[RESTORE SELECTED (N)]` and `[FULL ROLLBACK]`.
  - Manual snapshot creation input (`[SNAP]`).
- **`src/components/ProjectCard/CardHeader.vue` & `ProjectCard.vue`:**
  - Added `<History />` button with `<AppTooltip text="Snapshots & Safety Rollback" />` and dropdown menu item `Snapshots & Diff`.
  - Forwarded `openSnapshots` event to parent.
- **`src/App.vue`:**
  - Integrated `SnapshotTimelineModal` and bound `snapshotModalProject` state.

---

## 3. Verification & Compliance
- `cargo check`: Passed with 0 errors.
- `cargo test`: 11 unittests passed cleanly (including `test_gc_policy_configuration`, `test_diff_computation`, `test_partial_restore`).
- `npm run build` (`vue-tsc` + `vite build`): Built production bundle in 2.71s with 0 type errors.
- Updated `TECHNICAL_DEBT.md` (marked item #12 as RESOLVED - Build 0.2.1) and `TODO.md`.
