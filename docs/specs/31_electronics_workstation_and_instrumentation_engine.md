# Spec 31: Electronics Engineering Workstation & Hardware Instrumentation Engine

**Document Version:** 0.2.0  
**Domain:** Embedded Systems, Native USB Instrumentation, Oscilloscopes, Logic Analyzers, EDA Schematic Ingestion, SPICE Simulation  
**Status:** Specified & Operational  
**Maintainer:** Bosmang (Lead Systems Architect & Engineer)  

---

## 1. Overview & Architectural Motivation

Modern software engineering increasingly overlaps with embedded systems, IoT development, and custom hardware prototyping. However, developers and autonomous AI agents typically operate in disconnected silos:
1. **Firmware code** is edited in an IDE with no semantic awareness of the underlying physical schematic or pin connections.
2. **Physical test instruments** (oscilloscopes, logic analyzers) run in separate, isolated proprietary vendor GUIs, inaccessible to AI agents and difficult to correlate with running firmware.
3. **Hardware debugging** requires manual copy-pasting of pin configurations and trial-and-error measurement validation.

**Spec 31** specifies the **ONE-SHIP Electronics Engineering Workstation** (`kopeng::electronics`), transforming ONE-SHIP into a hardware-in-the-loop development platform.

### 1.1 The Dual-Actor Philosophy (Human + Agent Pair Programming)

Every electronics feature in ONE-SHIP is architected for dual actors:
- **The Human Engineer:** Needs a responsive, tactile visual interface—virtual oscilloscope knobs, voltage/timebase adjustments, trigger controls, interactive digital waveform tracks with zoom/pan cursors, and circuit schematic navigation.
- **The AI Coding Agent:** Cannot consume megabytes of raw analog point arrays or click visual knobs. Agents require **strongly-typed MCP tools** returning **token-efficient Hardware Digests** (scalar metrics, decoded protocol transactions, anomaly warnings, and pinout tables) to diagnose hardware bugs autonomously.

```text
+--------------------------------------------------------------------------------------------------+
|                                    ONE-SHIP ELECTRONICS WORKSPACE                                |
+--------------------------------------------------------------------------------------------------+
|                                                                                                  |
|   +-----------------------------+                     +--------------------------------------+   |
|   |   HUMAN USER INTERFACE      |                     |      AUTONOMOUS AI AGENT             |   |
|   | - Virtual Oscilloscope UI   |                     | - MCP Tools (read_scope, capture_la) |   |
|   | - 8-Ch Logic Analyzer UI    |                     | - Token-Efficient Hardware Digests   |   |
|   | - KiCanvas Schematic Viewer |                     | - Firmware Pinout Verification       |   |
|   +-----------------------------+                     +--------------------------------------+   |
|                 |                                                        |                       |
|                 +---------------------------+----------------------------+                       |
|                                             |                                                    |
|                                             v                                                    |
|                   +--------------------------------------------------+                           |
|                   |   ONE-SHIP DAEMON ENGINE (`kopeng::electronics`) |                           |
|                   |   - Hardware Lock & Resource Arbitrator          |                           |
|                   |   - Native Rust USB Drivers (Owon VDS1022)       |                           |
|                   |   - sigrok / fx2lafw Engine (24MHz 8-Ch LA)      |                           |
|                   |   - Fusion 360 / KiCad EDA XML/S-Expr Parser     |                           |
|                   |   - Headless ngspice SPICE Simulation Runner     |                           |
|                   +--------------------------------------------------+                           |
|                                             |                                                    |
|                 +---------------------------+----------------------------+                       |
|                 |                           |                            |                       |
|                 v                           v                            v                       |
|        [Physical Hardware]         [Firmware Toolchain]        [Project EDA Files]               |
|        - Owon VDS1022 (USB)        - PlatformIO Core / pio     - Fusion 360 (.sch / .brd)        |
|        - 24MHz 8ch LA (fx2lafw)    - probe-rs (SWD/JTAG/RTT)   - KiCad 6+ (.kicad_sch / _pcb)    |
|        - MCU Serial (/dev/ttyUSB*)                                                               |
+--------------------------------------------------------------------------------------------------+
```

---

## 2. Supported Hardware Baseline & Native Rust Driver Architecture

Per ONE-SHIP engineering principles, all instrument drivers and hardware interfaces **prioritize native Rust implementations** using memory-safe asynchronous USB abstractions (`rusb`) rather than heavy Java runtimes or external interpreted wrappers.

### 2.1 Hardware Integration Matrix

| Instrument Category | Physical Hardware Target | Chipset / Architecture | Driver Strategy | Real-Time Output |
| :--- | :--- | :--- | :--- | :--- |
| **Digital Oscilloscope** | **Owon VDS1022 / VDS1022I** | Cypress FX2 USB + FPGA (25MHz bandwidth, 100MSa/s real-time, 2 analog channels + multi-channel trigger) | **Native Rust Driver (`rusb`)** reverse-engineered from device USB protocol packets | 5,000 raw samples/channel, $V_{pp}$, $V_{rms}$, frequency, period, min/max |
| **Logic Analyzer** | **Generic 24MHz 8-Channel** | Cypress FX2LP (`CY7C68013A`) | **sigrok Engine (`sigrok-cli` / `libsigrok`)** with open-source `fx2lafw` firmware | 8 digital channels, WaveDrom JSON, Google Trace Event JSON, 100+ protocol decoders |
| **Microcontroller Flasher / Debugger** | **ST-Link, J-Link, CMSIS-DAP, ESP-Prog, USB-Serial** | ARM Cortex-M, RISC-V, ESP32, AVR | **probe-rs** (Rust native) + **PlatformIO Core (`pio`)** | Firmware binaries, memory flashing, bidirectional **RTT** and UART serial logs |

---

### 2.2 Native Rust Driver for Owon VDS1022 (`rusb`)

The Owon VDS1022 communicates via USB bulk endpoints using a proprietary FPGA command register set. The driver is implemented natively in Rust under `src-tauri/src/kopeng/electronics/instruments/owon_vds1022.rs` using `tokio::task::spawn_blocking` to execute low-level USB I/O without blocking the Tokio runtime.

#### A. USB Endpoints & Device Identification
- **USB Vendor ID (VID):** `0x5345`
- **USB Product ID (PID):** `0x1234`
- **Endpoint IN (`0x81`):** Bulk IN endpoint for command replies and waveform sample data frames.
- **Endpoint OUT (`0x01`):** Bulk OUT endpoint for command and register configuration packets.
- **Linux Udev Permissions:** `/etc/udev/rules.d/99-oneship-instruments.rules` sets `MODE="0666", TAG+="uaccess"`.

#### B. Cold FPGA Bootstrap & Flash Calibration Cache
1. **FPGA Bitstream Injection (`ensure_fpga_loaded`):**
   - The driver queries FPGA state via `QUERY_FPGA` (`0x0223`).
   - If uninitialized (`0`), it inspects the hardware version string via `READ_FLASH` (`0x01b0`) at address `0x08` (e.g. `V5.0.1` indicates Gowin V5).
   - Bundled bitstream `src-tauri/resources/firmware/owon/VDS1022_FPGAV5_gaoyun.bin` is streamed across USB Bulk OUT via `LOAD_FPGA` (`0x4000`) in 10,240-byte chunks.
2. **Flash Calibration Memory Cache (`FLASH_CALIBRATION_CACHE`):**
   - Reading Flash after the FPGA starts continuous sampling causes USB bus contention and `Input/Output Error`.
   - The driver reads Flash calibration tables (`GAIN`, `AMPL`, and `COMP` for CH1/CH2 across all 10 voltage attenuation ranges) once upon initialization, caching the table in memory for all subsequent capture operations.

#### C. Native Register Sequencing & Hardware Configuration
Before requesting acquisition frames, the driver pushes the full register bank to the FPGA:
1. **Timebase Prescaler (`SET_TIMEBASE` `0x0200`):**
   - The FPGA runs a 100 MHz clock base. The prescaler divisor is:
     $$N = \text{round}\left(\frac{100\text{ MSa/s}}{\text{Target Sampling Rate}}\right)$$
2. **Channel Control & Attenuation Relays (`SET_CHANNEL` `0x0201`):**
   - Configures input channel coupling (DC, AC, GND), probe multiplier ($1\times, 10\times$), hardware gain PGA, and hardware trigger levels.
3. **Capture Execution & Raw Framing (`READ_DATA` `0x0300`):**
   - Triggers acquisition and streams $5,000$ points per channel across bulk endpoint `0x81`. Raw bytes are parsed and scaled using the cached Flash calibration curve.

---

### 2.3 Device Discovery & Test Instrument Exclusion Invariants

Microcontroller UART detection scans system serial interfaces (`serialport::available_ports()`) with strict filtering to prevent false positives and catastrophic cross-wiring:
1. **Linux Dummy TTY Exclusion:** The Linux kernel exposes 32 legacy motherboard UARTs (`/dev/ttyS0`..`/dev/ttyS31`) which appear as unknown serial devices. The discovery engine strictly excludes all ports matching `/dev/ttyS*`.
2. **Test Instrument Port Exclusion:** Dedicated test instruments with onboard USB controllers (e.g. Owon VDS1022 at `0x5345:0x1234` or FX2LP logic analyzers at `0x0925:0x3881` / `0x04b4:0x8613`) must NEVER be treated as target microcontrollers or exposed as firmware upload destinations. The engine maintains an explicit `INSTRUMENT_USB_IDS` exclusion table. Target microcontrollers are recognized only when matching legitimate MCU USB-to-UART chips (CP210x, CH340, FTDI, RP2040, STM32 VCP) or configured manually.

---

## 3. Hardware Abstraction Layer (`HardwareInstrument` Trait)

All physical instruments implement a unified asynchronous trait (`src-tauri/src/kopeng/electronics/instruments/mod.rs`):

```rust
#[async_trait]
pub trait HardwareInstrument: Send + Sync {
    /// Unique hardware identifier (e.g. "owon_vds1022", "fx2lafw")
    fn id(&self) -> &'static str;

    /// Checks if device is plugged into physical USB host
    async fn is_connected(&self) -> bool;

    /// Initializes hardware registers and clocks
    async fn initialize(&mut self) -> Result<(), ElectronicsError>;

    /// Executes measurement acquisition frame
    async fn capture(&mut self, config: &CaptureConfig) -> Result<InstrumentFrame, ElectronicsError>;

    /// Releases hardware endpoints cleanly
    async fn close(&mut self) -> Result<(), ElectronicsError>;
}
```

### 3.1 Hardware Concurrency & Resource Lock Arbitrator

USB endpoints cannot be accessed concurrently by multiple callers without bus corruption. The `HardwareLockArbitrator` in `src-tauri/src/kopeng/electronics/arbitrator.rs` enforces:
1. **Exclusive Session Locks:** Only one entity (the User via UI or an Agent via MCP) can hold an active capture lease on an instrument at any given millisecond.
2. **Deterministic Timeouts:** Leases automatically expire after `session_lock_timeout_ms` ($5,000\text{ ms}$ default) to prevent runaway processes from hanging physical ports.
3. **Safe Teardown on App Exit:** Drops USB interface claims cleanly, avoiding orphaned kernel driver locks.

### 3.2 Declarative Device Capability Manifests (`OscilloscopeDescriptor`)

To adhere strictly to the *Anti-Mocking* and *Zero Dead Code* policies, the frontend NEVER hardcodes instrument models, channel counts, or supported discrete volt/div and timebase scales. Every compiled instrument driver exposes an immutable, strongly-typed capability manifest:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OscilloscopeDescriptor {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub model: String,
    pub channel_count: u8,
    pub channel_names: Vec<String>,
    pub volts_per_div_options_mv: Vec<u32>,
    pub couplings: Vec<String>,
    pub probe_attenuations: Vec<u32>,
    pub timebase_steps_us: Vec<f64>,
    pub min_timebase_us: f64,
    pub max_timebase_us: f64,
    pub max_sample_rate_hz: u64,
    pub bandwidth_hz: u64,
    pub buffer_points_count: usize,
    pub has_trigger: bool,
    pub trigger_sources: Vec<String>,
    pub trigger_modes: Vec<String>,
    pub trigger_slopes: Vec<String>,
    pub has_hardware_calibration: bool,
    pub has_signal_generator: bool,
}
```

1. **Daemon Discovery Command:** `list_supported_oscilloscopes` returns all compiled oscilloscope driver manifests (e.g. `owon_vds1022`).
2. **Dynamic UI Binding:** The frontend model selector, channel drawers, stepper scales, trigger sources, and hardware calibration wizards render dynamically driven by `activeScopeDescriptor`.
3. **Hotplug Auto-Selection:** When USB scanning detects a known physical instrument, the matching descriptor is automatically activated without manual intervention.

---

## 4. Frontend Visual Instrumentation Interfaces (Vue 3 / WebGL)

ONE-SHIP provides dedicated, brutalist-styled visual instruments directly in the desktop UI under the **Electronics** tab (`src/components/workspace/ProjectElectronicsTab.vue`).

```text
+-----------------------------------------------------------------------------------------------------------------+
| VIRTUAL OSCILLOSCOPE (Owon VDS1022)                                    [Calibrate]     [RUN]  [STOP]  [SINGLE]  |
+-----------------------------------------------------------------------------------------------------------------+
| +-------------------------------------------------------------------------+  +--------------------------------+ |
| |                                                                         |  | CHANNEL 1                      | |
| |          /\            /\            /\                                 |  | [ON]  [ 1X | 10X ]  [DC|AC|GND]| |
| |  -------/  \----------/  \----------/  \-------                         |  | Scale:  [-] [ 1 V/div ] [+]     | |
| |                                                                         |  | Offset: [-] [   0 mV  ] [+] [0] | |
| |  =====================================================================  |  | Vpp: 3.32V  Vrms: 1.65V        | |
| |                                                                         |  | Freq: 1.00kHz Duty: 50.0%      | |
| |  Cursors (Canvas HUD):                                                  |  |--------------------------------| |
| |  T1: -500µs  T2: 500µs  Δt: 1.00ms (1.00 kHz) | ΔV: 3.30V               |  | CHANNEL 2                      | |
| +-------------------------------------------------------------------------+  | [OFF] [ 1X | 10X ]  [DC|AC|GND]| |
| +-------------------------------------------------------------------------+  | Scale:  [-] [500 mV/div] [+]   | |
| | Timebase: [-] [ 1 ms/div ] [+]                                          |  | Offset: [-] [   0 mV  ] [+] [0] | |
| | [ Time (Δt) ] T1: [-][sl][+] [0]  T2: [-][sl][+] [0]                    |  | Vpp: ---  Vrms: ---            | |
| | [ Volt (ΔV) ] V1: [-][sl][+] [0]  V2: [-][sl][+] [0]                    |  | Freq: --- Duty: ---            | |
| +-------------------------------------------------------------------------+  |--------------------------------| |
|                                                                              | TRIGGER                        | |
|                                                                              | [CH1 | CH2 | EXT]    [ ↗ | ↘ ] | |
|                                                                              | [ Auto | Normal | Single ]     | |
|                                                                              | Level: [-] [   0 mV  ] [+] [0] | |
|                                                                              +--------------------------------+ |
+-----------------------------------------------------------------------------------------------------------------+
```

### 4.1 Virtual Oscilloscope Interface (`OscilloscopeView.vue` & `useElectronicsStore`)
- **State Persistence & Background Acquisition Continuity (`useElectronicsStore`):**
  - All channel setups, timebase, triggers, cursors, and display modes reside in a dedicated singleton Pinia store (`src/stores/electronicsStore.ts`).
  - Configuration automatically syncs with `localStorage['oneship_oscilloscope_config']`.
  - Continuous acquisition (`RUN` loop) executes at the store layer, preventing session abortion, disconnects, or blank viewports when navigating between workspace tabs.
- **Auto-Adjust & Quick Calibration Suite:**
  - **Master "AUTO SET":** Single-click global auto-tuning setting optimal voltage scales, centering offsets on $0\text{V}$, framing 2–3 signal cycles on the horizontal timebase ($T_{\text{div}} \approx 0.3 \times \frac{1}{f}$), and locking trigger level to the signal midpoint ($V_{\text{mid}}$).
  - **Auto-Center `[C]`:** Computes signal midpoint $V_{\text{mid}} = \frac{V_{\max} + V_{\min}}{2}$ (or $V_{\text{avg}}$) and adjusts offset to $-V_{\text{mid}}$ to snap the waveform onto the center grid line.
  - **Auto-Scale `[Fit]`:** Calculates target scale $V_{\text{div}} \ge \frac{V_{pp}}{6}$ to fit the waveform across ~60-75% vertical screen height.
- **Tactile Accelerating Steppers (`usePressAndHold`):**
  - Buttons (`[-]` and `[+]`) on offsets, scale, timebase, trigger level, and cursors support continuous press-and-hold with exponential acceleration (350ms initial delay $\rightarrow$ 150ms $\rightarrow$ 25ms clamp).
- **Dual Grid (Split Screen) vs Single Grid (Overlaid):**
  - **Single Grid (Overlaid):** Both channels rendered across a single 8-division vertical canvas with shared zero center line.
  - **Dual Grid (Split Screen):** Divides the canvas into independent top (CH1) and bottom (CH2) viewports, each with its own 8-division sub-grid, zero reference markers, and strict clipping, eliminating waveform overlap for contrasting signals.
- **Rendering Engine:** Native HTML5 Canvas / WebGL rendering at 60fps with high-DPI transform scaling and zero DOM overhead.
- **Full-Width Canvas Workstation (100% Center Space):** Maximizes waveform visibility with no left rails or vertical strip clutters. High-information HUD overlays display real-time timebase, coupling, trigger levels, and cursor delta readouts ($\Delta t, 1/\Delta t, \Delta V$) directly inside the viewport.
- **Physical Test-Bench Channel Modules (Right Drawer):**
  - Section 1 (Header): Channel power toggle, direct 2-position segmented toggle (`[ 1X | 10X ]` without redundant text label), and direct 3-way segmented toggle (`[ DC | AC | GND ]`).
  - Section 2 (Steppers): Tactile steppers for Vertical Scale (`[-] [ 1 V/div ] [+] [Fit]` cycling 1-2-5 ranges) and Offset (`[-] [ 0 mV ] [+] [C] [0]` with auto-center and zero-snap). Completely eliminates desktop dropdowns.
  - Section 3 (Channel Metrics Grid): Integrated 4-column $\times$ 2-row metrics block directly below each channel card (`Vpp`, `Vrms`, `Freq`, `Duty`). Active channels render in high-contrast legible font-mono; deactivated channels cleanly dim to `---` in `text-zinc-600`.
- **Dedicated Trigger Controls (Right Drawer):**
  - Segmented buttons for Sweep (`Auto / Normal / Single`), Source (`CH1 / CH2 / EXT`), Slope (`↗ / ↘`), and Stepper for Trigger Level (`[-] [ 0 mV ] [+] [0]`).
- **Pluggable Scope Model Selection & Comprehensive Disconnected Lockdown:**
  - Hardware model selection is dynamically populated from compiled backend manifests (`list_supported_oscilloscopes`), decoupling UI logic from vendor-specific drivers.
  - When disconnected, the entire workstation area (canvas, FFT spectrum, channel cards CH1/CH2, trigger panel, and measurements) is sealed by a global backdrop overlay (`absolute inset-0 bg-[#090a0f]/90 backdrop-blur-[2px] z-30`), preventing any interaction or invalid visual state.
  - All top toolbar actions (Auto Set, Dual/Single Grid, Run/Stop, Single sweep, Calibrate wizard, Timebase steppers, $\Delta t$/$\Delta V$ cursor toggles, Math, FFT, Meas, and Waveform exports) are disabled (`disabled:opacity-30 disabled:cursor-not-allowed disabled:pointer-events-none`), leaving exclusively the oscilloscope selector interactable.
- **Minimalist Top Header Toolbar & Bottom Toolbar Elimination:**
  - Primary workstation actions are consolidated in the top header toolbar using brutalist symbol-only buttons (AGENTS.md rules 5.3 & 5.5) with full descriptive `AppTooltip` overlays: Auto Set (`Sparkles`), Single/Dual Grid (`Rows2`/`LayoutGrid`), Run/Stop (`Play`/`Square`), Single (`Zap`), and Calibration (`Wrench`).
  - **Integrated Timebase Stepper:** A compact stepper `[-] {timebase} [+]` in the top toolbar provides fast decade cycling ($100\text{ ns}$ to $100\text{ ms/div}$) with accelerating press-and-hold.
  - **Symbol-Only Cursor Toggles:** Segmented `[Δt]` (time) and `[ΔV]` (voltage) toggles activate interactive cursor overlays on demand.
  - **Elimination of the Bottom Toolbar:** Redundant bottom slider tracks and controls are fully eliminated, maximizing the vertical waveform canvas area.
- **Direct Canvas Cursor Drag & Drop with Double-Click Reset:**
  - Time ($T1, T2$) and Voltage ($V1, V2$) cursors can be dragged directly across the waveform canvas with sub-pixel pointer capture.
  - Proximity detection ($10\text{ px}$ threshold) triggers dynamic cursor styling (`ew-resize` for time, `ns-resize` for voltage, `crosshair` in idle state).
  - Hover and active states display high-contrast solid lines ($2.0\text{ px}$) and handle badges on the top and left edges, with live 60 FPS updates to the floating measurements HUD.
  - **Double-Click Reset to Zero:** Double-clicking directly on any cursor line automatically resets its measurement offset to zero ($0\,\mu\text{s}$ or $0\,\text{mV}$).
- **Docked FFT Spectrum Analyzer Layout:**
  - When active, the FFT Spectrum Analyzer (`h-32`) is docked at the bottom of the center workstation pane directly underneath the main canvas container.
  - The main waveform canvas compresses vertically via `flex-1 min-h-0`, allowing the FFT spectrum to expand with full horizontal decade resolution without obscuring oscilloscope signals.
- **Hardware Channel Gating:** Deactivating a channel gates FPGA acquisition at the hardware layer, skipping USB transfer bandwidth and computing metrics strictly for active channels.
- **Hardware Configuration Cache & Zero-Relay-Thrashing Invariant:**
  - The driver caches the last applied hardware configuration (`LAST_APPLIED_CONFIG`).
  - During continuous acquisition (`RUN` mode), if range, offset, coupling, timebase, and trigger have not changed, the driver skips FPGA register reconfiguration and bypasses the $150\text{ ms}$ relay stabilization sleep.
  - The physical relays fire exactly once when a channel configuration is modified, eliminating coil wear and relay clicking noises, and accelerating streaming frame rates from $\sim 1\text{ FPS}$ to $20\text{--}30\text{ FPS}$.
- **Hardware vs. Software Voltage Calibration Invariant:**
  - The Owon VDS1022 FPGA register `0x010a` (`SET_ZERO_OFF`) applies an analog DAC offset before the ADC.
  - To prevent double-offset jumps during Auto Set and manual adjustments, the backend extracts raw ADC bytes $b$ and calculates absolute physical input voltage $V_{\text{in}} = ((b \text{ as } i8) \times \text{scale}) - \text{vertical\_offset\_mv}$.
  - The frontend draws $y = centerY - ((V_{\text{in}} + \text{offset}) / V_{\text{div}}) \times dy$, ensuring continuous spatial stability and preventing trace dislocation upon receiving subsequent hardware USB frames.

### 4.1.1 Phase 2 Advanced Laboratory Capabilities (`src/utils/electronicsDsp.ts`)
- **Math Channel Operations:**
  - Computes real-time vector mathematical combinations: Differential measurement $CH1 - CH2$, Sum $CH1 + CH2$, and Inverted $INV(CH1)$.
  - Rendered in brilliant fuchsia/magenta (`#d946ef`) on the main canvas with dedicated ground reference flag (`'M'`).
- **Real-Time FFT (Spectrum Analyzer):**
  - Radix-2 Cooley-Tukey FFT implementation operating on 1024 points with Hanning windowing to eliminate spectral leakage.
  - Generates logarithmic amplitude ($dBV = 20 \log_{10}(|X| / N)$) across a frequency spectrum from $0\text{ Hz}$ to the Nyquist ceiling ($f_s / 2$).
  - Features real-time peak frequency tracking with automated kHz annotation and harmonic inspection.
- **Extended Parametric Measurements Dashboard:**
  - Real-time extraction of IEEE standard pulse parameters:
    - Top Level ($V_{\text{top}}$) & Base Level ($V_{\text{base}}$) derived from 10%-90% amplitude distributions.
    - Rise Time ($t_r$): transition time from 10% to 90% threshold.
    - Fall Time ($t_f$): transition time from 90% to 10% threshold.
    - Channel Phase Difference ($\Delta \Phi$): cross-correlation zero-crossing phase offset in degrees ($-180^\circ$ to $+180^\circ$).
- **Waveform Data Export (CSV & JSON):**
  - **CSV Export:** Tabular stream formatted as `index,time_us,ch1_mv,ch2_mv,math_diff_mv` for external data processing in Jupyter, Python NumPy, or MATLAB.
  - **JSON Export:** Full session snapshot including calibrated sample arrays, timebase parameters, and scalar metrics.

### 4.2 Virtual Logic Analyzer Interface (`LogicAnalyzerView.vue`)
- **Digital Multi-Track Display:** Renders channels D0–D7 with high-contrast trace transitions.
- **Timeline Zoom & Navigation:** Smooth wheel zoom from microseconds to seconds; drag-to-pan across capture buffers.
- **Protocol Decoder Track:** Automatically decodes and overlays bus packets directly below raw pin waveforms (e.g., displaying I2C address, read/write bit, ACK/NACK, and data bytes as colored pill badges).
- **WaveDrom & SVG Export:** Single-click export of captured traces into WaveDrom JSON or SVG timing diagrams for project documentation.

---

## 5. Microcontroller Firmware Development, Project Scaffolding & Toolchain Diagnostics

Firmware development is integrated into the native supervisor lifecycle without requiring heavyweight external IDEs.

### 5.1 Toolchain Orchestration & Multi-Tool Diagnostics
The daemon probes the host toolchains exposed in `ProjectElectronicsStatus`:
- `platformio`: Microcontroller compiler and board SDK manager.
- `probe-rs`: Native Rust SWD/JTAG flasher and RTT monitor.
- `esptool`: ESP32/ESP8266 firmware flasher.
- `arduino-cli`: Arduino build and upload CLI.
- `sigrok-cli`: Digital signal analysis and protocol decoding engine.
- `mpremote`: MicroPython file transfer and REPL client (pip package, not pacman).

The UI displays a persistent top-bar counter (`Toolchains: X/6`) opening a dedicated diagnostic modal with live paths, version numbers, and install actions. Pacman-backed tools dispatch via `pkexec pacman -S --noconfirm`; `mpremote` exposes its `python -m pip install --user mpremote` hint for copying instead.

### 5.2 Dynamic PlatformIO Board Catalog & Project Scaffolding
Rather than constraining the engineer to an arbitrary preselected board list, ONE-SHIP queries the full PlatformIO board catalog:
- **Command:** `list_platformio_boards(query)` retrieves from a thread-safe daemon cache populated via `pio boards --json-output` (encompassing 1,656 boards across 44 platforms and 283 vendors).
- **Hardware flow:** the Firmware subtab separates `Hardware` (detected serial port, baud, board catalog and USB-based board suggestion), `Firmware Project` (language, framework, monitor speed, scaffold), and `Toolchain & Operations` (toolchain state, environment, compile/upload, logs).
- **Language:** `scaffold_firmware_project` accepts `language = cpp | c | micropython`. C/C++ generates `platformio.ini`, `src/main.cpp` (or `src/main.c`), `include/`, and `lib/` inside `[plugins.electronics].firmware_dir` (cascade setting, default `"."`, relative to project root).
- **MicroPython:** generates `main.py`, `boot.py`, and `README.md` for ESP32, ESP8266, RP2040, and STM32 boards; no `platformio.ini` is written. `flash_project_firmware` copies those files with `mpremote connect <port> fs cp … + soft-reset`, and `build_project_firmware` returns a typed "no compile step" error. The MicroPython runtime itself must be flashed once with `esptool`.
- Both scaffolds record `[plugins.electronics].firmware_language` in `rula.toml`; `ProjectElectronicsStatus.firmware_language` reports it (`platformio`, `micropython`, or `null`).
- `list_firmware_environments`, `build_project_firmware`, and `flash_project_firmware` execute inside the resolved `firmware_dir`; `[plugins.electronics].schematics_dir` (cascade setting, default `"."`) designates the hardware/EDA subdirectory.

### 5.3 Live Serial Monitor & Baud Rate Selection
- High-performance, non-blocking serial port monitor in Rust (`serialport` / `tokio-serial`) streaming output to the terminal at selectable baud rates (300 to 2,000,000 bps).
- Bidirectional text transmission with configurable line endings (`LF`, `CRLF`, `CR`).

---

## 6. Circuit Schematic & EDA Ingestion Engine (Fusion 360 & KiCad)

To enable AI agents to reason about physical circuits, ONE-SHIP parses EDA files and constructs a **Semantic Hardware Graph**.

```text
[Fusion 360 / EAGLE XML] (.sch / .brd)  ───> [quick-xml Parser] ───+
                                                                    │
[KiCad 6/7/8 S-Expression] (.kicad_sch) ───> [S-Expr Parser]   ────┼──> [Semantic Hardware Graph]
                                                                    │    - Component Inventory
[KiCanvas WebGL Component] (UI Viewer)  <─── [Raw Schematic]  ─────+    - Netlist Connections
                                                                         - MCU Pinout Mapping
```

### 6.1 Autodesk Fusion 360 Electronics & EAGLE Parsing
- **Format:** XML-based files (`.sch` for schematics, `.brd` for PCB layouts).
- **Parser Implementation:** Pure Rust zero-copy parser in `src-tauri/src/kopeng/electronics/eda/eagle.rs` using `quick-xml`.
- **Extracted Entities:**
  - `<parts>`: RefDes (`U1`, `R1`, `C3`), component values (`10k`, `100nF`), packages (`SOIC-8`, `0805`), library names.
  - `<nets>`: Net names (`RESET`, `SPI_MOSI`, `VCC_3V3`, `GND`) and `<pinref>` pin-to-component mappings.
  - `<signals>`: Board trace routing and pad connectivity.

### 6.2 KiCad Integration & KiCanvas Embedded Viewer
- **Format:** KiCad 6+ S-Expressions (`.kicad_sch`, `.kicad_pcb`).
- **Interactive UI Viewer:** Integrates **KiCanvas** (`theacodes/kicanvas`) as a native WebComponent in the Vue frontend. Users can pan, zoom, click components, inspect layers, and highlight nets directly within ONE-SHIP without launching external CAD software.

### 6.3 Semantic Pinout & Netlist Verification for LLMs
The engine synthesizes a normalized **Hardware Pinout Table** exposed to agents via MCP:

```json
{
  "microcontroller": "U1 (ESP32-WROOM-32D)",
  "pin_mappings": [
    { "pin_number": "23", "mcu_pin": "IO23", "net_name": "SPI_MOSI", "connected_to": ["U2.4 (FLASH_DI)"] },
    { "pin_number": "19", "mcu_pin": "IO19", "net_name": "SPI_MISO", "connected_to": ["U2.2 (FLASH_DO)"] },
    { "pin_number": "18", "mcu_pin": "IO18", "net_name": "SPI_SCK",  "connected_to": ["U2.6 (FLASH_CLK)"] },
    { "pin_number": "5",  "mcu_pin": "IO5",  "net_name": "STATUS_LED", "connected_to": ["R4.1 (330R) -> D1.A (GREEN)"] }
  ]
}
```

**Autonomous Cross-Verification:**
When an agent writes firmware (e.g., `#define LED_PIN 4`), it queries `electronics_read_schematic`. The tool alerts the agent:
> *"Warning: Schematic indicates STATUS_LED is connected to IO5, but code configures IO4."*

---

## 7. Headless SPICE Simulation Engine (`ngspice`)

ONE-SHIP provides automated analog and mixed-signal circuit verification via headless `ngspice` execution.

1. **Subcircuit Netlist Extraction:** Translates the parsed schematic or a targeted subcircuit into a standardized SPICE netlist (`.cir`).
2. **Headless Batch Execution:** Invokes `ngspice -b -r output.raw circuit.cir` in an isolated temporary sandbox (`<project>/.one-ship/electronics/sim/`).
3. **Simulation Types:**
   - `.op`: DC operating points, voltage drops, and quiescent bias currents.
   - `.tran`: Transient time-domain response (e.g., step response of an RC filter, power-on reset pulse duration).
   - `.ac`: Frequency sweep analysis, Bode magnitude/phase plots, filter cut-off frequencies.
4. **Agent Reporting:** Synthesizes simulation outcomes into ASCII plots or concise numerical summaries (e.g. *"Filter -3dB cutoff frequency verified at 15.9 kHz"*).

---

## 8. MCP Dialogue Protocol & Agent Tool Catalog

Autonomous agents communicate with the electronics engine via strongly-typed MCP tools registered in `src-tauri/src/mcp.rs`:

### 8.1 Tool Specifications

#### 1. `electronics_detect_hardware`
Scans USB buses, serial ports, and network interfaces for connected instruments and development boards.
- **Parameters:** None.
- **Returns:** List of discovered devices with IDs, categories, and connection states.

#### 2. `electronics_read_schematic`
Parses project schematic files and returns structured circuit topology.
- **Parameters:**
  - `schematic_path`: Optional relative path to `.sch` or `.kicad_sch` file.
  - `focus_component`: Optional RefDes (e.g. `"U1"`) to inspect specific component connections.
- **Returns:** Microcontroller pinout map, connected peripherals, and power rails.

#### 3. `electronics_capture_logic`
Arms the 24MHz logic analyzer, captures digital traces, and decodes protocol traffic.
- **Parameters:**
  - `sample_rate_hz`: Integer (e.g. `24000000`, `12000000`, `1000000`).
  - `duration_ms`: Integer capture window (e.g. `50`).
  - `decoder`: Optional protocol decoder name (e.g. `"i2c"`, `"spi"`, `"uart"`).
  - `pins`: Pin mapping map (e.g. `{"scl": 0, "sda": 1}`).
- **Returns:** Decoded transaction list and token-efficient WaveDrom timing JSON.

#### 4. `electronics_read_oscilloscope`
Acquires oscilloscope measurements and waveform telemetry from the Owon VDS1022.
- **Parameters:**
  - `channel`: Integer (`1` or `2`).
  - `timebase_div_us`: Integer microseconds per division.
  - `volts_div_mv`: Integer millivolts per division.
- **Returns:** `HardwareDigest` with $V_{pp}$, $V_{rms}$, frequency, min/max voltages, and signal quality metrics.

#### 5. `electronics_flash_firmware`
Compiles and uploads microcontroller firmware to the connected target board.
- **Parameters:**
  - `target_environment`: Optional PlatformIO environment name (e.g. `"esp32dev"`).
  - `flasher`: Optional tool selection (`"pio"` or `"probe_rs"`).
- **Returns:** Build status, compilation warnings/errors, and upload confirmation.

#### 6. `electronics_run_simulation`
Runs a headless SPICE simulation on a circuit netlist.
- **Parameters:**
  - `netlist_file`: Path to `.cir` file or raw SPICE netlist string.
  - `analysis_type`: `"op"`, `"ac"`, or `"tran"`.
- **Returns:** Numerical results and ASCII frequency/transient curve.

---

## 9. Tiger Style Engineering Standards & System Invariants

1. **Linux Permissions & `udev` Rules:**
   - Physical USB access requires non-root user permissions.
   - ONE-SHIP generates and verifies standard udev rules:
     - Owon VDS1022: `/etc/udev/rules.d/99-owon-vds1022.rules` (`ATTRS{idVendor}=="5345", MODE="0666", GROUP="plugdev"`).
     - Cypress FX2 Logic Analyzer: `/etc/udev/rules.d/60-libsigrok.rules` (`ATTRS{idVendor}=="0925", ATTRS{idProduct}=="3881", MODE="0666"`).
2. **Explicit Unit Suffixes:**
   - All time values: `_ns`, `_us`, `_ms`, `_seconds`.
   - All electrical values: `_volts`, `_mv`, `_uv`, `_amperes`, `_ma`, `_ohms`, `_farads`, `_hz`.
   - All sample counters: `_samples_count`, `_rate_hz`.
3. **Bounded Memory Allocations:**
   - Waveform buffers are strictly bounded to the physical hardware limit ($5,000$ samples per channel for VDS1022).
   - Dynamic allocations in capture loops are pre-allocated (`Vec::with_capacity(5000)`).
4. **Poison-Resistant Concurrency:**
   - Shared instrument sessions and lock states are accessed strictly via poison-safe patterns (`read_safe()` / `write_safe()`).
5. **Zero Dead Code & Zero Mocks:**
   - No mock oscilloscope engines in production builds. If hardware is disconnected, APIs return strongly-typed `HardwareError::DeviceDisconnected` errors.
