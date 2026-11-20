# Workspace Chrome and Panel Contracts

## Purpose

Defines the structural layout, chrome geometry, surface stacking, WebKitGTK rendering invariants, modal/popover contracts, and per-panel UI rules for the ONE-SHIP desktop workspace. It complements [Design System Foundations](01-design-system-foundations.md) and [UI Component Library and Composition Recipes](02-ui-component-library-and-recipes.md) by specifying how the application shell, toolbars, scrollbars, overlays, and each workspace tab are composed.

## Scope

In scope: application shell geometry, header and taskbar sizing invariants, workspace tab architecture, shared `.ui-*` utility classes, scrollbar contract, WebKitGTK overlay and backdrop-blur invariants, settings modal contract, anchored popover contract, and all per-panel contracts including the unsaved-change guard.

Out of scope: base CSS tokens and scales ([Design System Foundations](01-design-system-foundations.md)), reusable control APIs and generic layout recipes ([UI Component Library and Composition Recipes](02-ui-component-library-and-recipes.md)), canonical state persistence ([UI State Machine and Session Persistence](04-ui-state-machine-and-session-persistence.md)), and the Settings/Overview dashboard domain specs ([Project Settings and Overview Dashboard](05-project-settings-and-overview-dashboard.md)).

## Model / Design

### Application Shell and Workspace Chrome Geometry

- **Two-column shell:** Below `AppHeader`, the application is one flex row consisting of the left project rail (`ProjectVerticalRail`, `w-11` / 44 px), the workspace stage (tab bar + active content + agent taskbar), and the optional docked chat column.
- **Full-height rail:** The project rail spans from the first row below the header to the bottom edge of the main viewport. Its drawer overlays the full height and never stops at the tab bar.
- **Drawer elevation and stacking (`ProjectVerticalRail.vue`):** The project drawer backdrop teleports to `body` at `z-40` (dimming the main workspace and bottom `AgentTaskbar` at `z-30`), while the left vertical project rail and the full-height drawer panel (`top-0 bottom-0`, `border-t border-b border-r border-[var(--border-color)]`) sit at `z-50` above the backdrop.
- **Workspace stage:** The stage column contains, in order: the tab bar (`h-9`), the active tab content (`flex-1 min-h-0`), and `AgentTaskbar` (`h-9 shrink-0`). The agent taskbar therefore starts at the rail's right edge and ends at the workspace's right border.
- **Docked chat:** The docked chat column and its resizer sit outside the workspace border and share the main viewport's top and bottom edges. The main viewport has no bottom padding so the chat reaches the bottom edge. The agent taskbar is never rendered under the rail or under the docked chat.
- **Uniform chrome heights:** The tab bar and the agent taskbar are both `h-9` with `px-3`; their interactive controls are `h-6` (24 px). The rail's top action buttons are `w-8 h-8` (32 px) and share the same square geometry.

### Toolbar and Taskbar Sizing Invariants (`h-6` Baseline)

- **Strict uniform height:** All primary interactive elements in the application header toolbar (`AppHeader.vue`) and bottom taskbar (`AgentTaskbar.vue` / `AccountQuotaControls.vue`) adhere to a uniform `h-6` (24 px) structural height.
- **Account and quota controls (`AccountQuotaControls.vue` in `AgentTaskbar.vue`):** The Google Account Switcher and DeepSeek Wallet trigger buttons are mounted on the right side of the bottom taskbar (`AgentTaskbar.vue`) at `h-6 flex items-center gap-1.5 px-2 shrink-0 select-none`.
- **Zero extraneous container padding:** Nested child containers (such as the telemetry gauge wrapper in the Google switcher) must not introduce independent vertical padding (`py-0.5`) that disrupts this baseline.
- **Telemetry gauge sizing:** In-toolbar quota telemetry gauges (`AppQuotaGauge`) use `:size="14"` to mirror the 14 px provider icons (`GeminiIcon`, `DeepSeekIcon`), preserving vertical centering and sub-pixel symmetry.
- **Header action bar (`AppHeader.vue`):** Contains the inline Omnisearch input (`h-6`, `.ui-search-box` with `Ctrl+K` badge), Voice Controller pill (`h-6`), conditional Daemon Error badge, and a single Hamburger Application Menu (`Menu` icon, `.ui-icon-btn`) housing System Health status, a Global Voice Controller `AppToggle`, `System Logs`, `Settings`, and `About ONE-SHIP`. The standalone Diagnostics button is deprecated.

### Workspace Navigation, Tab Architecture, and Live Preview (`ProjectWorkspace.vue`)

- **Standardized function-driven naming:** Workspace sub-tab titles are short, unambiguous, and function-driven across the 14 workspace tabs (`Overview`, `Project`, `Explorer`, `Electronics`, `Design`, `CAD`, `PCB`, `Preview`, `Profiling`, `RE Lab`, `Scripts`, `Git & Snapshots`, `Journal`, `Skills`), supplemented by the fixed icon-only `Settings` tab and the `Graph` (`knowledge`) tab. Descriptive parentheticals (e.g. `(Workspace Root)`) or compound labels are strictly prohibited in the tab bar.
- **Live Web Preview tab (`Preview`, `ProjectPreviewTab.vue`):** Enabled when `[preview] url = "http://..."` is declared in `rula.toml` (`project.preview_url`). Provides an embedded responsive iframe viewport with preset device widths (`Desktop 100%`, `Tablet 768px`, `Mobile 390px`), fullscreen toggle, reload, and external browser launch. Fullscreen takes the whole window: the project rail and the agent bar are hidden while active, the overlay paints above workspace chrome and floating chat and below modals (`z-[45]`), and `Escape` exits.
- **Tab styling homogenization:** All workspace tabs adhere to a single unified brutalist visual convention (`border-[var(--accent-color)] bg-[var(--bg-color)] text-[var(--accent-color)]` when active, `border-transparent text-[var(--dim-color)] hover:border-[var(--border-color)]` when inactive).
- **Tab bar geometry:** The tab bar is an `h-9` row (`ui-surface-title`, `px-3`, `py-0`) and every tab control is exactly `h-6`: text tabs use `px-2.5`, the fixed gear settings tab uses `w-6 px-0`, and the overflow button uses `w-6 px-0`. This eliminates the historical 4 px height mismatch between the gear tab and text tabs.
- **Zero path clutter in tab bars:** Never render filesystem paths or URLs inside workspace tab bars. Directory locations belong exclusively in tooltips or dedicated header inspectors.

### Unified Visual Language Classes (`src/styles/ui.css`)

All search-driven modals, drawers, and overlays share a single set of reusable brutalist classes defined in `src/styles/ui.css` (imported globally in `src/main.ts`), modeled after `OmnisearchModal.vue`:

- `.ui-search-box` and `.ui-search-box__icon`: Standard search bar container (`border-[var(--border-color)]`, `bg-[var(--ui-bg-title)]`, `focus-within:border-[var(--accent-color)]`) with an accent-colored `<Search>` icon.
- `.ui-kbd`: Compact keyboard shortcut badge (`9px` monospace, `border-[var(--border-color)]`, translucent black backdrop).
- `.ui-filter-pill` and `.ui-filter-pill--active`: Compact uppercase filter toggle pills (`9px` bold) replacing oversized dropdowns where options are finite (e.g. Omnisearch scopes, Log levels).
- `.ui-icon-btn` and `.ui-icon-btn--danger`: Square `24px` (`1.5rem`) icon-only action buttons (e.g. Clear Logs `<Trash2>` without text), always paired with `<AppTooltip>`.
- `.ui-overlay-backdrop`: Standardized full-surface modal overlay (`rgba(0, 0, 0, 0.45)` scrim + `12px` backdrop blur). Every modal overlay in the application MUST use this class instead of ad-hoc `bg-black/N` scrims.
- `.ui-modal-footer`: Standardized modal footer bar (`ui-surface-title`, `9px` dim text) displaying keyboard hints and item counts. Text actions placed in a modal footer use `h-6 px-2.5 text-[9px] font-bold uppercase tracking-wider`; icon-only actions use `.ui-icon-btn`.

### Standardized Scrollbar Contract

- **Geometry:** Constant `6px` width/height, strictly square (`border-radius: 0px`), no directional arrow buttons. Width must not change on hover or active states.
- **Colors:** Track and corner are fully transparent at rest; on hover/active the track takes a translucent `var(--accent-color)` tint. The draggable thumb is a theme-aware dim tone at rest (never a fixed near-black) and switches to solid `var(--accent-color)` on hover/active.
- **Single source of truth:** `src/styles/ui.css` defines the only scrollbar styling in the application through global `::-webkit-scrollbar*` pseudo-elements. No component may redefine scrollbar geometry, track, or thumb.
- **Mandatory implementation rule (WebKit):** WebKit only instantiates the custom `RenderScrollbar` when the element's computed `scrollbar-width` is `auto` **and** `scrollbar-color` is `auto` (`ComputedStyle::usesLegacyScrollbarStyle()` in `StyleComputedStyle+GettersInlines.h`). Any other value makes WebKit ignore every `::-webkit-scrollbar` rule and fall back to the native Adwaita scrollbar:
  - Tailwind standard scrollbar utilities (`scrollbar-thin`, `scrollbar-thumb-*`, `scrollbar-track-*`) are **prohibited** on scroll containers. Tailwind v4 ships `scrollbar-thin` as `scrollbar-width: thin`, silently disabling the custom scrollbar.
  - `scrollbar-color` is inherited, so a standard-property utility on an ancestor breaks every nested scroll container in its subtree.
  - Intentionally invisible scrollbars use `.scrollbar-none` (`scrollbar-width: none` + `::-webkit-scrollbar { display: none }`). This is the only allowed standard-property usage.

### Overlay and Backdrop Blur Contract (WebKitGTK)

All modal windows (full-screen dialogs, workspace dialogs, chat-panel dialogs, and blocking card overlays) share one backdrop contract defined by `.ui-overlay-backdrop` in `src/styles/ui.css`:

- **Literal filter values only:** `backdrop-filter` (and `-webkit-backdrop-filter`) MUST be declared with literal values (`blur(12px)`). WebKit bug #289800 ignores CSS custom properties and `calc()` inside `backdrop-filter`, which makes every Tailwind v4 `backdrop-blur-*` utility (generated from `var(--tw-backdrop-blur,)`) a silent no-op on `webkit2gtk-4.1`.
- **Accelerated compositing is mandatory:** `backdrop-filter` is applied by the compositor and is silently ignored by WebKitGTK's non-accelerated software path. `WEBKIT_DISABLE_DMABUF_RENDERER=1` (and `WEBKIT_DISABLE_COMPOSITING_MODE=1`) empty the renderer transport set, making `AcceleratedBackingStore::canUseHardwareAcceleration()` return false in WebKitGTK 2.54.1. The development launcher (`scripts/run_dev.sh`) must never set them. If the DMA-BUF path must be avoided, use `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1`, which keeps accelerated compositing over shared-memory transport (see the runtime environment and packaging specification).
- **Bounded blurred layers:** Blurred backdrop layers are limited to one per open modal (expected nesting of at most two) plus at most one blocking overlay per initializing project card. Never mount a blurred backdrop per row, per HUD chip, or per repeated list item. WebKitGTK 2.54 leaks memory proportionally to the number of simultaneous `backdrop-filter` layers (WebKit bug #328065), staying stable up to roughly 10–14 layers and degrading sharply beyond ~18.
- **Zero paint filters on the application tree:** Applying `filter: blur(...)` to a container that holds live application content is strictly prohibited. `filter` is a paint filter that re-rasterizes the entire subtree on every invalidation (including modal scrolling) and creates a containing block that breaks `position: fixed` descendants. Backdrop blur belongs exclusively to the overlay leaf sitting above content.
- **Non-opaque scrim:** The scrim MUST remain semi-transparent (`alpha <= 0.55`) so backdrop blur is perceptible; opaque scrims negate the effect.
- **Prohibited utilities:** Tailwind `backdrop-blur-*`, `backdrop-brightness-*`, `backdrop-saturate-*`, and related utilities are prohibited app-wide until WebKit bug #289800 is fixed; decorative surfaces use flat semi-transparent backgrounds instead.

### Settings Modal Contract (`SettingsModal.vue`)

The settings modal follows the Omnisearch / System Logs visual language:

- **Container:** `ui-surface-base`, `border`, `max-w-4xl`, `h-[85vh]`; no ad-hoc `bg-black/*` panels.
- **Header:** A single `.ui-search-box` row (`!border-x-0 !border-t-0 px-4 py-2.5`) with the `Settings` icon, title, scope badge (`config.toml` / `rula.toml`), a vertical divider, the accent `Search` icon, the search input, match counter, clear button, `ESC` `.ui-kbd`, and close icon. `Escape` closes the modal from the capture phase.
- **Body:** Left sidebar `w-52` with `ui-surface-list`; category buttons `h-7 px-2.5 text-[10px]` with a 2 px accent left border. The content column uses `p-4`, `gap-3`, `ui-surface-base`; each parameter is a `ui-surface-card p-3` block with an 11 px label, 10 px description, and 9 px default hint.
- **Footer:** `.ui-modal-footer` with the resolved file path on the left and `Cancel` / `Save` actions on the right, both `h-6 px-2.5 text-[9px]`.
- **Form controls:** Inputs and selects keep the 32 px height; the inline `Test` voice action is `h-8` and uses only valid semantic tokens.

### Anchored Popover Surface Contract

- **Opaque elevated surface:** Anchored popovers (agent launcher, account/quota switchers, header application menu, log selection dropdown) declare a solid surface via `ui-surface-title` (or `--ui-bg-title`) over `--bg-color`; they must never rely on an undefined token or on `--bg-color` alone, which blends into the workspace and reads as transparent.
- **Stacking:** A popover uses `z-50` inside a positioned anchor and must paint above the content it overlaps. Popovers that open upward from the workspace stage live inside that stage's stacking context.
- **Bounds:** Every popover declares a bounded width; growing lists inside it use `overflow-y-auto` with an explicit `max-h`; upward popovers may not exceed the stage width. No blurred backdrop is mounted per popover.
- **No `backdrop-filter`:** Popover surfaces are flat and opaque; the blur contract remains exclusive to `.ui-overlay-backdrop`.

### Workspace Panel Contracts (No Redundant Chrome)

#### Shared Panel Invariants

- **No duplicated title:** A workspace tab whose tab label or tooltip already identifies the view must not render the same title inside the panel header. Panels configured as icon-only tabs (e.g. the gear `settings` tab) rely on the tab tooltip; they do not repeat a heading.
- **Single top toolbar:** Panel-level search, status, and primary actions share one `h-9` `.ui-search-box` or `ui-surface-title` row at the top. Panel action buttons use `h-6 px-2.5 text-[9px]`. A bottom action footer is reserved for modals, not for full-tab panels.
- **Full-bleed geometry:** The tab content wrapper provides no outer padding; the panel fills the full available width/height and owns its internal spacing. Sidebars extend to the bottom edge and keep their surface below short lists; the content column scrolls in the remaining space.
- **No floating card shell:** A panel that is the entire tab content uses flat surfaces (`ui-surface-base` / `ui-surface-list` / `ui-surface-card`); it does not add an extra bordered card inside the tab.
- **No redundant category headers:** In master-detail panels, the active category name and enabled/disabled state live only in the sidebar. The detail pane may keep a title-less quick-action row when the category exposes actions.
- **Panel bar (`SubtabBar.vue`):** One `h-9` row on `ui-surface-title` split into zones:
  - `leading` (optional): fixed control that must never scroll away (e.g. an `AppSegmentedToggle` mode switch) behind a divider.
  - `tabs`: panel tabs (`h-6 px-2.5`, `text-[9px]`, accent border/fill for the active tab, transparent border for the rest), scrolling only when they cannot fit.
  - `center` (optional, `flex-1`): inline control such as a borderless search input plus a dropdown trigger, without its own border or surface.
  - `right` (optional): caller tools (search, filters, actions) behind a left divider, omitted entirely when the `right` slot is empty so tabs-only bars keep identical geometry. Tool content never wraps; the right zone scrolls horizontally when needed. Mode + views must share this single row instead of stacking a dedicated mode bar above the view bar.
- **Unsaved-change guard:** Panels with editable state register the UI navigation guard while dirty (`src/utils/navigationGuard.ts`, consumed by tab and project navigation). Leaving the tab or switching project opens a confirmation modal with `Save & Continue`, `Discard & Continue`, and `Cancel`. Watcher-driven disk changes must never overwrite dirty edits; they surface a warning banner instead.

#### Per-Panel Contracts

- **Explorer (`ProjectWorkspace.vue` files view):** Two rows on `ui-surface-title` with no panel title and no current-path text. Row one holds the branch as plain clickable text (`GitBranchSelector`, no boxed control) and the collapse action; row two holds the `.ui-search-box` input with the filter trigger to its right. Filter choices use `AppToggle`. `F5` refreshes the tree while the Explorer tab is active and no dedicated refresh button is rendered. Reader breadcrumbs show the open document when one exists and otherwise the explored folder path, ending in the current folder.
- **Design / CAD / PCB (`ProjectDesignTab.vue`, `ProjectCadTab.vue`, `ProjectPcbTab.vue`):** One `h-9` row on `ui-surface-title` with a bottom border and no wrapping. The left group scrolls horizontally when the window is narrow; the right group stays fixed. Symbol actions use `.ui-icon-btn` (`h-6 w-6`) and toggle their accent state when active; text actions use `h-6 px-2 text-[9px] uppercase`; selects and inputs are `h-6` (`compact` selects). Sidebars use `ui-surface-list` surfaces, `text-[9px]` uppercase section labels, and `text-[10px]` rows with an accent border/fill for the selected item. Missing-file panels render `AppEmptyState` with the same card, title scale, and `.ui-icon-btn` actions.
- **RE Lab and Profiling (`ProjectReaTab.vue`, `ProjectProfilerTab.vue`):** Same `h-9 ui-surface-title` contract. No duplicate panel title (RE Lab drops its internal `RE Lab` heading); engine status is a compact `h-6` chip (`SLEEPING` or `ENGINE :port · PID`), and toolchain access is a `Wrench` `.ui-icon-btn` opening the inspection/installation modal. Profiling exposes its comparison view through a labeled `Compare`/`Runs` button instead of an icon-only toggle.
- **Scripts (`ProjectTerminalGrid.vue`):** The rail header is a title-less `ui-surface-title` block: `h-9` row holding only `History` and `+ Shell`, then a full-width `.ui-search-box` filter row. The console header uses the same `h-9` surface with `h-6` `ALL`/`AUTO`/`SIGINT` actions and `.ui-icon-btn` clear/copy controls; the terminal body keeps its own dark surface (`bg-[#09090b]`).
- **Electronics Firmware (`ProjectElectronicsTab.vue`):** Three stacked cards on `ui-surface-card` — `Hardware` (detected port, baud, board catalog with USB suggestion), `Firmware Project` (language `C++`/`C`/`Python`, framework, monitor speed, scaffold), and `Toolchain & Operations` (toolchain state, environment, compile/upload, logs). The toolchain modal uses semantic status tokens; `mpremote` is copy-only while pacman tools install inline.
- **Journal (`DevlogJournalView.vue`):** One `SubtabBar` for `Devlogs | Tasks | Tech Debt` with record counts as badges and add/refresh actions in the right slot. The center slot holds a borderless inline search (remaining width) plus a `Filter` icon button opening an anchored dropdown of contextual options (Agent; Status/Agent/Priority; Status/Severity). There is no second row and no chip strip.
- **Git & Snapshots (`ProjectGitSnapshotsTab.vue`):** Full-bleed dual-panel layout with no redundant top cockpit bar or manual snapshot creator. Left `w-80 ui-surface-list` timeline stream (`All | Snaps | Commits` + optional domain filter, `F5` refreshes). Right inspector opens with an `h-9 ui-surface-title` bar holding `GitBranchSelector`, active item metadata (`WORKING TREE`, snapshot, or commit), and right-aligned actions (`rula.toml` quick-action buttons, `+` action creator, `SYNC`, and `OPEN AGENT` in Working Tree mode; `RESTORE SELECTED` / `FULL ROLLBACK` in Snapshot mode). Below it, a resizable file/staging pane (`220px–560px` via a `w-1 cursor-col-resize` divider, default `288px`; commit message `AppTextarea` with AI-draft `Sparkles` trigger and `COMMIT`, plus `Staged Changes` and `Changes` with per-file and bulk `+`/`-` staging, two-line filename + relative path rows, `overflow-x-hidden`, and a `MAX_RENDERED_FILES_COUNT = 250` render cap) sits beside the live file diff pane. No global unified diff is loaded when `selectedFile` is `null` (`AppEmptyState`), and Design/PCB/CAD domain assets (`is_domain_asset`) remain tracked/stageable in Git while suppressing diff preview (`AppEmptyState`).
- **Skills (`ProjectSkillsTab.vue`):** One `SubtabBar` for `Installed | Explore` with `Installed` count badge, `#center` borderless inline search (live filter in `Installed`, unified SkillsMP search + automatic `http(s)://` URL detection with inline skill-name field and `Install` action in `Explore`), and `#right` dual view mode `.ui-icon-btn` toggles (`grid` default / `list`), refresh `.ui-icon-btn`, and active skill/token telemetry. Full-bleed `p-3` body with compact square cards in Grid mode (`min-h-[168px]`, `ui-surface-card`) and horizontal rows in List mode across both `Installed` and `Explore`, `AppEmptyState` for empty states, and an `h-9 ui-surface-title` split inspector for `SKILL.md`.

## Contracts

### Chrome, Class, and Guard Contracts

- **CSS utility classes (`src/styles/ui.css`):** `.ui-surface-{base,title,list,card}`, `.ui-search-box`, `.ui-search-box__icon`, `.ui-kbd`, `.ui-filter-pill`, `.ui-filter-pill--active`, `.ui-icon-btn`, `.ui-icon-btn--danger`, `.ui-overlay-backdrop`, `.ui-modal-footer`, `.scrollbar-none`.
- **Scrollbar invariants:** `::-webkit-scrollbar` fixed at `6px` width/height, `border-radius: 0px`; `scrollbar-width: thin` / `scrollbar-color` utilities prohibited on scroll containers.
- **Backdrop blur invariants:** `.ui-overlay-backdrop` uses `rgba(0, 0, 0, 0.45)` and literal `blur(12px)` on `-webkit-backdrop-filter` / `backdrop-filter`; Tailwind `backdrop-blur-*` prohibited.
- **Preview configuration:** `[preview] url = "http://..."` in `rula.toml` maps to `project.preview_url` and gates the `Preview` tab (`has_preview_url`).
- **Navigation guard (`src/utils/navigationGuard.ts`):** `registerNavigationGuard(guard)` and `runNavigationGuard(target)` gate `projectId` / `workspaceTab` transitions when a mounted panel has unsaved edits.
- **Constants referenced in panels:** `MAX_RENDERED_FILES_COUNT = 250` (`ProjectGitSnapshotsTab.vue`); file/staging pane resize bounds `220px`–`560px` (default `288px`).

### Workspace Component Inventory

| Component / Module | Role |
| :--- | :--- |
| `src/components/AppHeader.vue` | Global header, Omnisearch (`Ctrl+K`), Voice Controller pill, error badge, hamburger menu |
| `src/components/taskbar/AgentTaskbar.vue` | Bottom `h-9` taskbar, agent pills, `AccountQuotaControls.vue` (`h-6`) |
| `src/components/workspace/ProjectVerticalRail.vue` | `w-11` full-height left rail and teleported drawer (`z-40` backdrop, `z-50` drawer) |
| `src/components/workspace/ProjectWorkspace.vue` | Workspace stage, `h-9` tab bar (`h-6` tabs), Explorer header, panel mounting |
| `src/components/modals/SettingsModal.vue` | `max-w-4xl` `h-[85vh]` settings modal for `config.toml` and `rula.toml` |
| `src/components/ui/SubtabBar.vue` | Single `h-9` panel bar with `leading`, `tabs`, `center`, and `right` slots |
| `src/components/workspace/ProjectPreviewTab.vue` | Responsive iframe preview (`Desktop 100%`, `Tablet 768px`, `Mobile 390px`) and fullscreen `z-[45]` |
| `src/components/workspace/ProjectDesignTab.vue`, `ProjectCadTab.vue`, `ProjectPcbTab.vue` | Domain artifact studio toolbars and sidebars |
| `src/components/workspace/ProjectReaTab.vue`, `ProjectProfilerTab.vue` | RE Lab and Profiling toolbars and modals |
| `src/components/workspace/ProjectTerminalGrid.vue` | Scripts Hub rail and console header |
| `src/components/workspace/ProjectElectronicsTab.vue` | Electronics firmware cards and toolchain modal |
| `src/components/workspace/ProjectGitSnapshotsTab.vue` | Dual-panel Git & Snapshots timeline, staging, and diff inspector |
| `src/components/workspace/ProjectSkillsTab.vue` | Skills grid/list browser and `SKILL.md` inspector |

## Implementation Status

Verified against the repository:

- `src/styles/ui.css` implements `.ui-surface-*`, `.ui-search-box`, `.ui-kbd`, `.ui-filter-pill`, `.ui-icon-btn`, `.ui-overlay-backdrop` (with literal `blur(12px)`), `.ui-modal-footer`, and the `6px` square `::-webkit-scrollbar*` rules: **Implemented**.
- `src/utils/navigationGuard.ts` implements `registerNavigationGuard` / `runNavigationGuard`: **Implemented**.
- `src/components/AppHeader.vue`, `src/components/taskbar/AgentTaskbar.vue`, `src/components/modals/SettingsModal.vue`, and all listed `src/components/workspace/*.vue` components exist: **Implemented**.
- Workspace tabs in `ProjectWorkspace.vue` and `src-tauri/src/ui_state.rs` include `settings` (`Settings`) and `knowledge` (`Graph`) in addition to the 14 tabs enumerated in legacy prose (total 16 top-level tab nodes): **Implemented** (note the count discrepancy relative to legacy "14-Tab Architecture" wording).
- Full per-component CSS conformance and removal of legacy palette classes across every panel were not audited line by line here: **Partial / unverified**.

## Related Specs

- [Design System Foundations](01-design-system-foundations.md)
- [UI Component Library and Composition Recipes](02-ui-component-library-and-recipes.md)
- [UI State Machine and Session Persistence](04-ui-state-machine-and-session-persistence.md)
- [Project Settings and Overview Dashboard](05-project-settings-and-overview-dashboard.md)
- [Project Registry and Service Orchestration](../03-projects/01-project-registry-and-service-orchestration.md)
- [Git Workspace and Branches](../03-projects/02-git-workspace-and-branches.md)

Supersedes legacy: `13_design_system_and_ui_components.md`
