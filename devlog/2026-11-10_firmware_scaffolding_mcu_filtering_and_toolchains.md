# Devlog: Firmware Scaffolding, Dynamic PlatformIO Board Catalog, MCU Discovery Filtering & Toolchain Diagnostics

**Date:** 2026-11-10  
**Domain:** Embedded Systems, Electronics Workstation, Firmware Scaffolding, Serial Port Filtering, Toolchain Management  
**Status:** Completed  

---

## 1. Problem Statement & Motivation

During hardware-in-the-loop pair programming sessions:
1. **Ghost Microcontrollers Detection:** The system reported 33 microcontrollers connected when only an Owon VDS1022 USB oscilloscope was physically attached. The Linux kernel exposes 32 legacy motherboard UARTs (`/dev/ttyS0`..`/dev/ttyS31`), and the oscilloscope's USB interface was conflated with a generic target MCU serial port.
2. **PlatformIO Installation vs Project Configuration Conflation:** The user had PlatformIO Core installed on Omarchy Linux (`/usr/bin/pio` 6.2.0-1), yet the UI showed "PlatformIO Core Missing" because the status check conflated missing project `platformio.ini` with missing CLI installation.
3. **Comprehensive Embedded Project Scaffolding:** Embedded firmware projects lacked an automated onboarding wizard. The user required full support for the complete breadth of microcontrollers supported by PlatformIO (1,656+ boards across 44 platforms), selectable monitor baud rates, starter C/C++ code generation, and direct integration into `rula.toml`.
4. **Toolchain Visibility & One-Click Remediation:** Missing toolchains (`platformio`, `probe-rs`, `sigrok-cli`, `ngspice`, `kicad`) lacked unified visibility and actionable installation commands.

---

## 2. Architectural Changes & Implementations

### 2.1 Backend Port Filtering & Test Instrument Exclusion (`kopeng::electronics::discovery`)
- Added strict filtering against Linux legacy serial dummy devices (`!port_name.starts_with("/dev/ttyS")`), eliminating the 32 phantom UART ports.
- Defined `INSTRUMENT_USB_IDS` constant containing `(0x5345, 0x1234)` (Owon VDS1022) and `(0x0925, 0x3881)` / `(0x04b4, 0x8613)` (Logic Analyzers). USB-to-serial ports belonging to connected test equipment are excluded from target MCU candidates so firmware cannot be uploaded accidentally to test instruments.

### 2.2 Dynamic PlatformIO Board Catalog & Scaffolding Engine (`kopeng::electronics::firmware`)
- Implemented `list_platformio_boards(query)` in Rust daemon backed by a thread-safe `OnceLock<RwLock<Option<Vec<PlatformioBoard>>>>` cache. Populated dynamically via `pio boards --json-output` (1,656 boards across ESP32, STM32, AVR, RP2040, nRF52, etc.) with a built-in baseline catalog fallback.
- Implemented `scaffold_firmware_project(args)`:
  - Generates canonical `platformio.ini` with target platform, board ID, framework, and user-selected `monitor_speed` (e.g. 115200, 9600, 921600).
  - Scaffolds starter `src/main.cpp` (or `main.c`), `include/README`, and `lib/README`.
  - Automatically updates `rula.toml` to register firmware build and upload tasks for pair programming.
- Defined typed error `ElectronicsError::FirmwareScaffoldFailed` with `ApiError` code `FIRMWARE_SCAFFOLD_FAILED`.

### 2.3 Daemon Command Registration & TS Bindings Synchronization (Spec 12 §7.2)
- Registered daemon commands in `src-tauri/src/api.rs`:
  - `"list_platformio_boards"`
  - `"scaffold_firmware_project"`
- Mirrored commands in `DaemonCommands` (`src/types/daemonApi.ts`) and re-exported types in `src/types/index.ts`.
- Generated TypeScript types into `src/types/generated/`:
  - `PlatformioBoard.ts`
  - `ListPlatformioBoardsArgs.ts`
  - `ScaffoldFirmwareProjectArgs.ts`
  - `ScaffoldFirmwareResult.ts`
- Verified daemon command registry test `api_commands_match_the_frontend_map`.

### 2.4 Toolchain Diagnostics & Arch Linux Pacman Integration
- Updated `src-tauri/src/kopeng/diagnostic.rs` to map `"probe-rs" | "probe-rs-tools"` to pacman package `"probe-rs"`.
- Configured interactive privilege escalation hints (`pkexec pacman -S --noconfirm`) for all 5 electronics toolchains.

### 2.5 Frontend Scaffolding Wizard & Diagnostic Modal (`ProjectElectronicsTab.vue`)
- Decoupled `isPioCliInstalled` from `hasPlatformioConfig`. When the CLI is present but the project lacks `platformio.ini`, the UI presents a clean embedded scaffolding card.
- Implemented embedded scaffolding card with:
  - Live search input querying the 1,600+ board catalog.
  - Framework dropdown dynamically derived from the selected board's supported SDK frameworks.
  - Selectable monitor speed (baud rate: 9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600, 1000000, 2000000).
  - Language toggle (`C++` vs `C`).
  - One-click "Scaffold Firmware Project" button.
- Added top-bar `Toolchains: X/5` indicator opening an inspection modal displaying live binary paths, versions, and one-click pacman install buttons for all missing tools.

---

## 3. Verification & Validation

1. **Rust Tests:** `cargo test --manifest-path src-tauri/Cargo.toml` executed with 392 passed tests (0 failed).
2. **Daemon Protocol Consistency:** `api_commands_match_the_frontend_map` passed, confirming 1:1 parity between backend and frontend contracts.
3. **TypeScript / Vue Production Build:** `npm run build` (`vue-tsc --noEmit && vite build`) passed with zero errors.
4. **Specification Sync:** `docs/specs/31_electronics_workstation_and_instrumentation_engine.md` updated to v0.2.0 reflecting dynamic board catalog, scaffolding engine, instrument port exclusions, and toolchain modal.
