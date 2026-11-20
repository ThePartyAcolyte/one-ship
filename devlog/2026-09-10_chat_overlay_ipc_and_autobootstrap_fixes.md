# Devlog: 2026-09-10 — Chat Overlay IPC Alignment, Daemon SSE Routing & Agent Auto-Bootstrap

---

## 1. Problem Identification

When attempting to open and interact with AI agents through the unified Agent Dialogue workspace (`AgentDialogueWorkspace`), sent prompts did not reach the target agents, and no responses appeared in the UI. A root-cause diagnosis uncovered 5 interconnected failures across layers:

1. **IPC Parameter Mismatch in `dialogueStore.ts`:**
   - Rust command `prompt_herdr_agent(target: String, prompt: String)` was being invoked with `{ agentId, message }`.
   - Tauri v2 IPC rejected the invocation due to missing fields, silently caught and ignored in the frontend console.
2. **Missing Auto-Bootstrap in `prompt_herdr_agent`:**
   - For agents not yet launched in Herder, `resolve_herdr_target` returned the bare string (without pane/tab delimiter `:`).
   - Unlike `delegate_agent_task`, `prompt_herdr_agent` lacked auto-bootstrap logic, immediately erroring out with `target not found`.
3. **Dropped SSE `AgentResponse` Events:**
   - With the standalone `kapawu-daemon` running on port 8765, agent responses were broadcast via SSE (`/api/events`).
   - `coreStore.ts` received SSE events but `handleDaemonEvent` lacked a `case "AgentResponse"` handler, dropping incoming responses.
4. **Disabled MCP Dialogue Mode in Configuration:**
   - `rula.toml` lacked `mcp_dialogue_mode = true` under `[system]`, causing initialized agents to skip the `post_agent_response` response protocol.
5. **UI Inability to Launch Agents from Dialogue Workspace:**
   - `AgentList.vue` and `ConversationPanel.vue` displayed unlaunched agents without launch affordances or status indicators.

---

## 2. Work Accomplished

### 2.1 Frontend IPC & Store Layer (`src/stores/`)
- **`dialogueStore.ts`:**
  - Aligned invoke arguments to `{ target: agentId, prompt: text.trim() }`.
  - Added error forwarding to `coreStore.addStatusLog` on dispatch failure.
  - Implemented message deduplication in `handleAgentResponse` across IPC and SSE channels.
- **`coreStore.ts`:**
  - Added `case "AgentResponse"` in `handleDaemonEvent`, routing SSE responses directly to `dialogueStore.handleAgentResponse`.

### 2.2 Backend Auto-Bootstrap & Tracking (`src-tauri/src/kopeng/herdr.rs`)
- Enhanced `prompt_herdr_agent` with auto-bootstrap fallback: when an agent is unlaunched, it dynamically locates the associated project, invokes `launch_herdr_agent`, waits for initialization, and re-resolves the pane target.
- Added call to `crate::conversation::mark_user_message_sent(&target)` for orchestrator compliance tracking.

### 2.3 UI Ergonomics (`src/components/dialogue/`)
- **`AgentList.vue`:**
  - Added symbol-only `<Play>` launch button for unlaunched agents, wrapped in `<AppTooltip text="Launch agent in Herder">`.
  - Added `launchAgent` handler with status log telemetry.
- **`ConversationPanel.vue`:**
  - Added live agent lifecycle status badge (`unlaunched`, `idle`, `working`) in conversation header.
  - Added explicit `Launch` button for unlaunched agents in header.
  - Updated empty-state and textarea placeholder to inform the user that sending a message will automatically bootstrap the agent.

### 2.4 Configuration & Specs
- Enabled `mcp_dialogue_mode = true` under `[system]` in `rula.toml`.
- Synchronized `docs/specs/18_mcp_dialogue_channel_and_chat_overlay.md` Phase 2B completion.

---

## 3. Technical Health Verification

- Backend: `cargo check --manifest-path src-tauri/Cargo.toml` passed with 0 errors (0.44s).
- Frontend: `npx vue-tsc --noEmit` passed with 0 errors.
