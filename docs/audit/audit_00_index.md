# ONE-SHIP General Audit & Remediation Master Plan
**Audit Date:** `2026-11-16` | **Baseline Version:** `0.2.1` (Platform Skill Schema `0.2.6`)  
**Scope:** `92,065 LOC` (`47,456 LOC` Rust across 69 files + `44,609 LOC` Vue/TypeScript across 101 files), 31 living specifications (`docs/specs/`), 5 foundation documents (`docs/`), 18 platform skills (`templates/skills/`), and 239 session logs (`devlog/`).

> [!IMPORTANT]
> **Historical Audits Archived:**
> All previous audit reports from `2026-09-23` through `2026-10-08` have been moved to [`docs/audit/archived/`](archived/). Those reports were 100% superseded by the November 1–6 daemon ownership migration, universal `thiserror` conversion, and continuous profiler redesign. This `2026-11-16` audit series is the sole active audit baseline.

---

## 1. Mandatory Remediation Governance Protocol (Case-by-Case Execution)

Remediation of the findings in this audit **MUST NOT** be executed as blind bulk edits. Every discrepancy between specifications and implementation falls into one of two distinct phenomena that require different engineering actions:

```
+-----------------------------------------------------------------------------------+
| TWO PHENOMENA IN SPEC vs. IMPLEMENTATION DISCREPANCIES                            |
+-----------------------------------------------------------------------------------+
|                                                                                   |
| PHENOMENON A: Intentional Architectural Evolution (Code Supersedes Spec)          |
|  - During iterative development, a better architecture, schema, or UX was chosen  |
|    and implemented in code, leaving the specification behind.                     |
|  - Default Action: Update the specification (`docs/specs/`) to reflect the        |
|    implemented truth.                                                             |
|                                                                                   |
| PHENOMENON B: Unimplemented / Pending Specification Design (Spec Ahead of Code)   |
|  - The specification defines capabilities, phases, or tools that have not yet     |
|    been built (or were partially mocked/stubbed).                                 |
|  - Default Action: Present concrete options to the maintainer:                    |
|      (1) Implement the missing capability now,                                    |
|      (2) Explicitly mark it as a deferred Stage 2 / Post-1.0 section in the spec, |
|      (3) Prune the obsolete design from the spec if it is no longer desired.      |
+-----------------------------------------------------------------------------------+
```

### Execution Rule (`AGENTS.md` §1.1)
1. **Step-by-Step Technical Proposals:** Remediation proceeds one bounded work package at a time.
2. **Maintainer Decision Gate:** Before modifying any code, configuration, or specification in a work package, the agent presents a concise **Technical Proposal** analyzing each item case-by-case, stating whether it is Phenomenon A, Phenomenon B, a bug fix, or dead code removal, and waits for explicit maintainer confirmation.
3. **Devlog & Spec Synchronization:** Every approved and executed work package updates its corresponding specification and records a session log in `devlog/` within the same work cycle.

---

## 2. Audit Sub-Reports Index

| Document | Focus Area | Key Contents & Metrics |
| :--- | :--- | :--- |
| **[01 — Critical Bugs & Isolation](audit_01_critical_bugs_and_isolation.md)** | Logic defects, data corruption risks & scoping leaks | **8 Critical/High bugs** (`B-01`..`B-08`: LanceDB PK collision, no-op JIT line search, 16-byte ELF buffer truncation, unverified cross-project IDs in 9 MCP tools, destructive `rula.toml` overwrite in Electronics, incomplete MCP de-integration, Hyprland window class collision, DOM paste `preventDefault` after `await`). |
| **[02 — Spec vs. Implementation](audit_02_spec_vs_implementation.md)** | Foundation docs + Specs `01`–`34` + Unregistered modules | Per-spec completion matrix (**~89.4%** weighted average across 28 active specs), explicit separation of **Phenomenon A** (code evolved) vs **Phenomenon B** (not yet implemented), and **9 unregistered subsystems/features** lacking spec coverage. |
| **[03 — Dead & Permissive Code](audit_03_dead_and_permissive_code.md)** | Zero Dead Code (`§1.4`) & Zero Silent Fallbacks (`§1.11`) | **`3,735 LOC` of dead Vue/TS files** (`ArtifactPanel.vue`, `ConversationPanel.vue`, `VcsActionModal.vue`, `DocTreeNode.vue`, `legacyStorage.ts`, `ZoneOutlet.vue`, `pluginStore.ts`), 3 unused daemon commands, 8 unused Rust functions, legacy `kapawu` startup migrations, and permissive fallbacks (`"bosmang"` hardcode, synthetic init state, silent `.ok()` / `catch {}`). |
| **[04 — Code Quality & Standards](audit_04_code_quality_and_standards.md)** | `AGENTS.md` compliance across Rust & Vue/TS | God-modules (`db.rs`, `mcp.rs`, `supervisor.rs`), blocking I/O in `async fn` (`21` MCP handlers + `semantic/` + `kopeng/`), unbounded buffers (`spawn_task`, `approval_ttl_ms`), Tiger Style unit suffixes, **7 hardcoded `/home/butcherwutcher` paths**, **25 native `title=` attributes**, **23 raw HTML inputs**, **~100 hardcoded Tailwind palette lines**, missing logging (`project_mgmt.rs`, `electronics/`), and **188 undocumented public Rust items**. |
| **[Post-Audit Follow-Up (`2026-11-19`)](post_audit_2026-11-19_findings.md)** | Post-`2026-11-16` devlogs (`2026-11-17` to `2026-11-19`) | Verification of 35 post-audit devlogs against `AGENTS.md`: **5 High**, **19 Medium**, and **15 Low** findings (`PA-A01`..`PA-B15`) across `voice_controller.rs`, `AppHeader.vue`, `OscilloscopeView.vue`, `semantic/`, `forge/`, `milestones`, and `TODO.md`/`TECHNICAL_DEBT.md` sync. |

---

## 3. Confirmed Maintainer Decisions on Foundation Documents (`2026-11-16`)

During the initial audit review on `2026-11-16`, the maintainer established the following ground-truth decisions for **Work Package 1 (Foundation Documents Synchronization)**:

1. **Maker's Multidisciplinary Toolbox & Integrated Code Editor (`docs/00_manifesto.md`):**
   - ONE-SHIP evolved into a **Maker's Multidisciplinary Toolbox & Agentic Control Center** bridging worlds that belong together in physical/maker engineering (code/specs with CodeMirror 6 in `AppCodeEditor.vue`, 3D CAD, PCB/EDA, UI design, USB benchtop instrumentation, reverse engineering, and project management).
   - Continuous profiling (`samply`, `py-spy`, DuckDB) and stderr extraction are active, while end-to-end validation of cognitive LLM consumption of profiling/execution diagnostics remains tracked as an open milestone.
2. **SSE Event Stream Transport (`docs/01_ubiquitous_language.md`):**
   - Real-time state updates from `oneship-daemon` to the GUI stream over **HTTP Server-Sent Events (`GET /api/events`)**, not WebSockets.
3. **Supported Agent Runners & Models (`docs/01_ubiquitous_language.md`, `docs/02_architecture_stack.md`):**
   - `cursor` is NOT a supported runner and was removed from documentation.
   - Only the three runners actually installed and used on the maintainer's workstation are supported:
     * **Antigravity (`agy`)**
     * **Claude Code (`claude`)**
     * **OpenCode (`opencode`)** — specifically used with **DeepSeek**.
4. **Global System Agent Redesign -> Direct-API Voice & Interface Orchestrator (`docs/01_ubiquitous_language.md`, `Spec 24`):**
   - The legacy project-style CLI chat `system` persona was retired on `2026-11-16`.
   - It is planned to return under a **new architecture**: a **direct-API Voice & Interface Orchestrator** (e.g., via **Gemini Live API** with native function calling, bypassing CLI runners) operating globally at the interface level to inspect cross-project tasks and semantic RAG, control UI navigation (open projects/views), and draft/delegate instructions to project-scoped agents.
5. **Tri-Engine Storage Architecture (`docs/02_architecture_stack.md`):**
   - The architectural stack explicitly documents all three embedded persistence engines owned by `oneship-daemon`:
     * **SQLite (`oneship.db`):** Relational state, project registry, conversations, session minutes, tasks, and quotas.
     * **DuckDB (`oneship_telemetry.duckdb`):** Columnar time-series process tree telemetry and profiling samples.
     * **LanceDB (`vectors.lance/` with local `BGE-M3` INT8 ONNX):** Embedded vector database for transversal semantic search across code, specifications, journal minutes, tasks, and technical debt.
6. **Cinturonero (Lang Belta) Translation Strategy (`docs/03_lang_belta_dictionary.md`, `docs/04_ipc_connection_belta.json`, `Spec 07`, `Spec 14`):**
   - The Belter Creole translation layer is **strictly deferred** until the application reaches full functional completion and packaging. Once the application is complete, translation will be executed as a dedicated, separate post-completion pass.

---

## 4. Global Completion Metrics & Publication Timeline

### 4.1 Completion Breakdown by Architectural Pillar

| Architectural Pillar | Specifications | Active Specs | Deferred / Parked | Implementation Progress |
| :--- | :--- | :---: | :---: | :---: |
| **1. Core Architecture & Config** | `01, 02, 03, 06, 10, 24` | 4 | 2 (`02`, `03`) | **86.0%** |
| **2. Agent Runtime & MCP Protocol** | `08, 12, 18, 19, 29, 30` | 6 | 0 | **93.2%** |
| **3. Telemetry, Profiling & Graphs** | `09, 16, 22, 23, 25` | 5 | 0 | **88.0%** |
| **4. System, Hardware, CAD & OSS** | `04, 05, 11, 17, 26a, 26b, 31, 32, 33, 34` | 10 | 0 | **85.9%** |
| **5. UI Design System & Artifacts** | `07, 13, 14, 20, 21` | 3 (`13, 20, 21`) | 2 (`07`, `14`) | **85.0%** |
| **Total / Weighted Average** | **31 Spec Files** | **28 Active** | **4 Deferred/Parked** | **~89.4% Implemented** |

*(Note: Runtime verification progress across the active checklists in [`TODO.md`](../../TODO.md) stands at **~81.5%** pending execution of the 35 live verification blocks).*

### 4.2 Development Velocity & Release Schedule
- **Historical Velocity (`2026-09-02` to `2026-11-16`):** **239 `devlog/` sessions** across 76 calendar days (43 active development days = **5.56 sessions per active day**).
- **Target Milestones:**

```
+------------------------------------------------------------------------------------+
| ROADMAP TO PUBLICATION (Baseline: 2026-11-16 | Version 0.2.1 / Platform 0.2.6)     |
+------------------------------------------------------------------------------------+
|                                                                                    |
| STAGE 1: Foundation Docs & Spec Alignment (v0.2.7)                                 |
| Window: 2026-11-16 -> 2026-11-18 (3 days)                                          |
|  - Case-by-case alignment of `docs/00..03`, `docs/README.md`, and `docs/specs/`    |
|  - Resolution of Phenomenon B scope decisions (Spec 16, 20, 22, 31, 33)            |
|                                                                                    |
| STAGE 2: Dead Code Purge & Critical Bug Remediation (v0.2.8)                       |
| Window: 2026-11-19 -> 2026-11-22 (4 days)                                          |
|  - Remove ~4,050 LOC of dead Vue/TS/Rust code and legacy `kapawu` startup shims    |
|  - Fix Critical Bugs B-01..B-08 and Permissive Fallbacks P-01..P-09                |
|                                                                                    |
| STAGE 3: Async Purity, Bounded Memory, UI Standards & Observability (v0.2.9)       |
| Window: 2026-11-23 -> 2026-11-28 (6 days)                                          |
|  - Wrap blocking MCP/Semantic/Kopeng calls in `spawn_blocking`                     |
|  - Enforce bounded buffers (`spawn_task`, `approval_ttl_ms`, UI terminal/serial)   |
|  - Replace 25 native `title=`, 23 raw inputs, and ~100 Tailwind color violations   |
|  - Complete `logger::log` coverage (`project_mgmt`, `electronics`) & Rustdoc `///` |
|                                                                                    |
| STAGE 4: Live Runtime Verification Sweep & TODO Closure (v0.2.10)                  |
| Window: 2026-11-29 -> 2026-12-05 (7 days)                                          |
|  - Prune obsolete Visual Builder / `git` / `system` items from `TODO.md`           |
|  - Execute all ~35 active runtime verification checklists & `TECHNICAL_DEBT` #22/23|
|                                                                                    |
| STAGE 5: Portability & Native Arch / Omarchy Packaging (v0.3.0-alpha)              |
| Window: 2026-12-06 -> 2026-12-12 (7 days)                                          |
|  - Replace all 7 hardcoded `/home/butcherwutcher` paths with dynamic `$HOME`/`PATH`|
|  - Decouple `templates/` lookup for `/usr/share/one-ship/templates/`               |
|  - Create `PKGBUILD`, production `oneship-daemon.service`, and `.desktop` entry    |
|  >>> MILESTONE 1 — INSTALLABLE PUBLIC ALPHA (`v0.3.0`): ~2026-12-12 <<<            |
|                                                                                    |
| STAGE 6: Packaged Burn-In, Post-Completion Belta Pass & 1.0 Freeze (v1.0.0)        |
| Window: 2026-12-13 -> 2027-01-15 (~4 weeks)                                        |
|  - Multi-week stability burn-in on packaged build                                  |
|  - Separate Cinturonero (Belta) translation pass once UI is frozen                 |
|  >>> MILESTONE 2 — PRODUCTION RELEASE (`v1.0.0`): ~2027-01-15 <<<                  |
+------------------------------------------------------------------------------------+
```

---

## 5. Step-by-Step Remediation Plan (Work Packages)

Each Work Package (WP) below will be executed **only after presenting a specific Technical Proposal and receiving explicit maintainer approval**:

- **[COMPLETED] WP-1: Foundation Documents & Index Synchronization (`docs/00..04`, `docs/README.md`, `AGENTS.md`)**
  - Apply confirmed maintainer decisions (CodeMirror editor in `00_manifesto.md`, SSE and `agy`/`claude`/`opencode` [DeepSeek] in `01_ubiquitous_language.md`, SQLite + DuckDB + LanceDB/BGE-M3 in `02_architecture_stack.md`, deferred status note on `03_lang_belta_dictionary.md`, add Specs 32/33/34 to `docs/README.md`, fix `project-tracker.toml` -> `rula.toml` in `AGENTS.md:95`).
- **[COMPLETED] WP-2: Critical Bugs & Cross-Project Isolation (`B-01` to `B-08`)**
  - Fix LanceDB chunk ID collision (`chunker.rs`), broken JIT line search (`semantic/mod.rs`), 16-byte ELF header buffer (`rea.rs`), cross-project ID ownership checks in 9 MCP tools (`mcp.rs`), destructive `rula.toml` fallback and `[electronics]` section bug (`electronics/`), missing `"desigma"`/`"pcbcraft"` keys in `manifest.rs`, Hyprland window project verification (`desigma.rs`, `solidcraft.rs`, `pcbcraft.rs`), and paste `preventDefault()` ordering (`FloatingChatOverlay.vue`).
- **[COMPLETED] WP-3: Dead Code & Legacy Shim Purge (`audit_03` Parts 1 & 2)**
  - Purge the 10 dead frontend files (`3,735 LOC`), vestigial `workspaceLayoutStore` artifact-panel state, 3 unused daemon commands (`read_process_output`, `get_effective_project_config`, `check_electronics_toolchain`), 8 unused Rust functions, and obsolete root/template files (`project-tracker.toml`, `belter_ticker_prompt.txt`, `04_ipc_connection_belta.json`).
- **[COMPLETED] WP-4: Permissive Fallbacks & Silent Error Elimination (`audit_03` Part 3)**
  - Remove hardcoded `"bosmang"` callback fallback (`mcp.rs`), synthetic `greenfield` init state (`initializer.rs`), silent Genesis Milestone title substitution (`mcp.rs`), duplicate prompt dropping (`supervisor.rs`), `.one-ship/rea` directory override (`hydration.rs`), and frontend `resolveAgentProjectPath` guessing.
- **[COMPLETED] WP-5: Specification Synchronization — Core, Runtime & Telemetry (`Specs 01, 04, 05, 06, 08, 09, 10, 12, 13, 16, 17, 18, 19, 20, 21, 22, 23, 24, 26, 29, 30`)**
  - Case-by-case review separating Phenomenon A updates from Phenomenon B scope decisions ( plus fixing the `dna.rs` / `manifest.json` portrait bug in Spec 21).
- **[COMPLETED] WP-6: Specification & Scope Alignment — Hardware, CAD/PCB & Preview (`Specs 31, 32, 33, 34` + `Preview` Tab)**
  - Case-by-case decision on Spec 31 unimplemented sections (`HardwareInstrument` trait, mocked Serial Monitor in `ProjectElectronicsTab.vue`, EAGLE/KiCad parsers, `ngspice`, 6 `electronics_*` MCP tools) and Spec 33 SPICE bridge.
- **[COMPLETED] WP-7: Backend Async Purity, Bounded Buffers & Tiger Style (`audit_04` Parts 2, 3, 4)**
  - Offload blocking handlers in `mcp.rs`, `semantic/`, and `kopeng/` to `spawn_blocking`, cap `collected_stdout`/`stderr` in `spawn_task`, wire `mcp.approval_ttl_ms` timeout in `await_user_approval`, and standardize Tiger Style unit suffixes.
- **[COMPLETED] WP-8: Frontend UI Component Library, Theme Variables & Store Boundaries (`audit_04` Parts 1 & 5)**
  - Replace 25 native `title=` attributes with `AppTooltip`, 23 raw HTML inputs with `AppInput`/`AppTextarea`/`AppToggle`, ~100 hardcoded Tailwind palette lines with semantic CSS variables, fix `:key="idx"` and `any`, and clean up untracked timers/`voiceStore.teardown()`.
- **[COMPLETED] WP-9: Universal Logging, Rustdoc Coverage & Portability Packaging (`audit_04` Parts 4 & 6)**
  - Add `DEBUG`/`INFO`/`WARN`/`ERROR` logging to `project_mgmt.rs`, `electronics/`, `solidcraft.rs`, and `documents.rs`; add `///` Rustdoc to undocumented `pub` items; replace the 7 hardcoded `/home/butcherwutcher` paths; prepare `PKGBUILD` and production systemd service units.
