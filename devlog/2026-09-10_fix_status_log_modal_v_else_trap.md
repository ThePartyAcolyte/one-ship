# Devlog: 2026-09-10 — Fix Status Log Modal v-else Trap in Dialogue Workspace

---

## 1. Problem Description

When switching from the project dashboard to the Agent Dialogue workspace via the lateral rail (`» AGENT DIALOGUE // CHAT` or `Alt+2`), the application unexpectedly opened the full-screen expanded system log viewer modal (`fixed inset-4 z-50`), completely overlaying and masking the 3-column chat interface and preventing interaction with the conversation and artifact panels.

---

## 2. Root Cause Analysis

In `src/App.vue`:
```html
<RollingStatusLog :status-logs="statusLogs" :hide-collapsed="activeWorkspace === 'dialogue'" />
```

In `src/components/floating/RollingStatusLog.vue`:
```html
<div v-if="!isExpanded && !hideCollapsed" class="fixed bottom-4 left-4 right-20 z-40 flex items-end">
  <!-- Collapsed status ticker -->
</div>

<div 
  v-else
  class="fixed inset-4 z-50 flex flex-col bg-[var(--bg-color)] ... shadow-2xl"
>
  <!-- Full-screen expanded log history modal -->
</div>
```

When `activeWorkspace === 'dialogue'`, `hideCollapsed` evaluates to `true`.
The first branch `!isExpanded && !hideCollapsed` evaluated to `false`. Because the second branch was a plain `v-else` rather than `v-else-if="isExpanded"`, Vue immediately rendered the expanded modal even though `isExpanded` was `false`.

Furthermore, because `isExpanded` was `false`, clicking the close button (`toggleLog()`) set `isExpanded = true`, keeping the modal open, and clicking it again reset `isExpanded = false`, which still fell into `v-else`. The modal was permanently trapped open while in dialogue mode.

---

## 3. Implementation & Resolution

1. **`src/components/floating/RollingStatusLog.vue`:**
   - Changed line 377 from `v-else` to `v-else-if="isExpanded"`.
   - Added a captured `Escape` key event listener (`handleKeydown`) that intercepts `Escape` when `isExpanded` is true, dismissing the modal via `isExpanded = false` and calling `e.stopImmediatePropagation()` to prevent workspace switching.
2. **Behavioral Matrix:**
   - **Dashboard Workspace (`hideCollapsed: false`):**
     - `isExpanded === false`: Collapsed status ticker renders at `fixed bottom-4`.
     - `isExpanded === true`: Full-screen expanded log viewer renders (`z-50`).
   - **Dialogue Workspace (`hideCollapsed: true`):**
     - `isExpanded === false`: Neither component renders; the bottom area is completely clear for the chat input and docked Column 1 footer.
     - `isExpanded === true`: Full-screen expanded log viewer renders on explicit user request (e.g. from the agent list footer).
     - Pressing `Escape` or clicking `X` cleanly dismisses the modal.

---

## 4. Verification

- Frontend compilation: `npx vue-tsc --noEmit` passed with 0 errors.
- Hot module replacement (HMR) reloaded `RollingStatusLog.vue` in the running Vite instance.
