# 2026-11-17 — Standardized Modal Overlay with Backdrop Blur

## Summary

Fixed the long-standing "modal blur does not render" issue by identifying two independent root causes: (1) WebKitGTK ignores CSS variables inside `backdrop-filter`, which makes every Tailwind v4 `backdrop-blur-*` utility a no-op, and (2) the development launcher disabled accelerated compositing with `WEBKIT_DISABLE_DMABUF_RENDERER=1`, while `backdrop-filter` is a compositor-only effect. Every modal/blocking overlay now uses a single `.ui-overlay-backdrop` class with literal `blur(12px)` values, the launcher no longer disables the compositor, and the renderer-mode policy for future packaging was documented (implementation deferred).

## Root Cause Analysis

1. **Tailwind v4 emits variable-based filters.** `backdrop-blur-*` compiles to `--tw-backdrop-blur: blur(Npx)` plus `-webkit-backdrop-filter: var(--tw-backdrop-blur,) ...` and the unprefixed equivalent. WebKit bug [#289800](https://bugs.webkit.org/show_bug.cgi?id=289800) (`-webkit-backdrop-filter ignores css variables`, status NEW) ignores the declaration entirely, so all 31 `backdrop-blur-*` usages in the repository rendered nothing on `webkit2gtk-4.1 2.54.1-1`. Literal values (`blur(12px)`) work.
2. **The previous fallback used the wrong primitive.** `filter: blur(...)` on the application tree is a paint filter: it re-rasterizes the whole subtree on every invalidation, so scrolling inside a modal re-blurred the entire viewport each frame. It also creates a containing block that breaks `position: fixed` descendants.
3. **WebKitGTK 2.54 layer leak.** WebKit bug [#328065](https://bugs.webkit.org/show_bug.cgi?id=328065) grows memory proportionally to the number of simultaneous `backdrop-filter` layers (~10-14 stable, multi-gigabyte growth from ~18-22). The solution therefore mounts one blurred backdrop per open modal (expected nesting of at most two) plus at most one per initializing project card, and forbids blurred backdrops per row, HUD chip, or repeated list item.

## Changes Implemented

### 1. Shared Overlay Contract (`src/styles/ui.css`)

```css
.ui-overlay-backdrop {
  background-color: rgba(0, 0, 0, 0.45);
  -webkit-backdrop-filter: blur(12px);
  backdrop-filter: blur(12px);
}
```

- Literal values only (no `var()`, no `calc()`), per WebKit bug #289800.
- Scrim reduced from `0.55` to `0.45` (and from `0.70`-`0.85` on ad-hoc overlays) so the blur is perceptible.
- Documented the invariants inline: single blurred layer, no paint filters on the application tree, no Tailwind backdrop utilities.

### 2. All Modal Overlays Standardized

Replaced ad-hoc `bg-black/{70,75,80,85}` + inert `backdrop-blur-*` recipes with `.ui-overlay-backdrop` across 31 overlays in 30 components:

- `src/components/modals/`: `ConfirmDeleteModal`, `SwitchConfirmationModal`, `ScriptLaunchModal`, `RemovalFailedModal`, `ApprovalRequestModal`, `DocstyleSelectionModal`, `TaskReportModal`, `ProjectInitModal`, `AddProjectModal`, `AddPersonaModal`, `SettingsModal`, `RemoteVcsModal`, `GitBranchModal`, `CreateProjectTaskModal`, plus the previously standardized `OmnisearchModal` and `AboutModal`.
- `src/components/dialogue/`: `ConversationSearchModal`, `ContextTransferModal`.
- `src/components/workspace/`: `ProjectWorkspace` (unsaved-changes and batch-delete dialogs), `ProjectGitSnapshotsTab`, `ProjectTerminalGrid`, `ProjectElectronicsTab`, `ProjectCadTab`, `ProjectPcbTab`, `ProjectOverviewTab` (blocking init overlay), `ProjectVerticalRail` (drawer backdrop, previously already using the class).
- `src/components/workspace/electronics/OscilloscopeView.vue`: calibration modal.
- `src/components/ProjectCard/ProjectCard.vue`: blocking init overlay.
- `src/components/ui/AppExecutionConfigPicker.vue`: execution config dialog (absolute inside the chat viewport or fixed full-screen).
- `src/components/floating/RollingStatusLog.vue`: already used the class.

### 3. Dead Utility Cleanup

Removed the inert `backdrop-blur-*` classes from non-overlay decorative surfaces where they never rendered: `AgentTaskbar`, the workspace diff panel and right inspector, and the oscilloscope HUD chips/status overlay. No visual change was expected; the classes were dead code.

### 4. Development Launcher: Accelerated Compositing Restored (`scripts/run_dev.sh`)

After the CSS fix, the blur still did not render. Runtime inspection showed the web process had no compositing thread, and `WebKitWebProcess` was running on the non-accelerated software path.

- `scripts/run_dev.sh` passed `WEBKIT_DISABLE_DMABUF_RENDERER=1` to the GUI systemd unit.
- In WebKitGTK 2.54.1, `AcceleratedBackingStore::rendererBufferTransportMode()` returns an empty transport set for any truthy value of that variable; `canUseHardwareAcceleration()` then returns false and the web process never enters accelerated compositing.
- `backdrop-filter` is a compositor-only effect: the software path silently ignores it. This also explains why the previous `filter: blur(...)` attempt "worked" (it is a CPU paint filter) while being slow.
- The host has a single Intel Iris Xe GPU (`/dev/dri/card1`, `renderD128`) with Mesa and no NVIDIA PCI device, so the workaround was unnecessary.

The `--setenv=WEBKIT_DISABLE_DMABUF_RENDERER=1` line was removed and replaced by an explanatory comment. If a DMA-BUF regression ever appears, `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1` must be used instead, because it keeps accelerated compositing over shared-memory transport.

### 5. Living Documentation

- `docs/specs/13_design_system_and_ui_components.md`: rewrote the `.ui-overlay-backdrop` bullet and added §3.9 **Overlay & Backdrop Blur Contract (WebKitGTK)** with the literal-value, accelerated-compositing, bounded-layer, zero-paint-filter, non-opaque-scrim, and prohibited-utilities rules.
- `docs/specs/12_streamable_http_mcp_daemon.md`: added §7.4 **Graphics Renderer Mode & Packaging Policy (PENDING — packaging phase)** with the mode table, `auto` ladder, restart semantics, and installer constraints.
- `TECHNICAL_DEBT.md`: added item **#37** documenting WebKit bugs #289800 and #328065, the disabled-compositing root cause, the active workaround, remaining risk, revisit conditions, and the packaging follow-up.
- `TODO.md`: added the modal overlay runtime checks and the deferred packaging tasks for `[system] webkit_renderer_mode`.

### 6. Renderer Mode Policy for Packaging (Documented Only)

The renderer transport was promoted from an incidental environment flag to a documented runtime policy because it determines whether `backdrop-filter` (and other composited effects) work:

| `[system] webkit_renderer_mode` | Environment | Compositing | Blur |
|---|---|---|---|
| `dmabuf` (default) | none | yes | yes |
| `shared_memory` | `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1` | yes | yes |
| `software` | `WEBKIT_DISABLE_DMABUF_RENDERER=1` | no | no |

Decisions recorded:
- `shared_memory` is the NVIDIA/known-bad-driver fallback because it preserves accelerated compositing; `software` (the `DISABLE_DMABUF` flag) is the last resort and loses the blur.
- The mode must be resolved in `one-ship`'s `main()` before `tauri::Builder` creates the webview, from a global-only `[system] webkit_renderer_mode` key. It cannot be a daemon command because the webview exists before the frontend can call the daemon.
- WebKitGTK latches the transport set in a process-wide `std::call_once`, so a change requires restarting the interface process. Webview reload or window reopen is not enough; the daemon and supervised processes stay running.
- Installers may seed the initial value from GPU/driver detection, but cannot decide reliably at build time: the active EGL vendor, driver version, hybrid-GPU selection, sandbox access to `/dev/dri`, and distro patches all matter. Debian/Ubuntu's `disable-nvidia-dmabuf.patch` empties the transport set on NVIDIA regardless of the environment.
- Implementation of the setting, the `auto` heuristic, and the launcher retry is deferred to the packaging phase (TODO.md). No runtime code was added in this session beyond the launcher fix.

## Validation Required

This session intentionally did not start or restart any runtime process. Restart only the interface so the launcher change takes effect (the daemon and its agents keep running):

```bash
./scripts/run_dev.sh stop gui
./scripts/run_dev.sh gui
```

Then verify via the active `scripts/run_dev.sh` watchers (Vite HMR):

1. Confirm the GUI journal has no `Disabled hardware acceleration because GTK failed to initialize GL` warning (`journalctl --user -u oneship-dev-gui -n 50 --no-pager`) and that `WebKitWebProcess` now runs on the composited path.
2. Open `About`, `Omnisearch` (`Ctrl+K`), `System Logs`, `Settings`, `Project Init`, and a workspace dialog (`Port Conflict`, `CAD/PCB create`, `Git snapshots add action`) and confirm the background is visibly blurred with the `0.45` scrim.
3. Scroll inside `SettingsModal`, `RollingStatusLog`, and Omnisearch results and confirm there is no scroll jank or repeated blur recomputation.
4. Open two stacked modals (e.g. Settings while an approval request is pending) and confirm both scrims render and the app remains responsive.
5. Open and close modals repeatedly while watching the WebKit web process RSS (`ps -o rss= -p <pid>`) to confirm no layer-leak growth pattern from WebKit bug #328065.

## Pending Decisions

- **Renderer mode setting:** implementation of `[system] webkit_renderer_mode`, the `auto` heuristic, the Settings selector, and the launcher retry is deferred to the packaging phase (explicit decision on 2026-11-17). Only the policy is documented in Spec 12 §7.4 and the tasks are staged in `TODO.md`.
- **Blur validation:** requires restarting the interface with the new launcher (`run_dev.sh stop gui` + `gui`); the daemon and supervised processes stay running.
- If the blur still does not render on WebKitGTK 2.54.1 with literal values and accelerated compositing enabled, fall back to a frozen window snapshot captured via `grim` and displayed as a static `<img>` with `filter: blur(12px)` (rasterized once, zero per-frame cost).
- Phase 3 (`AppModal.vue` shell with `Teleport to="body"`, a dedicated sibling backdrop layer, and a bounded-layer guard) remains deferred until the visual effect is confirmed.
