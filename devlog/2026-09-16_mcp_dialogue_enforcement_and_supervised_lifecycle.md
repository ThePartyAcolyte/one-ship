# MCP Dialogue Enforcement & Supervised Mode Lifecycle Hardening

## 1. Context & Root Cause
- **Terminal stdout markdown leakage:** The `initializer` agent prompt (`templates/prompts/project_initializer.md`) lacked explicit MCP dialogue channel rules. The underlying runner (`agy`) printed the Phase 1 technical proposal directly to `stdout` in the terminal ring buffer instead of calling `post_agent_response`.
- **Premature supervised mode failure:** In Supervised mode, the agent presenting a technical proposal without immediately writing `rula.toml` is the expected behavior (awaiting developer review/approval). In `skill.rs`, the background observer erroneously marked the operation as failed with `"Initialization incomplete: rula.toml was not created"` on `exit_code: 0`.
- **Supervisor-first project resolution:** In `mcp.rs`, `find_project_for_agent` previously checked database records without consulting the live `ProcessSupervisor` active process map first.

---

## 2. Changes Executed

1. **Prompt Template & Skill Enforcement (`templates/prompts/project_initializer.md` & `templates/skills/kapawu-initializer/SKILL.md`):**
   - Added `MANDATORY MCP DIALOGUE CHANNEL (ZERO DIRECT TERMINAL OUTPUT)` section.
   - Enforced `post_agent_response(agent_id: "initializer", dialogue: "...", artifact: "...", artifact_type: "proposal", artifact_lang: "markdown")`.
   - Prohibited raw conversational text and markdown blocks in stdout.

2. **Supervised Mode Lifecycle Observer (`src-tauri/src/kopeng/skill.rs`):**
   - Added clear `MANDATORY SUPERVISED MODE DIRECTIVE` in prompt construction.
   - In background observer loop: when `exit_code == 0` and `!rula_exists` in `supervised` mode, marked `ProjectInitState` as `status: "running"` and `current_phase: "Proposal presented — review in Chat"` instead of failing.

3. **Supervisor Process Lookup & Event Emission Fallback (`src-tauri/src/mcp.rs`):**
   - In `find_project_for_agent`, added live `sup.list_active_processes()` inspection to immediately match the running project path.
   - In `execute_post_agent_response`, added `sup.emit_event` fallback to ensure `AgentResponse` and `AccountQuotaUpdated` events always broadcast to the frontend.

---

## 3. Verification & Results
- `cargo check --bins`: Succeeded (0 errors).
- `cargo test --lib`: 23 passed, 0 failed in 0.15s.
- `npm run build`: Succeeded (0 TypeScript/Vite errors).
