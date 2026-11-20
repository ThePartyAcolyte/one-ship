# 2026-11-06 — Daemon Ownership Migration, Stages 3 and 4

Follow-up of `2026-11-06_daemon_ownership_stage2.md` and `2026-11-06_interface_lifecycle_bar_widget.md`. With it the migration is complete: the daemon serves every domain command and the app is a window.

## Decisions (user, 2026-11-06)
- **Desktop commands go to the daemon.** This covers the theme, opening a file in the external editor and the Wayland clipboard paste. `open_system_terminal` followed for consistency, as reported to the user.
- **The profiler commands move now**, unchanged except for typed errors, after the conflict between "wait for the redesign" and "the app stops opening SQLite" was raised. The redesign reworks them inside the daemon.
- **Stage 4 in the same delivery:** the app opens no database and validates no configuration.
- **`get_agent_runner_schema` is deleted**, since nothing called it.

## Backend
- **66 new daemon commands, 147 in total.** Every one has a `camelCase`, `deny_unknown_fields` args struct, ts-rs result types and a domain error implementing `api::ApiError`.
- **New `ApiError` impls:** `LoggerError`, `TelemetryError`, `MigrationError`, `SkillError`, `DocumentsError`, `GitError`, `FsTreeError`, `ManifestError`, `SnapshotError`, `DesktopError`, and the new `ProfilerError`. Their `impl Serialize` and `From<…> for String` were Tauri-only and are deleted.
- **0 `Result<…, String>` left.** Phase 5 is closed.
- **Command renames:**
  - `_cmd` suffixes dropped: `get_snapshot_diff`, `toggle_project_skill`, `remove_project_skill`, `get_project_skill_content`, `install_project_skill`, `create_document_comment`, `list_document_comments`, `resolve_document_comment`, `delete_document_comment`, `get_continuous_profile`, `get_profiler_runs`, `compare_profiler_runs`.
  - `inject_specialized_skill` and `cleanup_ephemeral_skill` lose their Tauri registration; only the backend calls them.
- **Arguments made explicit:**
  - Limits are required `limitCount`: `get_task_executions`, `get_system_logs`, `list_project_snapshots`, `search_marketplace_skills`.
  - `get_system_logs` loses its `offset`, which only the removed command passed, always as 0. Paging is by cursor.
  - `remove_project` takes a required `purgeTraces`.
- **New shared `api::ProjectIdArgs`.**
- **`ProjectInitFlagRecord.id` is `i64`.** Inserts take the new `NewInitFlag`, which `db_insert_init_flag` stores `pending` with the current time. Every reader had to handle a `None` id that a stored row never has.
- **Telemetry:**
  - `TelemetryRole` is gone, along with the app's HTTP query to the daemon (`fetch_continuous_profile_from_daemon`), the `/api/telemetry/continuous-profile` route, the `mcp.daemon_request_timeout_ms` key, the client-only error variants and the `ureq` dependency.
  - `get_continuous_profile` reads DuckDB in the daemon.
- **Desktop windows outlive the daemon.**
  - `desktop::desktop_command` starts terminal and editor windows with `systemd-run --user --scope`, outside the daemon's cgroup. It is used by `open_system_terminal`, `open_file_in_external_editor` (now `xdg-open` instead of `tauri_plugin_opener::open_path`) and the `initiate_add_account` terminal.
  - Before this, with the daemon under a systemd unit with `KillMode=mixed` (previous devlog), every daemon restart would have killed those windows.
  - `desktop::spawn_detached` reaps the child, so a detached window no longer stays a zombie of the daemon.
- **App (`lib.rs`):**
  - `generate_handler!` holds only `get_build_id_cmd` and `get_daemon_port_cmd`.
  - `setup` only switches the logger to the file sink (`logger::use_file_sink_only`) and logs its start.
  - No `init_database`, `validate_global_config`, telemetry role or event channel.
  - The capability `opener:allow-open-path` is removed; the webview uses only `openUrl`.

## Frontend
- **Call sites:** about 150 `invoke` calls in 30 files are now `daemonCall`. `invoke` remains only in `daemonClient.ts` (`get_daemon_port_cmd`) and `coreStore.checkDaemonBuild` (`get_build_id_cmd`).
- **No Tauri `listen` left.** `projects-updated` and `init-flag-*` arrive only over SSE.
- **Hand-written types replaced by generated ones:**
  - in `types/index.ts`: `ProjectRecord`, now `HydratedProject`; `ProjectInitFlagRecord`; the skill types; and the four profiler types;
  - the documents, comments, git, snapshot, system log, settings schema and OpenDictate types.

  The hand-written `ContinuousProfileSummary` had `service_id` where the daemon sends `target_id`.
- **Recursive `JsonValue` in store state.** It appears in script options and schema defaults, and made `vue-tsc` fail with TS2589 in Pinia's type unwrapping. Fixed as follows:
  - `projects` is typed `Raw<ProjectRecord>[]` in `coreStore` and `useProjectStore`;
  - Settings uses a `shallowRef`;
  - the new `coreStore.projectByPath` has an explicit return type.
- **Defects fixed on the way:**
  - **Voice release:** `ArtifactPanel` and `ProjectWorkspace` called `cancel_voice_capture` without a session id, so Tauri rejected every call and the OpenDictate reservation was never released from those panels. Both now release the session id returned by the reservation.
  - **Undo limit:** the Visual Builder read its undo limit as `cfg.plugins.snapshots…`, but `get_effective_project_config` returns flat `<plugin>.<key>` keys, so it always used the default.

## Docs
- **Spec 12:** the event bus (no app channel), §7.1 (no telemetry route), §7.2 (stage 3 list, desktop windows, the two Tauri commands, the session environment).
- **Spec 25:** §3.3 ownership without roles.
- **Spec 10:** startup validation only in the daemon; the removed key.
- **Spec 06, spec 08 and spec 16:** command registration and names.
- **Spec 20:** the daemon commands.
- **AGENTS.md §9.6:** the last bullet now describes the final state.
- **`TODO.md`:**
  - a stage 3 runtime check;
  - Phase 5 and the migration item closed;
  - a separate event payload item;
  - the profiler item updated;
  - the frontend findings added to Phase 6, and the theme fallbacks to Phase 7.

## Verification
- `cargo check --all-targets` clean.
- `cargo test --lib`: 302 passed, including `api_commands_match_the_frontend_map` with 147 commands.
- `clippy` lib: 11 warnings, unchanged.
- `vue-tsc --noEmit` clean.
- Not run in the app.

- First runtime pass (user): the app starts after the restart; Settings and the system log viewer open.

## Cleanup
- `src/composables/useTauriCommand.ts` is deleted (user decision). Nothing imported it, and it wrapped `invoke` for arbitrary commands that no longer exist.
