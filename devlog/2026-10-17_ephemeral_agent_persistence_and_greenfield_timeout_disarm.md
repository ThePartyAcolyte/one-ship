# Development Session: 2026-10-17 — Ephemeral Agent Persistence & Greenfield Timeout Disarm

## Context & Objectives
- **Reported Incident:** During greenfield project onboarding (test project "Watchtower"), the `initializer` agent launched and welcomed the developer. After presenting the initial architectural vision and project goal, the developer switched to another project to inspect an agent, then navigated back.
- **Observed Failure:**
  1. The `initializer` agent was no longer active or attached to the project.
  2. The agent completely disappeared from the project taskbar (`AgentTaskbar.vue`), rendering "0 AGENTS".
  3. The Floating Chat Overlay was dismissed and the underlying process appeared cancelled or lost.
- **Architectural Scope:** The issue was not limited to `initializer`: all ephemeral agents (`docs-resolver`, `todos-resolver`, `techdebt-resolver`, `profiler-crafter`) shared similar lifecycle visibility vulnerabilities when transitioning to `Idle` while waiting for multi-turn user responses.

---

## Root Cause Analysis

1. **Autonomous Suicide Loop in `src-tauri/src/kopeng/initializer.rs`:**
   - In `spawn_initializer_completion_watcher`, a polling loop operated with `MAX_INIT_POLL_ATTEMPTS_COUNT = 1800` at `poll_step_ms = 250ms`.
   - After exactly 450 seconds (7.5 minutes), the loop assumed an unhandled timeout, dispatched `SIGKILL` to the child process group, deleted the `oneship-initializer` skill symlink, and updated SQLite `project_initializations.status = 'failed'`.
   - **Architectural Violation:** Per Spec 07 and Spec 26, greenfield initialization is strictly an interactive conversational architectural pairing session in the Floating Chat Overlay. Running an autonomous suicide watchdog on an interactive agent directly violated the Universal Headless Multi-Turn Agent Protocol (`stream-json`, Rule 9).

2. **`compute_project_state` Ephemeral Stripping (`src-tauri/src/supervisor.rs`):**
   - Step 3 evaluated `is_init_actively_running` strictly via `matches!(init_status, ProcessStatus::Working | ProcessStatus::Starting)`.
   - Once an agent finished processing Turn 1 and entered `ProcessStatus::Idle` (waiting for developer turn), `compute_project_state` stripped `initializer` from `state.agents`.

3. **Taskbar Visibility Masking (`src/components/taskbar/AgentTaskbar.vue`):**
   - The `activeResolvers` computed list required `ag.status === 'working' || ag.status === 'running'`.
   - When an ephemeral agent was in `Idle`, it was excluded from `activeResolvers`.
   - Furthermore, because `isEphemeralOrInit(ag.id)` excluded ephemeral personas from `domainAgents`, the agent completely vanished from the taskbar, displaying "0 AGENTS".
   - `isAgentBelongsToActiveProject` also dropped `initializer` if `has_config` was true or the process was `Idle`.

4. **Premature Floating Chat Dismissal (`src/stores/dialogueStore.ts`):**
   - In `switchProjectContext`, the store verified if `floatingChatAgentId` belonged to the newly selected project via `st?.agents?.some(...)`.
   - Because `compute_project_state` omitted `initializer` in `Idle`, `belongs` evaluated to `false`, clearing `floatingChatAgentId = null` and closing the floating chat window.

---

## Key Changes & Implementations

### 1. Disarming Greenfield Timeout & Timeout Duration Fix (`src-tauri/src/kopeng/initializer.rs`)
- In `spawn_initializer_completion_watcher`:
  - When `mode == "greenfield"`, disarmed the completion polling watchdog completely via early `return;`. Greenfield sessions remain active indefinitely until the developer concludes or stops the agent.
  - For non-greenfield modes, fixed the attempt count calculation to `(TIMEOUT_DURATION_MS / poll_step_ms.max(50)) as u32`, guaranteeing an authentic 15-minute timeout window regardless of configured polling interval.

### 2. Multi-Turn Idle State Retention in Supervisor (`src-tauri/src/supervisor.rs`)
- Updated Step 3 of `compute_project_state`:
  ```rust
  let is_init_actively_running = matches!(
      init_status,
      ProcessStatus::Working | ProcessStatus::Starting | ProcessStatus::Idle | ProcessStatus::Blocked
  );
  ```
- Ensures that when `initializer` is alive in `Idle` or `Blocked` awaiting developer instructions, it remains registered in `state.agents`.

### 3. Ephemeral Taskbar Visibility (`src/components/taskbar/AgentTaskbar.vue`)
- Updated `activeResolvers` to use `isAgentProcessActive(ag.id)` (checking `working`, `running`, `idle`, `starting`, `blocked`), `hasPendingFlag(ag.id)`, and `!activeProject.value.has_config` for `initializer`.
- Updated `isAgentBelongsToActiveProject` so ephemeral agents and resolvers with active processes or pending flags remain linked to the active project context.

### 4. Non-Destructive Project Context Switching (`src/stores/dialogueStore.ts`)
- In `switchProjectContext`, updated the `belongs` check:
  ```typescript
  const isTargetEphemeral =
    (isEphemeralOrInit(saved.floatingChatAgentId) || isFlagResolver(saved.floatingChatAgentId)) &&
    saved.floatingChatProjectPath === targetPath
  const belongs = isTargetEphemeral || st?.agents?.some((a) => a.id.toLowerCase() === saved.floatingChatAgentId?.toLowerCase())
  ```
- Preserves the open floating chat overlay across project switches if the active ephemeral session corresponds to the target project.

### 5. Living Specifications Synchronization
- Updated `docs/specs/26_greenfield_project_creation_and_conversational_bootstrapping.md`: Added rule 7 in Section 4.2 and Section 5 documenting timeout disarming and context continuity.
- Updated `docs/specs/08_native_process_supervisor.md`: Synchronized Section 2.3 and Section 5 detailing ephemeral process retention, `is_init_actively_running` states, and taskbar multi-turn persistence.

---

## Verification & Compilation
- `cargo check`: Passed with 0 errors across the entire backend workspace.
- `cargo test --lib supervisor::tests`: All 6 tests passed cleanly.
- `npm run build` (`vue-tsc --noEmit && vite build`): Passed cleanly with 0 TypeScript or template compilation errors.
