# Devlog: Dynamic Approval Options & Multi-Turn Flag Resolution Protocol

**Date:** 2026-09-17  
**Topic:** Dynamic Human-in-the-Loop Approval Options & Multi-Turn Interactive Flag Resolution Protocol

---

## 1. Problem Statement & Context
During testing of Phase 2 flag resolution agents (`todos-resolver`, `techdebt-resolver`, `profiler-crafter`, `docs-resolver`):
1. **Premature Task Closure:** In `todos-resolver`, the agent closed the database flag (`resolve_initialization_flag(flag_id)`) during its initial reconnaissance turn, clearing the UI badge before the developer had a chance to choose a cleanup strategy (Purge vs Annotate vs Preserve).
2. **Dropped Approval Payload:** `post_agent_response` in `mcp.rs` did not declare or forward the `approval` object to `AgentResponse` SSE events.
3. **Binary-Only UI Actions:** The approval card in `ConversationPanel.vue` only offered generic `[ Accept Proposal ]` / `[ Request Changes ]` buttons rather than rendering dynamic, contextual option buttons.

---

## 2. Key Architecture & Engineering Changes

### A. Backend MCP Handler (`src-tauri/src/mcp.rs` & `lib.rs`)
- **`#![recursion_limit = "512"]`:** Added to `lib.rs` to allow large JSON macro expansion for the full MCP tool schema suite.
- **`post_agent_response` Approval Schema:** Added `approval` object (`title: string`, `description?: string`, `options?: string[]`) to the tool's `inputSchema`.
- **Forwarding in `execute_post_agent_response`:** Extracted `approval` from args and included it in the `AgentResponse` payload.
- **Strict Invariants on `resolve_initialization_flag`:** Updated tool documentation and server log messages to reinforce that flag resolution must only occur after developer confirmation and execution.

### B. Frontend Dynamic Option Badges (`src/components/dialogue/ConversationPanel.vue`)
- **Dynamic Option Pills:** Renders `v-for="(option, optIdx) in msg.approval.options"` buttons with brutalist numbering and styling (`[1] Option text`).
- **`handleSelectOption(msg, option)`:** Immediately marks approval status as accepted and dispatches the preformed prompt `Selected option: "${option}". Please proceed with this execution.` directly to the agent via `prompt_agent` (NDJSON stream-json stdin).
- **Graceful Fallback:** Preserves standard `[ Accept Proposal ]` when no options are specified and retains `[ Request Changes ]` for free-form developer feedback.

### C. Standardized Multi-Turn Resolution Prompts (`templates/prompts/`)
- Updated `flag_resolve_todos.md`, `flag_resolve_techdebt.md`, `flag_resolve_profiler.md`, and `flag_resolve_docs.md`:
  - **Turn 1:** Reconnaissance & SQLite staging $\to$ Deliver inventory & options via `post_agent_response` with `approval: { title, options }`. Mandatory invariant: Zero `resolve_initialization_flag` in Turn 1.
  - **Turn 2+:** Receive selected option $\to$ Execute code/config modifications $\to$ Deliver confirmation report via `post_agent_response` $\to$ Conclude with `resolve_initialization_flag`.

### D. Specifications Synchronized (`docs/specs/18_mcp_dialogue_channel_and_chat_overlay.md`)
### E. Native MCP Calling Governance & Prompt Hardening
- **Universal Managed Block (`WANG_KAPAWU_MANAGED_BLOCK` in `manifest.rs` & `AGENTS.md`):** Added Section 6 mandating that all platform tools (`post_agent_response`, `manage_todo_item`, `record_technical_debt`, `resolve_initialization_flag`, `profile_service`) must be invoked exclusively via native MCP tool callers (`call_mcp_tool` / direct tool definitions). Strictly banned writing Python scripts, curl commands, or shell scripts to hit `http://localhost:8765/mcp`.
- **Concrete Tool Call Signatures:** Updated all flag resolution prompt templates (`flag_resolve_*.md`) with copy-paste ready JSON parameter signatures for `post_agent_response` with embedded `approval: { title, description, options }`.
- **Single Terminal Response Invariant:** Explicitly instructed models to call `post_agent_response` strictly once per turn upon completing all scanning and staging, forbidding dummy/probe calls.

---

## 3. Verification & Invariants
- `cargo check --manifest-path src-tauri/Cargo.toml`: **Passed (0 errors)**.
- `npm run build` (`vue-tsc --noEmit && vite build`): **Passed (0 errors)**.
- Tiger Style Invariants: Shallow nesting, explicit guard clauses, and no mock fallbacks.
