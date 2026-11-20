# Devlog: Omnisearch Modal (`Ctrl + K`), Brownfield Indexation & Decoupled Exclusion Standard

**Date:** 2026-11-10  
**Topic:** Spotlight Omnisearch Modal, Project Overview Knowledge Card, Journal Indexation, and `[plugins.semantic]` Configuration  
**Author:** Antigravity (Pair Programming with Developer)  

---

## 1. Context & User Directives

Following the initial construction of the BGE-M3 + LanceDB semantic engine, critical user observations identified three operational gaps:
1. **Missing Journal Vectorization:** Decisions, past milestones, and session minutes recorded in `agent_session_journal` were absent from vector indexation, preventing agents and users from discovering historical architectural context via natural language.
2. **Brownfield Project Indexation:** Existing projects (which bypassed greenfield onboarding) lacked a user-accessible trigger to compute baseline embeddings.
3. **Decoupling from `.gitignore`:** Repositories often ignore local research notes, external API manuals, and documentation directories (`research/`, `manuals/`, `docs/`, `*.pdf`) to keep Git clean. Relying blindly on `.gitignore` would exclude vital developer knowledge from semantic discovery.

---

## 2. Changes Implemented

### 2.1 Decoupled Exclusion Standard (`[plugins.semantic]` in `rula.toml`)
- In `src-tauri/src/config.rs`, registered `PluginConfigSchema` for `semantic` in `get_all_plugin_schemas()`:
  - `enabled`: `boolean`, default `true`, scope `cascade`.
  - `ignored_patterns`: `string_list`, default `["target", "node_modules", "dist", "build", ".git", ".env*", "*.min.js", "*.lock"]`. Decoupled from `.gitignore` so local docs and research remain indexed.
  - `included_extensions`: `string_list`, default `["rs", "ts", "js", "py", "vue", "c", "cpp", "h", "hpp", "html", "css", "md", "txt", "pdf"]`.
  - `extra_paths`: `string_list`, default `["docs", "specs", "devlog", "research", "manuals"]`.
- Automatically rendered in `ProjectConfigPanel.vue` and `SettingsModal.vue`.

### 2.2 Semantic Engine & Journal Vectorization (`src-tauri/src/semantic/`)
- In `src-tauri/src/semantic/mod.rs`:
  - Added extraction of `agent_session_journal` entries (`db_get_project_session_journals`) into `RawChunk` with `entity_type: "journal"`, incorporating summary, decisions, and debt notes.
  - Added non-blocking helper `index_single_journal_entry` called from `execute_record_session_minutes` in `src-tauri/src/mcp.rs` for atomic, immediate vectorization upon session minutes recording.
  - Replaced hardcoded ignore rules with `is_path_ignored` reading effective `rula.toml` configuration, scanning both code files and `extra_paths`.
  - Added `get_indexing_status(project_id)` and exported `ProjectIndexingStatus`.
- In `src-tauri/src/semantic/store.rs`:
  - Added `count_project_records(project_id)` using LanceDB's `.count_rows()`.
- In `src-tauri/src/semantic/types.rs`:
  - Enforced `#[ts(type = "number")]` on all `i64`/`u64` fields (`project_id`, `duration_ms`) to match ONE-SHIP TypeScript parity standards.

### 2.3 Protocol & Daemon RPC API
- In `src-tauri/src/api.rs`:
  - Registered `"index_project_knowledge"` and `"get_indexing_status"` in `API_COMMANDS` and `api::dispatch`.
- In `src/types/daemonApi.ts`:
  - Added type-safe command mappings for `index_project_knowledge` and `get_indexing_status`.

### 2.4 Omnisearch Spotlight Interface (`src/components/modals/OmnisearchModal.vue`)
- Built global Spotlight modal accessible via `Ctrl + K` (or `Cmd + K`) and header search button.
- Scope filter selector: `[ ALL ]`, `[ CODE ]`, `[ SPECS ]`, `[ JOURNAL ]`, `[ TASKS ]`, `[ DEBT ]`.
- Debounced 250 ms input querying `daemonCall('search_knowledge', ...)`.
- Ranked results list showing entity icon, scope badge, title, line offsets, relevance %, and monospace preview snippet.
- Keyboard navigation (`↑`, `↓`, `Enter`, `Esc`).
- On selection, jumps to file in Codebase Explorer via `explorerStore.selectProjectDocument` or navigates to `devlog`/`overview`.

### 2.5 Brownfield Overview Subsystem Card (`src/components/workspace/ProjectOverviewTab.vue`)
- Added **Semantic Knowledge** subsystem card under "Project Features & Subsystems".
- Displays current indexed chunk count (`X Chunks` or `Unindexed`).
- Provides interactive **"Index Knowledge"** button with live spinner while background indexation executes.

### 2.6 Native Concurrency Boundary & Host Telemetry Throttler Refinement
- **Eliminated OS Process Inspection:** Completely removed external process blacklists (`COMPETING_PROCESS_NAMES`) that erroneously matched idle dev servers (`npm`, `vite`, `esbuild`) and blocked indexing.
- **Process Supervisor Concurrency Boundary:** Background indexing yields only when ONE-SHIP itself is executing internal tasks (`RunTargetType::Task` in `Working` state: compilation, tests, linters, migrations).
- **Persistent Telemetry Sampling:** Maintained a singleton `SYSTEM_MONITOR` for accurate CPU and RAM deltas.
- **Interactive Indexing Exemption:** Manual user indexing (`force = true`) executes immediately, gated only by critical memory safety ($\ge 512\text{ MiB}$ free RAM) to prevent OOM panic.

### 2.7 Universal Root Scanning & Polyglot Extension Expansion
- **Decoupled Architecture Code Discovery:** Resolved the issue where decoupled projects like Watchtower (`backend/app/`, `frontend/src/`, `connectors/`) yielded 0 code symbols due to hardcoded scanning directories (`["src", "src-tauri/src", "core", "api"]`).
- **Recursive Project Traversal:** Replaced hardcoded folder arrays in `src-tauri/src/semantic/mod.rs` with `scan_code_directory` starting recursively from workspace `root_path`. The scanner skips documentation directories already processed, hidden paths (`.*`), and patterns in `ignored_patterns`.
- **Extended Schema Defaults:** Expanded `included_extensions` default in `src-tauri/src/config.rs` to include `tsx`, `jsx`, `toml`, `sql`, and `sh`.
- **Virtualenv & Cache Exclusions:** Added `__pycache__`, `.venv`, `venv`, `*.pyc`, and `scratchpad` to default `ignored_patterns` to prevent pollution from local Python runtime directories.

---

## 3. Verification & Validation

- `cargo check --manifest-path src-tauri/Cargo.toml`: Passed with 0 errors.
- `cargo test --manifest-path src-tauri/Cargo.toml semantic`: 14 passed, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib api::tests::api_commands_match_the_frontend_map`: Passed with 0 errors.
- `npm run build` (`vue-tsc --noEmit && vite build`): Built in 6.31s with 0 type errors.
- Living spec updated: `docs/specs/22_session_lifecycle_and_devlog_graph.md` §5.1, §5.2, §5.5.
