# 2026-11-01 — Full Audit and Remediation Phases 1–3

## Audit
Read-only audit of the whole repository (~70.6k lines: Rust 26k, Vue/TS 44k), 30 specs, TODO, debt registers and the 2026-09-23 audit. Tools: `cargo clippy --all-targets` (0 errors, 51 style warnings), `vue-tsc --noEmit` (clean), IPC cross-check (171/171 commands registered, 0 frontend calls to missing commands), read-only queries on `oneship.db`.

Key measurements:
- `system_logs`: 5.59 M rows in ~32 h (99.7 % DEBUG). Top sources: `db_get_projects` and `SqliteProjectRepository` (~19 calls per watcher tick), skill template reads from disk, per-project git status.
- `oneship.db`: 3.9 GB with 644,544 free pages (~2.6 GB); pruning ran only at startup, no vacuum.
- Every log line resolved two config keys; each resolution read and parsed `config.toml`. The logger and `fs_tree` asked plugin `general`, which is not registered, so the user's `[system] log_max_size_bytes`, `log_max_rotations_count`, `fs_tree_max_depth_count` and `log_level` were ignored.

Findings not addressed here are listed in `TODO.md` ("Audit 2026-11-01 follow-up").

## Phase 1 — Critical safety
- **C1 test isolation (F17):** `db.rs` test builds use a per-process database under the temp dir (`#[cfg(test)] get_db_path`). Removed the test fallback that adopted the first real project and deleted it on drop.
- **C2 operator API guard:** `CorsLayer::permissive()` replaced by `trusted_webview_cors()` (`http://localhost:1420`, `tauri://localhost`). `/api/*` routes go through `require_operator_caller`: trusted `Origin` required, and the caller process must resolve to `UnmanagedCaller` (managed agents are rejected even with a forged header). Spec 12 §7.1.

## Phase 2 — Performance and retention
- **C3 logger (`logger.rs` rewritten):** `system.log_level` gates both sinks before any I/O; settings cached by `config.toml` mtime; `oneship.log` handle kept open with size-tracked rotation; plugin id fixed to `system`. The SQLite worker returns errors to the file sink instead of swallowing them, and prunes `system_logs` every `log_db_prune_interval_seconds` (300) to `log_db_max_rows_count` (50000). `write_client_log` rejects unknown levels.
- **C3 database:** `db_prune_system_logs` (PK-offset delete + `PRAGMA incremental_vacuum`); `init_database` prunes, then switches once to `auto_vacuum = INCREMENTAL` with `VACUUM` (maintenance failures logged, retried next start). `db_insert_logs_batch` returns `Result`.
- **Config cache:** `load_global_config` caches the parsed file by mtime (all `resolve_effective_*` callers benefit).
- **C4 watcher:** each tick reads the registry (`spawn_blocking`) and file mtimes only; full hydration runs on the blocking pool and only when a change is detected. Keyring sync moved to `spawn_blocking`.
- **H7:** `lines().flatten()` replaced by `map_while(Result::ok)` in `snapshots.rs` and `stats.rs`.
- Schema: `log_db_max_rows_count`, `log_db_prune_interval_seconds` registered under `system` (SettingsModal renders them from the schema). Specs 09 §3–3.1 and 10 §1.1 updated; AGENTS.md §12 notes that DEBUG is emitted only at `log_level = "DEBUG"`.

## Phase 3 — Cleanup
- Deleted IPC commands with no frontend caller and no spec reference: `cancel_project_initialization`, `check_onlook_installed`, `download_onlook_binary`, `launch_onlook_studio`, `focus_service_tab`, `get_agent_conversations_cmd`, `get_conversation_turns_cmd`, `switch_active_conversation_cmd`, `get_project_audit_trail`, `get_resolution_prompt_template`, `get_services_telemetry_cmd`, `get_visual_builder_history`, `switch_agent_account_cmd`, plus the orphans `switch_agent_account`, `db_list_vb_snapshots`, `get_onlook_bin_path`. Spec-referenced unused commands are kept and listed in TODO.
- Removed `kapawu` compatibility shims: legacy skill purge/normalization, `kapawu_version`, `WANG_KAPAWU_MANAGED` block handling, `.wang-kapawu` migration, `wang-kapawu` MCP entries, legacy plugin-dir migration, `KAPAWU_VERSION` template var, `utils/legacyStorage.ts` call, legacy localStorage fallbacks. Renamed `manifest.rs` constants/functions to `ONESHIP_*` / `*_oneship_artifacts`, window events to `oneship-*`, localStorage keys to `oneship:*`, OpenDictate session ids/app names, `KAPAWU_STATUSLINE_FILE` → `ONESHIP_STATUSLINE_FILE`. Data-bound identifiers recorded as TECHNICAL_DEBT #25.
- README spec index rewritten (broken Herder links removed, specs 16–30 added). `package.json`, `package-lock.json`, `tauri.conf.json` versions aligned to `0.2.1`. TECHNICAL_DEBT #24 (resolved 2026-10-09) removed from the register.

## Verification
- `cargo clippy --all-targets`: no new warnings. `cargo test --lib db::tests logger::tests`: 9 passed (temp database; real `oneship.db` size unchanged). `vue-tsc --noEmit`: clean.
- Runtime checks pending (maintainer-run): see TODO "Audit 2026-11-01 follow-up".

## Files to delete (maintainer, no delete tool available to the agent)
`src/utils/legacyStorage.ts`, `templates/skills/kapawu-*` (14 dirs), `patch_mcp_tools.sh`, `patch_project_card.sh`, `RECOVERY_NOTES.md`, `batch`, `project-tracker.toml`.
