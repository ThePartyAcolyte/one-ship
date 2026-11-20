# Development Log: 2026-09-18

## Topic: MCP Dialogue Compliance, Persistent Flag Resolution, and Graceful De-Integration Protocol

### 1. Objectives & Context
- Ensure Phase 2 flag resolution agents (`techdebt-resolver`, `todos-resolver`, `profiler-crafter`, `docs-resolver`) reliably deliver responses through the Wang-Kapawu native MCP dialogue channel (`post_agent_response`) without falling back to raw terminal stdout output or attempting raw script execution.
- Maintain persistent, non-blocking agent visibility in `AgentList.vue` across app restarts and session switches while initialization flags remain active.
- Implement an automated, graceful project de-integration protocol on project deletion (`remove_project`), exporting SQLite knowledge (technical debt, todos, session journal) into portable Markdown documents (`TECHNICAL_DEBT.md`, `TODO.md`, `SESSION_JOURNAL.md`) and performing cascading database cleanup.

---

### 2. Changes Implemented

1. **Target Project Governance Synchronization:**
   - In [`src-tauri/src/kopeng/flag_resolver.rs`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/kopeng/flag_resolver.rs), added `prepare_project_agents_md(p)` before launching resolution agents to ensure target project `AGENTS.md` is always synchronized with `WANG_KAPAWU_MANAGED_BLOCK`.
   - Updated `AGENTS.md` in `/home/butcherwutcher/Projects/dev/fideiuris/` to enforce Section 6 native MCP calling constraints.

2. **Hardened Prompts & Tool Calling Constraints:**
   - In [`templates/prompts/`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/templates/prompts/) (`flag_resolve_techdebt.md`, `flag_resolve_todos.md`, `flag_resolve_profiler.md`, `flag_resolve_docs.md`), declared explicit `call_mcp_tool` invocations with ServerName `"wang-kapawu_wang-kapawu"` or `"wang-kapawu"`.
   - Explicitly mandated terminal stdout silence, single terminal response invariants, and multi-turn closure.

3. **Supervisor Compliance Reminder & Conversation State Sync:**
   - In [`src-tauri/src/supervisor.rs`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/supervisor.rs), updated the compliance reminder injection prompt to include exact MCP calling signatures and synchronized conversation tracking via `crate::conversation::mark_user_message_sent`.

4. **Persistent & Non-Blocking Resolution Agents:**
   - In `compute_project_state` ([`src-tauri/src/supervisor.rs`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/supervisor.rs)), dynamically discovered unresolved initialization flags from SQLite (`db_get_init_flags`) and injected `AgentState` with `is_internal: false` for `docs-resolver`, `todos-resolver`, `techdebt-resolver`, and `profiler-crafter`.
   - In [`src/components/dialogue/AgentList.vue`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/dialogue/AgentList.vue), updated `launchAgent` to detect resolution agents, lookup their flag record in `coreStore.projectInitFlags`, and dispatch `launch_flag_resolution_agent`.

5. **Graceful De-Integration Knowledge Export & Cascading Cleanup:**
   - In [`src-tauri/src/kopeng/manifest.rs`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/kopeng/manifest.rs), implemented `export_project_knowledge_to_markdown` exporting `project_technical_debt` -> `TECHNICAL_DEBT.md`, `project_todos` -> `TODO.md`, and `agent_session_journal` -> `SESSION_JOURNAL.md` prior to artifact deletion.
   - In [`src-tauri/src/db.rs`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/db.rs), updated `db_remove_project` to perform cascading deletion across all relational project tables.
   - In [`src-tauri/src/registry.rs`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/registry.rs), wired `export_project_knowledge_to_markdown` directly into `remove_project`.

6. **Cross-Process SQLite Compliance Synchronization & Listener Deduplication:**
   - In [`src-tauri/src/conversation.rs`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/conversation.rs), made `should_inject_compliance_reminder` cross-process aware by querying SQLite `chat_turns` (`has_recent_db_response`) to prevent false-positive reminder loops between `kapawu-daemon` and Tauri.
   - In [`src-tauri/src/supervisor.rs`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/supervisor.rs), marked `mark_agent_responded` directly upon stdout tool detection and prevented reminder loop resetting.
   - In [`src/stores/dialogueStore.ts`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/stores/dialogueStore.ts), eliminated redundant `AgentResponse` listener.

---

### 3. Verification & Invariants
- `cargo check`: Passed with zero warnings or errors.
- `npm run build`: Passed with zero type or build errors.
- External project governance, export fidelity, and single-turn compliance verified.
