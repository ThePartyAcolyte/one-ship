# 2026-11-06 — Profiler Redesign: Continuous, Run-Bound Profiling

Follow-up of `2026-11-06_daemon_ownership_stage3.md`. It implements the approved redesign (2026-11-05): profiling is continuous and bound to every run, never a manual session.

## Decisions (user, 2026-11-06)
- **Run registry:** a run is a `supervised_processes` row. The `profiler_*` tables are dropped.
- **Orderly stop:** services and tasks stop with SIGINT, then a grace period, then SIGKILL.
- **Comparison:** compares DuckDB metrics. Function-level comparison waits for engine output parsing.
- **Manifests:** the spec and skill format with `sampling_rate_hz` / `{sampling_rate_hz}`. The user's two global manifests are migrated.
- **Attach mode:** removed.
- **`rula.toml`:** only `profiler = "<id>"`.

## Defects found
1. **The engine was never applied.**
   - Real manifests, the spec and the skill use `[execution.defaults]` / `[execution.modes.wrapper]`. The code read `[defaults]` / `[execution.wrapper]` and silently ignored the rest.
   - So a service with `profiler = "py-spy"` started unwrapped.
   - The code read `.kapawu/plugins`; the skill writes `.one-ship/plugins`.
2. **Engines could never write their file.**
   - Stopping a service sent SIGKILL at once.
   - The readers closed the pipes on cancel, so output written while exiting would have killed the run with SIGPIPE.
3. **Every stop waited the full grace period.** The cancel path never set the status `stop_process` polls.
4. **Threads were counted as processes.**
   - With `sysinfo` 0.39 every Linux thread is listed as a child of its process.
   - The sampler summed each thread's RSS (the process's own) and CPU.
   - This affects existing DuckDB samples and the topbar service metrics.
5. **`cmdline` was always empty.** The refresh kind does not include the command line.
6. **The run history was always empty.** Nothing wrote `profiler_runs`, `profiler_hotspots` or `profiler_timeline_buckets`.
7. **Samples were pruned after 2 h**, against the "keep everything" decision, and compacted at startup.
8. **Invented run ids.** A run without a `run_id` got `svc_<id>_<project>`; port adoption fell back to a new id.
9. **Agent events for services.** `stop_process` emitted agent events for services and tasks: it checked the id for `svc:`, but the prefix is in the key.
10. **A stopped task was recorded as `failed` with -1.**
11. **Sync work in async tasks.** Process sampling (sysinfo refresh, SQLite, DuckDB) ran directly in async tasks.
12. **Sequential daemon drain.** `stop_all_processes` stopped processes one by one; two profiled services would have exceeded the unit's 30 s `TimeoutStopSec`.

## Backend
- **`kopeng/profiler.rs`:**
  - **Manifests:** strict `ProfilerManifest` (`deny_unknown_fields`, required wrapper, `ProfilerViewerType` enum), validated by `type = "profiler"` and the `{original_command}` / `{output_file}` tokens.
  - **Discovery:** `discover_profilers` returns the engines plus `ProfilerManifestFault`s. A missing config dir is an error, and an unreadable plugin dir is `ReadDir`.
  - **Built-in samply:** carries the package version and `--save-only`.
  - **Commands:** `list_available_profilers` (`ProfilerCatalog`), `get_target_runs`, `get_run_profile` and `compare_runs` (`RunComparison` with deltas).
  - **`ProfilerError`** gains `ConfigDirUnavailable`, `ReadDir`, `RunNotFound`, `NoSamples` and `Telemetry`.
- **`db.rs`:**
  - **Tables:** the `profiler_*` tables are dropped. `supervised_processes` gains `profiler_engine_id` and `profile_artifact_path`, and an index on `(project_id, target_id, spawned_at_ms)`.
  - **Queries:** `TargetRunRecord`, `db_get_target_runs` and `db_get_target_run` are new.
  - **Termination:** `db_record_process_termination` records once (`terminated_at_ms IS NULL`).
- **`duck.rs`:**
  - **Samples:** four new columns (virtual memory, threads, disk read and written), never pruned.
  - **Profile:** `get_run_profile` aggregates per tick, then over ticks, and adds a per-process breakdown.
  - **Removed:** compaction, prune, `get_latest_run_id`, the window summary and the unused `TelemetryError::{WalLeftOver, Task}`.
- **`telemetry.rs`:**
  - **Runs:** both loops read `ProcessSupervisor::running_runs()` instead of parsing every `rula.toml` at each tick.
  - **Process trees:** `ProcessForest` (one parent index per tick, threads excluded) replaces the recursive `collect_descendants`.
  - **Blocking pool:** each tick runs there.
  - **Removed:** `get_continuous_profile`.
- **`supervisor.rs`:**
  - **Stop:**
    - `stop_child_in_order`, `stop_adopted_in_order` and a shared `spawn_adopted_service_watcher` replace the two copies of the adopted watcher.
    - `ManagedProcess.stop_grace_period_ms`.
    - Separate reader tokens.
    - `stop_all_processes` is concurrent.
  - **Engine:** `resolve_profiler_engine` (manifest plus `check_command`, `ProfilerNotInstalled`).
  - **Runs:** `running_runs`, `key_kind`.
  - **Fixes:** `get_service_pid` only while `Working`; `get_service_run_id` removed; exit codes `128 + signal`.
- **`hydration.rs`:** `ScriptEntry.profiler: Option<String>`; `ScriptError::InvalidProfiler` for a table at launch.
- **`config.rs`:**
  - **Added:** `profiler.engine_shutdown_grace_period_ms` (15000) and `profiler.process_breakdown_max_items_count` (20).
  - **Removed:** the pruning, compaction, driver, hotspot and CPU-threshold keys.
- **`mcp.rs`:** `list_profiler_runs` (`target_id`, required `limit_count`) and `compare_profiler_runs` are rewritten, and `get_run_profile` is new.
- **`api.rs`:** 147 commands. 3 were removed and 3 added: `get_target_runs`, `get_run_profile`, `compare_runs`.

## Frontend
- **`useProfilerStore`:** rewritten (catalogs, runs, profiles, comparison, `lastError` shown in the tab).
- **`ProjectProfilerTab`:** rewritten.
  - **Engines:** listed with their install state, and manifest faults are shown.
  - **Runs:** a run list with status, exit code and engine; the selected run shows its profile metrics and process table.
  - **Engine file:** an open button for it (`open_file_in_external_editor`).
  - **Comparison:** DIFF with metric deltas and per-process deltas.
  - **Removed:** the inline SVG `v-html`; engine SVGs carry their own scripts.
- **Types:** generated ones; the stale generated files and `types/profiler.ts` are deleted. The hand-written `ScriptEntry.profiler` becomes `string | null`.
- **Terminal grid:** its badge shows the engine id instead of "PROFILED", since every run is profiled now.

## Docs
- **Spec 25:** rewritten (0.3.1).
- **Other specs:**
  - Spec 08: orderly stop, drain, `running_runs`, profiler errors.
  - Spec 06: discovery.
  - Spec 10: keys.
  - Spec 12: commands and tools.
- **Skills:** `oneship-profiler-crafter` 0.2.6 (strict format, wrapper contract, verification step); the `oneship-rea` profiler step.
- **`TECHNICAL_DEBT.md` §25:** `.kapawu/plugins` removed.
- **`TODO.md`:** the redesign item becomes its runtime check; stale profiler lines in earlier checks corrected.
- **Outside the repo:** `~/.config/one-ship/plugins/profiler-{pyspy,v8prof}/manifest.toml` migrated to `sampling_rate_hz`, without attach or `summary_extension`.

## Verification
- `cargo check --all-targets` clean.
- `cargo test --lib`: 309 passed. The new tests cover:
  - manifest wrapping and paths;
  - faults listed rather than dropped;
  - a missing plugin dir;
  - built-in validity;
  - the per-tick run profile.
- `clippy` lib: 11 warnings, unchanged.
- `vue-tsc --noEmit` clean.
- Not run in the app.

## Migrated projects (user decision)
- **To `profiler = "py-spy"`:** `dictate-whisper` `daemon` and `watchtower` `backend`, both with the global manifest.
- **Declaration removed:** `watchtower` `frontend` (v8prof) and `estudiorc` `core-dev` (`driver = "node"`, which never had a manifest). Both run `npm run dev`, which no current engine can wrap; they keep the continuous sampling.
- **Deleted:** `dictate-whisper/.kapawu/` (only `plugins/profiler-pyspy`, an old-format duplicate of the global py-spy with id `pyspy`).

## Open
- **No Node engine:** `v8prof` wraps `{original_command}` with `node --cpu-prof`, which cannot run `npm run dev` (Spec 25 §8).
- **The user's `config.toml`** still sets the removed keys (ignored).
