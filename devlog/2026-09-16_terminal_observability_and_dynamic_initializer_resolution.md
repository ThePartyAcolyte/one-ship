# Terminal Observability IPC & Dynamic Initializer Model Resolution

## 1. Problem Statement & Root Cause
- **Terminal Output Missing Command:** [ArtifactPanel.vue](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/dialogue/ArtifactPanel.vue) called `invoke('read_terminal_output', { paneId, lines, filter })` expecting formatted string output from the supervised agent's ring buffer. The command was previously named `read_process_output_cmd` (which returned `Vec<String>`) and was unregistered in `lib.rs`, triggering `Command read_terminal_output not found`.
- **Key Suffix Matching in Process Ring:** `read_output` in [supervisor.rs](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/supervisor.rs) checked only exact keys (`<project_path>:initializer`), failing when queried by relative pane ID (`initializer`).
- **Hardcoded Initializer Model Display:** [ConversationPanel.vue](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/dialogue/ConversationPanel.vue) contained fallback values hardcoding `'gemini-3.8-flash'`. `compute_project_state` in [supervisor.rs](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/supervisor.rs) did not query the live `ManagedProcess`'s active `runner`, `model`, and `effort` values, displaying outdated defaults even when `gemini-3.6-flash` was selected.

---

## 2. Implementation Summary

1. **Native Process Supervisor Enhancements (`src-tauri/src/supervisor.rs`):**
   - Added `runner: Option<String>`, `model: Option<String>`, and `effort: Option<String>` to `ManagedProcess`.
   - Populated these runtime fields upon `spawn_agent`.
   - Enhanced `ProcessSupervisor::read_output` to match either exact keys or suffix patterns (`:{key}` or `== key`).
   - Implemented `read_terminal_output(pane_id, target, lines, filter) -> Result<String, String>` supporting semantic error filtering via `extract_semantic_errors` and fallback querying against recent SQLite system logs.
   - Updated `compute_project_state` to dynamically derive the transient `initializer` persona's `runner`, `model`, and `effort` from the active `ManagedProcess`.

2. **IPC Command Registration (`src-tauri/src/lib.rs`):**
   - Registered `supervisor::read_terminal_output` in the Tauri `generate_handler!` macro.

3. **Frontend Dialogue & Conversation Panel (`src/components/dialogue/ConversationPanel.vue` & `src/stores/dialogueStore.ts`):**
   - Updated `activeAgentInfo` to match agent IDs case-insensitively and derive transient state for `initializer` without hardcoded model strings.
   - Updated `activeProjectPath` and `dialogueStore.getActiveProjectPath` to resolve paths for active initializing projects from `coreStore.initializingProjects`.

---

## 3. Verification & Results
- `cargo check --bins`: Succeeded with 0 errors.
- `cargo test --lib`: All 23 tests passed cleanly in 0.16s.
- `npm run build` (`vue-tsc --noEmit && vite build`): Succeeded with 0 TypeScript/compilation errors.
