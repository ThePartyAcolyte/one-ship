# REA Reverse Engineering Engine and Multi-Project Gateway

## Purpose

Integrate **REA (`morluto/rea` / `rea-agents`)** into ONE-SHIP to equip coding agents across all supported runners (`agy`, `claude`, `opencode`, `gemini`, `cursor`, `pi`) with 100+ reverse-engineering and runtime inspection tools while preventing idle memory waste, cross-project evidence contamination, heavy JVM saturation, and foreground project-switching disruption.

## Scope

In scope:

- `/mcp/rea` Axum HTTP gateway, global FIFO heavy-job semaphore, and per-project supervised `rea_mcp` HTTP engine (`kopeng::rea`).
- Strict project origin resolution, scoped `XDG_*` storage, target binary scanning, and installed-binary / toolchain management (`check_rea_toolchain`).
- Lazy on-demand startup, non-disruptive multi-project parallelism, and the activity-based idle reaper (`spawn_rea_idle_reaper_loop`).
- Dedicated greenfield RE projects and auxiliary on-demand RE Lab enablement.
- Visual Reverse Engineering Explorer (`ProjectReaTab.vue`) and strict view-file JSON schemas (`architecture_map.json`, `decompiled_highlights.json`, `protocols_and_endpoints.json`).

Out of scope:

- Internal implementation of `rea-agents`, Ghidra, radare2, JADX, or binwalk.
- General `ProcessSupervisor` reconciliation and global configuration cascade mechanics (see [Project Registry, Scripts & Service Orchestration](../03-projects/01-project-registry-and-service-orchestration.md) and [Configuration Schema and Cascade](../01-architecture/02-configuration-schema-and-cascade.md)).

## Model / Design

### 1. Architectural Motivation

Running `rea` in its default unmanaged `stdio` configuration across a multi-project, multi-agent orchestrator introduces four hazards:

1. **Duplicate Idle Footprint:** Spawning one Node.js `rea` process per open agent terminal wastes hundreds of megabytes of RAM when no reverse engineering is taking place.
2. **Cross-Project Evidence Contamination:** By default, `rea` stores analysis caches in `~/.cache` (`XDG_CACHE_HOME`) and cryptographic `Evidence` bundles in `~/.local/share` (`XDG_DATA_HOME`), violating ONE-SHIP's strict project scoping invariant.
3. **Concurrent Heavy Decompiler Saturation:** If two agents in parallel projects simultaneously trigger Ghidra headless (`analyzeHeadless`), multiple 2+ GiB JVM instances saturate CPU and memory.
4. **Background Workflow Continuity:** Developers working across multiple projects in parallel require background reverse-engineering jobs to continue uninterrupted when switching active projects in the UI.

### 2. Gateway Topology and Process Isolation

```text
+--------------------------------------------------------------------------------------------------+
|                        MULTI-PROJECT AGENT RUNNERS (Claude / OpenCode / Agy)                     |
+--------------------------------------------------------------------------------------------------+
           |                                                                |
           | POST http://localhost:8765/mcp/rea?project_hex=<hex>           |
           v                                                                v
+--------------------------------------------------------------------------------------------------+
| ONE-SHIP Axum Gateway (`src-tauri/src/kopeng/rea.rs` -> `rea_mcp_gateway_handler`)               |
|   1. Strict Origin Resolution (`CallerContext` or verified `project_hex` parameter)              |
|   2. Global Heavy-Job FIFO Queue (`tokio::sync::Semaphore`, `max_concurrent_heavy_jobs_count`)   |
|   3. On-Demand Auto-Wake (`ensure_rea_engine_running`)                                           |
+--------------------------------------------------------------------------------------------------+
           |
           | Forwards JSON-RPC 2.0 to `http://127.0.0.1:<project_rea_port>/mcp`
           v
+--------------------------------------------------------------------------------------------------+
| Supervised Per-Project REA HTTP Engine (`${project_path}:svc:rea_mcp`)                           |
|   - Command: `rea mcp --http --host 127.0.0.1 --port <project_rea_port>`                         |
|   - Process Group Isolation: `PR_SET_PDEATHSIG` + `setpgid` + `ONESHIP_RUN_ID`                   |
|   - Scoped Storage Environment:                                                                  |
|       XDG_DATA_HOME   = <project_path>/.one-ship/rea/data                                        |
|       XDG_CACHE_HOME  = <project_path>/.one-ship/rea/cache                                       |
|       XDG_CONFIG_HOME = <project_path>/.one-ship/rea/config                                      |
|       TMPDIR          = <project_path>/.one-ship/rea/tmp                                         |
|       GHIDRA_INSTALL_DIR = /opt/ghidra (configurable)                                            |
+--------------------------------------------------------------------------------------------------+
```

### 3. Lifecycle Guarantees

1. **Zero Idle Footprint (`0 MB RAM`):**
   - No `rea_mcp` process launches at ONE-SHIP startup.
   - A project's `rea_mcp` engine spawns lazily on the first MCP request to `/mcp/rea` or when explicitly started by the user (`start_rea_engine`), polling local TCP readiness (`REA_ENGINE_BOOT_MAX_ATTEMPTS_COUNT` = 60, `REA_ENGINE_BOOT_POLL_STEP_MS` = 250 ms).
2. **Non-Disruptive Multi-Project Parallelism:**
   - Switching the active workspace in the ONE-SHIP UI **never** terminates or interrupts a running `rea_mcp` engine or queued decompilation job in another project.
   - Concurrent `tools/call` requests across projects serialize through the global `heavy_job_semaphore` in FIFO order (`ReaQueuedJob`, bounded ring buffer `MAX_REA_QUEUE_HISTORY_COUNT` = 200), preventing JVM memory spikes while keeping background agent turns alive.
3. **Activity-Based Idle Reaper (`spawn_rea_idle_reaper_loop`):**
   - A background loop inspects active `rea_mcp` services every 15 seconds (`REA_IDLE_REAPER_INTERVAL_MS` = 15,000 ms).
   - A project's engine stops **only** when `in_flight_requests_count == 0` AND `queued_requests_count == 0` AND `now_ms - last_activity_epoch_ms >= idle_timeout_seconds * 1000`.
   - Termination dispatches `kill(-pgid, SIGKILL)` via `ProcessSupervisor::stop_process`, reclaiming 100% of Node.js and child JVM memory while preserving all decompiled caches and evidence bundles under `<project_path>/.one-ship/rea/`.

### 4. Dual Lifecycle Support: Dedicated RE Projects and Auxiliary Mode

1. **Dedicated Reverse Engineering Projects (Greenfield / reMarkable 2 Archetype):**
   - In `AddProjectModal.vue`, enabling **Reverse Engineering Lab (REA)** invokes `create_greenfield_project` with `enable_rea: true`.
   - The backend deterministically scaffolds `<project_path>/targets/` (with `targets/README.md`), `<project_path>/.one-ship/rea/views/`, injects `oneship-rea` into `.agents/skills/`, registers `/mcp/rea` in `.mcp.json` and `opencode.json`, and configures `[plugins.rea] enabled = true` along with the `reverser` specialist persona in `rula.toml`.
2. **Auxiliary Reverse Engineering Mode (Any Existing Project):**
   - Any tracked project can enable or disable the REA plugin on demand via the **Project Features & Subsystems** dashboard in `ProjectOverviewTab.vue` or through the Project Configuration Hub (`toggle_project_rea_plugin`).
   - Toggling REA updates `rula.toml` (`[plugins.rea] enabled = true/false`), injects/removes `oneship-rea`, and re-hydrates project records via `coreStore.reloadProjects()`.
   - **Contextual Tab Visibility:** The **RE Lab** tab in `ProjectWorkspace.vue` is strictly rendered when `activeProject?.has_rea_enabled` is true. If a project without REA is selected or if REA is disabled while on the tab, the workspace automatically falls back to `'overview'`.

### 5. Visual Reverse Engineering Explorer (`ProjectReaTab.vue`)

To make reverse engineering accessible without manual Ghidra GUI navigation, agents following `templates/skills/oneship-rea/SKILL.md` persist structured visual lenses in `<project_path>/.one-ship/rea/views/` (loaded via `get_rea_visual_views` and rendered in `ProjectReaTab.vue`):

1. **Architecture & Call Flow (`architecture_map.json`):** Interactive subsystem breakdown, entrypoints, symbol addresses, and caller/callee `xrefs` flow cards.
2. **Decompiled Pseudocode (`decompiled_highlights.json`):** Side-by-side readable C/C++/TypeScript pseudocode, plain-language functional explanations, hook points, and SHA-256 evidence verification badges.
3. **Protocols & Endpoints (`protocols_and_endpoints.json`):** Catalog of discovered cloud REST/WebSocket endpoints, IPC sockets, authentication headers, and payload schemas for building custom self-hosted clouds or agentic bridges.
4. **Live Multi-Project Queue & Toolchain Diagnostics:** Displays real-time active/queued jobs across all projects (`get_rea_queue_snapshot`), `targets/` binary SHA-256 and magic format badges (`list_rea_targets`), and one-click install or copy of the `pkexec pacman -S --needed ...` command when host tools are missing.

## Contracts

### Engine, Port, Origin, and Scanning Contracts

- **Engine binary:** Only the installed `rea` binary runs the engine; when missing, startup fails with `ReaError::EngineNotInstalled` (there is no runtime `npx -y rea-agents@latest` download). `check_rea_toolchain` probes `rea-agents`, Ghidra, `radare2`, `binutils`, `binwalk`, `jadx`, and `squashfs-tools` with distro-aware `install_hint` and `auto_installable` flags. Native packages install through `install_missing_tools` (`pkexec` + detected Linux package manager); non-package-manager tools (`rea-agents`, etc.) expose their command for copying.
- **Engine port:** `DEFAULT_REA_BASE_PORT (8790) + project id`, unique per registered project. An unregistered project is an error (replacing earlier `id % 200` collisions and unregistered path-hash fallbacks).
- **Gateway origin (`POST /mcp/rea`):** An attested agent is scoped to its own project. Only a caller that is not a managed agent (`CallerError::UnmanagedCaller`) may scope itself with `?project_hex=` (targeting a registered project); any other identity failure or malformed hex rejects the request. Plain-text `?project_path=` is rejected.
- **Project validation & `rula.toml`:** Every project command (`get_rea_engine_status`, `start_rea_engine`, `stop_rea_engine`, `toggle_project_rea_plugin`, `list_rea_targets`, `get_rea_visual_views`) requires a registered project and returns a typed `ReaError`. `toggle_project_rea_plugin` requires a valid `rula.toml` (invalid or missing files are never overwritten with the plugin table alone). `rea.ghidra_install_dir` must not be empty.
- **Targets Directory (`[plugins.rea].targets_dir`):** Cascade setting (default `"targets"`), resolved relative to `project_path` with parent-traversal (`..`) rejection; `ensure_project_rea_directories`, `build_rea_engine_status`, and `list_rea_targets` all operate on this configured subdirectory.
- **Scans & Hashing:** Directory sizes, target SHA-256 hashing (files up to `256 MB` / `MAX_TARGET_HASH_BYTES`; larger files report `large-file-deferred`), magic classification, and view file reads propagate I/O and JSON errors instead of returning empty or zero values.

### View File JSON Contract (`<project_path>/.one-ship/rea/views/`)

Each view file must be a JSON object with one exact shape (bare arrays and the former `edges` key are rejected):

- `architecture_map.json` $\rightarrow$ `{ "subsystems": [...], "call_edges": [...] }`
- `decompiled_highlights.json` $\rightarrow$ `{ "functions": [...] }`
- `protocols_and_endpoints.json` $\rightarrow$ `{ "items": [...] }`

Rules:

- Absent lists default to empty.
- Identifying fields are required non-empty strings: `subsystems[].name`, `call_edges[].from_symbol`, `call_edges[].to_symbol`, `functions[].symbol`, `items[].endpoint_or_key`. Missing any required field rejects the file with `ReaError::InvalidView` naming list, index, and field.
- `offset_hex`, `subsystem`, `signature`, and `category` are optional (`null` in the bundle, `—` in the UI); descriptive text fields default to empty; `id` defaults to a positional ID. No placeholder values are invented (never `"0x0"`, `"sub_unknown"`, `"Core"`, `"caller"`/`"callee"`, or `"protocol"`).

### Daemon Commands (`kopeng/rea.rs`, `api.rs`, `daemonApi.ts`)

| Command | Arguments | Result |
| :--- | :--- | :--- |
| `check_rea_toolchain` | `project_path?` | `Vec<ReaToolStatus>` |
| `get_rea_engine_status` | `project_path` | `ReaEngineStatus` |
| `get_rea_queue_snapshot` | — | `Vec<ReaQueuedJob>` |
| `start_rea_engine` | `project_path` | `ReaEngineStatus` |
| `stop_rea_engine` | `project_path` | `ReaEngineStatus` |
| `toggle_project_rea_plugin` | `project_path`, `enabled` | `ReaEngineStatus` |
| `list_rea_targets` | `project_path` | `Vec<ReaTargetArtifact>` |
| `get_rea_visual_views` | `project_path` | `ReaVisualViewsBundle` |

### Events and Error Hierarchy

- **Daemon SSE Event:** `rea-queue-updated` (`ReaQueueEventPayload` containing `jobs: Vec<ReaQueuedJob>` in `events.rs`).
- **`ReaError` (`src-tauri/src/kopeng/rea.rs`):** `ProjectNotFound`, `ProjectContextMissing`, `Io`, `EngineLifecycle`, `UpstreamTransport`, `EngineNotInstalled`, `InvalidSetting`, `InvalidView`, `Task`, `Registry`, `Config`, `Skill`.

### Configuration Keys (`[plugins.rea]` in `src-tauri/src/config.rs`)

| Key | Type | Scope | Default | Description |
| :--- | :--- | :--- | :--- | :--- |
| `enabled` | `boolean` | `project_only` | `false` | Enables REA MCP integration, `.one-ship/rea` storage, and RE Lab workspace |
| `max_concurrent_heavy_jobs_count` | `number` | `global_only` | `1` | Max simultaneous heavy decompilation jobs (e.g., Ghidra headless) across all projects |
| `idle_timeout_seconds` | `number` | `cascade` | `600` | Inactivity seconds with 0 active/queued requests before stopping `rea_mcp` |
| `ghidra_install_dir` | `string` | `cascade` | `"/opt/ghidra"` | Path to local Ghidra installation containing `support/analyzeHeadless` |
| `max_jvm_heap_mbytes` | `number` | `cascade` | `2048` | Max JVM heap (`-Xmx`) in MB for headless Ghidra workers |
| `targets_dir` | `string` | `cascade` | `"targets"` | Project-relative subdirectory scanned for target binaries and firmware images |

## Implementation Status

Verified against `src-tauri/src/` and `src/`:

| Component | Status | Verification Notes |
| :--- | :--- | :--- |
| Gateway (`/mcp/rea`), FIFO queue, port allocation, idle reaper, toolchain check (`kopeng/rea.rs`) | Implemented | Registered in `api.rs` (8 commands), `mcp.rs`, `events.rs`, `config.rs` |
| Strict target scanning (`targets_dir`), SHA-256 hashing, and JSON view validation | Implemented | `MAX_TARGET_HASH_BYTES`, `ReaError::InvalidView`, `list_rea_targets`, `get_rea_visual_views` in `kopeng/rea.rs` |
| Platform skill template (`templates/skills/oneship-rea`) | Implemented | Present on disk in `templates/skills/oneship-rea/` |
| RE Lab frontend explorer (`ProjectReaTab.vue`) and workspace gating | Implemented | Present in `src/components/workspace/ProjectReaTab.vue` and `ProjectWorkspace.vue` |

## Related Specs

- [Plugin Architecture](../01-architecture/01-plugin-architecture.md)
- [Configuration Schema and Cascade](../01-architecture/02-configuration-schema-and-cascade.md)
- [Platform Skill Injection](../01-architecture/03-platform-skill-injection.md)
- [Project Registry, Scripts & Service Orchestration](../03-projects/01-project-registry-and-service-orchestration.md)
- [Design System Foundations](../07-ui/01-design-system-foundations.md)

Supersedes legacy: `26_rea_reverse_engineering_integration.md`
