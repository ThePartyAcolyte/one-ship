# Development Log: Orphan Process Remediation & Chat Close-to-Kill Action

**Date:** 2026-09-23  
**Topic:** Linux Process Group Isolation, Kernel Death Signal (`PR_SET_PDEATHSIG`), Supervisor Teardown Hardening & Agent Window Termination

---

## 1. Problem Statement

During development sessions involving multiple restarts and visual builder iterations, system RAM consumption progressively increased throughout the day. Investigation confirmed four root causes:
1. **Unmanaged Grandchildren:** Sub-processes spawned by service shells or agent binaries (such as `npm run dev` spawning `node`, which in turn spawned `vite` and `esbuild`) did not have their own process groups. When the root PID was terminated, sub-processes were reparented to init (PID 1) and remained alive indefinitely.
2. **Missing Kernel Death Signal:** Processes lacked `PR_SET_PDEATHSIG`, allowing child processes to outlive abrupt daemon or desktop process terminations (e.g. crashes, SIGKILL).
3. **Fire-and-Forget Visual Builder:** Onlook Studio AppImage was launched via an unmanaged `tokio::process::Command` whose handle was dropped immediately, leaving it untracked.
4. **Non-Terminating Chat Close Action:** In the floating chat overlay and taskbar tabs, the "Close" button (`X`) merely toggled the overlay window visibility (`closeFloatingChat()`), leaving the underlying agent process running in the background.

---

## 2. Changes Implemented

### 2.1 Backend Concurrency & Process Lifecycle (`src-tauri/`)
- **`Cargo.toml`:** Added `libc = "0.2"` dependency.
- **`src-tauri/src/supervisor.rs`:**
  - Configured `setpgid(0, 0)` in `pre_exec` for `spawn_service`, `spawn_task`, and `spawn_agent` to ensure every spawned process tree resides in its own isolated process group.
  - Configured `libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0)` in `pre_exec` so the Linux kernel automatically kills all children if the supervisor parent dies.
  - Implemented `kill_process_and_group(pid, libc::SIGKILL)` to send `SIGKILL` directly to `-(pid as i32)` (entire process group) and `pid` (direct fallback) on process cancellation and `stop_process`.
  - Added `stop_all_processes(&self)` to cleanly terminate all running processes across projects.
  - Hardened `stop_agent` to properly resolve the system working directory when targeting the platform `system` agent.
- **`src-tauri/src/bin/daemon.rs`:**
  - Added `SIGINT` (Ctrl+C) and `SIGTERM` signal handlers invoking `supervisor.stop_all_processes().await` before shutting down Axum.
- **`src-tauri/src/lib.rs`:**
  - Added `tauri::RunEvent::Exit` handler in desktop app builder executing `supervisor.stop_all_processes().await`.
- **`src-tauri/src/kopeng/visual_builder.rs`:**
  - Replaced unmanaged fire-and-forget launch of Onlook Studio with `sup.spawn_service(..., "onlook_studio", ...)`, registering it directly under the ProcessSupervisor.

### 2.2 Frontend UI & Stores (`src/`)
- **`src/stores/dialogueStore.ts`:**
  - Added `closeAndKillAgent(projectPath?: string)` action to close the chat overlay and synchronously dispatch `stop_agent` to terminate the agent's runner process.
- **`src/components/dialogue/FloatingChatOverlay.vue`:**
  - Updated window header close button (`X`) to invoke `dialogueStore.closeAndKillAgent(activeProjectPath)`.
  - Updated tooltip to `"Close & Stop Agent"`.
- **`src/components/taskbar/AgentTaskbar.vue`:**
  - Updated `handleCloseTab` to invoke `stop_agent` on tab close button click, cleaning up the agent process.

### 2.3 Living Specifications (`docs/specs/`)
- **`docs/specs/08_native_process_supervisor.md`:**
  - Updated Section 2.3 to document process group isolation (`setpgid(0, 0)`), kernel death signals (`PR_SET_PDEATHSIG`), whole-tree termination (`kill(-pgid, SIGKILL)`), and application shutdown drain hooks.

---

## 3. Verification

- Ran `cargo check --manifest-path src-tauri/Cargo.toml`: Success (0 warnings, 0 errors).
- Ran `npx vue-tsc --noEmit`: Success (0 errors).
- Ran `npm run build`: Success (0 errors, dist bundle compiled cleanly).
