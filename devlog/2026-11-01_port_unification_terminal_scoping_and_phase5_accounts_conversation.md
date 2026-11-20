# 2026-11-01 — Port Unification, Terminal Input Scoping, Phase 5 Modules 3–4 (`accounts.rs`, `conversation.rs`)

Follow-up of `2026-11-01_daemon_required_subsystem_and_phase5_supervisor_errors.md`.

## Single MCP port source
- `config::mcp_daemon_port()` (`mcp.mcp_daemon_port`, default `DEFAULT_MCP_DAEMON_PORT = 8765`) is the only reader: daemon bind, app daemon client, `get_daemon_port_cmd`, REA gateway URLs, `opencode.json`/`.mcp.json` provisioning (`skill.rs`, `supervisor.rs`) and the global `agy` config sync (`diagnostic.rs`, `save_global_config`).
- `system.mcp_port` removed from the schema and from the default `config.toml` template. `save_global_config` logs propagation failures instead of discarding them.

## Terminal input scoping
- `send_terminal_input(project_path, process_key, input)`: the project is mandatory and the key must be a canonical key of that project. The "contains `:` → already a key" heuristic and the bare-key path are gone. `ProjectTerminalGrid.vue` sends `projectPath`/`processKey`. Spec 08 command table updated.

## Phase 5 — `kopeng/accounts.rs` (`AccountsError`)
- Variants: `InstallationIdUnavailable`, `Crypto`, `InvalidPayload`, `Keyring`, `AccountNotFound`, `InvalidRequest`, `UnsupportedProvider`, `Io`, `Remote`, `Database`. Every internal function returns it; the 8 Tauri commands keep `String`.
- Fallbacks removed while converting:
  - key derivation no longer falls back to `/etc/machine-id` (a different input would make every stored token undecryptable; `~/.gemini/installation_id` exists);
  - `get_active_deepseek_key` reads only the active DeepSeek account in SQLite (no `DEEPSEEK_API_KEY`, no `auth.json` lookup; `auth.json` keys are still imported once as an account by `list_accounts`);
  - `sync_opencode_auth_json` fails on an unreadable or non-object `auth.json` instead of overwriting it (which erased other providers' keys);
  - `read_keyring_token` returns `Ok(None)` only when nothing is stored; a `secret-tool` failure is an error (callers no longer use `.ok()`);
  - `switch_account` rejects unknown providers, logs stop/relaunch failures and skips (with an ERROR) agents whose project is no longer registered instead of inventing a project name.
- `SupervisorError::AccountUnavailable` now carries the typed `AccountsError` message.

## Phase 5 — `conversation.rs` (`ConversationError`)
- Variants: `ProjectNotRegistered`, `AgentNotDeclared`, `Brief`, `Database`. `resolve_project_id`, `has_recent_db_response`, `apply_conversation_config` and `reopen_conversation` return it; commands convert at the boundary.

## Verification
`cargo check`/`clippy` clean (35 pre-existing style warnings), 24 unit tests passed (accounts, db, conversation, logger, duck, supervisor), `vue-tsc` clean. `Result<_, String>` in the backend: 158 (245 at the audit). Runtime checks pending: TODO "Audit 2026-11-01 follow-up".

## Specs updated
08 (terminal input), 10 (`system.mcp_port` removed), 12 (port source), 17 (DeepSeek key source).
