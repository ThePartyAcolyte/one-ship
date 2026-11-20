# 2026-11-09 — Frontend UI Event Bus, Quota Severity Thresholds & Dead Code Purge

## Summary
Completed Fase 9 (resolving `TODO.md` line 15, `TECHNICAL_DEBT.md` #8, and dead code cleanup):

1. **Strongly-Typed UI Event Bus (`TECHNICAL_DEBT #8`):**
   - Implemented `src/utils/uiEvents.ts` declaring `UiEventMap` with strictly-typed payload contracts:
     - `oneship-add-persona`: `ProjectRecord | undefined`
     - `oneship-edit-persona`: `{ project: ProjectRecord; agent: AgentPersonaState }`
     - `oneship-open-history`: `ProjectRecord | undefined`
     - `oneship-open-task-report`: `{ project: ProjectRecord; execId: number }`
     - `oneship-refresh-project`: `void`
     - `switch-to-workspace-terminal`: `{ terminalKey?: string; projectPath?: string } | void`
     - `open-workspace-file`: `{ filePath: string; line?: number }`
     - `open-workspace-devlog`: `{ date?: string } | void`
     - `open-status-log`: `void`
     - `oneship-appearance-updated`: `void`
   - Added `emitUiEvent<K>(event, payload)` and `onUiEvent<K>(event, handler): () => void` returning deterministic unlisten callbacks.
   - Refactored `App.vue`, `ProjectTerminalGrid.vue`, `AgentTaskbar.vue`, `ProjectWorkspace.vue`, `RollingStatusLog.vue`, `SettingsModal.vue`, and `dialogueListeners.ts`, eliminating all untyped `window.dispatchEvent` CustomEvents and ensuring complete listener teardown on unmount.
   - Removed `TECHNICAL_DEBT #8`.

2. **Configurable Quota Runway Severity Thresholds (`TODO.md` line 15):**
   - Added `quota_danger_threshold_percent` (`default: 5.0`, `scope: "global_only"`) alongside `quota_alert_threshold_percent` (`15.0`) in `src-tauri/src/config.rs` and `DEFAULT_GLOBAL_CONFIG_TOML`.
   - Implemented `resolve_effective_f64(project_path, agent_name, plugin_id, key)` in `config.rs`.
   - Updated `src-tauri/src/kopeng/stats.rs`:
     - Refactored `usage_window` to compute dynamic severity: `"danger"` when `remaining <= danger_threshold`, `"warning"` when `remaining <= warning_threshold`, and `"normal"` otherwise.
     - `get_agent_metrics` resolves both thresholds dynamically via `resolve_effective_f64`.
   - Updated `~/.config/one-ship/config.toml` and living specification `docs/specs/10_configuration_schema_and_cascade.md`.
   - Removed resolved item from `TODO.md`.

3. **Dead Code Purge (AGENTS.md §1.4):**
   - Removed empty leftover stub files:
     - `src/stores/useProjectStore.ts`
     - `src/stores/useQuotaStore.ts`
     - `src/components/modals/SkillManagerModal.vue`

4. **Test Fixture Repair:**
   - Synchronized `search_fixture()` in `src-tauri/src/db.rs` to include `agent_uid TEXT` in its `agent_conversations` table schema, fixing `test_conversation_search_is_scoped_ranked_and_filterable`.

## Verification
- `cargo check --manifest-path src-tauri/Cargo.toml --all-targets`: passed with exit code 0.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib`: 340 passed, 0 failed.
- `(cd src-tauri && cargo test --lib export_bindings)`: 218 passed, 0 failed.
- `npx vue-tsc --noEmit`: clean (0 errors).
- `npm run build`: built in 4.68s (0 errors).
