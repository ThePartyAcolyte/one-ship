# Devlog — 2026-11-17: Global Voice Controller Stabilization, Surface Tools (Overview / Project / Files), Project Description & Specs 37–38

## Summary
Resolved Gemini Live function-calling execution failures and audio latency/clipping issues in the Global Voice Controller, added full voice selection and live testing in Settings, introduced project `description` metadata, standardized workspace subtabs with `SubtabBar.vue`, implemented initial panel surface tools (`overview`, `project`, `project/github`, `files`), and authored Specs 37 and 38 for the next architectural phase.

## Changes Implemented

### 1. Global Voice Controller Protocol & Audio UX (`src-tauri/src/voice_controller.rs`, `src-tauri/src/config.rs`, `src/components/modals/SettingsModal.vue`)
- **Tool Calling & Endpoint Fix:**
  - Switched `GEMINI_LIVE_WS_BASE` from `v1alpha` to `v1beta.GenerativeService.BidiGenerateContent`.
  - Updated `non_blocking_decl` to omit `parameters` on zero-argument functions (`ui_get_state`) and changed `surface_execute_action.params` to `params_json: STRING` (parsed as JSON in `execute_core_tool`) to avoid invalid empty `OBJECT` schemas.
  - Instrumented `gemini_reader` to log `interactionStatus`, `generationComplete`, `turnComplete`, `toolCallCancellation`, `goAway`, non-audio `modelTurn.parts`, and unknown frame keys.
- **Configurable Model & 30-Voice Catalog:**
  - Added `model` (`gemini-3.8-live` default without `thinkingConfig`; `gemini-3.8-live-extended-thinking` optional with `thinkingConfig.thinkingLevel`) and `voice_name` (all 30 Gemini Live studio voices, default `Fenrir`) to `[plugins.voice_controller]`.
  - Added daemon command `test_voice_controller_voice` (`src-tauri/src/api.rs`, `src/types/daemonApi.ts`) and a `Test` button next to `voice_name` in `SettingsModal.vue` that opens an ephemeral Live session and plays a spoken greeting via `pw-play`.
- **PipeWire Capture & Playback Synchronization:**
  - `ptt_start` now remains in `Connecting` until `spawn_pw_record_task` reads the first PCM frame from `pw-record`, only then sending `activityStart` and transitioning to `Listening`.
  - `ptt_stop` pre-warms `pw-play`, and `PwPlaySink::ensure_started` injects a 100 ms silence pre-roll (4800 bytes at 24 kHz s16le mono) so PipeWire sink negotiation does not clip the start of spoken responses.

### 2. Project Description Metadata (`db.rs`, `registry.rs`, `hydration.rs`, `CardHeader.vue`)
- Added nullable `description TEXT` column to SQLite `projects`, surfaced through `ProjectRecord`, `ProjectEntry`, `HydratedProject`, and `ui_get_state` for domain-based voice disambiguation.
- Added `update_project_description` daemon command and inline pencil-icon editing under the project title in `src/components/ProjectCard/CardHeader.vue` (showing `"No description"` when empty).

### 3. Unified Subtab Component (`src/components/ui/SubtabBar.vue`)
- Created reusable `SubtabBar.vue` (exported from `src/components/ui/index.ts`) and migrated `ProjectManagementTab.vue`, `GithubCollabPanel.vue`, `ProjectSkillsTab.vue`, and `ProjectElectronicsTab.vue`.

### 4. Surface Actions (`src-tauri/src/ui_state.rs`, `src-tauri/src/voice_controller.rs`, `src-tauri/src/config.rs`)
- **`overview`:** `get_project_config_schema`, `set_project_config_value` (backed by `get_project_config_tree` and `set_project_config_field` in `config.rs`).
- **`project`:** `get_project_summary`, `list_stages`, `create_stage`, `update_stage`, `delete_stage` (with `reassign_to_stage_id` protection against active tasks), `list_tasks` (status/domain/timeline date filters), `create_task`, `update_task` (partial), `delete_task`, `create_subtask`, `toggle_subtask`, `delete_subtask`.
- **`project/github`:** `list_issues`, `get_issue`, `list_prs`, `get_pr`, `list_releases`.
- **`files`:** Upgraded `search_files` to normalized token matching with relevance ranking, and added `semantic_search` backed by `crate::semantic::search_knowledge`.

### 5. Specifications (`docs/specs/`)
- Created `docs/specs/37_project_settings_tab_and_overview_dashboard.md`.
- Created `docs/specs/38_github_inbox_and_triage_flags.md`.
- Synchronized `docs/specs/36_global_voice_controller.md`.

## Verification
- `./scripts/run_dev.sh export-bindings`: `348 passed; 0 failed` (`TestVoiceArgs`, `UpdateProjectDescriptionArgs`, `HydratedProject`, and `api_commands_match_the_frontend_map` verified).
- `./scripts/run_dev.sh status`: `daemon` active/running after compilation.
- User confirmed voice navigation and UI control working end-to-end.
