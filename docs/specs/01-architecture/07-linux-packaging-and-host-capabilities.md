# Linux Packaging and Host Capabilities

## Purpose

Define ONE-SHIP's Linux workstation distribution architecture, relocatable asset resolution, multi-distro package and desktop portability, runtime `HostCapabilities` feature gating, unified Omarchy bar telemetry, and the interactive `./install.sh` feature wizard.

## Scope

- Target Linux distribution policy and exclusion of sandboxed Flatpak/Snap packages.
- Portrait asset licensing isolation and procedural SVG pixel-portrait fallback.
- Relocatable `TEMPLATES_ROOT`, Owon FPGA firmware lookup, `$PATH` executable resolution, and `systemd --user` / `.desktop` packaging files.
- Multi-distro package-manager detection (`pacman`, `apt-get`, `dnf`, `zypper`), X11/Wayland clipboard fallback, non-Hyprland desktop/terminal generalization, and WebKitGTK launcher retry.
- `HostCapabilities` daemon snapshot (`get_host_capabilities`), dynamic UI/settings gating, and OpenDictate decoupling.
- Unified dev/production Omarchy bar widget (`integrations/omarchy-bar/`).
- Interactive [`install.sh`](../../../install.sh) installer wizard and companion tool builds.

WebKitGTK renderer modes and packaging constraints are detailed in [Daemon Operator API and Lifecycle](05-daemon-operator-api-and-lifecycle.md).

## Model / Design

### Target platform policy

ONE-SHIP targets **Linux workstations** across major distributions (Arch/Omarchy, Ubuntu/Debian, Fedora, openSUSE). macOS and Windows are out of scope so the daemon can retain direct Linux `/proc` process-attested MCP identity (`caller_identity.rs`), `systemd` user-session supervision, `rusb` / `serialport` hardware access, and Freedesktop D-Bus IPC without container compromises. Sandboxed Flatpak and Snap distributions are incompatible with `/proc/<pid>/fd/*`, `systemd-run`, and host agent CLIs.

Distribution uses two primary channels:

1. **Repository clone + interactive `./install.sh` feature wizard** (primary installation path).
2. **Native system packages** (`PKGBUILD`/AUR, `.deb`, `.rpm`) installing co-located binaries (`one-ship`, `oneship-daemon`) in the same prefix, shared templates/resources, and a user `systemd` unit.

### Pre-publication hygiene and portrait licensing

- Paid/restricted sprite packs (`extras/Pixel-Portraits-*`, `public/portraits/`) are excluded from public repository distribution via `.gitignore`.
- `AppAgentPortrait.vue` parses `AgentPortraitDna` (`Bxx:Ex:Hxx:BDxx:Axx`) and renders a deterministic, MIT-clean procedural pixel-art portrait onto the `64x64` canvas whenever `/portraits/manifest.json` is absent. Users owning a local sprite pack can still generate `public/portraits/` via `scripts/build_portrait_catalog.mjs` (`ONESHIP_PORTRAITS_SRC` or `extras/`).

### Relocatable assets, `$PATH` resolution, and production service

- **Relocatable `TEMPLATES_ROOT` (`kopeng/skill.rs`, `config.rs`):** canonical templates (`skills/`, `prompts/`, `runners/`) resolve in deterministic order instead of hardcoding `CARGO_MANIFEST_DIR`:
  1. `ONESHIP_TEMPLATES_DIR` (if set, must exist and be a directory; fails loudly otherwise).
  2. `<current_exe_dir>/../share/one-ship/templates` (standard FHS prefix: `~/.local/share/one-ship/templates` or `/usr/local/share/one-ship/templates`).
  3. `<current_exe_dir>/templates` (portable bundle directory).
  4. `$XDG_DATA_HOME/one-ship/templates` or `$HOME/.local/share/one-ship/templates`.
  5. `/usr/local/share/one-ship/templates` and `/usr/share/one-ship/templates`.
  6. Compile-time dev tree (`<CARGO_MANIFEST_DIR>/../templates`).
- **Owon FPGA firmware (`owon_vds1022.rs`):** searches disk overrides (`ONESHIP_RESOURCES_DIR`, `<prefix>/share/one-ship/resources`, dev paths) first, falling back to compile-time embedded bitstreams (`include_bytes!`).
- **Executable `$PATH` lookup (`desktop::resolve_executable`):** companion binaries (`desigma`, `pcbcraft`, `solidcraft`) resolve both explicit paths and bare executable names via `$PATH` (`solidcraft` defaults normalized to `"solidcraft"` and `"solidcraft-cli"`).
- **Production `systemd --user` and `.desktop` entry:** `packaging/systemd/oneship-daemon.service` (`KillMode=mixed`, `TimeoutStopSec=30`) and `packaging/one-ship.desktop` run outside `scripts/run_dev.sh`. `one-ship` and `oneship-daemon` must remain co-located in the same `bin/` directory to satisfy `caller_identity::operator_app_exe()`.

### Multi-distro portability

- **Distro-aware diagnostics and tool hints (`diagnostic.rs`, `rea.rs`, `toolchain.rs`):** replaces Arch-only `pkexec pacman` assumptions with `/etc/os-release` + `$PATH` package-manager detection (`LinuxPackageManager`: `pacman`, `apt-get`, `dnf`, `zypper`) and distro-aware `install_hint` / `auto_installable` metadata.
- **Clipboard fallback (`documents.rs`):** `paste_clipboard_image` supports Wayland (`wl-paste`) and X11 (`xclip`) based on session environment and binary availability.
- **Desktop and terminal generalization (`desktop.rs`, `config.rs`, `accounts.rs`, `lib.rs`, `AppHeader.vue`, `App.vue`):**
  - Short-circuits `hyprctl` immediately when `HYPRLAND_INSTANCE_SIGNATURE` or `hyprctl` is absent (`is_hyprland_session()`).
  - Falls back to a detached Unix `process_group(0)` when `systemd-run` or user D-Bus is unavailable.
  - Expands `system.preferred_terminal` with standard DE terminals (`gnome-terminal`, `konsole`, `xfce4-terminal`, `x-terminal-emulator`, `xterm`) and auto-detects an installed fallback terminal with terminal-specific execution flags (`--` vs. `-e` vs. `start --` vs. bare).
  - **OS-adaptive window chrome and tray lifecycle (`!is_hyprland_session()`):**
    - On **Hyprland / Omarchy** (`hyprland_available == true`), `one-ship` remains frameless without CSD buttons and exits on window close (daemon lifecycle lives in `integrations/omarchy-bar/`).
    - On **floating Linux DEs** (Ubuntu GNOME, Linux Mint Cinnamon, Fedora, KDE, XFCE — `hyprland_available == false`), `AppHeader.vue` renders a draggable header plus brutalist Minimize/Maximize/Close buttons, `App.vue` mounts 8-way `4px` edge/corner `startResizeDragging` handles, closing the window hides to a native AppIndicator/SNI System Tray (`tauri` `tray-icon`), and Tray **Quit (Stop Daemon)** stops `oneship-daemon.service` before exiting.
- **Detached launcher and WebKitGTK startup retry (`packaging/bin/one-ship-launch`):** ensures `oneship-daemon.service` is active, detaches `one-ship` into its own session (`setsid`, logging to `$XDG_STATE_HOME/one-ship/launcher.log`, with `--foreground` for console debugging), and retries `one-ship` once with `ONESHIP_WEBKIT_LAUNCHER_RETRY=1` and `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1` (logged at `WARN`) if the interface exits non-zero within 5 seconds.

### `HostCapabilities`, dynamic feature gating, and OpenDictate decoupling

- **Host capability registry (`get_host_capabilities` in `diagnostic.rs`, `coreStore.ts`):** exposes a daemon snapshot reporting `installed_runners`, optional domain `integrations` (`rea`, `electronics`, `solidcraft`, `pcbcraft`, `desigma`, `voice_controller`), `opendictate_available`, `gdbus_available`, `omarchy_available`, `hyprland_available`, `clipboard_available`, and `package_manager`.
- **Dynamic UI and settings gating:**
  - `ProjectWorkspace.vue` gates optional workspace tabs (`electronics`, `design`, `cad`, `pcb`, `rea`) via `coreStore.isIntegrationAvailable(...)` and includes `pcb` and `hostCapabilities` in the active-tab fallback watcher.
  - `ProjectConfigPanel.vue` and `SettingsModal.vue` hide unavailable plugin categories and filter runner dropdowns to `installed_runners` (while keeping already-configured values selectable).
  - `ProjectConfigPanel.vue` surfaces a single missing-integration warning when an imported project's `rula.toml` enables a feature unavailable on the host; `SettingsModal.vue` includes an **Integrations** tab for capability inspection, custom binary overrides, and live refresh.
- **OpenDictate decoupling (`kopeng/opendictate.rs`, `voiceStore.ts`):**
  - `VoiceReservationResult` distinguishes `service_available: false` from `busy`, preventing the 1.2 s polling loop and false `"OpenDictate busy"` UI state on chat input focus.
  - `FloatingChatOverlay.vue` and `ProjectWorkspace.vue` hide OpenDictate controls when `!coreStore.isOpendictateAvailable`.
  - `spawn_dbus_monitor` cleanly skips monitoring when `gdbus` or `DBUS_SESSION_BUS_ADDRESS` is absent and checks `$XDG_RUNTIME_DIR/opendictate_state.json` before `/tmp/opendictate_state.json`.

### Unified Omarchy bar widget

`integrations/omarchy-bar/` (`tech.kirulab.one-ship`) always reads the daemon's atomic snapshot at `$XDG_RUNTIME_DIR/oneship-status.json` (`supervisor::write_runtime_status_snapshot` / `remove_runtime_status_snapshot`) and auto-adapts between **Development mode** (`run_dev.sh` Cargo build progress and `oneship-dev-*` unit controls when symlinked from a checkout) and **Production mode** (`oneship-daemon.service` and `one-ship-launch`/`one-ship`).

### Interactive `./install.sh` wizard and companion repositories

[`install.sh`](../../../install.sh) at repository root provides:

- Multi-distro detection (`/etc/os-release` + `pacman`/`apt-get`/`dnf`/`zypper`, AUR helper, Wayland/X11, Hyprland, Omarchy Bar), `--profile <minimal|standard|full>`, `--features <list>`, `--prefix <dir>`, `--repos-dir <dir>`, `--skip-deps`, `--yes`, `--dry-run`, and `--uninstall`.
- Installs distro build dependencies (including `protobuf`/`protobuf-compiler`/`protobuf-devel` for `lance-encoding` and `libudev`/`libusb` headers for `serialport`/`rusb`), builds the frontend and co-located release binaries (`cargo build --release --features tauri/custom-protocol --bin one-ship --bin oneship-daemon`), installs `one-ship-launch`, shared `templates/`, `packaging/one-ship.desktop`, `hicolor` icons (`32x32`, `128x128`, `256x256`, `512x512`) with `gtk-update-icon-cache`/`update-desktop-database`, and `packaging/systemd/oneship-daemon.service` (restarting the service on install/upgrade to keep `BUILD_ID` synchronized).
- Modular feature steps:
  - **RE Lab (`rea`):** reverse-engineering packages plus optional `npm install -g rea-agents@latest`.
  - **Electronics (`electronics`):** `ngspice` / `sigrok-cli` plus [`packaging/udev/99-oneship-instruments.rules`](../../../packaging/udev/99-oneship-instruments.rules) (Owon `0x5345:0x1234`, Saleae `0x0925:0x3881`, Cypress FX2 `0x04b4:0x8613`).
  - **DE-SIGMA (`design`):** checks `oneship-designer` & `desigma-mcp` on `$PATH`, checks local clone, optionally clones `ThePartyAcolyte/desigma` (`oneship` branch), builds, and installs into `<prefix>/bin`.
  - **PCBCraft (`pcb`):** checks `pcbcraft` & `pcbcraft-cli` on `$PATH`, checks local clone, optionally clones `tangency-labs/pcbcraft` (`main` branch), and builds with bounded memory (`-j 2`, `LTO=false`, `CODEGEN_UNITS=4`, `systemd-run --user --scope` `8G`/`10G` limits).
  - **SolidCraft (`cad`, binary-only exception):** checks `$PATH` only and guides/installs prebuilt packages (`solidcraft-bin` AUR helper or GitHub Releases binary artifact lookup); never clones or compiles SolidCraft source (`LICENSE-BRAND`).
  - **Voice (`voice`):** checks `opendictate` and optionally clones `ThePartyAcolyte/opendictate`.
  - **Omarchy widget (`omarchy`):** symlinks and enables `integrations/omarchy-bar` (`tech.kirulab.one-ship`).
- Non-destructive initial `~/.config/one-ship/config.toml` seeding adapted to the detected terminal emulator and installed runner CLI (`agy` / `claude` / `opencode`).

## Contracts

- **Daemon RPC command (`api.rs`):** `get_host_capabilities` -> `HostCapabilities` (`installed_runners`, `integrations: Vec<IntegrationCapability>`, `opendictate_available`, `gdbus_available`, `omarchy_available`, `hyprland_available`, `clipboard_available`, `package_manager`).
- **Environment overrides:** `ONESHIP_TEMPLATES_DIR`, `ONESHIP_RESOURCES_DIR`, `ONESHIP_PORTRAITS_SRC`, `ONESHIP_APP_BIN`, `ONESHIP_WEBKIT_LAUNCHER_RETRY`.
- **Runtime snapshot:** `$XDG_RUNTIME_DIR/oneship-status.json`.
- **Packaging artifacts:** [`install.sh`](../../../install.sh), [`packaging/bin/one-ship-launch`](../../../packaging/bin/one-ship-launch), [`packaging/systemd/oneship-daemon.service`](../../../packaging/systemd/oneship-daemon.service), [`packaging/one-ship.desktop`](../../../packaging/one-ship.desktop), [`packaging/udev/99-oneship-instruments.rules`](../../../packaging/udev/99-oneship-instruments.rules), [`integrations/omarchy-bar/`](../../../integrations/omarchy-bar/).
- **Config key:** `system.preferred_terminal`, `system.webkit_renderer_mode`, plus custom integration binary paths (`desigma`, `solidcraft`, `pcbcraft`).

## Implementation Status

| Area | Status | Notes |
|---|---|---|
| Phase 0: Portrait licensing isolation & procedural SVG fallback | Implemented | Exclusion rules and procedural canvas portrait in place (per legacy source). |
| Phase 1: Relocatable `TEMPLATES_ROOT`, firmware lookup, `$PATH` resolution, systemd/desktop units | Implemented | Verified `packaging/systemd/oneship-daemon.service`, `packaging/one-ship.desktop`. |
| Phase 2: Multi-distro diagnostics, clipboard/terminal fallbacks, launcher retry | Implemented | Verified `LinuxPackageManager` / `DiagnosticError` in `kopeng/diagnostic.rs` and `packaging/bin/one-ship-launch`. |
| Phase 3: `HostCapabilities`, dynamic UI gating, OpenDictate decoupling | Implemented | Verified `HostCapabilities`, `IntegrationCapability` in `diagnostic.rs` and `hostCapabilities` / `isIntegrationAvailable` / `isOpendictateAvailable` in `src/stores/coreStore.ts`. |
| Phase 4: Unified dev/production Omarchy bar widget | Implemented | Verified `integrations/omarchy-bar/` (`ControlPanel.qml`, `Widget.qml`, `manifest.json`). |
| Phase 5: Interactive `./install.sh` wizard & udev rules | Implemented (manual multi-distro runtime verification pending in `TODO.md`) | Verified [`install.sh`](../../../install.sh) and [`packaging/udev/99-oneship-instruments.rules`](../../../packaging/udev/99-oneship-instruments.rules) exist. |

## Related Specs

- [Configuration Schema and Cascade](02-configuration-schema-and-cascade.md)
- [Platform Skill Injection](03-platform-skill-injection.md)
- [Daemon Operator API and Lifecycle](05-daemon-operator-api-and-lifecycle.md)
- [Remote Repositories and Diagnostics](../03-projects/04-remote-repositories-and-diagnostics.md)

Supersedes legacy: `41_linux_packaging_and_host_capabilities.md`
