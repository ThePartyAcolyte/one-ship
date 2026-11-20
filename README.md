# ONE-SHIP

A personal agentic IDE for Arch Linux (Omarchy). Rust daemon, Tauri window, agents as the crew.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2021-orange.svg?logo=rust)](src-tauri/Cargo.toml)
[![Tauri](https://img.shields.io/badge/Tauri-v2-24C8D8.svg?logo=tauri)](src-tauri/)
[![Vue](https://img.shields.io/badge/Vue-3-4FC08D.svg?logo=vuedotjs)](package.json)
[![OpenDictate](https://img.shields.io/badge/Voice-OpenDictate_D--Bus-blueviolet.svg)](https://github.com/ThePartyAcolyte/opendictate)

## Why ONE-SHIP

In *The Expanse*, the Belters say a ship is one body: the hull, the crew,
everything aboard. A station is one ship too, and so, if you zoom out far
enough, is humanity.

I borrowed that idea for a smaller problem. The tools we use to work with LLMs feel broken to me. The craftsmen hand-write every line and refuse to let go. The swarm people launch a thousand agents, burn tokens, and hope. 

I wanted a third way: not typing the `for` loop, but still in command. A
captain, not a gambler.

A terminal is a flood of text. An IDE is a thousand tools built for writing
code by hand. ONE-SHIP is neither. The runner (`agy`, `claude`, `opencode`)
is still the harness, and ONE-SHIP is the control panel around it, with no
noise. A spaceship has no jacuzzi, only what is vital for the trip. That is
the bar: what is vital to survive a project, from my very personal point of
view.

The goal is to talk to the crew instead of clicking: soldering a board,
probe in hand, saying "this pin looks wrong" and having the agent act on it.
We're not there yet. Agents are still too terse in chat. But that's the
direction.

It's a personal tool, built for my machine. This repo is a record of how I
work.

## What it replaces

The terminal workflow around agent CLIs: one pane per agent, one pane per
dev server, a multiplexer to keep them alive, and a lot of scrolling. In
ONE-SHIP a Rust daemon owns every process, and the window is only a view:
close it and the agents and services keep running.

It does not replace Gemini on the web or the Claude desktop app.

## Working inside it

1. **Add a project.** An existing one gets a read-only reconnaissance by an
   initializer agent, which writes its `rula.toml` and raises flags for
   what is missing (docs, todos, tech debt, profiler). A new one starts as a
   conversation about what to build, spec first.
2. **Talk to the crew.** Each agent has one continuous thread per project.
   Its spoken answer stays in the chat, and the technical payload (code,
   diffs, plans) goes to an artifact beside it.
3. **Delegate and approve.** Agents hand tasks to each other through the
   daemon's MCP server and ask for approval before anything privileged.
4. **Stay safe.** Every agent session is snapshotted on hidden git refs, so
   any file can be rolled back without touching the project history.
5. **Remember.** Each conversation ends in minutes (decisions, files,
   debt) stored in a database, searchable by meaning with a local vector
   index.

Services and tasks declared in `rula.toml` run under the same supervisor,
and every run is profiled continuously.

## Workspace & Sections

The workspace is organized into functional tabs built for focused engineering:

- **Overview:** Project vitals, git remote connectivity, feature activation toggles, and pending initialization badges.
- **Explorer:** File tree and CodeMirror 6 editor with syntax highlighting, inline document comments, and clipboard image pasting.
- **Journal:** Project memory center tracking chronological session minutes, the architectural decisions graph, actionable tasks (`project_todos`), and recorded technical debt (`project_technical_debt`).
- **Git & Snapshots:** Commit graph and safety net: automatic hidden git refs (`refs/one-ship/snapshots/`) with point-in-time diffs and granular per-file restores without altering official commit history.
- **Scripts:** Process grid supervising persistent daemons and transient build/test tasks declared in `rula.toml`, featuring automatic port conflict detection and live terminal buffers.
- **Profiling:** Continuous process tree telemetry sampling (CPU %, peak RSS memory, disk I/O, threads) stored in DuckDB for every run.
- **Skills:** Marketplace discovery and catalog of platform skills injected on-demand into agent contexts.
- **Preview:** Embedded responsive dev-server viewport for web frontends configured via `[preview]` in `rula.toml`.
- **RE Lab:** Reverse engineering gateway powered by REA (`morluto/rea`) for ELF binaries, Electron ASAR unpacking, and Ghidra headless decompilation.
- **Electronics:** Hardware instrumentation center featuring virtual oscilloscope knobs, logic analyzer waveforms, and MCU serial consoles.
- **Floating Chat Overlay:** Three-column brutalist agent hub: contact list with real-time lifecycle states, continuous conversation stream, and side-by-side technical artifact reader.

## Extensible Hardware Instrumentation Workbench

Modern software engineering frequently meets embedded hardware. Rather than coupling to proprietary vendor utilities, ONE-SHIP implements an extensible **hardware abstraction layer (`HardwareInstrument` trait)** designed to support any digital USB instrument via native driver plugins:

- **Digital Oscilloscopes:** Real-time waveform acquisition frames, timebase prescalers, hardware gain PGA, attenuation relays, and trigger levels (initial driver implemented for the Owon VDS1022 via native asynchronous USB `rusb`).
- **Logic Analyzers:** Multi-channel digital capture and protocol decoding (24MHz 8-channel fx2lafw / sigrok integration) for inspecting I2C, SPI, and UART bus transactions.
- **Microcontroller Toolchains:** System serial detection (`/dev/ttyUSB*`), PlatformIO Core compilation/uploading, and `probe-rs` flashing.
- **The Dual-Actor Paradigm:** While the human engineer uses a tactile virtual instrument UI with interactive canvas HUD cursors, autonomous AI agents receive token-efficient **Hardware Digests** ($V_{pp}$, $V_{rms}$, frequency, decoded packets, pinout tables) via strongly-typed MCP tools. This enables hands-free circuit debugging while probing or soldering.

## Voice Dictation: OpenDictate Integration

ONE-SHIP integrates natively with [**OpenDictate**](https://github.com/ThePartyAcolyte/opendictate), an omnipresent, privacy-first local voice dictation daemon for Linux:

- **D-Bus Session Bus:** Communicates over Freedesktop D-Bus (`org.kirulab.OpenDictate`).
- **Click-to-Reserve:** Focusing or clicking the agent chat input reserves a dictation session; blurring before speaking cancels cleanly.
- **Real-Time Streaming:** Transcriptions inferred locally via `faster-whisper` stream incrementally into the input field in real time.
- **Local & Private:** Voice audio stays 100% on-device, allowing natural spoken interaction with agents without cloud latency or leakage.

## Architecture

```
  Window (Tauri + Vue)          Agents (agy, claude, opencode)
        │  events / RPC                  │  MCP (HTTP, PID-attested)
        ▼                                ▼
  ┌──────────────────────────────────────────────────────────┐
  │                 oneship-daemon (Rust, Axum)              │
  │   process supervisor  ·  MCP server  ·  watchers         │
  │   snapshots  ·  profiler  ·  accounts  ·  hardware HAL   │
  └─────────────┬─────────────────┬─────────────────┬────────┘
                │                 │                 │
                ▼                 ▼                 ▼
             SQLite            DuckDB            LanceDB
        (state, minutes)     (profiling)    (semantic search)
                ▲
                │ D-Bus Session Bus
        ┌───────┴────────┐
        │  OpenDictate   │ (Local voice STT)
        └────────────────┘
```

The daemon is the only owner of processes and state. Agents run headless
over `stream-json` or ACP, never through a terminal emulator.

## Stack

| Layer | Technology |
| :--- | :--- |
| Daemon | Rust, Tokio, Axum, `thiserror`, `ts-rs`, `rusb`, `serialport` |
| Storage | SQLite (WAL), DuckDB, LanceDB with local BGE-M3 embeddings |
| Agents | `agy`, `claude`, `opencode` over `stream-json` / ACP, MCP |
| Voice Bus | Freedesktop D-Bus session bus with OpenDictate (`faster-whisper`) |
| Window | Tauri v2, Vue 3, TypeScript, Pinia, Tailwind CSS, CodeMirror 6 |
| Host | Arch Linux (Omarchy), Hyprland (Wayland), systemd user units |

## How it is developed

ONE-SHIP is built with the same kind of agents it orchestrates, under a
strict contract in [`AGENTS.md`](AGENTS.md):

- **Proposal first.** Nothing is created, changed or deleted before a
  written proposal is approved.
- **Specs are alive.** Each subsystem has a spec in [`docs/specs/`](docs/specs/),
  updated in the same change as the code.
- **One change, every layer.** Backend, frontend, prompts and specs move
  together.
- **Fail loudly.** No silent fallbacks, no dead code, no backward
  compatibility.
- **The developer runs the app.** Agents edit, build and check. I start
  and stop the processes.
- **A devlog per session**, in [`devlog/`](devlog/).

## Run it & Installation

### Quick Linux Install (`./install.sh`)

ONE-SHIP provides an interactive multi-distro installer wizard supporting Arch/Omarchy (`pacman`), Ubuntu/Debian (`apt-get`), Fedora (`dnf`), and openSUSE (`zypper`):

```bash
git clone https://github.com/ThePartyAcolyte/one-ship.git
cd one-ship
./install.sh
```

The wizard lets you choose core IDE + daemon installation alongside optional integrations (*RE Lab*, *Electronics + USB udev rules*, *DE-SIGMA*, *PCBCraft*, *SolidCraft binary check*, *OpenDictate*, and the *Omarchy Top Bar Widget*). Automated flags (`--profile minimal|standard|full`, `--features ...`, `--dry-run`, `--uninstall`) are documented via `./install.sh --help`. Missing integrations remain dynamically hidden inside the UI via `get_host_capabilities`.

### Development Launcher (`scripts/run_dev.sh`)

Components run as transient systemd user units without cluttered terminal windows:

```bash
scripts/run_dev.sh              # Start/rebuild daemon, watcher, Vite, and window
scripts/run_dev.sh daemon       # Build and restart daemon, start watchexec watcher
scripts/run_dev.sh gui          # Focus running window or launch the interface
scripts/run_dev.sh status       # JSON telemetry of every running unit
scripts/run_dev.sh logs daemon  # Follow unit journal (daemon|build|watch|vite|gui)
scripts/run_dev.sh stop all     # Terminate daemon, watchers, and supervised child processes
```

The daemon rebuilds automatically on `src-tauri` changes when `watchexec` is installed.

### Omarchy Top Bar Widget

ONE-SHIP provides an auto-adaptive Quickshell bar widget (`integrations/omarchy-bar/`, plugin ID `tech.kirulab.one-ship`) for the Omarchy desktop:
- **Live Telemetry (Always Active):** Reads `$XDG_RUNTIME_DIR/oneship-status.json` to display active/working/idle agent counts, pending approvals, and running services.
- **Production Mode:** Manages `oneship-daemon.service` via `systemctl --user` and launches/focuses `one-ship-launch` / `one-ship`.
- **Automatic Development Mode:** When symlinked from a source checkout with active `oneship-dev-*` units (or without `oneship-daemon.service`), automatically exposes `scripts/run_dev.sh` Cargo build progress, `watchexec` watcher, Vite, and unit journal logs.

```bash
ln -s "$PWD/integrations/omarchy-bar" ~/.config/omarchy/plugins/tech.kirulab.one-ship
omarchy-shell shell rescanPlugins
omarchy plugin enable tech.kirulab.one-ship
omarchy-restart-shell
```

Left-click focuses or launches the interface; right-click opens the status and control popup.

## Documentation

The full architectural index is in [`docs/README.md`](docs/README.md). Key entry points:
- [00. Manifesto](docs/00_manifesto.md)
- [04. Dictation & NLP (OpenDictate)](docs/specs/04_dictation_and_nlp.md)
- [08. Native Process Supervisor](docs/specs/08_native_process_supervisor.md)
- [12. Streamable HTTP MCP Daemon](docs/specs/12_streamable_http_mcp_daemon.md)
- [25. Universal Telemetry & Profiling](docs/specs/25_universal_telemetry_and_profiling_engine.md)
- [30. Conversation & Context Continuity](docs/specs/30_agent_thread_conversation_and_context_continuity.md)
- [31. Electronics & Instrumentation](docs/specs/31_electronics_workstation_and_instrumentation_engine.md)

## Status & License

Active pre-1.0 development (`0.2.x`); Linux workstation installation via [`install.sh`](install.sh). When it reaches 1.0 across major Linux distros, I'll come back to the parked idea of an interface in Belter creole.

Licensed under the [MIT License](LICENSE). Third-party fonts (OFL-1.1), assets, and dependencies maintain their respective open-source licenses.
