# Devlog — 2026-11-19: WP-A Remediation (Voice Controller, Header Toggle, Config Diagnostics & Milestone Ownership)

## Summary
Executed Work Package **WP-A** from [`docs/audit/post_audit_2026-11-19_findings.md`](../docs/audit/post_audit_2026-11-19_findings.md), resolving **4 High** (`PA-A01`–`PA-A04`) and **7 Medium** (`PA-M01`–`PA-M05`, `PA-M09`, `PA-M10`) post-audit findings across `voice_controller.rs`, `config.rs`, `api.rs`, `db.rs`, `project_mgmt.rs`, `overview.rs`, `mcp.rs`, `daemonApi.ts`, `milestoneStore.ts`, `AppHeader.vue`, and `Spec 36`.

## Changes Implemented

1. **Voice Controller Strictness & Typed Errors (`src-tauri/src/voice_controller.rs`):**
   - Extended `VoiceControllerError` with `InvalidConfig`, `ProjectNotFound`, `InvalidPath`, `Tool`, and `Io`, migrating `resolve_project_info`, `search_project_files`, `list_project_directory`, `flatten_blocking_tool_result`, and `spawn_blocking` tool blocks away from `Result<_, String>` (`PA-M01`).
   - Added `validate_project_id(project_id: i64) -> Result<usize, VoiceControllerError>` guarding against `project_id <= 0` and replacing raw `project_id as usize` casts; wired `surface_get_capabilities` to validate `project_id` via `resolve_project_info` (`PA-M03`).
   - Updated `surface_execute_action` to reject invalid `params_json` explicitly and removed the legacy `args.get("params")` compatibility shim (`PA-A01`).
   - Checked `dispatch_ui_action(SetSlotValue files.selected_document_path)` results in `files.open_file`, logging `WARN` and returning an error JSON on failure (`PA-M02`).
   - Updated `test_voice_controller_voice` to return `VoiceControllerError::InvalidConfig` when an unrecognized `model`, `persona_style`, or `voice_name` is supplied (`PA-A02`).

2. **Typed Header Toggle & Config Warning Diagnostics (`src-tauri/src/config.rs`, `api.rs`, `daemonApi.ts`, `AppHeader.vue`):**
   - Added `SetVoiceControllerEnabledArgs` (`#[ts(export)]`) and `set_voice_controller_enabled` in `config.rs`, registered in `api::API_COMMANDS` / `api::dispatch` and mirrored in `src/types/daemonApi.ts` (`PA-A04`).
   - Replaced client-side regex TOML patching (`[voice_controller]`) in `AppHeader.vue` (`toggleGlobalVoiceController`) with `daemonCall('set_voice_controller_enabled', { enabled: nextEnabled })` and `reportAsyncFailure` (`PA-A04`, partial `PA-B03`).
   - Updated `get_voice_controller_config` to emit deduplicated `WARN` logs via `logger::log` whenever present fields in `[plugins.voice_controller]` have invalid types or values (`PA-A03`).
   - Updated `read_webkit_renderer_mode` to log explicit `WARN` messages to `stderr` when `config.toml` cannot be read/parsed or `[system].webkit_renderer_mode` is invalid (`PA-M05`).

3. **Project Management Tracking Mode & Transactional Milestone Ownership (`db.rs`, `project_mgmt.rs`, `overview.rs`, `mcp.rs`, `milestoneStore.ts`):**
   - Updated `resolve_tracking_mode` to return `Result<String, ProjectMgmtError>` (rejecting unreadable `rula.toml` or invalid `tracking_mode` values) and `is_project_management_enabled` to log `WARN` on unreadable `rula.toml` or non-boolean `enabled` (`PA-M04`).
   - Wrapped `db_insert_project_milestone`, `db_update_project_milestone`, and `db_delete_project_milestone` in explicit SQLite transactions (`conn.transaction()?`) using `MilestoneScopedOutcome` to enforce atomic `project_id` ownership (`PA-M09`).
   - Added `project_path: String` to `UpdateProjectMilestoneArgs` and `DeleteProjectMilestoneArgs`, propagating across `api.rs`, `mcp.rs`, `src/types/generated/`, and `src/stores/milestoneStore.ts` (`PA-M10`).

4. **Specifications & Audit Tracker (`docs/specs/36_global_voice_controller.md`, `docs/audit/post_audit_2026-11-19_findings.md`):**
   - Documented `set_voice_controller_enabled`, config warnings, and strict voice tool validation in Spec 36.
   - Marked `PA-A01`–`PA-A04`, `PA-M01`–`PA-M05`, `PA-M09`, and `PA-M10` as `RESOLVED (WP-A)` in `post_audit_2026-11-19_findings.md`.

## Verification

- `./scripts/run_dev.sh export-bindings`:
  - `test result: ok. 403 passed; 0 failed; 0 ignored; 0 measured; 156 filtered out; finished in 0.28s`
  - Verified generated bindings: `SetVoiceControllerEnabledArgs.ts`, `UpdateProjectMilestoneArgs.ts`, `DeleteProjectMilestoneArgs.ts`.
- `./scripts/verify_rules.sh | grep -E "^\[|^==="`:
  - `[PASS] Zero TypeScript 'any' in src/`
  - `[PASS] Zero Result<_, String> in production Rust modules` *(resolved from 4 hits to 0)*
  - `[PASS] Zero backdrop-blur-* in components/views`
  - `[PASS] Zero hardcoded /home/butcherwutcher paths`
  - `[FAIL] Zero Tailwind palette classes` / `[FAIL] Zero raw bg-black/* or bg-[#...]` *(pre-existing `OscilloscopeView.vue` / modal items tracked for WP-B)*
- `./scripts/run_dev.sh status`:
  - `daemon` (`active/running`, port `8765`), `daemon_watch` (`active/running`), `vite` (`active/running`), `gui` (`active/running`).
- `journalctl --user -u oneship-dev-daemon-build -n 20 --no-pager`:
  - `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 49.24s` (`oneship-dev-daemon-build.service` completed cleanly).

## Next Steps
- Present Technical Proposal for **WP-B** (`OscilloscopeView.vue` tokens, `open_system_terminal` dead code, `migrate_legacy_node_path`, `glab`/`git`/`mpremote` timeouts, `ProjectElectronicsTab.vue` `bg-[#09090b]`, `ProjectGitSnapshotsTab.vue` staged banner, `ProjectConfigPanel.vue` catches, `semantic/` improvements) upon maintainer go-ahead.
