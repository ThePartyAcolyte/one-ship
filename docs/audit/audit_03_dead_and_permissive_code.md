# Audit 03 — Dead Code Inventory & Permissive / Patch Code Analysis
**Audit Date:** `2026-11-16` | **Governed By:** `AGENTS.md` §1.4 (*Zero Backward Compatibility & Zero Dead Code*) & §1.11 (*Zero Silent Fallbacks*)

---

## Part 1: Dead Frontend & Repository Files (`3,780 LOC` Total)

### 1.1 Unused Frontend Source Files (`src/` — `3,735 LOC`)
Verified via workspace-wide import graph analysis (`0` active callers across `src/`):

| File Path | Lines | Reason Dead & Replacement |
| :--- | :---: | :--- |
| [`src/components/dialogue/ArtifactPanel.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/dialogue/ArtifactPanel.vue) | `2,221` | `0` imports. Superseded when the Explorer, CodeMirror reader, and Devlog tabs were integrated directly into `ProjectWorkspace.vue` and `FloatingChatOverlay.vue`. |
| [`src/components/dialogue/ConversationPanel.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/dialogue/ConversationPanel.vue) | `1,196` | `0` imports. Superseded by `FloatingChatOverlay.vue`. |
| [`src/components/modals/VcsActionModal.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/modals/VcsActionModal.vue) | `190` | `0` imports. VCS quick-action editing moved inline into `ProjectGitSnapshotsTab.vue`. |
| [`src/components/dialogue/DocTreeNode.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/dialogue/DocTreeNode.vue) | `119` | `0` imports outside its own recursive template (replaced by `AppFileTree.vue` / `AppFileTreeNode.vue`). |
| [`src/stores/pluginStore.ts`](file:///home/butcherwutcher/Projects/dev/one-ship/src/stores/pluginStore.ts) | `48` | `registerComponent`, `getComponentsForZone`, and `onDaemonEvent` have `0` active callers (`coreStore.ts:498` dispatches into an empty listener map). |
| [`src/composables/useResponsive.ts`](file:///home/butcherwutcher/Projects/dev/one-ship/src/composables/useResponsive.ts) | `36` | Only imported by dead `ArtifactPanel.vue:20`. |
| [`src/composables/useHoverMarquee.ts`](file:///home/butcherwutcher/Projects/dev/one-ship/src/composables/useHoverMarquee.ts) | `34` | `0` imports across `src/`. |
| [`src/utils/legacyStorage.ts`](file:///home/butcherwutcher/Projects/dev/one-ship/src/utils/legacyStorage.ts) | `26` | `0` imports. Legacy `wang_kapawu_` $\rightarrow$ `oneship_` localStorage shim scheduled for deletion in `devlog/2026-11-01_full_audit_and_remediation_phases_1_3.md`. |
| [`src/components/ui/ZoneOutlet.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/ui/ZoneOutlet.vue) | `21` | `0` imports across `src/`. |
| [`src/kopeng/agents/index.ts`](file:///home/butcherwutcher/Projects/dev/one-ship/src/kopeng/agents/index.ts) & [`AgentSection.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/kopeng/agents/components/AgentSection.vue) | `15` | `0` imports; empty `install(_registry)` stub marked `// Deprecated`. |

### 1.2 Obsolete Root & Template Files (`45 LOC`)
- [`project-tracker.toml`](file:///home/butcherwutcher/Projects/dev/one-ship/project-tracker.toml) (`9 lines`): Legacy config file (`name = "omarchy-project-tracker"`) from before `rula.toml`; scheduled for deletion on `2026-11-01`.
- [`templates/belter_ticker_prompt.txt`](file:///home/butcherwutcher/Projects/dev/one-ship/templates/belter_ticker_prompt.txt) (`13 lines`): Unreferenced Belter ticker prompt with legacy `kapawu` references.
- [`docs/04_ipc_connection_belta.json`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/04_ipc_connection_belta.json) (`23 lines`): Unreferenced JSON snippet (`0` references in repo).

### 1.3 Dead Store State, Actions & Unused Exports in Active Frontend Files
- **[`src/stores/workspaceLayoutStore.ts`](file:///home/butcherwutcher/Projects/dev/one-ship/src/stores/workspaceLayoutStore.ts):** `readerMode` (`L57`), `artifactPickerOpen` (`L58`), `isArtifactPanelOpen` (`L59`), `artifactPanelWidth` (`L60-64`), `setReaderMode` (`L96-99`), `toggleArtifactPanel` (`L101-103`), and `setArtifactPanelWidth` (`L105-113`) were only consumed by the dead `ArtifactPanel.vue` and `ConversationPanel.vue`. Active stores (`dialogueStore.ts:287, 347-349`, `projectExplorerStore.ts:92-94`, `DevlogJournalView.vue:251`) still mutate them as dead side effects.
- **Unused Exports in `src/`:**
  - `src/components/ui/quotaUtils.ts`: `formatLocalResetTime` (`L71`), `CategoryMeta` (`L6`)
  - `src/composables/useDocumentComments.ts`: `DisplayDocumentRef` (`L6`), `stripMarkdownFormatting` (`L28`), `findLineRangeForSelection` (`L41`)
  - `src/utils/format.ts`: `formatSpeedKbps` (`L21`)
  - `src/stores/electronicsStore.ts`: `formatVoltsPerDivLabel` (`L12`), `formatTimebaseLabel` (`L16`), `DEFAULT_VOLTS_PER_DIV_MV` (`L22`), `DEFAULT_TIMEBASE_STEPS_US` (`L23`), `VOLTS_PER_DIV_OPTIONS` (`L27`), `TIMEBASE_OPTIONS` (`L32`)
  - `src/types/index.ts`: `GitStatus` (`L12`), `SkillStatus` (`L5`)

---

## Part 2: Dead Backend Code & Legacy Migration Shims (`src-tauri/src/`)

### 2.1 Unused Daemon API Commands Registered in `api.rs` & `daemonApi.ts` (`0` Callers in `src/`)
1. **`read_process_output`** ([`src-tauri/src/api.rs:134, 414`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/api.rs#L134), [`src-tauri/src/supervisor.rs:4709-4718`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/supervisor.rs#L4709-L4718), `src/types/daemonApi.ts:368`) — Superseded by `read_terminal_output`.
2. **`get_effective_project_config`** ([`src-tauri/src/api.rs:246, 534`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/api.rs#L246), [`src-tauri/src/config.rs:1907-1918`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/config.rs#L1907-L1918), `src/types/daemonApi.ts:304`) — Superseded by `get_scoped_project_config`.
3. **`check_electronics_toolchain`** ([`src-tauri/src/api.rs:158, 446`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/api.rs#L158), `src/types/daemonApi.ts:272`) — Never called from the frontend (bundled inside `get_project_electronics_status`).

### 2.2 Uncalled Public Rust Functions, Dead Columns & Unused Error Variants
- [`src-tauri/src/kopeng/pcbcraft.rs:2446-2491`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/pcbcraft.rs#L2446-L2491): `pub fn extract_net_pin_map` — never called outside its unit test; comment says *"for future SPICE bridge reuse"*.
- [`src-tauri/src/kopeng/electronics/mod.rs:26-32`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/electronics/mod.rs#L26-L32): `pub fn resolve_electronics_schematics_dir` — `0` callers.
- [`src-tauri/src/db.rs:2407-2410`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/db.rs#L2407-L2410): `pub fn db_get_active_account` — `0` callers (`db_get_active_account_by_provider` is used instead).
- [`src-tauri/src/db.rs:3709-3715`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/db.rs#L3709-L3715): `pub fn db_get_latest_agent_session` — `0` callers.
- [`src-tauri/src/db.rs:2220-2227`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/db.rs#L2220-L2227): `pub fn db_update_snapshot_files_changed` — `0` callers.
- [`src-tauri/src/supervisor.rs:1267-1270`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/supervisor.rs#L1267-L1270): `pub fn list_active_pids` — `0` callers.
- [`src-tauri/src/semantic/store.rs:155-163`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/semantic/store.rs#L155-L163): `pub async fn delete_entity` — `0` callers.
- [`src-tauri/src/semantic/chunker.rs:9-13`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/semantic/chunker.rs#L9-L13): `pub fn hash_text` — `0` production callers (`#[allow(dead_code)]`).
- [`src-tauri/src/db.rs:252`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/db.rs#L252): `embedding BLOB` column in `agent_session_journal` — never written or read (embeddings live in LanceDB).
- **Dead Error Variants & Conversions:** `ElectronicsError::Disabled` (`electronics/errors.rs:11`), `GitlabError::InvalidRemoteUrl` (`gitlab.rs:21`), `InitializerError::NotInitializing` (`initializer.rs:31`), `SolidcraftError::ProjectNotFound` (`solidcraft.rs:33`), `RegistryError::Io` (`registry.rs:15`), `SemanticError::Io` (`semantic/errors.rs:27`), `impl From<DatabaseError> for String` (`db.rs:88`), `impl From<ReaError> for String` (`rea.rs:164`).
- **Unused Duplicate `DEFAULT_*` Constants:** `supervisor.rs:184-188` (5 constants), `rea.rs:30-36` (3 constants), `caller_identity.rs:24`, `config.rs:1944`.

### 2.3 Legacy `kapawu` & One-Shot Migration Shims Executing on Every Daemon Boot (`AGENTS.md §1.4`)
- [`src-tauri/src/db.rs:860, 1636-1694`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/db.rs#L1636-L1694) (`migrate_kapawu_identifiers`), [`src-tauri/src/kopeng/accounts.rs:142, 167-177`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/accounts.rs#L167-L177) (`LEGACY_ACCOUNT_SALT`, `rekey_account_token_if_needed`), and [`src-tauri/src/kopeng/snapshots.rs:298-323`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/snapshots.rs#L298-L323) (`migrate_project_snapshot_refs`): Since `oneship.db` and all registered projects were already migrated on `2026-11-09` (`devlog/2026-11-09_kapawu_identifiers_and_data_migration.md`), keeping these shims violates Zero Backward Compatibility (`AGENTS.md §1.4`).
- [`scripts/agy_statusline.py:27`](file:///home/butcherwutcher/Projects/dev/one-ship/scripts/agy_statusline.py#L27): Still reads `os.environ.get("KAPAWU_DB_PATH")` instead of `ONESHIP_DB_PATH`.

---

## Part 3: Unnecessarily Permissive or "Patch" Code (`AGENTS.md §1.11`)

### 3.1 Permissive Fallbacks & Heuristics (`P-01` – `P-09`)
1. **`P-01` — Hardcoded `"bosmang"` Lead Architect Callback Fallback ([`src-tauri/src/mcp.rs:2327-2334`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/mcp.rs#L2327-L2334)):**
   In `execute_report_task_status`, when `pending_del` is `None`, it spawns a background task calling `prompt_agent(p_path, "bosmang", ...)` and unconditionally returns `"Notified lead architect 'bosmang'"` even when `"bosmang"` does not exist in `rula.toml` or the agent was prompted directly by the user.
2. **`P-02` — Silent Genesis Milestone Substitution in `record_session_minutes` ([`src-tauri/src/mcp.rs:4614-4626, 4723`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/mcp.rs#L4614-L4626)):**
   When an agent passes an empty `title` or `summary`, `execute_record_session_minutes` silently substitutes `"Genesis Milestone — Phase 1 Discovery"` instead of returning an invalid-argument error; line 4723 also claims `"milestone snapshot created"` even when snapshots are disabled.
3. **`P-03` — Fabricated `greenfield` Init State in `complete_initialization` ([`src-tauri/src/kopeng/initializer.rs:521-532`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/initializer.rs#L521-L532)):**
   When `db_get_init_state` returns `None`, instead of returning `Err(InitializerError::NotInitializing)`, it fabricates a synthetic `ProjectInitState` with `mode: "greenfield"`.
4. **`P-04` — Duplicate User Prompt Dropping in `prompt_agent` ([`src-tauri/src/supervisor.rs:4604-4608`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/supervisor.rs#L4604-L4608)):**
   Skips inserting a user turn into `agent_chat_turns` if any of the last 3 turns has `t.role == "user" && t.dialogue == prompt`, silently dropping legitimate repeated short prompts (`"yes"`, `"continue"`, `"retry"`).
5. **`P-05` — Forced REA Enablement When `.one-ship/rea` Directory Exists ([`src-tauri/src/hydration.rs:356-357`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/hydration.rs#L356-L357)):**
   `has_rea_enabled: is_project_rea_enabled(p) || p.join(".one-ship").join("rea").exists()` overrides `[plugins.rea] enabled = false` in `rula.toml`.
6. **`P-06` — First-Sheet Fallback on Invalid `sheet_id` in PCBCraft ([`src-tauri/src/kopeng/pcbcraft.rs:1430-1435`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/pcbcraft.rs#L1430-L1435)):**
   When an explicit `sheet_id: Some(sid)` is not found, `.or_else(|| sheets.first())` silently renders the first sheet instead of returning an error.
7. **`P-07` — Hardcoded Fallback Boards & Default `"espressif32"` Platform in Electronics ([`src-tauri/src/kopeng/electronics/firmware.rs:202-205, 258`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/electronics/firmware.rs#L202-L205)):**
   `list_platformio_boards` returns a static 13-board list when `pio` fails, and `scaffold_firmware_project` defaults unknown boards to `"espressif32"`.
8. **`P-08` — Frontend Project Guessing via `resolveAgentProjectPath` ([`src/stores/dialogueStore.ts:219-241`](file:///home/butcherwutcher/Projects/dev/one-ship/src/stores/dialogueStore.ts#L219-L241) & [`RollingStatusLog.vue:243-261`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/floating/RollingStatusLog.vue#L243-L261)):**
   Guesses `projectPath` from active UI state when callers omit `projectPath`, and `RollingStatusLog.vue` flattens agents across all projects into a `Map` keyed by bare `agent.id`.
9. **`P-09` — Auto-Selecting First Unarchived Project in `toggleProjectArchived` ([`src/stores/coreStore.ts:254-257`](file:///home/butcherwutcher/Projects/dev/one-ship/src/stores/coreStore.ts#L254-L257)):**
   Selects `this.projects.find(p => !p.is_archived)` when archiving the active project, contradicting `coreStore.ts:164` (*"No project is ever auto-selected by position in the list"*).

### 3.2 Silent Error Swallowing (`let _ =`, `.ok()`, `.unwrap_or_default()`, empty `catch {}`)
- **Backend (`src-tauri/src/`):**
  - [`src-tauri/src/brief.rs:334, 356-399`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/brief.rs#L334-L399): Swallows SQLite errors via `if let Ok(...)` and `.unwrap_or_default()` while building context transfer Briefs.
  - [`src-tauri/src/hydration.rs:355`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/hydration.rs#L355): `db_get_init_state(&entry.path).unwrap_or(None)`.
  - [`src-tauri/src/semantic/mod.rs:81, 121-129, 226, 249, 268, 367-429`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/semantic/mod.rs#L81-L129): Drops LanceDB count errors, BGE-M3 embedding errors in `index_single_journal_entry`, and SQLite/fs errors during indexing without logging.
  - [`src-tauri/src/supervisor.rs:271, 285, 673, 702, 1513, 1879, 1891, 1902, 2177, 3405`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/supervisor.rs#L1879-L1902): `let _ = crate::db::db_record_process_termination(...)` and `let _ = crate::db::db_upsert_account_quota(...)` instead of `logger::log_failure`.
  - [`src-tauri/src/kopeng/electronics/mod.rs:113-115`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/electronics/mod.rs#L113-L115) & [`instruments/mod.rs:51, 55`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/electronics/instruments/mod.rs#L51-L55): Swallows USB/serial/toolchain probe errors via `.unwrap_or_default()` and `if let Ok(Some(...))`.
  - [`src-tauri/src/kopeng/electronics/instruments/owon_vds1022.rs:182, 204, 226, 241, 242, 255, 310, 356, 357, 701, 702, 751, 1015`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/electronics/instruments/owon_vds1022.rs#L182-L255): Discards USB bulk read/write and interface claim errors via `let _ =`.
- **Frontend (`src/`):**
  - Silent `catch` blocks in [`dialogueStore.ts:364`](file:///home/butcherwutcher/Projects/dev/one-ship/src/stores/dialogueStore.ts#L364), [`projectExplorerStore.ts:425`](file:///home/butcherwutcher/Projects/dev/one-ship/src/stores/projectExplorerStore.ts#L425), [`App.vue:137, 558, 583, 668`](file:///home/butcherwutcher/Projects/dev/one-ship/src/App.vue#L137), [`RollingStatusLog.vue:184`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/floating/RollingStatusLog.vue#L184), [`voiceStore.ts:47, 221, 410, 423`](file:///home/butcherwutcher/Projects/dev/one-ship/src/stores/voiceStore.ts#L47), [`AddPersonaModal.vue:64`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/modals/AddPersonaModal.vue#L64), and [`ProjectGitSnapshotsTab.vue:409`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectGitSnapshotsTab.vue#L409).
