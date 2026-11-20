# Devlog: Visual Builder Client Bridge Auto-Sync & Animation-Aware Overlay Tracking

**Date:** 2026-11-12
**Target:** `src-tauri/src/kopeng/visual_builder.rs`, `templates/visual-builder-client.ts`, `docs/specs/27_visual_web_builder_and_onlook_stack_integration.md`

## Problem
Projects using the Visual Builder (such as `Kirulab Website`) failed to clear the canvas initialization overlay (`Canvas Preview Unavailable` after 5 sync attempts) even when the Vite dev server was running and rendering the page beneath the overlay. Investigation showed:
1. `templates/visual-builder-client.ts` was never copied or imported into target project `src/` directories by `launch_visual_builder` or project initialization, so `vb-client-ready` was never emitted.
2. `templates/visual-builder-client.ts` only repositioned hover/selection overlays on `scroll` and `resize`, causing overlays to drift on elements animated via `motion/react` or CSS transforms.

## Changes
- **`src-tauri/src/kopeng/visual_builder.rs`:** Added `ensure_client_bridge(config: &VbConfig)` invoked during `launch_visual_builder` to synchronize `<frontend_root>/src/visual-builder-client.ts` from `templates/visual-builder-client.ts` and idempotently prepend `import './visual-builder-client';` to the frontend entry file (`main.tsx`, `main.ts`, `index.tsx`, `index.ts`). Added unit test `ensure_client_bridge_writes_bridge_and_entry_import`.
- **`templates/visual-builder-client.ts`:** Added `scheduleOverlaySyncLoop()` using `requestAnimationFrame` gated on `isInspectMode && !isInlineEditing && !isResizing && (selectedEl || hoveredEl)` with a `0.5px` rect delta guard and `isConnected` cleanup, plus `export {};` for `isolatedModules` compatibility.
- **`docs/specs/27_visual_web_builder_and_onlook_stack_integration.md`:** Documented §13.5 covering automatic client bridge synchronization and animation-aware overlay tracking.
- **`Kirulab Website`:** Synced `frontend/src/visual-builder-client.ts`, imported it in `frontend/src/main.tsx`, and verified `npm run typecheck`.
