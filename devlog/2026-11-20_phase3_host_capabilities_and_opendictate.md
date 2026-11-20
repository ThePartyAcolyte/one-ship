# 2026-11-20 — Phase 3: Host Capabilities Registry, Dynamic UI Gating & OpenDictate Decoupling

## Summary

1. **Host Capabilities Registry ([diagnostic.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/diagnostic.rs), [api.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/api.rs), [daemonApi.ts](file:///home/butcherwutcher/Projects/dev/one-ship/src/types/daemonApi.ts), [coreStore.ts](file:///home/butcherwutcher/Projects/dev/one-ship/src/stores/coreStore.ts)):**
   - Added `HostCapabilities` and `IntegrationCapability` (`#[ts(export)]`) plus the `get_host_capabilities` daemon command.
   - Probes installed agent runners from `templates/runners/*.toml` (`installed_runners`), optional domain binaries (`rea`, `electronics`, `solidcraft`, `pcbcraft`, `desigma`, `voice_controller` via PipeWire `pw-record`/`pw-play`), and host capabilities (`opendictate_available`, `gdbus_available`, `omarchy_available`, `hyprland_available`, `clipboard_available`, `package_manager`).
   - Hydrated `coreStore.hostCapabilities` on app mount, SSE reconnect/resync, after tool installation ([App.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/App.vue)), and after saving global settings.
2. **Dynamic Workspace Tab, Settings & Runner Gating:**
   - [ProjectWorkspace.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectWorkspace.vue): gated optional workspace tabs (`electronics`, `design`, `cad`, `pcb`, `rea`) with `coreStore.isIntegrationAvailable(...)` and updated the active-tab fallback watcher to cover `pcb` and `hostCapabilities`.
   - [ProjectConfigPanel.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectConfigPanel.vue): filtered unavailable plugin categories and runner dropdowns to installed host capabilities, and added a warning banner when an imported project enables a host-missing integration in `rula.toml`.
   - [SettingsModal.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/modals/SettingsModal.vue): filtered unavailable plugin categories and runner dropdowns, and added a Global **Integrations** tab displaying live `Ready` / `Missing` status, custom binary path overrides (`solidcraft`, `pcbcraft`, `desigma`, `rea`), and a `Refresh` action.
3. **OpenDictate Decoupling ([opendictate.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/opendictate.rs), [voiceStore.ts](file:///home/butcherwutcher/Projects/dev/one-ship/src/stores/voiceStore.ts), [FloatingChatOverlay.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/dialogue/FloatingChatOverlay.vue), [ProjectWorkspace.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectWorkspace.vue)):**
   - Added `service_available: bool` to `VoiceReservationResult` so `voiceStore.ts` distinguishes `service_available: false` from `busy`, eliminating the 1.2s polling loop and false `"OpenDictate busy"` UI state when OpenDictate is absent.
   - `spawn_dbus_monitor` now skips monitoring cleanly when `gdbus` or `DBUS_SESSION_BUS_ADDRESS` is missing, and `check_opendictate_autosend*` checks `$XDG_RUNTIME_DIR/opendictate_state.json` before `/tmp/opendictate_state.json`.
   - Hidden OpenDictate microphone/dictation controls in `FloatingChatOverlay.vue` and `ProjectWorkspace.vue` when `!coreStore.isOpendictateAvailable`.
4. **Specs & Task Sync:**
   - Updated [41_linux_packaging_and_host_capabilities.md](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/41_linux_packaging_and_host_capabilities.md) (Phase 3 marked Implemented) and [TODO.md](file:///home/butcherwutcher/Projects/dev/one-ship/TODO.md).

## Verification

1. **Bindings Export (`./scripts/run_dev.sh export-bindings`):**
   - `test result: ok. 415 passed; 0 failed; 0 ignored; 0 measured; 175 filtered out; finished in 19.03s` (generated `HostCapabilities.ts`, `IntegrationCapability.ts`, updated `VoiceReservationResult.ts`).
2. **Read-Only Rule Gate (`./scripts/verify_rules.sh`):**
   - `[PASS]` Zero TypeScript `any` in `src/`
   - `[PASS]` Zero Tailwind palette classes
   - `[PASS]` Zero `backdrop-blur-*` in components/views
   - `[PASS]` Zero hardcoded `/home/butcherwutcher` paths
   - Zero new violations in edited files.
3. **Daemon Build & Status (`./scripts/run_dev.sh status` & `journalctl --user -u oneship-dev-daemon-build -n 15 --no-pager`):**
   - `oneship-dev-daemon-build` finished `dev` profile build in `2m 03s` with zero compiler errors.
   - `oneship-dev-daemon` (`main_pid: 1028122`) active and listening on port `8765`.
