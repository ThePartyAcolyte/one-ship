# Devlog: Global Project Initialization Configuration & Runner/Model Selection
**Date:** 2026-09-05  
**Topic:** Outside-In Foundations, Process Supervision & Multi-Agent Delegation (Phase 0 / Initializer)  
**Author:** Bosmang (Lead Systems Architect & Supervisor)

---

## 1. Overview & Context
Previously, the project bootstrapping agent in `initialize_project_agent` was hardcoded to spawn `agy` directly without reading user runner preferences, and failed to pass any `--model` CLI argument. Furthermore, because a repository does not possess a `rula.toml` file prior to bootstrap, initialization parameters must be governed globally (`scope: "global_only"`).

In headless execution (`semi_attended` and `unattended`), the process runs as an unattached background Tokio task without a terminal TTY. If interactive permission flags (`--dangerously-skip-permissions` for `agy`) are omitted, the process hangs indefinitely waiting on unfillable stdin prompts.

## 2. Implementation Details

### A. Foundation & Global Configuration Schema (`src-tauri/src/config.rs`)
- Registered `init_runner` and `init_model` under the `orchestrator` plugin schema with `scope: "global_only"`.
  - `init_runner`: `select_enum` (`"agy"`, `"claude"`, `"gemini"`, `"cursor"`, `"pi"`, `"opencode"`), default `"agy"`.
  - `init_model`: `select_enum` (`"gemini-3.8-flash"`, `"gemini-3.7-flash"`, `"gemini-3.1-pro"`, etc.), default `"gemini-3.8-flash"`.
- Updated default template configuration in `load_global_config` to seed initial values in `~/.config/wang-kapawu/config.toml`.

### B. Execution Engine Refactor (`src-tauri/src/kopeng/skill.rs`)
- Updated `initialize_project_agent` signature to accept optional `runner`, `model`, and `auto_approve` overrides:
  ```rust
  pub async fn initialize_project_agent(
      project_path: String,
      project_name: String,
      mode: String,
      is_retry: bool,
      runner: Option<String>,
      model: Option<String>,
      auto_approve: Option<bool>,
  ) -> Result<String, String>
  ```
- **Dynamic Resolution:**
  - Effective runner falls back to global `orchestrator.init_runner` (`"agy"` default).
  - Effective model falls back to global `orchestrator.init_model` (`"gemini-3.8-flash"` default).
  - Queries `load_runner_schema` to resolve binary and CLI flags (`--model`, `--dangerously-skip-permissions`).
- **Permission Invariant:**
  - In `semi_attended` and `unattended` modes, auto-approve flag is **strictly enforced** and appended to the Tokio command to prevent background execution deadlocks.
  - In `supervised` mode, spawns a dedicated `"Init"` tab in Herder and honors the optional `auto_approve` setting.
- **Prompt Injection:**
  - Passes `effective_model` as the CLI argument `--model <effective_model>` and instructs the agent prompt to configure `effective_model` as the baseline default for generated personas in `rula.toml`.

### C. Multi-Agent Delegation via MCP Bus & Frontend Layer
- Delegated the UI/UX layer to the `frontend` specialist agent via Wang-Kapawu's MCP bus tool `delegate_agent_task`.
- **Scope Leakage Fix (`SettingsModal.vue`):** Updated field filter condition to `scope === 'global' ? f.scope !== 'project_only' : f.scope !== 'global_only'`, preventing `global_only` settings from leaking into project settings.
- **Project Initialization Modal (`ProjectInitModal.vue`):**
  - Integrated `AppSelect` for runner and dynamic model options.
  - Integrated `AppToggle` for `supervised` auto-approve and displayed a locked brutalist badge (`ENFORCED: AUTO-APPROVE`) for headless modes.
  - Passes dynamic `runner`, `model`, and `autoApprove` payload to `initialize_project_agent`.
- **Frontend Quality Audit & Polish (`frontend` Specialist):**
  - Enhanced `SettingsModal.vue` tab navigation by introducing `visiblePluginSchemas`, filtering out plugins with no fields in the active scope (eliminating blank tabs in Project Settings).
  - Eliminated native `title="Close"` attributes in `SettingsModal.vue` and `ProjectInitModal.vue`, replacing them with standardized `<AppTooltip>` per Spec 13 / Rule 5.
  - Fortified viewport robustness in `ProjectInitModal.vue` for narrow tiled screens (<=640px) using `shrink-0`, `min-w-0`, and `whitespace-nowrap`.

### D. Specifications Synchronization
- Updated `docs/specs/10_configuration_schema_and_cascade.md` to document `global_only` initializer keys.
- Updated `docs/specs/08_herder_agent_orchestration.md` (Section 8.6) to define the initialization execution contract and headless permission invariant.

---

## 3. Verification & Operational Checklist
- [x] Technical proposal presented and approved by user before execution.
- [x] Foundations built outside-in: schema registered before backend execution, followed by frontend UI.
- [x] Backend verified with `cargo check`: 0 errors, 0 warnings.
- [x] Frontend verified with `npm run build`: 0 TypeScript / template errors.
- [x] Multi-agent task delegation executed via Wang-Kapawu MCP tool `delegate_agent_task` without subagent invocation.
- [x] Documentation synchronized in `docs/specs/`.
