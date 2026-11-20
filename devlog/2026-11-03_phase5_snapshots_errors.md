# 2026-11-03 — Phase 5 Module 10 (`kopeng/snapshots.rs` Typed Errors, Snapshot Mechanism)

Follow-up of `2026-11-02_phase5_registry_errors.md`.

## Incident: snapshot commits on project branches
`create_snapshot` committed on the current branch, then ran `reset --soft HEAD~1` and `restore --staged .`. Read from the reflogs of the registered repositories:
- On a repository without commits the reset has no `HEAD~1`, so the first snapshot became the initial commit and every later one stayed on the branch. Labels with spaces (`Pre-Resolution: docs (estudiorc)`) are invalid ref names, so `update-ref` failed after the commit and the function returned before the reset; `flag_resolver` discarded that error.
- Result: `estudiorc/master` (7 commits) and `watchtower/master` (3) consisted only of `kapawu-snapshot` commits; `dictate-whisper/develop` has 3 unpushed snapshot commits on top; `l10n_cl_banking/master` has 3 in the middle of its history, already pushed to `origin/master` and `origin/18.0`.
- Every snapshot also ran the project's commit hooks and cleared the user's staging area.

## Repository cleanup (maintainer authorized git for this cleanup)
- `estudiorc`, `watchtower` (local only): `refs/backup/pre-snapshot-cleanup` saved at the old tip (`702ce550`, `c333c02a`), `refs/heads/master` deleted, index emptied (`read-tree --empty`). Both are back to "no commits", with every file untracked in the working tree; nothing on disk changed (15 `kapawu-*` / `.wang-kapawu` files of the last estudiorc snapshot were already absent before the cleanup and remain in the backup ref).
- `gc.reflogExpireUnreachable = never` and `gc.pruneExpire = never` (set by `configure_gc_policy`) removed from all 7 repositories (6 registered projects + one-ship); the empty `[gc]` sections were removed.
- `dictate-whisper` (unpushed tip): `refs/backup/pre-snapshot-cleanup` saved at `130de2da`, `develop` reset `--mixed` to `bd9bf97`; the three snapshot commits are gone from the branch and their content stays in the working tree as uncommitted changes (working tree untouched).
- `l10n_cl_banking`: left as is (maintainer decision; the three snapshot commits are already on `origin/master` and `origin/18.0`).
- The backup refs keep the removed commits reachable; they can be deleted with `git update-ref -d refs/backup/pre-snapshot-cleanup` once the projects are verified.
- `src/components/modals/SnapshotTimelineModal.vue` deleted (unused duplicate of `ProjectSnapshotsTab.vue`, still calling the old command signatures).

## `SnapshotError` and the new mechanism
Decisions (maintainer): private-index mechanism; delete `configure_gc_policy`.
- Creation: private `GIT_INDEX_FILE` seeded from HEAD (empty without commits) → `add -A [-- scoped files]` → `write-tree` → `commit-tree [-p HEAD]` → `update-ref <ref> <commit> ""`. HEAD, branches, the user's index and hooks are untouched. Ref components are reduced to `[A-Za-z0-9_-]` and checked with `check-ref-format`.
- Variants: `NoRepository`, `Git`, `Io`, `NotFound`, `FilesChangedJson`, `InvalidRequest`, `HomeUnavailable`, `Task`, `Registry`, `Database`; `Serialize`; the five commands return it.
- Scoping (security): `restore`, `diff` and `delete` accepted any ref or revision (`delete_project_snapshot(path, "refs/heads/main")` deleted a branch; `get_snapshot_diff_cmd` passed arbitrary arguments to `git diff`, e.g. `--output=<file>`). They now take only the exact `id` or `ref_path` of a snapshot record of the project; an unknown id is `NotFound` (it was used as a raw revision); `db_get_snapshot_by_id_or_ref` (prefix `LIKE … LIMIT 1`) is replaced by `db_find_snapshots_by_id_or_ref` (exact, ambiguity is an error). The Tauri commands take `project_id` only and resolve the path from the registry (they took a path and an id that could belong to different projects) and run on the blocking pool.
- Fallbacks removed: scoped `git add` failure → `git add -A`; three transcript path candidates (now the single `.system_generated/logs/transcript.jsonl`); canonicalization failures → raw paths; unknown snapshot id → raw revision; empty `files` → full restore (now rejected); `"manual"` sentinel for an empty conversation id (now `conversation_id: Option`).
- Behavior fixes: prune ignored git and DB errors and selected conversations with `SELECT DISTINCT … ORDER BY created_at` (undefined order); it now removes whole conversations by earliest snapshot until the new one fits, with a bounded loop. `restore` no longer runs `restore --staged .` (it cleared the user's index). Relative transcript targets are resolved inside the project. `files_changed` JSON errors are reported instead of read as empty.
- Diff semantics: without `compare_with` the diff is snapshot → working tree (was snapshot → HEAD commit, which fails in a repository without commits and does not show what a restore would revert). The UI label reads "Snapshot vs Working Tree".
- Callers: `flag_resolver` and milestone snapshots run only for projects with `.git`; a failed pre-resolution snapshot now stops the resolution. MCP `create_snapshot` passes the conversation id explicitly.
- Dead code removed: `configure_gc_policy`, `compute_files_changed` (test-only), `db_get_pre_snapshot_record` (no caller), the hand-written `chrono_iso_now` calendar (now `chrono`).
- Tests (git, temp repositories): snapshot leaves HEAD and the user's index untouched; scoped snapshot keeps other files at HEAD; repository without commits gets no branch; partial restore changes only the listed files and not the index; ref components are valid.
- Layers: `ProjectSnapshotsTab.vue` (new command arguments), MCP `restore_snapshot` schema, `oneship-snapshots` SKILL.md (§3.4–3.5), spec 16 §3.3/§5.1/§7.3/§8/§9.

## Verification
`cargo check --all-targets` clean; `clippy` no new warnings in the touched code; `cargo test --lib`: 60 passed, same 2 pre-existing `kopeng::resolution` failures; `vue-tsc` clean. Not run in the app.
