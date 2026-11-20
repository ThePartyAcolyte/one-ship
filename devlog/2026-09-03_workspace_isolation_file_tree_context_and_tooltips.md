# Work Session Log: Workspace Isolation, Context File Tree & Unified Tooltip Standardization

**Date:** 2026-09-03  
**Status:** Completed  
**Branch:** master  

---

## 1. Objectives & Context
Resolve cross-project Herder workspace false positives (such as an unrelated project claiming `w4` due to an ad-hoc terminal pane), introduce an interactive file tree component for visual selection of persona `context_docs`, implement context prompt injection on agent launch, and migrate remaining floating controls to `AppTooltip`.

---

## 2. Key Accomplishments

### A. Authoritative Workspace Matching & Isolation
- Updated `src-tauri/src/kopeng/herdr.rs`:
  - Enforced strict label matching (`ws.label == project_name`) as the primary binding.
  - Added guardrails preventing panes located in foreign workspaces from claiming ownership.
  - Projects without an open dedicated Herder workspace now correctly show `// UNLINKED` and `IDLE`.
- Fixed indexing key in `src/App.vue` (`herdrStates.value[project.id]` instead of string key).

### B. Visual Context Tree & Initial Prompt Injection
- Implemented `get_project_file_tree` in `src-tauri/src/kopeng/herdr.rs` and registered it in `src-tauri/src/lib.rs`. Recursively scans the project workspace while omitting noise directories (`.git`, `node_modules`, `target`, `dist`).
- Created `src/components/ui/AppFileTree.vue`: Collapsible folder structure with custom checkboxes and instant text filtering.
- Integrated `AppFileTree` into `AddPersonaModal.vue` under an expandable "[Browse Project Files]" toggle.
- Updated `launch_herdr_agent` to read each file in `context_docs`, format the text under `[PROJECT CONTEXT DOCUMENTS]`, and prepend it to the agent's initial prompt in Herder.

### C. Floating Controls Tooltip Standardization
- Replaced native `title="..."` in `SettingsFab.vue`, `VoiceFab.vue`, and `DiagnosticBubble.vue` with `<AppTooltip>` using positions `top` and `left`.

### D. Specifications & Documentation
- Updated `docs/specs/08_herder_agent_orchestration.md` with sections 8.1 (Context Document Tree & Prompt Prepend) and 8.2 (Strict Workspace Label Isolation).
- Updated `docs/specs/13_design_system_and_ui_components.md` with section 2.5 (`AppFileTree`).

---

## 3. Verification
- Frontend production build (`npm run build`) completed successfully with 0 errors.
- Backend compilation (`cargo check`) completed in 0.25s with 0 errors.
- Running background dev server (`npm run tauri dev`) picked up all backend and frontend changes via Vite HMR.
