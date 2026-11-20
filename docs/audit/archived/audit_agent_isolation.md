# Comprehensive Architectural Audit: Agent Scoping & Identity Isolation

**Date:** 2026-10-07  
**Scope:** Backend (`src-tauri/`), Frontend UI/Stores (`src/`), Database Schema (`oneship.db`), MCP Tools (`src-tauri/src/mcp.rs`)  
**Status:** Complete Forensic Investigation & Codebase Audit  

---

## 1. Executive Summary & Verdict

### The Core Question
> *"¿Existe este problema de estar invocando agentes por nombre como si los nombres fueran IDs, y no asociándolos al proyecto en el fondo para darles una identidad que realmente sea irrepetible?"*

### Verdict: **YES. The defect is widespread, systemic, and present across both Backend and Frontend.**

While the native Process Supervisor conceptually planned for a composite key (`{project_path}:{agent_id}`), the application has suffered from pervasive architectural leakage where bare `agent_id` strings (e.g. `"bosmang"`, `"frontend"`, `"git"`) are treated as globally unique identifiers. 

Because standard personas are reused across multiple project configurations (`rula.toml`), having two or more active projects in ONE-SHIP triggers:
1. **Cross-Project Process Hijacking:** Supervisor commands send input or inspect output of the wrong project's agent due to permissive suffix matching (`ends_with(":{agent_id}")`).
2. **UI & Event State Pollution:** Pinia stores (`dialogueStore`, `coreStore`) and reactive components index live streaming steps, unread badges, and chat message histories using flat `agentId` keys without project qualification.
3. **Ghost State & Cross-Project Leakage:** An agent working in Project A emits step updates that render directly inside the open chat overlay of Project B if both agents share a role name (e.g. `bosmang`).
4. **Keyring Credential Reversions:** Token switching in Linux Keyring uses an aggressive 3-second timeout that restores the global token, causing subsequent API calls to fail or revert accounts.
5. **Delegated Tasks Trapped in Memory:** Delegations to unstarted agents launch the agent into a `Working` state (initial greeting turn) and enqueue the instruction in memory, but no mechanism dispatches the queued prompt upon transitioning to `Idle`.

---

## 2. Forensic Analysis of the Real-World Incident

During runtime operations between `l10n_cl_banking` (Project ID 8) and `odoo-mcp` (Project ID 9), three interconnected failures produced the appearance of total system corruption:

### Incident A: Google Account Reversion (`thepartyacolyte` vs `von.carpentier`)
- **Evidence in `oneship.4.log` & SQLite `accounts`:**
  - Account `b637cda0-...` (`primary@example.com`) is stored with `is_active = 0`.
  - Account `d9bfc59a-...` (`secondary@example.com`) is stored with `is_active = 1`.
  - Conversation `conv-8-bosmang-e3a75437` had its config updated to `b637cda0-...`.
  - Log trail:
    ```
    [1791392565] [DEBUG] [db] db_update_conversation_config: conversation_id=conv-8-bosmang-e3a75437, account_id=Some("b637cda0-...")
    [1791392565] [DEBUG] [supervisor] Spawning agent 'bosmang' in '/home/.../l10n_cl_banking'
    [1791392568] [WARN]  [supervisor] Agent 'bosmang' AGY init handshake timeout (3s): restoring previous keyring token
    [1791392564] [WARN]  [supervisor] Agent 'bosmang' terminated with ERROR and no text: There was a network issue connecting to the server, please try again.
    ```
- **Root Cause 1 (Backend Keyring):** In `supervisor.rs:1154-1195` and `accounts.rs:1130-1133`, switching an agent's account temporarily writes its refresh token to the system keyring, spawns `agy`, and after 3 seconds **restores the previous keyring token**. Subsequent token refreshes or turns by `agy` re-read the system keyring and fail or revert.
- **Root Cause 2 (Frontend Watcher Hydration):** In `FloatingChatOverlay.vue:784-807`, the config watcher evaluates:
  ```typescript
  selectedAccountId.value = info?.account_id || conv?.account_id || '';
  ```
  `info?.account_id` (the running backend process account) takes priority over `conv?.account_id` (the conversation's desired account). Every 2 seconds, the `agent-states-updated` event broadcasts the process state, overwriting the user's dropdown back to the running process's account.

---

### Incident B: Cross-Project Agent Confusion (`bosmang` in Banking vs `odoo-mcp`)
- **Evidence in Supervisor Registry & PIDs:**
  - Process 1: `/home/.../l10n_cl_banking:bosmang` (PID 151684 / PID 130170).
  - Process 2: `/home/.../odoo-mcp:bosmang` (PID 134712).
- **Root Cause 1 (Flat Store Maps):** In `dialogueStore.ts`, the core reactive maps are keyed only by `agentId`:
  ```typescript
  conversations: Record<string, DialogueConversationState> // this.conversations[agentId]
  activeSteps: Record<string, LiveAgentStep | null>         // this.activeSteps[agentId.toLowerCase()]
  agentProjectMap: Record<string, string>                  // this.agentProjectMap[agentId.toLowerCase()]
  ```
  When `bosmang` in `l10n_cl_banking` performed work (reading files, executing scripts), it emitted `agent-step-update` events with `agent_id: "bosmang"`. Because `FloatingChatOverlay.vue:106` computes `currentLiveStep` from `activeSteps[agentId.toLowerCase()]`, viewing `bosmang` in `odoo-mcp` displayed the live execution steps and file reads of `l10n_cl_banking`.
- **Root Cause 2 (Project Resolution Pollution):** In `dialogueStore.ts:1080`:
  ```typescript
  const projectPath = explicitProjectPath || this.resolveAgentProjectPath(agentId) || coreStore.activeProject?.path;
  ```
  `resolveAgentProjectPath("bosmang")` returned `agentProjectMap["bosmang"]`, which was set to `/home/.../l10n_cl_banking`. Any message sent while viewing `odoo-mcp` was dispatched to the Banking project.
- **Root Cause 3 (Permissive Fallback Loop in UI):** In `FloatingChatOverlay.vue:82-85` and `ConversationPanel.vue:97`:
  ```typescript
  for (const state of Object.values(coreStore.agentStates as Record<string, any>)) {
    const found = state?.agents?.find((a: any) => a.id.toLowerCase() === agentId.value?.toLowerCase());
    if (found) return found;
  }
  ```
  If the active project had not finished polling, the UI scanned all registered projects and bound the first project matching `"bosmang"`.

---

### Incident C: Trapped Delegated Tasks (`task-1791299677`)
- **Evidence in SQLite `agent_delegations` & `agent_chat_turns`:**
  - Turn 214 of Banking `bosmang`: Formally dispatched `task-1791299677` to `frontend`.
  - Turn 213 of Banking `frontend`: Only logged greeting `"Online and ready for instructions. Standing by to assist with OWL components..."`.
  - The task remained stuck in `status = 'pending'`. The delegated instruction was never delivered to `frontend`'s stdin.
- **Root Cause 1 (Missing Dispatch on Turn Completion):** When `prompt_agent` launches an unstarted agent, `launch_agent` spawns it with the persona greeting prompt, putting its status in `ProcessStatus::Working`. `prompt_agent` then calls `send_input`, which enqueues the delegated instruction into `pending_inputs`. When the agent finishes its greeting turn and becomes `Idle` (lines 1623 and 1904 of `supervisor.rs`), **nothing ever invokes `dispatch_next_pending_input`**. The prompt remains trapped in memory indefinitely.
- **Root Cause 2 (Missing `.await` on MCP Callbacks):** In `mcp.rs:2089` and `mcp.rs:3715`:
  ```rust
  let _ = crate::supervisor::get_supervisor().dispatch_next_pending_input(&key);
  ```
  `dispatch_next_pending_input` is an `async fn`. Calling it without `.await` creates an unpolled Future that is immediately dropped, executing zero instructions.
- **Root Cause 3 (Unrecorded User Turn in SQLite):** `prompt_agent` sends raw input to the process pipe without saving a `role: "user"` turn in SQLite `agent_chat_turns`. Thus, even if the agent were to run, the chat conversation UI renders completely blank with no trace of the incoming delegated prompt.

---

## 3. Exhaustive Inventory of Un-Scoped Sites in Rust Backend

### A. Process Supervision (`supervisor.rs`)
1. **L445, L493, L543 (`read_output`, `send_input`, `dispatch_next_pending_input`)**:
   ```rust
   procs.get(key).or_else(|| {
       procs.iter()
           .find(|(k, _)| k.ends_with(&format!(":{}", key)) || k.as_str() == key)
           .map(|(_, p)| ...)
   })
   ```
   *Flaw:* Allows callers to pass a bare `agent_id` instead of `{project_path}:{agent_id}`. Suffix search arbitrarily selects whichever matching process is first in hash map order.
2. **L619, L2122, L2200, L2224, L279 (Event Emissions)**:
   Emits `agent-status-report`, `agent-error`, and `agent-step-update` payloads containing bare `"agent_id": proc.id` without `project_path` or `project_id`.
3. **L3051 (`get_last_agent_status`)**:
   Reads a single global file `last_agent_status.json` with zero project segregation.
4. **L3059 (`read_process_output_cmd`)**:
   Takes `target: String`, passing directly to `sup.read_output(&target)` without project context.
5. **L3069, L3094 (`read_terminal_output`, `send_terminal_input`)**:
   Takes `target: Option<String>` without `project_path`, falling back to suffix matching across all projects.

### B. Dialogue Compliance Tracker (`conversation.rs`)
1. **L35-36 (Global In-Memory Map)**:
   ```rust
   static STATE: OnceLock<Mutex<HashMap<String, AgentConversationState>>> = OnceLock::new();
   ```
   Keys are bare `agent_id` strings (`"frontend"`, `"bosmang"`).
2. **L51, L65, L90, L123 (`mark_user_message_sent`, `mark_agent_responded`, `should_inject_compliance_reminder`, `clear_agent_state`)**:
   All public tracking functions accept only `agent_id: &str`.
3. **L76 (`has_recent_db_response`)**:
   ```sql
   SELECT COUNT(*) FROM agent_chat_turns WHERE agent_id = ?1 AND role = 'agent' AND timestamp >= ?2
   ```
   *Flaw:* Omits `project_id`. A response by `"git"` in Project A satisfies the compliance check for `"git"` in Project B.

### C. MCP Tools & Heuristics (`mcp.rs`)
1. **L1466-1643 (`find_project_for_agent`)**:
   - *Strategy 2:* Scans `sup.list_active_processes()` for `proc.id == agent_id`. The first active process matching the name wins, regardless of project.
   - *Strategy 2b:* Queries `agent_conversations` by bare `agent_id` with `ORDER BY updated_at DESC LIMIT 1`.
2. **L2047 (`report_task_status`)**:
   Queries `db_get_pending_delegation_by_target(&agent_id)` with zero project qualification.
3. **L2096-2115 (Audit Journal in `report_task_status`)**:
   Loops through registered projects and logs the audit event to the *first* project containing an agent with that name.
4. **L4076-4085 (`switch_conversation_topic`)**:
   When project resolution fails, falls back to `projects.into_iter().next()`, binding the topic switch to an arbitrary project.
5. **L2089, L3715**:
   Missing `.await` on `dispatch_next_pending_input(&key)`.

### D. SQLite Persistence (`db.rs`)
1. **L1376 (`db_get_pending_delegation_by_target`)**:
   ```sql
   SELECT task_id, ... FROM agent_delegations 
   WHERE LOWER(target_agent) = LOWER(?1) AND status = 'pending' 
   ORDER BY dispatched_at DESC LIMIT 1
   ```
   *Flaw:* Missing `AND (target_project = ?2 OR target_project IS NULL)`.
2. **Schema: `agent_delegations`**:
   Stores `originator_project TEXT` and `target_project TEXT` as unvalidated strings instead of foreign keys referencing `projects(id)`.

### E. Telemetry & Stats (`kopeng/stats.rs`)
1. **L802 (Separator Bug & Fallback)**:
   Attempts `ak.split_once("::")`. Since `supervisor.rs` formats keys with a single colon (`path:agent_id`), this always fails and triggers an un-scoped fallback query on `agent_chat_turns` filtered only by `agent_id`.
2. **L918 (`get_all_active_agent_metrics`)**:
   Iterates through projects and active agents, but inside the loop invokes `get_agent_metrics(None, None)` without passing the agent or project, returning identical un-scoped data for every agent.

---

## 4. Exhaustive Inventory of Un-Scoped Sites in Vue/TypeScript Frontend

### A. Pinia Dialogue Store (`src/stores/dialogueStore.ts`)
1. **L485 (`conversations: Record<string, DialogueConversationState>`)**:
   Keyed solely by `agentId`. Messages from multiple projects sharing an agent name are merged into the same array.
2. **L486 (`activeSteps: Record<string, LiveAgentStep | null>`)**:
   Keyed solely by `agentId.toLowerCase()`. Live steps from one project render in another project's view.
3. **L487 (`agentProjectMap: Record<string, string>`)**:
   Flat map of `agentId -> projectPath`. Opening an agent in Project B permanently points all future lookups to Project B.
4. **L275-301 (`getActiveProjectPath`)**:
   Falls back to scanning `coreStore.projects` and returning the first project that declares the agent name.
5. **L560-600 (`resolveAgentProjectPath`)**:
   If candidate projects > 1 and active project is not matched, returns `candidateProjects[0]` as fallback.
6. **L639 (`listen('agent-conversation-switched')`)**:
   Filters on `payload.agent_id === this.activeAgentId` without checking `payload.project_path` or `payload.project_id`.
7. **L805 (`listen('agent-status-report')`)**:
   Clears errors in `this.agentErrors[payload.agent_id.toLowerCase()]` without project qualification.

### B. Pinia Core Store (`src/stores/coreStore.ts`)
1. **L585 (`agentOutcomes: Record<string, AgentOutcome>`)**:
   Keyed solely by bare `agent_id`.

### C. UI Components
1. **`FloatingChatOverlay.vue:82-85`**:
   Iterates `Object.values(coreStore.agentStates)` searching for `agentId` across all projects.
2. **`ConversationPanel.vue:97`**:
   Identical un-scoped fallback loop across all projects.
3. **`AgentTaskbar.vue:140, 294`**:
   Lookups in `dialogueStore.activeSteps[agentId.toLowerCase()]` display live status indicators from the wrong project.
4. **`ProjectStatusLeds.vue:31, 59`**:
   Queries `dialogueStore.conversations[ag.id]` and `dialogueStore.activeSteps[ag.id]` globally.

---

## 5. Architectural Remediation Plan

To establish absolute project-agent isolation and zero ambiguity, the following design rules must be implemented atomically:

### Rule 1: Canonical Scoped Key Standard
Every agent reference that crosses boundaries (IPC, events, store keys, memory maps) must use the composite identifier:
- **Rust Backend:** `{project_path}:{agent_id}` (already used in `ProcessSupervisor::format_agent_key`).
- **Frontend Pinia:** `{project_id}::{agent_id.toLowerCase()}` or `{project_path}::{agent_id.toLowerCase()}`.
- Bare `agent_id` strings are strictly prohibited as map keys.

### Rule 2: Elimination of Permissive Suffix Fallbacks
Remove `.find(|(k, _)| k.ends_with(&format!(":{}", key)))` in `supervisor.rs` (`send_input`, `read_output`, `dispatch_next_pending_input`). If a command does not provide a valid project-scoped key, it must fail explicitly.

### Rule 3: Event Scoping Guarantee
Every supervisor event (`agent-status-report`, `agent-step-update`, `agent-error`, `agent-usage-update`) must include `project_path` and `project_id` in its payload.

### Rule 4: Dialogue Compliance Scoping
Refactor `conversation.rs` to store state in `HashMap<(String, String), AgentConversationState>` keyed by `(project_path, agent_id)`. Update SQL queries to include `AND project_id = ?`.

### Rule 5: Autonomous Delegation Turn Dispatch
In `supervisor.rs` stream readers (both `agy` and `opencode`), whenever a process transitions to `ProcessStatus::Idle`, call `sup.dispatch_next_pending_input(&key).await`. In `prompt_agent`, record an entry in `agent_chat_turns` (`role = 'user'`) so that delegated instructions are immediately visible in the UI.

### Rule 6: UI Store Isolation
Refactor `dialogueStore.ts`:
- Change `conversations` to `Record<string, DialogueConversationState>` where key is `${projectPath}::${agentId.toLowerCase()}`.
- Change `activeSteps` to `${projectPath}::${agentId.toLowerCase()}`.
- Remove flat `agentProjectMap` and un-scoped cross-project loops (`for (const state of Object.values(...))`).
- Invert watcher precedence in `FloatingChatOverlay.vue`: `conv?.account_id || info?.account_id`.
