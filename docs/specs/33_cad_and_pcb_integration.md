# Spec 33: CAD (`SolidCraft`) & EDA/PCB (`PCBCraft`) Integration

**Document Version:** 0.2.0  
**Domain:** Parametric 3D CAD Preview, EDA/PCB Authoring & MCP Gateway, Mixed-Signal SPICE / EIS Simulation Bridge  
**Status:** Proposed & Specified  
**Related Specs:** [Spec 31 (Electronics Workstation)](31_electronics_workstation_and_instrumentation_engine.md), [Spec 32 (One-Ship Designer Integration)](32_oneship_designer_integration.md)

---

## 1. Overview & Architectural Scope

ONE-SHIP integrates **SolidCraft** (parametric 3D CAD) and **PCBCraft** (EDA schematics & PCB layout) as decoupled external toolchains managed by `oneship-daemon` (`kopeng::solidcraft`, `kopeng::pcbcraft`), replicating the project-scoped plugin architecture of Spec 32 **without** forked UI rebranding (`LICENSE-BRAND` compliant).

### 1.1 Division of Responsibilities (Human vs. Agent vs. Deterministic Engines)

| Domain | In-App ONE-SHIP UI | External Native Editor | Agent / MCP Scope | Deterministic Compute |
| :--- | :--- | :--- | :--- | :--- |
| **SolidCraft (CAD)** | Multi-view PNG preview (`iso`, `front`, `top`, `right`, section cuts) + physical metrics table | `/usr/bin/solidcraft` launched/focused via `hyprctl` | **No LLM CAD modeling.** Optional read-only inspection (`volume`, `bbox`, `parameters`) only | `solidcraft-cli snapshot` & `solidcraft-cli eval` |
| **PCBCraft (EDA/PCB)** | Native ONE-SHIP SVG schematic/board viewer + BOM, netlist, and DRC inspector | `pcbcraft` (`eframe`/`wgpu`) launched/focused via `hyprctl` | `/mcp/pcbcraft`: component search/selection, schematic authoring (`schematic.*`), netlist & DRC inspection (`pcb.check_design`). **No LLM copper routing** (`pcb.add_tracks`, `pcb.add_via` guarded by skill) | `pcbcraft-cli mcp` + ONE-SHIP SPICE bridge (`ngspice` / `xyce`) |

### 1.2 Licensing & Branching Policy
- **SolidCraft:** Consumes the host-installed binaries (`solidcraft-bin`: `/usr/bin/solidcraft`, `/usr/bin/solidcraft-cli`) or user-configured paths. In compliance with SolidCraft's `LICENSE-BRAND`, ONE-SHIP does not redistribute forked binaries containing `solidcraft-ui-kit`, `solidcraft-look`, or `solidcraft-theme`.
- **PCBCraft:** Compiles from a clean sibling clone (`<sibling-repos>/pcbcraft`) tracking `upstream/main` (`MIT OR Apache-2.0` core). Agnostic improvements are developed on `fix/*` / `feat/*` branches for upstream Pull Requests rather than maintaining a permanently diverged rebranded fork.
- **Resource-Bounded Local Builds:** On memory-constrained developer hosts (15 GiB RAM / 16 threads), `pcbcraft` builds must cap Cargo parallelism (`-j 2`), disable ThinLTO (`CARGO_PROFILE_RELEASE_LTO=false`), limit codegen units (`CARGO_PROFILE_RELEASE_CODEGEN_UNITS=4`), and run inside a cgroup v2 memory scope (`MemoryHigh=8G`, `MemoryMax=10G`).

---

## 2. Configuration Schema (`config.rs` & `rula.toml`)

All plugin knobs are registered in `get_all_plugin_schemas()` in `src-tauri/src/config.rs` and exposed in `SettingsModal.vue`:

### 2.1 `[plugins.solidcraft]`
- `enabled` (`boolean`, `project_only`, default `false`): Enables SolidCraft CAD preview and launcher for the project.
- `app_binary_path` (`string`, `cascade`, default `"solidcraft"`): Executable name or absolute path to the SolidCraft desktop editor binary.
- `cli_binary_path` (`string`, `cascade`, default `"solidcraft-cli"`): Executable name or absolute path to `solidcraft-cli`.
- `cad_dir` (`string`, `cascade`, default `"cad"`): Project-relative directory monitored for `.sc`, `.step`, `.stp`, and `.3mf` files.
- `snapshot_timeout_ms` (`number`, `cascade`, default `15000`): Maximum execution window in milliseconds for a single `solidcraft-cli snapshot` or `eval` invocation.
- `snapshot_width_px` (`number`, `cascade`, default `1280`): Preview image width in pixels.
- `snapshot_height_px` (`number`, `cascade`, default `800`): Preview image height in pixels.
- `auto_snapshot_on_edit` (`boolean`, `cascade`, default `true`): Automatically create a scoped Git snapshot when CAD files change.
- `max_snapshots_per_asset_count` (`number`, `cascade`, default `20`): Maximum CAD snapshots retained per target file.

### 2.2 `[plugins.pcbcraft]`
- `enabled` (`boolean`, `project_only`, default `false`): Enables PCBCraft EDA preview, launcher, and `/mcp/pcbcraft` gateway for the project.
- `app_binary_path` (`string`, `cascade`, default `"pcbcraft"`): Executable name or absolute path to the PCBCraft desktop editor binary.
- `cli_binary_path` (`string`, `cascade`, default `"pcbcraft-cli"`): Executable name or absolute path to `pcbcraft-cli`.
- `pcb_dir` (`string`, `cascade`, default `"pcb"`): Project-relative directory containing `pcbcraft-project/17` folders (`project.json`, `sheets/*.json`, `boards/*`) or `.pc` archives.
- `mcp_request_timeout_ms` (`number`, `cascade`, default `30000`): Maximum wait time in milliseconds for a single `pcbcraft-cli mcp` JSON-RPC response.
- `auto_snapshot_on_edit` (`boolean`, `cascade`, default `true`): Automatically create a scoped Git snapshot when PCBCraft saves or modifies PCB files.
- `max_snapshots_per_asset_count` (`number`, `cascade`, default `20`): Maximum PCB snapshots retained per board target.

### 2.3 `[plugins.electronics]` SPICE Extension (EIS & Mixed-Signal)
- `spice_engine` (`select`, `cascade`, default `"ngspice"`, options `["ngspice", "xyce"]`): Selects the headless SPICE solver used by Spec 31 simulations.
  - `ngspice` (`extra/ngspice`): Primary engine for mixed-signal analog + XSPICE digital event-driven simulation, linear `.ac` sweeps, `.noise`, and `.tran`.
  - `xyce` (`aur/xyce-serial-bin` / `aur/xyce-shylu`): Optional advanced solver for Harmonic Balance (`.hb`) and nonlinear DAE homotopy convergence in complex Electrical Impedance Spectroscopy (EIS) models.
- `xyce_binary_path` (`string`, `cascade`, default `"/usr/bin/Xyce"`): Path to the Xyce executable when selected.

---

## 3. Daemon Subsystems & API Contracts

Both modules follow Spec 12 §7.2 (registered in `api::API_COMMANDS`, `api::dispatch`, `DaemonCommands`, and exported via `ts-rs` to `src/types/generated/`), use `thiserror` domain error enums (`SolidcraftError`, `PcbcraftError`) implementing `api::ApiError`, and enforce Rule 11 strict project scoping (zero permissive fallbacks).

### 3.1 `kopeng::solidcraft`
- **File Watcher (`notify`):** Monitors `<project>/<cad_dir>` for `.sc`, `.step`, `.stp`, and `.3mf` changes, emitting `solidcraft-file-changed` over `/api/events`. In parallel, `kopeng::documents` maintains a debounced recursive project watcher emitting `project-files-changed` so external CAD/agent creations appear in the Project File Explorer automatically.
- **Daemon Commands:**
  - `get_project_solidcraft_status(project_path)` -> `SolidcraftStatus` (`project_path`, `is_enabled`, `cad_dir_path`, `cad_files_count`, `cad_files`, `app_available`, `cli_available`, `is_editor_running`).
  - `toggle_project_solidcraft_plugin(project_path, enabled)` -> `SolidcraftStatus`.
  - `create_solidcraft_file(project_path, file_name, template_id?)` -> spawns `solidcraft-cli mcp --headless --allow-path <cad_dir>` (`new_design` + `document.units` + optional `sketch.create` + `save`) to create a valid `.sc` design from a template (`blank_mm`, `blank_cm`, `blank_m`, `blank_in`, `blank_ft`, `xy_sketch_mm`, `xy_sketch_in`) and emits `solidcraft-file-changed`.
  - `launch_project_solidcraft(project_path, file_name?)` -> spawns `/usr/bin/solidcraft <file>` on an empty Hyprland workspace (`desktop.rs`) or focuses existing PID via `hyprctl`.
  - `render_solidcraft_preview(project_path, file_name, view, section?)` -> executes `solidcraft-cli snapshot --in <path> --out <cache_png> --view <view> --width <w> --height <h>` for any of the 7 camera views (`iso`, `front`, `back`, `top`, `bottom`, `left`, `right`), backed by `preview_cache.rs` and `hardwarePreviewStore.ts`.
  - `inspect_solidcraft_metrics(project_path, file_name)` -> executes `solidcraft-cli eval --in <path>` (read-only), returning volume ($\text{mm}^3$), bounding box ($\text{mm}$), body count, and editable parameters.
- **Deferred (Stage A.2 — `TECHNICAL_DEBT.md` §26):** Client-side interactive WebGL 3D orbit viewport (`Three.js` + `solidcraft-cli mcp` `export` to `.stl`/`.obj`).

### 3.2 `kopeng::pcbcraft`
- **File Watcher (`notify`):** Monitors `<project>/<pcb_dir>` for `project.json`, `sheets/*.json`, `boards/*`, and `.pc` changes, emitting `pcbcraft-file-changed` over `/api/events`.
- **Daemon Commands:**
  - `get_project_pcbcraft_status(project_path)` -> `PcbcraftStatus` (`is_enabled`, `pcb_dir_path`, `pcb_targets`, `app_available`, `cli_available`, `is_editor_running`).
  - `toggle_project_pcbcraft_plugin(project_path, enabled)` -> `PcbcraftStatus` (also synchronizes `"pcbcraft"` entry in `.mcp.json` / `opencode.json` pointing to `http://localhost:{port}/mcp/pcbcraft` and symlinks/removes `oneship-pcbcraft` skill on demand).
  - `create_pcbcraft_project(project_path, target_name)` -> scaffolds a validated `pcbcraft-project/16` folder inside `<project>/<pcb_dir>/<target_name>`, verifies loading via `pcbcraft-cli mcp --project`, and emits `pcbcraft-file-changed`.
  - `launch_project_pcbcraft(project_path, target_name?)` -> spawns or focuses `pcbcraft` on an empty Hyprland workspace via `desktop.rs`.
  - `inspect_pcbcraft_project(project_path, target_name?)` -> queries `pcbcraft-cli mcp --project <target>` (`inspect_project` + `execute("pcb.check_design")`) to return `PcbcraftProjectInspection` (sheets, boards, BOM/components with simulation attributes, nets, and DRC violations/status).
  - `render_pcbcraft_svg_preview(project_path, target_name?, sheet_id?, mode?)` -> renders `inspect_project` schematic or board geometry in pure Rust into a bounded ONE-SHIP SVG preview (`PcbcraftSvgPreviewResult`) styled with semantic CSS variables (`schematic` or `board` mode).
- **MCP Gateway (`/mcp/pcbcraft?project_hex=...`):**
  - Mounted in `src-tauri/src/mcp.rs` with Rule 11 caller attestation (`resolve_caller` / `?project_hex=`) and registered globally in `~/.gemini/config/mcp_config.json` (`config.rs`) plus project-local `.mcp.json` / `opencode.json` (`skill.rs`).
  - Manages a per-project stdio pool of `pcbcraft-cli mcp --project <resolved_pcb_target>`.
  - **Platform Skill Guardrail (`templates/skills/oneship-pcbcraft/SKILL.md`):** Instructs agents to use `inspect_project`, `library.*`, `schematic.*`, and `pcb.check_design`, and explicitly prohibits LLM copper routing commands (`pcb.add_tracks`, `pcb.add_via`, `pcb.reshape_tracks`).

### 3.3 SPICE / EIS Netlist Bridge (Spec 31 Integration)
- `kopeng::pcbcraft` provides a pure-Rust netlist extractor converting `pcbcraft-project/17` schematics (respecting PCBCraft's `metadata.attributes.exclude_from_simulation` flag) into normalized SPICE `.cir` netlists.
- `kopeng::electronics::spice` dispatches `.ac` (frequency sweep for EIS Bode & Nyquist $Z'(\omega)$ / $-Z''(\omega)$), `.tran`, `.noise`, and `.hb` (when `spice_engine == "xyce"`) in `<project>/.one-ship/electronics/sim/`, returning bounded numeric digests to agents and plot series to the UI.

---

## 4. Frontend Workspace Integration (`src/`)

- **`HydratedProject` (`src-tauri/src/hydration.rs`):** Adds `has_solidcraft_enabled: bool` and `has_pcbcraft_enabled: bool`.
- **`ProjectWorkspace.vue`:** Conditionally renders contextual tabs:
  - `CAD` (`ProjectCadTab.vue`): File selector, view presets (`iso`, `front`, `top`, `right`), optional section plane input (`z=...`), zoom/pan PNG stage, read-only physical metrics panel, and `Open in SolidCraft` action.
  - `PCB` (`ProjectPcbTab.vue`): Sheet/Board mode toggle, native ONE-SHIP SVG pan/zoom canvas, BOM & Netlist table, DRC findings panel, `Run SPICE / EIS Sweep` shortcut (linking to Spec 31 simulation), and `Open in PCBCraft` action.
- All UI controls follow Spec 13 (`AppTooltip`, `AppSelect`, `AppInput`, semantic CSS variables, zero `any`, explicit `onUnmounted` event cleanup).

---

## 5. Upstream Contribution Roadmap (`tangency-labs/pcbcraft`)

While ONE-SHIP stays fully functional out of the box via its native `pcbcraft-project/17` JSON SVG renderer and SPICE bridge, the following agnostic PRs are tracked for upstream collaboration:
1. **`pcbcraft-cli --help` / `-h`** (aligns with open issue `#319`): Low-friction starter PR.
2. **Pure-Rust Netlist / SPICE Export Command (`schematic.export_netlist`):** Leverages existing `exclude_from_simulation` component metadata without adding C/C++ dependencies or breaking `pcbcraft-web` WASM builds.
3. **Headless SVG / Snapshot Export (`pcbcraft-cli snapshot`):** Reuses `crates/engine` / `crates/app` scene primitives to render headless schematic/board SVGs for CI and external orchestrators.
4. **Routing Interoperability Research:** Evaluate Specctra `.dsn` / `.ses` export/import (for external `FreeRouting`) vs. native Rust `geo-index`-backed routing against upstream `#306` and `VISION.md` before writing code.

---

## 6. Tiger Style & Safety Invariants
1. **Bounded Memory:** Preview SVG string generation and PNG reads must enforce explicit upper size limits (`max_preview_bytes`) and bounded node counts (`max_svg_elements_count`).
2. **Bounded Process Execution:** Every `solidcraft-cli` / `pcbcraft-cli` / `ngspice` / `Xyce` subprocess must enforce deterministic timeouts (`*_timeout_ms`) and kill child process trees on timeout or cancellation.
3. **Zero Blocking I/O in Async Contexts:** All `project.json` / `sheets/*.json` reads and subprocess orchestration use `tokio::fs` / `tokio::process` or `spawn_blocking`.
4. **Zero Silent Fallbacks (Rule 11):** Missing binaries return `BinaryNotFound`; missing or ambiguous `project_hex` returns `ProjectContextMissing` immediately.

---

## 7. Stage D Roadmap — BOM Sourcing, Chile Availability & Datasheets

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

---

## 8. Stage E Roadmap — Fabrication Packaging & PCBWay Partner API

Scheduled after **Stage D**, connecting deterministic `pcbcraft` manufacturing exports directly to **PCBWay Partner API** (`https://api-partner.pcbway.com`, `api-key` header) for streamlined quoting and ordering:

1. **Deterministic Fabrication Bundle (`pcb.export_fabrication`):**
   - Triggers `pcbcraft`'s built-in `pcb.export_fabrication` command to package verified Gerber layers (`*-Top.gbr`, etc.), drill files (`*-PTH.drl`), pick-and-place CPL, and BOM into `<project>/<pcb_dir>/<target>/exports/<target>-fabrication.zip`.
2. **PCBWay Partner Quoting & Logistics (`kopeng::electronics::pcbway`):**
   - Derives board dimensions (`Length`, `width` in mm) and copper layer count from PCBCraft board setup/outline.
   - Calls `POST api/Pcb/PcbQuotation`, `POST api/SMT/SMTQuotation` (optional SMT assembly), `POST api/Address/GetCountry`/`GetState`/`GetCity`, and `POST api/Pcb/GetFreightByOrder` (freight to Chile / configured destination).
   - Fallback when no Partner `api-key` is configured: exports the ready-to-upload `-fabrication.zip` and opens PCBWay's standard quote workflow.
3. **Human-Gated Order Execution (Zero Unattended Financial Actions):**
   - Any state-changing order endpoint (`POST api/Pcb/PlaceOrder`, `POST api/Pcb/ConfirmOrder`) MUST block on an explicit UI confirmation / `ApprovalRequestModal` displaying itemized fabrication cost, freight cost, board revision, and `api/Account/QueryBalance` before transmission.
   - Order tracking uses `POST api/Pcb/CheckOrderDetails` and `POST api/Pcb/QueryOrderProcess`.

---

## 9. Dynamic PCBCraft MCP Routing, `save_project` Persistence & Review Workflow

1. **RAM Accumulation + Explicit `save_project`:**
   - `pcbcraft-mcp` (`tools.rs`, `backend.rs`, `live.rs`) exposes `save_project`:
     - In `Headless` (`--project`), atomic edits accumulate in `Session` RAM and `save_project` commits them to disk via `pcbcraft_storage::save` (or `encode_packed` for `.pc`).
     - In `Remote` (`--connect`), `save_project` invokes `file.save` on the open PCBCraft window.
2. **Dynamic `--connect` vs `--project` Gateway Selection:**
   - When `pcbcraft` launches on a project (`PcbApp::with_initial_project`), `LiveControl` publishes an ephemeral `0600` discovery record under `~/.cache/one-ship/pcbcraft-live/` (removed on `Drop`).
   - `get_or_spawn_mcp_process` in `kopeng/pcbcraft.rs` checks for an active live endpoint matching `target_path` (verifying `/proc/<pid>`), spawning `pcbcraft-cli mcp --connect <port> --token-file <file>` when open or `--project <target>` when closed, and recycling the pooled process if editor state changes.
3. **Pre-Save Auto-Snapshot & Post-Save Review Banner:**
   - `/mcp/pcbcraft` captures a scoped pre-save snapshot (`domain = "pcbcraft"`, `asset_key = <target>`) right before `save_project` (and suppresses duplicate `notify` snapshots within `2500 ms`), while external user edits in PCBCraft trigger watcher snapshots.
   - `ProjectPcbTab.vue` surfaces a post-update action bar (**Open in PCBCraft** / **Revert Snapshot** / **Keep**) when a target updates on disk.


