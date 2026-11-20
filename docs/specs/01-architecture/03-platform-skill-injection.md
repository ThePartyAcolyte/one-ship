# Platform Skill Injection

## Purpose

Define how the Base Platform Skills Catalog and specialized skills are injected into projects from a single template source, how templates and runner manifests are located, and how skill versions are detected so projects stay compatible across ONE-SHIP upgrades without corrupting customized configuration.

## Scope

- Base vs specialized platform skills and their on-disk layout in `<project_root>/.agents/skills/`.
- The single template source (`templates/`), its relocatable root, and error semantics.
- Skill content breakdown (MCP schemas as SSOT).
- Skill versioning, `SkillStatus` hydration and UI tri-state indication.

## Model / Design

### Base platform skills vs specialized skills (SSOT: `templates/skills/`)

Clicking the `<BrainCircuit />` button in the Project Card toolbar (or executing programmatic pre-boot during Phase 1 of project initialization) synchronizes the **Base Platform Skills Catalog** from `templates/skills/` into `<project_root>/.agents/skills/`.

**Single template source (`kopeng/skill.rs`):** the canonical tree is `templates/` of this repository. Every project skill is a symlink into `templates/skills/<name>`, and prompt templates are read from `templates/prompts/<name>.md`. There is no embedded copy, no CWD-relative search and no `~/.config/one-ship` override: a missing template, an unknown specialized skill or an unknown prompt name is a typed `SkillError`, and a failed step fails the whole sync. `inject_platform_skill` = base symlinks + re-link of present specialized skills + AGENTS.md managed block + runner MCP configs + `rula.toml` version stamp. A corrupt or non-object `opencode.json` / `.mcp.json` is reported, never overwritten. `CURRENT_PLATFORM_VERSION` is the only platform version; a unit test asserts that `oneship-core/SKILL.md` declares the same `version:`. Skill names accepted by the skill commands are single path components (`[A-Za-z0-9._-]`, no leading dot).

**Templates root (`TEMPLATES_ROOT`) resolution** (relocatable; see [Linux Packaging and Host Capabilities](07-linux-packaging-and-host-capabilities.md) for the packaging rationale). Canonical templates (`skills/`, `prompts/`, `runners/`) are resolved in this deterministic order instead of hardcoding `CARGO_MANIFEST_DIR` at compile time:

1. `ONESHIP_TEMPLATES_DIR` (if set, must exist and be a directory; fails loudly otherwise).
2. `<current_exe_dir>/../share/one-ship/templates` (standard FHS prefix: `~/.local/share/one-ship/templates` or `/usr/local/share/one-ship/templates`).
3. `<current_exe_dir>/templates` (portable bundle directory).
4. `$XDG_DATA_HOME/one-ship/templates` or `$HOME/.local/share/one-ship/templates`.
5. `/usr/local/share/one-ship/templates` and `/usr/share/one-ship/templates`.
6. Compile-time dev tree (`<CARGO_MANIFEST_DIR>/../templates`).

**Runner manifests** (`config::load_runner_schema`) are read only from `<templates root>/runners/<runner>.toml`; see [Configuration Schema and Cascade](02-configuration-schema-and-cascade.md).

#### 1. Base platform skills (injected pre-boot)

- `oneship-core`: runtime configuration (`rula.toml`), service execution, and bounded persona rules (<100 words).
- `oneship-delegation`: multi-agent task handoffs and closed-loop callbacks.
- `oneship-devlog`: database-first session journals and active task lifecycle.
- `oneship-snapshots`: pre-refactor Git checkpoints and rollbacks.
- `oneship-techdebt`: architectural debt triage and registry.
- `oneship-artifacts`: live artifact contracts (Inline Ephemeral, Scratchpad, Living Specs).

#### 2. Specialized skills (`SPECIALIZED_SKILLS` in `kopeng/skill.rs`; injected on demand during onboarding, flag resolution, or plugin activation)

- `oneship-initializer`: loaded during Phase 1 brownfield/greenfield project initialization and removed upon `complete_project_initialization`.
- `oneship-docstyles`, `oneship-docstyle-sdd`, `oneship-docstyle-lean`, `oneship-docstyle-contract`, `oneship-docstyles-crafter`: loaded when scaffolding or resolving living documentation (`docs` flag or greenfield scoping).
- `oneship-archetypes`: loaded during greenfield bootstrapping to guide project archetype selection.
- `oneship-profiler-crafter`: loaded when resolving the `profiler` flag to configure profiler engines.
- `oneship-rea`: loaded when `[plugins.rea]` is enabled.
- `oneship-desigma`: loaded when `[plugins.desigma]` is enabled.
- `oneship-pcbcraft`: loaded when `[plugins.pcbcraft]` is enabled.
- `oneship-project-mgmt`: loaded when `[plugins.project_management]` is enabled.

```text
<project_root>/
  └── .agents/
        └── skills/
              ├── oneship-core/
              │     ├── SKILL.md                 <-- Core orchestration & rula.toml configuration
              │     └── references/
              │           └── rula_schema.md     <-- Complete 4-level cascade & configuration spec
              ├── oneship-delegation/
              │     └── SKILL.md                 <-- Multi-agent task handoffs & closed-loop callbacks
              ├── oneship-devlog/
              │     └── SKILL.md                 <-- Database-first session journals & task lifecycle
              ├── oneship-snapshots/
              │     └── SKILL.md                 <-- Pre-refactor Git checkpoints & rollbacks
              ├── oneship-techdebt/
              │     └── SKILL.md                 <-- Architectural debt triage & registry
              └── oneship-artifacts/
                    └── SKILL.md                 <-- Triple-mode artifact delivery (Ephemeral, Scratchpad, Docs)
```

### Content breakdown and SSOT

1. **No duplicated MCP schemas:** MCP tool schemas via `tools/list` are the Single Source of Truth. Skills govern operational protocol, policies and invocation timing.
2. **`oneship-core`:** frontmatter version anchor (`version: 0.2.6` in the legacy text; it must equal `CURRENT_PLATFORM_VERSION`), `rula.toml` structure, interactive command options, and bounded persona rules (<100 words).
3. **`oneship-delegation`:** delegation protocol via `delegate_agent_task` and status callbacks via `report_task_status`.
4. **`oneship-devlog`:** database-first session journaling (`record_session_minutes`), auto-milestone snapshots, and active tasks (`manage_todo_item`).
5. **`oneship-snapshots`:** Git checkpoints (`create_snapshot`), snapshot inspection (`list_snapshots`), and rollback operations (`restore_snapshot`).
6. **`oneship-techdebt`:** severity classification and debt recording (`record_technical_debt`).
7. **`oneship-artifacts`:** triple-mode artifact contracts: inline ephemeral payloads (`[EPHEMERAL]`), evolving scratchpads (`scratchpad/*.md`), and persistent specifications (`docs/*.md`).

### Skill versioning and lifecycle detection

1. **`CURRENT_PLATFORM_VERSION`:** the single source of truth in `src-tauri/src/kopeng/skill.rs`.
2. **`SkillStatus` hydration (`kopeng/skill.rs`):** every project inspects `.agents/skills/oneship-core/SKILL.md` and `rula.toml` during hydration:
   - `is_installed: bool`: whether `oneship-core/SKILL.md` exists.
   - `installed_version: Option<String>`: extracted from `SKILL.md` frontmatter `version: X.Y.Z`.
   - `current_version: String`: `CURRENT_PLATFORM_VERSION`.
   - `is_outdated: bool`: true if `installed_version != CURRENT_PLATFORM_VERSION` or any base skill is not a valid symlink.
   - `is_symlinked: bool`: whether all base skills are valid symlinks into `templates/skills/`.
   - `schema_version: Option<String>`: extracted from `rula.toml [project].oneship_version`.
   - `needs_migration: bool`: true if the project requires a deterministic platform migration (see [Project Versioning and Migration](06-project-versioning-and-migration.md)).
3. **Safe updates:** injected updates overwrite `.agents/skills/oneship-*/` exclusively; `rula.toml` is strictly preserved.
4. **UI tri-state indication:**
   - *Not installed:* dim icon (`opacity-50`).
   - *Active / up to date:* green/emerald icon (`text-emerald-400`).
   - *Outdated:* amber warning indicator (`text-amber-400 animate-pulse`) with a version-upgrade tooltip.

## Contracts

- **Constants:** `CURRENT_PLATFORM_VERSION` and `SPECIALIZED_SKILLS` (`kopeng/skill.rs`); `ONESHIP_TEMPLATES_DIR` environment override.
- **Function:** `inject_platform_skill` (steps above); `cleanup_ephemeral_skill` for `oneship-initializer`.
- **Types:** `SkillStatus` (fields above), `SkillError` (typed).
- **Template layout:** `templates/skills/<name>`, `templates/prompts/<name>.md`, `templates/runners/<runner>.toml`.
- **Project artifacts:** `.agents/skills/oneship-*/` symlinks; `rula.toml [project].oneship_version`; AGENTS.md managed block; `.mcp.json` / `opencode.json`.

## Implementation Status

| Area | Status | Notes |
|---|---|---|
| Single template source, symlinked skills, typed `SkillError` | Implemented | `kopeng/skill.rs`. |
| Relocatable templates root (`ONESHIP_TEMPLATES_DIR` etc.) | Implemented | Verified `ONESHIP_TEMPLATES_DIR` handling and `DEV_TEMPLATES_ROOT` fallback in `kopeng/skill.rs`. The legacy configuration text still described a compile-time-only root; the relocatable order above is the current behavior. |
| `CURRENT_PLATFORM_VERSION` | Implemented | Value in code is `0.2.7` (legacy text quoted `0.2.6`). |
| `SkillStatus` and UI tri-state | Implemented (struct per legacy; UI styling not re-verified) | |

## Related Specs

- [Configuration Schema and Cascade](02-configuration-schema-and-cascade.md)
- [Project Versioning and Migration](06-project-versioning-and-migration.md)
- [Linux Packaging and Host Capabilities](07-linux-packaging-and-host-capabilities.md)
- [Streamable HTTP MCP Daemon](04-streamable-http-mcp-daemon.md)

Supersedes legacy: `10_configuration_schema_and_cascade.md` (§5 Platform Skill Catalog Injection), `41_linux_packaging_and_host_capabilities.md` (relocatable templates root)
