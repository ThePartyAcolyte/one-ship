# Work Session Log: Herder Agent Persona Form & rula.toml Persistence

**Date:** 2026-09-03  
**Status:** Completed  
**Branch:** master  

---

## 1. Objectives & Context
Enable developers to configure and persist custom Agent Personas directly from the Herder container in the project card. The feature needed to support setting the agent's unique ID, human-readable role, terminal CLI runner (`agy`, `claude`, `gemini`, `cursor`, `pi`, `opencode`), safe mode (tool auto-approve toggle), extra CLI arguments, and initial context documents, writing them directly into the project's `rula.toml` configuration file.

---

## 2. Key Accomplishments

### A. Backend Rust Engine (`src-tauri/src/kopeng/herdr.rs` & `src-tauri/src/lib.rs`)
- Defined `AgentPersonaInput` struct with typed parameters (`id`, `role`, `runner`, `auto_approve`, `args`, `context_docs`).
- Implemented `add_agent_persona` Tauri command:
  - Validates `id` as a clean lowercase slug without invalid characters.
  - Loads and parses `rula.toml`, ensuring the `[agents]` table exists.
  - Inserts/updates `[agents.<id>]` with role, runner, auto_approve, args, and context_docs.
  - Serializes and writes pretty TOML back to disk while preserving all other configuration sections.
  - Registered command in `lib.rs` invoke handler.

### B. Vue Frontend Implementation
- **`src/components/modals/AddPersonaModal.vue`:**
  - Dedicated brutalist modal matching the project's theme.
  - Provides inputs for ID, Role, CLI Runner dropdown, Safe Mode toggle switch, extra CLI arguments, and context documents.
  - Client-side validation ensuring valid identifiers before submission.
- **`src/components/ProjectCard/HerderSection.vue`:**
  - Added `+ Persona` dashed action button inside the mini-pills bar.
  - Emits `addPersona` event upon clicking.
- **`src/components/ProjectCard/ProjectCard.vue`:**
  - Forwards `addPersona` from `HerderSection` to `App.vue`.
- **`src/App.vue`:**
  - Manages `addPersonaProject` reactive state.
  - Invokes `add_agent_persona` and triggers an immediate refresh of `get_project_herdr_state`.
  - Newly created personas instantly appear as `+ Launch` pills ready for single-click execution in Herder.

### C. Specifications Synchronization
- Updated `docs/specs/08_herder_agent_orchestration.md` with Section 8 detailing the Persona UI and persistence contract.

---

## 3. Verification & Results
- Frontend type check and Vite production build (`npm run build`) succeeded in 1.51s with 0 errors.
- Backend compilation (`cargo check`) succeeded in 1.22s with 0 errors and 0 warnings.
- Live Vite HMR applied all changes to the running Tauri window.
