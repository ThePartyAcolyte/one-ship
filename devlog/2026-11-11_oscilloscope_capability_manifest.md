# Devlog: Oscilloscope Capability Manifest Architecture & Dynamic UI Rendering

**Date:** 2026-11-11  
**Author:** Bosmang (Lead Systems Architect & Engineer)  
**Topic:** Electronics Workstation / Instrument Driver Architecture / Manifest-Driven Interface

---

## 1. Context & Motivation

Following the user feedback regarding oscilloscope driver extensibility and UI coupling:
1. The oscilloscope interface previously relied on mock/placeholder entries (`Hantek 6022BE`, `SCPI Generic Scope`) in the frontend without real backend drivers, violating the *Anti-Mocking* and *Zero Dead Code* policies.
2. Channel parameters, stepper options (volts/div and timebase), trigger sources, and calibration actions were hardcoded in the frontend rather than being declared by the hardware driver itself.
3. To support arbitrary oscilloscopes (e.g. future 4-channel scopes, SCPI networked instruments, scopes with function generators / AWG), the architecture needed an explicit, declarative **Device Capability Manifest** (`OscilloscopeDescriptor`).

---

## 2. Implemented Architecture

### 2.1 Backend Capability Manifest (`OscilloscopeDescriptor`)
In `src-tauri/src/kopeng/electronics/instruments/driver.rs`:
- Defined `OscilloscopeDescriptor` deriving `ts-rs::TS` with `#[ts(export)]`.
- Followed Tiger Style with explicit unit suffixes (`_mv`, `_us`, `_hz`, `_count`).
- Captured hardware specifications:
  - Identification: `id`, `name`, `vendor`, `model`.
  - Channel layout: `channel_count`, `channel_names`, `volts_per_div_options_mv`, `couplings`, `probe_attenuations`.
  - Timebase & limits: `timebase_steps_us`, `min_timebase_us`, `max_timebase_us`, `max_sample_rate_hz`, `bandwidth_hz`, `buffer_points_count`.
  - Triggering: `has_trigger`, `trigger_sources`, `trigger_modes`, `trigger_slopes`.
  - Auxiliary hardware: `has_hardware_calibration`, `has_signal_generator`.

### 2.2 Driver Implementation & Daemon API
In `src-tauri/src/kopeng/electronics/instruments/owon_vds1022.rs`:
- Implemented `descriptor() -> OscilloscopeDescriptor` exposing Owon VDS1022 physical characteristics (2CH, 25MHz, 100MSa/s, 5mV-5V/div, CH1/CH2/EXT trigger, hardware calibration).
- Added `list_supported_oscilloscopes` to `instruments/mod.rs`.
- Registered `list_supported_oscilloscopes` in `api::API_COMMANDS` and `api::dispatch`.
- Mirrored in `src/types/daemonApi.ts` and verified by `api_commands_match_the_frontend_map`.

### 2.3 Store & UI Dynamic Binding
In `src/stores/electronicsStore.ts`:
- Added `supportedOscilloscopes`, `selectedScopeId`, and `activeScopeDescriptor`.
- Combined `scan_connected_instruments` and `list_supported_oscilloscopes` in `scanHardware()`.
- Dynamically derived `stepVoltsPerDiv` and `stepTimebase` from `activeScopeDescriptor`.
- Maintained store bounded size: strictly $\le 500$ lines (496 lines).

In `src/components/workspace/electronics/OscilloscopeView.vue`:
- Replaced static model array with dynamic `scopeModels` computed from `supportedOscilloscopes` (strictly zero placeholders).
- Bound trigger sources to `activeScopeDescriptor.triggerSources`.
- Gated hardware calibration button and modals with `v-if="activeScopeDescriptor?.hasHardwareCalibration"`.
- Elevated the Disconnected State Overlay to the root workstation container (`absolute inset-0 z-30`), covering canvas, FFT, channels (CH1/CH2), trigger and measurement sidebars.
- Disabled every action in the top toolbar when disconnected (Auto Set, Single/Dual Grid, Run, Single, Calibrate, Timebase stepper, Δt/ΔV cursors, Math, FFT, Meas, CSV/JSON, Agent Digest), keeping only the oscilloscope selector accessible.

---

## 3. Verification

- `cargo check --manifest-path src-tauri/Cargo.toml`: Finished with 0 errors.
- `cargo test --manifest-path src-tauri/Cargo.toml api_commands_match_the_frontend_map`: Passed 1/1.
- `npm run build`: `vue-tsc --noEmit` and `vite build` completed cleanly in 5.77s with 0 errors.
- Living Spec updated: `docs/specs/31_electronics_workstation_and_instrumentation_engine.md`.
