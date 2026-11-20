# Devlog: Script Launch Modal, Hover Marquee & UI Refinements

**Date:** 2026-09-05  
**Topic:** Script Launch Modal, Hover Marquee & UI Refinements  
**Status:** Completed

---

## 1. Context & Motivation
Following the compact four-subcolumn split of ProjectCard (Herder and Scripts side-by-side in a single row), several UX friction points remained on narrow viewports:
- Truncated script and agent names lacked a smooth way to read full identifiers.
- Inline option drawers expanded awkwardly inside constrained cards with unstyled, oversized toggles and non-palette backgrounds.
- Tooltips occasionally bled offscreen near window borders or caused horizontal scrollbars.
- Redundant actions (`<Terminal />` focus button on scripts when row click already focuses, blinking status dots, repetitive tooltip labels) created visual clutter.

## 2. Changes Implemented

### A. Modal Configuration & Launch Engine (`ScriptLaunchModal.vue`)
- Created `src/components/modals/ScriptLaunchModal.vue` providing a centered brutalist modal for scripts and deterministic tasks that define configurable flags/options.
- Replaced the oversized toggle with a minimalist option dot/square that activates with accent color matching the agent status indicator.
- Integrated `AppSelect` for choice-based options.
- Bound selection state to `localStorage` (`kapawu:script_options:${projectId}:${scriptId}`) ensuring configured options persist across restarts.
- Completely removed obsolete inline option drawers and redundant `<Terminal />` buttons from `ScriptsSection.vue`.

### B. Viewport-Clamped Tooltips (`AppTooltip.vue`)
- Enhanced boundary detection in `showTooltip()`:
  - If target bounding rect is within 220px of the right window border, the tooltip anchors to the right edge and translates leftwards (`translate(-100%, ...)`).
  - If within 220px of the left window border, it anchors to the left edge and translates rightwards (`translate(0, ...)`).
- Added `max-w-xs md:max-w-md break-all whitespace-normal` to wrap lengthy commands and arguments without triggering horizontal viewport overflow.
- Cleaned tooltip text across agents and scripts to display strictly technical details (`$ command args` or `role (runner)`) without repeating the entity identifier.

### C. Hover Marquee Composable (`useHoverMarquee.ts`)
- Created `src/composables/useHoverMarquee.ts` to detect text overflow (`scrollWidth - clientWidth > 2px`).
- On `mouseenter`, calculates pixel overflow distance and smoothly translates text leftward at ~35ms per pixel.
- On `mouseleave`, smoothly returns to origin.
- Applied subtle right-edge fade mask (`[mask-image:linear-gradient(to_right,black_85%,transparent_100%)] group-hover:[mask-image:none]`) on row labels.
- Pinned row action buttons absolutely to the right (`absolute right-1 top-1/2 -translate-y-1/2 bg-[var(--bg-color)]/95`) so scrolling text smoothly disappears underneath without shifting controls.

### D. Visibility Filters & Persistence
- Added `Eye` / `EyeOff` filter toggles to `PERSISTENT` and `TASKS` sub-headers in `ScriptsSection.vue`.
- Added `localStorage` persistence across all 4 sub-columns:
  - `kapawu:visibility:project_agents:${projectId}`
  - `kapawu:visibility:internal_agents:${projectId}`
  - `kapawu:visibility:persistent:${projectId}`
  - `kapawu:visibility:tasks:${projectId}`

## 3. Verification & Compliance
- Successfully compiled with `npm run build` (`vue-tsc --noEmit && vite build`) with zero type errors.
- Adheres to Rule 5 (symbol-only action buttons with `AppTooltip`), brutalist aesthetic with CSS variables, zero mock data, and zero dead code.
