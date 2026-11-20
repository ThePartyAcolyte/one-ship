# 2026-11-17 — Frontend Build Repair After the Forge Backend Handoff

## Summary

The frontend dev server failed after the Forge/backend implementation. The immediate Vite error was an invalid inline template statement; behind it, the ts-rs export had written the new Forge bindings to the wrong directory, so the TypeScript build was also broken. Both are fixed, and `vue-tsc --noEmit` now exits cleanly.

## Root Causes & Fixes

### 1. Vite/Vue template syntax error (`ProjectManagementTab.vue`)

The Gantt rows used `@click="if (!row.isExternal) selectedTaskId = row.bundle.task.id"`. Vue wraps event handlers without a semicolon in parentheses, so an `if` statement is not a valid expression (`Unexpected token (1:1)`).

- Added `selectGanttRow(row: GanttRow)`, which returns early for external overlay tasks and selects the internal task otherwise.
- Both Gantt row click handlers now call `selectGanttRow(row)`.
- Vite transforms `ProjectManagementTab.vue` with HTTP 200 again.

### 2. ts-rs bindings written to the wrong directory (`scripts/run_dev.sh`)

`./scripts/run_dev.sh export-bindings` ran `cargo test` from the repository root. Cargo only discovers `src-tauri/.cargo/config.toml` (which sets `TS_RS_EXPORT_DIR=../src/types/generated`) when it is invoked from `src-tauri`; from the repository root, ts-rs fell back to its default and wrote 372 files to `src-tauri/bindings/`.

- The new Forge types (`ForgeCollabContext`, `ForgeInboxItem`, `ForgeProviderKind`, `GitRemote`, …) were absent from `src/types/generated/`, so `vue-tsc` could not resolve the frontend imports.
- `export_bindings()` now runs the test with an explicit absolute destination:

```sh
TS_RS_EXPORT_DIR="$PROJECT_ROOT/src/types/generated" \
    cargo test --lib export_bindings --manifest-path "$PROJECT_ROOT/src-tauri/Cargo.toml"
```

- Re-ran the export: 376 export tests passed and `src/types/generated/` now contains 373 files, including the Forge and `GitRemote` types and the updated `GitStatus.remotes`.

### 3. TypeScript errors found by `vue-tsc --noEmit`

- `App.vue`: removed the unused `watch` import and the dead diagnostic-panel toggle/timer state.
- `AppHeader.vue`: `get_global_config` returns `JsonValue`; added an explicit `typeof !== 'string'` guard and typed the TOML regex callback.
- `CardHeader.vue`, `DevlogJournalView.vue`, `ProjectOverviewTab.vue`, `projectExplorerStore.ts`, `daemonApi.ts`: removed unused imports/variables.
- `FloatingChatOverlay.vue`: removed the obsolete `view: 'workspace'` fields that no longer exist on `NavigationTarget`.
- `AgentTaskbar.vue`: removed the reference to the undefined `systemAgents`; the fallback already covers agents outside the domain/resolver lists.
- `ProjectConfigPanel.vue`: added `agent_pairs: []` to the synthetic `skills` schema entry.
- `ProjectManagementTab.vue`: annotated `ganttGridStyle` with an explicit `Record<string, string>` return type; replaced the two silent `setSlotValue(...).catch(() => {})` calls with `reportAsyncFailure`.
- `workspaceLayoutStore.ts`: removed the unused `uiRegistry` state and its `get_ui_registry` call, which resolved the excessively deep Pinia type instantiation; moved `projectSlotValue` to a regular action.
- `OscilloscopeView.vue`: it referenced `props.project.id` without a `project` prop. Added `projectId` to its props and passed `:project-id="project.id"` from `ProjectElectronicsTab.vue`.
- `stores/index.ts`: removed the export of the non-existent `./pluginStore`.

## Validation

- `./scripts/run_dev.sh export-bindings` → 376 passed, 0 failed; bindings written to `src/types/generated/`.
- `./node_modules/.bin/vue-tsc --noEmit` → exit 0, no errors.
- Vite dev server (`oneship-dev-vite`) transforms `App.vue`, `ProjectManagementTab.vue`, `ForgeCollabPanel.vue`, `ProjectWorkspace.vue`, the Forge store, `daemonApi.ts` and the other touched modules with HTTP 200.
- No `vite build` was run (AGENTS.md §1.6); the type check and dev transforms cover the reported failure.

## 4. Unresponsive Interface: Unbounded Git Diff Rendering

After the restart, the WebKit renderer grew to ~6 GB RSS with ~100% CPU and the interface stopped responding. The active project (`Kirulab Website`) has a ~29 MB design file (`design/kirulab-web.desigma`) in its working tree, and the Git tab loaded the complete working-tree diff into the DOM: one row per diff line, with no bound. The renderer was stuck laying out the unbounded diff.

- `ProjectGitSnapshotsTab.vue` now bounds the diff viewer:
  - at most 4 MB of diff text is parsed (`MAX_DIFF_PARSE_CHARS_COUNT`);
  - at most 4,000 diff lines are rendered (`MAX_DIFF_RENDER_LINES_COUNT`);
  - each rendered line is capped at 2,000 characters (`MAX_DIFF_LINE_CHARS_COUNT`);
  - a warning bar reports text/line/length truncation and asks the operator to select a file to narrow the diff.
- The current WebKit process was already stuck before the fix; a GUI restart is required to load the bounded viewer. Once restarted, the Git tab loads the same project without the multi-GB layout.

## 5. Validation (continued)

- `vue-tsc --noEmit` → exit 0 after the diff cap.
- Vite transforms `ProjectGitSnapshotsTab.vue` with HTTP 200.
- `ps` on the pre-fix WebKit process showed the runaway renderer; it must be restarted to clear the leaked layout.

## Pending

- Reload/restart the GUI once to apply the bounded diff viewer; the old WebKit process cannot recover.
- `src-tauri/bindings/` remains as a duplicate 372-file export from the broken run. It is not imported by the frontend; `src/types/generated/` is canonical. Remove the stray directory (recorded in `TODO.md`).
- Runtime validation of the Forge/GitLab/inbox work and the Gantt/last-view restore remains pending in `TODO.md`.
