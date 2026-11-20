# Devlog: Global System Agent Lifecycle & Auto-Bootstrap Engine

**Date:** 2026-09-15  
**Topic:** Resolving Dead Message Delivery during Supervised Initialization & Wiring Platform System Agent in Herder  

---

## 1. Problem & Incident Diagnosis
During supervised/assisted project initialization (`ProjectInitModal.vue`), users experienced a critical bug where initial setup prompts were dispatched into the chat overlay without bootstrapping the underlying Global System Agent (`system`). This resulted in:
1. An unlaunched agent state in the UI.
2. An unhandled backend dispatch error in `prompt_herdr_agent`:
   `[Dialogue] Failed to dispatch prompt to agent 'system': Agent 'system' is not running and could not be bootstrapped in Herder`.
3. A "dead message" visible in the chat UI that could neither be processed nor responded to.

### Root Cause Analysis:
- `prompt_herdr_agent` (`src-tauri/src/kopeng/herdr.rs`) and `execute_delegate_agent_task` (`src-tauri/src/mcp.rs`) attempted auto-bootstrap by iterating through registered project configurations (`state.agents`).
- Since `system` is the cross-project Platform System Architect & Manager, it does not belong to any single project's `rula.toml`. Consequently, the project loop failed to locate it and failed to launch.
- In `ProjectInitModal.vue`, `handleStart()` invoked `dialogueStore.sendUserMessage('system', promptText)` immediately after logging `Launching Global System Agent...`, without ever issuing an IPC command to verify or trigger its launch in Herder.
- In the frontend (`ConversationPanel.vue` and `AgentList.vue`), manual "Launch" action buttons were hardcoded to project-only agent searches, rendering them completely ineffective for `system`.

---

## 2. Architecture & Implementation Highlights

1. **System Agent Lifecycle & Platform Workspace (`src-tauri/src/kopeng/herdr.rs`):**
   - **`resolve_system_working_dir()`:** Computes the common ancestor directory of registered projects (e.g. `/home/butcherwutcher/Projects/dev`), falling back to `~/Projects` or home directory.
   - **`inject_project_mcp_plugin()`:** Injects `.agents/plugins/wang-kapawu/mcp_config.json` in the system directory ensuring the agent CLI connects to `http://localhost:8765/mcp`.
   - **Dedicated Herder Workspace & Tab:** Manages workspace `"System"` and dedicated tab `"system"`.
   - **`launch_system_agent()`:** Idempotently launches `agy` with configured model (`gemini-3.8-flash`), flags (`--dangerously-skip-permissions`, `--mode accept-edits`), awaits readiness (`idle`), and injects the initial system persona prompt with MCP dialogue handshake directives (`post_agent_response`).
   - **`get_system_herdr_state()`:** Inspects Herder snapshots in real-time to report accurate agent status (`"unlaunched"`, `"idle"`, `"working"`, `"blocked"`).

2. **Backend Commands & Poller Stream (`src-tauri/src/lib.rs` & `herdr.rs`):**
   - Exposing `launch_system_agent_cmd` and `get_system_herdr_state_cmd` Tauri commands.
   - In `spawn_herdr_poller`, emitted `DaemonEvent { event_type: "system-agent-updated", payload: json!(system_agent_state) }` alongside project state updates.

3. **Auto-Bootstrap Fallback (`herdr.rs` & `mcp.rs`):**
   - In `prompt_herdr_agent` and `execute_delegate_agent_task`, added an explicit branch: when `target.eq_ignore_ascii_case("system")` and unresolved to an active pane, automatically execute `launch_system_agent()`, wait for pane allocation, re-resolve target, and dispatch the prompt.

4. **Frontend Integration & Onboarding Handshake:**
   - **`ProjectInitModal.vue`:** In supervised mode, `await invoke('launch_system_agent_cmd')` is called prior to opening the overlay and sending the user prompt.
   - **`coreStore.ts`:** Added `systemAgentState`, handled `"system-agent-updated"` event, and added `fetchSystemAgentState()`.
   - **`App.vue`:** Replaced hardcoded `status: 'idle'` with live `coreStore.systemAgentState?.status || 'unlaunched'`.
   - **`ConversationPanel.vue` & `AgentList.vue`:** Wired `activeAgentInfo` and "Launch" action buttons to invoke `launch_system_agent_cmd` when working with `system`.

---

## 3. Verification & Invariants
- `cargo check`: Verified clean compilation without warnings or errors.
- `npm run build` (`vue-tsc --noEmit && vite build`): Verified complete frontend TypeScript type integrity and packaging.
- Verified that all file changes strictly complied with governance rules (no shell file editing, zero git mutation).
