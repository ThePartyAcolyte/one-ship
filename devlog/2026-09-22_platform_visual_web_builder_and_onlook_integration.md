# Development Log: 2026-09-22 - Platform-Native Visual Web Builder & Onlook Integration

## 1. Context & Motivation
Modern web application development frequently suffers from friction between code-first architecture and visual user experience design. While AI coding agents excel at backend architecture, data contracts, and component composition, fine-tuning visual layouts through iterative text prompts is token-inefficient and cognitively disjointed.

Traditional visual builders (such as Webflow or Framer) introduce vendor lock-in, proprietary cloud databases, and rigid, bloated code exports that disrupt autonomous agent workflows.

To resolve this, Specification 27 was established to integrate a **Platform-Native Visual Web Builder** into Wang-Kapawu, built around an open-source, code-first bi-directional stack (**React 19, Tailwind CSS, Motion/Framer Motion, and Onlook**).

## 2. Architecture & Changes Implemented

### 2.1. Formal Specification 27 (`docs/specs/27_visual_web_builder_and_onlook_stack_integration.md`)
- Formalized architectural boundaries: the visual builder is an internal subsystem of Wang-Kapawu (not an arbitrary project-level script in `rula.toml`).
- Defined the mandatory development server contract: `rula.toml` must declare a persistent dev server script (`[scripts.persistent.frontend]`), with automated validation and programmatic/agentic self-healing guard clauses.
- Defined greenfield and brownfield lifecycles and webview container extensions (Tauri desktop and Capacitor mobile).

### 2.2. Database & Project Registry (`src-tauri/src/db.rs`, `src-tauri/src/registry.rs`, `src-tauri/src/hydration.rs`)
- SQLite migration: Added `has_web_builder INTEGER NOT NULL DEFAULT 0` and `web_builder_stack TEXT` columns to `projects` table with safe non-destructive migration.
- Updated `ProjectRecord`, `ProjectEntry`, and `HydratedProject` structures to propagate `has_web_builder` and `web_builder_stack`.
- Added helper `db_update_project_web_builder` for toggling visual builder capability.

### 2.3. Greenfield IPC & Initializer Prompt (`src-tauri/src/kopeng/initializer.rs`, `templates/prompts/greenfield_initializer.md`)
- Updated `create_greenfield_project` command signature to accept `enable_web_builder: Option<bool>`.
- When active, persists `has_web_builder = true` and updates project metadata.
- Injected `WEB_BUILDER_DIRECTIVE` into the greenfield agent's prompt to mandate scaffolding React 19, Tailwind CSS, Motion, and Onlook AST instrumentation during Phase C, and registering `[scripts.persistent.frontend]` during Phase D.

### 2.4. Brownfield Agentic Evaluation & Flag Resolver (`templates/prompts/flag_resolve_web_builder.md`, `flag_resolve_dev_server.md`, `src-tauri/src/kopeng/flag_resolver.rs`)
- Created `flag_resolve_web_builder.md` for guided multi-turn migration of legacy websites to the modern visual builder stack.
- Created `flag_resolve_dev_server.md` for configuring missing dev server scripts in `rula.toml`.
- Updated `launch_flag_resolution_agent` to route `web_builder` and `no_dev_server` flags to their respective dedicated resolution agents and templates.

### 2.5. Platform Visual Builder Subsystem & Supervisor Guard (`src-tauri/src/kopeng/visual_builder.rs`, `src-tauri/src/lib.rs`)
- Created `visual_builder.rs` containing `launch_visual_builder`:
  - Validates `rula.toml` for `[scripts.persistent.frontend]`.
  - Checks live process status in `ProcessSupervisor`.
  - Auto-boots stopped dev servers and awaits HTTP availability with deterministic polling (`DEV_SERVER_BOOT_TIMEOUT_MS = 15000`).
  - Returns `VisualBuilderStatus` connecting to live dev server.
- Registered `launch_visual_builder` command in `src-tauri/src/lib.rs`.

### 2.6. User Interface Components (`src/components/modals/AddProjectModal.vue`, `AppInitBadge.vue`, `CardHeader.vue`)
- `AddProjectModal.vue`: Added brutalist `AppToggle` for "Web Frontend (Visual Builder)" in the "New Project" tab.
- `AppInitBadge.vue`: Added visual badge styles for `[! INIT: WEB_BUILDER]` (emerald) and `[! INIT: NO DEV_SERVER]` (yellow).
- `CardHeader.vue`: Added direct action button with simple functional tooltip `Visual Builder` that triggers `launch_visual_builder` and opens the live preview when `project.has_web_builder` is true.

### 2.7. Process Supervisor Compound Command & Port Detection Fixes
- `src-tauri/src/supervisor.rs`:
  - Updated `spawn_service` and `spawn_task` to detect compound commands (e.g. `npm run dev`) and execute them via `/bin/sh -c "<command> <args>"` instead of treating multi-word strings as standalone binaries.
- `src-tauri/src/kopeng/visual_builder.rs`:
  - Added dynamic dev port detection: checks `port` attribute in `rula.toml` or inspects `vite.config.*` to default to `5173`, otherwise falling back to `3000`.
- `src/components/workspace/ProjectVisualBuilderTab.vue`:
  - Made the URL indicator an interactive, editable text input with Enter/blur reload triggers.

### 2.8. Platform-Managed Onlook Studio Integration & Native Proposal Archival
- Created [`docs/specs/28_native_ast_visual_studio_proposal.md`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/docs/specs/28_native_ast_visual_studio_proposal.md) archiving the complete architecture and research for the in-tab native AST Webflow-style studio.
- `src-tauri/src/kopeng/visual_builder.rs`:
  - Added platform binary path management (`get_onlook_bin_path` -> `~/.local/share/wang-kapawu/bin/Onlook.AppImage`).
  - Added IPC command `check_onlook_installed` to verify binary presence and executable permissions.
  - Added IPC command `download_onlook_binary` to download official `Onlook-x86_64.AppImage` (v0.2.32) directly from GitHub releases and mark executable (`0o755`).
  - Added IPC command `launch_onlook_studio` to auto-ensure the project's dev server is alive, then spawn Onlook Desktop targeting `project_path`.
- `src-tauri/src/lib.rs`: Registered `check_onlook_installed`, `download_onlook_binary`, and `launch_onlook_studio`.
- `src/components/workspace/ProjectVisualBuilderTab.vue`:
  - Added brutalist toolbar button `Open Studio` that checks installation, triggers on-demand download if missing, and spawns the studio.
  - Added error alert banner above canvas for immediate feedback on launch or download failures.

### 2.9. Native In-Tab Webflow/Tailwind Visual Studio & Telemetry Fixes (Spec 28 Implemented)
- **DuckDB Telemetry Log Spam Resolution (`src-tauri/src/duck.rs`, `src-tauri/src/telemetry.rs`):**
  - Added `is_duckdb_available()` helper checking `DUCK_POOL`.
  - Guarded continuous profiling sample insertion and pruning in `telemetry.rs` to only run when the pool is active, completely eliminating the 500ms warning log spam.
- **Client DOM Bridge (`estudiorc/frontend/src/visual-builder-client.ts`, `src/main.tsx`):**
  - Implemented client bridge script running strictly in `DEV` mode.
  - Added hover bounding box and selection overlay targeting elements with `[data-onlook-id]`.
  - Communicates bi-directionally with Wang-Kapawu via `window.parent.postMessage` (`vb-element-selected`, `vb-preview-classes`, `vb-set-inspect-mode`).
- **Rust AST Mutation Engine (`src-tauri/src/kopeng/visual_builder.rs`, `src-tauri/src/lib.rs`):**
  - Implemented `update_jsx_element_classes`: reads coordinates from decoded `data-onlook-id` and atomically rewrites `className="..."` on disk.
  - Implemented `update_jsx_element_text`: atomically updates text node content in source files.
  - Registered commands in Tauri IPC handler.
- **Webflow-Style Visual Studio Interface (`src/components/workspace/ProjectVisualBuilderTab.vue`):**
  - Added Mode Switcher: `Interact` (normal navigation) vs `Inspect` (visual selection).
  - Built full Style Inspector sidebar:
    - Selected component header with file path, line number, and tag name.
    - Box Model Spacing Widget: interactive margin and padding controls (`p-*`, `m-*`).
    - Layout & Flexbox: flex direction, alignment, justification, and gap buttons.
    - Typography: font sizes (`text-xs` to `text-6xl`), weights, and text colors.
    - Appearance & Radius: border radius (`rounded-*`) and shadow presets.
    - Direct Tailwind class editor with instantaneous live DOM preview and disk persistence.
    - Text editor with "Save Text" action.
    - Agent Co-Pilot action opening the floating chat overlay with component context.

### 2.10. AST Engine Robustification, Linked Dimension Locks & Dev Server Diagnostics
- **Corrupted Token & Duplicate Attribute Bug Resolution:**
  - Diagnosed that static character-column slicing in `update_jsx_element_classes` caused coordinate drift after the first file edit, producing duplicate `className="..."` attributes and trailing orphaned tokens (`>\n );\n};`) that crashed Vite/esbuild with "Unexpected token" errors.
  - Rewrote AST mutation engine in `src-tauri/src/kopeng/visual_builder.rs`:
    - Implemented `find_opening_tag_bounds` and `strip_and_replace_classname` to locate the opening tag `<tag ... >` bounded by quotes and brace depth.
    - Strips all existing and duplicated `className="..."` occurrences and inserts a single canonical `className="{new_classes}"`.
    - 100% immune to character drift, line shifting, or trailing file corruptions.
- **Linked Dimension Locks (`ProjectVisualBuilderTab.vue`):**
  - Added horizontal and vertical lock toggles (`Link` / `Link2Off`) for Margin (`mx-*`, `my-*`) and Padding (`px-*`, `py-*`).
  - Editing any linked side automatically updates both sides in tandem with symmetrical Tailwind tokens.
  - Automatically activates locks when initial values are identical.
- **Centering & Direct Canvas Manipulation (`visual-builder-client.ts`):**
  - Added 8 resize handles (`N`, `S`, `E`, `W`, `NE`, `NW`, `SE`, `SW`).
  - Added support for `Alt` key to trigger symmetrical scaling from the center (`factor = 2`).
  - Added automatic symmetrical scaling when an element is detected as centered (e.g. `mx-auto`, parent `text-center`, or centered flex).
  - Automatically appends `mx-auto` when resizing block elements inside centered containers so they remain centered instead of growing exclusively to the right.
  - Added a dedicated "Center Horizontally (`mx-auto`)" quick-action button in the sidebar.
- **Live Vite Dev Server Output Drawer:**
  - Added a collapsible "Server Logs" drawer at the bottom of the canvas stage.
  - Streams real-time stdout/stderr from the supervised `frontend` service via `read_process_output_cmd`.
  - Automatically flags compilation and syntax errors (`hasServerError`) with a glowing badge in the toolbar and colored log traces.

## 3. Verification & Validation
- Cleaned corrupted test files in `estudiorc/frontend` (`Hero.tsx`, `TriageChat.tsx`, `BookingSection.tsx`).
- TypeScript verification in target project: `npx tsc --noEmit` exited with code 0 (0 errors).
- Rust compilation: `cargo check` in `src-tauri` passed with 0 errors (0.64s).
- Frontend compilation: `npm run build` (`vue-tsc --noEmit && vite build`) passed with 0 errors (4.28s).




