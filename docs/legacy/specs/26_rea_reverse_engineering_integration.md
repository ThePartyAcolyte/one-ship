> [!WARNING]
> **LEGACY SPECIFICATION:** Superseded by [06-hardware-and-cad/04-rea-reverse-engineering-integration.md](../../specs/06-hardware-and-cad/04-rea-reverse-engineering-integration.md). Preserved for historical provenance only.

# Spec 26: REA Reverse Engineering Engine & Multi-Project FIFO Gateway

**Document Version:** 0.2.6  
**Domain:** Reverse Engineering Orchestration, Binary Decompilation Gateway, Multi-Project Queue & Visual Explorer  
**Status:** Phases 1, 2 & 3 Implemented (Build 0.2.6)  
**Maintainer:** Bosmang (Lead Systems Architect & Engineer)

---

## 1. Overview & Architectural Motivation

Integrating **REA (`morluto/rea` / `rea-agents`)** into **ONE-SHIP** equips any coding agent across any supported runner (`agy`, `claude`, `opencode`, `gemini`, `cursor`, `pi`) with over 100 reverse-engineering and runtime inspection tools (Ghidra headless decompilation, ELF/PE/Mach-O symbol analysis, Electron ASAR unpacking, Android JADX decompilation, SquashFS/Binwalk firmware extraction, and Chrome DevTools Protocol tracing).

However, running `rea` in its default unmanaged `stdio` configuration across a multi-project, multi-agent orchestrator introduces critical hazards:
1. **Duplicate Idle Footprint:** Spawning one Node.js `rea` process per open agent terminal wastes hundreds of megabytes of RAM when no reverse engineering is taking place.
2. **Cross-Project Evidence Contamination:** By default, `rea` stores analysis caches in `~/.cache` (`XDG_CACHE_HOME`) and cryptographic `Evidence` bundles in `~/.local/share` (`XDG_DATA_HOME`), violating ONE-SHIP Rule #11 (*Strict Prohibition of Permissive Fallbacks & Mandatory Exact Scoping*).
3. **Concurrent Heavy Decompiler Saturation:** If two agents in parallel projects simultaneously trigger Ghidra headless (`analyzeHeadless`), multiple 2+ GiB JVM instances will saturate CPU and memory.
4. **Background Workflow Continuity:** Developers working across multiple projects in parallel require background reverse-engineering jobs to continue uninterrupted when switching active projects in the UI.

---

## 2. Topology: `/mcp/rea` Gateway, FIFO Queue & Supervised Engine

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

---

### 2.1 Contracts (as implemented)
- **Engine binary:** only the installed `rea` binary runs the engine; when it is missing the start fails with `ReaError::EngineNotInstalled` (there is no runtime `npx -y rea-agents@latest` download). `check_rea_toolchain` reports the host toolchain (`rea-agents`, Ghidra, radare2, binutils, binwalk, jadx, squashfs-tools) with distro-aware `install_hint` and `auto_installable` flags. The RE Lab toolchain modal installs supported native packages through `install_missing_tools` (`pkexec` + detected Linux package manager); non-package-manager tools (`rea-agents`, etc.) expose their command for copying.
- **Engine port:** `DEFAULT_REA_BASE_PORT (8790) + project id`, unique per registered project; an unregistered project is an error (the former `id % 200` mapping collided and an unregistered path fell back to a path hash).
- **Gateway origin:** an attested agent is scoped to its own project. Only a caller that is not a managed agent (`CallerError::UnmanagedCaller`) may scope itself with `?project_hex=` (a registered project); any other identity failure, or malformed hex, rejects the request. The plain-text `?project_path=` parameter is not accepted.
- **Commands:** every project command (`get_rea_engine_status`, `start_rea_engine`, `stop_rea_engine`, `toggle_project_rea_plugin`, `list_rea_targets`, `get_rea_visual_views`) requires a registered project and returns a typed `ReaError`. `toggle_project_rea_plugin` requires a valid `rula.toml` (an invalid or missing file is never replaced by the plugin table alone). `rea.ghidra_install_dir` must not be empty.
- **Targets Directory (`[plugins.rea].targets_dir`):** cascade setting (default `"targets"`), resolved relative to `project_path` with parent-traversal (`..`) rejection; `ensure_project_rea_directories`, `build_rea_engine_status`, and `list_rea_targets` all operate on this configured subdirectory.
- **Scans:** directory sizes, target hashing (files up to 256 MB; larger ones report `large-file-deferred`), magic classification and view files report I/O and JSON errors instead of reading them as empty or zero.

## 3. Lifecycle Guarantees

1. **Zero Idle Footprint (`0 MB RAM`):**
   - No `rea_mcp` process is launched at ONE-SHIP startup.
   - A project's `rea_mcp` engine is spawned lazily on the first MCP request to `/mcp/rea` or when explicitly started by the user.
2. **Non-Disruptive Multi-Project Parallelism:**
   - Switching the active workspace in the ONE-SHIP UI **never** terminates or interrupts a running `rea_mcp` engine or queued decompilation job in another project.
   - Concurrent `tools/call` requests across projects are serialized through the global `heavy_job_semaphore` in FIFO order (`ReaQueuedJob`), preventing JVM memory spikes while keeping background agent turns alive.
3. **Activity-Based Idle Reaper (`spawn_rea_idle_reaper_loop`):**
   - A background loop inspects active `rea_mcp` services every 15 seconds.
   - A project's engine is stopped **only** when `in_flight_requests_count == 0` AND `queued_requests_count == 0` AND `now_ms - last_activity_epoch_ms >= idle_timeout_seconds * 1000`.
   - Termination dispatches `kill(-pgid, SIGKILL)` via `ProcessSupervisor::stop_process`, reclaiming 100% of Node.js and child JVM memory while preserving all decompiled caches and evidence bundles on disk inside `<project_path>/.one-ship/rea/`.

---

## 4. Dual Lifecycle Support: Dedicated RE Projects & Auxiliary Mode

1. **Dedicated Reverse Engineering Projects (Greenfield / reMarkable 2 Archetype):**
   - In `AddProjectModal.vue`, enabling **Reverse Engineering Lab (REA)** invokes `create_greenfield_project` with `enable_rea: true`.
   - The backend deterministically scaffolds `<project_path>/targets/` (with `targets/README.md`), `<project_path>/.one-ship/rea/views/`, injects `oneship-rea` into `.agents/skills/`, registers `/mcp/rea` in `.mcp.json` and `opencode.json`, and configures `[plugins.rea] enabled = true` along with the `reverser` specialist persona in `rula.toml`.
2. **Auxiliary Reverse Engineering Mode (Any Existing Project):**
   - Any tracked project can enable or disable the REA plugin on-demand via the **Project Features & Subsystems** dashboard in `ProjectOverviewTab.vue` or through the Project Configuration Hub (`toggle_project_rea_plugin`).
   - Toggling REA updates `rula.toml` (`[plugins.rea] enabled = true/false`), injects/removes `oneship-rea`, and re-hydrates project records via `coreStore.reloadProjects()`.
   - **Contextual Tab Visibility:** The **RE Lab** tab in `ProjectWorkspace.vue` is strictly rendered when `activeProject?.has_rea_enabled` is true. If a project without REA is selected or if REA is disabled while on the tab, the workspace automatically falls back to `'overview'`.

---

## 5. Visual Reverse Engineering Explorer (`ProjectReaTab.vue`)

To make reverse engineering accessible without manual Ghidra GUI navigation, agents following `templates/skills/oneship-rea/SKILL.md` persist structured visual lenses in `<project_path>/.one-ship/rea/views/` (loaded via `get_rea_visual_views` and rendered in `ProjectReaTab.vue`):

1. **Architecture & Call Flow (`architecture_map.json`):**
   - Interactive subsystem breakdown, entrypoints, symbol addresses, and caller/callee `xrefs` flow cards.
2. **Decompiled Pseudocode (`decompiled_highlights.json`):**
   - Side-by-side readable C/C++/TypeScript pseudocode, plain-language functional explanations, hook points, and SHA-256 evidence verification badges.
3. **Protocols & Endpoints (`protocols_and_endpoints.json`):**
   - Catalog of discovered cloud REST/WebSocket endpoints, IPC sockets, authentication headers, and payload schemas for building custom self-hosted clouds or agentic bridges.
**View file contract (as implemented):** each file is a JSON object with one shape: `architecture_map.json` → `{ subsystems, call_edges }`, `decompiled_highlights.json` → `{ functions }`, `protocols_and_endpoints.json` → `{ items }` (absent lists are empty; bare arrays and the former `edges` key are rejected). Identifying fields are required non-empty strings — `subsystems[].name`, `call_edges[].from_symbol` / `to_symbol`, `functions[].symbol`, `items[].endpoint_or_key`; a missing one rejects the file with `ReaError::InvalidView` naming list, index and field. `offset_hex`, `subsystem`, `signature` and `category` are optional (`null` in the bundle, `—` in the UI); descriptive text fields default to empty; `id` defaults to a positional id. No placeholder values are invented (formerly `"0x0"`, `"sub_unknown"`, `"Core"`, `"caller"`/`"callee"`, `"protocol"`). The shape is documented for agents in the `oneship-rea` skill.

4. **Live Multi-Project Queue & Omarchy Toolchain Diagnostics:**
   - Displays real-time active/queued jobs across all projects (`get_rea_queue_snapshot`), `targets/` binary SHA-256 and magic format badges (`list_rea_targets`), and one-click copy of the `pkexec pacman -S --needed ...` command when host tools are missing.
