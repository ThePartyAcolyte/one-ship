# Devlog: Universal Telemetry, Pluggable Profiler Engine & Initialization Flag Migration

- **Date:** 2026-09-14  
- **Topic:** Universal Telemetry, Pluggable Profiler Engine & Initialization Flag Migration  
- **Status:** Completed (Specification, Phase 1 Backend Foundation & Phase 2 Relational Analytics & MCP Tool Integration)

---

## 1. Context & Objectives
To enable stack-agnostic performance analysis, resource monitoring, and profiling across diverse user projects managed by Wang-Kapawu:
1. Researched and designed an OS-level passive telemetry architecture (/proc sampling) to aggregate CPU % and PSS/RSS memory across Herder terminal process trees (`svc:*` and `tsk:*`).
2. Established a declarative, pluggable profiler driver architecture (`manifest.toml`) with a three-tier discovery cascade (Project Local -> Global User -> Core Built-in), eliminating binary recompilation.
3. Designed zero-friction launcher variable substitution (`{pid}`, `{output_file}`, `{summary_file}`, `{rate}`) and a dual-dimension artifact lifecycle (Visual SVG in `scratchpad/profiling/` vs relational normalized SQLite hotspots for low-token LLM analytics).
4. Standardized declarative installation recipes (`[requirements.install]`) for secure, programmatic execution in Herder task tabs.
5. Unified project bootstrapping gaps and unconfigured capabilities under `project_technical_debt` using `category = 'initialization_flag'`, integrating them with the MCP Dialogue Chat Overlay (Spec 18) and standard `resolve_technical_debt` workflows.

---

## 2. Changes Implemented

### A. Living Specifications Synchronized
- **Created Spec 25 (`docs/specs/25_universal_telemetry_and_profiling_engine.md`):** Complete blueprint for passive OS telemetry, declarative profiler drivers, dual artifact generation, relational normalized persistence, installation recipes, and technical debt integration.
- **Updated Spec 22 (`docs/specs/22_session_lifecycle_and_devlog_graph.md`):** Added `category` column to `project_technical_debt` table schema and `record_technical_debt` MCP tool contract (`initialization_flag`, `architectural`, `performance`, `maintainability`).
- **Updated Spec 06 (`docs/specs/06_plugin_architecture.md`):** Added Section 6 defining external declarative plugins (recipes) and the discovery cascade linking to Spec 25.

### B. Agent Skill Authored
- **Created `.agents/skills/kapawu-profiler-crafter/SKILL.md`:** Standard operating protocol for system and project agents to diagnose runtime stacks, formulate `manifest.toml` templates, verify dependencies, and resolve initialization technical debt items.

### C. Technical Debt Registered
- **Updated `TECHNICAL_DEBT.md` (Item 19):** Registered "Unify Project Initialization Flags into Structured Technical Debt (`initialization_flag` Category)" to migrate disparate historical flags into the unified database-backed technical debt ledger.

### D. Phase 1 Implementation (Backend Foundation & Base Telemetry)
- **Service Process Tree Telemetry (`src-tauri/src/telemetry.rs`):** Implemented `sample_running_services_telemetry` and recursive process tree descendant crawler (`collect_descendants`) using `sysinfo` to aggregate CPU% and RAM (PSS/RSS) across Herder pane process groups. Emits real-time `services-telemetry` `DaemonEvent` and exposes `get_services_telemetry_cmd`.
- **Database Schema & Migrations (`src-tauri/src/db.rs`):** Created `profiler_runs` and `profiler_hotspots` tables with relational persistence, added `category` to `project_technical_debt`, and implemented corresponding CRUD functions (`db_insert_profiler_run`, `db_insert_profiler_hotspots`, `db_get_profiler_runs`, `db_get_profiler_hotspots`, `db_compare_profiler_runs`).
- **Declarative Manifest Driver Engine (`src-tauri/src/kopeng/profiler.rs`):** Implemented `manifest.toml` parsing, multi-tier discovery cascade (project local, global user, core built-ins), command availability checking, and registered Tauri commands `list_available_profilers`, `get_profiler_runs_cmd`, `profile_service_cmd`, `query_profiler_hotspots_cmd`, and `compare_profiler_runs_cmd`.
- **Frontend Live Badge (`coreStore.ts` & `ScriptsSection.vue`):** Added `servicesTelemetry` reactive state in Pinia and rendered minimal, clean brutalist CPU & RAM badges on active persistent service rows with informative tooltips.

### E. Phase 2 Implementation (Relational Analytics, MCP Tools & Context Actions)
- **MCP Profiler Suite (`src-tauri/src/mcp.rs`):** Wired 4 dedicated MCP tools exposing the profiler engine to AI agents:
  1. `profile_service`: Non-intrusively captures a profiling session on a running persistent service, normalizes hotspots to SQLite, and emits an SVG visual artifact.
  2. `list_profiler_runs`: Queries historical runs for a project/service with optional limits.
  3. `query_profiler_hotspots`: Queries top function/module hotspots filtered by `min_cpu_percent` and `limit` for sub-millisecond, low-token (< 500 tokens) analysis.
  4. `compare_profiler_runs`: Computes relational regression diffs between two runs (`run_before_id` vs `run_after_id`) calculating delta self CPU% and cumulative CPU% per symbol.
- **Service Context Action (`ScriptsSection.vue`):** Added right-click context menu item `[⚡ Profile Service (15s)]` for active running persistent services with reactive progress feedback and status log integration.

---

## 3. Verification & Compliance
- Full adherence to live documentation rules and pre-1.0 versioning conventions (`0.2.1`).
- Verified zero compilation errors across Rust (`cargo check` in 0.24s) and Vue (`npm run build` in 3.81s).
- Preserved token economy guidelines for agents with relational normalized queries (< 500 tokens) vs heavy raw trace files.
- Zero mock backend logic and zero dead code.
