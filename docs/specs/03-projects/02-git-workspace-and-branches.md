# Git Workspace, Branches & Snapshots Cockpit

## Purpose

Define how ONE-SHIP presents Git state (dual-signification status indicator), the unified **Git & Snapshots** workspace tab with diff viewer, branch lifecycle, and `rula.toml`-driven quick action presets.

## Scope

In scope: status indicator encoding, VCS action presets, the Git & Snapshots cockpit, working-tree/commit telemetry, Git command contracts, branch commands and modal, upstream actions in Overview.

Out of scope: remote/provider integration ([Remote Repositories & Diagnostics](04-remote-repositories-and-diagnostics.md)), forge collaboration ([Forge Provider Abstraction](05-forge-provider-abstraction.md)), logging ([System Logging & Status Terminal](03-system-logging-and-status-terminal.md)).

## Model / Design

### 1. Dual-Signification Git Status Model

The Git status indicator balances glanceability with technical precision.

```text
┌─────────────────────────────────────────────────────────────┐
│  (●) master       ! 2   ? 1   + 3   ↑ 1   ↓ 0               │
│   ▲     ▲         ▲     ▲     ▲     ▲     ▲                 │
│   │     │         │     │     │     │     └── Behind upstream
│   │     │         │     │     │     └──────── Ahead upstream
│   │     │         │     │     └────────────── Staged files
│   │     │         │     └──────────────────── Untracked files
│   │     │         └────────────────────────── Modified working tree
│   │     └──────────────────────────────────── Active branch name
│   └────────────────────────────────────────── Topology & Health Icon
└─────────────────────────────────────────────────────────────┘
```

- **Icon topology:** `<GitCompare />` = remote tracking branch exists (`upstream` configured); `<GitBranch />` = local-only repository (no remote).
- **Color health:** Green (`#22c55e` / theme accent) = clean (`is_clean: true`); Amber (`#f59e0b`) = dirty (modified, staged or untracked files); Dim grey (`#71717a`) = unborn repository (initialized, zero commits).
- **Symbolic metrics:** `! N` unstaged modified; `? N` untracked; `+ N` staged; `↑ N` ahead of upstream; `↓ N` behind upstream.

### 2. Dynamic Action Presets (`rula.toml`)

Actions are loaded strictly from `rula.toml` (no hardcoded mocks):

```toml
[project]
name = "one-ship"

[[plugins.vcs.actions]]
name = "Auto-Commit"
prompt = "Analyze git diff and create a clean semantic commit"

[[plugins.vcs.actions]]
name = "Commit & Push"
prompt = "Commit pending changes and push to current branch remote"
```

**Split-button UX (`Name` vs `»`):** each preset pill has a left segment (action name) that inserts the prompt into the instruction textarea for inspection/refinement, and a right segment (`»`, `<ChevronsRight />`) for **instant dispatch** to the background `git` agent, closing the modal.

### 3. Unified Git & Snapshots Cockpit

The former isolated `VcsActionModal` and standalone `Snapshots` modal are unified into the persistent **`Git & Snapshots`** tab (`workspaceTab = 'git'`).

1. **Unified timeline (left panel, `w-80`):** filter toggles `All`, `Snaps (N)`, `Commits (M)` (plus a domain filter when Design/PCB/CAD snapshots exist); a persistent `WORKING TREE` item at top showing the count of uncommitted/modified/untracked files; a chronological timeline interleaving ONE-SHIP safety snapshots and Git commits.
2. **Inspector header (`h-9 ui-surface-title`):** left: `GitBranchSelector` + active item metadata (`WORKING TREE` staged count, snapshot ID/label/agent, or commit hash/author/message). Right (Working Tree mode): quick-action buttons from `[[plugins.vcs.actions]]`, a `+` submodal to add actions, manual `SYNC` (`pull --rebase` + `push`), and `OPEN AGENT` (launches and opens chat for the project's `plugins.vcs.default_agent_id`).
3. **Resizable staging & files pane (`220px–560px`, default `288px`):**
   - *Commit composer (Working Tree mode):* `AppTextarea` for the message; AI commit-message generator (`generate_commit_message` via `GIT_AGENT_KEYS` using `stream-json`/`acp` over the staged diff); manual `COMMIT` (`commit_git_changes`).
   - *Staged Changes & Changes:* per-file and bulk `+` (`stage_git_files`) and `-` (`unstage_git_files`). Rows use a two-line layout (filename + relative directory) with `overflow-x-hidden`, rendering up to `MAX_RENDERED_FILES_COUNT = 250` rows per list to keep WebKit responsive with large untracked directories.
   - *Asset tracking without diff bloat:* Design, PCB and CAD assets (`is_domain_asset`) stay listed, stageable and committable but suppress diff rendering.
4. **Dual-gutter live file diff (right panel):**
   - *No automatic global diff:* when `selectedFile` is `null` (or `is_domain_asset` is true) it renders `AppEmptyState` without running a full-repo diff.
   - *Line numbering:* unified diff hunk headers (`@@ -oldStart,oldLen +newStart,newLen @@`) are parsed into structured line models with dual gutters (`oldLineNo`, `newLineNo`).
   - *Reader integration & deletion safeguards:* files existing on disk open in the Reader/Editor (`Explorer` tab) via `workspaceLayoutStore.navigateTo({ tab: 'files', filePath })`. Files deleted on disk (detected via `p.join(&path).exists()`) show a strike-through filename, disabled reader buttons with tooltips, and a `[ DELETED ON DISK ]` badge in the inspector header.

### 4. Working-Tree Auditing & Untracked Diff Generation

- **`WorkingTreeFile` / `CommitFileChange` telemetry:**
  - `get_working_tree_files(project_id)` runs `git status --porcelain=v1 -z -uall` (exact paths; renames carry `old_path`) and checks physical disk existence (`symlink_metadata`).
  - `get_commit_files_changed(project_id, commit_hash)` runs `git diff-tree --root -z` (a root commit lists its files). Codes `A`/`D`/`M`/`R`/`C`/`T` map to `added`/`deleted`/`modified`/`renamed`/`copied`/`type_changed`; any other code is an error.
  - Status badges: `[D]` deleted (absent on disk or `D` in index/tree; red strikethrough), `[U]` untracked, `[M]` modified (staged or unstaged), `[A]` added (staged new), `[R]` renamed (source path in hover tooltip).
- **Untracked diff support:** `get_working_tree_diff(project_id, file_path?)` is one `git diff --cached` over a private index seeded from HEAD (the snapshot mechanism, `snapshots::diff_working_tree`) after `git add -A [-- file]`. Untracked files are included, `.gitignore` is honoured, a repository without commits diffs against the empty tree, and the user's index is untouched. There is no per-file `--no-index` loop and no fallback command.

### 5. Branch Lifecycle

Available on all Git-tracked projects regardless of whether project management is enabled.

- Rendered as a clickable branch badge under the project title in `CardHeader` (and a compact trigger in the Explorer sidebar of `ProjectWorkspace`), opening `GitBranchModal` (`GitBranchSelector.vue`, `GitBranchModal.vue`):
  - **Local Branches** as the primary switching list.
  - **Origin Remote Branches**: collapsible secondary section, excluding local duplicates and strictly excluding third-party `upstream/*` refs from switching.
  - **Create Branch** form with optional base point (`Current HEAD`, or `upstream/<default>` when an upstream exists).
- **Overview Git actions (`ProjectOverviewTab`):** `Fetch Upstream` (`sync_upstream_fetch`) and `+ Add Upstream` (`configure_upstream_remote`) live in the Project Overview next to Git metrics, not in the Project Management tab (see [Forge Provider Abstraction](05-forge-provider-abstraction.md) for their semantics).

## Contracts

### Git command contracts (`kopeng/git.rs`)

- Every git command takes the registered `project_id`, runs in `spawn_blocking`, and returns a typed `GitError`. A project without `.git` is `NoRepository`, never an empty list. Only a repository without commits has an empty log.
- `get_project_git_log(project_id, limit_count)`: `limit_count` is required, `1..=MAX_GIT_LOG_ITEMS_COUNT` (200).
- Commit hashes must be 7–64 hexadecimal characters and are passed after `--end-of-options`, so a hash can never be read as a git option.
- `get_git_status` (project hydration) reads `git status --porcelain=v2 --branch -z` and `remote.origin.url`. `branch` is `null` for a detached HEAD; `has_commits` is false for an unborn branch. A failure leaves `git_status` empty and sets `HydratedProject.git_error`, which the card and rail show as `[! GIT]` (never a guessed `clean` state or an `INIT GIT` button). The remote registry fields (`remotes`, `has_remote`) are defined in [Remote Repositories & Diagnostics](04-remote-repositories-and-diagnostics.md).
- `init_git_repo(project_id)` only runs `git init`. ONE-SHIP never writes `.gitignore` / `README.md`; the initializer agent adapts existing files ([Greenfield Project Creation](10-greenfield-project-creation.md)).
- `save_vcs_action(project_id, name, prompt)` adds a `[[plugins.vcs.actions]]` entry to the project's valid `rula.toml`; the document is serialized and validated before writing. Name and prompt must be non-empty and the name unique. A project without `rula.toml` is an error. `get_vcs_actions` reads the validated `rula.toml`; a malformed action list is an error.
- Other commands: `generate_commit_message`, `commit_git_changes`, `stage_git_files`, `unstage_git_files`, `get_working_tree_files`, `get_commit_files_changed`, `get_working_tree_diff`.

### Branch commands

| Command | Behavior |
| :--- | :--- |
| `list_git_branches(project_id)` | Local branches (`refs/heads`) and remote-tracking refs (`refs/remotes`) as `GitBranchInfo`: `name`, `is_current`, `is_remote`, `remote_name`, `upstream`, `last_commit_hash`, `last_commit_subject`; sorted with the active branch first. |
| `switch_git_branch(project_id, branch_name)` | `git switch <branch>` (or `git switch --track <remote_branch>` for an `origin` branch; legacy wording: `git checkout [--track]`). Never force-discards or auto-stashes; conflicts fail explicitly with `GitError::Command`. |
| `create_git_branch(project_id, branch_name, from_ref?, switch)` | Validates the name via `git check-ref-format --branch`, then runs `git switch -c <name> [start_point]` (`switch = true`, default for manual creation) or `git branch <name> [start_point]` (`switch = false`, `HEAD` untouched). |

## Implementation Status

| Area | Status |
| :--- | :--- |
| Git commands and branch lifecycle (`kopeng/git.rs`, `api.rs`) | Implemented |
| Cockpit UI (`ProjectGitSnapshotsTab.vue`, `MAX_RENDERED_FILES_COUNT` present) | Implemented |
| `rula.toml` VCS actions | Implemented (not re-verified in detail) |

## Related Specs

- [Remote Repositories & Diagnostics](04-remote-repositories-and-diagnostics.md)
- [System Logging & Status Terminal](03-system-logging-and-status-terminal.md)
- [Project Management & Milestones](07-project-management-and-milestones.md)
- [Project Category Index](README.md)

Supersedes legacy: `09_vcs_and_logging_architecture.md` (Git, presets, cockpit portions)
