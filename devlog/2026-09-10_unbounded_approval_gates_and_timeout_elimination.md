# Devlog: 2026-09-10 - Unbounded Approval Gates & Timeout Elimination

## 1. Context & Objectives
During semi-attended project initialization, user approval requests were prematurely terminating or aborting due to external client-side timeouts rather than backend logic. The goal of this session was to:
1. Eliminate all timeouts from the approval gate flow so that the agent waits indefinitely until explicit human review is submitted.
2. Establish persistent approval state in SQLite (`pending_approvals`) to decouple gate requests from ephemeral GUI/browser sessions.
3. Implement a clean failure state banner on project cards with atomic `[ ROLLBACK TO CLEAN STATE ]` and `[ RETRY INITIALIZATION ]` controls.
4. Enforce strict directory isolation invariants during project initialization.

---

## 2. Root Cause Analysis
1. **CLI Turn Timeout (`--print-timeout` in `agy`):** When headless initialization was spawned via `tokio::process::Command`, the `agy` CLI enforced a default `--print-timeout 5m0s`. If the human developer took more than 5 minutes to deliberate, `agy` forcefully terminated the process.
2. **MCP Per-Tool-Call Timeout (`timeoutSeconds`):** Antigravity's internal MCP client defaults to a 180-second (3 minutes) HTTP timeout when `timeoutSeconds` is omitted from `~/.gemini/config/mcp_config.json`.
3. **Premature Completion Status:** `skill.rs` previously relied only on `out.status.success()`. If an agent timed out or exited without writing `rula.toml`, the database incorrectly marked the project as "completed".

---

## 3. Implemented Changes

### A. Unbounded Execution & Configuration
- **`src-tauri/src/kopeng/skill.rs`:** Injected `--print-timeout 24h` when spawning `agy` in headless (`semi_attended` and `unattended`) modes. Added explicit `MANDATORY UNATTENDED EXECUTION MANDATE` in the unattended prompt to pre-authorize direct file writing and prevent the agent from halting with a textual chat proposal.
- **`~/.gemini/config/mcp_config.json`:** Configured `"timeoutSeconds": 86400` on the `wang-kapawu` MCP server entry.
- **`templates/skills/kapawu-platform/SKILL.md` & `.agents/skills/kapawu-platform/SKILL.md`:** Updated client configuration templates to document `timeoutSeconds: 86400`.

### B. Persistent Approvals & Hydration
- **`src-tauri/src/db.rs`:** Created `pending_approvals` table (`task_id`, `project_path`, `phase`, `payload_markdown`, `status`, `created_at`) with CRUD operations.
- **`src-tauri/src/mcp.rs`:**
  - Persists pending approval records in SQLite before awaiting channel resolution.
  - Added HTTP endpoint `GET /api/pending-approvals` and Tauri command `get_pending_approvals_cmd`.
  - Cleans up resolved or rejected records upon user response.
  - Removed misleading "timed out" error messages.
- **`src/stores/coreStore.ts`:** Added `checkPendingApprovals()` hydration routine executed on Tauri IPC initialization and SSE connection.

### C. Rollback Engine & UI Controls
- **`src-tauri/src/kopeng/manifest.rs`:** Implemented `rollback_project_init(project_path)` command which purges partially generated files (`.agents`, `AGENTS.md`) and clears initialization state from the database.
- **`src/components/ProjectCard/ProjectCard.vue`:** Added failure banner with `[ ROLLBACK TO CLEAN STATE ]` and `[ RETRY INITIALIZATION ]` actions, styled according to brutalist UI conventions.

---

## 4. Verification
- `cargo check`: Passed with 0 errors.
- `cargo check --bin kapawu-daemon`: Passed with 0 errors.
- `npm run build`: Passed with 0 errors.
- `agy mcp list`: Verified clean MCP server parsing with updated configuration.
