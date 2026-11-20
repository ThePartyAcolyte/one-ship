# Audit 04 — Code Quality, Engineering Standards & Portability Audit
**Audit Date:** `2026-11-16` | **Governed By:** `AGENTS.md` §5, §8, §9, §10, §11, §12

---

## Part 1: Architectural Size & Bounded Context Violations

### 1.1 Backend God-Modules (`src-tauri/src/`)
Four files represent **18,693 LOC** of the backend:
1. **[`src-tauri/src/db.rs`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/db.rs) (`5,810 LOC`):** Contains DDL migrations and CRUD for 14+ domains (projects, logs, tasks, init state/flags, delegations, personas/instances, processes/runs, snapshots, accounts/quotas, approvals, conversations/sessions/FTS5/turns/minutes, todos/tech-debt, Kanban stages/tasks/subtasks, metrics, document comments).
2. **[`src-tauri/src/mcp.rs`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/mcp.rs) (`5,416 LOC`):** Combines HTTP/SSE routing, operator origin/PID guard, a 1,300-line JSON schema literal, and 40+ tool handlers across all domains.
3. **[`src-tauri/src/supervisor.rs`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/supervisor.rs) (`5,116 LOC`):** Includes a **1,530-line `spawn_agent` function** (`L2222-3752`) embedding inline stream parsers for OpenCode ACP, AGY `stream-json`, and Claude `stream-json`.
4. **[`src-tauri/src/config.rs`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/config.rs) (`2,351 LOC`):** Mixes a 1,137-line `get_all_plugin_schemas()` definition with TOML validation, mtime caching, and runner manifest loading.

### 1.2 Frontend Store & Component Boundaries (`AGENTS.md §10.2`)
- **Pinia Stores Exceeding $\le 500$ Lines or Mixing Unrelated Domains:**
  - [`src/stores/coreStore.ts`](file:///home/butcherwutcher/Projects/dev/one-ship/src/stores/coreStore.ts) (`501 LOC`): Bundles 7 domains (daemon SSE lifecycle, hardware/service telemetry, project registry, agent states, **pending user approvals**, terminal filter state, and legacy dictation state).
  - [`src/stores/dialogueStore.ts`](file:///home/butcherwutcher/Projects/dev/one-ship/src/stores/dialogueStore.ts) (`506 LOC`) + [`src/utils/dialogueActions.ts`](file:///home/butcherwutcher/Projects/dev/one-ship/src/utils/dialogueActions.ts) (`402 LOC`) + [`src/utils/dialogueListeners.ts`](file:///home/butcherwutcher/Projects/dev/one-ship/src/utils/dialogueListeners.ts) (`343 LOC`): Circular 3-file store cluster totaling `1,251 LOC`.
  - [`src/stores/journalStore.ts`](file:///home/butcherwutcher/Projects/dev/one-ship/src/stores/journalStore.ts) (`173 LOC`): Combines `sessionJournals`, `projectTodos`, and `projectTechnicalDebt` instead of dedicated stores (`agentSessionStore`, `todoTechDebtStore`).
  - [`src/stores/projectMgmtStore.ts`](file:///home/butcherwutcher/Projects/dev/one-ship/src/stores/projectMgmtStore.ts) (`371 LOC`): Mixes Kanban stage/task management with Git branch operations.
- **Oversized Active Vue Components (> 1,000 LOC):**
  - [`src/components/workspace/ProjectWorkspace.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectWorkspace.vue) (`2,570 LOC`)
  - [`src/components/workspace/electronics/OscilloscopeView.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/electronics/OscilloscopeView.vue) (`2,239 LOC`)
  - [`src/components/dialogue/FloatingChatOverlay.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/dialogue/FloatingChatOverlay.vue) (`1,915 LOC`)
  - [`src/components/workspace/ProjectTerminalGrid.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectTerminalGrid.vue) (`1,364 LOC`)
  - [`src/components/workspace/ProjectGitSnapshotsTab.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectGitSnapshotsTab.vue) (`1,267 LOC`)
  - [`src/App.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/App.vue) (`1,112 LOC`)

---

## Part 2: Async Runtime Purity Violations (`AGENTS.md §9.2`)

Synchronous disk I/O (`std::fs`), synchronous SQLite/DuckDB queries, and synchronous subprocesses (`std::process::Command`) executed directly inside `async fn` without `tokio::task::spawn_blocking`:

1. **[`src-tauri/src/mcp.rs:1664-1716`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/mcp.rs#L1664-L1716) (`handle_rpc_method`):** Dispatches **21 synchronous tool handlers** (`execute_discover_project`, `execute_query_project_task`, `execute_report_task_status`, `execute_query_project_agents`, `execute_list_project_scripts`, `execute_query_system_logs`, `execute_inspect_terminal_output`, `execute_query_project_audit`, `execute_create_snapshot`, `execute_list_snapshots`, `execute_restore_snapshot`, `execute_list_document_comments`, `execute_resolve_document_comment`, `execute_manage_todo_item`, `execute_resolve_todo_item`, `execute_query_project_tasks`, `execute_link_journal_to_task`, `execute_record_technical_debt`, `execute_resolve_technical_debt`, `execute_raise_initialization_flag`, `execute_resolve_initialization_flag`, `execute_list_profiler_runs`, `execute_get_run_profile`, `execute_compare_profiler_runs`, `execute_query_session_journal`, `execute_search_conversations`, `execute_reopen_conversation`) directly on the Tokio worker thread.
2. **[`src-tauri/src/mcp.rs:47-60, 2198, 2383, 2621, 4057, 4355, 4577, 5051`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/mcp.rs#L4057-L4294):** Async MCP handlers (`execute_post_agent_response`, `execute_record_session_minutes`, `execute_manage_project_skill`, `submit_approval_handler`) call synchronous `crate::db::*`, `create_snapshot` (`git` CLI), and `skill::*` (`std::fs`) directly.
3. **[`src-tauri/src/semantic/mod.rs:64-74, 134-322`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/semantic/mod.rs#L134-L322):** `index_project_knowledge` (registered via `call!` instead of `blocking!` in `api.rs:496`) executes synchronous `sysinfo` refreshes, SQLite queries, and recursive `std::fs::read_dir` / `std::fs::read_to_string` across the entire repository on the async worker thread.
4. **[`src-tauri/src/supervisor.rs:1590-3752, 4068-4277, 4569-4643`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/supervisor.rs#L1590-L1917):** `spawn_service`, `spawn_task`, `spawn_agent`, `launch_agent`, and `prompt_agent` execute synchronous `resolve_executable` (`Command::new("mise")`) and dozens of `crate::db::*` calls directly inside `async fn`.
5. **[`src-tauri/src/kopeng/accounts.rs:629-771`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/accounts.rs#L629-L771) & [`stats.rs:940-1159`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/stats.rs#L940-L1159):** `switch_account`, `initiate_add_account`, `probe_account_quota`, `run_quota_probe`, and `KeyringRestoreGuard::drop` execute synchronous `secret-tool` subprocesses and SQLite queries on the async executor.
6. **[`src-tauri/src/kopeng/`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng) (`resolution.rs:131`, `flag_resolver.rs:313`, `initializer.rs:207, 435, 467`, `profiler.rs:460`, `project_mgmt.rs:350`, `snapshots.rs:562`, `documents.rs:1168`, `github.rs:366, 407`, `electronics/mod.rs:101`, `electronics/instruments/mod.rs:48`, `desigma.rs:523`, `solidcraft.rs:531`, `pcbcraft.rs:645`, `rea.rs:665`):** Run synchronous `read_project_rula`, `db_get_project_by_path`, `rusb::devices()`, `serialport::available_ports()`, or `canonicalize()` inside `async fn` prior to `spawn_blocking`.

---

## Part 3: Unbounded Memory, Unbounded Loops, Locks & Error Handling (`AGENTS.md §8, §9`)

### 3.1 Unbounded Memory & Loops (`AGENTS.md §8.3, §8.4`)
- **[`src-tauri/src/supervisor.rs:2042-2081`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/supervisor.rs#L2042-L2081) (`spawn_task`):** `collected_stdout` and `collected_stderr` (`Vec<String>`) push every line of task output without a line/byte cap.
- **[`src-tauri/src/supervisor.rs:2701-2703`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/supervisor.rs#L2701-L2703) (`spawn_agent`):** `accumulated_response` and `accumulated_thought` grow via `.push_str()` without an upper byte bound.
- **[`src-tauri/src/mcp.rs:4510-4553`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/mcp.rs#L4510-L4553) (`await_user_approval`):** `rx.await` waits indefinitely without applying `get_approval_ttl_ms()` (`mcp.rs:37`), leaking entries in `APPROVAL_PENDING` if an agent disconnects.
- **[`src-tauri/src/kopeng/electronics/instruments/owon_vds1022.rs:522-529`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/electronics/instruments/owon_vds1022.rs#L522-L529) (`flush_endpoint`):** `while let Ok(n) = handle.read_bulk(...)` has no `max_attempts_count` ceiling.
- **[`src-tauri/src/kopeng/documents.rs:274-346, 479-555`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/documents.rs#L274-L346) & [`semantic/mod.rs:357-439`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/semantic/mod.rs#L357-L439):** Directory walks lack node count / recursion depth ceilings.
- **Frontend Unbounded Arrays:** `serialLines.value.push(...)` in [`ProjectElectronicsTab.vue:284`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectElectronicsTab.vue#L284) and `rawOutput.value + '\n' + line` in [`ProjectTerminalGrid.vue:764-766`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectTerminalGrid.vue#L764-L766).

### 3.2 Production `.unwrap()`, Lock Poisoning & Atomic Races (`AGENTS.md §8.2, §9.1`)
- **Production `.unwrap()` calls:**
  - [`src-tauri/src/kopeng/electronics/instruments/fx2lafw.rs:57`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/electronics/instruments/fx2lafw.rs#L57): `!which_sigrok.unwrap().status.success()`
  - [`src-tauri/src/kopeng/electronics/instruments/owon_vds1022.rs:796-797`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/electronics/instruments/owon_vds1022.rs#L796-L797): `.min().unwrap()` / `.max().unwrap()`
  - [`src-tauri/src/mcp.rs:259, 2599-2600`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/mcp.rs#L2599-L2600): `serde_json::to_value(s).unwrap(); v.as_object_mut().unwrap()`
  - [`src-tauri/src/supervisor.rs:592`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/supervisor.rs#L592): `project_path.unwrap()`
- **Non-Poison-Safe Lock Accesses:**
  - [`src-tauri/src/semantic/engine.rs:31-33, 70-72, 95-97`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/semantic/engine.rs#L31-L33): Propagates `PoisonError` as `SemanticError::EngineInit` instead of recovering via `into_inner()`.
  - [`src-tauri/src/supervisor.rs:2588, 2629, 2755, 3008, 3259`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/supervisor.rs#L2588): Uses `.read().ok()` / `if let Ok(mut g) = sid_arc.write()` instead of `.read_safe()` / `.write_safe()`.
  - [`src-tauri/src/kopeng/electronics/instruments/owon_vds1022.rs:246, 279, 717, 735, 887`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/electronics/instruments/owon_vds1022.rs#L246) & [`documents.rs:427, 436, 473`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/documents.rs#L427): Uses `if let Ok(guard) = mutex.lock()`.
- **Non-Atomic Lock Acquisition Race (`arbitrator.rs`):**
  - [`src-tauri/src/kopeng/electronics/instruments/arbitrator.rs:31-45`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/electronics/instruments/arbitrator.rs#L31-L45): `HardwareLockArbitrator::acquire_lock` uses separate `lock.load(Ordering::SeqCst)` and `lock.store(now, Ordering::SeqCst)` instead of `compare_exchange`.

### 3.3 Unstructured Error Types & Non-Standard `ApiError::code()` (`AGENTS.md §9.3, §9.6`)
- [`src-tauri/src/config.rs:1742-1746`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/config.rs#L1742-L1746): `pub fn resolve_project_subdir(...) -> Result<(PathBuf, String), String>` returns `Result<..., String>` instead of `ConfigError`.
- [`src-tauri/src/semantic/errors.rs:40-49`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/semantic/errors.rs#L40-L49): `SemanticError::code()` returns `"SEMANTIC_ENGINE_INIT_ERROR"` instead of `"SemanticError::EngineInit"`.

---

## Part 4: Tiger Style Suffixes, Hardcoded Constants & Machine Paths (`AGENTS.md §8.1, §11.1`)

### 4.1 Hardcoded Developer Machine Paths (`/home/butcherwutcher/...`) Blocking Portability
Seven hardcoded paths in runtime source code must be replaced with dynamic `$HOME` (`dirs::home_dir()`) or `/usr/bin/*` defaults before packaging:
1. [`src-tauri/src/config.rs:238`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/config.rs#L238): `system.default_projects_root_path` = `"/home/butcherwutcher/Projects/dev"`
2. [`src/components/modals/AddProjectModal.vue:291, 293`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/modals/AddProjectModal.vue#L291-L293): `'/home/butcherwutcher/Projects/dev'`
3. [`src-tauri/src/config.rs:988`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/config.rs#L988): `desigma.mcp_binary_path` = `"/home/butcherwutcher/Projects/dev/desigma/target/release/desigma-mcp"`
4. [`src-tauri/src/config.rs:997`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/config.rs#L997): `desigma.app_binary_path` = `"/home/butcherwutcher/Projects/dev/desigma/target/release/oneship-designer"`
5. [`src-tauri/src/config.rs:1155`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/config.rs#L1155): `pcbcraft.app_binary_path` = `"/home/butcherwutcher/Projects/dev/pcbcraft/target/release/pcbcraft"`
6. [`src-tauri/src/config.rs:1164`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/config.rs#L1164): `pcbcraft.cli_binary_path` = `"/home/butcherwutcher/Projects/dev/pcbcraft/target/release/pcbcraft-cli"`

### 4.2 Tiger Style Unit Suffix Omissions (`AGENTS.md §8.1`)
- **Config Keys (`src-tauri/src/config.rs`):** `snapshots.max_per_project` (`L782`), `electronics.default_baud_rate` (`L938`), `desigma.max_snapshots_per_asset` (`L1039`), `solidcraft.max_snapshots_per_asset` (`L1128`), `pcbcraft.max_snapshots_per_asset` (`L1198`).
- **Rust Structs & Parameters:**
  - `git.rs:179-183` (`GitStatus`: `staged`, `modified`, `untracked`, `ahead`, `behind` missing `_count`).
  - `github.rs:789-790` (`GithubPrDetail`: `additions`, `deletions` missing `_count`).
  - `documents.rs:218, 231` (`mtime` missing `_seconds`), `274, 479` & `fs_tree.rs:73` (`depth` missing `_count`).
  - `desigma.rs:164` (`depth` missing `_count`).
  - `stats.rs:150, 153` (`tokens_used`, `max_context_tokens` missing `_count`).
  - `pcbcraft.rs:260` (`drc_total_violations` missing `_count`), `1378-1381` (`BoundsNm` fields missing `_nm`).
  - `electronics/instruments/driver.rs:97-101, 147, 156-157` (`v_pp`, `v_rms`, `v_min`, `v_max`, `v_avg`, `zero_offsets` missing `_mv`; `start_sample`, `end_sample` missing `_index`).
  - `conversation.rs:80, 82` (`last_user_message_at`, `last_mcp_response_at` missing `_seconds`).
  - `events.rs:256, 283, 309, 311, 346, 393, 425, 427, 437` & `db.rs` record structs (`timestamp`, `created_at`, `updated_at`, `resolved_at`, `tokens_used`, `context_pct`).

---

## Part 5: Frontend UI Library, Tooltips, Theme Variables & Lifecycle (`AGENTS.md §5, §10, §11`)

### 5.1 Prohibited Native `title="..."` Attributes (24 Active Occurrences; Must Use `AppTooltip`)
- [`src/components/dialogue/DevlogJournalView.vue:452, 463`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/dialogue/DevlogJournalView.vue#L452-L463) (2)
- [`src/components/dialogue/FloatingChatOverlay.vue:1731`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/dialogue/FloatingChatOverlay.vue#L1731) (1)
- [`src/components/workspace/ProjectGitSnapshotsTab.vue:710, 717, 902, 910, 925, 962, 973, 1073, 1084`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectGitSnapshotsTab.vue#L710-L1084) (9)
- [`src/components/workspace/ProjectElectronicsTab.vue:501`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectElectronicsTab.vue#L501) (1)
- [`src/components/workspace/electronics/OscilloscopeView.vue:1528, 1539, 1551, 1567, 1576, 1715, 1726, 1738, 1754, 1763, 1893`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/electronics/OscilloscopeView.vue#L1528-L1893) (11)

### 5.2 Raw HTML `<input>` & `<textarea>` Elements Instead of `AppInput` / `AppTextarea` / `AppToggle`
- `RollingStatusLog.vue:403`, `OmnisearchModal.vue:190`, `GitBranchModal.vue:176`, `AppFileTree.vue:87`, `DevlogJournalView.vue:375`, `ProjectVerticalRail.vue:281`, `ProjectWorkspace.vue:1555, 1595, 1605, 1640, 1678`, `ProjectGitSnapshotsTab.vue:649, 673, 1227, 1237`, `ProjectTerminalGrid.vue:904, 1252`, `ProjectElectronicsTab.vue:840`, `OscilloscopeView.vue:1591, 1778, 1906`, `ProjectManagementTab.vue:452`, `GithubCollabPanel.vue:403`, `AccountQuotaControls.vue:333`.

### 5.3 Hardcoded Tailwind Palette Classes (`AGENTS.md §11.2`)
- [`src/components/workspace/electronics/OscilloscopeView.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/electronics/OscilloscopeView.vue): 70 lines with `emerald-*`, `rose-*`, `amber-*`, `cyan-*` + 45 lines with `zinc-*`.
- [`src/components/workspace/electronics/LogicAnalyzerView.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/electronics/LogicAnalyzerView.vue): 15 lines with `emerald-*`, `amber-*`, `cyan-*` + 10 lines with `zinc-*`.
- [`src/components/workspace/ProjectElectronicsTab.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectElectronicsTab.vue): 12 lines with `emerald-*`, `amber-*` + 12 lines with `zinc-*`.
- [`src/components/workspace/ProjectVerticalRail.vue:189, 332-335`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectVerticalRail.vue#L189) & [`ProjectOverviewTab.vue:244-247`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectOverviewTab.vue#L244-L247): `text-amber-400 fill-amber-400`.
- Also in Rust SVG generation: [`src-tauri/src/kopeng/pcbcraft.rs:1494-1908`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/pcbcraft.rs#L1494-L1908) and [`opendictate.rs:300, 321`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/opendictate.rs#L300).

### 5.4 TypeScript `any`, `v-for` Index Keys & Listener/Timer Leaks (`AGENTS.md §10.1, §10.3, §10.5`)
- **Explicit `any` (2 occurrences):** [`ProjectWorkspace.vue:99`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectWorkspace.vue#L99) (`icon: any`) and [`OscilloscopeView.vue:1944`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/electronics/OscilloscopeView.vue#L1944) (`op as any`).
- **`v-for` Array Index Keys (6 active occurrences):** `LogicAnalyzerView.vue:704`, `OscilloscopeView.vue:2214`, `FloatingChatOverlay.vue:1534`, `ProjectPcbTab.vue:848`, `ProjectGitSnapshotsTab.vue:1168`, `AppContextMenu.vue:87`.
- **Missing Teardown Hooks:**
  - [`src/App.vue:696-708`](file:///home/butcherwutcher/Projects/dev/one-ship/src/App.vue#L696-L708): Does not call `useVoiceStore().teardown()` on unmount.
  - [`src/stores/coreStore.ts:333, 337`](file:///home/butcherwutcher/Projects/dev/one-ship/src/stores/coreStore.ts#L333-L337): Reconnect `setTimeout(connectSse, 3000)` handle is not saved or cleared in `teardownListeners()`.
  - [`src/components/workspace/ProjectWorkspace.vue:1123`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectWorkspace.vue#L1123) (`copyTimer`) and [`ProjectElectronicsTab.vue:342`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/ProjectElectronicsTab.vue#L342) (`searchDebounce`) are not cleared in `onUnmounted`.

---

## Part 6: Universal Observability & Rustdoc Coverage (`AGENTS.md §9.7, §12`)

### 6.1 Modules Missing Universal Logging (`AGENTS.md §12.1, §12.2`)
- **Zero `logger::log` calls across entire files:**
  - [`src-tauri/src/kopeng/project_mgmt.rs`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/project_mgmt.rs) (16 public commands, 784 LOC).
  - [`src-tauri/src/kopeng/electronics/`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/electronics) (all 8 files; `owon_vds1022.rs:709-799` uses `eprintln!` to stderr).
  - [`src-tauri/src/kopeng/preview_cache.rs`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/preview_cache.rs), [`dna.rs`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/dna.rs), [`errors.rs`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/errors.rs), [`prompt.rs`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/prompt.rs).
- **Public functions missing `DEBUG` entry / `INFO` milestone logs:**
  - `solidcraft.rs` (all public functions), `desigma.rs` (5 public functions), `pcbcraft.rs` (7 public functions), `rea.rs` (8 public functions), `documents.rs` (17 synchronous CRUD functions), `github.rs` (16 OSS collaboration functions), `git.rs` (3 branch functions), `skill.rs` (10 functions).

### 6.2 Missing `///` Rustdoc Comments on Public Items (`AGENTS.md §9.7`)
- **188 public structs, enums, constants, and functions** lack `///` Rustdoc comments:
  - `src-tauri/src/db.rs` (66 items)
  - `src-tauri/src/events.rs` (39 event constructors)
  - `src-tauri/src/supervisor.rs` (21 items)
  - `src-tauri/src/kopeng/documents.rs` (16 items)
  - `src-tauri/src/kopeng/skill.rs` (10 items)
  - `src-tauri/src/config.rs` (9 items)
  - `src-tauri/src/kopeng/accounts.rs` (8 items)
  - `src-tauri/src/kopeng/github.rs` (6 items)
  - Remaining 13 items across `api.rs`, `logger.rs`, `duck.rs`, `hydration.rs`, `telemetry.rs`, `mcp.rs`, `git.rs`, `stats.rs`, `electronics/`.
