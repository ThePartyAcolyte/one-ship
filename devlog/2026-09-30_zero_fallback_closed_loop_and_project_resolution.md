# Zero-Fallback Closed Loop & Deterministic Project Resolution

**Date:** 2026-09-30  
**Topic:** Zero-Fallback Closed Loop and Origin-Bound Project Updates for MCP and Dialogue Subsystems

## 1. Problem Statement & Root Cause Analysis

During project onboarding and flag resolution for `L10N CL Banking` (`l10n_cl_banking`, project ID 8), the user resolved the technical debt initialization flag. The ephemeral resolution agent `techdebt-resolver` generated a structured migration proposal which was approved and executed. The 11 debt items were inserted into SQLite table `project_technical_debt`, but the UI displayed zero items in the Technical Debt tab.

### Forensic Findings
1. **Blind Fallback in `getActiveProjectPath`:** In `dialogueStore.ts`, `getActiveProjectPath(agentId)` iterated solely over permanent daemon agents registered in `agentStates`. When resolving ephemeral flag agents (e.g. `techdebt-resolver`), it failed to match and fell back blindly to `projects[0]` (`estudiorc`, project ID 3).
2. **Ghost Clear on Store Selection:** Selecting `techdebt-resolver` triggered `fetchProjectTechnicalDebt` using `estudiorc`'s path, overwriting `dialogueStore.projectTechnicalDebt` with an empty array.
3. **Silent MCP Mutations:** MCP tools (`record_technical_debt`, `manage_todo_item`, `resolve_project_init_flag`) recorded entries to SQLite but did not emit supervisor events informing the frontend of changes.
4. **Unwatched View Components:** `DevlogJournalView.vue` lacked lifecycle hooks (`onMounted`) and watchers on project path, requiring manual user refreshes to display updated data.

## 2. Architectural Solution: Zero-Fallback Closed Loop

In accordance with strict architectural mandates, all arbitrary fallbacks (`projects[0]`, unconstrained guesses) were eradicated. Project resolution was re-engineered into a deterministic closed loop:

### Backend & MCP Supervisor (`src-tauri/`)
1. **Supervisor Event Propagation:**
   - `execute_manage_todo_item` & `execute_resolve_todo_item` now emit `"todo-updated"` containing `project_id`, `project_path`, `todo_id`, `action`, and `agent_id`.
   - `execute_record_technical_debt` & `execute_resolve_technical_debt` now emit `"tech-debt-updated"` containing `project_id`, `project_path`, `debt_id`, `action`, and `agent_id`.
   - `execute_resolve_initialization_flag` and `db_resolve_init_flag` query the associated project record and emit `"init-flag-resolved"` with `project_id`, `project_path`, and `flag_type`.

### Frontend Stores & State Management (`src/stores/dialogueStore.ts`)
1. **Explicit Agent-to-Project Registry:**
   - Added `agentProjectMap: Record<string, string>` to `dialogueStore` state.
   - Updated `openOverlay(agentId, projectPath)`, `openFloatingChat(agentId, projectPath)`, and `selectAgent(agentId, projectPath)` to record the target project path upon invocation.
2. **Deterministic Resolution (`resolveAgentProjectPath`):**
   - Resolves via `agentProjectMap`, configured `agentStates`, initializing projects, or active conversation record's `project_id`.
   - Returns `null` if no project matches, eliminating the `projects[0]` fallback globally.
3. **Reactive Supervisor Listeners:**
   - Added listeners for `"todo-updated"` and `"tech-debt-updated"`, refreshing data for the explicitly provided `project_path`.
   - Updated `"init-flag-resolved"` listener to refresh flags and fetch domain data (todos, tech debt, docs) directly for the target project.

### Component Layer
1. **`DevlogJournalView.vue`:**
   - Defined `project` prop and bound `currentProject = computed(() => props.project ?? coreStore.activeProject ?? null)`.
   - Added `onMounted` and `watch(() => currentProject.value?.path)` to trigger automated refresh.
   - Passed explicit `pPath` to `store.createProjectTodo` and `store.recordTechnicalDebt`.
2. **`ProjectWorkspace.vue` & `ArtifactPanel.vue`:**
   - Passed explicit `:project="activeProject"` and `:project="currentProject"` to `<DevlogJournalView />`.
   - Updated file attachment chat openers to supply `activeProject.value?.path`.
3. **`FloatingChatOverlay.vue` & `ConversationPanel.vue`:**
   - Switched to `store.resolveAgentProjectPath(agentId)` without any fallback to `projects[0]`.
4. **`ProjectCard.vue` & `ProjectOverviewTab.vue`:**
   - Passed `props.project.path` to `dialogueStore.openOverlay(agentId, props.project.path)` in all flag resolution handlers.

## 3. Verification & Compliance
- **Compilation:** `cargo check` in `src-tauri` passed with 0 errors. `npm run build` (`vue-tsc --noEmit && vite build`) passed with 0 errors.
- **Tiger Style:** Unit suffixes, deterministic lookups, zero magic numbers, zero blind fallbacks.
