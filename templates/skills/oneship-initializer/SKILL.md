---
name: oneship-initializer
description: Ephemeral project onboarding skill for Phase 1 reconnaissance and flag emission.
version: 0.2.5
---

# Ephemeral Project Initializer Protocol

This skill is injected temporarily into `.agents/skills/oneship-initializer/` strictly during Phase 1 project onboarding and is removed upon genesis milestone completion.

## 1. Responsibilities
1. **Non-Invasive Reconnaissance:**
   - Source code is strictly read-only.
   - Detect project services, build tools, package managers, and directories.
   - `.gitignore` / `README.md` are never written by ONE-SHIP and are never replaced wholesale. Existing files are the starting point: propose only the minimal additions the detected stack needs (brownfield: applied after the developer approves; Unattended mode leaves them untouched). In greenfield, create them once the stack is agreed.
2. **Configuration Synthesis (`rula.toml` via `oneship-core` schema):**
   - Strictly follow `.agents/skills/oneship-core/references/rula_schema.md`:
     - **Persistent Services (`[scripts.persistent.<id>]`):** For all long-running processes (dev servers e.g. `uvicorn`, `npm run dev`, `vite`, watchers, `docker compose up`), setting `cwd` to the subsystem directory when not at root.
     - **Deterministic Tasks (`[scripts.task.<id>]`):** For all one-shot execution commands (`pytest`, `npm run build`, `npm run lint`, `alembic upgrade`), setting `cwd` when scoped to a subdirectory.
     - **Subsystem Directory Overrides (`[plugins.<id>]`):** When embedded firmware (`platformio.ini`), EDA schematics (`.kicad_sch`), or reverse-engineering targets live in dedicated subdirectories instead of defaults, set `[plugins.electronics].firmware_dir`, `[plugins.electronics].schematics_dir`, or `[plugins.rea].targets_dir` in `rula.toml` (omit when defaults apply).
     - **NEVER create `[agents.git]`:** Git is an internal platform agent automatically managed by ONE-SHIP.
     - **Bounded Personas (`[agents.<name>]`):** Define specialist personas strictly based on decoupled directories, bounded to ≤ 100 words per prompt.
3. **Initialization Flags Emission (`raise_initialization_flag`):**
   - Mandatory: Call `raise_initialization_flag` for any gaps detected, passing `title: "..."` and `recommended_option: "..."` (your identity and project are attested by ONE-SHIP automatically):
     - `flag_type: "docs"`: If project lacks standard living documentation (e.g. `docs/specs/`, `00_manifesto.md`).
     - `flag_type: "profiler"`: If project lacks continuous in-vivo profiler configuration.
     - `flag_type: "todos"`: If codebase contains pending TODOs/FIXMEs.
     - `flag_type: "tech_debt"`: If architectural workarounds or mock layers are detected.
4. **Session Topic Registration (`switch_conversation_topic`):**
   - At the start of your first active turn (after performing initial reconnaissance), call `switch_conversation_topic` passing `topic: "..."` to register the topic of this initialization session (e.g. `"Phase 1 Brownfield Init — <stack> stack discovery"`). This enables the continuous chat history to display a meaningful separator label for this session.
5. **Genesis Milestone (`record_session_minutes`):**
   - Call `record_session_minutes` passing `title: "Genesis Milestone — Phase 1 Discovery"` and `summary: "..."` to record the project genesis milestone.
   - **Closure (`complete_project_initialization`):** the initialization ends only with this tool, called as your final action, once `rula.toml` is valid with `[project].id`, the milestone is recorded and the final `post_agent_response` is delivered. If it reports missing requirements, satisfy them and invoke it again. NEVER inspect or manipulate host files or databases directly. ONE-SHIP then removes this skill and stops the session.
6. **MCP Dialogue Communication:**
   - In Supervised mode, formulate the Phase 1 Technical Proposal and present it via `post_agent_response`.
   - Conversational explanation, architectural rationale, and next steps go in `dialogue` (supports rich Markdown: lists, formatting, ASCII diagrams). Never leave dialogue as an empty redirection stub.
   - The structured configuration and technical guidelines go in `artifact` (`rula.toml` and `AGENTS.md`). FORBIDDEN: Never put conversational questions or greetings in `artifact`.
   - In Greenfield mode, scoping happens strictly in `dialogue`; do not emit an artifact on turn 1.
   - Never print raw user-facing output to terminal stdout.
