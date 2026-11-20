# Devlog: Electronics & Embedded Lab — Debloat & Oscilloscope Horizontal Canvas Tooling

**Date:** 2026-11-10  
**Topic:** Embedded Lab Top Bar De-bloat, Actionable PlatformIO Installer (`pkexec pacman`), Distinct Oscilloscope Cursor Palettes & Bottom-of-Canvas Tooling  
**Author:** Antigravity (Pair Programming with Developer)  

---

## 1. Context & Motivation

During hands-on physical testing with the Owon VDS1022 oscilloscope and embedded microcontrollers, two major areas required UX and ergonomic simplification:
1. **Top Bar Clutter in `ProjectElectronicsTab.vue`:**
   - Redundant top header banner (`Electronics & Embedded Lab` title, subtitle, and PlatformIO badge) consumed vertical height on compact screens while stating obvious tab information.
   - Ambiguous global "Rescan Hardware" button was divorced from the specific context of microcontroller serial scanning versus instrument scanning.
   - The passive `Toolchains: PlatformIO, sigrok-cli, probe-rs, esptool, arduino-cli` banner displayed missing states without offering actionable resolution.
2. **Oscilloscope Color Palette Confusion:**
   - Time cursors (T1/T2) used Amber and Sky colors, visually clashing with CH1 Yellow (`#eab308`) and CH2 Cyan (`#06b6d4`).
   - Cursors and Timebase were buried in the right-hand front-panel drawer alongside vertical channel cards, forcing unnecessary vertical drawer scrolling.
   - Cursors and horizontal timebase are tools intrinsically tied to the canvas grid and belong in a dedicated horizontal bar directly beneath the waveform canvas.

---

## 2. Changes Implemented

### 2.1 Backend Pacman Package Resolver (`src-tauri/src/kopeng/diagnostic.rs`)
- Extended `resolve_pacman_package` to map electronics toolchains to Arch `extra` packages:
  - `"platformio"` | `"platformio-core"` | `"pio"` -> `"platformio-core"`
  - `"sigrok-cli"` -> `"sigrok-cli"`
  - `"sigrok-firmware-fx2lafw"` -> `"sigrok-firmware-fx2lafw"`
  - `"esptool"` -> `"esptool"`
  - `"arduino-cli"` -> `"arduino-cli"`
- Verified with unit tests (`cargo test --lib kopeng::diagnostic` passed 4/4 tests).

### 2.2 Embedded Lab De-bloat (`src/components/workspace/ProjectElectronicsTab.vue`)
1. **Elimination of Redundant Header & Passive Toolchain Banner:**
   - Deleted the top header toolbar (`Electronics & Embedded Lab` title/subtitle).
   - Deleted the passive `Toolchains:` banner.
   - Elevated the sub-navigation bar (`[Firmware & Serial] [Oscilloscope] [Logic Analyzer]`) to become the clean, immediate top bar of the tab.
2. **Contextual Rescan & Quick Access:**
   - Added a compact `[⟳]` icon button next to the instrument status pill (`VDS1022: Online / FX2LP: Online`) in the top bar.
   - Added a direct `[⟳]` button inside the `Target Microcontroller` card next to `Connected` port counter.
3. **Actionable PlatformIO Installer:**
   - When PlatformIO Core is absent, the `Firmware Toolchain` card renders an alert banner with an actionable `[ Install PlatformIO (pkexec pacman) ]` button.
   - Invokes `daemonCall('install_missing_tools', { toolIds: ['platformio'] })` with native Polkit graphical elevation on Omarchy Linux.

### 2.3 Oscilloscope Canvas & Front-Panel Architecture (`src/components/workspace/electronics/OscilloscopeView.vue`)
1. **Distinct, High-Contrast Cursor Color Palette:**
   - Time Cursors:
     - T1: **Purple / Violet** (`#a855f7`)
     - T2: **Pink / Magenta** (`#ec4899`)
   - Voltage Cursors:
     - V1: **Lime** (`#84cc16`)
     - V2: **Emerald / Mint** (`#10b981`)
   - Eliminates all visual ambiguity against CH1 Yellow (`#eab308`), CH2 Cyan (`#06b6d4`), and Trigger Orange (`#f97316`).
   - Propagated across canvas dashed lines, HUD overlays, and slider thumbs.
2. **Elimination of Elastic Wrapping & Zero-Layout-Shift Bottom Toolbar:**
   - Eliminated elastic flex wrapping and text-length reflow under the canvas.
   - Positioned in a rigid fixed-height bar (`h-10 border-t border-[var(--border-color)] bg-black/70 px-3 flex items-center gap-3`).
   - **Slot 1 (Timebase Stepper):** Fixed-width container (`w-48`) with `[-] [ timebase ] [+]` (cycling 1-2-5 decade ranges from $100\text{ ns}$ to $100\text{ ms/div}$).
   - **Slot 2 (Time Cursors):** Rigid slot (`flex-1 min-w-[280px]`) containing toggle `[ Time (Δt) ]`, T1/T2 sliders with fine nudge buttons (`[-]`, `[+]`) and zero-snap (`[0]`).
   - **Slot 3 (Voltage Cursors):** Rigid slot (`flex-1 min-w-[280px]`) containing toggle `[ Volt (ΔV) ]`, V1/V2 sliders with fine nudge buttons (`[-]`, `[+]`) and zero-snap (`[0]`).
   - **Permanent Presence Invariant:** When cursors are toggled OFF, they do not collapse or unmount; instead, they transition to `opacity-25 pointer-events-none`. This ensures zero layout reflow, element jump, or width resize during toggle operations.
   - **HUD Measurement Relocation:** Redundant slider readout text (e.g. `250µs`, `-1000µs`, `ΔV: 3.3V`) was stripped from the bottom toolbar because all measurements are rendered dynamically in the high-contrast Canvas Overlay HUD, eliminating character-count jitter when dragging sliders.
3. **Channel Real-Time Metrics Migration to Right Drawer (4 Columns x 2 Rows):**
   - Removed the left vertical metrics strip (`w-28`), allowing the oscilloscope canvas to expand across 100% of the center workstation width.
   - Integrated the 4 real-time scalar metrics ($V_{pp}, V_{rms}$, frequency, duty cycle) directly into the bottom of each channel's front-panel card (CH1 and CH2).
   - Structured as a rigid 4-column $\times$ 2-row grid (`grid grid-cols-4 gap-1 pt-2 border-t font-mono text-center`):
     - Top row: Compact uppercase metric labels (`Vpp`, `Vrms`, `Freq`, `Duty`).
     - Bottom row: High-contrast tabular numbers (`text-white`, `text-emerald-400`, `tabular-nums truncate`). When a channel is toggled OFF, values cleanly dim to `---` (`text-zinc-600`).
4. **Clean Front-Panel Drawer (Right Column):**
   - Column hosts exclusively:
     - Channel 1 Controls Card (Power toggle, 1X/10X, DC/AC/GND, V/div stepper, Offset stepper, 4x2 metrics grid)
     - Channel 2 Controls Card (Power toggle, 1X/10X, DC/AC/GND, V/div stepper, Offset stepper, 4x2 metrics grid)
     - Trigger Controls Card (Sweep Mode, Source, Slope, Level stepper)
   - Zero unnecessary scrolling required to operate standard channels and triggers.
5. **Top Toolbar Cleanup:**
   - Removed redundant cursor toggles from top toolbar; top bar focuses strictly on device status, run/stop/single acquisition, calibration, and agent metrics export.

---

## 3. Verification

1. **Rust Diagnostics & Package Mapping Test:**
   - Ran `cargo test --manifest-path src-tauri/Cargo.toml --lib kopeng::diagnostic`:
     - `packages_are_deduplicated_and_unknown_or_empty_requests_rejected ... ok`
     - `a_missing_program_is_absent_not_an_error ... ok`
     - `export_bindings_diagnosticstate ... ok`
     - `export_bindings_installmissingtoolsargs ... ok`
2. **Frontend TypeScript & Vue Compilation:**
   - Ran `npm run build` (`vue-tsc --noEmit && vite build`):
     - Built cleanly with 0 type errors in 51.33s.
3. **Ergonomic Integrity:**
   - Layout is now balanced and uncluttered.
   - Canvas-specific controls (Timebase and Cursors) sit adjacent to the canvas.
   - Hardware front-panel cards (CH1, CH2, Trigger) sit neatly in the right drawer without overflowing.
