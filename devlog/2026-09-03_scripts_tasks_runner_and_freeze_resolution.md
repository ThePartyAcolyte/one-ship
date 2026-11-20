# Work Session Log: Scripts & Tasks Execution Engine, Report Modal & Event Loop Freeze Fix

**Date:** 2026-09-03  
**Status:** Completed  
**Branch:** master  

---

## 1. Root Cause Analysis: UI Freeze & "Application Not Responding" (ANR)
### The Problem
The user reported periodic application freezes where the OS (Hyprland / Wayland compositor) flagged "Application is not responding".

### Diagnosis
- In `src/App.vue`, `updateHerdrStates()` was being invoked every 2500ms via `setInterval`.
- Inside `updateHerdrStates()`, `projects.value = await invoke("get_projects")` was called on every tick.
- `get_projects` runs `hydrate_project` for every project in the registry, which executed 5 synchronous `std::process::Command::new("git")` subprocesses per project.
- Concurrently, `updateAllHerdrStates()` executed 3 synchronous `herdr` CLI commands per project.
- There was no mutual exclusion / non-overlapping guard (`let isUpdating = false`). When a child process or D-Bus query took more than 2500ms, ticks overlapped, saturating Tauri/WebKit IPC event queues and preventing Wayland ping/pong events from being processed.

### The Fix
1. Removed `get_projects` from the periodic interval. Registered projects are immutable during passive observation; `get_projects` is now strictly triggered upon explicit state transitions (`refreshProjects`, `add_project`, `remove_project`, etc.).
2. Added an atomic reentrancy guard (`let isUpdatingHerdr = false; if (isUpdatingHerdr) return; ...`).
3. Adjusted polling intervals to a relaxed, non-overlapping 3000ms.

---

## 2. Unified Scripts & Deterministic Tasks Architecture

### A. Hydration Pipeline (`src-tauri/src/hydration.rs`)
- Added `ScriptOption` (`key`, `flag`, `label`, `type` ["boolean" | "select"], `choices`, `default`).
- Added `ScriptEntry` (`id`, `name`, `command`, `args`, `cwd`, `is_persistent`, `options`).
- Enhanced `hydrate_project` to parse `[scripts.persistent.<id>]` and `[scripts.task.<id>]` directly from `rula.toml`, while retaining backward compatibility by mapping legacy services into `scripts`.

### B. Execution Engine (`src-tauri/src/kopeng/runner.rs`)
- Implemented `run_project_task(input: RunTaskInput) -> Result<TaskExecutionRecord, String>`:
  1. Resolves dynamic flags from selected option values against `rula.toml`.
  2. Inserts an initial running execution record into SQLite table `task_executions`.
  3. Spawns `tokio::process::Command` in the project directory, capturing stdout, stderr, and measuring execution time via `Instant::now()`.
  4. Stores duration, exit code, markdown summary, and output logs in SQLite.
  5. Returns the updated execution record directly to the frontend.

### C. UI Integration (`ScriptsSection.vue` & `TaskReportModal.vue`)
- Replaced `ServicesSection.vue` with `ScriptsSection.vue`:
  - **Persistent Group:** Dev servers / daemons with Start/Stop controls, status indicators, and options drawers.
  - **Tasks Group:** Deterministic tasks with symbolic `[ ▶ ]` execution buttons, inline spinner, and execution result badges (`[ ✓ 1.2s ]`).
  - **Context Menus:** Right-click context menus on all script items.
- Built `TaskReportModal.vue`:
  - Visualizes past executions from SQLite `task_executions`.
  - Detailed view with tabs for Raw Log (with clipboard copy button) and Markdown Summary.
