# Devlog: PCBCraft Backend, SVG Renderer, MCP Gateway & `ts-rs` Fix (2026-11-16)

## Summary
Fixed the `TS_RS_EXPORT_DIR` working-directory mismatch in `scripts/run_dev.sh` and implemented **Spec 33 Stage B.1** (PCBCraft daemon integration, pure-Rust schematic/board SVG renderer, BOM/netlist/DRC inspection, `/mcp/pcbcraft` stdio gateway, and `oneship-pcbcraft` guardrail skill).

## Changes Implemented

### 1. `ts-rs` Binding Export Hygiene (`scripts/run_dev.sh`)
- Added `--setenv=TS_RS_EXPORT_DIR="$PROJECT_ROOT/src/types/generated"` to `build_daemon` in `scripts/run_dev.sh` so automated watcher builds (`oneship-dev-daemon-build`) always emit generated TypeScript contracts into `src/types/generated/` regardless of Cargo invocation CWD.

### 2. Stage B.1 — PCBCraft Backend & MCP Gateway
- **Daemon Subsystem (`src-tauri/src/kopeng/pcbcraft.rs`, `kopeng/mod.rs`):**
  - Implemented `PcbcraftError` (`thiserror` + `ApiError`), project target discovery (`project.json` folders and `.pc` archives under `pcb_dir`), recursive debounced `notify` watcher emitting `pcbcraft-file-changed`, and per-project `pcbcraft-cli mcp --project` stdio process pool.
  - Added daemon commands: `get_project_pcbcraft_status`, `toggle_project_pcbcraft_plugin`, `create_pcbcraft_project`, `launch_project_pcbcraft`, `inspect_pcbcraft_project` (`inspect_project` + `pcb.check_design`), and `render_pcbcraft_svg_preview` (bounded pure-Rust SVG rendering for both `schematic` and `board` modes), plus `extract_net_pin_map` helper for downstream Stage C SPICE extraction and 3 unit tests.
- **Configuration, Hydration, Events & API (`config.rs`, `hydration.rs`, `events.rs`, `mcp.rs`, `api.rs`):**
  - Registered `[plugins.pcbcraft]` schema (`enabled`, `app_binary_path`, `cli_binary_path`, `pcb_dir`, `mcp_request_timeout_ms`) and synced `/mcp/pcbcraft` into `~/.gemini/config/mcp_config.json`.
  - Added `has_pcbcraft_enabled` on `HydratedProject`, `"pcbcraft-file-changed"` event (`PcbcraftFileChangedPayload`), `/mcp/pcbcraft` route in `mcp.rs`, and all 6 commands in `api.rs`.
- **Platform Skill & Runner Config Injection (`templates/skills/oneship-pcbcraft/SKILL.md`, `src-tauri/src/kopeng/skill.rs`):**
  - Added `oneship-pcbcraft` specialized skill enforcing architecture/schematic/DRC authoring and prohibiting manual LLM copper routing (`pcb.add_tracks`, `pcb.add_via`, `pcb.reshape_tracks`).
  - Synchronized `"pcbcraft"` server entries in `.mcp.json` and `opencode.json` on toggle/sync.
- **Frontend API Contracts (`src/types/daemonApi.ts`, `src/utils/daemonEvents.ts`):**
  - Mirrored all 6 `pcbcraft` commands and `pcbcraft-file-changed` event against `ts-rs` exports.

### 3. Stage B.2 — Frontend Integration & Spec 33 Stage D/E Roadmap
- **Configuration & Workspace Tabs:**
  - Wired `pcbcraft` toggle and quick-launch links (`Open CAD`, `Open PCB`) into `src/components/workspace/ProjectConfigPanel.vue` (`TOGGLEABLE_PLUGIN_IDS`, `isCategoryEnabled`, `handleCategoryToggle`).
  - Registered `'pcb'` in `src/stores/workspaceLayoutStore.ts` and `src/components/workspace/ProjectWorkspace.vue`.
- **PCB Viewer & Inspector (`src/components/workspace/ProjectPcbTab.vue`):**
  - Target/mode (`Schematic` / `Board`) and sheet selectors, `+` modal calling `create_pcbcraft_project`, zoom/pan SVG stage reacting live to `pcbcraft-file-changed` (with `onUnmounted` teardown), `DRC` / `BOM` / `Nets` sidebar, and external `pcbcraft` launcher.
- **Spec 33 Roadmap (`docs/specs/33_cad_and_pcb_integration.md`):**
  - Documented **Stage D** (Mouser SearchAPI + AliExpress Open Platform/CL deep-links + RS Chile deep-links + local PDF datasheets) and **Stage E** (`pcb.export_fabrication` ZIP + PCBWay Partner API quoting/logistics/approval-gated ordering).

### 4. Direct Project CLI Launch Patch (`pcbcraft` + `one-ship`)
- Added `PcbApp::with_initial_project(Option<PathBuf>, &egui::Context)` in `/home/butcherwutcher/Projects/dev/pcbcraft/crates/app/src/lib.rs` and positional argument parsing in `/home/butcherwutcher/Projects/dev/pcbcraft/apps/pcbcraft/src/main.rs`.
- Compiled `/home/butcherwutcher/Projects/dev/pcbcraft/target/release/pcbcraft` under `systemd-run` (`MemoryHigh=8G`, `MemoryMax=10G`, `-j 2`, `CARGO_PROFILE_RELEASE_LTO=false`) cleanly in `1m 18s`.
- Updated `launch_project_pcbcraft` in `src-tauri/src/kopeng/pcbcraft.rs` to pass `<target_path>` directly to `pcbcraft` when launching.

### 5. Persistent Preview Disk Cache & Instant Pinia Tab Cache (`CAD` + `PCB`)
- **Persistent Disk Cache (`src-tauri/src/kopeng/preview_cache.rs`, `solidcraft.rs`, `pcbcraft.rs`):**
  - Added bounded `~/.cache/one-ship/previews/` JSON disk cache (`MAX_DISK_CACHE_ENTRY_BYTES = 24 MiB`, `MAX_DISK_CACHE_FILES_COUNT = 32`, `modified_ms`-validated via `tokio::fs` + `spawn_blocking` mtime pruning) so daemon restarts reuse unchanged rendered previews, SolidCraft metrics, and PCBCraft inspections/SVGs without re-invoking external CLIs.
- **Instant Frontend Store Cache (`src/stores/hardwarePreviewStore.ts`, `ProjectWorkspace.vue`, `ProjectCadTab.vue`, `ProjectPcbTab.vue`):**
  - Preserved CAD and PCB previews, metrics, inspections, statuses, and active selections in Pinia across tab unmounts, listening globally for `solidcraft-file-changed` / `pcbcraft-file-changed` to invalidate affected targets, parallelizing CAD preview + metrics fetches when dirty, and serving clean cached views on tab switch without daemon round-trips.

## Next
- Confirm `oneship-dev-daemon-build` finishes compiling and runs `export_bindings` into `src/types/generated/`.
- Implement **Stage C** (`kopeng::electronics::spice` mixed-signal `ngspice` / optional `Xyce` bridge) on approval.
