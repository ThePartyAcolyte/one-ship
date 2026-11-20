# Devlog — 2026-11-19: Task Date-Range Validation, Explorer Exclusion Controls, Electronics Token Migration & F5 Context Refresh

## Summary
Resolved four active technical debt items (`#27`, `#29`, `#41`, `#42`) across the Rust daemon and Vue frontend:

1. **Task Date-Range Validation (`TECHNICAL_DEBT.md #42`, Spec 34):**
   - Added `ProjectMgmtError::InvalidDateRange` (`400 BAD_REQUEST`) and `validate_date_range(start_epoch_seconds, due_epoch_seconds)` in `src-tauri/src/kopeng/project_mgmt.rs` (`create_project_task` and `update_project_task`).
   - Added reactive inline validation (`dateRangeError`) in `src/components/modals/CreateProjectTaskModal.vue` and `src/components/workspace/ProjectTaskDetailPanel.vue`, blocking submission when `dueDate < startDate`.

2. **Semantic `max_file_size_bytes` & Explorer Exclusion Context Menu (`TECHNICAL_DEBT.md #29`):**
   - Added `[plugins.semantic].max_file_size_bytes` (`262144` bytes / `256 KiB` default, `0` disables) in `src-tauri/src/config.rs` and enforced it in `scan_docs_directory` and `scan_code_directory` (`src-tauri/src/semantic/mod.rs`).
   - Added `get_project_exclusion_status` and `toggle_project_exclusion` daemon commands (`src-tauri/src/kopeng/documents.rs`, `src-tauri/src/api.rs`, `src/types/daemonApi.ts`) allowing one-click toggling of `.gitignore` and `[plugins.semantic].ignored_patterns` (`rula.toml`) from the File Explorer context menu (`src/components/workspace/ProjectWorkspace.vue`), with automatic background re-indexing on semantic exclusion changes.

3. **Electronics Instrumentation Spec 13 Style Migration (`TECHNICAL_DEBT.md #41`):**
   - Migrated `src/components/workspace/electronics/LogicAnalyzerView.vue` and `src/components/workspace/electronics/OscilloscopeView.vue` to semantic CSS variables (`--bg-color`, `--surface-bg`, `--border-color`, `--accent-color`, `--status-ok-*`, `--status-warn-*`, `--status-error-*`), `h-9 ui-surface-title` non-wrapping toolbars, and `.ui-icon-btn` controls while preserving all `<canvas>` waveform rendering and hardware features.

4. **`F5` (`oneship:context-refresh`) Completion & Watcher Evaluation (`TECHNICAL_DEBT.md #27`):**
   - Audited reactive watchers across the remaining tabs: `ProjectPcbTab.vue` and `ProjectCadTab.vue` already react to `pcbcraft-file-changed` / `solidcraft-file-changed` via `hardwarePreviewStore.ts` (so `F5` acts as a manual fallback), while `ProjectPreviewTab.vue` (reloads `<iframe>` on stale client state) and `ProjectReaTab.vue` (refreshes targets/status) benefit directly from `F5`.
   - Wired `oneship:context-refresh` listeners and removed redundant `RefreshCw` toolbar buttons from all four tabs.
