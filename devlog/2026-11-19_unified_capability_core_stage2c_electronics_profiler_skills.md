# Unified Capability Core — Stage 2C (`electronics`, `profiler`, `skills` & Legacy Cleanup)

**Date:** 2026-11-19  
**Specs:** Spec 35, Spec 36, Spec 40  

## Changes Implemented

1. **Dynamic Hardware Capabilities Provider ([src-tauri/src/capabilities/electronics.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/capabilities/electronics.rs)):**
   - Registered `domain = "electronics"` (`required_capability = Some("has_electronics_enabled")`, `linked_surface_path = Some("electronics/oscilloscope")`).
   - Implemented `dynamic_actions(project)` reading `OscilloscopeDescriptor` (`kopeng::electronics::instruments::owon_vds1022::descriptor()`) to advertise supported channels, timebase steps, volts/div steps, trigger sources, slopes, and modes.
   - Actions: `scan_instruments`, `get_oscilloscope_descriptor`, `set_channel_enabled`, `set_timebase`, `set_trigger`, and backend `capture_single` (reading persisted `ui_state` slots and returning scalar electrical metrics without waveform flooding).

2. **Slot-Backed Oscilloscope Sync ([src-tauri/src/ui_state.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/ui_state.rs), [src/stores/workspaceLayoutStore.ts](file:///home/butcherwutcher/Projects/dev/one-ship/src/stores/workspaceLayoutStore.ts), [src/components/workspace/electronics/OscilloscopeView.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/electronics/OscilloscopeView.vue)):**
   - Added 6 persistent state slots under `electronics/oscilloscope`: `electronics.oscilloscope_ch1_enabled`, `ch2_enabled`, `timebase_us`, `trigger_source`, `trigger_slope`, and `trigger_sweep`.
   - Synced slot snapshots into `useElectronicsStore()` inside `workspaceLayoutStore.applyUiSnapshot` and extended `OscilloscopeView` `ui-surface-command` handlers for `set_timebase` and `set_trigger`.

3. **Profiler, Skills & Journal Convergence ([src-tauri/src/capabilities/profiler.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/capabilities/profiler.rs), [src-tauri/src/capabilities/skills.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/capabilities/skills.rs), [src-tauri/src/capabilities/journal.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/capabilities/journal.rs), [src-tauri/src/capabilities/mod.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/capabilities/mod.rs)):**
   - Registered all 11 providers (`project`, `journal`, `forge`, `files`, `settings`, `git`, `scripts`, `logs`, `electronics`, `profiler`, `skills`) and typed error variants (`Electronics`, `Profiler`, `Skill`).
   - Added `record_technical_debt` and active-conversation MCP binding to `journal.rs`, and eliminated `.ok()` / `.unwrap_or` fallbacks in `profiler.rs` and `journal.rs`.

4. **MCP & Voice Legacy Removal ([src-tauri/src/mcp.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/mcp.rs), [src-tauri/src/voice_controller.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/voice_controller.rs), [docs/specs/40_unified_capability_core_and_voice_harness.md](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/40_unified_capability_core_and_voice_harness.md)):**
   - Routed `list_profiler_runs`, `get_run_profile`, `compare_profiler_runs`, `search_conversations`, `manage_todo_item`, `resolve_todo_item`, `record_technical_debt`, `manage_project_skill`, and project-scoped `search_project_skills` through `call_mcp_capability`.
   - Completely removed `execute_surface_action`, surface-node fallback in `capability_discover`, hardcoded `electronics/oscilloscope` injection in `ui_get_state`, and unused `VoiceControllerError::InvalidPath`.

## Verification

1. **Daemon Build & Runtime Health (`./scripts/run_dev.sh status && journalctl --user -u oneship-dev-daemon-build -n 15 --no-pager`):**
   - Build succeeded cleanly: `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 30.67s`.
   - `oneship-dev-daemon` active (`main_pid: 3451849`), listening on port `8765`.
   - Note: `oneship-dev-gui` remains `failed` from an earlier intermediate build; not restarted manually per `AGENTS.md` §1.4.

2. **MCP `tools/list` Live Probe (`POST http://127.0.0.1:8765/mcp`):**
   - Confirmed `search_conversations`, `manage_todo_item`, `record_technical_debt`, `resolve_todo_item`, `list_profiler_runs`, `get_run_profile`, `compare_profiler_runs`, `search_project_skills`, and `manage_project_skill` are registered and served on `127.0.0.1:8765`.

3. **Rule Gate, Fallback Audit & Legacy Removal (`./scripts/verify_rules.sh`, `grep`, `rg`):**
   - `rg -n "execute_surface_action" src-tauri/src/` -> **0** matches.
   - `grep -nE "unwrap_or|unwrap_or_default|\.ok\(\)|let _ =|catch..." src-tauri/src/capabilities/{mod,journal,forge,files,settings,git,scripts,logs,electronics,profiler,skills}.rs src/stores/workspaceLayoutStore.ts src/components/workspace/electronics/OscilloscopeView.vue` -> **0** matches across all 12 files audited in Stages 2A–2C (existing hits in Stage 1 `project.rs` optional field-update defaults only).
   - `./scripts/verify_rules.sh` -> zero violations in any Stage 2C Rust or frontend files (1 pre-existing failure remains on unrelated Vue modal `bg-black/*` classes).

4. **Full Test Suite (`cargo test --manifest-path src-tauri/Cargo.toml`):**
   - Removed orphaned `"system-agent-updated"` event entry and unused `AgentState` import from [src/utils/daemonEvents.ts](file:///home/butcherwutcher/Projects/dev/one-ship/src/utils/daemonEvents.ts) so frontend events match `DAEMON_EVENT_TYPES`.
   - `cargo test`: `oneship_lib` **`572 passed; 0 failed; 0 ignored`** (`0.64s`), including `events::tests::daemon_events_match_the_frontend_map`, `api::tests::api_commands_match_the_frontend_map`, `capabilities::tests::*`, `voice_harness::tests::*`, and `ui_state::tests::*`.
   - Deferred semantic-search engine/ranking adjustments per user direction pending separate effectiveness audit.
