# Devlog: One-Ship Designer Fork, Rebranding, and Editor Fixes (2026-11-15)

## Summary
Established the fork workflow for `/home/butcherwutcher/Projects/dev/desigma` (`ThePartyAcolyte/desigma` + `upstream`), resolved two upstream-agnostic editor bugs on clean branches, rebranded the `oneship` integration branch to **One-Ship Designer** (`oneship-designer`), and synchronized the binary path in ONE-SHIP.

## Changes Implemented

### 1. `fix/async-file-dialog` (`e2de8a2`)
- **Problem:** Synchronous `rfd::FileDialog` calls blocked the `egui`/`winit` UI thread on Wayland/Hyprland, triggering "Application is not responding" dialogs when opening file pickers.
- **Solution:** Added `crates/desigma-app/src/file_dialog.rs` using `std::sync::mpsc` + `std::thread::spawn` polled from `Shell::draw`, and migrated all 12 `rfd::FileDialog` call sites across `interact.rs`, `properties.rs`, `ops.rs`, `home/mod.rs`, `variables/view.rs`, `panels/export.rs`, `panels/inspect.rs`, `decks/export.rs`, and `motion/playback/export.rs`.
- **Verification:** `cargo test -p desigma-app` (`1021 passed; 0 failed; 2 ignored`).

### 2. `fix/svg-resize` (`eb16f29`)
- **Problem:** Imported `.svg` files became a clipping `Frame` whose children (`Group`, `Vector`) kept default `Constraint::Min`, while `apply_constraints` returned early on `NodeKind::Group` and `apply_resize` did not scale `NodeKind::Vector` path/network coordinates.
- **Solution:**
  - Set `Constraint::Scale` (horizontal & vertical) on all imported SVG descendants in `crates/desigma-svg/src/convert.rs`.
  - Enabled recursive `Scale` propagation through `NodeKind::Group` (and rotated `Scale` children) in `crates/desigma-engine/src/layout.rs`.
  - Added `scale_vector_geometry` in `crates/desigma-engine/src/resize.rs` to scale `PathCmd` and `VectorNetwork` coordinates when `NodeKind::Vector` resizes, plus unit test `resizing_svg_frame_and_group_scales_vector_paths`.
- **Verification:** `cargo test -p desigma-engine` (`42 passed; 0 failed`), `cargo test -p desigma-svg` (`1 + 1 + 25 + 2 passed; 0 failed; 1 ignored`).

### 3. Rebranding (`oneship` branch) & ONE-SHIP Sync
- Renamed binary in `crates/desigma-app/Cargo.toml` to `oneship-designer`, updated `APP_TITLE` in `crates/desigma-app/src/theme.rs` to `"One-Ship Designer"`, made the *About* dialog in `crates/desigma-app/src/home/dialog.rs` explicitly state `"One-Ship Designer is based on desigma..."`, and set `window_icon()` in `crates/desigma-app/src/main.rs` to `assets/icon/oneship-designer-256.png`.
- Updated `plugins.desigma.app_binary_path` default in `one-ship/src-tauri/src/config.rs` to `/home/butcherwutcher/Projects/dev/desigma/target/release/oneship-designer`.
- Replaced the vector `"d"` mark in `crates/desigma-app/src/icons.rs` (`paint_logo`) with `assets/icon/oneship-designer-256.png` and updated `Help -> About` in `crates/desigma-app/src/panels/menus.rs` to `"About One-Ship Designer"`.
- Fixed `resolve_enable_bridge` in `one-ship/src-tauri/src/kopeng/desigma.rs` to use `resolve_effective_bool` instead of `resolve_effective_string` so `enable_bridge = false` is respected on editor launch.
- Documented the architecture and branching workflow in `docs/specs/32_oneship_designer_integration.md`.
