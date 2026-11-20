# Devlog: Unified "Git & Snapshots" Workspace Cockpit

**Date:** 2026-09-20  
**Topic:** Implementation of Proposal 2: Cockpit Unificado (Timeline Híbrido & Stage Central)

---

## 1. Context & Motivation
Following the workspace restructuring that consolidated project card views into an Overview tab and replaced the separate dashboard with a slide-over drawer, VCS controls and safety snapshots remained fragmented:
- VCS instructions were triggered via an isolated modal (`VcsActionModal.vue`).
- Safety snapshots and surgical rollback were located in a standalone `Snapshots` tab.
- Users lacked an integrated overview interleaving Git commits, dirty working tree state, and point-in-time safety snapshots.

To create an ergonomic, developer-focused experience, we implemented **Proposal 2: Cockpit Unificado**, unifying Git telemetry, agent instruction dispatching, rula actions, git history, and Wang-Kapawu snapshots into a central cockpit tab: **`Git & Snapshots`**.

---

## 2. Key Architecture & Implemented Features

### A. Tauri Backend Commands (`src-tauri/src/kopeng/git.rs` & `src-tauri/src/lib.rs`)
- `GitCommitInfo`: Struct exposing `hash`, `short_hash`, `author_name`, `author_email`, `timestamp`, and `message`.
- `get_project_git_log(project_path, limit_count)`: Extracts delimited git commits with ISO timestamps.
- `get_git_commit_diff(project_path, commit_hash, file_path)`: Produces unified diff of a specific commit or file within that commit.
- `get_commit_files_changed(project_path, commit_hash)`: Lists all files touched in a given commit.
- `get_working_tree_diff(project_path, file_path)`: Computes live uncommitted diff against `HEAD`.
- `get_working_tree_files(project_path)`: Scans porcelain status for modified, untracked, and staged files.

### B. State & Routing Integration (`src/stores/dialogueStore.ts`)
- Updated `activeWorkspaceTab` and `NavigationTarget` union type from `'overview' | 'files' | 'devlog' | 'snapshots' | 'terminal'` to `'overview' | 'files' | 'devlog' | 'git' | 'terminal'`.
- Configured keyboard navigation (`Alt+5` and `Alt+W` cycling).

### C. Unified Cockpit Component (`src/components/workspace/ProjectGitSnapshotsTab.vue`)
1. **Top Dispatcher Bar**:
   - Live branch badge (`main`, `clean` vs `N modified`).
   - Instruction prompt input for the `git` specialist agent with instant submission (`Enter`) and `Dispatch` button.
   - Quick action chips loaded directly from `rula.toml` (`[[plugins.vcs.actions]]`) with one-click direct dispatch (`>>`) and prompt insertion.
   - Submodal to dynamically save new custom actions into `rula.toml`.
   - Point-in-time safety snapshot creator with custom label (`[+ SNAP]`).
2. **Left Panel: Unified Timeline Stream (`w-80`)**:
   - Filter toggles: `[ All ]`, `[ Snaps (N) ]`, `[ Commits (M) ]`.
   - Persistent `[⚡ WORKING TREE]` item at the top showing uncommitted modified/untracked files.
   - Interleaved chronological timeline combining Wang-Kapawu safety snapshots (with badges, agent ID, files count) and Git commits (with short hash, message, author).
3. **Right Panel: Unified Inspector & Live Diff Stage**:
   - **Snapshot Mode**: Metadata overview, surgical restore checklist (`RESTORE SELECTED (N)`), full repository rollback (`FULL ROLLBACK`), and unified diff vs HEAD.
   - **Commit Mode**: Commit metadata, list of touched files, and commit diff.
   - **Working Tree Mode**: Uncommitted file checklist, live working tree diff vs HEAD, and inline agent commit trigger button (`COMMIT & SYNC VIA AGENT`).

### D. Zero Dead Code & Clean Integration
- Retired `VcsActionModal.vue` and replaced `ProjectSnapshotsTab.vue` with `ProjectGitSnapshotsTab.vue`.
- Wired all VCS triggers and Snapshot navigation buttons in `CardHeader.vue`, `ProjectOverviewTab.vue`, `ProjectVerticalRail.vue`, and `App.vue` to switch directly to `workspaceTab = 'git'`.

---

## 3. Verification & Validation
- **Rust Backend:** `cargo check` in `src-tauri` completed with 0 errors.
- **TypeScript:** `npx vue-tsc --noEmit` passed with 0 errors.
- **Production Build:** `npm run build` completed successfully in 3.96s.
