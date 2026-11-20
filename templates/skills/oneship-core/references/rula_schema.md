# ONE-SHIP Configuration Contract (`rula.toml`)

This document defines the schema, cascade resolution, and conventions for project configuration files.

---

## 1. The 4-Level Configuration Cascade

Configuration values are resolved dynamically from most specific to least specific:

```text
+-----------------------------------------------------------------------------+
|                       CONFIGURATION CASCADE HIERARCHY                       |
|                                                                             |
|  [ Level 4: Agent Persona Override ]                                        |
|  rula.toml -> [agents.<name>] (e.g. runner, auto_approve)                   |
|         ^                                                                   |
|         | (Falls back if omitted)                                           |
|  [ Level 3: Project Plugin Override ]                                       |
|  rula.toml -> [plugins.<plugin_id>] (e.g. plugins.vcs.auto_approve)         |
|         ^                                                                   |
|         | (Falls back if omitted)                                           |
|  [ Level 2: User Global Config ]                                            |
|  ~/.config/one-ship/config.toml                                          |
|         ^                                                                   |
|         | (Falls back if omitted)                                           |
|  [ Level 1: In-Memory Plugin Schema Default ]                               |
|  Factory default defined by plugin schema                                   |
+-----------------------------------------------------------------------------+
```

---

## 2. Project Section (`[project]`)

Required top-level metadata for every project.

```toml
[project]
id = "proj-123e4567-e89b-12d3-a456-426614174000" # Unique project UUID for multi-project agent routing
name = "my-project"
oneship_version = "0.2.5" # Platform compliance schema version
description = "Project description" # Optional
```

---

## 3. Plugins Sections (`[plugins.<id>]`)

Configure behavior for Kopeng plugins.

### 3.1 Version Control & Snapshots Plugin (`[plugins.vcs]`)
- `default_agent_id` (string, `project_only`): Declared project agent used for Git actions and OPEN AGENT sessions.
- `default_branch` (string, default: `"master"`, `global_only`): Base branch for new repository initialization.
- `snapshots_enabled` (boolean, default: `true`, `cascade`): Master toggle for project-level snapshot tracking and automatic checkpoints.
- `minutes_checkpoint_mode` (select, default: `"snapshot"`, `cascade`): `"snapshot"` (hidden ref), `"commit"` (real Git commit on active branch), or `"off"`.
- `auto_commit_detail_level` (select, default: `"title_and_decisions"`, `cascade`): `"title_only"`, `"title_and_decisions"`, or `"full"`.
- `auto_snapshot_on_file_change` (boolean, default: `true`, `cascade`): Scoped snapshot when a project file is saved.
- `max_per_project_count` (number, default: `50`, `cascade`): Max code/milestone snapshots retained per project.
- `scratchpad_retention_days` (number, default: `0`, `cascade`): Days before auto-pruning `scratchpad/` files (`0` = disabled).

#### VCS Action Presets (`[[plugins.vcs.actions]]`)
Repeated table defining quick prompts rendered as executable actions in ONE-SHIP UI:
```toml
[[plugins.vcs.actions]]
name = "Auto-Commit"
prompt = "Analyze git diff and create a clean semantic commit"

[[plugins.vcs.actions]]
name = "Branch Sync"
prompt = "Fetch upstream changes and rebase current branch"
```

### 3.2 Reverse Engineering Lab (`[plugins.rea]`)
Project-level overrides for REA (only declare keys that differ from global `config.toml` or schema defaults):
- `enabled` (boolean, default: `false`, `project_only`): Enables the REA MCP engine and `.one-ship/rea` isolation.
- `targets_dir` (string, default: `"targets"`, `cascade`): Project-relative subdirectory scanned for target binaries, firmware images, and packages (e.g. `"re/targets"`, `"firmware/dumps"`).
- `ghidra_install_dir` (string, default: `"/opt/ghidra"`, `cascade`): Local Ghidra installation path.
- `idle_timeout_seconds` (number, default: `600`, `cascade`): Inactivity auto-stop timeout.
- `max_jvm_heap_mbytes` (number, default: `2048`, `cascade`): Max Ghidra JVM `-Xmx` heap.

### 3.3 Electronics & Embedded Lab (`[plugins.electronics]`)
Project-level overrides for embedded firmware and hardware engineering (only declare keys that differ from defaults):
- `enabled` (boolean, default: `false`, `project_only`): Enables hardware inspection, firmware build/flash, and instrumentation.
- `firmware_dir` (string, default: `"."`, `cascade`): Project-relative subdirectory containing the microcontroller firmware project (`platformio.ini`, `src/`, `include/`). Set to e.g. `"firmware"` in multi-subsystem repositories.
- `schematics_dir` (string, default: `"."`, `cascade`): Project-relative subdirectory containing EDA schematics (`.kicad_sch`) and SPICE simulation netlists (`.cir`), e.g. `"hardware"`.
- `default_baud_rate_bps` (select, default: `"115200"`, `cascade`): Serial monitor baud rate.
- `flasher_tool` (select, default: `"pio"`, `cascade`): `"pio"`, `"probe-rs"`, or `"esptool"`.

---

## 4. Agent Personas (`[agents.<name>]`)

Define specialized agent personas supervised by ONE-SHIP:

```toml
[system]
mcp_dialogue_mode = true                         # Global default: routes responses via MCP post_agent_response

[agents.bosmang]
runner = "agy"                                   # Required: a runner with a manifest
model = "gemini-3.8-flash"                       # Required: a model that runner declares
effort = "high"                                  # Optional: one of the efforts declared for that model
accept_edits = true                              # Passes --mode accept-edits if true (auto-accepts diffs)
auto_approve = false                             # Passes --dangerously-skip-permissions if true
mcp_dialogue_mode = true                         # Routes user responses via post_agent_response
system_prompt = """
Role: Lead Systems Architect for project-name.
Ownership: Root manifests and architecture decisions.
Boundaries: Coordinate multi-agent tasks via delegate_agent_task; delegate UI to frontend.
Conventions: Adhere strictly to AGENTS.md. Validate with cargo check before completion.
"""
args = []                                        # Additional CLI parameters passed to the runner
context_docs = [                                 # Relative paths referenced by the agent on demand
    "docs/00_manifesto.md",
    "docs/01_ubiquitous_language.md"
]

[agents.backend]
runner = "agy"
model = "gemini-3.8-flash"
effort = "high"
accept_edits = true
auto_approve = true
args = []
context_docs = ["docs/specs/README.md"]
system_prompt = """
Role: Core Backend Specialist for project-name.
Ownership: src/backend/.
Boundaries: Do NOT touch UI code in src/frontend/; delegate UI to frontend via delegate_agent_task.
"""

# NOTE: NEVER define [agents.git] here.
# Git is an internal platform agent automatically managed by ONE-SHIP when .git/ exists.
```

Reserved platform agent IDs, never used as persona names: `system`, `git`, `initializer`, `removal-resolver`, `config-resolver`. Persona IDs are lowercase letters, digits, `-` or `_`.

**Runner / model / effort pairs:** a model always belongs to its runner. The ONE-SHIP runner manifests are the only source of which models each runner has and which efforts each model accepts; ONE-SHIP injects that catalog into the agents that write personas (initializer, config repair). Never write a model from memory or the examples above: take the pair from the catalog. A persona without `runner` and `model`, a model of another runner (or one removed from the runner) or an unsupported `effort` makes `rula.toml` invalid (`[! CONFIG]`) and ONE-SHIP ignores the file until it is fixed. The same applies to `[plugins.orchestrator]`: a table that sets `agent_runner` sets `default_model` too (and `init_runner` with `init_model`).

### 4.1 Bounded Persona Prompt Specification (Token Economy)
To preserve token quota and maintain clear domain boundaries, persona `system_prompt` strings must adhere strictly to these rules:
1. **Length Budget:** Hard ceiling of ≤ 100 words (~120 tokens).
2. **3-Part Anatomy:**
   - **Identity & Directory Ownership:** Specify the persona role and exact directory owned (e.g. `src/`, `src-tauri/`).
   - **Negative Boundaries & Delegation:** Declare out-of-scope domains and the peer agent to delegate to via `delegate_agent_task`.
   - **Primary Tech Stack:** Primary libraries/frameworks (purely descriptive; never include actionable verification commands in the prompt — verification commands belong strictly under `[scripts]`).
3. **Zero Redundancy with `AGENTS.md`:** Never duplicate MCP communication protocols, approval requirements, or generic coding advice in the persona prompt. Global rules are loaded automatically from `AGENTS.md`.

### Supported Runners:
- `agy`: Antigravity CLI agent
- `claude`: Anthropic Claude Code CLI
- `opencode`: OpenCode CLI

---

## 5. Scripts & Tasks Configuration (`[scripts]`)

ONE-SHIP orchestrates persistent processes (dev servers, watchers) and one-shot deterministic tasks (build, test, lint) under the `[scripts]` hierarchy.

### 5.1 Persistent Scripts (`[scripts.persistent.<id>]`)
Processes that run continuously in the background or inside dedicated workspace terminal tabs:

```toml
[scripts.persistent.dev]
command = "npm run dev"
cwd = "."
profiler = "v8prof"   # Optional: profiler engine ID (samply, py-spy, v8prof) wrapping the service

# Unified options schema (boolean toggle or select dropdown)
[scripts.persistent.dev.options.debug]
flag = "--debug"
label = "Verbose Logs"
type = "boolean"      # Default
default = false

[scripts.persistent.dev.options.env]
flag = "--env"
label = "Environment"
type = "select"
choices = ["dev", "staging", "prod"]
default = "dev"
```

### 5.2 Deterministic Tasks (`[scripts.task.<id>]`)
One-shot tasks with exit code verification and execution duration tracking:

```toml
[scripts.task.build]
command = "npm run build"
cwd = "."

[scripts.task.build.options.profile]
flag = "--profile"
label = "Build Profiler"
type = "boolean"
default = false

[scripts.task.build.options.target]
flag = "--target"
label = "Compilation Target"
type = "select"
choices = ["x86_64", "aarch64", "wasm"]
default = "x86_64"
```
