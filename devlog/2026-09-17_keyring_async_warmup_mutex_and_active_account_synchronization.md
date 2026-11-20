# Devlog: Keyring Async Warmup Mutex & Active Account Synchronization

**Date:** 2026-09-17  
**Topic:** Concurrency synchronization for background account warmup probes, OS keyring locking, and agent spawning safety.

---

## 1. Context & Problem
- During background account quota observation, `spawn_quota_watcher` periodically executes `probe_account_quota()` every 60s for secondary accounts marked with `warmup_enabled = true`.
- `probe_account_quota()` temporarily swaps the active credentials in the OS keyring (`secret-tool`) to execute a lightweight probe (`agy -p hi`).
- If an agent was launched during that exact window (e.g. clicking `[ RESOLVE ]` for the profiler flag), `spawn_agent` in `supervisor.rs` would spawn `agy` while the secondary account was in the keyring.
- If the secondary account held internal or dogfood Google credentials (or unverified account claims), `agy` attempted to connect to `daily-cloudcode-pa.googleapis.com`, crashing with `dial tcp 172.217.119.4:443: connect: no route to host`.

---

## 2. Architecture & Implementation
1. **Centralized Async Keyring Lock (`accounts::keyring_lock()`):**
   - Implemented a global `tokio::sync::Mutex<()>` in [`accounts.rs`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/kopeng/accounts.rs).
   - Any operation that mutates or temporarily overrides the OS keyring (`probe_account_quota`, `switch_account`, `initiate_add_account`, `switch_agent_account`) acquires this lock.
2. **Active Account Invariant (`ensure_active_account_in_keyring()`):**
   - Automatically synchronizes the active SQLite account credentials into the OS keyring whenever called.
   - Guaranteed cleanup at the end of every probe or account operation.
3. **Agent Launch Coordination:**
   - In [`supervisor.rs`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/supervisor.rs), `spawn_agent()` acquires `keyring_lock().await` and invokes `ensure_active_account_in_keyring()` immediately prior to `cmd.spawn()`.
   - If a warmup probe is currently running, `spawn_agent()` seamlessly awaits the probe's conclusion, ensuring the newly spawned agent process always reads the authentic active account token.
4. **Real-Time Warmup Telemetry & UX Feedback:**
   - [`stats.rs`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/kopeng/stats.rs) emits `keyring-warmup-status` (`{"in_progress": true/false}`).
   - [`dialogueStore.ts`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/stores/dialogueStore.ts) tracks `isKeyringWarmupInProgress`.
   - [`AppHeader.vue`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/AppHeader.vue) renders an animated brutalist indicator `[⚡ WARMING UP]` during background warmup probes.
5. **Project Model Resolution Fallback:**
   - [`flag_resolver.rs`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/kopeng/flag_resolver.rs) dynamically inspects the project's `rula.toml` for declared agent models (e.g. `gemini-3.6-flash`) before falling back to the global default.

---

## 3. Verification
- `cargo check`: Finished in 18.50s with 0 errors.
- `npm run build`: `vue-tsc --noEmit && vite build` completed in 4.01s with 0 errors.
