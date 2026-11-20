# Devlog: Phase 5 — Interactive Linux Installer Wizard & Companion Repositories (2026-11-20)

## Summary

Completed **Phase 5** of [Spec 41 (Linux Packaging & Host Capabilities)](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/41_linux_packaging_and_host_capabilities.md), delivering the interactive/flag-driven multi-distro installer [`install.sh`](file:///home/butcherwutcher/Projects/dev/one-ship/install.sh) and hardware instrumentation udev rules [`packaging/udev/99-oneship-instruments.rules`](file:///home/butcherwutcher/Projects/dev/one-ship/packaging/udev/99-oneship-instruments.rules).

## Changes Implemented

1. **USB Hardware Instrumentation `udev` Rules (`packaging/udev/99-oneship-instruments.rules`):**
   - Created [`packaging/udev/99-oneship-instruments.rules`](file:///home/butcherwutcher/Projects/dev/one-ship/packaging/udev/99-oneship-instruments.rules) granting logged-in workstation user access (`MODE="0666", TAG+="uaccess"`) for:
     - Owon VDS1022 / VDS1022I (`0x5345:0x1234`)
     - Saleae Logic / `fx2lafw` (`0x0925:0x3881`)
     - Cypress EZ-USB FX2LP (`0x04b4:0x8613`)

2. **Interactive & Automated Installer Wizard (`install.sh`):**
   - Implemented [`install.sh`](file:///home/butcherwutcher/Projects/dev/one-ship/install.sh) (`chmod +x`) supporting:
     - **Multi-distro & environment probe:** reads `/etc/os-release`, detects `pacman` (`yay`/`paru` AUR helper), `apt-get`, `dnf`, `zypper`, Wayland/X11, Hyprland, Omarchy Bar, and elevation (`pkexec`/`sudo`).
     - **CLI modes:** interactive feature wizard by default plus `--profile <minimal|standard|full>`, `--features <list>`, `--prefix <dir>`, `--repos-dir <dir>`, `--skip-deps`, `--yes`, `--dry-run`, `--uninstall`, and `--help`.
     - **Core install:** optional local portrait catalog build, frontend release build (`npm run build`), co-located Cargo release binaries (`one-ship` + `oneship-daemon` + `one-ship-launch`) installed under `<prefix>/bin/`, `templates/` synced to `<prefix>/share/one-ship/templates/`, desktop entry installed under `~/.local/share/applications/one-ship.desktop`, and `oneship-daemon.service` installed under `~/.config/systemd/user/`.
     - **Non-destructive initial config seeding:** creates `~/.config/one-ship/config.toml` only when absent, matching `DEFAULT_GLOBAL_CONFIG_TOML` and adapting `preferred_terminal` plus the initial runner/model/effort triple (`agy`/`claude`/`opencode`) to installed host binaries.
     - **Companion repository detect-then-clone-and-build workflow:**
       - **DE-SIGMA (`design`):** checks `oneship-designer` & `desigma-mcp` on `$PATH` -> checks local clone (`<repos-dir>/desigma`) -> offers cloning `ThePartyAcolyte/desigma` (`oneship` branch) -> builds and installs release binaries.
       - **PCBCraft (`pcb`):** checks `pcbcraft` & `pcbcraft-cli` on `$PATH` -> checks local clone (`<repos-dir>/pcbcraft`) -> offers cloning `tangency-labs/pcbcraft` (`main` branch) -> builds under Spec 33 §1.2 memory limits (`-j 2`, `CARGO_PROFILE_RELEASE_LTO=false`, `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=4`, `systemd-run --user --scope` `8G`/`10G` when available).
       - **OpenDictate (`voice`):** checks `opendictate` on `$PATH` -> offers cloning `ThePartyAcolyte/opendictate` and running its local installer.
       - **RE Lab (`rea`) & Electronics (`electronics`):** installs distro dependencies (`radare2`, `binutils`, `binwalk`, `squashfs-tools`, `ngspice`, `sigrok-cli`), optional `rea-agents` npm CLI, and `99-oneship-instruments.rules`.
       - **SolidCraft (`cad`, binary-only exception):** never clones or builds from source (`LICENSE-BRAND`); checks `$PATH` only and offers `solidcraft-bin` (AUR helper on Arch) or inspects GitHub Releases for Linux binary artifacts (`https://github.com/tangency-labs/solidcraft/releases/latest`).
       - **Omarchy Bar Widget (`omarchy`):** symlinks `integrations/omarchy-bar` (`tech.kirulab.one-ship`) into `~/.config/omarchy/plugins/`.

3. **Docs & Roadmap Sync:**
   - Updated [`docs/specs/41_linux_packaging_and_host_capabilities.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/41_linux_packaging_and_host_capabilities.md) (Phase 5 -> Implemented), [`README.md`](file:///home/butcherwutcher/Projects/dev/one-ship/README.md), and [`TODO.md`](file:///home/butcherwutcher/Projects/dev/one-ship/TODO.md).

## Verification

### 1. Installer Syntax & Dry-Run Checks (`install.sh`)

```text
$ bash -n install.sh
(exit 0)

$ ./install.sh --dry-run --profile minimal --yes
=== ONE-SHIP Installation Plan Summary ===
  Distro          : Omarchy (omarchy)
  Package Manager : pacman
  Session / WM    : wayland (Hyprland=1, Omarchy=1)
  Install Prefix  : /home/butcherwutcher/.local
  Repos Directory : /home/butcherwutcher/Projects/dev
  Features        : core=1 rea=0 electronics=0 design=0 pcb=0 cad=0 voice=0 omarchy=0
...
[ok]    Core installation complete.
[info]  Existing configuration preserved: /home/butcherwutcher/.config/one-ship/config.toml

$ ./install.sh --dry-run --profile full --repos-dir /tmp/oneship-dryrun-repos --yes
[dry-run] git clone --branch oneship https://github.com/ThePartyAcolyte/desigma.git /tmp/oneship-dryrun-repos/desigma
[dry-run] cargo build --release --manifest-path /tmp/oneship-dryrun-repos/desigma/Cargo.toml -p desigma-app -p desigma-mcp
[dry-run] git clone --branch main https://github.com/tangency-labs/pcbcraft.git /tmp/oneship-dryrun-repos/pcbcraft
[dry-run] systemd-run --user --scope -p MemoryHigh=8G -p MemoryMax=10G env CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_CODEGEN_UNITS=4 cargo build --release -j 2 --manifest-path /tmp/oneship-dryrun-repos/pcbcraft/Cargo.toml -p pcbcraft -p pcbcraft-cli
[info]  Checking SolidCraft (prebuilt binary package)...
[ok]    SolidCraft binaries already installed: /usr/bin/solidcraft, /usr/bin/solidcraft-cli
[info]  Checking OpenDictate voice integration...
[ok]    OpenDictate already installed: /home/butcherwutcher/.local/bin/opendictate

$ ./install.sh --uninstall --dry-run
[dry-run] systemctl --user disable --now oneship-daemon.service
[dry-run] rm -f /home/butcherwutcher/.local/bin/one-ship /home/butcherwutcher/.local/bin/oneship-daemon /home/butcherwutcher/.local/bin/one-ship-launch /home/butcherwutcher/.local/share/applications/one-ship.desktop /home/butcherwutcher/.config/systemd/user/oneship-daemon.service
[dry-run] rm -rf /home/butcherwutcher/.local/share/one-ship
[dry-run] rm -f /home/butcherwutcher/.config/omarchy/plugins/tech.kirulab.one-ship
```

### 2. Read-Only Rule Gate (`./scripts/verify_rules.sh`)

```text
[PASS] Zero TypeScript 'any' in src/
[PASS] Zero Tailwind palette classes (text-/bg-/border-{color}-*)
[FAIL] Zero raw bg-black/* or bg-[#...] outside ui.css (pre-existing Debt #45 files)
[FAIL] Unstructured Result<_, String> found outside tests (pre-existing src-tauri/src/config.rs:1920)
[PASS] Zero backdrop-blur-* in components/views
[PASS] Zero hardcoded /home/butcherwutcher paths
=== Summary: 2 hard failure(s) ===
```

### 3. Dev Daemon Status (`./scripts/run_dev.sh status`)

```text
{"units":{"daemon":{"active_state":"active","sub_state":"running","result":"success","main_pid":1067062,"log_tail":null},"daemon_build":{"active_state":"inactive","sub_state":"dead","result":"success","main_pid":null,"log_tail":null},"daemon_watch":{"active_state":"active","sub_state":"running","result":"success","main_pid":3872353,"log_tail":null},"vite":{"active_state":"active","sub_state":"running","result":"success","main_pid":3867329,"log_tail":null},"gui":{"active_state":"active","sub_state":"running","result":"success","main_pid":1072135,"log_tail":null}},"daemon_listening_ports":[8765],"vite_listening_ports":[],"gui_window_pid":1072206,"build_progress":{"phase":"cargo_build","completed_count":1187,"total_count":1188,"percent":99,"current_item":"linking","started_at_seconds":1795188901,"last_duration_seconds":null},"gui_progress":{"phase":"running","completed_count":null,"total_count":null,"percent":100,"current_item":"one-ship","started_at_seconds":1795188962,"last_duration_seconds":null}}
```

## Pending Manual / Runtime Items

- End-to-end live execution test of `./install.sh` on fresh Arch/Omarchy, Ubuntu/Debian, Fedora, and openSUSE environments (tracked in [`TODO.md`](file:///home/butcherwutcher/Projects/dev/one-ship/TODO.md)).
- Optional repository cleanup before first public `git push`: create a clean orphan `main` commit (without `extras/Pixel-Portraits-*` or `public/portraits/`) to push cleanly to `origin` (`https://github.com/ThePartyAcolyte/one-ship.git`).
