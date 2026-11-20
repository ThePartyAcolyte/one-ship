# Devlog: Dynamic PCBCraft MCP Persistence, Live Connect & Domain-Scoped Snapshots (2026-11-16)

## Summary
Resolved the silent in-memory-only edit issue in `pcbcraft-cli mcp`, added dynamic switching between headless disk persistence (`--project` + `save_project`) and live editor control (`--connect`), implemented domain-isolated auto-snapshots (`code`, `pcbcraft`, `solidcraft`, `desigma`) with per-asset retention limits, and added a post-update review/revert banner in `ProjectPcbTab.vue`.

## Changes Implemented

### 1. Phase 1 — Domain-Scoped Snapshots & Retention (`one-ship`)
- **Database & Snapshot Engine (`src-tauri/src/db.rs`, `src-tauri/src/kopeng/snapshots.rs`):**
  - Added `domain` (`'code'`, `'pcbcraft'`, `'solidcraft'`, `'desigma'`) and `asset_key` columns to `snapshots` (`idx_snapshots_domain_asset`) and exported `Snapshot` (`ts-rs`).
  - Added `create_domain_snapshot`, `prune_domain_asset_snapshots`, and `try_create_auto_domain_snapshot` using millisecond-precision `ref_path` timestamps to prevent collisions.
- **Per-Feature Config (`src-tauri/src/config.rs`):**
  - Added `auto_snapshot_on_edit` (`boolean`, default `true`) and `max_snapshots_per_asset` (`number`, default `20`) to `desigma`, `solidcraft`, and `pcbcraft` plugin schemas.
- **Git & Snapshots UI (`src/components/workspace/ProjectGitSnapshotsTab.vue`):**
  - Added domain filter pills (`All`, `Code`, `PCB`, `CAD`, `Design`) and domain/`asset_key` badges on non-code snapshots.

### 2. Phase 2 — `save_project` & Dynamic `--connect` / `--project` (`pcbcraft` + `one-ship`)
- **PCBCraft Upstream (`crates/mcp/src/{tools.rs,backend.rs,live.rs}`, `apps/pcbcraft-cli/src/main.rs`, `crates/app/src/{live_control.rs,lib.rs}`):**
  - Added `save_project` MCP tool persisting `Headless` sessions via `pcbcraft_storage::save` / `encode_packed` and `Remote` sessions via `file.save`.
  - Auto-started `LiveControl` when `pcbcraft` opens a project via `with_initial_project`, publishing ephemeral `0600` discovery metadata under `~/.cache/one-ship/pcbcraft-live/` (cleaned up on `Drop`).
- **ONE-SHIP Gateway & UI (`src-tauri/src/kopeng/pcbcraft.rs`, `templates/skills/oneship-pcbcraft/SKILL.md`, `src/components/workspace/ProjectPcbTab.vue`):**
  - Updated `get_or_spawn_mcp_process` to dynamically select `--connect` when PCBCraft is open on the target or `--project` when closed.
  - Added pre-save auto-snapshot before `save_project` (with 2.5s dedup against `notify` watcher snapshots) and external-edit auto-snapshot in the watcher.
  - Added post-update review bar in `ProjectPcbTab.vue` (`Open in PCBCraft` / `Revert Snapshot` / `Keep`).

### 3. Phase 3 — `SolidCraft` & `Desigma` Auto-Snapshots
- Connected `notify` file-change watchers in `src-tauri/src/kopeng/solidcraft.rs` and `src-tauri/src/kopeng/desigma.rs` to `try_create_auto_domain_snapshot`.

### 4. Specs Synchronized
- Updated `docs/specs/16_agent_session_snapshot_system.md` (§11) and `docs/specs/33_cad_and_pcb_integration.md` (§9).

### 5. Native Schematic SVG Preview Renderer (`pcbcraft-cli render-svg`)
- Fixed `pcbcraft-cli` `load_project` baseline initialization (`pcbcraft_storage::load(&root)`) so `save_project` writes cleanly without false disk-conflict errors.
- Added [svg_preview.rs](file:///home/butcherwutcher/Projects/dev/pcbcraft/apps/pcbcraft-cli/src/svg_preview.rs) (`pcbcraft-cli render-svg <path> [--sheet <id>]`) rendering real `pcbcraft-engine` symbol graphics (`Shape::Graphic`, `Line`, `Polyline`, `Rect`, `Circle`, `Text`) and wired `render_pcbcraft_svg_preview` in `src-tauri/src/kopeng/pcbcraft.rs` (`svg-v2` cache key).
