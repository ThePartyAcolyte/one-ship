# Devlog: Inter-Agent Communication, Asynchronous Task Lifecycle & Discovery Audit

**Date:** 2026-09-30  
**Context:** Comprehensive audit and implementation of inter-agent delegation (both local and cross-project), deterministic asynchronous task lifecycles, non-disruptive agent turn queuing, and zero-burden project discovery via ONE-SHIP MCP.

---

## 1. Architectural Implementations & Invariants

1. **Cross-Project Delegation (`agent_delegations` Schema & Routing):**
   - Extended `agent_delegations` table with `originator_project` and `target_project` columns, with idempotent runtime migration in `init_db`.
   - Updated `DelegationRecord` and CRUD operations (`db_insert_delegation`, `db_get_pending_delegation_by_task`, `db_get_pending_delegation_by_target`).
   - When an agent delegates across projects (`target_project = "<name_or_path>"`), the supervisor targets the specific project directory for the target persona while preserving the originating project path.
   - Upon completion, `report_task_status` looks up the delegation record, automatically identifies the originating project, and dispatches the callback notice back to the originator agent in its own project.

2. **Non-Disruptive Turn Queuing (Safe Concurrency for Busy Agents):**
   - Added `pending_inputs: Arc<RwLock<VecDeque<String>>>` to `ManagedProcess` in `ProcessSupervisor`.
   - When input is sent via `send_input` / `prompt_agent` while a process has status `ProcessStatus::Working`, the input is non-disruptively enqueued into the FIFO buffer instead of colliding with the active turn on stdin.
   - Implemented `dispatch_next_pending_input(&self, key: &str) -> Result<bool, String>`.
   - Integrated automatic queue draining into `post_agent_response` (turn conclusion) and `report_task_status`.

3. **Deterministic Asynchronous Task Lifecycle:**
   - Implemented `start_project_task_async(input: RunTaskInput)` in `kopeng/runner.rs`, executing tasks asynchronously under the supervisor with ring buffers and persisting lifecycle states in `task_executions`.
   - Added MCP tools:
     - `start_project_task`: Dispatches finite tasks (`[scripts.task]`) asynchronously, returning an `exec_id` immediately to avoid HTTP timeouts.
     - `query_project_task`: Queries execution lifecycle, duration, exit code, and live terminal buffers for an `exec_id`.
     - `cancel_project_task`: Cancels running task executions under the supervisor.
   - Enforced strict separation in `manage_project_script`: rejects finite tasks, directing callers to use `start_project_task`.

4. **Zero-Burden Project Discovery (`discover_project`):**
   - Added `discover_project` MCP tool returning a consolidated manifest: project metadata, Git branch, specialist personas (with roles and live statuses), persistent services, and finite tasks with options schemas.
   - Uses `resolve_project_with_fallback` to automatically infer caller project from TCP socket origin (`caller_cwd`), requiring zero self-referential IDs.

5. **Declarative Platform Skills & Living Specifications:**
   - Updated `templates/skills/kapawu-delegation/SKILL.md` (v0.2.5): Documented cross-project delegation, discovery, and non-disruptive queuing without leaking raw JSON schemas.
   - Updated `templates/skills/kapawu-core/SKILL.md` (v0.2.5): Documented async task lifecycle vs persistent background services.
   - Updated `docs/specs/12_streamable_http_mcp_daemon.md`: Updated tool functional domains and multi-agent delegation specifications.

---

## 2. Modified Files

- `src-tauri/src/db.rs`: Added `originator_project` and `target_project` columns and queries to `agent_delegations`.
- `src-tauri/src/supervisor.rs`: Added `pending_inputs` ring buffer and `dispatch_next_pending_input` to `ProcessSupervisor`.
- `src-tauri/src/kopeng/runner.rs`: Implemented `start_project_task_async`.
- `src-tauri/src/mcp.rs`:
  - Added tool schemas for `discover_project`, `start_project_task`, `query_project_task`, `cancel_project_task`.
  - Implemented `resolve_project_with_fallback`, `execute_discover_project`, `execute_start_project_task`, `execute_query_project_task`, `execute_cancel_project_task`.
  - Updated `execute_report_task_status`, `execute_delegate_agent_task`, `execute_query_project_agents`, `execute_list_project_scripts`, `execute_manage_project_script`, `execute_inspect_terminal_output`, `execute_query_project_audit`.
  - Integrated `dispatch_next_pending_input` in `execute_post_agent_response`.
- `templates/skills/kapawu-delegation/SKILL.md`: Bumped to v0.2.5 with declarative cross-project and discovery instructions.
- `templates/skills/kapawu-core/SKILL.md`: Bumped to v0.2.5 with async task lifecycle guidance.
- `docs/specs/12_streamable_http_mcp_daemon.md`: Synchronized tool functional domains and inter-agent callback specifications.

---

## 3. Verification

- Validated schema definitions against MCP JSON-RPC 2.0 specs.
- Confirmed zero-burden resolution fallback for omitted caller arguments.
- Verified absence of git commands or bash file edits per protocol rules.
