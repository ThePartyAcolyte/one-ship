# 2026-11-03 — Phase 5 Module 18 (`kopeng/initializer.rs` Typed Errors)

Follow-up of `2026-11-03_phase5_git_errors.md`.

## `InitializerError`
New enum with `Serialize`: `InvalidProjectName`, `ParentNotDirectory`, `TargetNotEmpty`, `NotInitializing`, `Incomplete`, `Io`, `Task`, `Registry`, `Config`, `Database`, `Skill`, `Snapshot`, `Supervisor`, `Git`, `Rea`. Both Tauri commands return it (the 4 `Result<_, String>` are gone).

## Initialization closure (maintainer decision: explicit MCP tool)
- **Bug:** `post_agent_response` from the initializer marked the initialization `completed` as soon as `rula.toml` existed. It then removed the skill and killed the agent. A greenfield REA lab, whose scaffold writes `rula.toml`, was closed by its first scoping reply, and a retry or re-init of a configured project closed on its first response.
- **New MCP tool `complete_project_initialization`** (initializer only, caller's project). It requires a running init, a valid `rula.toml` with `[project].id`, and an initializer journal entry (Genesis). It records `completed`, removes `oneship-initializer`, emits `agent-status-report` / `agent-step-update`, then stops the agent. `initializer::complete_initialization` holds the logic.
- The auto-detection in `mcp.rs` is removed. An initializer response only sets `PROPOSAL_PRESENTED_PHASE`, a shared constant compared exactly (formerly `contains("Proposal presented")`).
- The prompts (brownfield, greenfield) and the `oneship-initializer` skill instruct the agent to call the tool as its final action. The greenfield prompt also says to extend an existing `rula.toml`, such as the REA scaffold.

## Fixes
- **Scoping and mode:** `initialize_project_agent(project_id, mode: InitMode, …)`. The project must be registered, and name and path come from the registry. `mode` was a free string, and any unknown value ran **unattended**, the mode that writes files without approval; it is now an enum and unknown values are rejected. The unused `_auto_approve` parameter is gone.
- **Project uid:** an unreadable or invalid `rula.toml` silently generated a new uid, which changed the project's identity. It is read with `config::read_project_rula`: an invalid file stops the launch (repaired via `[! CONFIG]`). The legacy `uid` alias is gone.
- **Swallowed errors:** the git checkpoint (`get_git_head` → `snapshots::head_commit`), the init-state read (`.ok().flatten()`) and the upsert (`let _ =`) now propagate. `is_web_builder` scanned every project with `unwrap_or(false)`; it now comes from the registry entry. A failed prompt delivery is logged at `ERROR`. The duplicate `stop_process` is removed.
- **`write_init_status`:** it wrote `last_agent_status.json`, falling back to `.` when no config dir existed, with an `unwrap` on the clock and its errors discarded. The initializer already emits `agent-status-report`, so the file only duplicated it. It is removed; the global file mechanism is in TODO for the supervisor module.
- **Watcher:**
  - The 15-minute constant became `orchestrator.init_timeout_minutes`, registered and shown in Settings.
  - The poll step and the prompt delay come from `agent_spawn_poll_step_ms` (the delay was a fixed 250 ms).
  - The five duplicated closing blocks are now `fail_initialization`.
  - Exit with code 0 plus an existing `rula.toml` used to count as success. An exit, failure or vanished process before completion is now a failure, re-checked against the DB to avoid racing the closing tool.
  - `Unlaunched => break` exited silently; it now marks the initialization failed.
- **Greenfield:**
  - The names `.` and `..` are rejected; `..` used to create the project outside the parent.
  - The parent must be a directory.
  - A target that is a file used to pass because the `read_dir` error was ignored.
  - `db_update_project_web_builder` used `let _`.
  - `init_git`, `enable_web_builder` and `enable_rea` are plain `bool`s; the frontend always sends them.
- **REA lab `rula.toml`:** it was hand-written with `format!`, so a `"` in the name broke it. It used `[[agents]] id = "reverser"`, a non-canonical array that config and persona code never read. It defaulted `runner` to `"agy"`. It is now a `toml::Value` with `[project]` (`id`, `name`, `oneship_version`), `[plugins.rea]` and `[agents.reverser]` (runner only when chosen), written through `config::write_project_rula` (validated).

## Frontend
`ProjectInitModal.vue`: `projectId` replaces `projectPath`/`projectName`, and `autoApprove` is dropped. `AddProjectModal.vue` needs no change.

## Docs
- Spec 12: the new tool and caller-scoped tools.
- Spec 24 §4.4: closure and watcher, plus a new launch contract item 5; §5.3 skill purge.
- Spec 26: greenfield closure.
- Spec 18: status reports go out as events only.
- TODO: a runtime check.

## Follow-ups closed in the same session
At first these were deferred to the `supervisor.rs` module. The maintainer asked why they were left open, so they were closed here:
- **`last_agent_status.json` removed:**
  - It was a single global file without project segregation, written by `mcp::execute_report_task_status` and polled by the supervisor's agent state watcher, which seeded and re-read it every tick and re-broadcast it as `agent-status-report`.
  - `report_task_status` now emits the event directly on the same broadcast channel.
  - The watcher's seed and poll are gone. The `get_last_agent_status` command, which had no frontend caller, is deleted and unregistered.
  - Spec 18 updated.
- **Init-state updates in the stream readers:** the OpenCode, AGY and Claude readers used `if let Ok(Some(mut st))` and silently dropped a DB error or a missing state. They now call `initializer::record_initializer_session`, which logs both at `ERROR`.
- **`launch_agent` init lookup:** the initializer resume lookup propagates the DB error (`?`).
- **`compute_project_state`:** it cannot fail (watcher tick), so it now logs an unreadable init state at `ERROR` instead of `.ok().flatten().unwrap_or(false)`.
- **`dispatch_git_instruction(project_id, instruction)`:** the project comes from the registry, replacing `project_path` / `project_name` from the frontend (`App.vue` voice dispatch, `ProjectGitSnapshotsTab.vue`). It still returns `String`, until the supervisor IPC boundary is moved to `Serialize`.

## Verification
- `cargo check --all-targets` clean.
- `clippy`: the only warning in `initializer.rs` is `too_many_arguments` on `create_greenfield_project`, a Tauri command (existing); 20 in total. Verified again after the follow-ups.
- `cargo test --lib`: 87 passed (3 new: names, mode enum, REA rula). The same 2 `kopeng::resolution` failures remain.
- `vue-tsc` clean.
- Backend `Result<_, String>`: 48.
- Not run in the app.
