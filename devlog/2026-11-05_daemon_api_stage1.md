# 2026-11-05 — Daemon Ownership Migration, Stage 1 (Typed Operator RPC API)

First stage of the daemon ownership migration (`TODO.md`): the daemon becomes the only supervisor and state owner, and the app becomes an interface.

## Approved design
- **RPC per command:** `POST /api/v1/call/<command>` with JSON arguments.
- **Errors:** `{"error": {"code", "message"}}`, with the HTTP status taken from the `ApiError` trait.
- **Operator auth hardened** with an app-descendant check.
- **SSE:** one stream with a sequence `id:` and a `resync` event.
- **TS types** generated with `ts-rs`.
- **Pilot:** `diagnostic` and `github`.

## Backend
- **`api.rs` (new):**
  - `ApiError` trait (`status()`, `code()`), `ApiErrorResponse` / `ApiErrorBody`, and `ApiCallError` (`UnknownCommand` 404, `InvalidArguments` 400, `Serialize` 500);
  - `NoArgs`, `API_COMMANDS`, `dispatch` and `api_router()`.
  - The body is parsed by the handler so a malformed body also gets a typed 400.
  - Failures are logged at WARN (4xx) or ERROR (5xx).
- **`mcp.rs`:**
  - `api_router()` is merged into the operator API behind `require_operator_caller`.
  - **Check 3:** an `UnmanagedCaller` must also descend from the app binary (`require_app_caller` → `caller_identity::require_app_descendant`). This closes `env -u ONESHIP_AGENT_INSTANCE curl -H 'Origin: tauri://localhost'`.
  - **SSE:** each event gets a per-connection sequence `id:`. `BroadcastStreamRecvError::Lagged` used to be dropped silently; it now emits `resync` with `missed_count`, logged at WARN.
- **`caller_identity.rs`:**
  - `NotFromApp` error;
  - `APP_BINARY_NAME`;
  - `operator_app_exe()`, which returns `one-ship` next to the daemon executable;
  - `read_process_exe()`, which strips ` (deleted)` after a rebuild;
  - `require_app_descendant()`, bounded by `supervisor.identity_ancestor_walk_max_depth_count`.
- **Pilot:**
  - `run_diagnostics`, `install_missing_tools`, `search_github_repos`, `link_github_remote` and `create_github_repo` lose `#[tauri::command]` and their `lib.rs` registration; they are served only by the daemon.
  - Argument structs (`InstallMissingToolsArgs`, `LinkGithubRemoteArgs`, `CreateGithubRepoArgs`) live in their modules, `camelCase` with `deny_unknown_fields`.
  - `ApiError` is implemented for `DiagnosticError`, `GithubError` and `RegistryError`; `GithubError::Registry` delegates to the registry error.
  - The `Serialize` impls of `DiagnosticError` and `GithubError` are gone, since Tauri no longer returns them.
- **`ts-rs` 11.1:**
  - `#[ts(export)]` on the API, args and result types;
  - `src-tauri/.cargo/config.toml` sets `TS_RS_EXPORT_DIR` to `src/types/generated/`, and `cargo test` writes 8 files.

## Frontend
- `src/types/daemonApi.ts`: the `DaemonCommands` map (args and result per command) over the generated types.
- `src/utils/daemonClient.ts`:
  - `daemonCall(command, args)`, typed by `DaemonCommands`;
  - `DaemonCallError` (status, code, message; `name` = code) and `DaemonUnreachableError`;
  - `daemonBaseUrl()`, which reads `get_daemon_port_cmd` once.
- `coreStore`:
  - uses `daemonBaseUrl()`, so there is a single source for the daemon URL;
  - handles `resync` with `resyncFromDaemon()`, which reloads projects and pending approvals.
- `App.vue` (diagnostics) and `GithubRemoteModal.vue` call `daemonCall`. The modal uses the generated `GithubRepo`, and `types/index.ts` re-exports the generated `DiagnosticState`.

## Docs
- Spec 12:
  - §7: SSE sequence and `resync`;
  - §7.1: check 3;
  - §7.2 (new): the operator RPC API.
- Spec 11: the pilot commands as daemon commands with their status mapping.
- `AGENTS.md` §9.6: rewritten as the daemon command registration invariant (`api.rs` + `daemonApi.ts` + `ts-rs`; no new `#[tauri::command]`; a migrated command is removed from Tauri in the same cycle).

## Known risk (runtime)
- Check 3 assumes WebKitGTK's network process, which issues the webview's `fetch`/`EventSource`, is a child of the app process. If it is not, every operator call returns 403 `NotFromApp`. This is the first item of the runtime check.
- The pilot commands now run in the daemon's environment, which needs the session D-Bus, a Polkit agent and the `gh` credentials.

## Runtime follow-up (same day)
The user started the app; no `OPERATOR API REJECTION` was logged for the webview, so its startup `run_diagnostics` passed check 3. That confirms WebKit's network process descends from the app.

A forged `curl -H 'Origin: tauri://localhost'` from a terminal got 403, but for the wrong reason: `Failed to read process exe: Permission denied`. The ancestor walk hit a process whose `/proc/<pid>/exe` is not readable (the compositor, which has capabilities) and failed as an I/O error.

`read_process_exe` now returns `None` on `EACCES` and the walk continues, since such a process is never the app; the rejection is `NotFromApp`. A new test covers acceptance from the app's own exe and `NotFromApp` across unreadable ancestors.

Also observed: unit tests that log (e.g. the `api` dispatch tests) write into the user's real `~/.config/one-ship/logs/oneship.log` and `system_logs`, recorded in `TODO.md`.

## Verification
- `cargo check --all-targets` clean.
- `clippy`: 17 warnings, none in the new code.
- `cargo test --lib`: 116 passed. New: the frontend-map sync, unknown command / bad and extra arguments, domain validation → 400, plus the ts-rs export tests. The same 2 `kopeng::resolution` failures remain.
- `vue-tsc` clean.
- `Result<…, String>`: 52.
- Not run in the app.
