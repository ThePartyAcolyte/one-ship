# Devlog: SolidCraft 3D CAD Integration & Spec 33 (2026-11-15)

## Summary
Authored [Spec 33](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/33_cad_and_pcb_integration.md) defining the integration architecture for **SolidCraft** (3D CAD) and **PCBCraft** (EDA/PCB) plus a dual-solver SPICE/EIS bridge (`ngspice` + optional `Xyce`), validated a memory-capped local build of `tangency-labs/pcbcraft`, and implemented **Stage A (SolidCraft 3D CAD)** across backend, IPC, and frontend.

## Changes Implemented

### 1. PCBCraft Controlled Build & Capability Verification
- Cloned `tangency-labs/pcbcraft` to `/home/butcherwutcher/Projects/dev/pcbcraft`.
- Compiled `pcbcraft` (`65M`) and `pcbcraft-cli` (`13M`, `0.1.0`) under cgroup v2 memory bounds (`MemoryHigh=8G`, `MemoryMax=10G`, `-j 2`, `CARGO_PROFILE_RELEASE_LTO=false`, `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=4`) in `12m 30s` without OOM.
- Probed `pcbcraft-cli mcp --project examples/sensor-node`, verifying `inspect_project` and 101 engine commands (`library.*`, `schematic.*`, `pcb.check_design`, `pcb.export_fabrication`, `net_classes.*`).

### 2. Specification (`docs/specs/33_cad_and_pcb_integration.md`)
- Scoped **SolidCraft** to headless snapshot previews, read-only physical metrics evaluation, and external editor launching (no LLM CAD modeling).
- Scoped **PCBCraft** to native ONE-SHIP SVG/JSON schematic/board visualization, external editor launching, and `/mcp/pcbcraft` focused on component selection, schematic authoring, netlist inspection, and DRC (guarding against LLM copper routing).
- Documented SPICE netlist bridge feeding `ngspice` (XSPICE mixed-signal) and optional `Xyce` (Harmonic Balance / DAE convergence for EIS).

### 3. Stage A — SolidCraft Integration in ONE-SHIP
- **Backend (`src-tauri/`):**
  - Registered `[plugins.solidcraft]` schema in `src-tauri/src/config.rs`.
  - Added shared async Hyprland workspace/window helpers in `src-tauri/src/desktop.rs`.
  - Implemented `src-tauri/src/kopeng/solidcraft.rs` (`SolidcraftError`, `notify` watcher emitting `solidcraft-file-changed`, `get_project_solidcraft_status`, `toggle_project_solidcraft_plugin`, `launch_project_solidcraft`, `render_solidcraft_preview`, `inspect_solidcraft_metrics`).
  - Wired `has_solidcraft_enabled` into `HydratedProject` (`src-tauri/src/hydration.rs`), SSE events (`src-tauri/src/events.rs`), and daemon RPC dispatch (`src-tauri/src/api.rs`).
- **Frontend (`src/`):**
  - Added `src/components/workspace/ProjectCadTab.vue` (file selector, camera view selector, section cut filter, zoom/pan PNG stage, physical metrics sidebar, external launcher).
  - Registered the `cad` tab in `src/stores/workspaceLayoutStore.ts`, `src/components/workspace/ProjectWorkspace.vue`, and toggle card in `src/components/workspace/ProjectOverviewTab.vue`.
  - Updated `src/types/daemonApi.ts` and `src/utils/daemonEvents.ts` against `ts-rs` generated bindings.

## Verification
- `cargo test` (run prior to adding the final 3 helper unit tests in `solidcraft.rs`): `406 passed; 0 failed` and exported `src/types/generated/*Solidcraft*`.
- `npx vue-tsc --noEmit`: 0 errors in SolidCraft/CAD files (3 pre-existing errors remain in `ProjectOverviewTab.vue:296, 307` and `OscilloscopeView.vue:127`).
- `npx vite build`: `2227 modules transformed`, built cleanly in `10.06s`.
- Final `cargo test` re-run skipped per user request due to active `target/` directory file lock.

### 4. Stage A Follow-up — CAD File Templates, Live Project File Explorer Sync & 3D Debt
- **Programmatic `.sc` Creation (`create_solidcraft_file`):**
  - Added `create_solidcraft_file` in `src-tauri/src/kopeng/solidcraft.rs` + `src-tauri/src/api.rs` + `src/types/daemonApi.ts`, driving `solidcraft-cli mcp --headless --allow-path <cad_dir>` (`new_design`, `document.units`, optional `sketch.create`, `save`) across 7 unit/starter templates (`blank_mm`, `xy_sketch_mm`, `blank_in`, `xy_sketch_in`, `blank_cm`, `blank_m`, `blank_ft`).
  - Added `+` creation controls and modal (`File Name`, `Template`, `Open in SolidCraft` toggle) in `src/components/workspace/ProjectCadTab.vue` passing the new `file_name` straight to `launch_project_solidcraft`.
- **Workspace File Explorer Real-Time Sync:**
  - Added debounced recursive `notify` project watcher (`ensure_project_files_watcher`) in `src-tauri/src/kopeng/documents.rs`, emitting `project-files-changed` (`src-tauri/src/events.rs`, `src/utils/daemonEvents.ts`) for non-ignored project files.
  - Subscribed `src/components/workspace/ProjectWorkspace.vue` to `project-files-changed` with cleanup in `onUnmounted`, plus a manual refresh icon button in the Explorer toolbar.
- **Technical Debt:** Documented Stage A.2 WebGL 3D CAD Viewport (`Three.js`) in `TECHNICAL_DEBT.md` §26 and `docs/specs/33_cad_and_pcb_integration.md`.
