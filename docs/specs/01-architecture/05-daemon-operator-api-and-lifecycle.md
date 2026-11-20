# Daemon Operator API and Lifecycle

## Purpose

Define the strict separation between `oneship-daemon` (the sole supervisor, database owner, background-loop host, and state authority) and `one-ship` (a stateless Tauri/WebKitGTK UI window), covering the Server-Sent Events (SSE) bus, fail-closed operator API security guard, typed HTTP RPC contract, systemd development launcher, Omarchy bar widget, and WebKitGTK graphics renderer policy.

## Scope

- Real-time SSE telemetry bus (`GET /api/events`), build-identity verification (`GET /api/build`), and lag `resync` semantics.
- Mandatory daemon availability (`coreStore.daemonStatus`) with zero in-process fallback.
- Fail-closed `/api/*` operator caller guard (`require_operator_caller`) and CORS policy.
- Typed operator RPC API (`POST /api/v1/call/<command>`), `ApiError` / `ApiCallError`, `API_COMMANDS`, `ts-rs` TypeScript binding generation, and desktop window cgroup detachment.
- Interface lifecycle, `scripts/run_dev.sh` systemd user units and hot reload, and the auto-adaptive Omarchy bar widget.
- Linux WebKitGTK renderer transport modes (`[system] webkit_renderer_mode`), `auto` resolution ladder, and packaging constraints.

The MCP JSON-RPC protocol and `/proc` caller attestation for managed agents are specified in [Streamable HTTP MCP Daemon](04-streamable-http-mcp-daemon.md).

## Model / Design

### The daemon as a required subsystem

`oneship-daemon` is the only process supervisor, background-loop owner, and MCP server. The desktop app (`one-ship`) never starts an in-process MCP server, opens no database, validates no configuration, supervises no processes, and logs solely to `oneship.log` (`logger::use_file_sink_only`; webview logs forward via `write_client_log`). A missing daemon is treated explicitly as an error state, never substituted.

`coreStore.daemonStatus` transitions across `connecting`, `connected`, `unreachable`, and `build_mismatch`. The `unreachable` and `build_mismatch` states render a red header indicator (`ServerCrash` with tooltip detail) and an `ERROR` status-log entry while the SSE client probes every 3 s until the daemon recovers.

### Server-Sent Events (SSE) bus and real-time telemetry

The daemon exposes an HTTP SSE stream at `GET /api/events` via `sse_events_handler`, backed by a Tokio broadcast channel (`broadcast::Sender<DaemonEvent>`):

- **Single event bus:** the daemon hosts every background loop (project and keyring watchers, host and service telemetry, continuous profiler, agent state watcher, quota watcher, native dictation, REA idle reaper, rearm worker) and the process supervisor. All real-time events exist exclusively on `/api/events`; the Tauri app exposes no event channel and the frontend uses no Tauri `listen`.
- **Sequence and `resync`:** every SSE frame carries a per-connection sequence `id:`. If a slow subscriber overflows the broadcast buffer (`mcp.event_channel_capacity_count`, default `1024`), skipped events cannot be replayed: the daemon logs `WARN` and emits a `resync` event (`{"missed_count": n}`), triggering `coreStore.resyncFromDaemon()` to reload projects and pending approvals. Lag is never silently dropped.
- **Build identity check:** `GET /api/build` and MCP `initialize.serverInfo.build_id` return `BUILD_ID` (epoch seconds recorded by `build.rs`, refreshed whenever `src/` changes). On every SSE connect, `coreStore.checkDaemonBuild()` compares the daemon's `build_id` against the app's `get_build_id_cmd`. Mismatch or absence is logged as `ERROR` because a long-running daemon on an older binary would execute stale SQL against a migrated database. Additionally, `db::init_database` refuses any SQLite file whose `PRAGMA user_version` is newer than the binary supports.
- **Frontend integration:** `daemonClient.daemonBaseUrl()` reads `mcp.mcp_daemon_port` once via `get_daemon_port_cmd`. `coreStore.initDaemonSse()` owns the `/api/events` `EventSource` and binds it to `src/utils/daemonEvents.ts`, where stores and components subscribe via `onDaemonEvent(name, handler)` / `listenDaemonEvent` and unsubscribe on teardown. Pending approvals are hydrated exclusively from SQLite (`get_pending_approvals`).

### Fail-closed operator API guard (`/api/*`)

`/api/events`, `/api/approval-response`, `/api/v1/voice/stream`, `/api/build`, and `/api/v1/call/<command>` serve only the ONE-SHIP desktop webview. The `require_operator_caller` middleware layer applies three fail-closed checks (rejecting any failure with HTTP `403 Forbidden` and a `WARN [OPERATOR API REJECTION]` log):

1. **Trusted `Origin`:** the request must carry an `Origin` header present in `TRUSTED_WEBVIEW_ORIGINS` (`http://localhost:1420` for Vite dev; `tauri://localhost` for the bundled Linux webview). Browsers prevent pages from forging `Origin`, and `Origin`-less CLI tools (`curl`, scripts) are rejected.
2. **Not a managed agent:** `caller_identity::resolve_caller` must return `Err(CallerError::UnmanagedCaller { pid })`. Managed agents (or children inheriting `ONESHIP_AGENT_INSTANCE`) are rejected even with a forged `Origin`, preventing an agent from approving its own pending gate. Any other resolution error rejects.
3. **ONE-SHIP app descendant:** `caller_identity::require_app_descendant` walks `/proc/<pid>/exe` upward (at most `supervisor.identity_ancestor_walk_max_depth_count` steps) and verifies descent from the `one-ship` executable co-located in the daemon's binary directory (`caller_identity::operator_app_exe()`). Replaced binaries reading `<path> (deleted)` still match; unreadable ancestors (`EACCES`, such as the compositor or another user's process) are skipped during the walk. WebKitGTK's network process descends from `one-ship`, whereas an agent stripping `ONESHIP_AGENT_INSTANCE` (`env -u ... curl -H 'Origin: tauri://localhost'`) fails this check.

The daemon CORS layer (`trusted_webview_cors`) permits only `TRUSTED_WEBVIEW_ORIGINS`, methods `GET` and `POST`, and header `Content-Type`. MCP runner endpoints (`/mcp`, `/mcp/rea`, `/mcp/desigma`, `/mcp/pcbcraft`) remain unaffected and rely on per-tool process attestation.

### Operator RPC API (`POST /api/v1/call/<command>`)

All domain state mutations and queries execute in `oneship-daemon` over HTTP RPC:

- **Request:** `POST /api/v1/call/<command>` with a JSON object of arguments (`{}` when empty). Argument structs are owned by their domain module, serialized in `camelCase`, and marked `#[serde(deny_unknown_fields)]`.
- **Success:** HTTP `200 OK` with the command's JSON payload (`null` for void results).
- **Failure:** structured `ApiErrorResponse` (`{"error": {"code": "<Enum>::<Variant>", "message": "..."}}`). Every domain error implements `api::ApiError` (`status()`, `code()`):
  - `4xx` for caller-correctable inputs (`400` invalid input, `404` not found, `409` conflict, `424` missing dependency), logged at `WARN`.
  - `502` for failing external tools, `504` for a dev server failing to boot, `500` for internal daemon failures, logged at `ERROR`.
  - Wrapped domain errors preserve their inner status and code (e.g., `SupervisorError` inside `OrchestratorError` answers `SupervisorError::PortOccupied`).
  - RPC-layer errors use `ApiCallError`: `UnknownCommand` (`404`), `InvalidArguments` (`400`, including malformed JSON bodies), `Serialize` (`500`), and `Task` (`500`, blocking-pool task failure).
- **Execution model:** `call!` awaits async handlers; `blocking!` executes synchronous SQLite, `/proc`, or filesystem handlers on Tokio's blocking pool.
- **Registry and TypeScript sync:** `api::API_COMMANDS` and `api::dispatch` (`src-tauri/src/api.rs`) are mirrored in `src/types/daemonApi.ts` (`DaemonCommands`), enforced by the unit test `api_commands_match_the_frontend_map`. Argument, result, and error structs derive `ts_rs::TS` with `#[ts(export)]`, emitting generated types into `src/types/generated/` (`TS_RS_EXPORT_DIR` in `src-tauri/.cargo/config.toml`).
- **Frontend client (`src/utils/daemonClient.ts`):** `daemonCall(command, args)` rejects with `DaemonCallError` (`status`, `code`, `message`, `name = code`) or `DaemonUnreachableError` (connection failure or non-JSON response). There is no fallback to Tauri IPC.
- **Tauri bootstrap surface (`src-tauri/src/lib.rs`):** the desktop app registers only `get_build_id_cmd` and `get_daemon_port_cmd`, plus `tauri_plugin_dialog` and `tauri_plugin_opener`.
- **Daemon session environment and detached desktop windows:** the daemon inherits the user session environment (`DBUS_SESSION_BUS_ADDRESS` for the Secret Service keyring, `WAYLAND_DISPLAY` for windows and `wl-paste`, Polkit agent for `pkexec`, `gh`/`glab` credentials, `SHELL` for interactive shells). Desktop windows launched by the daemon (`open_file_in_external_editor` via `xdg-open`, `initiate_add_account` terminals) spawn through `desktop::desktop_command` using `systemd-run --user --scope` (falling back to a detached Unix `process_group(0)` when `systemd-run` or user D-Bus is absent) and are reaped by `desktop::spawn_detached` so they outlive the daemon unit without becoming zombies.

### Interface lifecycle and development launcher

- **Window closure and desktop tray lifecycle:**
  - On **Hyprland / Omarchy** (`desktop::is_hyprland_session() == true`), closing the `one-ship` window terminates the interface process immediately with no hidden background window (`integrations/omarchy-bar/` manages the daemon).
  - On **floating Linux desktops** (`!desktop::is_hyprland_session()`), `one-ship` provides CSD drag/resize/Min/Max/Close controls and a resident AppIndicator System Tray (`tray-icon`): closing the window hides it to the tray (`Open ONE-SHIP`, `Close Interface`, `Restart Daemon`, `Quit (Stop Daemon)`), while **Quit (Stop Daemon)** stops `oneship-daemon.service` and exits.
  - In all environments, all supervised processes and state live in `oneship-daemon`, and reopening the interface reconnects the webview.
- **Daemon shutdown and startup:** on boot, only the daemon runs `caller_identity::close_stale_agent_instances()` and `supervisor::reconcile_orphan_processes()`. On `SIGINT`/`SIGTERM`, the daemon executes `supervisor::stop_all_processes()`, cancels its `CancellationToken` (which immediately closes all open `/api/events` SSE streams via `take_until` so Axum exits cleanly), and removes `$XDG_RUNTIME_DIR/oneship-status.json`.
- **Development launcher (`scripts/run_dev.sh`):** runs each component as a transient systemd user unit (`systemd-run --user` from the project root with the user manager's `WAYLAND_DISPLAY`, `DBUS_SESSION_BUS_ADDRESS`, `HYPRLAND_INSTANCE_SIGNATURE`, and caller `PATH`):

| Unit | Runs | Notes |
|---|---|---|
| `oneship-dev-daemon` | `target/debug/oneship-daemon`, `RUST_LOG=debug` | `KillMode=mixed`, `TimeoutStopSec=30`: `SIGTERM` targets the daemon to clean up supervised processes; remaining cgroup processes receive `SIGKILL` after timeout. |
| `oneship-dev-daemon-build` | `cargo build --no-default-features --bin oneship-daemon --bin one-ship` + `cargo test --lib export_bindings` (oneshot) | Serializes compilation of both binaries and `ts-rs` bindings (`src/types/generated/`) to prevent `target/` lock contention. |
| `oneship-dev-daemon-watch` | `watchexec` on `src-tauri/src`, `src-tauri/Cargo.toml`, `src-tauri/build.rs` (`.rs`, `.toml`, 500 ms debounce, queued) | Triggers `run_dev.sh rebuild-daemon`; restarts daemon and relaunches `oneship-dev-gui` (if active) only on build success. |
| `oneship-dev-vite` | `npm run dev` | Vite HMR server for the frontend. |
| `oneship-dev-gui` | `npm run tauri dev -- --no-watch` | Launches UI against Vite and prebuilt `one-ship` without a competing Cargo watcher; closing the window ends the unit. |

| Subcommand | Effect |
|---|---|
| `all` (default) | Stops all dev units, starts Vite, builds both binaries + `ts-rs` bindings, starts daemon and watcher, and starts the GUI. |
| `daemon` | Builds both binaries + `ts-rs` bindings, restarts the daemon, starts the watcher. |
| `gui` | Focuses an existing interface window (`hyprctl dispatch focuswindow pid:<pid>`), or ensures Vite, any active build, and the daemon are ready before starting the GUI unit. |
| `stop daemon\|gui\|all` | Stops the selected `oneship-dev-*` units via `systemctl --user stop`. |
| `status` | Emits one JSON line containing `units.{daemon, daemon_build, daemon_watch, vite, gui}` (`active_state`, `sub_state`, `result`, `main_pid`, plus last 15 journal lines for failed units), `daemon_listening_ports`, `vite_listening_ports`, `gui_window_pid`, and `build_progress` (from `$XDG_RUNTIME_DIR/oneship-dev-build.json`). |
| `logs <daemon\|build\|watch\|vite\|gui>` | Opens a terminal tailing `journalctl --user -u <unit> -f`. |
| `rebuild-daemon` | Invoked by `oneship-dev-daemon-watch` on Rust changes. |
| `export-bindings` | Exports `ts-rs` TypeScript bindings on demand. |

- **Omarchy bar widget (`integrations/omarchy-bar/`, plugin id `tech.kirulab.one-ship`):** unified auto-adaptive Quickshell plugin reading `$XDG_RUNTIME_DIR/oneship-status.json` (working/idle/error counts, pending approvals, running services, active agents) without touching the guarded operator API. In development mode (symlinked from a checkout with `../../scripts/run_dev.sh` and active `oneship-dev-*` units or absent `oneship-daemon.service`), it also polls `run_dev.sh status` (`pollIntervalMs`, default `2000`) to display build progress and unit controls; in production mode, it manages `oneship-daemon.service` and launches/focuses `one-ship-launch`/`one-ship`. After editing the widget, `omarchy-restart-shell` is required because Quickshell's watcher does not reload symlink targets.

### WebKitGTK graphics renderer mode and packaging policy

The desktop interface renders via WebKitGTK. CSS `backdrop-filter` (see [Design System Foundations](../07-ui/01-design-system-foundations.md)) requires accelerated compositing, making the renderer buffer transport a user-visible capability rather than an internal detail.

#### Modes and environment mapping

WebKitGTK reads these environment variables once per interface process, latched via `std::call_once` before the first backing store is created:

| `[system] webkit_renderer_mode` | Environment set by `main()` | Transport set | Accelerated compositing | Backdrop blur |
|---|---|---|---|---|
| `dmabuf` (default transport) | none | `{SharedMemory, Hardware}` | yes | yes |
| `shared_memory` | `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1` | `{SharedMemory}` | yes | yes |
| `software` | `WEBKIT_DISABLE_DMABUF_RENDERER=1` | empty | no | no |

- In WebKitGTK 2.54.1, `WEBKIT_DISABLE_DMABUF_RENDERER=1` empties `RendererBufferTransportMode` and disables accelerated compositing completely; it is a last resort, never the default for NVIDIA hardware.
- A value of `0` in either variable is treated as unset by WebKitGTK.

#### Pre-boot resolution, `auto` ladder, and restart semantics

1. `one-ship` resolves `[system] webkit_renderer_mode` (default `"auto"`) in `main()` before `tauri::Builder` initializes the webview, via a pre-boot typed read (`config::read_webkit_renderer_mode`).
2. **`auto` resolution ladder:**
   - `shared_memory` if `ONESHIP_WEBKIT_LAUNCHER_RETRY=1` is set (unless `software` is configured), when `/proc/driver/nvidia/version` exists (NVIDIA proprietary driver), or when `/dev/dri` cannot be read (e.g., sandboxed environments).
   - `dmabuf` otherwise (Intel/AMD Mesa with `/dev/dri` access).
   - `software` only when manually configured (or when GTK cannot initialize a GL context, accepting loss of backdrop blur and composited animations).
3. `lib.rs` logs the resolved mode and origin (`INFO`, or `WARN` on `launcher retry`) at setup. Changing `[system] webkit_renderer_mode` in Settings requires restarting the `one-ship` process; reloading the webview is insufficient, and `oneship-daemon` is unaffected.

#### Packaging constraints

- Installers may inspect hardware (`/proc/driver/nvidia/version`, `lspci -d 10de:`, `/sys/module/nvidia`) to seed `config.toml`, but runtime resolution and manual override remain authoritative: Debian/Ubuntu ship `disable-nvidia-dmabuf.patch` (emptying the transport set on NVIDIA regardless of environment), whereas Arch does not.
- Packaged deployments use `packaging/bin/one-ship-launch`: if `one-ship` exits abnormally within 5 seconds of startup (and no WebKit renderer override env var is already set), the wrapper retries once with `ONESHIP_WEBKIT_LAUNCHER_RETRY=1 WEBKIT_DMABUF_RENDERER_FORCE_SHM=1`.

## Contracts

- **Daemon binary & app binary:** `oneship-daemon` (`src-tauri/src/bin/daemon.rs`), `one-ship` (`src-tauri/src/main.rs`).
- **Guarded operator routes (`src-tauri/src/mcp.rs`):** `POST /api/v1/call/<command>`, `POST /api/approval-response`, `GET /api/events`, `GET /api/v1/voice/stream`, `GET /api/build`.
- **Trusted origins (`TRUSTED_WEBVIEW_ORIGINS`):** `http://localhost:1420`, `tauri://localhost`.
- **Tauri bootstrap commands (`src-tauri/src/lib.rs`):** `get_build_id_cmd`, `get_daemon_port_cmd`.
- **Operator RPC commands (`api::API_COMMANDS` in `src-tauri/src/api.rs`, mirrored in `src/types/daemonApi.ts`):**
  - *Diagnostics & host capabilities:* `run_diagnostics`, `get_host_capabilities`, `install_missing_tools`.
  - *Remotes & agent pairs:* `search_github_repos`, `check_git_remote_access`, `clone_and_add_project`, `link_project_remote`, `create_github_repo`, `search_gitlab_repos`, `create_gitlab_repo`, `get_configured_agent_pair`.
  - *Services & tasks:* `start_service`, `stop_service`, `release_port`, `run_project_task`.
  - *Agents & terminals:* `get_project_agent_state`, `dispatch_git_instruction`, `launch_agent`, `restart_agent`, `stop_agent`, `interrupt_agent`, `prompt_agent`, `read_terminal_output`, `send_terminal_input`, `launch_interactive_shell`, `list_interactive_shells`.
  - *Personas, initialization & flags:* `create_agent_persona`, `update_agent_persona`, `delete_agent_persona`, `initialize_project_agent`, `create_greenfield_project`, `launch_flag_resolution_agent`, `launch_project_removal_agent`.
  - *Domain workstations (REA, Electronics, Desigma, SolidCraft, PCBCraft):* `check_rea_toolchain`, `get_rea_engine_status`, `get_rea_queue_snapshot`, `start_rea_engine`, `stop_rea_engine`, `toggle_project_rea_plugin`, `list_rea_targets`, `get_rea_visual_views`; `list_connected_mcus`, `scan_connected_instruments`, `list_supported_oscilloscopes`, `get_project_electronics_status`, `toggle_project_electronics_plugin`, `build_project_firmware`, `flash_project_firmware`, `capture_oscilloscope`, `capture_logic_analyzer`, `calibrate_oscilloscope`, `list_platformio_boards`, `scaffold_firmware_project`; `get_project_desigma_status`, `toggle_project_desigma_plugin`, `launch_project_desigma`, `render_desigma_preview`; `get_project_solidcraft_status`, `toggle_project_solidcraft_plugin`, `create_solidcraft_file`, `launch_project_solidcraft`, `render_solidcraft_preview`, `inspect_solidcraft_metrics`; `get_project_pcbcraft_status`, `toggle_project_pcbcraft_plugin`, `create_pcbcraft_project`, `launch_project_pcbcraft`, `inspect_pcbcraft_project`, `render_pcbcraft_svg_preview`.
  - *Conversations, RAG, minutes, TODOs, debt & approvals:* `get_active_conversation`, `create_agent_conversation`, `update_active_conversation_config`, `get_agent_turns_continuous`, `save_user_chat_turn`, `update_turn_approval`, `get_project_session_journals`, `build_conversation_brief`, `get_project_todos`, `create_project_todo`, `update_todo_status`, `get_project_technical_debt`, `record_technical_debt`, `resolve_technical_debt`, `reopen_conversation`, `search_conversations`, `search_knowledge`, `index_project_knowledge`, `get_indexing_status`, `get_project_knowledge_graph`, `rebuild_project_knowledge_graph`, `get_file_line_insights`, `get_file_line_range_content`, `get_file_symbols`, `resolve_symbol_definition`, `get_pending_approvals`.
  - *Accounts & quotas:* `list_accounts`, `switch_account`, `remove_account`, `rename_account`, `initiate_add_account`, `check_active_agent_sessions`, `get_deepseek_balance`, `save_deepseek_api_key`, `get_agent_metrics`, `get_session_metrics`, `get_account_quotas`, `toggle_account_warmup`, `kickstart_account_quota`, `get_context_threshold`.
  - *Projects, migration & logs:* `get_projects`, `add_project`, `remove_project`, `toggle_project_favorite`, `toggle_project_archived`, `toggle_project_tab_visibility`, `update_project_description`, `update_project_name`, `get_project_overview_summary`, `save_project_overview_layout`, `get_project_init_flags`, `resolve_project_init_flag`, `migrate_project_deterministic`, `rollback_project_init`, `get_task_executions`, `get_task_report`, `get_system_logs`, `get_system_log_sources`, `clear_system_logs`, `write_client_log`.
  - *Config, desktop, UI state & voice:* `get_plugin_schemas`, `get_global_config`, `get_scoped_project_config`, `save_global_config`, `save_project_rula_config`, `list_agent_runners`, `get_active_theme`, `get_gemini_api_key_status`, `set_gemini_api_key`, `clear_gemini_api_key`, `start_dictation`, `stop_dictation`, `cancel_dictation`, `get_dictation_status`, `set_dictation_auto_send`, `get_ui_registry`, `get_ui_state`, `dispatch_ui_action`, `get_voice_controller_status`, `set_voice_controller_enabled`, `test_voice_controller_voice`.
  - *Git, Project Management & Forge:* `init_git_repo`, `get_vcs_actions`, `save_vcs_action`, `get_project_git_log`, `get_git_commit_diff`, `get_commit_files_changed`, `get_working_tree_diff`, `get_working_tree_files`, `list_git_branches`, `switch_git_branch`, `create_git_branch`, `stage_git_files`, `unstage_git_files`, `discard_git_files`, `commit_git_changes`, `sync_git_branch`, `generate_commit_message`; `toggle_project_management_plugin`, `list_project_milestones`, `get_project_milestone_detail`, `create_project_milestone`, `update_project_milestone`, `delete_project_milestone`, `list_project_stages`, `create_project_stage`, `update_project_stage`, `reorder_project_stages`, `delete_project_stage`, `list_project_tasks`, `create_project_task`, `update_project_task`, `delete_project_task`, `create_task_subtask`, `toggle_task_subtask`, `delete_task_subtask`, `link_task_relation`, `unlink_task_relation`, `take_forge_issue`; `get_forge_context`, `configure_upstream_remote`, `sync_upstream_fetch`, `list_forge_issues`, `get_forge_issue`, `list_forge_pull_requests`, `get_forge_pull_request`, `list_forge_releases`, `create_forge_draft_pull_request`, `list_forge_inbox`, `mark_forge_item_read`, `set_forge_item_flag`, `get_forge_unread_counts`.
  - *Skills, documents, snapshots & profiler:* `inject_platform_skill`, `get_project_skills`, `toggle_project_skill`, `remove_project_skill`, `get_project_skill_content`, `search_marketplace_skills`, `install_project_skill`, `get_project_file_tree`, `get_project_documents`, `read_project_document`, `save_project_document`, `create_document_comment`, `list_document_comments`, `resolve_document_comment`, `delete_document_comment`, `create_project_directory`, `create_project_file`, `rename_project_item`, `delete_project_items`, `copy_project_items`, `move_project_items`, `save_clipboard_image`, `attach_file_to_project`, `paste_clipboard_image`, `open_file_in_external_editor`, `get_project_exclusion_status`, `toggle_project_exclusion`, `create_project_snapshot`, `list_project_snapshots`, `restore_project_snapshot`, `delete_project_snapshot`, `get_snapshot_diff`, `list_available_profilers`, `toggle_project_profiler_plugin`, `get_target_runs`, `get_run_profile`, `compare_runs`.
  *(Note: `get_effective_project_config` from early migration stages was superseded by `get_scoped_project_config`; `get_omarchy_colors` became `get_active_theme`; `read_process_output` is exposed via `read_terminal_output`.)*
- **SSE events (`events::DAEMON_EVENT_TYPES` in `src-tauri/src/events.rs`, mirrored in `src/utils/daemonEvents.ts`):** `AccountQuotaUpdated`, `AgentResponse`, `ApprovalRequested`, `DocumentCommentResolved`, `SessionMinutesRecorded`, `account-quotas-updated`, `account-switched`, `accounts-updated`, `agent-conversation-turn-added`, `agent-error`, `agent-states-updated`, `agent-status-report`, `agent-step-update`, `agent-usage-update`, `conversation-topic-switched`, `desigma-file-changed`, `dictation-cancelled`, `dictation-finished`, `dictation-interim-text`, `dictation-state`, `forge-inbox-updated`, `init-flag-raised`, `init-flag-resolved`, `keyring-warmup-status`, `new-account-detected`, `pcbcraft-file-changed`, `project-files-changed`, `project-milestones-updated`, `project-tasks-updated`, `projects-updated`, `rea-queue-updated`, `resync`, `running-services-updated`, `services-telemetry`, `solidcraft-file-changed`, `system-telemetry`, `task-completed`, `task-output-chunk`, `tech-debt-updated`, `todo-updated`, `ui-state-updated`, `ui-surface-command`, `voice-controller-state`.
- **Renderer config & runtime snapshots:** `[system] webkit_renderer_mode` (`"auto"` | `"dmabuf"` | `"shared_memory"` | `"software"`); `$XDG_RUNTIME_DIR/oneship-status.json`; `$XDG_RUNTIME_DIR/oneship-dev-build.json`.

## Implementation Status

| Area | Status | Notes |
|---|---|---|
| Daemon binary & background loops (`src-tauri/src/bin/daemon.rs`) | Implemented | Verified startup validation, DB/DuckDB init, stale instance close, orphan reconcile, and signal shutdown. |
| Operator guard & SSE bus (`mcp.rs`, `events.rs`, `daemonEvents.ts`) | Implemented | Verified `require_operator_caller`, `TRUSTED_WEBVIEW_ORIGINS`, `sse_events_handler`, and 46 `DAEMON_EVENT_TYPES`. |
| Operator RPC API (`api.rs`, `daemonClient.ts`) | Implemented | Verified `API_COMMANDS` (165 commands), `ApiError` / `ApiCallError`, `DaemonCallError`, `DaemonUnreachableError`. |
| WebKitGTK renderer mode & launcher retry (`main.rs`, `lib.rs`, `config.rs`, `one-ship-launch`) | Implemented | Verified `WebkitRendererMode`, `configure_webkit_renderer_mode`, and `packaging/bin/one-ship-launch`. |
| Development launcher & Omarchy bar widget (`scripts/run_dev.sh`, `integrations/omarchy-bar/`) | Implemented | Files present and active in workspace. |

## Related Specs

- [Configuration Schema and Cascade](02-configuration-schema-and-cascade.md)
- [Streamable HTTP MCP Daemon](04-streamable-http-mcp-daemon.md)
- [Linux Packaging and Host Capabilities](07-linux-packaging-and-host-capabilities.md)
- [Design System Foundations](../07-ui/01-design-system-foundations.md)

Supersedes legacy: `12_streamable_http_mcp_daemon.md` (§7)
