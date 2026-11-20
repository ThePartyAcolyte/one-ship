# Greenfield Project Creation & Conversational Bootstrapping

## Purpose

Define how ONE-SHIP creates new projects from scratch (**Greenfield**): validating target directories, initializing Git and platform assets deterministically, launching an interactive spec-first Lead Systems Architect initializer session over `stream-json`, selecting a documentation archetype (`oneship-docstyles`), configuring `rula.toml`, and retaining idle ephemeral initializer/resolver sessions across UI context switches.

## Scope

In scope:
- `AddProjectModal.vue` dual-path ingestion (`New Project` vs `Existing Project`), agent runner/model/effort selection, and `tracking_mode` choice.
- The `create_greenfield_project` daemon command (`kopeng/initializer.rs`), preconditions, and pre-boot asset injection.
- The Greenfield Initializer Agent conversational phases (`A`–`D`) and 7 behavioral invariants (`greenfield_initializer.md`).
- Ephemeral `Idle`/`Blocked` process visibility in `AgentTaskbar.vue` and non-destructive context switching in `dialogueStore.ts`.

Out of scope:
- Brownfield reconnaissance, `rula.toml` script execution, and project removal ([Project Registry, Scripts & Service Orchestration](01-project-registry-and-service-orchestration.md)).
- Remote repository cloning/linking ([Remote Repositories & Diagnostics](04-remote-repositories-and-diagnostics.md)).
- Global runner configuration schema ([Configuration Schema & Cascade](../01-architecture/02-configuration-schema-and-cascade.md)) and platform skill injection mechanics ([Platform Skill Injection](../01-architecture/03-platform-skill-injection.md)).

## Model / Design

### 1. Motivation: Brownfield vs. Greenfield

Brownfield onboarding scans an existing codebase, infers services from manifests (`package.json`, `Cargo.toml`), writes `rula.toml`, and emits bootstrap gap flags (`docs`, `profiler`, `todos`, `tech_debt`). On an empty or non-existent directory, emitting profiler or technical-debt flags before code exists violates platform invariants and creates noise. Greenfield bootstrapping therefore begins with a human-in-the-loop scoping and documentation-standard dialogue before any code or service definitions exist.

### 2. Bootstrapping Lifecycle

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                    GREENFIELD BOOTSTRAPPING LIFECYCLE                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  [ UI: + Add Project ] ──► [ AddProjectModal.vue ]                          │
│                                    │                                        │
│          ┌─────────────────────────┴─────────────────────────┐              │
│          ▼                                                   ▼              │
│   [ Existing Project ]                                [ New Project ]       │
│   (System Folder Picker)                         • Name: "telemetry-api"    │
│   • Brownfield Phase 1 Recon                     • Parent: "/home/dev"      │
│                                                  • [x] git init             │
│                                                  • Tracking / Runner / Model│
│                                                              │              │
│                                                              ▼              │
│                                                [ create_greenfield_project ]│
│                                                              │              │
│   ┌──────────────────────────────────────────────────────────┴──────────┐   │
│   │ 1. Create Target Directory: /home/dev/telemetry-api                 │   │
│   │ 2. Run git init (if initGit == true; failure aborts creation)       │   │
│   │ 3. Register in SQLite: add_project(path)                            │   │
│   │ 4. Inject Runner MCP Configs: .mcp.json, opencode.json              │   │
│   │ 5. Symlink Base Skills: .agents/skills/oneship-*/                   │   │
│   │ 6. Symlink Greenfield Advisory Skills: docstyles, archetypes        │   │
│   │ 7. Create Relative Symlink: CLAUDE.md -> AGENTS.md                  │   │
│   │ 8. Prepend AGENTS.md Governance Block                               │   │
│   └──────────────────────────┬──────────────────────────────────────────┘   │
│                              │                                              │
│                              ▼                                              │
│         [ initialize_project_agent(mode="greenfield") ]                     │
│                              │                                              │
│                              ▼                                              │
│         [ Native Supervisor: Interactive REPL (stream-json) ]                │
│                              │                                              │
│                              ▼                                              │
│      [ FloatingChatOverlay Auto-Open on Initializer Agent ]                 │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 3. Frontend Ingestion Modal (`AddProjectModal.vue`)

Built strictly with design-system inputs (`AppInput`, `AppSelect`, `AppToggle`):
- **Mode Toggle:** `New Project` (default) and `Existing Project`.
- **Greenfield Inputs:**
  - `projectName`: sanitized directory name.
  - `parentPath`: base directory with native folder picker (`@tauri-apps/plugin-dialog`).
  - Dynamic target path preview (`<parentPath>/<projectName>`).
  - `initGit`: boolean toggle for `git init`.
  - `enableRea`: optional Reverse Engineering Lab scaffold toggle.
  - `trackingMode` (`projectNature`): optional selector mapping `Personal / Emergent` → `linear`, `R&D / Exploratory` → `shape_up`, `Commercial / Client` → `weighted` (written to `[plugins.project_management].tracking_mode` in `rula.toml`).
  - `selectedRunner`, `selectedModel`, `selectedEffort` (`useAgentPairPicker`): options sourced exclusively from runner manifests (`list_agent_runners`), preselected from `get_configured_agent_pair` (invalid config surfaces as an error). Changing the runner clears an undeclared model; creation is refused until a valid declared pair is chosen.
- **Events:**
  - `@added` for brownfield selection.
  - `@created-greenfield` on `create_greenfield_project` success; `App.vue` selects the new active project in `coreStore` and opens `dialogueStore.openOverlay('initializer')`.

### 4. Daemon Preconditions & Pre-Boot (`create_greenfield_project`)

Defined in `src-tauri/src/kopeng/initializer.rs`:
0. `runner`, `model`, and `effort` must be supplied together or omitted together and must match a declared runner manifest pair before any directory or DB mutation occurs. When REA is enabled, the `reverser` persona uses that pair (or the default agent pair); the initializer writes all other personas strictly from the runner catalog injected into its prompt.
1. `name` must be non-empty and must not contain `/` or `\`.
2. `parent_path` must exist on disk.
3. If `parent_path.join(&name)` already exists, it must be empty; non-empty targets return an error.
4. When `init_git == true`, `kopeng::git::init_git_repo` runs `git init` only (failure aborts creation; `.gitignore` and `README.md` are created later by the agent once the stack is chosen).
5. `pre_boot` injects base platform skills, `AGENTS.md` governance block, runner MCP configs, `CLAUDE.md -> AGENTS.md` symlink, and greenfield advisory skills (`oneship-docstyles`, `oneship-archetypes`).
6. Launches `initialize_project_agent` with `InitMode::Greenfield` (`templates/prompts/greenfield_initializer.md`).

### 5. Greenfield Initializer Agent Protocol

#### Conversational Phases (`A`–`D`)

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                    CONVERSATIONAL BOOTSTRAPPING PHASES                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Phase A: Vision & Scoping Dialogue                                         │
│  • Lead Architect welcomes developer in Floating Chat Overlay.             │
│  • Explores concept, consumers, platform constraints, and performance goals.│
│                                                                             │
│  Phase B: Documentation Standard Selection (Spec-First)                     │
│  • Evaluates archetypes via oneship-docstyles / oneship-archetypes.         │
│  • Delivers interactive proposal via post_agent_response:                   │
│    Options: ["living-specs", "lean", "contract", "test-anchored", "custom"].│
│  • Injects chosen docstyle skill (oneship-docstyle-*).                      │
│  • Scaffolds living specifications (manifesto, lexicon, specs/) under docs/ │
│    with mandatory bi-directional code ◄► spec synchronization guarantee.    │
│                                                                             │
│  Phase C: Stack Definition & Codebase Scaffolding                           │
│  • Based on documented requirements, defines languages and frameworks.     │
│  • Scaffolds initial project skeleton.                                      │
│                                                                             │
│  Phase D: Configuration & Genesis Milestone                                 │
│  • Writes rula.toml declaring [project] id = "{{PROJECT_UID}}", verified    │
│    scripts, [plugins.project_management].tracking_mode, and specialist      │
│    personas.                                                                │
│  • Scaffolds initial project_milestones (with weight_percent and baseline   │
│    dates when tracking_mode = "weighted") before decomposing into tasks.    │
│  • Records Genesis Milestone via record_session_minutes.                    │
│  • Emits continuous profiler flag (raise_initialization_flag) only now.     │
│  • Transitions project to standard ONE-SHIP operational lifecycle.       │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Seven Behavioral Invariants

1. **Terminal silence & chat-first protocol:** Never write conversational dialogue or Markdown blocks to `stdout`; every message routes through `post_agent_response`.
2. **Chat-only first turn:** On Turn 1, `post_agent_response` MUST be the agent's first and only tool call. Scanning files or running shell tools is forbidden; introductions, roadmaps, ASCII diagrams, and opening questions go directly in `dialogue` with no artifact emitted.
3. **Strict sandbox isolation:** Bounded strictly to the target project directory; inspecting parent directories, host application sources, or global configs (`~/.config`, `~/.gemini`) is forbidden.
4. **Zero premature flags:** Never invoke `raise_initialization_flag` during Phase A or Phase B.
5. **Bounded specialist personas:** Persona prompts written into `rula.toml` must remain `<= 100` words without verification command directives or governance rule duplication.
6. **Unique project attribution (`PROJECT_UID`):** Pre-boot supplies `PROJECT_UID` (`proj-<uuid_v4>`, or preserves an existing valid `[project].id`). The agent writes `[project] id = "{{PROJECT_UID}}"` in `rula.toml`; `post_agent_response` routing derives project identity from the process-attested caller context rather than an explicit project parameter.
7. **Zero autonomous timeout & multi-turn retention:** No background watcher terminates `InitMode::Greenfield`. The session closes only when the initializer calls `complete_project_initialization`. A pre-existing REA lab `rula.toml` is extended by the agent and never closes initialization on its own.

### 6. Ephemeral Lifecycle & Context Switching Continuity

- **Persistent `Idle` state discovery:** When an ephemeral agent (`initializer` or flag resolvers `docs-resolver`, `todos-resolver`, `techdebt-resolver`, `profiler-crafter`) enters `ProcessStatus::Idle` or `ProcessStatus::Blocked` awaiting user input, `compute_project_state` retains it under `matches!(status, ProcessStatus::Working | ProcessStatus::Starting | ProcessStatus::Idle | ProcessStatus::Blocked)`.
- **Taskbar multi-turn visibility:** `AgentTaskbar.vue` monitors ephemeral processes via `isAgentProcessActive` without hiding `Idle` initializers/resolvers.
- **Non-destructive context switching:** `dialogueStore.switchProjectContext` preserves active ephemeral and resolution chat overlays when the target project path matches the active session.

## Contracts

### Daemon Command (`api.rs` & `kopeng/initializer.rs`)

```rust
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CreateGreenfieldProjectArgs {
    pub name: String,
    pub parent_path: String,
    pub init_git: bool,
    pub enable_rea: bool,
    pub tracking_mode: Option<String>,
    pub runner: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
}
```

- Command: `create_greenfield_project(args: CreateGreenfieldProjectArgs) -> Result<HydratedProject, InitializerError>`
- Related agent/UI commands: `initialize_project_agent(mode: InitMode)`, `list_agent_runners`, `get_configured_agent_pair`, `complete_project_initialization` (MCP), `raise_initialization_flag` (MCP), `record_session_minutes` (MCP).

## Implementation Status

| Area | Status | Notes |
| :--- | :--- | :--- |
| `create_greenfield_project`, `InitMode::Greenfield`, `pre_boot`, `tracking_mode` scaffolding (`kopeng/initializer.rs`) | Implemented | — |
| `AddProjectModal.vue` New/Existing toggle, `useAgentPairPicker`, `trackingMode`, `@created-greenfield` | Implemented | — |
| `templates/prompts/greenfield_initializer.md`, `oneship-docstyles`, `oneship-archetypes` (superseded alias), `oneship-docstyles-crafter` | Implemented | — |
| Ephemeral `Idle`/`Blocked` retention & taskbar/overlay continuity (`supervisor.rs`, `AgentTaskbar.vue`, `dialogueStore.ts`) | Implemented | — |

## Related Specs

- [Project Registry, Scripts & Service Orchestration](01-project-registry-and-service-orchestration.md)
- [Git Workspace, Branches & Snapshots Cockpit](02-git-workspace-and-branches.md)
- [Remote Repositories & Diagnostics](04-remote-repositories-and-diagnostics.md)
- [Project Management & Milestones](07-project-management-and-milestones.md)
- [Live Artifacts & Scratchpad](08-live-artifacts-and-scratchpad.md)
- [Project Category Index](README.md)

Supersedes legacy: `26_greenfield_project_creation_and_conversational_bootstrapping.md`
