# 2026-11-16 — UI Cleanup, Project Archiving, Snapshot Scopes & Platform Agent Removal

## Summary
Streamlined the project workspace interface, unified feature toggles into `ProjectConfigPanel.vue`, refined global vs. project configuration scopes, added project archiving, moved quota/account controls to the bottom agent taskbar, and removed the redundant `git` and `system` platform agents.

## Changes

### 1. Project Archiving (`is_archived`)
- **Backend (`src-tauri/src/db.rs`, `registry.rs`, `hydration.rs`, `api.rs`):**
  - Added `is_archived INTEGER NOT NULL DEFAULT 0` to `projects` table and exposed `is_archived: bool` on `ProjectRecord`, `ProjectEntry`, and `HydratedProject`.
  - Registered daemon command `toggle_project_archived`.
- **Frontend (`src/types/daemonApi.ts`, `src/stores/coreStore.ts`, `ProjectVerticalRail.vue`, `ProjectOverviewTab.vue`):**
  - Added `coreStore.toggleProjectArchived(projectId, isArchived)`, automatically switching `activeProjectId` if the active project is archived.
  - Filtered archived projects out of the vertical rail and expanded drawer by default, with a persistent `Eye` / `EyeOff` visibility toggle and per-project archive/restore buttons.

### 2. Bottom Taskbar Quota & Account Controls
- Extracted Google OAuth multi-account switcher (`AppQuotaGauge` x4, warmup indicator) and DeepSeek Wallet from `src/components/AppHeader.vue` into `src/components/taskbar/AccountQuotaControls.vue` with upward-opening popovers.
- Integrated `<AccountQuotaControls />` on the right side of `src/components/taskbar/AgentTaskbar.vue`.

### 3. Unified Project Configuration & Scope Cleanup
- **Overview Unification (`ProjectOverviewTab.vue`, `ProjectConfigPanel.vue`):**
  - Removed the redundant **Project Features & Subsystems** card grid from `ProjectOverviewTab.vue`.
  - Integrated subsystem and tab visibility `AppToggle` switches directly into the left category list of `ProjectConfigPanel.vue` (including `semantic`, `snapshots`, and a virtual `skills` category), dimmed inactive categories, hid duplicate `enabled` fields in the right pane, and disabled parameter inputs when a category is disabled.
  - Added contextual quick actions (`Open Lab`, `Open Board`, `View Snapshots & Diffs` / `Initialize Git Repository`, `Inspect Scripts & Ports`, `Index Knowledge`, `Skills` / `Sync`) and an explicit empty-parameters notice when a category has no additional project-level fields.
- **Schema & Scope Fixes (`src-tauri/src/config.rs`, `conversation.rs`, `mcp.rs`):**
  - Fixed `plugins.electronics.default_baud_rate` and `flasher_tool` `field_type` from `"select"` to `"select_enum"`.
  - Scoped `orchestrator.auto_approve`, `orchestrator.agent_runner`, `orchestrator.default_model`, `orchestrator.default_effort`, `accounts.context_warning_threshold_percent`, `snapshots.auto_snapshot_on_init`, and `vcs.default_branch` to `"global_only"`.
  - Moved `default_docstyle` from `snapshots` to `orchestrator` (`global_only`).
  - Added project-level snapshot toggles in `snapshots`: `enabled` (default `true`), `auto_snapshot_on_conversation_start` (wired in `conversation::create_agent_conversation`), and `auto_snapshot_on_file_change` (wired in `mcp::record_session_minutes`).
- **Initialization Detection (`src/components/ProjectCard/CardHeader.vue`):**
  - Updated `isProjectInitialized` to check `skill_status?.is_installed || init_state?.status === 'completed'` so saving a partial `rula.toml` does not suppress project initialization.

### 4. Removal of `git` and `system` Platform Agents
- **Backend (`supervisor.rs`, `api.rs`, `events.rs`, `config.rs`, `kopeng/resolution.rs`, `kopeng/accounts.rs`):**
  - Removed automatic synthesis of the `git` specialist in `compute_project_state`, `git_agent_status` from `ProjectAgentState`, and `vcs.agent_args` / `vcs.auto_approve` from `config.rs`.
  - Removed `launch_system_agent`, `get_system_agent_state`, `system_agent_state`, `resolve_system_working_dir`, `system-agent-updated` events, `ConfiguredAgentPair::System`, and `orchestrator.system_model` / `system_effort`.
- **Frontend (`daemonApi.ts`, `coreStore.ts`, `AgentTaskbar.vue`, `dialogueActions.ts`, `ConversationPanel.vue`, `FloatingChatOverlay.vue`, `App.vue`):**
  - Removed `systemAgentState`, `fetchSystemAgentState`, `Platform & System` taskbar section, and `agentId === 'system'` branches.

### 5. Scrollable Workspace Tab Bar
- Updated `src/components/workspace/ProjectWorkspace.vue` so workspace tabs stay single-line (`whitespace-nowrap shrink-0`), scroll horizontally with invisible scrollbars (`overflow-x-auto scrollbar-none`) and mouse-wheel support (`@wheel.prevent`), auto-scroll the active tab into view, and expose an overflow dropdown (`ChevronDown`) via `ResizeObserver`.
