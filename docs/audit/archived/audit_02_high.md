# Phase 2 — High Priority Findings
**Audit date:** 2026-09-23 | **Execute after Phase 1 is complete**

These are AGENTS.md governance violations, significant anti-patterns, and spec/implementation contradictions.
They do not cause immediate crashes but accumulate into structural debt that blocks clean feature development.

---

## H-01 · `kopeng/errors.rs` contains no typed errors
**File:** `src-tauri/src/kopeng/errors.rs`  
**Problem:** The file contains zero struct or enum definitions and no `std::error::Error` implementations. It is a single utility function `extract_semantic_errors`. Every kopeng module returns `Result<T, String>` — primitive string errors with no structure, no pattern matching, no typed recovery.

**Required fix:**
Adopt `thiserror` and define a `KopengError` enum:
```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum KopengError {
    #[error("Lock poisoned: {context}")]
    LockPoisoned { context: &'static str },
    #[error("Project path does not exist: {path}")]
    ProjectNotFound { path: String },
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("IO error in {context}: {source}")]
    Io { context: &'static str, #[source] source: std::io::Error },
    // ... extend as needed
}
```
Replace all `Result<T, String>` with `Result<T, KopengError>` progressively. This also enables typed error logging before propagation.

---

## H-02 · God store: `dialogueStore.ts` (~1500 lines, 6 domains)
**File:** `src/stores/dialogueStore.ts`  
**Problem:** Violates single responsibility principle. One store manages:
1. Active conversations and dialogue artifacts
2. Agent status telemetry and live steps
3. Devlog journals, Todos, and Technical Debt items
4. Token economy, quota polling, and cached metrics
5. File system explorer state
6. UI window layout (`activeWorkspaceTab`, `chatLayoutMode`)

**Also affected:** `coreStore.ts` (~580 lines) mixes system telemetry, project lists, Google authentication, and desktop window IPC.

**Required decomposition:**
```
coreStore.ts        → keep only: app bootstrap, global UI state
useProjectStore.ts  → project list, init flags
useAccountStore.ts  → multi-account, Google auth
useQuotaStore.ts    → token economy, quota polling, metrics caching
useFileExplorerStore.ts → fs tree state, document comments
useDevlogStore.ts   → journals, todos, technical debt
```
`dialogueStore.ts` should retain only: active conversation, artifact panel, chat layout.

---

## H-03 · Blocking I/O in async Tauri command handlers
**Files:** `src-tauri/src/kopeng/initializer.rs`, `src-tauri/src/kopeng/flag_resolver.rs`, `src-tauri/src/kopeng/runner.rs`  
**Problem:** Async command handlers perform synchronous file I/O and SQLite operations directly, blocking the Tokio runtime thread.

**Affected call sites:**
- `initializer::initialize_project_agent` (async): calls `prepare_project_agents_md` (file I/O) and `db_upsert_init_state` (SQLite) directly
- `flag_resolver::launch_flag_resolution_agent` (async): calls `prepare_project_agents_md` and `inject_project_mcp_plugin` synchronously
- `runner::run_project_task` (async): calls `db_insert_task_execution` synchronously

**Required fix:**
Wrap all blocking operations with `tokio::task::spawn_blocking`:
```rust
let result = tokio::task::spawn_blocking(move || {
    prepare_project_agents_md(&path)
}).await??;
```

---

## H-04 · Spec 03 (Tray Hub) and Spec 02 (Error Curator) — No implementation
**Files:** `docs/specs/03_tray_hub_interface.md`, `docs/specs/02_diagnostic_laboratory.md`  
**Problem:** Both specs describe fully designed features that have zero implementation in the codebase.

- **Spec 03 (Tray Hub):** Defines a "compact floating window" with tray icon. No tray setup exists in `main.rs`, `lib.rs`, or the frontend.
- **Spec 02 (Error Curator):** Defines a background LLM prompt triggered on service crashes. `supervisor.rs:469` only sets `ProcessStatus::Failed` with no further action.

**Required action:**
Each spec needs an explicit status header. Until implementation begins, add to both specs:
```markdown
> **Status: DEFERRED** — Feature specified but not yet implemented.
> Implementation tracker: [link to TODO item]
```
This prevents new agents from assuming these features exist.

---

## H-05 · Spec 06 (Plugin Architecture) contradicts `lib.rs` reality
**Files:** `docs/specs/06_plugin_architecture.md`, `src-tauri/src/lib.rs`  
**Problem:** Spec 06 declares "each plugin must implement a standardized Rust interface (`trait Plugin`) to register its Tauri commands." No such trait exists. `lib.rs:31-155` is a monolithic block hardcoding ~100 command registrations.

**Required action (choose one and document the decision):**
1. **Implement the trait:** Define `trait KopengPlugin` with a `register_commands()` method, migrate existing modules to implement it.
2. **Revise the spec:** Update Spec 06 to reflect the current intentional design (centralized registration) and remove the trait requirement.

Either way, the contradiction between spec and code must be resolved.

---

## H-06 · Spec 20 — MCP tool `switch_conversation_topic` missing
**File:** `src-tauri/src/mcp.rs`  
**Problem:** Spec 20 §5.2 defines the MCP tool `switch_conversation_topic` for transitioning between discrete conversations without token bloat. The tool is not registered in `mcp.rs`. Agents cannot programmatically execute this workflow.

**Required fix:** Implement and register `switch_conversation_topic` in `mcp.rs` following the same pattern as `post_agent_response`. Wire to the appropriate `db.rs` / `conversation.rs` functions.

---

## H-07 · Tauri `invoke()` error handling duplicated in 30+ places
**Files:** `src/kopeng/agents/components/AgentSection.vue`, `src/kopeng/services/components/ScriptsSection.vue`, `src/components/modals/SnapshotTimelineModal.vue`, and 27+ other files  
**Problem:** Identical error handling boilerplate copied verbatim across every component:
```typescript
// Repeated 30+ times verbatim
try {
  const res = await invoke('some_command', args);
  // ...
} catch (err: any) {
  console.error('some_command failed:', err);
  addStatusLog('Error: ' + err);
}
```

**Required fix:**
Create `src/composables/useTauriCommand.ts`:
```typescript
export async function useTauriCommand<T>(
  cmd: string,
  args?: Record<string, unknown>
): Promise<T | null> {
  try {
    return await invoke<T>(cmd, args);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    console.error(`[tauri:${cmd}] ${message}`);
    addStatusLog(`Error in ${cmd}: ${message}`);
    return null;
  }
}
```

---

## H-08 · `AppInput` and `AppTooltip` violations in modals
**Problem:** Spec 13 explicitly forbids raw `<input>` and `title="..."` attributes. Both are in active violation across multiple modals.

**Raw `<input>` instead of `<AppInput>` — locations:**
| File | Lines |
|------|-------|
| `src/components/modals/ConfirmDeleteModal.vue` | 145 |
| `src/components/modals/GithubRemoteModal.vue` | 215, 283, 295, 299 |
| `src/components/modals/VcsActionModal.vue` | 149 |
| `src/components/modals/SettingsModal.vue` | 342 |
| `src/components/modals/SnapshotTimelineModal.vue` | 269 |
| `src/components/workspace/ProjectVisualBuilderTab.vue` | 1414, 1762, 1791, 1825, 1846, 1874, 1886, 1899, 1920, 2183, 2189, 2206 |

**Raw `title="..."` instead of `<AppTooltip>` — locations:**
| File | Lines |
|------|-------|
| `src/components/modals/GithubRemoteModal.vue` | 132, 148 |
| `src/components/modals/VcsActionModal.vue` | 86, 101, 109 |
| `src/components/modals/SnapshotTimelineModal.vue` | 354, 377, 446 |
| `src/components/workspace/AppImageGalleryModal.vue` | 244, 251 |
| `src/components/workspace/ProjectVisualBuilderTab.vue` | 1768, 1797, 1831, 1852, 1880, 1892, 1905, 1926, 2070, 2313 |
| `src/components/dialogue/FloatingChatOverlay.vue` | (various) |
| `src/components/workspace/ProjectGitSnapshotsTab.vue` | (various) |
