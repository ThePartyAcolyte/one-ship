# Devlog: Voice Schema Descriptions, Unclipped 2-Layer Discovery, Pagination & API Telemetry

- **Date:** 2026-11-20
- **Spec:** [`docs/specs/40_unified_capability_core_and_voice_harness.md`](../docs/specs/40_unified_capability_core_and_voice_harness.md) (§4.3)
- **Scope:** Remove silent response truncation, add parameter descriptions to all 11 capability providers, introduce explicit `offset`/`limit` pagination, and log Gemini Live token & payload byte telemetry.

## Summary of Changes

1. **Human-Readable Parameter Descriptions (11 Providers):**
   - Added concise 1-line `"description"` metadata to parameters inside `parameters_schema` across all 11 capability providers (`project.rs`, `journal.rs`, `forge.rs`, `files.rs`, `settings.rs`, `git.rs`, `scripts.rs`, `logs.rs`, `electronics.rs`, `profiler.rs`, `skills.rs`) so Gemini Live / Voice and MCP callers understand parameter semantics without memorizing internal field names.
2. **Unclipped 2-Layer Capability Discovery (`src-tauri/src/capabilities/mod.rs`):**
   - Added `build_discovery_payload(&HydratedProject, &CapabilityProvider, &CapabilityCaller) -> Result<Value, CapabilityError>` projecting concise caller-visible actions (`id`, `description`, `risk`, `parameters`) without internal fields (`mcpName`, `visibility`, `target`, `voiceMaxItems`).
   - Updated `discover_domain` to propagate `build_discovery_payload` errors via `?` (no `.ok()` swallowing).
   - Replaced `every_provider_discovery_payload_fits_within_voice_budget` with `every_provider_discovery_payload_is_complete_and_valid`.
   - Updated `ui_get_state` in `src-tauri/src/voice_controller.rs` to return `DomainIndexEntry` objects (`domain`, `label`, `actionCount`) from `list_enabled_domains`.
3. **Explicit `offset`/`limit` Pagination (`capabilities/`):**
   - Added `parse_pagination` and `paginate_items` in `src-tauri/src/capabilities/mod.rs` returning `{ <list_key>, total_count, offset, limit, has_more }`. When omitted, `limit` returns the full result set (`offset = 0`, `limit = total_count`).
   - Updated list actions in `project.rs` (`list_tasks`), `journal.rs` (`query_session_journal`, `list_todos`, `list_technical_debt`), `forge.rs` (`list_issues`, `list_prs`, `list_releases`, `list_inbox`), `git.rs` (`list_commits`, `list_snapshots`), and `files.rs` (`search_files`, `list_directory`) to use `paginate_items`.
   - Removed forced caps/truncation helpers (`bounded_limit`, `.truncate(...)`, `MAX_VOICE_DIFF_CHARS` / `truncate_utf8`, `MAX_FILE_SEARCH_RESULTS_COUNT`, `MAX_DIR_LIST_ENTRIES_COUNT`, `compact_tool_response` / `compact_value_recursive`).
4. **Gemini Live API & Token Telemetry (`src-tauri/src/voice_controller.rs`, `src-tauri/src/config.rs`):**
   - Logged `INFO` setup payload size (`Sending Gemini Live setup payload ({} bytes)`) in `try_open_gemini_setup`.
   - Logged `INFO` `Gemini usageMetadata: prompt=... response=... total=... details=...` on upstream `usageMetadata` frames.
   - Logged per-call `raw_bytes` and batch `toolResponse` byte size, turning `voice_tool_response_max_bytes` into a non-truncating `WARN` log threshold (updated schema & docs in `config.rs` and Spec 40 §4.3).
   - Updated upstream `Close` frame handling to transition `VoiceControllerState` to `Error` on non-normal WebSocket close codes (`f.code != CloseCode::Normal`) and log `ERROR` if sending `toolResponse` to the Gemini writer channel fails.

## Verification

### 1. Full Rust Test Suite (`cargo test --manifest-path src-tauri/Cargo.toml`)
Executed after all final code edits (`task-938`):

```text
Finished `test` profile [unoptimized + debuginfo] target(s) in 46.27s
Running unittests src/lib.rs (src-tauri/target/debug/deps/oneship_lib-9aea6580ec5b5c2d)

test result: ok. 579 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.07s

Running unittests src/main.rs (src-tauri/target/debug/deps/one_ship-f63f56ed17e6cf54)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

Running unittests src/bin/daemon.rs (src-tauri/target/debug/deps/oneship_daemon-8ce2447ebb4e6358)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

Doc-tests oneship_lib
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### 2. Targeted Fallback & Swallowing Check
Checked `capabilities/{mod,project,journal,forge,git,files}.rs`, `voice_controller.rs`, `voice_harness.rs`:
- `capabilities/mod.rs`: only integer conversion `u64 -> usize` (`try_from(n).ok()`) inside `parse_pagination`; zero `.ok()` in `discover_domain`.
- `capabilities/{journal,git,files}.rs`: pre-validation `let _ = parse_pagination(&params)?;` before I/O (propagates errors via `?`).
- `capabilities/project.rs`: optional patch field updates fallback to `existing.*` or stage/counter zero-init `0`.
- `voice_controller.rs` & `voice_harness.rs`: pre-existing channel `try_send` / child process waits & optional JSON string extractions; zero new silent error swallowing introduced.

### 3. Project Rule Gate (`./scripts/verify_rules.sh`)
```text
[PASS] Zero TypeScript 'any' in src/
[WARN] Native title= attributes found (verify these are component props, not HTML attributes): ... (pre-existing Vue AppEmptyState props)
[WARN] Lock .unwrap()/.expect() found (must only exist inside #[cfg(test)]): src-tauri/src/kopeng/forge/gitlab.rs:721
[PASS] Zero Tailwind palette classes (text-/bg-/border-{color}-*)
[FAIL] Zero raw bg-black/* or bg-[#...] outside ui.css (pre-existing floating/modals/*.vue)
[FAIL] Unstructured Result<_, String> found outside tests: src-tauri/src/db.rs:7519 (false positive regex match on HashMap<String, String>)
[PASS] Zero backdrop-blur-* in components/views
[PASS] Zero hardcoded /home/butcherwutcher paths
=== Summary: 2 hard failure(s) ===
```
Zero violations in any file modified during this workstream.

### 4. Daemon Health (`./scripts/run_dev.sh status`)
- `daemon` service `active/running` (`main_pid: 1386336`, listening on port `8765`).
- Previous `oneship-dev-daemon-build` finished cleanly (`Finished 'dev' profile [unoptimized + debuginfo] target(s) in 1m 22s`) and a follow-up rebuild (`main_pid: 1391648`) was queued on the cargo directory lock while `cargo test` completed.
