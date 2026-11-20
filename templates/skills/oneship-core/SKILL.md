---
name: oneship-core
description: Operational rules for dispatching project tasks and persistent services via start_project_task and manage_project_script, modifying rula.toml, and maintaining AGENTS.md.
version: 0.2.7
---

# ONE-SHIP Core Maintenance & Configuration Rules

This skill defines the rules for safely inspecting, modifying, and maintaining your project's configuration (`rula.toml`) and guidelines (`AGENTS.md`) during ongoing development.

---

## 1. Rules for Modifying `rula.toml`

When modifying `<project_root>/rula.toml` to register new tools, adjust personas, or configure services:

### A. Registering & Modifying Scripts (`[scripts]`)
Always categorize new commands correctly so the ONE-SHIP dashboard and runner can manage their lifecycle:

1. **Persistent Services (`[scripts.persistent.<id>]`):**
   - Use strictly for long-running processes (e.g. dev servers, daemons, file watchers).
   - Require `name`, `command`, and optional `args`, `cwd`.
   - **Continuous Profiling Binding (`profiler = "<plugin_id>"`):** When a service has an assigned profiler (e.g. `pyspy`, `v8prof`, `samply`), declare `profiler = "<plugin_id>"`. The Process Supervisor will automatically bind the continuous profiler to the process lifecycle whenever the service runs.

2. **Finite Tasks (`[scripts.task.<id>]`):**
   - Use for finite verification commands (e.g. `test`, `build`, `lint`, `typecheck`).
3. **Interactive Script Options (`options.<key>`):**
   - When a script supports dynamic flags or modes (e.g. `--release`, `--filter`), define an options schema so the ONE-SHIP UI can render form controls:
     ```toml
     [scripts.task.test]
     name = "Run Tests"
     command = "cargo"
     args = ["test"]
     options.release = { flag = "--release", label = "Release Mode", type = "boolean", default = false }
     options.suite = { flag = "--package", label = "Target Package", type = "select", choices = ["core", "ui"], default = "core" }
     ```
   - Every option needs a `flag`. `default` applies whenever a launch does not set the option (including `manage_project_script` starts and `start_project_task` calls without that key in `options`), so it must be a boolean for `boolean` options and one of `choices` for `select` options.

### B. Tuning Agent Personas (`[agents.<id>]`)
When adjusting specialist personas or adding new specialists:
1. **Strict 100-Word Constraint:** `system_prompt` MUST remain strictly under 100 words.
2. **Content Boundaries:** Include ONLY:
   - Specific role and exact directory ownership (e.g. "Owns `src/components/`").
   - Negative boundaries and delegation target (e.g. "Delegate backend tasks to `core`").
   - Key tech stack keywords (descriptive only).
3. **Strict Invariants:**
   - NEVER place imperative execution commands or test scripts in `system_prompt` (define them in `[scripts.task]`).
   - NEVER duplicate platform communication rules or approvals (governed by `AGENTS.md`).
   - Always ensure `mcp_dialogue_mode = true` and `auto_approve = true`.
4. **Context References (`context_docs`):**
   - Keep limited to 1-2 targeted documentation paths (loaded lazily on demand).

### C. Protected Platform Invariants
Never remove or alter these platform keys:
- `[system] mcp_dialogue_mode = true`
- `[project] oneship_version = "0.2.5"`

---

## 2. Rules for Maintaining `AGENTS.md`

1. **Protected Managed Block:**
   - NEVER modify or delete the header block between `<!-- ONE_SHIP_MANAGED_START -->` and `<!-- ONE_SHIP_MANAGED_END -->`. This block is managed automatically by platform migrations.
2. **Updating Project Guidelines (Bidirectional Synchronization):**
   - When introducing new architectural patterns, testing conventions, or stack constraints, update the sections **below** the managed header block (`## Architecture Overview`, `## Development & Verification`).
   - Ensure `AGENTS.md` continuously reflects the active reality of the codebase.

---

## 3. Runtime Script & Task Lifecycle via MCP

To inspect, launch, or manage scripts and tasks dynamically without direct terminal interference:
- **Zero Raw Terminal Bypass:** NEVER execute tests, builds, linters, or dev servers via raw shell/bash commands if they are registered in the project's `rula.toml` catalog.
- **Project Discovery & Inspection:** Use `discover_project` or `list_project_scripts` to inspect available persistent services and finite tasks.
- **Finite Tasks (Builds, Tests, Migrations, Linters):** Use `start_project_task(task_id="...", wait=true)` to run tasks synchronously and receive exit codes and logs directly. If `wait=false`, the platform returns an execution ID immediately for background execution. Poll status or logs using `query_project_task(exec_id=...)`, or cancel with `cancel_project_task(exec_id=...)`.
- **Persistent Background Services (Dev Servers, Watchers):** Use `manage_project_script` strictly for long-running services (`[scripts.persistent]`) to control their start, stop, or restart lifecycle.

---

## 4. Native Tool Calling & Zero Scripting Bypasses

- All platform interactions (`post_agent_response`, `manage_todo_item`, `record_technical_debt`, `resolve_initialization_flag`, `request_approval`, `complete_project_removal`, `search_project_skills`, `manage_project_skill`) MUST be made exclusively via native MCP tool calls (`call_mcp_tool` / direct MCP wrappers).
- NEVER execute Python scripts, bash curl commands, or shell scripts to send raw HTTP requests to `http://localhost:8765/mcp`.
- Call `post_agent_response` strictly ONCE at turn conclusion when all actions are completed. Dummy / test calls are strictly prohibited.

---

## 5. On-Demand Skill Discovery & Management (`search_project_skills`, `manage_project_skill`)

When working with specialized, niche, or newly introduced third-party libraries, SDKs, or proprietary APIs where you lack deep procedural context:
1. **Search Before Guessing:** Call `search_project_skills(query="...")` to locate community or reference skills from the registry.
2. **Propose & Confirm:** If a skill is found, propose it to the user or request approval via `request_user_approval` before installing.
3. **Internal Installation:** Call `manage_project_skill(action="install", project="...", skill_name="...", source_url="...")` to install it safely into `.agents/skills/`.
4. **Lifecycle Control:** Use `manage_project_skill(action="disable" | "enable" | "remove")` to manage project context budget.
5. **Zero Scripting Bypasses:** NEVER use curl, git clone, or bash commands to create skill directories manually.

---

## 6. Host Platform Protection & Zero Introspection Policy

- **Zero Host Introspection:** Agents must NEVER inspect, query, read, or manipulate ONE-SHIP internal files or databases:
  * Platform directories: `~/.config/one-ship/` (including `oneship.db`, `oneship_telemetry.duckdb`, `config.toml`, and runtime logs).
  * Executing database queries against the platform database (e.g. `sqlite3 ~/.config/one-ship/oneship.db ...` is strictly prohibited).
  * Inspecting or grepping ONE-SHIP source code repositories (`one-ship/src-tauri/`, `one-ship/src/`).
- **MCP Surface Exclusivity:** All platform state operations must pass exclusively through official ONE-SHIP MCP tools. If an MCP tool returns an error, explain the issue in dialogue; never attempt to bypass MCP tools by querying or editing host databases directly.

