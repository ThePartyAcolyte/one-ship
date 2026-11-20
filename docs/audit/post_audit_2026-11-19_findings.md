# ONE-SHIP Post-Audit Findings Report (`2026-11-17` to `2026-11-19`)

**Audit Date:** `2026-11-19` | **Baseline Reference:** [`2026-11-16` Audit Suite](audit_00_index.md)  
**Scope:** 35 session logs (`devlog/2026-11-17_*.md`, `2026-11-18_*.md`, `2026-11-19_*.md`) verified read-only against `src-tauri/`, `src/`, `docs/specs/`, `templates/`, `TODO.md`, and `TECHNICAL_DEBT.md`.  
**Execution Method:** Read-only code inspection and targeted `grep` scans (no `git`, `cargo`, `npm`, or runtime processes executed).

---

## 1. Summary & Root Cause Analysis

While major post-audit initiatives (Forge backend Spec 39, Semantic tree-sitter + incremental SHA-256 pipeline, Git staging + AI commit, Project milestones) landed with strong adherence to `spawn_blocking`, `API_COMMANDS` mirroring, and TypeScript `any` elimination, recurring regressions appeared across sessions:

| Root Cause Pattern | Count | Description |
| :--- | :---: | :--- |
| **RC-1: Silent Fallback / Defaulting** | 9 | Using `.unwrap_or(...)`, `.unwrap_or_default()`, `.filter().unwrap_or(...)`, or `catch {}` instead of failing explicitly (`§1.11`, `§13.3`). |
| **RC-2: Unverified Devlog / Stale Debt Sync** | 7 | Devlogs claiming UI migrations or debt closures that are incomplete in code, or dangling `#39`–`#44` references (`§1.10`, `§3`). |
| **RC-3: Hardcoded Operational Constants** | 4 | Magic limits, timeouts, and thresholds kept outside `config.rs` (`§11.1`, `§13.2`). |
| **RC-4: Unbounded External Processes** | 3 | Calling `cmd.output()` / `.output().await` on `glab`, `git`, `mpremote`, `pio` without timeouts (`§8.4`). |
| **RC-5: Incomplete Layer Propagation** | 4 | Client-side regex TOML edits, missing `project_path` ownership checks, or legacy shims/dead commands left behind (`§1.4`, `§1.10`). |
| **RC-6: Unmigrated UI Tokens / Components** | 4 | Hardcoded Tailwind palettes (`bg-black/*`, `text-amber-*`, `bg-[#09090b]`), `text-[7px]`, or raw `<input>` (`§5`, `§11.2`). |

---

## 2. Verified Compliant Areas

- **Forge Backend (`Spec 39`):** `src-tauri/src/kopeng/forge/{mod,github,gitlab,inbox}.rs` implemented; all commands registered in `api.rs` and mirrored in `src/types/daemonApi.ts`; uses `ForgeError` (`thiserror`) and `spawn_blocking`; temporary frontend `localStorage` Gantt and `project/timeline`/`project/github` mappings removed.
- **Semantic Pipeline:** Scan, SHA-256, tree-sitter, and `db_replace_project_knowledge_edges` run inside `spawn_blocking` with DB transactions and poison-safe locks (`engine.rs`, `throttler.rs`).
- **Git Staging & AI Commit:** 6 commands registered and mirrored; `validate_rel_paths` guards against `-`, `..`, and absolute paths; `GitError` typed.
- **Global Code Hygiene (`grep` verified):**
  - Zero `: any` / `as any` / `<any>` in `src/` (excluding `src/types/generated/`).
  - Zero native `title="..."` attributes on interactive elements in `src/` (only `AppEmptyState` `title` props).
  - Zero `.lock().unwrap()` / `.read().unwrap()` / `.write().unwrap()` in production Rust paths (only `gitlab.rs:700` inside `#[cfg(test)]`).
  - Zero hardcoded `/home/butcherwutcher` paths in `src/` or `src-tauri/src/`.
  - Zero `backdrop-blur-*` classes outside `src/styles/ui.css` and zero legacy CSS tokens (`--surface-color`, `--card-bg`, etc.).
  - Listener teardowns verified in `RollingStatusLog.vue`, `ProjectConfigPanel.vue`, `forgeCollabStore.ts`, `voiceControllerStore.ts`, `voiceStore.ts`, `workspaceLayoutStore.ts`, `hardwarePreviewStore.ts`, and `AppDatePicker`.

---

## 3. High Severity Findings

| ID | Location | Rule Violated | Root Cause | Description | Status |
| :--- | :--- | :--- | :--- | :--- | :---: |
| **PA-A01** | `src-tauri/src/voice_controller.rs:1537-1540` | `§1.11`, `§1.4` | RC-1, RC-5 | `surface_execute_action` silently degrades invalid `params_json` to `json!({})` via `.unwrap_or_else(|_| json!({}))` and keeps a legacy `args.get("params")` compatibility shim. Must return a structured error on invalid JSON and remove `params`. | `RESOLVED (WP-A)` |
| **PA-A02** | `src-tauri/src/voice_controller.rs:196-209` | `§1.11` | RC-1 | `test_voice_controller_voice` silently substitutes `cfg.model`, `cfg.voice_name`, and `cfg.persona_style` when invalid values are passed instead of returning a typed error. | `RESOLVED (WP-A)` |
| **PA-A03** | `src-tauri/src/config.rs:2270-2300` | `§1.11`, `§13.3` | RC-1 | `get_voice_controller_config` silently falls back to defaults (`"Fenrir"`, `enabled = true`, etc.) when `model`, `voice_name`, `thinking_level`, or `persona_style` are invalid in `config.toml`, with no warning or error. | `RESOLVED (WP-A)` |
| **PA-A04** | `src/components/AppHeader.vue:89-99` | `§1.10`, `§9.6` | RC-5 | `toggleGlobalVoiceController` edits `config.toml` client-side using regex `/\[voice_controller\]/` and appends `[voice_controller]` at root if missing. Backend reads `[plugins.voice_controller]` (`config.rs:2265`), making the header toggle ineffective and risking duplicate keys. Requires a typed daemon command. | `RESOLVED (WP-A)` |
| **PA-A05** | `src/components/workspace/electronics/OscilloscopeView.vue` (lines `1426, 1444, 1860, 1960, 2208, 2227`, etc.) | `§11.2`, Spec 13 §0 | RC-2, RC-6 | Devlog `2026-11-19_task_dates_exclusions_electronics_ui_and_f5.md` claims `OscilloscopeView.vue` was migrated to Spec 13 tokens and `#41` resolved, but the file still contains 57 Tailwind palette classes (`text-amber-400`, `text-emerald-400`, `border-orange-500/40`, `border-fuchsia-500/40`, `border-sky-500/30`, `bg-rose-950/40`, etc.) and 10 `bg-black/*` classes. `#41` is absent from `TECHNICAL_DEBT.md`. | `RESOLVED (WP-B)` |

---

## 4. Medium Severity Findings

| ID | Location | Rule Violated | Root Cause | Description | Status |
| :--- | :--- | :--- | :--- | :--- | :---: |
| **PA-M01** | `src-tauri/src/voice_controller.rs:1233, 1548, 2118, 2177, 2199` (~20 sites) | `§9.3` | RC-5 | `execute_core_tool` helpers (`resolve_project_info`, `search_project_files`, `list_project_directory`, `flatten_blocking_tool_result`) return `Result<_, String>` instead of extending `VoiceControllerError` (`thiserror`). | `RESOLVED (WP-A)` |
| **PA-M02** | `src-tauri/src/voice_controller.rs:1580` | `§13.3` | RC-1 | `let _ = tokio::task::spawn_blocking(...)` for `SetSlotValue files.selected_document_path` discards errors and still returns `ok: true`. | `RESOLVED (WP-A)` |
| **PA-M03** | `src-tauri/src/voice_controller.rs:1506` & `1913, 1935, 1951, 1972, 1987, 2015, 2049` | `§1.11`, `§8.2` | RC-1 | `surface_get_capabilities` discards `Some(_project_id)` without scoping; tool handlers cast `project_id: i64` directly via `project_id as usize` without validating `> 0`. | `RESOLVED (WP-A)` |
| **PA-M04** | `src-tauri/src/kopeng/project_mgmt.rs:131-152, 1177` | `§1.11`, `§13.3` | RC-1 | `resolve_tracking_mode` and `is_project_management_enabled` silently fall back to `"linear"` / `false` on invalid `rula.toml` values or unreadable files without logging or erroring. | `RESOLVED (WP-A)` |
| **PA-M05** | `src-tauri/src/config.rs:1722-1735` | `§1.11`, `§13.3` | RC-1 | `read_webkit_renderer_mode` silently falls back to `Auto` on `config.toml` read/parse failure or invalid value (`unwrap_or(Auto)`). | `RESOLVED (WP-A)` |
| **PA-M06** | `src-tauri/src/kopeng/forge/inbox.rs:211, 350, 399` | `§1.11` | RC-1 | Uses `ctx.target_repo.unwrap_or_default()` when building GitLab/GitHub inbox items and DB rows instead of returning an explicit error if `target_repo` is absent. | `RESOLVED (WP-B)` |
| **PA-M07** | `src-tauri/src/kopeng/forge/gitlab.rs:21, 35, 99-110` | `§8.4` | RC-4 | `run_glab_api` and `git` helpers invoke `cmd.output()` with no deterministic timeout. | `RESOLVED (WP-B)` |
| **PA-M08** | `src-tauri/src/kopeng/electronics/firmware.rs:578` (~600+) | `§8.4` | RC-4 | `mpremote` and `pio` `.output().await` invocations lack a timeout ceiling; a hung upload waits indefinitely. | `RESOLVED (WP-B)` |
| **PA-M09** | `src-tauri/src/db.rs:5078-5112, 5167-5230` | `§9.4` | RC-5 | `db_insert_project_milestone`, `db_update_project_milestone`, and `db_delete_project_milestone` perform `SELECT` + write without a transaction (`MAX(sort_order)+1` TOCTOU). | `RESOLVED (WP-A)` |
| **PA-M10** | `src-tauri/src/kopeng/project_mgmt.rs:1285, 1317` | `§1.11`, `§1.10` | RC-5 | Daemon commands `update_project_milestone` and `delete_project_milestone` accept bare `milestone_id` without `project_path` ownership validation (inconsistent with `mcp.rs:3541-3544`). | `RESOLVED (WP-A)` |
| **PA-M11** | `src-tauri/src/semantic/mod.rs:719-721, 1026-1028` | `§9.2` | RC-3 | Calls `libc::nice(19)` inside Tokio `spawn_blocking` worker threads (Linux `nice` is per-thread and threads are reused by unrelated commands) and ignores the return value. *(Thread reuse impact inferred from Linux behavior.)* | `RESOLVED (WP-B)` |
| **PA-M12** | `src-tauri/src/semantic/{mod.rs,chunker.rs,store.rs,throttler.rs,engine.rs}` | `§11.1`, `§13.1`, `§13.2` | RC-3 | Runtime knobs (`embedding_batch_size_count`, `embedding_batch_sleep_ms`, `throttler_*`, `onnx_intra_threads_count`, `max_relations_per_result_count`) moved to `config.rs`; internal `chunker.rs`/`store.rs` heuristics and debounce clamps remain internal constants (`PARTIAL`). | `PARTIAL (WP-B)` |
| **PA-M13** | `src-tauri/src/semantic/mod.rs:979-983, 1456, 1471-1491` & `chunker.rs:1213` | `§1.11`, `§13.3` | RC-1 | Transient file read errors omit files from `retained_ids`, purging their existing indexed vectors as orphans *(inferred from control flow)*. PDFs bypass `exceeds_max_file_size` and are read twice. | `RESOLVED (WP-B)` |
| **PA-M14** | `src-tauri/src/api.rs:258, 574`, `desktop.rs:65-75`, `daemonApi.ts:421` | `§1.4` | RC-5 | `open_system_terminal` remains registered across daemon/TS/specs (`Spec 08`, `Spec 12`) with zero callers in `src/` after the UI button was removed. | `RESOLVED (WP-B)` |
| **PA-M15** | `src-tauri/src/ui_state.rs:1090-1101` | `§1.4` | RC-5 | `migrate_legacy_node_path` retains a compatibility shim for `project/timeline` and `project/github*`. | `RESOLVED (WP-B)` |
| **PA-M16** | `src/components/workspace/ProjectGitSnapshotsTab.vue:141` | `§13.1` | RC-3 | Silently truncates the `Staged` file list at `MAX_RENDERED_FILES_COUNT` (250) without showing the `"Showing first..."` banner used in `Changes` and `Files`. | `RESOLVED (WP-B)` |
| **PA-M17** | `src/components/workspace/ProjectElectronicsTab.vue:909, 955` | `§11.2`, Spec 13 | RC-6 | Uses hardcoded `bg-[#09090b]` despite the devlog stating hardcoded palettes were removed. | `RESOLVED (WP-B)` |
| **PA-M18** | `src/components/workspace/ProjectConfigPanel.vue:212, 612` | `§13.3` | RC-1 | Line `212` silently swallows `get_project_agent_state` failure (`catch { projectAgents.value = [] }`); line `612` (`toggleCategory`) logs only to `console.error` with no user-facing feedback. | `RESOLVED (WP-B)` |
| **PA-M19** | `TECHNICAL_DEBT.md`, `TODO.md:135-141, 160` | `§3`, `§1.10` | RC-2 | `TECHNICAL_DEBT.md` ends at `#37`, leaving `#38`–`#44` referenced by devlogs and `TODO.md` dangling (`TODO.md:160` also reuses `#42` for date-range validation). `TODO.md:135-136` still references `GithubCollabPanel` and fallback to Board. | `RESOLVED (WP-C)` |

---

## 5. Low Severity Findings

- **`PA-B01` (`src-tauri/src/voice_controller.rs:36-40`):** `OUTBOUND_QUEUE_CAPACITY_COUNT`, `MAX_FILE_SEARCH_RESULTS_COUNT`, `MAX_DIR_LIST_ENTRIES_COUNT`, `MAX_FILE_WALK_VISITED_COUNT` kept as internal module constants per maintainer decision (`INTENTIONAL`).
- **`PA-B02` (`src/components/ProjectCard/CardHeader.vue`):** Migrated description input to `<AppInput>` and added `catch` + `reportAsyncFailure` (`RESOLVED (WP-C)`).
- **`PA-B03` (`src/components/AppHeader.vue:222-225`):** Hardcoded canvas stroke fallback hexes (`#22c55e`, `#f59e0b`, `#eab308`) tracked in `TECHNICAL_DEBT.md #46` (`DEFERRED (TD #46)`).
- **`PA-B04` (`src/stores/voiceControllerStore.ts:42`):** Replaced silent `.catch(() => {})` with `console.warn` (`RESOLVED (WP-C)`).
- **`PA-B05` (`docs/specs/09_vcs_and_logging_architecture.md`, `12_streamable_http_mcp_daemon.md`):** Documented `get_system_log_sources` (`RESOLVED (WP-C)`).
- **`PA-B06` (`src-tauri/src/mcp.rs:144`):** `POST /api/v1/audit/semantic` outside `API_COMMANDS` tracked in `TECHNICAL_DEBT.md #46` (`DEFERRED (TD #46)`).
- **`PA-B07` (`src-tauri/src/main.rs:37`):** Removed dead `GTK_OVERLAY_SCROLLING=0` env var (`RESOLVED (WP-C)`).
- **`PA-B08` (`src-tauri/src/kopeng/git.rs`):** Added `GIT_TERMINAL_PROMPT=0` + configurable timeout in `git()`, raised commit prompt timeout to `>= 90_000 ms`, and replaced prefix-stripping `trim_start_matches("text"/"git")` with fence-only stripping (`RESOLVED (WP-C)`).
- **`PA-B09` (`src-tauri/src/supervisor.rs`):** Bounded `dispatch_git_instruction` log to `instruction.len()`, used `Path::file_name()` for runner binary detection, and propagated stdin errors on ACP prompt delivery (`RESOLVED (WP-C)`; `platform_agent_pair` intentional watch-tick behavior).
- **`PA-B10` (`src-tauri/src/semantic/mod.rs:144, 170, 315, 1419` & `store.rs:197-220, 291`):** `exceeds_max_file_size` `metadata` failure logged in WP-B; remaining minor `.ok()`/`.flatten()` in background debounce/Arrow column extraction deferred with `TD #46`.
- **`PA-B11` (`src/components/workspace/ProjectSkillsTab.vue:46-63`):** Replaced silent `localStorage` `catch {}` with `console.warn` (`RESOLVED (WP-C)`).
- **`PA-B12` (`QuotaGaugePreviewSelector.vue`, `AppExecutionConfigPicker.vue`, ~19 files):** Migrated all 10 `text-[7px]` occurrences in `QuotaGaugePreviewSelector.vue` and `AppExecutionConfigPicker.vue` to `text-[8px]` (`PARTIAL (WP-C)`); 7 remaining `text-[7px]` hits (`ProjectWorkspace.vue`: 4, `ProjectTerminalGrid.vue`: 3) and ~90 legacy `bg-black/*` classes are tracked in `TECHNICAL_DEBT.md #45` (`DEFERRED (TD #45)`).
- **`PA-B13` (`ProjectConfigPanel.vue`, `SettingsModal.vue`, `SubtabBar.vue`, large SFCs):** Tracked in `TECHNICAL_DEBT.md #45` (`hover:bg-white/5`) and `#46` (monolithic SFC decomposition) (`DEFERRED (TD #45/#46)`).
- **`PA-B14` (`templates/skills/{oneship-project-mgmt,oneship-desigma,oneship-pcbcraft}/SKILL.md`):** Added `version: "0.2.6"` to frontmatter (`RESOLVED (WP-C)`).
- **`PA-B15` (`docs/specs/13_*.md`, `docs/specs/39_*.md`, `ProjectManagementTab.vue:281`, `TODO.md`):** Set Spec 13 to `0.2.14`, marked Spec 39 §1 as historical pre-migration baseline, updated `ProjectManagementTab.vue` comment, and added scrollbar runtime check to `TODO.md` (`RESOLVED (WP-C)`; legacy SQLite column names tracked in `TD #46`).

---

## 6. Unverified Items (Runtime / Deep Inspection Deferred)

- `libc::nice(19)` cross-command impact on Tokio blocking pool threads, orphan purge on transient read failure, and milestone `sort_order` race were inferred from code inspection, not reproduced at runtime.
- `db_sum_conversation_journal_usage` (`db.rs:5396`) delta read-then-insert concurrency was not audited in depth.
- `export_bindings` and `cargo test` were not executed per `AGENTS.md` §1.6.
- Runtime verification checklists in `TODO.md` remain open.
