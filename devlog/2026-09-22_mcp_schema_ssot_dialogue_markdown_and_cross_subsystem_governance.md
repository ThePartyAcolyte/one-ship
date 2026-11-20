# Development Log: 2026-09-22 - MCP Schema SSOT, Rich Dialogue Markdown, and Cross-Subsystem Governance

## 1. Context & Problem Statement
During the bootstrap of the new greenfield project `estudiorc`, the initialization agent engaged in ~45 unexpected file-searching and codebase-reading steps inspecting Wang-Kapawu's own backend and frontend source code instead of executing its intended first turn.

Investigation into transcript `fd699937-4a83-43c9-8085-598da72dffc5` revealed:
1. **Schema Mismatch & Contradiction:** The prompt instructed `- artifact_type: "proposal"`, whereas `post_agent_response` tool schema declared a rigid enum `["code", "diff", "markdown", "json", "plan"]`. Lacking `instructions.md` (which returned a 404), the agent read `~/.gemini/antigravity-cli/config.json` via a permissive shell command, discovered the path to `wang-kapawu`, and began inspecting `mcp.rs` and `ArtifactPanel.vue` to check if `proposal` and `initializer` were valid parameters.
2. **Chat vs. Artifact Inversion:** Because `dialogue` had been rigidly described as *"spoken language only, no headers, no markdown, TTS-ready"*, the agent moved the entire scoping roadmap, workflow agenda, and open questions into an ephemeral artifact, leaving the chat dialogue as an empty 2-sentence stub.
3. **Decoupling Failure:** Hardcoding micro-parameters and JSON structures in system prompts and skills caused brittle drift whenever the underlying MCP server schema evolved.

## 2. Architecture & Changes Implemented

### 2.1. Backend MCP Server & Protocol (`src-tauri/src/mcp.rs`, `src-tauri/src/kopeng/skill.rs`)
- **Initialize Instructions (Protocol SSOT):** Injected universal agent operational guidelines directly into the `initialize` method payload of the MCP server, providing runtime guidance on dialogue vs artifact separation, sandbox isolation, and tool invocation.
- **Auto-Provisioning `instructions.md`:** `inject_project_mcp_plugin` now programmatically provisions `~/.gemini/antigravity-cli/mcp/wang-kapawu/instructions.md` with guidelines, eliminating 404 file read errors by agent runners.
- **Schema Extensibility & Documentation:**
  - Expanded `artifact_type` enum to include `"proposal"`, defaulting to `"markdown"`.
  - Updated `agent_id` parameter description to explicitly declare `'initializer'` as a valid system role for bootstrap sessions.
  - Updated `dialogue` description to establish it as the primary human engagement channel supporting rich Markdown and ASCII flowcharts.
  - Updated `artifact` description to strictly forbid placing conversational questions or scoping agendas in artifacts.

### 2.2. Frontend Rich Markdown in Chat Overlay (`ConversationPanel.vue`, `FloatingChatOverlay.vue`)
- Imported `marked` with `{ breaks: true, gfm: true }`.
- Created `renderDialogueMarkdown()` helper and bound messages with `v-html="renderDialogueMarkdown(msg.dialogue)"` and class `chat-markdown-body`.
- Added brutalist scoped CSS for `.chat-markdown-body` ensuring typography, lists (`ul`, `ol`), bold text, inline code, and `<pre>` code blocks for ASCII box diagrams render cleanly inside the chat bubble without expanding line heights or padding excessively.

### 2.3. Agent Prompts & Greenfield Initializer Alignment (`templates/prompts/`, `src-tauri/src/kopeng/initializer.rs`)
- **Strict Sandbox & Isolation Invariant:** Injected strict sandbox rules into `greenfield_initializer.md` and `project_initializer.md` forbidding agents from inspecting, grepping, or listing any files outside `{{PROJECT_PATH}}` (including `~/.config`, `~/.gemini`, or host repositories).
- **Greenfield First-Turn Directive (Chat-Only):**
  - Enforced zero shell or inspection tools on Turn 1 (`post_agent_response` must be the first and only action).
  - Welcome, scoping steps, ASCII roadmap, and primary open questions are placed directly into `dialogue` in the Chat.
  - Forbade emitting an artifact on Greenfield Turn 1, ensuring the chat remains the active center of collaboration.

### 2.4. Governance & Specification Updates (`AGENTS.md`, `docs/specs/`)
- **Rule 10 in `AGENTS.md` (Full-Stack Cross-Subsystem Atomic Interconnection Policy):** Mandates that any change to MCP tool schemas, protocols, or response formats must be atomically propagated across backend, frontend, prompts, skills, and specifications in the same work cycle.
- **Spec 18 & Spec 26 Synchronization:** Updated `docs/specs/18_mcp_dialogue_channel_and_chat_overlay.md` and `docs/specs/26_greenfield_project_creation_and_conversational_bootstrapping.md` with the updated tool contracts and invariant rules.
- **Skill Alignment (`kapawu-artifacts` & `kapawu-initializer`):** Synchronized `templates/skills/` with the conversational dialogue hygiene rules.
- **Scope Sanitization (`.agents/skills/` Removal):** Removed redundant `.agents/skills/` folder from the Wang-Kapawu repository root, ensuring platform skills live exclusively in `templates/skills/` (the single source of truth for code compilation) and avoiding prompt pollution in host developer sessions.

## 3. Verification & Compliance
- **Backend:** `cargo check --manifest-path src-tauri/Cargo.toml` compiled cleanly with 0 errors.
- **Frontend:** `npm run build` (`vue-tsc --noEmit && vite build`) built cleanly with 0 type or lint errors.
- **Scope Hygiene:** `.agents/skills/` removed from repo root. Only `templates/skills/` maintained.
- **Specs & Governance:** All living specifications synchronized bidirectionally. Zero Git commands executed.
