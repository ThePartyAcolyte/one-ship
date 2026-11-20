# 2026-11-20 — Linux Packaging Plan, Pre-Publication Hygiene (Phase 0) & Relocatable Runtime (Phase 1)

## Summary

1. **Master Packaging & Portability Spec ([41_linux_packaging_and_host_capabilities.md](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/41_linux_packaging_and_host_capabilities.md)):**
   - Documented the 6-phase Linux-only distribution, multi-distro compatibility, `HostCapabilities` gating, OpenDictate decoupling, Omarchy production widget separation, and `./install.sh` feature wizard roadmap.
   - Updated [docs/README.md](file:///home/butcherwutcher/Projects/dev/one-ship/docs/README.md), [TODO.md](file:///home/butcherwutcher/Projects/dev/one-ship/TODO.md), and [TECHNICAL_DEBT.md](file:///home/butcherwutcher/Projects/dev/one-ship/TECHNICAL_DEBT.md) (added `#48` Omarchy Bar Widget production separation and `#49` `HostCapabilities` & OpenDictate unavailability handling).
2. **Phase 0 — Pre-Publication Hygiene & Portrait Strategy:**
   - Added `extras/Pixel-Portraits-Free/`, `extras/Pixel-Portraits-Addon/`, and `public/portraits/` to [.gitignore](file:///home/butcherwutcher/Projects/dev/one-ship/.gitignore) so restricted third-party sprite packs are not distributed via the public repository.
   - Updated [scripts/build_portrait_catalog.mjs](file:///home/butcherwutcher/Projects/dev/one-ship/scripts/build_portrait_catalog.mjs) to check `ONESHIP_PORTRAITS_SRC`, `extras/`, and `$XDG_DATA_HOME/one-ship/portraits-src`, exiting cleanly when absent.
   - Added a deterministic MIT-clean procedural `16x16` pixel-art renderer (`renderProceduralPortrait`) inside [AppAgentPortrait.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/ui/AppAgentPortrait.vue) keyed off `AgentPortraitDna` (`Bxx:Ex:Hxx:BDxx:Axx`) whenever `/portraits/manifest.json` is absent.
   - Sanitized personal `/home/butcherwutcher/...` paths and personal email references across [14_cinturonero_translator_protocol.md](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/14_cinturonero_translator_protocol.md), [19_agent_quota_and_context_observability.md](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/19_agent_quota_and_context_observability.md), [32_oneship_designer_integration.md](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/32_oneship_designer_integration.md), and [33_cad_and_pcb_integration.md](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/33_cad_and_pcb_integration.md).
   - Updated [scripts/verify_rules.sh](file:///home/butcherwutcher/Projects/dev/one-ship/scripts/verify_rules.sh) regex #6 to allow nested generics such as `Result<HashMap<String, String>, DomainError>` without false positives.
3. **Phase 1 — Relocatable Assets, `$PATH` Resolution & Production Unit:**
   - Replaced the compile-time `TEMPLATES_ROOT` path in [skill.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/skill.rs) with `resolve_templates_root()` (`ONESHIP_TEMPLATES_DIR`, `<exe>/../share/one-ship/templates`, `<exe>/templates`, `$XDG_DATA_HOME/one-ship/templates`, `/usr/local/share/one-ship/templates`, `/usr/share/one-ship/templates`, `DEV_TEMPLATES_ROOT`), and wired [config.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/config.rs) (`load_runner_schema`, `list_agent_runners`, `ConfigError::TemplatesNotFound`) through `canonical_templates_dir("runners")`.
   - Updated [owon_vds1022.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/electronics/instruments/owon_vds1022.rs) with `load_firmware_bitstream()` checking `ONESHIP_RESOURCES_DIR`, installed share directories, and dev paths before falling back to compile-time `include_bytes!` for all 5 Owon VDS1022 FPGA bitstreams.
   - Added `desktop::resolve_executable()` in [desktop.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/desktop.rs) (supporting both explicit paths and `$PATH` executable resolution) and wired [desigma.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/desigma.rs), [pcbcraft.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/pcbcraft.rs), and [solidcraft.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/solidcraft.rs), normalizing `solidcraft` defaults in `config.rs` to `"solidcraft"` and `"solidcraft-cli"`.
   - Added production systemd user service [packaging/systemd/oneship-daemon.service](file:///home/butcherwutcher/Projects/dev/one-ship/packaging/systemd/oneship-daemon.service) and desktop entry [packaging/one-ship.desktop](file:///home/butcherwutcher/Projects/dev/one-ship/packaging/one-ship.desktop).

## Verification

1. **Read-Only Rule Gate (`./scripts/verify_rules.sh`):**
   - `[PASS]` Zero TypeScript `any` in `src/`
   - `[PASS]` Zero Tailwind palette classes (`text-/bg-/border-{color}-*`)
   - `[PASS]` Zero `backdrop-blur-*` in components/views
   - `[PASS]` Zero hardcoded `/home/butcherwutcher` paths
   - Pre-existing unrelated hits remain tracked in Debt `#45` (`bg-black/*` in legacy modals/panels) and `config.rs:1915` (`migrate_toml_snapshots_to_vcs` returning `Result<Option<String>, String>`). Zero new violations in edited files.
2. **Daemon Build & Status (`./scripts/run_dev.sh status` & `journalctl --user -u oneship-dev-daemon-build -n 20 --no-pager`):**
   - `oneship-dev-daemon-build` finished `dev` profile build in `38.23s` with zero compiler errors.
   - `oneship-dev-daemon` restarted cleanly (`active (running)`, `main_pid: 896826`).

## Open Items

- Untrack previously committed portrait assets (`git rm -r --cached extras/Pixel-Portraits-Free extras/Pixel-Portraits-Addon public/portraits`) before pushing to a public remote.
- Proceed to **Phase 2** (Linux multi-distro package manager abstraction, X11 clipboard fallback, terminal auto-detection, Hyprland/systemd-run fallback, WebKitGTK launcher retry) and **Phase 3** (`HostCapabilities` + OpenDictate decoupling) upon approval.
