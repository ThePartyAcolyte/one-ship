# Devlog: Agent Callbacks, UI Refactor & Service Orchestration

**Date:** 2026-09-03  
**Status:** Completed  
**Milestones Closed:** Phase 3.2 (Agent Execution Engine & Callbacks), Phase 4 (Service Orchestration)

---

## 1. Ergonomic Cockpit & UI Refactor

1. **Master-Detail Settings Modal:**
   - Replaced horizontal tabs with a vertical sidebar (`w-48`) listing plugins dynamically, accompanied by an independent right-side scrollable settings editor.
   - Configured dark styling for native dropdown elements (`<select style="color-scheme: dark" class="bg-[#141414]">`) to prevent stark white OS default rendering.
   - Relocated global settings trigger to a floating button at `bottom-4 left-4`.
2. **Project Card Header & Tooltips:**
   - Replaced permanent delete and configure icons with a contextual three-dots menu (`<MoreVertical />`) featuring `Settings` and `Remove Project` with click-away auto-dismiss.
   - Reoriented upper action tooltips (`top-7 left-0` / `top-7 right-0`) to render beneath buttons and eliminate card boundary clipping.
3. **Smart Diagnostic Bubble:**
   - Auto-hides when all diagnostic checks (`git`, `gh`, `OpenDictate`) report operational status.
   - Features click-away listener and an 8-second auto-dismiss timeout upon opening.

---

## 2. Multi-Agent Task Delegation Ledger & Bidirectional Callback Loop

1. **Task Delegation Ledger (`~/.config/wang-kapawu/delegations.json`):**
   - Implemented correlation ledger tracking `task_id`, `originator_agent`, `target_agent`, `instruction`, `status`, and timestamps.
   - Enhanced MCP tool `delegate_agent_task` to generate traceable task IDs and append correlation instructions to prompts dispatched to Herder panes.
2. **Dynamic Pane Resolution (`resolve_herdr_target`):**
   - Resolves target agent labels (`bosmang`, `lang_belta`, `git`) directly to live Herder pane IDs (`w8:p1`, `w8:p2`) by cross-referencing `herdr agent list`, `herdr tab list`, and `herdr pane list`.
3. **Automated Return Callbacks in `report_task_status`:**
   - When an agent completes a task and calls `report_task_status`, the MCP server marks the delegation as completed and automatically dispatches a return prompt to the commissioning agent's Herder pane:
     ```text
     [KAPAWU CALLBACK from '<agent_id>']
     Task '<task_id>' finished with status [COMPLETED].
     Summary: <summary>
     ```
   - Field-tested live: `bosmang` (`w8:p1`) delegated translation tasks to `lang_belta` (`w8:p2`), which executed and autonomously returned the callback notification back to `w8:p1`.

---

## 3. Phase 4: Service Orchestration Fleet

1. **Project Service Hydration (`hydration.rs`):**
   - Parses service configurations from `rula.toml` (supporting both `[[plugins.orchestrator.services]]` arrays and `[services.<id>]` tables).
   - Injects structured `services: Vec<ServiceEntry>` into `HydratedProject`.
2. **Process Management & Automated Reaping (`orchestrator.rs`):**
   - Managed background subprocesses via Tokio `Command` stored in `OrchestratorState`.
   - Implemented `get_running_services` command with automated process reaping (`processes.retain`) using non-blocking `try_wait()` to detect process termination.
3. **Live Output Streaming:**
   - Attached asynchronous `BufReader` tasks to child `stdout` and `stderr` streams, streaming output to `kapawu.log` and emitting `service_output` Tauri events to the frontend.
4. **Interactive Service Cards in Project View:**
   - Rendered dynamic service cards in the Project Card replacing `Pending Hydration...`.
   - Integrated live `running` (pulsing accent) vs `stopped` badges and one-click Start (`Play`) / Stop (`Square`) controls.
