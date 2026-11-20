# Design System Foundations

## Purpose

Defines the ONE-SHIP visual language: an industrial, high-contrast, brutalist developer interface that adapts dynamically to the Omarchy desktop theme. This document is the single source of truth for every visual element and the contract every agent or contributor must follow before writing UI code. It covers theme variables, surfaces, spacing, sizing, typography, interaction states, color semantics, copy rules, overflow, accessibility, motion and layering.

## Scope

In scope: theme tokens, surface hierarchy, spacing/height/width/typography scales, icons, opacity, interaction state matrix, color semantics, copy and unit conventions, overflow behavior, accessibility, motion, z-index ladder, the agent design contract and the post-change audit.

Out of scope (see Related Specs): the component library and composition recipes, workspace chrome and per-panel contracts, UI state persistence.

## Model / Design

### Agent Design Contract (Mandatory)

Any agent implementing or modifying UI MUST follow this contract before writing code.

**Reading order**

1. This contract and the Agent Checklist (below) — mandatory.
2. Spacing, Sizing & Typography Scale and Surface Color Hierarchy — mandatory for any layout.
3. Component library and mandatory rules (see [UI Component Library and Composition Recipes](02-ui-component-library-and-recipes.md) and [Workspace Chrome and Panel Contracts](03-workspace-chrome-and-panel-contracts.md)) — mandatory when choosing controls.
4. Composition recipes — mandatory for a new panel, toolbar, table, form, modal or dropdown.
5. The specific panel contract when editing a workspace tab.

**Precedence (highest to lowest)**

1. Active theme variables (`--bg-color`, `--fg-color`, `--dim-color`, `--border-color`, `--accent-color`, `--status-*`) resolved at runtime via `get_active_theme` (built-in dark/light presets, or live Omarchy `colors.toml` sync when `Auto (Omarchy)` is selected).
2. Four surface tokens and shared primitives in `src/styles/ui.css` (`.ui-search-box`, `.ui-icon-btn`, `.ui-kbd`, `.ui-modal-footer`, `.ui-overlay-backdrop`).
3. Standardized components in `src/components/ui/` (`AppTooltip`, `AppSelect`, `AppInput`, `AppTextarea`, `AppToggle`, `AppSegmentedToggle`, `AppEmptyState`, `SubtabBar`, …).
4. Panel contracts and composition recipes.
5. Local Tailwind composition (spacing/layout only). Local color, height, radius or typography tokens are prohibited.

**Non-negotiables**

- **NEVER** hardcode a Tailwind palette color (`zinc-*`, `emerald-*`, `amber-*`, `rose-*`, `cyan-*`, `text-white`, `bg-black/N`). Use semantic tokens or `.ui-surface-*`.
- **NEVER** render native `<select>`/`<option>` or native `title="..."` tooltips.
- **NEVER** duplicate a panel title inside the panel; the workspace tab label identifies the view.
- **NEVER** add an outer padding/bordered shell around a full-tab panel; panels are full-bleed.
- **NEVER** invent a new control when `src/components/ui/` already provides one; extend the component instead.
- **ALWAYS** express numeric units (time, size, counts) with explicit suffixes in code and configuration.
- **ALWAYS** propagate a visual contract change to code, the affected spec and the devlog in the same work cycle.

**Definition of Done for visual work.** A visual change is complete only when:

1. It uses only semantic tokens and standardized components.
2. It implements the full Interaction State Matrix (hover, active, selected, focus, disabled where applicable).
3. It passes the Agent Checklist and the targeted grep audit.
4. The affected spec (panel contract, component section, or a new subsection) reflects the implemented truth.
5. A devlog entry records the change and any deliberate exception.

### Theme Variables

All styling is driven strictly by CSS custom properties:

| Variable | Semantic Meaning | Usage Guidelines |
| :--- | :--- | :--- |
| `--bg-color` | Base window / canvas background | Applied to main viewports, modal containers, and dropdown lists. |
| `--fg-color` | Primary text and icons | Highest contrast content: titles, active inputs, strong indicators. |
| `--dim-color` | Secondary / muted text | Subtitles, helper text, inactive borders, secondary labels. |
| `--border-color` | Structural 1px border lines | Structural cards, dividers, buttons, inputs. |
| `--accent-color` | Dynamic Omarchy accent | Focus rings, active tabs, pulse indicators, primary action buttons. |
| `--status-ok-color` | Semantic success / operational status | Parsed from Omarchy `green` / `color2` (fallback `#10b981`). |
| `--status-warn-color` | Semantic warning / alert status | Parsed from Omarchy `yellow` / `orange` / `color3` (fallback `#f59e0b`). |
| `--status-error-color` | Semantic error / failure status | Parsed from Omarchy `red` / `color1` (fallback `#ef4444`). |
| `--status-info-color` | Semantic informational status | Parsed from Omarchy `cyan` / `blue` / `color6` (fallback `#06b6d4`). |

Derived translucent borders and background washes are generated automatically via `color-mix(in srgb, var(--status-*-color) X%, transparent)` tokens (`--status-ok-border`, `--status-ok-bg`, `--status-warn-border`, `--status-warn-bg`, `--status-error-border`, `--status-error-bg`, `--status-info-border`, `--status-info-bg`) to maintain brutalist styling without hardcoded Tailwind palette classes.

### Typography Hierarchy, Optical Normalization & Scaling

Typography is strictly hierarchical, functional, and user-configurable:

| Token / CSS Property | Value / Source | Usage & Invariants |
| :--- | :--- | :--- |
| `--title-font-family` | `'Protest Guerrilla', sans-serif` | Reserved strictly and exclusively for the primary application title (`ONE-SHIP`). Never used as body or interface font. |
| `--app-font-family` | Configurable (`IBM Plex Mono`, `Anonymous Pro`, `Courier Prime`) | Monospace typography family applied globally to all UI labels, metrics, editors, and terminal elements (`.font-mono`). |
| `size-adjust` | `112%` (`Anonymous Pro`), `115%` (`Courier Prime`) | Optical normalization defined in `@font-face` rules to align baseline x-height and optical weight across alternative monospace fonts against the baseline `IBM Plex Mono`. |
| `--app-font-scale` | `0.9` to `1.3` (Default: `1.0` / 100%) | Global zoom scaling factor bound via `html { zoom: var(--app-font-scale, 1); }`, enabling smooth viewport and font scaling without breaking flexbox constraints. |

### Surface Color Hierarchy (`src/styles/ui.css`)

To enforce consistent depth and visual hierarchy across modals, drawers, and panels, surfaces derive from `--bg-color` via four standardized semantic tokens and utility classes:

| Token | Utility Class | Derivation | Usage |
| :--- | :--- | :--- | :--- |
| `--ui-bg-base` | `.ui-surface-base` | `var(--bg-color)` | Base container background for modals, drawers, and workspace panels. |
| `--ui-bg-title` | `.ui-surface-title` | `color-mix(in srgb, var(--bg-color) 78%, #000)` | Header bars, title bars, search bars, and modal footers. |
| `--ui-bg-list` | `.ui-surface-list` | `color-mix(in srgb, var(--bg-color) 62%, #000)` | Recessed scrollable list/log/navigation areas. |
| `--ui-bg-card` | `.ui-surface-card` | `color-mix(in srgb, var(--bg-color) 90%, var(--fg-color) 5%)` | Inner cards, items, and grouped rows inside lists or base containers. |

**Single Source of Truth (Strict).** Only the four `--ui-bg-*` tokens are valid surface tokens. The legacy tokens `--surface-color`, `--card-bg`, `--bg-card`, `--text-color`, `--text-primary`, `--bg-primary`, and `--status-warning-*` are undefined and prohibited. A `var()` reference to an undefined token invalidates the declaration at computed-value time, silently degrading backgrounds to `transparent` (this is what made the bottom agent taskbar and multiple popovers appear translucent). New surfaces must use `--ui-bg-*`, `--fg-color`, and `--status-warn-*` directly.

### Spacing, Sizing & Typography Scale

**Base grid: 2 px.** Only the steps below are valid for spacing (`p-*`, `m-*`, `gap-*`, `space-y-*`). Arbitrary values (`p-[7px]`) are prohibited.

| Step | Tailwind | Usage |
| :--- | :--- | :--- |
| 2 px | `*-0.5` | icon↔label, pill inner gaps |
| 4 px | `*-1` | button groups, chip gaps |
| 6 px | `*-1.5` | toolbar group separation |
| 8 px | `*-2` | standard group separation, bar padding |
| 10 px | `*-2.5` | tight card padding |
| 12 px | `*-3` | standard card padding |
| 14 px | `*-3.5` | dense cards (legacy, avoid in new work) |
| 16 px | `*-4` | section separation, panel gutters |
| 24 px | `*-6` | empty-state/modal padding |

**Interactive heights.** No other heights are allowed for controls:

| Height | Tailwind | Usage |
| :--- | :--- | :--- |
| 20 px | `h-5` | inline micro badges only (never buttons) |
| 24 px | `h-6` | every control inside a bar, compact select, inline input, `.ui-icon-btn` |
| 28 px | `h-7` | form actions, modal footer actions, secondary/primary form buttons |
| 32 px | `h-8` | standalone primary form action |
| 36 px | `h-9` | panel bars (`SubtabBar`), workspace tab bar, `AgentTaskbar`, modal headers/footers |

Taller rows (`h-10+`) are reserved for non-interactive content (log lines, list items, telemetry).

**Widths.** Left project rail `w-11` (44 px); sidebars `w-60` (240), `w-64` (256), `w-72` (288), `w-80` (320); selects `w-32`/`w-36`/`w-40`/`w-44`; search inputs flex. Arbitrary `w-[...]` is allowed only for measured canvases (preview, oscilloscope, PCB stage).

**Geometry.** `border-radius: 0` everywhere. Borders are 1 px semantic tokens. Shadows (`shadow-2xl`) are reserved for modals, anchored popovers and menus; cards and toolbars are flat.

**Typography scale.**

| Size | Tailwind | Usage |
| :--- | :--- | :--- |
| 8 px | `text-[8px]` | badges, counts, micro metadata, uppercase tags |
| 9 px | `text-[9px]` | bar labels, controls, tabs, chips, table headers |
| 10 px | `text-[10px]` | panel body, dense lists, settings labels |
| 11 px | `text-[11px]` | standard body text, markdown, editor content |
| 12 px | `text-[12px]` | card titles, modal titles (maximum) |
| 14–16 px | `text-sm`/`text-base` | exceptional standalone headings only |

- `text-[7px]` and arbitrary sizes are prohibited in new work.
- Uppercase is produced by CSS (`uppercase tracking-wider`), never typed into strings.
- Prose uses `leading-relaxed`; logs and code blocks use `leading-tight`.
- The interface font is the configured `--app-font-family`; `--title-font-family` is reserved for the single application title.

**Icons.** Use Lucide components only. Sizes: 9 (micro), 10–11 (bar controls), 12–14 (toolbar/primary actions), 16 (empty-state actions), 20–32 (empty-state art). Always use `:size`, never `w-* h-*`, for Lucide. Icon-only buttons require `AppTooltip` plus `aria-label`.

**Opacity and washes.** Accent washes are `/10`, `/15`, `/25`; semantic status washes use `--status-*-bg`; borders use `/40` or `/60`; neutral hover may use `hover:bg-white/5`. Solid white/black surfaces are prohibited. Sanctioned exception: the terminal/log body uses `bg-[#09090b]` (see Terminal and log surfaces in the recipes spec).

### Interaction State Matrix

| Control | Rest | Hover | Active / Selected | Focus | Disabled |
| :--- | :--- | :--- | :--- | :--- | :--- |
| Text button | `border-[var(--border-color)] text-[var(--dim-color)]` | accent border + text | accent border + `bg-accent/15` + accent text | visible accent border | `opacity-40 cursor-not-allowed` |
| Icon button (`.ui-icon-btn`) | base surface + dim | accent border + accent icon | `!border-accent !text-accent` (+ optional `!bg-accent/15`) | visible accent border | `opacity-40` |
| Input / Select | `border-[var(--border-color)]` | border stays; text brightens | — | accent border (`focus` / `focus-within`) | `opacity-50` |
| Tab (`SubtabBar`) | transparent border + dim | fg text + `bg-white/5` | accent border + `bg-accent/15` + accent text | accent border | not used |
| Chip / filter | border + dim | accent border + fg | accent border + `bg-accent/15` + accent bold | accent border | `opacity-40` |
| List row | transparent border | border + fg | accent border/fill | accent border | `opacity-50` |
| Card | border + card surface | accent border `/60` | selected accent border/fill | accent border | `opacity-50` |
| Segmented option | dim | fg + `bg-white/5` | accent fill `/15` + accent text | accent border | `opacity-40` |

Focus must always be visible; never remove an outline without replacing it with an accent border. Keyboard order follows visual order.

### Color Semantics

| Meaning | Token | Typical usage |
| :--- | :--- | :--- |
| Base canvas | `--bg-color` / `ui-surface-base` | panels, modals, dropdown bodies |
| Bar / header | `--ui-bg-title` / `ui-surface-title` | SubtabBar, toolbars, modal header/footer |
| Recessed list | `--ui-bg-list` / `ui-surface-list` | sidebars, log panes, row containers |
| Card / grouped item | `--ui-bg-card` / `ui-surface-card` | cards, rows, popovers, empty state |
| Primary text | `--fg-color` | titles, values, active labels |
| Secondary text | `--dim-color` | metadata, hints, inactive labels |
| Structure | `--border-color` | all 1 px borders and dividers |
| Brand/active | `--accent-color` | active tab, selected row, focus, primary action, live pulse |
| Success | `--status-ok-*` | running, passed, installed, resolved |
| Warning | `--status-warn-*` | pending, degraded, missing tool, P2 |
| Error | `--status-error-*` | failed, destructive, critical, P1 |
| Info | `--status-info-*` | neutral informational badges/links |

Rules:
- One primary accent action per bar/region. Accent is not a decoration.
- Status colors only describe status, never brand or emphasis.
- Semantic washes use the derived `--status-*-bg`/`-border`; do not mix `color-mix` inline when a token exists.
- Tailwind palette classes (`zinc`, `emerald`, `amber`, `rose`, `cyan`, `slate`, `text-white`, `bg-black/N`) are prohibited in new work. Legacy occurrences are migration debt.

### Copy, Labels and Units

- English only. Tabs, buttons and labels use terse function-driven terms; explanations go in `AppTooltip`. (Optional Lang Belta immersion is a separate, deferred tier; see [Localization and Belta Language Strategy](06-localization-and-belta-language.md). Internationalization is reserved for that progressive dialect tier and must not introduce fragmented or partial translations into core developer workflows.)
- Uppercase comes from CSS, never from typed strings.
- No redundant parentheticals (`..`, `Root`, not `.. (Parent Directory)`), no "Click here", no duplicate panel titles.
- Empty state copy: what is missing → why it matters → next action.
- Error copy: what failed → cause → next action; keep the technical detail in a monospace block or log.
- Units follow Tiger Style: `_ms`, `_seconds`, `_bytes`, `_mbytes`, `_percent`, `_count`; UI values include the unit (`12 ms`, `48 MB`, `72%`).
- Numbers in tables use `tabular-nums`; durations and sizes are right-aligned.

### Overflow and Responsive Behavior

- Bars never wrap. Put variable content in a `min-w-0 overflow-x-auto scrollbar-none` group.
- One vertical scroll container per pane; avoid nested scroll areas unless a card list genuinely needs its own bounded scroll.
- Sidebars keep their minimum width; collapse to icons before truncating essential controls.
- Truncated text (`truncate`) that carries meaning gets an `AppTooltip`.
- Wide tables scroll horizontally inside their own wrapper; headers stay aligned.
- Never use `scrollbar-thin`, `scrollbar-thumb-*` or `scrollbar-track-*`; use the global scrollbar contract (see the chrome spec) or `.scrollbar-none` intentionally.
- Respect `--app-font-scale`; avoid fixed pixel canvases that clip at 130%.

### Accessibility and Keyboard

- Icon-only buttons: `AppTooltip` + `aria-label`; symbol-only actions are allowed only when the container makes the meaning obvious.
- Toggles and segmented controls expose `aria-pressed` (or an equivalent selected state).
- `Escape` closes modals, popovers, menus and fullscreen overlays; focus returns to the trigger.
- Every control is reachable by keyboard in visual order; no keyboard traps.
- Inputs use visible labels; placeholders are hints, not labels.
- Status is never color-only: pair it with text or an icon.
- Do not use `--dim-color` at low opacity for essential instructions.

### Motion and Layering

- Transitions: 150–200 ms, color/border/background only (`transition-colors`). No layout animations.
- Spinners: `Loader2 animate-spin`; live indicators may pulse; nothing else animates continuously.
- Official z-index ladder:

| Layer | z-index | Examples |
| :--- | :--- | :--- |
| Content | 0 | panels, cards, tables |
| Sticky content | 10 | sticky headers, floating HUD chips |
| Docked chat | 20 | right chat column |
| Taskbar / resizer | 30 | `AgentTaskbar`, chat resize handle |
| Drawer backdrop / floating chat | 40 | project drawer scrim, floating conversation |
| Fullscreen preview | 45 | `ProjectPreviewTab` fullscreen |
| Rail / drawers / modals / popovers | 50 | `ProjectVerticalRail`, modals, anchored menus, context menus |
| Drag shield | 9999 | global resize shield |
| Tooltips / select menus | 99999 | `AppTooltip`, `AppSelect` popover |

A new layer must be added to this table before use. Never guess a z-index.

### Agent Checklist and Anti-Patterns

**Before coding**

1. Identify the workspace panel and read its panel contract.
2. List the components and states involved; reuse library components.
3. Decide surfaces, bar height, control height and empty/loading/error states from the scales and recipes.
4. Confirm the data contract change (if any) and plan the atomic propagation to backend, UI, spec and devlog.

**While coding**

- Use semantic tokens and `.ui-surface-*`; no palette classes.
- One `SubtabBar` per panel; no stacked mode + view bars.
- Use stable `:key` identifiers for every `v-for`.
- Register and teardown every event listener; no leaks.
- Keep sidebars scoped to the project; never key state by bare `agent_id`.

**Post-change audit (targeted grep).** Run from the repository root and justify every remaining hit:

```
rg -n "zinc-|emerald-|amber-|rose-|cyan-|slate-|text-white|bg-black|text-\[7px\]|flex-wrap" src/components
rg -n "title=\"" src/components --glob '*.vue'
rg -n "<select" src/components --glob '*.vue'
rg -n "scrollbar-thin|scrollbar-thumb|scrollbar-track" src
rg -n "v-for=.*:key=\"(index|idx)\"" src/components
rg -n "p-2[^0-9].*(h-full|overflow-hidden)" src/components/workspace
```

Charts, third-party SVG and intentional terminal surfaces are valid exceptions; record them in the devlog.

**Definition of Done**

- Tokens/components only; no new palette or size token.
- State matrix implemented.
- Audit clean or exceptions documented.
- Spec updated (panel contract or component section).
- Devlog entry written.

**Anti-patterns**

| Anti-pattern | Why it fails | Correct approach |
| :--- | :--- | :--- |
| `p-2` wrapper around a full-tab panel | breaks full-bleed alignment with other tabs | remove the wrapper padding |
| Mode bar stacked above a view bar | wastes 36 px and duplicates navigation | `SubtabBar` leading + tabs |
| Second row of filter chips | consumes vertical space and fragments the bar | inline search + filter dropdown |
| Icon-only action for a primary workflow | undiscoverable | labeled `h-6 px-2 text-[9px]` button |
| Duplicate panel title | redundant chrome | rely on the tab label |
| `text-white` / `bg-black/40` | ignores the Omnisearch/Omarchy theme | `--fg-color` / `ui-surface-*` |
| `text-[7px]` metadata | unreadable | minimum `text-[8px]`, prefer 9 |
| Native `<select>` or `title=""` | broken Linux rendering / inconsistent tooltip | `AppSelect` / `AppTooltip` |
| `flex-wrap` on a toolbar | changes bar height and breaks alignment | horizontal scroll group |
| `v-for` with index key | reconciliation bugs | stable `item.id` |
| Modal without `Escape` | keyboard trap | close contract (modal recipe) |
| Accent on many competing elements | no visual hierarchy | one primary accent per region |
| Fixed pixel canvas at 100% | clips at `--app-font-scale` > 1 | fluid layout / scroll container |

## Contracts

- **CSS custom properties:** `--bg-color`, `--fg-color`, `--dim-color`, `--border-color`, `--accent-color`, `--status-{ok,warn,error,info}-color`, derived `--status-*-border` / `--status-*-bg`, `--ui-bg-{base,title,list,card}`, `--title-font-family`, `--app-font-family`, `--app-font-scale`.
- **Utility classes (`src/styles/ui.css`):** `.ui-surface-{base,title,list,card}`, `.ui-search-box`, `.ui-icon-btn`, `.ui-kbd`, `.ui-filter-pill`, `.ui-overlay-backdrop`, `.ui-modal-footer`, `.scrollbar-none`.
- **Theme source:** `get_active_theme` command; `src-tauri/src/theme.rs` and Omarchy `colors.toml` supply the variables.
- **Config:** user-configurable font family and font scale (`0.9`–`1.3`).

### Reference Map

| Path | Responsibility |
| :--- | :--- |
| `src/styles/ui.css` | Only scrollbar implementation, surface utilities, `.ui-search-box`, `.ui-icon-btn`, `.ui-kbd`, `.ui-filter-pill`, `.ui-overlay-backdrop`, `.ui-modal-footer` |
| `src-tauri/src/theme.rs` / Omarchy `colors.toml` | `--bg-color`, `--fg-color`, `--dim-color`, `--border-color`, `--accent-color`, `--status-*` |
| `src/components/ui/` | Standardized controls |
| `src/components/workspace/ProjectWorkspace.vue` | Workspace shell, tab bar, tab wrappers, panel mounting |
| `src/components/workspace/*` | Panel contracts and recipes |
| `src/components/taskbar/AgentTaskbar.vue` | Bottom taskbar geometry and agent controls |
| `src/components/AppHeader.vue` | Global header, Omnisearch, application menu |

Other specs consumed by panels: Scripts Hub (service orchestrator), Journal/tasks/technical-debt records (session lifecycle and devlog graph), Electronics panels (electronics workstation and instrumentation engine), CAD/PCB panels (CAD and PCB integration), Tasks/Forge panel (project management and OSS collaboration), Forge inbox/triage (GitHub inbox and triage flags), Forge provider handoff, UI surface nodes/navigation/unsaved guard ([UI State Machine](04-ui-state-machine-and-session-persistence.md)), Settings and overview panels ([Project Settings and Overview Dashboard](05-project-settings-and-overview-dashboard.md)).

## Implementation Status

Verified against the repository:

- `src/styles/ui.css` exists and defines `.ui-filter-pill`: **Implemented**.
- `src/components/ui/` contains the listed components (`AppTooltip`, `AppSelect`, `AppInput`, `AppTextarea`, `AppToggle`, `AppSegmentedToggle`, `AppEmptyState`, `SubtabBar`, `AppContextMenu`, `AppFileTree`, `AppInitBadge`, `AppCodeEditor`, `AppAgentPortrait`, `AppQuota*`, `AppExecutionConfigPicker`, `AppModelEffortSelect`, `QuotaGaugePreviewSelector`): **Implemented**.
- `src/utils/ansi.ts` and `src/utils/navigationGuard.ts` exist: **Implemented**.
- `src-tauri/src/theme.rs` exists: **Implemented**.
- Individual token values, per-component class strings and legacy palette migration debt were not audited line by line: **Partial / unverified**. Legacy palette occurrences are acknowledged migration debt.

## Related Specs

- [UI Component Library and Composition Recipes](02-ui-component-library-and-recipes.md)
- [Workspace Chrome and Panel Contracts](03-workspace-chrome-and-panel-contracts.md)
- [UI State Machine and Session Persistence](04-ui-state-machine-and-session-persistence.md)
- [Localization and Belta Language Strategy](06-localization-and-belta-language.md)

Supersedes legacy: `13_design_system_and_ui_components.md`
