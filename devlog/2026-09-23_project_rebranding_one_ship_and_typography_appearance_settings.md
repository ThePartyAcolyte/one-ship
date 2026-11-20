# Work Session Devlog: Project Rebranding to ONE-SHIP, Typography Engine & Appearance Settings

**Date:** 2026-09-23  
**Status:** Completed & Verified  
**Scope:** Rebranding, Database Reset Command, Typography Normalization, Appearance Settings & Header Toolbar Homogenization  

---

## 1. Overview & Objectives
This session executed a major architectural rebranding and aesthetic refinement milestone for the platform:
1. **Global Rebranding (`wang-kapawu` → `ONE-SHIP`)**: Clean global migration across Cargo binaries, crate libraries, frontend metadata, SQLite database, DuckDB telemetry engine, logging files, and system configuration directories.
2. **Database Clean Start**: Added forensic log clearing capabilities (`clear_system_logs`) in backend SQLite persistence and exposed a brutalist `CLEAR` action in the diagnostic logs drawer.
3. **Typography Engine**: Integrated custom fonts (`Protest Guerrilla`, `Anonymous Pro`, `Courier Prime`) from `extras/fonts/`, configured title typography, implemented optical x-height normalization using `@font-face` `size-adjust`, and added dynamic application-wide font scaling.
4. **Appearance Plugin Configuration**: Established a dedicated `appearance` configuration subsystem in `config.rs`, migrating `quota_gauge_style` out of `system` and grouping it with `body_font` and `font_scale` in `SettingsModal.vue`.
5. **Header Toolbar Baseline Homogenization**: Standardized Google and DeepSeek provider dropdown triggers and adjacent action buttons to an identical 24px (`h-6`) structural height.

---

## 2. Implemented Changes

### 2.1 Global Platform Rebranding to ONE-SHIP
- **Cargo & Rust Binaries (`src-tauri/Cargo.toml`, `main.rs`, `bin/daemon.rs`)**:
  - Renamed package to `one-ship`, default binary to `one-ship`, daemon binary to `oneship-daemon`, and library to `oneship_lib`.
- **Frontend & App Manifest (`package.json`, `index.html`, `src-tauri/tauri.conf.json`)**:
  - Updated package name to `one-ship`, title to `ONE-SHIP`, product name to `ONE-SHIP`, and identifier to `tech.kirulab.one-ship`.
  - Updated developer runner script `scripts/run_dev.sh` to target `oneship-daemon` and `one-ship`.
- **Filesystem Paths, Databases & Configuration**:
  - Configuration directory updated to `~/.config/one-ship/`.
  - SQLite database renamed to `oneship.db` (`src-tauri/src/db.rs`).
  - DuckDB telemetry storage renamed to `oneship_telemetry.duckdb` (`src-tauri/src/duck.rs`).
  - Log file updated to `oneship.log` (`src-tauri/src/logger.rs`).
  - MCP daemon identifier updated to `one-ship` (`src-tauri/src/mcp.rs`, `supervisor.rs`, `config.rs`).
  - AGY statusline helper updated to `[ONE-SHIP]` and path references (`scripts/agy_statusline.py`).

### 2.2 Forensic Log Purge (`clear_system_logs`)
- Implemented `clear_system_logs(conn: &Connection) -> Result<(), KopengError>` in `src-tauri/src/db.rs` executing `DELETE FROM system_logs` within an immediate database transaction followed by vacuum.
- Registered Tauri IPC command `clear_system_logs` in `src-tauri/src/lib.rs`.
- Added a brutalist `CLEAR` button with confirmation prompt in `src/components/floating/RollingStatusLog.vue`.

### 2.3 Typography Normalization & Appearance Configuration
- **Asset Migration (`public/fonts/`)**:
  - Placed font families into public assets:
    - `Protest_Guerrilla/ProtestGuerrilla-Regular.ttf`
    - `Anonymous_Pro/` (`Regular`, `Bold`, `Italic`, `BoldItalic`)
    - `Courier_Prime/` (`Regular`, `Bold`, `Italic`, `BoldItalic`)
- **Title & Interface Typography (`src/App.vue`)**:
  - Title styled strictly with `font-title` (`Protest Guerrilla`), removing secondary subtitle text (`// HUB`).
  - Added optical normalization using `@font-face` `size-adjust: 112%;` for `Anonymous Pro` and `size-adjust: 115%;` for `Courier Prime`, balancing their glyph baselines with `IBM Plex Mono`.
  - Defined `--app-font-scale` dynamically bound to `html { zoom: var(--app-font-scale, 1); }`, enabling global scaling (90% to 130%) without breaking flexbox layouts.
- **Backend Schema (`src-tauri/src/config.rs`)**:
  - Created new `appearance` schema (`PluginConfigSchema`):
    - `body_font`: enum `["IBM Plex Mono", "Anonymous Pro", "Courier Prime"]` (default `"IBM Plex Mono"`).
    - `font_scale`: enum `["90% (Compact)", "100% (Normal)", "110% (Medium)", "120% (Large)", "130% (Extra Large)"]` (default `"100% (Normal)"`).
    - `quota_gauge_style`: enum `["concentric", "bars", "classic"]` (default `"concentric"`), migrated from `system`.
  - Updated global config defaults in `load_global_config()` and synchronized `~/.config/one-ship/config.toml`.
- **UI Settings Integration (`src/components/modals/SettingsModal.vue`)**:
  - Dynamic Appearance category rendering in modal navigation.
  - Multi-source state fallback across `config.toml`, `localStorage`, and legacy keys.
  - Emits `oneship-appearance-updated` on save with listener teardown in `src/App.vue`.
  - Synchronized `useQuotaStore.ts` and `coreStore.ts` to respect `appearance.quota_gauge_style`.

### 2.4 Header Toolbar Sizing Homogenization
- **Sizing Parity (`src/components/AppHeader.vue`)**:
  - Removed loose font switcher from the header bar, keeping controls centralized in Settings.
  - Configured `h-6` (24px) fixed height with `px-2` on both the **Google Account Switcher** and **DeepSeek Wallet** button triggers.
  - Removed redundant `py-0.5` vertical padding from Google's active gauge container and standardized gauge component dimensions to `:size="14"` to match the 14px provider icons.
  - Added `leading-none` to dropdown caret indicators (`▼`) and balance values.
  - Standardized adjacent action buttons (`Voice`, `History`, `Diagnostics`, `Settings`) to `h-6 w-6 flex items-center justify-center`.

---

## 3. Verification & Compliance
1. **Compilation Validation**:
   - `cargo check --manifest-path src-tauri/Cargo.toml`: **Passed (0 warnings, 0 errors)**.
   - `npm run build` (`vue-tsc --noEmit && vite build`): **Passed (0 errors)**.
2. **Tiger Style & Governance Rules**:
   - Zero git command executions.
   - Zero shell command file writes; dedicated editing tools used exclusively.
   - Event listener lifecycle teardown implemented in `App.vue` (`oneship-appearance-updated`).
   - Bidirectional specifications synchronized.
