# Unified Capability Core — Stage 2B (`git`, `scripts`, `logs`)

**Date:** 2026-11-19  
**Specs:** Spec 12, Spec 16, Spec 40  

## Changes Implemented

1. **Capability Core Extension ([src-tauri/src/capabilities/mod.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/capabilities/mod.rs)):**
   - Registered `git`, `scripts`, and `logs` in `PROVIDERS`.
   - Added typed `CapabilityError` variants (`Git`, `Snapshot`, `Runner`, `Script`, `Orchestrator`, `Supervisor`, `Conflict`) with `ApiError` status/code delegation.
   - Updated `domain_enablement_respects_capabilities_independent_of_hidden_tabs` unit test to cover all 8 providers.

2. **Git & Snapshots Provider ([src-tauri/src/capabilities/git.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/capabilities/git.rs)):**
   - Registered `domain = "git"` (`required_capability = Some("has_git")`, `linked_surface_path = Some("git")`).
   - Actions: `get_status`, `list_commits`, `get_working_tree_diff` (with bounded 3,000-char diff preview for Voice callers), `list_branches`, `list_snapshots`, `create_snapshot` (`Mutating`, resolving active agent conversation when called via MCP), and `restore_snapshot` (`ConfirmRequired`).

3. **Scripts, Tasks & Terminals Provider ([src-tauri/src/capabilities/scripts.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/capabilities/scripts.rs)):**
   - Registered `domain = "scripts"` (`linked_surface_path = Some("overview")`).
   - Actions: `list_scripts`, `start_task` (async and synchronous `wait` up to `timeout_ms`), `query_task_status` (strict `project_id` ownership verification + live supervisor logs), `cancel_task` (`ConfirmRequired`, strict `project_id` ownership verification), `manage_service` (declared persistent services via `orchestrator`), and `inspect_terminal` (`all` | `errors_only` semantic error extraction).

4. **Logs, Audit & Agents Provider ([src-tauri/src/capabilities/logs.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/capabilities/logs.rs)):**
   - Registered `domain = "logs"` (`ReadOnly`).
   - Actions: `query_system_logs` (`project` | `system` scope), `query_project_audit`, and `query_agents` (`supervisor::compute_project_state`).

5. **Voice Controller & MCP Convergence ([src-tauri/src/voice_controller.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/voice_controller.rs), [src-tauri/src/mcp.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/mcp.rs), [docs/specs/40_unified_capability_core_and_voice_harness.md](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/40_unified_capability_core_and_voice_harness.md)):**
   - Updated `capability_discover` example list in `voice_controller.rs` to include `'git'`, `'scripts'`, and `'logs'`.
   - Routed all 12 corresponding MCP tools (`start_project_task`, `query_project_task`, `cancel_project_task`, `list_project_scripts`, `manage_project_script`, `inspect_terminal_output`, `query_system_logs`, `query_project_agents`, `query_project_audit`, `create_snapshot`, `list_snapshots`, `restore_snapshot`) through `call_mcp_capability` in `mcp.rs`, preserving the `"system"` origin special case for `query_system_logs` and deleting dead `task_process_key` / `ExecutionProjectUnregistered` code.

## Verification

1. **Daemon Build & Runtime Health (`./scripts/run_dev.sh status && journalctl --user -u oneship-dev-daemon-build -n 25 --no-pager`):**
   - Build succeeded cleanly: `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 30.70s`.
   - `oneship-dev-daemon` active (`main_pid: 3243391`), listening on port `8765`.
   - Note: `oneship-dev-gui` remains in `failed` state from the earlier Stage 2A intermediate build; not started manually per `AGENTS.md` §1.4.

2. **MCP `tools/list` Live Probe (`POST http://127.0.0.1:8765/mcp`):**
   - Confirmed all 12 Stage 2B MCP tools remain registered cleanly.

3. **Rule Gate & Fallback Audit (`./scripts/verify_rules.sh` + `grep`):**
   - `grep -nE "unwrap_or|unwrap_or_default|\.ok\(\)|let _ =|catch..." src-tauri/src/capabilities/{mod,git,scripts,logs}.rs` -> **0** hits.
   - `sed -n '2260,2315p;2610,2750p' src-tauri/src/mcp.rs | grep -nE "unwrap_or|unwrap_or_default|\.ok\(\)|let _ ="` -> only explicit envelope key unwraps (`unwrap_or(&res)`) and optional `project` target string in `execute_query_system_logs` `"system"` special case.
   - `./scripts/verify_rules.sh` reported zero violations in any Stage 2B Rust files (same pre-existing `[FAIL]` on legacy Vue modal `bg-black/*` classes).
