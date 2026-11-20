# Development Session: 2026-10-09 — Claude Code Runner Headless Multi-Turn Integration

## Context & Objectives
- **Target Problem (TECHNICAL_DEBT #24):** Claude Code CLI integration had been left in a partial state with a temporary refusal guard returning `SupervisorError::UnsupportedRunner`. Claude Code (`claude` v2.1.292) needed to be promoted to a first-class autonomous agent runner alongside Google Antigravity (`agy`) and OpenCode (`opencode`).
- **Research Findings & Protocol Invariants:**
  - Claude Code CLI enforces that `--print` with `--output-format=stream-json` strictly requires `--verbose`. Without `--verbose`, the process terminates immediately with an error.
  - Interactive pipes without `--print` terminate with code 1.
  - Resuming sessions requires `--resume <session_id>` (not `--conversation`).
  - Quotas are emitted during turns as `type: "rate_limit_event"` containing `unifiedWindows.five_hour` and `unifiedWindows.seven_day` with `utilization` (0.0–1.0) and `resetsAt` (Unix epoch seconds).
  - MCP tools are discovered natively via `<project_path>/.mcp.json`.

---

## Key Changes & Implementations

### 1. Runner Definition & Schema Fallback (`templates/runners/claude.toml` & `src-tauri/src/config.rs`)
- Created `templates/runners/claude.toml` defining the Generation 5.5 model family (`claude-opus-5-5`, `claude-sonnet-5-5`, `claude-haiku-5-5`, `claude-fable-5-1`, etc.) with default effort settings.
- Updated `src-tauri/src/config.rs` to register the embedded fallback for `load_runner_schema("claude")` and added comprehensive unit test coverage.

### 2. Project MCP Provisioning & Documentation Symlinks (`src-tauri/src/kopeng/skill.rs`)
- Added `.mcp.json` generation in `inject_project_mcp_plugin` pointing to the ONE-SHIP MCP HTTP server (`http://localhost:8765/mcp`).
- Created relative symlink `CLAUDE.md -> AGENTS.md` ensuring Claude Code loads project governance without duplicating rule files.

### 3. Multi-Account OAuth Discovery & Switching (`src-tauri/src/kopeng/accounts.rs`)
- Implemented `discover_claude_credentials`:
  - Scans `~/.claude/.credentials.json`.
  - Extracts active account ID and OAuth token.
  - Persists encrypted credentials in SQLite under `provider = 'anthropic'`.
- Implemented `save_claude_credentials` and `switch_claude_account` for seamless multi-account credential swaps.

### 4. Native Process Supervisor Integration (`src-tauri/src/supervisor.rs`)
- **Removed Refusal Guard:** Purged the temporary early rejection from `spawn_agent`.
- **Command Construction:**
  - Assembles CLI arguments: `["--print", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--include-partial-messages", "--dangerously-skip-permissions"]`.
  - Injects dynamic `--model` and `--effort` when configured.
  - Appends `--resume <session_id>` for resumed agent conversations.
- **Stdin Pipe Forwarding:**
  - Wraps user prompt text into Claude's stream-json payload:
    ```json
    {"type":"user","session_id":"...","message":{"role":"user","content":[{"type":"text","text":"..."}]}}
    ```
- **Asynchronous Stream Reader:**
  - `type: "system"` (`subtype: "init"`): Extracts session ID and registers conversation state.
  - `type: "stream_event"`: Emits real-time assistant text and thinking deltas via `agent-step-update`.
  - `type: "assistant"`: Captures tool usage calls (including `post_agent_response`).
  - `type: "rate_limit_event"`: Ingests 5-hour and 7-day rolling window utilization metrics directly into SQLite `account_quotas`.
  - `type: "result"`: Transitions agent to `ProcessStatus::Idle`, executes compliance verification fallback, and calls `dispatch_next_pending_input` to drain queued tasks.

### 5. Frontend Fallback & Technical Debt Closure
- Updated `src/components/dialogue/FloatingChatOverlay.vue` to include Claude 5.5 models in the offline catalog fallback.
- Marked item #24 in `TECHNICAL_DEBT.md` as resolved.
- Updated `docs/specs/08_native_process_supervisor.md` (trimodal supervision) and `docs/specs/17_multi_account_management.md` (Claude OAuth and stream quota observability).

---

## Verification & Compilation
- `cargo check`: Passed with 0 errors across all backend crates.
- `vue-tsc --noEmit`: Passed with 0 errors across frontend TypeScript.
