# 2026-11-03 — Phase 5 Module 12 (`kopeng/manifest.rs`, `kopeng/migration.rs` Typed Errors)

Follow-up of `2026-11-03_phase5_mcp_helpers_errors.md`.

## Decision (maintainer)
The knowledge export before removal keeps overwriting `TECHNICAL_DEBT.md`, `TODO.md` and `SESSION_JOURNAL.md` (`docs/` when it exists, else the root): these files are imported into the database at onboarding, the database is the source of truth while the project is in ONE-SHIP, and the export hands that truth back to the project when it leaves.

## Root cause found: `rula.toml` was never parsed
`content.parse::<toml::Value>()` (manifest, migration, `skill::evaluate_skill_status`) parses a single TOML *value* in `toml` 1.x and rejects every document; the error was discarded. Consequences: the project version always read `0.2.1`, `schema_version` was always `None`, so every project with a `rula.toml` showed `needs_migration` / outdated, and the purge never found the persona/workflow documents declared in `rula.toml`. All three now use `toml::from_str`.

## `ManifestError`
- Variants: `Io`, `InvalidFile`, `ProjectDirMissing`, `PurgeIncomplete { failures }`, `Task`, `Registry`, `Database`; `Serialize`.
- Export: database and write failures are returned (they were `if let Ok` / `let _ =`), a missing project directory is `ProjectDirMissing` (it returned `Ok` without exporting, so the removal lost the data), and the whole journal is exported (`db_count_project_session_journals`, new; it was capped at 1000 silently).
- Purge now reverts everything ONE-SHIP installs and reports every failure (it warned and returned `Ok`):
  - AGENTS.md: removed if ONE-SHIP created it, the backup restored, else the block stripped (write errors were discarded);
  - CLAUDE.md: the `-> AGENTS.md` link removed and a replaced original restored — `inject_project_mcp_plugin` deleted a project's own `CLAUDE.md` without a backup; it now copies it to `.one-ship/backups/CLAUDE.md.orig` first;
  - `.mcp.json` / `opencode.json`: the `one-ship` and `rea` entries removed; the file deleted when nothing else remains (they were left pointing at the daemon);
  - every platform and specialized skill link (specialized links such as `oneship-docstyles` were left behind; dangling links were not detected because `exists()` follows symlinks);
  - `rula.toml`, `.one-ship/`, declared persona/workflow documents, empty `.agents/` directories (`read_dir` and `remove_dir` errors were discarded).
  - The legacy `.agents/plugins/one-ship` entry is gone from the manifest (`inject_project_mcp_plugin` already removes it).
- `prepare_project_agents_md`: the "created by ONE-SHIP" marker write was discarded (a failure made a later purge keep a ONE-SHIP-created AGENTS.md); the block stripping loop is bounded by the number of start markers.
- `rollback_project_init` accepted any path (purging `rula.toml`/AGENTS.md of an arbitrary directory) and discarded the init-state deletion; it now requires a registered project, propagates both, and runs on the blocking pool.

## `MigrationError`
- Variants: `Io`, `InvalidRula`, `MissingRula`, `Skill` (boxed, `SkillError` ↔ `MigrationError` are mutual), `Snapshot`, `Task`, `Registry`; `Serialize`. `SkillError::ProjectFiles(String)` is replaced by `Manifest(ManifestError)` / `Migration(MigrationError)`.
- `get_project_version` returns `Result`; only an absent key/file means `UNVERSIONED_PLATFORM_VERSION` (`0.2.1`).
- `set_project_version` (line-based, no new dependency): exact `oneship_version` key match (`oneship_version_note` was replaced too), header detection with comments/spacing, a missing `[project]` appended at the end (it was inserted at line 0, moving every root key into `[project]`), and the result is parsed and read back before writing.
- `run_deterministic_migration`: the logged "0.2.1 → 0.2.2 / 0.2.2 → 0.2.3" steps did nothing; `prepare_project_agents_md` and the version stamp ran twice (they are part of `inject_platform_skill`). It now runs `inject_platform_skill`, verifies the recorded version, fails for a project without `rula.toml` (the version could not be recorded but success was reported), lists only real steps, and takes the HEAD checkpoint from `snapshots::head_commit` (git errors were discarded). `MigrationResult.success` (always true) removed. The command requires a registered project and runs on the blocking pool.

## Tests
Purge restores AGENTS.md/CLAUDE.md, cleans runner configs, removes dangling skill links, `.one-ship/` and `rula.toml`; an invalid runner config makes the purge incomplete; managed blocks stripped; migration stamps the current version; version stamp keeps root keys, comments and look-alike keys and appends a missing `[project]`; invalid `rula.toml` reported.

## Verification
`cargo check --all-targets` clean; `clippy` 25 warnings (none in the touched modules); `cargo test --lib`: 67 passed, same 2 pre-existing `kopeng::resolution` failures; `vue-tsc` clean. Backend `Result<_, String>`: 78. Specs 01 §1 and 24 §3/§4 updated. Not run in the app.
