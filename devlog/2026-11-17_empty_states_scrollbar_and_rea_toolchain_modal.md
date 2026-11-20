# 2026-11-17 — Shared Empty States, Transparent Scrollbar & RE Toolchain Modal

## Summary

Standardized the Design/CAD/PCB missing-file panels through a single `AppEmptyState` component, restyled the global scrollbar so its track is fully transparent at rest with an accent hover state, moved the RE Lab chrome to the panel-bar contract (no duplicate title, compact engine chip, toolchain modal), and relabeled the existing Profiling comparison view. The REAA toolchain modal can now install the pacman-backed reverse-engineering tools through the daemon.

## Changes Implemented

### 1. `AppEmptyState` (`src/components/ui/AppEmptyState.vue`, new)

- Flat `ui-surface-card` card, accent icon at 32 px, uppercase title, optional description slot, and `.ui-icon-btn` action slots.
- Exported from `src/components/ui/index.ts`.
- Adopted by `ProjectDesignTab.vue` (`Layers`), `ProjectCadTab.vue` (`Box`), and `ProjectPcbTab.vue` (`CircuitBoard`). Toolbar hints normalized to `No design files in …`, `No CAD files in …`, and `No PCB files in …`.

### 2. Global scrollbar (`src/styles/ui.css`)

- `::-webkit-scrollbar`, track, and corner are fully transparent at rest.
- Track takes a `color-mix(var(--accent-color) 22%, transparent)` tint on hover/active.
- Thumb is `color-mix(var(--fg-color) 22%, transparent)` at rest (lighter than the former near-black) and solid accent on hover/active.

### 3. RE Lab (`ProjectReaTab.vue`, `diagnostic.rs`)

- Removed the duplicate `RE Lab` title and its `Cpu` icon; bar is `h-9 ui-surface-title` without wrapping.
- Engine badge is a compact `h-6` chip: `SLEEPING` or `ENGINE :port · PID`, with an auto-wake tooltip instead of the `0 MB RAM` copy.
- Wake/Stop, Logs, Cache, and MCP controls normalized to `h-6`/`text-[9px]`; refresh uses `.ui-icon-btn`.
- Toolchain access is now a `Wrench` `.ui-icon-btn` (ok/warn border by installation state) that opens a modal listing every probed tool. The modal marks installed tools, copies npm hints for `rea-agents`, and installs pacman-backed tools individually or all at once, then re-runs `check_rea_toolchain`. Install failures surface in the modal and `system_logs`.
- Daemon: `install_missing_tools` whitelist extended with `ghidra`, `radare2`/`r2`, `binutils`/`objdump`/`readelf`, `binwalk`, `jadx`, and `squashfs-tools`/`unsquashfs`; unit assertion added.

### 4. Profiling (`ProjectProfilerTab.vue`)

- Bar standardized to `h-9 ui-surface-title`; target selector `compact`.
- Removed the decorative `Activity` icon from the bar.
- The icon-only comparison toggle is now a labeled `Compare`/`Runs` button (accent when active) that opens the existing BASELINE/COMPARE view; refresh uses `.ui-icon-btn`.

## Specs Updated

- `13 §2.9` — `AppEmptyState` contract.
- `13 §3.8` — transparent-at-rest scrollbar colors.
- `13 §3.13` — empty-state adoption and RE/Profiling bar contract.
- `26 §2.1` — host toolchain reporting and pacman installation path.

## Verification

No build or runtime process was started (AGENTS.md §1.6). Pending manual check: the three empty states, scrollbar appearance at rest/hover, RE toolchain modal install flow with `pkexec` (requires a daemon rebuilt by the watcher), and Profiling `Compare`/`Runs` toggle.
