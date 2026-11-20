# 2026-09-13 - MCP Workspace Scoping, Fallback Removal & Self-Correcting Prompt Guardrails

## Context & Objectives
When `wang-kapawu` was configured globally in `~/.gemini/config/mcp_config.json`, external Antigravity sessions in unrelated workspaces discovered the tool definitions (specifically `post_agent_response`, `request_approval`, and `report_task_status`). Foreign LLMs frequently attempted to conclude normal conversational turns by invoking `post_agent_response`. Ambiguous fallback inferences in the backend (`running_agents.len() == 1` or recent message dispatch) erroneously bound these foreign requests to active Wang-Kapawu agents or returned uninformative JSON-RPC errors that derailed external assistants.

This session implemented a 3-layer defense-in-depth architecture:
1. **Local Workspace Scoping:** Relocated Wang-Kapawu MCP server registration from global `~/.gemini/config/mcp_config.json` to project-scoped Antigravity plugins (`.agents/plugins/wang-kapawu/`).
2. **Fallback Elimination:** Stripped loose identity inferences in `src-tauri/src/mcp.rs`, requiring verified agent identifiers registered in active project workspaces.
3. **Self-Correcting Prompt Guardrails:** Formatted error responses from Wang-Kapawu MCP endpoints with explicit directives instructing foreign LLMs to stop calling internal tools and reply directly via standard conversational output.

---

## Key Changes & Implementations

### 1. Project-Scoped MCP Plugin Injection (`src-tauri/src/kopeng/skill.rs`)
- Implemented `inject_project_mcp_plugin(project_path: &Path) -> Result<(), String>`.
- Injects `.agents/plugins/wang-kapawu/plugin.json` and `.agents/plugins/wang-kapawu/mcp_config.json` targeting `http://localhost:8765/mcp`.
- Wired into `inject_platform_skill` and `initialize_project_agent` so new and existing projects receive scoped tool configurations automatically.
- Deployed scoped plugins across registered workspaces (`wang-kapawu`, `fideiuris`, `dictate-whisper`).

### 2. Global Client Config Sanitization (`~/.gemini/config/mcp_config.json`)
- Cleanly removed the global `wang-kapawu` entry from `~/.gemini/config/mcp_config.json`.
- Non-Kapawu projects opened in Antigravity no longer discover or load Wang-Kapawu tools into their context window.

### 3. Elimination of Ambiguous Inferences (`src-tauri/src/mcp.rs`)
- Removed `Fallback inference 1` (`conversation.rs` recent message tracker) and `Fallback inference 2` (`running_agents.len() == 1` single active agent presumption).
- Enforced strict validation rejecting `agent_id == "unknown"`, empty string, or `"self"`.

### 4. Self-Correcting Prompt Guardrails (`src-tauri/src/mcp.rs`)
- Updated rejection payloads in `execute_post_agent_response`, `execute_report_task_status`, `execute_delegate_agent_task`, `execute_record_session_minutes`, `execute_resume_agent_session`, and `execute_request_approval`.
- Returns explicit instruction: *"CRITICAL INSTRUCTION: You are running in an external workspace or session outside of Wang-Kapawu... You MUST NOT call post_agent_response or any Wang-Kapawu MCP tools. Immediately render your response directly to the user in your standard conversation output stream."*
- Prefixed tool descriptions in `tools/list` with `"[Wang-Kapawu Internal Only]"`.

### 5. Manifest & Artifact Lifecycle (`src-tauri/src/kopeng/manifest.rs`)
- Added `KAPAWU_PLATFORM_PLUGINS = &[".agents/plugins/wang-kapawu"]`.
- Included in `resolve_kapawu_artifacts` to ensure clean uninstallation during project removal without leaving orphaned plugin files.

---

## Verification
- `cargo check --manifest-path src-tauri/Cargo.toml`: Passed (0 errors, 0 warnings).
- `cargo check --bin kapawu-daemon --manifest-path src-tauri/Cargo.toml`: Passed (0 errors, 0 warnings).
- `npm run build` (`vue-tsc --noEmit && vite build`): Passed cleanly.
- Verified absence of `wang-kapawu` in global `~/.gemini/config/mcp_config.json`.
- Verified presence of `.agents/plugins/wang-kapawu/` across active projects.
