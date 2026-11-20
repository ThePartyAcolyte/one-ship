# Phase 3 — Medium Priority Findings
**Audit date:** 2026-09-23 | **Execute after Phase 2 is complete**

Technical debt: hardcoded values that should be user-configurable settings, code duplication, and insufficient logging.
These don't cause crashes but degrade maintainability and user control.

---

## M-01 · Hardcoded values not exposed in Settings
**Problem:** The Settings modal iterates over `get_all_plugin_schemas()`. Because the following values are hardcoded constants (not `PluginConfigSchema` entries), they are completely invisible to the user.

**Required action:** Add each to the relevant config schema in `config.rs` so they render automatically in `SettingsModal.vue`.

### Backend — Rust (`src-tauri/`)

| File | Line | Current Value | Suggested Setting Key |
|------|------|---------------|----------------------|
| `src-tauri/src/db.rs` | ~45 | `max_size(4)` — SQLite pool size | `db_connection_pool_max_count` |
| `src-tauri/src/logger.rs` | top | `MAX_LOG_SIZE_BYTES = 512_000` | `log_max_size_bytes` |
| `src-tauri/src/logger.rs` | top | `MAX_ROTATIONS = 2` | `log_max_rotations_count` |
| `src-tauri/src/kopeng/profiler.rs` | 476 | `from_millis(500)` — profiler tick | `profiler_sample_interval_ms` |
| `src-tauri/src/kopeng/stats.rs` | 1120 | `from_secs(60)` — stats refresh | `stats_refresh_interval_seconds` |
| `src-tauri/src/bin/daemon.rs` | 38 | `port = 8765` — MCP daemon port | `mcp_daemon_port` |
| `src-tauri/src/kopeng/initializer.rs` | 317 | `from_millis(250)` — spawn poll | `agent_spawn_poll_step_ms` |
| `src-tauri/src/kopeng/initializer.rs` | 330 | `from_millis(500)` — spawn poll | `agent_spawn_poll_step_ms` |
| `src-tauri/src/kopeng/initializer.rs` | 417 | `read_output(..., 50)` — log tail | `agent_log_tail_lines_count` |
| `src-tauri/src/kopeng/flag_resolver.rs` | 201 | `from_millis(250)` — spawn poll | `agent_spawn_poll_step_ms` |
| `src-tauri/src/kopeng/fs_tree.rs` | 11 | `if depth > 12` — tree depth | `fs_tree_max_depth_count` |
| `src-tauri/src/supervisor.rs` | 1036 | `from_millis(150)` — shutdown step | `shutdown_step_delay_ms` |
| `src-tauri/src/supervisor.rs` | 1533,1537,1844 | `100ms` process poll delays | `process_poll_step_ms` |
| `src-tauri/src/watcher.rs` | 37 | `from_millis(1500)` — debounce | `watcher_debounce_ms` |
| `src-tauri/src/mcp.rs` | — | `from_millis(500)` — approval poll | `approval_poll_delay_ms` |
| `src-tauri/src/mcp.rs` | — | `"ttlMs": 15000` in responses | `approval_ttl_ms` |

### Frontend — TypeScript/Vue (`src/`)

| File | Value | Suggested Setting Key |
|------|-------|-----------------------|
| `src/App.vue` | `diagnosticTimer = 8000ms` | `diagnostic_poll_interval_ms` |
| `src/App.vue` | Theme sync `5000ms` | `theme_sync_interval_ms` |
| `src/kopeng/agents/components/AgentSection.vue:164` | `elapsed < 6000` | `agent_status_decay_ms` |
| `src/kopeng/services/components/ScriptsSection.vue` | `durationS: 15` — profiler | `profiler_default_duration_seconds` |
| `src/kopeng/services/components/ScriptsSection.vue` | `checkIntervalMs = 2000` | `service_check_interval_ms` |
| `src/stores/dialogueStore.ts:339` | Panel width default `460`, cap `850` | `chat_panel_default_width_px` |
| `src/components/modals/AddProjectModal.vue` | `/home/butcherwutcher/Projects/dev` | `default_projects_root_path` |
| `src/components/workspace/ProjectVisualBuilderTab.vue:1378` | `setInterval(fetchDevLogs, 2500)` | `visual_builder_log_poll_interval_ms` |
| `src/components/workspace/ProjectVisualBuilderTab.vue:131` | `undoStack.value.length > 50` — **partially resolved** (devlog 2026-09-23): `max_visual_builder_undo_snapshots_count` is registered in `rula.toml` and the SQLite `visual_builder_snapshots` pruning reads it. **Remaining gap:** the in-memory `undoStack` in the frontend still hardcodes `50` and does not read the config at mount time. Fix: call `get_global_config` on mount and use the returned value instead. |

---

## M-02 · stdout/stderr reader loop duplicated 3× in `supervisor.rs`
**File:** `src-tauri/src/supervisor.rs`  
**Problem:** The Tokio IO reader task (ring buffer writes from stdout/stderr) is copy-pasted nearly identically across:
- `spawn_service` (lines 343–415)
- `spawn_task` (lines 523–596)
- `spawn_agent` (lines 799–855)

**Required fix:** Extract to a shared helper:
```rust
fn spawn_io_monitors(
    key: String,
    stdout: ChildStdout,
    stderr: ChildStderr,
    ring_buf: Arc<Mutex<VecDeque<String>>>,
) -> (JoinHandle<()>, JoinHandle<()>) { ... }
```

---

## M-03 · Config resolution logic duplicated in initializer + flag_resolver
**Files:** `src-tauri/src/kopeng/initializer.rs:109-146`, `src-tauri/src/kopeng/flag_resolver.rs:110-145`  
**Problem:** The fallback resolution logic for `effective_runner`, `effective_model`, and `effective_effort` is copied near-identically between both files.

**Required fix:** Extract to a shared function in `kopeng/mod.rs` or a new `kopeng/resolution.rs`:
```rust
pub fn resolve_effective_agent_config(
    project_config: &ProjectConfig,
    global_config: &GlobalConfig,
) -> EffectiveAgentConfig { ... }
```

---

## M-04 · DB error handling boilerplate repeated ~100× in `db.rs` and `duck.rs`
**Files:** `src-tauri/src/db.rs`, `src-tauri/src/duck.rs`  
**Problem:** Every single database operation manually wraps errors with the same pattern:
```rust
.map_err(|e| format!("Failed to {operation}: {}", e))?
```
This appears hundreds of times across 3251 lines.

**Required fix — extension trait or macro:**
```rust
trait DbResultExt<T> {
    fn db_context(self, op: &str) -> Result<T, KopengError>;
}

impl<T> DbResultExt<T> for rusqlite::Result<T> {
    fn db_context(self, op: &str) -> Result<T, KopengError> {
        self.map_err(|e| KopengError::Database { operation: op.to_string(), source: e })
    }
}

// Usage:
conn.execute(...).db_context("insert project")?;
```
Requires H-01 (typed errors) to be completed first.

---

## M-05 · `migration.rs` — only 1 aggregate log for N migration steps
**File:** `src-tauri/src/kopeng/migration.rs`  
**Problem:** The migration loop applies N steps but only logs a single aggregate success message at the very end. If the migration partially applies (e.g., steps 1–3 succeed, step 4 fails), the log provides no useful context.

**Required fix:** Log each step inside the execution loop:
```rust
for (i, step) in steps.iter().enumerate() {
    crate::logger::log("DEBUG", "migration", &format!(
        "Applying step {}/{}: {}", i + 1, steps.len(), step.description
    ));
    step.apply(&conn)?;
    crate::logger::log("INFO", "migration", &format!(
        "Step {}/{} applied: {}", i + 1, steps.len(), step.description
    ));
}
```

---

## M-06 · `contentWindow.postMessage` bridge inline in `ProjectVisualBuilderTab.vue`
**File:** `src/components/workspace/ProjectVisualBuilderTab.vue`  
**Problem:** Direct `contentWindow.postMessage(...)` calls are scattered throughout the component (e.g., line 532). This is a leaky abstraction — the iframe bridge protocol is mixed with UI logic.

**Required fix:** Extract to `src/composables/useOnlookClientBridge.ts`:
```typescript
export function useOnlookClientBridge(iframeRef: Ref<HTMLIFrameElement | null>) {
  function sendMessage(type: string, payload: unknown) {
    iframeRef.value?.contentWindow?.postMessage({ type, payload }, '*');
  }
  return { sendMessage };
}
```

---

## M-07 · `runner::run_project_task` registered `#[tauri::command]` but not in `invoke_handler`
**File:** `src-tauri/src/kopeng/runner.rs`  
**Problem:** `run_project_task` has `#[tauri::command]` annotation but is not registered in the `invoke_handler` in `lib.rs`. It is unreachable dead code with a misleading annotation.

**Required action (choose one):**
1. Register it in `lib.rs` `invoke_handler` if it should be callable from the frontend.
2. Remove the `#[tauri::command]` annotation and make it a plain internal function if it is backend-only.

---

## M-08 · Hardcoded Tailwind class tokens in `ProjectVisualBuilderTab.vue`
**File:** `src/components/workspace/ProjectVisualBuilderTab.vue`  
**Problem:** The same active-state class string is copy-pasted dozens of times:
```html
:class="isActive ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/20 text-white font-bold'
                 : 'border-[var(--border-color)] text-[var(--dim-color)] hover:border-white'"
```

**Required fix:** Extract to a computed helper or CSS utility class:
```typescript
const activeTabClasses = computed(() =>
  isActive.value
    ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/20 text-white font-bold'
    : 'border-[var(--border-color)] text-[var(--dim-color)] hover:border-white'
);
```
Or define `.tab-active` / `.tab-inactive` utility classes in the global stylesheet.
