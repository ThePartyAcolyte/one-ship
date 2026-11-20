# Devlog: Configuration Schemas, Cascade Engine & Platform Skill Injection
**Date:** 2026-09-02  
**Topic:** 4-Level Configuration Cascade, Plugin Schemas, Decoupled Agent Flags, and Agnostic Platform Skill

## Overview
Implemented Phase 3.3 by replacing hardcoded execution parameters with a hierarchical 4-level configuration cascade (Agent Persona -> Project Plugin -> Global Config -> Schema Defaults). Additionally, restored `<BrainCircuit />` to its correct conceptual role: injecting the purely agnostic Kapawu platform specification kit into `.agents/skills/kapawu-platform.md`.

## Key Changes
1. **Configuration Engine (`src-tauri/src/config.rs`)**:
   - Implemented `PluginConfigSchema` and `ConfigFieldSchema` representing typed configuration metadata (`boolean`, `string`, `select_enum`, `number`, `string_list`).
   - Created global configuration loader creating `~/.config/omarchy-project-tracker/config.toml` with safe defaults (`agent_runner = "agy"`, `dangerously_skip_permissions = false`).
   - Built cascade resolvers: `resolve_effective_bool`, `resolve_effective_string`, `resolve_effective_string_list`.
   - Exposed Tauri commands: `get_plugin_schemas`, `get_global_config`, `get_effective_project_config`.
2. **Decoupled Herder Launcher (`src-tauri/src/kopeng/herdr.rs`)**:
   - Replaced static flags with dynamic queries to the cascade: runner executable (`agy` by default, or configurable per-agent) and `--dangerously-skip-permissions` (passed only if enabled in config).
3. **Agnostic Platform Skill Injection (`src-tauri/src/kopeng/skill.rs`)**:
   - Implemented `inject_platform_skill` command writing `.agents/skills/kapawu-platform.md`.
   - Strictly platform-agnostic: documents `rula.toml` structure, Kapawu MCP tool primitives, and environment paths without assuming project business logic.
4. **UI Refinement (`src/App.vue`)**:
   - Restored `<BrainCircuit />` to trigger platform skill injection.
   - Introduced dedicated "HERDER" workspace container in the project card body showing workspace ID, connection status, and active agent mini-pills.
