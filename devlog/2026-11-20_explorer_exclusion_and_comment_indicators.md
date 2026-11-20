# Devlog: File Explorer Exclusion Indicators, Comment Badges & Selection UX (2026-11-20)

## Summary
Added visual state indicators and improved selection/context-menu behavior in the Project Workspace file explorer:

1. **Backend (`DocumentItem` & DB & Git & Semantic):**
   - Extended `DocumentItem` in `src-tauri/src/kopeng/documents.rs` with `in_gitignore: bool`, `in_semantic_ignore: bool`, `is_semantically_indexed: bool`, `git_status: Option<String>`, and `open_comment_count: u32`.
   - Added `db_get_open_comment_counts_by_file` in `src-tauri/src/db.rs` to fetch open comment counts (`status = 'open'`) grouped by `file_path` in a single query.
   - Added `get_working_tree_status_map` in `src-tauri/src/kopeng/git.rs` reusing `git status --porcelain=v1 -z -uall` and `parse_status_v1`, with directory status propagation (`conflict` > `modified` > `added` > `renamed` > `untracked`).
   - Added `list_indexed_file_paths` in `src-tauri/src/semantic/store.rs` (LanceDB `file_path` column projection) combined with `db_get_project_code_symbol_hashes` (SQLite) to set `is_semantically_indexed`.
   - Converted `get_project_documents` to `async fn` (`call!` in `src-tauri/src/api.rs`).
   - Reused `crate::semantic::is_path_ignored` (`pub(crate)`) and added `load_gitignore_patterns` / `matches_gitignore_patterns` in `documents.rs` with parent-to-child inheritance across `build_codebase_tree`, `build_doc_tree`, and root `docs`/`scratchpad` nodes.
   - Exported updated TypeScript bindings (`src/types/generated/DocumentItem.ts`).

2. **Frontend (`projectExplorerStore.ts` & `ProjectWorkspace.vue`):**
   - Normal tracked, clean files and directories render with `text-[var(--fg-color)]` by default, with hover highlighting only the row background/border.
   - `.gitignore`d files and directories render dimmed (`text-[var(--dim-color)] opacity-60`).
   - Git status colors and right-side badges (`M`, `U`, `A`, `R`, `!` for files; colored dot for directories with descendant changes) using Spec 13 status CSS variables.
   - Semantic indexing status indicator moved to the right of the name alongside badges and comment icon: `Brain` with diagonal slash (`Excluded from semantic indexing`) when `in_semantic_ignore`, `Brain` (`Semantically indexed`) when `!in_semantic_ignore && !is_dir && is_semantically_indexed`.
   - Files with open comments display a `MessageSquare` icon (`:size="10"`, `text-[var(--status-warn-color)]`) with an `AppTooltip` showing the open comment count; `projectExplorerStore.ts` keeps `open_comment_count` synchronized when comments are created, resolved, or deleted.
   - Removed the docked bottom multi-select action bar (`selectedItemPaths.length > 0`) and fixed `handleCtxDelete()` to preserve multi-selection when right-clicking a selected item.
   - Scoped row selection (`onExplorerItemClick` / `onExplorerItemDblClick`) to the icon/name wrapper (`inline-flex ... cursor-pointer`), while clicking empty row background or empty list space calls `clearSelection`.
   - Fixed `handleCtxToggleExclusion` to call `explorerStore.fetchProjectDocuments(projectPath)` for both `gitignore` and `semantic` toggles.

## Verification
- `./scripts/run_dev.sh export-bindings`: `test result: ok. 413 passed; 0 failed; 0 ignored; 0 measured; 166 filtered out`
- `./scripts/verify_rules.sh`: Zero new violations in modified files (2 pre-existing unrelated failures in `RollingStatusLog.vue` / modals and `db.rs:7548`).
- `grep -nE "unwrap_or|unwrap_or_default|\.ok\(\)|let _ =|catch..."` on edited files: zero hits in newly added code.
- `./scripts/run_dev.sh status`: `daemon` active/running on port `8765`, `daemon_watch` active/running, `vite` active/running, `gui` active/running.
