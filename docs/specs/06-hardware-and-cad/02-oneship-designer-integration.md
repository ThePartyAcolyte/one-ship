# One-Ship Designer Integration

## Purpose

Integrate a native vector and UI design editor (**One-Ship Designer**, binary `oneship-designer`, based on `desigma`) into ONE-SHIP as a decoupled sibling repository paired with a per-project MCP stdio process pool, file watcher, SHA-256 preview cache, and HTTP gateway in `oneship-daemon`.

## Scope

In scope:

- Sibling repository topology (`<sibling-repos>/desigma`), upstream synchronization, pull request workflow, and branding preservation (`TRADEMARKS.md`).
- Runtime binary resolution and project-scoped configuration under `[plugins.desigma]`.
- Daemon commands, headless preview rendering (`--preview-json`), Hyprland editor launching (`--bridge` / `--no-bridge`), file watching, auto-snapshots, and the `/mcp/desigma` HTTP gateway.
- Frontend design tab (`ProjectDesignTab.vue`) and preview cache store (`hardwarePreviewStore.ts`).

Out of scope:

- Internal canvas and rendering implementation of `desigma` upstream crates.
- Global configuration cascade and skill injection mechanics (see [Configuration Schema and Cascade](../01-architecture/02-configuration-schema-and-cascade.md) and [Platform Skill Injection](../01-architecture/03-platform-skill-injection.md)).

## Model / Design

### 1. Repository and Branching Topology

ONE-SHIP integrates `desigma` as a decoupled sibling repository (`<sibling-repos>/desigma`) rather than compiling it inside `oneship-daemon`:

- **Upstream:** `https://github.com/deepfounder-ai/desigma.git`
- **Origin (Fork):** `git@github.com:ThePartyAcolyte/desigma.git`
- **`main`:** Tracks `upstream/main` cleanly without ONE-SHIP-specific changes.
- **Agnostic Bugfix / Feature Branches (`fix/*`, `feat/*`):** Branched from `main` for clean upstream Pull Requests (e.g., `fix/async-file-dialog`, `fix/svg-resize`).
- **`oneship`:** Integration branch based on `main` + agnostic fixes + ONE-SHIP customizations:
  - Rebranded as **One-Ship Designer** (`APP_TITLE = "One-Ship Designer"`, `[[bin]] name = "oneship-designer"`, `assets/icon/oneship-designer-256.png`, explicit `"is based on desigma"` attribution in *About* per `TRADEMARKS.md`).
  - Headless `--preview-json` renderer (with Google Fonts loading via `desigma_fonts::ensure_fonts` + `desigma_export::register_font`) and `--no-bridge` flag for isolated preview subprocesses.
  - `Mode::File` fallback in `desigma-mcp` when the GUI editor is not running.

#### 1.1 Upstream Sync, Pull Requests, and Branding Preservation Workflow

1. **Sync `main` from upstream:**

   ```sh
   git fetch upstream
   git checkout main
   git merge --ff-only upstream/main
   git push origin main
   ```

2. **Contribute agnostic fixes/features upstream:**
   - Branch from `main` (`git checkout -b fix/<topic> main` or `feat/<topic>`). Keep commits strictly free of ONE-SHIP branding or ONE-SHIP-only flags.
   - Verify against upstream `CONTRIBUTING.md` checks (`cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt` on touched files).
   - Push to `origin` (`git push -u origin fix/<topic>`) and open a Pull Request against `deepfounder-ai/desigma`.
3. **Update `oneship` without losing branding:**
   - Merge `main` (and any local `fix/*` branches prior to upstream merge) into `oneship`:

     ```sh
     git checkout oneship
     git merge main
     ```

   - Because branding is isolated in dedicated commits (`Cargo.toml`, `theme.rs`, `home/dialog.rs`, `main.rs`, `assets/icon/oneship-designer-256.png`), non-branding upstream changes merge cleanly. If upstream touches those exact branding lines/files, keep ONE-SHIP's branding (`git checkout --ours ...`) and re-verify `"is based on desigma"` compliance per `TRADEMARKS.md`.
   - Rebuild the release binaries (`cargo build --release -p desigma-app -p desigma-mcp`); `oneship-daemon` picks up `oneship-designer` on the next launch or preview render without rebuilding ONE-SHIP.

### 2. Daemon Resolution, Preview Pipeline, and MCP Gateway

`oneship-daemon` does **not** compile `desigma` during its own build. It resolves external binaries and parameters dynamically at runtime via `[plugins.desigma]`:

- **Editor Launch:** `launch_project_desigma` spawns `oneship-designer` with `--bridge` or `--no-bridge` according to `enable_bridge` on an empty Hyprland workspace via `desktop.rs`.
- **Headless Preview Rendering:** `render_desigma_preview` spawns `oneship-designer --preview-json` (optionally targeting a specific `file_name` and `node_id`), backed by a SHA-256 disk cache (`kopeng/preview_cache.rs`) and `hardwarePreviewStore.ts` in the frontend.
- **File Watcher & Auto-Snapshots:** A `notify` watcher monitors `<project>/<design_dir>` for `.desigma` / `.fig` changes, emits `desigma-file-changed` over `/api/events`, and triggers `try_create_auto_domain_snapshot` (governed by `auto_snapshot_on_edit` and `max_snapshots_per_asset_count`).
- **Per-Project MCP Gateway:** `/mcp/desigma?project_hex=...` routes JSON-RPC tool calls to a per-project `desigma-mcp --workspace <project>/<design_dir>` stdio process pool with strict caller context attestation (attested managed agent or explicit registered `?project_hex=`), allowing all supported agent runners (`agy`, Claude Code, OpenCode) to share identical `desigma_*` tool names.

## Contracts

### Daemon Commands (`src-tauri/src/kopeng/desigma.rs`, `api.rs`, `src/types/daemonApi.ts`)

| Command | Arguments | Result | Behavior |
| :--- | :--- | :--- | :--- |
| `get_project_desigma_status` | `project_path` | `DesigmaStatus` | Returns enabled state, resolved `design_dir_path`, `gateway_url`, `.desigma`/`.fig` count/list, and running editor state |
| `toggle_project_desigma_plugin` | `project_path`, `enabled` | `DesigmaStatus` | Updates `[plugins.desigma].enabled` in `rula.toml` and symlinks/cleans the `oneship-desigma` skill |
| `launch_project_desigma` | `project_path` | `DesigmaStatus` | Spawns `oneship-designer` (`--bridge` or `--no-bridge`) on an empty Hyprland workspace via `desktop.rs` |
| `render_desigma_preview` | `project_path`, `file_name?`, `node_id?` | Preview bundle (`DesigmaPageInfo`, `DesigmaLayerInfo`, rendered asset) | Runs `oneship-designer --preview-json` with SHA-256 cache (`preview_cache.rs`) |

### HTTP MCP Gateway & SSE Events

- **MCP Route:** `POST /mcp/desigma?project_hex=...` (`desigma_mcp_gateway_handler` in `src-tauri/src/mcp.rs` & `kopeng/desigma.rs`).
- **Global MCP Registration:** Registered under `"desigma"` in `~/.gemini/config/mcp_config.json` (`serverUrl: http://localhost:{port}/mcp/desigma`) and per-project `.mcp.json` / `opencode.json`.
- **Daemon SSE Event:** `desigma-file-changed` (`DesigmaFileChangedPayload` in `src-tauri/src/events.rs`), emitted when `.desigma` or `.fig` files change under `design_dir`.

### Error Hierarchy (`DesigmaError` in `src-tauri/src/kopeng/desigma.rs`)

Variants implementing `api::ApiError`: `ProjectNotFound { path }`, `ProjectContextMissing { details }`, `PluginDisabled { project_path }`, `InvalidSetting { key, details }`, `BinaryNotFound { path }`, `ProcessLifecycle { details }`, `McpTransport { details }`, `Io { context, source }`, `Registry(RegistryError)`, `Config(ConfigError)`, `Skill(SkillError)`, `Task(String)`.

### Configuration Keys (`[plugins.desigma]` in `src-tauri/src/config.rs`)

| Key | Type | Scope | Default | Description |
| :--- | :--- | :--- | :--- | :--- |
| `enabled` | `boolean` | `project_only` | `false` | Enables One-Ship Designer MCP integration and `.desigma` prototyping |
| `mcp_binary_path` | `string` | `cascade` | `"desigma-mcp"` | Path or command name for the `desigma-mcp` stdio server binary |
| `app_binary_path` | `string` | `cascade` | `"oneship-designer"` | Path or command name for the One-Ship Designer desktop binary |
| `design_dir` | `string` | `cascade` | `"design"` | Project-relative directory where `.desigma` / `.fig` files are stored and scoped |
| `enable_bridge` | `boolean` | `cascade` | `false` | Launches One-Ship Designer with `--bridge` for live MCP canvas edits (`--no-bridge` when false) |
| `mcp_request_timeout_ms` | `number` | `cascade` | `30000` | Maximum wait time for a single `desigma-mcp` stdio JSON-RPC response |
| `auto_snapshot_on_edit` | `boolean` | `cascade` | `true` | Automatically creates a scoped Git snapshot when Design files change |
| `max_snapshots_per_asset_count` | `number` | `cascade` | `20` | Maximum Design undo snapshots retained per `.desigma` file |

## Implementation Status

Verified against `src-tauri/src/` and `src/`:

| Component | Status | Verification Notes |
| :--- | :--- | :--- |
| Backend plugin, watcher, preview cache, and MCP stdio pool (`kopeng/desigma.rs`, `preview_cache.rs`) | Implemented | Registered in `api::API_COMMANDS` (4 commands) and `mcp.rs` (`/mcp/desigma`) |
| Configuration schema (`[plugins.desigma]`) & `DesigmaError` | Implemented | All 8 keys present in `src-tauri/src/config.rs`; `DesigmaError` implements `ApiError` |
| SSE event (`desigma-file-changed`) & global MCP registration | Implemented | Verified in `src-tauri/src/events.rs` and `src-tauri/src/config.rs` |
| Platform skill template (`templates/skills/oneship-desigma`) | Implemented | Present on disk in `templates/skills/oneship-desigma/` |
| Frontend Design workspace (`ProjectDesignTab.vue`, `hardwarePreviewStore.ts`) | Implemented | Present in `src/components/workspace/ProjectDesignTab.vue` and `src/stores/hardwarePreviewStore.ts` |

## Related Specs

- [CAD and PCB Integration](03-cad-and-pcb-integration.md)
- [Plugin Architecture](../01-architecture/01-plugin-architecture.md)
- [Configuration Schema and Cascade](../01-architecture/02-configuration-schema-and-cascade.md)
- [Platform Skill Injection](../01-architecture/03-platform-skill-injection.md)
- [Git Workspace and Branches](../03-projects/02-git-workspace-and-branches.md)

Supersedes legacy: `32_oneship_designer_integration.md`
