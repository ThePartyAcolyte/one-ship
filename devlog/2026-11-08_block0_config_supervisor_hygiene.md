# 2026-11-08 — Block 0: Config Schema Hygiene, Supervisor Exact Scoping & Canonical Task Keys

## Summary
Completed Block 0 of the post-profiler remediation roadmap before starting Phase 6 (Frontend Store Split & Typed SSE Events):

1. **Global Config Cleanup & Log Level Adjustment (0.1):**
   - Removed the 5 obsolete keys (`telemetry.retention_window_hours`, `telemetry.prune_interval_minutes`, `profiler.default_driver`, `profiler.max_hotspots_count`, `profiler.min_cpu_threshold_percent`) from `~/.config/one-ship/config.toml` that had been causing `InvalidGlobalConfig` startup warnings since the profiler redesign.
   - Downgraded `db_upsert_account_quota` completion log in `src-tauri/src/db.rs` from `INFO` to `DEBUG` to eliminate periodic log noise.

2. **`rula_schema.md` & Reserved Agent ID Validation (0.2):**
   - Made `kopeng::persona::RESERVED_AGENT_IDS` (`system`, `git`, `initializer`, `removal-resolver`, `config-resolver`) `pub(crate)` and enforced rejection of reserved IDs inside `config::agent_persona_problems`.
   - Updated `supervisor::compute_project_state` so `git` resolves its runner/model/auto_approve through the plugin cascade (`None` agent name) and reserved IDs in `[agents]` are never hydrated as user personas.
   - Updated `templates/skills/oneship-core/references/rula_schema.md` (removed `[plugins.herdr]`, listed only manifest runners `agy`, `claude`, `opencode`, documented `profiler = "<id>"` under persistent scripts) and `docs/specs/10_configuration_schema_and_cascade.md`.

3. **Single Source of Truth for `resolve_effective_u64` & `resolve_effective_usize` Defaults (0.3):**
   - Removed the redundant `default_value` parameter from `config::resolve_effective_u64` and `config::resolve_effective_usize` in `src-tauri/src/config.rs`, adding a `debug_assert!` that every queried `(plugin_id, key)` exists in `get_all_plugin_schemas()`.
   - Updated all 35 call sites across `config.rs`, `db.rs`, `supervisor.rs`, `mcp.rs`, `telemetry.rs`, `duck.rs`, `logger.rs`, `conversation.rs`, `watcher.rs`, `brief.rs`, `caller_identity.rs`, `orchestrator.rs`, and `kopeng/{diagnostic,flag_resolver,fs_tree,github,initializer,rea}.rs`, removing duplicate caller-side `DEFAULT_*` constants.

4. **Strict Path Matching in Supervisor (0.4):**
   - Removed `p.name.eq_ignore_ascii_case(&p_path)` from `supervisor::insert_readiness_stub` and `supervisor::prompt_agent` (`src-tauri/src/supervisor.rs`), enforcing exact `p.path == p_path` lookup per `AGENTS.md §1.11`.

5. **Canonical Task Process Keys & Terminal Pane Support (0.5):**
   - Standardized task supervisor keys to `<project_path>:tsk:<task_id>` (`ProcessSupervisor::format_task_key(project_path, task_id)`), matching the frontend Terminal Grid (`ProjectTerminalGrid.vue`) and MCP `inspect_terminal_output` (`mcp::task_process_key`).
   - Added `SupervisorError::TaskAlreadyRunning` (`StatusCode::CONFLICT`) when a task with the same `task_id` is already `Working` in the same project.
   - Wired `stdin_rx` to `child.stdin` inside `spawn_task` and preserved `output_ring` in `retained_buffers` upon task completion so the Terminal Grid pane can send input to running tasks and inspect completed task output.
   - Updated `docs/specs/01_service_orchestrator.md`.

## Verification
- `cargo check --manifest-path src-tauri/Cargo.toml --all-targets`: passed with 0 warnings.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib`: 309 passed, 0 failed.
