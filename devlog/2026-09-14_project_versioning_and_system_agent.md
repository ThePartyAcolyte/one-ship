# Devlog: 2026-09-14 - Comprehensive Project Versioning & Global System Agent

## Topic
Implementation of a two-tier project versioning and migration engine in Wang-Kapawu, paired with the introduction of a first-class Global System Agent accessible via the unified chat overlay.

## Objectives
1. Introduce explicit `kapawu_version = "0.2.4"` in `rula.toml` alongside `CURRENT_PLATFORM_VERSION = "0.2.4"`.
2. Update hydration pipeline to track both platform skill lifecycle and project schema versioning.
3. Establish the Global System Agent (`system`) within the daemon and chat overlay for project management, initialization, and stepwise migrations.
4. Support 3 execution modes for project updates (Semi-attended, Unattended, Supervised via Chat).
5. Build the deterministic AST migration engine (`src-tauri/src/kopeng/migration.rs`) for gap-resilient stepwise upgrades.

## Progress
- [x] Evaluated architectural trade-offs between deterministic and non-deterministic agent migrations.
- [x] Defined specification ([`docs/specs/24_project_versioning_and_migration_engine.md`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/docs/specs/24_project_versioning_and_migration_engine.md)).
- [x] Phase 1: Platform Version Invariants (`v0.2.4`), templates synchronization, and `SkillStatus` hydration telemetry (`schema_version`, `needs_migration`, `is_outdated`).
- [x] Phase 2: Global System Agent (`system`) integrated into `App.vue` and pinned to the top of `AgentList.vue`.
- [x] Phase 3: Stepwise Deterministic Migrator (`src-tauri/src/kopeng/migration.rs`) with pre-migration Git checkpoints and atomic file/schema patching.
- [x] Phase 4: `ProjectInitModal.vue` enhanced to handle both initial bootstraps and migrations with adaptive titles, badges, and the 3-mode workflow connecting Supervised Mode directly to the System Agent in the Chat Overlay.
- [x] Verified full Rust compilation (`cargo check`) and Vue TypeScript build (`npm run build`).

## Update: MCP Primitives Cleanup & SSOT Consolidation
- Audited all usages of `mcp_primitives.md` across backend, templates, and injected skills.
- Removed `mcp_primitives.md` template and removed its compilation/injection overhead from `skill.rs`.
- Added automatic legacy cleanup for `mcp_primitives.md` if found in `.agents/skills/kapawu-platform/references/`.
- Updated `SKILL.md` to establish the Streamable HTTP MCP server's `tools/list` JSON-RPC endpoint as the sole source of truth for tool schemas.
- Cleaned and verified baseline migration definitions for version `0.2.4`.
