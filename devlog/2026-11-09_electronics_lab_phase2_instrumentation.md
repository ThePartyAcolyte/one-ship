# Devlog: Electronics & Embedded Lab — Phase 2: Physical Instrumentation Engine

**Date:** 2026-11-09  
**Topic:** Physical Test Instrumentation, Native Owon VDS1022 USB Oscilloscope Driver, FX2LP 24MHz Logic Analyzer & Interactive Digital Workstation  
**Author:** Antigravity (Pair Programming with Developer)  

---

## 1. Context & Motivation

Physical test instrumentation is essential for hardware validation, protocol debugging, and closing the physical-to-digital loop. While software compiling and flashing (Phase 1) controls firmware input to microcontrollers, physical instrumentation measures and verifies the actual electrical and digital outputs of the circuit.

Phase 2 establishes the physical instrumentation infrastructure of the ONE-SHIP Electronics & Embedded Lab:
1. **Native Rust Device Drivers:** Built directly in Rust using `rusb` and `libudev` rather than fragile multi-runtime Python or Java wrappers.
2. **Owon VDS1022 Oscilloscope Support:** Direct USB communication with the vendor's bulk endpoints (`0x02` OUT, `0x86` IN) with automated calculation of scalar metrics ($V_{pp}$, $V_{rms}$, frequency, duty cycle, $V_{min}$, $V_{max}$).
3. **Chinese Generic 24MHz 8-Channel Logic Analyzer Support:** Integrated Cypress FX2LP controller via `sigrok-cli` / `libsigrok` protocol decoder pipelines, producing both raw timing streams and standard WaveDrom JSON waveforms.
4. **Hardware Lock Arbitrator:** Enforces deterministic concurrency control and lease expiration preventing concurrent write collisions across human UI interactions and autonomous agent tasks.
5. **Dual-Actor Paradigm:** Human engineers get visual control knobs, interactive cursors ($\Delta t, 1/\Delta t, \Delta V$), zoom/pan digital traces, and protocol packet tables; autonomous AI agents query token-efficient scalar digests (`HardwareDigest`).

---

## 2. Changes Implemented

### 2.1 Backend (`src-tauri/`)
1. **Dependencies (`Cargo.toml`):**
   - Added `rusb = "0.9"` for direct USB device communication.
2. **Instrumentation Subsystem (`src-tauri/src/kopeng/electronics/instruments/`):**
   - `driver.rs`: Canonical hardware definitions and ts-rs exported contracts (`DiscoveredInstrument`, `OscilloscopeChannelConfig`, `OscilloscopeTriggerConfig`, `OscilloscopeMetrics`, `OscilloscopeCaptureResult`, `DecodedProtocolPacket`, `LogicCaptureResult`).
   - `arbitrator.rs`: `HardwareLockArbitrator` implementing poison-resistant mutex locks with lease timeouts.
   - `owon_vds1022.rs`: Native USB driver communicating with Owon VDS1022 / VDS1022I (VID `0x5345`, PID `0x1234`). Implements bulk transfer capture, realistic fallback emulation when unplugged, and automated electrical metric extraction (`compute_channel_metrics`).
   - `fx2lafw.rs`: Driver for Chinese generic 24MHz 8CH logic analyzer (VID `0x0925`, PID `0x3881`) orchestrating `sigrok-cli --driver fx2lafw` captures, protocol decoders (I2C, SPI, UART), and WaveDrom formatting.
   - `mod.rs`: Top-level router providing `scan_connected_instruments`, `capture_oscilloscope`, and `capture_logic_analyzer`.
3. **Daemon RPC & API Invariant (`src-tauri/src/api.rs`):**
   - Registered 3 new daemon RPC commands:
     - `scan_connected_instruments`
     - `capture_oscilloscope`
     - `capture_logic_analyzer`
   - Mirrored all 3 in `DaemonCommands` (`src/types/daemonApi.ts`), satisfying the 156-command verification test `api_commands_match_the_frontend_map`.

### 2.2 Frontend (`src/`)
1. **Interactive Oscilloscope View (`src/components/workspace/electronics/OscilloscopeView.vue`):**
   - Responsive HTML5 `<canvas>` rendering 10x8 divisions oscilloscope grid with sub-ticks and center crosshairs.
   - Dual-channel rendering: CH1 (Yellow `#eab308`) and CH2 (Cyan `#06b6d4`) with adjustable V/div, coupling (DC/AC/GND), probe attenuation (1X/10X), and vertical offset.
   - Trigger marker and horizontal timebase selector ($100\text{ns}$ to $100\text{ms}$).
   - Interactive Cursors: Time cursors (T1, T2) measuring $\Delta t$ and $1/\Delta t$ frequency; Voltage cursors (V1, V2) measuring $\Delta V$.
   - Real-time electrical measurements HUD ($V_{pp}, V_{rms}$, freq, duty cycle).
   - "Agent Digest" inspector displaying the exact token-optimized scalar JSON stream consumed by LLM agents.
2. **Interactive Logic Analyzer View (`src/components/workspace/electronics/LogicAnalyzerView.vue`):**
   - 8 digital trace tracks (D0–D7) with channel color coding, state indicators, and horizontal scroll wheel zoom & pan.
   - Interactive hover cursor inspecting sample index, timestamp in $\mu\text{s}$, and pin states across all 8 pins simultaneously.
   - Protocol Decoder Bar for I2C, SPI, and UART with configurable pin assignments.
   - Decoded Protocol Stream packet inspector table with timestamps, protocol badges, summaries, and payload bytes.
   - WaveDrom JSON modal viewer with one-click clipboard copy and CSV export.
3. **Sub-Navigation & Instrument Status (`src/components/workspace/ProjectElectronicsTab.vue`):**
   - Added persistent sub-navigation bar:
     - `[Firmware & Serial]`
     - `[Oscilloscope (VDS1022)]`
     - `[Logic Analyzer (24MHz)]`
   - Instrument status indicators showing online/offline status for VDS1022 and FX2LP devices.

---

## 3. Verification & Results

1. **Host Environment & Hardware Permissions:**
   - Installed `/etc/udev/rules.d/99-oneship-instruments.rules` for Owon VDS1022 (`5345:1234`) and FX2LP (`0925:3881`, `04b4:8613`) with `MODE="0666", TAG+="uaccess"`.
   - Verified non-root direct read-write access to `/dev/bus/usb/003/004`.
   - Resolved bulk transfer endpoints to `0x01` (OUT) and `0x81` (IN) with dynamic config descriptor fallback.
   - Synchronized frontend instrument IDs (`owon_vds1022` and `fx2lafw_8ch`).
2. **Physical Oscilloscope Protocol Discovery & Frame Extraction:**
   - Validated native USB command packets: `SET_CHL_ON` (`0x0b`), `SET_RUNSTOP` (`0x61`), and `GET_DATA` (`0x1000`).
   - Captured and parsed the canonical **5,211-byte waveform frame** directly from the physical Owon VDS1022.
   - Sliced 5,000 screen display ADC samples from bytes `[161..5161]` (50 pre-trigger + 5000 screen + 50 post-trigger).
   - Calibrated raw 8-bit counts (centered at 128) into millivolts (`(volts_per_div_mv / 25.0) * probe_attenuation`).
   - Offloaded blocking USB bulk I/O to `tokio::task::spawn_blocking` to preserve Tokio async runtime purity and satisfy Axum `Send + 'static` handler invariants.
3. **High-DPI HTML5 Canvas Rendering Fix:**
   - Fixed Canvas Device Pixel Ratio (`dpr`) scaling bug in `OscilloscopeView.vue`: explicit `ctx.setTransform(dpr, 0, 0, dpr, 0, 0)` with logical dimensions (`canvas.width / dpr`) guarantees that center crosshairs, sub-ticks, GND markers, and waveform lines render crisply without being displaced off-screen.
   - Added brutalist Capture Error HUD alert banner for instant visibility of any hardware/transport exceptions.
4. **Rust Backend Tests:**
   - `cargo test --lib kopeng::electronics`: All 20 tests passed (metric calculations, frequency detection, USB probe scan, contracts).
   - `cargo test --lib api_commands_match_the_frontend_map`: All 156 daemon commands validated with 1:1 match.
5. **Frontend Type Checking & Bundle:**
   - `npm run build` (`vue-tsc --noEmit && vite build`): Succeeded cleanly with 0 TypeScript errors.

---

## 4. Hardware Milestone: Cold FPGA Bootstrap, Calibration Engine & Physical Waveform Verification

1. **Automatic Cold FPGA Bitstream Injection:**
   - Implemented `ensure_fpga_loaded()` querying FPGA state via `QUERY_FPGA` (`0x0223`).
   - If cold/uninitialized (`0`), parses Flash version string (`0x01b0` address `0x08` -> `V5.0.1` Gowin V5) and streams `src-tauri/resources/firmware/owon/VDS1022_FPGAV5_gaoyun.bin` via `LOAD_FPGA` (`0x4000`) in 10,240-byte chunks.
2. **Flash Calibration Table Caching:**
   - Resolved USB bus contention (`Input/Output Error`) caused by accessing Flash while the FPGA is sampling.
   - Cached `GAIN`, `AMPL`, and `COMP` calibration tables across all 10 voltage attenuation ranges in an atomic `FLASH_CALIBRATION_CACHE` static Mutex.
3. **Register Bank & Coupling Precision:**
   - Corrected coupling bits: `AC = 0`, `DC = 1` (`1 << 5`), `GND = 2` (`2 << 5`).
   - Implemented 100MHz clock prescaler divisor $N = \text{round}(100\text{ MSa/s} / \text{Target Rate})$.
   - Configured full register set: `SET_DEEPMEMORY` (5100), `SET_PRE_TRG` (2550), `SET_SUF_TRG` (2550), `SET_TRIGGER` (Auto, Rising, CH1), `SET_TRG_HOLDOFF` (`0x8002`), `SET_EDGE_LEVEL`, `SET_ZERO_OFF`, and `SET_VOLT_GAIN`.
   - Added a 200ms relay settling delay after channel switching.
4. **Physical Capture & Calibration Verification on Real Hardware:**
   - Connected CH1 probe to the 1 kHz physical calibration terminal on Owon VDS1022.
   - Physical test `test_physical_capture_vds1022`:
     - Frame header: `chl=0, time_sum=16800000, period_num=168, cursor=5108`.
     - **CH1 Vpp: 4920.0 mV, Frequency: exactly 1000.0 Hz, Vavg: 2445.0 mV** (both FPGA hardware counter and zero-crossing DSP read 1000.0 Hz).
   - Probe check test `test_physical_calibrate_probe_check`:
     - Succeeded: `1kHz calibration signal verified: 1000.0 Hz, 3120 mV Vpp, 49.9% duty cycle`.
     - Trimmer advice: `Probe is optimally compensated: Waveform has sharp vertical transitions and flat horizontal plateaus.`
5. **Interactive Calibration UI Modal:**
   - Added `[Calibrate]` action button in the Oscilloscope top toolbar.
   - Built dual-tab modal with Probe Compensation (1 kHz test pin evaluation, nominal frequency/amplitude/duty metrics, trimmer advice) and Zero Baseline offset alignment.
   - Synchronized ts-rs types, daemon RPC registration (`calibrate_oscilloscope`), and Spec 31.
