# 2026-11-03 — Phase 5 Module 11 (`mcp.rs` Helpers Typed Errors)

Follow-up of `2026-11-03_phase5_snapshots_errors.md`.

## `McpToolError`
- Variants: `SystemAgentHasNoProject`, `CallerProjectUnregistered`, `ProjectNotFound`, `AmbiguousProject`, `ExecutionProjectUnregistered`, `InvalidRula`, `FlagNotInProject`, `RemovalFlagClosedByTool`, `ArtifactPathOutsideProject`, `Io`, `Registry`, `Database`. It implements `Serialize` (as its message), so tool handlers embed it in their results unchanged; messages keep the previous wording.
- Converted: `caller_project`, `resolve_target_project`, `find_registered_project`, `task_process_key`, `conversation_tool_scope`, the `resolve_initialization_flag` scope check, and the `switch_conversation_topic` blocking closure (`DatabaseError`). `mcp.rs` has no `Result<_, String>` left; the backend total is 85 (245 at the audit).

## Fixes found on the way
- Security: `post_agent_response` joined `file_path` (and artifact text read as a path) to the project unchecked, so `../../.ssh/id_rsa` was read, stored as the chat artifact and shown in the UI; the agy brain recovery joined the same path under `~/.gemini/.../brain/<session>/` and fell back to `HOME="."`. `artifact_path_in_project` now accepts only relative paths of normal components that resolve inside the project (symlinks included); `file_path` outside is rejected, artifact text that is not such a path stays ephemeral. `resolve_post_artifact` returns `Result`: read, directory and write failures of the recovery are returned instead of logged and ignored.
- `rula_project_uid` returned "no uid" for an unreadable or unparsable `rula.toml`, so a target named by uid silently failed to match; now `InvalidRula`.
- The pending-approval map used `.lock().expect(..)` in three places (AGENTS.md §9.1); `lock_approval_map` recovers a poisoned lock with a WARN.
- `discover_project` and `list_project_scripts` treated a `get_running_services` failure as "nothing running"; they return the error.
- `execute_request_user_approval` duplicated the approval wait; it now uses `await_user_approval` (`approval_type: "phase_approval"`), so the pending row is deleted in one place.
- Supervisor `report_agent_failure` and `persist_fallback_agent_response` resolved a project path through `find_registered_project`, which also matches names and uids; they use the exact `registry::find_project_by_path`.
- The project-removal helper that deleted snapshot refs (`registry.rs`, still on `String`) moved to `snapshots::delete_project_snapshot_refs` (`SnapshotError`), next to the other snapshot ref operations. Backend `Result<_, String>` total: 84.
- Pre-existing clippy warnings in `mcp.rs` fixed (2 `needless_borrow`, 1 `manual_strip`).

## Tests
`artifact_paths_must_stay_inside_the_project` (`..`, absolute, symlink escape rejected; new and existing paths accepted), `rula_uid_reports_invalid_files`.

## Verification
`cargo check --all-targets` clean; `clippy` 28 warnings (none in `mcp.rs`); `cargo test --lib`: 62 passed, same 2 pre-existing `kopeng::resolution` failures. Spec 12 §6.7 updated. Not run in the app.
