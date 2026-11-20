# Devlog: Session Binding, Idle Pool Reuse & Devlog MCP Primitives

**Date:** 2026-09-11  
**Topic:** Spec 22 (Phases 2, 3, 4) - Antigravity Session Lifecycle Binding, Idle Pool Reuse, Database Devlog & Knowledge Graph

## 1. Problem Statement
1. **Unbound Turn Streams:** Turn records in `agent_chat_turns` were previously stored without an associated `agy_conversation_id`, disconnecting the continuous chat feed from the actual underlying Antigravity CLI session threads and `.gemini/antigravity-cli/brain/<uuid>` context files.
2. **Session Waste on Startup:** Every launch of an agent spawned a fresh `agy` process without reusing unengaged sessions, wasting tokens on repeated startup persona prompt injections.
3. **Flat-File Sprawl & Tool Misalignment:** Devlogs, task backlogs, and technical debt were maintained in flat markdown files inaccessible to agents via relational queries. Meanwhile, the deprecated `switch_conversation_topic` MCP tool forced an artificial "topics" abstraction that conflicted with real CLI sessions.
4. **Unscoped MCP Invocations:** An external pair-programming assistant session without an assigned project agent accidentally invoked `post_agent_response`, and a loose fallback defaulted to the first registered project, corrupting that project's chat stream.

## 2. Changes Implemented
- **`/proc` Session Scanner & Turn Binding (`src-tauri/src/kopeng/accounts.rs`, `src-tauri/src/conversation.rs`, `src-tauri/src/mcp.rs`):**
  - Added `find_agy_session_for_agent(project_path, agent_id)` which inspects `/proc/<pid>/environ` for `HERDR_PANE_ID` and matches against the agent's dedicated Herder tab, falling back to `/proc/<pid>/cwd`.
  - Populated `agy_conversation_id` on all user turns (`save_user_chat_turn_cmd`) and agent turns (`execute_post_agent_response`).
  - Recorded touched artifacts automatically via `db_record_artifact_touched`.
- **Idle Pool Session Reuse (`src-tauri/src/kopeng/herdr.rs`):**
  - In `bootstrap_agent_in_pane`, checked `db_get_latest_agent_session`. If the latest session has `user_turns == 0`, `agy` is launched with `--conversation <latest_uuid>`.
  - Skipped initial persona prompt injection during idle pool reuse and session resumes to prevent token waste and prompt duplication.
- **Spec 22 MCP Primitives (`src-tauri/src/mcp.rs`):**
  - Deleted deprecated `switch_conversation_topic` per Zero Dead Code policy.
  - Implemented `record_session_minutes`: records structured meeting minutes, decisions, touched files, and tags into `agent_session_journal`.
  - Implemented `resume_agent_session`: hot-restarts an agent pane with `agy --conversation <id>` and appends an inline system milestone turn (`──── // RESUMED CONTEXT: ... ────`).
  - Implemented `manage_todo_item`: CRUD on `project_todos` (`list`, `create`, `update_status`).
  - Implemented `record_technical_debt`: CRUD on `project_technical_debt` (`list`, `record`, `resolve`).
- **Strict Project Validation & Caller Safety (`src-tauri/src/mcp.rs`, `TECHNICAL_DEBT.md`):**
  - Added strict guard in `post_agent_response` rejecting calls where the `agent_id` is not registered in an active Wang-Kapawu project, completely eliminating the dangerous first-project fallback.
  - Updated tool descriptions to explicitly warn: *"CRITICAL: Exclusively for agents running inside a Wang-Kapawu managed Herder workspace. Do NOT call from external developer assistants or unmanaged sessions."*
  - Added Technical Debt Item #18 in `TECHNICAL_DEBT.md` for passive caller provenance verification (e.g. Unix socket credentials `SO_PEERCRED` or transient bearer tokens).
- **Tauri IPC Command Endpoints (`src-tauri/src/conversation.rs`, `src-tauri/src/lib.rs`):**
  - Registered `get_project_session_journals_cmd`, `get_project_todos_cmd`, `create_project_todo_cmd`, `update_todo_status_cmd`, `get_project_technical_debt_cmd`, `record_technical_debt_cmd`, `resolve_technical_debt_cmd`, and `resume_agent_session_cmd`.
- **Skills & Documentation Sync:**
  - Synchronized tool reference schemas in `.agents/skills/kapawu-platform/references/mcp_primitives.md` and `templates/skills/kapawu-platform/references/mcp_primitives.md`.
  - Updated `TODO.md` marking Phases 1–4 of Spec 22 complete.

## 3. Verification
- `cargo check`: Passed with 0 warnings/errors (1.49s).
- `npx vue-tsc --noEmit`: Passed with 0 errors.
