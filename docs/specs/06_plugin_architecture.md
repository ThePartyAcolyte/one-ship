# Spec 06: ONE-SHIP Plugin Architecture (The "Kopeng" System)

**Document Version:** 2.0.0  
**Domain:** Modular Domain Subsystems, Declarative Configuration & External Plugin Recipes  
**Status:** Implemented & Validated (Build 0.2.6)

---

## 1. Overview
To prevent monolithic coupling across its multidisciplinary maker domains, ONE-SHIP structures domain capabilities as modular subsystems (`kopeng`). In the Rust daemon, each domain lives in its own isolated module under `src-tauri/src/kopeng/`; in the Vue frontend, domain workspaces are mounted in `ProjectWorkspace.vue` and dynamically enabled or hidden per project via `[plugins.<id>].enabled` and `projects.hidden_tabs`.

## 2. Belta Terminology
- **Bosmang (Orchestrator):** The core daemon and process supervisor that manages lifecycles of services, tasks, and agents.
- **Kopeng (Plugins):** "Friends/Allies". Domain modules that attach to the ONE-SHIP core.

## 3. `rula.toml` Namespacing (Plain English Rule)
**CRITICAL RULE:** While internal Rust modules use the `kopeng` namespace, the user-facing `rula.toml` configuration file MUST strictly use plain, standardized English for keys and tables:
- Services and tasks are declared under `[scripts.persistent.<id>]` and `[scripts.task.<id>]` (Spec 01).
- Domain plugin overrides and activation flags are declared under `[plugins.<plugin_id>]` (Spec 10).

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

## 4. Rust Backend Structure (`src-tauri/src/kopeng/`)
Each Kopeng capability is encapsulated within its own domain module under `src-tauri/src/kopeng/`:
- **Core Lifecycle & Config:** `initializer.rs`, `flag_resolver.rs`, `manifest.rs`, `migration.rs`, `persona.rs`, `prompt.rs`, `resolution.rs`, `runner.rs`, `skill.rs`, `snapshots.rs`, `fs_tree.rs`, `documents.rs`, `dna.rs`, `errors.rs`, `diagnostic.rs`.
- **Accounts, Quotas & Voice:** `accounts.rs`, `stats.rs`, `opendictate.rs`.
- **VCS & OSS Collaboration:** `git.rs`, `github.rs`, `gitlab.rs`, `project_mgmt.rs`.
- **Maker & Hardware Workstations:** `electronics/` (oscilloscopes, logic analyzers, MCU toolchains), `solidcraft.rs` (3D CAD), `pcbcraft.rs` (EDA/PCB), `desigma.rs` (One-Ship Designer), `rea.rs` (reverse engineering), `profiler.rs`, `preview_cache.rs`.

Command handlers are plain functions of their domain module, registered as daemon commands in `api::API_COMMANDS` / `api::dispatch` (`src-tauri/src/api.rs`, Spec 12 §7.2) with typed arguments, results, and errors exported to TypeScript via `ts-rs`. No domain command is a `#[tauri::command]`; the GUI app registers only the two commands that bootstrap its daemon connection.

## 5. Frontend Workspace Integration & Typed UI Event Bus
1. **Feature-Gated Workspace Tabs (`ProjectWorkspace.vue` & `ProjectConfigPanel.vue`):**
   - Domain tabs (`Electronics`, `Design`, `CAD`, `PCB`, `RE Lab`, `Profiling`, `Project`) are gated by their respective `[plugins.<id>].enabled` flag in `rula.toml` (hydrated into `HydratedProject.has_*_enabled`) and per-project tab visibility (`projects.hidden_tabs`).
   - *(Historical Note)*: The early `<ZoneOutlet>` / `pluginStore.ts` dynamic component injection experiment was retired in favor of explicit, strongly-typed Vue tab components and Pinia domain stores.
2. **Typed UI Event Bus (`src/utils/uiEvents.ts`):**
   - Cross-component UI actions (such as opening modals or triggering workspace transitions like `oneship-add-persona` or `oneship-open-history`) communicate through the strongly-typed `emitUiEvent` / `onUiEvent` (`UiEventMap`) wrapper around `CustomEvent`, while daemon broadcasts arrive over `/api/events` SSE (`listenDaemonEvent`).

## 6. External Declarative Plugins (Recipes)
In addition to compiled Kopeng modules, ONE-SHIP supports external declarative plugins specified via `manifest.toml`. The active implementation is the profiler engine catalog (see **Spec 25 §4**).

### Discovery Cascade:
1. **Project Local:** `<project_root>/.one-ship/plugins/<dir>/manifest.toml`
2. **Global User:** `~/.config/one-ship/plugins/<dir>/manifest.toml`
3. **Core Built-in:** Embedded in daemon binary (`samply`, `py-spy`, `v8prof`).

The first manifest with an ID wins. Manifests are read when needed, with no file watcher, and parsed strictly: an invalid one is listed as a fault, never skipped silently.
