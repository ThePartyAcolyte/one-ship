# On-Demand Skills and Skill Manager

## Purpose

Keep agent context windows lean by establishing a project-scoped, just-in-time (JIT) skill lifecycle: projects start with mandatory platform base skills only, specialized skills are discovered/installed on demand via MCP or the Skills UI, disabled skills are isolated out of runner scan paths, and domain skills are scoped per persona in `rula.toml`.

## Scope

In scope: project `.agents/skills/` and `.agents/.skills-disabled/` layout, `src-tauri/src/kopeng/skill.rs`, MCP tools (`search_project_skills`, `manage_project_skill`), base platform skill protection, specialized domain skill symlinking (`SPECIALIZED_SKILLS`), daemon commands for skill management, `ProjectSkillsTab.vue` (Installed & Explore tabs with URL progressive disclosure and SkillsMP search), and persona `skills` scoping.

Out of scope: core platform skill template packaging and injection cascade (see [Platform Skill Injection](../01-architecture/03-platform-skill-injection.md)).

## Model / Design

### 1. Directory Layout & Runner Isolation

```text
<project-root>/
├── .agents/
│   ├── skills/                       # ACTIVE skills (scanned by agy, claude, opencode)
│   │   ├── oneship-core/
│   │   │   └── SKILL.md
│   │   └── stripe-api/
│   │       └── SKILL.md
│   └── .skills-disabled/             # INACTIVE skills (ignored by all runners)
│       └── meilisearch-patterns/
│           └── SKILL.md
```

Any runner implementing the `SKILL.md` standard (`agy`, `claude`, `opencode`) scans `<project-root>/.agents/skills/`:
- **Disabling:** Atomically moves the skill folder to `.agents/.skills-disabled/<skill-name>/`, hiding both its name and YAML frontmatter description from model system prompts.
- **Re-enabling:** Moves the folder back to `.agents/skills/<skill-name>/`.

### 2. Platform Base Skills, Specialized Skills & Persona Scoping

1. **Universal platform base invariant:** Base platform skills (`oneship-core`, `oneship-delegation`, `oneship-devlog`, `oneship-snapshots`, `oneship-techdebt`, `oneship-artifacts`) are universal, mandatory, and protected from disabling or deletion via `manage_project_skill` and UI controls.
2. **Specialized ephemeral/plugin skills (`SPECIALIZED_SKILLS`):** Dynamically symlinked and cleaned up via `inject_specialized_skill` / `cleanup_ephemeral_skill` for onboarding, docstyles, profiler crafting, and domain plugins (`oneship-rea`, `oneship-desigma`, `oneship-pcbcraft`, `oneship-project-mgmt`).
3. **Persona skill scoping (`rula.toml`):**
   ```toml
   [agents.frontend]
   role = "Frontend Specialist"
   runner = "agy"
   skills = ["pinia", "vue-skill"]
   ```
   Configured via `AddPersonaModal.vue` (*Assigned Project Skills*). At launch, `build_agent_initial_prompt` injects:
   `"Assigned Domain Skills: [pinia, vue-skill]. When planning, reasoning, or executing tasks within your domain, prioritize and consult these skills."`

### 3. Skill Manager UI (`ProjectSkillsTab.vue`)

Accessible via the **Skills** workspace tab or `BrainCircuit` action button:
1. **Installed Skills Tab:**
   - Lists active and disabled skills with enable/disable toggles, estimated YAML description token weight, content inspector drawer, delete action (with base skill protection), and scope badges:
     - `[UNIVERSAL (PLATFORM)]`: Base platform skills.
     - `[@<agent-id>]`: Assigned to specific personas.
     - `[GENERAL (ALL AGENTS)]`: Workspace-wide on-demand skills.
2. **Explore Tab:**
   - **Direct URL & documentation ingestion:** Ingests arbitrary HTTP/HTTPS URLs (e.g., `llms.txt`, raw GitHub `SKILL.md`), auto-derives skill names from domain/path, synthesizes YAML frontmatter if raw markdown/plain text, and applies **Progressive Disclosure** when content exceeds `4 KB` (storing raw content in `references/documentation.txt` and generating a concise `SKILL.md` with relative pointers).
   - **Community registry search (SkillsMP):** Searches `SkillsMP` returning author, GitHub stars, description, and 1-click install.

## Contracts

### MCP Tools (`src-tauri/src/mcp.rs`)

- **`search_project_skills`:**
  - `query` (`string`, required): Technology, library, or API name.
  - `limit` (`number`, optional, default `10`): Maximum results.
  - Output: Markdown list with name, author, stars, description, and source repository URL.
- **`manage_project_skill`:**
  - `action` (`"list" | "install" | "enable" | "disable" | "remove"`, required).
  - `project` (`string`, optional): Project ID, name, or path (defaults to attested `CallerContext` project).
  - `skill_name` (`string`, required for `install`/`enable`/`disable`/`remove`).
  - `source_url` (`string`, required for `install`): Repository or raw `SKILL.md` URL.
  - Safety invariants: rejects disabling/removing protected platform base skills; confines all filesystem operations strictly within `<project>/.agents/`.

### Daemon Commands (`api::API_COMMANDS` / `daemonCall`)

- `inject_platform_skill`
- `get_project_skills`
- `toggle_project_skill`
- `remove_project_skill`
- `get_project_skill_content`
- `search_marketplace_skills`
- `install_project_skill`

*Note:* Errors serialize as `SkillError` in `ApiErrorResponse`.

## Implementation Status

| Item | Status | Evidence |
| :--- | :--- | :--- |
| `.agents/skills` & `.agents/.skills-disabled` management, `SkillError`, `SPECIALIZED_SKILLS` | Implemented | `src-tauri/src/kopeng/skill.rs` |
| MCP tools (`search_project_skills`, `manage_project_skill`) | Implemented | `src-tauri/src/mcp.rs` |
| All 7 skill daemon commands in `API_COMMANDS` | Implemented | `src-tauri/src/api.rs` |
| Persona `skills` prompt injection | Implemented | `src-tauri/src/kopeng/prompt.rs`, `src/components/modals/AddPersonaModal.vue` |
| `ProjectSkillsTab.vue` Installed & Explore UI | Implemented | `src/components/workspace/ProjectSkillsTab.vue` |

## Related Specs

- [Platform Skill Injection](../01-architecture/03-platform-skill-injection.md)
- [Native Process Supervisor](01-native-process-supervisor.md)
- [MCP Dialogue Channel and Chat Overlay](03-mcp-dialogue-and-chat-overlay.md)

Supersedes legacy: `29_on_demand_skill_manager_and_mcp_protocol.md`
