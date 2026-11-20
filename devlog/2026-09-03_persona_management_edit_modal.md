# Work Session Log: Refactor Persona Management UI into Edit Modal

**Date:** 2026-09-03  
**Status:** Completed  
**Branch:** master  

---

## 1. Objectives & Context
Improve the Agent Persona management lifecycle in Wang-Kapawu. Previously, right-clicking an agent in `HerderSection.vue` triggered an immediate "Delete Persona" action directly from the context menu, risking accidental deletions. The objective was to replace this with an "Edit Persona" action, refactor `AddPersonaModal.vue` to support dual Add/Edit modes with pre-populated fields and a locked identifier, and relocate the destructive deletion control into the modal footer with an inline two-step confirmation state.

---

## 2. Key Accomplishments

### A. Dual-Mode Persona Modal (`src/components/modals/AddPersonaModal.vue`)
- Extended component props to accept optional `persona?: HerderAgentState | null`.
- Added `isEditMode = computed(() => !!props.persona)` to dynamically switch header, inputs, and actions.
- Pre-populates all reactive fields (`role`, `runner`, `auto_approve`, `args`, `context_docs`, and `selectedDocs` for `AppFileTree`).
- Primary key protection: `agentId` input is disabled (`:disabled="isEditMode"`) in edit mode with a helper notice.
- Destructive deletion in modal footer:
  - Positioned on the left side of the footer toolbar with brutalist red tokens (`border border-rose-800/80 text-rose-400 hover:bg-rose-950/40`).
  - Added inline two-step confirmation (`Confirm Delete` vs `Cancel`), preventing accidental persona loss.
  - Emits `delete` with `{ agentId: string }`.
- Robust CLI argument parsing supporting both space- and comma-separated tokens.

### B. Herder Section Context Menu (`src/components/ProjectCard/HerderSection.vue`)
- Imported `SlidersHorizontal` from `@lucide/vue`.
- Replaced the direct "Delete Persona" context menu action with "Edit Persona".
- Emits `editPersona({ project: props.project, agent })`.

### C. Event Forwarding & State Coordination (`ProjectCard.vue` & `App.vue`)
- **`src/components/ProjectCard/ProjectCard.vue`:** Added `editPersona` emit and forwarded the event from `HerderSection`.
- **`src/App.vue`:**
  - Added `editingPersonaTarget` reactive state (`{ project: any, persona: any } | null`).
  - Implemented `openEditPersonaModal`, `closePersonaModal`, `handleSavePersona`, and `handleDeletePersona`.
  - On save: invokes `add_agent_persona` (upserts in `rula.toml`), closes modal, and refreshes project Herder state.
  - On delete: closes associated Herder tab if open (`close_herdr_tab`), invokes `delete_agent_persona`, logs status, closes modal, and refreshes project Herder state.
  - Bound `<AddPersonaModal>` dynamically to active project and persona.

### D. Specifications Synchronization
- Updated `docs/specs/08_herder_agent_orchestration.md` (Subsection 8.4) detailing the Edit modal lifecycle, primary key locking, and in-modal deletion protocol.

---

## 3. Verification & Results
- Frontend production build (`npm run build`) completed in 1.53s with 0 errors.
- Backend compilation (`cargo check`) completed in 1.49s with 0 errors and 0 warnings.
- MCP daemon verification confirmed responsive on `http://localhost:8765/mcp`.
