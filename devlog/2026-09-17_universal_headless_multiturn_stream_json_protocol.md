# Development Log: Universal Headless Multi-Turn Agent Protocol (`stream-json`)

**Date:** 2026-09-17  
**Topic:** Universal Headless Multi-Turn Protocol (`stream-json`), Transparent Stdin NDJSON Framing, and Stream Output Telemetry  
**Build Version:** 0.2.1 (Pre-1.0 Baseline)

---

## 1. Problem Context & Root Cause Analysis

When migrating Wang-Kapawu agents to interactive REPL execution:
1. Spawning `agy` with standard piped stdin without `--input-format stream-json --output-format stream-json` caused `agy` to enter interactive full-screen TUI readline mode.
2. In headless background subprocesses without a pseudo-terminal (TTY), `agy` waited for ANSI terminal escape keycodes (e.g. project trust dialogs, readline keyboard strokes) instead of processing incoming plain text over piped stdin, leading to process hangs or immediate exit without processing prompts or calling MCP tools (`post_agent_response`).
3. To resolve this universally across all agent personas, the **Universal Headless Multi-Turn Agent Protocol (`stream-json`)** was established as the platform-wide standard.

---

## 2. Implemented Architecture & Code Changes

### 2.1 Native Process Supervisor (`src-tauri/src/supervisor.rs`)
1. **Universal Stream-JSON CLI Flags:**
   In `spawn_agent`, all agents using `runner_bin == "agy"` are automatically spawned with:
   ```rust
   cli_args.push("--input-format".to_string());
   cli_args.push("stream-json".to_string());
   cli_args.push("--output-format".to_string());
   cli_args.push("stream-json".to_string());
   ```
2. **Transparent Stdin NDJSON Framing:**
   In the Tokio stdin forwarding task, any raw prompt or instruction sent via `send_input` or `prompt_agent` is automatically encapsulated into the NDJSON event structure:
   ```rust
   let payload = if is_agy && !raw_input.trim().starts_with('{') {
       serde_json::json!({
           "event": "user",
           "message": {
               "role": "user",
               "content": raw_input
           }
       }).to_string()
   } else {
       raw_input
   };
   ```
3. **Stream Output Telemetry & Formatting:**
   In the Tokio stdout reader task, incoming stream-json events (`init`, `step_update`, `result`) are decoded in real-time. Clean, formatted status entries (`[INIT]`, text deltas, `[STEP]`, `[RESULT]`) are pushed to the circular ring buffer, ensuring terminal logs provide clear observability without noisy raw JSON strings.

### 2.2 Global Specifications & Governance Rules
1. **`AGENTS.md`:** Updated Rule 9 under **Fundamental Governance Rules** to mandate the **Zero Batch Mode & Universal Headless Multi-Turn Agent Protocol (`stream-json`)** across all system agents, project initializers, and specialist personas.
2. **`docs/specs/08_native_process_supervisor.md`:** Updated Section 2.2 with detailed specifications for `stream-json` execution, stdin framing, and stdout decoding.

---

## 3. Verification & Compliance Matrix

| Check | Target | Result |
|---|---|---|
| Rust Backend Check | `cargo check --bins` | Passed (0 errors, 0 warnings) |
| Rust Unit Tests | `cargo test --lib` | Passed (23/23 tests green) |
| Frontend TypeScript & Bundle | `npm run build` | Passed (0 errors) |
| Headless Multi-Turn Invariant | All `agy` processes use `stream-json` | Verified |
