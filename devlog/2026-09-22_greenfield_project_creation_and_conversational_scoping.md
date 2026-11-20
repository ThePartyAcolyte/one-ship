# Development Log: 2026-09-22 - Greenfield Project Creation & Conversational Bootstrapping

## 1. Context & Motivation
Previously, adding a project in Wang-Kapawu strictly assumed that an existing codebase was already present on disk. Clicking `+ Add Project` triggered a native directory selection dialog that directly added the folder to SQLite and launched the brownfield Phase 1 reconnaissance agent.

For greenfield projects (projects starting from scratch), this brownfield flow broke down:
- Users could not create and configure a brand-new directory directly through the application.
- Emitting profiler, task, or technical debt flags in an empty directory caused noisy friction and violated system invariants.
- Bootstrapping a project from scratch requires an outside-in, human-in-the-loop conversation to explore domain scope, choose an architectural documentation standard (`docs/`), define the stack, and craft `rula.toml` incrementally.

## 2. Architecture & Changes Implemented

### 2.1. Dual-Path Modal (`src/components/modals/AddProjectModal.vue`)
- Created a brutalist modal with tabs for `New Project` and `Existing Project`.
- `Existing Project`: Preserves the legacy brownfield flow (calls OS directory picker and registers project in SQLite).
- `New Project`:
  - Input for `projectName` (sanitized project and directory name).
  - Input with browse button for `parentPath` (workspace root directory).
  - Real-time calculated preview of target directory path.
  - `initGit` toggle (`git init` on project root).
  - Runner, model, and effort selection via dynamic schema loading (`get_agent_runner_schema`).
  - Calls Tauri command `create_greenfield_project`.

### 2.2. Backend Command & Pre-Boot (`src-tauri/src/kopeng/initializer.rs`)
- Implemented `create_greenfield_project`:
  - Validates input strings (no path separators in name).
  - Validates parent directory and ensures target directory does not conflict with existing files.
  - Creates directory on disk with `fs::create_dir_all`.
  - Executes `init_git_repo` when `initGit` is enabled.
  - Registers the new project in SQLite (`crate::registry::add_project`).
  - Invokes `initialize_project_agent` with `mode = "greenfield"`.
- Enhanced `initialize_project_agent`:
  - Injects `kapawu-docstyles` and `kapawu-archetypes` during pre-boot when `mode == "greenfield"`.
  - Selects `greenfield_initializer` prompt template.
  - Formulates first-turn directive instructing the agent to welcome the user as Lead Systems Architect and start conversational scoping without scanning or raising premature flags.
- Registered `create_greenfield_project` in `src-tauri/src/lib.rs`.

### 2.3. Specialized Greenfield Agent Prompt (`templates/prompts/greenfield_initializer.md`)
- Established dedicated system instructions for the Greenfield Initializer:
  - Role: Lead Systems Architect.
  - Invariant: Zero premature initialization flags (`profiler`, `todos`, `tech_debt`).
  - Spec-First Methodology: Guides the developer through scoping, documentation standard selection (`sdd`, `lean`, `contract`, `custom`), living specifications scaffolding under `docs/`, and eventual stack definition and `rula.toml` creation.
  - Embedded template in `src-tauri/src/kopeng/skill.rs`.

### 2.4. Frontend Integration (`src/App.vue`)
- Integrated `AddProjectModal.vue` replacing the direct `open()` call in `addProject()`.
- Upon greenfield creation, automatically updates `coreStore.projects`, sets the new project as active, and opens `FloatingChatOverlay` focusing on `initializer`.

## 3. Verification & Compliance
- **Backend:** `cargo check --manifest-path src-tauri/Cargo.toml` compiled cleanly with 0 errors.
- **Frontend:** `vue-tsc --noEmit` and `vite build` completed cleanly with 0 type or compilation errors.
- **Governance:** Conforms to Tiger Style conventions, strict shell edit ban, zero terminal stdout in agent dialogue, and live documentation sync rules.
