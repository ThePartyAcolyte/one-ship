# Development Log: 2026-09-16 — Project Initialization Lifecycle, Transient Persona & Flags Engine

## Context & Objectives
1. **First-Class Transient Persona Model (Tech Debt #20):** Resolve missing persona discovery in uninitialized projects. Ensure the platform state engine (`compute_project_state`), the Chat Overlay (`getActiveProjectPath`), and the MCP tool routing (`find_project_for_agent`) recognize the transient `initializer` agent identity regardless of whether `rula.toml` exists on disk.
2. **Supervised Process Decoupling (Tech Debt #20):** Eliminate raw unmanaged `tokio::process::Command::output().await` and raw `git add/commit` commands from `skill.rs` and `migration.rs`. Route initialization through `ProcessSupervisor::spawn_agent` with real-time stdout/stderr ring buffers (500 lines) and asynchronous status observation.
3. **Safe Async Teardown:** Refactor `cancel_project_initialization` to terminate active supervisor processes cleanly via cancellation tokens without executing destructive git reset commands.
4. **Initialization Flags Data Model (Tech Debt #18):** Upgrade `project_initialization_flags` in SQLite and `mcp.rs` with first-class fields `recommended_option` and `options_json`.
5. **Ergonomic Registrable Resolution Tools (Tech Debt #19):** Expose dedicated MCP tools `resolve_todo_item` and `resolve_technical_debt` for atomic completion of staged tasks and debt items.

---

## Architectural & Implementation Details

### 1. Transient Initializer Persona Injection (`src-tauri/src/supervisor.rs`)
- In `compute_project_state`, added dynamic discovery:
  - If `rula.toml` is absent OR `{project_path}:initializer` has an active process in supervisor OR `ProjectInitState` reports `"running"`, injects `AgentState { id: "initializer", role: "Project Initializer", is_internal: true, mcp_dialogue_mode: true, ... }`.
  - Fixes `getActiveProjectPath("initializer")` in `dialogueStore.ts`.
  - Fixes `find_project_for_agent("initializer")` in `mcp.rs`.
- In `prompt_agent`:
  - When target agent's status is `Unlaunched`, `Completed`, or `Failed`, automatically calls `launch_agent` before piping stdin.

### 2. Supervisor-Managed Initialization & Cancellation (`src-tauri/src/kopeng/skill.rs`)
- Removed `tokio::process::Command::new` and blocking `cmd.output().await`.
- Implemented `initialize_project_agent` using `ProcessSupervisor::spawn_agent`:
  - Registers under `{project_path}:initializer`.
  - Attaches standard 500-line ring buffers (`output_ring`).
  - Passes CLI arguments `["--print-timeout", "24h", "-p", prompt]`.
- Implemented Tokio background observer loop:
  - Periodically checks process status (`Completed` vs `Failed`).
  - Verifies presence of `rula.toml` upon zero exit code.
  - Updates `ProjectInitState` in SQLite and records exit status or error logs extracted from the supervisor ring buffer.
- Refactored `cancel_project_initialization`:
  - Asynchronously terminates the running supervisor process via `sup.stop_process(&key).await`.
  - Safely removes transient `rula.toml` if generated, without running destructive `git reset --hard` or `git clean -fd`.

### 3. Strict Git Ban Compliance (`src-tauri/src/kopeng/migration.rs`)
- Replaced raw mutating `git add .` and `git commit -m` checkpoints in `run_deterministic_migration` with read-only head inspection (`get_git_head`).

### 4. Initialization Flags & Resolution Tools (`src-tauri/src/db.rs` & `src-tauri/src/mcp.rs`)
- Updated `project_initialization_flags` table and `ProjectInitFlagRecord` with `recommended_option TEXT` and `options_json TEXT`.
- Non-destructive DB migrations with `ALTER TABLE ... ADD COLUMN ...`.
- Updated `raise_initialization_flag` schema and handler in `mcp.rs` to support `recommended_option` and `options` array.
- Added dedicated MCP tools `resolve_todo_item` and `resolve_technical_debt`.

---

## Verification & Validation
- `cargo check --lib` & `cargo check --bins`: 0 errors.
- `cargo test --lib`: 23 passed (including new tests `test_compute_project_state_uninitialized_has_initializer` and `test_init_flags_crud_with_options`).
- `npm run build`: 0 errors, compiled cleanly with Vite and vue-tsc.
- Resolved Tech Debt #18, #19, and #20 in `TECHNICAL_DEBT.md`.
