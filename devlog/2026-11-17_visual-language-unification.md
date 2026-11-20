# 2026-11-17 — Visual Language Unification, Header Menu, Log Viewer & Surface Hierarchy

## Summary
Unified the UI visual language around the `OmnisearchModal.vue` aesthetic, consolidated header actions into an application menu, updated the system log viewer with subsystem filtering and infinite-scroll fixes, and formalized the 4-level surface color hierarchy in Spec 13.

## Changes Implemented

### 1. Shared Design System Classes & Surface Hierarchy (`src/styles/ui.css`, `src/main.ts`, `docs/specs/13_design_system_and_ui_components.md`)
- Created `src/styles/ui.css` and imported it from `src/main.ts`.
- Defined reusable surface tokens and classes derived from `--bg-color`:
  - `--ui-bg-base` / `.ui-surface-base`
  - `--ui-bg-title` / `.ui-surface-title`
  - `--ui-bg-list` / `.ui-surface-list`
  - `--ui-bg-card` / `.ui-surface-card`
- Defined Omnisearch-derived component classes: `.ui-search-box`, `.ui-search-box__icon`, `.ui-kbd`, `.ui-filter-pill`, `.ui-filter-pill--active`, `.ui-icon-btn`, `.ui-icon-btn--danger`, `.ui-overlay-backdrop`, and `.ui-modal-footer`.
- Updated `OmnisearchModal.vue` to use these shared classes and accept `initialQuery`.

### 2. Header Consolidation & Application Menu (`src/components/AppHeader.vue`, `src/components/modals/AboutModal.vue`, `src/App.vue`)
- Integrated an inline Omnisearch search box (`.ui-search-box` with accent `<Search>` icon and `Ctrl+K` `.ui-kbd`) into `AppHeader.vue`.
- Removed the standalone `System Diagnostics` button and popover, as well as separate `Logs` and `Settings` buttons.
- Added a Hamburger Application Menu (`Menu` icon) containing:
  - System Health summary row (daemon & diagnostics status).
  - Global Voice Controller quick toggle (`AppToggle` updating `voice_controller.enabled` via `save_global_config`).
  - Navigation items for `System Logs`, `Settings`, and a new `AboutModal.vue` (`v0.2.1`).

### 3. System Log Viewer & Subsystem Filtering (`src/components/floating/RollingStatusLog.vue`, `src/components/ui/AppSelect.vue`, `src-tauri/src/db.rs`, `src-tauri/src/api.rs`, `src-tauri/src/mcp.rs`, `src/types/daemonApi.ts`)
- Added `source: Option<String>` to `GetSystemLogsArgs` and `get_system_log_sources` daemon command, registered in `api.rs` and `daemonApi.ts`, and exported bindings via `./scripts/run_dev.sh export-bindings` (`348 passed`).
- Adapted `RollingStatusLog.vue` to the Omnisearch visual language: `.ui-search-box`, `.ui-kbd` `ESC`, `.ui-modal-footer`, icon-only Clear button (`.ui-icon-btn--danger` `<Trash2>`), `.ui-filter-pill` level filters, and `compact` `AppSelect` dropdowns for `Origin` and `Subsystem`.
- Fixed backward log loading when scrolling up: `initialFetch` now auto-fills up to `MAX_AUTO_FILL_PAGES` (5) if the container does not yet overflow `clientHeight`, plus a `Load older logs` fallback button.

### 4. Project Drawer Borders & Layering (`src/components/workspace/ProjectVerticalRail.vue`)
- Teleported the drawer backdrop to `body` at `z-40` so the main workspace and `AgentTaskbar` (`z-30`) dim behind the overlay while the left vertical project rail and drawer (`z-50`) remain crisp.
- Added `border-t border-b border-r border-[var(--border-color)]`, `.ui-search-box`, and the 4-level surface hierarchy to the drawer.

### 5. WebKitGTK Blur & Scrollbar Investigation (`src-tauri/src/main.rs`, `TECHNICAL_DEBT.md` #36)
- Verified `backdrop-filter: blur(...)` does not render on fixed overlays in `webkit2gtk-4.1 2.54.1-1`, and `filter: blur(6px)` on large background DOM trees degrades modal scroll performance; reverted to a clean semi-transparent backdrop (`.ui-overlay-backdrop`).
- Set `GTK_OVERLAY_SCROLLING=0` in `src-tauri/src/main.rs` and `color-scheme: dark` in `src/styles/ui.css`, but WebKitGTK 2.54 still falls back to native wide rounded scrollbars on several scrollable panels. Logged as **Technical Debt #36**.
