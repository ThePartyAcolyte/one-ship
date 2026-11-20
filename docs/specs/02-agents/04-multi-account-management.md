# Multi-Account Management

## Purpose

Define how ONE-SHIP discovers, encrypts, switches, attributes, and probes credentials across multiple AI providers (`google` Antigravity via OS keyring, `deepseek` via API key, and `claude` via CLI credentials) while maintaining per-process attribution, model-pool-aware quota rotation, and race-free keyring handshakes.

## Scope

In scope: `src-tauri/src/kopeng/accounts.rs`, keyring watching in `watcher.rs`, `accounts` SQLite schema and machine-bound AES-256-GCM encryption, Linux `/proc` and presence lock session inspection, global and per-launch account switching with `Notify` handshake, "Add Account" OAuth flow, DeepSeek wallet balance integration, Claude Code discovery and single-session constraints, and model-pool rotation + auto-retry.

Out of scope: quota metrics storage, PTY warmup probe mechanics, and context window observability (see [Quota Observability and Token Economy](05-quota-observability-and-token-economy.md)), and concurrent `agy` keyring isolation spikes (see [Thread, Conversation, and Runner-Neutral Context Continuity](02-thread-conversation-and-continuity.md)).

## Model / Design

### 1. Multi-Provider Credential Architecture

```text
System Keyring (single slot: service=gemini, username=antigravity)
  └── { token: { refresh_token, access_token, ... }, id_token, auth_method }
                                    │
                                    ▼
                          ONE-SHIP Account Store (`oneship.db` -> `accounts`)
                          ├── provider = 'google'   (AES-256-GCM encrypted refresh_token)
                          ├── provider = 'deepseek' (AES-256-GCM encrypted sk-... API key)
                          └── provider = 'claude'   (discovered from ~/.claude/.credentials.json)
```

Active status is partitioned per provider (`UPDATE accounts SET is_active = 0 WHERE provider = ?`). Users can maintain an active Google, DeepSeek, and Claude account in parallel without collision.

1. **Google Antigravity (`google`):**
   - Stores OAuth2 JSON in the FreeDesktop Secret Service keyring under `(service=gemini, username=antigravity)`, accessed via `libsecret` (`secret-service` crate / `secret-tool`):
     ```json
     {
       "token": {
         "access_token":  "<bearer token, ~1h lifetime>",
         "token_type":    "Bearer",
         "refresh_token": "<long-lived OAuth2 refresh token>",
         "expiry":        "<RFC3339 timestamp>"
       },
       "auth_method": "consumer",
       "id_token": "<JWT with user identity claims>"
     }
     ```
   - `agy` refreshes `access_token` automatically without changing `refresh_token`. Therefore, **`refresh_token` change is the sole signal for a new login**.
   - `extract_email_from_id_token` base64url-decodes the RS256 JWT middle segment (with `=` padding) to read the `email` claim without signature verification.
2. **DeepSeek (`deepseek`):**
   - Authenticates via API keys (`sk-...`), encrypted in SQLite (`accounts`, `provider = 'deepseek'`).
   - On first discovery when no DeepSeek account exists in SQLite, an existing key in `~/.local/share/opencode/auth.json` is imported once; thereafter SQLite is the sole source of truth (no `DEEPSEEK_API_KEY` or `auth.json` fallback). Activating a DeepSeek account writes its key back to `auth.json` while preserving other providers (an unreadable `auth.json` fails explicitly rather than being overwritten).
   - **Live wallet balance:** Queries `GET https://api.deepseek.com/user/balance` (`Authorization: Bearer <API_KEY>`) returning `total_balance`, `topped_up_balance`, and `granted_balance` (`DeepSeekBalanceResponse`). Rendered in `AccountQuotaControls.vue` as `[◉ $X.XX]` using semantic tokens (`--status-ok-color` > $2.00, `--status-warn-color` > $0, `--status-error-color` empty).
3. **Anthropic Claude Code (`claude`):**
   - `discover_claude_account` inspects `~/.claude/.credentials.json` and `claude auth status --json` to register the authenticated account (`provider = 'claude'`).
   - **Single-account CLI constraint:** Because `claude` manages a single active session in `~/.claude/.credentials.json`, `switch_account` updates only SQLite (`is_active`) and does not rewrite `.credentials.json`.
   - **Zero-PTY passive telemetry:** `claude` emits `type: "rate_limit_event"` over `--output-format stream-json`, which `supervisor.rs` writes directly to `account_quotas` (`provider = 'claude'`, `model = 'claude'`) without synthetic probes.

### 2. Encryption & Storage (`accounts` Table)

```sql
CREATE TABLE accounts (
    id          TEXT PRIMARY KEY,
    label       TEXT NOT NULL DEFAULT '',
    email       TEXT NOT NULL UNIQUE,
    is_active   INTEGER NOT NULL DEFAULT 0,
    enc_token   BLOB NOT NULL,
    added_at    TEXT NOT NULL
    -- plus provider column ('google' | 'deepseek' | 'claude')
);
```

Tokens (`JSON { "refresh_token": "<value>" }` or API key) are encrypted with AES-256-GCM using a 32-byte key derived via HKDF-SHA256 from `~/.gemini/installation_id`:

```text
master_key = HKDF-SHA256(
    ikm  = read_file("~/.gemini/installation_id"),
    salt = b"one-ship-accounts-v1",
    info = b"account-token-encryption",
    len  = 32 bytes
)

encrypted_blob = AES-256-GCM(
    key   = master_key,
    nonce = random 12 bytes,  // prepended to ciphertext
    data  = JSON { "refresh_token": "<value>" }
)
```

Because `installation_id` is machine-scoped, encrypted tokens are non-portable across machines without re-authentication.

### 3. Active Session Detection & Passive Keyring Watcher

- **Active `agy` session scan (`get_active_agy_sessions`):**
  1. Enumerates `/proc/<pid>/cmdline`; filters executable basename `== "agy"`.
  2. Extracts `conversation_id` from `--conversation <uuid>`, falling back to `/proc/<pid>/fd/` symlink targets matching `/antigravity-cli/conversations/<uuid>.db` or `/antigravity-cli/presence/<uuid>.lock`.
  3. Maps `agent_key = ProcessSupervisor::get_agent_key_by_pid(pid)` and `cwd = read_link(/proc/<pid>/cwd)` into `AgySession { pid, agent_key, conversation_id, cwd }`.
- **Presence lock check (`check_active_agent_sessions`):** Inspects `~/.gemini/antigravity-cli/presence/<uuid>.lock` for `mtime < 30s`.
- **Passive keyring watcher (`watcher.rs`):** Watches `~/.local/share/keyrings/Default_keyring.keyring` via `inotify` (`IN_MODIFY`). On change, reads `(service=gemini, username=antigravity)`, decodes `refresh_token` and `email`, compares `refresh_token` against `accounts`:
  - Match -> normal `agy` access-token renewal (no-op).
  - No match -> registers new account via `save_account(&email, token, is_new)` without silently overriding `is_active`, and emits `new-account-detected`.

### 4. Global Account Switch & Deterministic Keyring Handshake

When `switch_account(target_account_id, relaunch_active)` is triggered while active presence locks exist, `SwitchConfirmationModal.vue` presents three choices:
1. **Cancel:** Aborts with zero state changes.
2. **Keep Active Chats (`relaunch_active = false`):** Swaps keyring credentials for future launches while active agents continue under startup credentials.
3. **Switch & Restart Chats (`relaunch_active = true`):** Stops active agents in `ProcessSupervisor`, swaps the keyring entry, updates `is_active` in SQLite, relaunches agents with their active native session ID, and emits `account-switched`.

Keyring payload written on swap forces an immediate access-token refresh on the next turn:

```json
{
  "token": {
    "access_token": "",
    "token_type": "Bearer",
    "refresh_token": "<decrypted refresh_token>",
    "expiry": "2000-01-01T00:00:00Z"
  },
  "auth_method": "consumer",
  "id_token": "",
  "_account_email": "<target_account.email>"
}
```

- **Deterministic init handshake (replacing legacy 250ms sleep):** When `launch_agent` temporarily swaps keyring credentials for an agent launch, the supervisor acquires the shared `keyring_lock`, writes the target credentials, spawns `agy`, and awaits `Arc<tokio::sync::Notify>` (`init_notify`) triggered when stdout parses `{"event":"init","conversation_id":"..."}` (or a 10-second safety timeout) before restoring original credentials and releasing the lock.
- **Keyring restoration RAII invariant:** Targeted quota probes use a `KeyringRestoreGuard` (`Drop`) to guarantee original active credentials are restored on every exit path (success, timeout, error, or panic) before releasing `keyring_lock`.
- **Process attribution & zero-rollback chat switching:** `ManagedProcess` and `AgentState` record spawn-time `account_id` and `account_email`. Selecting an account in `FloatingChatOverlay.vue` updates `agent_conversations.account_id`, swaps credentials permanently without deferred rollback timers, and cycles the agent gracefully so the next turn loads the selected account.

### 5. Active Account Registration ("Add Account" Flow)

`initiate_add_account`:
1. Reads and retains the current keyring blob in memory.
2. Clears `(service=gemini, username=antigravity)` via `libsecret`.
3. Spawns `system.preferred_terminal` (default `"ghostty"`) running `agy` (`ghostty`, `foot`, `kitty`, `alacritty` -> `-e`; `wezterm` -> `start --`).
4. Waits up to 5 minutes for `new-account-detected` from the keyring watcher.
5. Closes the terminal child (`kill(child_pid)`), restores the preserved original keyring blob, and emits/notifies success (or timeout with DB untouched).

### 6. Model-Pool Rotation & Seamless Auto-Retry

Antigravity partitions rate limits into independent pools resolved by `canonical_pool_identity(provider, model)`:
- **`gemini-pool`:** Native Google models (`gemini-*`).
- **`3p-pool`:** Third-party models (`claude-*`, `gpt-*`).

When selecting or recommending an account (`[REC]`) for a model:
1. Resolve target pool via `canonical_pool_identity`.
2. Filter accounts with `> 5%` 5-hour quota and `> 0%` 7-day quota in that pool.
3. Sort by `weekly_reset_epoch_seconds ASC` so expiring rolling cycles are consumed first.

When a turn fails with HTTP 429 / `RESOURCE_EXHAUSTED`, `dialogueStore.ts` captures the prompt in `pendingRetryPrompts` (keyed by `${projectPath}::${agentId}`), displays the Error Resolution Banner with `[REC]`, and automatically re-dispatches `prompt_agent` upon account switch/restart without duplicating chat bubbles.

### 7. Edge Cases & Historical Notes

| Condition | Behavior |
| :--- | :--- |
| D-Bus session unavailable | Keyring access fails; switch aborts with `AccountsError`, original keyring untouched |
| `agy` without `--conversation` in cmdline | Relaunched without session resume |
| Duplicate email | Rejected by `email UNIQUE` constraint; UI notifies |
| `~/.gemini/installation_id` missing | Encryption key derivation fails; registration aborted with explicit error |
| Token refresh race | Watcher ignores keyring writes where `refresh_token` is unchanged |
| Remove active or sole account | Blocked at command level; user must switch first or retain >= 1 account |
| Add Account timeout (5 min) | Terminal killed; original keyring restored; DB unchanged |

*Historical note:* Legacy Phase 2 pane-scoped tmux/Herder swapping (`switch_pane_account` / `switch_pane_account_cmd`) was superseded by `ProcessSupervisor` per-launch `accountId` overrides and the deterministic `init_notify` keyring handshake.

## Contracts

### Daemon Commands (`api::API_COMMANDS` / `daemonCall`)

| Command | Arguments | Returns | Description |
| :--- | :--- | :--- | :--- |
| `list_accounts` | `{}` | `AccountSummary[]` | Lists accounts; auto-discovers active keyring / Claude accounts if needed |
| `switch_account` | `{ accountId, relaunchActive }` | `null` | Executes switch sequence; emits `account-switched` and `accounts-updated` |
| `remove_account` | `{ accountId }` | `null` | Deletes non-active account from SQLite |
| `rename_account` | `{ accountId, label }` | `null` | Updates user display label |
| `initiate_add_account` | `{}` | `null` | Launches isolated terminal OAuth login (`system.preferred_terminal`) |
| `check_active_agent_sessions` | `{}` | `AgySession[]` | Running sessions with presence lock files modified `< 30s` ago |
| `get_deepseek_balance` | `{}` | `DeepSeekBalanceResponse` | Queries DeepSeek `/user/balance` for active DeepSeek key |
| `save_deepseek_api_key` | `{ apiKey, email, label }` | `string` | Encrypts and stores DeepSeek key; emits `accounts-updated` |

*Note:* Errors serialize as `AccountsError` in `ApiErrorResponse`.

### Daemon Events (SSE `/api/events`)

- `accounts-updated`
- `new-account-detected` (`{ email }`)
- `account-switched` (`{ email }`)

### Configuration Keys

- `system.preferred_terminal` (default `"ghostty"`)

## Implementation Status

| Item | Status | Evidence |
| :--- | :--- | :--- |
| Keyring R/W, HKDF+AES-256-GCM encryption, `/proc` `get_active_agy_sessions`, `AccountsError` | Implemented | `src-tauri/src/kopeng/accounts.rs`, `src-tauri/src/db.rs` |
| Keyring `inotify` watcher & `new-account-detected` / `account-switched` events | Implemented | `src-tauri/src/watcher.rs`, `src-tauri/src/events.rs` |
| All 8 account daemon commands in `API_COMMANDS` | Implemented | `src-tauri/src/api.rs` |
| DeepSeek balance + `auth.json` sync & Claude Code discovery (`discover_claude_account`) | Implemented | `src-tauri/src/kopeng/accounts.rs`, `src/components/taskbar/AccountQuotaControls.vue` |
| Deterministic `init_notify` handshake & `KeyringRestoreGuard` | Implemented | `src-tauri/src/supervisor.rs`, `src-tauri/src/kopeng/stats.rs` |
| Frontend switcher, confirmation modal & auto-retry banner | Implemented | `src/stores/useAccountStore.ts`, `src/components/modals/SwitchConfirmationModal.vue`, `src/components/dialogue/FloatingChatOverlay.vue` |
| Multi-account hot-swap for Claude Code (`~/.claude/.credentials.json`) | Roadmap | Tracked as single-session CLI constraint |
| Legacy `switch_pane_account` | Retired | Replaced by `launch_agent` + `update_active_conversation_config` |

## Related Specs

- [Native Process Supervisor](01-native-process-supervisor.md)
- [Thread, Conversation, and Runner-Neutral Context Continuity](02-thread-conversation-and-continuity.md)
- [Quota Observability and Token Economy](05-quota-observability-and-token-economy.md)
- [Configuration Schema and Cascade](../01-architecture/02-configuration-schema-and-cascade.md)

Supersedes legacy: `17_multi_account_management.md`
