# 2026-11-05 — Test Isolation and Runner → Model Single Source

Follow-up of `2026-11-05_daemon_api_stage1.md`.

## Test isolation
Unit tests wrote into the user's real `~/.config/one-ship/logs/oneship.log` and read the real `config.toml` (the database was already isolated by audit F17).

Under `#[cfg(test)]`, `config::get_global_config_path`, `logger::get_logs_dir` and `db::get_db_path` now use one per-process directory, `config::test_process_dir()` → `$TMP/oneship-test-process-<pid>`. The config is written there with the defaults, so test results no longer depend on the machine's settings.

Verified: a full test run leaves the real log and `config.toml` untouched.

## Root cause of the 2 `kopeng::resolution` failures
The tests and `flag_resolver.rs` read the model with `orchestrator.agent_model`, which does not exist (the registered key is `default_model`). With an empty candidate, `resolution` fell back to the first model of the agy manifest (`gemini-4-argon`).

In production, every flag-resolution agent therefore ignored `default_model`.

## Principle (user decision)
A model always belongs to a runner, and the runner → models relation comes from one source in the whole system: the runner manifests. The initializer takes models from them and never chooses on its own; manual editing offers only the models of the selected runner. An incompatible pair can only come from a badly generated `rula.toml` or an obsolete model after a runner update, and it is reported, never substituted.

## Backend
- **Manifests:**
  - New required `quota_fallback_model` (it replaces the chat's hardcoded `LIGHT_MODEL_BY_RUNNER`).
  - `load_runner_schema` enforces the invariants (`manifest_problems`): models declared, unique IDs, `default_effort` ⇔ `effort_levels` and one of them, `flags.effort` when a model has efforts, `quota_fallback_model` declared, `id` matching the file name.
  - `RunnerSchema` / `RunnerModel` / `RunnerFlags` are exported with ts-rs.
- **`config.rs`:**
  - New errors: `UnknownModel`, `InvalidEffort`, `MissingModel`, `IncompletePair`.
  - `validate_agent_pair`.
  - Per-plugin `AgentPairSchema`, and the field types `runner` / `runner_model` / `runner_effort`, which replace the hardcoded enums that offered `gemini`/`cursor`/`pi` and a mixed model list.
  - `agent_pair_problems`: a table that sets any key of a pair sets its model (and runner) too, and the pair must be declared.
  - `agent_persona_problems`: every `[agents.<id>]` declares runner + model.
  - New pairs: `system_model` / `system_effort` (runner fixed to agy) and `quota_probe_effort`. `quota_probe_model` changes from `gemini-3.8-flash-low`, which is not an agy manifest model, to `gemini-3.8-flash` + effort `low`.
  - `ApiError` implemented for `ConfigError`.
- **`resolution.rs` (rewritten):**
  - `resolve_effective_agent_config` → `Result`, with sources explicit / `[agents.<id>]` (strict, except for `config-resolver`) / global pair;
  - `ExplicitAgentConfig`, `finalize_agent_config`, `INIT_AGENT_KEYS` / `DEFAULT_AGENT_KEYS`, `resolve_system_agent_config`, `runner_catalog_markdown`;
  - daemon command `get_configured_agent_pair`.
  - Every literal and "first model" is gone.
- **`supervisor.rs`:**
  - `spawn_agent` takes a required model and validates the pair before stopping anything. Its swallowed `let _ = stop_process` now propagates.
  - The `"agy"` runner fallback and the duplicated default-effort logic are removed.
  - `launch_agent` takes the pair whole from one source (explicit > Conversation > agent) and never pairs the agent's model with an overriding runner.
  - `compute_project_state` shows the resolved pair of git, the initializer and resolvers (`platform_agent_pair`) instead of `"agy"` / `"gemini-3.8-flash"` / `"high"`; personas read their validated pair.
  - The system agent launches with `system_model`.
- **Other callers:**
  - `flag_resolver`: `default_model` key fix; `RUNNER_CATALOG` injected into its prompts.
  - `initializer`: personas default to the agents' pair (no longer the initializer's own), and the prompt carries the runner catalog.
  - Greenfield: the pair is validated before anything is created, and the REA `reverser` is written with a model.
  - `conversation::apply_conversation_config` validates the pair in effect before storing.
  - `persona.rs`: the model is required and the pair validated.
  - `stats.rs`: the quota probe is a validated agy pair, with flags from the manifest.

## Frontend
- `utils/runnerCatalog.ts`: catalog cache, helpers and `useAgentPairPicker`.
- `SettingsModal.vue`:
  - selects driven by the manifests;
  - the name-based effort heuristics (`flash`/`pro`) are removed;
  - a runner change clears the model;
  - saving is blocked while a pair is invalid.
- `AddPersonaModal.vue`: model required and from the manifest; no "Runner Default" / "(Custom)"; no preselected pair.
- `ProjectInitModal.vue`, `AddProjectModal.vue`: the picker plus the configured init pair (no `'agy'` / `'gemini-3.8-flash'` fallbacks).
- `FloatingChatOverlay.vue`:
  - the hardcoded model lists are removed;
  - runner change → model cleared, applied only once declared;
  - pairing mirrors the backend;
  - the quota switch uses `quota_fallback_model`.
- `AgentTaskbar.vue`, `ConversationPanel.vue`: no invented `agy` runner or model in placeholders.
- Generated TS types: `AgentPersonaInput`, `RunnerSchema`, `RunnerModel`, `RunnerFlags`, `EffectiveAgentConfig`, `ConfiguredAgentPair`, `GetConfiguredAgentPairArgs`.

## Docs
- Spec 10:
  - §1.2 rewritten (it described the sanitization fallbacks as a feature);
  - §2.2 field types and `agent_pairs`;
  - config example;
  - persona examples with pairs.
- Specs 17, 19, 23: the quota probe pair.
- Spec 26: the modal and the greenfield invariants.
- `rula_schema.md`: pair rules.
- `flag_resolve_config.md`: pair errors and the catalog.

## Impact on registered projects
`estudiorc`'s `bosmang` declares `claude-sonnet-4-6` under `agy`. It becomes `[! CONFIG]`, and `config-resolver` asks which listed model to use. The other registered projects' personas declare valid pairs.

## Verification
- `cargo check --all-targets` clean.
- `clippy`: 15 warnings (was 17).
- `cargo test --lib`: 132 passed, 0 failed. The two old `resolution` failures now pass; new tests cover pair resolution and errors, document pair and persona validation, manifest invariants, schema defaults, persona pairs and the REA scaffold.
- `vue-tsc` clean.
- Not run in the app.
