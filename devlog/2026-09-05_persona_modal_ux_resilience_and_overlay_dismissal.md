# Devlog: 2026-09-05 - Persona Modal UX Resilience & Overlay Safety Dismissal

## Objective
Diagnose and resolve the interface locking issues where the agent persona edit form (`AddPersonaModal.vue`) felt unresponsive or trapped, ensure persistent saved data in `rula.toml` is verified, eliminate blocking IPC loops, add standard dismiss interactions (backdrop click and Escape key), provide explicit saving states, and add safety dismissal to the project under-construction overlay. Delegate backend lifecycle completion to `bosmang`.

## Actions Taken

### 1. Frontend UX & Form Interaction (`AddPersonaModal.vue`)
- Added `@click.self="emit('close')"` to the fixed backdrop container to allow dismissing the modal by clicking outside.
- Added a native `Escape` key listener (`window.addEventListener('keydown', handleKeydown)`) with lifecycle hook teardown.
- Added `:is-saving` prop and local `Loader2` spinning indicator on the Save button with `:disabled="isSaving"`.
- Eliminated synchronous full-depth (depth 12) filesystem tree scanning on modal mount/watch. Replaced it with lazy-loading (`loadFileTree`) triggered only when the user explicitly clicks `[Browse Project Files]`, with a visual scanning indicator while IPC is resolving.

### 2. Async Decoupling & Non-Blocking State Refresh (`App.vue`)
- Introduced `isSavingPersona` ref state to provide visual feedback during IPC execution.
- Executed `closePersonaModal()` immediately on successful `add_agent_persona` invocation.
- Dispatched `get_project_herdr_state` asynchronously in the background (`.then(...)`), assigning directly to `coreStore.herdrStates[targetProject.id]` without blocking the UI or mutating a computed ref synchronously.

### 3. State Management & Overlay Safety (`coreStore.ts` & `ProjectCard.vue`)
- Added `clearProjectInitializing(projectId: number)` action to `coreStore`.
- Expanded initializer status handling in `coreStore` to handle terminal statuses (`completed`, `done`, `error`, `failed`, `idle`) so `isInitializing` transitions out reliably.
- Added a brutalist `[X] Dismiss` button wrapped in `AppTooltip` within the `ProjectCard.vue` Under Construction overlay so users can manually dismiss the blocking overlay if a background process terminates silently.

### 4. Backend Delegation (`bosmang`)
- Delegated task `task-1788590103` to `bosmang` via `delegate_agent_task`:
  1. Emit `agent-status-report` upon completion or error of the Tokio background task in `src-tauri/src/kopeng/skill.rs`.
  2. Add `#[serde(default)]` to `AgentPersonaInput` in `src-tauri/src/kopeng/herdr.rs`.

## Verification
- Verified persistence in `rula.toml` for `[agents.mcp]` (`model = "Gemini 3.8 Flash"`).
- Ran full TypeScript and Vite production build (`npm run build`). Clean compilation with 0 errors.
