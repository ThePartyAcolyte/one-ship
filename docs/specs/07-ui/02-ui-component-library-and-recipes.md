# UI Component Library and Composition Recipes

## Purpose

Defines the reusable controls in `src/components/ui/` and the normative composition recipes (panels, master-detail, toolbars, cards, lists, forms, modals, popovers, states, terminals, status mapping). These prevent inconsistent styling and ad-hoc CSS drift.

## Scope

In scope: the component library API and usage rules, component index, and composition recipes. Out of scope: tokens and scales ([Design System Foundations](01-design-system-foundations.md)) and per-panel contracts ([Workspace Chrome and Panel Contracts](03-workspace-chrome-and-panel-contracts.md)).

## Model / Design

### Core UI Component Library (`src/components/ui/`)

#### `AppTooltip` (`AppTooltip.vue`)
- **Strict rule:** Native HTML `title="..."` attributes are strictly prohibited for actionable UI controls. All hover explanations must use `AppTooltip`.
- **Styling:** Brutalist box with `bg-[var(--bg-color)]`, `border-[var(--border-color)]`, `font-mono`, `text-[10px]`, `shadow-2xl`, `z-[99999]` (tooltips must paint above modals, popovers and menus).

#### `AppSelect` (`AppSelect.vue`)
- **Strict rule:** Native `<select>` elements with default browser `<option>` styling are banned. Default browser options in Chromium/WebKit on Linux render with a jarring white background.
- **Guarantee:** `AppSelect` forces `bg-[var(--bg-color)] text-[var(--fg-color)]` on both the select trigger and all `<option>` children, complemented by a custom brutalist SVG chevron.
- Compact usage requires `class="[&>button]:h-6"` to pin the trigger height. The popover is fixed-positioned at `z-[99999]` and clamps to the viewport.

#### `AppInput` & `AppTextarea`
- Standardized inputs: `bg-[var(--bg-color)]/60`, `border-[var(--border-color)]`, `focus:border-[var(--accent-color)]`, monospace, default height 32 px. Inline bar usage requires `!h-6 !text-[10px]`; form usage keeps the default height unless the spacing/sizing scale says otherwise.

#### `AppToggle` (`AppToggle.vue`)
- Brutalist boolean switch for toggling flags (e.g. Safe Mode, Auto-Approve).

#### `AppFileTree` (`AppFileTree.vue`)
- Interactive project file explorer with collapsible folder nodes, checkboxes, and instantaneous text filtering.
- Maps checked items to an array of relative paths (`modelValue`) suitable for configuration bindings like `context_docs`.
- Skips vendor and build artifacts (`.git`, `node_modules`, `target`, `dist`).

#### `AppContextMenu` (`AppContextMenu.vue`)
- Standardized brutalist right-click context menu engine rendered via Vue `Teleport` to document body.
- Accepts an array of `ContextMenuItem` objects (`label`, `icon`, `action`, `danger`, `disabled`, `separator`).
- Features automatic viewport boundary clamping, closing on outside `pointerdown`, `Escape` key, or item activation.
- Strict brutalist aesthetics: `bg-[var(--bg-color)]`, `border-[var(--border-color)]`, monospace typography, and accent hover transitions.

#### `AppInitBadge` (`AppInitBadge.vue`)
Rendered in the Project Card header and Overview panel for pending initialization and operational flags (`project_initialization_flags`), using semantic CSS variables:

| Flag | Badge | Tokens |
| :--- | :--- | :--- |
| `docs` | `[! INIT: DOCS]` | `--status-warn-border` / `--status-warn-color` |
| `todos` | `[! INIT: TODOS]` | `--status-info-border` / `--status-info-color` |
| `tech_debt` | `[! INIT: TECH_DEBT]` | `--accent-color` |
| `profiler` | `[! INIT: NO PROFILER]` | `--status-error-border` / `--status-error-color` |
| `invalid_config` | `[! CONFIG]` | `--status-error-border` / `--status-error-color` |
| `removal` | `[! REMOVAL]` | `--status-error-border` / `--status-error-color` |

- **Interactive action:** Clicking a badge triggers an independent Phase 2 resolution session in the Chat Overlay for that specific flag (`docs` opens `DocstyleSelectionModal` first).
- **Micro-actions:** `RESOLVE` (initiates resolution session) and `✕` (dismisses the flag, except `invalid_config` which resolves automatically when `rula.toml` validates).
- **Tooltip enforcement:** Wrapped in `<AppTooltip>` detailing the detected stack gaps or validation errors.

#### `AppSegmentedToggle` (`AppSegmentedToggle.vue`)
- Strict mutually exclusive switch (`modelValue` + `items`) rendered as one bordered strip (`h-6`, `text-[9px]`, accent fill on the active option, quiet inactive options).
- Used when the selection is a mode rather than a view and must not consume a full tab row (e.g. `Tasks | Forge` in the Project panel). The `hidden` flag drops options whose prerequisites are absent; hidden options never render a leading divider.

#### `AppEmptyState` (`AppEmptyState.vue`)
- Shared empty state for workspace panels: flat `ui-surface-card` (`max-w-md`, `p-6`), accent icon at 32 px, `text-xs` uppercase title, optional `text-[11px]` dim description slot, and an actions slot rendered as `.ui-icon-btn` controls.
- Panels must use this component instead of re-declaring empty-state card geometry; only the icon, copy, and actions vary per tab.

### Component Index (`src/components/ui/index.ts`)

| Component | Use it for | Do not use it for |
| :--- | :--- | :--- |
| `AppTooltip` | Every hover/focus explanation of an icon or action | Body copy or permanent labels |
| `AppSelect` | Any enumerated choice, in bars (`compact`) or forms | Fewer than 3 options where a segmented toggle reads better |
| `AppInput` / `AppTextarea` | Text, number, search and multiline input | Rich text; use `AppCodeEditor` |
| `AppToggle` | Boolean settings (on/off) | Multi-option or mutually exclusive modes |
| `AppSegmentedToggle` | 2–3 mutually exclusive modes that must not consume a tab row | Lists of 4+ options |
| `AppEmptyState` | Full-width panel empty state | Narrow sidebar lists (use compact inline state) |
| `SubtabBar` | The single `h-9` panel bar: tabs, optional leading/center zones, right tools | Page titles or stacked navigation |
| `AppContextMenu` | Right-click actions on rows/cards | Primary actions or forms |
| `AppFileTree` | Project document selection with checkboxes | Generic nested lists |
| `AppInitBadge` | Initialization/flag badges on project cards and overview | Generic status chips |
| `AppCodeEditor` | Code/markdown editing surfaces | Single-line input |
| `AppAgentPortrait` | Agent identity avatars | Generic user images |
| `AppQuota*` (`Pie`, `Bar`, `Concentric`, `Gauge`) | Quota/telemetry visuals; gauge in toolbars uses `:size="14"` | Decorative charts unrelated to quotas |
| `AppExecutionConfigPicker`, `AppModelEffortSelect` | Runner/model/effort selection | Free-form text |
| `QuotaGaugePreviewSelector` | Settings preview of gauge styles | Runtime telemetry panels |

### Mandatory Design Rules (component-level)

**Minimalist action buttons (contextual symbols).**
- Buttons that append items to a contextual container or list (adding an agent persona, a service, a VCS preset) MUST display only the action symbol (e.g. `+` or an icon) without text.
- In a contextual container (e.g. agent pills) the function of `+` is self-evident. Text labels like `+ Persona` or `+ Add Agent` create horizontal clutter and waste viewport space.
- Explanatory descriptions are provided strictly via `AppTooltip`.

**Contextual availability.**
- System specialists must not be displayed if their underlying prerequisite is absent.
- Example: the `git` agent is strictly conditioned on `project.has_git == true`. If a project has no `.git` repository, the `git` agent is hidden, aligning with the VCS action button behavior.

**Right-click contextual menus.**
- Interactive card elements that map to project configuration entities (agent personas, scripts) must expose contextual management options via right-click (`@contextmenu.prevent`).
- Options are declarative and supplied by the host component, providing rapid workflows (Focus Session, Restart, Delete from `rula.toml`).

**Language conventions.**
- **English UI baseline:** all standard UI views, forms, dialogs, button labels and system messages are strictly in English.
- **Role of `i18n.ts`:** internationalization is reserved for the progressive Lang Belta dialect tier (see [Localization and Belta Language Strategy](06-localization-and-belta-language.md)), allowing optional immersion in Belter Creole terminology without introducing fragmented or partial translations into core developer workflows.

### Composition Recipes

These recipes are normative. Copy their structure before inventing a layout.

#### Full-tab panel skeleton

```
<div class="flex-1 min-h-0 h-full flex flex-col overflow-hidden">
  <SubtabBar … />                     <!-- one h-9 bar, optional -->
  <div class="flex-1 min-h-0 overflow-y-auto p-3">…</div>
</div>
```

- The workspace wrapper provides no padding; never render `<div class="… p-2"><Panel/></div>`.
- One bar at the top, one scroll container in the remaining space, no bottom footer.
- A panel that is the whole tab does not add a bordered card shell around itself.
- A panel with no tabs starts directly with its own `h-9 ui-surface-title` toolbar.

#### Master-detail

- Sidebar `w-60`–`w-80 shrink-0 border-r ui-surface-list`, own `h-9 ui-surface-title` header and independent scroll.
- Rows `text-[10px]` (`h-7`–`h-8`), selected row `border-[var(--accent-color)]/60 bg-[var(--accent-color)]/15 text-[var(--accent-color)]`, hover only changes border/text.
- The detail column is `flex-1 min-w-0` with its own header. It never repeats the selected category name; actions live in the header.
- Sidebar collapse is a `.ui-icon-btn` with `PanelLeftClose` / `PanelLeft` and a tooltip.

#### Panel bar with tabs, inline search and filters

- Use `SubtabBar` zones: `leading` (fixed mode switch), tabs (scrollable), `center` (`flex-1` inline search), `right` (actions behind a divider).
- Inline search inside `center` is borderless: accent `Search` icon + transparent `AppInput` + clear `X`.
- Filters are an `.ui-icon-btn`-style trigger at the end of the search with an accent dot when active; the menu is an anchored popover. Do not add a second filter row or chip strip.
- Counts are badges on the tabs, never a duplicate counter in the body.

#### Toolbars

- Container: `h-9 px-3 ui-surface-title border-b border-[var(--border-color)] flex items-center justify-between gap-2 shrink-0`.
- Left group `min-w-0 overflow-x-auto scrollbar-none`; right group `shrink-0`. No `flex-wrap`.
- Symbol actions: `.ui-icon-btn` (24 px) + `AppTooltip`; active state `!border-[var(--accent-color)] !text-[var(--accent-color)]` (optional `!bg-[var(--accent-color)]/15`).
- Text actions: `h-6 px-2 text-[9px] uppercase font-bold border` with accent hover.
- Selects: `compact` + `class="[&>button]:h-6"`; inline inputs `!h-6 !text-[10px]`.

#### Cards

- `border border-[var(--border-color)] ui-surface-card p-3` (dense: `p-2.5`). No shadow.
- Header row separated by `border-b border-[var(--border-color)] pb-2`: icon + `text-[10px] uppercase tracking-wider font-bold`, optional badge on the right.
- Body text `text-[10px]`/`text-[11px]`; metadata `text-[8px]`/`text-[9px] text-[var(--dim-color)]`.
- Actions align top-right at `h-6`; primary actions use `h-7` only in forms.
- Never nest more than one card level; use `ui-surface-list` inside cards for recessed content.

#### Lists and tables

- List container `ui-surface-list`; rows `text-[10px]`, `px-2 py-1`, `border-b border-[var(--border-color)]/40`, hover `hover:bg-white/5` or accent border.
- Table wrapper `border`; `thead` on `ui-surface-title` with `text-[9px] uppercase tracking-wider`; cells `px-2 py-1 text-[10px]`; numeric columns right-aligned `tabular-nums`.
- Wide tables live in `overflow-x-auto`; one scroll container.
- Empty table body renders a compact inline empty state, not a blank area.
- `v-for` keys are stable identifiers (`:key="item.id"`), never indexes.

#### Forms

- Label: `text-[9px] uppercase tracking-wider text-[var(--dim-color)]`.
- Controls: `h-6` in bars, default 32 px in forms; `compact` selects in bars.
- Actions right-aligned: Cancel `h-7 px-3 text-[9px] uppercase border`, primary `h-7 px-3 text-[9px] uppercase font-bold border border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)]`. Solid accent fill is reserved for the single highest-emphasis action.
- Validation: inline `text-[9px] text-[var(--status-error-color)]`; submission failures use a status-error banner. Never rely on placeholder text as a label.
- Dirty state registers the navigation guard (see the unsaved-change guard in [Workspace Chrome and Panel Contracts](03-workspace-chrome-and-panel-contracts.md)).

#### Modals

- Overlay: `.ui-overlay-backdrop` (never ad-hoc scrims or `backdrop-blur-*`).
- Container: `ui-surface-base border w-full max-w-lg max-h-[85vh] flex flex-col shadow-2xl`.
- Header and footer: `h-9`/`h-10` on `ui-surface-title`, title `text-[10px] uppercase tracking-wider font-bold`; close is `.ui-icon-btn`.
- Body scrolls (`p-3`–`p-4`); footer actions `h-7 px-3 text-[9px] uppercase`.
- One modal at a time. `Escape` closes. Focus the first meaningful control. Destructive confirmations name the object and the consequence.

#### Anchored popovers, menus and dropdowns

- Anchor wrapper `relative`; panel `absolute right-0 top-full mt-1 z-50 ui-surface-card border shadow-2xl` with explicit `w-*` and `max-h-* overflow-y-auto`.
- Close on outside `pointerdown`, `Escape`, tab change and selection. Keep the trigger inside the outside-click guard.
- Group headers `text-[8px] uppercase tracking-wider text-[var(--dim-color)]`; options `h-6 px-2 text-[9px] uppercase`, active `border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] font-bold`.
- No blur, no nested popovers, no viewport overflow.

#### Empty, loading and error states

- Main panel empty: `AppEmptyState` (icon 32, title, description slot, `.ui-icon-btn` actions).
- Narrow rail empty: centered inline block, icon 20 at `opacity-20`, title `text-[10px] uppercase tracking-widest`, detail `text-[9px]`.
- Loading: `Loader2` 11–14 px `animate-spin` plus `text-[9px]`/`text-[10px]`; disable the actions that caused the load and do not shift layout.
- Error: banner below the bar with `--status-error-border/bg/color`, the failed operation and the next step; dismiss `X`.
- Success: transient `--status-ok-*` banner, auto-dismiss or explicit close.

#### Terminal and log surfaces

- The terminal body uses the sanctioned `bg-[#09090b]`, `font-mono text-[10px]`/`text-[11px] leading-tight`, `text-[var(--fg-color)]`, dim metadata.
- ANSI parsing is centralized (`src/utils/ansi.ts`); never inject raw HTML from logs.
- Provide autoscroll, clear and copy controls in the header (`h-6` or `.ui-icon-btn`), and keep the stdin bar at the bottom.

#### Status, severity and telemetry

- Status pills: `px-1.5 py-0.5 border text-[8px] uppercase font-bold` with `--status-{ok,warn,error,info}-*` tokens.
- Severity mapping: critical → error; high → warn; medium → warn (lighter wash); low → dim.
- Priority mapping: P1 → error, P2 → warn, P3 → dim.
- Never encode meaning with color alone: pair the token with a label or icon.
- Thresholds come from configuration (`agentMetricsStore.contextThresholdPct`, quota configs), never new hardcoded numbers.

## Contracts

### Component props

| Component | Prop | Type / default |
| :--- | :--- | :--- |
| `AppTooltip` | `text` | `string?` — label content |
| `AppTooltip` | `position` | `'top' \| 'bottom' \| 'left' \| 'right'`, default `'bottom'` |
| `AppTooltip` | `disabled` | `boolean?` — suppresses tooltip rendering when true |
| `AppSelect` | `modelValue`, `options`, `compact?`, `placeholder`, `disabled` | `options: Array<{ label: string; value: string }>`; `compact` = 24 px trigger, `px-2 text-[10px]` |
| `AppSegmentedToggle` | `modelValue`, `items` (each with optional `hidden`) | — |
| `AppFileTree` | `modelValue` | array of relative paths |
| `AppContextMenu` | items | `ContextMenuItem { label, icon, action, danger, disabled, separator }` |

### Context menu and badge events
- `AppInitBadge` flags: `docs`, `todos`, `tech_debt`, `profiler`, `invalid_config`, `removal` (source: `project_initialization_flags`).
- Module `src/components/ui/index.ts` exports the library.

## Implementation Status

Verified: all listed components exist in `src/components/ui/` (including `AppDatePicker.vue`, `AppFileTreeNode.vue`, `DeepSeekIcon.vue`, `GeminiIcon.vue`, `quotaUtils.ts`, which are not described in the legacy source), plus `index.ts` and `SubtabBar.vue`; `src/utils/ansi.ts` exists. **Implemented** at component level. Per-class conformance of every consumer and the legacy palette migration were not audited: **Partial / unverified**.

## Related Specs

- [Design System Foundations](01-design-system-foundations.md)
- [Workspace Chrome and Panel Contracts](03-workspace-chrome-and-panel-contracts.md)
- [UI State Machine and Session Persistence](04-ui-state-machine-and-session-persistence.md)
- [Localization and Belta Language Strategy](06-localization-and-belta-language.md)

Supersedes legacy: `13_design_system_and_ui_components.md`
