# Visual Web Builder: Explicit Preview Reload Lifecycle & React Self-Healing Architecture

**Date:** 2026-09-23  
**Status:** Completed  
**Domain:** Visual Web Builder / Client Bridge Protocol / Host UI  

---

## 1. Problem Statement & Root Cause Analysis

When editing text or executing the **"Aplanar / Limpiar"** action in the Visual Web Builder canvas, the preview iframe turned blank white and failed to reload:

1. **Virtual DOM / Fiber Tree Corruption:**  
   In `visual-builder-client.ts`, `flattenBlockToRootStyle` previously mutated the DOM directly via `el.textContent = plainText`. In React applications, direct child DOM destruction invalidates React's Fiber node references.
2. **Reconciliation Crash on Vite Fast Refresh:**  
   When the Tauri AST engine wrote changes to disk (`Hero.tsx`), Vite pushed an HMR Fast Refresh update. React attempted to reconcile its updated component tree with the mutilated DOM, throwing fatal unhandled exceptions (`NotFoundError: Failed to execute 'removeChild' on 'Node'`).
3. **Blank Preview & Lack of Explicit Reload:**  
   Without an Error Boundary wrapping the root, the unhandled error unmounted the whole React application (`<div id="root"></div>`), rendering a dead white screen. Furthermore, `ProjectVisualBuilderTab.vue` did not notify the iframe or execute a clean preview reload after text mutations.

---

## 2. Architectural Remediation & Implementation Details

### A. Dual-Phase Deterministic Preview Reload Lifecycle
- **Clean Bridge Reload (Phase 1):**  
  Added `reloadPreview()` to `useOnlookClientBridge.ts` dispatching `{ type: 'vb-reload-preview' }`. The client bridge executes `window.location.reload()`, performing a native, seamless browser page reload that mounts React cleanly with fresh DOM and zero fiber desync.
- **Deterministic Timeout Fallback (Phase 2):**  
  Implemented `triggerExplicitReload(reason)` in `ProjectVisualBuilderTab.vue` with a bounded fallback timer (`RELOAD_FALLBACK_TIMEOUT_MS = 1200ms`). If the iframe is completely crashed or frozen and fails to emit `vb-client-ready` within 1200ms, Vue forces iframe element recreation via `iframeKey.value++`.
- **Automatic State & Selection Resumption:**  
  When `vb-client-ready` is received from the reloaded iframe:
  - `isReloading` is cleared and `isPreviewSynced` is briefly displayed (`1500ms`).
  - Active inspect mode and project theme colors are immediately synchronized.
  - If an element was selected prior to reload (`selectedNode.value?.onlookId`), `clientBridge.selectElementByOnlookId(...)` automatically re-selects the element in the reloaded DOM.

### B. Self-Healing Runtime Error Boundary in Client Bridge
- **Global Error Trap (`visual-builder-client.ts` & `templates/visual-builder-client.ts`):**  
  Added a global runtime error listener inside the client script. If a React reconciler exception (`removeChild`, `Minified React error`, `NotFoundError`) or an empty unmounted root container is detected, the bridge emits `vb-preview-crashed` to the host and automatically recovers via `window.location.reload()`.

### C. Explicit UI Observability & Brutalist Status Feedback
- **Canvas Top Bar Indicators:**  
  Added real-time status pills directly adjacent to the interactive URL input:
  - `[RELOADING PREVIEW...]` (accent background, spinning icon).
  - `[SAVING...]` (neutral pill, pulsing accent text).
  - `[SYNCED]` (semantic status-ok badge with checkmark).
- **Reload Action Button:**  
  The manual reload button in the canvas header now calls `triggerExplicitReload('Manual reload button')` and renders an active spinning animation while `isReloading` is active.
- **Mutation Integration:**  
  All AST and file mutations (`vb-element-text-updated`, Undo / Redo snapshots, image source updates, theme color edits) now explicitly invoke `triggerExplicitReload(...)`.

### D. Zero-Shift Decoupled Toolbar Layout
- **Absolute Center Anchoring:**  
  The central toolset (mode switcher, viewport buttons, undo/redo controls) is anchored using `absolute left-1/2 -translate-x-1/2 pointer-events-auto` inside the `relative` top toolbar container.  
  Decouples the center from the Left and Right containers, completely eliminating horizontal layout jitter when status pills (`[SAVING...]`, `[RELOADING...]`, `[SYNCED]`) appear and disappear.
- **Left Container Constraint:**  
  Left container is bounded with `max-w-[calc(50%-190px)] min-w-0` to prevent any overlap on narrow screens.

### E. Cold-Boot TCP Port Readiness Probe & Canvas Initialization Overlay
- **TCP Socket Readiness Probe:**  
  In `launch_visual_builder` (`visual_builder.rs`), added a bounded TCP probe (`tokio::net::TcpStream::connect(("127.0.0.1", detected_port))`, `tcp_probe_timeout_ms = 3000ms`, `tcp_poll_interval_ms = 200ms`) verifying the dev server is listening before returning `success: true`.
- **Canvas Initialization Overlay:**  
  Added `isCanvasReady` state in `ProjectVisualBuilderTab.vue`. An explicit brutalist overlay renders over the canvas while `devServerStatus === 'running' && !isCanvasReady`, eliminating cold-boot blank white frames.
- **Automated Connection Retry Poller:**  
  If `vb-client-ready` is not received within `1500ms`, the host automatically triggers bounded reload sync probes (up to `MAX_CONNECT_ATTEMPTS_COUNT = 5`), ensuring automatic recovery without manual reloads or tab switching.

---

## 3. Verification & Compliance
- `npx tsc --noEmit` in `estudiorc/frontend`: 0 errors.
- `npm run build` (`vue-tsc --noEmit && vite build`) in `wang-kapawu`: built cleanly in 6.56s with 0 errors.
- `cargo check` in `src-tauri`: built in 3.74s with 0 errors.

