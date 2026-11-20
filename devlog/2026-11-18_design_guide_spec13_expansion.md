# 2026-11-18 — Design Guide: Spec 13 Expanded into an Agent UI Contract

## Summary

Spec 13 was expanded from a token/component reference into the normative design guide agents must follow for any visual work. The guide now defines an agent contract, spacing/sizing/typography scales, a component index, composition recipes, state and color semantics, copy/overflow/accessibility/motion rules, an implementation checklist with anti-patterns, and a reference map. `AGENTS.md` §5 now declares Spec 13 §0/§11 mandatory.

## Changes

### Spec 13 (version 1.3.1 → 1.4.0)

- **§0 Agent Design Contract:** reading order, precedence (tokens → primitives → components → panel contracts → local layout), non-negotiables, Definition of Done.
- **§1.3 Spacing, Sizing & Typography:** 2 px grid with allowed steps; interactive height scale (`h-6` controls, `h-7` form actions, `h-8` primary form action, `h-9` bars); width conventions; square geometry; 8/9/10/11/12 px type scale (7 px prohibited); Lucide sizes; opacity/wash rules.
- **§2.10 Component Index:** full `src/components/ui/` inventory with use/avoid guidance.
- **§4 Composition Recipes:** panel skeleton, master-detail, single bar with inline search and filter menu, toolbars, cards, lists/tables, forms, modals, popovers, empty/loading/error, terminal surfaces, status/telemetry.
- **§5 State Matrix:** rest/hover/active/selected/focus/disabled for every control family.
- **§6 Color Semantics:** token-to-meaning table, accent budget, status-only status colors, palette prohibition.
- **§7 Copy, Labels & Units:** English, CSS uppercase, no redundant parentheses, empty/error copy structure, Tiger Style units.
- **§8 Overflow & Responsive:** no wrapping bars, one scroll container per pane, minimum sidebar widths, scrollbar contract, truncation tooltips.
- **§9 Accessibility & Keyboard:** tooltips plus `aria-label`, `aria-pressed`, Escape/focus return, visible focus, labels.
- **§10 Motion & Layering:** 150–200 ms color transitions and the official z-index ladder (`0` → `99999`).
- **§11 Agent Checklist & Anti-Patterns:** pre/during/post workflow, targeted grep audit, Definition of Done and a 14-row anti-pattern table.
- **§12 Reference Map:** files, panels and related specs.
- Corrected stale facts: `AppTooltip`/`AppSelect` popover z-index (`99999`), `AppInput` surface tokens, `AppSelect` compact usage.

### `AGENTS.md` §5

- Added the mandatory blockquote requiring Spec 13 §0/§11 before any UI change and making its checklist part of the Definition of Done.
- Corrected the tooltip z-index reference.

### `TECHNICAL_DEBT.md`

- Added item **#41 Legacy Palette and Micro-Typography Migration**: Oscilloscope/Logic Analyzer still use hardcoded palette classes and legacy bar geometry; tracked as the first migration under the new guide.

## Verification

Documentation-only change; no build or runtime process was started (AGENTS.md §1.6). Pending: the first UI work item (Electronics instrument views, debt #41) should apply §11 end-to-end and validate the checklist in practice.
