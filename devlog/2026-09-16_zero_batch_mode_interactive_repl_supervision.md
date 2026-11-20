# Development Log: Zero Batch Mode & Mandatory Interactive REPL Supervision

**Date:** 2026-09-16  
**Topic:** Zero Batch Mode Mandate, Interactive REPL Supervisor Stdin Injections, and Dual MCP Discovery Injection  
**Build Version:** 0.2.1 (Pre-1.0 Baseline)

---

## 1. Problem Context & Root Cause Analysis

During project onboarding and supervised/unattended initialization in Wang-Kapawu:
1. The project initializer agent in `src-tauri/src/kopeng/skill.rs` was spawned using batch mode flags (`init_args = ["--print-timeout", "24h", "-p", prompt]`).
2. In batch mode (`-p`), Antigravity runs non-interactively for a single turn, outputs markdown directly to stdout, and terminates immediately. This prevented:
   - Maintaining an interactive multi-turn REPL session for follow-ups and approvals.
   - Using the Wang-Kapawu MCP dialogue channel (`post_agent_response`) properly in conversation overlays.
   - Emitting live initialization flags (`raise_initialization_flag`).
3. The user established a strict architectural mandate:
   - **Zero Batch Mode:** Spawning agents with `-p` or batch prompt flags is strictly prohibited across the entire platform.
   - **Mandatory Interactive REPL Supervision:** All agent processes must be spawned in interactive REPL mode with open stdin/stdout pipes, and initial instructions/prompts must be piped via `send_input`.

---

## 2. Implemented Architecture & Code Changes

### 2.1 Codebase Audit of Agent Spawning
- Audited all agent spawning across the codebase.
- Verified that all domain agents, system agents, git specialists, and restarted sessions in `src-tauri/src/supervisor.rs` (`launch_agent`, `launch_system_agent_cmd`, `restart_agent_with_session`, `dispatch_git_instruction`) already spawn interactively with empty `extra_args: &[]` and dispatch inputs via `send_input`.
- Confirmed that `initialize_project_agent` in `src-tauri/src/kopeng/skill.rs` was the only place passing batch arguments.

### 2.2 Migrated `initialize_project_agent` to Interactive REPL Mode
- In `src-tauri/src/kopeng/skill.rs`:
  - Removed batch argument construction (`init_args`).
  - Updated `spawn_agent` to pass empty `extra_args: &[]`.
  - Added an asynchronous prompt injector:
    ```rust
    let key_input = key.clone();
    let prompt_input = prompt.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
        let sup_input = crate::supervisor::get_supervisor();
        let _ = sup_input.send_input(&key_input, &prompt_input).await;
        crate::conversation::mark_user_message_sent("initializer");
    });
    ```

### 2.3 Dual MCP Configuration Injection
- In `inject_project_mcp_plugin`:
  - Ensured `mcp_config.json` is written to both `<project>/.agents/plugins/wang-kapawu/mcp_config.json` and `<project>/.agents/mcp_config.json` for deterministic agent discovery.

### 2.4 Governance Rules & Specs Update
- In `AGENTS.md`:
  - Added Rule 9 under `1. Fundamental Governance Rules`: **Zero Batch Mode & Mandatory Interactive REPL Supervision**.
- In `docs/specs/08_native_process_supervisor.md`:
  - Updated Section 2.2 with the Zero Batch Mode Invariant.

---

## 3. Verification & Compliance Matrix

| Check | Target | Result |
|---|---|---|
| Rust Backend Check | `cargo check --bins` | Passed (0 errors, 0 warnings) |
| Rust Unit Tests | `cargo test --lib` | Passed (23/23 tests green) |
| Frontend TypeScript & Bundle | `npm run build` | Passed (0 errors) |
| Process Supervisor Invariant | All agents REPL-based | Verified |
