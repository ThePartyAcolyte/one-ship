# Work Session Log: Agent Restart Deterministic Readiness Wait & Prompt Injection

**Date:** 2026-09-05  
**Topic:** Herder Protocol 20 `agent.wait` integration, elimination of startup prompt race conditions, and 3-phase concurrent mass agent reset.  
**Maintainer:** Bosmang (Lead Systems Architect & Engineer)  

---

## 1. Context & Problem Statement

When restarting an agent or resetting all project agents via `[RESET ALL]` in `HerderSection.vue`, the agent runner process (`agy`) was terminated and re-spawned in its existing tab and pane. However, `bootstrap_agent_in_pane` attempted to inject the agent's initial prompt (`system_prompt` + `[CONTEXT REFERENCE]`) after a brittle, hardcoded `500ms` sleep.

Because CLI agents (`agy`, `claude`, `cursor`) take 1.5s to 3.0s to initialize OAuth credentials, runtime dependencies, and render their interactive prompt interface, Herder reported the pane as `interactive_ready: false` during the initial 500ms. Consequently, `herdr_socket::prompt_agent_sync` was dropped or rejected by the PTY, leaving restarted agents running in an empty, unprompted prompt loop without their persona definitions or context document references.

---

## 2. Architectural Solution

### 2.1 Native Protocol 20 `agent.wait` Integration (`herdr_socket.rs`)
1. Extended `HerdrAgent` model with `interactive_ready: Option<bool>`.
2. Implemented `wait_agent_sync(target, until, timeout_ms)` and `wait_agent_async(target, until, timeout_ms)`:
   - Dispatches `agent.wait` over `~/.config/herdr/herdr.sock`.
   - Awaits until the agent transitions into `agent_status: "idle"`.
   - Deserializes and returns the confirmed `HerdrAgent` instance.

### 2.2 Deterministic Prompt Injection (`bootstrap_agent_in_pane`)
1. Extracted `build_agent_initial_prompt(agent_info)` helper:
   - Compiles `system_prompt` and `[CONTEXT REFERENCE]` document references from `rula.toml`.
2. Replaced the 500ms sleep in `bootstrap_agent_in_pane` with `herdr_socket::wait_agent_sync(pane_id, vec!["idle"], 8000)`.
3. Included a 150ms buffer settling delay once `idle` is confirmed before calling `herdr_socket::prompt_agent_sync`.
4. Added comprehensive logging to track prompt dispatch success or error.

### 2.3 Three-Phase Concurrent Mass Agent Reset (`reset_all_project_agents`)
Refactored `reset_all_project_agents` from a slow, synchronous sequential restart into a 3-phase pipeline:
1. **Phase 1 (Terminate & Clear):** Rapidly terminates active foreground processes across all project panes and clears terminal viewports.
2. **Phase 2 (Concurrent Launch):** Changes working directory and spawns all configured runner processes simultaneously.
3. **Phase 3 (Readiness Await & Prompt Injection):** Awaits each pane's transition into `idle` and immediately injects its persona prompt and context reference documents.

---

## 3. Verification & Results

- **Rust Core (`cargo check`):** 0 errors, 0 warnings.
- **Unit Testing (`cargo test --lib -- kopeng::herdr_socket`):** `test_live_herdr_snapshot` passed in 0.01s.
- **Vue Frontend (`npm run build`):** Compiled in 2.00s with 0 TypeScript errors.
- **Specs Synchronization:** Updated `docs/specs/08_herder_agent_orchestration.md` (§8.5) and `docs/specs/15_herder_unix_socket_driver_migration.md` (§3).
