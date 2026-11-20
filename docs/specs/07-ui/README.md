# UI & Design System Specifications

Specifications for the ONE-SHIP visual language, standardized UI components, workspace chrome, state synchronization, dashboard/settings surfaces, localization strategy, and deferred diagnostic/tray interfaces.

## Reading Order

1. [Design System Foundations](01-design-system-foundations.md) — Core theme tokens, surface hierarchy, spacing/typography scales, interaction state matrix, copy/unit rules, and the mandatory agent design contract.
2. [UI Component Library and Composition Recipes](02-ui-component-library-and-recipes.md) — Reusable `src/components/ui/` primitives and normative composition recipes for panels, toolbars, cards, lists, forms, modals, and popovers.
3. [Workspace Chrome and Panel Contracts](03-workspace-chrome-and-panel-contracts.md) — Application shell geometry, `.ui-*` classes, scrollbar and WebKitGTK backdrop-blur invariants, Settings modal, and per-tab panel contracts.
4. [UI State Machine and Session Persistence](04-ui-state-machine-and-session-persistence.md) — Daemon-owned canonical UI state, declarative `UiSurfaceRegistry`, agent chat FSM, SQLite persistence, and SSE projection.
5. [Project Settings and Overview Dashboard](05-project-settings-and-overview-dashboard.md) — Fixed `settings` tab, editable project title/description metadata, and the 4-column plugin-driven `overview` widget grid.
6. [Localization and Belta Language Strategy](06-localization-and-belta-language.md) — English-first policy and the deferred 3-tier Lang Belta / Cinturonero translation protocol.
7. [Diagnostic Laboratory and Tray Hub Interface](07-diagnostic-laboratory-and-tray-hub.md) — Deferred post-`0.2.x` roadmap specifications for LLM error curation, Mini-Grafana telemetry, and the floating system tray hub.

## Specification Index

| Document | Description |
| :--- | :--- |
| [01-design-system-foundations.md](01-design-system-foundations.md) | Theme tokens, surface hierarchy, spacing/typography scales, state matrix, and mandatory UI contract. |
| [02-ui-component-library-and-recipes.md](02-ui-component-library-and-recipes.md) | Standardized `src/components/ui/` controls and normative layout/control composition recipes. |
| [03-workspace-chrome-and-panel-contracts.md](03-workspace-chrome-and-panel-contracts.md) | Shell chrome geometry, scrollbar/WebKitGTK blur invariants, modals/popovers, and per-panel rules. |
| [04-ui-state-machine-and-session-persistence.md](04-ui-state-machine-and-session-persistence.md) | Daemon-owned UI state machine, surface tree registry, chat FSM, `oneship.db` tables, and SSE sync. |
| [05-project-settings-and-overview-dashboard.md](05-project-settings-and-overview-dashboard.md) | Fixed `settings` tab, mutable project title/description, and 4-column plugin-driven `overview` grid. |
| [06-localization-and-belta-language.md](06-localization-and-belta-language.md) | Deferred 3-tier Lang Belta i18n architecture, Cinturonero boundary matrix, and corpus workflow. |
| [07-diagnostic-laboratory-and-tray-hub.md](07-diagnostic-laboratory-and-tray-hub.md) | Deferred roadmap specs for LLM crash error curation, OS telemetry dashboard, and floating tray hub. |
