# Devlog: Visual Builder & Onlook Removal (2026-11-15)

## Summary
Removed the stack-specific Platform Visual Builder (Onlook AST bridge, JSX/Tailwind source mutation engine, and embedded inspector) across all ONE-SHIP subsystems, along with the `web_builder` and `no_dev_server` initialization flags. Cleaned all residual Onlook/Visual Builder boilerplate directly from existing registered projects (`estudiorc`, `watchtower`) and `~/.config/one-ship/oneship.db`, configuring `[preview]` in their `rula.toml` files instead.

## Changes Executed

### 1. Backend & Database (`src-tauri/`)
- Deleted `src-tauri/src/kopeng/visual_builder.rs` and removed `pub mod visual_builder;` from `src-tauri/src/kopeng/mod.rs`.
- Removed all 12 Visual Builder commands (`launch_visual_builder`, `toggle_project_web_builder`, `update_jsx_element_classes`, `update_jsx_element_text`, `update_jsx_image_src`, `restore_source_file_snapshot`, `list_project_images`, `import_project_image`, `scan_project_colors`, `add_tailwind_theme_color`, `update_tailwind_theme_color`, `merge_project_colors`) from `API_COMMANDS` and `dispatch` in `src-tauri/src/api.rs`.
- Removed `visual_builder_snapshots` table, `idx_vb_snapshots_project`, `VbSnapshotRecord`, `db_insert_vb_snapshot`, `db_delete_vb_snapshot`, `db_update_project_web_builder`, and `has_web_builder` / `web_builder_stack` columns from `src-tauri/src/db.rs`, `src-tauri/src/registry.rs`, `src-tauri/src/hydration.rs`, and `src-tauri/src/kopeng/runner.rs`.
- Removed `enable_web_builder` from `CreateGreenfieldProjectArgs` and `scaffold_greenfield_project` in `src-tauri/src/kopeng/initializer.rs`.
- Removed `web_builder` and `no_dev_server` flags from `src-tauri/src/kopeng/flag_resolver.rs` and `src-tauri/src/mcp.rs`.
- Removed `max_visual_builder_undo_snapshots_count` and `resolve_vb_undo_snapshots_max` from `src-tauri/src/config.rs`, and unused `chrono_iso_now` from `src-tauri/src/kopeng/snapshots.rs`.

### 2. Frontend (`src/`)
- Deleted `src/components/workspace/ProjectVisualBuilderTab.vue`, `src/composables/useOnlookClientBridge.ts`, `src/components/workspace/AppImageGalleryModal.vue`, and obsolete generated types.
- Removed `'visual_builder'` tab and `has_web_builder` guards from `src/App.vue`, `src/stores/workspaceLayoutStore.ts`, `src/components/workspace/ProjectWorkspace.vue`, `src/components/workspace/ProjectOverviewTab.vue`, `src/components/ProjectCard/CardHeader.vue`, `src/components/modals/AddProjectModal.vue`, `src/components/ui/AppInitBadge.vue`, and `src/types/daemonApi.ts`.
- Removed leftover `vb-set-freeze-animations` / `vb-set-inspect-mode` `postMessage` calls from `src/components/workspace/ProjectPreviewTab.vue`.

### 3. Templates, Prompts, Skills & Specs
- Deleted `templates/visual-builder-client.ts`, `templates/prompts/flag_resolve_web_builder.md`, `templates/prompts/flag_resolve_dev_server.md`, `docs/specs/27_visual_web_builder_and_onlook_stack_integration.md`, and `docs/specs/28_native_ast_visual_studio_proposal.md`.
- Updated `templates/prompts/greenfield_initializer.md`, `templates/prompts/project_initializer.md`, `templates/skills/oneship-initializer/SKILL.md`, `templates/skills/oneship-core/references/rula_schema.md`, `README.md`, `docs/README.md`, `AGENTS.md`, and specs `06`, `08`, `10`, `12`, `13`, `18`, `24`, `26`.

### 4. Registered Projects & Live SQLite Cleanup
- Cleaned `~/.config/one-ship/oneship.db` (`DROP TABLE IF EXISTS visual_builder_snapshots`, deleted `web_builder`/`no_dev_server` rows in `project_initialization_flags`, dropped `has_web_builder` and `web_builder_stack` columns from `projects`).
- `estudiorc`: removed `frontend/src/visual-builder-client.ts`, its import in `frontend/src/main.tsx`, `@onlook/babel-plugin-react` from `frontend/vite.config.ts` and `frontend/package.json`, and replaced `[scripts.persistent.frontend.visual_builder]` with `[preview] url = "http://localhost:5173"` in `rula.toml`.
- `watchtower`: removed `@onlook/babel-plugin-react-onlook` from `frontend/vite.config.ts` and `frontend/package.json`, and replaced `[scripts.persistent.frontend.visual_builder]` with `[preview] url = "http://localhost:5173"` in `rula.toml`.

## Verification
- `cargo test`: 395 passed, 0 failed, 0 ignored (regenerated `ts_rs` bindings).
- `npm run build`: 0 errors related to Visual Builder / Onlook.
