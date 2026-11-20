# Audit 02 — Specifications vs. Implementation & Unregistered Subsystems
**Audit Date:** `2026-11-16` | **Scope:** Foundation Docs (`00`–`04`, `docs/README.md`) + Specifications `01`–`34` (`docs/specs/`)

---

## 1. Classification Methodology: Phenomenon A vs. Phenomenon B

Per maintainer governance established on `2026-11-16`, every discrepancy between specifications and code is classified into:
- **[PHENOMENON A — Code Evolved / Update Spec]:** The implementation was intentionally changed, refined, or expanded during development. The code is authoritative; the specification is outdated and must be updated to match the code.
- **[PHENOMENON B — Unimplemented in Code / Case-by-Case Decision]:** The specification describes features, phases, or tools that have not yet been built. During remediation, each Phenomenon B item is evaluated case-by-case with the maintainer to decide whether to **(1) implement it**, **(2) defer it explicitly to Stage 2 / Post-1.0**, or **(3) remove it from the spec**.

---

## 2. Foundation Documents Audit (`docs/00_*` – `04_*`, `docs/README.md`)

*(Maintainer decisions already confirmed on `2026-11-16` for WP-1):*

| Document | Finding | Classification & Confirmed Action |
| :--- | :--- | :--- |
| [`docs/00_manifesto.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/00_manifesto.md) | • Line 15 states *"Not an IDE clone or code editor"*, whereas CodeMirror 6 (`AppCodeEditor.vue`) and file CRUD were intentionally added.<br>• Line 10 claims an LLM Diagnostic Laboratory (Spec 02, which is deferred). | **Phenomenon A (Confirmed):** Update `00_manifesto.md` to document the integrated code/spec editor and remove the active claim for the deferred LLM Error Curator. |
| [`docs/01_ubiquitous_language.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/01_ubiquitous_language.md) | • Line 4 says Daemon emits state updates *"via WebSocket"* instead of **HTTP SSE (`GET /api/events`)**.<br>• Line 18 lists `cursor` as a Runner.<br>• Lines 7–8 define `Error Curator` / `Core Error` (Spec 02 deferred). | **Phenomenon A (Confirmed):** Replace WebSocket with SSE `/api/events`; restrict Runners to the 3 installed/used harnesses (`agy`, `claude`, and `opencode` with DeepSeek); mark Error Curator as deferred. |
| [`docs/02_architecture_stack.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/02_architecture_stack.md) | • Does not document **DuckDB** (`oneship_telemetry.duckdb`) or **LanceDB + local BGE-M3** (`vectors.lance/`) in the storage layer, nor the runner/DeepSeek specifics. | **Phenomenon A (Confirmed):** Add the 3-database storage architecture (SQLite, DuckDB, LanceDB/BGE-M3) and runner stack (`agy`, `claude`, `opencode` + DeepSeek). |
| [`docs/03_lang_belta_dictionary.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/03_lang_belta_dictionary.md) & `tekidok_lang_belta.*` | • Written in Spanish with legacy `omarchy-project-tracker` paths; Cinturonero translation is parked until the app is complete. | **Deferred by Design (Confirmed):** Mark clearly as parked/deferred for the post-completion translation phase; delete dead `docs/04_ipc_connection_belta.json` (0 references). |
| [`docs/README.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/README.md) | • Claims 31 specs on line 21, but omits links to **Spec 32** (`oneship_designer`), **Spec 33** (`cad_and_pcb`), and **Spec 34** (`project_management_and_oss`). | **Phenomenon A (Confirmed):** Synchronize `docs/README.md` index with all existing specifications. |

---

## 3. Per-Specification Discrepancy Breakdown (`Spec 01` – `Spec 34`)

> **Numbering Anomalies in `docs/specs/`:**
> - **Duplicate `26_` prefix:** `26_greenfield_project_creation_and_conversational_bootstrapping.md` and `26_rea_reverse_engineering_integration.md`.
> - **Missing numbers:** `15`, `27`, `28` (27 and 28 were removed with Visual Builder / Onlook on `2026-11-15`).

---

### Spec 01: Project Scripts & Service Orchestration ([`01_service_orchestrator.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/01_service_orchestrator.md)) — **92%**
- **Phenomenon A (Code Evolved -> Update Spec):**
  1. `§1 L15`: `system_logs` column is `origin TEXT NOT NULL` in [`db.rs:341-348`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/db.rs#L341-L348), not `project_id`.
  2. `§1`: `projects` table in `db.rs:335-337` adds `is_favorite`, `is_archived`, `hidden_tabs`; `supervised_processes` in `db.rs:810-816` adds `profiler_engine_id` and `profile_artifact_path`; `project_initialization_flags` also supports `'invalid_config'` and `'removal'`.
  3. `§2 L53-56`: `reconcile_orphan_processes` in [`supervisor.rs:591-656`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/supervisor.rs#L591-L656) **adopts** live persistent services (`target_type == "service"`) and only terminates ephemeral tasks; no `process-reconciliation-summary` event is used.
  4. `§3.2 L73-79`: Context menu `Profile Service (15s)` was replaced by `'Performance Telemetry'` (switching to the continuous Profiler tab), and `kapawu-open-history` was renamed to `oneship-open-history`.
  5. `§3.3`: Live task output streaming uses SSE events `task-output-chunk` and `task-completed` ([`events.rs:180-186`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/events.rs#L180-L186)).
- **Phenomenon B (Unimplemented in Code):** None.

---

### Spec 02: Diagnostic Laboratory ([`02_diagnostic_laboratory.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/02_diagnostic_laboratory.md)) — **0% (DEFERRED)**
- Explicitly marked `Status: DEFERRED (Planned for post-0.2.x roadmap)`. Only regex line extraction (`extract_semantic_errors` in `kopeng/errors.rs`) exists.

---

### Spec 03: Tray Hub Interface ([`03_tray_hub_interface.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/03_tray_hub_interface.md)) — **0% (DEFERRED / Superseded)**
- Explicitly marked `Status: DEFERRED`. Superseded in practice by the native Omarchy Quickshell bar widget (`integrations/omarchy-bar/`, documented in Spec 12 §7.3).

---

### Spec 04 & Spec 05: OpenDictate Integration & D-Bus Protocol ([`04_dictation_and_nlp.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/04_dictation_and_nlp.md), [`05_dictate_ipc_protocol.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/05_dictate_ipc_protocol.md)) — **90%**
- **Phenomenon A (Code Evolved -> Update Spec):**
  1. Replace legacy `Kapawu` and Tauri event references in Spec 04 with `ONE-SHIP` and `/api/events` SSE (`listenDaemonEvent`).
  2. `Spec 04 §1 L19 & §2 L55`: Session IDs use `oneship-${normalizedAgentId}-${Date.now()}` ([`voiceStore.ts:236, 325`](file:///home/butcherwutcher/Projects/dev/one-ship/src/stores/voiceStore.ts#L236)) rather than random `UUIDv4`.
  3. `Spec 04 §5`: Signal `.SessionReservationReleased` emits `opendictate_session_released` ([`events.rs:38`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/events.rs#L38)), and `.SessionReserved` emits `opendictate_session_reserved` ([`opendictate.rs:274-285`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/opendictate.rs#L274-L285)).
  4. `Spec 04 §2 & §6`: `FloatingChatOverlay.vue:133-144` provides an interactive mic button with `<AppTooltip>` status feedback.
  5. Document `/tmp/opendictate_state.json` auto-send inspection (`auto_send: bool` on `SessionFinishedPayload` in [`opendictate.rs:143-170`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/opendictate.rs#L143-L170)).
  6. `Spec 05 §5.1`: Align `GetStatus()` JSON schema with `OpenDictateStatus` in [`opendictate.rs:444-452`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/opendictate.rs#L444-L452) (`is_busy`, `is_available`, `autosend_enabled`, `reserved_session: Option<String>`).

---

### Spec 06: Plugin Architecture — Kopeng ([`06_plugin_architecture.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/06_plugin_architecture.md)) — **65%**
- **Phenomenon A (Code Evolved -> Update Spec):**
  1. `§3`: Services and tasks in `rula.toml` live under `[scripts.persistent.<id>]` and `[scripts.task.<id>]`, never `[[plugins.orchestrator.services]]`.
  2. `§4`: `src-tauri/src/kopeng/mod.rs` now houses 23 modular Rust domain subsystems.
  3. `§5`: The dynamic `<ZoneOutlet>` / `src/kopeng/` frontend plugin registry was superseded by static workspace tabs in `ProjectWorkspace.vue` gated by `[plugins.<id>].enabled` in `rula.toml` and typed DOM events in `src/utils/uiEvents.ts`. (`ZoneOutlet.vue`, `pluginStore.ts`, and `src/kopeng/agents/` are dead code).

---

### Spec 07 & Spec 14: i18n & Cinturonero Translator Protocol ([`07_i18n_translation_strategy.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/07_i18n_translation_strategy.md), [`14_cinturonero_translator_protocol.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/14_cinturonero_translator_protocol.md)) — **20% (PARKED)**
- **Confirmed Status:** Parked until packaging. Add explicit header banners to both Spec 07 and Spec 14 noting that UI development is strictly English-only during active development (`AGENTS.md §1.2`) and that Belta translation is a separate post-packaging pass.

---

### Spec 08: Native Process Supervisor ([`08_native_process_supervisor.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/08_native_process_supervisor.md)) — **94%**
- **Phenomenon A (Code Evolved -> Update Spec):**
  1. `§2.1`: `ProcessSupervisor` is a daemon-global `OnceLock<ProcessSupervisor>` (`get_supervisor()`), not Tauri `State<Arc<ProcessSupervisor>>`. Update struct definition to match [`supervisor.rs:814-905`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/supervisor.rs#L814-L905).
  2. `§2.5`: Locks use poison-recovering `.read_safe()` / `.write_safe()` (`RwLockSafe`), not `.expect()`.
  3. `§2.7`: Backend `ProcessSupervisor::format_agent_key` uses a single colon `<project_path>:<agent_id>` ([`supervisor.rs:1091`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/supervisor.rs#L1091)), and the `system` agent special case was removed.
  4. `§2.8 L159`: Only `oneship-daemon` (`bin/daemon.rs:30`) calls `reconcile_orphan_processes()`, never `lib.rs`.
  5. `§4 L245`: Remove reference to non-existent `system-agent-updated` event.
  6. `§5`: `openFloatingChat` and `switchProjectContext` live in `workspaceLayoutStore.ts`.

---

### Spec 09: VCS Dual-Signification & Cyclic Logging ([`09_vcs_and_logging_architecture.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/09_vcs_and_logging_architecture.md)) — **95%**
- **Phenomenon A (Code Evolved -> Update Spec):**
  1. `§3 L76-79`: `oneship.log` format includes 4 bracketed fields: `[timestamp_seconds] [LEVEL] [module] [origin] message` ([`logger.rs:254`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/logger.rs#L254)).
  2. Document the Git branch management commands (`list_git_branches`, `switch_git_branch`, `create_git_branch` in [`git.rs:586-752`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/git.rs#L586-L752)) and `GitBranchSelector.vue` / `GitBranchModal.vue`.

---

### Spec 10: Configuration Schemas, Cascade Engine & Platform Skill Injection ([`10_configuration_schema_and_cascade.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/10_configuration_schema_and_cascade.md)) — **90%**
- **Phenomenon A (Code Evolved -> Update Spec):**
  1. `§2.1`: Update from "9 Functional Subsystems" to the **16 plugin schemas** registered in [`config.rs:144-1282`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/config.rs#L144-L1282) (adding `mcp`, `rea`, `electronics`, `desigma`, `solidcraft`, `pcbcraft`, `project_management`, `semantic`).
  2. Fix misplaced keys in `§2.1, §3, §4`: `default_docstyle` is under `orchestrator` (`global_only`), `default_branch` (`vcs`) and `context_warning_threshold_percent` (`accounts`) are `global_only`, and `vcs.auto_approve` does not exist.
  3. `§5.1-5.3`: Update `CURRENT_PLATFORM_VERSION` to `"0.2.6"`, list all 12 `SPECIALIZED_SKILLS` ([`skill.rs:26-39`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/skill.rs#L26-L39)), and update `SkillStatus` to its 7 actual fields ([`skill.rs:215-226`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/skill.rs#L215-L226)).

---

### Spec 11: Remote VCS Integration ([`11_remote_github_integration.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/11_remote_github_integration.md)) — **98%**
- **Phenomenon A (Code Evolved -> Update Spec):**
  1. Document `kopeng/gitlab.rs` (`glab` CLI remote inspection, creation, and URL slug parsing) and the extended `install_missing_tools` package catalog in `diagnostic.rs:122-135`.

---

### Spec 12: Streamable HTTP MCP Daemon ([`12_streamable_http_mcp_daemon.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/12_streamable_http_mcp_daemon.md)) — **95%**
- **Phenomenon A (Code Evolved -> Update Spec):**
  1. `§6.2 L171-175`: Remove outdated note claiming the desktop app spawns agents/owns the supervisor (contradicts `§7.2`).
  2. `§7.1`: Document all 4 MCP gateway routes mounted in [`mcp.rs:135-138`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/mcp.rs#L135-L138): `/mcp`, `/mcp/rea`, `/mcp/desigma`, and `/mcp/pcbcraft`.
  3. `§3`: Add the 16 MCP tools registered in `mcp.rs` that are missing from the Spec 12 catalog table (or cross-reference Specs 20, 22, 29, 30, 34).
  4. `§7.3`: Document the real-time build progress telemetry file `$XDG_RUNTIME_DIR/oneship-dev-build.json` streamed by `scripts/run_dev.sh` to `integrations/omarchy-bar/ControlPanel.qml`.

---

### Spec 13: Design System & UI Component Library ([`13_design_system_and_ui_components.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/13_design_system_and_ui_components.md)) — **90%**
- **Phenomenon A (Code Evolved -> Update Spec):**
  1. `§1`: Add `--status-info-color` (`--status-info-border`, `--status-info-bg`).
  2. `§2.7`: Update `AppInitBadge.vue` documentation to reflect semantic CSS variables, `[! CONFIG]` and `[! REMOVAL]` badge variants, and `✕` dismiss button.
  3. `§3.5`: Google Account Switcher and DeepSeek Wallet (`AccountQuotaControls.vue`) reside in `AgentTaskbar.vue`, not `AppHeader.vue`.
  4. `§3.6`: Update workspace navigation rail list from 7 tabs to the 14 tabs in `ProjectWorkspace.vue`.

---

### Spec 16: Agent Session Snapshot System ([`16_agent_session_snapshot_system.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/16_agent_session_snapshot_system.md)) — **75%**
- **Phenomenon A (Code Evolved -> Update Spec):**
  1. `§3, §4, §5, §7, §10`: Snapshots use unconstrained labels (`conv-start:<id>`, `milestone:<id>`, `auto-edit:<asset>`, manual checkpoints), millisecond ref timestamps (`refs/one-ship/snapshots/<project_id>/<ts_ms>-<conv>-<label>`), `files_changed` diffed against `HEAD` or transcript-scoped files, optional `project`/`conversation_id` in MCP schemas, and an active UI in `ProjectGitSnapshotsTab.vue`.
  2. Document `try_create_auto_domain_snapshot` and `prune_domain_asset_snapshots` (`snapshots.rs:411-620`) for CAD/PCB/Design binary assets.
- **Phenomenon B (Unimplemented / Replaced in Code -> Decision Required):**
  1. `§3.1, §3.2, §6`: Watcher-based `PRE` (first file write) and `POST` (`~/.gemini/antigravity-cli/presence/*.lock` deletion) triggers in `watcher.rs` were never implemented; instead, snapshots are taken at conversation start (`conversation.rs:309`) and at session minutes recording (`mcp.rs:4677`). **Decision needed in WP-5:** Confirm that conversation-start + milestone + domain-asset snapshots are the definitive design and remove the `presence/*.lock` watcher section from Spec 16.

---

### Spec 17: Multi-Account Management ([`17_multi_account_management.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/17_multi_account_management.md)) — **88%**
- **Phenomenon A (Code Evolved -> Update Spec):**
  1. Remove obsolete sections `§7.3` (in-chat mismatch banner), `§7.4` (`switch_agent_account`), and `§13` (`switch_pane_account`).
  2. `§5.1`: `watcher.rs` polls `Default_keyring.keyring` mtime rather than `inotify`.
  3. `§14.2`: DeepSeek balance badge lives in `AccountQuotaControls.vue` inside `AgentTaskbar.vue`, and `sync_opencode_auth_json` syncs `~/.local/share/opencode/auth.json`.
- **Phenomenon B (Unimplemented in Code -> Decision Required):**
  1. `§18.1`: Spec states switching a Claude account decrypts credentials and writes `~/.claude/.credentials.json`, but [`accounts.rs:703-706`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/accounts.rs#L703-L706) only flips `is_active` in SQLite without writing `~/.claude/.credentials.json`. **Decision needed in WP-5:** Implement writing `~/.claude/.credentials.json` on Claude account switch, or update `§18.1`.

---

### Spec 18: MCP Dialogue Channel & Chat Overlay ([`18_mcp_dialogue_channel_and_chat_overlay.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/18_mcp_dialogue_channel_and_chat_overlay.md)) — **90%**
- **Phenomenon A (Code Evolved -> Update Spec):**
  1. `§1.2, §2, §4.1-4.4`: Replace references to decommissioned components (`ChatFab.vue`, `AgentDialogueOverlay.vue`, `AgentList.vue`, `AgentDialogueWorkspace.vue`) with `FloatingChatOverlay.vue` (docked + floating modes) and `AgentTaskbar.vue`.
  2. `§3.1`: Add `file_path` parameter to `post_agent_response`.
  3. `§4.5`: Update store ownership (`workspaceLayoutStore.ts`) and 14 `WorkspaceTab` values; remove claim of legacy `kapawu:` localStorage fallback.
  4. `§5`: Mark Phase 2E compliance reminder as implemented (triggered at turn completion in `supervisor.rs`).

---

### Spec 19 & Spec 23: Agent Quota, Context Observability & Kickstart ([`19_agent_quota_and_context_observability.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/19_agent_quota_and_context_observability.md), [`23_token_economy_and_quota_kickstart.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/23_token_economy_and_quota_kickstart.md)) — **93%**
- **Phenomenon A (Code Evolved -> Update Spec):**
  1. Synchronize struct/table field names (`reset_epoch_seconds`, `tokens_used`, `cost_dollars`, `max_context_tokens`, `*_resets_at_local`).
  2. Document the 5 additional daemon commands in `stats.rs`, the `agent-usage-update` and `keyring-warmup-status` SSE events, `canonical_pool_identity`, and the `openpty` `agy -i "hi"` probe mechanism.
  3. Fix `Spec 23 §3.1` config header from `[system]` to `[plugins.accounts]`.

---

### Spec 20: Live Artifacts & Project Scratchpad ([`20_live_artifacts_and_project_scratchpad.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/20_live_artifacts_and_project_scratchpad.md)) — **85%**
- **Phenomenon A (Code Evolved -> Update Spec):**
  1. Explorer & reader UI was consolidated from `ArtifactPanel.vue` / `DocTreeNode.vue` into `ProjectWorkspace.vue` (`files` tab) and `FloatingChatOverlay.vue`, with desktop multi-select (`Ctrl/Shift+click`), context menu, inline file/folder creation & rename, side-by-side diff, Wayland clipboard image paste, and real-time `notify` file watching (`project-files-changed`).
- **Phenomenon B (Unimplemented in Code -> Decision Required):**
  1. `§2.2 & §9`: Phase 2 Scratchpad >7-day auto-pruning and `[Purge Scratchpad]` button are unimplemented. **Decision needed in WP-5:** Implement `[Purge Scratchpad]` or remove Phase 2 from Spec 20.

---

### Spec 21: Agent Pixel Portrait DNA ([`21_agent_pixel_portrait_dna.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/21_agent_pixel_portrait_dna.md)) — **80%**
- **Phenomenon A (Code Evolved -> Update Spec):**
  1. Update catalog asset counts to match `public/portraits/manifest.json` (`B01–B14`, `BD00–BD05`, `H00–H21`, `A00–A16`) and document `AppAgentPortrait.vue` props (`dna`, `agentId`, `size`).
- **Bugs to Fix (`B-09`):** Fix `dna.rs` gender inversion and out-of-bounds indices, and either populate expression paths or remove dead expression layer code.

---

### Spec 22: Session Lifecycle, Devlog & Knowledge Graph ([`22_session_lifecycle_and_devlog_graph.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/22_session_lifecycle_and_devlog_graph.md)) — **82%**
- **Phenomenon A (Code Evolved -> Update Spec):**
  1. `§2.1-2.2`: The early `unengaged_draft` / `trampoline` lifecycle was superseded by Spec 30 (Threads, Conversations & Native Sessions).
  2. `§3 & §4`: Synchronize SQLite columns (`context_pct`, `total_tokens`, `conversation_id`, `recommended_option`, `options_json`) and MCP schemas (`record_session_minutes` string fields, `resolve_todo_item`, `resolve_technical_debt`, and real-time `index_single_journal_entry`).
- **Phenomenon B (Unimplemented in Code -> Decision Required):**
  1. `§5.1-5.5`: Spec claims Tree-sitter AST chunking, two-tier SHA-256 incremental diffing, background `nice 19` indexing loop, live `get_indexing_status` progress, and `.pdf` indexing; actual code in `src-tauri/src/semantic/` uses line-prefix heuristics (`pub fn `, `def `), full re-indexing on trigger, a static `is_indexing: false` stub, and `.md`/`.txt` only. **Decision needed in WP-5:** Update Spec 22 §5 to match the current lightweight chunker + fix `B-01`/`B-02`, or implement SHA-256 incremental indexing / real `is_indexing` status tracking.

---

### Spec 24: Project Versioning & Migration Engine ([`24_project_versioning_and_migration_engine.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/24_project_versioning_and_migration_engine.md)) — **85%**
- **Phenomenon A (Code Evolved -> Update Spec):**
  1. Align platform version (`0.2.6` in `skill.rs:8` vs `0.2.4` in `mcp.rs:319`), remove references to the retired global `system` agent in UI, update `resolve_initialization_flag(flag_id, status)` signature, and document the `removal` (`removal-resolver`), `invalid_config` (`config-resolver`), and de-integration/export workflows (`manifest.rs`).

---

### Spec 25: Universal Telemetry & Continuous Profiling ([`25_universal_telemetry_and_profiling_engine.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/25_universal_telemetry_and_profiling_engine.md)) — **98%**
- Fully aligned (minor return type `Result<bool, ProfilerError>` on `toggle_project_profiler_plugin`).

---

### Spec 26a & Spec 26b: Greenfield Bootstrapping & REA Integration ([`26_greenfield_project_creation_and_conversational_bootstrapping.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/26_greenfield_project_creation_and_conversational_bootstrapping.md), [`26_rea_reverse_engineering_integration.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/26_rea_reverse_engineering_integration.md)) — **94%**
- **Phenomenon A (Code Evolved -> Update Spec):**
  1. Resolve duplicate `26_` numbering if desired (e.g. keep or renumber REA to `27_rea_reverse_engineering_integration.md`).
  2. In Spec 26a, update docstyle skill names to the actual skills in `templates/skills/` (`oneship-docstyle-sdd`, `oneship-docstyle-lean`, `oneship-docstyle-contract`, `oneship-docstyles-crafter`) and remove invalid `agent_id: "initializer"` from the `post_agent_response` example in `initializer.rs:295, 308`.

---

### Spec 29: On-Demand Skill Manager ([`29_on_demand_skill_manager_and_mcp_protocol.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/29_on_demand_skill_manager_and_mcp_protocol.md)) — **90%**
- **Phenomenon A (Code Evolved -> Update Spec):**
  1. Update `manage_project_skill` schema (uses `CallerContext` instead of `agent_id`), document SkillsMP-only search, and list the 7 skill daemon commands and 12 `SPECIALIZED_SKILLS`.

---

### Spec 30: Conversation & Context Continuity ([`30_agent_thread_conversation_and_context_continuity.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/30_agent_thread_conversation_and_context_continuity.md)) — **92%**
- **Phenomenon A (Code Evolved -> Update Spec):**
  1. Update header (line 3), `§2` table, and `§9 L183` to reflect that Phases A–E, `BriefStateReport`, and `BriefOpenItems` (`src-tauri/src/brief.rs`) are all implemented.

---

### Spec 31: Electronics Engineering Workstation ([`31_electronics_workstation_and_instrumentation_engine.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/31_electronics_workstation_and_instrumentation_engine.md)) — **55%**
- **Phenomenon A (Code Evolved -> Update Spec):**
  1. `§5.1`: `toolchain.rs` probes `platformio`, `probe-rs`, `esptool`, `arduino-cli`, and `sigrok-cli`.
  2. `discovery.rs`: Implements `/sys/class/tty` serial port scanning (`list_serial_ports`).
- **Phenomenon B (Major Unimplemented Sections -> Maintainer Scope Decision Required in WP-6):**
  1. **`§3` `HardwareInstrument` Trait:** Drivers (`owon_vds1022.rs`, `fx2lafw.rs`) are standalone modules; no `pub trait HardwareInstrument` abstraction exists.
  2. **`§5.3` Live Serial Monitor (Frontend Mock Violation of `AGENTS.md §4.1`):** `ProjectElectronicsTab.vue:281-305` fakes a serial console in the browser by echoing user strings into a local array without opening any serial port in Rust.
  3. **`§6` Circuit Schematic & EDA Ingestion:** No EAGLE `.sch`/`.brd` XML parser, no KiCad `.kicad_sch` parser, no KiCanvas viewer, and no semantic pinout extractor exist in `kopeng/electronics/`.
  4. **`§7` Headless SPICE Simulation:** No `ngspice` runner exists.
  5. **`§8` MCP Tool Catalog:** None of the 6 `electronics_*` MCP tools (`electronics_detect_hardware`, `electronics_read_schematic`, `electronics_capture_logic`, `electronics_read_oscilloscope`, `electronics_flash_firmware`, `electronics_run_simulation`) exist in `src-tauri/src/mcp.rs`.

---

### Spec 32: One-Ship Designer Integration ([`32_oneship_designer_integration.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/32_oneship_designer_integration.md)) — **95%**
- **Phenomenon A (Code Evolved -> Update Spec):**
  1. Document the full `[plugins.desigma]` schema (`design_dir`, `enable_bridge` defaulting to `false`, `mcp_request_timeout_ms`, `auto_snapshot_on_edit`, `max_snapshots_per_asset`), `get_project_desigma_status`, `toggle_project_desigma_plugin`, domain auto-snapshots, and `oneship-desigma` skill.

---

### Spec 33: CAD (`SolidCraft`) & EDA/PCB (`PCBCraft`) Integration ([`33_cad_and_pcb_integration.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/33_cad_and_pcb_integration.md)) — **75%**
- **Phenomenon A (Code Evolved -> Update Spec):**
  1. Document the 7 supported SolidCraft camera views (`iso, front, back, top, bottom, left, right`), `SolidcraftStatus` fields, `auto_snapshot_on_edit` / `max_snapshots_per_asset`, `preview_cache.rs`, and `hardwarePreviewStore.ts`.
- **Phenomenon B (Unimplemented in Code -> Decision Required in WP-6):**
  1. `§2.3, §3.3, §4`: SPICE / EIS Netlist Bridge (`spice_engine`, `xyce_binary_path`, `Run SPICE / EIS Sweep` button in `ProjectPcbTab.vue`) and Stages D & E are not implemented.
  2. No `oneship-solidcraft` skill exists in `templates/skills/` (unlike `oneship-desigma` and `oneship-pcbcraft`).

---

### Spec 34: Project Management & OSS Collaboration ([`34_project_management_and_oss_collaboration.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/34_project_management_and_oss_collaboration.md)) — **98%**
- **Phenomenon A (Code Evolved -> Update Spec):**
  1. Document automatic injection/cleanup of `oneship-project-mgmt` skill on `toggle_project_management_plugin` and the `link_task_relation` / `unlink_task_relation` daemon commands.

---

## 4. Unregistered Subsystems & Features (Implemented Without Spec Coverage)

| Subsystem / Feature | Implementation Files | Recommended Target Spec |
| :--- | :--- | :--- |
| **1. Responsive Web Preview Tab (`[preview] url`)** | `src-tauri/src/hydration.rs:86-93`, `src/components/workspace/ProjectPreviewTab.vue` | Add to **Spec 01** or **Spec 13** (or create `Spec 27: Live Web Preview Viewport`). |
| **2. Hyprland Workspace & Window Automation** | `src-tauri/src/desktop.rs:117-224` (`find_empty_hyprland_workspace`, `wait_and_move_hyprland_window`) | Document in **Spec 32** & **Spec 33** (and **Spec 12 §7**). |
| **3. SHA-256 Hardware/Design Preview Cache** | `src-tauri/src/kopeng/preview_cache.rs`, `src/stores/hardwarePreviewStore.ts` | Document in **Spec 32** & **Spec 33**. |
| **4. Live Cargo Build Progress Telemetry** | `scripts/run_dev.sh:55-109`, `integrations/omarchy-bar/ControlPanel.qml:59-94` (`oneship-dev-build.json`) | Document in **Spec 12 §7.3**. |
| **5. Project Favorites, Archiving & Tab Visibility** | `src-tauri/src/registry.rs`, `src-tauri/src/db.rs:964-1035`, `ProjectVerticalRail.vue` | Document in **Spec 01 §1** & **Spec 13 §3.6**. |
| **6. Linux `/proc` Port & Socket Inspector** | `src-tauri/src/port_inspect.rs:1-281` | Document in **Spec 01 §2** & **Spec 08 §2**. |
| **7. Operator Binary Attestation (`require_app_descendant`)** | `src-tauri/src/caller_identity.rs:255-300` | Document in **Spec 12 §7.1**. |
