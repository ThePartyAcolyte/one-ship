# 2026-11-20 — Configurable Graph Tab Visibility (`[plugins.semantic].graph_tab_enabled`)

## Summary
- **Cascade Config Setting (`src-tauri/src/config.rs`):**
  - Added `graph_tab_enabled` (`boolean`, default `true`, `scope: "cascade"`, label `"Show Graph Tab"`) to the `"semantic"` (`Knowledge & Semantic Search`) plugin schema.
  - Added `crate::db::notify_projects_updated()` on `save_global_config`, `save_project_rula_config`, and `set_project_config_field` so toggling `graph_tab_enabled` globally or per-project immediately refreshes hydrated workspace state.
- **Hydration & UI Surface Gating (`src-tauri/src/hydration.rs`, `src-tauri/src/ui_state.rs`, `src-tauri/src/capabilities/mod.rs`):**
  - Added `has_knowledge_graph_enabled: bool` to `HydratedProject`, resolved from `[plugins.semantic].graph_tab_enabled`.
  - Gated the `"knowledge"` (`Graph`) surface node in `ui_state::build_surface_registry` with `required_capability: Some("has_knowledge_graph_enabled")`.
- **Workspace Tab Visibility & Spec Sync (`src/components/workspace/ProjectWorkspace.vue`, `docs/specs/22_session_lifecycle_and_devlog_graph.md`):**
  - Updated `ALL_TABS.knowledge.isVisible` to `Boolean(p?.has_knowledge_graph_enabled && !p?.hidden_tabs?.includes('knowledge'))` and added `has_knowledge_graph_enabled` to the active-tab fallback watcher.
  - Documented `graph_tab_enabled` in `docs/specs/22_session_lifecycle_and_devlog_graph.md` (§5.5).

## Verification

1. **Bindings Export (`./scripts/run_dev.sh export-bindings`):**
   - `test result: ok. 413 passed; 0 failed; 0 ignored; 0 measured; 170 filtered out; finished in 0.42s` (regenerated `src/types/generated/HydratedProject.ts`).
2. **Read-Only Rule Gate (`./scripts/verify_rules.sh`):**
   - `[PASS]` for `any`, Tailwind palettes, `backdrop-blur-*`, and hardcoded `/home/` paths; zero new violations in edited files.
3. **Daemon Health & Build Logs (`./scripts/run_dev.sh status` & `journalctl --user -u oneship-dev-daemon-build -n 20 --no-pager`):**
   - `daemon` is `active (running)` on port `8765`; `oneship-dev-daemon-build` completed `dev` profile build in `39.51s` with zero compiler errors.
