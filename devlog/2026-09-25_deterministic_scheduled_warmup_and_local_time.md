# Devlog: Deterministic Scheduled Warmup and Local Timezone Presentation

**Date:** 2026-09-25  
**Topic:** Scheduled Quota Warmup, Local Time Presentation, Zero Startup Probing  
**Subsystems Affected:** `src-tauri/src/kopeng/stats.rs`, `src-tauri/src/db.rs`, `src-tauri/Cargo.toml`, `src/stores/dialogueStore.ts`, `src/components/ui/quotaUtils.ts`, `src/components/ui/AppQuota*.vue`, `src/components/AppHeader.vue`, `docs/specs/17_multi_account_management.md`

---

## 1. Problem Statement & Root Cause Analysis

When the application booted or restarted after an edit/work cycle, Gemini models appeared in warmup (`⚡ WARMING UP`) even when their quota was healthy (e.g. 94% remaining) and their rolling quota clock cycles had not completed.

Investigating the supervisor and quota watcher revealed the following root causes:
1. **Immediate Tick in Tokio Interval:** `spawn_quota_watcher` created `tokio::time::interval(refresh_s)`, which in Tokio completes its initial 0th tick immediately upon invocation at $t=0$.
2. **Stale Epochs Evaluated as Expired Events:** The expiration check `now >= epoch && q.last_pinged_at.map(|lp| lp < epoch).unwrap_or(true)` triggered whenever `epoch` was in the past (e.g. from an old session, yesterday, or machine sleep), treating past deadlines as fresh reset events on application startup.
3. **Probing Healthy Accounts:** The watcher did not check whether quota was actually depleted. An account with 94% Gemini quota but an expired Anthropic 3P window triggered a Gemini micro-probe (`gemini-3.8-flash-low`), causing `⚡ WARMING UP` to render over the Gemini gauges.
4. **Infinite Probe Storms on Failure:** `probe_account_quota` only updated `last_pinged_at` if `sync_quota_from_statusline_file` succeeded. Failed probes or empty rate-limit outputs left `last_pinged_at` stale, causing repeated probe triggers on every 60-second tick.
5. **Missing ISO-8601 Parsing & Local Timezone Conversion:** Antigravity statuslines provide `reset_time` as UTC ISO-8601 timestamps (e.g. `2026-09-25T09:15:59Z`). The parser failed on ISO strings and did not format target reset times into the developer's local timezone (UTC-3).

---

## 2. Architectural Remediation & Implementation

### 2.1 Backend: Zero Startup Probes & Runtime Boundary Watcher (`src-tauri/src/kopeng/stats.rs`)
- **Initial Tick Consumption:** `refresh_interval.tick().await;` is called once before entering the loop to ensure zero probes fire on boot.
- **Missed Tick Behavior:** Configured `MissedTickBehavior::Delay` to prevent tick bursts.
- **Runtime Crossing Invariant:** A reset triggers strictly when crossed during active runtime:
  $$\text{epoch} \le \text{now} \land \text{epoch} > \text{last\_check\_epoch\_seconds}$$
  or when `now >= epoch` and the account has not been pinged since the reset.
- **Depleted Quota Filter:** Kickstart probes are restricted exclusively to exhausted pools (`remaining_pct <= 5.0%`). Accounts with ample quota are never probed.
- **Probe Cooldown & Ping Update:** `probe_account_quota` immediately executes `db_update_account_quota_ping(account_id, now)`, enforcing a minimum 15-minute (900s) backoff interval.
- **ISO-8601 & RFC 3339 Parsing:** `parse_countdown_to_epoch` parses ISO-8601 timestamps via `chrono::DateTime::parse_from_rfc3339` and common ISO formats, with `epoch_to_local_datetime_string` converting timestamps into local host time.

### 2.2 Database Schema & Local Time Persistence (`src-tauri/src/db.rs`)
- Added `five_hour_resets_at_local` and `weekly_resets_at_local` columns to `account_quotas` in SQLite with automatic PRAGMA migration checks.
- Updated `AccountQuotaRecord` struct, queries, and upsert statements across the backend.

### 2.3 Frontend Presentation & Tooltip Formatting
- **`src/stores/dialogueStore.ts`:**
  - Extended `AccountQuotaRecord` interface with local reset properties.
  - Updated `keyring-warmup-status` listener to track `warmupAccountId` and `warmupPool`.
- **`src/components/ui/quotaUtils.ts`:**
  - Implemented `formatLocalResetTime(epochSeconds)` converting timestamps into `HH:MM` or `MM/DD HH:MM`.
  - Updated `formatQuotaTooltip` to display both local clock time and remaining countdown (e.g. `Gemini 5h: 71% remaining • Resets at 06:15 local (2h 55m left)`).
- **`src/components/ui/AppQuotaConcentric.vue`, `AppQuotaBar.vue`, `AppQuotaPie.vue`:**
  - Passed `props.resetEpochSeconds` down to `formatQuotaTooltip`.
- **`src/components/AppHeader.vue`:**
  - Scoped the top-bar `⚡ WARMING UP` indicator strictly to when `dialogueStore.warmupAccountId === coreStore.activeGoogleAccount?.id`.

---

## 3. Verification & Validation

1. **Unit Testing:**
   - Added unit test assertions in `test_parse_countdown_to_epoch` verifying RFC 3339 UTC timestamps and local datetime formatting.
   - Executed `cargo test test_parse_countdown_to_epoch` -> **1 passed; 0 failed**.
2. **Backend Compilation:**
   - Ran `cargo check --manifest-path src-tauri/Cargo.toml` -> **Finished with 0 errors**.
3. **Frontend Compilation:**
   - Executed `npm run build` (`vue-tsc --noEmit && vite build`) -> **Built successfully in 7.71s with 0 errors**.
