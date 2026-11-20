# Devlog: Electronics & Embedded Lab — Oscilloscope Ergonomics & Hardware Channel Gating

**Date:** 2026-11-10  
**Topic:** Owon VDS1022 FPGA Hardware Channel Gating, Physical Test-Bench Tactile Controls, Jitter-Free Cursors & Ultra-Compact Vertical Metrics Rail  
**Author:** Antigravity (Pair Programming with Developer)  

---

## 1. Context & Motivation

Following physical validation with the Owon VDS1022 oscilloscope connected to the 1 kHz hardware calibration pin, key ergonomic and behavioral refinements were required to transition the interface from generic desktop form controls into a tactile, authentic test-bench instrument:
1. **Hardware Channel Deactivation Bug:** When disabling a channel (e.g. unchecking CH2), the channel continued to stream measurements. Deserialization in `capture_oscilloscope` fell back to `OscilloscopeChannelConfig::default()` (`enabled: true`), unintentionally forcing the FPGA into two-channel mode on every acquisition.
2. **Elimination of Software Dropdowns:** Front-panel controls (Volts/Div, Timebase/Div, Coupling, Probe Attenuation, Trigger Source/Slope/Sweep) were previously implemented using standard software `<AppSelect>` dropdowns, which felt disconnected from physical instrument ergonomics and cluttered compact displays.
3. **Left Rail Proportions & Neutral Dimming:** The original `w-52` left rail consumed too much horizontal canvas space and left empty vertical gaps. Furthermore, disabled channels produced odd background contrast rather than a clean, neutral, desaturated dimming.
4. **Integrated Cursor Controls:** Cursors needed to be permanently visible and independently controllable inside the drawer, mirroring the channel power switches.

---

## 2. Changes Implemented

### 2.1 Backend Hardware Channel Gating (`src-tauri/src/kopeng/electronics/instruments/`)
1. **`mod.rs` Safe Deserialization Fallback:**
   - Updated `capture_oscilloscope` so missing channel parameters unwrap to `OscilloscopeChannelConfig { enabled: false, ..Default::default() }`.
2. **`owon_vds1022.rs` FPGA Gating & Early Return:**
   - Early return guard: if both CH1 and CH2 are disabled, the driver returns an empty `OscilloscopeCaptureResult` immediately without waking the FPGA or issuing USB transfers.
   - Slices samples and calculates scalar metrics ($V_{pp}, V_{rms}$, frequency, duty cycle) strictly for enabled channels; disabled channels return `Vec::new()` and `OscilloscopeMetrics::default()`.

### 2.2 Frontend Physical Test-Bench Refactor (`OscilloscopeView.vue`)
1. **Ultra-Compact Left Metrics Rail (`w-28` / 112px):**
   - Replaced bulky cards with a slim, space-efficient vertical strip displaying miniature chips for $V_{pp}, V_{rms}$, frequency, and duty cycle.
   - Active CH1 renders in Yellow (`#eab308`), active CH2 in Cyan (`#06b6d4`).
   - Disabled channels cleanly desaturate into neutral dark gray (`border-zinc-800 bg-zinc-900/30 text-zinc-600`) with placeholder dashes (`---`), eliminating color conflicts.
2. **Channel Control Modules (Two Compact Sections):**
   - **Section 1: Hardware State & Coupling Header:**
     - Channel power toggle: `[● CH1]` / `[● CH2]`.
     - Direct 2-position segmented toggle: `[ 1X | 10X ]` (removed verbose "Probe" label).
     - Direct 3-way segmented button pill: `[ DC | AC | GND ]` (eliminated dropdown).
   - **Section 2: Tactile Numerical Steppers:**
     - Volts/Div: Stepper `[-] [ 1 V/div ] [+]` cycling standard 1-2-5 decade ranges ($5\text{ mV}$ to $5\text{ V/div}$). Completely eliminated `<AppSelect>`.
     - Vertical Offset: Stepper `[-] [ 0 mV ] [+] [0]` with instant zero-snap.
3. **Horizontal Timebase Stepper:**
   - Replaced dropdown with tactile hardware stepper: `[-] [ 1 ms/div ] [+]`, cycling 1-2-5 decade ranges from $100\text{ ns}$ to $100\text{ ms/div}$.
4. **Trigger System Hardware Switches:**
   - Sweep Mode: 3-way segmented pill `[ Auto | Normal | Single ]`.
   - Source: 3-way segmented pill `[ CH1 | CH2 | EXT ]`.
   - Slope: 2-way toggle pill `[ ↗ | ↘ ]`.
   - Level: Stepper `[-] [ 0 mV ] [+] [0]` (50 mV steps with zero-snap).
5. **Always-Visible Cursors Panel with Independent Toggles:**
   - Permanently rendered in the right controls drawer.
   - Independent sub-section toggles:
     - `[● Time Cursors]` -> exposes T1, T2 range sliders with `[-] [slider] [+] [0] [value]`, and live $\Delta t, 1/\Delta t$ readout.
     - `[● Volt Cursors]` -> exposes V1, V2 range sliders with `[-] [slider] [+] [0] [value]`, and live $\Delta V$ readout.
   - Synchronized bidirectionally with the top toolbar toggles.

---

## 3. Verification

1. **Rust Backend Check:**
   - `cargo check --manifest-path src-tauri/Cargo.toml`: Clean exit code 0.
2. **TypeScript & Vue Bundle Verification:**
   - `npm run build` (`vue-tsc --noEmit && vite build`): Succeeded in 4.34s with 0 type errors.
3. **Hardware & Layout Stability:**
   - Waveform screen canvas area significantly enlarged horizontally.
   - All interactive controls now behave as stepped physical hardware controls.
