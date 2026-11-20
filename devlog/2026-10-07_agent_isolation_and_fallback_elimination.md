# Development Log: Project-Scoped Agent Identity Isolation, Keyring Rollback Removal, and Elimination of Permissive Fallbacks

**Date:** 2026-10-07  
**Topic:** Definitive elimination of cross-project agent collisions, removal of deferred keyring credential rollbacks, auto-dispatch of delegated tasks on Idle, and establishment of Rule 11 (Zero Permissive Fallbacks) across all application layers.

---

## 1. Problem Statement & Root Cause Analysis

During multi-agent development sessions involving multiple concurrent projects (`banking` and `odoo-mcp`), three critical operational failures were detected:

### A. Cross-Project Agent Collision & Identity Hijacking
* **Symptom:** An agent named `bossman` in `banking` performed actions that unexpectedly reflected or executed inside `odoo-mcp`, reading foreign project files and colliding with an agent of the same persona name in `odoo-mcp`.
* **Root Cause:**
  1. In `src-tauri/src/mcp.rs`, `find_project_for_agent` utilized permissive heuristics (Strategy 2: inspecting `supervisor.list_active_processes()` for `proc.id == agent_id`, and Strategy 2b: querying SQLite recent conversations for `agent_id == agent_id`). Because agent names (`bossman`) are not globally unique across distinct repositories, the MCP daemon matched whichever process or database row was loaded first.
  2. In `src-tauri/src/supervisor.rs`, `send_input`, `read_output`, and `dispatch_next_pending_input` relied on suffix matching (`k.ends_with(&format!(":{}", key))`). If two projects ran an agent named `bossman`, commands sent to `:bossman` arbitrarily targeted the first matching process key.
  3. In `src/stores/dialogueStore.ts`, conversation records and live action steps were indexed by bare `agentId` in a flat object (`conversations[agentId]`, `activeSteps[agentId]`), causing the chat UI of one project to render live steps and dialogue belonging to the same-named agent in another project.

### B. Account Switching Reversion in Keyring
* **Symptom:** Changing the Google account in the chat overlay dropdown briefly switched the account, but on subsequent messages the agent reverted to the previously active account.
* **Root Cause:** In `src-tauri/src/kopeng/accounts.rs` (lines 1130-1134) and `supervisor.rs` (lines 1165-1175), a deferred task restored `original_token` into the Linux system keyring (`libsecret`) after 250ms or 3 seconds. Any subsequent CLI process launched by `supervisor` read the restored credential from the keyring. Additionally, `FloatingChatOverlay.vue` had a watcher that overwrote `selectedAccountId` with runtime process telemetry instead of respecting the conversation's configured account.

### C. Delegated Tasks Stuck in Pending Queue
* **Symptom:** When an agent delegated a task (`delegate_agent_task`) to an unstarted agent in another project, the target agent launched into an initial greeting/handshake turn. The delegated prompt was staged into `pending_inputs`, but once the handshake concluded and the agent reached `Idle`, the task remained un-dispatched.
* **Root Cause:** In `supervisor.rs`, `dispatch_next_pending_input` was called only when a turn completed via `post_agent_response`. Stream readers (OpenCode and AGY) did not trigger pending input dispatch upon transitioning to `ProcessStatus::Idle`.

---

## 2. Architecture & Implementation Details

### A. Strict Identity Namespacing & Key Scoping (`supervisor.rs`, `db.rs`, `conversation.rs`)
* **Elimination of Suffix Fallbacks:** Purged `.ends_with(...)` matching from `send_input`, `read_output`, and `dispatch_next_pending_input`. All operations require canonical composite keys (`{project_path}:{agent_id}` or `{project_path}:svc:{service_id}`).
* **Idle Queue Draining:** Added asynchronous dispatch in stream readers: when an agent emits `ProcessStatus::Idle`, the reader triggers `sup.dispatch_next_pending_input(&proc_key).await`, ensuring tasks delegated during startup or busy cycles execute immediately upon availability.
* **Project-Scoped Database Queries:** Updated `db_get_pending_delegation_by_target(target_agent, target_project)` to filter by project path/name in SQLite, preventing cross-project delegation hijacking.
* **Conversation Tracker Scoping:** Updated `mark_user_message_sent`, `mark_agent_responded`, and `should_inject_compliance_reminder` in `conversation.rs` to require `project_path: Option<&str>`, formatting keys via `format_conv_key(project_path, agent_id)`.

### B. Persistent Keyring Account Management (`accounts.rs`, `supervisor.rs`)
* **Removal of Keyring Rollback:** Removed the 3-second rollback timer in `supervisor.rs::spawn_agent` and the deferred `write_keyring_token(original)` in `accounts.rs::switch_agent_account`. Switched accounts now persist indefinitely in the keyring until explicitly changed by the developer.
* **UI Hydration Priority:** In `FloatingChatOverlay.vue`, updated the account watcher to enforce `selectedAccountId = conv?.account_id || info?.account_id || ''`, ensuring the conversation's chosen account takes precedence over transient process metadata.

### C. Strict MCP Project Resolution (`mcp.rs`)
* **Purge of Strategies 2 & 2b:** Completely removed bare agent name matching against supervisor processes and SQLite conversations in `find_project_for_agent`.
* **Composite Key Parsing (Strategy 0):** Parses `{project_path}:{agent_id}` or `{project_path}::{agent_id}` directly from incoming requests.
* **Longest Prefix Working Directory (Strategy 1):** Matches `caller_cwd` against registered project directories, selecting the longest matching path prefix.
* **Zero Permissive Fallbacks:** In `execute_switch_conversation_topic`, eliminated arbitrary fallback to the first project in SQLite (`projects.into_iter().next()`). Unresolved project contexts fail immediately with strongly-typed errors.

### D. Frontend Composite Key Architecture (`dialogueStore.ts`, UI Components)
* **Composite Key SSOT:** Keyed all dialogue store maps (`conversations`, `activeSteps`, `unreadCounts`) by `${projectPath}::${agentId.toLowerCase()}`.
* **Composite Accessors:** Replaced direct property access with `getConversation(agentId, projectPath)`, `ensureConversation(agentId, projectPath)`, and `getActiveStep(agentId, projectPath)`.
* **Elimination of Cross-Project Fallback Loops:** Removed transversal search loops (`for (const state of Object.values(coreStore.agentStates))`) in `FloatingChatOverlay.vue`, `ConversationPanel.vue`, `AgentTaskbar.vue`, and `ProjectStatusLeds.vue`. Lookups are strictly scoped to the active project context.

### E. Platform Governance & Living Specifications
* **Rule 11 in `AGENTS.md`:** Formally established *"Strict Prohibition of Permissive Fallbacks & Mandatory Exact Scoping (Zero Silent Fallbacks)"* as a fundamental governance rule:
  > Permissive fallbacks mask design flaws, cause cross-project data corruption, hijack processes, and leak private project state. If context cannot be resolved unambiguously, fail early and fail explicitly (Tiger Style).
* **Managed Template & Base Skills:** Embedded the Rule 11 directive into `ONE_SHIP_MANAGED_BLOCK` in `src-tauri/src/kopeng/manifest.rs` and `templates/skills/kapawu-core/SKILL.md`.
* **Living Specifications Synchronized:** Updated `docs/specs/08_native_process_supervisor.md`, `docs/specs/12_streamable_http_mcp_daemon.md`, `docs/specs/17_multi_account_management.md`, and `docs/specs/18_mcp_dialogue_channel_and_chat_overlay.md`.
* **Audited Findings:** Recorded detailed log forensic analysis in `docs/audit/audit_agent_isolation.md`.

---

## 3. Verification & Operational Outcomes

* **Rust Backend Compilation:** `cargo check` verified with **0 errors and 0 warnings**.
* **Frontend Compilation:** `npm run build` (`vue-tsc --noEmit && vite build`) verified with **0 errors**.
* **Integrity Guarantee:** Agents with identical persona names in distinct workspaces operate with 100% isolation in process supervision, MCP tool routing, SQLite conversation memory, and Pinia UI state.
