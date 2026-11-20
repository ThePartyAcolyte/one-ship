# 2026-11-03 — Phase 5 Module 16 (`kopeng/flag_resolver.rs` Typed Errors)

Follow-up of `2026-11-03_phase5_rea_errors.md`.

## `FlagResolverError`
New enum with `Serialize`: `FlagNotInProject`, `FlagNotPending`, `UnknownFlagType`, `MissingDocsOption`, `InvalidDocsOption`, `OptionNotApplicable`, `Serialization`, `Task`, `Registry`, `Database`, `Skill`, `Snapshot`, `Supervisor`. Both Tauri commands return it (the 4 `Result<_, String>` are gone).

## Fixes
- Launch contract: `launch_flag_resolution_agent` took `project_path`, `project_name`, `flag_type` and `metadata_json` from the frontend and never read the flag, so a wrong or stale argument launched an agent for a flag of another type, another project, or one already closed. The signature is now `(project_id, flag_id, chosen_option?, cleanup_strategy?, runner?, model?, effort?)`. The flag is read with the new `db::db_get_init_flag`. It must belong to the project and be pending. Its type and metadata come from SQLite, and the name and path from the registry.
- Defaults: a missing `chosen_option` became `"sdd"` and an unknown archetype silently became `oneship-docstyles`. `cleanup_strategy` became `"preserve"` and metadata became `"{}"`. Now `docs` requires a listed archetype and cleanup strategy. Other flag types reject both, so the frontend no longer sends `chosenOption: flag.flag_type`. Absent metadata reaches the prompt as `null`.
- `custom` archetype: the modal advertised `oneship-docstyles-crafter`, but the backend injected `oneship-docstyles`, so the crafter skill never reached the agent. It now maps to the crafter, with `oneship-docstyles` as the base.
- The opening user turn was inserted with `let _ =` after the spawn. It is now recorded before the spawn, and a failed insert stops the launch. A failed stdin delivery of the prompt (deferred task) is logged at `ERROR` instead of being discarded.
- Removed the duplicate `stop_process` (`spawn_agent` stops the earlier instance) and the ad-hoc path existence check (the registry lookup replaces it).

## Frontend
- `ProjectCard`, `ProjectOverviewTab`, `ConversationPanel`: the new arguments. In the touched handlers, `catch (err: any)` became `unknown` and `flag: any` became the flag type.
- `DocstyleSelectionModal`: the "Pre-Resolution Safety Snapshot" toggle was never sent to the backend, so it had no effect. It is removed (maintainer decision): the snapshot is mandatory for projects under git. The unused `skillName` (a duplicate of the backend mapping) is also gone.
- `ProjectVisualBuilderTab`: "Configure Script in Chat" launched an agent for flag id `0`, which does not exist, so the agent could never close it. The button is removed (maintainer decision). The Visual Builder is only visible once the stack is resolved, and State C only states what `rula.toml` lacks.

## Docs
Spec 24 §5.2 has a new launch-contract item 0, and the docs item lists the injected skills accurately. Spec 27 §State C notes that it launches no agent.

## Verification
- `cargo check --all-targets` clean.
- `clippy`: no warnings in `flag_resolver.rs`; 22 in total.
- `cargo test --lib`: 77 passed, including 2 new tests (docs options and the flag type mapping). The same 2 `kopeng::resolution` failures as before remain.
- `vue-tsc` clean.
- Backend `Result<_, String>`: 56.
- Not run in the app.
