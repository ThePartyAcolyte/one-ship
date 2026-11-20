# Initializer Agent Cleanup, Skill Synchronization & Scripts Separation

## 1. Context & Identified Gaps
- **Lingering Initializer Agent:** After completing Phase 1 initialization, `compute_project_state` in `supervisor.rs` checked `init_status != Unlaunched` which evaluated to true on `Completed { exit_code: 0 }`, causing `initializer` to stay permanently in the project card and dialogue agent list.
- **Missing Persistent Scripts Categorization:** The initializer placed all commands under one-shot `[scripts.task.<id>]` instead of declaring long-running dev servers (`uvicorn`, `npm run dev`, `docker compose up`) under `[scripts.persistent.<id>]`.
- **Git Agent Duplication:** Outdated examples in `rula_schema.md` caused the model to create `[agents.git]` in `rula.toml`, duplicating the platform's internal git agent.
- **Skill Versioning Mismatch:** Platform skill templates lacked `version: 0.2.4` in their frontmatter, causing the UI to flag skills as outdated.

---

## 2. Changes Executed

1. **Native Process Supervisor (`src-tauri/src/supervisor.rs`):**
   - Corrected `should_inject_initializer`: Initializer is only injected if `!rula_exists` or if the initialization process is actively running (`Working` or `Starting`). Once done, it is immediately removed from `state.agents`.

2. **MCP Tool Project Resolution (`src-tauri/src/mcp.rs`):**
   - In `execute_raise_initialization_flag`, added automatic fallback to `find_project_for_agent(agent_id)` if `project` argument is omitted or relative.

3. **Prompt & Skill Templates Alignment:**
   - In `templates/prompts/project_initializer.md` & `templates/skills/kapawu-initializer/SKILL.md`:
     * Strictly mandated separating dev servers into `[scripts.persistent.<id>]` and one-shot commands into `[scripts.task.<id>]`.
     * Explicitly prohibited creating `[agents.git]`.
     * Required calling `raise_initialization_flag` during reconnaissance for detected gaps (`docs`, `profiler`, `todos`, `tech_debt`).
   - In `templates/skills/kapawu-core/references/rula_schema.md`:
     * Removed `[agents.git]` example and noted that Git is an internal platform agent.
   - Added `version: 0.2.4` to all platform skill templates (`kapawu-core`, `kapawu-delegation`, `kapawu-devlog`, `kapawu-snapshots`, `kapawu-techdebt`, `kapawu-artifacts`).

4. **UI Toolbar Modernization (`src/components/ProjectCard/CardHeader.vue`):**
   - Modernized tooltips to "Platform Orchestration Assets (v0.2.4)".

---

## 3. Verification & Results
- `cargo check --bins`: Succeeded (0 errors).
- `cargo test --lib`: All 23 tests passed.
- `npm run build`: Succeeded (0 TypeScript/bundle errors).
