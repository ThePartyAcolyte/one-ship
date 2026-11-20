# Spec: Diagnostic Laboratory

> **Status: DEFERRED** — Feature specified but not yet implemented.
> Implementation tracker: Planned for post-0.2.x roadmap.

## 1. Error Curation & LLM Processing
*   **Analysis:** When a service crashes, the Error Curator extracts the Core Error and runs a short-lived background LLM prompt to generate a human explanation.
*   **Agent Routing:** Configurable trigger. Auto-route to an active agent if a session exists, or trigger manually via a button in the Hub.

## 2. Telemetry and History
*   **Mini-Grafana:** The UI provides a historical dashboard of service crashes and real-time OS-level performance metrics (CPU/RAM spikes per PID).
