> [!WARNING]
> **LEGACY SPECIFICATION:** Superseded by [01-architecture/02-configuration-schema-and-cascade.md](../../specs/01-architecture/02-configuration-schema-and-cascade.md) and [01-architecture/03-platform-skill-injection.md](../../specs/01-architecture/03-platform-skill-injection.md). Preserved for historical provenance only.

# Spec: Configuration Schemas, Cascade Engine & Platform Skill Injection
**Document Version:** 1.1.0  
**Domain:** System & Project Configuration, Plugin Schemas, Cascade Resolution & Agnostic Agent Skill  
**Maintainer:** ONE-SHIP / Omarchy Development Team

---

## 1. The 4-Level Configuration Cascade

ONE-SHIP replaces hardcoded flags with a hierarchical, schema-driven configuration engine. Settings cascade from factory defaults to granular agent overrides:

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

### 1.1 Resolution Rule
A setting query:
```rust
resolve_effective_setting(project_path, agent_name, plugin_id, key)
```
evaluates from Level 4 downwards. The first non-null definition wins.

`plugin_id` must be the id under which the key is registered in `get_all_plugin_schemas()` (e.g. logger and file-tree keys live under `system`). Every key read must be registered; reading an unregistered key logs an ERROR and yields `null`.

**Locations (no aliases):** Level 4 is `rula.toml [agents.<name>].<key>`. Levels 3 and 2 read exactly one table per plugin: `[system]` for the `system` plugin and `[plugins.<plugin_id>]` for every other plugin. Root `[<plugin_id>]` tables, `[system]` for non-system plugins and alternate key names (`*_pct`, `dangerously_skip_permissions`, `profiler_sample_interval_ms`) are no longer read.

### 1.1.1 Validation
- A configuration document is valid when it parses as TOML and every registered key it sets under `[system]` / `[plugins.<id>]` has its schema type (`boolean`, `number`, `string`, `select_enum` value in `options`, `string_list`). Unregistered keys are not checked.
- `config.toml`: validated at daemon startup (`validate_global_config`); an unreadable or invalid file stops the daemon, and the UI then shows it unreachable. The app only reads `mcp.mcp_daemon_port` from it. A missing file is created from the defaults (a write failure stops startup). `save_global_config` validates before writing. If the file is edited into an invalid state at runtime, the last valid version stays in effect and the error is logged once per file version. The parsed file is cached by mtime, so hot paths (logger, watcher ticks) pay one `stat`.
- `rula.toml`: validated (and cached by path + mtime) on every read. An invalid file does not block the project: its settings are ignored and the project runs on Levels 2–1, while the project watcher raises a pending `invalid_config` initialization flag with the error (on first sight of the project and on every `rula.toml` change). The `[! CONFIG]` badge launches the `config-resolver` agent (`templates/prompts/flag_resolve_config.md`) in the project. The flag closes automatically when the file validates again (agents cannot resolve it with `resolve_initialization_flag`). `save_project_rula_config` requires a registered project and a valid current `rula.toml` (it never replaces an unparsable file with the settings alone) and validates the result before writing.
- `sync_antigravity_global_mcp_config` fails on a corrupt `~/.gemini/config/mcp_config.json` instead of replacing it.

### 1.2 Runner, Model & Effort Pairs (`kopeng/resolution.rs`)

**Single source:** a model always belongs to a runner, and the runner manifests (`templates/runners/<id>.toml`) are the only source of the runner → models → efforts relation in the whole system. No module, prompt or component declares, picks or substitutes models: there is no "first model of the list", no hardcoded model literal and no default runner in code. A pair the manifest does not declare is an error, never sanitized.

1. **Manifests (`config::RunnerSchema`):** `id` (must match the file name), `name`, `binary`, `version`, `quota_fallback_model` (lightest model, the chat's quota switch target), `flags` (`model`, `effort`, `auto_approve`, `accept_edits`) and `models` (`id`, `label`, `effort_levels`, `default_effort`, `context_window_tokens_count`). `load_runner_schema` enforces the invariants (`manifest_problems`): at least one model, unique IDs, `default_effort` set exactly when `effort_levels` is not empty and one of them, `flags.effort` declared when any model has efforts, `quota_fallback_model` declared. `list_agent_runners()` returns every manifest sorted by ID.
2. **Validation (`config::validate_agent_pair(runner, model, effort)`):** an unknown runner (`UnknownRunner`), a model the runner does not declare (`UnknownModel`) or an effort the model does not declare (`InvalidEffort`; any effort for a model without efforts) is an error. A missing effort returns the model's `default_effort`.
3. **Resolution (`resolve_effective_agent_config`):** the pair comes whole from the first source that has one — the explicit choice (`ExplicitAgentConfig`; runner without model or the reverse is `IncompletePair`), the agent's `[agents.<id>]` table (read strictly; only `config-resolver` reads an invalid file leniently), or a global pair of `[plugins.orchestrator]` (project overrides global). Pairs: `INIT_AGENT_KEYS` (`init_runner` / `init_model` / `init_effort`) for the initializer and `DEFAULT_AGENT_KEYS` (`agent_runner` / `default_model` / `default_effort`) for platform agents without a persona (flag resolution) and as the default persona pair. Errors are `ConfigError`.
4. **Launch (`supervisor::launch_agent` / `spawn_agent`):** the pair comes from the explicit launch arguments, else the Conversation's overrides, else the agent; the agent's own model never pairs with an overriding runner. `spawn_agent` takes a required model, validates the pair before stopping or spawning anything (`finalize_agent_config`) and passes the manifest's effort flag only for a model with efforts. `apply_conversation_config` validates the pair in effect before storing an override.
5. **Writers of `[agents.<id>]`:** the persona modal and `create/update_agent_persona` (runner and model required), the initializer and the `config-resolver` (both receive a runner catalog generated from the manifests, `runner_catalog_markdown`, and are told to take pairs only from it), and the greenfield REA scaffold (the chosen pair, else the default pair).

---

## 2. Plugin Schema Contract & Category Taxonomy (`ConfigFieldSchema`)

Every Kopeng plugin declares its configurable settings explicitly. This metadata enables dynamic, schema-generated UI forms with real-time parameter search and filtering without hardcoding frontend controls.

### 2.1 Category Taxonomy (15 Functional Subsystems in `config.rs`)
1. **`system` (General & Host Platform):** Host terminal emulator (`ghostty`, `foot`, etc.), default projects root path, daemon log verbosity (`log_level`), log size/rotation/pruning bounds, file-tree depth/node limits, and D-Bus call timeout.
2. **`appearance` (Visual Theming & Typography):** Interface body font family (`IBM Plex Mono`, `Anonymous Pro`, `Courier Prime`), global font scale/zoom (`90%` to `130%`), account quota gauge style (`concentric`, `bars`, `classic`).
3. **`orchestrator` (AI Agents & Personas):** Initializer runner/model/effort pair, default agent runner/model/effort pair, default living documentation style (`default_docstyle`: `sdd`, `lean`, `contract`, `custom`), tool auto-approval, spawn poll step, log tail lines, init timeout, and port release timeouts.
4. **`accounts` (Accounts, Quotas & Token Observability):** 1-token quota kickstart warmup pair (`quota_probe_model`, `quota_probe_effort`), `auto_warmup_enabled`, stats refresh interval, context warning threshold %, quota alert/danger/depleted thresholds %, and probe cooldown/timeout.
5. **`supervisor` (Native Process Supervisor):** In-memory ring buffer capacity, max queued pending inputs (`max_pending_inputs_count`), process teardown grace period, state watcher poll rate, Brief turn/byte limits, conversation search limit, caller identity ancestor walk depth, and watcher debounce.
6. **`telemetry` (System Telemetry & Database Pool):** Host telemetry poll interval (`telemetry_interval_ms`) and SQLite connection pool maximum size (`db_connection_pool_max_count`).
7. **`profiler` (Continuous Profiling, Spec 25):** Sampling interval of the run process trees (`sample_interval_ms`), the stop grace period of a run wrapped by a profiler engine (`engine_shutdown_grace_period_ms`), and the process breakdown limit of a run profile (`process_breakdown_max_items_count`).
8. **`vcs` (Git & Snapshots):** Default branch name (`default_branch`, global only), Git runner/model/effort pair, Forge CLI limits & timeout, `snapshots_enabled`, `minutes_checkpoint_mode` (`snapshot` | `commit` | `off`), `auto_commit_detail_level` (`title_only` | `title_and_decisions` | `full`), `auto_snapshot_on_file_change`, `max_per_project_count`, `auto_snapshot_on_init` (global only), and `scratchpad_retention_days`.
9. **`mcp` (MCP Daemon & Event Stream):** Local MCP/RPC TCP port (`mcp_daemon_port`, default `8765`), `task_wait_timeout_ms`, `approval_ttl_ms`, and SSE `event_channel_capacity_count`.
10. **`rea` (Reverse Engineering Engine, Spec 26b):** Enable toggle, max concurrent heavy jobs, idle timeout, Ghidra installation directory, max JVM heap size, and targets directory.
11. **`electronics` (Electronics Workstation, Spec 31):** Enable toggle, firmware directory, schematics directory, default serial baud rate, and default flasher tool.
12. **`desigma` (One-Ship Designer, Spec 32):** Enable toggle, `mcp_binary_path`, `app_binary_path`, `design_dir`, `enable_bridge`, `mcp_request_timeout_ms`, `auto_snapshot_on_edit`, and `max_snapshots_per_asset_count`.
13. **`solidcraft` (3D CAD Workstation, Spec 33):** Enable toggle, `app_binary_path`, `cli_binary_path`, `cad_dir`, `snapshot_timeout_ms`, `snapshot_width_px`, `snapshot_height_px`, `auto_snapshot_on_edit`, and `max_snapshots_per_asset_count`.
14. **`pcbcraft` (EDA/PCB Workstation, Spec 33):** Enable toggle, `app_binary_path`, `cli_binary_path`, `pcb_dir`, `mcp_request_timeout_ms`, `auto_snapshot_on_edit`, and `max_snapshots_per_asset_count`.
15. **`project_management` & `semantic` (Kanban/OSS & Vector RAG, Specs 22 & 34):** `project_management` (`enabled`, `tracking_mode` [`linear` | `shape_up` | `weighted`], `github_cache_ttl_seconds`) and `semantic` (`enabled`, `ignored_patterns`, `included_extensions`, `extra_paths`).

### 2.2 Schema Definition
* **`key`**: Unique identifier (e.g. `auto_approve`, `sample_interval_ms`).
* **`label`**: Human-readable title for UI rendering.
* **`field_type`**:
  * `boolean`: Renders as toggle switch (`AppToggle`).
  * `string`: Single-line text input (`AppInput`).
  * `select_enum`: Dropdown selection with defined `options` (`AppSelect`).
  * `project_agent`: Dropdown selection (`AppSelect`) populated dynamically from the project's declared `[agents.<id>]` personas (validated as a string value; e.g. `plugins.vcs.default_agent_id`).
  * `number`: Numeric input (`AppInput`).
  * `string_list`: Comma-separated array input (`AppInput`).
  * `runner`, `runner_model`, `runner_effort`: one field of a runner/model/effort pair; the Settings selects list only what the manifests declare (runners; models of the pair's runner; efforts of the selected model, plus "model default"). A runner change clears a model it does not declare, a model change clears an effort it does not declare, and saving is blocked while a pair is not declared.
* **`default_value`**: Baseline factory fallback.
* **`options`**: Allowed values for `select_enum`.
* **`agent_pairs`** (per plugin, `AgentPairSchema`): the triples among the fields (`runner_key` or `fixed_runner`, `model_key`, `effort_key`). A document that sets any key of a pair sets its model (and its runner, when configurable) in the same table, and the triple must be declared by the runner's manifest (`agent_pair_problems`). Every `[agents.<id>]` table must declare `runner` and `model` (and may declare `effort`) as such a pair (`agent_persona_problems`). Either problem makes the document invalid: a `config.toml` stops startup and a Settings save, a `rula.toml` raises `[! CONFIG]`.
* **`scope`**:
  * `global_only`: Applies system-wide and exclusively in `~/.config/one-ship/config.toml`.
  * `project_only`: Project-specific (e.g. base repo URL).
  * `cascade`: Eligible for cascading overrides at global, project, and agent levels.
* **`description`**: Explanatory note.

---

## 3. Global Configuration File (`config.toml`)

Stored at: `~/.config/one-ship/config.toml`

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

---

## 4. Project Configuration File (`rula.toml`)

Stored at project root: `<project_root>/rula.toml`

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

**Persona commands (`kopeng/persona.rs`):** the persona modal edits `[agents.<id>]` through `create_agent_persona(project_id, agent)` (an existing ID is an error), `update_agent_persona(project_id, agent)` (an undeclared ID is an error; only the modal fields — `role`, `runner`, `model`, `effort`, `accept_edits`, `auto_approve`, `args`, `context_docs`, `skills`, `system_prompt`, `dna` — are written, every other key such as `mcp_dialogue_mode` is kept, and a blank field removes its key) and `delete_agent_persona(project_id, agent_id)` (stops the agent first; a failed stop keeps the persona). All three require a valid current `rula.toml` and validate the result before writing (`write_project_rula`). The ID must be lowercase letters, digits, `-` or `_`, the role non-empty, and `runner` / `model` (required) / `effort` (optional) a pair its manifest declares (§1.2); the modal lists only the runners of `list_agent_runners()` and only the models and efforts the selected runner declares, with no preselected pair for a new persona. The platform agent IDs `system`, `git`, `initializer`, `removal-resolver` and `config-resolver` cannot be declared in `[agents.<id>]` (rejected by `agent_persona_problems`) or created as personas; the `git` agent resolves its runner/model/auto_approve through the plugin cascade (`[plugins.orchestrator]` / `[plugins.vcs]`). The three are daemon commands (Spec 12 §7.2; arguments `{ projectId, agent }` / `{ projectId, agentId }`); errors are `PersonaError` as `ApiErrorResponse`.

### 4.1 In-Workspace Project Configuration Hub (`ProjectConfigPanel.vue`)
Project-scoped configuration (`rula.toml`) is hosted by the fixed `settings` workspace tab (Spec 37); it is no longer embedded in `ProjectOverviewTab.vue`:
1. **Full-Bleed Geometry:** The settings tab wrapper has no padding or scroll container; `ProjectConfigPanel.vue` fills the full width/height of the stage. The panel has no outer border or card background. The left category sidebar is full height (`w-56`, `ui-surface-list`) and keeps its background below the last category when the list is short; the right content column scrolls independently in the remaining space.
2. **Single Top Toolbar:** One `h-9` `.ui-search-box` row replaces the former title block, file badge, search input, reload button and bottom save footer. It contains the parameter search (categories and fields), the match counter, `Discard` and `Save` (both `h-6`, text labels, no tooltips). The panel title and the `rula.toml` badge are not rendered; the gear tab tooltip identifies the view.
3. **No Redundant Category Header:** The right pane does not repeat the active category name or its enabled/disabled status (the sidebar already shows both). Category quick actions (`Open Lab`, `Open Board`, `Index Knowledge`, `Sync`, etc.) remain in a title-less action row.
4. **Vertical Tabbed Categories:** The left sidebar dynamically lists all plugins declaring project or cascade scope fields (`orchestrator`, `vcs`, `rea`, `supervisor`, `telemetry`, `profiler`, `mcp`). Category buttons are `h-7`/`text-[10px]`; the list has no separate “Categories” heading.
5. **Real-Time Parameter Search:** Live filtering matches against plugin names, field labels, parameter keys, and descriptions in the top toolbar.
6. **Provenance Observability:** Every field displays an explicit badge denoting whether its effective value originates from `[rula.toml]`, `[global]`, or schema `[default]`. Field cards use `ui-surface-card`, an 11px label, 10px description and 9px default hint.
7. **Automatic Disk Reload:** The daemon project watcher already emits `projects-updated` when `rula.toml` changes. The panel subscribes, debounces, re-reads `get_scoped_project_config`, and compares the values. With no local edits it applies the disk version automatically; with local edits it shows a warning banner (`rula.toml changed on disk. Discard loads the disk version; Save overwrites it with the in-memory version.`) and never overwrites them silently. There is no manual reload button.
8. **Unsaved-Changes Navigation Guard:** A JSON snapshot of the loaded values defines the dirty state. While dirty, the panel registers a UI navigation guard consumed by `coreStore.setActiveProject` and `workspaceLayoutStore.setWorkspaceTab` (and by `navigateTo`/daemon snapshot project switches before they mutate state). Leaving the settings tab or switching project opens a blocking confirmation modal with `Save & Continue`, `Discard & Continue` and `Cancel`. Saving or discarding resumes the pending navigation; a failed save/discard keeps the user in the panel with the error banner.
9. **Unified Feature Toggles & Subsystem Visibility (`ProjectConfigPanel.vue`):** The panel integrates subsystem activation (`[plugins.<id>] enabled`) and workspace tab visibility (`toggle_project_tab_visibility` via `projects.hidden_tabs`) directly into the left category list (with contextual quick actions in the right pane and disabled parameter forms when a feature is off).
10. **Project Star / Favorite & Archiving Hierarchy:** Projects support a favorite toggle (`projects.is_favorite` integer in SQLite, toggled via `toggle_project_favorite`) and an archive toggle (`projects.is_archived` integer in SQLite, toggled via `toggle_project_archived`). Favorited projects are ordered first, and archived projects are hidden from the vertical project rail and drawer unless the archived filter is enabled. The archived filter and “Track New Project” live only in the open drawer; the collapsed rail exposes only the drawer toggle.
11. **Scope Decoupling:** `SettingsModal.vue` is reserved strictly for global system settings (`~/.config/one-ship/config.toml`), triggered from the main desktop header (`AppHeader.vue`).

---

## 5. Platform Skill Catalog Injection & Taxonomy

### 5.1 Base Platform Skills vs Specialized Skills (SSOT: `templates/skills/`)
Clicking the `<BrainCircuit />` button in the Project Card toolbar (or executing programmatic pre-boot during Phase 1) synchronizes the **Base Platform Skills Catalog** from `templates/skills/` into `<project_root>/.agents/skills/`.

**Single template source (`kopeng/skill.rs`):** the canonical tree is `templates/` of this repository, located at compile time (`TEMPLATES_ROOT = $CARGO_MANIFEST_DIR/../templates`). Every project skill is a symlink into `templates/skills/<name>`, and prompt templates are read from `templates/prompts/<name>.md`. There is no embedded copy, no CWD-relative search and no `~/.config/one-ship` override: a missing template, an unknown specialized skill or an unknown prompt name is a typed `SkillError`, and a failed step fails the whole sync (`inject_platform_skill` = base symlinks + re-link of present specialized skills + AGENTS.md managed block + runner MCP configs + `rula.toml` version stamp). A corrupt or non-object `opencode.json` / `.mcp.json` is reported, never overwritten. `CURRENT_PLATFORM_VERSION` is the only platform version; a unit test asserts that `oneship-core/SKILL.md` declares the same `version:`. Skill names accepted by the skill commands are single path components (`[A-Za-z0-9._-]`, no leading dot).

**Runner manifests (`config::load_runner_schema`):** read only from `TEMPLATES_ROOT/runners/<runner>.toml`; an unknown runner, unreadable file or invalid manifest is an error (no CWD-relative search, no embedded copies, no runner-id aliases). Each `[[models]]` entry may declare `context_window_tokens_count`, the source of the context percentage in Spec 19 §3.1.

1. **Base Platform Skills (Injected Pre-boot):**
   - `oneship-core`: Runtime configuration (`rula.toml`), service execution, and bounded persona rules (<100 words).
   - `oneship-delegation`: Multi-agent task handoffs and closed-loop callbacks.
   - `oneship-devlog`: Database-first session journals and active task lifecycle.
   - `oneship-snapshots`: Pre-refactor Git checkpoints and rollbacks.
   - `oneship-techdebt`: Architectural debt triage and registry.
   - `oneship-artifacts`: Live artifact contracts (Inline Ephemeral, Scratchpad, Living Specs).

2. **Specialized Skills (`SPECIALIZED_SKILLS` in `kopeng/skill.rs`, Injected On-Demand during Onboarding, Flag Resolution, or Plugin Activation):**
   - `oneship-initializer`: Loaded during Phase 1 brownfield/greenfield project initialization and removed upon `complete_project_initialization`.
   - `oneship-docstyles`, `oneship-docstyle-sdd`, `oneship-docstyle-lean`, `oneship-docstyle-contract`, `oneship-docstyles-crafter`: Loaded when scaffolding or resolving living documentation (`docs` flag or greenfield scoping).
   - `oneship-archetypes`: Loaded during greenfield bootstrapping to guide project archetype selection.
   - `oneship-profiler-crafter`: Loaded when resolving the `profiler` flag to configure profiler engines.
   - `oneship-rea`: Loaded when `[plugins.rea]` is enabled (Spec 26b).
   - `oneship-desigma`: Loaded when `[plugins.desigma]` is enabled (Spec 32).
   - `oneship-pcbcraft`: Loaded when `[plugins.pcbcraft]` is enabled (Spec 33).
   - `oneship-project-mgmt`: Loaded when `[plugins.project_management]` is enabled (Spec 34).

```text
<project_root>/
  └── .agents/
        └── skills/
              ├── oneship-core/
              │     ├── SKILL.md                 <-- Core orchestration & rula.toml configuration
              │     └── references/
              │           └── rula_schema.md     <-- Complete 4-level cascade & configuration spec
              ├── oneship-delegation/
              │     └── SKILL.md                 <-- Multi-agent task handoffs & closed-loop callbacks
              ├── oneship-devlog/
              │     └── SKILL.md                 <-- Database-first session journals & task lifecycle
              ├── oneship-snapshots/
              │     └── SKILL.md                 <-- Pre-refactor Git checkpoints & rollbacks
              ├── oneship-techdebt/
              │     └── SKILL.md                 <-- Architectural debt triage & registry
              └── oneship-artifacts/
                    └── SKILL.md                 <-- Triple-mode artifact delivery (Ephemeral, Scratchpad, Docs)
```

### 5.2 Content Breakdown & SSOT
1. **No Duplicated MCP Schemas:** MCP tool schemas via `tools/list` serve as the Single Source of Truth (SSOT). Skills govern operational protocol, policies, and invocation timing.
2. **`oneship-core`:** Frontmatter version anchor (`version: 0.2.6`), `rula.toml` structure, interactive command options, and bounded persona rules (<100 words).
3. **`oneship-delegation`:** Delegation protocol via `delegate_agent_task` and status callbacks via `report_task_status`.
4. **`oneship-devlog`:** Database-first session journaling (`record_session_minutes`), auto-milestone snapshots, and active tasks (`manage_todo_item`).
5. **`oneship-snapshots`:** Git checkpoints (`create_snapshot`), snapshot inspection (`list_snapshots`), and rollback operations (`restore_snapshot`).
6. **`oneship-techdebt`:** Severity classification and debt recording (`record_technical_debt`).
7. **`oneship-artifacts`:** Triple-mode artifact contracts: inline ephemeral payloads (`[EPHEMERAL]`), evolving scratchpads (`scratchpad/*.md`), and persistent specifications (`docs/*.md`).

### 5.3 Skill Versioning & Lifecycle Detection
To ensure projects maintain compatibility across ONE-SHIP orchestrator upgrades without corrupting customized configurations:
1. **`CURRENT_PLATFORM_VERSION`:** Managed globally as the single source of truth in `src-tauri/src/kopeng/skill.rs` (`"0.2.6"`).
2. **`SkillStatus` Hydration (`kopeng/skill.rs`):** Every project inspects `.agents/skills/oneship-core/SKILL.md` and `rula.toml` during hydration:
   - `is_installed: bool`: Whether `oneship-core/SKILL.md` exists.
   - `installed_version: Option<String>`: Extracted from `SKILL.md` frontmatter `version: X.Y.Z`.
   - `current_version: String`: `CURRENT_PLATFORM_VERSION`.
   - `is_outdated: bool`: True if `installed_version != CURRENT_PLATFORM_VERSION` or any base skill is not a valid symlink.
   - `is_symlinked: bool`: Whether all base skills are valid symlinks into `templates/skills/`.
   - `schema_version: Option<String>`: Extracted from `rula.toml [project].oneship_version`.
   - `needs_migration: bool`: True if the project requires a deterministic platform migration (Spec 24).
3. **Safe Updates:** Injected updates overwrite `.agents/skills/oneship-*/` exclusively; `rula.toml` is strictly preserved.
4. **UI Tri-State Indication:**
   - *Not Installed:* Dim icon (`opacity-50`).
   - *Active / Up to Date:* Green/Emerald icon (`text-emerald-400`).
   - *Outdated:* Amber warning indicator (`text-amber-400 animate-pulse`) with version upgrade tooltip.



