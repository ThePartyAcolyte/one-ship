# 2026-11-05 — Phase 5 Module 22 (`kopeng/persona.rs` Typed Errors)

Follow-up of `2026-11-05_phase5_github_errors.md`.

## `PersonaError`
New enum with `Serialize`: `InvalidField`, `ReservedId`, `AlreadyExists`, `NotDeclared`, `NoConfig`, `MalformedAgents`, `Task`, `Config`, `Registry` and `Supervisor`. The 2 `Result<_, String>` are gone.

## Fixes
- **Data loss on an unreadable `rula.toml`:** `read_to_string(..).unwrap_or_default()` treated it as empty, so saving a persona rewrote the file with `[agents]` alone, dropping services, tasks and plugins. The file is now read with the strict `read_project_rula`: a missing or invalid file is an error and is never rewritten. Writes go through `write_project_rula`, which validates the result first.
- **Edit dropped keys:** editing replaced the whole `[agents.<id>]` table, losing keys the modal does not show (e.g. `mcp_dialogue_mode`). `update_agent_persona` now writes only the modal fields; a blank field removes its key, and a blank DNA keeps the existing one.
- **Create overwrote silently:** `add_agent_persona` both created and overwrote. It is split into `create_agent_persona` (`AlreadyExists`) and `update_agent_persona` (`NotDeclared`).
- **Reserved IDs:** `git`, `system`, `initializer`, `removal-resolver` and `config-resolver` could be created as personas. They now return `ReservedId`.
- **Runner fallback:** a blank runner became `agy`, and runners were never validated. The runner is now required and checked against the runner manifests (`load_runner_schema`).
- **ID normalization:** the backend lowercased the ID and replaced spaces without saying so. It now validates and rejects, since the modal already normalizes.
- **Deleting an undeclared persona** reported success. It now returns `NotDeclared`.
- **Agent left running:** `App.vue` swallowed a failed `stop_agent` and deleted the persona anyway. `delete_agent_persona` now stops the agent itself and keeps the persona when the stop fails.
- **Main thread:** the commands were synchronous with file I/O on the main thread. They are now async with `spawn_blocking`, take `project_id` and log at `DEBUG` on entry.

## Frontend
- `AddPersonaModal.vue`:
  - emits a typed `AgentPersonaInput` (new in `types/index.ts`);
  - the role is required (the `'Project Specialist'` default is removed), and so is the `|| 'agy'` runner fallback;
  - no `as any`.
- `App.vue`:
  - calls create or update by modal mode;
  - no separate `stop_agent`;
  - the persona refs are typed.

## Docs
- Spec 10: persona commands, reserved IDs, and `[agents.git]` as a hand-written cascade override.
- `rula_schema.md`: reserved IDs, ID format and runners.
- Open contradiction, unchanged: spec 10 shows `[agents.git]` as a cascade override, which the supervisor does read, while `rula_schema.md` says never to define it.

## Verification
- `cargo check --all-targets` clean.
- `clippy`: 17 warnings, none in `persona.rs`.
- `cargo test --lib`: 100 passed, 3 of them new (create vs update, update keeps unmanaged keys and DNA, reserved / malformed IDs and unknown runners). The same 2 `kopeng::resolution` failures remain.
- `vue-tsc` clean.
- `Result<…, String>`: 56.
- Not run in the app.
