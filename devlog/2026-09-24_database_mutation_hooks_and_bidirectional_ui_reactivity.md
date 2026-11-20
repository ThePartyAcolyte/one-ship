# Development Log: 2026-09-24 - Database Mutation Hooks & Bidirectional UI Reactivity

## 1. Problem Statement & Root Cause
During project initialization and flag resolution (specifically the `web_builder` initialization flag for web projects), the `web-builder-resolver` specialist agent executed `resolve_initialization_flag`, and SQLite correctly updated:
- `projects.has_web_builder = 1` and `projects.web_builder_stack = 'react-tailwind-motion'`.
- `project_initialization_flags.status = 'resolved'`.

However, the "Visual Builder" tab did NOT appear immediately in the UI.

### Root Causes
1. **Scattered and Missed UI Event Emissions:**
   - Previous implementations relied on individual callers (such as initializers, MCP handlers, or command endpoints) to remember to emit `projects-updated`.
   - `kopeng/initializer.rs`, `src-tauri/src/registry.rs` (`add_project`, `remove_project`), and `src-tauri/src/mcp.rs` (`execute_resolve_initialization_flag`) all mutated the database without broadcasting `projects-updated`.
   - The frontend's in-memory `coreStore.projects` state remained stale with `has_web_builder: false` until a manual application refresh.
2. **Missing Frontend IPC Listeners:**
   - `src/stores/coreStore.ts` had handling in `handleDaemonEvent` for `init-flag-resolved`, but `initListeners()` lacked Tauri IPC `listen("init-flag-resolved", ...)` and `listen("init-flag-raised", ...)`.
   - `execute_raise_initialization_flag` in `src-tauri/src/mcp.rs` never emitted an `init-flag-raised` event.

---

## 2. Solutions Implemented

### A. Centralized Database Mutation Event Hook Architecture (`src-tauri/src/db.rs`)
Rather than expecting individual endpoints or MCP tools to remember to notify the UI, project and flag change notifications were hooked directly into the database access layer (`db.rs`):
- Created `notify_projects_updated()`:
  - Fetches re-hydrated projects from `crate::registry::get_projects()`.
  - Emits `projects-updated` through `crate::supervisor::get_supervisor().emit_event("projects-updated", ...)`.
- Hooked `notify_projects_updated()` into:
  - `db_add_project`
  - `db_update_project_web_builder`
  - `db_remove_project`
- Enhanced `db_insert_init_flag`:
  - Automatically emits `init-flag-raised` with `flag_id`, `project_id`, and `flag_type`.
- Enhanced `db_resolve_init_flag`:
  - Inspects flag metadata: if the resolved flag is `web_builder` and status is `resolved`, automatically invokes `db_update_project_web_builder(project_id, true, Some("react-tailwind-motion"))`, which automatically triggers `notify_projects_updated()`.
  - Automatically emits `init-flag-resolved`.

### B. Decoupling and Cleanup of MCP Handlers (`src-tauri/src/mcp.rs`)
- Simplified `execute_resolve_initialization_flag` to delegate directly to `crate::db::db_resolve_init_flag(flag_id, status)`.
- Removed duplicated domain queries, manual updates, and redundant manual event emissions.

### C. Frontend IPC Event Listeners (`src/stores/coreStore.ts`)
- In `initListeners()`: Added native Tauri IPC listeners for:
  - `init-flag-resolved`
  - `init-flag-raised`
- In `handleDaemonEvent()`: Bound both `init-flag-raised` and `init-flag-resolved` to trigger `this.fetchInitFlags(p.id)` across all loaded projects, ensuring badges refresh reactively.

---

## 3. Verification
1. **Backend Compilation:** Verified with `cargo check --manifest-path src-tauri/Cargo.toml` (0 errors).
2. **Backend Unit Tests:** Executed `cargo test --manifest-path src-tauri/Cargo.toml` (all 27 tests passed, 0 failures).
3. **Frontend Compilation:** Verified with `npm run build` (`vue-tsc --noEmit && vite build`, 0 errors).
