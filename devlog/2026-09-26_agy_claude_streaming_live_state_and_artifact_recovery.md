# Development Log: AGY Claude Streaming, Live State Bubble Remediation, and Artifact Recovery

**Date:** 2026-09-26  
**Topic:** Claude Sonnet AGY Runner Integration, Live Thinking/Tool UI Bubbles, MCP String Argument Resilience, and Artifact Scoping Recovery  
**Version:** v0.2.3  

---

## 1. Context & Forensic Findings

During an active session in project `estudiorc` using agent `bosmang` (powered by Claude Sonnet via the AGY runner):
1. **Missing Tool Execution & Thinking Indicators:** The chat overlay did not display the active thinking stream or the tool execution box.
2. **Missing Artifact on Disk:** The agent generated a copy revision proposal (`scratchpad/copy-revision-v2.md`), but the file was missing in the project directory, and clicking the artifact link failed.
3. **Frozen Conversation State on Turn Completion:** The agent completed all tool executions, but the turn ended with an error state not reflected in the chat. The chat remained indefinitely stopped on the user's approval message.

### Root Cause Analysis:
- **Supervisor Status Desync & Frontend Suppression:** `status_arc` in `supervisor.rs` remained `ProcessStatus::Idle` throughout AGY execution because only OpenCode updated process status. In Vue (`FloatingChatOverlay.vue` and `ConversationPanel.vue`), `isWorking` checked `if (status === 'idle') return false;` before checking `currentLiveStep`, permanently hiding the thinking and tool execution bubble.
- **Runner Internal Brain Path Divergence:** AGY instructed the model to write scratch files into its internal directory (`~/.gemini/antigravity-cli/brain/<cid>/scratch/`). Claude obeyed, creating the file outside the project repository. `post_agent_response` only searched `<project_path>/<rel>`, failing to find the file.
- **Claude Sonnet Stringified JSON Arguments:** Claude Sonnet emitted `Arguments` as an escaped JSON string with trailing characters (`"{\"agent_id\": \"bosmang\", ...}}"`) inside `call_mcp_tool`. The MCP server expected `Value::Object`, defaulted `agent_id` to `"unknown"`, and threw `WORKSPACE_SCOPING_ERROR`. Claude subsequently output its final summary to stdout.
- **Error Status Discarding Fallback Turn:** In `supervisor.rs`, `let is_interrupted_result = status == "ERROR" || ...` treated `status == "ERROR"` as a user interruption, discarding `accumulated_response` and skipping fallback ingestion.

---

## 2. Technical Implementation Summary

### A. Immediate Data Recovery
- Located the unlinked 7,914-byte artifact at `~/.gemini/antigravity-cli/brain/ab5aa7cc-91cb-4adc-83a2-19d225fb933d/scratchpad/copy-revision-v2.md`.
- Restored the file to `/home/butcherwutcher/Projects/dev/estudiorc/scratchpad/copy-revision-v2.md`.
- Updated turn 113 in `~/.config/one-ship/oneship.db` with full artifact content and inserted turn 115 containing the agent's completed response text.

### B. Frontend Live Bubble State Remediation (`FloatingChatOverlay.vue` & `ConversationPanel.vue`)
- Updated `isWorking` to immediately return `true` whenever `currentLiveStep?.state === 'ACTIVE'`.
- Preserves the live thinking stream and tool action indicator regardless of transient process status transitions.

### C. Backend Supervisor Process State & Stream Ingestion (`src-tauri/src/supervisor.rs`)
- In `send_input`: Immediately marks `proc.status` as `ProcessStatus::Working`.
- In AGY `step_update`:
  - When `state == "ACTIVE"`, sets `proc_status_arc` to `ProcessStatus::Working`.
  - Supports both `"thought"` and `"thinking"` step types, inspecting `thinking_delta`, `thinking`, `thought`, and `text_delta`.
- In AGY `result`:
  - Immediately sets `proc_status_arc` to `ProcessStatus::Idle`.
  - Refined `is_interrupted_result` to only match true cancellation (`res.get("error") == "interrupted"` or `status == "CANCELLED"`).
  - When `accumulated_response` contains text, ingests it via `persist_fallback_agent_response`.
  - When `status == "ERROR"` and output is empty, persists an explicit error chat turn so the chat never hangs silently.

### D. Resilient MCP Tool Parsing & Artifact Scoping (`src-tauri/src/mcp.rs`)
- **Stringified Argument Sanitization:** In `tools/call`, added tolerant parsing for stringified arguments, stripping extraneous trailing brackets before parsing JSON.
- **Agent ID Fallback Inference:** If `agent_id` is missing or `"unknown"`, infers it from active session tokens or registered agent names.
- **Brain Directory Recovery Fallback:** If an artifact file is not found at `<project_root>/<path>`, `check_and_recover_from_brain` automatically inspects `~/.gemini/antigravity-cli/brain/<cid>/<path>` and copies it to the project root.

### E. Template & Skill Hardening (`templates/skills/kapawu-artifacts/SKILL.md`)
- Injected explicit workspace boundary rules (`CRITICAL WORKSPACE BOUNDARY`): agents must write artifacts and scratchpads exclusively within the active project workspace root, never in internal brain paths.
- Bumped version to `v0.2.5` and propagated the updated skill directly to `estudiorc`.

---

## 3. Verification & Compliance

- **Rust Backend:** `cargo check --manifest-path src-tauri/Cargo.toml` completed cleanly with 0 errors.
- **Frontend App:** `npm run build` (`vue-tsc --noEmit && vite build`) passed with 0 errors.
- **Strict Git Ban Compliance:** No git commands executed.
