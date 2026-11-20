# 2026-11-18 — Nucleus 1: WebKitGTK Renderer Mode (Spec 12 §7.4)

## Summary
Implemented runtime resolution and Settings configuration for `[system] webkit_renderer_mode` (`auto | dmabuf | shared_memory | software`) per Spec 12 §7.4, and audited the remaining `projects[0]` / `.first()` occurrences in the codebase.

## Changes
1. **Configuration Schema & Pre-Boot Reader (`src-tauri/src/config.rs`):**
   - Added `webkit_renderer_mode` (`select_enum`, `global_only`, default `"auto"`) to `get_all_plugin_schemas()` under `system` and `DEFAULT_GLOBAL_CONFIG_TOML`.
   - Added `WebkitRendererMode` and `read_webkit_renderer_mode()` for pre-boot typed reads of `config.toml` with safe `Auto` fallback.
2. **Pre-Webview Resolution & Startup Logging (`src-tauri/src/main.rs`, `src-tauri/src/lib.rs`):**
   - Added `configure_webkit_renderer_mode()` in `main()` before `oneship_lib::run()` on Linux:
     - `auto` resolves to `shared_memory` when `/proc/driver/nvidia/version` exists or `/dev/dri` is inaccessible, and `dmabuf` otherwise.
     - `shared_memory` sets `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1`; `software` sets `WEBKIT_DISABLE_DMABUF_RENDERER=1`; `dmabuf` leaves both unset.
   - Logged the resolved mode and origin at `INFO` in `oneship_lib::run()` setup.
3. **Documentation & Task Hygiene:**
   - Removed completed runtime/Settings items from `TODO.md` under Spec 12 §7.4 (retaining only packaging installer/retry).
   - Updated Spec 12 §7.4 status header and `docs/WORK_PLAN.md` Nucleus 1.

## Verification
- Read-only diagnostics via `./scripts/run_dev.sh status` and `journalctl --user -u oneship-dev-daemon-build -n 30 --no-pager` (respecting `AGENTS.md` §1.6 build contention ban).
