# 2026-11-01 — Daemon as Required Subsystem, Exact Delegation Scoping, Phase 5 Module 2 (`supervisor.rs`)

Follow-up of `2026-11-01_audit_phase4_backend_correctness_and_phase5_database_errors.md`.

## Maintainer decision recorded
Subsystems are not interchangeable: if the daemon is not running, ONE-SHIP is in an error state. No component substitutes another (the earlier in-process MCP server "fallback when the daemon is absent" was itself a defect).

## Daemon owns telemetry; app requires the daemon
- `duck::TelemetryRole { Owner, Client { daemon_port } }`, set once per process. The daemon sets `Owner`, opens DuckDB (failure aborts startup) and runs the continuous profiler. The app sets `Client` and never opens the file.
- Removed the silent in-memory DuckDB fallback; `init_duckdb_memory` is test-only. A legacy-schema `DROP` failure is now an error.
- New daemon endpoint `GET /api/telemetry/continuous-profile` (operator guard). `get_continuous_profile_cmd`, `query_profiler_hotspots_cmd` (now async) and `execute_profiling_session` use `duck::query_continuous_profile_summary` on the blocking pool; the app side calls the daemon with `ureq` (already in the lock, added as a direct dependency with default features off) and timeout `mcp.daemon_request_timeout_ms` (5000, new schema key). Telemetry errors propagate; profiling no longer silently switches to a driver run.
- Removed `start_in_process_mcp_daemon` and its spawn; the app no longer syncs the global `agy` MCP config (the daemon does). `init_database` failure aborts app startup (it previously printed a warning and continued).
- Frontend: `get_daemon_port_cmd` is the single source for daemon URLs; `coreStore.daemonStatus` (`connecting/connected/unreachable/build_mismatch`) drives a red `ServerCrash` header indicator with the detail in its tooltip. Pending approvals are hydrated only from SQLite; `/api/pending-approvals` and its handler were deleted.

## Delegation lookup scoped exactly
- `db_get_pending_delegation_by_target(agent, project_path: &str)`: project mandatory, exact path match (no "any project", no directory-name match). All stored `target_project` values are full paths.
- `report_task_status`: an explicit `task_id` is authoritative (no fallback to "newest delegation to this agent"); lookup errors are returned to the agent instead of `unwrap_or(None)`.

## Phase 5 — module 2: `supervisor.rs`
- `SupervisorError` (`thiserror`): `ProcessNotRunning`, `ProjectNotRegistered`, `Spawn`, `MissingPid`, `PortOccupied`, `AccountUnavailable`, `InputDelivery`, `InputQueueFull`, `InvalidRequest`, `Database`. All internal functions (`reconcile_orphan_processes`, `resolve_spawn_identity`, `send_input`, `dispatch_next_pending_input`, `stop_process`, `spawn_service/task/agent`, preamble/context queueing, `resolve_system_working_dir`) return it; the 13 Tauri commands keep `String` at the IPC boundary. Callers in `orchestrator.rs`, `rea.rs`, `flag_resolver.rs` adapted.

## Verification
`cargo check`/`clippy` clean (35 pre-existing style warnings), `cargo test` 19 passed (db, conversation, logger, duck, supervisor), `vue-tsc` clean. Runtime checks pending: TODO "Audit 2026-11-01 follow-up".

## Specs updated
12 §6/§7 (daemon required, operator API routes, port source), 25 §3.3 (telemetry ownership), 10 (`mcp.daemon_request_timeout_ms`).
