# Development Log: Service Tab Isolation (`svc:<id>`) & Contextual Focus

- **Date:** 2026-09-03
- **Topic:** Persistent Service Tab Isolation, Namespace Collision Prevention, and Herder Terminal Autofocus

---

## 1. Problem Diagnosis & Root Cause
- **Issue:** Launching the `mcp` persistent service via the UI marked the service as `RUNNING`, but no dedicated tab appeared in the project workspace `w8`.
- **Root Cause:**
  - `rula.toml` had both `[agents.mcp]` (AI agent persona) and `[scripts.persistent.mcp]` (persistent daemon).
  - Herder workspace `w8` had an existing tab (`w8:tC`) labeled `mcp` representing the agent persona terminal.
  - `orchestrator::start_service` queried workspace tabs matching `label.eq_ignore_ascii_case(&service_id)`. Because `service_id == "mcp"` matched tab `w8:tC`, it treated the agent tab as the service tab and injected the `cargo run --bin kapawu-mcp` execution string directly into `w8:pC` instead of opening a dedicated tab.

---

## 2. Architectural Changes
1. **Service Tab Naming Convention (`svc:<service_id>`):**
   - Implemented `orchestrator::service_tab_label(service_id)` producing `svc:<service_id>` (e.g. `svc:mcp`).
   - `create_service_tab` labels tabs with `--label svc:<service_id>`.
   - `start_service`, `stop_service`, and `get_running_services` now strictly match `svc:<service_id>`, completely eliminating collisions with agent tabs (`<agent_id>`).
   - `start_service` now contextually invokes `focus_herdr_tab` upon dispatching the service process, bringing the developer's Herder terminal directly to the new service tab.
2. **IPC Command & UI Integration:**
   - Added `focus_service_tab` Tauri command in `src-tauri/src/orchestrator.rs` and registered it in `src-tauri/src/lib.rs`.
   - Updated `src/components/ProjectCard/ScriptsSection.vue`:
     - Added `Terminal` icon and `focusService(serviceId)` helper.
     - Added "Focus Terminal" option in right-click context menu for running services.
     - Replaced native `title="..."` attributes with brutalist `<AppTooltip>`.
     - Added interactive focus badge and control bar button to bring Herder terminal focus directly to `svc:<id>`.
3. **Dead Code Cleanup (AGENTS.md Rule 4):**
   - Removed obsolete `src/components/ProjectCard/ServicesSection.vue`.

---

## 3. Verification
- Sent `C-c` to `w8:pC` to stop the rogue process and restore the clean bash shell.
- Port 8765 freed.
- `cargo check --manifest-path src-tauri/Cargo.toml` succeeded with 0 errors.
- `npm run build` completed with 0 errors.
- Hot module reloading and binary recompilation verified in running Tauri development instance.
