# Development Log: 2026-09-23 - Strict Project Context & MCP Scoping Guard

## 1. Problem Statement & Root Cause
External developer assistants running in interactive CLI / terminal sessions (e.g. Antigravity in Herdr) outside of Wang-Kapawu were attempting to invoke Wang-Kapawu's MCP dialogue tools (`post_agent_response`, `request_approval`) due to global MCP registration in `~/.gemini/config/mcp_config.json`.
When invoked by an external agent:
1. Permissive fallbacks in `find_project_for_agent`:
   - Fallback 2 searched all registered projects for any agent name match (`bosmang`).
   - Fallback 4 searched the SQLite `agent_conversations` table for any active conversation with that `agent_id`, which incorrectly attributed the turn to an unrelated project (`estudiorc`, creating spurious turn 71).
2. The `wang-kapawu` repository itself had residual `rula.toml`, `.agents/plugins`, and `.agents/personas` files, misleading agents into treating the orchestrator's own source repository as a managed project.

## 2. Solutions Implemented

### A. Repository Cleanup & Governance
- Deleted `/home/butcherwutcher/Projects/dev/wang-kapawu/rula.toml`, `.agents/plugins/`, and `.agents/personas/`.
- Updated `AGENTS.md` explicitly stating that `wang-kapawu` is developed strictly via external terminal / Herdr sessions, and agents working on it must NOT invoke `post_agent_response` or any Wang-Kapawu MCP dialogue tools.
- Purged spurious turn 71 from `agent_chat_turns` in SQLite.

### B. TCP Socket Peer & Caller Working Directory Attribution
- Configured `axum::serve` in `daemon.rs` and `start_in_process_mcp_daemon` in `mcp.rs` to extract `ConnectInfo<SocketAddr>`.
- Implemented `resolve_caller_cwd_from_peer_port(peer_port)` parsing `/proc/net/tcp` (matching peer hex port to inode) and inspecting `/proc/<pid>/fd` to resolve the calling process's PID and its working directory (`/proc/<pid>/cwd`).
- Added an early scoping guard in `tools/call`: If `caller_cwd` is resolved and does not belong to any registered project, immediately reject the call with instructions to render to terminal stdout.

### C. Elimination of Permissive Fallbacks in `find_project_for_agent`
- Refactored `find_project_for_agent(agent_id, project_hint, caller_cwd)`:
  1. **Caller CWD Match:** Matches against registered project directories.
  2. **Active Supervisor Process:** Matches currently running managed supervisor processes for `agent_id`.
  3. **Explicit Project Hint:** Matches SQLite ID, project path, project name, or `rula.toml` `[project].id`/`uid`.
  4. **Active Initializer:** Matches running `project_initializations` table row if `agent_id` is `"initializer"`.
- Completely removed Fallback 2 (scanning arbitrary project rula files) and Fallback 4 (querying `agent_conversations` for last active conversation).
- If no match is found, returns `Err(String)` describing the rejection.

### D. Strict Project Resolution in MCP Handlers
- Updated `execute_post_agent_response` and `execute_request_approval` in `mcp.rs` to accept `caller_cwd`, extract `project_hint` (`project_uid` / `project_id` / `project`), and fail fast if project resolution fails.
- Rejection responses return `isError: true` with zero database writes and zero daemon events.
- In `execute_request_approval`, added pending approval persistence into SQLite with resolved `project_path`.
- Updated `persist_fallback_agent_response` in `supervisor.rs` to pass `_project_path` into `find_project_for_agent`.

### E. Dynamic `project_uid` in Project Initializer
- In `src-tauri/src/kopeng/initializer.rs`, dynamically read existing or generate a new `project_uid` (`proj-<uuid_v4>`) and inject `("PROJECT_UID", project_uid.as_str())` into template variables.
- Updated `templates/prompts/project_initializer.md`, `templates/prompts/greenfield_initializer.md`, and `templates/skills/kapawu-core/references/rula_schema.md` instructing initializers to declare `[project] id = "{{PROJECT_UID}}"` and pass `project_uid` in `post_agent_response`.

### F. Bidirectional Living Specifications Synchronization (`docs/specs/`)
- Synchronized `docs/specs/12_streamable_http_mcp_daemon.md`: Updated Section 2.1 (global `~/.gemini/config/mcp_config.json`) and Section 6 (TCP socket origin inspection `/proc/net/tcp`, early scoping guard, strict resolution cascade, zero permissive fallbacks).
- Synchronized `docs/specs/18_mcp_dialogue_channel_and_chat_overlay.md`: Updated Section 3.1 & 3.2 tool contracts (`post_agent_response`, `request_approval`) with `project_uid` argument and strict scoping rejection behavior.
- Synchronized `docs/specs/10_configuration_schema_and_cascade.md`: Updated Section 4 `rula.toml` schema to document `[project].id`.
- Synchronized `docs/specs/26_greenfield_project_creation_and_conversational_bootstrapping.md`: Updated Section 4.2 with Rule 6 (`PROJECT_UID` generation, `rula.toml` declaration, and `post_agent_response` routing invariant).

## 3. Verification
- Backend Rust compilation verified via `cargo check --manifest-path src-tauri/Cargo.toml` (0 errors, 0 warnings).
- Frontend TypeScript / Vite compilation verified via `npm run build` (`vue-tsc --noEmit && vite build`, 0 errors).
