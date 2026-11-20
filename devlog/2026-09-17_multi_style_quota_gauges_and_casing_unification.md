# Devlog: Multi-Style Quota Gauges, Brutalist Select & MCP Port Synchronization

**Date:** 2026-09-17  
**Topic:** Implementation of Selectable Multi-Style Quota Gauges (Concentric Dual-Ring Radar Clock, Dual-Linear Bi-Metric Bars, Classic Pie), Interactive Settings Preview Selector, Full 4-Stream Telemetry (5h + 7d Windows), Uppercase Account Casing, Custom Brutalist `AppSelect`, Canonical Orchestrator Model Catalog with Adaptive Reasoning Effort, and Multi-Project MCP Port Auto-Propagation.

---

## 1. Context & Objectives

The multi-account switcher, quota telemetry, and settings configuration required architectural refinements:
1. **Casing Inconsistency:** Account identifiers appeared in lowercase in the closed selector button while rendering uppercase in dropdown menus.
2. **Missing Time Horizon in Quota Gauges:** Existing micro pie charts only rendered remaining token capacity percentages without representing the progress and time countdown of the 5-hour rolling and 7-day weekly windows.
3. **Multi-Style Visual Selection:** The user requested the ability to configure and preview different visualization styles (`concentric`, `bars`, `classic`) dynamically from the global settings panel with real-time rendered previews.
4. **4-Stream Completeness Rule:** Both 5-hour and 7-day metrics for 1P (Gemini Pool) and 3P (Claude/GPT Pool) must be permanently represented in both compact header mode and expanded list mode.
5. **Orchestrator Model & Reasoning Effort:** `default_model` contained a truncated, non-canonical list, and `default_effort` was missing. Reasoning effort needed to adapt dynamically to model capability: Flash (low, medium, high), Pro (low, high), Claude/3P (fixed / N/A).
6. **Generic Dropdown Popover:** `AppSelect.vue` utilized native HTML `<select>` elements, causing OS-native menus that broke theme consistency.
7. **MCP Port Propagation:** `mcp_port` was hardcoded to 8765 in `skill.rs` and needed dynamic resolution and auto-propagation to all registered project `.agents/plugins/wang-kapawu/mcp_config.json` files on global save.

---

## 2. Implemented Architecture & Components

### A. Modular Quota Gauge Suite (`src/components/ui/`)
- **[`quotaUtils.ts`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/ui/quotaUtils.ts):** Shared duration parsing and time decay ratio math.
- **[`AppQuotaConcentric.vue`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/ui/AppQuotaConcentric.vue):** Dual-ring radar clock with window countdown track and inner capacity pie.
- **[`AppQuotaBar.vue`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/ui/AppQuotaBar.vue):** Segmented dual-linear bars.
- **[`AppQuotaPie.vue`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/ui/AppQuotaPie.vue):** Refactored classic pie.
- **[`AppQuotaGauge.vue`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/ui/AppQuotaGauge.vue):** Unified dispatcher.
- **[`QuotaGaugePreviewSelector.vue`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/ui/QuotaGaugePreviewSelector.vue):** Live preview selector in Settings.

### B. Custom Brutalist Dropdown (`AppSelect.vue`)
- **[`src/components/ui/AppSelect.vue`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/ui/AppSelect.vue):**
  - Replaced native HTML `<select>` with custom brutalist popover.
  - Features chevron state indicator, active option marker (`● [✓]`), keyboard navigation (ArrowUp, ArrowDown, Enter, Space, Escape), and outside-click dismissal.

### C. Orchestrator Model Catalog & Adaptive Reasoning Effort
- **[`src-tauri/src/config.rs`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/config.rs):**
  - Updated `default_model` to canonical identifiers (`gemini-3.8-flash`, `gemini-3.7-flash`, `gemini-3.6-flash`, `gemini-3.1-pro`, `claude-sonnet-4-6`, `claude-opus-4-6-thinking`, `gpt-oss-120b-medium`).
  - Added `default_effort` and `init_effort` (`select_enum`, options `["low", "medium", "high"]`, default `"high"`).
- **[`src-tauri/src/kopeng/initializer.rs`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/kopeng/initializer.rs):**
  - `effective_effort` dynamically queries `orchestrator.init_effort` from effective config.
- **[`src/components/modals/SettingsModal.vue`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/modals/SettingsModal.vue):**
  - Connected `default_effort` and `init_effort` options dynamically based on the chosen model (Flash: low/medium/high; Pro: low/high; Claude/3P: N/A) with automated watcher normalization.

### D. MCP Port Resolution & Multi-Project Propagation
- **[`src-tauri/src/kopeng/skill.rs`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/kopeng/skill.rs):**
  - `inject_project_mcp_plugin` dynamically resolves `mcp_port` from `load_global_config()`.
- **[`src-tauri/src/config.rs`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/config.rs):**
  - In `save_global_config`, whenever `mcp_port` is saved, the engine automatically iterates over all registered projects via `crate::registry::get_projects()` and synchronizes both `.agents/plugins/wang-kapawu/mcp_config.json` and `.agents/mcp_config.json`.

---

## 3. Verification & Compliance

- **TypeScript / Vue Build:** `npm run build` passed with zero errors.
- **Rust Backend:** `cargo check` passed with zero errors.
- **Tiger Style Engineering:** Explicit unit suffixes (`_seconds`, `_pct`), deterministic time clamping, bounded geometry.
