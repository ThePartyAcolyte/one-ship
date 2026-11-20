# Universal Telemetry, Continuous Profiling & Profiler Engines

## Purpose

Define ONE-SHIP's continuous process-tree profiling loop bound to every supervised service and task run, its append-only DuckDB storage and aggregation model, its declarative `manifest.toml` profiler engine wrapper framework, and missing-profiler onboarding flag resolution.

## Scope

In scope:
- Continuous sampling of `Working` persistent services and transient tasks into DuckDB (`oneship_telemetry.duckdb`).
- Daemon-exclusive telemetry ownership, `supervised_processes` linkage, run profile aggregation, and cross-run comparison deltas.
- Declarative profiler engine discovery (`project` > `global` > `built-in`), strict `manifest.toml` schema, template token substitution, and graceful `SIGINT` shutdown timing.
- Daemon commands, MCP profiler tools, `ProjectProfilerTab.vue`, and `[! INIT: NO PROFILER]` resolution workflow.
- Historical removals and remaining open roadmap items.

Out of scope:
- Native process supervision, `ONESHIP_RUN_ID` attestation, and startup orphan reconciliation (covered in [Project Registry, Scripts & Service Orchestration](../03-projects/01-project-registry-and-service-orchestration.md)).
- Global plugin cascade and `rula.toml` configuration mechanics (covered in [Plugin Architecture (Kopeng)](../01-architecture/01-plugin-architecture.md) and [Configuration Schema and Cascade](../01-architecture/02-configuration-schema-and-cascade.md)).

## Model / Design

### 1. Core Principles & Architecture

ONE-SHIP orchestrates polyglot projects whose persistent services (`[scripts.persistent.<id>]`) and tasks (`[scripts.task.<id>]`) run under `ProcessSupervisor`. **Profiling is continuous and bound to every run**: no manual profiling session is started. Whenever a service or task runs, the daemon samples its entire process tree for the duration of the run, building an append-only history for detecting regressions, memory growth, and per-process resource consumption.

- **A run is the unit:** One `run_id` (`svc_<id>_<epoch_ms>`, `tsk_<id>_<exec>_<epoch_ms>`) corresponds to one row in `supervised_processes` and one profile.
- **Samples are kept:** There is no pruning or aggregation; retention policies are revisited only if the store reaches ~100 GB.
- **Profiler engines** (`samply`, `py-spy`, ...) add code-level detail by wrapping a service launch and linking the engine's output artifact to the run.

```text
ProcessSupervisor (daemon)
  ├─ spawn_service / spawn_task ── supervised_processes row (run_id, engine, artifact path)
  │      └─ engine wrapper (optional) ── scratchpad/profiling/<run_id>.<ext>
  └─ running_runs() ─────────────────┐
                                     ▼
Continuous profiler loop (telemetry.rs, every profiler.sample_interval_ms)
  └─ process tree of each running run ──► DuckDB process_samples (never pruned)
                                     ▼
Daemon commands / MCP tools: get_target_runs, get_run_profile, compare_runs
  └─ Profiler tab: run history, run profile, run comparison
```

### 2. Continuous Profiler & Telemetry Ownership

#### 2.1 Sampling Loop (`telemetry::spawn_continuous_profiler_loop`)
Every `profiler.sample_interval_ms` (default `500` ms):
1. Reads active runs from `ProcessSupervisor::running_runs`—every service and task in `Working` state with its real `run_id` and root PID. Shells and agents are excluded; adopted services (from a previous daemon or port adoption) are included.
2. Refreshes host processes via `sysinfo` (CPU, memory, disk I/O, command line, task list) on the blocking thread pool.
3. Builds each run's process tree (`ProcessForest`). On Linux, `sysinfo` reports OS threads as child entries whose parent is their owning process; threads are excluded from the process tree and counted in `threads_count` so memory is summed once per process.
4. Appends one `process_samples` row per process in each tree using the DuckDB `Appender`.

A run shorter than one sampling interval records no samples. Separately, the host telemetry loop (`telemetry.telemetry_interval_ms`, default `2000` ms) reads the same running runs to broadcast per-service `services-telemetry` SSE events for the topbar.

#### 2.2 Daemon-Exclusive DuckDB Ownership
Because DuckDB permits only one writer process, `oneship-daemon` is the sole owner of `oneship_telemetry.duckdb`:
- Opening the store occurs at daemon startup; failure aborts startup.
- The desktop GUI never opens the DuckDB file directly.
- There is no in-memory fallback: a locked file fails with `TelemetryError`, and an unreachable daemon surfaces as `DaemonUnreachableError` in the UI.
- All DuckDB results pass through `DuckResultExt::duck_context`, which logs failures at `ERROR` before returning `TelemetryError`.

#### 2.3 Run Linkage (`supervised_processes`)
Run history queries `supervised_processes` filtered to `target_type IN ('service', 'task')`, linking engine artifacts via two columns:

| Column | Meaning |
| :--- | :--- |
| `profiler_engine_id` | Engine ID that wrapped the run, or `NULL` |
| `profile_artifact_path` | Project-relative engine output file: `scratchpad/profiling/<run_id>.<visual_extension>` |

A run's termination is recorded once (`db_record_process_termination` updates only rows where `terminated_at_ms IS NULL`): the process watcher records the real exit code (`128 + signal` for signal deaths) and terminal status (`completed`, `failed`, or `stopped`).

#### 2.4 Run Profile Aggregation & Cross-Run Comparison
`duck::get_run_profile(run_id)` aggregates per tick first (summing the entire process tree at each instant), then aggregates across ticks:
- Tick count, first/last sample timestamps, and sampled seconds.
- CPU average, p50, p95 (`QUANTILE_CONT`), and peak.
- RSS memory average and peak; peak process count; peak thread count; total disk bytes read and written.
- Per-process-name breakdown (instances of the same process name summed per tick): ticks present, peak instances, CPU average/peak, RSS average/peak, ordered descending by average CPU and capped at `profiler.process_breakdown_max_items_count` (default `20`).

`compare_runs` returns both profiles and after-minus-before deltas for sampled seconds, CPU (average, p95, peak), and RSS (average, peak); the UI joins per-process lists by process name. Both runs must exist (`ProfilerError::RunNotFound`) and contain samples (`ProfilerError::NoSamples`).

### 3. Declarative Profiler Engines (`manifest.toml`)

#### 3.1 Discovery Cascade
`kopeng::profiler::discover_profilers(project)` resolves engines in strict precedence order (the first manifest with a given `id` wins):

```text
1. Project:   <project>/.one-ship/plugins/<dir>/manifest.toml
2. Global:    ~/.config/one-ship/plugins/<dir>/manifest.toml
3. Built-in:  samply (shipped in the daemon binary)
```

Manifests are read on demand (engine listing, service start) with no file watcher:
- A missing plugin directory yields zero manifests; an unreadable directory returns `ProfilerError::ReadDir`; an unavailable config directory returns `ProfilerError::ConfigDirUnavailable`.
- Manifests that fail parsing or validation are never silently ignored—they are returned as `ProfilerManifestFault` (`manifest_path`, `error`) alongside valid engines, displayed in the Profiler tab, and logged at `WARN`.

#### 3.2 Binding, Wrapping & Graceful Shutdown
A persistent service binds an engine by ID in `rula.toml`:

```toml
[scripts.persistent.backend]
command = "uvicorn app.main:app --port 8000"
profiler = "py-spy"
```

Any non-string form of `profiler` (such as a TOML table) fails service start with `ScriptError::InvalidProfiler`; engine parameters belong in `manifest.toml`.

`ProcessSupervisor::spawn_service`:
1. Resolves the manifest (`SupervisorError::ProfilerNotFound` if missing or invalid) and executes `[requirements].check_command` (`SupervisorError::ProfilerNotInstalled` if non-zero exit).
2. Creates `<project>/scratchpad/profiling/`, expands the wrapper `command_template`, and launches it via `/bin/sh -c`.
3. Persists `profiler_engine_id` and `profile_artifact_path` on the run row.

Wrapper mode is the only execution mode: the engine starts and stops with the run. Stopping a wrapped run sends `SIGINT` to its process group and waits `profiler.engine_shutdown_grace_period_ms` (default `15000` ms) instead of `supervisor.shutdown_grace_period_ms`, giving the engine time to flush its artifact before `SIGKILL`. Output (`stdout`/`stderr`) readers remain open until the process is reaped so late writes during shutdown cannot trigger `SIGPIPE`.

### 4. Initialization Flags & Profiler Crafting Workflow

1. **Registration:** A tech stack without a matching profiler engine registers a `profiler` row in `project_initialization_flags` via `raise_initialization_flag`. Greenfield projects never raise this flag.
2. **Project Card Badge:** Pending flags render as `[! INIT: NO PROFILER]` badges; clicking one launches a resolution session in the Chat Overlay.
3. **Resolution:**
   - `oneship-profiler-crafter` is symlinked from `templates/skills/` into `<project>/.agents/skills/` (see [Platform Skill Injection](../01-architecture/03-platform-skill-injection.md)).
   - The agent inspects the stack, drafts a strict `manifest.toml`, and requests user approval (`request_approval`).
   - It writes the manifest to `~/.config/one-ship/plugins/<dir>/` or `<project>/.one-ship/plugins/<dir>/`, verifies that the Profiler tab loads it without a fault, and binds `profiler = "<id>"` in `rula.toml`.
   - It calls `resolve_initialization_flag`, clearing the badge via SSE.

### 5. Removed Design Elements & Open Roadmap

- **Historical Removals (2026-11-06):**
  - *On-demand profiling:* Removed `profile_service` MCP tool and `profile_service_cmd`, the 2-second `sysinfo` sampler, synthetic hotspots, SVG summary generation, and `query_profiler_hotspots`.
  - *Continuous profiler redesign:* Dropped unused SQLite tables `profiler_runs`, `profiler_hotspots`, and `profiler_timeline_buckets` on startup; removed `get_continuous_profile`, `get_profiler_runs`, and function-hotspot `compare_profiler_runs`; removed sample pruning, startup compaction, and config keys `telemetry.retention_window_hours`, `telemetry.prune_interval_minutes`, `telemetry.duckdb_compaction_threshold_mbytes`, `profiler.default_driver`, `profiler.max_hotspots_count`, and `profiler.min_cpu_threshold_percent`; removed attach mode, summary files, `{pid}` and `{summary_file}` template tokens, table-form `profiler` bindings in `rula.toml`, and `.kapawu/plugins` lookup paths.
- **Open Roadmap:**
  - Engine output artifacts are stored and opened externally (`open_file_in_external_editor`), not yet parsed per `viewer_type` for function-level hotspot extraction and diffing.
  - No built-in engine wraps `npm run ...` scripts directly because `node --cpu-prof` must wrap the `node` binary rather than the `npm` launcher.

## Contracts

### 1. DuckDB Schema (`oneship_telemetry.duckdb`)

Created on open and extended idempotently via `ADD COLUMN IF NOT EXISTS`. Rows written before `2026-11-06` counted threads as processes, have empty `cmdline`, and store `NULL` for the final four columns:

```sql
CREATE TABLE IF NOT EXISTS process_samples (
    sampled_at_ms BIGINT,
    project_id BIGINT,
    target_type VARCHAR,          -- 'service' | 'task'
    target_id VARCHAR,
    run_id VARCHAR,
    pid UINTEGER,
    process_name VARCHAR,
    cmdline VARCHAR,
    cpu_usage_percent FLOAT,
    rss_memory_bytes UBIGINT,
    rss_memory_mbytes FLOAT,
    virtual_memory_bytes UBIGINT, -- NULL before 2026-11-06
    threads_count UINTEGER,       -- NULL before 2026-11-06
    disk_read_bytes UBIGINT,      -- since the previous tick; NULL before 2026-11-06
    disk_written_bytes UBIGINT    -- since the previous tick; NULL before 2026-11-06
);
CREATE INDEX IF NOT EXISTS idx_samples_run ON process_samples (run_id);
```

### 2. Profiler Manifest Schema (`manifest.toml`) & Template Tokens

Parsed strictly with `deny_unknown_fields`:

```toml
[plugin]
id = "py-spy"
name = "py-spy Python Sampling Profiler"
version = "0.4.2"
type = "profiler"            # required value
author = "ONE-SHIP Agent"    # optional
description = "Sampling profiler for Python services"

[detection]                  # optional; read by initializer and profiler-crafter skill
file_triggers = ["pyproject.toml", "requirements.txt", "*.py"]
binary_triggers = ["python", "python3", "uvicorn"]

[requirements]
check_command = "py-spy --version"   # exit 0 = installed
install_hint = "pacman -S py-spy"    # optional

[requirements.install]               # optional
arch = "pacman -S --noconfirm py-spy"
debian = "apt-get install -y py-spy"
pip = "pip install py-spy"
cargo = "cargo install py-spy"
npm = "npm install -g <pkg>"
script = "scripts/install.sh"
requires_sudo = false

[execution.defaults]
sampling_rate_hz = 100
format = "flamegraph"

[execution.modes.wrapper]
command_template = "py-spy record --rate {sampling_rate_hz} --format {format} --output {output_file} -- {original_command}"

[artifacts]
visual_extension = "svg"
viewer_type = "flamegraph"   # flamegraph | speedscope | firefox_profiler | text
```

`command_template` MUST include `{original_command}` and `{output_file}`, and the engine MUST emit its output artifact on `SIGINT`:

| Token | Value |
| :--- | :--- |
| `{original_command}` | The service command line from `rula.toml`, arguments shell-quoted |
| `{output_file}` | Absolute, shell-quoted path of `<project>/scratchpad/profiling/<run_id>.<visual_extension>` |
| `{sampling_rate_hz}` | `execution.defaults.sampling_rate_hz` |
| `{format}` | `execution.defaults.format` |

### 3. Daemon Commands, MCP Tools, Events & UI

| Daemon Command | Args | Result |
| :--- | :--- | :--- |
| `list_available_profilers` | `projectPath` | `ProfilerCatalog` (`engines` with install state, `faults`) |
| `get_target_runs` | `projectPath`, `targetId?`, `limitCount` | `TargetRunRecord[]`, newest first |
| `get_run_profile` | `runId` | `RunProfile \| null` (`null`: no samples yet) |
| `compare_runs` | `runBeforeId`, `runAfterId` | `RunComparison` |
| `toggle_project_profiler_plugin` | `projectPath`, `enabled` | `boolean` (`Result<bool, ProfilerError>`, toggles `[plugins.profiler].enabled` in `rula.toml`) |

- **Errors:** Typed `ProfilerError` (`RunNotFound` 404, `NoSamples` 409, `ReadDir`/`ConfigDirUnavailable` 500, wrapped registry/database/telemetry errors) and `TelemetryError` (500).
- **MCP Tools:** `list_profiler_runs` (`project?`, `target_id?`, `limit_count`), `get_run_profile` (`run_id`), `compare_profiler_runs` (`run_before_id`, `run_after_id`).
- **SSE Events (`/api/events`):** `services-telemetry` (topbar live per-service resource telemetry).
- **Frontend UI:** `ProjectProfilerTab.vue` and `useProfilerStore` display engines, manifest faults, target-filtered run history, run profile and process breakdown, artifact launch (`open_file_in_external_editor`), and two-run comparison; conditionally displayed in `ProjectWorkspace.vue` via `has_profiler_enabled`.

### 4. Configuration Keys (`[plugins.profiler]` & `[plugins.telemetry]`)

| Key | Type | Scope | Default | Purpose |
| :--- | :--- | :--- | :--- | :--- |
| `profiler.enabled` | `boolean` | `cascade` | `true` | Enables continuous telemetry, profiling, and the Profiling workspace tab. |
| `profiler.sample_interval_ms` | `number` | `global_only` | `500` | Sampling interval across running service and task process trees. |
| `profiler.engine_shutdown_grace_period_ms` | `number` | `global_only` | `15000` | Grace window between `SIGINT` and `SIGKILL` when stopping a wrapped service. |
| `profiler.process_breakdown_max_items_count` | `number` | `global_only` | `20` | Maximum process names returned in a run profile, ordered by average CPU. |
| `telemetry.telemetry_interval_ms` | `number` | `global_only` | `2000` | Interval for broadcasting `services-telemetry` SSE payloads. |

## Implementation Status

| Area | Status | Notes |
| :--- | :--- | :--- |
| DuckDB `process_samples` store, `TelemetryError`, `DuckResultExt` | Implemented | Verified in `src-tauri/src/duck.rs`. |
| Continuous profiler loop & `services-telemetry` event loop | Implemented | Verified in `src-tauri/src/telemetry.rs` and `src-tauri/src/events.rs`. |
| Declarative profiler discovery, built-in `samply` manifest, strict parsing, wrapper spawning, `SIGINT` grace period | Implemented | Verified in `src-tauri/src/kopeng/profiler.rs` and `src-tauri/src/supervisor.rs`. |
| Daemon commands (`list_available_profilers`, `get_target_runs`, `get_run_profile`, `compare_runs`, `toggle_project_profiler_plugin`) & MCP tools (`list_profiler_runs`, `get_run_profile`, `compare_profiler_runs`) | Implemented | Verified in `src-tauri/src/api.rs`, `src-tauri/src/mcp.rs`, and `src-tauri/src/capabilities/profiler.rs`. |
| Profiler UI (`ProjectProfilerTab.vue`, `useProfilerStore.ts`) | Implemented | Verified file presence in `src/components/workspace/` and `src/stores/`. |
| Engine artifact parsing (`viewer_type` hotspot parsers) & `npm run` Node wrapper | Roadmap | Explicitly listed as open work in §5; artifacts open externally via `open_file_in_external_editor`. |

## Related Specs

- [Observability Index](README.md)
- [Session Lifecycle, Database Devlog & Knowledge Graph](01-session-lifecycle-and-devlog-graph.md)
- [Plugin Architecture (Kopeng)](../01-architecture/01-plugin-architecture.md)
- [Configuration Schema and Cascade](../01-architecture/02-configuration-schema-and-cascade.md)
- [Platform Skill Injection](../01-architecture/03-platform-skill-injection.md)
- [Project Registry, Scripts & Service Orchestration](../03-projects/01-project-registry-and-service-orchestration.md)

Supersedes legacy: `25_universal_telemetry_and_profiling_engine.md`
