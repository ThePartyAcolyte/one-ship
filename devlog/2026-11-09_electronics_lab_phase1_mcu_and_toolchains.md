# Devlog: Electronics & Embedded Lab — Phase 1: Software Foundation & MCU Toolchains

**Date:** 2026-11-09  
**Topic:** Embedded Systems Architecture, Microcontroller Discovery, PlatformIO Toolchains & Live Serial Interface  
**Author:** Antigravity (Pair Programming with Developer)  

---

## 1. Context & Motivation

Expanding ONE-SHIP into embedded systems development requires closing the loop between software engineering and physical microcontrollers. In Phase 1, we implemented the software foundation, project configuration cascade, hardware detection layer, and compilation/flashing pipelines:
1. **Zero Out-of-Process Silos:** Microcontroller compilation and uploading are integrated directly into the Process Supervisor rather than relying on disparate external IDEs.
2. **Contextual Subsystem Visibility:** Following the architectural refactorings in Spec 10 & 26, the Electronics & Embedded Lab exists as an opt-in project subsystem represented in the Project Overview Feature Matrix and contextual tab bar.
3. **Dual-Actor Foundations:** Establishes strongly-typed RPC endpoints and UI control panels for both developers and autonomous AI agents.

---

## 2. Changes Implemented

### 2.1 Backend (`src-tauri/`)
1. **Dependencies (`Cargo.toml`):**
   - Integrated `serialport = "4.7"` backed by the host's native `libudev` (v261).
2. **New Kopeng Domain Module (`src-tauri/src/kopeng/electronics/`):**
   - `errors.rs`: `ElectronicsError` enum implementing `api::ApiError` with typed HTTP status codes and stable error keys.
   - `discovery.rs`: `scan_connected_microcontrollers()` detecting `/dev/ttyUSB*` and `/dev/ttyACM*` ports with VID/PID, product descriptions, vendor names, and serial numbers.
   - `toolchain.rs`: `check_electronics_toolchain()` probing versions for PlatformIO (`pio`), `probe-rs`, `esptool.py`, and `arduino-cli` with Omarchy `install_hint_arch`.
   - `firmware.rs`: PlatformIO compilation (`pio run`) and flashing (`pio run -t upload`) operations with structured `FirmwareOpResult` execution summaries.
   - `mod.rs`: Lifecycle management, project activation detection (`is_project_electronics_enabled`), and aggregated `ProjectElectronicsStatus`.
3. **Configuration Schema (`src-tauri/src/config.rs`):**
   - Registered `[plugins.electronics]` in `get_all_plugin_schemas()` exposing `enabled` (project scope), `default_baud_rate` (cascade, default `"115200"`), and `flasher_tool` (cascade, default `"pio"`).
4. **Project Hydration (`src-tauri/src/hydration.rs`):**
   - Added `has_electronics_enabled: bool` to `HydratedProject` (reading `rula.toml` or detecting `platformio.ini`).
5. **Daemon Commands & Type Synchronization (`src-tauri/src/api.rs`):**
   - Registered 6 new daemon RPC commands:
     - `check_electronics_toolchain`
     - `list_connected_mcus`
     - `get_project_electronics_status`
     - `toggle_project_electronics_plugin`
     - `build_project_firmware`
     - `flash_project_firmware`
   - Generated corresponding TypeScript contracts via `ts-rs` into `src/types/generated/`.

### 2.2 Frontend (`src/`)
1. **Daemon RPC Interface (`src/types/daemonApi.ts` & `src/types/index.ts`):**
   - Mirrored all 6 daemon commands in `DaemonCommands`, satisfying `api_commands_match_the_frontend_map`.
   - Exported generated types (`DiscoveredMcuPort`, `ProjectElectronicsStatus`, `ElectronicsToolStatus`, `FirmwareOpResult`, `ElectronicsError`).
2. **Workspace Navigation Store (`src/stores/workspaceLayoutStore.ts`):**
   - Added `'electronics'` to `WorkspaceTab`.
   - Updated `switchProjectContext` with safe fallback to `'overview'` if target project lacks electronics capability.
3. **Global Keyboard Navigation (`src/App.vue`):**
   - Synchronized `Alt+W` tab switching cycle to dynamically include `electronics` when active project has the capability enabled.
4. **Project Overview Capabilities Matrix (`src/components/workspace/ProjectOverviewTab.vue`):**
   - Expanded Section 1 into a 4-card grid featuring **Electronics Lab** with live `Active`/`Disabled` status badge, toggle switch backed by `toggle_project_electronics_plugin`, and quick-open navigation.
5. **Contextual Workspace Tab (`src/components/workspace/ProjectWorkspace.vue`):**
   - Added contextual tab button `v-if="activeProject?.has_electronics_enabled"` with status dot.
   - Unified watcher fallback ensuring automatic redirection to `'overview'` if capability is disabled.
6. **Workspace Tab Implementation (`src/components/workspace/ProjectElectronicsTab.vue`):**
   - **Header & Toolchain Health:** Visual indicators for PlatformIO, probe-rs, esptool, and arduino-cli with installation command hints.
   - **Target Microcontroller Selector:** Live selection of detected serial ports (`AppSelect`), baud rate picker, and hardware metadata chip (VID/PID, Vendor, Serial).
   - **Firmware Compilation & Upload:** Environment selection, compile/flash triggers with execution timers, status banners, and collapsible compiler log viewer.
   - **Interactive Serial Monitor:** Real-time monospace terminal, pause/play toggling, log clearing, and command transmission input bar.

---

## 3. Verification & Validation

1. **Rust Test Suite & API Mirroring:**
   - `cargo test --lib api::tests::api_commands_match_the_frontend_map`: Passed (153/153 commands synchronized).
   - `cargo test --lib kopeng::electronics`: 9/9 unit and ts-rs export tests passed cleanly.
2. **TypeScript Compilation:**
   - `npx vue-tsc --noEmit`: 0 errors.
3. **Frontend Production Bundle:**
   - `npm run build`: Vite production bundle succeeded cleanly in 3.80s.
4. **Daemon Compilation:**
   - `cargo build --bin oneship-daemon`: Succeeded in 6.97s with 0 errors.
