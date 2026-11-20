---
name: oneship-snapshots
description: Safety snapshots, manual pre-refactor checkpoints, and selective repository state recovery.
version: 0.2.7
---

# ONE-SHIP Safety Snapshots & State Recovery Protocol

This skill establishes the rules for manual safety checkpoints, inspecting snapshot timelines, and executing clean rollbacks in ONE-SHIP repositories.

---

## 1. Automatic vs Manual Snapshots

1. **Automatic Platform Checkpoints (No Action Required):**
   - The daemon automatically records milestone snapshots (`snapshot` mode) or real Git commits (`commit` mode) whenever the primary/orchestrating agent calls `record_session_minutes`, controlled by `[plugins.vcs].minutes_checkpoint_mode`.
   - File saves trigger scoped code snapshots when `[plugins.vcs].auto_snapshot_on_file_change` is enabled, and domain editors manage per-asset snapshots.
2. **Manual Checkpoints (`create_snapshot`):**
   - Explicitly created by the agent prior to risky, destructive, or speculative modifications.

---

## 2. When to Create a Manual Safety Snapshot

Invoke `create_snapshot` manually before:
1. **Multi-File Structural Refactoring:** Before renaming, moving, or fundamentally changing shared interfaces or schemas.
2. **Experimental Implementations:** Prior to testing a speculative architecture or unverified library integration.
3. **Complex Mass File Edits:** Before applying large search-and-replace transformations across multiple directories.

---

## 3. Rollback & State Recovery Policy (`restore_snapshot`)

When an implementation encounters unrecoverable regressions, compile errors that do not converge, or broken invariants:
1. **Inspect Safety Points:** Use `list_snapshots` to locate the most recent clean checkpoint or milestone hash.
2. **Prefer Clean Rollback over Dirty Diff Stacking:** Rather than accumulating broken patch attempts, propose or execute a restoration using `restore_snapshot`.
3. **Selective File Restoration:** When only a subset of files is broken, specify the exact file paths in `files` to revert only those files without discarding good concurrent work.
4. **Exact Snapshot Identity:** Pass the exact `id` or `ref_path` returned by `list_snapshots`; prefixes are not resolved, and an `id` shared by two snapshots is rejected (use its `ref_path`).
5. **No History Impact:** Snapshots never touch HEAD, branches, the index or hooks, and a restore only changes working-tree files; there is nothing to clean up in git afterwards.
