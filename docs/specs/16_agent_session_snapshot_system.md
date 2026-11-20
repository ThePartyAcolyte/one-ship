# Spec 16: Event-Driven & Category-Scoped Snapshot System
**Document Version:** 2.0.0  
**Domain:** VCS Safety, Automated Milestone & Asset Checkpoints, Per-Category Retention, File Recovery  
**Maintainer:** ONE-SHIP / Omarchy Development Team

---

## 1. Overview & Motivation

When autonomous agents or external domain editors modify project files, developers need a reliable, zero-friction mechanism to recover previous states without polluting the project's official Git commit history.

The ONE-SHIP Snapshot System provides an automatic, token-free safety net by storing file states as commits on hidden Git refs (`refs/one-ship/snapshots/<project_id>/`) outside the main branch namespace. Rather than coupling snapshots to fragile `PRE`/`POST` session lock pairs (where the `POST` of one turn merely duplicates the `PRE` of the next), snapshots are **event-driven** and **partitioned by category (`domain`)**:

```text
main ──► A ──► B  (official commits, user-controlled, never touched)

refs/one-ship/snapshots/<project-id>/
  ├── <ts_ms>-<conv_id>-milestone-<journal_id>   ← domain: "code" (recorded with session minutes)
  ├── <ts_ms>-manual-file-save-<filename>        ← domain: "code" (recorded on document save)
  ├── <ts_ms>-manual-pcb-save-main-board         ← domain: "pcbcraft" (asset_key: "main-board")
  ├── <ts_ms>-manual-cad-edit-enclosure          ← domain: "solidcraft" (asset_key: "enclosure.sc")
  └── <ts_ms>-manual-design-edit-ui              ← domain: "desigma" (asset_key: "ui.desigma")
```

---

## 2. Core Principles

1. **Zero Impact on Official History:** Snapshot commits are stored exclusively under `refs/one-ship/snapshots/`. They never appear in `git log`, `git branch -a`, or standard Git output, and they never touch `HEAD`, the active branch, or the user's staging index.
2. **Event-Driven Automation (Decoupled from Session Lock Files):** Snapshots are triggered automatically upon critical persistence milestones—when an agent records session minutes (`record_session_minutes`), when files are saved, before flag resolution, or when a domain editor (`PCBCraft`, `SolidCraft`, `One-Ship Designer`) saves an asset.
3. **Domain Categorization & Independent Retention:** Snapshots are categorized into four distinct domains (`code`, `pcbcraft`, `solidcraft`, `desigma`) with independent, configurable retention limits so high-frequency CAD/PCB/Design saves never evict code milestone history.
4. **Partial Restore:** Any snapshot can be restored globally or scoped to a specific subset of files using `git restore --source <ref> --worktree`.
5. **Projects Without Git:** Snapshots are skipped cleanly if the project root has no `.git` directory.

---

## 3. Automated Snapshot Triggers & Configuration

### 3.1 Code & Milestone Checkpoints (`domain = "code"`, `[plugins.vcs]`)
Configured under the unified `[plugins.vcs]` (`Git & Snapshots`) table in `config.toml` / `rula.toml`:
- `snapshots_enabled` (`bool`, default `true`): Master toggle for project snapshots.
- `minutes_checkpoint_mode` (`enum`: `"snapshot"` | `"commit"` | `"off"`, default `"snapshot"`): Controls what happens when a primary/orchestrating agent calls `record_session_minutes` (delegated specialists with a pending `delegate_agent_task` are blocked from recording minutes and must report via `report_task_status`):
  - `"snapshot"`: Automatically creates a `"code"` snapshot (`milestone:<journal_id>`) when `snapshots_enabled` is `true` and Git detects changed non-asset files (`detect_changed_code_files`, runner-agnostic via `git status --porcelain=v1 -z -uall`).
  - `"commit"`: Validates `.git`, non-detached `HEAD`, and at least one initial commit; resets any residual staging index, stages changed code/docs files (excluding domain assets), and executes a real `git commit` formatted per `auto_commit_detail_level` (`title_only` | `title_and_decisions` | `full`). Hook failures roll back the index and the journal row and return the hook output to the agent (`isError: true`).
  - `"off"`: Records session minutes in SQLite without creating a snapshot or Git commit.
- `auto_snapshot_on_file_change` (`bool`, default `true`): Automatically creates a scoped `"code"` snapshot (`file-save:<rel_path>`) when a project document or code file is saved (`save_project_document`).
- `auto_snapshot_on_init` (`bool`, default `true`, global only): Automatically creates a genesis safety snapshot upon project initialization or before flag resolution (`pre-flag-<flag_type>`).
- `max_per_project_count` (`number`, default `50`): Maximum number of `"code"` snapshots retained per project before pruning the oldest conversation/manual snapshots.

### 3.2 Domain Asset Snapshots (`pcbcraft`, `solidcraft`, `desigma`)
Each hardware/design plugin (`[plugins.pcbcraft]`, `[plugins.solidcraft]`, `[plugins.desigma]`) manages its own categorized snapshots via `try_create_auto_domain_snapshot`:
- `auto_snapshot_on_edit` (`bool`, default `true`): Automatically creates a domain-scoped snapshot whenever an agent saves via MCP (e.g., `save_project` in `/mcp/pcbcraft`) or when the debounced filesystem watcher detects a modification to a `.pcbcraft`/`.pc`, `.sc`/`.step`/`.3mf`, or `.desigma`/`.fig` asset.
- `max_snapshots_per_asset_count` (`number`, default `20`): Maximum number of snapshots retained per `(project_id, domain, asset_key)` group before `prune_domain_asset_snapshots` prunes the oldest refs for that asset.

### 3.3 Snapshot Creation Algorithm (`create_domain_snapshot`)

Executed atomically on the blocking pool within `oneship-daemon`:

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

---

## 4. Database Schema (`oneship.db`)

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

---

## 5. MCP Tool Definitions (`src-tauri/src/mcp.rs`)

### 5.1 `create_snapshot`
Creates a labeled snapshot of the current project state on demand (e.g. before risky refactors).
- **Arguments:** `label` (`string`, required), `project` (`string`, optional), `conversation_id` (`string`, optional), `explicit_files` (`string[]`, optional).

### 5.2 `list_snapshots`
Lists available snapshots for the project with optional filters (`project`, `conversation_id`, `agent_id`, `limit`).

### 5.3 `restore_snapshot`
Restores a project's working tree to a snapshot state, optionally restricted to a specific `files` array (`git restore --source <ref> --worktree -- ...`).
- **Arguments:** `snapshot_id` (`string`, required — exact `id` or `ref_path` belonging to the project), `project` (`string`, optional), `files` (`string[]`, optional).

---

## 6. Daemon Commands & UI Surface (`ProjectGitSnapshotsTab.vue`)

1. **Daemon Commands (`api.rs`, Spec 12 §7.2):**
   - `create_project_snapshot({ projectId, agentId, label, explicitFiles? })`
   - `list_project_snapshots({ projectId, conversationId?, agentId?, limitCount })`
   - `get_snapshot_diff({ projectId, snapshotId, compareWith?, filePath? })`
   - `restore_project_snapshot({ projectId, snapshotId, files? })`
   - `delete_project_snapshot({ projectId, snapshotId })`
2. **UI Cockpit (`Git & Snapshots` Tab — `ProjectGitSnapshotsTab.vue`):**
   - **Category Filter Pills:** Filters timeline by `All`, `Code`, `PCB`, `CAD`, and `Design`, rendering domain and `asset_key` badges on each snapshot card.
   - **Dual-Gutter Diff, Milestone Comparison & Granular Restore:** Inspects unified diffs per file against the working tree or the previous `milestone:*` snapshot (`compareWith`), supporting single-file or full-snapshot restoration and deletion.
