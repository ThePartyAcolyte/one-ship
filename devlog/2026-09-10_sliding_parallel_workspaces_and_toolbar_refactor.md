# Session Devlog: Sliding Parallel Workspaces, Unified Header Toolbar & Minimap Re-alignment

**Date:** 2026-09-10  
**Author:** Bosmang (Lead Systems Architect)  
**Collaborators:** Core Assistant (Antigravity)

---

## 1. Objectives Achieved

1. **Dual Parallel Workspace Architecture (Dashboard vs Agent Dialogue):**
   - Transformed the Agent Dialogue from an ephemeral floating modal into an equal first-class workspace (`activeWorkspace: 'dashboard' | 'dialogue'`).
   - Implemented full-height tactile **Peeker Rails** (`w-11` vertical strips):
     - **Chat Peeker (Dashboard mode)**: Displays live unread message count badges, rotated vertical label `AGENT DIALOGUE // CHAT`, and quick switch trigger (`Alt+2` / `»`).
     - **Dashboard Peeker (Dialogue mode)**: Displays active project count (`NP`), rotated vertical label `PROJECT DASHBOARD`, and quick switch trigger (`Alt+1` / `«`).
   - Added global workspace keyboard shortcuts: `Alt+1` (Dashboard), `Alt+2` (Dialogue), `Ctrl+Tab` (Toggle Workspace), and `Escape` (return from dialogue to dashboard).

2. **Unified Action Toolbar & Centered System Telemetry (`AppHeader.vue`):**
   - Restructured `AppHeader.vue` into a clean 3-zone layout:
     - **Left**: Brand `WANG-KAPAWU // HUB` with language/dialect toggle.
     - **Center**: Monospace system telemetry monitors (CPU, RAM, SWAP, DISK, NET) centered under the header.
     - **Right Toolbar**: Consolidated topbar actions:
       - Multi-Account Switcher (Google OAuth / Keyring).
       - Voice Mode Toggle (OpenDictate).
       - System Diagnostics Popover (Git, GH CLI, GitHub Auth, OpenDictate IPC).
       - Global Settings button (`config.toml`).

3. **Floating Components Cleanup (Zero Dead Code):**
   - Eliminated obsolete FAB floating components:
     - Removed `ChatFab.vue`, `SettingsFab.vue`, `VoiceFab.vue`, and `DiagnosticBubble.vue`.
     - Replaced `AgentDialogueOverlay.vue` with `AgentDialogueWorkspace.vue` dedicated workspace component.
   - Retained and cleanly docked `RollingStatusLog.vue` at the bottom.

4. **Project Minimap Left-Side Placement (`ProjectMinimap.vue`):**
   - Relocated the Project Dot Minimap to `left-2 top-1/2 -translate-y-1/2` with right-oriented brutalist tooltips (`position="right"`).

5. **Refined Visual-First Telemetry Suite (`TelemetryCharts.vue`):**
   - Eliminated layout jitter and dynamic width shifts by removing variable inline text strings for CPU, RAM, SWAP, and Disk.
   - **CPU Multi-Core Equalizer**: Array of vertical level bars proportional to individual core utilization (`cpu_cores`), simplified tooltip to `CPU: <pct> (<n> cores)`.
   - **RAM & SWAP Gauge**: Twin proportional vertical gauges (`RAM` and `SWP`) with subtle track backgrounds and threshold colors.
   - **Disk Gauge**: Single vertical gauge (`hidden lg:flex`) for root partition occupancy.
   - **Network Dual Sparkline + Fixed-Width Speed Readout**: Real-time 20-tick rolling SVG sparkline paired with a strictly fixed-width monospace tabular container (`w-[66px]`, `tabular-nums`) displaying 3-4 char formatted speed slots (`↓<rx> ↑<tx>`), eliminating layout shifting.
   - Standardized container styling to application surface theme (`bg-[var(--surface-color)]`, `border-[var(--border-color)]`), increased height to `h-7` (28px), and normalized pointer cursor to `cursor-default`.

6. **Responsive Adaptive Action Toolbar:**
   - Designed the right toolbar in `AppHeader.vue` with adaptive layout (`flex-col lg:flex-row items-end lg:items-center`):
     - **Extended View (≥1024px / Full Screen)**: Renders a single horizontal row (`[ Account Selector ] [ 🎙 Voice ] [ ⚡ Diagnostics ] [ ⚙ Settings ]`).
     - **Compact View (<1024px / Half-Screen & Split Views)**: Automatically stacks into 2 rows (Row 1: Account Selector, Row 2: Action buttons), preventing header overlap and preserving micro-chart visibility.

7. **WebKitGTK Tiling Window Manager Resize Bridge (`index.html` & `App.vue`):**
   - Fixed WebKitGTK viewport unit freeze on tiling resize (Hyprland / Omarchy / Sway) by establishing strict `height: 100%; width: 100%;` geometry from `html` -> `body` -> `#app` down to the root flex containers.
   - Attached native Tauri v2 window resize listener (`appWindow.onResized(...)`) to immediately dispatch `window.dispatchEvent(new Event('resize'))`, guaranteeing instantaneous re-flow and SVG sparkline re-scaling on every window tiling transition.

---

## 2. Verification
- `cargo check`: Finished in 0.45s with 0 errors.
- `npm run build`: Compiled with `vue-tsc --noEmit && vite build` in 1.63s with 0 errors.
- Verified Zero Dead Code: Deleted obsolete floating components and purged obsolete imports from `App.vue`.
- Clean responsive layout and instant tiling adaptation verified for split-screen viewports (~650px - 900px).
