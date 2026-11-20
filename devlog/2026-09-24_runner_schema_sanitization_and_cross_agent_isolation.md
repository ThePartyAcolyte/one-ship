# Devlog: Runner Schema Sanitization, Model Validation, and Cross-Agent Isolation

**Date:** 2026-09-24  
**Topic:** Atomic resolution of runner, model, and reasoning effort, OpenCode ACP permission alignment, and cross-agent isolation.

---

## 1. Problem Statement & Root Cause Analysis

When configuring an agent in a project (`estudiorc`) to use the OpenCode CLI runner with DeepSeek (`runner = "opencode"`, `model = "deepseek/deepseek-v4-pro"`), launching other agents configured with Google Antigravity (`runner = "agy"`) or triggering flag resolution (`docs-resolver`) failed immediately with:
```text
error: invalid model selection (--model "deepseek/deepseek-v4-pro" --effort "high"): --effort is not supported for model "deepseek/deepseek-v4-pro"
```

### Forensic Root Causes Identified:
1. **Key Mismatch in Agent Scoping:**
   `resolve_effective_agent_config` in `resolution.rs` attempted to resolve agent keys using `model_config_key = "agent_model"` and `runner_config_key = "agent_runner"`. In `rula.toml` and database records, the fields under `[agents.<agent_id>]` are `model`, `runner`, and `effort`.
2. **Blind Fallback Cross-Pollution (`t.values().find_map(...)`):**
   Failing to match `"agent_model"` on the target agent, `resolution.rs` fell back to `rula.get("agents").and_then(|a| a.as_table()).and_then(|t| t.values().find_map(|v| v.get("model")))`. Because TOML tables in Rust iterate keys in alphabetical order, `backend` (configured with DeepSeek) was evaluated first. Consequently, ANY agent without an explicit command-line override inherited `deepseek/deepseek-v4-pro` while remaining on runner `agy`.
3. **Incompatible CLI Flag Combination in AGY:**
   AGY rejects `--effort high` when paired with non-native reasoning models, crashing the spawned child process immediately.
4. **UI Flag Desynchronization:**
   `AddPersonaModal.vue` displayed the *Auto-Accept File Edits* toggle for all runners, including OpenCode (which operates via ACP `session/request_permission` rather than `--mode accept-edits`).

---

## 2. Implemented Architecture & Solutions

### A. Atomic Configuration Resolution (`src-tauri/src/kopeng/resolution.rs`)
- **Direct Agent Scope Lookup:** Queries `rula.agents.<agent_id>` specifically for `runner`, `model`, and `effort`.
- **Elimination of Global Map Leaks:** Removed `t.values().find_map(...)` entirely.
- **Cross-Runner Schema Sanitization:** Validates `model` against `load_runner_schema(&effective_runner)`. If a model does not exist in the selected runner's catalog, it gracefully sanitizes to the runner's default model (e.g. `gemini-3.8-flash` for `agy`, `deepseek/deepseek-flash` for `opencode`).
- **Effort Level Validation:** Inspects `RunnerModel.effort_levels`. If a model does not support reasoning effort, `effort` resolves to `None`, preventing invalid `--effort` flags from being passed to CLI runners.
- **Unit Test Coverage:** Added unit tests verifying default fallback, OpenCode model resolution, and cross-runner model sanitization.

### B. OpenCode ACP Permission Compliance (`src-tauri/src/supervisor.rs`)
- Enforced `is_auto_approve` check in OpenCode ACP `session/request_permission` RPC handler. Responds `"accepted"` when `auto_approve` is true, and `"rejected"` when false.

### C. Dynamic UI Capabilities (`src/components/modals/AddPersonaModal.vue` & `AgentList.vue`)
- **Adaptive Model Selection:** Switching runners automatically sanitizes the selected model to the new runner's supported catalog.
- **Conditional Capability Display:** The *Auto-Accept File Edits* toggle is now wrapped in `v-if="runnerSchema?.flags?.accept_edits"`, hiding it for OpenCode.
- **Function-Driven Labeling:** Cleaned up UI toggle descriptions to focus strictly on functionality rather than low-level CLI flags.
- **Synchronized Flag Resolution:** `AgentList.vue` now passes the agent's declared `runner`, `model`, and `effort` when launching resolution agents.

---

## 3. Verification & Invariants

- `cargo test kopeng::resolution`: All 3 tests passed.
- `cargo check`: Rust backend compiled cleanly.
- `npm run build`: `vue-tsc --noEmit && vite build` passed with zero errors.

---

## 4. Synchronized Living Specifications

- **`docs/specs/08_native_process_supervisor.md`:** Updated Section 2.2 and Section 6.1 with ACP permission protocols (`session/request_permission` -> accepted/rejected based on `auto_approve`) and dual-runner governance rules.
- **`docs/specs/10_configuration_schema_and_cascade.md`:** Added Section 1.2 formalizing the multi-runner, model, and effort resolution contract (direct persona lookup, cross-runner schema sanitization, and effort capability validation), and updated sample `rula.toml` declarations.
