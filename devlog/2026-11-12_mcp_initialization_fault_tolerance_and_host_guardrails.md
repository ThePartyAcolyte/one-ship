# Devlog: MCP Initialization Fault Tolerance & Host Guardrails

- **Date:** 2026-11-12
- **Topic:** Fault-tolerant Project Initialization Completion, State Synchronization, and Host Introspection Guardrails
- **Status:** Complete

---

## 1. Context & Incident Analysis
During greenfield onboarding of the `Kirulab Website` project, the `initializer` agent completed project scaffolding (React 19, Tailwind CSS, `rula.toml`, Genesis milestone recording, typecheck, and build verification with exit code 0) and invoked the MCP tool `complete_project_initialization`.

The MCP call failed with:
```text
Project 'Kirulab Website' has no initialization in progress. Fix it, then call complete_project_initialization again.
```

### Root Cause
1. **Unsynchronized State in SQLite:** During an earlier morning debugging session (`devlog/2026-11-12_shim_dispatcher_process_naming_compatibility.md`), an orphaned `running` initialization record in `project_initializations` had been manually purged from SQLite.
2. **Launch Disconnect in Supervisor:** When the user resumed/re-armed the initializer agent from the Floating Chat Overlay, the frontend called `launch_agent`. Unlike `initialize_project_agent`, `launch_agent` spawned the agent process but did not check or upsert an active `running` record into `project_initializations`.
3. **Rigid Precondition in MCP:** `complete_initialization` required `db_get_init_state(&project.path)?` to strictly match `status == "running"`, failing with `InitializerError::NotInitializing` even though `rula.toml` had a valid `[project].id` and the Genesis milestone had been recorded.
4. **Accidental Metaprogramming Directive:** The MCP error wrapper appended `. Fix it, then call complete_project_initialization again.`, while the `oneship-initializer` skill prompt instructed `Fix whatever it reports and call it again`. The LLM agent interpreted this as an imperative directive to debug the platform state, using `read_file` to inspect `src-tauri/src/db.rs` and executing `sqlite3 ~/.config/one-ship/oneship.db` read queries (`SELECT`, `.schema`, `.tables`).

---

## 2. Changes Implemented

### Backend (`src-tauri/src/kopeng/initializer.rs` & `src-tauri/src/mcp.rs`)
1. **Fault-Tolerant Initialization Completion:**
   - In `complete_initialization`, verified `rula.toml` exists with `[project].id` and the Genesis milestone is recorded in the session journal.
   - If `project_initializations` state is missing or out of sync, automatically synthesized and repaired the state record to `status = "completed"` instead of aborting with `NotInitializing`.
   - Formulated specific prescriptive errors for genuine caller errors (missing `rula.toml`, missing `[project].id`, missing Genesis milestone) clearly naming the prerequisite and the MCP tool (`record_session_minutes`) to satisfy it.
2. **Eliminated Open-Ended "Fix it" Directive:**
   - Updated `execute_complete_project_initialization` in `mcp.rs` to return `e.to_string()` directly without the ambiguous `. Fix it, then call complete_project_initialization again.` trailer.

### Supervisor State Synchronization (`src-tauri/src/supervisor.rs`)
1. **`launch_agent` Init-State Healing:**
   - When `launch_agent` is invoked for `INITIALIZER_AGENT_ID`, inspected `db_get_init_state(&project_path)`.
   - If the state row is missing or not in `running`/`completed`, automatically upserted `ProjectInitState` with `status: "running"`, restoring continuity between UI-triggered launches and MCP completion.

### Host Protection Guardrails & Policy Enforcement
1. **Managed Governance Block (`src-tauri/src/kopeng/manifest.rs`):**
   - Added Section 8 (`Host Platform Protection & Zero Introspection Policy`) to `ONE_SHIP_MANAGED_BLOCK` (automatically propagated into `AGENTS.md` and `CLAUDE.md` across all projects).
   - Explicitly prohibited inspecting or executing commands against `~/.config/one-ship/` (`oneship.db`, telemetry DuckDB), executing `sqlite3` against host databases, or inspecting host source repositories.
2. **System Prompts & Skills:**
   - Updated `templates/prompts/greenfield_initializer.md` and `templates/prompts/project_initializer.md` with explicit host platform boundary protection invariants.
   - Updated `templates/skills/oneship-initializer/SKILL.md` to instruct `If it reports missing requirements, satisfy them and invoke it again. NEVER inspect or manipulate host files or databases directly.`
   - Updated `templates/skills/oneship-core/SKILL.md` with Section 6 (`Host Platform Protection & Zero Introspection Policy`).

### Specs & Technical Debt
1. **Technical Debt Register (`TECHNICAL_DEBT.md`):**
   - Recorded Item 25: *Standardized Prescriptive Error Messaging across MCP Server Tools*.
2. **Living Specification (`docs/specs/12_streamable_http_mcp_daemon.md`):**
   - Synchronized Section 4 with auto-synthesis and prescriptive error behaviors for `complete_project_initialization`.

---

## 3. Verification & Results
- Verified unit test suites:
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib kopeng::initializer` (6 passed; 0 failed)
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib supervisor` (23 passed; 0 failed)
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib mcp` (3 passed; 0 failed)
- Watchexec automatically compiled the updated binary and restarted `oneship-dev-daemon.service` cleanly.
