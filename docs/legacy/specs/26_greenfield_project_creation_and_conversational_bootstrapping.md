> [!WARNING]
> **LEGACY SPECIFICATION:** Superseded by [03-projects/10-greenfield-project-creation.md](../../specs/03-projects/10-greenfield-project-creation.md). Preserved for historical provenance only.

# Specification 26: Greenfield Project Creation & Conversational Bootstrapping

---

## 1. Overview & Objectives

### 1.1 Context
In earlier releases of ONE-SHIP, project registration was strictly brownfield: users selected an existing directory on disk containing established code. The system immediately launched non-invasive stack reconnaissance, inferred runtime services (`package.json`, `Cargo.toml`, etc.), crafted `rula.toml`, and emitted bootstrapping gap flags (`docs`, `profiler`, `todos`, `tech_debt`).

When starting a project from scratch (**Greenfield**):
- The repository directory does not exist or is empty.
- Architecture, domain scope, and tech stacks are undetermined.
- Emitting profiler, task, or technical debt flags prematurely violates system invariants and causes noisy friction.
- Bootstrapping naturally begins with human-in-the-loop vision, domain scoping, and spec-first documentation archetype selection.

### 1.2 Core Objectives
1. **Dual-Path Project Ingestion (`AddProjectModal`):** Provide an explicit choice between importing an *Existing Project* (brownfield scan) and creating a *New Project* (greenfield bootstrapping).
2. **Deterministic Greenfield Pre-Boot:** Safely validate directory targets, initialize VCS (`git init` only; a failure stops the creation, and `.gitignore` / `README.md` are written by the initializer agent once the stack is agreed), register project in SQLite, and symlink foundational platform assets (`AGENTS.md`, runner MCP configs `.mcp.json` / `opencode.json`, `CLAUDE.md -> AGENTS.md`, Base Platform Skills, `oneship-docstyles`, and `oneship-archetypes`).
3. **Spec-First Conversational Initializer:** Dedicated greenfield prompt (`greenfield_initializer.md`) positioning the agent as Lead Systems Architect, guiding the developer through scoping, documentation standard selection, living specs creation, tech stack definition, and `rula.toml` configuration.
4. **Zero Premature Flag Emission:** Strict invariant preventing `raise_initialization_flag` calls for profilers or code debt until persistent services and application code actually exist.

---

## 2. Architecture & Data Flow

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
│                                                  • Runner / Model / Effort  │
│                                                              │              │
│                                                              ▼              │
│                                                [ create_greenfield_project ]│
│                                                              │              │
│   ┌──────────────────────────────────────────────────────────┴──────────┐   │
│   │ 1. Create Target Directory: /home/dev/telemetry-api                 │   │
│   │ 2. Run git init                                                     │   │
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

---

## 3. Detailed Component Specifications

### 3.1. Frontend: Add Project Modal (`src/components/modals/AddProjectModal.vue`)
- **Brutalist Design & Component Standards:** Built strictly with `AppInput`, `AppSelect`, and `AppToggle`.
- **Mode Toggle:** Two primary tabs: `New Project` (default) and `Existing Project`.
- **Inputs & Path Resolution:**
  - `projectName`: Sanitized directory name.
  - `parentPath`: Base directory, with a `Browse` button triggering native `@tauri-apps/plugin-dialog`.
  - Dynamic preview of target directory: `<parentPath>/<projectName>`.
  - `initGit`: Boolean toggle for automated repository initialization.
  - `projectNature` / `trackingMode`: Optional selector (`Personal / Emergent` → `linear`, `R&D / Exploratory` → `shape_up`, `Commercial / Client` → `weighted`), persisted in `rula.toml` under `[plugins.project_management].tracking_mode`.
  - `selectedRunner`, `selectedModel`, `selectedEffort`: the runner/model/effort pair of the initializer (`useAgentPairPicker`). Options come only from the runner manifests (`list_agent_runners`); the initializer pair of the configuration is preselected (daemon command `get_configured_agent_pair`, an invalid configuration is shown as an error). A runner change clears a model it does not declare, and creation is refused until the pair is declared (Spec 10 §1.2).
- **Event Lifecycle:**
  - Emits `@added` for brownfield selections.
  - Emits `@created-greenfield` upon successful completion of `create_greenfield_project`.
  - `src/App.vue` immediately selects the new active project in `coreStore` and executes `dialogueStore.openOverlay('initializer')`.

### 3.2. Daemon Command (`create_greenfield_project`)
Defined in `src-tauri/src/kopeng/initializer.rs` and registered in `api::API_COMMANDS` (Spec 12 §7.2); the frontend calls it with `daemonCall`:

```rust
pub async fn create_greenfield_project(
    name: String,
    parent_path: String,
    init_git: bool,
    enable_rea: bool,
    runner: Option<String>,
    model: Option<String>,
    effort: Option<String>,
) -> Result<crate::hydration::HydratedProject, InitializerError>
```

**Preconditions & Invariants:**
0. `runner` / `model` / `effort` are given together or not at all and must be a pair of the runner's manifest; they are validated before anything is created. The REA lab's `reverser` persona is written with that pair (else the default agent pair), and the initializer writes every other persona's pair only from the runner catalog injected in its prompt.
1. `name` must be non-empty and must not contain `/` or `\`.
2. `parent_path` must exist on disk.
3. If `parent_path.join(&name)` exists, it must be empty; otherwise, returns an error.
4. If `init_git == true`, calls `crate::kopeng::git::init_git_repo`.
5. Pre-boot sequence injects `oneship-docstyles` and `oneship-archetypes` in addition to Base Platform Skills.
6. Invokes `initialize_project_agent` with `mode: "greenfield"`.

---

## 4. Greenfield Initializer Agent Protocol

### 4.1. Conversational Phasing & Lifecycle

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
│  • Evaluates archetypes via oneship-archetypes knowledge.                   │
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

### 4.2. Invariant Rules for Greenfield Initializer
1. **Terminal Silence & Chat-First Protocol:** Must not write conversational dialogue or raw Markdown blocks to `stdout`. All responses must route through native MCP tool `post_agent_response`.
2. **Chat-Only First Turn (Zero Tools & Zero Artifacts):** On Turn 1, the agent must make `post_agent_response` its first and only action. It is strictly forbidden from scanning files or running shell tools. All introductions, roadmaps, ASCII workflow diagrams, and opening questions MUST be delivered directly in `dialogue`. No artifact is emitted on Turn 1.
3. **Strict Sandbox Isolation:** The agent is strictly bounded to the target project directory. It is forbidden from inspecting parent directories, host application sources, or global configs (`~/.config`, `~/.gemini`).
4. **Zero Premature Flags:** Never call `raise_initialization_flag` during Phase A or B.
5. **Bounded Specialist Personas:** Persona prompts in `rula.toml` must remain strictly $\le 100$ words, without verification command directives or governance rule duplication.
6. **Unique Project Attribution (`PROJECT_UID`):** Pre-boot generates and injects a deterministic `PROJECT_UID` (`proj-<uuid_v4>`). The agent MUST write `[project] id = "{{PROJECT_UID}}"` in `rula.toml`. Routing of `post_agent_response` does not depend on it: the project is derived from the process-attested caller identity (spec 12 §6), so the agent supplies no project parameter.
7. **Zero Autonomous Timeout & Multi-Turn Retention:** Greenfield initialization is strictly an interactive pair-architect session. No background watcher is started for `InitMode::Greenfield`; the session ends when the initializer calls `complete_project_initialization` (Spec 12). A pre-existing `rula.toml` (REA lab scaffold) is extended by the agent and never closes the initialization by itself. Ephemeral agents waiting for user input (`ProcessStatus::Idle` or `ProcessStatus::Blocked`) MUST remain visible in the taskbar (`AgentTaskbar.vue`), retained in `compute_project_state`, and preserved during project context switches in `dialogueStore.ts`.

---

## 5. Ephemeral Lifecycle & Context Switching Continuity

1. **Persistent `Idle` State Discovery:** When an ephemeral agent (such as `initializer` or flag resolvers `docs-resolver`, `todos-resolver`, `techdebt-resolver`, `profiler-crafter`) completes a turn and enters `Idle`, the Process Supervisor retains the agent entry in `compute_project_state` under `matches!(status, ProcessStatus::Working | ProcessStatus::Starting | ProcessStatus::Idle | ProcessStatus::Blocked)`.
2. **Taskbar Multi-Turn Visibility:** `AgentTaskbar.vue` monitors ephemeral agent processes using `isAgentProcessActive`. Resolvers and initializers are not masked or collapsed when transitioning to `Idle`, ensuring developer controls and status badges remain reachable.
3. **Non-Destructive Project Context Switching:** Switching between projects via `dialogueStore.switchProjectContext` preserves active ephemeral and resolution agent chat overlays if the target project path corresponds to the active session, preventing unintended floating chat termination.


