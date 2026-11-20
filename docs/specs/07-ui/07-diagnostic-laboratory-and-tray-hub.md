# Diagnostic Laboratory and Tray Hub Interface

> **Status: DEFERRED** — Planned for the post-`0.2.x` roadmap; neither subsystem is implemented in the current release.

## Purpose

Captures the planned post-`0.2.x` specifications for two companion diagnostic and quick-access surfaces: the **Diagnostic Laboratory** (automated crash error curation with LLM explanation and historical OS-level performance telemetry) and the **Tray Hub Interface** (a compact floating system-tray dashboard matching the Omarchy desktop theme).

## Scope

In scope: error curation and LLM explanation routing when a supervised service crashes, historical crash and per-PID CPU/RAM telemetry ("Mini-Grafana"), and the compact system-tray floating dashboard adhering to Omarchy aesthetics.

Out of scope: the currently implemented host-tool availability diagnostics (`run_diagnostics` / `DiagnosticState` in the header menu, covered in [Remote Repositories and Diagnostics](../03-projects/04-remote-repositories-and-diagnostics.md)) and live system logs ([System Logging and Status Terminal](../03-projects/03-system-logging-and-status-terminal.md)).

## Model / Design

### Diagnostic Laboratory

#### Error Curation and LLM Processing

- **Analysis:** When a supervised service crashes, the Error Curator extracts the Core Error from process output and runs a short-lived background LLM prompt to generate a human-readable explanation.
- **Agent routing:** Driven by a configurable trigger—either auto-routing the curated error to an active agent if a session exists, or triggering manually via a button in the Hub.

#### Telemetry and History

- **Mini-Grafana:** The UI provides a historical dashboard of service crashes paired with real-time OS-level performance metrics (CPU and RAM spikes per PID).

### Tray Hub Interface

#### Compact Dashboard

- Clicking the system tray icon spawns a compact floating window rather than a static text menu.
- Displays a condensed view of active projects alongside quick-access action buttons.

#### Omarchy Ethos

- Strictly follows the active Omarchy visual theme (palette and semantic CSS variables defined in [Design System Foundations](01-design-system-foundations.md)).
- TUI-inspired aesthetics while remaining fully functional with a mouse pointer.

## Contracts

No daemon commands, SSE events, or database tables are currently defined for either deferred feature. Future implementation will align with the `API_COMMANDS` / `ts-rs` pipeline and Omarchy theme variable contracts.

## Implementation Status

Verified against the repository:

- **Diagnostic Laboratory** (Error Curator LLM background analysis, agent auto-routing, Mini-Grafana crash/PID telemetry dashboard): **Roadmap** (not present in `src-tauri/src/` or `src/`).
- **Tray Hub Interface** (floating system tray dashboard window): **Roadmap** (no system tray icon or floating tray window is registered in `src-tauri/src/`).
- *(Unrelated existing capability)*: `src-tauri/src/kopeng/diagnostic.rs` implements host-dependency checks (`run_diagnostics` -> `DiagnosticState` for `git`, `gh`, and `glab`) surfaced in `AppHeader.vue`.

## Related Specs

- [Design System Foundations](01-design-system-foundations.md)
- [Project Registry and Service Orchestration](../03-projects/01-project-registry-and-service-orchestration.md)
- [System Logging and Status Terminal](../03-projects/03-system-logging-and-status-terminal.md)
- [Remote Repositories and Diagnostics](../03-projects/04-remote-repositories-and-diagnostics.md)

Supersedes legacy: `02_diagnostic_laboratory.md`, `03_tray_hub_interface.md`
