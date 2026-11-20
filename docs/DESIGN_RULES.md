# ONE-SHIP Design, Engineering & Architecture Rules (`docs/DESIGN_RULES.md`)

This document holds the detailed engineering, UI/UX, concurrency, logging, and onboarding standards referenced by [`AGENTS.md`](../AGENTS.md). Every rule here is mandatory.

---

## 1. Atomic Development & Anti-Mocking (Outside-In)

1. **No Partial Logic Mocks:**
   - Visual mockups in the UI are acceptable for layout design, but **never mock backend logic in the frontend** (e.g., using JS `setInterval` for process uptime that should be tracked by the daemon). If a feature requires backend state, wire it to the backend properly or leave it statically labeled as a placeholder until the backend is ready.
2. **Outside-In Flow:**
   - Develop features atomically from the foundation up:
     1. Global Machine State / Telemetry.
     2. Project Object & Database Registry.
     3. Project Configuration Parsing (`rula.toml`).
     4. Subprocess Orchestration & Agent Integrations.

---

## 2. UI & Design Rules: Component Library, Visual Simplicity & Tooltips

> [!IMPORTANT]
> **Mandatory Design Guide:** Before implementing or modifying any visual element, read [`Design System Foundations`](specs/07-ui/01-design-system-foundations.md), [`UI Component Library and Composition Recipes`](specs/07-ui/02-ui-component-library-and-recipes.md), and [`Workspace Chrome and Panel Contracts`](specs/07-ui/03-workspace-chrome-and-panel-contracts.md). Passing that checklist — tokens only, component reuse, full state coverage, targeted grep audit, spec and devlog updates — is part of the Definition of Done for every UI change.

1. **Mandatory UI Component Library (`src/components/ui/`):**
   - Always use standardized components (`AppTooltip`, `AppSelect`, `AppInput`, `AppTextarea`, `AppToggle`, `SubtabBar`, `AppEmptyState`) instead of ad-hoc HTML/Tailwind form elements.
   - `<select>` elements and `<option>` children must never render default OS white backgrounds; always use `AppSelect`.
2. **Unified Tooltip Standard (`AppTooltip`):**
   - Native HTML `title="..."` attributes are strictly prohibited on interactive buttons, icons, or metrics.
   - All hover descriptions must be wrapped in `<AppTooltip>` (`font-mono`, `text-[10px]`, `border-[var(--border-color)]`, `z-[99999]`).
3. **Minimalist Action Buttons (Symbol-Only):**
   - Action buttons that append items to contextual containers (e.g., `+` in agent lists, services, presets) MUST display only the action symbol (`+` or an icon) without redundant text labels. Explanations belong in `AppTooltip`.
4. **Contextual Feature Availability:**
   - Specialists and operations must not render when their prerequisites are absent (e.g., `git` agent hidden if `!project.has_git`).
5. **Concise & Direct UI Labeling (Zero Redundant Explanations):**
   - UI labels, buttons, navigation items, and directory headers MUST be clean and concise without redundant parentheticals (use `..`, not `.. (Parent Directory)`; use `Root`, not `Root (Workspace Root)`).

---

## 3. Target Operating Environment & Native Platform Specification

1. **Host Environment Baseline:**
   - **OS:** Omarchy Linux (Arch-based rolling distribution).
   - **Compositor / Display Server:** Hyprland on Wayland (`XDG_SESSION_TYPE=wayland`, `WAYLAND_DISPLAY=wayland-1`).
   - **Clipboard:** `wl-clipboard` (`/usr/bin/wl-paste`, `/usr/bin/wl-copy`).
2. **Hardware & Subsystem Integration:**
   - Design directly against Wayland tools (`wl-paste`, `hyprctl`) in the Rust backend rather than relying on browser/DOM synthetic fallbacks.
   - Design specifically for this host machine; do not introduce hypothetical cross-platform shims or multi-distro abstractions.

---

## 4. Project Onboarding & Lifecycle Protocol (Brownfield & Greenfield)

1. **Brownfield Onboarding (Deterministic Pre-Boot & Non-Invasive Reconnaissance):**
   - *Pre-Boot Injection (Daemon):* Injects runner MCP configs (`.mcp.json`, `opencode.json`), creates relative symlink `CLAUDE.md -> AGENTS.md`, symlinks Base Platform Skills (`oneship-core`, `oneship-delegation`, `oneship-devlog`, `oneship-snapshots`, `oneship-techdebt`, `oneship-artifacts`) from `templates/skills/` into `<project>/.agents/skills/`, and prepends the managed governance block to `AGENTS.md`.
   - *Execution Boundary:* Initializer Agent executes inside `<project_path>` managed by the Process Supervisor. Source code is strictly **read-only**.
   - *Configuration & Genesis:* Generates `rula.toml` from decoupled directory boundaries (≤ 100 words per prompt), records baseline milestone in `agent_session_journal`, and emits `raise_initialization_flag` for detected gaps (`docs`, `todos`, `tech_debt`, `profiler`).
2. **Greenfield Bootstrapping:**
   - Creates directory, initializes Git (`git init`), registers project in SQLite, symlinks Base Platform Skills + `oneship-docstyles` + `oneship-archetypes`, and injects runner configs.
   - Strictly forbidden from raising `profiler`, `todos`, or `tech_debt` flags on an empty greenfield directory.
3. **Phase 2: On-Demand Flag Resolution:**
   - Flags render on the Project Card (`[! INIT: DOCS]`, `[! INIT: TODOS]`, `[! INIT: TECH_DEBT]`, `[! INIT: NO PROFILER]`).
   - Specialized skills (`oneship-docstyles`, `oneship-profiler-crafter`) are symlinked on-demand only during resolution. Invoking `resolve_initialization_flag` closes the flag.

---

## 5. Tiger Style Engineering Standards (Performance, Safety & Contracts)

1. **Explicit Unit Suffixes:**
   - Numerical variables, fields, configuration keys, database columns, and IPC payload parameters MUST include explicit unit suffixes:
     - Time: `_ms`, `_us`, `_ns`, `_seconds`, `_minutes`, `_hours`, `_days`.
     - Size & Capacity: `_bytes`, `_kbytes`, `_mbytes`, `_gbytes`, `_lines_count`, `_items_count`, `_count`.
     - Ratios & Rates: `_percent`, `_ratio`, `_rate_hz`, `_bps`.
2. **Aggressive Invariants & Assertions (Crash Early & Loudly):**
   - Use `debug_assert!` and contract validations for internal invariants across process supervision, IPC routing, and state machine transitions. Fail immediately and noisily on invalid internal states.
3. **Bounded Memory Allocations & Ring Buffers:**
   - Unbounded dynamic allocations in persistent background loops, telemetry streams, and log captures are strictly prohibited. Use bounded Ring Buffers (`VecDeque` capped at `max_items_count`) and pre-allocate known sizes (`Vec::with_capacity`).
4. **Bounded Loops & Deterministic Timeouts:**
   - Every retry, polling, external CLI call (`git`, `glab`, `mpremote`, `pio`), or teardown loop must have explicit upper bounds (`max_attempts_count`), step delays (`poll_step_ms`), or deterministic timeouts.
5. **Shallow Nesting & Guard Clauses:**
   - Limit indentation depth (aim for ≤ 2 levels of nesting) using early returns and `?` propagation.

---

## 6. Backend Concurrency, Async Safety & Data Integrity (Rust & Tauri)

1. **Poison-Resistant Concurrency:**
   - Never access `RwLock` or `Mutex` via bare `.unwrap()` or `.expect()` in production paths. Use `.read_safe()` / `.write_safe()` or `.unwrap_or_else(PoisonError::into_inner)`.
2. **Async Runtime Purity:**
   - Synchronous filesystem operations (`std::fs::*`, `walkdir`), synchronous `db::*` queries, and blocking `std::process::Command` are forbidden inside `async fn`. Use `tokio::fs`, `tokio::process`, or `tokio::task::spawn_blocking` (or `blocking!` in `api.rs`).
3. **Strongly-Typed Domain Error Hierarchies (`thiserror`):**
   - Returning `Result<T, String>` in backend domain logic, supervisor routines, DB modules, voice tools, or IPC handlers is prohibited. Define typed error enums using `thiserror` implementing `api::ApiError`.
4. **Mandatory Transactional Batching:**
   - Iterative DB writes and multi-step read-then-write sequences (`SELECT MAX(...)` + `INSERT`/`UPDATE`/`DELETE`) in SQLite or DuckDB MUST execute inside an explicit transaction (`tx = conn.transaction()?`, `tx.commit()?`).
5. **Ergonomic Contextual Database Results:**
   - Use `.db_context(...)` / `.duck_context(...)` rather than `.map_err(|e| format!(...))` boilerplate.
6. **Daemon Command Registration Invariant (Spec 12 §7.2):**
   - `oneship-daemon` owns all processes and state; new domain commands are daemon commands, never `#[tauri::command]`.
   - Every daemon command MUST be registered in `api::API_COMMANDS` and `api::dispatch` (`src-tauri/src/api.rs`) and mirrored in `DaemonCommands` (`src/types/daemonApi.ts`). Frontend calls go exclusively through `daemonCall`.
   - Argument, result, and error types derive `ts_rs::TS` with `#[ts(export)]` and are generated into `src/types/generated/` via `./scripts/run_dev.sh export-bindings`.
   - The desktop app's `tauri::generate_handler![...]` (`src-tauri/src/lib.rs`) holds only `get_build_id_cmd` and `get_daemon_port_cmd`.
7. **Comprehensive Public Rustdoc Coverage:**
   - All public structs, enums, traits, and functions in core backend modules MUST include `///` Rustdoc comments.

---

## 7. Frontend Architecture, Lifecycle & State Management (Vue & TypeScript)

1. **Mandatory Event Listener Lifecycle Teardown:**
   - Every listener (`onDaemonEvent`, `listen`, `window.addEventListener`, `setInterval`) registered in stores, composables, or components MUST retain its cleanup handle and invoke it during teardown (`$reset` / `onUnmounted`).
2. **Domain-Bounded Pinia Stores:**
   - Keep Pinia stores single-responsibility and bounded (≤ 500 lines).
3. **Strict Zero `any` TypeScript Policy:**
   - `any` is strictly forbidden in application code, stores, IPC wrappers, and props.
4. **Composable Bridge Layer for External Transports:**
   - UI components MUST NOT execute raw `postMessage` or raw WebSockets directly; encapsulate them in typed composables.
5. **Stable Virtual DOM Keys (`v-for`):**
   - Never use array indices (`:key="idx"`, `:key="index"`) on dynamic or mutable lists; always use unique IDs (`:key="item.id"`).

---

## 8. Configuration-Driven Architecture & Dynamic Theming

1. **Dynamic Configuration Schema (Zero Magic Numbers):**
   - Operational parameters (timeouts, buffer limits, polling intervals, retry ceilings, capacities) must NEVER be hardcoded constants. Register them in `get_all_plugin_schemas()` in `src-tauri/src/config.rs`, expose them in `SettingsModal.vue`, and resolve them dynamically.
   - Performance, memory, and runtime stability limits MUST use `scope: "global_only"`; use project scope only when behavior genuinely differs per project.
2. **Dynamic Semantic Theme Variables (Zero Hardcoded Color Palettes):**
   - Hardcoding Tailwind palette classes (`border-emerald-500`, `bg-rose-950`, `text-amber-300`, `bg-black/*`, `bg-[#...]`) is strictly prohibited.
   - Use semantic CSS variables (`--status-ok-color`, `--status-warn-color`, `--status-error-color`, `--bg-surface`, `--border-color`, etc.) parsed from Omarchy's active desktop theme (`colors.toml`).

---

## 9. Universal Logging & Diagnostic Observability

1. **Zero Silent Code Invariant:** No function, DB query, state transition, or IPC command may execute silently without diagnostic traceability in `oneship.log` / `system_logs`.
2. **Level Taxonomy:**
   - `DEBUG`: Entry of public CRUD/SQL/IPC operations with key IDs/paths/counts (never unbounded payloads). Dropped before I/O when `system.log_level != "DEBUG"`.
   - `INFO`: Discrete business and lifecycle milestones.
   - `WARN`: Recoverable anomalies, poison recovery, quota thresholds (≥ 80%), orphan cleanup, scoping rejections.
   - `ERROR`: Operational/domain failures, spawn failures, aborted transactions.
3. **Non-Blocking Persistence:** `crate::logger::log` dispatches through a bounded non-blocking channel (`sync_channel::<QueuedLog>(4096)`) and commits in micro-batches.
4. **Cursor-Based Paging:** Log viewers paginate via monotonic IDs (`before_id` / `after_id`), enforce bounded sliding windows (≤ 1000 entries), and use `:key="log.id"`.

---

## 10. Human-Scale Bounds (Single-User Desktop, NOT Multi-Tenant SaaS)

- ONE-SHIP is a local desktop workbench for one human maker. NEVER apply aggressive multi-tenant SaaS timeouts (1–2s) to user interactions, builds, hardware/USB probes, or agent workflows.
- Bounded loops and timeouts exist strictly to prevent permanent hangs and OOM growth, NEVER to abort legitimate slow work early. Human-facing waits and approval gates must use generous human-scale defaults (≥ 60,000 ms).
