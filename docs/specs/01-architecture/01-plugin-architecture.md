# Plugin Architecture (Kopeng)

## Purpose

Prevent monolithic coupling across ONE-SHIP's multidisciplinary maker domains by structuring each domain capability as an isolated, modular subsystem (a *kopeng*), and by supporting external declarative plugins ("recipes") discovered from disk.

## Scope

- Compiled domain modules in the Rust daemon (`src-tauri/src/kopeng/`).
- Frontend integration of domain workspaces (feature-gated tabs, typed UI event bus).
- The user-facing `rula.toml` namespacing rule.
- External declarative plugins described by `manifest.toml` and their discovery cascade.

Configuration keys, schemas and the cascade are specified in [Configuration Schema and Cascade](02-configuration-schema-and-cascade.md). The daemon's command registry is specified in [Daemon Operator API and Lifecycle](05-daemon-operator-api-and-lifecycle.md).

## Model / Design

### Terminology

- **Bosmang (Orchestrator):** the core daemon and process supervisor that manages the lifecycles of services, tasks and agents.
- **Kopeng (Plugins):** "friends/allies". Domain modules that attach to the ONE-SHIP core.

In the Rust daemon each domain lives in its own isolated module under `src-tauri/src/kopeng/`. In the Vue frontend, domain workspaces are mounted in `ProjectWorkspace.vue` and enabled or hidden per project via `[plugins.<id>].enabled` and `projects.hidden_tabs`.

### `rula.toml` namespacing (plain English rule)

**Critical rule:** internal Rust modules use the `kopeng` namespace, but the user-facing `rula.toml` configuration file MUST use plain, standardized English for keys and tables:

- Services and tasks are declared under `[scripts.persistent.<id>]` and `[scripts.task.<id>]`.
- Domain plugin overrides and activation flags are declared under `[plugins.<plugin_id>]` (see [Configuration Schema and Cascade](02-configuration-schema-and-cascade.md)).

```toml
[project]
id = "proj-123e4567-e89b-12d3-a456-426614174000"
name = "My Project"
oneship_version = "0.2.6"

[scripts.persistent.dev]
name = "Dev Server"
command = "npm"
args = ["run", "dev"]
port = 5173

[plugins.rea]
enabled = true

[plugins.electronics]
enabled = true
firmware_dir = "firmware"
```

### Rust backend structure (`src-tauri/src/kopeng/`)

Each capability is encapsulated in its own domain module:

- **Core lifecycle & config:** `initializer.rs`, `flag_resolver.rs`, `manifest.rs`, `migration.rs`, `persona.rs`, `prompt.rs`, `resolution.rs`, `runner.rs`, `skill.rs`, `snapshots.rs`, `fs_tree.rs`, `documents.rs`, `dna.rs`, `errors.rs`, `diagnostic.rs`.
- **Accounts, quotas & voice:** `accounts.rs`, `stats.rs`, `dictation.rs`, `voice_controller.rs`.
- **VCS & OSS collaboration:** `git.rs`, `github.rs`, `gitlab.rs`, `project_mgmt.rs`.
- **Maker & hardware workstations:** `electronics/` (oscilloscopes, logic analyzers, MCU toolchains), `solidcraft.rs` (3D CAD), `pcbcraft.rs` (EDA/PCB), `desigma.rs` (One-Ship Designer), `rea.rs` (reverse engineering), `profiler.rs`, `preview_cache.rs`.

Command handlers are plain functions of their domain module, registered as daemon commands in `api::API_COMMANDS` / `api::dispatch` (`src-tauri/src/api.rs`) with typed arguments, results and errors exported to TypeScript via `ts-rs`. No domain command is a `#[tauri::command]`; the GUI app registers only the two commands that bootstrap its daemon connection.

### Frontend workspace integration

1. **Feature-gated workspace tabs (`ProjectWorkspace.vue`, `ProjectConfigPanel.vue`):** domain tabs (`Electronics`, `Design`, `CAD`, `PCB`, `RE Lab`, `Profiling`, `Project`) are gated by their `[plugins.<id>].enabled` flag in `rula.toml` (hydrated into `HydratedProject.has_*_enabled`) and by per-project tab visibility (`projects.hidden_tabs`). Host-level gating is described in [Linux Packaging and Host Capabilities](07-linux-packaging-and-host-capabilities.md).
   - *Historical note:* the early `<ZoneOutlet>` / `pluginStore.ts` dynamic component injection experiment was retired in favor of explicit, strongly typed Vue tab components and Pinia domain stores.
2. **Typed UI event bus (`src/utils/uiEvents.ts`):** cross-component UI actions (opening modals, triggering workspace transitions such as `oneship-add-persona` or `oneship-open-history`) go through the strongly typed `emitUiEvent` / `onUiEvent` (`UiEventMap`) wrapper around `CustomEvent`. Daemon broadcasts arrive separately over the `/api/events` SSE stream (`listenDaemonEvent`).

### External declarative plugins (recipes)

In addition to compiled kopeng modules, ONE-SHIP supports external declarative plugins specified by a `manifest.toml`. The active implementation is the profiler engine catalog (see the profiling specification).

**Discovery cascade:**

1. **Project local:** `<project_root>/.one-ship/plugins/<dir>/manifest.toml`
2. **Global user:** `~/.config/one-ship/plugins/<dir>/manifest.toml`
3. **Core built-in:** embedded in the daemon binary (`samply`, `py-spy`, `v8prof`).

The first manifest with a given ID wins. Manifests are read when needed, with no file watcher, and are parsed strictly: an invalid manifest is listed as a fault, never skipped silently.

## Contracts

- **`rula.toml` tables:** `[project]`, `[scripts.persistent.<id>]`, `[scripts.task.<id>]`, `[plugins.<plugin_id>]` (`enabled` plus plugin keys).
- **Plugin manifest file:** `manifest.toml` in a plugin directory (discovery order above).
- **Daemon commands:** registered only in `api::API_COMMANDS` / `api::dispatch`; never `#[tauri::command]`.
- **Hydration flags:** `HydratedProject.has_*_enabled`; SQLite `projects.hidden_tabs`.
- **Frontend:** `emitUiEvent` / `onUiEvent` (`UiEventMap`) in `src/utils/uiEvents.ts`; `listenDaemonEvent` for daemon SSE events.

## Implementation Status

| Area | Status | Notes |
|---|---|---|
| Kopeng modules under `src-tauri/src/kopeng/` | Implemented | Module files listed above exist (verified in `src-tauri/src/kopeng/`). |
| Daemon command registration (`api.rs`) | Implemented | `api::API_COMMANDS` / `api::dispatch`. |
| Feature-gated tabs, typed UI event bus | Implemented | `src/utils/uiEvents.ts` present. |
| External declarative plugin discovery (profiler catalog) | Implemented (as stated in the legacy source; not re-verified in code) | Strict parsing, no watcher. |

## Related Specs

- [Configuration Schema and Cascade](02-configuration-schema-and-cascade.md)
- [Platform Skill Injection](03-platform-skill-injection.md)
- [Daemon Operator API and Lifecycle](05-daemon-operator-api-and-lifecycle.md)
- [Linux Packaging and Host Capabilities](07-linux-packaging-and-host-capabilities.md)

Supersedes legacy: `06_plugin_architecture.md`
