# Phase 1 — Critical Findings
**Audit date:** 2026-09-23 | **Must resolve before next release**

These findings carry active crash risk, memory leak potential, or data integrity risk.
Tackle in this order: stability first (items 1-2), then data integrity (item 3), then observability (item 4), then type safety (item 5).

---

## C-01 · 44 `unwrap()` on RwLock in `supervisor.rs`
**File:** `src-tauri/src/supervisor.rs`  
**Risk:** If any RwLock becomes poisoned (e.g., a thread panics while holding the lock), the daemon crashes with no diagnostic output.

**All affected lines:**
```
163, 168, 179, 184, 186, 193, 195, 199, 208, 213, 218, 226, 235, 241,
262, 276, 285, 385, 414, 451, 465, 469, 550, 557, 598, 605, 645, 672,
688, 692, 693, 1060, 1090, 1137, 1151, 1155, 1181, 1243, 1290, 1352,
1392, 1668, 2019, 2049
```

All are `RwLock` unwraps of the form `self.event_tx.write().unwrap()`.

**Required fix:**
Replace every `.unwrap()` with a descriptive `.expect()`:
```rust
// Before
self.processes.write().unwrap()

// After
self.processes.write().expect("supervisor: processes RwLock poisoned")
```
For cases where propagation is appropriate, use `.map_err(|_| KopengError::LockPoisoned("processes"))` once the typed error enum is in place (see Phase 2, H-01).

**Also in `mcp.rs`:** `.lock().unwrap()` on `approval_map` in multiple MCP handlers. If poisoned, the entire MCP daemon crashes, severing all agent communication.

---

## C-02 · 22+ Tauri event listeners with no `unlisten()` cleanup
**Files:** `src/stores/coreStore.ts`, `src/stores/dialogueStore.ts`, `src/App.vue`  
**Risk:** Memory leak on hot reload, account switch, or store re-initialization. Stale listeners fire on destroyed component state.

**`coreStore.ts` — `initTauriListeners()`:**
Registers 13+ listeners via `await listen(...)`. The returned `UnlistenFn` functions are discarded:
```typescript
// Current (leaks)
await listen('ProjectsUpdated', (event: any) => { ... });

// Required fix
const unlisteners: UnlistenFn[] = [];
unlisteners.push(await listen('ProjectsUpdated', (event) => { ... }));
// ... all 13 listeners
// Store must expose a teardown() that calls unlisteners.forEach(fn => fn())
```

`coreStore.ts` also opens an `EventSource` to `http://127.0.0.1:8765/api/events` with no `es.close()` path on re-initialization:
```typescript
// Line ~268 — no teardown path outside error cases
const es = new EventSource("http://127.0.0.1:8765/api/events");
```

**`dialogueStore.ts` — `initListener()`:**
Registers 9 listeners (lines 439–527). All `unlisten` promises ignored.

**`App.vue` — line 502:**
```typescript
window.addEventListener('switch-to-workspace-terminal', onSwitchToWorkspaceTerminal);
// Missing: onUnmounted(() => window.removeEventListener(...))
```

**Required pattern for all stores:**
```typescript
const _unlisteners: UnlistenFn[] = [];

export function initListeners() {
  // register all listeners, push unlisten fns to array
}

export function teardown() {
  _unlisteners.forEach(fn => fn());
  _unlisteners.length = 0;
}
```
Call `teardown()` from `App.vue` `onUnmounted` or before store re-initialization.

---

## C-03 · N+1 writes without SQL transaction in SQLite migration
**File:** `src-tauri/src/db.rs`, lines 135–149  
**Risk:** With N projects, N separate atomic writes. No atomicity: a mid-migration crash leaves the registry in a partially migrated state with no rollback.

**Pattern (current):**
```rust
for p in legacy_projects {
    conn.execute("INSERT OR IGNORE INTO projects ...", params![...])?;
    // no transaction wrapping
}
```

**Required fix — wrap in explicit transaction:**
```rust
let tx = conn.transaction()?;
for p in &legacy_projects {
    tx.execute("INSERT OR IGNORE INTO projects ...", params![...])?;
}
tx.commit()?;
```

Note: `profiler_hotspots` batch inserts at line 2440 do this correctly with `tx.prepare()`. Use that as the reference pattern.

---

## C-04 · Catastrophically low log coverage in critical files
**Risk:** The system is completely opaque to diagnosis. When failures occur in production, there is no trace to follow.

| File | Lines | Log statements | Coverage |
|------|-------|----------------|----------|
| `src-tauri/src/supervisor.rs` | 2054 | 15 | ~0.7% |
| `src-tauri/src/db.rs` | 3251 | 2 | ~0.06% |
| `src-tauri/src/telemetry.rs` | 393 | 1 | ~0.25% |
| `src-tauri/src/migration.rs` | ~150 | 1 (aggregate only) | — |

**Required additions — `supervisor.rs`:**
- `spawn_service` (line ~257): `DEBUG` log with exact command + args before spawn
- stdout/stderr reader loops (lines ~390): `WARN` when IO pipe breaks unexpectedly
- `spawn_agent_state_watcher` (line ~1869): `DEBUG` on each state transition

**Required additions — `db.rs`:**
- Every query function: `DEBUG` log at entry with operation name and key parameters
- Slow query detection: log `WARN` if any operation exceeds a threshold

**Required additions — `telemetry.rs`:**
- `spawn_continuous_profiler_loop`: `INFO` on startup, `DEBUG` on each interval tick

**Required additions — `migration.rs`:**
- Log each individual migration step completion inside the execution loop, not just the final aggregate

---

## C-05 · Pervasive `any` type nullifies TypeScript safety
**Risk:** Tauri IPC payloads are entirely untyped. A backend schema change causes silent runtime breakage with no compile-time warning.

**Most critical instances in `coreStore.ts`:**
```typescript
// Line 64–67 — core state untyped
projects: [] as any[]
agentStates: {} as Record<number, any>
systemAgentState: null as any
projectInitFlags: {} as Record<number, any[]>

// Lines 175–227 — ALL Tauri event payloads
await listen('ProjectsUpdated', (event: any) => { ... })

// Lines 235, 335, 345, 413, 462 — all invoke calls
await invoke<any[]>("get_pending_approvals_cmd")
const cfg: any = await invoke("get_global_config")
```

**`dialogueStore.ts`:** `any` at lines 255, 439, 446, 465, 489, 501, 521, 650, 735, 1109.

**All modals and components:** `project` prop typed as `any` universally.
- `ProjectCard.vue`: `project: any`, `agentState?: any`, `flag: any`
- `TaskReportModal.vue`: `project: any`, `executions: any[]`, `selectedExec: any | null`
- `AgentSection.vue`: `context?: any`
- `ScriptsSection.vue`: `context?: any`

**Required fix:**
Create `src/types/index.ts` with interfaces for all core domain objects:
```typescript
export interface Project { id: number; name: string; path: string; ... }
export interface AgentState { status: string; pid?: number; ... }
export interface InitFlag { flag_type: string; resolved: boolean; ... }
export interface AgentSessionMetrics { used_percent: number; remaining_percent: number; ... }
```
Replace all `any` usage with the appropriate interface. Tauri's `invoke<T>()` generic should carry the correct type at each call site.
