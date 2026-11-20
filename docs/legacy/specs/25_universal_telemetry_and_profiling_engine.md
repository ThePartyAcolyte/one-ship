> [!WARNING]
> **LEGACY SPECIFICATION:** Superseded by [04-observability/02-universal-telemetry-and-profiling.md](../../specs/04-observability/02-universal-telemetry-and-profiling.md). Preserved for historical provenance only.

# Spec 25: Universal Telemetry, Continuous Profiling & Profiler Engines
**Document Version:** 0.3.1  
**Domain:** Process Telemetry, DuckDB Continuous Profiling, Native Process Supervisor, Initialization Flags  
**Maintainer:** Kapawu / Omarchy Development Team  

---

## 1. Overview

ONE-SHIP orchestrates polyglot projects whose persistent services (`[scripts.persistent.<id>]`) and tasks (`[scripts.task.<id>]`) run under the native `ProcessSupervisor` (Spec 08).

**Profiling is continuous and bound to every run.** Nobody starts a profiling session: whenever a service or task runs, the daemon samples its whole process tree for the whole run and keeps the samples, building a history used later to find optimization opportunities (regressions between runs, memory growth, which process of the tree consumes what).

* **A run is the unit.** One `run_id` (`svc_<id>_<epoch_ms>`, `tsk_<id>_<exec>_<epoch_ms>`) is one row of `supervised_processes` and one profile.
* **Samples are kept.** There is no pruning or aggregation; retention is revisited only if the store reaches ~100 GB.
* **Profiler engines** (`samply`, `py-spy`, …) add code-level detail: a service that declares one is launched wrapped by it, and the engine's output file is linked to the run. The file is not parsed yet (§8).

---

## 2. Architecture

```
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

---

## 3. Continuous Profiler

### 3.1 Sampling
Every `profiler.sample_interval_ms` (default 500) the loop (`telemetry::spawn_continuous_profiler_loop`):
1. Takes the running runs from the supervisor (`ProcessSupervisor::running_runs`): every service and task in `Working` state, with its real `run_id` and root PID. Shells and agents are not runs. Adopted services (a run of a previous daemon, or found on its port) are included.
2. Refreshes the host's processes (`sysinfo`: CPU, memory, disk I/O, command line, task list) on the blocking pool.
3. Builds each run's process tree (`ProcessForest`). On Linux `sysinfo` also lists every thread as a process whose parent is its owner; threads are excluded from the tree and counted in `threads_count`, so memory is summed once per process.
4. Appends one `process_samples` row per process of each tree (DuckDB Appender).

A run shorter than one interval gets no sample. The host telemetry loop (`telemetry_interval_ms`) reads the same running runs to publish the per-service `services-telemetry` event (topbar).

### 3.2 Schema (`process_samples`, `oneship_telemetry.duckdb`)
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
The schema is created on open and extended with `ADD COLUMN IF NOT EXISTS`. Samples written before 2026-11-06 counted every thread as a process (inflated RSS and CPU) and have an empty `cmdline`.

### 3.3 Ownership: the Daemon Owns Telemetry
DuckDB allows one writer process, so telemetry has exactly one owner: `oneship-daemon`. It opens `oneship_telemetry.duckdb` (failure aborts daemon startup) and runs the profiler loop. The desktop app never opens the file. There is no in-memory substitute: a locked file is an error, and an unreachable daemon is the UI's `DaemonUnreachableError`. Store failures are `TelemetryError`; DuckDB results go through `DuckResultExt::duck_context`, which logs at ERROR.

### 3.4 Runs (`supervised_processes`)
The run history is `supervised_processes` (Spec 08), filtered to `target_type IN ('service', 'task')`. Two columns link a run to its engine:

| Column | Meaning |
| :--- | :--- |
| `profiler_engine_id` | Engine the run was wrapped by, or NULL |
| `profile_artifact_path` | Project-relative engine output file, `scratchpad/profiling/<run_id>.<visual_extension>` |

A run's termination is recorded once (`db_record_process_termination` only updates a row whose `terminated_at_ms` is NULL): the process watcher records the real exit code (`128 + signal` for a signal death) and status `completed`, `failed` or `stopped`.

### 3.5 Run Profile & Comparison
`duck::get_run_profile(run_id)` aggregates per tick first (the whole tree summed at each instant), then over ticks:
* tick count, first/last sample, sampled seconds;
* CPU average, p50, p95 (`QUANTILE_CONT`) and peak;
* RSS average and peak; peak processes; peak threads; total disk read/written;
* per process name (instances of a name summed per tick): ticks present, peak instances, CPU average/peak, RSS average/peak, ordered by average CPU, at most `profiler.process_breakdown_max_items_count` (default 20).

`compare_runs` returns both profiles and the after-minus-before deltas of sampled seconds, CPU (average, p95, peak) and RSS (average, peak); the UI joins the per-process lists by name. Both runs must exist (`ProfilerError::RunNotFound`) and have samples (`ProfilerError::NoSamples`).

---

## 4. Profiler Engines

### 4.1 Discovery
`kopeng::profiler::discover_profilers(project)` reads, in order (the first manifest with an id wins):
```
1. Project:   <project>/.one-ship/plugins/<dir>/manifest.toml
2. Global:    ~/.config/one-ship/plugins/<dir>/manifest.toml
3. Built-in:  samply (shipped in the daemon binary)
```
Manifests are read when needed (engine list, service start); there is no file watcher. A missing plugins directory has no manifests; an unreadable one is `ProfilerError::ReadDir`; a missing config directory is `ProfilerError::ConfigDirUnavailable`. A manifest that does not parse or validate is never ignored: it is returned as a fault (`ProfilerManifestFault`, path and error) next to the engines, shown in the Profiler tab and logged at WARN.

### 4.2 Manifest (`manifest.toml`)
Parsed strictly (`deny_unknown_fields`):

```toml
[plugin]
id = "py-spy"
name = "py-spy Python Sampling Profiler"
version = "0.4.2"
type = "profiler"            # required value
author = "ONE-SHIP Agent"    # optional
description = "Sampling profiler for Python services"

[detection]                  # optional; read by the initializer and the profiler-crafter skill
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

The wrapper is the only mode: the engine starts and stops with the run. The template must contain `{original_command}` and `{output_file}`. The engine must write its file when it receives SIGINT (§4.4).

### 4.3 Template Tokens
| Token | Value |
| :--- | :--- |
| `{original_command}` | The service command line from `rula.toml`, arguments shell-quoted |
| `{output_file}` | Absolute, shell-quoted path of `<project>/scratchpad/profiling/<run_id>.<visual_extension>` |
| `{sampling_rate_hz}` | `execution.defaults.sampling_rate_hz` |
| `{format}` | `execution.defaults.format` |

### 4.4 Binding & Lifecycle
A persistent service declares its engine by id:
```toml
[scripts.persistent.backend]
command = "uvicorn app.main:app --port 8000"
profiler = "py-spy"
```
Any other form of `profiler` (a table) fails the start with `ScriptError::InvalidProfiler`; parameters live in the manifest.

`ProcessSupervisor::spawn_service`:
1. Resolves the manifest (`SupervisorError::ProfilerNotFound` when no valid manifest has the id) and runs its `check_command` (`SupervisorError::ProfilerNotInstalled`).
2. Creates `scratchpad/profiling/`, wraps the command with the template and runs it through `/bin/sh -c`.
3. Records `profiler_engine_id` and `profile_artifact_path` on the run.

Stopping a run (Spec 08) sends SIGINT to its process group, waits, then sends SIGKILL. A run with an engine waits `profiler.engine_shutdown_grace_period_ms` (default 15000) instead of `supervisor.shutdown_grace_period_ms`, since the engine writes its file after the run exits. The run's stdout/stderr readers stay open until the run is reaped, so output written while stopping cannot kill it with SIGPIPE.

---

## 5. Daemon Commands & MCP Tools

| Daemon command | Args | Result |
| :--- | :--- | :--- |
| `list_available_profilers` | `projectPath` | `ProfilerCatalog` (`engines` with install state, `faults`) |
| `get_target_runs` | `projectPath`, `targetId?`, `limitCount` | `TargetRunRecord[]`, newest first |
| `get_run_profile` | `runId` | `RunProfile \| null` (null: no samples yet) |
| `compare_runs` | `runBeforeId`, `runAfterId` | `RunComparison` |
| `toggle_project_profiler_plugin` | `projectPath`, `enabled` | `boolean` (`Result<bool, ProfilerError>`, toggles `[plugins.profiler] enabled` in `rula.toml`) |

Errors are `ProfilerError` (`RunNotFound` 404, `NoSamples` 409, `ReadDir`/`ConfigDirUnavailable` 500, wrapped registry, database and telemetry errors).

MCP tools for agents: `list_profiler_runs` (`project?`, `target_id?`, `limit_count`), `get_run_profile` (`run_id`), `compare_profiler_runs` (`run_before_id`, `run_after_id`).

The Profiler tab (`ProjectProfilerTab.vue`, `useProfilerStore`) shows the engines and manifest faults, the run history (filter by target), the selected run's profile and process breakdown, a button that opens the engine output file (`open_file_in_external_editor`), and the comparison of two runs. The tab is conditionally displayed in `ProjectWorkspace.vue` when `has_profiler_enabled` is true, and managed via the Project Overview feature matrix.

---

## 6. Initialization Flags & Engine Crafting

### 6.1 Registration
A stack without an engine is registered as a `profiler` row of `project_initialization_flags` (`raise_initialization_flag`). On a greenfield project the flag is never raised (AGENTS.md §7.2).

### 6.2 Project Card
Pending flags render as badges (`[! INIT: NO PROFILER]`); clicking one opens a resolution session in the Chat Overlay (Spec 18).

### 6.3 Resolution
1. `oneship-profiler-crafter` is symlinked from `templates/skills/` into `<project>/.agents/skills/`.
2. The agent inspects the stack, drafts a manifest in the §4.2 format and asks for approval (`request_approval`).
3. It writes the manifest to `~/.config/one-ship/plugins/<dir>/` or `<project>/.one-ship/plugins/<dir>/`, checks the Profiler tab shows it without a fault, and binds it with `profiler = "<id>"`.
4. It calls `resolve_initialization_flag`; the badge clears over SSE.

---

## 7. Removed

* **2026-11-06, on-demand profiling:** the `profile_service` MCP tool and `profile_service_cmd`, the 2 s sysinfo sampler, the synthetic hotspots, the SVG summary and `query_profiler_hotspots`.
* **2026-11-06, redesign:**
  * the SQLite tables `profiler_runs`, `profiler_hotspots` and `profiler_timeline_buckets`, which nothing wrote (dropped on startup);
  * `get_continuous_profile` (last N minutes of a target), `get_profiler_runs` and `compare_profiler_runs` (function hotspot diff);
  * sample pruning and the startup compaction, with the keys `telemetry.retention_window_hours`, `telemetry.prune_interval_minutes` and `telemetry.duckdb_compaction_threshold_mbytes`;
  * the unread keys `profiler.default_driver`, `profiler.max_hotspots_count` and `profiler.min_cpu_threshold_percent`;
  * the attach mode, the summary file, `{pid}` and `{summary_file}`, the table form of `profiler` in `rula.toml`, and the `.kapawu/plugins` project directory.

---

## 8. Open

* Engine output files are stored, not parsed: function-level hotspots and their comparison between runs come with a parser per `viewer_type`.
* No built-in engine wraps `npm run …` commands: `node --cpu-prof` must be the Node process itself, not a wrapper of `npm`.
