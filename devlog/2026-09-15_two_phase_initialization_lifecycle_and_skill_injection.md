# Devlog: Two-Phase Project Lifecycle Protocol & Programmatic Skill Injection

**Date:** 2026-09-15  
**Topic:** Implementation of the Strict Two-Phase Project Initialization Protocol, Separation of Base vs. Specialized Skills, Execution Directory Isolation, and Resolution of Initializer Silent Hangs  

---

## 1. Context & Architecture Review

During project initialization testing, two critical operational issues were observed:
1. **Execution Directory Misconfiguration**: When launching in Supervised mode, `ProjectInitModal.vue` branched into `launch_system_agent_cmd` (running in `~/Projects/dev`), causing an uninitialized agent to execute in an external workspace with zero context or project skills.
2. **Missing Pre-Boot Sequence & Silent Tool Failures**: The agent ran before MCP configuration and platform skills were programmatically injected into the target project workspace. Furthermore, if a tool call encountered a failure, the agent remained stuck indefinitely without emitting dialogue or error notices.
3. **Spec Discrepancies**: Specs contained legacy references to `semi_attended` mode, arbitrary specialist count heuristics ("2 to 4 specialists"), and conflated the Global System Agent (`system`) with the project-scoped Initializer agent (`initializer`).

---

## 2. Implemented Architecture & Lifecycle

### Phase 1: Deterministic Pre-Boot & Non-Invasive Reconnaissance
- **Programmatic Pre-Boot (`kopeng/skill.rs`)**:
  - Automatically writes `.agents/plugins/wang-kapawu/mcp_config.json` inside `<project_path>`.
  - Automatically copies **Base Platform Skills** (`kapawu-core`, `kapawu-delegation`, `kapawu-devlog`, `kapawu-snapshots`, `kapawu-techdebt`, `kapawu-artifacts`) from canonical `templates/skills/` into `<project_path>/.agents/skills/`.
  - Prepends the reversible governance block to `<project_path>/AGENTS.md`.
  - Enforces mandatory `auto_approve: true` in initializer agent execution args.
- **Execution Boundary**:
  - Initializer Agent runs strictly inside `<project_path>` within the project's Herder workspace.
  - Source code is strictly **read-only** (zero direct edits or file mutations).
- **Flag Emission**:
  - Analyzes stack, scripts, services, and directory boundaries to generate `rula.toml` with bounded specialist personas (≤ 100 words per prompt, zero arbitrary numerical limits).
  - Emits `raise_initialization_flag` for detected bootstrapping gaps (`docs`, `todos`, `tech_debt`, `profiler`).

### Phase 2: On-Demand Flag Resolution (Subsequent Independent Sessions)
- **Brutalist Badges (`AppInitBadge.vue`)**:
  - Renders on `ProjectCard.vue` with status badges: `[! INIT: DOCS]`, `[! INIT: TODOS]`, `[! INIT: TECH_DEBT]`, `[! INIT: NO PROFILER]`.
- **On-Demand Specialized Skill Injection**:
  - Specialized skills (`kapawu-docstyles`, `kapawu-profiler-crafter`) reside exclusively in `templates/skills/` and are copied to `<project_path>/.agents/skills/` on demand via `inject_specialized_skill` only when the user clicks a badge to resolve that specific gap.
- **Resolution Closure**:
  - Agent scaffolds documentation or crafts profiler manifests and invokes `resolve_initialization_flag`, removing the badge from the UI and updating SQLite.

---

## 3. Key Changes & File Modifications

1. **Governance & Living Documentation**:
   - `AGENTS.md`: Enforced Section 7 "Project Onboarding & Two-Phase Lifecycle Protocol".
   - `docs/01_ubiquitous_language.md`: Updated definitions for Docstyles, Initialization Flags, and Base vs. Specialized Skills.
   - `docs/specs/08_herder_agent_orchestration.md`: Added Section 8.9 (programmatic pre-boot sequence), eliminated `semi_attended` mode, removed arbitrary specialist count rules.
   - `docs/specs/24_project_versioning_and_migration_engine.md`: Disambiguated `system` agent (anchored to Wang-Kapawu root) from `initializer` (anchored inside `project_path`).
   - `docs/specs/25_universal_telemetry_and_profiling_engine.md`: Synchronized schema documentation with `project_initialization_flags` and `raise_initialization_flag`.
   - Synchronized specs `01`, `10`, `12`, `13`, `17`, `18`, `20`, `22` to version `0.2.4`.

2. **Backend Engine (`src-tauri/`)**:
   - `src-tauri/src/kopeng/skill.rs`:
     - Isolated `inject_platform_skill` for Base skills only.
     - Implemented `inject_specialized_skill(app, project_path, skill_name)` for Phase 2 on-demand injection.
     - Enforced programmatic pre-boot sequence before launching `initialize_project_agent`.
   - `src-tauri/src/kopeng/herdr.rs`:
     - Added compliance monitoring / error wakeup in `spawn_herdr_watcher` for all dialogue agents, `initializer`, and `system`.
   - `src-tauri/src/lib.rs`: Registered `inject_specialized_skill` command.

3. **Frontend Architecture (`src/`)**:
   - `src/components/ui/AppInitBadge.vue`: Created standardized brutalist badge component with `@lucide/vue` icons.
   - `src/components/modals/ProjectInitModal.vue`: Removed branching to global `system` agent in `~/Projects/dev`. Unified startup through `initialize_project_agent` in `<project_path>`, opening the Chat Overlay for `'initializer'` in Supervised mode.
   - `src/components/ProjectCard/ProjectCard.vue`: Added dynamic initialization flag badges, Phase 2 on-demand skill injection trigger, and English prompt generation. Cleaned unused imports (`Play`, `XCircle`).

---

## 4. Verification & Build Integrity

- **Frontend Compilation (`vue-tsc --noEmit && vite build`)**:
  - Exited with code 0 in 4.03s (`dist/` generated cleanly, 0 TypeScript errors).
- **Backend Rust Compilation (`cargo check`)**:
  - Exited with code 0 (0 errors, 0 warnings).
- **Governance Audit**:
  - Zero git commands executed.
  - File changes executed strictly through dedicated editing tools.
  - Zero mock logic introduced.

---

## 5. Technical Debt & Tokio Migration Registration

Following user architectural direction regarding the upcoming migration from Herder to native Tokio subprocess management, Herder-specific workarounds for the initialization flow were shelved in favor of general architectural requirements. The problem space and requirements were formalized as **Technical Debt Item 20** in `TECHNICAL_DEBT.md`:
- Decoupled Tokio process supervisor operating inside `<project_path>`.
- Deterministic pre-boot file injection before runner launch.
- First-class transient persona state for `initializer` prior to `rula.toml` existence.
- Unified stdout/stderr event streaming and idle error recovery.

