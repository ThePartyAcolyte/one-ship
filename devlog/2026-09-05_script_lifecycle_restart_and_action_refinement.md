# Devlog: Script Lifecycle Refactor, Resilient Multi-Signal Restart & Streamlined Actions

**Date:** 2026-09-05  
**Author:** Frontend (UI/UX and Brutalist Component Specialist)  
**Topic:** Persistent Script Restart Mechanism, Stubborn Process Signal Retries, and Action Bar Clutter Elimination

---

## 1. Context & Motivation

During operations involving multi-process daemon scripts (e.g. Odoo servers, Python worker pools), terminating processes via terminal interrupts (`Ctrl+C`) often left foreground processes occupying the PTY terminal. The process would trap the initial SIGINT but require a secondary or tertiary interrupt to force termination and liberate the shell prompt.

Furthermore, script cards in [`ScriptsSection.vue`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/kopeng/services/components/ScriptsSection.vue) suffered from layout clutter due to a dedicated configuration gear button (`Settings2`). Clicking the Play button on persistent services bypassed the configuration modal even when options were declared, leading to accidental launches with stale or default options.

---

## 2. Implementation Details

### A. Action Bar Clutter Elimination
- Removed the standalone configuration gear button (`Settings2`) from both Persistent Services and Deterministic Tasks.
- Card actions now adhere to a clean binary model:
  - **Stopped State:** Exactly 1 button (`Play`).
  - **Running State (Persistent):** Exactly 2 buttons (`RotateCw` Restart + `Square` Stop).
- Retained `"Configure Options"` and `"Configure Flags"` in the right-click [`AppContextMenu`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/ui/AppContextMenu.vue) for advanced management without polluting card viewports.

### B. Auto-Launch Modal Workflow
- Unified `handleRunOrLaunch(script)`:
  - When starting a script (service or task) that declares options (`script.options?.length > 0`), clicking Play or the title card automatically opens [`ScriptLaunchModal.vue`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/modals/ScriptLaunchModal.vue).
  - The modal allows reviewing/adjusting flags and parameters before confirming with "Launch".
  - Scripts without options launch immediately.

### C. Resilient Multi-Signal Stop & Restart State Machine
Implemented `executeStopWithRetry(svc, onSuccess, onFailure)` in [`ScriptsSection.vue`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/kopeng/services/components/ScriptsSection.vue):
1. Dispatches an initial `stop_service` (`Ctrl+C`).
2. Enters a reactive `serviceTransitions` state (`mode: 'stopping' | 'restarting'`, `attempt: 1`, `maxAttempts: 3`).
3. Polls `isServiceRunning(project.id, svc.id)` every 2000ms:
   - If the process liberates the terminal (`!isServiceRunning`), clears the transition and triggers `start_service` (if restarting) or completes stop.
   - If the process is still detected in foreground after 2.0s, resends `stop_service` and logs a warning to [`coreStore.addStatusLog`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/stores/coreStore.ts).
   - If the process fails to terminate after 3 attempts (~6s total), logs an error to the status log and aborts restart to prevent running into a dirty terminal.
4. Added visual transition feedback:
   - Restart button displays spinning `RotateCw` with dynamic tooltip: `"Restarting service (Attempt X/3)..."`.
   - Stop button displays spinning `Loader2` with dynamic tooltip: `"Stopping service (Attempt X/3)..."`.
   - Action buttons on transitioning cards are disabled to prevent double-clicks.
5. Added `"Restart Service"` to the right-click context menu.

---

## 3. Verification

- **Production Build:** Ran `npm run build` (`vue-tsc --noEmit && vite build`) — compiled cleanly with 0 TypeScript or template errors in 2.19s.
