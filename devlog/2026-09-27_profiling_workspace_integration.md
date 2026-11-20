# Devlog: Universal Telemetry & Profiler Workspace Integration, Minimalist UX & Script Association

**Date:** 2026-09-27  
**Scope:** Backend `rula.toml` script profiler hydration, global plugin directory resolution, Pinia `useProfilerStore`, minimalist UX refactoring in `ScriptsSection.vue`, and full-fledged `ProjectProfilerTab.vue` workspace tab.

---

## 1. Problem Overview & Architectural Goals

1. **Inaccessible Profiling Subsystem:**
   - Although the backend possessed high-frequency DuckDB process sampling (`process_samples`), SQLite run persistence (`profiler_runs`), and MCP dialogue tools (`profile_service`, `list_profiler_runs`, `query_profiler_hotspots`, `compare_profiler_runs`), there was no user-facing UI tab in the workspace to view recorded runs, DuckDB in-vivo telemetry, or hotspot comparisons.
   - Profiling results from right-click context actions previously only emitted string logs to `coreStore.addStatusLog`, leaving visual flamegraph SVGs and analytical data invisible.

2. **Decoupled Script Configuration in `rula.toml`:**
   - `[scripts.persistent.<id>.profiler]` and `[scripts.task.<id>.profiler]` tables were not hydrated into `ScriptEntry` in `hydration.rs`.
   - The UI had no visibility into whether a script had an associated profiler driver (`cargo-flamegraph`, `samply`, `py-spy`), its sampling rate, or its duration.

3. **Superfluous Explanatory Text & Non-Minimalist UI Tooltips:**
   - Buttons across the scripts section contained obvious, redundant tooltips (e.g. `Restart Service`, `Stop Service`, `Start Service`, `Run Task`), violating the user UX mandate that universal icons must speak for themselves and tooltips must be strictly reserved for relevant contextual runtime data.

---

## 2. Key Changes & Implementation Details

1. **Backend Script Profiler Hydration (`src-tauri/src/hydration.rs`):**
   - Introduced `ScriptProfilerConfig` struct with `driver`, `duration_seconds`, `sampling_rate_hz`, and `format`.
   - Updated `parse_script_entry` to deserialize `s.get("profiler")` directly from `rula.toml`.
   - Normalized `get_global_plugins_dir()` in `src-tauri/src/kopeng/profiler.rs` to point to `~/.config/wang-kapawu/plugins/` (following Spec 25 and zero backward compatibility rule).

2. **Frontend Type System & Pinia Store (`src/types/profiler.ts`, `src/stores/useProfilerStore.ts`):**
   - Defined TypeScript contracts matching backend structures (`ProfilerRunRecord`, `ProfilerHotspotRecord`, `ProfilerHotspotDiffRecord`, `ContinuousProfileSummary`, `ProfilerManifestSummary`, `ScriptProfilerConfig`).
   - Created `useProfilerStore` with domain-bounded actions for fetching available profilers, querying runs, loading hotspots, streaming continuous DuckDB telemetry, executing sessions, and comparing runs.

3. **Workspace Profiler Tab (`src/components/workspace/ProjectProfilerTab.vue`):**
   - Registered `'profiler'` in `workspaceTab` union across `dialogueStore.ts`, `ProjectWorkspace.vue`, and keyboard shortcuts (`Alt+7`, and tab cycler `Alt+W` / `Ctrl+Tab`).
   - Built dual-pane inspection interface:
     - **Top Control Bar:** Target service/task selector, in-vivo active indicator, symbol-only manual profiling trigger (`Play`), diff view toggle (`GitCompare`), and refresh (`RotateCw`).
     - **Left Pane:** Continuous in-vivo DuckDB telemetry card (live p50, p95 CPU %, peak RAM MB) + Chronological list of recorded runs with driver badge and metrics summary.
     - **Right Pane:** Multi-format visual artifact reader rendering raw SVG flamegraphs from `scratchpad/profiling/` + Top Hotspots table ranked by self CPU %.
     - **Diff Comparator View:** Baseline vs. comparison run selector with delta percentage highlighting (red for regressions, green for improvements).

4. **Minimalist UX Refactoring in `ScriptsSection.vue`:**
   - Removed trivial action tooltips (`Restart Service`, `Stop Service`, `Start Service`, `Run Task`) in favor of universal icons.
   - Added contextual profiler indicator button (`Activity`):
     - Displays driver name, duration, sampling rate, and total run count on hover (e.g. `Profiler: samply (15s @ 1000Hz) • 3 runs`).
     - Animates with amber pulse when an active profiling session is in progress.
     - Single click triggers profiling on running services or opens the Profiler tab directly.
   - Enriched right-click context menu with `Profile Service (<N>s)` and `Open Profiling Tab`.

---

## 3. Verification & Diagnostics

- **Backend Tests:** `cargo test --manifest-path src-tauri/Cargo.toml` executed cleanly with 30 passed unit tests (0 failures).
- **Frontend Type & Build Verification:** `npm run build` (`vue-tsc --noEmit && vite build`) completed successfully with zero type or template errors.
- **Spec Synchronization:** Updated implementation checklist in `docs/specs/25_universal_telemetry_and_profiling_engine.md`.
