# Devlog: 2026-09-10 — Project Initialization Flow, Reversible AGENTS.md & Bounded Persona Architecture

---

## 1. Work Accomplished in this Session

### 1.1 Reversible `AGENTS.md` Lifecycle & Backup Engine (`src-tauri/src/kopeng/manifest.rs`)
- **Deterministic Boundary Delimiters:**
  - Registered `MANAGED_BLOCK_START` (`<!-- WANG_KAPAWU_MANAGED_START -->`) and `MANAGED_BLOCK_END` (`<!-- WANG_KAPAWU_MANAGED_END -->`).
  - Added `WANG_KAPAWU_MANAGED_BLOCK` containing universal agent governance: terminal stdout silence, mandatory `post_agent_response` routing, TTS-friendly spoken dialogue rules, technical artifact separation, authorization via `request_approval`, and token-economy lazy documentation loading.
- **Preparation Function (`prepare_project_agents_md`):**
  - Runs automatically before agent launch during project initialization.
  - If `AGENTS.md` already exists: creates `.wang-kapawu/backups/AGENTS.md.orig` and safely injects or updates the delimited block at the top while preserving 100% of pre-existing user rules below.
  - If `AGENTS.md` does not exist: marks `.wang-kapawu/backups/AGENTS.md.created` and creates a fresh `AGENTS.md` with the managed block and baseline guideline sections.
- **Clean Uninstall Reversal (`purge_kapawu_artifacts`):**
  - If `AGENTS.md.created` marker exists: deletes `AGENTS.md` outright.
  - If `AGENTS.md.orig` exists: restores the pre-existing file.
  - If neither exists: cleanly strips the delimited managed block.

### 1.2 Bounded Persona Prompt Architecture & Initializer Engine (`src-tauri/src/kopeng/skill.rs`)
- **Pre-Initialization Hook:**
  - `initialize_project_agent` calls `prepare_project_agents_md` prior to launching runner processes.
- **Prompt Token Economy & Strict Persona Rules:**
  - Instructs the Initializer Agent to propose 2 to 4 domain-focused specialists in `rula.toml` based on stack reconnaissance (`lead`, `frontend`, `core`/`backend`, `git`).
  - Imposes a strict hard limit of ≤ 100 words (~120 tokens) per persona `system_prompt` with a 3-part anatomy:
    1. Role & directory ownership.
    2. Negative boundaries & delegation target via `delegate_agent_task`.
    3. Primary tech stack & verification command.
  - Prohibits duplicating `AGENTS.md` platform governance rules in persona prompts.
  - Bounded initial prompts across `semi_attended`, `unattended`, and `supervised` modes.
- **Skill Version Bump:**
  - Incremented `CURRENT_SKILL_VERSION` from `0.2.1` to `0.2.2` (following pre-1.0 sequential build-number convention).

### 1.3 Dual-Path Artifact Resolution (`src-tauri/src/mcp.rs`)
- **Enhanced `execute_post_agent_response`:**
  - Added dual-path artifact resolution: if `artifact` is an inline string, it is emitted directly; if `artifact` is a file path (or Antigravity brain artifact path in `~/.gemini/antigravity-cli/brain/<cid>/`), the daemon automatically reads the file content, detects language and type, and embeds it in the `AgentResponse` payload.

### 1.4 Platform Skills & Live Documentation Synchronization
- **Platform Skill Updates (`templates/skills/kapawu-platform/`):**
  - Bumped version to `0.2.2` in `SKILL.md`.
  - Added `post_agent_response` and `request_approval` to `SKILL.md` and `references/mcp_primitives.md`.
  - Documented `mcp_dialogue_mode` and the Bounded Persona Prompt specification in `references/rula_schema.md`.
  - Synchronized `.agents/skills/kapawu-platform/` in the repository.
- **Specs Updated:**
  - Updated `docs/specs/08_herder_agent_orchestration.md` (added Section 8.7 for `AGENTS.md` lifecycle and Section 8.8 for Bounded Persona Prompts).
  - Updated `docs/specs/18_mcp_dialogue_channel_and_chat_overlay.md` (Section 3.1 dual-path artifact resolution).

### 1.5 Reasoning Effort Selector & Backend Resolution (`skill.rs` & `ProjectInitModal.vue`)
- **Root Cause Resolution:** Addressed `agy` CLI rejection (`invalid model selection: requires --effort`) when running Gemini models without explicit effort levels.
- **Frontend Wizard Integration (`ProjectInitModal.vue`):**
  - Added reactive `selectedEffort` and computed `effortOptions` derived from the selected runner schema model.
  - Automatically synchronizes to `model.default_effort` on model selection.
  - Expanded layout to a 3-column grid (`Runner CLI`, `Model`, `Reasoning Effort`), rendering `AppSelect` when configurable and a brutalist `Fixed / Default` badge with `AppTooltip` when fixed.
  - Included `effort` in the `initialize_project_agent` payload.
- **Backend Resolution (`skill.rs`):**
  - Updated `initialize_project_agent` signature to accept `effort: Option<String>`.
  - Automatically resolves `effective_effort` from `schema.models` (`default_effort` or first available level) if omitted.
  - Injects `--effort <level>` into CLI args across `supervised`, `semi_attended`, and `unattended` modes.

### 1.6 Target Project Invariant & Real-Time Approval SSE Event Bus
- **Target Project Invariant (`src-tauri/src/kopeng/skill.rs`):**
  - Injected explicit `TARGET PROJECT SPECIFICATION (MANDATORY INVARIANT)` into the Initializer Agent prompt, locking reconnaissance and writes strictly to `project_path`.
  - Explicitly forbade invoking `query_workspace_state` to determine project paths, preventing target workspace leakage into active Herder terminal sessions.
  - Specified structured approval handling protocol (`Approved` vs changes requested).
- **MCP Event Bus & SSE Stream (`src-tauri/src/mcp.rs`, `daemon.rs`, `lib.rs`, `Cargo.toml`):**
  - Added `tokio-stream = { version = "0.1", features = ["sync"] }` to `src-tauri/Cargo.toml`.
  - Updated `create_mcp_router(tx)` in `mcp.rs` to wire `.layer(Extension(tx))` and expose `GET /api/events` streaming `DaemonEvent` via Server-Sent Events (SSE).
  - Implemented `start_in_process_mcp_daemon(tx)` in `mcp.rs` and spawned it inside Tauri's `setup()` in `lib.rs` (yielding gracefully if port 8765 is already bound by the standalone daemon).
  - Updated standalone `daemon.rs` binary to create a broadcast channel and pass it to `create_mcp_router(tx)`.
- **Frontend Real-Time SSE Listener (`src/stores/coreStore.ts`):**
  - Added `initDaemonSse()` automatically connecting to `http://127.0.0.1:8765/api/events` on mount.
  - Handles `ApprovalRequested` and `AgentResponse` events, dispatching them to `handleDaemonEvent()` with automatic reconnection if the daemon restarts.
- **Collateral Restoration:**
  - Restored `wang-kapawu/rula.toml` and cleaned up `wang-kapawu/AGENTS.md` following the earlier rogue initialization run.

---

## 2. Technical Health & Verification
- Backend: `cargo check` and `cargo check --bin kapawu-daemon` completed with 0 errors and 0 warnings.
- Frontend: `npm run build` (`vue-tsc --noEmit && vite build`) completed with 0 errors.
- End-to-End Approval Verification: Tested `request_user_approval` and `POST /api/approval-response` roundtrip via `curl` with live SSE verification on `http://127.0.0.1:8765/api/events`.

