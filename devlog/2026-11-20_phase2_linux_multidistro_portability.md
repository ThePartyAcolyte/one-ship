# 2026-11-20 — Phase 2: Linux Multi-Distro Portability & Launcher Retry

## Summary

1. **Multi-Distro Package Manager Detection & Tool Installation ([diagnostic.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/diagnostic.rs)):**
   - Added `LinuxPackageManager` (`Pacman`, `Apt`, `Dnf`, `Zypper`) resolved via `/etc/os-release` (`ID` / `ID_LIKE`) and `PATH` executable verification.
   - Replaced hardcoded Arch-only `pkexec pacman` mappings with `resolve_distro_packages`, `resolve_tool_install_hint`, and `DiagnosticError::UnsupportedPackageManager` / `ToolUnavailableInDistro`.
   - Extended `DiagnosticState` (`package_manager: Option<String>`, `can_auto_install_tools: bool`) and replaced `install_hint_arch` with `install_hint` + `auto_installable` across [rea.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/rea.rs), [toolchain.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/electronics/toolchain.rs), [fx2lafw.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/electronics/instruments/fx2lafw.rs), [AppHeader.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/AppHeader.vue), [ProjectElectronicsTab.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectElectronicsTab.vue), and [ProjectReaTab.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectReaTab.vue).
2. **Wayland + X11 Clipboard Image Paste ([documents.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/documents.rs), [api.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/api.rs), [daemonApi.ts](file:///home/butcherwutcher/Projects/dev/one-ship/src/types/daemonApi.ts), [projectExplorerStore.ts](file:///home/butcherwutcher/Projects/dev/one-ship/src/stores/projectExplorerStore.ts), [FloatingChatOverlay.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/dialogue/FloatingChatOverlay.vue)):**
   - Renamed `paste_wayland_clipboard_image` to `paste_clipboard_image` across backend and frontend, supporting `wl-paste` (Wayland) and `xclip` (X11) with explicit missing-utility error reporting.
3. **Desktop, Hyprland & Terminal Generalization ([desktop.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/desktop.rs), [config.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/config.rs), [accounts.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/accounts.rs)):**
   - `desktop_command` falls back to a detached Unix `process_group(0)` when `systemd-run` or user D-Bus session is unavailable.
   - `is_hyprland_session()` short-circuits all `hyprctl` queries/polling immediately on non-Hyprland desktops (GNOME, KDE, Sway, X11).
   - Expanded `system.preferred_terminal` options (`gnome-terminal`, `konsole`, `xfce4-terminal`, `x-terminal-emulator`, `xterm`) and added `resolve_preferred_terminal` + `build_terminal_exec_args` for OAuth terminal spawning.
4. **WebKitGTK Startup Retry Launcher ([packaging/bin/one-ship-launch](file:///home/butcherwutcher/Projects/dev/one-ship/packaging/bin/one-ship-launch), [packaging/one-ship.desktop](file:///home/butcherwutcher/Projects/dev/one-ship/packaging/one-ship.desktop), [main.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/main.rs), [lib.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/lib.rs)):**
   - Added `one-ship-launch` wrapper ensuring `oneship-daemon.service` is active and retrying once with `ONESHIP_WEBKIT_LAUNCHER_RETRY=1` / `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1` (logged at `WARN`) on early abnormal exit (`< 5s`).
5. **Specs & Task Sync:**
   - Updated [11_remote_github_integration.md](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/11_remote_github_integration.md), [26_rea_reverse_engineering_integration.md](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/26_rea_reverse_engineering_integration.md), [41_linux_packaging_and_host_capabilities.md](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/41_linux_packaging_and_host_capabilities.md), and [TODO.md](file:///home/butcherwutcher/Projects/dev/one-ship/TODO.md).

## Verification

1. **Bindings Export (`./scripts/run_dev.sh export-bindings`):**
   - `test result: ok. 413 passed; 0 failed; 0 ignored; 0 measured; 175 filtered out; finished in 0.34s` (updated `DiagnosticState.ts`, `ReaToolStatus.ts`, `ElectronicsToolStatus.ts`).
2. **Read-Only Rule Gate (`./scripts/verify_rules.sh`):**
   - `[PASS]` Zero TypeScript `any` in `src/`
   - `[PASS]` Zero Tailwind palette classes
   - `[PASS]` Zero `backdrop-blur-*` in components/views
   - `[PASS]` Zero hardcoded `/home/butcherwutcher` paths
   - Zero new violations in edited files.
3. **Daemon Build & Status (`./scripts/run_dev.sh status` & `journalctl --user -u oneship-dev-daemon-build -n 25 --no-pager`):**
   - `oneship-dev-daemon-build` finished `dev` profile build in `55.72s` with zero compiler errors.
   - `oneship-dev-daemon` (`main_pid: 963463`) active and listening on port `8765`; `oneship-dev-gui` (`main_pid: 968492`) active.
