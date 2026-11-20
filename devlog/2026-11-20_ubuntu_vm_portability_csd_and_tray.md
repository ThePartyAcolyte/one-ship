# Devlog: Ubuntu 26.04 VM Portability, CSD Window Controls & Resident Tray (2026-11-20)

## Summary

Resolved real-world build, packaging, window-management, and desktop-integration issues surfaced during clean **Ubuntu 26.04** VM verification, and synchronized the reorganized architecture specifications ([`docs/specs/01-architecture/05-daemon-operator-api-and-lifecycle.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/01-architecture/05-daemon-operator-api-and-lifecycle.md) and [`docs/specs/01-architecture/07-linux-packaging-and-host-capabilities.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/01-architecture/07-linux-packaging-and-host-capabilities.md)).

## Changes Implemented

1. **Production Frontend Build (`vue-tsc --noEmit`) Fixes (`71f5a20`, `f1130e1`):**
   - [`src/components/workspace/ProjectManagementTab.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectManagementTab.vue#L1317): removed inline TypeScript parameter type annotation from `@add` template handler.
   - [`src/utils/reportAsyncFailure.ts`](file:///home/butcherwutcher/Projects/dev/one-ship/src/utils/reportAsyncFailure.ts): overloaded `reportAsyncFailure` to accept both `(err)` and `(context, err)`.
   - [`src/components/modals/OmnisearchModal.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/modals/OmnisearchModal.vue#L66-L74), [`src/components/workspace/ProjectKnowledgeGraphTab.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectKnowledgeGraphTab.vue#L126-L199), [`src/components/workspace/ProjectOverviewTab.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectOverviewTab.vue#L66): supplied explicit `null` fields required by generated `ts-rs` argument types (`SearchKnowledgeArgs`, `IndexProjectArgs`).
   - [`src/components/workspace/MilestoneDetailPanel.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/MilestoneDetailPanel.vue#L390-L472), [`src/components/workspace/ProjectMilestonesView.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectMilestonesView.vue#L15-L199): added required `:icon` props on `AppEmptyState`, moved `description` into `#description` slot, and removed unused `const props =`.
   - [`src/components/workspace/ProjectGitSnapshotsTab.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectGitSnapshotsTab.vue#L20), [`src/components/workspace/ProjectWorkspace.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectWorkspace.vue#L1164): removed unused `CheckCircle2` import and fixed `selectedInsightNode` -> `selectedInsightDoc`.

2. **Installer Dependencies, Embedded Bundle & Icons (`3c1a5e9`, `9f6eba1`, `ce6390f`, `9e200d5`):**
   - [`install.sh`](file:///home/butcherwutcher/Projects/dev/one-ship/install.sh):
     - Added `protobuf` / `protobuf-compiler` / `protobuf-devel` (for `lance-encoding`) and `libudev` / `libusb` headers (for `serialport` / `rusb`) across `pacman`, `apt-get`, `dnf`, and `zypper`, plus explicit `protoc` pre-flight check.
     - Enabled `--features tauri/custom-protocol` on release builds so `one-ship` embeds `../dist` instead of connecting to `http://localhost:1420`.
     - Restarted `oneship-daemon.service` on install/upgrade (`systemctl --user enable` + `restart`) so `BUILD_ID` stays synchronized.
     - Installed `hicolor` PNG icons (`32x32`, `128x128`, `256x256`, `512x512`) and refreshed `gtk-update-icon-cache` / `update-desktop-database` (plus cleanup in `--uninstall`).

3. **OS-Adaptive CSD Window Chrome, Detached Launcher & Resident Tray (`cc05922`, `9e200d5`):**
   - [`src-tauri/capabilities/default.json`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/capabilities/default.json): granted `core:window:allow-{start-dragging,start-resize-dragging,minimize,toggle-maximize,close,is-maximized}`.
   - [`src/components/AppHeader.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/AppHeader.vue) & [`src/App.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/App.vue): when `!hyprland_available`, added draggable/double-click header, brutalist Minimize/Maximize/Close buttons, and 8-way `4px` edge/corner `startResizeDragging` handles.
   - [`src/stores/coreStore.ts`](file:///home/butcherwutcher/Projects/dev/one-ship/src/stores/coreStore.ts#L308-L312): updated `build_mismatch` message to mention `systemctl --user restart oneship-daemon.service` alongside `scripts/run_dev.sh daemon`.
   - [`packaging/bin/one-ship-launch`](file:///home/butcherwutcher/Projects/dev/one-ship/packaging/bin/one-ship-launch): detached execution via `setsid` logging to `$XDG_STATE_HOME/one-ship/launcher.log` (preserving `--foreground` for console debugging and 5 s WebKitGTK retry).
   - [`src-tauri/Cargo.toml`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/Cargo.toml) & [`src-tauri/src/lib.rs`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/lib.rs): enabled `tauri` `tray-icon` feature; on `!is_hyprland_session()`, closing the window hides to a resident AppIndicator System Tray (`Open ONE-SHIP`, `Close Interface`, `Restart Daemon`, `Quit (Stop Daemon)`).

4. **Specs Sync (`59b1a37`):**
   - Updated [`docs/specs/01-architecture/05-daemon-operator-api-and-lifecycle.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/01-architecture/05-daemon-operator-api-and-lifecycle.md) and [`docs/specs/01-architecture/07-linux-packaging-and-host-capabilities.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/01-architecture/07-linux-packaging-and-host-capabilities.md).

## Verification

- **`./node_modules/.bin/vue-tsc --noEmit`:** exited `0` (0 TypeScript errors across `src/`).
- **`bash -n install.sh && bash -n packaging/bin/one-ship-launch`:** exited `0`.
- **`oneship-dev-daemon-build` (`journalctl --user -u oneship-dev-daemon-build -n 15 --no-pager`):** `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 23.90s`; `oneship-dev-daemon` running (`main_pid: 1318691`, port `8765`).
- **`./scripts/verify_rules.sh`:** 0 new violations (only the 2 pre-existing unrelated hard failures: Debt `#45` `bg-black/*` in legacy modals and `src-tauri/src/config.rs:1920`).

## Pending Manual / Runtime Items

- Re-run `git pull && ./install.sh --profile minimal --skip-deps --yes` in the **Ubuntu 26.04** VM to verify CSD drag/resize/buttons, app-launcher icon, detached launch, and System Tray daemon controls.
- Repeat `./install.sh` verification on **Linux Mint** and **Fedora Workstation** VMs.
