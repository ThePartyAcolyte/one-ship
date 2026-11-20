# Specification 29: On-Demand Skill Manager & MCP Protocol

## 1. Context & Motivation

AI models powering agentic runners (e.g. Gemini, Claude, GPT, DeepSeek) already have extensive training on mainstream languages and popular frameworks (Vue 3, React, Tokio, Tailwind, FastAPI). Prematurely injecting generic framework skills into every project bloats the agent's context window, adds unnecessary token cost, and dilutes model focus.

Furthermore, managing global machine-level skills (`~/.agents/skills/`) introduces host environment coupling and potential instability across disparate repositories.

The **On-Demand Skill System** establishes a clean, project-scoped, just-in-time architecture where:
1. **Projects Start Clean (Zero Bloat):** Only core platform orchestrator skills (`oneship-core`, `oneship-delegation`, etc.) are injected during pre-boot.
2. **Just-in-Time (JIT) Skill Adoption:** Skills are incorporated only when facing specialized libraries, complex external APIs (e.g. Stripe, Meilisearch, WebRTC, custom SDKs), or proprietary runbooks.
3. **Autonomous Agent Capabilities (MCP):** Agents discover and install skills safely via dedicated MCP tools (`search_project_skills`, `manage_project_skill`) without executing unmanaged shell scripts or Git commands.
4. **Universal Runner Context Isolation:** Skills can be toggled ON or OFF. Inactive skills are stored in `.agents/.skills-disabled/` so any runner scanning `.agents/skills/` ignores them completely.

---

## 2. Skill Directory Layout

```text
<project-root>/
├── .agents/
│   ├── skills/                       # ACTIVE skills (scanned by all runners)
│   │   ├── oneship-core/
│   │   │   └── SKILL.md
│   │   └── stripe-api/
│   │       └── SKILL.md
│   └── .skills-disabled/             # INACTIVE skills (ignored by all runners)
│       └── meilisearch-patterns/
│           └── SKILL.md
```

---

## 3. Dedicated MCP Tool Suite

Agents managed by ONE-SHIP utilize two native MCP tools registered in `src-tauri/src/mcp.rs`:

### 3.1 `search_project_skills`
- **Description:** Searches the public agent skills registry (SkillsMP API) for specialized skills matching a query.
- **Parameters:**
  - `query` (string, required): Technology, library, or API name (e.g. `"stripe"`, `"pinia"`, `"webrtc"`).
  - `limit` (number, optional, default: 10): Maximum results to return.
- **Output:** Structured markdown list with name, author, stars, description, and source repository URL.

### 3.2 `manage_project_skill`
- **Description:** Lifecycle management of skills strictly within `<project>/.agents/skills/`. Calling identity and default project scope are resolved automatically from `CallerContext` (Spec 12 §6).
- **Parameters:**
  - `action` (string, required, enum: `"list"`, `"install"`, `"enable"`, `"disable"`, `"remove"`).
  - `project` (string, optional): Target project ID, name, or path (defaults to caller's project).
  - `skill_name` (string, required for install/enable/disable/remove): Skill identifier.
  - `source_url` (string, required for install): Repository URL or raw `SKILL.md` URL.
- **Safety Invariants:**
  - Base Platform Skills (`oneship-core`, `oneship-delegation`, `oneship-devlog`, `oneship-snapshots`, `oneship-techdebt`, `oneship-artifacts`) are protected from removal or disabling.
  - All file operations are confined strictly within the project sandbox.

---

## 4. Universal Runner Context Isolation

Any supported agent runner (`agy`, `claude`, `opencode`) that implements the open `SKILL.md` standard scans `.agents/skills/`.

## 4.1 Daemon Commands (`api.rs`, Spec 12 §7.2) & Specialized Skills
- **Daemon Commands:** `inject_platform_skill`, `get_project_skills`, `toggle_project_skill`, `remove_project_skill`, `get_project_skill_content`, `search_marketplace_skills`, `install_project_skill`.
- **Specialized Skills (`SPECIALIZED_SKILLS`):** Dynamically symlinked/cleaned via `inject_specialized_skill` / `cleanup_ephemeral_skill` for onboarding, docstyles, profiler crafting, and domain plugins (`oneship-rea`, `oneship-desigma`, `oneship-pcbcraft`, `oneship-project-mgmt`).

When a skill is disabled via UI or MCP:
- The skill folder is atomically moved to `.agents/.skills-disabled/<skill-name>/`.
- The runner will NOT find it in `.agents/skills/`, ensuring that neither its name nor its description YAML frontmatter is injected into the model's system prompt.
- When re-enabled, it is moved back to `.agents/skills/<skill-name>/`.

---

---

## 5. Skill Manager UI & Ingestion Modes

Accessible via the dedicated **Skills** workspace tab or the `BrainCircuit` action button:
1. **Installed Skills Tab:**
   - Active and disabled skills list with individual enable/disable switches.
   - Estimated token weight for YAML description.
   - Skill content inspector drawer.
   - Delete action (with platform base skill protection).
   - **Scope Badges:**
     - `[UNIVERSAL (PLATFORM)]`: For base platform skills (`oneship-core`, `oneship-devlog`, etc.), always active for all agents.
     - `[@<agent-id>]`: Explicitly assigned to specialized agent personas.
     - `[GENERAL (ALL AGENTS)]`: On-demand skill available to the entire workspace without restriction.
2. **Explore Tab:**
   - **Direct URL & Documentation Ingestion:**
     - Allows direct ingestion from arbitrary HTTP/HTTPS URLs (e.g. `https://fastapi.tiangolo.com/llms.txt`, `https://docs.astro.build/llms.txt`, raw GitHub `SKILL.md`).
     - Auto-derives skill names from URL domain/path.
     - Automatically synthesizes YAML frontmatter if the input is raw markdown or plain text.
     - Applies **Progressive Disclosure** for large documentations (> 4KB): raw content is stored in `references/documentation.txt` while a concise `SKILL.md` is generated with relative pointers.
   - **Community Registry Search (SkillsMP):**
     - Real-time search query box powered by `SkillsMP`.
     - Results showing author, GitHub stars count, description, and 1-Click install action.

---

## 6. Agent Persona Skill Scoping Protocol

1. **Universal Platform Base Invariant:**
   - Platform base skills (`oneship-core`, `oneship-delegation`, `oneship-devlog`, `oneship-snapshots`, `oneship-techdebt`, `oneship-artifacts`) are universal and mandatory.
   - They **cannot** be restricted or assigned to individual agents. They remain active for all personas.
2. **On-Demand Domain Specialization (`rula.toml`):**
   - Specialized on-demand skills can be assigned to agent personas:
     ```toml
     [agents.frontend]
     role = "Frontend Specialist"
     runner = "agy"
     skills = ["pinia", "vue-skill"]
     ```
   - Managed via the `AddPersonaModal.vue` *Assigned Project Skills* selector.
3. **Supervisor Prompt Injection:**
   - When an agent persona is launched, `build_agent_initial_prompt` injects a domain directive:
     `"Assigned Domain Skills: [pinia, vue-skill]. When planning, reasoning, or executing tasks within your domain, prioritize and consult these skills."`
   - This provides semantic alignment and domain specialization across all runners without causing filesystem race conditions.
