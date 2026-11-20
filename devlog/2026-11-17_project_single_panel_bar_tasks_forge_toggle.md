# 2026-11-17 — Project Panel: Single Bar with a Tasks/Forge Toggle

## Summary

The Project workspace panel stacked two 36 px `SubtabBar` rows: one dedicated to the `Tasks | Forge` mode and another for the active mode's views (and, in Forge, a third bar owned by `ForgeCollabPanel`). Both are now collapsed into a single row: a fixed segmented `Tasks | Forge` toggle on the left, the active mode's views next to it, and that mode's tools on the right. `ForgeCollabPanel` became content-only so only one component owns the panel bar. No daemon, IPC, or Rust changes.

## Changes Implemented

### 1. `SubtabBar` leading zone (`src/components/ui/SubtabBar.vue`)

- Added an optional `leading` slot rendered before the tabs, outside the horizontal scroll container, behind a divider that only appears when the slot is provided.
- Tabs keep their existing scroll behavior; the leading control never scrolls away.

### 2. `AppSegmentedToggle` (`src/components/ui/AppSegmentedToggle.vue`, new)

- Strict mutually exclusive switch (`modelValue` + `items` with `id`, `label`, `icon`, `hidden`).
- One bordered `h-6` strip: active option carries the accent fill, inactive options stay quiet with hover surface. Dividers are placed between visible options only, so hidden prerequisites do not leave a leading border.
- Exported from `src/components/ui/index.ts` as `AppSegmentedToggle` and `SegmentedItem`.

### 3. `ProjectManagementTab.vue`

- Removed the standalone mode bar and the `modeItems` tab list; the mode is now `modeToggleItems` rendered inside the single bar's `leading` slot through `AppSegmentedToggle` (only when the project has git).
- The bar's tabs are mode-dependent: `Board | Schedule | Gantt` for Tasks, `Issues | Pull Requests (or Merge Requests) | Releases` for Forge, using the counters from `forgeCollabStore` and the provider's PR label.
- Forge without a remote renders the toggle alone (no contextual tabs, no right zone) above the existing link-repository empty state.
- The `project/forge` view node is now read here through `useUiSurfaceNode`, so the bar and the panel share the same selection.
- The right zone swaps between the Tasks filters/Gantt toolset and the new `ForgeTopologyTools`.

### 4. `ForgeTopologyTools.vue` (new)

- Provider badge plus `origin` / `upstream` / `target` chips extracted verbatim from the old Forge bar, reading `forgeCollabStore.collabContext` directly.

### 5. `ForgeCollabPanel.vue`

- Dropped its own `SubtabBar`, the local `handleSwitchTab`, the unread-count computed, and the unused icons/imports.
- Tab selection now arrives through `project/forge`; a `watch(activeTab)` keeps the lazy Releases fetch, and `loadInitial` also loads Releases when the restored node is `project/forge/releases` (previously the click handler was the only trigger, which no longer exists for an already-active tab).

## Specs Updated

- `13 §2.8` — `AppSegmentedToggle` contract.
- `13 §3.13` — panel bar leading zone and the rule that mode + views share one row.
- `34 §6.5` — single-bar panel chrome for Tasks/Forge.
- `38 §3` and `39 §10` — unread badges and topology ownership moved to the Project panel bar.

## Verification

No build or runtime process was started (AGENTS.md §1.6). Pending manual check in the running GUI: toggle both modes, Forge with and without a remote, Gantt tools, restored `project/forge/releases`, and narrow-width tab scrolling.
