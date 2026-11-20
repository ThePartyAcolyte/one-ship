# 2026-11-04 — Phase 5 Module 19 (`orchestrator.rs` + `port_inspect.rs` Typed Errors)

Follow-up of `2026-11-03_phase5_initializer_errors.md`.

## Errors
- `OrchestratorError` (`Serialize`): `PortOccupantChanged`, `Task`, `Registry`, `Script`, `Supervisor`, `PortInspect`. `CommandResult` (its `success` was always `true`) is removed.
- `PortInspectError`: `ProcRead`, `Malformed`, `OwnerNotVisible`, `ProtectedProcess`, `Signal`, `ReleaseTimeout`, `Task`. `SupervisorError` gains `ProcInspection`.
- `hydration::ScriptError`: `NoConfig`, `NotDeclared`, `MissingCommand`, `UnknownOption`, `InvalidOption`, `Config`.

## Service commands (maintainer decisions: typed start outcome, declared defaults)
- **`start_service(project_id, service_id, options)`:**
  - It took `project_path` and ran the `command` / `args` / `cwd` sent by the frontend without checking `rula.toml`. The frontend also expanded the option flags.
  - Now the service is read from `[scripts.persistent.<id>]` (`hydration::find_declared_script`, strict `read_project_rula`), and `hydration::expand_script_options` expands the options. An omitted option takes its declared `default`, where before it added nothing. Unknown keys, wrong types and values outside `choices` are errors.
  - The helper is shared so that `runner.rs` can use it for tasks.
- **Port conflict:** the UI detected it with a regex on the error text. `start_service` now returns `ServiceStartOutcome` (`started { pid }` | `port_occupied { port, pid, cmdline }`).
- **`stop_service(project_id, service_id)`:** the unused `project_name` is gone.
- **`release_port(port, target_pid)`:**
  - `target_pid` is required; without it, anything on the port was killed.
  - The 3000 ms timeout and the 50 ms step are now `orchestrator.port_release_timeout_ms` / `port_release_poll_step_ms`, shown in Settings.
  - The `/proc` scan runs on the blocking pool.
- **Removed commands:** `get_running_services` and `check_port_occupant`, which had no frontend caller.
  - `get_running_services` read every project's `rula.toml` on the main thread, skipped invalid ones silently, and was used by MCP for a single project.
  - `ProcessSupervisor::running_service_ids(project_path)` replaces it: `Working` `svc` processes from the live registry, shells excluded. The state watcher uses it too, so it no longer reads `rula.toml` per project per tick, and a crashed service no longer counts as running.
- **MCP `manage_project_script`:** uses the same functions. A restart is a start, because `spawn_service` replaces the running instance, so the stop + sleep is gone. A port conflict tells the agent to have the user release the port.
- **`approval_poll_delay_ms` removed:** its only use was that restart sleep, and the approval flow never read it.
- `spawn_service`: `let _ = self.stop_process(..)` → `?`.

## `port_inspect.rs`
- `find_port_listener` now returns `Result<Option<_>>`. Each of these used to be reported as a free port and is now an error:
  - an unreadable `/proc/net/tcp`;
  - a listening socket whose owner cannot be inspected (`OwnerNotVisible`).
- `/proc/net/tcp6` may be absent (IPv6 disabled).
- An unreadable PGID used to fall back to the PID, so `kill(-pid)` targeted a group that may not exist. It is now an error, and so is an unreadable environment.
- `terminate_port_occupant`:
  - checks `kill` errors (`ESRCH` = already gone);
  - refuses an occupant in ONE-SHIP's own PID or process group. Before, a service declaring the daemon's port could make the UI kill the daemon.
- `scan_proc_for_oneship_run_ids` returns `Result`: an unreadable `/proc` used to skip orphan detection silently. Per-process unreadable environments are still skipped (other users, exited processes).
- The non-Linux stubs are removed (single-host target).

## Shells (found while updating the terminal grid)
- **Shell selection bug:** selecting a `shell_<ts>` session matched no target branch and fell through to the `system` target. Output showed the supervisor and stdin went to `system`.
- **Interrupt bug:** the interrupt button stopped the non-existent legacy `shell` ID.
- **Orphaned shells:** the session list was cleared on a project switch and lost on unmount (tab change) while the shells kept running.
- Fixes:
  - New `list_interactive_shells(project_id)`; the grid reloads it on mount, project switch, launch and close.
  - `launch_interactive_shell(project_id, shell_id)` requires the `shell_` prefix, so a shell cannot replace a declared service. The `"shell"` fallback and the `/bin/bash` fallback for a missing `$SHELL` are removed.
  - A failed shell close is reported instead of being ignored.

## Frontend
- `ProjectTerminalGrid.vue`: the new commands. Arguments are no longer built in the frontend. Stop failures go to the status log. Touched handlers use `unknown` errors.
- `types/index.ts`: `ScriptOption`, `ScriptEntry`, `ScriptOptionValues`, `ServiceStartOutcome`.
- Deleted dead code (maintainer approval: "borra todas"):
  - `src/kopeng/services/` (an empty deprecated `ScriptsSection.vue` and a no-op `ServicesKopengPlugin`, no importer);
  - the 14 `templates/skills/kapawu-*` directories, copies from before the rename to `oneship-*` that nothing referenced. The `kapawu` account salt and snapshot message prefix are identifiers and stay.
  - `fideiuris` has its own copies (not links) of five `kapawu-*` skills, so nothing broke there; it was left untouched.
- Spec 24: platform skill path `oneship-*`.

## Docs
- Spec 01: option expansion, shell IDs, service commands.
- Spec 08:
  - §2.4 port inspection;
  - the command table (`launch_interactive_shell`, `list_interactive_shells`, `release_port`, and `dispatch_git_instruction`, still on `project_path` from the previous session);
  - §4 watcher.
- Spec 12: `manage_project_script`.
- `oneship-core` skill: option `flag` / `default` rules.

## Verification
- `cargo check --all-targets` clean.
- `clippy`: 17 warnings (was 20), none in the touched modules.
- `cargo test --lib`: 93 passed. 6 are new: option defaults/overrides/rejections, start-outcome serialization, environ parsing, own-group refusal. The same 2 `kopeng::resolution` failures remain.
- `vue-tsc` clean.
- `Result<…, String>`: 65 with a grep that includes nested generics (`Result<Vec<…>, String>`). The earlier sessions' "48" missed them.
- Not run in the app.
