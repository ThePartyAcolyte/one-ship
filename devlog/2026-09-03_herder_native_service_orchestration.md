# Devlog: Herder-Native Service Orchestration & Decoupled Lifecycle

**Date:** 2026-09-03  
**Author:** bosmang (Lead Systems Architect)  
**Status:** Completed & Validated

## 1. Objective
Refactor service execution to eliminate in-memory Tokio child processes inside the GUI binary, aligning with `01_service_orchestrator.md`. Make Herder the sole execution fabric and terminal multiplexer for persistent services, ensuring that closing the GUI never kills services, and that active services are dynamically detected via Herder process introspection upon GUI startup.

## 2. Changes Executed
1. **`rula.toml` Integration**:
   - Added persistent script `[scripts.persistent.mcp]` pointing to `cargo run --bin kapawu-mcp --manifest-path src-tauri/Cargo.toml`.
2. **`src-tauri/src/orchestrator.rs`**:
   - Eliminated in-memory `OrchestratorState` with `Mutex<HashMap<String, Child>>` and `kill_on_drop(true)`.
   - Re-implemented `start_service`: Resolves or creates the project's Herder workspace, creates/reuses a dedicated tab labeled with `service_id`, and dispatches the execution string (`cd <cwd> && <cmd>`) to the pane via `send-text` and `Enter`.
   - Re-implemented `stop_service`: Locates the dedicated Herder tab/pane for `service_id` and dispatches `C-c` terminal interrupt.
   - Re-implemented `get_running_services`: Queries all registered projects and their Herder workspaces, inspecting tabs matching persistent scripts via `herdr pane process-info --pane <pane_id>`. Active foreground processes are accurately identified across GUI restarts.
3. **`src-tauri/src/lib.rs` & `src-tauri/src/kopeng/herdr.rs`**:
   - Removed `.manage(orchestrator::OrchestratorState::default())`.
   - Exposed `get_pane_id_for_tab` and `get_pane_busy_process` as `pub fn`.
4. **`src/App.vue`**:
   - Updated `toggleService` to pass `projectPath`, `projectName`, and `serviceId` to `start_service` and `stop_service`.
5. **Spec Synchronization**:
   - Synchronized `docs/specs/01_service_orchestrator.md` with Herder-native terminal execution and process introspection.

## 3. Verification
- `cargo check --manifest-path src-tauri/Cargo.toml`: Passed (0 errors).
- `npm run build`: Passed (0 errors in 1.54s).
- Live dev server (`wang-kapawu` in `w8:p5`): Recompiled and running smoothly.
