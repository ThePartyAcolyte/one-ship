# Spec 13: Design System, UI Component Library & Visual Simplicity Standards
**Document Version:** 0.2.14  
**Domain:** Brutalist Design System, UI Component Library (`src/components/ui/`), Agent UI Implementation Guide, Tooltip Standardization & Minimalist Actions  
**Maintainer:** ONE-SHIP / Omarchy Development Team

---

## 0. Agent Design Contract (Mandatory)

This document is the single source of truth for every visual element. Any agent implementing or modifying UI MUST follow this contract before writing code.

### 0.1 Reading order

1. §0 (this contract) and §11 (checklist) — mandatory.
2. §1.3 (spacing, sizing, typography) and §1.2 (surfaces) — mandatory for any layout.
3. §2 (component library) and §3 (mandatory rules) — mandatory when choosing controls.
4. §4 (composition recipes) — mandatory for a new panel, toolbar, table, form, modal or dropdown.
5. The specific panel contract in §3.13 when editing a workspace tab.

### 0.2 Precedence (highest to lowest)

1. Active theme variables (`--bg-color`, `--fg-color`, `--dim-color`, `--border-color`, `--accent-color`, `--status-*`) resolved at runtime via `get_active_theme` (built-in dark/light presets, or live Omarchy `colors.toml` sync when `Auto (Omarchy)` is selected).
2. Four surface tokens and shared primitives in `src/styles/ui.css` (`.ui-search-box`, `.ui-icon-btn`, `.ui-kbd`, `.ui-modal-footer`, `.ui-overlay-backdrop`).
3. Standardized components in `src/components/ui/` (`AppTooltip`, `AppSelect`, `AppInput`, `AppTextarea`, `AppToggle`, `AppSegmentedToggle`, `AppEmptyState`, `SubtabBar`, …).
4. Panel contracts in §3.13 and the recipes in §4.
5. Local Tailwind composition (spacing/layout only). Local color, height, radius or typography tokens are prohibited.

### 0.3 Non-negotiables

- **NEVER** hardcode a Tailwind palette color (`zinc-*`, `emerald-*`, `amber-*`, `rose-*`, `cyan-*`, `text-white`, `bg-black/N`). Use semantic tokens or `.ui-surface-*`.
- **NEVER** render native `<select>`/`<option>` or native `title="..."` tooltips.
- **NEVER** duplicate a panel title inside the panel; the workspace tab label identifies the view.
- **NEVER** add an outer padding/bordered shell around a full-tab panel; panels are full-bleed.
- **NEVER** invent a new control when `src/components/ui/` already provides one; extend the component instead.
- **ALWAYS** express numeric units (time, size, counts) with explicit suffixes in code and configuration.
- **ALWAYS** propagate a visual contract change to code, the affected spec and the devlog in the same work cycle.

### 0.4 Definition of Done for visual work

A visual change is complete only when:

1. It uses only semantic tokens and standardized components.
2. It implements the full state matrix of §5 (hover, active, selected, focus, disabled where applicable).
3. It passes the §11 checklist and the targeted grep audit.
4. The affected spec (§3.13 panel contract, component section, or a new subsection) reflects the implemented truth.
5. A devlog entry records the change and any deliberate exception.

---

## 1. Design System & Theme Variables

ONE-SHIP implements an industrial, high-contrast, brutalist developer interface adapted dynamically to the Omarchy desktop environment. All styling is driven strictly by CSS custom properties:

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

Additionally, derived translucent borders and background washes are generated automatically via `color-mix(in srgb, var(--status-*-color) X%, transparent)` tokens (`--status-ok-border`, `--status-ok-bg`, `--status-warn-border`, `--status-warn-bg`, `--status-error-border`, `--status-error-bg`, `--status-info-border`, `--status-info-bg`) to maintain brutalist styling without hardcoded Tailwind palette classes.

### 1.1 Typography Hierarchy, Optical Normalization & Scaling
Typography in ONE-SHIP is strictly hierarchical, functional, and user-configurable:

| Token / CSS Property | Value / Source | Usage & Invariants |
| :--- | :--- | :--- |
| `--title-font-family` | `'Protest Guerrilla', sans-serif` | Reserved strictly and exclusively for the primary application title (`ONE-SHIP`). Never used as body or interface font. |
| `--app-font-family` | Configurable (`IBM Plex Mono`, `Anonymous Pro`, `Courier Prime`) | Monospace typography family applied globally to all UI labels, metrics, editors, and terminal elements (`.font-mono`). |
| `size-adjust` | `112%` (`Anonymous Pro`), `115%` (`Courier Prime`) | Optical normalization defined in `@font-face` rules to align baseline x-height and optical weight across alternative monospace fonts against the baseline `IBM Plex Mono`. |
| `--app-font-scale` | `0.9` to `1.3` (Default: `1.0` / 100%) | Global zoom scaling factor bound via `html { zoom: var(--app-font-scale, 1); }`, enabling smooth viewport and font scaling without breaking flexbox constraints. |

### 1.2 Surface Color Hierarchy (`src/styles/ui.css`)
To enforce consistent depth and visual hierarchy across modals, drawers, and panels, surfaces derive from `--bg-color` via four standardized semantic tokens and utility classes:

| Token | Utility Class | Derivation | Usage |
| :--- | :--- | :--- | :--- |
| `--ui-bg-base` | `.ui-surface-base` | `var(--bg-color)` | Base container background for modals, drawers, and workspace panels. |
| `--ui-bg-title` | `.ui-surface-title` | `color-mix(in srgb, var(--bg-color) 78%, #000)` | Header bars, title bars, search bars, and modal footers. |
| `--ui-bg-list` | `.ui-surface-list` | `color-mix(in srgb, var(--bg-color) 62%, #000)` | Recessed scrollable list/log/navigation areas. |
| `--ui-bg-card` | `.ui-surface-card` | `color-mix(in srgb, var(--bg-color) 90%, var(--fg-color) 5%)` | Inner cards, items, and grouped rows inside lists or base containers. |

**Single Source of Truth (Strict):** Only the four `--ui-bg-*` tokens above are valid surface tokens. The legacy tokens `--surface-color`, `--card-bg`, `--bg-card`, `--text-color`, `--text-primary`, `--bg-primary`, and `--status-warning-*` are undefined and prohibited. A `var()` reference to an undefined token invalidates the declaration at computed-value time, silently degrading backgrounds to `transparent` (this is what made the bottom agent taskbar and multiple popovers appear translucent). New surfaces must use `--ui-bg-*`, `--fg-color`, and `--status-warn-*` directly.

### 1.3 Spacing, Sizing & Typography Scale

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

**Opacity and washes.** Accent washes are `/10`, `/15`, `/25`; semantic status washes use `--status-*-bg`; borders use `/40` or `/60`; neutral hover may use `hover:bg-white/5`. Solid white/black surfaces are prohibited. Sanctioned exception: the terminal/log body uses `bg-[#09090b]` (§4.11).

---

## 2. Core UI Component Library (`src/components/ui/`)

To prevent inconsistent styling and ad-hoc CSS drift, all common UI controls are encapsulated within reusable components:

### 2.1 `AppTooltip` (`AppTooltip.vue`)
- **Strict Rule:** Native HTML `title="..."` attributes are strictly prohibited for actionable UI controls. All hover explanations must use `AppTooltip`.
- **Styling:** Brutalist box with `bg-[var(--bg-color)]`, `border-[var(--border-color)]`, `font-mono`, `text-[10px]`, `shadow-2xl`, `z-[99999]` (tooltips must paint above modals, popovers and menus).
- **Props:**
  - `text?: string`: Tooltip label content.
  - `position?: 'top' | 'bottom' | 'left' | 'right'` (default: `'bottom'`).
  - `disabled?: boolean`: Suppresses tooltip rendering when true.

### 2.2 `AppSelect` (`AppSelect.vue`)
- **Strict Rule:** Native `<select>` elements with default browser `<option>` styling are banned. Default browser options in Chromium/WebKit on Linux render with a jarring white background.
- **Guarantee:** `AppSelect` forces `bg-[var(--bg-color)] text-[var(--fg-color)]` on both the select trigger and all `<option>` children, complemented by a custom brutalist SVG chevron.
- **Props:** `modelValue`, `options: Array<{ label: string; value: string }>`, `compact?: boolean` (24 px trigger, `px-2 text-[10px]`), `placeholder`, `disabled`. Compact usage requires `class="[&>button]:h-6"` to pin the trigger height. The popover is fixed-positioned at `z-[99999]` and clamps to the viewport.

### 2.3 `AppInput` & `AppTextarea`
- Standardized inputs: `bg-[var(--bg-color)]/60`, `border-[var(--border-color)]`, `focus:border-[var(--accent-color)]`, monospace, default height 32 px. Inline bar usage requires `!h-6 !text-[10px]`; form usage keeps the default height unless §1.3 says otherwise.

### 2.4 `AppToggle` (`AppToggle.vue`)
- Brutalist boolean switch for toggling flags (e.g. Safe Mode, Auto-Approve).

### 2.5 `AppFileTree` (`AppFileTree.vue`)
- Interactive project file explorer with collapsible folder nodes, checkboxes, and instantaneous text filtering.
- Maps checked items to an array of relative paths (`modelValue`) suitable for configuration bindings like `context_docs`.
- Skips vendor and build artifacts (`.git`, `node_modules`, `target`, `dist`).

### 2.6 `AppContextMenu` (`AppContextMenu.vue`)
- Standardized brutalist right-click context menu engine rendered via Vue `Teleport` to document body.
- Accepts an array of `ContextMenuItem` objects (`label`, `icon`, `action`, `danger`, `disabled`, `separator`).
- Features automatic viewport boundary clamping, closing on outside `pointerdown`, `Escape` key, or item activation.
- Strict brutalist aesthetics: `bg-[var(--bg-color)]`, `border-[var(--border-color)]`, monospace typography, and accent hover transitions.

### 2.7 `AppInitBadge` (`AppInitBadge.vue`)
- Standardized brutalist badge component rendered in the Project Card header and Overview panel for pending initialization and operational flags (`project_initialization_flags`), using semantic CSS variables:
  - `docs`: `[! INIT: DOCS]` (`--status-warn-border` / `--status-warn-color`).
  - `todos`: `[! INIT: TODOS]` (`--status-info-border` / `--status-info-color`).
  - `tech_debt`: `[! INIT: TECH_DEBT]` (`--accent-color`).
  - `profiler`: `[! INIT: NO PROFILER]` (`--status-error-border` / `--status-error-color`).
  - `invalid_config`: `[! CONFIG]` (`--status-error-border` / `--status-error-color`).
  - `removal`: `[! REMOVAL]` (`--status-error-border` / `--status-error-color`).
- **Interactive Action:** Clicking a badge triggers an independent Phase 2 resolution session in the Chat Overlay for that specific flag (`docs` opens `DocstyleSelectionModal` first).
- **Micro-Actions:** Contains concise action controls: `RESOLVE` (initiates resolution session) and `✕` (dismisses the flag, except `invalid_config` which resolves automatically when `rula.toml` validates).
- **Tooltip Enforcement:** Wrapped in `<AppTooltip>` detailing the detected stack gaps or validation errors.

### 2.8 `AppSegmentedToggle` (`AppSegmentedToggle.vue`)
- Strict mutually exclusive switch (`modelValue` + `items`) rendered as one bordered strip (`h-6`, `text-[9px]`, accent fill on the active option, quiet inactive options).
- Used when the selection is a mode rather than a view and must not consume a full tab row (e.g. `Tasks | Forge` in the Project panel). The `hidden` flag drops options whose prerequisites are absent; hidden options never render a leading divider.

### 2.9 `AppEmptyState` (`AppEmptyState.vue`)
- Shared empty state for workspace panels: flat `ui-surface-card` (`max-w-md`, `p-6`), accent icon at 32 px, `text-xs` uppercase title, optional `text-[11px]` dim description slot, and an actions slot rendered as `.ui-icon-btn` controls.
- Panels must use this component instead of re-declaring empty-state card geometry; only the icon, copy, and actions vary per tab.

### 2.10 Component Index (`src/components/ui/index.ts`)

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

---

## 3. Mandatory Design Rules

### 3.1 Minimalist Action Buttons (Contextual Symbols)
- Buttons that append items to a contextual container or list (such as adding an agent persona, adding a service, or adding a VCS preset) **MUST only display the action symbol (e.g., `+` or an icon) without text**.
- In a contextual container (e.g. agent pills), the function of `+` is self-evident. Text labels like `+ Persona` or `+ Add Agent` create horizontal clutter and waste valuable viewport space.
- Explanatory descriptions must be provided **strictly via `AppTooltip`**.

### 3.2 Contextual Availability (e.g. Git Agent)
- System specialists must not be displayed if their underlying prerequisite is absent.
- Example: The `git` agent is strictly conditioned on `project.has_git == true`. If a project does not have a `.git` repository, the `git` agent is hidden, aligning with the VCS action button behavior.

### 3.3 Right-Click Contextual Menus
- Interactive card elements that map to project configuration entities (such as agent personas and scripts) must expose contextual management options via right-click (`@contextmenu.prevent`).
- Options must be declarative and supplied by the host component, providing rapid workflows (Focus Session, Restart, Delete from `rula.toml`).

### 3.4 Language Conventions & Thematic Dialect Tier
- **English UI Baseline:** In compliance with `AGENTS.md` §1.2, all standard UI views, forms, dialogs, button labels, and system messages are strictly in English.
- **Role of `i18n.ts`:** Internationalization is explicitly reserved for the progressive Lang Belta dialect tier system (Spec 07: `07_i18n_translation_strategy.md`), allowing optional immersion into Belter Creole terminology without introducing fragmented or partial translations into the core developer workflows.

### 3.5 Toolbar & Taskbar Component Sizing Invariants (`h-6` Baseline)
- **Strict Uniform Height:** All primary interactive elements in the application header toolbar (`AppHeader.vue`) and bottom taskbar (`AgentTaskbar.vue` / `AccountQuotaControls.vue`) adhere to a uniform `h-6` (24px) structural height.
- **Account & Quota Controls (`AccountQuotaControls.vue` in `AgentTaskbar.vue`):** The Google Account Switcher and DeepSeek Wallet trigger buttons are mounted on the right side of the bottom taskbar (`AgentTaskbar.vue`) at `h-6 flex items-center gap-1.5 px-2 shrink-0 select-none`.
- **Zero Extraneous Container Padding:** Nested child containers (such as the telemetry gauge wrapper in the Google switcher) are forbidden from introducing independent vertical padding (`py-0.5`) that disrupts this baseline.
- **Telemetry Gauge Sizing:** In-toolbar quota telemetry gauges (`AppQuotaGauge`) utilize `:size="14"` to mirror the 14px provider icons (`GeminiIcon`, `DeepSeekIcon`), preserving clean vertical centering and sub-pixel symmetry.
- **Header Action Bar (`AppHeader.vue`):** Contains the inline Omnisearch input (`h-6`, `.ui-search-box` with `Ctrl+K` badge), Voice Controller pill (`h-6`), conditional Daemon Error badge, and a single Hamburger Application Menu (`Menu` icon, `.ui-icon-btn`) housing System Health status, a Global Voice Controller `AppToggle`, `System Logs`, `Settings`, and `About ONE-SHIP`. The standalone Diagnostics button is deprecated.

### 3.6 Workspace Navigation, 14-Tab Architecture & Live Preview (`ProjectWorkspace.vue`)
- **Standardized Function-Driven Naming:** Workspace sub-tab titles are short, unambiguous, and function-driven across the 14 workspace tabs (`Overview`, `Project`, `Explorer`, `Electronics`, `Design`, `CAD`, `PCB`, `Preview`, `Profiling`, `RE Lab`, `Scripts`, `Git & Snapshots`, `Journal`, `Skills`). Descriptive parentheticals (e.g. `(Workspace Root)`) or compound labels are strictly prohibited in the tab bar.
- **Live Web Preview Tab (`Preview`, `ProjectPreviewTab.vue`):** Enabled when `[preview] url = "http://..."` is declared in `rula.toml` (`project.preview_url`). Provides an embedded responsive iframe viewport with preset device widths (`Desktop 100%`, `Tablet 768px`, `Mobile 390px`), fullscreen toggle, reload, and external browser launch. Fullscreen takes the whole window: the project rail and the agent bar are hidden while active, the overlay paints above workspace chrome and the floating chat and below modals (`z-[45]`), and `Escape` exits.
- **Tab Styling Homogenization:** All workspace tabs adhere to a single unified brutalist visual convention (`border-[var(--accent-color)] bg-[var(--bg-color)] text-[var(--accent-color)]` when active, `border-transparent text-[var(--dim-color)] hover:border-[var(--border-color)]` when inactive).
- **Tab Bar Geometry:** The tab bar is a `h-9` row (`ui-surface-title`, `px-3`, `py-0`) and every tab control is exactly `h-6`: text tabs use `px-2.5`, the fixed gear settings tab uses `w-6 px-0`, and the overflow button uses `w-6 px-0`. This removes the historical 4px height mismatch between the gear tab and text tabs.
- **Zero Path Clutter in Tab Bars:** Never render filesystem paths or URLs inside workspace tab bars. Directory locations belong exclusively in tooltips or dedicated header inspectors.

### 3.7 Unified Visual Language Classes (`src/styles/ui.css`, Omnisearch-Derived)
All search-driven modals, drawers, and overlays share a single set of reusable brutalist classes defined in `src/styles/ui.css` (imported globally in `src/main.ts`), modeled after `OmnisearchModal.vue`:
- `.ui-search-box` & `.ui-search-box__icon`: Standard search bar container (`border-[var(--border-color)]`, `bg-[var(--ui-bg-title)]`, `focus-within:border-[var(--accent-color)]`) with an accent-colored `<Search>` icon.
- `.ui-kbd`: Compact keyboard shortcut badge (`9px` monospace, `border-[var(--border-color)]`, `bg-black/40`).
- `.ui-filter-pill` / `.ui-filter-pill--active`: Compact uppercase filter toggle pills (`9px` bold) replacing oversized dropdowns where options are finite (e.g., Omnisearch scopes, Log levels).
- `.ui-icon-btn` / `.ui-icon-btn--danger`: Square `24px` icon-only action buttons (e.g., Clear Logs `<Trash2>` without text), always paired with `<AppTooltip>`.
- `.ui-overlay-backdrop`: Standardized full-surface modal overlay (`rgba(0, 0, 0, 0.45)` scrim + `12px` backdrop blur). Every modal overlay in the application MUST use this class instead of ad-hoc `bg-black/N` scrims. See §3.9 for the WebKit backdrop-filter invariants.
- `.ui-modal-footer`: Standardized modal footer bar (`ui-surface-title`, `9px` dim text) displaying keyboard hints and item counts. Text actions placed in a modal footer use `h-6 px-2.5 text-[9px] font-bold uppercase tracking-wider`; icon-only actions use `.ui-icon-btn`.
- **Drawer Elevation & Stacking (`ProjectVerticalRail.vue`):** The project drawer backdrop teleports to `body` at `z-40` (dimming the main workspace and bottom `AgentTaskbar` at `z-30`), while the left vertical project rail and the full-height drawer panel (`top-0 bottom-0`, `border-t border-b border-r border-[var(--border-color)]`) sit at `z-50` above the backdrop.

### 3.8 Standardized Scrollbar Contract
- **Geometry:** Constant `6px` width/height, strictly square (`border-radius: 0px`), no directional arrow buttons. Width must not change on hover/active.
- **Colors:** Track and corner are fully transparent at rest; on hover/active the track takes a translucent `var(--accent-color)` tint. The draggable thumb is a theme-aware dim tone at rest (never a fixed near-black) and switches to solid `var(--accent-color)` on hover/active.
- **Single Source of Truth:** `src/styles/ui.css` defines the only scrollbar styling in the application through the global `::-webkit-scrollbar*` pseudo-elements. No component may redefine scrollbar geometry, track, or thumb.
- **Mandatory Implementation Rule (WebKit):** WebKit only instantiates the custom `RenderScrollbar` when the element's computed `scrollbar-width` is `auto` **and** `scrollbar-color` is `auto` (`ComputedStyle::usesLegacyScrollbarStyle()` in `StyleComputedStyle+GettersInlines.h`). Any other value makes WebKit ignore every `::-webkit-scrollbar` rule and fall back to the native Adwaita scrollbar.
  - Therefore, Tailwind standard scrollbar utilities (`scrollbar-thin`, `scrollbar-thumb-*`, `scrollbar-track-*`) are **prohibited** on scroll containers. Tailwind v4 ships `scrollbar-thin` as `scrollbar-width: thin`, which silently disables the custom scrollbar.
  - `scrollbar-color` is an inherited property, so a standard-property utility on an ancestor would break every nested scroll container in its subtree.
  - Intentionally invisible scrollbars use `.scrollbar-none` (`scrollbar-width: none` + `::-webkit-scrollbar { display: none }`). This is the only allowed standard-property usage.

### 3.9 Overlay & Backdrop Blur Contract (WebKitGTK)
All modal windows (full-screen dialogs, workspace dialogs, chat-panel dialogs, and blocking card overlays) share one backdrop contract defined by `.ui-overlay-backdrop` in `src/styles/ui.css`:
- **Literal filter values only:** `backdrop-filter` MUST be declared with literal values (`blur(12px)`). WebKit bug #289800 ignores custom properties and `calc()` inside `backdrop-filter`, which makes every Tailwind v4 `backdrop-blur-*` utility (generated from `var(--tw-backdrop-blur,)`) a silent no-op on `webkit2gtk-4.1`.
- **Accelerated compositing is mandatory:** `backdrop-filter` is applied by the compositor and is silently ignored by WebKitGTK's non-accelerated software path. `WEBKIT_DISABLE_DMABUF_RENDERER=1` (and `WEBKIT_DISABLE_COMPOSITING_MODE=1`) empty the renderer transport set, making `AcceleratedBackingStore::canUseHardwareAcceleration()` return false in WebKitGTK 2.54.1. The development launcher (`scripts/run_dev.sh`) must never set them. If the DMA-BUF path must be avoided, use `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1`, which keeps accelerated compositing over shared-memory transport. Packaging resolution, the `auto` ladder, and the restart semantics are defined in Spec 12 §7.4.
- **Bounded blurred layers:** Blurred backdrop layers are limited to one per open modal (expected nesting of at most two) plus at most one blocking overlay per initializing project card. Never mount a blurred backdrop per row, per HUD chip, or per repeated list item. WebKitGTK 2.54 leaks memory proportionally to the number of simultaneous `backdrop-filter` layers (WebKit bug #328065), which stays stable up to roughly 10-14 layers and degrades sharply beyond ~18.
- **Zero paint filters on the application tree:** Applying `filter: blur(...)` to a container that holds live application content is strictly prohibited. `filter` is a paint filter that re-rasterizes the entire subtree on every invalidation (including modal scrolling) and creates a containing block that breaks `position: fixed` descendants. Backdrop blur belongs exclusively to the overlay leaf that sits above the content.
- **Non-opaque scrim:** The scrim MUST remain semi-transparent (`alpha <= 0.55`) so the backdrop blur is perceptible; opaque scrims negate the effect.
- **Prohibited utilities:** Tailwind `backdrop-blur-*`, `backdrop-brightness-*`, `backdrop-saturate-*`, and related utilities are prohibited app-wide until WebKit bug #289800 is fixed; decorative surfaces use flat semi-transparent backgrounds instead.

### 3.10 Application Shell & Workspace Chrome Geometry
- **Two-column shell:** Below `AppHeader`, the application is one flex row: the left project rail (`ProjectVerticalRail`, `w-11`), the workspace stage (tab bar + active content + agent taskbar) and the optional docked chat column.
- **Full-height rail:** The project rail spans from the first row below the header to the bottom edge of the main viewport. Its drawer overlays the full height. It never stops at the tab bar.
- **Workspace stage:** The stage column contains, in order: the tab bar (`h-9`), the active tab content (`flex-1 min-h-0`) and `AgentTaskbar` (`h-9 shrink-0`). The agent taskbar therefore starts exactly where the tab bar starts (the rail's right edge) and ends at the workspace's right border.
- **Docked chat:** The docked chat column and its resizer are outside the workspace border and share the main viewport's top and bottom edges. The main viewport has no bottom padding; the chat reaches the bottom edge. The agent taskbar is never rendered under the rail or under the docked chat.
- **Uniform chrome heights:** The tab bar and the agent taskbar are both `h-9` with `px-3`; their interactive controls are `h-6` (24px). The rail's top action buttons are `w-8 h-8` (32px) and share the same square geometry.

### 3.11 Settings Modal Contract (`SettingsModal.vue`)
The settings modal follows the Omnisearch/System Logs visual language exactly:
- **Container:** `ui-surface-base`, `border`, `max-w-4xl`, `h-[85vh]`; no ad-hoc `bg-black/*` panels.
- **Header:** a single `.ui-search-box` row (`!border-x-0 !border-t-0 px-4 py-2.5`) with the `Settings` icon, title, scope badge (`config.toml` / `rula.toml`), a vertical divider, the accent `Search` icon, the search input, the match counter, the clear button, the `ESC` `.ui-kbd` and the close icon. `Escape` closes the modal from the capture phase.
- **Body:** left sidebar `w-52` with `ui-surface-list`; category buttons `h-7 px-2.5 text-[10px]` with a 2px accent left border. The content column uses `p-4`, `gap-3`, `ui-surface-base`; each parameter is a `ui-surface-card p-3` block with an 11px label, 10px description and 9px default hint.
- **Footer:** `.ui-modal-footer` with the resolved path on the left and `Cancel` / `Save` actions on the right, both `h-6 px-2.5 text-[9px]`.
- **Form controls:** inputs and selects keep the 32px height; the inline `Test` voice action is `h-8` and uses only valid semantic tokens.

### 3.12 Anchored Popover Surface Contract
- **Opaque elevated surface:** Anchored popovers (agent launcher, account/quota switchers, header application menu, log selection dropdown) declare a solid surface via `ui-surface-title` (or `--ui-bg-title`) over `--bg-color`; they must never rely on an undefined token or on `--bg-color` alone, which blends into the workspace and reads as transparent.
- **Stacking:** A popover uses `z-50` inside a positioned anchor and must be painted above the content it overlaps. Popovers that open upward from the workspace stage live inside that stage's stacking context.
- **Bounds:** Every popover declares a bounded width; growing lists inside it use `overflow-y-auto` with an explicit `max-h`; upward popovers may not exceed the stage width. No blurred backdrop is mounted per popover.
- **No `backdrop-filter`:** Popover surfaces are flat and opaque; the blur contract remains exclusive to `.ui-overlay-backdrop`.

### 3.13 Workspace Panel Contract (No Redundant Chrome)
- **No duplicated title:** A workspace tab whose tab label or tooltip already identifies the view must not render the same title inside the panel header. Panels configured as icon-only tabs (e.g. the gear `settings` tab) rely on the tab tooltip; they do not repeat a heading.
- **Single top toolbar:** Panel-level search, status and primary actions share one `h-9` `.ui-search-box` row at the top. Panel action buttons use `h-6 px-2.5 text-[9px]`. A bottom action footer is reserved for modals, not for full-tab panels.
- **Full-bleed geometry:** The tab content wrapper provides no outer padding; the panel fills the full available width/height and owns its internal spacing. Sidebars extend to the bottom edge and keep their surface below short lists; the content column scrolls in the remaining space.
- **No floating card shell:** A panel that is the entire tab content uses flat surfaces (`ui-surface-base` / `ui-surface-list` / `ui-surface-card`); it does not add an extra bordered card inside the tab.
- **No redundant category headers:** In master-detail panels, the active category name and enabled/disabled state live only in the sidebar. The detail pane may keep a title-less quick-action row when the category exposes actions.
- **Panel bar (`SubtabBar.vue`):** One `h-9` row on `ui-surface-title` split into zones. The optional leading zone holds a fixed control that must never scroll away (e.g. an `AppSegmentedToggle` mode switch) behind a divider. The tabs zone holds the panel tabs (`h-6 px-2.5`, `text-[9px]`, accent border/fill for the active tab, transparent border for the rest) and scrolls only when they cannot fit. The optional center zone (`flex-1`) hosts an inline control such as a borderless search input plus a dropdown trigger, without its own border or surface. The right zone holds caller tools (search, filters, actions) behind a left divider and is omitted entirely when the `right` slot is empty, so tabs-only bars keep the same geometry. Tool content never wraps; the right zone scrolls horizontally when needed. Mode + views must share this single row instead of stacking a dedicated mode bar above the view bar.
- **Files tab header (`ProjectWorkspace.vue`):** Two rows on `ui-surface-title` with no panel title and no current-path text. Row one holds the branch as plain clickable text (`GitBranchSelector`, no boxed control) and the collapse action; row two holds the `.ui-search-box` input with the filter trigger to its right. Filter choices use `AppToggle`. `F5` refreshes the tree while the Files tab is active and no dedicated refresh button is rendered. Reader breadcrumbs show the open document when one exists and otherwise the explored folder path, ending in the current folder.
- **Design/CAD/PCB toolbars (`ProjectDesignTab.vue`, `ProjectCadTab.vue`, `ProjectPcbTab.vue`):** One `h-9` row on `ui-surface-title` with a bottom border and no wrapping. The left group scrolls horizontally when the window is narrow; the right group stays fixed. Symbol actions use `.ui-icon-btn` (`h-6 w-6`) and toggle their accent state when active; text actions use `h-6 px-2 text-[9px] uppercase`; selects and inputs are `h-6` (`compact` selects). Sidebars use `ui-surface-list` surfaces, `text-[9px]` uppercase section labels, and `text-[10px]` rows with an accent border/fill for the selected item. Missing-file panels render `AppEmptyState` with the same card, title scale, and `.ui-icon-btn` actions.
- **RE Lab and Profiling bars (`ProjectReaTab.vue`, `ProjectProfilerTab.vue`):** Same `h-9 ui-surface-title` contract. No duplicate panel title (RE Lab drops its internal `RE Lab` heading); engine status is a compact `h-6` chip (`SLEEPING` or `ENGINE :port · PID`), and toolchain access is a `Wrench` `.ui-icon-btn` that opens the inspection/installation modal. Profiling exposes its existing comparison view through a labeled `Compare`/`Runs` button instead of an icon-only toggle.
- **Scripts Hub (`ProjectTerminalGrid.vue`):** The rail header is a title-less `ui-surface-title` block: `h-9` row holding only `History` and `+ Shell`, then a full-width `.ui-search-box` filter row. The console header uses the same `h-9` surface with `h-6` `ALL`/`AUTO`/`SIGINT` actions and `.ui-icon-btn` clear/copy controls; the terminal body keeps its own dark surface.
- **Electronics Firmware subtab (`ProjectElectronicsTab.vue`):** Three stacked cards on `ui-surface-card` — `Hardware` (detected port, baud, board catalog with USB suggestion), `Firmware Project` (language `C++`/`C`/`Python`, framework, monitor speed, scaffold), and `Toolchain & Operations` (toolchain state, environment, compile/upload, logs). The toolchain modal uses semantic status tokens; `mpremote` is copy-only while pacman tools install inline.
- **Journal panel (`DevlogJournalView.vue`):** One `SubtabBar` for `Devlogs | Tasks | Tech Debt` with record counts as badges and the add/refresh actions in the right slot. The center slot holds a borderless inline search (remaining width) plus a `Filter` icon button that opens an anchored dropdown of contextual options (Agent; Status/Agent/Priority; Status/Severity). There is no second row and no chip strip.
- **Git & Snapshots panel (`ProjectGitSnapshotsTab.vue`):** Full-bleed dual-panel layout with no redundant top cockpit bar or manual snapshot creator. Left `w-80 ui-surface-list` timeline stream (`All | Snaps | Commits` + optional domain filter, `F5` refreshes). Right inspector opens with an `h-9 ui-surface-title` bar holding `GitBranchSelector`, active item metadata (`WORKING TREE`, snapshot, or commit), and right-aligned actions (`rula.toml` quick-action buttons, `+` action creator, `SYNC`, and `OPEN AGENT` in Working Tree mode; `RESTORE SELECTED` / `FULL ROLLBACK` in Snapshot mode). Below it, a resizable file/staging pane (`220px–560px` via a `w-1 cursor-col-resize` divider, default `288px`; commit message `AppTextarea` with AI-draft `Sparkles` trigger and `COMMIT`, plus `Staged Changes` and `Changes` with per-file and bulk `+`/`-` staging, two-line filename + relative path rows, `overflow-x-hidden`, and a `MAX_RENDERED_FILES_COUNT = 250` render cap) sits beside the live file diff pane. No global unified diff is loaded when `selectedFile` is `null` (`AppEmptyState`), and Design/PCB/CAD domain assets (`is_domain_asset`) remain tracked/stageable in Git while suppressing diff preview (`AppEmptyState`).
- **Skills panel (`ProjectSkillsTab.vue`):** One `SubtabBar` for `Installed | Explore` with `Installed` count badge, `#center` borderless inline search (live filter in `Installed`, unified SkillsMP search + automatic `http(s)://` URL detection with inline skill-name field and `Install` action in `Explore`), and `#right` dual view mode `.ui-icon-btn` toggles (`grid` default / `list`), refresh `.ui-icon-btn`, and active skill/token telemetry. Full-bleed `p-3` body with compact square cards in Grid mode (`min-h-[168px]`, `ui-surface-card`) and horizontal rows in List mode across both `Installed` and `Explore`, `AppEmptyState` for empty states, and an `h-9 ui-surface-title` split inspector for `SKILL.md`.
- **Unsaved-change guard:** Panels with editable state register the UI navigation guard while dirty (`src/utils/navigationGuard.ts`, consumed by `setWorkspaceTab` / `setActiveProject`). Leaving the tab or switching project opens a confirmation modal with `Save & Continue`, `Discard & Continue` and `Cancel`. Watcher-driven disk changes must never overwrite dirty edits; they surface a warning banner instead.

---

## 4. Composition Recipes

These recipes are normative. Copy their structure before inventing a layout.

### 4.1 Full-tab panel skeleton

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

### 4.2 Master-detail

- Sidebar `w-60`–`w-80 shrink-0 border-r ui-surface-list`, own `h-9 ui-surface-title` header and independent scroll.
- Rows `text-[10px]` (`h-7`–`h-8`), selected row `border-[var(--accent-color)]/60 bg-[var(--accent-color)]/15 text-[var(--accent-color)]`, hover only changes border/text.
- The detail column is `flex-1 min-w-0` with its own header. It never repeats the selected category name; actions live in the header.
- Sidebar collapse is a `.ui-icon-btn` with `PanelLeftClose` / `PanelLeft` and a tooltip.

### 4.3 Panel bar with tabs, inline search and filters

- Use `SubtabBar` zones: `leading` (fixed mode switch), tabs (scrollable), `center` (`flex-1` inline search), `right` (actions behind a divider).
- Inline search inside `center` is borderless: accent `Search` icon + transparent `AppInput` + clear `X`.
- Filters are an `.ui-icon-btn`-style trigger at the end of the search with an accent dot when active; the menu is an anchored popover (§4.9). Do not add a second filter row or chip strip.
- Counts are badges on the tabs, never a duplicate counter in the body.

### 4.4 Toolbars

- Container: `h-9 px-3 ui-surface-title border-b border-[var(--border-color)] flex items-center justify-between gap-2 shrink-0`.
- Left group `min-w-0 overflow-x-auto scrollbar-none`; right group `shrink-0`. No `flex-wrap`.
- Symbol actions: `.ui-icon-btn` (24 px) + `AppTooltip`; active state `!border-[var(--accent-color)] !text-[var(--accent-color)]` (optional `!bg-[var(--accent-color)]/15`).
- Text actions: `h-6 px-2 text-[9px] uppercase font-bold border` with accent hover.
- Selects: `compact` + `class="[&>button]:h-6"`; inline inputs `!h-6 !text-[10px]`.

### 4.5 Cards

- `border border-[var(--border-color)] ui-surface-card p-3` (dense: `p-2.5`). No shadow.
- Header row separated by `border-b border-[var(--border-color)] pb-2`: icon + `text-[10px] uppercase tracking-wider font-bold`, optional badge on the right.
- Body text `text-[10px]`/`text-[11px]`; metadata `text-[8px]`/`text-[9px] text-[var(--dim-color)]`.
- Actions align top-right at `h-6`; primary actions use `h-7` only in forms.
- Never nest more than one card level; use `ui-surface-list` inside cards for recessed content.

### 4.6 Lists and tables

- List container `ui-surface-list`; rows `text-[10px]`, `px-2 py-1`, `border-b border-[var(--border-color)]/40`, hover `hover:bg-white/5` or accent border.
- Table wrapper `border`; `thead` on `ui-surface-title` with `text-[9px] uppercase tracking-wider`; cells `px-2 py-1 text-[10px]`; numeric columns right-aligned `tabular-nums`.
- Wide tables live in `overflow-x-auto`; one scroll container.
- Empty table body renders a compact inline empty state, not a blank area.
- `v-for` keys are stable identifiers (`:key="item.id"`), never indexes.

### 4.7 Forms

- Label: `text-[9px] uppercase tracking-wider text-[var(--dim-color)]`.
- Controls: `h-6` in bars, default 32 px in forms; `compact` selects in bars.
- Actions right-aligned: Cancel `h-7 px-3 text-[9px] uppercase border`, primary `h-7 px-3 text-[9px] uppercase font-bold border border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)]`. Solid accent fill is reserved for the single highest-emphasis action.
- Validation: inline `text-[9px] text-[var(--status-error-color)]`; submission failures use a status-error banner. Never rely on placeholder text as a label.
- Dirty state registers the navigation guard (§3.13).

### 4.8 Modals

- Overlay: `.ui-overlay-backdrop` (never ad-hoc scrims or `backdrop-blur-*`).
- Container: `ui-surface-base border w-full max-w-lg max-h-[85vh] flex flex-col shadow-2xl`.
- Header and footer: `h-9`/`h-10` on `ui-surface-title`, title `text-[10px] uppercase tracking-wider font-bold`; close is `.ui-icon-btn`.
- Body scrolls (`p-3`–`p-4`); footer actions `h-7 px-3 text-[9px] uppercase`.
- One modal at a time. `Escape` closes. Focus the first meaningful control. Destructive confirmations name the object and the consequence.

### 4.9 Anchored popovers, menus and dropdowns

- Anchor wrapper `relative`; panel `absolute right-0 top-full mt-1 z-50 ui-surface-card border shadow-2xl` with explicit `w-*` and `max-h-* overflow-y-auto`.
- Close on outside `pointerdown`, `Escape`, tab change and selection. Keep the trigger inside the outside-click guard.
- Group headers `text-[8px] uppercase tracking-wider text-[var(--dim-color)]`; options `h-6 px-2 text-[9px] uppercase`, active `border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] font-bold`.
- No blur, no nested popovers, no viewport overflow.

### 4.10 Empty, loading and error states

- Main panel empty: `AppEmptyState` (icon 32, title, description slot, `.ui-icon-btn` actions).
- Narrow rail empty: centered inline block, icon 20 at `opacity-20`, title `text-[10px] uppercase tracking-widest`, detail `text-[9px]`.
- Loading: `Loader2` 11–14 px `animate-spin` plus `text-[9px]`/`text-[10px]`; disable the actions that caused the load and do not shift layout.
- Error: banner below the bar with `--status-error-border/bg/color`, the failed operation and the next step; dismiss `X`.
- Success: transient `--status-ok-*` banner, auto-dismiss or explicit close.

### 4.11 Terminal and log surfaces

- The terminal body uses the sanctioned `bg-[#09090b]`, `font-mono text-[10px]`/`text-[11px] leading-tight`, `text-[var(--fg-color)]`, dim metadata.
- ANSI parsing is centralized (`src/utils/ansi.ts`); never inject raw HTML from logs.
- Provide autoscroll, clear and copy controls in the header (`h-6` or `.ui-icon-btn`), and keep the stdin bar at the bottom.

### 4.12 Status, severity and telemetry

- Status pills: `px-1.5 py-0.5 border text-[8px] uppercase font-bold` with `--status-{ok,warn,error,info}-*` tokens.
- Severity mapping: critical → error; high → warn; medium → warn (lighter wash); low → dim.
- Priority mapping: P1 → error, P2 → warn, P3 → dim.
- Never encode meaning with color alone: pair the token with a label or icon.
- Thresholds come from configuration (`agentMetricsStore.contextThresholdPct`, quota configs), never new hardcoded numbers.

---

## 5. Interaction State Matrix

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

---

## 6. Color Semantics

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

---

## 7. Copy, Labels and Units

- English only. Tabs, buttons and labels use terse function-driven terms; explanations go in `AppTooltip`.
- Uppercase comes from CSS, never from typed strings.
- No redundant parentheticals (`..`, `Root`, not `.. (Parent Directory)`), no "Click here", no duplicate panel titles.
- Empty state copy: what is missing → why it matters → next action.
- Error copy: what failed → cause → next action; keep the technical detail in a monospace block or log.
- Units follow Tiger Style: `_ms`, `_seconds`, `_bytes`, `_mbytes`, `_percent`, `_count`; UI values include the unit (`12 ms`, `48 MB`, `72%`).
- Numbers in tables use `tabular-nums`; durations and sizes are right-aligned.

---

## 8. Overflow and Responsive Behavior

- Bars never wrap. Put variable content in a `min-w-0 overflow-x-auto scrollbar-none` group.
- One vertical scroll container per pane; avoid nested scroll areas unless a card list genuinely needs its own bounded scroll.
- Sidebars keep their minimum width; collapse to icons before truncating essential controls.
- Truncated text (`truncate`) that carries meaning gets an `AppTooltip`.
- Wide tables scroll horizontally inside their own wrapper; headers stay aligned.
- Never use `scrollbar-thin`, `scrollbar-thumb-*` or `scrollbar-track-*`; use the global contract (§3.8) or `.scrollbar-none` intentionally.
- Respect `--app-font-scale`; avoid fixed pixel canvases that clip at 130%.

---

## 9. Accessibility and Keyboard

- Icon-only buttons: `AppTooltip` + `aria-label`; symbol-only actions are allowed only when the container makes the meaning obvious.
- Toggles and segmented controls expose `aria-pressed` (or an equivalent selected state).
- `Escape` closes modals, popovers, menus and fullscreen overlays; focus returns to the trigger.
- Every control is reachable by keyboard in visual order; no keyboard traps.
- Inputs use visible labels; placeholders are hints, not labels.
- Status is never color-only: pair it with text or an icon.
- Do not use `--dim-color` at low opacity for essential instructions.

---

## 10. Motion and Layering

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

---

## 11. Agent Checklist and Anti-Patterns

### 11.1 Before coding

1. Identify the workspace panel and read its §3.13 contract.
2. List the components and states involved; reuse §2 components.
3. Decide surfaces, bar height, control height and empty/loading/error states from §1.3/§4.
4. Confirm the data contract change (if any) and plan the atomic propagation to backend, UI, spec and devlog.

### 11.2 While coding

- Use semantic tokens and `.ui-surface-*`; no palette classes.
- One `SubtabBar` per panel; no stacked mode + view bars.
- Use stable `:key` identifiers for every `v-for`.
- Register and teardown every event listener; no leaks.
- Keep sidebars scoped to the project; never key state by bare `agent_id`.

### 11.3 Post-change audit (targeted grep)

Run from the repository root and justify every remaining hit:

```
rg -n "zinc-|emerald-|amber-|rose-|cyan-|slate-|text-white|bg-black|text-\[7px\]|flex-wrap" src/components
rg -n "title=\"" src/components --glob '*.vue'
rg -n "<select" src/components --glob '*.vue'
rg -n "scrollbar-thin|scrollbar-thumb|scrollbar-track" src
rg -n "v-for=.*:key=\"(index|idx)\"" src/components
rg -n "p-2[^0-9].*(h-full|overflow-hidden)" src/components/workspace
```

Charts, third-party SVG and intentional terminal surfaces are valid exceptions; record them in the devlog.

### 11.4 Definition of Done

- Tokens/components only; no new palette or size token.
- State matrix implemented.
- Audit clean or exceptions documented.
- Spec updated (panel contract or component section).
- Devlog entry written.

### 11.5 Anti-patterns

| Anti-pattern | Why it fails | Correct approach |
| :--- | :--- | :--- |
| `p-2` wrapper around a full-tab panel | breaks full-bleed alignment with other tabs | remove the wrapper padding |
| Mode bar stacked above a view bar | wastes 36 px and duplicates navigation | `SubtabBar` leading + tabs |
| Second row of filter chips | consumes vertical space and fragments the bar | inline search + filter dropdown (§4.3) |
| Icon-only action for a primary workflow | undiscoverable | labeled `h-6 px-2 text-[9px]` button |
| Duplicate panel title | redundant chrome | rely on the tab label |
| `text-white` / `bg-black/40` | ignores the Omnisearch/Omarchy theme | `--fg-color` / `ui-surface-*` |
| `text-[7px]` metadata | unreadable | minimum `text-[8px]`, prefer 9 |
| Native `<select>` or `title=""` | broken Linux rendering / inconsistent tooltip | `AppSelect` / `AppTooltip` |
| `flex-wrap` on a toolbar | changes bar height and breaks alignment | horizontal scroll group |
| `v-for` with index key | reconciliation bugs | stable `item.id` |
| Modal without `Escape` | keyboard trap | close contract (§4.8) |
| Accent on many competing elements | no visual hierarchy | one primary accent per region |
| Fixed pixel canvas at 100% | clips at `--app-font-scale` > 1 | fluid layout / scroll container |

---

## 12. Reference Map

| Path | Responsibility |
| :--- | :--- |
| `src/styles/ui.css` | Only scrollbar implementation, surface utilities, `.ui-search-box`, `.ui-icon-btn`, `.ui-kbd`, `.ui-filter-pill`, `.ui-overlay-backdrop`, `.ui-modal-footer` |
| `src-tauri/src/theme.rs` / Omarchy `colors.toml` | `--bg-color`, `--fg-color`, `--dim-color`, `--border-color`, `--accent-color`, `--status-*` |
| `src/components/ui/` | Standardized controls (§2 and §2.10) |
| `src/components/workspace/ProjectWorkspace.vue` | Workspace shell, tab bar, tab wrappers, panel mounting |
| `src/components/workspace/*` | Panel contracts (§3.13) and recipes (§4) |
| `src/components/taskbar/AgentTaskbar.vue` | Bottom taskbar geometry and agent controls |
| `src/components/AppHeader.vue` | Global header, Omnisearch, application menu |
| `docs/specs/01_service_orchestrator.md` | Scripts Hub panel |
| `docs/specs/22_session_lifecycle_and_devlog_graph.md` | Journal, tasks and technical debt records |
| `docs/specs/31_electronics_workstation_and_instrumentation_engine.md` | Electronics panels |
| `docs/specs/33_cad_and_pcb_integration.md` | CAD/PCB panels |
| `docs/specs/34_project_management_and_oss_collaboration.md` | Tasks/Forge panel |
| `docs/specs/35_ui_state_machine_and_session_persistence.md` | UI surface nodes, navigation, unsaved guard |
| `docs/specs/37_project_settings_tab_and_overview_dashboard.md` | Settings and overview panels |
| `docs/specs/38_github_inbox_and_triage_flags.md` | Forge inbox/triage |
| `docs/specs/39_forge_provider_and_project_surface_handoff.md` | Forge provider handoff |




