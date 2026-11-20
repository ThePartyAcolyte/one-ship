> [!WARNING]
> **LEGACY SPECIFICATION:** Superseded by [03-projects/02-git-workspace-and-branches.md](../../specs/03-projects/02-git-workspace-and-branches.md) and [03-projects/03-system-logging-and-status-terminal.md](../../specs/03-projects/03-system-logging-and-status-terminal.md). Preserved for historical provenance only.

# Spec 09: Version Control Dual-Signification, Branch Management, Quick Presets & Cyclic Logging
**Document Version:** 1.1.0  
**Domain:** Git Telemetry, Branch Lifecycle, Action Presets, File Logging & UI Terminal  
**Maintainer:** ONE-SHIP / Omarchy Development Team

---

## 1. Dual-Signification Git Status Model

The Git Status indicator in ONE-SHIP balances instant glanceability with dense technical precision.

```text
┌─────────────────────────────────────────────────────────────┐
│  (●) master       ! 2   ? 1   + 3   ↑ 1   ↓ 0               │
│   ▲     ▲         ▲     ▲     ▲     ▲     ▲                 │
│   │     │         │     │     │     │     └── Behind upstream
│   │     │         │     │     │     └──────── Ahead upstream
│   │     │         │     │     └────────────── Staged files
│   │     │         │     └──────────────────── Untracked files
│   │     │         └────────────────────────── Modified working tree
│   │     └──────────────────────────────────── Active branch name
│   └────────────────────────────────────────── Topology & Health Icon
└─────────────────────────────────────────────────────────────┘
```

### 1.1 Visual Encoding
* **Icon Topology:**
  * `<GitCompare />`: Remote tracking branch exists (`upstream` configured).
  * `<GitBranch />`: Local-only repository (unconnected to any remote).
* **Color Health Semantics:**
  * **Green (`#22c55e` / Theme Accent):** Working tree is clean (`is_clean: true`).
  * **Amber (`#f59e0b`):** Working tree is dirty (has modified, staged, or untracked files).
  * **Dim Grey (`#71717a`):** Unborn repository (git initialized, but zero commits).
* **Symbolic Metrics:**
  * `! N`: Unstaged modified files.
  * `? N`: Untracked files.
  * `+ N`: Staged changes ready for commit.
  * `↑ N`: Commits ahead of remote upstream.
  * `↓ N`: Commits behind remote upstream.

---

## 2. Dynamic Action Presets in `rula.toml`

To support fast, repetitive agent workflows without hardcoded mocks, actions are strictly loaded from `rula.toml`:

```toml
[project]
name = "one-ship"

[[plugins.vcs.actions]]
name = "Auto-Commit"
prompt = "Analyze git diff and create a clean semantic commit"

[[plugins.vcs.actions]]
name = "Commit & Push"
prompt = "Commit pending changes and push to current branch remote"
```

### 2.1 Split-Button UX (`Name` vs `»`)
Each preset pill in the UI is rendered as a split button:
* **Left Segment (Action Name):** Inserts the prompt into the instruction textarea for user inspection, refinement, or addition.
* **Right Segment (`»` with `<ChevronsRight />`):** **Instant Dispatch.** Immediately dispatches the instruction to the background `git` agent and closes the modal, bypassing typing friction.

---

## 3. Cyclic Rotating File Logger (`oneship.log`)

To prevent unbounded log accumulation while ensuring AI coding assistants can rapidly diagnose errors:
* **Location:** `~/.config/one-ship/logs/oneship.log`.
* **Cap Size:** `system.log_max_size_bytes` (default 512000). The file handle stays open and rotation is decided from the tracked written size, not a `stat` per line.
* **Rotations:** `system.log_max_rotations_count` generational backups (default 2: `oneship.1.log`, `oneship.2.log`).
* **Minimum level:** `system.log_level` (default `INFO`) gates both this file and the `system_logs` table; entries below it are dropped before any I/O. Set `DEBUG` for exhaustive diagnostics.
* **Settings cache:** logger settings are re-resolved only when the `config.toml` mtime changes.
* **Format (`[timestamp_seconds] [LEVEL] [module] [origin] message`):**
  ```text
  [1772654400] [INFO] [vcs] [one-ship] Dispatched instruction for 'one-ship': "Auto-Commit"
  [1772654410] [ERROR] [supervisor] [system] Failed to spawn agent process: command not found
  ```
* **Levels:** `DEBUG`, `INFO`, `WARN`, `ERROR`. `write_client_log` rejects any other level name. `logger::log_with_origin` tags entries with a specific project or subsystem origin (`system_logs.origin`), defaulting to `"system"`.

### 3.1 SQLite `system_logs` Sink & Retention

* Entries at or above `system.log_level` are queued on a bounded channel (4096) and committed by one worker thread in transactions of up to 64 rows.
* A full channel never blocks the caller: the entry is still in `oneship.log` but not queued for SQLite. Dropped entries are counted, and after each batch the worker writes `WARN ... N entries dropped from system_logs` to `oneship.log` and resets the count. If the worker thread is gone, each entry writes an `ERROR` line to the file instead. A failed batch insert or prune is reported in the file too, never through the channel.
* `write_client_log(level, module, message)` is the frontend entry point; an unknown level is `LoggerError::UnknownLevel`. `get_system_logs` supports cursor-based paging (`before_id`/`after_id`) and module/level filtering, backed by `get_system_log_sources` for distinct source discovery.
* The worker prunes `system_logs` to the newest `system.log_db_max_rows_count` rows (default 50000) every `system.log_db_prune_interval_seconds` (default 300); `init_database` prunes once at startup.
* The database runs with `auto_vacuum = INCREMENTAL`; each prune is followed by `PRAGMA incremental_vacuum`. Databases created before this setting are rebuilt once with `VACUUM` at startup, after the prune (audit F18: a 3.9 GB file with ~2.6 GB of free pages caused by 5.6 M DEBUG rows in 32 h).

---

## 4. Subtle 5-Line Rolling Status Terminal

In place of intrusive floating modal alerts or toasts:
* **Location:** Fixed bottom-left overlay (`fixed bottom-4 left-4`).
* **Capacity:** 5-line FIFO buffer.
* **Behavior:** Older messages roll off silently as new events occur (`[VOICE] armed`, `[VCS] dispatched`, `[AGENT] agent working`).

---

## 5. Unified Git & Snapshots Workspace Cockpit & Diff Viewer

The legacy isolated `VcsActionModal` and standalone `Snapshots` modal are unified into a persistent, high-density cockpit tab: **`Git & Snapshots`** (`workspaceTab = 'git'`).

### 5.1 Architecture & Components
1. **Unified Timeline Stream (Left Panel, `w-80`):**
   - Filter toggles: `All`, `Snaps (N)`, `Commits (M)` (plus optional domain filter when Design/PCB/CAD snapshots exist).
   - Persistent `WORKING TREE` item at top showing count of uncommitted/modified/untracked files.
   - Chronological interleaved timeline merging ONE-SHIP safety snapshots and Git commits.

2. **Inspector Header (`h-9 ui-surface-title`):**
   - Left: `GitBranchSelector` + active item metadata (`WORKING TREE` staged count, snapshot ID/label/agent, or commit hash/author/message).
   - Right (Working Tree mode): quick-action buttons from `rula.toml` (`[[plugins.vcs.actions]]`), `+` submodal to add actions, manual `SYNC` (`pull --rebase` + `push`), and `OPEN AGENT` (launches and opens the chat for the project's configured `plugins.vcs.default_agent_id`).

3. **Resizable Staging & Files Pane (`220px–560px`, default `288px`):**
   - **Commit Composer (Working Tree mode):** `AppTextarea` for commit message, AI commit-message generator button (`generate_commit_message` via `GIT_AGENT_KEYS` using `stream-json`/`acp` over the staged code diff), and manual `COMMIT` button (`commit_git_changes`).
   - **Staged Changes & Changes:** Per-file and bulk `+` (`stage_git_files`) and `-` (`unstage_git_files`) controls. Rows use a two-line layout (filename + relative directory) with `overflow-x-hidden` and render up to `MAX_RENDERED_FILES_COUNT = 250` rows per list to keep WebKit responsive on repositories with large untracked directories.
   - **Asset Tracking without Diff Bloat:** Design, PCB, and CAD assets (`is_domain_asset`) remain listed, stageable, and committable in Git, but suppress diff rendering in the viewer.

4. **Dual-Gutter Live File Diff Stage (Right Panel):**
   - **No Automatic Global Diff:** When `selectedFile` is `null` (or `is_domain_asset` is true), renders `AppEmptyState` without running full-repo diffs.
   - **Line Numbering:** Unified diff hunk headers (`@@ -oldStart,oldLen +newStart,newLen @@`) are parsed into structured line models with dual line number gutters (`oldLineNo` and `newLineNo`), matching IDE ergonomics.
   - **Reader Integration & Deletion Safeguards:**
     - Files existing on disk can be opened directly in the workspace Reader/Editor (`Explorer` tab) via `workspaceLayoutStore.navigateTo({ tab: 'files', filePath })`.
     - Files deleted on disk are detected via `p.join(&path).exists()`, marked with a strike-through filename, disabled reader buttons with clear tooltips, and a prominent `[ DELETED ON DISK ]` badge in the diff inspector header.

### 5.2 Working Tree Status Auditing & Untracked Diff Generation
1. **Structured Backend Telemetry (`WorkingTreeFile` & `CommitFileChange`):**
   - `get_working_tree_files(project_id)` runs `git status --porcelain=v1 -z -uall` (exact paths, renames carry `old_path`) and checks physical disk existence (`symlink_metadata`).
   - `get_commit_files_changed(project_id, commit_hash)` runs `git diff-tree --root -z` (a root commit lists its files); codes `A`/`D`/`M`/`R`/`C`/`T` map to `added`/`deleted`/`modified`/`renamed`/`copied`/`type_changed`, and any other code is an error.
   - Files are categorized with brutalist status badges:
     - `[D]`: Deleted (absent on disk or marked `D` in index/tree). Rendered with strikethrough in red.
     - `[U]`: Untracked (new files detected in working directory).
     - `[M]`: Modified (tracked file with unstaged or staged line alterations).
     - `[A]`: Added (staged newly added files).
     - `[R]`: Renamed (shows source path in hover tooltip).

2. **Untracked File Diff Support:**
   - `get_working_tree_diff(project_id, file_path?)` is one `git diff --cached` over a private index seeded from HEAD (the snapshot mechanism, `snapshots::diff_working_tree`) after `git add -A [-- file]`. Untracked files are included, `.gitignore` is honoured, a repository without commits diffs against the empty tree, and the user's index is untouched. There is no per-file `--no-index` loop and no fallback command.

### 5.3 Command Contracts (as implemented)
- Every git command takes the registered `project_id`, runs in `spawn_blocking` and returns a typed `GitError`. A project without `.git` is `NoRepository`, never an empty list. Only a repository without commits has an empty log.
- `get_project_git_log(project_id, limit_count)`: `limit_count` is required, `1..=MAX_GIT_LOG_ITEMS_COUNT` (200).
- Commit hashes must be 7–64 hexadecimal characters and are passed after `--end-of-options`, so a hash can never be read as a git option.
- `get_git_status` (project hydration) reads `git status --porcelain=v2 --branch -z` and `remote.origin.url`. `branch` is `null` for a detached HEAD, and `has_commits` is false for an unborn branch. A failure leaves `git_status` empty and sets `HydratedProject.git_error`, which the card and rail show as `[! GIT]` (never a guessed `clean` state or an `INIT GIT` button).
- `init_git_repo(project_id)` only runs `git init`. ONE-SHIP never writes `.gitignore` / `README.md`; the initializer agent adapts the existing files as a starting point (Spec 26).
- `save_vcs_action(project_id, name, prompt)` adds a `[[plugins.vcs.actions]]` entry to the project's valid `rula.toml`. The document is serialized and validated before writing. Name and prompt must be non-empty and the name unique. A project without `rula.toml` is an error. `get_vcs_actions` reads the validated `rula.toml`, and a malformed action list is an error.
- **Branch Lifecycle Commands (`kopeng/git.rs`, `GitBranchSelector.vue`, `GitBranchModal.vue`):**
  - `list_git_branches(project_id)`: Returns local and remote branches (`GitBranchInfo`: `name`, `is_current`, `is_remote`, `upstream`, `last_commit_hash`, `last_commit_subject`), sorted with the active branch first.
  - `switch_git_branch(project_id, branch_name)`: Switches the working tree (`git checkout <branch>` or `git checkout --track <remote_branch>`). Fails cleanly without auto-stashing if uncommitted changes conflict.
  - `create_git_branch(project_id, branch_name, from_ref?, checkout)`: Validates the branch ref name (`git check-ref-format --branch`) and creates/checks out the new branch.
