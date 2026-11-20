# Agent Session Snapshots

## Purpose

Provide an automatic, zero-token safety net for recovering code and domain assets modified by autonomous agents or external domain editors without polluting the project's official Git commit history, branches, or staging index.

## Scope

In scope: `src-tauri/src/kopeng/snapshots.rs`, hidden Git ref namespace `refs/one-ship/snapshots/<project_id>/`, per-domain categorization (`code`, `pcbcraft`, `solidcraft`, `desigma`) and retention pruning, private temporary Git index algorithm, SQLite `snapshots` table, MCP tools (`create_snapshot`, `list_snapshots`, `restore_snapshot`), daemon commands, `[plugins.vcs]` / domain plugin triggers, and `ProjectGitSnapshotsTab.vue`.

Out of scope: normal user-controlled Git staging/branching/commits (see [Git Workspace & Branches](../03-projects/02-git-workspace-and-branches.md)) and project removal cleanup of snapshot refs (see [Project Registry, Scripts & Service Orchestration](../03-projects/01-project-registry-and-service-orchestration.md)).

## Model / Design

### 1. Event-Driven & Category-Scoped Ref Architecture

```text
main ──► A ──► B  (official commits, user-controlled, never touched)

refs/one-ship/snapshots/<project-id>/
  ├── <ts_ms>-<conv_id>-milestone-<journal_id>   ← domain: "code" (recorded with session minutes)
  ├── <ts_ms>-manual-file-save-<filename>        ← domain: "code" (recorded on document save)
  ├── <ts_ms>-manual-pcb-save-main-board         ← domain: "pcbcraft" (asset_key: "main-board")
  ├── <ts_ms>-manual-cad-edit-enclosure          ← domain: "solidcraft" (asset_key: "enclosure.sc")
  └── <ts_ms>-manual-design-edit-ui              ← domain: "desigma" (asset_key: "ui.desigma")
```

**Core invariants:**
1. **Zero impact on official history:** Commits live strictly under `refs/one-ship/snapshots/<project_id>/`, hidden from `git log` and `git branch -a`, never altering `HEAD`, active branches, or the user's staging index.
2. **Event-driven automation:** Triggered upon persistence milestones (`record_session_minutes`), document saves (`save_project_document`), pre-flag-resolution (`pre-flag-<flag_type>`), project initialization, and domain editor saves (`PCBCraft`, `SolidCraft`, `One-Ship Designer`).
3. **Domain categorization & independent retention:** Four domains (`code`, `pcbcraft`, `solidcraft`, `desigma`) maintain separate retention caps so high-frequency CAD/PCB/Design edits never evict code milestone history.
4. **Partial restore:** Restores either full snapshots or selected file paths via `git restore --source <ref> --worktree -- ...`.
5. **Non-Git projects:** Skipped cleanly when `.git` is absent.

### 2. Automated Triggers & Configuration

1. **Code & milestone checkpoints (`domain = "code"`, `[plugins.vcs]`):**
   - `vcs.snapshots_enabled` (`bool`, default `true`): Master toggle.
   - `vcs.minutes_checkpoint_mode` (`"snapshot" | "commit" | "off"`, default `"snapshot"`): Executed when a primary agent calls `record_session_minutes` (delegated specialists with pending `delegate_agent_task` are blocked from recording minutes and report via `report_task_status`):
     - `"snapshot"`: Creates `"code"` snapshot (`milestone:<journal_id>`) if `snapshots_enabled` is `true` and `detect_changed_code_files` (`git status --porcelain=v1 -z -uall`, excluding domain assets) finds changes.
     - `"commit"`: Validates `.git`, non-detached `HEAD`, and at least one initial commit; resets residual staging index, stages changed code/docs files (excluding domain assets), and commits per `vcs.auto_commit_detail_level` (`title_only` | `title_and_decisions` | `full`). Hook failures roll back the index and journal row and return `isError: true`.
     - `"off"`: Records minutes in SQLite only.
   - `vcs.auto_snapshot_on_file_change` (`bool`, default `true`): Creates scoped `"code"` snapshot (`file-save:<rel_path>`) on `save_project_document`.
   - `vcs.auto_snapshot_on_init` (`bool`, default `true`, global-only): Genesis/pre-flag safety snapshot (`pre-flag-<flag_type>`).
   - `vcs.max_per_project_count` (`number`, default `50`): Retention cap for `"code"` snapshots.
2. **Domain asset snapshots (`pcbcraft`, `solidcraft`, `desigma`):**
   - Configured per plugin (`[plugins.pcbcraft]`, `[plugins.solidcraft]`, `[plugins.desigma]`) via `try_create_auto_domain_snapshot`:
   - `auto_snapshot_on_edit` (`bool`, default `true`): Fires on MCP domain saves (`save_project`) or debounced filesystem edits to `.pcbcraft`/`.pc`, `.sc`/`.step`/`.3mf`, or `.desigma`/`.fig`.
   - `max_snapshots_per_asset_count` (`number`, default `20`): Retention cap per `(project_id, domain, asset_key)` pruned via `prune_domain_asset_snapshots`.

### 3. Atomic Creation Algorithm (`create_domain_snapshot`)

Executed on the blocking pool in `oneship-daemon`:

```text
fn create_domain_snapshot(
    project_path, project_id, conversation_id?, agent_id, label,
    explicit_files?, domain, asset_key?, max_allowed
) -> Snapshot:

  0. Prune oldest snapshots in the target category:
     - If domain == "code": prune_excess_snapshots(project_path, project_id, max_allowed)
     - Else: prune_domain_asset_snapshots(project_path, project_id, domain, asset_key, max_allowed)

  1. Scope: explicit_files, else deterministic Git diff (`detect_changed_code_files`) for `domain == "code"`, else working tree.
     Files on disk or tracked deletions in `HEAD` are kept. When `domain == "code"`, Design/PCB/CAD domain assets are
     filtered out (and excluded via `:(exclude)` pathspecs when capturing the whole working tree).

  2. Private index (GIT_INDEX_FILE=<git-dir>/oneship-snapshot-<uuid>.index):
     git read-tree HEAD          (git read-tree --empty when the repo has no commits)
     git add -A [-- <scoped files>]
     tree = git write-tree

  3. commit = git commit-tree <tree> [-p HEAD]
              -m "oneship-snapshot: <label> | domain:<domain> | conv:<conv> | agent:<agent>"

  4. ref = "refs/one-ship/snapshots/<project_id>/<unix_ts_ms>-<conv>-<label>"
     (validated with git check-ref-format)
     git update-ref <ref> <commit> ""

  5. Insert SnapshotRecord into SQLite `snapshots` table (id = first 12 chars of commit).
```

### 4. UI Cockpit (`ProjectGitSnapshotsTab.vue`)

- **Category filter pills:** `All`, `Code`, `PCB`, `CAD`, and `Design`, rendering domain and `asset_key` badges.
- **Dual-gutter diff & granular restore:** Compares snapshot files against the working tree or the previous `milestone:*` snapshot (`compareWith`), supporting single-file or full-snapshot restoration and deletion.

## Contracts

### Database Schema (`oneship.db`)

```sql
CREATE TABLE snapshots (
    id              TEXT NOT NULL,
    ref_path        TEXT PRIMARY KEY,
    project_id      INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    conversation_id TEXT NOT NULL,
    agent_id        TEXT NOT NULL,
    label           TEXT NOT NULL,
    files_changed   TEXT,
    created_at      TEXT NOT NULL,
    domain          TEXT NOT NULL DEFAULT 'code',
    asset_key       TEXT
);

CREATE INDEX idx_snapshots_project      ON snapshots(project_id);
CREATE INDEX idx_snapshots_conversation ON snapshots(conversation_id);
CREATE INDEX idx_snapshots_domain_asset ON snapshots(project_id, domain, asset_key);
```

### MCP Tools (`src-tauri/src/mcp.rs`)

- **`create_snapshot`:** `label` (`string`, required), `project` (`string`, optional), `conversation_id` (`string`, optional), `explicit_files` (`string[]`, optional).
- **`list_snapshots`:** `project` (`string`, optional), `conversation_id` (`string`, optional), `agent_id` (`string`, optional), `limit` (`number`, optional, default `50`).
- **`restore_snapshot`:** `snapshot_id` (`string`, required — exact `id` or `ref_path` belonging to project), `project` (`string`, optional), `files` (`string[]`, optional).

### Daemon Commands (`api::API_COMMANDS` / `daemonCall`)

| Command | Arguments | Description |
| :--- | :--- | :--- |
| `create_project_snapshot` | `{ projectId, agentId, label, explicitFiles? }` | Creates manual snapshot |
| `list_project_snapshots` | `{ projectId, conversationId?, agentId?, limitCount }` | Lists persisted project snapshots |
| `get_snapshot_diff` | `{ projectId, snapshotId, compareWith?, filePath? }` | Computes diff vs. working tree or milestone |
| `restore_project_snapshot` | `{ projectId, snapshotId, files? }` | Restores full or file-scoped snapshot |
| `delete_project_snapshot` | `{ projectId, snapshotId }` | Deletes Git ref and SQLite row |

*Note:* Errors serialize as `SnapshotError` in `ApiErrorResponse`.

### Configuration Keys

- `[plugins.vcs]`: `snapshots_enabled` (`true`), `minutes_checkpoint_mode` (`"snapshot"`), `auto_commit_detail_level`, `auto_snapshot_on_file_change` (`true`), `auto_snapshot_on_init` (`true`, global-only), `max_per_project_count` (`50`).
- `[plugins.pcbcraft]`, `[plugins.solidcraft]`, `[plugins.desigma]`: `auto_snapshot_on_edit` (`true`), `max_snapshots_per_asset_count` (`20`).

## Implementation Status

| Item | Status | Evidence |
| :--- | :--- | :--- |
| Hidden ref snapshot engine, domain pruning & `SnapshotError` | Implemented | `src-tauri/src/kopeng/snapshots.rs` |
| `snapshots` SQLite table & domain indexes | Implemented | `src-tauri/src/db.rs` |
| MCP tools (`create_snapshot`, `list_snapshots`, `restore_snapshot`) | Implemented | `src-tauri/src/mcp.rs` |
| All 5 snapshot daemon commands in `API_COMMANDS` | Implemented | `src-tauri/src/api.rs` |
| `[plugins.vcs]` snapshot config keys | Implemented | `src-tauri/src/config.rs` |
| `ProjectGitSnapshotsTab.vue` UI | Implemented | `src/components/workspace/ProjectGitSnapshotsTab.vue` |

## Related Specs

- [Thread, Conversation, and Runner-Neutral Context Continuity](02-thread-conversation-and-continuity.md)
- [Git Workspace & Branches](../03-projects/02-git-workspace-and-branches.md)
- [Project Registry, Scripts & Service Orchestration](../03-projects/01-project-registry-and-service-orchestration.md)
- [Configuration Schema and Cascade](../01-architecture/02-configuration-schema-and-cascade.md)

Supersedes legacy: `16_agent_session_snapshot_system.md`
