# Devlog: Unified Capability Core — Knowledge & Causal Graph Provider (`domain = "knowledge"`)

- **Date:** 2026-11-20
- **Specs:** [`docs/specs/40_unified_capability_core_and_voice_harness.md`](../docs/specs/40_unified_capability_core_and_voice_harness.md), [`docs/specs/22_session_lifecycle_and_devlog_graph.md`](../docs/specs/22_session_lifecycle_and_devlog_graph.md)
- **Preceded by:** [`devlog/2026-11-20_causal_knowledge_graph_and_prose_indexing.md`](2026-11-20_causal_knowledge_graph_and_prose_indexing.md)

## Summary of Changes

1. **New Capability Provider ([`src-tauri/src/capabilities/knowledge.rs`](../src-tauri/src/capabilities/knowledge.rs)):**
   - Registered `PROVIDER` (`domain_id = "knowledge"`, `linked_surface_path = Some("knowledge")`) in [`src-tauri/src/capabilities/mod.rs`](../src-tauri/src/capabilities/mod.rs) as provider #12 (`&knowledge::PROVIDER`).
   - Exposed 7 `Shared` backend actions aligned with the refactored `semantic` subsystem:
     - `search_knowledge` (`mcp_name: Some("search_knowledge")`): BGE-M3 prose vector search (`specs`, `journal`, `tasks`, `tech_debt`) + deterministic Tree-sitter AST code lookup + 1-hop causal/transversal relations (`scope`, `limit`, `include_relations`, `max_relations_per_result`, `min_relation_score_percent`) with strict parameter validation.
     - `resolve_symbol_definition`: Zero-ONNX deterministic SQLite `project_code_symbols` lookup with `paginate_items` (`offset`/`limit`).
     - `get_file_symbols`: Structural Tree-sitter AST / Markdown outline per file with `paginate_items` (`offset`/`limit`) and strict relative path containment check.
     - `get_file_line_insights` (`mcp_name: Some("get_file_line_insights")`): Deterministic causal context (enclosing symbol/section, `minutes` from `journal_file_hunks`/`files_touched`, anchored `debt_and_todos`, `related_docs`) for `(rel_path, line)`.
     - `get_indexing_status`: Index health & counts (`total_records_count`, `code_chunks_count`, `spec_chunks_count`, `entity_chunks_count`, `graph_edges_count`, `is_indexing`, `auto_index_enabled`, `min_score_percent`).
     - `index_project`: Triggers `semantic::index_project_knowledge(project_id, force)`.
     - `rebuild_graph`: Triggers `<300 ms` `semantic::rebuild_project_knowledge_graph(project_id)`.
   - All parameters include concise 1-line `"description"` fields (enforced by unit test `every_knowledge_parameter_declares_a_description`).
2. **`files` Provider Separation ([`src-tauri/src/capabilities/files.rs`](../src-tauri/src/capabilities/files.rs)):**
   - Removed legacy `semantic_search` action, `MAX_SEMANTIC_LIMIT_COUNT`, `parse_scope`, and `crate::semantic` imports so `files` handles strictly filesystem search/listing/excerpts/scope/`open_file` and `knowledge` owns all semantic/AST/causal tools.
3. **MCP Server Migration ([`src-tauri/src/mcp.rs`](../src-tauri/src/mcp.rs)):**
   - Replaced standalone `execute_search_knowledge` (and its silent fallbacks) with `call_mcp_capability(caller, target.as_deref(), "knowledge", "search_knowledge", args)`.
   - Updated `search_knowledge` `tools/list` schema with `project`, `max_relations_per_result`, and `min_relation_score_percent`.
   - Added `get_file_line_insights` to `tools/list` and `tools/call` routed through `call_mcp_capability`.
4. **Living Specs Updated:**
   - Synced [`docs/specs/40_unified_capability_core_and_voice_harness.md`](../docs/specs/40_unified_capability_core_and_voice_harness.md) and [`docs/specs/22_session_lifecycle_and_devlog_graph.md`](../docs/specs/22_session_lifecycle_and_devlog_graph.md).

## Verification

1. **Dev Daemon Build & Live Runtime (`./scripts/run_dev.sh status`, `journalctl --user -u oneship-dev-daemon-build -n 15 --no-pager`, live `POST /mcp`):**
   - `oneship-dev-daemon-build`: `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 32.35s`.
   - `daemon` active (`main_pid: 1569979`), listening on port `8765`; `gui` active (`gui_window_pid: 1575063`).
   - Live `tools/list` probe on `http://127.0.0.1:8765/mcp` confirmed both `"search_knowledge"` and `"get_file_line_insights"`.
2. **Project Rule Gate (`./scripts/verify_rules.sh`):**
   - Zero violations in any newly added or edited file (`knowledge.rs`, `files.rs`, `mod.rs`, `mcp.rs`, specs); remaining 2 summary failures are pre-existing (`modals/*.vue` / `RollingStatusLog.vue` `bg-black/*` and false positive `db.rs:7548`).
3. **Fallback Audit (`grep -nE "unwrap_or|unwrap_or_default|\.ok\(\)|let _ =|catch..." src-tauri/src/capabilities/{knowledge,files,mod}.rs`):**
   - `knowledge.rs`: only `.unwrap_or("")` inside `#[cfg(test)]` assertion helper at line `426`; runtime code propagates all errors via `?` and `CapabilityError`.
   - `files.rs`: early `parse_pagination(&params)?` validation before blocking walk (`lines 347, 359`).
   - `mod.rs`: integer conversion `u64 -> usize` (`try_from(n).ok()`) inside `parse_pagination`.
4. **Full Rust Test Suite (`cargo test --manifest-path src-tauri/Cargo.toml`):**
   - `oneship_lib`: `test result: ok. 582 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.65s` (compiled in `27.28s`), including:
     - `capabilities::knowledge::tests::scope_and_numeric_parsers_reject_invalid_inputs`
     - `capabilities::knowledge::tests::validate_relative_path_rejects_traversal_and_absolute_paths`
     - `capabilities::knowledge::tests::every_knowledge_parameter_declares_a_description`
     - `capabilities::tests::domain_enablement_respects_capabilities_independent_of_hidden_tabs`
     - `capabilities::tests::every_provider_discovery_payload_is_complete_and_valid`
   - `src/main.rs`, `src/bin/daemon.rs`, and Doc-tests `oneship_lib`: `0 passed; 0 failed`.
