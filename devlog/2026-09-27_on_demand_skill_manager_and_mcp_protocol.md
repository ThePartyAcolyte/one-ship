# Devlog: 2026-09-27 — On-Demand Skill Manager & MCP Protocol

## Context & Objectives
- Solved context window bloat and established an On-Demand / Just-in-Time (JIT) skill architecture for Wang-Kapawu.
- Avoided unnecessary boilerplate stack skills during project onboarding, keeping LLM prompts lean and clean.
- Provided universal runner isolation: disabled skills are stored in `.agents/.skills-disabled/` so any runner scanning `.agents/skills/` (e.g. `agy`, `claude`, `opencode`) automatically ignores them without residual token leakage.
- Equipped autonomous agents with native MCP tools (`search_project_skills`, `manage_project_skill`) to discover and install community skills from SkillsMP / GitHub safely within the project sandbox.
- Created `SkillManagerModal.vue` for manual developer control (toggles, token weights, live search, 1-click install).

## Key Changes
1. **Backend Engine (`src-tauri/src/kopeng/skill.rs`):**
   - Implemented `list_project_skills`, scanning active (`.agents/skills`) and disabled (`.agents/.skills-disabled`) skills.
   - Added `toggle_project_skill` moving skill directories between active and disabled states.
   - Added `remove_project_skill` protecting platform base skills from deletion.
   - Added `search_skillsmp_registry` querying the public SkillsMP REST API via async curl.
   - Added `install_project_skill` downloading and validating raw `SKILL.md` into `.agents/skills/<skill-name>/`.
   - Registered Tauri commands: `get_project_skills`, `toggle_project_skill_cmd`, `remove_project_skill_cmd`, `get_project_skill_content_cmd`, `search_marketplace_skills`, `install_project_skill_cmd`.
2. **MCP Tool Suite (`src-tauri/src/mcp.rs`):**
   - Registered tool schemas and async execution handlers for `search_project_skills` and `manage_project_skill`.
3. **Platform Skill Guidelines (`templates/skills/kapawu-core/SKILL.md`):**
   - Instructed agents to query `search_project_skills` and use `manage_project_skill` when working with specialized, niche, or newly introduced libraries or APIs.
4. **Living Specification (`docs/specs/29_on_demand_skill_manager_and_mcp_protocol.md`):**
   - Documented the JIT skill philosophy, runner isolation directory structure, and MCP tool schemas.
5. **Frontend UI (`src/components/modals/SkillManagerModal.vue`):**
   - Brutalist dual-tab modal: Installed Skills (with ON/OFF toggle, description token estimation, inspection, deletion) and Explore Marketplace (with SkillsMP live search, quick suggestion tags, 1-click install).
   - Wired the `BrainCircuit` action button in `CardHeader.vue` and `App.vue` to open `SkillManagerModal` for initialized projects.

## Verification
- `cargo check`: Passed with 0 errors.
- `npm run build` (`vue-tsc && vite build`): Passed with 0 errors.

## Post-Session Bugfix: Orphaned Test Project Cleanup
- Root Cause: `test_init_flags_crud_with_options` in `src-tauri/src/db.rs` was calling `db_add_project("/tmp/test_init_flag_proj", "test_init_flag_proj")` against the live SQLite DB without cleanup, causing the test project to persistently reappear whenever tests or IDE background analyzers ran.
- Resolution: Deleted the orphaned row (`id=6`) and associated flags from `~/.config/one-ship/oneship.db`.
- Prevention: Implemented `TestProjectGuard` with RAII `Drop` invoking `db_remove_project(self.0)`, ensuring tests always clean up their temporary project records on exit or panic.

## Workspace Tab Integration (`ProjectSkillsTab.vue`)
- Created dedicated workspace tab component [`src/components/workspace/ProjectSkillsTab.vue`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/workspace/ProjectSkillsTab.vue) alongside Overview, Files & Docs, Devlog, Git, Terminals, and Profiling.
- Sub-tabs:
  - **Installed Skills:** Lists active and disabled skills, toggle switches, estimated prompt tokens, markdown drawer inspection, and safe removal (with base-platform protection).
  - **Explore Marketplace:** Real-time SkillsMP search, keyword filter chips, and 1-click installation directly into the project's `.agents/skills/`.
- Updated [`src/components/workspace/ProjectWorkspace.vue`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/workspace/ProjectWorkspace.vue) with tab navigation button `[BrainCircuit] Skills` and mounted stage component.
- Updated [`src/stores/dialogueStore.ts`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/stores/dialogueStore.ts) to manage `'skills'` in workspace tab state.
- Added global keyboard shortcut `Alt+8` and Tab switcher rotation support in [`src/App.vue`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/App.vue).

## Direct URL & Documentation Ingestion + Agent Persona Skill Scoping
- **Explore Tab Naming:** Renamed sub-tab to simply `EXPLORE` adhering to functional brutalist UX standards.
- **Direct URL & `llms.txt` Ingestion:**
  - Extended `install_project_skill` in [`src-tauri/src/kopeng/skill.rs`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/kopeng/skill.rs) to ingest from arbitrary URLs (documentation endpoints, `llms.txt`, raw GitHub files).
  - Automatically synthesizes YAML frontmatter when missing.
  - Applies progressive disclosure for large doc files (> 4KB), storing raw content in `references/documentation.txt` and generating a concise `SKILL.md`.
  - Added "Install from URL / Documentation" input panel in [`ProjectSkillsTab.vue`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/workspace/ProjectSkillsTab.vue).
- **Agent Persona Skill Scoping:**
  - Base platform skills (`kapawu-core`, `kapawu-delegation`, `kapawu-devlog`, `kapawu-snapshots`, `kapawu-techdebt`, `kapawu-artifacts`) are strictly universal and exempt from per-agent filtering/assignment.
  - Added `skills: Vec<String>` to `AgentPersonaInput` in [`src-tauri/src/kopeng/persona.rs`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/kopeng/persona.rs) and persisted to `rula.toml`.
  - Added domain skill prompt injection in `build_agent_initial_prompt` in [`src-tauri/src/kopeng/prompt.rs`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/kopeng/prompt.rs) and [`src-tauri/src/supervisor.rs`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/supervisor.rs).
  - Updated [`AddPersonaModal.vue`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/modals/AddPersonaModal.vue) with an *Assigned Project Skills* selector for custom on-demand skills.
  - Displayed persona assignment badges (`[@<agent-id>]`, `[UNIVERSAL (PLATFORM)]`, `[GENERAL (ALL AGENTS)]`) on skill cards in [`ProjectSkillsTab.vue`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/workspace/ProjectSkillsTab.vue).
