# Development Session: 2026-10-17 — Living Specs Realignment & Process-Bound Continuous Profiling

## Context & Objectives
- **Target Problem 1 (Living Specifications vs SDA Drift):** In earlier releases, documentation styles drifted into a rigid, bureaucratic "SDA" (Software Design Architecture) format. The greenfield initializer imposed static architectural paperwork rather than facilitating an interactive, bidirectional pair-architect dialogue. In ONE-SHIP's philosophy:
  - `docs/00_manifesto.md` is an ontological baseline and boundary alignment document for agents and humans.
  - `docs/01_ubiquitous_language.md` stabilizes domain semantics across multi-turn sessions.
  - `docs/specs/*.md` are living technical specifications that guide code development but **must be updated bi-directionally in the same turn** when technical reality evolves. Task management remains native to the platform (`TODO.md`, `manage_todo_item`), avoiding redundant file-based task duplication.
- **Target Problem 2 (Process-Bound Profiling vs Arbitrary 15s Benchmark):** Profiling had become misaligned with Spec 25:
  - A hardcoded 15-second benchmark duration (`duration_seconds = 15`) was scattered across manifests, skills, backend sleep loops, and UI buttons.
  - In real polyglot development, a 15-second sample tells developers and AI agents nothing about real interactive usage or hot-reload behavior.
  - Profiling is not an isolated one-off task: when a persistent service has an associated profiler (e.g. Py-Spy, V8, Samply), the profiler must be **eternally bound to the service process**, launching and terminating together with it, and continuously feeding DuckDB telemetry.

---

## Key Changes & Implementations

### 1. Living Specifications & Greenfield Initializer Realignment
- **Prompt Architecture (`templates/prompts/greenfield_initializer.md`):**
  - Replaced rigid "SDA" terminology with `living-specs` (Living Specifications / Bidirectional Spec-Driven).
  - Clarified that `docs/00_manifesto.md` sets ontological baseline rules and non-negotiable boundaries, `docs/01_ubiquitous_language.md` stabilizes domain lexicon, and `docs/specs/*.md` serve as a bi-directional living source of truth synchronized atomically with code changes.
  - Updated selection options: `["living-specs", "lean", "contract", "test-anchored", "custom"]`.
- **Docstyle Skills (`templates/skills/oneship-docstyles/SKILL.md` & `oneship-docstyle-sdd/SKILL.md`):**
  - Renamed Docstyle 1 to "Living Specifications (Bidirectional Spec-Driven)".
  - Articulated the core invariant: specifications guide code, and specifications reflect system truth. When implementations change, specs must be updated in the same turn.

### 2. Process-Bound Continuous Profiling Architecture (Eliminating 15s)
- **Configuration Schema (`src-tauri/src/hydration.rs` & `templates/skills/oneship-core/SKILL.md`):**
  - Enabled string and table parsing for `profiler = "<plugin_id>"` under `[scripts.persistent.<id>]` in `rula.toml`.
  - Documented the `profiler` key in `oneship-core` as a permanently bound continuous profiling engine.
- **Skill Realignment (`templates/skills/oneship-profiler-crafter/SKILL.md`):**
  - Completely purged `duration_s = 15` and `--duration {duration_s}` from manifest templates and protocols.
  - Redefined `profiler-crafter` mission: diagnose stack, craft continuous wrapper/attach `manifest.toml`, bind `profiler = "<id>"` in `rula.toml`, and resolve the initialization flag.
- **Native Process Supervisor (`src-tauri/src/supervisor.rs`):**
  - In `spawn_service`: inspects `rula.toml` for `svc.profiler`.
  - If bound to a profiler with wrapper mode, transparently wraps the execution command (e.g. `py-spy record --output {output_file} --rate {sampling_rate} -- {original_command}`).
  - On `stop_process`, the whole process group (PGID) is terminated via `SIGKILL`, cleanly stopping the profiler and service atomically.
- **Backend Profiler Engine (`src-tauri/src/kopeng/profiler.rs`):**
  - Removed `default_duration_seconds() -> u32 { 15 }`. Made `duration_seconds` optional in `ProfilerExecutionDefaults`.
  - In `execute_profiling_session`: when continuous telemetry exists in DuckDB (`crate::duck::get_continuous_profile_summary`), immediately returns the in-vivo session summary in <10ms without sleeping or blocking.
- **Frontend UI (`ProjectTerminalGrid.vue` & `ProjectProfilerTab.vue`):**
  - In `ProjectTerminalGrid.vue`:
    - Removed `Profile Service (15s)` from the right-click context menu; replaced with `Performance Telemetry` navigating to the profiler tab.
    - Updated script card header: renders discrete `[PROFILED]` badge when `svc.profiler` is configured.
    - Updated snapshot action: tooltip changed to `Capture Performance Snapshot`, passing `durationS: null` to extract the accumulated DuckDB session snapshot immediately.
  - In `ProjectProfilerTab.vue`:
    - Removed 15s attach tooltip and hardcoded `15` parameter; changed to `Capture Performance Snapshot (${selectedServiceId})`.
- **Living Specifications Synchronization (`docs/specs/25_...` & `docs/specs/26_...`):**
  - Updated Spec 25 to document Section 4.4 Process-Bound Profiler Lifecycle (`rula.toml` integration) and purged legacy 15-second benchmark defaults.
  - Updated Spec 26 to reflect Living Specifications and bidirectional synchronization in Phase B.

---

## Verification & Compilation
- `cargo check`: Passed with 0 errors across all backend crates.
- `cargo test --lib supervisor::tests`: All 6 tests passed cleanly.
- `npm run build` (`vue-tsc --noEmit && vite build`): Passed cleanly with 0 errors.
