# Devlog: Centralized Artifact Manifest & Space Him De-integration Flow

**Date:** 2026-09-09  
**Topic:** Implementation of Dual Project Removal Strategies & Centralized Artifact Manifest (`kopeng::manifest`)

---

## 1. Summary of Work

Implemented a surgical project removal flow in Wang-Kapawu featuring two user-selectable options when deleting a project:
1. **Unregister from Hub (Database Only):** Removes project entry from the SQLite database and clears initialization state while leaving `rula.toml`, `.agents/`, and project files untouched for future re-linking.
2. **Space Him (De-integrate & Purge Traces):** Surgically purges Wang-Kapawu specific artifacts (`rula.toml`, `.agents/skills/kapawu-platform/`, registered agent personas, `.wang-kapawu/`, and hidden git snapshot references) without affecting user source code or custom skills/workflows.

---

## 2. Architecture & Key Changes

### Rust Backend
* Created `src-tauri/src/kopeng/manifest.rs`:
  * Defined centralized arrays for platform-owned files (`KAPAWU_MANAGED_FILES`), directories (`KAPAWU_MANAGED_DIRECTORIES`), and platform skills (`KAPAWU_PLATFORM_SKILLS`).
  * Implemented `resolve_kapawu_artifacts(project_path: &Path) -> Vec<PathBuf>` to inspect static constants and dynamically parse `rula.toml` for persona files under `.agents/personas/`.
  * Implemented `purge_kapawu_artifacts(project_path: &Path) -> Result<Vec<String>, String>` to safely delete resolved artifacts and clean up empty parent `.agents/` subdirectories.
* Updated `src-tauri/src/registry.rs`:
  * Updated Tauri IPC `remove_project(id: usize, purge_traces: Option<bool>)` to execute artifact resolution and git snapshot ref pruning when `purge_traces` is `true`.

### Frontend & i18n
* Redesigned `src/components/modals/ConfirmDeleteModal.vue`:
  * Radio options matching brutalist styling (`font-mono`, `border-[var(--border-color)]`, theme variables).
  * Strategy cards for "Unregister from Hub" and "Space Him".
  * Project name confirmation input and action buttons.
* Updated `src/App.vue`:
  * Handled `{ project, purgeTraces }` payload from modal and invoked `remove_project` with `purgeTraces`.
* Updated `src/i18n.ts`:
  * Added internationalization labels for `purge_title`, `unregister_title`, `unregister_desc`, `space_him_title`, `space_him_desc`, `purge_action`, `space_him_action`.

---

## 3. Verification

* **Rust Compilation:** Ran `cargo check` in `src-tauri` -> Passed with exit code 0.
* **Frontend Build:** Ran `npm run build` -> Passed cleanly with exit code 0 (`vite v6.4.3 building for production...`).
