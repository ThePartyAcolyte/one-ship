# Project Versioning and Migration

## Purpose

Keep tracked projects synchronized with evolving ONE-SHIP platform invariants (MCP tools, platform skills, runner configs, and `rula.toml` schemas) without configuration drift, broken agent communication, or destructive rewrites, while cleanly separating non-invasive Phase 1 onboarding from on-demand Phase 2 flag resolution.

## Scope

- Platform versioning (`0.2.x`) in `rula.toml` and `SkillStatus` hydration telemetry.
- Two-tier migration architecture: Tier 1 deterministic schema/asset migration (`migrate_project_deterministic`) and Tier 2 contextual remediation.
- Status of the global `system` agent (retired CLI persona vs. planned direct-API Voice and Interface Orchestrator).
- Phase 1 project initialization (`initialize_project_agent`, `create_greenfield_project`, `project_initializations` ledger, REPL teardown via `complete_project_initialization`, and bounded watcher).
- Phase 2 on-demand initialization flag resolution (`launch_flag_resolution_agent`) across `docs`, `profiler`, `todos`, `tech_debt`, `invalid_config`, and `removal`, plus ephemeral skill cleanup and objective persona creation.

Base vs. specialized skill injection mechanics and `TEMPLATES_ROOT` resolution are specified in [Platform Skill Injection](03-platform-skill-injection.md).

## Model / Design

### Architecture and data flow

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ ONE-SHIP ONBOARDING, LIFECYCLE & MIGRATION ENGINE                           │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  [ Project Ingestion / Hydration ] ──► [ Version Status Comparison ]        │
│                                                   │                         │
│                    ┌──────────────────────────────┴──────────────────────┐  │
│                    ▼                                                     ▼  │
│          [ Up-to-date: current ]                              [ Outdated ]  │
│                    │                                                     │  │
│              (Active State)                                              ▼  │
│                                                              [ UI Migration │
│                                                                Badge / Action]
│                                                                          │  │
│                                                                          ▼  │
│                                                                [ Migration  │
│                                                                  Engine ]   │
│                                                                          │  │
│                    ┌─────────────────────────────────────────────────────┤  │
│                    ▼                                                     ▼  │
│       [ Tier 1: Deterministic Engine ]                       [ Tier 2: AI   │
│       - Git HEAD checkpoint recorded                           Orchestrator │
│       - Migrate [plugins.snapshots] -> [plugins.vcs]           (Planned) ]  │
│       - Sync Base Skills (.agents/skills/oneship-*/ symlinks)- Guided schema│
│       - Sync runner MCP configs (.mcp.json, opencode.json)   adjustments  │
│       - Relative CLAUDE.md -> AGENTS.md symlink             - Contextual   │
│       - Refresh AGENTS.md managed block                      remediation  │
│       - Set rula.toml [project] oneship_version                             │
│                                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│ PHASE 1: PROJECT INITIALIZATION (Inside <project_path>)                     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  [ Programmatic Pre-Boot (Daemon) ]                                         │
│    ├── Inject runner MCP configs (.mcp.json, opencode.json)                 │
│    ├── Symlink Base Skills from templates/skills/ (6 base skills)           │
│    ├── Create relative symlink CLAUDE.md -> AGENTS.md                       │
│    └── Prepend AGENTS.md managed governance block                           │
│              │                                                              │
│              ▼                                                              │
│  [ Project Initializer Execution (inside project_path) ]                    │
│    ├── Modes: Supervised (Chat Overlay) | Unattended (Background Tokio)     │
│    │          | Greenfield                                                  │
│    ├── Immutable: auto_approve = true (zero stdin stalls)                   │
│    ├── Non-invasive: Source code strictly read-only                         │
│    ├── Generate: rula.toml (services, tasks, decoupled personas <= 100w)    │
│    ├── Record: Genesis Milestone in SQLite agent_session_journal            │
│    └── Emit: raise_initialization_flag (docs, todos, tech_debt, profiler)   │
│              │                                                              │
│              ▼                                                              │
│  [ Phase 1 Clean Termination ] ──► Project Card displays brutalist badges   │
│                                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│ PHASE 2: ON-DEMAND FLAG RESOLUTION (Independent User-Triggered Sessions)    │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Project Card Badges:                                                       │
│    [! INIT: DOCS]      [! INIT: TODOS]   [! INIT: TECH_DEBT]   [! PROFILER] │
│           │                   │                   │                 │       │
│           ▼                   ▼                   ▼                 ▼       │
│    [Select Docstyle]   [Stage Tasks]      [Stage Debt]      [Craft Profiler]│
│    Inject docstyle     Review & persist   Review & persist  Inject crafter  │
│    skill on-demand     into SQLite        into SQLite       skill on-demand │
│           │                   │                   │                 │       │
│           └───────────────────┴─────────┬─────────┴─────────────────┘       │
│                                         ▼                                   │
│                           [ resolve_initialization_flag ]                   │
│                           SQLite status = 'resolved'                        │
│                           Badge cleared from Project Card                   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### `rula.toml` version invariant and deterministic migration

Projects declare their platform baseline in `rula.toml`:

```toml
[project]
name = "service-backend"
oneship_version = "0.2.7"
```

- A valid `rula.toml` without `[project].oneship_version` (or an absent `rula.toml` during `get_project_version`) reads as `UNVERSIONED_PLATFORM_VERSION` (`"0.2.1"`).
- An unreadable or syntactically invalid `rula.toml` returns `MigrationError::InvalidRula` — it is never silently treated as `"0.2.1"`. *(Historical note: prior to 2026-11-03, `str::parse::<toml::Value>()` failed on full TOML documents in `toml` 1.x and discarded the error, causing all projects to read `"0.2.1"`.)*
- `set_project_version` edits only the `oneship_version` line inside `[project]` (appending `[project]` at EOF if absent), preserves all other lines and comments, verifies that the result parses as TOML and reads back the new version, and only then writes to disk.
- `migrate_project_deterministic(project_path)` (registered projects only, executed on Tokio's blocking pool) requires `rula.toml` (`MigrationError::MissingRula` otherwise), records the current Git `HEAD` (`snapshots::head_commit`) as `git_checkpoint` without committing, migrates any legacy `[plugins.snapshots]` configuration in `rula.toml` into `[plugins.vcs]`, invokes `skill::inject_platform_skill` (base skill symlinks, specialized skill re-linking, `AGENTS.md` managed block, runner MCP configs, `CLAUDE.md` symlink, and version stamp), and verifies that `[project].oneship_version` now equals `CURRENT_PLATFORM_VERSION`. `MigrationResult` lists only the steps actually performed.

### Global system agent vs. project initializer

- **Global system agent (`system`) — planned redesign:** the legacy CLI-based `system` persona was retired from the project taskbar and chat UI on `2026-11-16`. It is planned to return as a direct-API **Voice and Interface Orchestrator** (e.g., Gemini Live API with native function calling rather than a CLI runner) operating at the interface level to inspect cross-project tasks and semantic RAG, navigate UI views, and draft/delegate instructions to project-scoped agents.
- **Project initializer (`initializer`):** a project-scoped transient persona instantiated strictly inside `<project_path>`. In `supervised` mode, its dialogue stream connects to the Chat Overlay so the developer can review stack analysis, refine persona boundaries, and observe Genesis Milestone recording.

#### Coordinated REPL process teardown and watcher

Because agent subprocesses run in interactive REPL mode (`--input-format stream-json --output-format stream-json`), they stay alive after completing a turn:

- Initialization closes exclusively through the `complete_project_initialization` MCP tool ([Streamable HTTP MCP Daemon](04-streamable-http-mcp-daemon.md)), which the initializer calls as its final action. It requires a valid `rula.toml` declaring `[project].id` and a Genesis milestone in `agent_session_journal`. On success it marks `completed`, removes `oneship-initializer`, emits `agent-status-report` (`status: "completed"`, clearing the `"ORCHESTRATION UNDER CONSTRUCTION"` overlay), and stops the agent. The mere existence of `rula.toml` closes nothing (e.g., greenfield REA labs create `rula.toml` before the first turn).
- Any initializer response advances the phase to `PROPOSAL_PRESENTED_PHASE` (`"Proposal presented — review in Chat"`).
- The daemon watcher (active for `supervised` and `unattended`; `greenfield` has none) polls every `orchestrator.agent_spawn_poll_step_ms` up to `orchestrator.init_timeout_minutes` (default `15`). It stops when status reaches `completed`; in `supervised` mode it also stops at `PROPOSAL_PRESENTED_PHASE`, emitting `waiting_for_user` so review continues in chat. If the agent exits, fails, disappears, or times out before completing, initialization is marked `failed`, `oneship-initializer` is removed, the agent is stopped, and `agent-status-report` (`failed`) is emitted.

#### Initialization and greenfield launch contracts

- **`initialize_project_agent(project_id, mode, is_retry, runner?, model?, effort?)`:** requires a registered project (name and path come from the registry). `mode` must be `"greenfield"`, `"supervised"`, or `"unattended"` (other values are rejected). The project UID comes from `[project].id` of a valid `rula.toml` (a new `proj-<uuid>` is generated if the file or key is absent); an invalid `rula.toml` stops launch for repair via `[! CONFIG]`. The legacy `uid` alias is not read. Init-state persistence, Git checkpoint (`snapshots::head_commit`), and skill injection propagate errors; progress is reported only via `agent-status-report`.
- **`create_greenfield_project`:** rejects empty names, `.`/`..`, names containing path separators, and non-directory parent or existing non-empty target paths (`InitializerError::InvalidProjectName`, `ParentNotDirectory`, `TargetNotEmpty`). A failed `git init` aborts creation. For REA labs, `rula.toml` is built programmatically as a validated TOML document (`[project]` with `id`/`name`/`oneship_version`, `[plugins.rea]`, `[agents.reverser]` with `runner` only when selected) before writing.

### Docstyles and initialization flags lifecycle

#### Phase 1: non-invasive reconnaissance

- Application source code is strictly **read-only** during Phase 1; the initializer never modifies source files to inject task IDs or comments.
- Documentation gaps, inline `TODO`/`FIXME` items, architectural technical debt, and missing profiler drivers are recorded via `raise_initialization_flag`.
- The inaugural baseline state is recorded as a **Genesis Milestone** in `agent_session_journal`.

#### Phase 2: on-demand flag resolution modalities

1. **Launch contract (`launch_flag_resolution_agent(project_id, flag_id, chosen_option?, cleanup_strategy?, runner?, model?, effort?)`):**
   - Reads the flag from SQLite; it must belong to `project_id` and be `pending` (`FlagResolverError::FlagNotInProject` / `FlagNotPending`). Its stored `flag_type` selects the agent, prompt template, and skill, and `metadata_json` is passed to the prompt (`null` when absent).
   - Only `docs` flags accept and require `chosen_option` (`sdd` | `lean` | `contract` | `custom`) and `cleanup_strategy` (`purge` | `annotate` | `preserve`); other flag types reject them (`FlagResolverError::OptionNotApplicable`).
   - Captures a Pre-Resolution Safety Snapshot for any Git-tracked project (no opt-out); snapshot failure aborts launch. The opening user turn is persisted before spawning, and an insert failure aborts launch.
2. **Documentation standard (`docs`):** `DocstyleSelectionModal` selects archetype and legacy-docs cleanup policy, takes a safety snapshot, injects the archetype skill (`oneship-docstyle-sdd`, `oneship-docstyle-lean`, `oneship-docstyle-contract`, or `oneship-docstyles-crafter`) plus `oneship-docstyles`, and opens the Action-First Chat Overlay (`templates/prompts/flag_resolve_docs.md`) with an initial `docs/` draft.
3. **In-vivo profiler driver (`profiler`):** takes a safety snapshot, injects `oneship-profiler-crafter`, opens `templates/prompts/flag_resolve_profiler.md`, inspects `[scripts]` in `rula.toml`, and proposes `[scripts.<name>.profiler]` bindings per script.
4. **Inline tasks and technical debt (`todos` / `tech_debt`):** takes a safety snapshot, scans code in an Action-First Chat Overlay, stages items to SQLite ledgers, and applies the chosen cleanup policy (Purge vs. Annotate vs. Preserve).
5. **Operational resolvers (`invalid_config` and `removal`):**
   - `invalid_config`: raised automatically when `rula.toml` fails validation (`sync_project_config_flag`), launches `config-resolver` (`templates/prompts/flag_resolve_config.md`), and closes automatically once `rula.toml` validates.
   - `removal`: raised when project de-integration fails (`launch_project_removal_agent`), launches `removal-resolver` (`templates/prompts/flag_resolve_removal.md`), and concludes only via `complete_project_removal`.
6. **Closure:** standard bootstrapping flags (`docs`, `todos`, `tech_debt`, `profiler`) conclude by calling `resolve_initialization_flag(flag_id, status)` (`"resolved"` | `"dismissed"`), updating SQLite and clearing the Project Card badge in real time.

### Ephemeral skill lifecycle and objective persona creation

- **Ephemeral initializer skill:** `oneship-initializer` is injected into `.agents/skills/oneship-initializer/` during Phase 1 pre-boot and purged via `cleanup_ephemeral_skill` by `complete_project_initialization` on success or by the init watcher on failure.
- **Objective persona creation:** specialist personas under `[agents.<id>]` must reflect actual decoupled directory boundaries and subsystem domains (e.g., a single lead architect for compact utilities; separate domain specialists only where distinct subsystems exist). Arbitrary numerical quotas (such as forcing "2 to 4 specialists") are prohibited.

## Contracts

- **Version constants (`src-tauri/src/kopeng/migration.rs`, `skill.rs`):** `UNVERSIONED_PLATFORM_VERSION = "0.2.1"`, `CURRENT_PLATFORM_VERSION = "0.2.7"`.
- **`SkillStatus` (`kopeng/skill.rs`, `#[ts(export)]`):** `{ is_installed: bool, installed_version: Option<String>, current_version: String, is_outdated: bool, is_symlinked: bool, schema_version: Option<String>, needs_migration: bool }`.
- **`MigrationResult` (`kopeng/migration.rs`, `#[ts(export)]`):** `{ from_version: String, to_version: String, applied_steps: Vec<String>, git_checkpoint: Option<String>, message: String }`.
- **SQLite table `project_initializations` (`src-tauri/src/db.rs`):**

```sql
CREATE TABLE IF NOT EXISTS project_initializations (
    project_path TEXT PRIMARY KEY,
    status TEXT NOT NULL,
    mode TEXT NOT NULL,
    current_phase TEXT,
    last_message TEXT,
    git_checkpoint TEXT,
    agy_conversation_id TEXT,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

- **Daemon RPC commands (`src-tauri/src/api.rs`):** `migrate_project_deterministic`, `initialize_project_agent`, `create_greenfield_project`, `launch_flag_resolution_agent`, `launch_project_removal_agent`, `get_project_init_flags`, `resolve_project_init_flag`, `rollback_project_init`.
- **MCP tools (`src-tauri/src/mcp.rs`):** `raise_initialization_flag`, `resolve_initialization_flag`, `complete_project_initialization`, `complete_project_removal`.
- **Typed errors (`api::ApiError`):** `MigrationError`, `InitializerError`, `FlagResolverError`.
- **Config keys (`config.rs`):** `orchestrator.init_runner`, `orchestrator.init_model`, `orchestrator.init_effort`, `orchestrator.default_docstyle`, `orchestrator.agent_spawn_poll_step_ms`, `orchestrator.init_timeout_minutes`, `vcs.auto_snapshot_on_init`.

## Implementation Status

| Area | Status | Notes |
|---|---|---|
| `rula.toml` version stamping & deterministic migration (`kopeng/migration.rs`) | Implemented | Verified `UNVERSIONED_PLATFORM_VERSION`, `MigrationError`, `MigrationResult`, `run_deterministic_migration` (including `[plugins.snapshots]` -> `[plugins.vcs]`). |
| `SkillStatus` hydration (`kopeng/skill.rs`) | Implemented | Verified `CURRENT_PLATFORM_VERSION = "0.2.7"` and `SkillStatus` struct. |
| Phase 1 initializer & `project_initializations` (`kopeng/initializer.rs`, `db.rs`) | Implemented | Verified `INITIALIZER_AGENT_ID`, `PROPOSAL_PRESENTED_PHASE`, `InitializerError`, and SQLite table. |
| Phase 2 flag resolver (`kopeng/flag_resolver.rs`) | Implemented | Verified `FlagResolverError` and launch contracts for `docs`, `profiler`, `todos`, `tech_debt`, `invalid_config`, `removal`. |
| Global `system` Voice & Interface Orchestrator | Roadmap | Legacy CLI `system` persona retired `2026-11-16`; direct-API Gemini Live orchestrator planned. |

## Related Specs

- [Configuration Schema and Cascade](02-configuration-schema-and-cascade.md)
- [Platform Skill Injection](03-platform-skill-injection.md)
- [Streamable HTTP MCP Daemon](04-streamable-http-mcp-daemon.md)
- [Daemon Operator API and Lifecycle](05-daemon-operator-api-and-lifecycle.md)

Supersedes legacy: `24_project_versioning_and_migration_engine.md`
