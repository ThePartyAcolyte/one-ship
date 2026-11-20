# Configuration Schema and Cascade

## Purpose

Replace hardcoded flags with a hierarchical, schema-driven configuration engine. Settings cascade from factory defaults to granular agent overrides, and plugin-declared schemas drive dynamic settings UI without hardcoding frontend controls.

## Scope

- The four-level configuration cascade and its resolution rule, locations and validation.
- Runner / model / effort pair resolution (single source: runner manifests).
- The plugin schema contract (`ConfigFieldSchema`) and the 17 registered plugin schemas (`config.rs`).
- The global `config.toml` and project `rula.toml` formats, including agent personas.
- The project configuration hub UI (`ProjectConfigPanel.vue`) and its scope relative to global settings.

Skill injection and skill versioning are in [Platform Skill Injection](03-platform-skill-injection.md).

## Model / Design

### The 4-level configuration cascade

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                       CONFIGURATION CASCADE HIERARCHY                       │
│                                                                             │
│  [ Level 4: Agent Persona Override ]                                        │
│  rula.toml -> [agents.<name>] (e.g. runner, dangerously_skip_permissions)   │
│         ▲                                                                   │
│         │ (If omitted, falls back to)                                       │
│  [ Level 3: Project Plugin Override ]                                       │
│  rula.toml -> [plugins.<plugin_id>] (e.g. plugins.vcs.skip_permissions)    │
│         ▲                                                                   │
│         │ (If omitted, falls back to)                                       │
│  [ Level 2: User Global Config ]                                            │
│  ~/.config/one-ship/config.toml                                             │
│         ▲                                                                   │
│         │ (If omitted, falls back to)                                       │
│  [ Level 1: In-Memory Plugin Schema Default ]                               │
│  Plugin-declared static default value                                       │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Resolution rule

A setting query:

```rust
resolve_effective_setting(project_path, agent_name, plugin_id, key)
```

evaluates from Level 4 downwards. The first non-null definition wins.

`plugin_id` must be the id under which the key is registered in `get_all_plugin_schemas()` (for example logger and file-tree keys live under `system`). Every key read must be registered; reading an unregistered key logs an ERROR and yields `null`.

**Locations (no aliases):** Level 4 is `rula.toml [agents.<name>].<key>`. Levels 3 and 2 read exactly one table per plugin: `[system]` for the `system` plugin and `[plugins.<plugin_id>]` for every other plugin. Root `[<plugin_id>]` tables, `[system]` for non-system plugins and alternate key names (`*_pct`, `dangerously_skip_permissions`, `profiler_sample_interval_ms`) are no longer read.

#### Validation

- A configuration document is valid when it parses as TOML and every registered key it sets under `[system]` / `[plugins.<id>]` has its schema type (`boolean`, `number`, `string`, `select_enum` value in `options`, `string_list`). Unregistered keys are not checked.
- **`config.toml`:** validated at daemon startup (`validate_global_config`); an unreadable or invalid file stops the daemon, and the UI then shows it unreachable. The app only reads `mcp.mcp_daemon_port` from it. A missing file is created from the defaults (a write failure stops startup). `save_global_config` validates before writing. If the file is edited into an invalid state at runtime, the last valid version stays in effect and the error is logged once per file version. The parsed file is cached by mtime, so hot paths (logger, watcher ticks) pay one `stat`.
- **`rula.toml`:** validated (and cached by path + mtime) on every read. An invalid file does not block the project: its settings are ignored and the project runs on Levels 2–1, while the project watcher raises a pending `invalid_config` initialization flag with the error (on first sight of the project and on every `rula.toml` change). The `[! CONFIG]` badge launches the `config-resolver` agent (`templates/prompts/flag_resolve_config.md`) in the project. The flag closes automatically when the file validates again (agents cannot resolve it with `resolve_initialization_flag`). `save_project_rula_config` requires a registered project and a valid current `rula.toml` (it never replaces an unparsable file with the settings alone) and validates the result before writing.
- `sync_antigravity_global_mcp_config` fails on a corrupt `~/.gemini/config/mcp_config.json` instead of replacing it.

### Runner, model and effort pairs (`kopeng/resolution.rs`)

**Single source:** a model always belongs to a runner, and the runner manifests (`templates/runners/<id>.toml`) are the only source of the runner → models → efforts relation in the whole system. No module, prompt or component declares, picks or substitutes models: there is no "first model of the list", no hardcoded model literal and no default runner in code. A pair the manifest does not declare is an error, never sanitized.

1. **Manifests (`config::RunnerSchema`):** `id` (must match the file name), `name`, `binary`, `version`, `quota_fallback_model` (lightest model, the chat's quota switch target), `flags` (`model`, `effort`, `auto_approve`, `accept_edits`) and `models` (`id`, `label`, `effort_levels`, `default_effort`, `context_window_tokens_count`). `load_runner_schema` enforces the invariants (`manifest_problems`): at least one model, unique IDs, `default_effort` set exactly when `effort_levels` is not empty and one of them, `flags.effort` declared when any model has efforts, `quota_fallback_model` declared. `list_agent_runners()` returns every manifest sorted by ID. Each `[[models]]` entry may declare `context_window_tokens_count`, the source of the context percentage in the quota/metrics specification.
2. **Validation (`config::validate_agent_pair(runner, model, effort)`):** an unknown runner (`UnknownRunner`), a model the runner does not declare (`UnknownModel`) or an effort the model does not declare (`InvalidEffort`; any effort for a model without efforts) is an error. A missing effort returns the model's `default_effort`.
3. **Resolution (`resolve_effective_agent_config`):** the pair comes whole from the first source that has one — the explicit choice (`ExplicitAgentConfig`; runner without model or the reverse is `IncompletePair`), the agent's `[agents.<id>]` table (read strictly; only `config-resolver` reads an invalid file leniently), or a global pair of `[plugins.orchestrator]` (project overrides global). Pairs: `INIT_AGENT_KEYS` (`init_runner` / `init_model` / `init_effort`) for the initializer and `DEFAULT_AGENT_KEYS` (`agent_runner` / `default_model` / `default_effort`) for platform agents without a persona (flag resolution) and as the default persona pair. Errors are `ConfigError`.
4. **Launch (`supervisor::launch_agent` / `spawn_agent`):** the pair comes from the explicit launch arguments, else the Conversation's overrides, else the agent; the agent's own model never pairs with an overriding runner. `spawn_agent` takes a required model, validates the pair before stopping or spawning anything (`finalize_agent_config`) and passes the manifest's effort flag only for a model with efforts. `apply_conversation_config` validates the pair in effect before storing an override.
5. **Writers of `[agents.<id>]`:** the persona modal and `create/update_agent_persona` (runner and model required), the initializer and the `config-resolver` (both receive a runner catalog generated from the manifests, `runner_catalog_markdown`, and are told to take pairs only from it), and the greenfield REA scaffold (the chosen pair, else the default pair).

**Runner manifest loading (`config::load_runner_schema`):** manifests are read only from `TEMPLATES_ROOT/runners/<runner>.toml`; an unknown runner, unreadable file or invalid manifest is an error (no CWD-relative search, no embedded copies, no runner-id aliases). `TEMPLATES_ROOT` resolution is specified in [Platform Skill Injection](03-platform-skill-injection.md).

### Plugin schema contract and category taxonomy

Every kopeng plugin declares its configurable settings explicitly. This metadata enables dynamic, schema-generated UI forms with real-time parameter search and filtering without hardcoding frontend controls.

#### Category taxonomy (17 registered plugin schemas in `config.rs`)

1. **`system` (General & host platform):** host terminal emulator (`ghostty`, `foot`, `gnome-terminal`, `konsole`, `xfce4-terminal`, `x-terminal-emulator`, `xterm`, etc.), default projects root path, daemon log verbosity (`log_level`), log size/rotation/pruning bounds, file-tree depth/node limits, D-Bus call timeout, and OS keyring timeout. (Also `webkit_renderer_mode`, see [Daemon Operator API and Lifecycle](05-daemon-operator-api-and-lifecycle.md).)
2. **`appearance` (Visual theming & typography):** interface body font family (`IBM Plex Mono`, `Anonymous Pro`, `Courier Prime`), global font scale/zoom (`90%` to `130%`), account quota gauge style (`concentric`, `bars`, `classic`).
3. **`orchestrator` (AI agents & personas):** initializer runner/model/effort pair, default agent runner/model/effort pair, default living documentation style (`default_docstyle`: `sdd`, `lean`, `contract`, `custom`), tool auto-approval, spawn poll step, log tail lines, init timeout, and port release timeouts.
4. **`accounts` (Accounts, quotas & token observability):** 1-token quota kickstart warmup pair (`quota_probe_model`, `quota_probe_effort`), `auto_warmup_enabled`, stats refresh interval, context warning threshold %, quota alert/danger/depleted thresholds %, and probe cooldown/timeout.
5. **`supervisor` (Native process supervisor):** in-memory ring buffer capacity, max queued pending inputs (`max_pending_inputs_count`), process teardown grace period, state watcher poll rate, Brief turn/byte limits, conversation search limit, caller identity ancestor walk depth (`identity_ancestor_walk_max_depth_count`), and watcher debounce.
6. **`telemetry` (System telemetry & database pool):** host telemetry poll interval (`telemetry_interval_ms`) and SQLite connection pool maximum size (`db_connection_pool_max_count`).
7. **`profiler` (Continuous profiling):** sampling interval of the run process trees (`sample_interval_ms`), the stop grace period of a run wrapped by a profiler engine (`engine_shutdown_grace_period_ms`), and the process breakdown limit of a run profile (`process_breakdown_max_items_count`).
8. **`vcs` (Git & snapshots):** default branch name (`default_branch`, global only), Git runner/model/effort pair, `default_agent_id`, Forge CLI limits & timeout, `snapshots_enabled`, `minutes_checkpoint_mode` (`snapshot` | `commit` | `off`), `auto_commit_detail_level` (`title_only` | `title_and_decisions` | `full`), `auto_snapshot_on_file_change`, `max_per_project_count`, `auto_snapshot_on_init` (global only), and `scratchpad_retention_days`.
9. **`mcp` (MCP daemon & event stream):** local MCP/RPC TCP port (`mcp_daemon_port`, default `8765`), `task_wait_timeout_ms`, `approval_ttl_ms`, and SSE `event_channel_capacity_count`.
10. **`rea` (Reverse engineering engine):** enable toggle, max concurrent heavy jobs, idle timeout, Ghidra installation directory, max JVM heap size, and targets directory.
11. **`electronics` (Electronics workstation):** enable toggle, firmware directory, schematics directory, default serial baud rate, and default flasher tool.
12. **`desigma` (One-Ship Designer):** enable toggle, `mcp_binary_path`, `app_binary_path`, `design_dir`, `enable_bridge`, `mcp_request_timeout_ms`, `auto_snapshot_on_edit`, and `max_snapshots_per_asset_count`.
13. **`solidcraft` (3D CAD workstation):** enable toggle, `app_binary_path`, `cli_binary_path`, `cad_dir`, `snapshot_timeout_ms`, `snapshot_width_px`, `snapshot_height_px`, `auto_snapshot_on_edit`, and `max_snapshots_per_asset_count`.
14. **`pcbcraft` (EDA/PCB workstation):** enable toggle, `app_binary_path`, `cli_binary_path`, `pcb_dir`, `mcp_request_timeout_ms`, `auto_snapshot_on_edit`, and `max_snapshots_per_asset_count`.
15. **`voice_controller` (Global Voice Controller / Gemini Live):** global-only configuration for `enabled`, `model` (`gemini-3.8-live` | `gemini-3.8-live-extended-thinking`), `voice_name`, `thinking_level`, `persona_style`, `idle_disconnect_seconds`, `turn_ledger_max_entries_count`, `scratchpad_max_items_count`, `context_compression_trigger_tokens`, `context_compression_target_tokens`, and `voice_tool_response_max_bytes`. The Gemini API key is stored in the OS keyring, not in this table.
16. **`dictation` (Native Gemini Live dictation):** `enabled`, `mode` (`smart` | `verbatim`), `auto_send`, `speech_energy_threshold`, `rollover_min_session_seconds`, `rollover_silence_ms`, `rollover_finalize_timeout_seconds`, `watchdog_silence_seconds`, `finalize_timeout_seconds`, `reconnect_attempts_count`, `recovery_min_interval_seconds`, and `stale_audio_retention_hours`. No duration limit is configured.
17. **`project_management` (Kanban, milestones & Forge collaboration):** `enabled`, `tracking_mode` (`linear` | `shape_up` | `weighted`), `github_cache_ttl_seconds`.
18. **`semantic` (Vector RAG & knowledge graph):** `enabled`, `ignored_patterns`, `included_extensions`, `extra_paths`.

#### Schema definition (`ConfigFieldSchema`)

- **`key`**: unique identifier (e.g. `auto_approve`, `sample_interval_ms`).
- **`label`**: human-readable title for UI rendering.
- **`field_type`**:
  - `boolean`: toggle switch (`AppToggle`).
  - `string`: single-line text input (`AppInput`).
  - `select_enum`: dropdown with defined `options` (`AppSelect`).
  - `project_agent`: dropdown (`AppSelect`) populated dynamically from the project's declared `[agents.<id>]` personas (validated as a string value; e.g. `plugins.vcs.default_agent_id`).
  - `number`: numeric input (`AppInput`).
  - `string_list`: comma-separated array input (`AppInput`).
  - `runner`, `runner_model`, `runner_effort`: one field of a runner/model/effort pair; the Settings selects list only what the manifests declare (runners; models of the pair's runner; efforts of the selected model, plus "model default"). A runner change clears a model it does not declare, a model change clears an effort it does not declare, and saving is blocked while a pair is not declared.
- **`default_value`**: baseline factory fallback.
- **`options`**: allowed values for `select_enum`.
- **`agent_pairs`** (per plugin, `AgentPairSchema`): the triples among the fields (`runner_key` or `fixed_runner`, `model_key`, `effort_key`). A document that sets any key of a pair sets its model (and its runner, when configurable) in the same table, and the triple must be declared by the runner's manifest (`agent_pair_problems`). Every `[agents.<id>]` table must declare `runner` and `model` (and may declare `effort`) as such a pair (`agent_persona_problems`). Either problem makes the document invalid: a `config.toml` stops startup and a Settings save, a `rula.toml` raises `[! CONFIG]`.
- **`scope`**:
  - `global_only`: applies system-wide and exclusively in `~/.config/one-ship/config.toml`.
  - `project_only`: project-specific (e.g. base repo URL).
  - `cascade`: eligible for cascading overrides at global, project and agent levels.
- **`description`**: explanatory note.

### Project configuration hub (`ProjectConfigPanel.vue`)

Project-scoped configuration (`rula.toml`) is hosted by the fixed `settings` workspace tab; it is no longer embedded in `ProjectOverviewTab.vue`.

1. **Full-bleed geometry:** the settings tab wrapper has no padding or scroll container; `ProjectConfigPanel.vue` fills the full width/height of the stage with no outer border or card background. The left category sidebar is full height (`w-56`, `ui-surface-list`) and keeps its background below the last category when the list is short; the right content column scrolls independently in the remaining space.
2. **Single top toolbar:** one `h-9` `.ui-search-box` row replaces the former title block, file badge, search input, reload button and bottom save footer. It contains the parameter search (categories and fields), the match counter, `Discard` and `Save` (both `h-6`, text labels, no tooltips). The panel title and the `rula.toml` badge are not rendered; the gear tab tooltip identifies the view.
3. **No redundant category header:** the right pane does not repeat the active category name or its enabled/disabled status (the sidebar already shows both). Category quick actions (`Open Lab`, `Open Board`, `Index Knowledge`, `Sync`, etc.) remain in a title-less action row.
4. **Vertical tabbed categories:** the left sidebar dynamically lists all plugins declaring project or cascade scope fields (`orchestrator`, `vcs`, `rea`, `supervisor`, `telemetry`, `profiler`, `mcp`). Category buttons are `h-7` / `text-[10px]`; the list has no separate "Categories" heading.
5. **Real-time parameter search:** live filtering matches plugin names, field labels, parameter keys and descriptions in the top toolbar.
6. **Provenance observability:** every field displays an explicit badge denoting whether its effective value originates from `[rula.toml]`, `[global]` or schema `[default]`. Field cards use `ui-surface-card`, an 11px label, 10px description and 9px default hint.
7. **Automatic disk reload:** the daemon project watcher emits `projects-updated` when `rula.toml` changes. The panel subscribes, debounces, re-reads `get_scoped_project_config`, and compares the values. With no local edits it applies the disk version automatically; with local edits it shows a warning banner (`rula.toml changed on disk. Discard loads the disk version; Save overwrites it with the in-memory version.`) and never overwrites them silently. There is no manual reload button.
8. **Unsaved-changes navigation guard:** a JSON snapshot of the loaded values defines the dirty state. While dirty, the panel registers a UI navigation guard consumed by `coreStore.setActiveProject` and `workspaceLayoutStore.setWorkspaceTab` (and by `navigateTo` / daemon snapshot project switches before they mutate state). Leaving the settings tab or switching project opens a blocking confirmation modal with `Save & Continue`, `Discard & Continue` and `Cancel`. Saving or discarding resumes the pending navigation; a failed save/discard keeps the user in the panel with the error banner.
9. **Unified feature toggles & subsystem visibility:** the panel integrates subsystem activation (`[plugins.<id>] enabled`) and workspace tab visibility (`toggle_project_tab_visibility` via `projects.hidden_tabs`) directly into the left category list (with contextual quick actions in the right pane and disabled parameter forms when a feature is off).
10. **Project star / favorite & archiving hierarchy:** projects support a favorite toggle (`projects.is_favorite` integer in SQLite, toggled via `toggle_project_favorite`) and an archive toggle (`projects.is_archived` integer in SQLite, toggled via `toggle_project_archived`). Favorited projects are ordered first, and archived projects are hidden from the vertical project rail and drawer unless the archived filter is enabled. The archived filter and "Track New Project" live only in the open drawer; the collapsed rail exposes only the drawer toggle.
11. **Scope decoupling:** `SettingsModal.vue` is reserved strictly for global system settings (`~/.config/one-ship/config.toml`), triggered from the main desktop header (`AppHeader.vue`).

## Contracts

### Global configuration file (`~/.config/one-ship/config.toml`)

```toml
# ONE-SHIP Global Configuration

[system]
preferred_terminal = "ghostty"
log_level = "INFO"                     # Minimum level for oneship.log and system_logs
log_max_size_bytes = 512000
log_max_rotations_count = 2
log_db_max_rows_count = 50000          # Newest system_logs rows kept in SQLite
log_db_prune_interval_seconds = 300
fs_tree_max_depth_count = 12           # File tree scan (`get_project_file_tree`): deeper directories are marked truncated
fs_tree_max_nodes_count = 5000         # File tree scan: entries beyond this are cut and the tree is marked truncated
dbus_call_timeout_seconds = 2          # gdbus --timeout of the diagnostics D-Bus checks

[plugins.appearance]
body_font = "IBM Plex Mono"
font_scale = "100% (Normal)"
quota_gauge_style = "concentric"

[plugins.orchestrator]
init_runner = "agy"                    # Initializer pair (global only), declared by the agy manifest
init_model = "gemini-3.8-flash"
init_effort = "high"
agent_runner = "agy"                   # Default agent pair (global only): flag resolution, default persona pair
default_model = "gemini-3.8-flash"
default_effort = "high"
default_docstyle = "sdd"               # Default living documentation archetype (global only)
auto_approve = false

[plugins.accounts]
quota_probe_model = "gemini-3.8-flash" # Warmup probe pair (runner fixed to agy)
quota_probe_effort = "low"
auto_warmup_enabled = true
context_warning_threshold_percent = 30.0
quota_alert_threshold_percent = 15.0
quota_danger_threshold_percent = 5.0

[plugins.supervisor]
max_buffer_lines_count = 500
max_pending_inputs_count = 16          # Prompts queued while an agent is busy
shutdown_grace_period_ms = 3000
state_watcher_interval_ms = 2500

[plugins.telemetry]
telemetry_interval_ms = 2000

[plugins.mcp]
mcp_daemon_port = 8765                 # Single source for every daemon URL (read by the app at startup)
event_channel_capacity_count = 1024    # Events buffered per /api/events subscriber before a resync

[plugins.profiler]
sample_interval_ms = 500                   # Run process trees sampled into DuckDB (never pruned)
engine_shutdown_grace_period_ms = 15000    # SIGINT -> SIGKILL wait of a run wrapped by an engine
process_breakdown_max_items_count = 20     # Process names listed in a run profile

[plugins.vcs]
git_runner = "agy"                      # Cascade: runner for AI commit-message generation
git_model = "gemini-3.8-flash"          # Cascade: model paired with git_runner
git_effort = "high"                     # Cascade: reasoning effort for git_model
default_branch = "master"              # Global only
github_repo_list_limit_count = 1000     # Repositories listed by `gh repo list` (global only)
snapshots_enabled = true
minutes_checkpoint_mode = "snapshot"    # "snapshot" | "commit" | "off"
auto_commit_detail_level = "title_and_decisions" # "title_only" | "title_and_decisions" | "full"
auto_snapshot_on_file_change = true
max_per_project_count = 50
auto_snapshot_on_init = true           # Global only
scratchpad_retention_days = 0          # 0 = disabled (never auto-delete scratchpad files)
# Project-only in rula.toml: default_agent_id = "<declared_agent_id>" (used by OPEN AGENT & quick actions)
```

### Project configuration file (`<project_root>/rula.toml`)

```toml
[project]
id = "proj-123e4567-e89b-12d3-a456-426614174000" # Unique project UUID for multi-project agent routing
name = "my-project"
oneship_version = "0.2.7"

[plugins.orchestrator]
auto_approve = true # Overrides global default for this project

[plugins.vcs]
max_per_project_count = 100
minutes_checkpoint_mode = "commit"

[[plugins.vcs.actions]]
name = "Auto-Commit"
prompt = "Analyze git diff and create a clean semantic commit"

# Project-Specific Agent Personas: every table declares a runner/model pair of the runner's manifest
# Reserved platform agent IDs (system, git, initializer, removal-resolver, config-resolver) are forbidden here.
[agents.bosmang]
runner = "agy"
model = "gemini-4-argon"
effort = "high"
auto_approve = true
context_docs = ["docs/00_manifesto.md", "docs/01_ubiquitous_language.md"]

[agents.backend]
runner = "opencode"
model = "deepseek/deepseek-v4-pro"
auto_approve = true
context_docs = ["docs/02_architecture.md"]

[agents.lang_belta]
runner = "claude"
model = "claude-sonnet-5-5"
auto_approve = true
context_docs = ["docs/03_lang_belta_dictionary.md"]
```

### Persona commands (`kopeng/persona.rs`)

The persona modal edits `[agents.<id>]` through three daemon commands (arguments `{ projectId, agent }` / `{ projectId, agentId }`); errors are `PersonaError` as `ApiErrorResponse`:

- `create_agent_persona(project_id, agent)`: an existing ID is an error.
- `update_agent_persona(project_id, agent)`: an undeclared ID is an error. Only the modal fields — `role`, `runner`, `model`, `effort`, `accept_edits`, `auto_approve`, `args`, `context_docs`, `skills`, `system_prompt`, `dna` — are written; every other key such as `mcp_dialogue_mode` is kept, and a blank field removes its key.
- `delete_agent_persona(project_id, agent_id)`: stops the agent first; a failed stop keeps the persona.

All three require a valid current `rula.toml` and validate the result before writing (`write_project_rula`). The ID must be lowercase letters, digits, `-` or `_`; the role non-empty; and `runner` / `model` (required) / `effort` (optional) a pair its manifest declares. The modal lists only the runners of `list_agent_runners()` and only the models and efforts the selected runner declares, with no preselected pair for a new persona. The platform agent IDs `system`, `git`, `initializer`, `removal-resolver` and `config-resolver` cannot be declared in `[agents.<id>]` (rejected by `agent_persona_problems`) or created as personas; the `git` agent resolves its runner/model/auto_approve through the plugin cascade (`[plugins.orchestrator]` / `[plugins.vcs]`).

### Other contracts

- **Configuration commands (daemon RPC):** `get_plugin_schemas`, `get_global_config`, `get_effective_project_config`, `get_scoped_project_config`, `save_global_config`, `save_project_rula_config`, `list_agent_runners`, `get_configured_agent_pair`, `toggle_project_favorite`, `toggle_project_archived`, `toggle_project_tab_visibility`.
- **Errors:** `ConfigError` (`UnknownRunner`, `UnknownModel`, `InvalidEffort`, `IncompletePair`, `ExplicitAgentConfig`), `PersonaError`.
- **Event:** `projects-updated` (emitted on `rula.toml` change).
- **Initialization flag:** `invalid_config` (see [Project Versioning and Migration](06-project-versioning-and-migration.md)).

## Implementation Status

| Area | Status | Notes |
|---|---|---|
| Cascade resolution, schema registry (`config.rs`), `validate_global_config` | Implemented | Verified: `validate_global_config`, `validate_agent_pair` in `config.rs`; `resolve_effective_agent_config` in `kopeng/resolution.rs`. |
| Runner manifests and pair validation | Implemented | `templates/runners/<id>.toml`. |
| Persona commands | Implemented | `kopeng/persona.rs`. |
| `ProjectConfigPanel.vue` hub | Implemented | Present at `src/components/workspace/ProjectConfigPanel.vue`; individual UI details (sizes, banners) not re-verified. |
| `system.webkit_renderer_mode` key | Implemented | Present in `config.rs` with default `"auto"`. |

> Note: the example `oneship_version` values in the two TOML samples differ (`0.2.6` in the plugin example, `0.2.7` here) as in the sources; the current platform version in code is `0.2.7`.

## Related Specs

- [Plugin Architecture](01-plugin-architecture.md)
- [Platform Skill Injection](03-platform-skill-injection.md)
- [Streamable HTTP MCP Daemon](04-streamable-http-mcp-daemon.md)
- [Daemon Operator API and Lifecycle](05-daemon-operator-api-and-lifecycle.md)
- [Project Versioning and Migration](06-project-versioning-and-migration.md)

Supersedes legacy: `10_configuration_schema_and_cascade.md`
