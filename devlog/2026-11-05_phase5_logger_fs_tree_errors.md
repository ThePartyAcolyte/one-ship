# 2026-11-05 — Phase 5 Modules 24–25 (`logger.rs`, `kopeng/fs_tree.rs` Typed Errors)

Follow-up of `2026-11-05_phase5_diagnostic_errors.md`. These close the standalone Phase 5 modules. The 52 `Result<…, String>` left belong to the daemon ownership migration (46: `conversation`, `supervisor`, `accounts`, `db`, `mcp`) and the profiler redesign (6).

## `logger.rs`
- `LoggerError::UnknownLevel` (with `Serialize`) replaces the `String` error of `write_client_log`.
- **Silent drops:** a full SQLite log channel dropped entries with `let _ = try_send(..)`. Drops are now counted, and after each batch the worker writes `WARN N entries dropped from system_logs` to `oneship.log`. If the worker thread is gone, each entry writes an `ERROR` line to the file. Both are reported through the file sink, never back into the channel.

## `kopeng/fs_tree.rs`
- `FsTreeError` (`Io`, `Task`, `Registry`, with `Serialize`).
- `get_project_file_tree(project_id)` is async with `spawn_blocking` and logs at `DEBUG` on entry.
- **Swallowed errors:** an unreadable directory showed up empty, unreadable entries were dropped, and an unreadable file type showed as a file. All three are errors now.
- **Config re-read:** `fs_tree_max_depth_count` was re-read for every directory. Both limits are now read once per scan.
- **Silent cut:** a directory at the depth limit showed as empty, and the node count was unbounded. The new `system.fs_tree_max_nodes_count` (default 5000) bounds the scan. The response is `{ nodes, truncated }`, and every cut directory carries `truncated: true`.
- The excluded names are now a constant. Symlinks are still never followed (no loops) and are listed as files.

## Runner list (follow-up of the persona change)
- The persona modal offered `gemini`, `cursor` and `pi`, which have no manifest and are now rejected by `create/update_agent_persona`.
- The new `config::list_agent_runners()` returns every `templates/runners/*.toml` sorted by ID, and the modal lists exactly those.

## Frontend
- `AppFileTree.vue`: `FileTree` type, and `FileTreeNode.children` nullable with `truncated`.
- `AddPersonaModal.vue`:
  - the file tree takes `projectId`, shows its error instead of swallowing it, and marks "Tree truncated" (with an `AppTooltip` naming the settings);
  - runner options come from `list_agent_runners`;
  - a runner schema error is shown instead of `console.warn`.

## Docs
- Spec 09: the drop counter, the disconnected worker and `LoggerError`.
- Spec 10: the `fs_tree_*` keys and `list_agent_runners`.
- `rula_schema.md`: the runner list.

## Verification
- `cargo check --all-targets` clean.
- `clippy`: 17 warnings, none in these modules.
- `cargo test --lib`: 105 passed, 3 of them new (tree ordering and exclusions, depth/node truncation, runner listing). The same 2 `kopeng::resolution` failures remain.
- `vue-tsc` clean.
- `Result<…, String>`: 52.
- Not run in the app.
