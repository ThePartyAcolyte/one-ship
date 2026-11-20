# Devlog: Native Process Supervisor & Complete Herder Decoupling

**Date:** 2026-09-15  
**Topic:** Herder Decoupling, Native Tokio Process Supervisor, Full Zero Backward Compatibility Purge  
**Status:** Completed & Validated  

---

## 1. Overview & Context

Wang-Kapawu was initially reliant on the external Herder terminal multiplexer (`herdr.sock`, Protocol 20) for agent pane management, terminal tab focusing, and command injection.

With the evolution of Wang-Kapawu's integrated Chat Overlay, Dialogue Channel, and Document Hub, external terminal pane multiplexing became redundant, introduced socket IPC overhead, and constrained agent lifecycle observability.

We designed and executed a complete architectural transition to an internal, asynchronous **Tokio Process Supervisor** in Rust (`src-tauri/src/supervisor.rs`), completely replacing the external Herder dependency across the backend and frontend while adhering to a strict **Zero Backward Compatibility & Zero Dead Code Policy**.

---

## 2. Key Changes Made

### 2.1 Backend Architecture (`src-tauri/`)
- **Native `ProcessSupervisor` (`src-tauri/src/supervisor.rs`):**
  - Child process management via `tokio::process::Command` with asynchronous stdout/stderr line streaming into 500-line circular ring buffers (`VecDeque<String>`).
  - Stdin pipe injection via `tokio::process::ChildStdin`.
  - Process retention invariant: Process termination preserves the output ring buffer with `ProcessStatus::Completed { exit_code }` or `ProcessStatus::Failed { error }` for live user inspection until explicit removal or restart.
  - Native Tauri commands: `launch_agent`, `stop_agent`, `restart_agent`, `prompt_agent`, `read_process_output_cmd`, `get_project_agent_state_cmd`, `reset_all_project_agents`, `dispatch_git_instruction`.
  - Background state watcher (`spawn_agent_state_watcher`) emitting `agent-states-updated`.
- **Pure Module Decoupling:**
  - Extracted independent modules: `kopeng/dna.rs`, `kopeng/errors.rs`, `kopeng/fs_tree.rs`, `kopeng/persona.rs`, `kopeng/prompt.rs`.
  - Purged obsolete legacy modules: `src-tauri/src/kopeng/herdr.rs` and `src-tauri/src/kopeng/herdr_socket.rs`.
  - Decoupled callers across `orchestrator.rs`, `profiler.rs`, `skill.rs`, `accounts.rs`, `stats.rs`, `telemetry.rs`, `conversation.rs`, and `mcp.rs`.

### 2.2 Frontend Architecture (`src/`)
- **Store Modernization (`src/stores/coreStore.ts` & `src/stores/dialogueStore.ts`):**
  - Purged obsolete `herdrStates` and `herdr-states-updated` listener.
  - Unified strictly on `agentStates` and `agent-states-updated`.
- **UI Components:**
  - `CardHeader.vue`: Replaced Herder workspace connection indicator with **Process Supervisor Status Indicator** using `hasActiveProcesses`.
  - `HerderSection.vue` (Agent Card Body):
    - Replaced `launch_herdr_agent` with `launch_agent`.
    - Replaced `restart_herdr_agent` with `restart_agent`.
    - Replaced `close_herdr_tab` with `stop_agent`.
    - Replaced terminal tab focusing with direct Chat Overlay triggering (`dialogueStore.openOverlay(agent.id)`).
  - `ConversationPanel.vue` & `AgentList.vue`: Replaced `launch_herdr_agent` with `launch_agent`.
  - `RollingStatusLog.vue`: Replaced `prompt_herdr_agent` with `prompt_agent`.
  - `ProjectCard.vue`: Updated props from `herdrState` to `agentState`.
  - `AddPersonaModal.vue`: Renamed `HerderAgentState` to `AgentPersonaState` and cleaned up descriptive text.

### 2.3 Documentation & Governance
- **`AGENTS.md`**: Updated execution boundary from Herder workspace to native Process Supervisor.
- **`TODO.md`**: Recorded completion of Phase 12.
- **`docs/specs/08_herder_agent_orchestration.md`**: Rewritten as Spec 08 v2.0 (Native Process Supervisor & Autonomous Agent Orchestration).
- **`docs/specs/15_herder_unix_socket_driver_migration.md`**: Marked as historically superseded.

---

## 3. Verification & Validation

1. **Frontend Type & Template Check:**
   - `npx vue-tsc --noEmit` exited with code 0 (0 errors).
2. **Production Bundle Build:**
   - `npm run build` completed successfully in 4.38s.
3. **Backend Unit Tests:**
   - `cargo test --lib` passed all 19 tests in 0.16s (`supervisor::tests::test_supervisor_spawn_and_read_output` verified).
