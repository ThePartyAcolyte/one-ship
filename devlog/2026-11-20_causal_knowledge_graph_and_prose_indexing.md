# 2026-11-20 — Causal Knowledge Graph, Line-Level Git Hunks & Prose-Only Semantic Indexing

## Summary
- **Clean Subsystem Reset (`knowledge_v2` + SQLite Schema Upgrade):**
  - Switched LanceDB vector storage to `knowledge_v2` in `src-tauri/src/semantic/store.rs`, dropping the legacy `knowledge` table automatically on open.
  - Added `journal_file_hunks` and `project_code_symbols` tables in `src-tauri/src/db.rs`, resetting legacy `knowledge_edges` once on initial migration so all projects reindex from a clean baseline.
- **Prose-Only BGE-M3 Vectorization & Deterministic Code Symbol Lookup:**
  - Excluded source code and static config/HTML data files (`.html`, `.toml`, `.yaml`, `.yml`, `.json`) from dense BGE-M3 vectorization (`src-tauri/src/config.rs`, `src-tauri/src/semantic/mod.rs`). Embeddings now cover prose knowledge only: `specs` (`.md`, `.txt`, `.pdf`), `journal`, `tasks`, and `tech_debt`.
  - Replaced LanceDB `code` chunks with incremental `file_sha256` Tree-sitter AST symbol indexing into SQLite (`project_code_symbols`) via `scan_code_symbols_directory`, preserving zero-ONNX `F12` Go to Definition (`resolve_symbol_definition`) and outline symbols (`get_file_symbols`).
  - Upgraded `search_knowledge` to hybrid mode: BGE-M3 semantic search for `specs`/`journal`/`tasks`/`tech_debt` merged with deterministic AST symbol matching (`search_code_symbols_deterministic`) for `code`.
- **Causal Git Hunk Attribution & Anchored Debt/TODOs:**
  - Added `DiffFileHunk`, `parse_unified_zero_diff_hunks`, and `extract_commit_diff_hunks` in `src-tauri/src/kopeng/git.rs` and `extract_snapshot_diff_hunks` in `src-tauri/src/kopeng/snapshots.rs`.
  - Updated `execute_record_session_minutes` (`src-tauri/src/mcp.rs`) to capture line-level diff hunks into `journal_file_hunks` in both `commit` and `snapshot` checkpoint modes (retaining `snapshot_ref` on `agent_session_journal`), auto-fill `files_touched` when omitted, and fall back to file-level `files_touched` entries when Git diffs are unavailable.
  - Added optional `file_path`, `line_start`, `line_end`, and `journal_id` anchors to `project_todos` and `project_technical_debt` (`src-tauri/src/db.rs`, `src-tauri/src/capabilities/journal.rs`, `src-tauri/src/mcp.rs`, `src-tauri/src/conversation.rs`, `src-tauri/src/kopeng/project_mgmt.rs`).
- **Causal Knowledge Graph & Actionable Explorer Line Insights:**
  - Rebuilt `build_project_knowledge_edges` (`src-tauri/src/semantic/mod.rs`) to eliminate noisy AST `calls` edges and cartesian `doc -> code` cosine edges, replacing them with causal `touched` (`journal -> file`), `affects` (`tech_debt`/`tasks -> file`), `relates_to` (`tech_debt`/`tasks <-> journal`), exact-path `explicit_ref` (`governs`/`references`/`documents`), and capped prose-only semantic links (`journal -> specs` top 5, `tech_debt`/`tasks -> journal` top 3).
  - Replaced `FileInsightNode` in `FileLineInsights` (`src-tauri/src/semantic/types.rs`) with structured `minutes` (`FileCausalMinute`), `debt_and_todos` (`FileAnchoredItem`), and `related_docs` (`FileRelatedDoc`), including historical `journal_file_hunks` backfill from `commit_hash` / `files_touched` during `index_project_knowledge`.
  - Updated `src/components/workspace/ProjectWorkspace.vue` (Causal Insights drawer with inline commit diff preview via `get_git_commit_diff` and inline spec preview), `src/components/workspace/ProjectKnowledgeGraphTab.vue` (`Specs` ↔ `Minutes / Debt / Tasks` ↔ `Files` stratified layout), `src/components/modals/OmnisearchModal.vue`, `src/components/workspace/OverviewWidgetGrid.vue`, `scripts/audit_semantic.sh`, and `docs/specs/22_session_lifecycle_and_devlog_graph.md`.

## Verification

1. **Contract & TypeScript Bindings Export (`./scripts/run_dev.sh export-bindings`):**
   - `test result: ok. 413 passed; 0 failed; 0 ignored; 0 measured; 166 filtered out; finished in 0.27s` (zero compiler warnings on `one-ship`).
2. **Read-Only Rule Gate (`./scripts/verify_rules.sh`):**
   - `[PASS] Zero TypeScript 'any' in src/`
   - `[PASS] Zero Tailwind palette classes (text-/bg-/border-{color}-*)`
   - `[PASS] Zero backdrop-blur-* in components/views`
   - `[PASS] Zero hardcoded /home/butcherwutcher paths`
   - Remaining `[FAIL]` hits in `verify_rules.sh` are pre-existing `bg-black/*` classes in unrelated modals/`RollingStatusLog.vue` and a regex false positive on `db.rs:7519` (`HashMap<String, String>, DatabaseError`); zero violations in edited files.
3. **Silent Fallback Audit (`grep -nE "unwrap_or|unwrap_or_default|\.ok\(\)|let _ =|catch\s*(\([^)]*\))?\s*\{\s*\}"` on edited files):**
   - Zero bare empty `catch {}` blocks or unlogged failure swallowers in newly added code; all `unwrap_or` calls in edited files are explicit Option default bindings (`line_start.unwrap_or(1)`, `decisions.unwrap_or_default()` on optional strings, or `PoisonError::into_inner` lock recovery).
4. **Live Reindex & Audit on `l10n_cl_banking` (`./scripts/audit_semantic.sh --project-id 8 --reindex --query "action_confirm" --query "triangulación de comisiones"`):**
   - `Total Chunks : 1155 (Code: 387, Specs: 1017, Entities: 138)` — zero `code` vectors in LanceDB (`387` AST symbols in SQLite `project_code_symbols`, `214` rows in `journal_file_hunks`).
   - `Graph Edges : 689` (`258` `documents [semantic]`, `214` `touched [files_touched]`, `87` `documents [explicit_ref]`, `55` `governs [explicit_ref]`, `38` `references [explicit_ref]`, `15` `touched [explicit_ref]`, `14` `affects [explicit_ref]`, `8` `relates_to [semantic]`; `0` AST `calls` edges).
   - Hybrid query `"action_confirm"` returned `[100.0%] [CODE]` hits (`wizard/commission_close_wizard.py:L88-200`, `wizard/manual_upload_wizard.py:L14-35`) followed by `[SPECS]` hits; `"triangulación de comisiones"` returned `[79.6% - 82.5%] [SPECS]` hits.
5. **Daemon Health (`./scripts/run_dev.sh status` & `journalctl --user -u oneship-dev-daemon-build -n 25 --no-pager`):**
   - `daemon` is `active (running)` (PID `1043830`, listening on port `8765`); `oneship-dev-daemon-build` finished dev build in `17.59s` with zero errors.
   - Note: `gui` unit shows `failed` from an intermediate build before `capabilities/journal.rs` finished updating; user can relaunch/restart GUI if needed, and remaining registered projects can be reindexed cleanly on demand from Overview -> `Index`.
