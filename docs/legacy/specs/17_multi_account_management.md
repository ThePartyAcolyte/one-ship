> [!WARNING]
> **LEGACY SPECIFICATION:** Superseded by [02-agents/04-multi-account-management.md](../../specs/02-agents/04-multi-account-management.md). Preserved for historical provenance only.

# Spec: Multi-Account Management System
**Document Version:** 0.2.6
**Domain:** Authentication, Session Management, Account Lifecycle
**Maintainer:** Kapawu / Omarchy Development Team

---

## 1. Overview & Motivation

Antigravity CLI (`agy`) authenticates via OAuth2 against Google accounts and stores the resulting credentials as a single JSON blob in the system keyring under the fixed key `(service=gemini, username=antigravity)`. There is no built-in mechanism for managing or switching between multiple Google accounts.

ONE-SHIP implements a multi-account management layer that:

1. **Passively detects** new account logins by watching the system keyring for credential changes.
2. **Stores** additional account credentials encrypted in its local SQLite database.
3. **Switches** between registered accounts by atomically replacing the keyring entry, with flexible session management: either continuing active conversations under their original credentials or seamlessly restarting active agents via native `ProcessSupervisor` to continue the conversation under the new account.
4. **Tracks Process-Level Account Attribution** across the system, enabling in-chat mismatch detection and on-demand session reloading without losing thread context.

```
System Keyring (single slot)
  └── (service=gemini, username=antigravity)
        └── { token: { refresh_token, access_token, ... }, id_token, auth_method }
                                    |
                          ONE-SHIP Account Store
                          oneship.db → accounts table
                          { email, encrypted refresh_token, is_active }
```

---

## 2. Credential Structure

The system keyring entry for Antigravity stores a JSON payload with the following structure:

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

**Key identifiers:**

| Field | Role |
|-------|------|
| `refresh_token` | Stable account identifier; used to request new `access_token`s |
| `id_token` | JWT (RS256); payload decodable without key; contains `email` and `sub` claims |
| `access_token` | Short-lived API bearer; expires ~1h; renewed automatically by `agy` |

**Token renewal:** `agy` automatically refreshes the `access_token` via the `refresh_token` when it nears expiry, writing the updated blob back to the keyring. This update changes the `access_token` and `expiry` fields but leaves `refresh_token` unchanged. The ONE-SHIP keyring watcher uses `refresh_token` change as the sole signal for a new login event.

---

## 3. System Keyring Access

ONE-SHIP reads and writes the keyring entry via the `libsecret` D-Bus API (FreeDesktop Secret Service). The Rust crate `secret-service` is used for programmatic access.

**Read (current active token):**
```bash
secret-tool lookup service gemini username antigravity
```

**Write (swap to different account):**
```bash
secret-tool store \
  --label "Password for 'antigravity' on 'gemini'" \
  --attribute service gemini \
  --attribute username antigravity \
  <<< "<new JSON token blob>"
```

**Clear (force re-authentication):**
```bash
secret-tool clear service gemini username antigravity
```

The Tauri daemon must run within a D-Bus session. This is guaranteed on the Omarchy/Hyprland desktop environment where ONE-SHIP operates.

---

## 4. Active Session Detection (ProcessSupervisor & Linux `/proc` Mapping)

Before performing an account switch, ONE-SHIP inspects running agent instances across workspaces to determine if active sessions would be affected. Active session detection is derived natively using the Linux `/proc` filesystem and coordinated with the native `ProcessSupervisor`.

### 4.1 Algorithm

```
get_active_agy_sessions() -> Vec<AgySession>:

  For each directory /proc/<pid>/:
    1. Read /proc/<pid>/cmdline (null-byte separated args)
       Does the executable basename == "agy"?
         NO  -> skip
         YES ->
    2. Extract conversation_id:
       a. Parse cmdline args for "--conversation <uuid>"
       b. Fallback: inspect /proc/<pid>/fd/ symlinks for targets containing:
          - "/antigravity-cli/conversations/<uuid>.db"
          - "/antigravity-cli/presence/<uuid>.lock"
    3. Match agent_key by PID from native ProcessSupervisor:
       agent_key = ProcessSupervisor::get_agent_key_by_pid(pid)
    4. Extract working directory:
       cwd = read_link(/proc/<pid>/cwd)
    5. Append AgySession { pid, agent_key, conversation_id, cwd }
```

### 4.2 AgySession Struct

```rust
pub struct AgySession {
    pub pid: u32,
    pub agent_key: Option<String>,        // e.g. "/path/to/project::bosmang"
    pub conversation_id: Option<String>,  // UUID matching conversation database / lock
    pub cwd: Option<String>,              // Working directory of the agent process
}
```

### 4.3 Verified Output Example

From a live system with active agents:

```
PID=9746  | AGENT_KEY=/workspace::bosmang   | CONV=c4c8a8da-dcff-4d86-a8fd-5c16031e3837
PID=9765  | AGENT_KEY=/workspace::frontend  | CONV=f98dc8e1-6e14-42d8-835c-2cf5b21393c0
PID=9806  | AGENT_KEY=/workspace::core      | CONV=13beb9c0-1ad9-4d04-b400-e0fc6292bd4c
PID=9815  | AGENT_KEY=/workspace::git       | CONV=b4973ae6-a47c-402d-8c16-38530445bbab
```

The `conversation_id` correlates directly to the SQLite conversation store in `~/.gemini/antigravity-cli/conversations/<uuid>.db` and the active session records in `agent_conversations`.

---

## 5. Passive Account Detection (Keyring Watcher)

### 5.1 Mechanism

ONE-SHIP's existing `watcher.rs` is extended to monitor the system keyring file using `inotify`:

**Watch target:** `~/.local/share/keyrings/Default_keyring.keyring`
**Event type:** `IN_MODIFY`

### 5.2 Decision Logic on Keyring Change

```
KeyringModified event:
    |
    +--> Read current keyring entry:
    |    secret-tool lookup service gemini username antigravity
    |
    +--> Parse JSON blob, extract:
    |      new_refresh_token = token.refresh_token
    |      email = decode_jwt_payload(id_token).email
    |
    +--> Compare new_refresh_token against all rows in accounts table:
             MATCH FOUND -> automatic token renewal by agy, no action required
             NO MATCH    -> new login detected:
                            save_account(email, new_refresh_token)
                            emit DaemonEvent::NewAccountDetected { email }
                            UI: non-blocking notification "Account <email> detected"
```

### 5.3 JWT Payload Decoding

The `id_token` is a standard RS256 JWT. Its payload (middle segment between the two `.` characters) is base64url-encoded JSON and requires no signature verification to extract the `email` claim:

```rust
fn extract_email_from_id_token(id_token: &str) -> Option<String> {
    let parts: Vec<&str> = id_token.splitn(3, '.').collect();
    let payload_b64 = parts.get(1)?;
    let padding = (4 - payload_b64.len() % 4) % 4;
    let padded = format!("{}{}", payload_b64, "=".repeat(padding));
    let decoded = base64::decode_config(padded, base64::URL_SAFE)?;
    let json: Value = serde_json::from_slice(&decoded).ok()?;
    json.get("email")?.as_str().map(String::from)
}
```

---

## 6. Account Registration & Storage

### 6.1 Database Schema

```sql
CREATE TABLE accounts (
    id          TEXT PRIMARY KEY,
    -- UUID generated by ONE-SHIP at registration time

    label       TEXT NOT NULL DEFAULT '',
    -- User-editable display name, e.g. "Work", "Personal"
    -- Defaults to email address if not explicitly set

    email       TEXT NOT NULL UNIQUE,
    -- Extracted from id_token JWT payload

    is_active   INTEGER NOT NULL DEFAULT 0,
    -- 1 if this account's refresh_token is currently in the system keyring

    enc_token   BLOB NOT NULL,
    -- AES-256-GCM encrypted JSON: { "refresh_token": "..." }
    -- Encrypted with key derived from installation_id (see §6.2)

    added_at    TEXT NOT NULL
    -- ISO 8601 timestamp of first detection
);

);
```

### 6.2 Encryption of Stored Tokens

Stored `refresh_token` values are encrypted with AES-256-GCM using a key derived from the machine's stable installation identifier:

```
master_key = HKDF-SHA256(
    ikm  = read_file("~/.gemini/installation_id"),  // stable UUID per machine
    salt = b"one-ship-accounts-v1",
    info = b"account-token-encryption",
    len  = 32 bytes
)

encrypted_blob = AES-256-GCM(
    key   = master_key,
    nonce = random 12 bytes,        // stored prepended to ciphertext
    data  = JSON { "refresh_token": "<value>" }
)
```

The `installation_id` file is machine-scoped and not user-specific. It is not a secret itself but serves as a deterministic per-machine salt, making the encrypted token store non-portable across machines without re-authentication.

---

## 7. Active Account Switch Flow

### 7.1 Precondition Check & 3-Choice Resolution

Before initiating an account switch, ONE-SHIP inspects active presence lock files and processes in `ProcessSupervisor`:

```
~/.gemini/antigravity-cli/presence/<uuid>.lock
    mtime < 30 seconds ago -> conversation is in progress
```

If any active agent conversations are detected, the UI presents an interactive confirmation modal (`SwitchConfirmationModal.vue`) with three explicit paths:

```
┌──────────────────────────────────────────────────────────────────┐
│  ACTIVE SESSIONS DETECTED                                        │
│                                                                  │
│  The following agent sessions are currently running:             │
│  - bosmang (PID 9746, Project: one-ship)                      │
│                                                                  │
│  Choose how to proceed:                                          │
│                                                                  │
│  [ Cancel ]                                                      │
│  Aborts the account switch. No state changes occur.              │
│                                                                  │
│  [ Keep Active Chats ] (relaunch_active = false)                 │
│  Switches keyring credentials for future agent launches. Active  │
│  conversations continue untouched under their startup account.   │
│                                                                  │
│  [ Switch & Restart Chats ] (relaunch_active = true)             │
│  Swaps credentials and immediately restarts running agents       │
│  via ProcessSupervisor, preserving full conversation continuity. │
└──────────────────────────────────────────────────────────────────┘
```

### 7.2 Switch Sequence

```
switch_account(target_account_id, relaunch_active):

① IDENTIFY RUNNING PROCESSES (if relaunch_active == true)
  active_agents = ProcessSupervisor::list_active_processes()
  Filter out non-agent processes (services/tasks).
  Extract (agent_key, project_path, agent_id).

② STOP ACTIVE AGENTS (if relaunch_active == true)
  for each agent in active_agents:
    ProcessSupervisor::stop_process(agent.agent_key)

③ SWAP KEYRING CREDENTIAL & UPDATE DATABASE
  Acquire keyring_lock (mutex guard).
  target_account = db_get_account_by_id(target_account_id)
  refresh_token = decrypt(target_account.enc_token)
  
  payload = {
    "token": {
      "access_token": "",
      "token_type": "Bearer",
      "refresh_token": refresh_token,
      "expiry": "2000-01-01T00:00:00Z" // forces immediate refresh on next agy turn
    },
    "auth_method": "consumer",
    "id_token": "",
    "_account_email": target_account.email
  }
  
  write_keyring_token(payload)
  db_set_active_account(target_account_id)

④ RELAUNCH AGENTS UNDER NEW CREDENTIALS (if relaunch_active == true)
  for each (agent_key, project_path, agent_id) in active_agents:
    ProcessSupervisor::launch_agent(project_path, project_name, agent_id)
    // Resolves active agy_conversation_id from SQLite and passes
    // `--conversation <uuid>` to seamlessly resume context.

⑤ UI EVENT EMISSION
  Emit DaemonEvent::AccountSwitched { email: target_account.email }
  Refresh accounts and agent states across frontend stores.
```

### 7.3 Process-Level Account Attribution & In-Chat Mismatch Detection

When `Keep Active Chats` is selected (`relaunch_active = false`), active agent instances remain running under their initial credentials. Because `agy` reads OAuth credentials into process memory upon launch, these processes do not switch accounts automatically mid-turn.

To maintain clarity and control:

1. **Process Attribution in Supervisor:**
   - Every `ManagedProcess` and `AgentState` records `account_id: Option<String>` and `account_email: Option<String>` captured from SQLite at spawn time.
   - `compute_project_state` propagates these attributes across all agent representations.

2. **In-Chat Mismatch Banner (`FloatingChatOverlay.vue`):**
   - The chat overlay evaluates `isAccountMismatched`:
     ```ts
     isAccountMismatched = activeAgentInfo.account_email !== coreStore.activeAccount.email
     ```
   - When mismatched, an amber warning banner appears below the chat header:
     ```
     [!] Running on alice@example.com. Global login switched to bob@example.com.
         [ RELOAD WITH NEW LOGIN ]
     ```
   - Clicking `[ RELOAD WITH NEW LOGIN ]` triggers `restart_agent`, which gracefully stops the process and restarts it under the new active account using `--conversation <active_conv_id>`, resuming the conversation thread under the new account.

### 7.4 Contextual In-Chat Account Switching (`switch_agent_account`)

Developers can change the active account directly from the chat overlay account dropdown (`selectedAccountId`):
1. **Keyring Credential Swap:** Swaps the system keyring token (`libsecret`) to the chosen target account.
2. **Permanent Credential Persistence (Zero Rollback):** Historically, transient rollbacks (restoring the previous credential after 250ms or 3s) caused subsequent agent invocations to revert back to the former account. All deferred keyring rollback routines have been removed, ensuring the selected Google account stays permanently active.
3. **Graceful Agent Cycle:** Sends `SIGTERM` to the active agent process under `agent_key`, waiting up to 500ms for teardown. When the agent is next prompted, `launch_agent` loads the newly swapped credentials from the keyring.
4. **Conversation Binding:** The conversation state in SQLite (`agent_conversations.account_id`) is updated, guaranteeing that UI reactivity in `FloatingChatOverlay.vue` retains the chosen account across session reloads.

---

## 8. Active Account Registration ("Add Account" Flow)

### 8.1 Trigger

A dedicated UI button `[ + Add Account ]` initiates the flow. This is the only way to explicitly trigger a new OAuth login through ONE-SHIP.

### 8.2 Sequence

```
add_account():

① PRESERVE CURRENT TOKEN
  current_blob = secret-tool lookup service gemini username antigravity
  stored in memory (not persisted — operation is transient)

② CLEAR KEYRING
  secret-tool clear service gemini username antigravity
  AGY on its next invocation will detect the missing token and initiate OAuth.

③ SPAWN ISOLATED TERMINAL
  preferred_terminal = config.resolve("orchestrator", "preferred_terminal") ?? "ghostty"
  child = Command::new(preferred_terminal)
    .arg("-e")
    .arg("agy")
    .spawn()
  child_pid = child.id()
  UI: shows "Complete login in the terminal window that just opened."

④ WAIT FOR NEW ACCOUNT DETECTION
  The keyring watcher (§5) is already running.
  ONE-SHIP waits for DaemonEvent::NewAccountDetected.
  Timeout: 5 minutes. On timeout:
    kill(child_pid)
    Restore original blob to keyring
    UI: "Login timed out. No account was added."

⑤ ON DETECTION
  kill(child_pid)            // close the terminal automatically
  Restore original blob to keyring
  // The new account is already saved by the watcher (§5.2)
  UI: "Account <email> added successfully."
```

### 8.3 Terminal Compatibility

The spawn command uses the `-e` flag which is standard across Ghostty, Foot, Kitty, Alacritty, and WezTerm (`wezterm start --`). WezTerm requires a different flag; a terminal-to-flag mapping is maintained in config:

```toml
# rula.toml or ONE-SHIP global settings
[system]
preferred_terminal = "ghostty"   # default

# Internal mapping (not user-configurable):
# ghostty  -> -e
# foot     -> -e
# kitty    -> -e
# alacritty -> -e
# wezterm  -> start --
```

---

## 9. Rust Module Structure

```
src-tauri/src/kopeng/
  └── accounts.rs    AgySession scan, keyring R/W, encryption, ProcessSupervisor restart logic

src-tauri/src/
  ├── watcher.rs     EXTENDED — keyring file watcher, NewAccountDetected event
  ├── db.rs          EXTENDED — accounts table
  ├── supervisor.rs  EXTENDED — ProcessSupervisor process lifecycle, account attribution
  └── api.rs         EXTENDED — daemon commands exposed to the frontend
```

### 9.1 Daemon Commands (Spec 12 §7.2)

The account domain runs in the daemon: account switches relaunch agents there, and the keyring lock is shared with the quota watcher (Spec 23). Errors are `AccountsError` as `ApiErrorResponse`.

```text
list_accounts({})                                   -> AccountSummary[]
  // Automatically discovers and syncs the active keyring token if the DB store is empty.
switch_account({ accountId, relaunchActive })       -> null
  // Executes the switch sequence (§7.2), then emits `account-switched` and `accounts-updated`.
remove_account({ accountId })                       -> null
  // Deletes from DB. Cannot remove the currently active account.
rename_account({ accountId, label })                -> null
initiate_add_account({})                            -> null
  // Begins the add_account flow (§8) in a `system.preferred_terminal` window.
check_active_agent_sessions({})                     -> AgySession[]
  // Running sessions with presence lock files modified < 30 seconds ago.
get_deepseek_balance({})                            -> DeepSeekBalanceResponse
save_deepseek_api_key({ apiKey, email, label })     -> string (account id); emits `accounts-updated`
```

### 9.2 Public API of `kopeng/accounts.rs`

```rust
pub struct AgySession {
    pub pid: u32,
    pub agent_key: Option<String>,
    pub conversation_id: Option<String>,
    pub cwd: Option<String>,
}

pub struct AccountSummary {
    pub id: String,
    pub label: String,
    pub email: String,
    pub is_active: bool,
    pub added_at: String,
}

pub fn get_active_agy_sessions() -> Vec<AgySession>;
pub fn read_keyring_token() -> Result<Value, String>;
pub fn write_keyring_token(payload: &Value) -> Result<(), String>;
pub fn clear_keyring_token() -> Result<(), String>;
pub fn check_and_register_keyring_account() -> Result<Option<(AccountSummary, bool)>, String>;
pub fn check_active_presence_sessions() -> Result<Vec<AgySession>, String>;
pub fn extract_email_from_id_token(id_token: &str) -> Option<String>;
pub fn save_account(email: &str, refresh_token: &str, is_active: bool) -> Result<String, String>;
pub async fn switch_account(target_account_id: &str, relaunch_active: bool) -> Result<(), String>;
pub fn initiate_add_account(preferred_terminal: Option<String>) -> Result<(), String>;
pub fn remove_account(account_id: &str) -> Result<(), String>;
pub fn rename_account(account_id: &str, label: &str) -> Result<(), String>;
```

---

## 10. DaemonEvent Extensions

Two new variants are added to the `DaemonEvent` enum (`src-tauri/src/events.rs`):

```rust
pub enum DaemonEvent {
    // ... existing variants ...
    NewAccountDetected { email: String },
    AccountSwitched    { email: String },
}
```

The frontend subscribes to these via the existing Tauri event system to update the account switcher UI reactively.

---

## 11. UI Components

### 11.1 Account Switcher

A compact dropdown in the main toolbar or sidebar:

```
[ user@example.com ▾ ]
  ├── user@example.com  ●  (active)
  ├── work@example.com
  ├── ─────────────────────────
  └── + Add Account
```

The active account is indicated by a filled dot. Clicking a non-active account triggers `switch_account()`.

### 11.2 Switch Confirmation Modal

Displayed when a switch is requested while any active presence lock exists:

```
┌──────────────────────────────────────────────────────┐
│  Active session in progress                           │
│                                                       │
│  "bosmang" is currently working on a task.            │
│  Switching now will interrupt this session.           │
│                                                       │
│  [ Wait for completion ]    [ Switch anyway ]         │
└──────────────────────────────────────────────────────┘
```

---

## 12. Constraints & Edge Cases

| Condition | Behavior |
|-----------|----------|
| D-Bus session unavailable | `secret-tool` fails; switch aborted with error; original keyring untouched |
| `agy` without `--conversation` in cmdline | Relaunched as plain `agy` (new session, no conversation to resume) |
| Two accounts with same email | Duplicate detection via `email UNIQUE` constraint; second entry rejected; UI notifies |
| `installation_id` file missing | Encryption key derivation fails; account registration aborted; fallback: prompt user to re-run `agy` to regenerate the file |
| Switch while snapshot in progress | Snapshot operation completes first (it is synchronous and fast); switch proceeds after |
| Token refresh race condition | `agy` may refresh the `access_token` at any time, writing to keyring; the watcher ignores changes where `refresh_token` is unchanged |
| Remove active account | Blocked at command level; user must first switch to another account |
| No other accounts registered | `remove_account` blocked if only one account exists (would leave system in unauthenticated state) |
| Add Account timeout (5 min) | Terminal process killed; original token restored to keyring; no state change in DB |

---

## 13. Phase 2: Pane-Scoped / Window-Scoped Temporary Account Override

While the primary account switcher operates globally across all active sessions, individual developer workflows often require dedicating a specific agent pane or chat conversation to a secondary Google account (e.g. reserving a corporate quota pool for heavy refactoring while keeping personal accounts on default agents).

### 13.1 Sequential Atomic Swap Pattern

```
[ User triggers account change on Pane wT:p1 ]
                       │
                       ▼
 1. READ & REMEMBER current default keyring token (Account A)
                       │
                       ▼
 2. TEMPORARY SWAP: Write Account B's payload to secret-tool
                       │
                       ▼
 3. RELAUNCH PANE: Send "agy --conversation <conv_id>" to Pane wT:p1
                       │
                       ▼
 4. HANDSHAKE DELAY: Allow agy process to read keyring into memory (150-200ms)
                       │
                       ▼
 5. RESTORE DEFAULT: Re-write Account A payload to secret-tool
                       │
                       ▼
 [ Result: Pane wT:p1 runs under Account B; global default remains Account A ]
```

### 13.2 Backend Command Interface
- `switch_pane_account(pane_id: &str, conversation_id: Option<&str>, target_account_id: &str) -> Result<(), String>`
- Exposed as `switch_pane_account_cmd` in Tauri.

---

## 14. Multi-Provider Architecture & DeepSeek Integration (Build 0.2.2)

### 14.1 Provider Decoupling & Storage Isolation
ONE-SHIP decouples runners from account credentials and AI providers:
- **Google Provider (`google`):** Authenticates via OAuth2 and synchronizes tokens with the OS keyring (`secret-tool` / libsecret). Tracks rolling usage quotas via sliding 5-hour and 7-day rate-limit windows.
- **DeepSeek Provider (`deepseek`):** Authenticates via API keys (`sk-...`), stored encrypted via machine-bound AES-256-GCM in SQLite (`accounts` table, `provider = 'deepseek'`). A key found in OpenCode credentials (`~/.local/share/opencode/auth.json`) is imported once as an account when none exists; after that SQLite is the only source (no `DEEPSEEK_API_KEY` or `auth.json` lookup fallback), and activating a DeepSeek account writes its key back to `auth.json`, preserving other providers (an unreadable file is an error, never overwritten).
- **Parallel Active Accounts:** Active status is partitioned per provider (`UPDATE accounts SET is_active = 0 WHERE provider = ?`). Users maintain an active Google account and an active DeepSeek account simultaneously without collision.

### 14.2 Live Wallet Balance Observability
DeepSeek charges on token consumption against a pre-paid wallet rather than rate-limit quotas:
- **Endpoint:** `GET https://api.deepseek.com/user/balance` with `Authorization: Bearer <API_KEY>`.
- **Telemetry Payload:**
  - `total_balance`: Total available funds in USD.
  - `topped_up_balance`: Cash deposits.
  - `granted_balance`: Promotional bonus balance.
- **Daemon command:** `get_deepseek_balance` returns `DeepSeekBalanceResponse`.
- **UI Presentation:** Rendered in `AccountQuotaControls.vue` inside the bottom taskbar (`AgentTaskbar.vue`) as a brutalist `[◉ $X.XX]` badge with semantic theme styling (`--status-ok-color` when balance > $2.00, `--status-warn-color` when positive, `--status-error-color` when empty). Clicking the badge opens a wallet breakdown popover with realtime refresh and API key configuration controls.

---

## 15. Deterministic Keyring Handshake & Model-Pool Quota Rotation (Build 0.2.3)

### 15.1 Deterministic Init Handshake
Previous versions utilized an arbitrary 250ms sleep when swapping temporary credentials in the system keyring before launching Antigravity CLI. Because the Node/V8 runtime requires 3–5 seconds to initialize and query the OS Secret Service, the token was restored prematurely, resulting in credential desync.

Build 0.2.3 replaces all arbitrary sleep intervals with an event-driven synchronization channel (`Arc<tokio::sync::Notify>`):
1. **Keyring Lock & Swap:** Supervisor acquires the global `keyring_lock`, sets the target credentials into the keyring, and spawns the child process.
2. **Notification Listener:** The supervisor's stdout stream reader monitors child output. Upon parsing `{"event":"init","conversation_id":"..."}`, the reader triggers `init_notify.notify_waiters()` and maps `agy_conversation_id` into SQLite (`agent_conversations`).
3. **Restoration & Lock Release:** A detached background worker holds the keyring lock until `init_notify` fires (or a 10-second safety timeout elapses), immediately restoring original credentials and releasing the lock.
4. **Non-Invasive Watcher:** Passive filesystem mtime scans on credentials files invoke `save_account(&email, token, is_new)`, preventing passive updates from silently overriding `is_active` in SQLite.

### 15.2 Model-Pool-Aware Rotation & Weekly Reset Priority
Antigravity partitions token limits into independent pools:
- **`gemini-pool`:** Consumed by native Google models (`gemini-*`).
- **`3p-pool`:** Consumed by external third-party models (`claude-*`, `gpt-*`).
Exhaustion in one pool does not impact availability in the other.

When selecting or recommending an account for a given model:
1. Identify the target pool via `canonical_pool_identity(provider, model)`.
2. Filter for viable accounts with $> 5\%$ 5-hour quota and $> 0\%$ 7-day quota in that specific pool.
3. Prioritize accounts closest to their weekly 7-day reset date (`weekly_reset_epoch_seconds ASC`), ensuring expiring rolling cycles are utilized before resetting.

### 15.3 Seamless Auto-Retry on Quota Error
When an agent turn encounters a quota limit (HTTP 429 / `RESOURCE_EXHAUSTED`):
1. The frontend dialogue store captures the failed user prompt into `pendingRetryPrompts[agentId]`.
2. The Chat Overlay displays the actionable Error Resolution Banner highlighting the optimal recommended account (`[REC]`).
3. Upon selecting an alternative account or clicking restart, the supervisor relaunches the agent, and the dialogue store automatically re-dispatches the captured prompt via `prompt_agent`.
4. Because the user message is already rendered in the chat UI, the prompt is retried seamlessly without duplicate chat bubbles.

---

## 16. Deterministic Scheduled Warmup & Local Timezone Presentation (Build 0.2.4)

### 16.1 Zero Startup Warmup & Passive Boot Invariant
Previous versions triggered proactive probes on startup because `tokio::time::interval` completes its 0th tick immediately upon spawning, treating stale reset epochs from previous sessions as expired events.

Build 0.2.4 establishes the strict **Zero Startup Warmup Invariant**:
1. **Initial Tick Consumption:** `spawn_quota_watcher` consumes the initial 0th tick (`refresh_interval.tick().await`) before entering the monitoring loop. Startup executions perform strictly passive observation (`sync_quota_from_statusline_file`) without invoking `probe_account_quota`.
2. **Runtime Crossing Detection:** Warmup probes execute **strictly** when the scheduled reset boundary is crossed during active runtime:
   $$\text{epoch} \le \text{now} \land \text{epoch} > \text{last\_check\_epoch\_seconds}$$
   or when `now >= epoch` and the account has not been pinged since the reset boundary.
3. **Depleted-Only Kickstart Filter:** Accounts with healthy quotas ($> 5\%$) are never probed. Kickstarts are restricted exclusively to exhausted windows (`remaining_pct <= 5.0%`).
4. **Mandatory Probe Cooldown:** Every probe attempt immediately updates `last_pinged_at` in SQLite via `db_update_account_quota_ping`, enforcing a minimum 15-minute (900s) backoff interval. Failed probes or repeated rate limits cannot produce recursive probe storms.

### 16.2 Local Timezone Presentation & ISO-8601 Synchronization
Antigravity publishes quota reset deadlines as UTC ISO-8601 strings (e.g. `"2026-09-25T09:15:59Z"`):
1. **RFC 3339 Parser:** `parse_countdown_to_epoch` parses ISO-8601 timestamps directly into Unix epoch seconds (`chrono::DateTime::parse_from_rfc3339`) before falling back to duration strings (`3h 20m`).
2. **Local Time Conversion:** Backend persistence computes `five_hour_resets_at_local` and `weekly_resets_at_local` in the host's local timezone (`chrono::Local`, e.g. UTC-3 `06:15:59`).
3. **Gauge & Dropdown Tooltips:** Frontend quota gauges (`AppQuotaConcentric`, `AppQuotaBar`, `AppQuotaPie`) format both the local clock time and the remaining countdown:
   - Healthy: `Gemini 5h: 71% remaining • Resets at 06:15 local (2h 55m left)`
   - Depleted: `Other 5h: 0% remaining (Depleted) • Resets at 06:30 local (58m left)`
4. **Scoped Warmup Feedback:** The `keyring-warmup-status` event and top-bar `⚡ WARMING UP` badge are scoped strictly to the active account and target pool, preventing false warmup indicators on healthy un-probed accounts.

---

## 17. Interactive PTY Probe Isolation & Cross-Account Attribution Guard (Build 0.2.5)

### 17.1 Interactive Terminal Requirement for Statusline Hook
Antigravity CLI only executes its configured `statusLine` hook command (`scripts/agy_statusline.py`) when attached to an interactive terminal driving the Charmbracelet Bubbletea TUI loop. When executed in headless batch mode (`agy -p`), Antigravity skips Bubbletea and the `statusLine` hook is never executed. Consequently, batch probes fail to update the observation state file (`~/.local/state/one-ship/agy-statusline.json`).

### 17.2 Native Linux PTY Allocation via `libc::openpty`
Build 0.2.5 introduces native pseudo-terminal allocation for quota probes:
1. **PTY Pair Allocation:** `probe_account_quota` calls `libc::openpty` to allocate master and slave file descriptors.
2. **Master FD Drain:** A background blocking task reads and drains `master_fd` to prevent the probe child process from blocking on stdout/stderr buffer saturation.
3. **Slave Stdio Attachment:** Duplicated slave file descriptors are attached to child process `stdin`, `stdout`, and `stderr`, presenting a valid interactive TTY.
4. **Interactive Kickstart:** The probe launches `agy -i hi --model <quota_probe_model> --effort <quota_probe_effort> --disable-slash-commands` (default `gemini-3.8-flash` / `low`, a pair of the agy manifest). Antigravity detects a valid TTY, initializes the TUI loop, queries the Google quota endpoint for the authenticated account in the keyring, and triggers `scripts/agy_statusline.py`.
5. **State Detection & Teardown:** The daemon polls `agy-statusline.json` for an entry matching the target account (`email` or `account_id`) with `timestamp >= probe_start_epoch - 1`. Once detected (or upon a 7-second timeout), the probe child process is immediately killed (`child.kill().await`).

### 17.3 Cross-Account Attribution Guard
To prevent cross-account contamination when reading observation files:
1. **Target Account Validation:** In `sync_quota_from_statusline_file`, when `target_account_id` is specified, the parser strictly verifies that the statusline payload's `email` or `account_id` matches the target account. Mismatched payloads are rejected with a `WARN` log and discarded.
2. **Deterministic Unspecified Resolution:** When `target_account_id` is `None` (passive background watcher), the account is resolved strictly by matching `val["account_id"]` or `val["email"]` against the database before falling back to the active account.
3. **Strict Script Email Fallback:** In `scripts/agy_statusline.py`, fallback to `is_active = 1` is restricted strictly to payloads lacking any email field, preventing mismatched OAuth identities from being attributed to the active account.

### 17.4 Keyring Restoration RAII Invariant
To ensure the active account's credentials are never stranded during targeted probes:
- A `KeyringRestoreGuard` struct implements `Drop`, restoring the original active account token into the keyring on any exit path (success, timeout, error, or panic) before the keyring lock is released.

---

## 18. Anthropic Claude Code Account Discovery & Telemetry Integration (Build 0.2.6)

### 18.1 Account Discovery & Single-Session Constraint
Anthropic Claude Code CLI (`claude`) authenticates via OAuth with Anthropic / Claude Pro and persists its active credentials at `~/.claude/.credentials.json`:
- **Token Discovery (`discover_claude_account` in `kopeng/accounts.rs`):** The daemon inspects `~/.claude/.credentials.json` and `claude auth status --json` to register the authenticated Claude account (`provider = 'claude'`) in SQLite (`accounts` table).
- **Parallel Active Account Partition:** Active accounts are partitioned per provider (`google`, `deepseek`, `claude`) so each provider maintains its own active account record in SQLite simultaneously.
- **Single-Account CLI Constraint (Tracked in `TECHNICAL_DEBT.md` #28):** Unlike Antigravity (which reads from the OS keyring on spawn), Claude Code CLI manages a single active session directly inside `~/.claude/.credentials.json`. Consequently, `switch_account` only updates the active record in SQLite and does not overwrite `~/.claude/.credentials.json` on disk; multi-account hot-swapping for Claude Code is deferred as technical debt pending viability evaluation.

### 18.2 Zero-PTY Passive Quota Telemetry via `stream-json`
Unlike Antigravity CLI (which requires an interactive Linux PTY and statusline script to observe Google quota windows), Anthropic Claude Code CLI natively outputs rate-limit telemetry when operating in headless mode (`--output-format stream-json`):
- **Stream Event Structure:** Claude Code emits asynchronous NDJSON events with `type: "rate_limit_event"`.
- **Real-Time Database Synchronization (`supervisor.rs`):** The supervisor's stdout stream reader intercepts `rate_limit_event`, computes `five_hour_remaining_pct` and `weekly_remaining_pct` along with `five_hour_reset_epoch_seconds` and `weekly_reset_epoch_seconds`, and writes directly to the SQLite `account_quotas` table (`provider = 'claude'`, `model = 'claude'`).
- **100% Passive Observability:** Eliminates the need for periodic synthetic warmup probes or PTY kickstarts for Claude Code.
