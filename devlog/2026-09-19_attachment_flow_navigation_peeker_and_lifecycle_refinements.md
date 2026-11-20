# Devlog: Attachment Flow, Navigation Peeker, and Agent Lifecycle Refinements

**Date:** 2026-09-19  
**Topic:** Reader Attachment Flow Auto-Open, Lateral Peeker & Header View Switcher, Agent Categorization & Ephemeral Resolver Filtering, Initialization Rollback Gate, and Workspace Sub-Tab Synchronization.

---

## 1. Problem Statement & Motivation
Following the decoupling of the Project Workspace from the dialogue system, five specific refinement gaps were identified and addressed:
1. **Attachment Workflow Disconnection:** In the decoupled reader, attaching a document did not automatically focus or unminimize the conversation overlay.
2. **View Navigation Affordance:** Loss of the lateral view switcher made jumping between Dashboard and Workspace dependent entirely on card clicks.
3. **Agent Lifecycle Pollution:** Completed initializers and ephemeral flag resolution agents (`docs_resolver`, `todos_resolver`, `tech_debt_resolver`, `profiler_resolver`) persisted in the active launcher and tab bar even after completion.
4. **Initialization Overlay Gate:** The initialization blocking overlay on `ProjectCard.vue` lacked a real cancellation and rollback action, and could linger on completed unattended tasks.
5. **Workspace Sub-Tab Synchronization:** Clicking artifact chips or `[View Agent Terminal »]` in the Chat Overlay switched views but failed to activate the corresponding sub-tab (`Files & Docs` vs `Terminals`) or load the referenced file.

---

## 2. Architectural Changes & Refinements

### A. Document Reader Attachment Flow & Overlay Auto-Focus
- **`ProjectWorkspace.vue`**:
  - In `toggleAttachCurrentDoc()`, staging a document now automatically invokes `dialogueStore.openFloatingChat(targetAgentId)`.
  - The staged attachment is immediately visible in `FloatingChatOverlay.vue` as a badge above the input and is formatted as a structured reference (`[ATTACHED CONTEXT & REFERENCES]`) upon message dispatch.

### B. Bi-Directional View Navigation & Lateral Peeker Rail
- **`AppHeader.vue`**:
  - Integrated a segmented `[DASHBOARD]` / `[WORKSPACE]` view switcher with active state styling.
- **`App.vue`**:
  - Added a lateral workspace peeker rail on the right edge of the Dashboard view (`[ WORKSPACE » ]`), enabling one-click navigation to the active project workspace.
  - Added global hotkeys `Alt+1` (Dashboard), `Alt+2` (Floating Chat toggle), `Alt+3` (Workspace), `Alt+W`, and `Ctrl+Tab`.
- **`ProjectVerticalRail.vue`**:
  - Maintained symmetric return affordance via `[« Dashboard]`.

### C. Agent Categorization & Ephemeral Resolver Filtering
- **`AgentTaskbar.vue`**:
  - Structured the Agent Launcher popover into three distinct sections:
    1. **Project Specialists:** Bounded personas declared in `rula.toml` (`bosmang`, `frontend`, `core`, `devops`).
    2. **Platform & System Agents:** Global `system` System Architect and project `git` Version Control Specialist.
    3. **Active Resolutions:** Ephemeral flag resolvers and initializers, rendered strictly while `status === 'working'` or running.
  - Filtered completed `initializer` and resolved flag agents from the bottom conversation tabs unless actively selected.

### D. Initialization Overlay Reactivity & Rollback Gate
- **`ProjectCard.vue`**:
  - Updated `isInitializing` reactivity to automatically unblock the card when `status === 'completed'`.
  - Replaced the passive dismiss button with `[✕] Cancel & Rollback`, which invokes `coreStore.rollbackProjectInit(project.path)` to purge partially generated files (`.agents`, `AGENTS.md`) and clears SQLite initialization state.

### E. Workspace Sub-Tab & Artifact Synchronization
- **`ProjectWorkspace.vue`**:
  - Added event listeners for `switch-to-workspace-terminal` (activates `terminal` sub-tab), `open-workspace-file` (activates `files` sub-tab and loads the target document into reader/editor), and `open-workspace-devlog` (activates `devlog` sub-tab).
### F. In-Chat Terminal Viewport Switcher (`FloatingChatOverlay.vue`)
- Introduced internal window mode `chatViewMode = ref<'messages' | 'terminal'>('messages')`.
- Clicking the header toggle button switches the body of the floating chat window between conversation dialogue messages and live agent stdout/stderr.
- Includes automatic polling (`read_terminal_output`), error filtering toggle (`[ERRORS ONLY]` vs `[ALL OUTPUT]`), and one-click buffer copy.
- Decouples single agent conversation streams from the workspace terminal grid, allowing `ProjectTerminalGrid.vue` to focus cleanly on project background services, run scripts, and system logs.

### G. Full-Height Dashboard Peeker Rail (`App.vue`)
- Replaced the small floating button with a full-height interactive vertical rail (`w-11`) matching the aesthetic of `ProjectVerticalRail.vue`.
- Features an action button at the top, full-height rotated title (`WORKSPACE // <PROJECT_NAME>`), active pulsing indicator, and project initials badge at the bottom.
- The entire rail is interactive (`cursor-pointer`), allowing one-click transition anywhere on the rail.

---

## 3. Verification & Validation
- **Frontend Build (`vue-tsc --noEmit && vite build`):** Clean exit code 0.
- **Backend Cargo Checks (`cargo check`):** Clean exit code 0.
