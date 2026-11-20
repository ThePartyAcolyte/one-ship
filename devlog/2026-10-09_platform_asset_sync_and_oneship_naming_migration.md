# Development Session: 2026-10-09 — Platform Asset Sync & oneship-* Naming Migration

## Context & Objectives
- **Target Problem 1 (Warning Loop in `l10n_cl_banking`):** The project card button showed a warning prompting to update from `v0.2.5` to `v0.2.5` because skills were symlinked but `rula.toml` retained legacy `kapawu_version = "0.2.4"`.
- **Target Problem 2 (Rule 7 Permissive Fallbacks Invariant Decoupling):** Clarified by user that Rule 7 (strict prohibition of permissive fallbacks) was strictly an internal platform development design rule for `one-ship/AGENTS.md`, and should NOT be imposed onto orchestrated client projects.
- **Target Problem 3 (Option A Execution — Unification to `oneship-*`):** Full migration of platform skills, schemas, and templates from legacy `kapawu-*` to `oneship-*` (`oneship-core`, `oneship-delegation`, `oneship-devlog`, `oneship-snapshots`, `oneship-techdebt`, `oneship-artifacts`, and specialized oneship skills), purging legacy Wang-Kapawu references.

---

## Key Changes & Implementations

### 1. Platform Skill Catalog Migration (`templates/skills/`)
- Created full suite of 14 `oneship-*` skills at version `0.2.5`:
  - `oneship-core` (Rule 6 removed, schema references `oneship_version = "0.2.5"`).
  - `oneship-delegation`
  - `oneship-devlog`
  - `oneship-snapshots`
  - `oneship-techdebt`
  - `oneship-artifacts`
  - `oneship-initializer`
  - `oneship-docstyles`
  - `oneship-docstyles-crafter`
  - `oneship-docstyle-sdd`
  - `oneship-docstyle-lean`
  - `oneship-docstyle-contract`
  - `oneship-archetypes`
  - `oneship-profiler-crafter` (purged legacy `.kapawu` path reference).

### 2. Backend Engine Refactoring (`src-tauri/`)
- **`src-tauri/src/kopeng/skill.rs`:**
  - Updated embedded skill constants to `ONESHIP_*` referencing `templates/skills/oneship-*/SKILL.md`.
  - Updated `PLATFORM_BASE_SKILLS` to `["oneship-core", "oneship-delegation", "oneship-devlog", "oneship-snapshots", "oneship-techdebt", "oneship-artifacts"]`.
  - Refactored `get_project_skill_status`:
    - Checks all 6 base skills presence and symlink status.
    - Evaluates both `oneship_version` and legacy `kapawu_version`. Marks `needs_migration = true` if legacy key is present.
    - Inspects `AGENTS.md` for legacy `<!-- WANG_KAPAWU_MANAGED_START -->` blocks.
    - Checks runner configs (`.mcp.json`, `opencode.json`, `CLAUDE.md -> AGENTS.md` symlink).
  - Refactored `inject_platform_skill`:
    - Cleans up legacy `kapawu-*` directories and symlinks from `.agents/skills/`.
    - Symlinks base `oneship-*` skills.
    - Preserves third-party / bespoke project skills and relinks existing specialized `oneship-*` skills.
    - Atomically refreshes `AGENTS.md` via `prepare_project_agents_md` (purging legacy blocks and writing modern ONE-SHIP governance without Rule 7).
    - Injects runner MCP configs (`.mcp.json`, `opencode.json`) and relative `CLAUDE.md` symlink (with backup of physical file if present).
    - Synchronizes `oneship_version = "0.2.5"` in `rula.toml`.
  - Refactored `inject_specialized_skill`: Supports `oneship-*` skills with backward-tolerant name normalization.
  - Refactored `cleanup_ephemeral_skill`: Purges `oneship-initializer` (and any legacy `kapawu-initializer`).
- **`src-tauri/src/kopeng/migration.rs`:**
  - `get_project_version` reads `oneship_version`, falling back to `kapawu_version`.
  - `set_project_version` writes `oneship_version = "..."` and removes legacy `kapawu_version`.
- **`src-tauri/src/kopeng/manifest.rs`:**
  - Removed Rule 7 from `ONE_SHIP_MANAGED_BLOCK`.
  - Updated `KAPAWU_PLATFORM_SKILLS` to include `oneship-*` for purge resolution.
- **`src-tauri/src/kopeng/flag_resolver.rs` & `initializer.rs`:**
  - Updated specialized skills and ephemeral initializer references to `oneship-*`.
- **`src-tauri/src/mcp.rs`:**
  - Updated ephemeral cleanup invocation to `oneship-initializer`.
- **`src-tauri/src/logger.rs`:**
  - Renamed background thread to `oneship-sqlite-logger`.

### 3. Frontend & Prompt Harmonization
- **`src/components/modals/DocstyleSelectionModal.vue`:** Updated skill names to `oneship-docstyle-*`.
- **`src/components/ProjectCard/CardHeader.vue`:** Refined `skillTooltip` to distinguish `Upgrade Configuration Schema (v0.2.4 -> v0.2.5)` from asset symlink sync, eliminating the confusing `v0.2.5 -> v0.2.5` notice.
- **`templates/prompts/`:** Updated `project_initializer.md` and `greenfield_initializer.md` to reference `oneship-core` schema, `oneship-docstyles`, and `oneship_version`.

### 4. Living Specifications Synchronization
- Updated `docs/specs/24_project_versioning_and_migration_engine.md`.
- Updated `docs/specs/26_greenfield_project_creation_and_conversational_bootstrapping.md`.
- Updated `docs/specs/29_on_demand_skill_manager_and_mcp_protocol.md`.
- Updated `docs/specs/25_universal_telemetry_and_profiling_engine.md`.
- Updated `docs/01_ubiquitous_language.md`.
- Updated `AGENTS.md` Section 7.

---

## Verification
- `cargo check`: Passed with 0 errors.
- `npm run build` (`vue-tsc --noEmit && vite build`): Built in 4.36s with 0 errors.
