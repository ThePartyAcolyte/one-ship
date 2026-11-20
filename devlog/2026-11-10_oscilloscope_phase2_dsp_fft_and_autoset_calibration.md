# Development Log: Oscilloscope Auto-Set Calibration & Phase 2 Laboratory Capabilities

**Date:** 2026-11-10  
**Domain:** Embedded Instrumentation, Digital Signal Processing, Oscilloscope Hardware Driver, Spectrum Analysis  
**Specification Reference:** [Spec 31](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/31_electronics_workstation_and_instrumentation_engine.md)

---

## 1. Objectives & Context
1. **Diagnosis & Resolution of Auto Set Double-Offset Glitch:**
   - Eliminate the 1-second centering followed by downward jump where waveform crests landed on `centerY`.
   - Restore true physical voltage invariance across hardware analog offset adjustments.
2. **Implementation of Phase 2 Advanced Laboratory Capabilities:**
   - **Math Channel Operations:** Vector differential ($CH1 - CH2$), summation ($CH1 + CH2$), and inversion ($INV\_CH1$).
   - **Real-Time FFT (Spectrum Analyzer):** Radix-2 Cooley-Tukey FFT with Hanning window and peak frequency detection.
   - **Extended Parametric Measurements Dashboard:** Calculation of $V_{\text{top}}$, $V_{\text{base}}$, Rise Time ($t_r$), Fall Time ($t_f$), and Phase Difference ($\Delta \Phi$).
   - **Waveform Data Export:** Instant CSV and JSON export pipelines for captured 5,000-point frames.

---

## 2. Root Cause Analysis: Hardware vs. Software Double-Offset
- In `owon_vds1022.rs`, FPGA register `0x010a` (`SET_ZERO_OFF`) shifts the physical analog DAC before ADC quantization.
- ADC bytes `b as i8` were scaled to mV without subtracting `vertical_offset_mv`.
- Consequently, when `autoCenterChannel` set `offsetMv = -1650`, the frontend initially centered the previous frame. Upon arrival of the next USB frame, the hardware had already shifted the samples down by $1650\text{ mV}$. The frontend then added `offsetMv` a second time:
  $$y = centerY - \frac{(V_{\text{in}} - 1650) + (-1650)}{V_{\text{div}}} \times dy = centerY - \frac{V_{\text{in}} - 3300}{V_{\text{div}}} \times dy$$
  The top crest ($3300\text{ mV}$) was mapped to $0\text{ mV}$ (`centerY`).
- **Fix:** In `owon_vds1022.rs`, raw samples are calibrated to absolute input voltage $V_{\text{in}} = ((b \text{ as } i8) \times \text{scale}) - \text{vertical\_offset\_mv}$. All computed metrics ($V_{\text{avg}}, V_{\max}, V_{\min}, V_{\text{rms}}$) and UI renderings are now invariant to screen offset shifts and perfectly idempotent.

---

## 3. Implementation Details

### 3.1 Dedicated DSP Utility Module (`src/utils/electronicsDsp.ts`)
- To preserve the $\le 500$ line limit on Pinia stores (Spec Anti-God-Store Policy), all mathematical DSP routines were implemented in an isolated TypeScript module:
  - `computeMathTrace(ch1, ch2, op)`
  - `computeFftSpectrum(samples, sampleRateHz, 1024)`: Cooley-Tukey Radix-2 in-place bit-reversal FFT with Hanning windowing $w(n) = 0.5 \times (1 - \cos(\frac{2\pi n}{N-1}))$ and peak detection.
  - `computeExtendedMetrics(samples, timeUs, ch2Samples)`: Computes $V_{\text{top}}, V_{\text{base}}$, $10\% \to 90\%$ Rise Time, $90\% \to 10\%$ Fall Time, and cross-channel Phase Difference $\Delta \Phi$.
  - `exportCaptureCsv(ch1, ch2, timeUs)` and `exportCaptureJson(capture)`.

### 3.2 Pinia Store Update (`src/stores/electronicsStore.ts`)
- Added reactive properties: `mathEnabled`, `mathOp`, `showFft`, and `showExtendedMetrics`.
- Hydrated and persisted to `localStorage['oneship_oscilloscope_config']`.
- Line count strictly maintained at 490 lines ($\le 500$ ceiling).

### 3.3 Frontend Instrumentation View (`OscilloscopeView.vue`)
- **Top Toolbar:** Added brutalist toggle buttons for Math Channel (`Sigma` icon), Real-Time FFT (`BarChart2` icon), Parametric Metrics (`Meas`), and direct `[CSV]` / `[JSON]` export triggers.
- **Canvas Math Rendering:** Renders bright fuchsia (`#d946ef`) trace with `'M'` ground reference marker.
- **FFT Spectrum Analyzer Canvas:** Dockable 160px spectrum analyzer displaying dBV grid (-80 to 0 dBV), frequency decades (kHz/MHz), and annotated peak frequency.
- **Extended Measurements Card:** Displays real-time parametric pulse metrics and phase relationship in the right sidebar.

---

## 4. Verification & Testing
- `cargo check --manifest-path src-tauri/Cargo.toml`: Success (0 errors, 4.96s).
- `cargo test --manifest-path src-tauri/Cargo.toml test_compute_channel_metrics_square_wave`: Passed (1 passed, 0 failed).
- `npm run build`: Success (`vue-tsc --noEmit` clean, Vite bundle generated in 4.75s).
- Spec 31 updated and synchronized.

---

## 5. Runtime Remediation: Stepper Events, Relay Thrashing & Layout Containment
1. **Vue 3 Event Handler Name Resolution in Steppers:**
   - In `usePressAndHold.ts`, bindings were defined with prefix `onPointerdown`, causing Vue's `toHandlers()` to listen for `onOnPointerdown`.
   - Corrected keys to `pointerdown`, `pointerup`, `pointerleave`, `pointercancel`, `keydown`, and `keyup`. All stepper buttons (`+`/`-`) now respond instantly to single clicks and accelerating press-and-hold.
2. **Hardware Relay Thrashing Elimination (`LAST_APPLIED_CONFIG`):**
   - In `owon_vds1022.rs`, cached applied hardware parameters `(ch1_cfg, ch2_cfg, trigger_cfg, timebase_key)`.
   - Continuous `RUN` captures bypass register reconfiguration and the $150\text{ ms}$ settling sleep when settings have not changed. Eliminates mechanical relay clicking and boosts FPS from $1\text{ FPS}$ to $20\text{--}30\text{ FPS}$.
3. **Auto Set Reactivity Decoupling:**
   - Decoupled `[Auto Set]` button disabling from continuous acquisition loop `isCapturing`. Introduced dedicated `isAutoSetting` flag with spinner, eliminating button flickering and permitting on-the-fly auto-tuning during active acquisition.
4. **FFT Viewport Containment & Layout Reordering:**
   - Relaxed canvas container from `min-h-[300px]` to `min-h-0` with `flex-1`.
   - Reordered DOM layout: moved FFT spectrum analyzer (`h-32`) below the fixed bottom toolbar (Timebase and Cursors). The main oscilloscope canvas compresses vertically cleanly via `flex-1 min-h-0`, keeping the timebase and cursor controls directly attached to the bottom edge of the oscilloscope display.
5. **Interactive Canvas Cursor Drag & Drop:**
   - Implemented pointer event tracking (`pointerdown`, `pointermove`, `pointerup`, `pointercancel`, `pointerleave`) on the main canvas with pointer capture.
   - Added hit-testing with a $10\text{ px}$ proximity threshold for horizontal time cursors ($T1, T2$) and vertical voltage cursors ($V1, V2$).
   - Dynamic CSS cursor feedback (`ew-resize` for time, `ns-resize` for voltage, `crosshair` in idle state).
   - Real-time conversion between canvas pixel coordinates and physical instrumentation units ($\mu\text{s}$ and $\text{mV}$), directly mutating the reactive store with 60 FPS updates on both HUD measurements and slider inputs.
   - Enhanced cursor visual rendering: solid lines with $2.0\text{ px}$ stroke during hover/drag, dashed lines when idle, and high-contrast handle badges on the canvas edges.
6. **UI Minimalist Consolidation & Bottom Toolbar Removal:**
   - Added `@dblclick` handler on canvas to reset cursor lines ($T1, T2, V1, V2$) directly to $0$ on double-click.
   - Completely removed redundant fixed bottom toolbar (`h-10`), freeing maximum vertical canvas space.
   - Migrated horizontal Timebase stepper `[-] {timebase} [+]` and symbol-only cursor toggles (`Δt`, `ΔV`) to the top header toolbar.
   - Streamlined all primary top-bar action buttons (Auto Set, Single/Dual Grid, Run/Stop, Single, Calibrate) to minimalist symbol-only icons per AGENTS.md rules 5.3 & 5.5, with full explanatory `AppTooltip` hover tags.

