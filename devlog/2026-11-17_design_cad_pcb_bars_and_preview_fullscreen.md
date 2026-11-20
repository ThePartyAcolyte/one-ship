# 2026-11-17 — Design/CAD/PCB Toolbars, Sidebars & Preview Fullscreen

## Summary

Standardized the Design, CAD, and PCB toolbars on the single `h-9` panel-bar contract and gave their sidebars the same surface/typography/selection treatment. Preview received the same toolbar geometry plus a fullscreen fix: the overlay now covers the whole window, the project rail and the agent bar are hidden while fullscreen is active, and the agent bar no longer reflows upward. No behavior, commands, or element sets changed.

## Changes Implemented

### 1. Design (`ProjectDesignTab.vue`)

- Toolbar: `h-9 px-3 ui-surface-title` with a bottom border, no wrapping; the left group scrolls horizontally when narrow, the right group stays fixed.
- Symbol actions (sidebar toggle, editor launch) use `.ui-icon-btn`; the sidebar toggle keeps the accent state while open. The file selector is a `compact` `h-6` `AppSelect`.
- Zoom cluster is `h-6`: symbol buttons, `text-[9px]` zoom readout, `text-[9px]` uppercase `Fit Width/Fit All/Free` and `100%` actions with accent when the mode is not `Free`.
- Sidebar: `ui-surface-list`; section labels at `text-[9px]`; pages/layers at `text-[10px]` with hover border and an accent border/fill for the selected node.
- Empty state card uses `ui-surface-card`.

### 2. CAD (`ProjectCadTab.vue`)

- Same toolbar contract. `flex-wrap` removed; `Cut`/`Clear` and the view selector are `h-6`, and the section input is `!h-6 !text-[10px]`.
- Refresh and editor actions use `.ui-icon-btn`.
- Sidebar: `ui-surface-list`; summary/bodies/parameters use `ui-surface-card`; section labels at `text-[9px]`. No sections or controls were moved.

### 3. PCB (`ProjectPcbTab.vue`)

- Same toolbar contract with `compact` selects for target, mode, and sheet.
- Sidebar: `ui-surface-list`; summary header and BOM/Nets/DRC tabs on `ui-surface-title`; tabs at `text-[9px]` with an accent fill when active; BOM filter at `h-6`; component and net rows use `ui-surface-card`.
- Review banner, error banner, DRC findings, and the canvas stage keep their structure and semantic colors; the floating snapshot readout uses `ui-surface-card`.

### 4. Preview fullscreen (`ProjectPreviewTab.vue`, `ProjectWorkspace.vue`)

- `ProjectPreviewTab` emits `fullscreen-change`; `ProjectWorkspace` stores it locally and hides `ProjectVerticalRail` and `AgentTaskbar` with `v-show` while active.
- The fullscreen root is `fixed inset-0 z-[45]`: above the rail, docked/floating chat, and chat resizer; below modals (`z-50`) and tooltips.
- `Escape`, the toolbar toggle, and unmounting the tab all exit fullscreen and restore the rail and agent bar.
- Toolbar restyled to `h-9 ui-surface-title`: plain truncated URL, `.ui-icon-btn` reload, `h-6` device presets with accent fill, and `h-6` `Browser`/`Fullscreen` actions.

## Specs Updated

- `13 §3.6` — Preview fullscreen ownership and stacking.
- `13 §3.13` — Design/CAD/PCB toolbar and sidebar contract.

## Verification

No build or runtime process was started (AGENTS.md §1.6). Pending manual check: Design sidebar collapse, CAD section cut controls, PCB BOM/Nets/DRC tabs, narrow-width toolbar scrolling, and Preview fullscreen (rail and agent bar hidden, `Escape` restores both, tab switch restores both).
