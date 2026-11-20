# Devlog: Working Tree Status Auditing, Deletion Detection & Untracked Diffs

**Date:** 2026-09-20  
**Topic:** Accurate File Status Indicators, Disk Existence Auditing, Untracked File Diffs & Reader Navigation Guards

---

## 1. Problem Diagnosis & Motivation
During real-world inspection in the `Git & Snapshots` cockpit (`ProjectGitSnapshotsTab.vue`), an audit revealed discrepancy issues in the Working Tree files list:
1. **Missing File Status Indicators:** The working tree file list rendered files as raw strings with identical generic file icons, failing to communicate whether a file was modified (`[M]`), untracked (`[U]`), added/staged (`[A]`), renamed (`[R]`), or deleted (`[D]`).
2. **Ghost / Deleted Files Appearing as Modified:** A file like `rula.toml` appeared in the working tree list with line diffs, but the file was completely missing from disk in the active project directory. Because `get_working_tree_files` stripped status characters from `git status --porcelain` and never verified physical disk presence via `Path::exists()`, deleted files looked identical to existing modified files.
3. **Broken Reader Action on Deleted Files:** Clicking the `<Eye />` ("Open in Reader") icon or the `[ OPEN IN READER ]` button in the diff viewer attempted to navigate to and open a deleted file, resulting in read errors from `read_project_document`.
4. **Empty Diffs for Untracked Files:** `git diff HEAD -- <untracked_file>` outputs nothing because HEAD does not track new files. Clicking an untracked file showed an empty diff pane.

---

## 2. Architecture & Implementation

### A. Rust Backend Enhancements (`src-tauri/src/kopeng/git.rs`)
1. **Structured Telemetry Models:**
   - Introduced `WorkingTreeFile`:
     ```rust
     pub struct WorkingTreeFile {
         pub path: String,
         pub status: String,          // "modified" | "deleted" | "untracked" | "added" | "renamed"
         pub staged_code: char,       // 'M', 'A', 'D', 'R', ' '
         pub unstaged_code: char,     // 'M', 'D', '?', ' '
         pub is_staged: bool,
         pub is_deleted: bool,
         pub is_untracked: bool,
         pub exists_on_disk: bool,    // Verified via p.join(&path).exists()
         pub old_path: Option<String>,
     }
     ```
   - Introduced `CommitFileChange`:
     ```rust
     pub struct CommitFileChange {
         pub path: String,
         pub status: String,          // "modified" | "added" | "deleted" | "renamed"
         pub status_code: char,       // 'M', 'A', 'D', 'R'
     }
     ```
2. **Disk Existence & Deletion Auditing (`get_working_tree_files`):**
   - Scans `git status --porcelain=v1 -uall`.
   - Checks `p.join(&path).exists()`. If `x == 'D'`, `y == 'D'`, or `!exists_on_disk`, explicitly flags `is_deleted = true` and `status = "deleted"`.
   - Handles renames (`old_path -> new_path`).
3. **Untracked Diffs via `git diff --no-index` (`get_working_tree_diff`):**
   - When a specific file is selected: queries porcelain status. If untracked (`??`), invokes `git diff --no-index -- /dev/null <path>` to generate clean addition diffs.
   - When global diff is requested (`file_path: None`): automatically appends untracked file diffs alongside `git diff HEAD`, providing a comprehensive, zero-blind-spot view.
4. **Commit Files Changed Status (`get_commit_files_changed`):**
   - Upgraded from `--name-only` to `--name-status -r <commit_hash>`, returning `CommitFileChange` with exact git alteration codes (`M`, `A`, `D`, `R`).

### B. Frontend UI & UX (`src/components/workspace/ProjectGitSnapshotsTab.vue`)
1. **Brutalist File Status Badges:**
   - Rendered in working tree and commit file lists:
     - `[D]`: Red (`border-rose-500/50 text-rose-400 bg-rose-500/10`), indicating deleted on disk.
     - `[U]`: Sky blue (`border-sky-500/50 text-sky-400 bg-sky-500/10`), indicating untracked.
     - `[M]`: Amber (`border-amber-500/50 text-amber-400 bg-amber-500/10`), indicating modified.
     - `[A]`: Emerald (`border-emerald-500/50 text-emerald-400 bg-emerald-500/10`), indicating added.
     - `[R]`: Purple (`border-purple-500/50 text-purple-400 bg-purple-500/10`), indicating renamed with source in tooltip.
2. **Visual Strikethrough for Deleted Files:**
   - Any deleted or non-existent file on disk renders with `line-through text-rose-400/80` for immediate clarity.
3. **Reader Navigation Safeguards:**
   - File list row: `<Eye />` icon is disabled for deleted files (`opacity-20 cursor-not-allowed`) with tooltip `"File deleted on disk (cannot open)"`.
   - Diff pane header: If the selected file does not exist on disk, `[ OPEN IN READER ]` is replaced with a prominent `[ DELETED ON DISK ]` warning badge.
   - Central navigation function `openFileInReader(filePath)` includes an invariant guard checking `entry.is_deleted || entry.exists_on_disk === false` before dispatching navigation.

---

## 3. Verification & Validation
- **Rust Backend:** `cargo check` in `src-tauri` executed in 0.20s with 0 errors or warnings.
- **Frontend & TypeScript:** `npm run build` (`vue-tsc --noEmit && vite build`) passed cleanly in 3.48s with 0 errors.
- **Spec Synchronization:** Updated `docs/specs/09_vcs_and_logging_architecture.md` (Section 5.2) documenting status indicators, deletion detection, untracked diff generation, and Reader safeguards.
