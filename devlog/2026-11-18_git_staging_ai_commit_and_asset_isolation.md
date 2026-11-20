# 2026-11-18 — Git Staging Workflow, AI Commit Message, Asset Isolation & Project Git Agent

## Summary

Replaced the automatic full-repo unified diff in `ProjectGitSnapshotsTab.vue` with a VS Code-style manual staging and commit workflow, added ephemeral AI commit message generation from staged code diffs, excluded Design/PCB/CAD assets from diff previews and code/milestone snapshots while keeping them tracked in Git, bound `OPEN AGENT` and quick actions to a configurable project agent (`plugins.vcs.default_agent_id`), fixed symlinked skill file opening in Reader, and aligned the panel with Spec 13.

## Changes Implemented

### Backend (`src-tauri/src/`)
- **`kopeng/snapshots.rs` & `mcp.rs`:**
  - Added `is_domain_asset_path` and `domain_asset_exclude_pathspecs` covering configured `design_dir`, `pcb_dir`, `cad_dir` and known Design/PCB/CAD extensions.
  - Updated `create_domain_snapshot` (`domain == "code"`) to exclude domain assets from `scoped` files and whole-tree `git add -A` captures.
  - Added `create_milestone_snapshot_if_needed` and wired `execute_record_session_minutes` to it so milestones that only touch Design/PCB/CAD assets (or no code files) skip creating redundant milestone snapshots.
- **`supervisor.rs`:**
  - Added `run_ephemeral_runner_prompt` using `stream-json` (AGY `step_update` / Claude) and `acp` (OpenCode) protocols (Rule 9 compliant, zero batch flags).
  - Updated `dispatch_git_instruction` to resolve `plugins.vcs.default_agent_id` (failing explicitly when unset) and launch/send instructions to that declared project agent.
- **`kopeng/git.rs`, `api.rs`, `src/types/daemonApi.ts`:**
  - Added `is_domain_asset` to `WorkingTreeFile` and `CommitFileChange`.
  - Added manual daemon commands: `stage_git_files`, `unstage_git_files`, `discard_git_files`, `commit_git_changes`, `sync_git_branch`, and `generate_commit_message` (reads staged code diff only, lists staged assets by path, uses `GIT_AGENT_KEYS`).
- **`config.rs` & `kopeng/resolution.rs`:**
  - Added `GIT_AGENT_KEYS` (`plugin: "vcs"`, `git_runner` / `git_model` / `git_effort`) and `default_agent_id` (`field_type: "project_agent"`, `scope: "project_only"`) to `vcs` plugin schema; added `"project_agent"` to string type validation.
- **`kopeng/documents.rs`:**
  - Allowed `read_project_document` to read symlinked files under `.agents/skills/` while keeping `..` traversal blocked and writes strictly inside canonical project root.

### Frontend (`src/components/workspace/`)
- **`ProjectGitSnapshotsTab.vue`:**
  - **Diff Performance & Bounded Lists:** Removed automatic global unified diff when `selectedFile === null` and removed `View Global Diff`; renders `AppEmptyState` until a file is selected, suppresses diff rendering for `is_domain_asset` files while keeping them stageable/committable, and caps rendered file rows at `MAX_RENDERED_FILES_COUNT = 250` to prevent WebKit freezes on repositories missing `.gitignore`.
  - **Simplified Cockpit & VS Code Staging:** Removed the 2-row top cockpit bar and manual snapshot creator; placed `GitBranchSelector`, quick actions, `+` action creator, `SYNC`, and `OPEN AGENT` in the inspector bar; added commit composer (`AppTextarea` + `Sparkles` AI generate + `COMMIT`) and `Staged Changes` / `Changes` lists with `+`/`-` staging (removed accidental one-click discard).
  - **Resizable Two-Line File Pane:** Added a draggable `w-1 cursor-col-resize` divider (`220px–560px`) and two-line rows (filename + relative dir, `overflow-x-hidden`).
  - **Spec 13 Alignment:** Replaced `bg-black/*` surfaces with `.ui-surface-*`, removed `flex-wrap` from bars, removed manual reload icon (`F5` refreshes), and added `Escape` modal close.
- **`ProjectConfigPanel.vue`:**
  - Loaded project agents via `get_project_agent_state` and added `project_agent` `AppSelect` rendering for `plugins.vcs.default_agent_id`.

## Specs Updated
- `docs/specs/09_vcs_and_logging_architecture.md` §5.1 — Updated Git & Snapshots cockpit, staging, AI commit message, and bounded file list.
- `docs/specs/10_configuration_schema_and_cascade.md` §2.2, §3 — Documented `project_agent` field type and `[plugins.vcs]` keys (`default_agent_id`, `git_runner`, `git_model`, `git_effort`).
- `docs/specs/13_design_system_and_ui_components.md` §3.13 — Git & Snapshots panel contract.
- `docs/specs/16_agent_session_snapshot_system.md` §3.1, §3.3 — Milestone/code snapshot asset exclusion.

## Verification
- Targeted Spec 13 §11.3 grep audit on `ProjectGitSnapshotsTab.vue` clean (only `AppEmptyState` `title` props).
- `ts-rs` bindings export (`./scripts/run_dev.sh export-bindings`) passed (`381 passed; 0 failed`).
- Daemon and GUI verified running via `./scripts/run_dev.sh status`.
