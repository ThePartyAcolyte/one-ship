# Devlog: Symlink Architecture Migration & Project Initialization UI Decoupling

**Date:** 2026-10-08  
**Scope:** Architecture & UX decoupling for platform skills and project initialization.

---

## 1. Context & Motivation
Previously, clicking the `<BrainCircuit>` icon button in `CardHeader.vue` triggered `injectSkill`, which opened `ProjectInitModal.vue` (`projectToInit.value = project`). This conflated two distinct responsibilities:
1. **Initial Project Bootstrapping / Initialization Wizard:** Setting up runners, models, and running initial reconnaissance (`rula.toml`, Genesis Milestone, initialization flags).
2. **Platform Asset & Skill Updates:** Keeping base skills and MCP configs synchronized with the platform.

Furthermore:
- Base platform skills were physically copied into each project's `.agents/skills/` directory. Whenever templates were updated in `one-ship`, linked projects did not receive updates automatically.
- Claude Code runner discovery required `.mcp.json` and `CLAUDE.md`, which was being copied as a static text file rather than a relative symlink to `AGENTS.md`.
- `~/.gemini/antigravity-cli/mcp/one-ship/instructions.md` was originally created to silence 404 errors but contained strict agent behavioral constraints that contaminated external standalone terminal sessions across the host machine.
- Obsolete files (`.agents/plugins/one-ship/plugin.json` and `.agents/plugins/one-ship/mcp_config.json`) remained in the codebase based on an obsolete assumption of per-project AntiGravity configuration.

---

## 2. Architectural Decisions & Implementation

### 2.1 UI Decoupling (Initialization vs. Skill Synchronization)
- **`CardHeader.vue`:**
  - Introduced `isProjectInitialized = computed(() => Boolean(props.project.has_config || props.project.skill_status?.is_installed || props.project.init_state?.status === 'completed'))`.
  - Added dedicated `[+ INITIALIZE]` button directly next to `project.name` in the title row, shown strictly when `!isProjectInitialized`. Clicking emits `initProject`, opening `ProjectInitModal.vue`.
  - The `<BrainCircuit>` icon button in the toolbar is rendered **only** when `isProjectInitialized`. It emits `syncSkills`, directly invoking `syncPlatformAssets` (`inject_platform_skill`) without launching the initialization modal.
  - Hover tooltip indicates symlink status: `Platform Assets Synced & Symlinked` or `Migrate Platform Assets to Symlinks (v... -> v...)`.
- **Event Propagation:** Updated `ProjectCard.vue`, `ProjectOverviewTab.vue`, `ProjectWorkspace.vue`, and `App.vue` to cleanly separate `initProject` and `syncSkills`.

### 2.2 Symlink Architecture Migration
- **Canonical Template Resolution:** Added `resolve_canonical_templates_dir()` in `src-tauri/src/kopeng/skill.rs` searching known disk locations and executable ancestors.
- **Directory Symlinks:** Added `link_platform_directory(source_dir, target_link)`. Base platform skills (`kapawu-core`, `kapawu-delegation`, `kapawu-devlog`, `kapawu-snapshots`, `kapawu-techdebt`, `kapawu-artifacts`) and specialized skills are symlinked directly to canonical templates.
- **Relative Symlink `CLAUDE.md -> AGENTS.md`:** Claude Code reads governance rules directly without text file duplication.
- **Symlink Detection in `SkillStatus`:** Added `pub is_symlinked: bool` to `SkillStatus`. If `!is_symlinked` or `.mcp.json` is missing on an installed project, `is_outdated = true` triggers visual notification on the `<BrainCircuit>` button.

### 2.3 Runner MCP Discovery & Legacy Purge
- Injected `.mcp.json` for Claude Code (`http://localhost:{port}/mcp`) and `opencode.json` for OpenCode (`remote`).
- Purged dead code generating `.agents/plugins/one-ship/` and `.agents/mcp_config.json`.
- Neutralized `~/.gemini/antigravity-cli/mcp/one-ship/instructions.md` into a minimal technical server descriptor.

---

## 3. Verification & Compliance
- `cargo check`: Passed with 0 errors.
- `npm run build` (`vue-tsc --noEmit && vite build`): Passed with 0 errors.
- Strict Git & Shell Editing Ban: Followed without exception.
