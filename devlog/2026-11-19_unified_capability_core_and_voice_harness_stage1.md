# Devlog: Unified Capability Core & Voice Orchestrator Harness (Spec 40 — Stage 1)

**Date:** `2026-11-19`  
**Spec:** [`docs/specs/40_unified_capability_core_and_voice_harness.md`](../docs/specs/40_unified_capability_core_and_voice_harness.md)  
**Scope:** Base Infrastructure (`CapabilityCore` + `VoiceSessionHarness`) + Pilot Providers (`project` and `journal`)

---

## Summary of Changes

1. **Specification (`docs/specs/40_unified_capability_core_and_voice_harness.md`):**
   - Defined the two-layer architecture unifying `mcp.rs` and `voice_controller.rs`:
     - `CapabilityProvider` / `CapabilityRegistry` with inversion of control, `Shared | McpOnly | VoiceOnly` visibility, `ReadOnly | Mutating | ConfirmRequired` risk levels, and dynamic plugin capabilities.
     - `VoiceSessionHarness` with SQLite working session persistence, scratchpad, deterministic rolling summary folding, `sessionResumption`, `contextWindowCompression`, idle WebSocket hibernation, and 65K TPM response compaction.
     - Complete decoupling of `ui_navigate` (visual presentation only) from domain capability execution.

2. **Configuration (`src-tauri/src/config.rs`):**
   - Added 6 `global_only` fields to `[plugins.voice_controller]` schema and `VoiceControllerConfig`: `idle_disconnect_seconds` (`180`), `turn_ledger_max_entries_count` (`12`), `scratchpad_max_items_count` (`20`), `context_compression_trigger_tokens` (`32000`), `context_compression_target_tokens` (`16000`), and `voice_tool_response_max_bytes` (`6144`), with explicit range/type validation warnings.

3. **Persistence (`src-tauri/src/db.rs`):**
   - Added `voice_sessions`, `voice_scratchpad_items`, and `voice_turn_ledger` tables + indexes.
   - Added transactional CRUD helpers (`db_get_or_create_active_voice_session`, `db_archive_and_create_voice_session`, `db_update_voice_session_resumption`, `db_update_voice_session_focus`, scratchpad CRUD, `db_list_voice_turn_ledger`, `db_append_voice_turn_and_fold`) and `db_search_project_conversations` for project-wide FTS5 conversation search.
   - Added unit test `voice_turn_ledger_folds_overflow_deterministically_into_summary`.

4. **Unified Capability Core (`src-tauri/src/capabilities/` & `src-tauri/src/ui_state.rs`):**
   - Created `src-tauri/src/capabilities/mod.rs` (`CapabilityProvider`, `CapabilityDef`, `CapabilityCaller`, `CapabilityError` implementing `api::ApiError`, `discover_domain`, `execute_capability`, `list_enabled_domains`, and unit tests).
   - Created `src-tauri/src/capabilities/project.rs` (`domain = "project"`): stages, tasks, subtasks, milestones, and task relations, enforcing `project_id` ownership checks (`db_get_project_stage`, `db_get_project_task`, `db_get_task_subtask_project_id`, `db_get_project_milestone`) before every mutation/deletion.
   - Created `src-tauri/src/capabilities/journal.rs` (`domain = "journal"`): `query_session_journal`, `list_todos`, `create_todo`, `update_todo_status`, `list_technical_debt`, `resolve_technical_debt`, and `search_conversations`.
   - Made `ui_state::project_has_capability` `pub(crate)` for shared gating.

5. **Voice Orchestrator Harness & Controller (`src-tauri/src/voice_harness.rs`, `src-tauri/src/voice_controller.rs`, `src-tauri/src/lib.rs`):**
   - Created `src-tauri/src/voice_harness.rs` implementing Context Brief compilation, `harness_scratchpad` / `harness_session_reset`, turn ledger recording, resumption handle persistence, and `compact_tool_response` byte-budget enforcement (plus unit tests).
   - Updated `src-tauri/src/voice_controller.rs`:
     - Wired `VoiceSessionHarness` lifecycle, `contextWindowCompression`, `sessionResumption`, `sessionResumptionUpdate` persistence, per-turn transcription/action tracking, and idle WebSocket hibernation.
     - Replaced `surface_get_capabilities` / `surface_execute_action` with navigation-agnostic `capability_discover` / `capability_execute` + `harness_scratchpad` / `harness_session_reset`.
     - Enriched `ui_get_state` with per-project `available_domains`.
     - Deleted the duplicated `("project", ...)` handlers from `execute_surface_action`; remaining unmigrated nodes (`settings`, `project/forge`, `files`, `electronics/oscilloscope`) stay routed through `execute_surface_action` until Stage 2.

6. **MCP Bridge (`src-tauri/src/mcp.rs`):**
   - Added `call_mcp_capability` adapter preserving `CallerContext` project resolution and bridged pilot tools (`query_project_tasks`, `link_journal_to_task`, `manage_project_stage`, `manage_project_milestone`, `manage_project_task`, `manage_task_subtask`, `resolve_technical_debt`, `query_session_journal`) onto `crate::capabilities::execute_capability`, deleting the duplicated handler implementations.

---

## Verification

### 1. Daemon Build & Health (Read-Only)
Command: `./scripts/run_dev.sh status && journalctl --user -u oneship-dev-daemon-build -n 30 --no-pager`

```text
{"units":{"daemon":{"active_state":"active","sub_state":"running","result":"success","main_pid":2742212,"log_tail":null},"daemon_build":{"active_state":"inactive","sub_state":"dead","result":"success","main_pid":null,"log_tail":null},"daemon_watch":{"active_state":"active","sub_state":"running","result":"success","main_pid":4160739,"log_tail":null},"vite":{"active_state":"active","sub_state":"running","result":"success","main_pid":4157981,"log_tail":null},"gui":{"active_state":"active","sub_state":"running","result":"success","main_pid":2746970,"log_tail":null}},"daemon_listening_ports":[8765],"vite_listening_ports":[],"gui_window_pid":2747093,"build_progress":{"phase":"cargo_build","completed_count":1187,"total_count":1188,"percent":99,"current_item":"linking","started_at_seconds":1795117284,"last_duration_seconds":null},"gui_progress":{"phase":"running","completed_count":null,"total_count":null,"percent":100,"current_item":"one-ship","started_at_seconds":1795117354,"last_duration_seconds":null}}
...
Nov 19 16:42:05 omarchy run_dev.sh[2737212]:     Finished `dev` profile [unoptimized + debuginfo] target(s) in 41.18s
Nov 19 16:42:05 omarchy systemd[1259]: Finished [systemd-run] /home/butcherwutcher/Projects/dev/one-ship/scripts/run_dev.sh _build-daemon-inner.
```

### 2. Read-Only Rule Gate (`./scripts/verify_rules.sh`)
- Zero TypeScript `any`, zero `Result<_, String>` in production Rust modules, zero `backdrop-blur-*`, zero hardcoded `/home/butcherwutcher` paths.
- Existing unrelated `FAIL` reported on legacy `bg-black/*` classes in modal/log Vue files (tracked in `TECHNICAL_DEBT.md #45`); zero violations introduced by this session (no `.vue`/`.ts` files edited).

### 3. Silent Fallback Scan on New/Edited Rust Files
Command: `grep -nE "unwrap_or|unwrap_or_default|\.ok\(\)|let _ =|catch\s*(\([^)]*\))?\s*\{\s*\}" src-tauri/src/capabilities/mod.rs src-tauri/src/capabilities/project.rs src-tauri/src/capabilities/journal.rs src-tauri/src/voice_harness.rs src-tauri/src/voice_controller.rs`
- All hits in `capabilities/{project,journal}.rs` and `voice_harness.rs` are explicit optional parameter defaults (`limit`, `priority`, `category`, `done_only`, `max_items`) or partial-update field preservation (`existing.*`).

### 5. Post-Test Follow-up (`BEGIN IMMEDIATE` & `voiceActivity`)
- Observed `database is locked` during back-to-back `harness_scratchpad` `add` calls caused by SQLite `BEGIN DEFERRED` read-to-write snapshot upgrade contention against concurrent `logger` `system_logs` micro-batches.
- Switched all 6 `voice_*` write transactions in `src-tauri/src/db.rs` (`db_get_or_create_active_voice_session`, `db_archive_and_create_voice_session`, `db_update_voice_session_focus`, `db_insert_voice_scratchpad_item`, `db_update_voice_scratchpad_item`, `append_voice_turn_and_fold_on`) to `TransactionBehavior::Immediate`, and added `"voiceActivity"` to recognized Gemini Live frame keys in `src-tauri/src/voice_controller.rs`.
- Re-verified via `./scripts/run_dev.sh status && journalctl --user -u oneship-dev-daemon-build -n 15 --no-pager`:
  - `Finished dev profile [unoptimized + debuginfo] target(s) in 29.18s`
  - `daemon` active (`main_pid: 2847956`), `gui` active (`gui_window_pid: 2844167`).

### 6. Resumption Handle Fallback Fix (`BidiGenerateContent session not found or expired`)
- Observed `CloseFrame { code: Policy, reason: "BidiGenerateContent session not found or expired" }` when connecting with an expired `resumption_handle`, causing `ptt_start` to wait 5s for `setupComplete` before the socket tore down without recording.
- Added `try_open_gemini_setup` and `open_gemini_live_and_await_setup` in `src-tauri/src/voice_controller.rs` to await `setupComplete` before spawning `local_reader`/`gemini_reader`; if `resumption_handle` is rejected before `setupComplete`, it clears the handle in SQLite and immediately reconnects cleanly hydrated via Context Brief, while keeping MPRIS pause + `pw-record` strictly gated on `setupComplete`.
- Cleared invalidated handles proactively on `"not found"` / `"expired"` / `"inactivity"` `CloseFrame`s, and reduced `RESUMPTION_HANDLE_TTL_SECONDS` to `600` in `src-tauri/src/voice_harness.rs`.
- Re-verified via `./scripts/run_dev.sh status && journalctl --user -u oneship-dev-daemon-build -n 10 --no-pager`:
  - `Finished dev profile [unoptimized + debuginfo] target(s) in 41.44s`
  - `daemon` active (`main_pid: 2977443`), `gui` active (`gui_window_pid: 2972438`).

### 7. Remaining Open Items
- `cargo test` (new unit tests in `capabilities/mod.rs`, `voice_harness.rs`, and `db.rs`) has not been executed yet per `AGENTS.md` §1.4 (must be requested explicitly by user).
- Runtime verification of the Stage 1 pilot: while focused on `project/board`, query `journal` / `todos` via Global Voice Controller without navigation, update `harness_scratchpad`, and test idle-disconnect + resume continuity.
- Stage 2 migration of remaining domains (`forge` + MCP `github_*` -> `forge_*`, `files`/`semantic`, `settings`, `electronics` dynamic descriptors, `git`/snapshots, `scripts`, `profiler`, `desigma`, `cad`, `pcb`, `rea`, `skills`).
