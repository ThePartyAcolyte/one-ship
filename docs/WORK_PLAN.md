# Work Plan by Subsystem & Context Nuclei

> Living tracking document consolidating unimplemented specs (`docs/specs/01–39`), active items in `TECHNICAL_DEBT.md`, and pending tasks in `TODO.md`.  
> Ordered from **core / cross-cutting platform layers** to **domain-specific subsystems**, grouped by shared files to minimize context switching.  
> *(Nucleus 0 completed on 2026-11-18 — see `devlog/2026-11-18_nucleus0_hygiene_and_spec_sync.md`.)*

---

## Nucleus 1 — Core Platform: Daemon, Runtime, Config & Caller Identity
**Target files:** `src-tauri/src/main.rs`, `src-tauri/src/config.rs`, `src-tauri/src/watcher.rs`
- [x] **Spec 12 §7.4 + Debt #37 (WebKitGTK Renderer Mode):** `[system] webkit_renderer_mode` (`auto|dmabuf|shared_memory|software`), pre-boot resolution in `main.rs`, and INFO startup logging implemented (`devlog/2026-11-18_webkit_renderer_mode_spec12.md`). Packaging installer/retry remains in `TODO.md`.
- [x] **Spec 12 §6 (`projects[0]` audit):** Verified `ui_state.rs`, `ProjectMinimap.vue`, and `AddProjectModal.vue` are UI/initial-selection defaults, not MCP caller-identity fallbacks.
- [ ] **Spec 12 §6.3 (Optional):** Evaluate `/proc/*/fd` identity cache (`identity_cache_max_items_count`) if MCP call overhead warrants it.
- [ ] **Spec 17 §5.1 (Optional):** Replace `mtime` polling in `watcher.rs` with `inotify` (`IN_MODIFY`) on `Default_keyring.keyring`.
- [ ] **Rustdoc (`///`) tail (`TODO.md` WP-9):** Add remaining public docstrings in `db.rs`, `documents.rs`, `skill.rs`, `config.rs`, `accounts.rs`, `github.rs`.

---

## Nucleus 2 — Core UI: Design System (Spec 13), Tooltips, F5 & DatePicker
**Target files:** `src/styles/ui.css`, `src/components/ui/{AppTooltip.vue,AppDatePicker.vue}`, `src/components/AppHeader.vue`, `src/components/taskbar/*`, `src/components/workspace/*`
- [x] **Debt #31 (`AppTooltip.vue`):** Implemented viewport-aware collision flipping & clamping via `getBoundingClientRect()` after `nextTick()`.
- [x] **Debt #39 (`--accent-rgb`):** Migrated all 5 `rgba(var(--accent-rgb,...))` glows to `color-mix(in srgb, var(--accent-color) X%, transparent)`.
- [x] **`AppDatePicker.vue` (`TODO.md` / Spec 39 §14):** Created brutalist `AppDatePicker.vue` and replaced native `<input type="date">` in `CreateProjectTaskModal.vue` and `ProjectTaskDetailPanel.vue`.
- [x] **Spec 13 §11.3 (Shell & Core Tabs Cleanup):** Removed `bg-black/*`, `text-[7px]`, `amber-400`, and native `:title` across `AppHeader.vue`, `AgentTaskbar.vue`, `AccountQuotaControls.vue`, `ProjectVerticalRail.vue`, `ProjectOverviewTab.vue`, `ProjectSkillsTab.vue`, `ProjectManagementTab.vue`, and `ProjectElectronicsTab.vue`.
- [ ] **Spec 13 §3.13 (Remaining Panels):** Audit remaining workspace panels (one panel per cycle) and wire `oneship:context-refresh` (`F5`) listeners in tabs with `RefreshCw` buttons (Debt #27).

---

## Nucleus 3 — Agent Engine, MCP, Conversations & Accounts
**Target files:** `src-tauri/src/mcp.rs`, `src-tauri/src/supervisor.rs`, `src-tauri/src/brief.rs`, `src/components/ui/AppAgentPortrait.vue`
- [x] **Debt #25 (Prescriptive MCP Errors):** Audited `src-tauri/src/mcp.rs` and replaced the remaining open-ended `"Fix these failures, then call ... again"` error in `complete_project_removal` with a strictly prescriptive message.
- [x] **Spec 30 Phase D (`snapshots.rs` `conversation_id`):** Verified `snapshots.rs:403` already resolves `db_latest_native_session_for_conversation` from `agent_conversations.id`.
- [x] **Spec 21 (`AppAgentPortrait.vue` Square Framing):** Cropped `64×96` tiles to a bottom-anchored `64×64` (`1:1`) square canvas (`y = 32..96`) and replaced `bg-black/40` / `bg-black/30` in `AppAgentPortrait.vue` & `AddPersonaModal.vue`. (Visual layer customizer deferred indefinitely; randomizer + DNA input retained.)
- [ ] **Debt #22 (`ACTIVE` — MCP Discovery & Fallback):** Verify end-to-end on Flag 27 (`fideiuris`) dynamic `~/.gemini/config/mcp_config.json` sync and `supervisor.rs` `agent_chat_turns` fallback.
- [ ] **Spec 30 §12 / Debt #23 (F6 Account Isolation):** Run concurrent two-agent test; depending on outcome, remove spawn-time `db_set_active_account` in `supervisor.rs:launch_agent` or isolate keyrings.
- [ ] **Spec 30 Phase E (Deferred / Separate Review):** Evaluate whether `brief.rs` should preserve `reopened_from` summary after a reopened conversation accumulates turns on a different runner.
- [ ] **Deferred — Debt #28:** Claude Code CLI `~/.claude/.credentials.json` hot-swapping.

---

## Nucleus 4 — Project Subsystems: Overview Dashboard (Spec 37) & Forge/PM (Spec 39)
**Target files:** `package.json`, `src/components/workspace/{OverviewWidgetGrid,ProjectOverviewTab}.vue`, `src-tauri/src/{overview,db,api}.rs`, `src-tauri/src/kopeng/forge/{mod,gitlab}.rs`, `src-tauri/src/mcp.rs`
- [x] **Spec 37 §4 + Debt #40 (Plugin-Driven Overview Dashboard & Widget Grid):**
  - Installed `grid-layout-plus` (4-column standardized drag/resize dashboard grid) and `vue-draggable-plus` (for upcoming 1D/Kanban/tab reordering).
  - Implemented `src-tauri/src/overview.rs` (`get_project_overview_summary`, `save_project_overview_layout`), `projects.overview_layout_json` in `db.rs`, and `OverviewWidgetGrid.vue` in `ProjectOverviewTab.vue` (including Semantic Knowledge widget).
- [x] **1D Reordering Adoption (`vue-draggable-plus`):** Refactored workspace tab reordering (`ProjectWorkspace.vue`) and Kanban stage/task drag-and-drop (`ProjectManagementTab.vue`) onto `vue-draggable-plus`.
- [ ] **Spec 39 (Minor Forge Remainders):**
  - Add `[plugins.vcs] forge_provider` override in `rula.toml` / `kopeng/git.rs` for self-hosted instances (§13.7).
  - Migrate `mcp.rs:5273-5448` from `kopeng::github::*_blocking` to the `forge` abstraction when appropriate.

---

## Nucleus 6 — Knowledge, Semantic Search & Explorer (Spec 22 §5 + Debt #24/#29)
**Target files:** `src-tauri/src/semantic/{chunker.rs,engine.rs,store.rs,throttler.rs,mod.rs}`, `src-tauri/Cargo.toml`
- [ ] **Debt #29 / Spec 22 §5.2 (Robust Indexing Pipeline):**
  - Integrate `tree-sitter` AST chunking in `semantic/chunker.rs` replacing line-prefix heuristics.
  - Implement two-tier SHA-256 incremental diffing in LanceDB (`store.rs`).
  - Run background indexing on `nice 19` / E-cores with live `get_indexing_status` progress (`is_indexing`, `last_duration_ms`).
  - Add `.pdf` text extraction in `scan_docs_directory`.
- [ ] **Spec 30 §8:** Semantic indexing/search over conversations (`agent_conversations`).
- [ ] **Deferred — Debt #24:** Secondary `gemini-embedding-2` (3072-dim) cloud worker + A/B evaluation against BGE-M3.

---

## Nucleus 7 — Profiler (Spec 25 §8)
**Target files:** `src-tauri/src/kopeng/profiler.rs`, `src/components/workspace/ProjectProfilerTab.vue`
- [ ] **Spec 25 §8.1:** Per-`viewer_type` parsers to extract function-level hotspots from `scratchpad/profiling/<run_id>.<ext>` and compute hotspot diffs across runs.
- [ ] **Spec 25 §8.2:** Evaluate wrapping `npm run …` commands (`node --cpu-prof`).

---

## Nucleus 8 — Design, 3D CAD & PCB (Spec 33)
**Target files:** `src-tauri/src/kopeng/solidcraft.rs`, `src-tauri/src/kopeng/pcbcraft.rs`, `src/components/workspace/{ProjectCadTab,ProjectPcbTab}.vue`, `package.json`
- [ ] **Spec 33 §3.1 / Debt #26 (Stage A.2 — Interactive WebGL 3D Viewport):**
  - Cached `.stl`/`.obj` export via `solidcraft-cli mcp` in `kopeng/solidcraft.rs`.
  - Integrate `three` (`STLLoader`/`OBJLoader` + `OrbitControls`) in `ProjectCadTab.vue` alongside headless PNG snapshots.
- [ ] **Spec 33 §7–§8 (Stages D & E Roadmap):**
  - Stage D: `kopeng::electronics::sourcing` (Mouser API, Chile availability, datasheets) + MCP `lookup_component_sourcing` + UI actions.
  - Stage E: `kopeng::electronics::pcbway` fabrication packaging.

---

## Nucleus 9 — Electronics & Instrumentation (Spec 31 & Spec 33 §2.3/§3.3)
- **9A — Serial Monitor & MCP Tools (`src-tauri/src/kopeng/electronics/`, `api.rs`, `mcp.rs`, `ProjectElectronicsTab.vue`):**
  - Replace local echo mock (`handleSendSerial`) with real `tokio-serial` streaming backend + `LF`/`CRLF`/`CR` selector (Spec 31 §5.3, Debt #30).
  - Implement `trait HardwareInstrument` unification and expose the 6 `electronics_*` MCP tools (Spec 31 §3, §8).
- **9B — Instrument Views & Logic Analyzer Driver (`OscilloscopeView.vue`, `LogicAnalyzerView.vue`, `fx2lafw.rs`):**
  - Migrate `OscilloscopeView.vue` and `LogicAnalyzerView.vue` to Spec 13 §11.3 semantic tokens (Debt #41).
  - Add configurable per-channel oscilloscope colors (Debt #32).
  - Add SVG timing diagram export in `LogicAnalyzerView.vue` (Spec 31 §4.2) and fix `channels` population in `fx2lafw.rs:131-132`.
- **9C — EDA Import & SPICE/EIS (`kopeng/electronics/eda/`, `kopeng/electronics/spice.rs`, `ProjectPcbTab.vue`):**
  - EAGLE (`.sch`/`.brd`) & KiCad (`.kicad_sch`) parsers + `kicanvas` viewer + Semantic Hardware Graph (Spec 31 §6).
  - `ngspice` / `Xyce` netlist extraction and `Run SPICE / EIS Sweep` (Spec 31 §7, Spec 33 §2.3/§3.3).

---

## Nucleus 9 — Global Voice Controller & Surface Actions (Spec 36)
*(Scheduled after domain panels are polished so surface actions target final UI contracts.)*
**Target files:** `src-tauri/src/ui_state.rs`, `src-tauri/src/voice_controller.rs`, `src/stores/*`, `src/components/workspace/*`
- [ ] **Debt #34 / Spec 36 (Remaining Panel Surface Actions):** Register `UiSurfaceActionDef` and handlers in `ui_state.rs` + `voice_controller.rs` + frontend listeners (`overview` `get_overview_summary`, `devlog`, `git`, `terminal`, `profiler`, `skills`, `electronics`, `design`, `cad`, `pcb`).
- [ ] **Deferred — Debt #35:** Dynamic routing between `gemini-3.8-live` and `gemini-3.8-live-extended-thinking`.
- [ ] **Optional — Spec 05:** `ai_processing`/`ai_prompt`, Model 2 direct capture, or `/tmp/opendictate.socket` UDS transport if needed.

---

## Nucleus 10 — Deferred to Packaging / Future Phases
- **Specs 07 & 14 (i18n / Cinturonero):** Full string extraction and `belta_translated` subtitles at packaging time.
- **Spec 02 (Diagnostic Laboratory):** LLM Error Curator + Mini-Grafana crash/PID dashboard.
- **Spec 03 (Tray Hub Interface):** System tray icon and compact floating window.
- **Debt #7:** Distributed multi-agent audit ledger.

---

## Appendix — Pending Runtime Verification (`TODO.md`)
*(Live checks only; no code changes required unless a check fails)*
1. **Daemon & Security:** Operator API guard (403 on `curl`), `auto_vacuum` / `compact_duckdb_file`, `oneship_telemetry.duckdb` ownership, quiet watcher INFO logs, `run_dev.sh` + Omarchy bar widget.
2. **Agents, MCP & Conversations:** Caller identity (`agy`, `claude`, `opencode`, Spec 12 §6.8), Claude 1M context window check, Spec 30 Phases A–E, cross-project isolation, Flag 27 (`fideiuris`).
3. **Services, Tasks & Profiler:** Port release, shell stdin, space-quoting, `py-spy` SVG output, run `DIFF`, `list_profiler_runs` MCP.
4. **Semantic Search & Devlog (Spec 22):** BGE-M3 INT8 init, adaptive throttler, `Ctrl+K` Omnisearch, atomic journal indexing.
5. **UI, Settings & Visual:** WebKitGTK 2.54 backdrop blur + RSS leak check, shell geometry (`h-9` / `h-6`), unsaved-changes guard + `rula.toml` watcher, 4,000-line bounded Git diff.
6. **Projects, Forge & Gantt (Specs 34/38/39):** `Tasks | Forge`, `Schedule`, `Gantt` (zoom, Today, `All PM Projects` overlay), `GitLab` (`glab api`), `forge_inbox_items` + `forge-inbox-updated`, `propose_github_draft_pr` approval gate.
