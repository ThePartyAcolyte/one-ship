# Development Log: Account Quota Isolation & Interactive PTY Warm-up Probe

**Date:** 2026-09-25  
**Topic:** Account Quota Isolation, Native PTY-backed Warm-up Probes, and Attribution Guards  
**Subsystems:** `src-tauri/src/kopeng/stats.rs`, `scripts/agy_statusline.py`, `src/stores/dialogueStore.ts`, `docs/specs/17_multi_account_management.md`

---

## 1. Problem Statement & Bug Forensic

When triggering manual quota warm-up (`⚡`) on secondary Google accounts (e.g. `kirulabcl@gmail.com` / "qirulao") from the account dropdown, the quota metrics recorded in SQLite were identical duplicates of the active session account (`thepartyacolyte@gmail.com` / "partiaco lite") rather than querying the targeted account.

### Root Causes
1. **Interactive TUI Requirement for Statusline Hook:**
   Antigravity CLI executes the `statusLine` command hook (`scripts/agy_statusline.py`) exclusively during interactive terminal sessions driven by the Bubbletea TUI loop. When executed in batch print mode (`agy -p`), Antigravity skips the TUI loop entirely and never fires the hook. Consequently, `~/.local/state/one-ship/agy-statusline.json` was never updated during batch probes, retaining stale observation data from the active terminal.
2. **Missing Attribution Guard in `sync_quota_from_statusline_file`:**
   When `target_account_id` was passed to `sync_quota_from_statusline_file`, the function assigned the stale observation data directly to the target account without validating whether `email` or `account_id` in the JSON payload matched the target account. This wrote `thepartyacolyte`'s quota percentages into `kirulabcl`'s database records.

---

## 2. Technical Implementation

### A. Strict Attribution Guard in `sync_quota_from_statusline_file`
- Implemented a verification check: if `target_account_id` is specified, compare `val["email"]` (and `val["raw_payload"]["email"]`) against `db_get_account_by_id(target_id)`.
- If neither `account_id` nor `email` matches, log a `WARN` and return `false`, rejecting cross-account pollution.
- When `target_account_id` is `None` (passive background watcher), resolve the account identity strictly against the database by matching `account_id` or `email`, only falling back to active account if no identity was provided.

### B. Native Linux PTY Allocation via `libc::openpty` in `probe_account_quota`
- Replaced `tokio::process::Command::new("agy").args(["-p", ...])` with native pseudo-terminal allocation:
  - Allocates PTY pair using `libc::openpty`.
  - Attaches duplicated slave file descriptors to child `stdin`, `stdout`, and `stderr`.
  - Spawns background drain task for `master_fd` to prevent the child from blocking on write buffer exhaustion.
  - Spawns `agy -i hi --model gemini-3.8-flash-low --disable-slash-commands`, ensuring Antigravity runs in interactive mode and triggers `scripts/agy_statusline.py`.
- Polling loop checks `agy-statusline.json` for an entry with `timestamp >= probe_start_epoch - 1` and matching `acc.email` or `acc.id` (timeout: 7 seconds).
- Once observed, the child process is terminated immediately via `child.kill().await`.

### C. RAII Keyring Restoration Guard
- Created `KeyringRestoreGuard` with `Drop` implementation:
  - Guarantees the original active account credentials are restored to the system keyring on any exit path (early return, error, timeout, or panic) before the keyring lock is released.

### D. Strict Email Matching in Statusline Hook (`scripts/agy_statusline.py`)
- Adjusted account resolution in `process_payload`: fallback to `is_active = 1` is strictly restricted to payloads omitting the `email` key, preventing unknown OAuth identities from being attributed to the active account.

### E. Frontend Quota Reactive Refresh (`src/stores/dialogueStore.ts`)
- Added automatic `await this.fetchAccountQuotas()` inside the `keyring-warmup-status` event listener when `!payload.in_progress`, ensuring UI gauges and dropdowns reflect the latest quota upon probe completion.

---

## 3. Verification

1. **Compilation Checks:**
   - `cargo check --manifest-path src-tauri/Cargo.toml`: Success (exit code 0).
   - `npm run build`: Success (`vue-tsc --noEmit && vite build`, exit code 0).
2. **Database Integrity:**
   - Account metrics verified isolated in SQLite between `thepartyacolyte@gmail.com` (weekly 71.76%) and `kirulabcl@gmail.com` (weekly 41.65%).
3. **Living Documentation:**
   - Updated `docs/specs/17_multi_account_management.md` with Section 17 detailing PTY probe requirements, attribution guards, and RAII keyring invariants.
