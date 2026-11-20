# Devlog: Agent ID Injection, MCP Fallback Inference, and App Boundary Rule

**Date:** 2026-09-11  
**Topic:** Agent Dialogue Routing Hardening & Process Execution Governance

## 1. Problem Statement
When launching agent personas (e.g. `bosmang` in `fideiuris`), the agent responded via `post_agent_response`, but the frontend dialogue stream did not receive or render the message.
Root cause analysis revealed:
1. `src-tauri/src/kopeng/herdr.rs::build_agent_initial_prompt` omitted `agent_id: "${agent_info.id}"` from both startup directives and ongoing rules.
2. `src-tauri/src/kopeng/manifest.rs` (`WANG_KAPAWU_MANAGED_BLOCK`) omitted `agent_id` under Section 2 ("Response Structure").
3. `src-tauri/src/mcp.rs::execute_post_agent_response` silently defaulted missing `agent_id` to `"unknown"`, routing into `conversations["unknown"]` instead of the active agent's stream.
4. Agents in Herder were not marked as prompted in `crate::conversation` during startup prompt injection.

## 2. Changes Implemented
- **Prompt Injection (`src-tauri/src/kopeng/herdr.rs`):**
  - Updated `build_agent_initial_prompt` to inject explicit `agent_id: "{agent_id}"` in both Startup Directives and Ongoing Rules.
  - Added `crate::conversation::mark_user_message_sent(&agent_info.id)` in `launch_herdr_agent` and `reset_workspace_agents` upon initial prompt dispatch.
- **MCP Server & Fallback Inference (`src-tauri/src/mcp.rs`):**
  - Clarified schema description for `post_agent_response.agent_id` to emphasize it must match the assigned role ID.
  - Added multi-stage fallback inference in `execute_post_agent_response`:
    1. Check `crate::conversation::get_all_states()` for the agent with the highest `last_user_message_at`.
    2. Check running Herder agents across registered projects; if exactly one running agent exists, resolve to that agent.
    3. Log warning when inference is applied and bind the resolved `agent_id` to conversation state, SQLite persistence, and SSE event dispatch.
- **Governance & Skill Synchronization:**
  - Updated `WANG_KAPAWU_MANAGED_BLOCK` in `manifest.rs` and `fideiuris/AGENTS.md`.
  - Bumped platform skill version to `0.2.3` in `CURRENT_SKILL_VERSION` (`skill.rs`) and `SKILL.md`.
  - Synced updated platform skill to `fideiuris/.agents/skills/kapawu-platform/`.
- **Project Rule Addition (`AGENTS.md`):**
  - Added Rule 6 ("Application Lifecycle & Process Execution Boundary") under Fundamental Governance Rules, strictly prohibiting the AI agent from starting, restarting, killing, or backgrounding application runtime processes, daemons, or dev servers.
