# Spec 41: Linux Packaging, Multi-Distro Compatibility & Host Capabilities

**Document Version:** 0.2.1  
**Domain:** Distribution, Linux Multi-Distro Portability, Host Capability Gating & Installer Wizard  
**Status:** Implemented (Phases 0–5 Complete; Manual Host/Distro Runtime Verification Tracked in `TODO.md`)  
**Related Specs:** [Spec 10 (Configuration)](10_configuration_schema_and_cascade.md), [Spec 12 (Daemon & Packaging Policy)](12_streamable_http_mcp_daemon.md), [Spec 21 (Agent Pixel Portrait DNA)](21_agent_pixel_portrait_dna.md), [Spec 32 (One-Ship Designer)](32_oneship_designer_integration.md), [Spec 33 (CAD & PCB)](33_cad_and_pcb_integration.md)

---

## 1. Scope & Target Platform Policy

ONE-SHIP targets **Linux workstations** across major distributions (Arch/Omarchy, Ubuntu/Debian, Fedora, openSUSE). macOS and Windows are explicitly out of scope so the daemon can retain direct Linux `/proc` process-attested MCP identity (`caller_identity.rs`), `systemd` user-session supervision, `rusb` / `serialport` hardware access, and Freedesktop D-Bus IPC without sandboxed container compromises (Flatpak/Snap are incompatible with `/proc/<pid>/fd/*`, `systemd-run`, and host agent CLIs).

Distribution focuses on:
1. **Repository clone + interactive `./install.sh` feature wizard** (primary installation path).
2. **Native system packages** (`PKGBUILD`/AUR, `.deb`, `.rpm`) installing side-by-side binaries (`one-ship`, `oneship-daemon`) in the same prefix, shared templates/resources, and a user `systemd` unit.

---

## 2. Phased Architecture & Implementation Plan

### Phase 0 — Pre-Publication Hygiene & Portrait Licensing (Implemented)
- **Portrait Licensing Isolation:** Paid/restricted sprite packs (`extras/Pixel-Portraits-*`, `public/portraits/`) are excluded from public repository distribution via `.gitignore`.
- **Procedural SVG Pixel Portrait Fallback:** `AppAgentPortrait.vue` parses `AgentPortraitDna` (`Bxx:Ex:Hxx:BDxx:Axx`) and renders a deterministic, MIT-clean procedural pixel-art portrait directly onto the `64x64` canvas whenever `/portraits/manifest.json` is absent. Users who locally own a sprite pack can still generate `/public/portraits/` via `scripts/build_portrait_catalog.mjs` (`ONESHIP_PORTRAITS_SRC` or `extras/`).
- **Spec & Verification Hygiene:** Remove personal filesystem paths and personal emails from living specs; refine `scripts/verify_rules.sh` regexes to avoid false positives on `HashMap<String, String>`.

### Phase 1 — Relocatable Assets, `$PATH` Resolution & Production Service (Implemented)
- **Relocatable `TEMPLATES_ROOT` (`kopeng::skill`, `config`):** Resolve canonical templates (`skills/`, `prompts/`, `runners/`) in deterministic order instead of hardcoding `CARGO_MANIFEST_DIR` at compile time:
  1. `ONESHIP_TEMPLATES_DIR` (if set, must exist and be a directory; fails loudly otherwise).
  2. `<current_exe_dir>/../share/one-ship/templates` (standard FHS prefix: `~/.local/share/one-ship/templates` or `/usr/local/share/one-ship/templates`).
  3. `<current_exe_dir>/templates` (portable bundle directory).
  4. `$XDG_DATA_HOME/one-ship/templates` or `$HOME/.local/share/one-ship/templates`.
  5. `/usr/local/share/one-ship/templates` and `/usr/share/one-ship/templates`.
  6. Compile-time dev tree (`<CARGO_MANIFEST_DIR>/../templates`).
- **Owon FPGA Firmware (`owon_vds1022.rs`):** Search disk overrides (`ONESHIP_RESOURCES_DIR`, prefix `share/one-ship/resources`, dev paths) first, then fall back to compile-time embedded bitstreams (`include_bytes!`).
- **Executable `$PATH` Lookup (`desktop::resolve_executable`):** External companion binaries (`desigma`, `pcbcraft`, `solidcraft`) resolve both explicit paths and bare executable names via `$PATH` (`solidcraft` defaults normalized to `"solidcraft"` and `"solidcraft-cli"`).
- **Production `systemd --user` & Desktop Entry (`packaging/`):** Provide `packaging/systemd/oneship-daemon.service` (`KillMode=mixed`, `TimeoutStopSec=30`) and `packaging/one-ship.desktop` so `one-ship` and `oneship-daemon` run outside `scripts/run_dev.sh`. Both binaries remain co-located in the same `bin/` directory to satisfy `caller_identity::operator_app_exe()`.

### Phase 2 — Linux Multi-Distro Portability (Implemented)
- **Distro-Aware Diagnostics & Tool Hints (`diagnostic.rs`, `rea.rs`, `toolchain.rs`, UI):** Replaced Arch-only `pkexec pacman` assumptions with `/etc/os-release` + `PATH` package-manager detection (`pacman`, `apt-get`, `dnf`, `zypper`) and distro-aware `install_hint` / `auto_installable` metadata.
- **Clipboard Fallback (`documents.rs`):** `paste_clipboard_image` supports Wayland (`wl-paste`) and X11 (`xclip`) based on session environment and binary presence.
- **Desktop & Terminal Generalization (`desktop.rs`, `config.rs`, `accounts.rs`):**
  - Short-circuits `hyprctl` calls immediately when `HYPRLAND_INSTANCE_SIGNATURE` or `hyprctl` is absent (`is_hyprland_session()`).
  - Falls back to a detached Unix `process_group(0)` when `systemd-run` or user D-Bus session is absent.
  - Expands `system.preferred_terminal` with standard DE terminals (`gnome-terminal`, `konsole`, `xfce4-terminal`, `x-terminal-emulator`, `xterm`) and auto-detects an installed fallback terminal with terminal-specific execution flags (`--` vs `-e` vs `start --` vs bare).
- **WebKitGTK Startup Retry (`packaging/bin/one-ship-launch`, `main.rs`, `lib.rs`):** Launcher wrapper ensures `oneship-daemon.service` is active and retries `one-ship` once with `ONESHIP_WEBKIT_LAUNCHER_RETRY=1` / `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1` (logged at `WARN`) on abnormal early exit within 5 seconds.

### Phase 3 — `HostCapabilities`, Dynamic Feature Gating & OpenDictate Decoupling (Implemented)
- **Host Capability Registry (`get_host_capabilities` in `diagnostic.rs`, `coreStore.ts`):** Exposes a daemon snapshot reporting installed runners (`installed_runners`), optional domain integrations (`rea`, `electronics`, `solidcraft`, `pcbcraft`, `desigma`, `voice_controller`), `opendictate_available`, `gdbus_available`, `omarchy_available`, `hyprland_available`, `clipboard_available`, and `package_manager`.
- **Dynamic UI & Settings Gating:**
  - `ProjectWorkspace.vue` gates optional workspace tabs (`electronics`, `design`, `cad`, `pcb`, `rea`) with `coreStore.isIntegrationAvailable(...)` and includes `pcb` and `hostCapabilities` in the active-tab fallback watcher.
  - `ProjectConfigPanel.vue` and `SettingsModal.vue` hide unavailable plugin categories and filter runner dropdowns to `installed_runners` (while keeping already-configured values selectable).
  - `ProjectConfigPanel.vue` displays a single missing-integration warning when an imported project's `rula.toml` enables a feature missing on the host; Global `SettingsModal.vue` adds an **Integrations** tab for capability inspection, custom binary path overrides, and live refresh.
- **OpenDictate Decoupling (`kopeng/opendictate.rs`, `voiceStore.ts`):**
  - `VoiceReservationResult` distinguishes `service_available: false` from `busy`, stopping the 1.2s polling loop and false `"OpenDictate busy"` state on chat input focus.
  - `FloatingChatOverlay.vue` and `ProjectWorkspace.vue` hide OpenDictate controls when `!coreStore.isOpendictateAvailable`.
  - `spawn_dbus_monitor` skips monitoring cleanly when `gdbus` or `DBUS_SESSION_BUS_ADDRESS` is absent and checks `$XDG_RUNTIME_DIR/opendictate_state.json` before `/tmp/opendictate_state.json`.

### Phase 4 — Unified Production & Dev Omarchy Bar Widget (Implemented, Debt #48)
- Unified `integrations/omarchy-bar/` (`tech.kirulab.one-ship`) to always read the daemon's read-only atomic snapshot at `$XDG_RUNTIME_DIR/oneship-status.json` (`supervisor::write_runtime_status_snapshot` / `remove_runtime_status_snapshot`) and auto-adapt between **Development mode** (`run_dev.sh` Cargo build progress + `oneship-dev-*` unit controls when symlinked from a checkout) and **Production mode** (`oneship-daemon.service` + `one-ship-launch`/`one-ship`), so live agent/approval/services telemetry is continuously dogfooded during development without maintaining a separate `omarchy-bar-dev` plugin.

### Phase 5 — Interactive `./install.sh` Wizard & Companion Repositories (Implemented)
- Implemented [`install.sh`](../../install.sh) at repository root with:
  - Multi-distro detection (`/etc/os-release` + `pacman`/`apt-get`/`dnf`/`zypper`, AUR helper, Wayland/X11, Hyprland, Omarchy Bar), `--profile <minimal|standard|full>`, `--features <list>`, `--prefix <dir>`, `--repos-dir <dir>`, `--skip-deps`, `--yes`, `--dry-run`, and `--uninstall`.
  - Co-located release build and installation of `one-ship`, `oneship-daemon`, `one-ship-launch`, shared `templates/`, `packaging/one-ship.desktop`, and `packaging/systemd/oneship-daemon.service`.
  - Modular feature steps:
    - *RE Lab (`rea`):* reverse-engineering packages + optional `npm install -g rea-agents@latest`.
    - *Electronics (`electronics`):* `ngspice` / `sigrok-cli` + [`packaging/udev/99-oneship-instruments.rules`](../../packaging/udev/99-oneship-instruments.rules) (Owon `0x5345:0x1234`, Saleae `0x0925:0x3881`, Cypress FX2 `0x04b4:0x8613`).
    - *DE-SIGMA (`design`):* detects `oneship-designer` & `desigma-mcp` on `$PATH` -> checks local clone -> prompts to clone `ThePartyAcolyte/desigma` (`oneship` branch), builds and installs into `<prefix>/bin`.
    - *PCBCraft (`pcb`):* detects `pcbcraft` & `pcbcraft-cli` on `$PATH` -> checks local clone -> prompts to clone `tangency-labs/pcbcraft` (`main` branch) and compiles with Spec 33 §1.2 memory bounds (`-j 2`, `LTO=false`, `CODEGEN_UNITS=4`, `systemd-run --user --scope` `8G`/`10G`).
    - *SolidCraft (`cad`, binary-only exception):* checks `$PATH` only and guides/installs prebuilt packages (`solidcraft-bin` AUR helper or GitHub Releases binary artifact lookup), never cloning or compiling SolidCraft source (`LICENSE-BRAND`).
    - *Voice (`voice`):* checks `opendictate` and optionally clones `ThePartyAcolyte/opendictate`.
    - *Omarchy Widget (`omarchy`):* symlinks and enables `integrations/omarchy-bar` (`tech.kirulab.one-ship`).
  - Non-destructive initial `~/.config/one-ship/config.toml` seeding adapted to the detected terminal emulator and installed runner CLI (`agy` / `claude` / `opencode`).
