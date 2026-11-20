# Phase 4 — Low Priority Findings
**Audit date:** 2026-09-23 | **Execute after Phase 3, or opportunistically alongside**

Housekeeping: Tiger Style naming, documentation gaps, dead code, and minor i18n/design system issues.
These do not block progress but accumulate into readability and maintainability debt.

---

## L-01 · Tiger Style — Missing unit suffixes on numeric variables
**Rule (AGENTS.md §8.1):** All numeric variables must include explicit unit suffixes.

### Rust violations

| File | Variable | Current | Required |
|------|----------|---------|----------|
| `src-tauri/src/telemetry.rs:127` | `total_mem` | `u64` | `total_mem_bytes` |
| `src-tauri/src/telemetry.rs:350` | `mem_mb` | `u64` | `mem_mbytes` |
| `src-tauri/src/supervisor.rs:341` | `buffer_capacity` | `usize` | `buffer_lines_count` |
| `src-tauri/src/db.rs:680` | `limit` (query param) | `usize` | `limit_count` |
| `src-tauri/src/kopeng/stats.rs:1120` | `let mut interval` | `Duration` | `let mut refresh_interval` (or use named constant `STATS_REFRESH_INTERVAL_SECONDS`) |
| DB schema column | `memory_allocated_mb` in `profiler_hotspots` table | — | `memory_allocated_mbytes` |

### TypeScript violations

| File | Variable | Current | Required |
|------|----------|---------|----------|
| `src/components/workspace/ProjectVisualBuilderTab.vue` | `devPort` | `number` | `devPortNumber` |
| `src/components/workspace/ProjectVisualBuilderTab.vue:211` | `opacityVal` | `number` | `opacityPercent` |
| `src/components/workspace/ProjectVisualBuilderTab.vue` | magic `2500` in setInterval | inline | `const POLL_INTERVAL_MS = 2500` |

---

## L-02 · Dead code — functions defined but never called

| File | Symbol | Status |
|------|--------|--------|
| `src-tauri/src/config.rs:624` | `pub fn resolve_effective_bool_with_default` | Never called anywhere. Remove or use. |
| `src-tauri/src/kopeng/opendictate.rs` | `pub fn spawn_dbus_monitor` | Defined as `pub`, never called from `lib.rs` or `daemon.rs`. Either register or make private. |
| `src-tauri/src/kopeng/errors.rs` | `extract_semantic_errors` | Only called in its own test block. Move to `#[cfg(test)]` module or remove. |

---

## L-03 · Unbounded loops — Tiger Style §4 violations
**Rule (AGENTS.md §8.4):** Every loop must have explicit upper bounds or deterministic timeout.

| File | Line | Loop type | Required fix |
|------|------|-----------|-------------|
| `src-tauri/src/kopeng/initializer.rs` | 329 | `loop {}` polling process status | Add `max_attempts_count` + break on timeout |
| `src-tauri/src/kopeng/opendictate.rs` | 59 | `loop {}` reading DBus indefinitely | Add shutdown signal receiver |
| `src-tauri/src/supervisor.rs` | 1869 | `loop` in `spawn_agent_state_watcher` | Add `max_attempts_count` safeguard |
| `src-tauri/src/watcher.rs` | 32 | File watcher `loop` | Add explicit termination via cancellation token |
| `src-tauri/src/telemetry.rs` | 177, 305 | Background telemetry loops | Add bounds safety check |

---

## L-04 · `Cargo.toml` — suspicious dependency version
**File:** `src-tauri/Cargo.toml`  
**Problem:** `toml = "1.1.4"` — the `toml` crate on crates.io does not have a `1.x` release. Current stable is `~0.8.x`. This version string is either a typo, a fork, or will fail to resolve from the public registry.

**Action:** Verify with `cargo tree | grep toml` and correct to the actual intended version.

**Also:** Consider pinning minor versions on critical dependencies:
```toml
# Current (allows any patch)
serde = "1"
tauri = "2"

# More predictable (allows only compatible patches)
serde = "1.0"
tauri = "2.0"
```

---

## L-05 · Deep nesting without guard clauses — Tiger Style §5 violations
**Rule (AGENTS.md §8.5):** Limit indentation to ≤2 levels. Use early returns via guard clauses.

| File | Lines | Depth | Description |
|------|-------|-------|-------------|
| `src-tauri/src/hydration.rs` | 82–195 | 7 levels | Configuration parsing loop |
| `src-tauri/src/kopeng/manifest.rs` | 207–232 | 5 levels | `purge_kapawu_artifacts` |

Both should be refactored with early returns and extracted helper functions to stay within 2 levels.

---

## L-06 · Documentation — critical public types with zero doc comments
**Affected types in `supervisor.rs`:**
```rust
// All of these need /// doc comments:
pub enum ProcessStatus { ... }       // state machine values undocumented
pub struct AgentState { ... }        // fields undocumented
pub struct ProcessSupervisor { ... } // no overview of lifecycle
pub fn spawn_agent_state_watcher(...)// complex state machine, no description
```

**Affected in `config.rs`:**
```rust
pub struct ConfigFieldSchema { ... }        // no field documentation
pub fn get_effective_project_config(...) { } // no description of cascade logic
```

**Affected in `telemetry.rs`:**
```rust
pub struct SystemMetrics { ... } // completely undocumented
```

**Standard to follow (from `stats.rs` — positive reference):**
```rust
/// Canonical quota pool resolver. Returns the active Gemini quota snapshot
/// for a given agent session, merging live counters with the 5-hour rolling window.
pub fn resolve_quota_pool(session_id: &str) -> Option<QuotaSnapshot> { ... }
```

---

## L-07 · `v-for` with index as key on dynamic list
**File:** `src/components/workspace/ProjectVisualBuilderTab.vue:1694`  
**Problem:**
```html
<div v-for="(line, idx) in devLogs" :key="idx">
```
Using array index as `:key` on a list that updates dynamically causes Vue to reuse DOM nodes incorrectly when items are inserted or removed from the beginning.

**Fix:** Use a stable unique identifier per log line (e.g., timestamp + sequence number), or if lines are only ever appended, use a monotonic counter assigned at insertion time.

---

## L-08 · Hardcoded color tokens bypassing design system in Visual Builder
**File:** `src/components/workspace/ProjectVisualBuilderTab.vue`  
**Problem:** Direct Tailwind color classes used where CSS design tokens should be applied:
- `text-amber-300` — status indicator color
- `border-emerald-500/60` — success state border
- `bg-rose-950/20` — error state background

These bypass the global `var(--...)` token system mandated by Spec 13, making theme changes require editing component source code.

**Fix:** Map semantic states to CSS variables:
```css
/* In global stylesheet */
--status-warn-color: theme('colors.amber.300');
--status-ok-color: theme('colors.emerald.500');
--status-error-bg: theme('colors.rose.950' / 20%);
```
Then use `text-[var(--status-warn-color)]` etc. in templates.

---

## L-09 · i18n — User-facing strings hardcoded in English
**File:** `src/App.vue` and most modals  
**Problem:** The `vue-i18n` system is initialized in `src/i18n.ts` but bypassed by the majority of dynamic user-facing text.

**Examples from `App.vue`:**
```typescript
addStatusLog("Error adding project: " + e);   // not i18n
addStatusLog(`Created greenfield project: ${project?.name}`); // not i18n
addStatusLog("Saved global settings (~/.config/one-ship/config.toml)"); // not i18n
```

Modal titles across the codebase are hardcoded English strings instead of `t('modal.addProject.title')`.

**Note:** Given the developer-tool nature of the app (AGENTS.md §2 mandates English for all UI), i18n may be intentionally scoped. If the decision is English-only, remove `i18n.ts` and the `vue-i18n` dependency entirely to avoid the misleading infrastructure. If multilingual support is planned, systematically replace hardcoded strings.

**Action required:** Explicitly decide and document the i18n strategy, then either remove the dead infrastructure or complete the implementation.
