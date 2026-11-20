# 2026-11-04 — Phase 5 Module 20 (`kopeng/runner.rs` Typed Errors)

Follow-up of `2026-11-04_phase5_orchestrator_errors.md`.

## `RunnerError`
New enum with `Serialize`: `ExecutionMissing`, `Task`, `Registry`, `Script`, `Database`. The 3 `Result<_, String>` are gone, and `RunTaskInput` is removed.

## Fixes
- **Task definition from `rula.toml`:**
  - `run_project_task` took `project_path`, `command`, `args` and `cwd` from the frontend; the MCP tool passed them from a full `hydrate_project`, which includes git status. Now `run_project_task(project_id, task_id, options)` resolves `[scripts.task.<id>]` with `hydration::find_declared_script` / `expand_script_options`, as services do.
  - The two duplicated option expansions are gone. They ignored defaults, accepted undeclared keys, and could push an empty flag for a `select`.
- **One execution path:** `run_project_task` (Tauri, synchronous) and `start_project_task` (MCP, background) share `prepare_task` → `record_start` → `execute`; about 100 duplicated lines are removed.
- **Swallowed record close:** the background path did `let _ = spawn_blocking(|| { let _ = db_finish_task_execution(..) })`. A failed close left the execution `running` forever, and `wait=true` then ran into its timeout. Errors are now returned, and the background task also logs them at `ERROR`.
- **Spawn failure:** it was recorded with the sentinel `exit_code = -1`. It is now recorded as `failed` with no exit code (`db_finish_task_execution` takes `Option<i32>`); the grid shows `NOT STARTED`.
- **MCP `wait=true`:**
  - It polled SQLite synchronously on the async thread with `if let Ok(Some(..))`, ignoring DB errors.
  - It now awaits the background task's `JoinHandle` with `tokio::time::timeout`; on timeout the task keeps running.
  - The default timeout (30000, hardcoded) is the new `mcp.task_wait_timeout_ms`, which replaces the now-unused `mcp.task_wait_poll_step_ms`.
  - `is_error` treated a missing exit code as success (`unwrap_or(0)`).
- **MCP `manage_project_script`:**
  - It no longer hydrates the whole project to find the script; it resolves the declared service directly.
  - A `stop` can no longer target an undeclared `svc` process, such as a user's shell.
  - An unreadable or invalid `rula.toml` is reported as such, where before the script was "not found".
- **Grid status bug:** the grid compared the task status with `'success'`, but records hold `completed` / `failed`. A successful task therefore showed `idle` and a grey LED.

## Frontend
- `ProjectTerminalGrid.vue`: `runTask` sends `projectId` / `taskId` / `options` and is typed with `ScriptEntry` and `TaskExecutionRecord`. It shows `completed` correctly, renders `NOT STARTED` for a null exit code, and uses `unknown` errors.

## Docs
- Spec 01: task commands.
- Spec 12: `start_project_task` (single entry, replacing two contradictory ones) and `manage_project_script`.
- `oneship-core` skill: option defaults also apply to `start_project_task`.
- TODO: runtime check. The `/bin/sh -c` unquoted-args issue of `spawn_service` / `spawn_task` was added to the `supervisor.rs` item.

## Verification
- `cargo check --all-targets` clean.
- `clippy`: 17 warnings, none in `runner.rs`.
- `cargo test --lib`: 95 passed, 2 of them new (task resolved with defaults and `cwd`; services and undeclared IDs are not tasks). The same 2 `kopeng::resolution` failures remain.
- `vue-tsc` clean.
- `Result<…, String>`: 62.
- Not run in the app.
