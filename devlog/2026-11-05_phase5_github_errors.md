# 2026-11-05 — Phase 5 Module 21 (`kopeng/github.rs` Typed Errors) and Profiler / Daemon Decisions

Follow-up of `2026-11-04_phase5_runner_errors.md`.

## Decisions (profiler review)
`kopeng/profiler.rs` was reviewed against the intended concept: profiling is continuous and bound to every run, building a history for later optimization, never a manual session. Findings:
- The continuous sampler runs only in the daemon and only sees the daemon's supervisor. Services and agents started from the UI live in the app's supervisor, so they are never sampled.
- Tasks are not sampled.
- Raw samples are pruned after 2 hours.
- The output file of a bound engine (`samply`) is never linked to its run.
- `profile_service` is a manual session that, from the UI, always ends in a 2 s sysinfo sample with synthetic hotspots.

The user decided:
- **Daemon ownership:** the daemon becomes the only supervisor and the only state owner, and the app is only an interface.
  - The frontend talks to the daemon directly (REST + the existing SSE).
  - The app does not open SQLite.
- **Retention:** raw samples are kept without pruning until disk use is a real problem (100 GB+).
- **Engine output:** the output of a bound profiler engine is stored and linked to its run, not parsed yet.
- **Order:** the small Phase 5 modules first, then the daemon migration, then the profiler redesign.

Both are recorded in `TODO.md`. `kopeng/profiler.rs` and the typed IPC boundaries of `conversation` / `supervisor` / `accounts` / `db` move into them.

## `GithubError`
New enum with `Serialize`: `Gh`, `InvalidRepoName`, `InvalidRemoteUrl`, `UnexpectedOutput`, `Task`, `Git` and `Registry`. The 4 `Result<_, String>` are gone. `git::git` is now `pub(crate)` and reused for `git remote`.

## Fixes
- **Main thread blocked:** the commands were synchronous, so `gh repo list` and `gh repo create` (network calls) blocked the app's main thread. They are now async and run in `spawn_blocking`.
- **Search limited to 30 repositories:** the list was capped at a hardcoded 30, and the modal filtered only those 30. The cap is now `vcs.github_repo_list_limit_count` (global, default 1000). The unused backend `query` filter is removed; the modal filters locally.
- **Option injection:**
  - A repository name starting with `-` reached `gh repo create` as an option. Names are now validated against GitHub's format.
  - A remote URL starting with `-` reached `git remote add/set-url` as an option. URLs are now validated.
- **`link_github_remote`:** when `git remote get-url` could not run, it was treated as "no origin". The check now uses `git config --get remote.origin.url` strictly (exit 1 = absent; anything else is an error).
- **Authentication detection:** it guessed by matching "auth" / "login" in stderr. It now returns gh's own stderr, which already says `gh auth login`.
- **`create_github_repo`:** it returned `""` as the URL when gh printed nothing. That is now `UnexpectedOutput`.
- **`get_git_remote_url` removed:** it duplicated `git_status.remote_url` and turned every failure into "no remote", which the modal then swallowed. The modal reads the hydrated project.
- IPC takes `project_id` resolved through the registry, and each command logs at `DEBUG` on entry.

## Frontend
- `GithubRemoteModal.vue`:
  - takes a `ProjectRecord`, sends `projectId`;
  - has no empty `catch` and no `any`;
  - types `viewerPermission` as `string | null`.
- `App.vue`: types the modal project.

## Docs
- Spec 11: v1.1.0 with the signatures, validations, configurable limit and local filtering; "Kapawu" renamed to ONE-SHIP.
- Spec 10: the new key.

## Verification
- `cargo check --all-targets` clean.
- `clippy`: 17 warnings, none in `github.rs`.
- `cargo test --lib`: 97 passed, 2 of them new (name and URL validation). The same 2 `kopeng::resolution` failures remain.
- `vue-tsc` clean.
- `Result<…, String>`: 58.
- Not run in the app.
