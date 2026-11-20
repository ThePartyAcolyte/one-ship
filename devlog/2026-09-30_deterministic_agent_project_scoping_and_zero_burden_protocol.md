# Development Log: Deterministic Agent Project Scoping & Zero-Burden Orchestration Protocol

**Date:** 2026-09-30  
**Topic:** Architectural forensic remediation of agent project scoping, process supervisor routing, and zero-burden MCP/IPC encapsulation.

---

## 1. Overview & Forensic Root Cause Analysis

### Incident Summary
When attempting to launch and prompt an agent (e.g. `bosmang`) within project `L10NSL Banking` (`l10n_cl_banking`, project ID 8), the agent became active in and responded from project `Estudio RC` (`estudiorc`, project ID 3).

### Forensic Findings
1. **Blind SQLite Loop in Backend `prompt_agent`:**
   - In `src-tauri/src/supervisor.rs`, `prompt_agent` did not require `project_path`. When omitted, it iterated `repo.load_projects()` from index 0. The first project in the database declaring the agent persona (`estudiorc` at index 0 had `[agents.bosmang]`) was arbitrarily selected.
   - Even though the agent was originally spawned in `l10n_cl_banking` (turn #139), user prompts dispatched via `prompt_agent` without `project_path` spun up a duplicate process `estudiorc::bosmang` (turn #140 saved with project ID 3).
2. **Missing `projectPath` in Frontend Dispatchers:**
   - In `dialogueStore.sendUserMessage`, `invoke('prompt_agent', { target: agentId, prompt })` passed no `projectPath`.
   - In `AgentSection.vue`, `dialogueStore.openFloatingChat(agent.id)` failed to pass `project.value?.path`.
   - In `AgentTaskbar.vue`, `dialogueStore.openFloatingChat(agentId)` failed to pass `activeProject.value?.path`.
   - In `RollingStatusLog.vue`, `prompt_agent` was called with `{ projectPath, agentId, prompt }` instead of `{ projectPath, target, prompt }` and used a fallback defaulting to `projects[0]`.
3. **Blind Fallback in `resolveAgentProjectPath` & `AgentList.vue`:**
   - In `dialogueStore.resolveAgentProjectPath`, it scanned `coreStore.projects` from index 0 without prioritizing `coreStore.activeProject`.
   - In `AgentList.vue`, `findProjectForAgent` returned `projects[0]` on lookup failure.

---

## 2. Core Architectural Principle: Zero Agent Burden

As mandated, the AI agent (LLM model) remains completely agnostic to platform-internal identifiers, paths, and routing mechanics. The agent must never be burdened with managing its own project IDs, passing metadata back and forth, or making routing decisions. One-Ship/Wang-Kapawu owns the terminal, process lifecycle, supervisor process keys, working directories, and UI context, and handles 100% of the scoping deterministically.

---

## 3. Changes Implemented

### A. Backend Rust Core (`src-tauri/src/`)
- **Deterministic Process Supervisor (`supervisor.rs`):**
  - Updated `prompt_agent(project_path: Option<String>, target: String, prompt: String)`:
    - If `project_path` is provided, resolves the canonical key via `ProcessSupervisor::format_agent_key(&p.path, &target)`.
    - If `target == "system"`, routes deterministically to the system working directory.
    - If `project_path` is omitted, inspects active processes in `sup.processes` for an unambiguous match. If none or ambiguous, fails with an explicit error rather than guessing `projects[0]`.
  - Updated `interrupt_agent` to pass `Some(effective_path.clone())` to `prompt_agent`.
- **MCP Tool Interconnection & Envelope (`mcp.rs`):**
  - Updated `execute_report_task_status` and `execute_delegate_agent_task` to resolve the calling project strictly from `caller_cwd` and pass `project_path` to `prompt_agent`.
  - Updated `execute_post_agent_response` to inject `"project_id"` and `"project_path"` into the emitted `AgentResponse` event payload without requiring the agent LLM to produce them.
- **Conversation Persistence (`conversation.rs`):**
  - In `save_user_chat_turn_cmd`, ensured `project_id` is queried from `agent_conversations` for existing `conversation_id`, preventing foreign key project mismatches.

### B. Frontend Stores & State Management (`src/stores/dialogueStore.ts`)
- **Deterministic Project Resolution (`resolveAgentProjectPath`):**
  - 1. Checks direct registry `agentProjectMap[lower]`.
  - 2. Checks `coreStore.activeProject` first before any iterative scans.
  - 3. Checks `activeConversationRecord?.project_id`.
  - 4. Only if unambiguous across candidate projects does it resolve without active project context.
- **Priority Fallback in `getActiveProjectPath`:**
  - Updated exception fallback to check `coreStore.activeProject` first.
- **Scoped User Prompt Dispatch (`sendUserMessage` & `submitAgentPrompt`):**
  - `sendUserMessage` and `submitAgentPrompt` accept optional `explicitProjectPath`.
  - Resolves `projectPath = explicitProjectPath || resolveAgentProjectPath(agentId) || coreStore.activeProject?.path`.
  - Caches resolution into `agentProjectMap`.
  - Dispatches `invoke('prompt_agent', { projectPath, target: agentId, prompt })`.
- **Chat Open & Select Handlers (`openFloatingChat`, `selectAgent`, `updateAgentExecutionConfig`):**
  - `openFloatingChat` and `selectAgent` resolve `projectPath` with fallback to `coreStore.activeProject?.path` and update `agentProjectMap`.
  - Fixed retry prompt dispatch in `updateAgentExecutionConfig` to pass `{ projectPath, target: agentId, prompt: retryPrompt }`.
  - Fixed `startCleanSessionWithMilestone` to pass `projectPath` explicitly to `sendUserMessage`.

### C. UI Components
- **`src/kopeng/agents/components/AgentSection.vue`:**
  - Passed `project.value?.path` in `handleLaunchAndOpenAgent`, `handleAgentClick`, and context menu actions.
- **`src/components/taskbar/AgentTaskbar.vue`:**
  - Passed `activeProject.value?.path` in `handleSelectAgent` and `handleTabClick`.
- **`src/components/dialogue/FloatingChatOverlay.vue`:**
  - In `activeAgentInfo`, prioritized `activeProject.value?.id`.
  - In `send()`, passed `activeProjectPath.value || undefined` to `submitAgentPrompt`.
  - In `handleAcceptApproval`, `handleSelectOption`, and context handoff, passed `activeProjectPath.value || undefined` to `sendUserMessage`.
- **`src/components/dialogue/AgentList.vue`:**
  - In `findProjectForAgent`, prioritized `resolveAgentProjectPath` and `activeProject`, eliminating blind `projects[0]` fallback.
  - In `selectAgent`, passed resolved project path to `store.selectAgent`.
- **`src/components/dialogue/ConversationPanel.vue`:**
  - Prioritized `activeProject.value?.id` in `activeAgentInfo`.
  - Passed `activeProjectPath.value || undefined` in `send()`, `handleAcceptApproval`, and `handleSelectOption`.
- **`src/components/floating/RollingStatusLog.vue`:**
  - Replaced blind `projects[0]` loop with `dialogueStore.resolveAgentProjectPath(agentId)`.
  - Corrected IPC parameter from `agentId` to `target: agentId`.
- **`src/App.vue`:**
  - Passed `coreStore.activeProject?.path` to `dialogueStore.openFloatingChat` in the global `Alt+2` hotkey handler.

---

## 4. Verification & Validation

- `cargo check`: Rust backend compiled cleanly with 0 warnings/errors (0.49s).
- `npm run build` (`vue-tsc --noEmit && vite build`): Frontend TypeScript verification and production bundle completed successfully with 0 errors (4.50s).
- Bidirectional live spec synchronization completed across `docs/specs/08_native_process_supervisor.md`, `docs/specs/18_mcp_dialogue_channel_and_chat_overlay.md`, and `docs/specs/12_streamable_http_mcp_daemon.md`.
- No Git commands were executed; all modifications adhered to the zero dead code and zero backward-compatibility rules.
