# Development Log: 2026-09-22 - Project Initialization Lifecycle & MCP Routing Fixes

## 1. Problem Statement & Incident Forensics
During semi-attended (supervised) project initialization of `fideiuris`:
1. Phase 1 non-invasive inspection completed successfully and presented the technical proposal in Chat Overlay.
2. User reviewed and approved the proposal via interactive approval choice (`"Approve & Execute Phase 1"`).
3. The initializer agent created `rula.toml` and enriched `AGENTS.md`, concluding its execution turn by calling `post_agent_response`.
4. However, the Project Card and Project Overview UI remained indefinitely locked behind the `"ORCHESTRATION UNDER CONSTRUCTION"` blocking overlay, and the agent child process remained running in the background.

## 2. Root Cause Analysis
Four interconnected defects caused this failure:
1. **SQLite Schema Desynchronization (`project_initializations`):**
   `CREATE TABLE IF NOT EXISTS project_initializations` in `src-tauri/src/db.rs` defined `agy_conversation_id TEXT`. On existing databases created prior to this column addition, SQLite did not add the column. As a result, every `db_upsert_init_state` call failed silently with `table project_initializations has no column named agy_conversation_id`, leaving `project_initializations` completely empty.
2. **Project Target Resolution Failure in `find_project_for_agent`:**
   Because `wang-kapawu` (GUI) and `kapawu-daemon` run as separate processes, `kapawu-daemon` does not own the agent process in memory. When the initializer agent finished creating `rula.toml` and called `post_agent_response`, `find_project_for_agent` checked:
   - Live supervisor processes in daemon (none found).
   - Fallback `compute_project_state` (`should_inject_initializer = !rula_exists || is_init_actively_running`).
     Because `rula.toml` had just been written (`!rula_exists == false`) and DB state was missing due to Bug 1, `compute_project_state` omitted `initializer`.
   `find_project_for_agent` returned `None`, skipping Turn 2 persistence and bypassing the auto-detection completion block entirely.
3. **REPL Stream-JSON Child Process Teardown Gap:**
   Agents run in interactive REPL mode (`--input-format stream-json --output-format stream-json`) and do not self-terminate at turn conclusion. The background watcher loop in `src-tauri/src/kopeng/initializer.rs` broke out only on process exit without terminating the child or emitting `agent-status-report` when DB state reached `completed`.
4. **Frontend SSE Channel Disconnect:**
   `initDaemonSse()` in `src/stores/coreStore.ts` only subscribed to `ApprovalRequested` and `AgentResponse`, ignoring `agent-status-report`, `agent-step-update`, and `AccountQuotaUpdated` broadcast over the daemon's SSE bus.

## 3. Changes Applied
1. **Idempotent SQLite Column Migration (`src-tauri/src/db.rs`):**
   - Added PRAGMA table inspection in `init_database()` ensuring `agy_conversation_id TEXT` is added via `ALTER TABLE project_initializations ADD COLUMN agy_conversation_id TEXT` if missing.
2. **Robust Multi-Tier Fallback in `find_project_for_agent` (`src-tauri/src/mcp.rs`):**
   - Added Fallback 3: For `initializer` / `init`, queries `project_initializations WHERE status = 'running' ORDER BY updated_at DESC LIMIT 1`.
   - Added Fallback 4: Checks active agent conversations in SQLite (`agent_conversations WHERE agent_id = ?1 AND status = 'active'`).
3. **Coordinated Supervisor Teardown & IPC Event Emission (`src-tauri/src/kopeng/initializer.rs`):**
   - When the watcher loop detects `completed` status in SQLite, it cleans up `kapawu-initializer`, explicitly stops the supervisor process (`sup_status.stop_process(&key_clone)`), writes `last_agent_status.json`, and emits `agent-status-report` with status `"completed"`.
   - Added explicit `agent-status-report` emission on process exit or failure.
4. **SSE Event Bus Subscriptions (`src/stores/coreStore.ts`):**
   - Added SSE event listeners in `initDaemonSse` for `agent-status-report`, `agent-step-update`, and `AccountQuotaUpdated`.
5. **Database State & Process Cleanup:**
   - Applied schema alteration to `kapawu.db` and updated `project_initializations` record for `fideiuris` to `completed`.

## 4. Verification
- Backend compiled cleanly (`cargo check --manifest-path src-tauri/Cargo.toml` passed in 7.86s).
- Frontend compiled cleanly (`npm run build` / `vue-tsc` passed in 8.07s).
- Verified schema in `kapawu.db` includes `agy_conversation_id`.
