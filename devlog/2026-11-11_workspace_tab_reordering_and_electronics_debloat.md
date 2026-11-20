# Development Log: Workspace Tab Reordering, Electronics Debloat & Scope Model Selector

**Date:** 2026-11-11  
**Domain:** Workspace Layout, Embedded Instrumentation, UI Ergonomics, Zero Dead Code  
**Specification Reference:** [Spec 31](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/31_electronics_workstation_and_instrumentation_engine.md)

---

## 1. Objectives & Context
1. **Clean Visual Clutter from Electronics Navigation:**
   - Remove the distracting green dot indicator from the `Electronics` tab (and `RE Lab`).
   - Eliminate redundant verbose instrumentation status strings in the sub-tabs navigation bar.
   - Convert the bulky `Toolchains: X/5` button into a compact, symbol-only tool button (`<Wrench />`).
   - Remove the redundant global refresh button from the sub-tabs bar.
2. **Generic Oscilloscope Device Abstraction & Disconnected State:**
   - Replace static Owon text with a model selector (`AppSelect`) to support pluggable driver architectures (Owon VDS1022, Hantek 6022BE, generic SCPI).
   - Suppress loud red capture error banners when hardware is absent; render a clean, generic disconnected overlay on the oscilloscope canvas and gate interactive acquisition buttons.
3. **Dynamic Reorderable & Persistent Primary Workspace Tabs:**
   - Enforce user-defined default logical sequence: Overview $\to$ Explorer $\to$ Electronics $\to$ Visual Builder $\to$ Profiler $\to$ RE Lab $\to$ Scripts $\to$ Git $\to$ Journal $\to$ Skills.
   - Implement HTML5 Drag and Drop reordering across tabs.
   - Persist custom tab sequences per project in `localStorage['oneship_tab_order_<project_id>']`.

---

## 2. Implementation Details

### 2.1 Primary Workspace Dynamic Tabs (`src/components/workspace/ProjectWorkspace.vue`)
- Established canonical tab metadata dictionary `ALL_TABS` with explicit type safety.
- Default ordering updated to:
  `['overview', 'files', 'electronics', 'visual_builder', 'profiler', 'rea', 'terminal', 'git', 'devlog', 'skills']`.
- Added reactive drag handlers (`handleTabDragStart`, `handleTabDragOver`, `handleTabDrop`) with visual opacity and dashed border cues during dragging.
- Added project-scoped hydration from `localStorage` on project change.
- Removed hardcoded green dots on `rea` and `electronics`.

### 2.2 Electronics Sub-Tab Header Simplification (`src/components/workspace/ProjectElectronicsTab.vue`)
- Cleaned sub-tab labels to direct identifiers: `Firmware`, `Oscilloscope`, `Logic Analyzer`.
- Replaced the verbose `Toolchains: X/5` button with an icon-only `<Wrench />` button wrapped in `AppTooltip`.
- Removed dead computed properties `owonInstrument` and `logicInstrument` (Zero Dead Code Policy).
- Removed the redundant `<RotateCw />` global button.

### 2.3 Scope Model Selection & Clean Disconnect Overlay (`src/components/workspace/electronics/OscilloscopeView.vue`)
- Integrated `AppSelect` for `selectedScopeModel` (`owon_vds1022`, `hantek_6022be`, `scpi_generic`).
- Gated capture action buttons (`:disabled="!connectedOwon?.isConnected || ..."`).
- Suppressed loud red error banner when offline (`captureError && connectedOwon?.isConnected`).
- Rendered generic, elegant disconnected backdrop inside `containerRef`:
  - Title: *Oscilloscope Disconnected*
  - Subtitle: *Connect a supported device to start acquisition*

---

## 3. Verification & Testing
- `npm run build`: Success (`vue-tsc --noEmit` and Vite bundle generated in 4.07s with 0 errors).
- `cargo check --manifest-path src-tauri/Cargo.toml`: Success (0 errors, 0.43s).
