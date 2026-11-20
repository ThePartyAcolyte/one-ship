> [!WARNING]
> **LEGACY SPECIFICATION:** Superseded by [01-architecture/06-project-versioning-and-migration.md](../../specs/01-architecture/06-project-versioning-and-migration.md). Preserved for historical provenance only.

# Specification 24: Project Versioning, Stepwise Migration Engine & Global System Agent

---

## 1. Overview & Objectives

### 1.1 Context
As ONE-SHIP evolves, platform invariants (such as Streamable HTTP MCP tools, `mcp_dialogue_mode`, modular platform skills in `.agents/skills/oneship-*/`, MCP plugin configurations, and `rula.toml` configuration schemas) are updated. Projects initialized in earlier builds may lag behind by multiple platform releases.

To prevent configuration drift, broken agent communication, or manual maintenance overhead, this specification establishes:
1. **Platform Version Invariant:** Platform baseline versioned sequentially under `0.2.x` (e.g. `0.2.4`).
2. **Two-Tier Migration Architecture:**
   - **Tier 1 (Deterministic):** Atomic, idempotent schema migrations and asset synchronization (Base Platform Skills catalog in `.agents/skills/`, `mcp_config.json`, `AGENTS.md` managed governance block) with automated Git checkpointing.
   - **Tier 2 (Contextual AI):** Dynamic semantic inspection (evaluating `package.json`, build scripts, bounded domain specialist personas) via targeted prompts.
3. **First-Class Global System Agent (`system`):** A platform-level orchestrator agent available in the unified Chat Overlay, providing cross-project health monitoring, multi-account management, guided platform migrations, and repository diagnostics.
4. **Two-Phase Project Onboarding Protocol:** Strict decoupling of Phase 1 (deterministic pre-boot, non-invasive scan, `rula.toml` generation, Genesis Milestone, and flag emission) from Phase 2 (independent, on-demand resolution of documentation, task backlogs, technical debt, and profilers via Project Card badges).

---

## 2. Architecture & Data Flow

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ ONE-SHIP ONBOARDING, LIFECYCLE & MIGRATION ENGINE                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  [ Project Ingestion / Hydration ] ──► [ Version Status Comparison ]        │
│                                                   │                         │
│                    ┌──────────────────────────────┴──────────────────────┐  │
│                    ▼                                                     ▼  │
│          [ Up-to-date: v0.2.4 ]                               [ Outdated ]  │
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
│       - Pre-migration Git checkpoint                           System Agent │
│       - Stepwise AST patch (v0.2.1 -> v0.2.4)                  (`system`) ] │
│       - Sync Base Skills (.agents/skills/oneship-*/ symlinks) - Guided schema│
│       - Sync runner MCP configs (.mcp.json, opencode.json)   adjustments  │
│       - Relative CLAUDE.md -> AGENTS.md symlink             - Contextual   │
│       - Refresh AGENTS.md managed block                      remediation  │
│       - Set rula.toml [project] oneship_version                            │
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

---

## 3. Configuration & Schema Contracts

### 3.1 `rula.toml` Version Invariant
```toml
[project]
name = "service-backend"
oneship_version = "0.2.5"
```

A `rula.toml` without the key is version `0.2.1` (`UNVERSIONED_PLATFORM_VERSION`); an unreadable or invalid `rula.toml` is an error (`MigrationError::InvalidRula`), never read as `0.2.1`. `set_project_version` edits only the `oneship_version` line of `[project]` (appending the table at the end of the file when missing), keeps every other line and comment, and writes only after the result parses and reads back the new version.

`migrate_project_deterministic(project_path)` (registered projects only, blocking pool) runs `skill::inject_platform_skill` — platform skill links, AGENTS.md managed block, runner MCP configs, `CLAUDE.md` link, version stamp — then verifies the recorded version. A project without `rula.toml` cannot be migrated (`MissingRula`). `MigrationResult { from_version, to_version, applied_steps, git_checkpoint, message }` lists only the actions actually performed.

> Until 2026-11-03 `rula.toml` was parsed with `str::parse::<toml::Value>()`, which in `toml` 1.x parses a single value and rejects every document; the error was discarded, so the version always read `0.2.1` and every project with a `rula.toml` showed `needs_migration`.

### 3.2 Hydration Telemetry (`SkillStatus`)
```rust
pub struct SkillStatus {
    pub is_installed: bool,
    pub installed_version: Option<String>,
    pub current_version: String, // CURRENT_PLATFORM_VERSION ("0.2.6")
    pub is_outdated: bool,
    pub is_symlinked: bool,
    pub schema_version: Option<String>,
    pub needs_migration: bool,
}
```

---

## 4. Global System Agent (Planned Redesign) vs. Project Initializer

### 4.1 Global System Agent (`system`) — Planned Voice & Interface Orchestrator Redesign
The legacy CLI-based `system` persona was retired from the project taskbar/chat UI on `2026-11-16`. It is planned to be reintroduced under a **new architecture**: a direct-API **Voice & Interface Orchestrator** (e.g., using **Gemini Live API** with native function calling rather than a CLI runner) operating globally at the interface level to inspect cross-project tasks and semantic RAG, navigate UI views, and draft/delegate instructions to project-scoped agents.

### 4.2 Project Initializer
1. **Identity & Scope:**
   - A project-scoped transient persona instantiated strictly **inside `<project_path>`** within the project's workspace.
   - It is NOT the `system` agent.
2. **Supervised Mode Integration:**
   - In Supervised mode, the Project Initializer runs in `<project_path>` with its dialogue stream wired into the Chat Overlay.
   - The developer can review stack analysis, provide feedback on persona boundaries, and observe Genesis Milestone recording.
3. **Relational Ledger & Idempotent Schema (`project_initializations`):**
   - Active and completed initialization runs are recorded in SQLite `project_initializations`:
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
   - Schema validation dynamically guarantees the presence of `agy_conversation_id` via PRAGMA inspection and idempotent `ALTER TABLE` execution.
4. **Coordinated REPL Process Teardown & Overlay Unlock:**
   - Because agent processes run in interactive REPL mode (`--input-format stream-json --output-format stream-json`), they remain open and do not self-terminate at turn conclusion.
   - The initialization closes only through the `complete_project_initialization` MCP tool (Spec 12), which the initializer calls as its final action. It requires a valid `rula.toml` declaring `[project].id` and the Genesis milestone. On success it records `completed`, removes `oneship-initializer`, emits `agent-status-report` (`status: "completed"`), which clears the `"ORCHESTRATION UNDER CONSTRUCTION"` overlay, and stops the agent. The mere existence of `rula.toml` closes nothing: a greenfield REA lab has one before the first turn.
   - Any initializer response marks the phase `PROPOSAL_PRESENTED_PHASE`.
   - **Watcher (supervised / unattended; greenfield has none):** it polls every `orchestrator.agent_spawn_poll_step_ms`, bounded by `orchestrator.init_timeout_minutes` (default 15). It ends on `completed`. A supervised run also ends at the proposal phase, emitting `waiting_for_user`, and the review continues in the chat. If the agent exits, fails or disappears before completing, or the timeout expires, the initialization is marked `failed`: skill removed, agent stopped, `agent-status-report` `failed`.

5. **Launch Contract (`initialize_project_agent(project_id, mode, is_retry, runner?, model?, effort?)`):**
   - The project must be registered; name and path come from the registry. `mode` is `greenfield` | `supervised` | `unattended`, and any other value is rejected (it formerly ran as unattended).
   - The uid is `[project].id` of a valid `rula.toml` (a new `proj-<uuid>` when the file or key is absent). An invalid `rula.toml` stops the launch and is repaired through the `[! CONFIG]` flag. The legacy `uid` alias is not read.
   - Init-state reads/writes, the git checkpoint (`snapshots::head_commit`) and skill injection propagate their errors. The initializer reports only through the `agent-status-report` event.
   - `create_greenfield_project` rejects empty, `.`/`..` and separator-containing names and a non-directory parent or target. It treats a failed `git init` as an error. The REA lab `rula.toml` is built as a TOML document (`[project]` with `id`/`name`/`oneship_version`, `[plugins.rea]`, `[agents.reverser]` with `runner` only when chosen) and validated before it is written.

---

## 5. Docstyles & Initialization Flags Lifecycle

### 5.1 Non-Invasive Reconnaissance & Phase 1 Boundary
- Application source code is strictly **read-only** during Phase 1. The agent never alters, rewrites, or mutates code files to inject task IDs or comments.
- Discovered documentation gaps, inline tasks (`TODO`/`FIXME`), architectural technical debt, and missing profiler drivers are recorded via the `raise_initialization_flag` MCP tool.
- Phase 1 terminates cleanly without resolving flags or selecting Docstyles during the scan.
- The inaugural baseline state is preserved as a **Genesis Milestone** in `agent_session_journal`.

### 5.2 Phase 2 Resolution Modalities & Targeted Archetype Skills
When the user initiates Phase 2 resolution from the Project Card badges:

0. **Launch Contract (`launch_flag_resolution_agent(project_id, flag_id, chosen_option?, cleanup_strategy?, runner?, model?, effort?)`):**
   - The flag is read from SQLite: it must belong to `project_id` and be `pending` (`FlagResolverError::FlagNotInProject` / `FlagNotPending`); its stored `flag_type` selects the agent, template and skill, and its `metadata_json` reaches the prompt (`null` when absent). Project name and path come from the registry.
   - Only `docs` takes `chosen_option` (`sdd` | `lean` | `contract` | `custom`) and `cleanup_strategy` (`purge` | `annotate` | `preserve`), and requires both; other flag types reject them (`OptionNotApplicable`).
   - The Pre-Resolution Safety Snapshot is taken for every flag of a project under git (no opt-out); a failed snapshot stops the launch. The opening user turn is recorded before the spawn and a failed insert stops it too.
1. **Documentation Standard (`docs`):**
   - Launches `DocstyleSelectionModal` presenting archetype comparison (SDD, Lean, Contract, Custom) and the legacy-docs cleanup policy.
   - Captures automated Pre-Resolution Safety Snapshot (`create_snapshot`).
   - Injects the selected archetype skill (`oneship-docstyle-sdd`, `oneship-docstyle-lean`, `oneship-docstyle-contract`, or `oneship-docstyles-crafter` for Custom) plus the base `oneship-docstyles` into `<project_path>/.agents/skills/`.
   - Spawns Action-First Chat Overlay (`flag_resolve_docs.md`), where the agent immediately presents the initial `docs/` draft.
2. **In-Vivo Profiler Driver (`profiler`):**
   - Captures Pre-Resolution Safety Snapshot.
   - Injects `oneship-profiler-crafter` and spawns Action-First Chat Overlay (`flag_resolve_profiler.md`).
   - Agent inspects all `[scripts]` in `rula.toml` and presents a Multi-Script Profiling Proposal binding drivers per script (`[scripts.<name>.profiler]`).
3. **Inline Tasks & Technical Debt (`todos` / `tech_debt`):**
   - Captures Pre-Resolution Safety Snapshot.
   - Action-First Chat Overlay scans code, stages items to SQLite ledgers, and applies developer cleanup policy (Purge vs Annotate vs Preserve).
4. **Operational Resolvers (`invalid_config` & `removal`) & Closed-Loop Resolution:**
   - `invalid_config`: Automatically raised when `rula.toml` fails validation (`sync_project_config_flag`), launching `config-resolver` (`templates/prompts/flag_resolve_config.md`) and closing automatically once `rula.toml` validates.
   - `removal`: Raised when project de-integration fails (`launch_project_removal_agent`), launching `removal-resolver` (`templates/prompts/flag_resolve_removal.md`), which concludes via `complete_project_removal`.
   - Standard bootstrapping flags (`docs`, `todos`, `tech_debt`, `profiler`) conclude by invoking `resolve_initialization_flag(flag_id, status)` (`"resolved"` | `"dismissed"`).
   - SQLite status updates and badges clear from the Project Card in real-time.

### 5.3 Ephemeral Initializer Skill Lifecycle
- During Phase 1 pre-boot, the ephemeral skill `oneship-initializer` is temporarily injected into `.agents/skills/oneship-initializer/`.
- `oneship-initializer` is purged via `cleanup_ephemeral_skill` by `complete_project_initialization` on success, or by the watcher when the initialization fails, leaving only permanent operational skills.

### 5.4 Objective Persona Creation (Zero Arbitrary Limits)
- Agent specialist personas under `[agents.<id>]` must be derived strictly from actual decoupled directory boundaries and subsystem domains (e.g. a single lead architect for simple scripts or compact utilities; core, frontend, and git only when distinct subsystems exist).
- Arbitrary numerical limits (such as forcing "2 to 4 specialists") are strictly prohibited.



