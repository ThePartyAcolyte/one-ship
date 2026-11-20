# Devlog: Oscilloscope State Persistence, Background Continuity, Auto-Adjust Suite & Accelerating Steppers

**Date:** 2026-11-10  
**Domain:** Embedded Instrumentation, Virtual Oscilloscope, Pinia State Architecture, UI Ergonomics, Signal Processing  
**Status:** Completed  

---

## 1. Problem Statement & Motivation

During hands-on lab pair programming sessions, several key user-experience and workflow limitations were identified:
1. **State Loss on Tab Navigation:** Switching between workspace tabs unmounted `OscilloscopeView.vue`, immediately stopping the acquisition loop and discarding all channel configurations (V/div, offsets, coupling, timebase, trigger levels, and cursor positions).
2. **Signal Clipping & Manual Tuning Friction:** Signals with significant DC bias or unexpected amplitude were clipped or scrolled off-canvas, requiring repetitive manual adjustments to find the waveform.
3. **Single-Click Stepper Fatigue:** Numeric fields (offset, trigger level, timebase) required individual mouse clicks for every step increment, making large adjustments slow and tedious.
4. **Channel Trace Overlap:** Viewing two large signals simultaneously on a single shared center grid created visual clutter.

---

## 2. Architectural Changes & Implementations

### 2.1 Domain-Bounded Pinia Store (`src/stores/electronicsStore.ts`)
- Implemented `useElectronicsStore` bounded to 444 lines (strictly within the $\le 500$ line bounded store rule).
- Lifted hardware state, channel configs (CH1/CH2), trigger, timebase, cursors, and display preferences into the store.
- Integrated automated `localStorage['oneship_oscilloscope_config']` synchronization for session persistence across application restarts.
- Centralized continuous acquisition (`RUN` loop) at the store layer. Switching workspace tabs no longer tears down the capture loop or resets waveform buffers.
- Exported `useElectronicsStore` via `src/stores/index.ts`.

### 2.2 Tactile Accelerating Steppers (`src/composables/usePressAndHold.ts`)
- Created `usePressAndHold` composable with `createHoldTrigger`.
- Configured with an initial delay of $350\text{ ms}$, starting interval of $150\text{ ms}$, and exponential acceleration factor $0.85$ down to a fast clamp of $25\text{ ms}$ (~40 steps/sec).
- Bound to channel offsets, vertical scale, horizontal timebase, trigger level, and cursors.

### 2.3 Auto-Adjust & Quick Calibration Suite
- **Master "AUTO SET":**
  - Evaluates active channels and computes target vertical scale $V_{\text{div}} \ge \frac{V_{pp}}{6}$.
  - Computes midpoint $V_{\text{mid}} = \frac{V_{\max} + V_{\min}}{2}$ (or $V_{\text{avg}}$) and adjusts offset to $-V_{\text{mid}}$ to snap waveform vertically onto the zero axis.
  - Automatically calculates ideal timebase division ($T_{\text{div}} \approx 0.3 \times \frac{1}{f}$) to frame 2–3 full cycles across the 10 screen divisions.
  - Aligns trigger level with the signal midpoint ($V_{\text{mid}}$) in Auto sweep mode.
- **Per-Channel Quick Actions:**
  - Added `[C]` (Auto-Center) button next to offset zero-reset (`[0]`).
  - Added `[Fit]` (Auto-Scale) button in the Volts/Div card header.

### 2.4 Split Screen (Dual Grid Display) Mode
- Added `[Dual Grid]` vs `[Single Grid]` toggle button in the oscilloscope top toolbar.
- Extended `drawOscilloscope()` in `OscilloscopeView.vue`:
  - **Single Grid (Overlaid):** Standard shared 8-division center grid.
  - **Dual Grid (Split Screen):** Splits the canvas horizontally into two independent $4 \times 10$ division viewports (top for CH1 with yellow accent, bottom for CH2 with cyan accent), each with its own zero reference line, ground markers, and clipping boundary, completely eliminating waveform overlap.

---

## 3. Verification & Validation

1. **Rust Backend Tests:** `cargo test --manifest-path src-tauri/Cargo.toml` passed cleanly with **393 passed tests, 0 failures**.
2. **Frontend Build:** `npm run build` (`vue-tsc --noEmit && vite build`) passed with **0 TypeScript and Vue errors**.
3. **Spec Alignment:** [Spec 31](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/31_electronics_workstation_and_instrumentation_engine.md) updated reflecting `useElectronicsStore`, accelerating steppers, auto-adjust calculations, and split-screen mode.
