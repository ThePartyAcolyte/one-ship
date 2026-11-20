# Work Session Devlog: `wang-kapawu` → ONE-SHIP Rebrand Audit & Completion

**Date:** 2026-09-30  
**Status:** Completed  
**Scope:** Full residual audit of the legacy `wang-kapawu` / `Wang-Kapawu` brand and atomic remediation across backend, frontend, templates, managed projects, documentation, and host system state.

---

## 1. Overview

The `wang-kapawu` → ONE-SHIP rebrand performed on 2026-09-23 left residual references across source code, injected artifacts, living specs, and host configuration. This session executed a complete forensic audit (530 matches / 154 files at start) and remediated every actionable reference while preserving the intentional `kapawu` Belter domain namespace, the HKDF credential salt, and the historical `devlog/` record.

## 2. Approved Policy Decisions

| ID | Decision |
|----|----------|
| D1 | Preserve the short `kapawu` namespace (skills `kapawu-*`, `kapawu_version`, `refs/kapawu/`, `.kapawu/`, UI events). |
| D2 | Rename per-project hidden state dir `.wang-kapawu/` → `.one-ship/` with one-time migration. |
| D3 | Rename managed AGENTS.md markers to `ONE_SHIP_MANAGED_*` with idempotent legacy-block replacement. |
| D4 | Freeze `devlog/` history (226 references retained). |
| D5 | Migrate legacy localStorage keys once and remove fallbacks. |
| D6 | Preserve the HKDF salt literal `wang-kapawu-accounts-v1` (credential decryption stability). |

## 3. Executed Phases

### Phase 1 — Backend Rust
- `mcp.rs`: 33 tool descriptions and all scoping error messages rebranded; `serverInfo.name` confirmed `one-ship`.
- `manifest.rs`: `ONE_SHIP_MANAGED_*` markers, `strip_managed_blocks()` idempotent legacy-block replacement, `migrate_legacy_project_dir()` for `.wang-kapawu` → `.one-ship`, purge list covers both legacy and current plugin dirs, `created_by_one_ship` marker.
- `skill.rs`: duplicate `wang-kapawu` MCP server injection removed (plugin/mcp/opencode configs now single-server with active pruning); Antigravity instructions provisioned under `~/.gemini/antigravity-cli/mcp/one-ship/`; dev hardcoded path removed.
- `config.rs`: single-server sync with active removal of the legacy entry from `~/.gemini/config/mcp_config.json`; runner path fallback added.
- `supervisor.rs` / `flag_resolver.rs`: prompt ServerName unified to `one-ship` (fixed corrupted `wang-kapawu_wang-kapawu`).
- `profiler.rs`: global plugins dir `~/.config/one-ship/plugins` with one-time migration; author and SVG labels rebranded.
- `documents.rs`: attachments under `.one-ship/attachments` with migration helper; `diagnostic.rs`, `initializer.rs`, `visual_builder.rs` (Onlook AppImage one-time move), `stats.rs` (legacy statusline fallback removed).
- Verification: `cargo check` clean; `cargo test` 30/30.

### Phase 2 — Frontend
- New `src/utils/legacyStorage.ts`: generic one-time migration of `wang_kapawu_*` → `oneship_*` localStorage keys, invoked in `main.ts` before mount.
- `coreStore.ts`, `useProjectStore.ts`, `useQuotaStore.ts`, `SettingsModal.vue`: canonical keys only, zero fallbacks.
- `i18n.ts`: visible hub title `ONE-SHIP // Stashang`.
- Verification: `vue-tsc` + `vite build` clean.

### Phase 3 — Templates & Injected Artifacts
- 10 platform skills + `rula_schema.md`: brand and config paths updated to `~/.config/one-ship/`; managed markers updated.
- 8 prompts: identity and MCP ServerName only `one-ship`.
- Runners, Belter ticker prompt, visual builder client comments rebranded.
- Managed projects (odoo-mcp, estudiorc, dictate-whisper, fideiuris): legacy MCP entries removed from `opencode.json`, `.agents/mcp_config.json`, `.agents/plugins/one-ship/mcp_config.json` (JSON validated); AGENTS.md migrated to `ONE_SHIP_MANAGED_*`; estudiorc `visual-builder-client.ts` comments updated.

### Phase 4 — Meta, Docs & Scripts
- `docs/`: 121 replacements across 30 files (two disjoint sub-agent batches). Only intentional residual: the D6 salt literal in spec 17.
- `README.md`, `TECHNICAL_DEBT.md`, `AGENTS.md` (brand, `oneship-daemon`, `oneship.log`, plugin path), `package-lock.json` name, `scripts/run_dev.sh` legacy `pkill` patterns removed, `scripts/agy_statusline.py` docstring/state path.

### Phase 5 — Host System State
- Legacy DB archived: `~/.config/wang-kapawu/kapawu.db` (+ `-wal`/`-shm`, 27 MB / 26 MB) → `~/.config/one-ship/archive/`. DB analysis confirmed 0 projects/chats/journal/flags/todos/debt/snapshots; only pre-rebrand forensic logs (8.538), 16 visual-builder undo snapshots, 1 pending resume, and obsolete account rows.
- Removed: `~/.config/wang-kapawu`, `~/.local/state/wang-kapawu`, `~/.local/share/wang-kapawu`, `~/.local/share/tech.kirulab.wang-kapawu`, stale Antigravity MCP dirs (`wang-kapawu`, `wang-kapawu_wang-kapawu`, `one-ship_one-ship`, `one-ship_wang-kapawu`), empty DBs (`~/.config/one-ship/kapawu.db`, repo-root `kapawu.db`), and `~/Projects/dev/.agents/plugins/wang-kapawu`.
- Migrated remaining `.wang-kapawu` state dirs to `.one-ship`: odoo-mcp, estudiorc, dictate-whisper, and `odoo-dev/odoo18/addons/l10n_cl_banking`.
- Removed the legacy `wang-kapawu` server entry from `~/.gemini/config/mcp_config.json` (validated).

## 4. Verification

- `cargo check`: clean. `cargo test`: 30/30.
- `npm run build` (`vue-tsc` + Vite): clean.
- Global residual audit (excluding frozen `devlog/`, D6 salt, legacy migration/purge constants): only `manifest.rs` legacy purge path remains, required to clean old projects.
- Host residual scan (excluding the repository directory name itself): clean.

## 5. Incidents

- **Managed backup markers lost during migration:** the first migration pass used `mv -n` against an already-created target `backups/` dir, which silently skipped the source entries before `rm -rf` removed them. Lost: `AGENTS.md.created` markers (odoo-mcp, estudiorc — recreated as `created_by_one_ship`) and `dictate-whisper/.wang-kapawu/backups/AGENTS.md.orig` (pre-injection original AGENTS.md backup). Recovery path: check the dictate-whisper Git history for the backup file; the runtime fallback (strip managed block) covers deintegration semantics meanwhile. Migration corrected afterwards with a safe rename for `odoo-dev`.
- Historical `audit_wang_kapawu_2026-09-23.md` remains inside `~/.gemini/antigravity-cli/brain/` as an archived AI session artifact and was intentionally not deleted.

## 6. Pending Developer Actions

1. Rename the repository directory `…/Projects/dev/wang-kapawu` → `…/Projects/dev/one-ship` (deferred: active session workspace root and running daemon).
2. Restart the app via `scripts/run_dev.sh` to load the new binaries (active pruning of the global MCP entry and all migrations now execute in code).
3. Optional: recover `dictate-whisper` `.orig` backup from Git history.
