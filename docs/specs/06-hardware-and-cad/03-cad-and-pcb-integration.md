# CAD and PCB Integration (SolidCraft & PCBCraft)

## Purpose

Integrate **SolidCraft** (parametric 3D CAD) and **PCBCraft** (EDA schematics and PCB layout) as decoupled external toolchains managed by `oneship-daemon` (`kopeng::solidcraft`, `kopeng::pcbcraft`), replicating ONE-SHIP's project-scoped plugin architecture without forked UI rebranding (`LICENSE-BRAND` compliant).

## Scope

In scope:

- Division of responsibilities across ONE-SHIP UI, external native editors, AI agents, and deterministic compute engines.
- Licensing, sibling-repository branching, and resource-bounded local build policies.
- `[plugins.solidcraft]`, `[plugins.pcbcraft]`, and `[plugins.electronics]` SPICE extension configuration schemas.
- `kopeng::solidcraft` headless PNG preview rendering, read-only metrics inspection, and Hyprland editor launch.
- `kopeng::pcbcraft` native SVG schematic/board previews, BOM/netlist/DRC inspection, `/mcp/pcbcraft` gateway, live editor discovery (`LiveControl`), `save_project` persistence, and post-save snapshot review workflow.
- SPICE / EIS netlist bridge (`ngspice` / `xyce`) connecting PCBCraft schematics to circuit simulation.
- Upstream contribution roadmap (`tangency-labs/pcbcraft`) and Stage D/E sourcing and fabrication roadmaps (Mouser/AliExpress/RS Chile/Datasheets and PCBWay Partner API).

Out of scope:

- Physical benchtop instruments and microcontroller firmware workflows (see [Electronics Workstation and Instrumentation Engine](01-electronics-workstation-and-instrumentation.md)).
- General snapshot storage internals (see [Git Workspace and Branches](../03-projects/02-git-workspace-and-branches.md)).

## Model / Design

### 1. Division of Responsibilities (Human vs. Agent vs. Deterministic Engines)

| Domain | In-App ONE-SHIP UI | External Native Editor | Agent / MCP Scope | Deterministic Compute |
| :--- | :--- | :--- | :--- | :--- |
| **SolidCraft (CAD)** | Multi-view PNG preview (`iso`, `front`, `top`, `right`, section cuts) + physical metrics table | `/usr/bin/solidcraft` launched/focused via `hyprctl` | **No LLM CAD modeling.** Optional read-only inspection (`volume`, `bbox`, `parameters`) only | `solidcraft-cli snapshot` & `solidcraft-cli eval` |
| **PCBCraft (EDA/PCB)** | Native ONE-SHIP SVG schematic/board viewer + BOM, netlist, and DRC inspector | `pcbcraft` (`eframe`/`wgpu`) launched/focused via `hyprctl` | `/mcp/pcbcraft`: component search/selection, schematic authoring (`schematic.*`), netlist & DRC inspection (`pcb.check_design`). **No LLM copper routing** (`pcb.add_tracks`, `pcb.add_via` guarded by skill) | `pcbcraft-cli mcp` + ONE-SHIP SPICE bridge (`ngspice` / `xyce`) |

### 2. Licensing and Branching Policy

- **SolidCraft:** Consumes host-installed binaries (`solidcraft-bin`: `/usr/bin/solidcraft`, `/usr/bin/solidcraft-cli`) or user-configured paths. In compliance with SolidCraft's `LICENSE-BRAND`, ONE-SHIP does not redistribute forked binaries containing `solidcraft-ui-kit`, `solidcraft-look`, or `solidcraft-theme`.
- **PCBCraft:** Compiles from a clean sibling clone (`<sibling-repos>/pcbcraft`) tracking `upstream/main` (`MIT OR Apache-2.0` core). Agnostic improvements are developed on `fix/*` / `feat/*` branches for upstream Pull Requests rather than maintaining a permanently diverged rebranded fork.
- **Resource-Bounded Local Builds:** On memory-constrained developer hosts (15 GiB RAM / 16 threads), `pcbcraft` builds must cap Cargo parallelism (`-j 2`), disable ThinLTO (`CARGO_PROFILE_RELEASE_LTO=false`), limit codegen units (`CARGO_PROFILE_RELEASE_CODEGEN_UNITS=4`), and run inside a cgroup v2 memory scope (`MemoryHigh=8G`, `MemoryMax=10G`).

### 3. Daemon Subsystems

Both modules register commands in `api::API_COMMANDS` / `api::dispatch` and `DaemonCommands`, export TypeScript bindings via `ts-rs`, use `thiserror` domain error enums (`SolidcraftError`, `PcbcraftError`) implementing `api::ApiError`, and enforce strict project scoping with zero permissive fallbacks.

#### 3.1 `kopeng::solidcraft`

- **File Watcher (`notify`):** Monitors `<project>/<cad_dir>` for `.sc`, `.step`, `.stp`, and `.3mf` changes, emitting `solidcraft-file-changed` over `/api/events`. In parallel, `kopeng::documents` maintains a debounced recursive project watcher emitting `project-files-changed` so external CAD/agent creations appear in the Project File Explorer automatically.
- **Headless Creation & Inspection:**
  - `create_solidcraft_file` spawns `solidcraft-cli mcp --headless --allow-path <cad_dir>` (`new_design` + `document.units` + optional `sketch.create` + `save`) to create a valid `.sc` design from one of 7 templates (`blank_mm`, `blank_cm`, `blank_m`, `blank_in`, `blank_ft`, `xy_sketch_mm`, `xy_sketch_in`) and emits `solidcraft-file-changed`.
  - `render_solidcraft_preview` executes `solidcraft-cli snapshot --in <path> --out <cache_png> --view <view> --width <w> --height <h>` across 7 camera views (`iso`, `front`, `back`, `top`, `bottom`, `left`, `right`) with optional section plane, backed by `preview_cache.rs` and `hardwarePreviewStore.ts`.
  - `inspect_solidcraft_metrics` executes `solidcraft-cli eval --in <path>` (read-only), returning volume ($\text{mm}^3$), bounding box ($\text{mm}$), body count, and editable parameters.
  - `launch_project_solidcraft` spawns `/usr/bin/solidcraft <file>` on an empty Hyprland workspace (`desktop.rs`) or focuses an existing PID via `hyprctl`.
- **Deferred Interactive Viewport:** Client-side interactive WebGL 3D orbit viewport (`Three.js` + `solidcraft-cli mcp` `export` to `.stl`/`.obj`).

#### 3.2 `kopeng::pcbcraft`

- **File Watcher (`notify`):** Monitors `<project>/<pcb_dir>` for `project.json`, `sheets/*.json`, `boards/*`, and `.pc` changes, emitting `pcbcraft-file-changed` over `/api/events`.
- **Project Scaffolding, Inspection & Native SVG Rendering:**
  - `create_pcbcraft_project` scaffolds a validated `pcbcraft-project/16` (or `pcbcraft-project/17`) folder inside `<project>/<pcb_dir>/<target_name>`, verifies loading via `pcbcraft-cli mcp --project`, and emits `pcbcraft-file-changed`.
  - `inspect_pcbcraft_project` queries `pcbcraft-cli mcp --project <target>` (`inspect_project` + `execute("pcb.check_design")`) to return `PcbcraftProjectInspection` (sheets, boards, BOM/components with simulation attributes, nets, and DRC violations/status).
  - `render_pcbcraft_svg_preview` renders `inspect_project` schematic or board geometry in pure Rust into a bounded ONE-SHIP SVG preview (`PcbcraftSvgPreviewResult`) styled with semantic CSS variables (`schematic` or `board` mode).
  - `launch_project_pcbcraft` spawns or focuses `pcbcraft` on an empty Hyprland workspace via `desktop.rs`.
- **MCP Gateway (`/mcp/pcbcraft?project_hex=...`):**
  - Mounted in `src-tauri/src/mcp.rs` with strict caller attestation (`resolve_caller` / `?project_hex=`), registered globally in `~/.gemini/config/mcp_config.json` (`config.rs`) and per-project `.mcp.json` / `opencode.json` (`skill.rs`).
  - Manages a per-project stdio pool of `pcbcraft-cli mcp --project <resolved_pcb_target>` (or `--connect` when live editor is running).
  - **Platform Skill Guardrail (`templates/skills/oneship-pcbcraft/SKILL.md`):** Instructs agents to use `inspect_project`, `library.*`, `schematic.*`, and `pcb.check_design`, and explicitly prohibits LLM copper routing commands (`pcb.add_tracks`, `pcb.add_via`, `pcb.reshape_tracks`).

#### 3.3 Dynamic PCBCraft MCP Routing, `save_project` Persistence, and Review Workflow

1. **RAM Accumulation + Explicit `save_project`:**
   - `pcbcraft-mcp` (`tools.rs`, `backend.rs`, `live.rs`) exposes `save_project`:
     - In `Headless` (`--project`), atomic edits accumulate in `Session` RAM and `save_project` commits them to disk via `pcbcraft_storage::save` (or `encode_packed` for `.pc`).
     - In `Remote` (`--connect`), `save_project` invokes `file.save` on the open PCBCraft window.
2. **Dynamic `--connect` vs. `--project` Gateway Selection:**
   - When `pcbcraft` launches on a project (`PcbApp::with_initial_project`), `LiveControl` publishes an ephemeral `0600` discovery record under `~/.cache/one-ship/pcbcraft-live/` (removed on `Drop`).
   - `get_or_spawn_mcp_process` in `kopeng/pcbcraft.rs` checks for an active live endpoint matching `target_path` (verifying `/proc/<pid>`), spawning `pcbcraft-cli mcp --connect <port> --token-file <file>` when open or `--project <target>` when closed, and recycling the pooled process if editor state changes.
3. **Pre-Save Auto-Snapshot & Post-Save Review Banner:**
   - `/mcp/pcbcraft` captures a scoped pre-save snapshot (`domain = "pcbcraft"`, `asset_key = <target>`) right before `save_project` (and suppresses duplicate `notify` snapshots within `2500 ms`), while external user edits in PCBCraft trigger watcher snapshots.
   - `ProjectPcbTab.vue` surfaces a post-update action bar (**Open in PCBCraft** / **Revert Snapshot** / **Keep**) when a target updates on disk.

#### 3.4 SPICE / EIS Netlist Bridge

- `kopeng::pcbcraft` provides a pure-Rust netlist extractor converting `pcbcraft-project/17` schematics (respecting PCBCraft's `metadata.attributes.exclude_from_simulation` flag) into normalized SPICE `.cir` netlists (`extract_net_pin_map` / netlist bridge).
- `kopeng::electronics::spice` dispatches `.ac` (frequency sweep for EIS Bode & Nyquist $Z'(\omega)$ / $-Z''(\omega)$), `.tran`, `.noise`, and `.hb` (when `spice_engine == "xyce"`) in `<project>/.one-ship/electronics/sim/`, returning bounded numeric digests to agents and plot series to the UI.

### 4. Frontend Workspace Integration

- **`HydratedProject` (`src-tauri/src/hydration.rs`):** Exposes `has_solidcraft_enabled: bool` and `has_pcbcraft_enabled: bool`.
- **`ProjectWorkspace.vue`:** Conditionally renders contextual tabs:
  - **`CAD` (`ProjectCadTab.vue`):** File selector, view presets (`iso`, `front`, `top`, `right`), optional section plane input (`z=...`), zoom/pan PNG stage, read-only physical metrics panel, and `Open in SolidCraft` action.
  - **`PCB` (`ProjectPcbTab.vue`):** Sheet/Board mode toggle, native ONE-SHIP SVG pan/zoom canvas, BOM & Netlist table, DRC findings panel, `Run SPICE / EIS Sweep` shortcut (linking to [Electronics Workstation and Instrumentation Engine](01-electronics-workstation-and-instrumentation.md)), and `Open in PCBCraft` action.
- All UI controls adhere to [Design System Foundations](../07-ui/01-design-system-foundations.md) (`AppTooltip`, `AppSelect`, `AppInput`, semantic CSS variables, zero `any`, explicit `onUnmounted` event cleanup).

### 5. Tiger Style and Safety Invariants

1. **Bounded Memory:** Preview SVG string generation and PNG reads enforce explicit upper size limits (`max_preview_bytes` / `MAX_PREVIEW_PNG_BYTES` = 16 MiB, `MAX_SVG_BYTES` = 2 MiB) and bounded node counts (`max_svg_elements_count` / `MAX_SVG_ELEMENTS_COUNT` = 8,000; cache limits `MAX_PREVIEW_CACHE_ITEMS_COUNT` = 32, `MAX_INSPECT_CACHE_ITEMS_COUNT` = 24, `MAX_METRICS_CACHE_ITEMS_COUNT` = 32).
2. **Bounded Process Execution:** Every `solidcraft-cli`, `pcbcraft-cli`, `ngspice`, and `Xyce` subprocess enforces deterministic timeouts (`*_timeout_ms`) and kills child process trees on timeout or cancellation.
3. **Zero Blocking I/O in Async Contexts:** All `project.json` / `sheets/*.json` reads and subprocess orchestration use `tokio::fs` / `tokio::process` or `spawn_blocking`.
4. **Zero Silent Fallbacks:** Missing binaries return `BinaryNotFound`; missing or ambiguous `project_hex` returns `ProjectContextMissing` immediately.

### 6. Upstream Contribution Roadmap (`tangency-labs/pcbcraft`)

While ONE-SHIP stays fully functional out of the box via its native `pcbcraft-project/17` JSON SVG renderer and SPICE bridge, the following agnostic PRs are tracked for upstream collaboration:

1. **`pcbcraft-cli --help` / `-h`** (aligns with open issue `#319`): Low-friction starter PR.
2. **Pure-Rust Netlist / SPICE Export Command (`schematic.export_netlist`):** Leverages existing `exclude_from_simulation` component metadata without adding C/C++ dependencies or breaking `pcbcraft-web` WASM builds.
3. **Headless SVG / Snapshot Export (`pcbcraft-cli snapshot`):** Reuses `crates/engine` / `crates/app` scene primitives to render headless schematic/board SVGs for CI and external orchestrators.
4. **Routing Interoperability Research:** Evaluate Specctra `.dsn` / `.ses` export/import (for external `FreeRouting`) vs. native Rust `geo-index`-backed routing against upstream `#306` and `VISION.md` before writing code.

### 7. Stage D Roadmap — BOM Sourcing, Chile Availability, and Datasheets

Scheduled after **Stage B (PCBCraft)** and **Stage C (SPICE / EIS)**, extending `kopeng::electronics` (`sourcing`) directly from the normalized BOM extracted by `inspect_pcbcraft_project`:

1. **Official Distributor APIs & Regional Sourcing (Chile):**
   - **Mouser Electronics (`api.mouser.com` SearchAPI v1/v2):** Primary automated source using a user-configured `mouser_api_key` in global `[plugins.electronics]` settings. Queries live stock, volume price breaks, MPN/manufacturer metadata, shipping eligibility, and official `DataSheetUrl` PDF links.
   - **AliExpress (Chile `CL`):** Supports optional official **AliExpress Open Platform API** (`AppKey`/`AppSecret`, `ship_to_country=CL`, USD/CLP reference price) plus deterministic search deep-links per BOM row (`component value + package`). Direct headless HTML scraping is prohibited due to anti-bot / CAPTCHA gates.
   - **RS Components Chile (`cl.rsdelivers.com`):** Generates zero-friction deep-link search URLs per component MPN/value and supports optional local per-project price overrides (RS public catalog APIs require enterprise B2B accounts and web pages are WAF-protected).
   - **Optional Multi-Distributor Extension:** DigiKey API v4 / Nexar (Octopart) adapter interface for alternative stock comparison when Mouser shows zero stock.
2. **Local Datasheet Acquisition & Agent Readability:**
   - Daemon download worker fetches direct manufacturer/Mouser PDF datasheets into `<project>/docs/datasheets/<MPN>.pdf` (bounded size & timeout), automatically surfaced in the Project File Explorer (`project-files-changed`) and readable/indexable by project agents.
3. **UI & MCP Contracts:**
   - BOM sidebar actions in `ProjectPcbTab.vue`: `Quote BOM`, per-row distributor links (`Mouser`, `RS Chile`, `AliExpress`), cached price/stock badges with TTL indicator, and `Download Datasheet`.
   - Agent MCP tool `lookup_component_sourcing(mpn | query, country)` returning cached stock/pricing digests and downloaded datasheet paths.

### 8. Stage E Roadmap — Fabrication Packaging and PCBWay Partner API

Scheduled after **Stage D**, connecting deterministic `pcbcraft` manufacturing exports directly to **PCBWay Partner API** (`https://api-partner.pcbway.com`, `api-key` header) for streamlined quoting and ordering:

1. **Deterministic Fabrication Bundle (`pcb.export_fabrication`):** Triggers `pcbcraft`'s built-in `pcb.export_fabrication` command to package verified Gerber layers (`*-Top.gbr`, etc.), drill files (`*-PTH.drl`), pick-and-place CPL, and BOM into `<project>/<pcb_dir>/<target>/exports/<target>-fabrication.zip`.
2. **PCBWay Partner Quoting & Logistics (`kopeng::electronics::pcbway`):**
   - Derives board dimensions (`Length`, `width` in mm) and copper layer count from PCBCraft board setup/outline.
   - Calls `POST api/Pcb/PcbQuotation`, `POST api/SMT/SMTQuotation` (optional SMT assembly), `POST api/Address/GetCountry`/`GetState`/`GetCity`, and `POST api/Pcb/GetFreightByOrder` (freight to Chile / configured destination).
   - Fallback when no Partner `api-key` is configured: exports the ready-to-upload `-fabrication.zip` and opens PCBWay's standard quote workflow.
3. **Human-Gated Order Execution (Zero Unattended Financial Actions):**
   - Any state-changing order endpoint (`POST api/Pcb/PlaceOrder`, `POST api/Pcb/ConfirmOrder`) MUST block on an explicit UI confirmation / `ApprovalRequestModal` displaying itemized fabrication cost, freight cost, board revision, and `api/Account/QueryBalance` before transmission.
   - Order tracking uses `POST api/Pcb/CheckOrderDetails` and `POST api/Pcb/QueryOrderProcess`.

## Contracts

### SolidCraft Daemon Commands & Errors (`kopeng/solidcraft.rs`, `api.rs`, `daemonApi.ts`)

| Command | Arguments | Result |
| :--- | :--- | :--- |
| `get_project_solidcraft_status` | `project_path` | `SolidcraftStatus` (`project_path`, `is_enabled`, `cad_dir_path`, `cad_files_count`, `cad_files`, `app_available`, `cli_available`, `is_editor_running`) |
| `toggle_project_solidcraft_plugin` | `project_path`, `enabled` | `SolidcraftStatus` |
| `create_solidcraft_file` | `project_path`, `file_name`, `template_id?` | Created design result (7 templates: `blank_mm`, `blank_cm`, `blank_m`, `blank_in`, `blank_ft`, `xy_sketch_mm`, `xy_sketch_in`) |
| `launch_project_solidcraft` | `project_path`, `file_name?` | `SolidcraftStatus` |
| `render_solidcraft_preview` | `project_path`, `file_name`, `view`, `section?` | `SolidcraftPreviewResult` |
| `inspect_solidcraft_metrics` | `project_path`, `file_name` | `SolidcraftMetricsResult` |

- **`SolidcraftError`:** `PluginDisabled`, `InvalidSetting`, `BinaryNotFound`, `ProcessLifecycle`, `Io`, `Registry`, `Config`.
- **Event:** `solidcraft-file-changed` (`SolidcraftFileChangedPayload` in `events.rs`).

### PCBCraft Daemon Commands, Gateway & Errors (`kopeng/pcbcraft.rs`, `api.rs`, `mcp.rs`)

| Command | Arguments | Result |
| :--- | :--- | :--- |
| `get_project_pcbcraft_status` | `project_path` | `PcbcraftStatus` (`project_path`, `is_enabled`, `pcb_dir_path`, `pcb_targets_count`, `pcb_targets`, `app_available`, `cli_available`, `is_editor_running`) |
| `toggle_project_pcbcraft_plugin` | `project_path`, `enabled` | `PcbcraftStatus` (syncs `.mcp.json` / `opencode.json` & `oneship-pcbcraft` skill) |
| `create_pcbcraft_project` | `project_path`, `target_name` | Created target result |
| `launch_project_pcbcraft` | `project_path`, `target_name?` | `PcbcraftStatus` |
| `inspect_pcbcraft_project` | `project_path`, `target_name?` | `PcbcraftProjectInspection` |
| `render_pcbcraft_svg_preview` | `project_path`, `target_name?`, `sheet_id?`, `mode?` | `PcbcraftSvgPreviewResult` |

- **MCP Route:** `POST /mcp/pcbcraft?project_hex=...` (`pcbcraft_mcp_gateway_handler`).
- **`PcbcraftError`:** `ProjectNotFound`, `ProjectContextMissing`, `PluginDisabled`, `InvalidSetting`, `BinaryNotFound`, `ProcessLifecycle`, `McpTransport`, `Io`, `Task`, `Registry`, `Config`, `Skill`.
- **Event:** `pcbcraft-file-changed` (`PcbcraftFileChangedPayload` in `events.rs`).

### Configuration Keys (`src-tauri/src/config.rs` & `rula.toml`)

#### `[plugins.solidcraft]`

| Key | Type | Scope | Default | Description |
| :--- | :--- | :--- | :--- | :--- |
| `enabled` | `boolean` | `project_only` | `false` | Enables SolidCraft CAD preview and launcher |
| `app_binary_path` | `string` | `cascade` | `"solidcraft"` | Executable name or absolute path to SolidCraft desktop editor |
| `cli_binary_path` | `string` | `cascade` | `"solidcraft-cli"` | Executable name or absolute path to `solidcraft-cli` |
| `cad_dir` | `string` | `cascade` | `"cad"` | Project-relative directory monitored for `.sc`, `.step`, `.stp`, `.3mf` |
| `snapshot_timeout_ms` | `number` | `cascade` | `15000` | Max execution window for `solidcraft-cli snapshot` / `eval` |
| `snapshot_width_px` | `number` | `cascade` | `1280` | Preview image width in pixels |
| `snapshot_height_px` | `number` | `cascade` | `800` | Preview image height in pixels |
| `auto_snapshot_on_edit` | `boolean` | `cascade` | `true` | Auto-creates scoped Git snapshot on CAD file changes |
| `max_snapshots_per_asset_count` | `number` | `cascade` | `20` | Max CAD snapshots retained per file |

#### `[plugins.pcbcraft]`

| Key | Type | Scope | Default | Description |
| :--- | :--- | :--- | :--- | :--- |
| `enabled` | `boolean` | `project_only` | `false` | Enables PCBCraft EDA preview, launcher, and `/mcp/pcbcraft` |
| `app_binary_path` | `string` | `cascade` | `"pcbcraft"` | Executable name or absolute path to PCBCraft desktop editor |
| `cli_binary_path` | `string` | `cascade` | `"pcbcraft-cli"` | Executable name or absolute path to `pcbcraft-cli` |
| `pcb_dir` | `string` | `cascade` | `"pcb"` | Project-relative directory for `pcbcraft-project/17` folders and `.pc` archives |
| `mcp_request_timeout_ms` | `number` | `cascade` | `30000` | Max wait for a single `pcbcraft-cli mcp` JSON-RPC response |
| `auto_snapshot_on_edit` | `boolean` | `cascade` | `true` | Auto-creates scoped Git snapshot when PCBCraft saves/modifies PCB files |
| `max_snapshots_per_asset_count` | `number` | `cascade` | `20` | Max PCB snapshots retained per board target |

#### `[plugins.electronics]` SPICE Extension (Planned)

| Key | Type | Scope | Default | Description |
| :--- | :--- | :--- | :--- | :--- |
| `spice_engine` | `select` | `cascade` | `"ngspice"` | Headless SPICE solver (`"ngspice"` or `"xyce"`) |
| `xyce_binary_path` | `string` | `cascade` | `"/usr/bin/Xyce"` | Path to the Xyce executable when selected |

## Implementation Status

Verified against `src-tauri/src/` and `src/`:

| Component | Status | Verification Notes |
| :--- | :--- | :--- |
| SolidCraft backend (`kopeng/solidcraft.rs`), 6 daemon commands, `solidcraft-file-changed` | Implemented | Registered in `api.rs`, `events.rs`, `config.rs`, `hydration.rs` |
| SolidCraft CAD UI (`ProjectCadTab.vue`, `hardwarePreviewStore.ts`) | Implemented | Present in `src/components/workspace/ProjectCadTab.vue` |
| Client-side Three.js 3D orbit viewport | Roadmap | Tracked as deferred (`STL`/`OBJ` export) |
| PCBCraft backend (`kopeng/pcbcraft.rs`), 6 daemon commands, `/mcp/pcbcraft`, `oneship-pcbcraft` skill | Implemented | Registered in `api.rs`, `mcp.rs`, `events.rs`, `config.rs`, `hydration.rs`, `templates/skills/oneship-pcbcraft/` |
| PCBCraft UI (`ProjectPcbTab.vue`), `save_project` pre-save snapshot & review workflow | Implemented | Present in `src/components/workspace/ProjectPcbTab.vue` and `kopeng/pcbcraft.rs` |
| Pure-Rust netlist helper (`extract_net_pin_map` in `kopeng/pcbcraft.rs`) | Implemented | Present in `kopeng/pcbcraft.rs` |
| SPICE / EIS execution bridge (`kopeng::electronics::spice`, `spice_engine`, `xyce_binary_path`) | Roadmap | Not yet present in `src-tauri/src/config.rs` or `kopeng/electronics/` |
| Stage D (BOM sourcing: Mouser, AliExpress, RS Chile, datasheets) | Roadmap | Not yet implemented |
| Stage E (Fabrication packaging & PCBWay Partner API) | Roadmap | Not yet implemented |

## Related Specs

- [Electronics Workstation and Instrumentation Engine](01-electronics-workstation-and-instrumentation.md)
- [One-Ship Designer Integration](02-oneship-designer-integration.md)
- [Plugin Architecture](../01-architecture/01-plugin-architecture.md)
- [Configuration Schema and Cascade](../01-architecture/02-configuration-schema-and-cascade.md)
- [Git Workspace and Branches](../03-projects/02-git-workspace-and-branches.md)
- [Design System Foundations](../07-ui/01-design-system-foundations.md)

Supersedes legacy: `33_cad_and_pcb_integration.md`
