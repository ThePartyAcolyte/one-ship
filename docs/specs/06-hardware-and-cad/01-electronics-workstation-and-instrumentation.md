# Electronics Workstation and Instrumentation Engine

## Purpose

Transform ONE-SHIP into a hardware-in-the-loop embedded development platform (`kopeng::electronics`) where firmware engineering, physical test instruments, schematic topology, and circuit simulation share a unified workspace for both human engineers and autonomous AI agents.

## Scope

In scope:

- Dual-actor workstation architecture (visual benchtop UI for humans and token-efficient hardware digests for agents).
- Native Rust USB drivers (`rusb`) for the Owon VDS1022 / VDS1022I digital oscilloscope and `sigrok` / `fx2lafw` orchestration for 24 MHz 8-channel logic analyzers.
- Hardware resource lock arbitration (`HardwareLockArbitrator`), declarative oscilloscope capability manifests (`OscilloscopeDescriptor`), and serial/MCU discovery invariants.
- Virtual Oscilloscope and Logic Analyzer interfaces (`ProjectElectronicsTab.vue`, `OscilloscopeView.vue`, `LogicAnalyzerView.vue`, `electronicsStore.ts`, `electronicsDsp.ts`).
- Firmware toolchain diagnostics, dynamic PlatformIO board discovery, C/C++/MicroPython project scaffolding, and live serial monitoring.
- Circuit schematic ingestion (Fusion 360 / EAGLE XML and KiCad S-Expressions + KiCanvas), headless SPICE simulation (`ngspice`), and the electronics MCP tool catalog.

Out of scope:

- PCBCraft project authoring, native PCB SVG previews, and Stage D/E BOM sourcing and PCBWay fabrication packaging (see [CAD and PCB Integration](03-cad-and-pcb-integration.md)).
- Global configuration cascade rules (see [Configuration Schema and Cascade](../01-architecture/02-configuration-schema-and-cascade.md)).

## Model / Design

### 1. Architectural Motivation and Dual-Actor Philosophy

Embedded systems and custom hardware prototyping traditionally suffer from three disconnected silos:

1. **Firmware code** is edited in an IDE with no semantic awareness of the underlying physical schematic or pin connections.
2. **Physical test instruments** (oscilloscopes, logic analyzers) run in separate proprietary vendor GUIs, inaccessible to AI agents and hard to correlate with running firmware.
3. **Hardware debugging** requires manual copy-pasting of pin configurations and trial-and-error measurement validation.

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

### 2. Supported Hardware Baseline and Native Rust Driver Architecture

All instrument drivers and hardware interfaces **prioritize native Rust implementations** using memory-safe asynchronous USB abstractions (`rusb`) rather than heavy Java runtimes or external interpreted wrappers.

#### 2.1 Hardware Integration Matrix

| Instrument Category | Physical Hardware Target | Chipset / Architecture | Driver Strategy | Real-Time Output |
| :--- | :--- | :--- | :--- | :--- |
| **Digital Oscilloscope** | **Owon VDS1022 / VDS1022I** | Cypress FX2 USB + FPGA (25MHz bandwidth, 100MSa/s real-time, 2 analog channels + multi-channel trigger) | **Native Rust Driver (`rusb`)** reverse-engineered from device USB protocol packets | 5,000 raw samples/channel, $V_{pp}$, $V_{rms}$, frequency, period, min/max |
| **Logic Analyzer** | **Generic 24MHz 8-Channel** | Cypress FX2LP (`CY7C68013A`) | **sigrok Engine (`sigrok-cli` / `libsigrok`)** with open-source `fx2lafw` firmware | 8 digital channels, WaveDrom JSON, Google Trace Event JSON, 100+ protocol decoders |
| **Microcontroller Flasher / Debugger** | **ST-Link, J-Link, CMSIS-DAP, ESP-Prog, USB-Serial** | ARM Cortex-M, RISC-V, ESP32, AVR | **probe-rs** (Rust native) + **PlatformIO Core (`pio`)** | Firmware binaries, memory flashing, bidirectional **RTT** and UART serial logs |

#### 2.2 Native Rust Driver for Owon VDS1022 (`rusb`)

The Owon VDS1022 communicates via USB bulk endpoints using a proprietary FPGA command register set. The driver lives in `src-tauri/src/kopeng/electronics/instruments/owon_vds1022.rs` and uses `tokio::task::spawn_blocking` to execute low-level USB I/O without blocking the Tokio runtime.

- **USB Endpoints & Device Identification:**
  - **USB Vendor ID (VID):** `0x5345`
  - **USB Product ID (PID):** `0x1234`
  - **Endpoint IN (`0x81`):** Bulk IN endpoint for command replies and waveform sample data frames.
  - **Endpoint OUT (`0x01`):** Bulk OUT endpoint for command and register configuration packets.
  - **Linux Udev Permissions:** `/etc/udev/rules.d/99-oneship-instruments.rules` sets `MODE="0666", TAG+="uaccess"`.
- **Cold FPGA Bootstrap & Flash Calibration Cache:**
  1. **FPGA Bitstream Injection (`ensure_fpga_loaded`):** Queries FPGA state via `QUERY_FPGA` (`0x0223`). If uninitialized (`0`), inspects the hardware version string via `READ_FLASH` (`0x01b0`) at address `0x08` (e.g., `V5.0.1` indicates Gowin V5). Bundled bitstream `src-tauri/resources/firmware/owon/VDS1022_FPGAV5_gaoyun.bin` is streamed across USB Bulk OUT via `LOAD_FPGA` (`0x4000`) in 10,240-byte chunks.
  2. **Flash Calibration Memory Cache (`FLASH_CALIBRATION_CACHE`):** Reading Flash after the FPGA starts continuous sampling causes USB bus contention and `Input/Output Error`. The driver reads Flash calibration tables (`GAIN`, `AMPL`, and `COMP` for CH1/CH2 across all 10 voltage attenuation ranges) once upon initialization, caching the table in memory for all subsequent capture operations.
- **Native Register Sequencing & Hardware Configuration:**
  1. **Timebase Prescaler (`SET_TIMEBASE` `0x0200`):** The FPGA runs a 100 MHz clock base. The prescaler divisor is:
     $$N = \text{round}\left(\frac{100\text{ MSa/s}}{\text{Target Sampling Rate}}\right)$$
  2. **Channel Control & Attenuation Relays (`SET_CHANNEL` `0x0201`):** Configures input channel coupling (DC, AC, GND), probe multiplier ($1\times, 10\times$), hardware gain PGA, and hardware trigger levels.
  3. **Capture Execution & Raw Framing (`READ_DATA` `0x0300`):** Triggers acquisition and streams $5,000$ points per channel across bulk endpoint `0x81`. Raw bytes are parsed and scaled using the cached Flash calibration curve.

#### 2.3 Device Discovery and Test Instrument Exclusion Invariants

Microcontroller UART detection scans system serial interfaces (`serialport::available_ports()`) with strict filtering to prevent false positives and cross-wiring:

1. **Linux Dummy TTY Exclusion:** The Linux kernel exposes 32 legacy motherboard UARTs (`/dev/ttyS0`..`/dev/ttyS31`) that appear as unknown serial devices. The discovery engine strictly excludes all ports matching `/dev/ttyS*`.
2. **Test Instrument Port Exclusion:** Dedicated test instruments with onboard USB controllers (Owon VDS1022 at `0x5345:0x1234` or FX2LP logic analyzers at `0x0925:0x3881` / `0x04b4:0x8613`) must NEVER be treated as target microcontrollers or exposed as firmware upload destinations. The engine maintains an explicit `INSTRUMENT_USB_IDS` exclusion table. Target microcontrollers are recognized only when matching legitimate MCU USB-to-UART chips (CP210x, CH340, FTDI, RP2040, STM32 VCP) or configured manually.

### 3. Hardware Abstraction Layer and Concurrency Arbitration

#### 3.1 `HardwareInstrument` Trait Target

The unified asynchronous hardware instrument contract is designed as:

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

#### 3.2 Hardware Concurrency and Resource Lock Arbitrator

USB endpoints cannot be accessed concurrently by multiple callers without bus corruption. `HardwareLockArbitrator` (`src-tauri/src/kopeng/electronics/instruments/arbitrator.rs`) enforces:

1. **Exclusive Session Locks:** Only one entity (the User via UI or an Agent via MCP) can hold an active capture lease on an instrument at any given millisecond.
2. **Deterministic Timeouts:** Leases automatically expire after `session_lock_timeout_ms` ($5,000\text{ ms}$ default) to prevent runaway processes from hanging physical ports.
3. **Safe Teardown on App Exit:** Drops USB interface claims cleanly, avoiding orphaned kernel driver locks.

#### 3.3 Declarative Device Capability Manifests (`OscilloscopeDescriptor`)

To adhere strictly to anti-mocking and zero-dead-code policies, the frontend NEVER hardcodes instrument models, channel counts, or supported discrete volt/div and timebase scales. Every compiled instrument driver exposes an immutable, strongly-typed capability manifest:

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

- **Daemon Discovery Command:** `list_supported_oscilloscopes` returns all compiled oscilloscope driver manifests (e.g., `owon_vds1022`).
- **Dynamic UI Binding:** The frontend model selector, channel drawers, stepper scales, trigger sources, and hardware calibration wizards render dynamically driven by `activeScopeDescriptor`.
- **Hotplug Auto-Selection:** When USB scanning detects a known physical instrument, the matching descriptor is automatically activated without manual intervention.

### 4. Frontend Visual Instrumentation Interfaces (Vue 3 / WebGL)

Visual instruments live under the **Electronics** workspace tab (`src/components/workspace/ProjectElectronicsTab.vue`).

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

#### 4.1 Virtual Oscilloscope Interface (`OscilloscopeView.vue` & `useElectronicsStore`)

- **State Persistence & Background Acquisition Continuity (`useElectronicsStore`):**
  - Channel setups, timebase, triggers, cursors, and display modes reside in a singleton Pinia store (`src/stores/electronicsStore.ts`).
  - Configuration syncs automatically with `localStorage['oneship_oscilloscope_config']`.
  - Continuous acquisition (`RUN` loop) executes at the store layer, preventing session abortion, disconnects, or blank viewports when navigating between workspace tabs.
- **Auto-Adjust & Quick Calibration Suite:**
  - **Master `AUTO SET`:** Single-click global auto-tuning setting optimal voltage scales, centering offsets on $0\text{ V}$, framing 2–3 signal cycles on the horizontal timebase ($T_{\text{div}} \approx 0.3 \times \frac{1}{f}$), and locking trigger level to the signal midpoint ($V_{\text{mid}}$).
  - **Auto-Center `[C]`:** Computes signal midpoint $V_{\text{mid}} = \frac{V_{\max} + V_{\min}}{2}$ (or $V_{\text{avg}}$) and adjusts offset to $-V_{\text{mid}}$ to snap the waveform onto the center grid line.
  - **Auto-Scale `[Fit]`:** Calculates target scale $V_{\text{div}} \ge \frac{V_{pp}}{6}$ to fit the waveform across ~60–75% vertical screen height.
- **Tactile Accelerating Steppers (`usePressAndHold`):**
  - `[-]` and `[+]` buttons on offsets, scale, timebase, trigger level, and cursors support continuous press-and-hold with exponential acceleration ($350\text{ ms}$ initial delay $\rightarrow 150\text{ ms} \rightarrow 25\text{ ms}$ clamp).
- **Dual Grid (Split Screen) vs. Single Grid (Overlaid):**
  - **Single Grid (Overlaid):** Both channels rendered across a single 8-division vertical canvas with shared zero center line.
  - **Dual Grid (Split Screen):** Divides the canvas into independent top (CH1) and bottom (CH2) viewports, each with its own 8-division sub-grid, zero reference markers, and strict clipping, eliminating waveform overlap for contrasting signals.
- **Rendering Engine & Full-Width Canvas:**
  - Native HTML5 Canvas / WebGL rendering at 60 fps with high-DPI transform scaling and zero DOM overhead.
  - Maximizes waveform visibility with no left rails or vertical strip clutter. HUD overlays display real-time timebase, coupling, trigger levels, and cursor delta readouts ($\Delta t, 1/\Delta t, \Delta V$) inside the viewport.
- **Physical Test-Bench Channel Modules (Right Drawer):**
  - **Header:** Channel power toggle, direct 2-position segmented toggle (`[ 1X | 10X ]` without redundant text label), and direct 3-way segmented toggle (`[ DC | AC | GND ]`).
  - **Steppers:** Vertical Scale (`[-] [ 1 V/div ] [+] [Fit]` cycling 1-2-5 ranges) and Offset (`[-] [ 0 mV ] [+] [C] [0]` with auto-center and zero-snap), eliminating desktop dropdowns.
  - **Channel Metrics Grid:** Integrated 4-column $\times$ 2-row metrics block below each channel card (`Vpp`, `Vrms`, `Freq`, `Duty`). Active channels render in high-contrast monospace; deactivated channels dim to `---` in `text-zinc-600`.
- **Dedicated Trigger Controls (Right Drawer):** Segmented buttons for Sweep (`Auto / Normal / Single`), Source (`CH1 / CH2 / EXT`), Slope (`↗ / ↘`), and Stepper for Trigger Level (`[-] [ 0 mV ] [+] [0]`).
- **Pluggable Scope Model Selection & Comprehensive Disconnected Lockdown:**
  - Hardware model selection is populated dynamically from `list_supported_oscilloscopes`.
  - When disconnected, the workstation area (canvas, FFT spectrum, channel cards CH1/CH2, trigger panel, and measurements) is sealed by a global backdrop overlay (`absolute inset-0 bg-[#090a0f]/90 backdrop-blur-[2px] z-30`), preventing interaction or invalid visual states.
  - Top toolbar actions (Auto Set, Dual/Single Grid, Run/Stop, Single sweep, Calibrate wizard, Timebase steppers, $\Delta t$/$\Delta V$ cursor toggles, Math, FFT, Meas, and Waveform exports) are disabled (`disabled:opacity-30 disabled:cursor-not-allowed disabled:pointer-events-none`), leaving only the oscilloscope selector interactable.
- **Minimalist Top Header Toolbar & Bottom Toolbar Elimination:**
  - Consolidated symbol-only buttons with descriptive `AppTooltip` overlays: Auto Set (`Sparkles`), Single/Dual Grid (`Rows2`/`LayoutGrid`), Run/Stop (`Play`/`Square`), Single (`Zap`), and Calibration (`Wrench`).
  - Compact integrated timebase stepper `[-] {timebase} [+]` providing fast decade cycling ($100\text{ ns}$ to $100\text{ ms/div}$) with accelerating press-and-hold.
  - Segmented `[Δt]` (time) and `[ΔV]` (voltage) toggles activate interactive cursor overlays on demand; redundant bottom slider tracks are omitted to maximize canvas height.
- **Direct Canvas Cursor Drag & Drop with Double-Click Reset:**
  - Time ($T1, T2$) and Voltage ($V1, V2$) cursors drag directly across the canvas with sub-pixel pointer capture.
  - Proximity detection ($10\text{ px}$ threshold) switches cursor styling (`ew-resize` for time, `ns-resize` for voltage, `crosshair` idle). Hover/active states draw $2.0\text{ px}$ solid lines and edge handle badges with live 60 fps HUD updates.
  - Double-clicking directly on any cursor line resets its measurement offset to zero ($0\,\mu\text{s}$ or $0\,\text{mV}$).
- **Docked FFT Spectrum Analyzer Layout:**
  - When active, the FFT Spectrum Analyzer (`h-32`) docks at the bottom of the center workstation pane underneath the main canvas container, which compresses vertically via `flex-1 min-h-0`.
- **Hardware Channel Gating & Zero-Relay-Thrashing Invariant:**
  - Deactivating a channel gates FPGA acquisition at the hardware layer, skipping USB transfer bandwidth and computing metrics strictly for active channels.
  - `LAST_APPLIED_CONFIG` caches the last applied hardware configuration. During continuous `RUN` acquisition, if range, offset, coupling, timebase, and trigger have not changed, the driver skips FPGA register reconfiguration and bypasses the $150\text{ ms}$ relay stabilization sleep, eliminating coil wear and relay clicking while accelerating streaming frame rates from $\sim 1\text{ FPS}$ to $20\text{--}30\text{ FPS}$.
- **Hardware vs. Software Voltage Calibration Invariant:**
  - FPGA register `0x010a` (`SET_ZERO_OFF`) applies an analog DAC offset before the ADC.
  - To prevent double-offset jumps during Auto Set and manual adjustments, the backend extracts raw ADC bytes $b$ and computes absolute physical input voltage:
    $$V_{\text{in}} = ((b \text{ as } i8) \times \text{scale}) - \text{vertical\_offset\_mv}$$
  - The frontend draws $y = \text{centerY} - ((V_{\text{in}} + \text{offset}) / V_{\text{div}}) \times dy$, ensuring continuous spatial stability across subsequent USB frames.

#### 4.2 Advanced DSP & Waveform Export (`src/utils/electronicsDsp.ts`)

- **Math Channel Operations:** Real-time vector combinations: Differential ($CH1 - CH2$), Sum ($CH1 + CH2$), and Inverted ($INV(CH1)$), rendered in fuchsia/magenta (`#d946ef`) with a dedicated ground reference flag (`'M'`).
- **Real-Time FFT (Spectrum Analyzer):** Radix-2 Cooley-Tukey FFT on 1,024 points with Hanning windowing, generating logarithmic amplitude ($dBV = 20 \log_{10}(|X| / N)$) from $0\text{ Hz}$ to the Nyquist ceiling ($f_s / 2$) with real-time peak frequency tracking and harmonic inspection.
- **Extended Parametric Measurements Dashboard:** IEEE pulse parameters: Top Level ($V_{\text{top}}$) & Base Level ($V_{\text{base}}$) from 10%–90% amplitude distributions, Rise Time ($t_r$, 10% to 90%), Fall Time ($t_f$, 90% to 10%), and Channel Phase Difference ($\Delta \Phi \in [-180^\circ, +180^\circ]$ via cross-correlation zero-crossing).
- **Waveform Data Export:**
  - **CSV Export:** Tabular stream `index,time_us,ch1_mv,ch2_mv,math_diff_mv`.
  - **JSON Export:** Full session snapshot with calibrated sample arrays, timebase parameters, and scalar metrics.

#### 4.3 Virtual Logic Analyzer Interface (`LogicAnalyzerView.vue`)

- **Digital Multi-Track Display:** Renders channels D0–D7 with high-contrast trace transitions.
- **Timeline Zoom & Navigation:** Smooth wheel zoom from microseconds to seconds; drag-to-pan across capture buffers.
- **Protocol Decoder Track:** Overlays decoded bus packets directly below raw pin waveforms (e.g., I2C address, R/W bit, ACK/NACK, and data bytes as colored pill badges).
- **WaveDrom & SVG Export:** Single-click export into WaveDrom JSON or SVG timing diagrams.

### 5. Firmware Development, Project Scaffolding and Toolchain Diagnostics

#### 5.1 Toolchain Orchestration and Multi-Tool Diagnostics

The daemon probes six host toolchains reported in `ProjectElectronicsStatus`:

- `platformio` — Microcontroller compiler and board SDK manager.
- `probe-rs` — Native Rust SWD/JTAG flasher and RTT monitor.
- `esptool` — ESP32/ESP8266 firmware flasher.
- `arduino-cli` — Arduino build and upload CLI.
- `sigrok-cli` — Digital signal analysis and protocol decoding engine.
- `mpremote` — MicroPython file transfer and REPL client (pip package, not pacman).

The UI displays a persistent top-bar counter (`Toolchains: X/6`) opening a diagnostic modal with live paths, version numbers, and install actions. Pacman-backed tools dispatch via `pkexec pacman -S --noconfirm`; `mpremote` exposes its `python -m pip install --user mpremote` command for copying.

#### 5.2 Dynamic PlatformIO Board Catalog and Project Scaffolding

- **Board Catalog (`list_platformio_boards(query)`):** Queries a thread-safe daemon cache populated via `pio boards --json-output` (1,656 boards across 44 platforms and 283 vendors).
- **Firmware Subtab Structure:** Separates `Hardware` (detected serial port, baud, board catalog, and USB-based board suggestion), `Firmware Project` (language, framework, monitor speed, scaffold), and `Toolchain & Operations` (toolchain state, environment, compile/upload, logs).
- **Language Scaffolding (`scaffold_firmware_project`):** Accepts `language = cpp | c | micropython`.
  - **C/C++:** Generates `platformio.ini`, `src/main.cpp` (or `src/main.c`), `include/`, and `lib/` inside `[plugins.electronics].firmware_dir` (cascade setting, default `"."`, relative to project root).
  - **MicroPython:** Generates `main.py`, `boot.py`, and `README.md` for ESP32, ESP8266, RP2040, and STM32 boards without `platformio.ini`. `flash_project_firmware` copies files via `mpremote connect <port> fs cp … + soft-reset`, and `build_project_firmware` returns a typed "no compile step" error. The MicroPython runtime itself must be flashed once with `esptool`.
  - Both scaffolds record `[plugins.electronics].firmware_language` in `rula.toml`; `ProjectElectronicsStatus.firmware_language` reports `"platformio"`, `"micropython"`, or `null`.
- `list_firmware_environments`, `build_project_firmware`, and `flash_project_firmware` execute inside the resolved `firmware_dir`; `[plugins.electronics].schematics_dir` (cascade setting, default `"."`) designates the hardware/EDA subdirectory.

#### 5.3 Live Serial Monitor

- Non-blocking serial port monitor in Rust (`serialport` / `tokio-serial`) streaming output at selectable baud rates ($300$ to $2,000,000\text{ bps}$) with bidirectional text transmission and configurable line endings (`LF`, `CRLF`, `CR`).

### 6. Circuit Schematic and EDA Ingestion Engine (Fusion 360 & KiCad)

ONE-SHIP parses EDA files to construct a **Semantic Hardware Graph** for agent reasoning:

```text
[Fusion 360 / EAGLE XML] (.sch / .brd)  ───> [quick-xml Parser] ───+
                                                                    │
[KiCad 6/7/8 S-Expression] (.kicad_sch) ───> [S-Expr Parser]   ────┼──> [Semantic Hardware Graph]
                                                                    │    - Component Inventory
[KiCanvas WebGL Component] (UI Viewer)  <─── [Raw Schematic]  ─────+    - Netlist Connections
                                                                         - MCU Pinout Mapping
```

- **Autodesk Fusion 360 Electronics & EAGLE Parsing:** Pure Rust zero-copy parser (`quick-xml`) targeting XML `.sch` and `.brd` files, extracting `<parts>` (RefDes, values, packages, library names), `<nets>` (net names and `<pinref>` mappings), and `<signals>` (board trace routing and pad connectivity).
- **KiCad Integration & KiCanvas Viewer:** Parses KiCad 6+ S-Expressions (`.kicad_sch`, `.kicad_pcb`) and embeds **KiCanvas** (`theacodes/kicanvas`) as a WebComponent for panning, zooming, component inspection, layer selection, and net highlighting.
- **Semantic Pinout & Netlist Verification for LLMs:** Synthesizes a normalized **Hardware Pinout Table** exposed via MCP:

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

When an agent writes firmware (e.g., `#define LED_PIN 4`), querying `electronics_read_schematic` alerts it: *"Warning: Schematic indicates STATUS_LED is connected to IO5, but code configures IO4."*

### 7. Headless SPICE Simulation Engine (`ngspice`)

Automated analog and mixed-signal verification executes via headless `ngspice`:

1. **Subcircuit Netlist Extraction:** Translates a parsed schematic or targeted subcircuit into a standardized SPICE netlist (`.cir`).
2. **Headless Batch Execution:** Runs `ngspice -b -r output.raw circuit.cir` inside an isolated temporary sandbox (`<project>/.one-ship/electronics/sim/`).
3. **Simulation Types:**
   - `.op`: DC operating points, voltage drops, and quiescent bias currents.
   - `.tran`: Transient time-domain response (RC filter step response, power-on reset pulse duration).
   - `.ac`: Frequency sweep analysis, Bode magnitude/phase plots, and filter cut-off frequencies.
4. **Agent Reporting:** Synthesizes simulation outcomes into ASCII plots or concise numerical summaries (e.g., *"Filter -3dB cutoff frequency verified at 15.9 kHz"*).

### 8. Tiger Style Engineering Standards and System Invariants

1. **Linux Permissions & `udev` Rules:** Non-root USB access requires standard udev rules:
   - Owon VDS1022: `/etc/udev/rules.d/99-owon-vds1022.rules` (`ATTRS{idVendor}=="5345", MODE="0666", GROUP="plugdev"`) and `/etc/udev/rules.d/99-oneship-instruments.rules` (`MODE="0666", TAG+="uaccess"`).
   - Cypress FX2 Logic Analyzer: `/etc/udev/rules.d/60-libsigrok.rules` (`ATTRS{idVendor}=="0925", ATTRS{idProduct}=="3881", MODE="0666"`).
2. **Explicit Unit Suffixes:** Time (`_ns`, `_us`, `_ms`, `_seconds`), electrical quantities (`_volts`, `_mv`, `_uv`, `_amperes`, `_ma`, `_ohms`, `_farads`, `_hz`), and sample counters (`_samples_count`, `_rate_hz`).
3. **Bounded Memory Allocations:** Waveform buffers are strictly bounded to the physical hardware limit ($5,000$ samples per channel for VDS1022) and pre-allocated (`Vec::with_capacity(5000)`).
4. **Poison-Resistant Concurrency:** Shared instrument sessions and lock states are accessed via poison-safe patterns (`read_safe()` / `write_safe()` / `PoisonError::into_inner`).
5. **Zero Dead Code & Zero Mocks:** No mock oscilloscope engines in production builds. Disconnected hardware returns strongly-typed errors (`ElectronicsError::DeviceNotFound` / planned `HardwareError::DeviceDisconnected`).

## Contracts

### Daemon Commands (`api::API_COMMANDS` / `DaemonCommands`)

| Command | Purpose |
| :--- | :--- |
| `list_connected_mcus` | Scans host serial ports excluding `/dev/ttyS*` and `INSTRUMENT_USB_IDS` |
| `scan_connected_instruments` | Probes USB bus for supported instruments (Owon VDS1022, FX2LP LA) |
| `list_supported_oscilloscopes` | Returns `Vec<OscilloscopeDescriptor>` for all compiled drivers |
| `get_project_electronics_status` | Returns `ProjectElectronicsStatus` (enabled flag, firmware language, envs, MCUs, instruments, toolchains) |
| `toggle_project_electronics_plugin` | Updates `[plugins.electronics].enabled` in `rula.toml` and returns refreshed status |
| `list_platformio_boards` | Searches cached `pio boards --json-output` catalog |
| `scaffold_firmware_project` | Generates C/C++ (PlatformIO) or MicroPython project under `firmware_dir` |
| `build_project_firmware` | Compiles PlatformIO project (returns typed error for MicroPython) |
| `flash_project_firmware` | Flashes target via `pio` / `probe-rs` / `esptool` or `mpremote` |
| `capture_oscilloscope` | Acquires dual-channel analog frame and metrics via `HardwareLockArbitrator` |
| `calibrate_oscilloscope` | Executes `probe_check` or `zero_offset` calibration |
| `capture_logic_analyzer` | Captures digital traces and decodes protocol packets via `sigrok-cli` |

### Error Hierarchy (`ElectronicsError` in `src-tauri/src/kopeng/electronics/errors.rs`)

Variants implementing `api::ApiError`: `DeviceNotFound { port }`, `ToolchainMissing { tool, hint }`, `SerialError { port, details }`, `BuildFailed { environment, details }`, `FlashFailed { port, details }`, `InvalidSetting { key, details }`, `Io(String)`, `FirmwareScaffoldFailed(String)`, `UnregisteredProject(String)`.

### Configuration Keys (`[plugins.electronics]` in `config.rs` / `rula.toml`)

| Key | Type | Scope | Default | Description |
| :--- | :--- | :--- | :--- | :--- |
| `enabled` | `boolean` | `project_only` | `false` | Enables Electronics & Embedded Lab for the project |
| `firmware_dir` | `string` | `cascade` | `"."` | Project-relative subdirectory containing firmware sources |
| `schematics_dir` | `string` | `cascade` | `"."` | Project-relative subdirectory containing EDA schematics (`.kicad_sch`) and SPICE netlists (`.cir`) |
| `default_baud_rate_bps` | `select_enum` | `cascade` | `"115200"` | Default serial monitor baud rate (`9600`..`921600`) |
| `flasher_tool` | `select_enum` | `cascade` | `"pio"` | Default flashing tool (`"pio"`, `"probe-rs"`, `"esptool"`) |
| `usb_flush_max_attempts_count` | `number` | `global_only` | `256` | Max bulk read attempts when draining stale USB packets |
| `firmware_upload_timeout_seconds` | `number` | `global_only` | `180` | Maximum wait time for `mpremote` and `pio` uploads |
| `firmware_language` | `string` (`rula.toml`) | project | — | Recorded by scaffolding (`"platformio"` or `"micropython"`) |

### MCP Tool Catalog (Agent Interface)

Registered/specified under `src-tauri/src/mcp.rs`:

1. **`electronics_detect_hardware`:** Parameters: none. Returns discovered instruments and development boards with IDs, categories, and connection states.
2. **`electronics_read_schematic`:** Parameters: `schematic_path` (optional relative `.sch` / `.kicad_sch`), `focus_component` (optional RefDes e.g. `"U1"`). Returns MCU pinout map, connected peripherals, and power rails.
3. **`electronics_capture_logic`:** Parameters: `sample_rate_hz` (int), `duration_ms` (int), `decoder` (optional string e.g. `"i2c"`, `"spi"`, `"uart"`), `pins` (map e.g. `{"scl": 0, "sda": 1}`). Returns decoded transaction list and WaveDrom JSON.
4. **`electronics_read_oscilloscope`:** Parameters: `channel` (`1` or `2`), `timebase_div_us` (int), `volts_div_mv` (int). Returns `HardwareDigest` ($V_{pp}$, $V_{rms}$, frequency, min/max voltages, signal quality metrics).
5. **`electronics_flash_firmware`:** Parameters: `target_environment` (optional string), `flasher` (optional `"pio"` or `"probe_rs"`). Returns build status, compilation warnings/errors, and upload confirmation.
6. **`electronics_run_simulation`:** Parameters: `netlist_file` (path to `.cir` or raw SPICE netlist), `analysis_type` (`"op"`, `"ac"`, or `"tran"`). Returns numerical results and ASCII frequency/transient curve.

## Implementation Status

Verified against `src-tauri/src/` and `src/`:

| Component | Status | Verification Notes |
| :--- | :--- | :--- |
| Native Owon VDS1022 USB driver (`owon_vds1022.rs`) | Implemented | `FLASH_CALIBRATION_CACHE`, `LAST_APPLIED_CONFIG`, `HardwareLockArbitrator`, `OscilloscopeDescriptor` present |
| Logic analyzer driver (`fx2lafw.rs`) & `HardwareLockArbitrator` (`arbitrator.rs`) | Implemented | Present under `src-tauri/src/kopeng/electronics/instruments/` |
| MCU/Serial discovery (`discovery.rs`) & toolchain probe (`toolchain.rs`) | Implemented | `INSTRUMENT_USB_IDS` exclusion and 6-tool probe present |
| Firmware catalog, C/C++/MicroPython scaffolding, build/flash (`firmware.rs`) | Implemented | All 12 electronics commands registered in `src-tauri/src/api.rs` and `src/types/daemonApi.ts` |
| `HardwareInstrument` trait abstraction | Partial | Driver functions and `driver.rs` structs are implemented directly rather than behind an `#[async_trait] pub trait HardwareInstrument` |
| Virtual Oscilloscope, Logic Analyzer & DSP UI | Implemented | `ProjectElectronicsTab.vue`, `OscilloscopeView.vue`, `LogicAnalyzerView.vue`, `src/stores/electronicsStore.ts`, `src/utils/electronicsDsp.ts` |
| Live Serial Monitor | Partial | Configuration key `default_baud_rate_bps` exists; full interactive UART console streaming was not verified in `electronics/` |
| EDA Schematic Ingestion (`quick-xml` EAGLE / KiCanvas) | Roadmap | `schematics_dir` config key and `.kicad_*` snapshot exclusions exist; `kopeng::electronics::eda` and KiCanvas viewer are not yet in `src-tauri/src` or `src/` |
| Headless SPICE runner (`ngspice` / `xyce`) | Roadmap | Not yet present in `src-tauri/src/kopeng/electronics/` (see also [CAD and PCB Integration](03-cad-and-pcb-integration.md) for PCBCraft SPICE bridge) |
| Electronics MCP tools (`electronics_*` in `mcp.rs`) | Roadmap | None of the 6 `electronics_*` tool names are registered in `src-tauri/src/mcp.rs` yet |

## Related Specs

- [CAD and PCB Integration](03-cad-and-pcb-integration.md)
- [Plugin Architecture](../01-architecture/01-plugin-architecture.md)
- [Configuration Schema and Cascade](../01-architecture/02-configuration-schema-and-cascade.md)
- [Design System Foundations](../07-ui/01-design-system-foundations.md)

Supersedes legacy: `31_electronics_workstation_and_instrumentation_engine.md`
