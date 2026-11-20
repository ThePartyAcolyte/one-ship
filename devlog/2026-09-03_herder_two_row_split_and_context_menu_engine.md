# Work Session Log: Herder 2-Row Layout, Visibility Filters & Right-Click Context Menu Engine

**Date:** 2026-09-03  
**Status:** Completed  
**Branch:** master  

---

## 1. Objectives & Context
1. Restructure the Herder agents section within project cards into two distinct visual tiers:
   - **`PROJECT`:** Personas defined in the project's `rula.toml`.
   - **`INTERNAL`:** Core tools and background plugin specialists (e.g. `git`).
2. Add smart visibility toggles (`Eye` / `EyeOff`) to filter inactive/unlaunched agents while ensuring active agents remain permanently visible.
3. Build a global brutalist right-click context menu engine (`AppContextMenu.vue`) with boundary awareness and support for rapid agent actions (Focus, Restart, Close Tab, Delete Persona).

---

## 2. Key Accomplishments

### A. Backend Discriminator (`src-tauri/src/kopeng/herdr.rs`)
- Added `is_internal: bool` to `HerdrAgentState`.
- Marked `git` agent as `is_internal = true` and `rula.toml` agents as `is_internal = false`.
- Implemented `delete_agent_persona(project_path, agent_id)` to atomically excise agent personas from `rula.toml`.
- Implemented `close_herdr_tab(tab_id)` to terminate and clean up Herder tabs on demand.
- Registered both commands in `src-tauri/src/lib.rs`.

### B. Brutalist Context Menu Component (`src/components/ui/AppContextMenu.vue`)
- Created `AppContextMenu.vue` using Vue `Teleport` to body.
- Supports `ContextMenuItem` interface (`label`, `icon`, `action`, `danger`, `disabled`, `separator`).
- Viewport clamping prevents menus from extending past window borders.
- Automatically dismisses on outside pointer clicks or `Escape` keypresses.
- Exported in `src/components/ui/index.ts`.

### C. Herder 2-Row Layout & Visibility Governance (`HerderSection.vue`)
- Divided the agent container into `PROJECT` and `INTERNAL` rows, each with count badges `(visible/total)`.
- Added `Eye` / `EyeOff` filter buttons with `AppTooltip`:
  - When eye is off, only unlaunched agents are hidden.
  - Active agents (`working`, `idle`, `blocked`, `completed`) **always remain visible** to ensure unhindered process monitoring.
- Right-clicking any agent pill displays the contextual options:
  - Active: `Focus Session`, `Restart Agent`, `Close Tab`.
  - Unlaunched: `Launch <agent_id>`.
  - Project Personas: `Delete Persona` (removes from `rula.toml` and reloads state).

---

## 3. Verification
- Frontend build (`npm run build`) completed cleanly with 0 errors.
- Backend check (`cargo check`) succeeded with 0 errors.
- Dev server (`task-75`) hot-reloaded and verified operational.
