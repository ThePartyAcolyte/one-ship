# Devlog: General Project Audit & Full Remediation Sweep (2026-11-16)

## Summary
Executed a full codebase, specification, and quality audit across ONE-SHIP (~92k LOC, 31 specs, 5 foundation docs), established a clean `2026-11-16` audit suite in `docs/audit/`, and executed Work Packages `WP-1` through `WP-9` step-by-step under maintainer-approved Technical Proposals.

## Changes Implemented

### 1. Audit Baseline & Foundation/Spec Synchronization (`WP-1`, `WP-5`, `WP-6`)
- Created clean audit reports in `docs/audit/` (`audit_00_index.md` through `audit_04_code_quality_and_standards.md`).
- Synchronized `docs/00_manifesto.md` (Multidisciplinary Maker's Toolbox + integrated CodeMirror 6 editor), `docs/01_ubiquitous_language.md` (HTTP SSE `/api/events`, 3 supported runners `agy`/`claude`/`opencode` [DeepSeek], removed `cursor`, documented `system` agent redesign as direct-API Voice & Interface Orchestrator), `docs/02_architecture_stack.md` (SQLite + DuckDB + LanceDB/BGE-M3), `docs/03_lang_belta_dictionary.md` (deferred post-completion), `docs/README.md`, and `AGENTS.md`.
- Synchronized Specs `01`, `04`–`14`, `16`–`21`, `23`–`25`, `29`–`34`:
  - Event-driven snapshots on `record_session_minutes` (`snapshots.auto_snapshot_on_minutes`) and file save (`snapshots.auto_snapshot_on_file_change`), with per-category retention and optional `snapshots.scratchpad_retention_days` (`0` = disabled).
  - Fixed `B-09` DNA portrait gender/asset mismatches in `src-tauri/src/kopeng/dna.rs`, `src/components/modals/AddPersonaModal.vue`, and `src/components/ui/AppAgentPortrait.vue`.
  - Recorded deferred items in `TECHNICAL_DEBT.md` (`#28` Claude multi-account hot-swap, `#29` Spec 22 §5 robust semantic search pipeline, `#30` Spec 31/33 unimplemented Electronics/EDA/SPICE subsystems).

### 2. Critical Bugs & Cross-Project Isolation (`WP-2`)
- **`B-01` & `B-02` (`src-tauri/src/semantic/`):** Scoped vector chunk IDs with `project_id` + `line_start` (`chunker.rs`) and fixed JIT line resolution inside `spawn_blocking` (`mod.rs`).
- **`B-03` (`src-tauri/src/kopeng/rea.rs`):** Expanded ELF header buffer from 16 to 64 bytes.
- **`B-04` (`src-tauri/src/db.rs`, `src-tauri/src/mcp.rs`):** Added project-scoped DB helpers (`db_update_todo_status_scoped`, `db_resolve_technical_debt_scoped`, `db_resolve_document_comment_scoped`) and project ownership guards across 9 MCP tools (`query_project_task`, `cancel_project_task`, `resolve_document_comment`, `manage_todo_item`, `resolve_todo_item`, `record_technical_debt`, `resolve_technical_debt`, `get_run_profile`, `compare_profiler_runs`).
- **`B-05` (`src-tauri/src/kopeng/electronics/`):** Removed destructive `rula.toml` overwrite fallback and fixed `[plugins.electronics]` table path.
- **`B-06` (`src-tauri/src/kopeng/manifest.rs`):** Added `"desigma"` and `"pcbcraft"` to `ONESHIP_MCP_SERVER_KEYS`.
- **`B-07` & Hyprland Window Management (`src-tauri/src/desktop.rs`, `desigma.rs`, `solidcraft.rs`, `pcbcraft.rs`):** Replaced ambiguous class-only matching with `HyprlandWindowMatch` (class + `/proc/<pid>/cwd|cmdline` verification), focus by `address:`, and `wait_and_move_hyprland_window` to contiguous workspace.
- **`B-08` (`src/components/dialogue/FloatingChatOverlay.vue`):** Executed `event.preventDefault()` synchronously before `await` in paste handler and removed `FileReader` fallback.

### 3. Dead Code & Legacy Shim Purge (`WP-3`)
- Removed dead commands (`read_process_output`, `get_effective_project_config`, `check_electronics_toolchain`), unused Rust symbols/error variants/`DEFAULT_*` constants, legacy `kapawu` startup migration shims (`db.rs`, `accounts.rs`, `snapshots.rs`, `scripts/agy_statusline.py`), dead `workspaceLayoutStore` artifact-panel state, and 14 unreferenced files (~3,780 LOC) deleted from disk.

### 4. Permissive Fallbacks & Silent Errors (`WP-4`)
- Eliminated `P-01`–`P-09`: removed hardcoded `"bosmang"` callback fallback (`mcp.rs`), empty-title/summary Genesis substitution (`mcp.rs`), fabricated `greenfield` init state (`initializer.rs`), duplicate prompt dropping (`supervisor.rs`), `.one-ship/rea` override (`hydration.rs`), invalid `sheet_id` first-sheet fallback (`pcbcraft.rs`), static fallback boards / `"espressif32"` default (`electronics/firmware.rs`), `resolveAgentProjectPath` guessing + un-scoped agent map (`dialogueStore.ts`, `RollingStatusLog.vue`), and auto-selecting next project on archive (`coreStore.ts`).
- Replaced silent `let _ =` / `.ok()` / `.unwrap_or_default()` / empty `catch {}` blocks with error propagation or explicit logging across `brief.rs`, `hydration.rs`, `semantic/mod.rs`, `supervisor.rs`, `electronics/`, `owon_vds1022.rs`, and Vue stores/components.

### 5. Governance, Async Purity, Human-Scale Bounds & Tiger Style (`WP-7`)
- Added **Section 13 (`Human-Scale Bounds, Configurability & Audit-Anchored Anti-Patterns`)** to `AGENTS.md`.
- Registered global human-scale configurable limits in `src-tauri/src/config.rs`: `system.max_dir_walk_depth_count` (`32`), `system.max_dir_walk_nodes_count` (`50_000`), `supervisor.max_task_capture_lines_count` (`20_000`), `supervisor.max_agent_response_bytes` (`4_194_304`), `electronics.usb_flush_max_attempts_count` (`256`), and raised `mcp.approval_ttl_ms` to `300_000` ms (5 min).
- Bounded `spawn_task` capture, `spawn_agent` response/thought buffers, `await_user_approval` timeout, `flush_endpoint`, directory walks (`documents.rs`, `semantic/mod.rs`), and frontend serial/terminal buffers (`ProjectElectronicsTab.vue`, `ProjectTerminalGrid.vue`).
- Offloaded 27 synchronous MCP tools (`dispatch_sync_mcp_tool` in `mcp.rs`), `index_project_knowledge` scan (`semantic/mod.rs`), `resolve_executable` (`supervisor.rs`), `switch_account`/`initiate_add_account` (`accounts.rs`), and MCU/instrument probing (`electronics/`) to `tokio::task::spawn_blocking`.
- Fixed production `.unwrap()` calls, poison-safe locks (`semantic/engine.rs`, `supervisor.rs`, `owon_vds1022.rs`, `documents.rs`), atomic `compare_exchange` in `arbitrator.rs`, `ConfigError` in `resolve_project_subdir`, and `"SemanticError::<Variant>"` codes.
- Standardized Tiger Style unit suffixes across `config.rs`, `snapshots.rs`, `git.rs`, `github.rs`, `documents.rs`, `fs_tree.rs`, `pcbcraft.rs`, `driver.rs`, `owon_vds1022.rs`, `fx2lafw.rs`, `conversation.rs`, `stats.rs`, `events.rs`, `supervisor.rs`, specs, skills, and Vue/TS consumers.

### 6. Frontend UI Standards, Portability, Logging & Rustdoc (`WP-8`, `WP-9`)
- Replaced 24 native `title=` attributes with `<AppTooltip>` and migrated raw text/number `<input>`/`<textarea>` elements to `<AppInput>`/`<AppTextarea>` (with `defineExpose({ focus })` in `AppInput.vue`).
- Eliminated remaining `any` types (`ProjectWorkspace.vue`, `OscilloscopeView.vue`), unstable `:key="idx"` keys (`LogicAnalyzerView.vue`, `OscilloscopeView.vue`), and missing teardown hooks (`App.vue`, `coreStore.ts`, `ProjectWorkspace.vue`, `ProjectElectronicsTab.vue`).
- Preserved distinct entity/channel colors per maintainer guidance and logged `TECHNICAL_DEBT.md` `#31` (`AppTooltip` viewport collision) and `#32` (configurable oscilloscope channel colors).
- Removed all 7 hardcoded `/home/butcherwutcher/...` paths (`config.rs`, `AddProjectModal.vue`), replaced `eprintln!` in `owon_vds1022.rs` with `logger::log`, added `DEBUG`/`INFO` logging to `project_mgmt.rs`, and added `///` Rustdoc comments across 17 core/kopeng modules.

## Verification
- Verified all 14 dead files deleted from disk.
- Verified `./scripts/run_dev.sh status` shows `daemon` (port `8765`), `daemon_watch`, `vite`, and `gui` running; fixed final `project_mgmt.rs` compilation error detected in `daemon_build` logs.
- Runtime verification items added to `TODO.md` under `Audit 2026-11-16 follow-up`.
