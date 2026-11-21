# ONE-SHIP

A multidisciplinary maker's workstation and autonomous agent control center for Linux. Rust daemon, Tauri window, headless agents as the crew.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2021-orange.svg?logo=rust)](src-tauri/Cargo.toml)
[![Tauri](https://img.shields.io/badge/Tauri-v2-24C8D8.svg?logo=tauri)](src-tauri/)
[![Vue](https://img.shields.io/badge/Vue-3-4FC08D.svg?logo=vuedotjs)](package.json)
[![Specs](https://img.shields.io/badge/Docs-41_Living_Specs-1f6feb.svg)](docs/specs/README.md)

## Why ONE-SHIP

In *The Expanse*, the Belters say a ship is one body: the hull, the crew, everything aboard. A station is one ship too, and so, if you zoom out far enough, is humanity.

I borrowed that idea for a smaller problem. The tools I use to work feel broken and disconnected. LLMs, CLIs, IDEs, CADs, EDAs, Project Managers and yada yada. Can I put everything I need in one ship? 

We live in very weird times. We try to balance *artificial intelligence* with *human intelligence*. Many programmers have lost the joy of writing code and while the craftsmen hand-write every line and refuse to let go, the swarm people launch a thousand agents, burning a zillion tokens hoping for the best.

I like to think there is another way: not typing the `for` loop, but still in command of every single desision. A captain, not a gambler.

One of the core principles of Mazda's design philosophy is *Jinba Ittai*: person and horse as one body. ONE-SHIP is the bridge, connecting tools, human and AIs. The runner is still the harness, and ONE-SHIP is the control panel around it, packed with every tool *(I think)* I need. A spaceship built with a lot of tradeoffs, for my ***very personal adventure***.


## What it replaces

Coming from the ERP world, I apreciate *(a little too much)* the value of centralized data and tools. I'm also a maker with ADHD, so keeping my little home lab in order and keeping track of my projects has always been a strugle. This tool is not a replacement for a fully fledged IDE. Is just a different aproach. 

## Working inside it

1. **Add or bootstrap a project.** Existing repositories receive read-only reconnaissance from an initializer agent that analyzes the project, proposes an initial config file, organizes documentations and specs, defines the crew (agents personas, that will specialize depending on the project's nature) and sets the initial state. New projects starts with a grill and then go through the same process.
2. **Talk to the crew across continuous threads.** Agents receive a name, specialty and a portrait. The aproach here is that conversation occurs in a natural *(and TTS ready)* language, much like a colleage would talk to you. The technical stuff is separated in an artifact attached to the message. Underneath, there still a CLI running the agent.
3. **Control everything by voice or UI.** Press `Alt+V` inside any text field to dictate natively with **Gemini 3.5 Transcribe Live**, or outside a field to invoke the **Global Voice Controller** ("Ship Brain") backed by Gemini Live, PipeWire audio, and the **Unified Capability Core** (`capability_discover` / `capability_execute`) across all project domains regardless of which tab is active.
4. **Delegate, approve, and rotate quota.** Agents delegate tasks through the daemon's PID-attested HTTP MCP bus and block on explicit user approval for privileged actions (GitHub/GitLab draft PRs, deletions, migrations). Multi-account management (`google`, `deepseek`, `claude`) encrypts tokens with AES-256-GCM and rotates model pools when quotas tighten.
5. **Stay safe with hidden snapshots.** Every agent turn and milestone can be snapshotted onto domain-scoped hidden Git refs (`refs/one-ship/snapshots/`) for per-file diffing and rollback without polluting commit history.
6. **Remember everything in a causal graph.** Each session closes with structured minutes (`agent_session_journal`), tasks, and technical debt in SQLite, indexed alongside Tree-sitter code symbols, Markdown specs, and Git history in a local **LanceDB + BGE-M3** vector store and `knowledge_edges` graph.

## Core Subsystems

Based on the [40 living specifications](docs/specs/README.md):

- **Autonomous Agent Runtime & MCP Bus:** Headless multi-turn supervision (`agy`, `claude` over `stream-json`; `opencode` with DeepSeek over ACP) with process-group isolation, boot-time adoption, on-demand skill symlinking (`.agents/skills/`), and a 59-tool HTTP MCP server attested via Linux `/proc`.
- **Voice & Unified Capability Core:** Native Gemini Live dictation (`gemini-3.5-transcribe-live`) with revision-safe transcript reconciliation, VAD rollover and replay into focused fields, plus the global Gemini Live conversational orchestrator wired to a navigation-agnostic `CapabilityProvider` registry and persistent voice sessions with a 65K TPM budget guard.
- **Projects, Git & Forge Collaboration:** 4-level cascading configuration (`rula.toml` ↔ `~/.config/one-ship/config.toml`), supervised persistent services and one-shot tasks with port-conflict release, cyclic logging, dual-signification Git workspace, provider-neutral Forge workflows (GitHub via `gh`, GitLab via `glab`, persistent triage inbox, `Fork ↔ Upstream` flow), opt-in Kanban/Schedule/Gantt/Milestones, and a sandboxed CodeMirror 6 explorer with inline document comments.
- **Observability & Continuous Profiling:** Every supervised service and task run is sampled continuously into **DuckDB** (`CPU %`, tree `RSS`, threads, disk I/O) and can wrap profiler engines (`samply`, `py-spy`, `v8prof`) via declarative `manifest.toml` plugins.
- **Multidisciplinary Hardware, Design, CAD/PCB & RE Lab:**
  - *Electronics:* Native USB drivers via `rusb` (**Owon VDS1022** oscilloscope) and **FX2LP / `sigrok-cli`** logic analyzer, hardware lock arbitration, PlatformIO / `probe-rs` flashing, and udev rules (`packaging/udev/99-oneship-instruments.rules`); EDA schematic ingestion and headless `ngspice` simulation remain on the roadmap.
  - *Design, CAD & PCB:* Bridges and MCP gateways for **One-Ship Designer** (`desigma` fork, `/mcp/desigma`), **PCBCraft** (`/mcp/pcbcraft`), and **SolidCraft** (prebuilt 3D CAD binary integration).
  - *RE Lab:* Supervised `rea` HTTP engine with a multi-project FIFO heavy-job queue (`/mcp/rea`) for binary, ASAR, and firmware analysis.
- **OS-Adaptive Linux Desktop & Host Gating:** `get_host_capabilities` probes installed runners, optional companion tools, `gdbus`, clipboard backend (`wl-paste` on Wayland, `xclip` on X11), package manager (`pacman`, `apt-get`, `dnf`, `zypper`), and desktop session. Optional workspace tabs and runner entries hide dynamically when their host tools are absent.

## Workspace & Sections

The workspace adapts dynamically to enabled `rula.toml` plugins and `HostCapabilities`, exposing up to 16 top-level views plus the agent overlay:

- **Overview:** 4-column plugin-driven dashboard (project vitals, Git/upstream metrics, active agents, tasks, profiling, and pending `[! INIT]` flags).
- **Explorer:** Sandboxed file tree and CodeMirror 6 editor with inline review comments, clipboard image paste (Wayland/X11), and multi-tier artifact views.
- **Journal:** Session minutes timeline (`agent_session_journal`), actionable to-dos, and technical debt tracking.
- **Project:** Opt-in multidisciplinary planning (`Tasks`: Kanban Board, Schedule, Gantt, Milestones) and `Forge` (Issues, PRs/MRs, Releases, persistent Inbox/Triage).
- **Git & Snapshots:** Commit graph, working-tree diffs, AI-assisted commit messages, branch selector, custom VCS presets, and domain-scoped hidden snapshots.
- **Scripts:** Supervised persistent services, one-shot tasks, and shells with live rolling output and port-conflict guards.
- **Profiling:** Run-bound DuckDB process-tree metrics, run comparison (`DIFF`), and flamegraph artifact launch.
- **Skills:** Project-scoped skill catalog and JIT toggling between `.agents/skills/` and `.agents/.skills-disabled/`. Search and import new skills directly from SkillsMP. 
- **Graph:** Knowledge graph explorer and transversal semantic search (`Ctrl + K` Omnisearch).
- **Preview:** Embedded dev-server viewport for web frontends configured under `[preview]`.
- **RE Lab / Electronics / Design / CAD / PCB:** Domain workbenches gated by host tool availability and `rula.toml` plugin flags.
- **Settings:** Per-project `rula.toml` editor with external-change watcher, unsaved-change guard, and tab visibility controls (alongside the global Settings & Integrations modal).
- **Agent Taskbar & Floating Chat Overlay:** Bottom taskbar with portrait DNA, real-time status/quota gauges, and a docked or floating three-column chat + artifact + approval drawer.

## Architecture

```
  Window (Tauri + Vue)             Headless Agents (agy, claude, opencode)
        │  /api/v1/call/* (RPC)            │  /mcp, /mcp/rea,
        │  /api/events (SSE)               │  /mcp/desigma, /mcp/pcbcraft
        ▼                                  ▼  (HTTP, Linux /proc PID-attested)
  ┌─────────────────────────────────────────────────────────────────────────┐
  │                       oneship-daemon (Rust, Tokio, Axum :8765)          │
  │  Process Supervisor · UI State Machine · Unified Capability Core        │
  │  Kopeng Plugins · Git/Snapshots · Multi-Account · Hardware USB/Serial   │
  │  Native Gemini Dictation + Global Voice Controller (Gemini Live/PipeWire) │
  └──────────────┬────────────────────────┬────────────────────────┬────────┘
                 │                        │                        │
                 ▼                        ▼                        ▼
              SQLite                   DuckDB                   LanceDB
      (state, minutes, FTS5,     (continuous tree      (BGE-M3 INT8 embeddings +
       knowledge_edges, logs)     profiling samples)    Tree-sitter RAG chunks)

  [Microphone] ──pw-record──► [Native Gemini Live STT] ──text──► focused field
```

## Stack

| Layer | Technology |
| :--- | :--- |
| Daemon | Rust, Tokio, Axum (`:8765`), `thiserror`, `ts-rs`, `rusb`, `serialport` |
| Storage | SQLite (WAL + FTS5), DuckDB, LanceDB + local `BGE-M3` INT8 ONNX (`fastembed` / `ort`) + Tree-sitter |
| Agents | `agy`, `claude` (`stream-json`), `opencode` + DeepSeek (ACP), Streamable HTTP MCP |
| Voice | PipeWire (`pw-record`/`pw-play`) + Gemini Live (`gemini-3.5-transcribe-live` dictation and "Ship Brain") |
| Window | Tauri v2, Vue 3, TypeScript, Pinia, Tailwind CSS, CodeMirror 6, AppIndicator/SNI tray |
| Host Targets | Linux workstations: Arch / Omarchy (Hyprland), Ubuntu (GNOME), Fedora Workstation, Linux Mint (Cinnamon) |

## Installation & Running

### Non-Dev Linux Installation (`./install.sh`)

ONE-SHIP includes an interactive multi-distro installer ([`install.sh`](install.sh)) designed to build and install production (non-dev) co-located binaries (`one-ship`, `oneship-daemon`, `one-ship-launch`), `templates/`, `.desktop` entry, `hicolor` icons, and `systemd --user` service (`oneship-daemon.service`) under `~/.local` (or `--prefix <dir>`).

**1. Prerequisites on a fresh Linux machine (`git`, `curl`, and Rust via `rustup`):**

```bash
# Ubuntu / Linux Mint / Debian:
sudo apt update && sudo apt install -y git curl

# Fedora Workstation:
# sudo dnf install -y git curl

# Arch / Omarchy:
# sudo pacman -S --needed git curl

# Install modern Rust toolchain (and Node.js >= 20 is required; install.sh offers Node 22 LTS on Debian/Ubuntu/Mint if apt ships v18):
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
```

**2. Clone and run the installer wizard:**

```bash
git clone https://github.com/ThePartyAcolyte/one-ship.git
cd one-ship
./install.sh
```

**3. First-run setup:**

- Install at least one supported AI CLI runner (`claude`, `opencode`, or `agy`) before launching autonomous agents.
- Launch **ONE-SHIP** from your desktop app launcher (or `one-ship-launch`). If workspace prerequisites (projects directory, Git identity, `gh`, or CLI runners) are missing, the built-in **Setup & Readiness** checklist opens automatically to guide you.

Useful flags:

```bash
./install.sh --profile minimal   # Core IDE + daemon + systemd unit + desktop entry
./install.sh --profile standard  # Core + RE Lab + Electronics (+udev) + Omarchy widget
./install.sh --profile full      # Standard + DE-SIGMA + PCBCraft + SolidCraft binary lookup
./install.sh --dry-run           # Print planned actions without modifying the system
./install.sh --uninstall         # Remove installed binaries, service, desktop entry, and templates
```

#### Multi-Distro Testing Matrix & Desktop Adaptation

The non-dev installer and desktop runtime are built for **Arch / Omarchy** and targeted at three primary non-dev test OSes:

| Target OS | Package Manager | Window & Tray Behavior | Current Test Status |
| :--- | :--- | :--- | :--- |
| **Ubuntu (GNOME)** | `apt-get` | Floating DE mode (CSD header controls, 8-edge resize, AppIndicator/SNI tray) | **Active testing in progress** |
| **Fedora Workstation** | `dnf` | Floating DE mode (CSD header controls, 8-edge resize, AppIndicator/SNI tray) | Planned / verification pending |
| **Linux Mint (Cinnamon)** | `apt-get` | Floating DE mode (CSD header controls, 8-edge resize, AppIndicator/SNI tray) | Planned / verification pending |
| **Arch / Omarchy (Hyprland)** | `pacman` | Frameless tiled mode + unified `integrations/omarchy-bar/` widget | Primary development host |

Recent Ubuntu and floating-desktop adaptations built into the installer and launcher include:
- **Ubuntu/Debian (`apt-get`) toolchain dependencies:** installs `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev` (tray support on GNOME/Cinnamon), `protobuf-compiler` (required by `lance-encoding`), `libudev-dev`, `libusb-1.0-0-dev`, `libxdo-dev`, `librsvg2-dev`, and `libssl-dev`.
- **Floating-desktop window & tray lifecycle (`!is_hyprland_session()`):** outside Hyprland, `one-ship` mounts draggable header controls (Minimize / Maximize / Close), 8-direction `4px` edge resize handles (`startResizeDragging`), and a native System Tray icon (`one-ship-tray`) so closing the window hides to the tray while `oneship-daemon.service` stays alive (use tray **Quit (Stop Daemon)** to stop both).
- **Detached production launcher (`one-ship-launch`):** ensures `oneship-daemon.service` is active, detaches the GUI via `setsid` (logs in `$XDG_STATE_HOME/one-ship/launcher.log`, or pass `--foreground`), and retries once with `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1` if WebKitGTK exits abnormally within 5 seconds.
- **Session portability:** Wayland (`wl-paste`) and X11 (`xclip`) clipboard image paste, fallback detection for standard terminals (`gnome-terminal`, `konsole`, `xfce4-terminal`, `x-terminal-emulator`, `xterm`), and native dictation that degrades with an explicit error when PipeWire or the OS keyring is unavailable.

> [!NOTE]
> Sandboxed Flatpak and Snap packages are intentionally unsupported because the daemon relies on direct Linux `/proc/<pid>` caller attestation, `systemd --user` units, USB/serial device nodes, and host agent CLIs.

### Development Launcher (`scripts/run_dev.sh`)

For contributors working from source, components run as transient `systemd --user` units (`oneship-dev-*`):

```bash
scripts/run_dev.sh              # Start/rebuild daemon, watcher, Vite, and window
scripts/run_dev.sh daemon       # Build and restart daemon, start watchexec watcher
scripts/run_dev.sh gui          # Focus running window or launch the interface
scripts/run_dev.sh status       # JSON telemetry of every running unit
scripts/run_dev.sh logs daemon  # Follow unit journal (daemon|build|watch|vite|gui)
scripts/run_dev.sh stop all     # Terminate daemon, watchers, and supervised child processes
```

### Omarchy Top Bar Widget

On Omarchy desktops, `integrations/omarchy-bar/` (`tech.kirulab.one-ship`) reads `$XDG_RUNTIME_DIR/oneship-status.json` and auto-switches between development units (`oneship-dev-*`) and production (`oneship-daemon.service` + `one-ship-launch`):

```bash
ln -s "$PWD/integrations/omarchy-bar" ~/.config/omarchy/plugins/tech.kirulab.one-ship
omarchy-shell shell rescanPlugins
omarchy plugin enable tech.kirulab.one-ship
omarchy-restart-shell
```

## How it is developed

ONE-SHIP is developed under strict engineering governance in [`AGENTS.md`](AGENTS.md) and [`docs/DESIGN_RULES.md`](docs/DESIGN_RULES.md):

- **Proposal first.** Every change requires an explicit technical proposal before editing files.
- **Specs are alive.** Subsystem specifications in [`docs/specs/`](docs/specs/README.md) stay synchronized with code in the same cycle and record verified `Implementation Status` (`Implemented`, `Partial`, or `Roadmap`).
- **One change, every layer.** Rust backend (`ts-rs` bindings via `scripts/run_dev.sh export-bindings`), TypeScript/Vue frontend, prompts/skills, and specs move together.
- **Fail loudly.** Typed `thiserror` errors, explicit config validation, no silent fallbacks, and no dead code.
- **Session devlogs.** Every work session is recorded in [`devlog/`](devlog/).

## Documentation

Start at [`docs/README.md`](docs/README.md) or browse the categorized specification tree in [`docs/specs/README.md`](docs/specs/README.md):

| Area | Entry Point |
| :--- | :--- |
| Vision, Language & Stack | [Manifesto](docs/00_manifesto.md) · [Ubiquitous Language](docs/01_ubiquitous_language.md) · [Architecture Stack](docs/02_architecture_stack.md) |
| Architecture & Packaging | [01-architecture/](docs/specs/01-architecture/README.md) (Kopeng, config cascade, skills, HTTP MCP, daemon RPC, migrations, Linux install) |
| Agents & Continuity | [02-agents/](docs/specs/02-agents/README.md) (Supervisor, threads, chat overlay, multi-account, quota, snapshots, skills, portraits) |
| Projects, Git & Forge | [03-projects/](docs/specs/03-projects/README.md) (Registry/scripts, Git cockpit, logging, remotes, Forge, inbox, milestones, artifacts, greenfield) |
| Observability & Graph | [04-observability/](docs/specs/04-observability/README.md) (Session lifecycle, SQLite devlog, LanceDB/BGE-M3 graph, DuckDB profiler) |
| Voice & Capability Core | [05-voice/](docs/specs/05-voice/README.md) (Native Gemini dictation, Gemini Live controller, Unified Capability Core) |
| Hardware, CAD & RE | [06-hardware-and-cad/](docs/specs/06-hardware-and-cad/README.md) (Electronics/USB, One-Ship Designer, SolidCraft & PCBCraft, REA) |
| UI & Design System | [07-ui/](docs/specs/07-ui/README.md) (Tokens, component library, chrome, daemon UI state machine, settings/overview, Lang Belta i18n) |
| Legacy Mapping | [docs/legacy/MAPPING.md](docs/legacy/MAPPING.md) (Traceability from original 39 numbered specs) |

## Status & License

Active pre-1.0 development (`0.2.x`). Core daemon, agent supervision, session continuity, Forge/PM, semantic knowledge graph, global voice controller, and multi-distro Linux packaging are implemented, while non-dev installer verification across Ubuntu, Fedora Workstation, and Linux Mint — plus select hardware/CAD sub-stages (e.g., SPICE simulation, BOM sourcing) — remain in active testing or roadmap as tracked in [`TODO.md`](TODO.md) and each spec's `Implementation Status`. When `1.0` stabilizes across target Linux distros, I'll revisit the parked Lang Belta interface tier.

Licensed under the [MIT License](LICENSE). Third-party fonts (`OFL-1.1`), dependencies, and optional external sprite packs maintain their respective licenses.
