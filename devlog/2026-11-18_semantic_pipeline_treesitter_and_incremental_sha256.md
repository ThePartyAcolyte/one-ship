# Devlog — 2026-11-18: Semantic Search Pipeline Upgrade (Phase A: Multi-Language Tree-sitter AST, Two-Tier SHA-256 Incremental Indexing & PDF Extraction)

## Summary
Implemented Phase A of the Semantic Search & Knowledge Graph architecture (Spec 22 §5.2–§5.8, resolving the original Technical Debt #29 pipeline items):

1. **Multi-Language Tree-sitter AST & Structural Chunker (`src-tauri/src/semantic/chunker.rs`, `src-tauri/Cargo.toml`, `src-tauri/src/config.rs`):**
   - Integrated `tree-sitter` (`0.27`) with 16 language grammars (`rust`, `typescript` + `tsx`, `javascript`, `python`, `go`, `java`, `c-sharp`, `c`, `cpp`, `ruby`, `php`, `bash`, `html`, `css`, `json`).
   - Added Single-File Component (`.vue`, `.svelte`, `.astro`) extraction that delegates `<script>`, `<template>`, and `<style>` blocks to their respective AST grammars while preserving wrapper line offsets.
   - Replaced line-number-dependent IDs with stable line-independent symbol keys (`code::<project_id>::<rel_path>::<qualified_symbol>`, with `#2`, `#3` disambiguation on duplicates) and attached leading docstrings/comments to symbol chunks.
   - Upgraded Tier 2 fallback into a brace-depth `{}` and blank-line-aware structural segmenter (`40`-line sliding window with `10`-line overlap when blocks exceed `55` lines).
   - Fixed UTF-8 preview slicing (`truncate_utf8`) to prevent char-boundary panics on multi-byte characters.
   - Added pure-Rust `.pdf` page extraction (`pdf-extract`) producing `specs::<project_id>::<rel_path>#page_<n>` chunks.

2. **Two-Tier SHA-256 Incremental Indexing & Batch Orphan Pruning (`src-tauri/src/semantic/store.rs`, `src-tauri/src/semantic/mod.rs`, `src-tauri/src/semantic/types.rs`):**
   - Extended the LanceDB `knowledge` Arrow schema with `file_sha256` and `content_sha256` columns (auto-recreating legacy pre-hash tables on schema mismatch).
   - Added `store::fetch_project_records` and `store::delete_records_by_ids` (batched `id IN (...)` deletion replacing N+1 per-record deletes).
   - Implemented Level 1 (`file_sha256` fast-skip of unchanged files), Level 2 (`content_sha256` diffing that reuses stored 1024-dim BGE-M3 vectors when only line offsets shift), and automatic batch pruning of orphaned record IDs when files or symbols are removed.

3. **Live Runtime Status, `nice 19` Worker Priority & JIT Line Resolution Fix (`src-tauri/src/semantic/mod.rs`):**
   - Added poison-safe `INDEXING_STATES` registry and `ActiveIndexingGuard` so `get_indexing_status` reports live `is_indexing` and `last_duration_ms` and rejects duplicate concurrent runs on the same project.
   - Applied `libc::nice(19)` priority and mid-run throttler checks for background (`force == false`) indexing workers.
   - Fixed JIT line resolution in `search_knowledge` to resolve relative `file_path` entries against the owning project's `root_path` and preserve line spans (`line_end - line_start`).

4. **Phase B — Hybrid Knowledge Graph (`knowledge_edges`), Clean Store Reset & Omnisearch Ordering Fix (`src-tauri/src/db.rs`, `src-tauri/src/semantic/`, `src/components/modals/OmnisearchModal.vue`):**
   - Created the `knowledge_edges` SQLite table and `(project_id, source_id)` / `(project_id, target_id)` indexes (`db_replace_project_knowledge_edges`, `db_get_knowledge_relations_for_ids`).
   - Extended `chunker.rs` to extract deterministic AST call/import references (`collect_ast_references`) and prose backtick/path references (`extract_text_references`), building `calls`, `governs`, and `affects` edges during `index_project_knowledge`.
   - Added `symbol_name` to the LanceDB `knowledge` Arrow schema, automatically resetting any pre-Phase-B `knowledge` table so vector storage starts clean.
   - Added `visited_files` deduplication in `scan_docs_directory` and `scan_code_directory` (preventing double indexing when `docs/specs` and `docs` overlap), batch deduplication in `store::upsert_records` and `store::search_knowledge`, and `:key="`${item.id}-${idx}`"` + explicit descending score sort and clickable 1-hop `Graph:` relation chips in `OmnisearchModal.vue`.

5. **Reactive Auto-Indexing, Uncapped Weighted Semantic Edges, Overview Widget & CLI Audit Bridge (`src-tauri/src/semantic/mod.rs`, `src-tauri/src/config.rs`, `src-tauri/src/overview.rs`, `src/components/workspace/OverviewWidgetGrid.vue`, `scripts/audit_semantic.sh`):**
   - Added configurable `[plugins.semantic]` parameters (`semantic_edge_min_score_percent = 70`, `file_watch_debounce_ms = 10000`, `entity_watch_debounce_ms = 2000`) and removed artificial storage-time edge caps (`semantic_edge_max_per_node_count`), storing all qualifying edges with their exact similarity `weight` (`0.0–1.0`) and exposing optional query-time filters (`max_relations_per_result`, `min_relation_score_percent`) in `SearchKnowledgeArgs`.
   - Connected debounced reactive background indexing (`schedule_background_indexing_for_path` in `documents.rs` and `schedule_background_indexing_for_entity` in `mcp.rs`) so file edits and SQLite entity mutations automatically update LanceDB and `knowledge_edges`.
   - Added cross-domain semantic vector edges (`origin = "semantic"`) linking `specs`, `journal`, `tasks`, and `tech_debt` chunks to `code` chunks when cosine similarity meets `semantic_edge_min_score_percent` (excluding static config/data files `.toml`/`.json`/`.yaml`), plus Astro `---` frontmatter + template extraction and Python container deduplication (`Class::method` instead of `Class::Class::method`) in `chunker.rs`.
   - Implemented dynamic hierarchical Markdown coalescing (`H1..H6` with fenced-code guard and ancestor breadcrumbs, coalescing `H4..H6` sub-sections $\le 4,000$ bytes and subdividing oversized sections without artificial input truncation).
   - Added 10-chunk incremental checkpointing (`store::upsert_records` per micro-batch) with universal `libc::nice(19)` worker priority and `20 ms` cooperative pauses so interrupted initial indexation runs never lose progress and the desktop remains responsive.
   - Expanded `OverviewKnowledgeSummary` and `OverviewWidgetGrid.vue` to display total `CHUNKS`, `GRAPH EDGES`, `CODE`/`SPECS`/`ENTITIES` counts, `AUTO-SYNC: ON/OFF`, and active `MIN %` threshold.
   - Added local `/api/v1/audit/semantic` endpoint and `./scripts/audit_semantic.sh` CLI bridge to audit chunks, graph edges, threshold candidates, and query rankings against the active daemon without spawning a second ONNX instance.

6. **First-Level Interactive Knowledge Graph Tab (`ProjectKnowledgeGraphTab.vue`, `src-tauri/src/api.rs`, `src/components/workspace/ProjectWorkspace.vue`):**
   - Added `get_project_knowledge_graph` and `rebuild_project_knowledge_graph` (< 300 ms edge rebuild directly from LanceDB vectors without re-running BGE-M3) daemon commands.
   - Created first-level workspace tab **`Graph` (`id: 'knowledge'`)** (`src/components/workspace/ProjectKnowledgeGraphTab.vue`) powered by D3 (`d3-force`, `d3-zoom`, `d3-drag`, `d3-selection`, `d3-transition`) with expandable file/spec super-nodes (`+`/`-`), directional curved edges with arrowheads and animated flow, `Flow` vs `Force` layouts with `120`-tick synchronous pre-warming, explicit-submit semantic spotlight (`Enter`/button) with module auto-expansion, live `Min Weight` slider (`50%..100%`), and a resizable inspector with `1-Hop`/`2-Hop` Ego-Graph isolation and direct editor navigation.

7. **Documentation & Backlog Synchronization (`docs/specs/22_session_lifecycle_and_devlog_graph.md`, `TECHNICAL_DEBT.md`, `TODO.md`):**
   - Updated Spec 22 to v`0.2.3` documenting Phase A, Phase B, reactive auto-indexing, weighted semantic edges, the CLI audit bridge, and the interactive `Graph` tab.
