# Devlog: Multi-Account Management Bugfix Session

**Date:** 2026-09-10
**Session scope:** Diagnosis and repair of critical bugs in the multi-account system (Spec 17).

---

## Symptoms Reported

1. Account dropdown did not visually mark which account was active.
2. Clicking a non-active account to switch produced no visible result.
3. (Architectural) Session restoration on switch affects all `agy` processes system-wide, not scoped to a project.

---

## Root Cause Analysis

### Bug 1 — `write_keyring_token`: Invalid `--attribute` flag in `secret-tool store` (CRITICAL)

`secret-tool store` accepts positional `attribute value` pairs only. The implementation was passing
`--attribute service gemini --attribute username antigravity` which is not a valid flag syntax.
Every keyring write (core of `switch_account`) silently failed with non-zero exit code.

**Fix:** Removed the invalid `--attribute` flags.
Correct invocation: `secret-tool store --label "..." service gemini username antigravity`

---

### Bug 2 — Phantom account creation post-switch

After `switch_account` writes to the keyring (with `id_token: ""`), the watcher polling loop
(every 1500ms) detected the mtime change and called `check_and_register_keyring_account`.
Email extraction failed on the empty JWT, fell back to `"unknown@antigravity"`, and created a
phantom account with `is_active = true` in DB — overriding the correct switch state.

**Fix:**
1. Added `_account_email` synthetic field to the switch keyring payload.
2. `check_and_register_keyring_account` now checks `_account_email` first, then falls back to
   JWT extraction, and bails with `Ok(None)` (no phantom creation) if email is unresolvable.
3. Returned `AccountSummary.is_active` now reflects actual DB state.

---

### Bug 3 — No Tauri events emitted after switch

The frontend listened for `account-switched` and `accounts-updated` events, but `switch_account_cmd`
never emitted them. The UI depended solely on a `fetchAccounts()` call that raced with the watcher.

**Fix:** `switch_account_cmd` now accepts `app: tauri::AppHandle` and emits both events
(`account-switched` + `accounts-updated`) immediately after a successful switch.

---

## Files Modified

- `src-tauri/src/kopeng/accounts.rs`:
  - `write_keyring_token`: removed invalid `--attribute` flags
  - `check_and_register_keyring_account`: `_account_email` field resolution + bail on unknown email
  - `switch_account` payload: added `_account_email` synthetic field
  - `switch_account_cmd`: added `AppHandle`, emits account events post-switch

---

## Deferred Items (→ TODO)

- Per-conversation account scope (temporary account override per Herder pane)
- Session restoration scoped to project Herder instance only

---

## Session Resumption & Confirmation Guard Enhancements

1. **Proc FD Symlink Inspection for Conversation UUID:**
   - When `agy` processes are spawned without explicit `--conversation` CLI flags, `get_active_agy_sessions()` now scans `/proc/<PID>/fd` symlinks to find open handles to `.../conversations/<UUID>.db` or `.../presence/<UUID>.lock`.
   - Captures the exact active `conversation_id` dynamically.
2. **Unconditional Session Confirmation Prompt:**
   - `check_active_presence_sessions()` now returns all active `agy` sessions found in `/proc` regardless of presence lock file age.
   - Triggers `SwitchConfirmationModal` whenever agents are alive, giving the user explicit control to "Wait" or "Switch Anyway".
3. **In-place Conversation Restoration:**
   - Post-switch relaunch commands dispatched to Herder panes now properly include `--conversation <UUID>`, restoring the exact conversation context under the new account credentials.

---

## Non-blocking Asynchronous Switch Optimization

1. **Sub-second PID Reactive Polling:**
   - Replaced fixed 10s wait loop with reactive polling on `/proc/<PID>` (50ms interval, 800ms max timeout) followed by immediate `SIGKILL` fallback for unresponsive processes.
   - Reduced process teardown latency from 10s to ~100-300ms.
2. **Asynchronous Command Execution in Tauri:**
   - Converted `switch_account_cmd` to `async fn` using `tokio::task::spawn_blocking`.
   - Prevents blocking the Tauri event loop and GUI thread, completely eliminating the OS "Application is not responding" warning.
