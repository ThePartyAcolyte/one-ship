# 2026-11-19 — Unified Git & Snapshots Config, Runner-Agnostic Checkpoints, and Auto-Commit Session Minutes

## Summary
- **Unified Configuration Category (`[plugins.vcs]` — `Git & Snapshots`):** Consolidated the former `[plugins.snapshots]` plugin into `[plugins.vcs]` across schema, defaults, and config consumers (`src-tauri/src/config.rs`, `src-tauri/src/kopeng/documents.rs`, `src/components/workspace/ProjectConfigPanel.vue`). Added `snapshots_enabled`, `minutes_checkpoint_mode` (`snapshot` | `commit` | `off`), and `auto_commit_detail_level` (`title_only` | `title_and_decisions` | `full`).
- **Deterministic Config & Project Migration (`0.2.6` -> `0.2.7`):**
  - Added `migrate_toml_snapshots_to_vcs` in `src-tauri/src/config.rs` and wired it into `read_global_config()` so global `config.toml` upgrades `[plugins.snapshots]` into `[plugins.vcs]`.
  - Bumped `CURRENT_PLATFORM_VERSION` to `"0.2.7"` (`src-tauri/src/kopeng/skill.rs`) and updated `run_deterministic_migration` (`src-tauri/src/kopeng/migration.rs`) to migrate `[plugins.snapshots]` into `[plugins.vcs]` in initialized projects' `rula.toml` files.
- **Runner-Agnostic Changed-File Detection:** Replaced transcript parsing (`extract_agent_touched_files`) in `src-tauri/src/kopeng/snapshots.rs` with `detect_changed_code_files` using `git status --porcelain=v1 -z -uall` while excluding hardware/design domain assets (`is_domain_asset_path`) and preserving tracked deletions.
- **Delegated Specialist Gate & Atomic Auto-Commit in `record_session_minutes`:**
  - `execute_record_session_minutes` (`src-tauri/src/mcp.rs`) now rejects callers that have a pending delegation (`db_get_pending_delegation_by_target`), directing them to report via `report_task_status` so the primary/orchestrating agent consolidates collective minutes.
  - Added `validate_auto_commit_ready` and `auto_commit_session_minutes` in `src-tauri/src/kopeng/git.rs` (per-project commit mutex, rejects detached `HEAD` and unborn branches without an initial commit, resets residual staging, commits non-asset code/docs files, and resets the index while returning hook `stdout`/`stderr` on failure).
  - `execute_record_session_minutes` rolls back the inserted journal row (`db_delete_session_journal`) if auto-commit fails, and attaches `commit_hash` (`db_set_session_journal_commit_hash`) when it succeeds.
- **UI & Knowledge Continuity Sync:**
  - Added `commit_hash: Option<String>` to `SessionJournalRecord` (`src-tauri/src/db.rs` and generated `src/types/generated/SessionJournalRecord.ts`) and rendered the commit badge in `src/components/dialogue/DevlogJournalView.vue`.
  - Added `vs Working Tree` / `vs <previous milestone>` diff mode toggling for `milestone:*` snapshots in `src/components/workspace/ProjectGitSnapshotsTab.vue`.
  - Synchronized `src-tauri/src/kopeng/prompt.rs`, `src-tauri/src/ui_state.rs` (`set_project_config_value` surface example `plugin_id='vcs'`), `templates/skills/oneship-core/`, `oneship-devlog`, `oneship-delegation`, `oneship-snapshots`, and living specs `10`, `16`, `20`, and `22`.

## Verification

1. **Bindings Export (`./scripts/run_dev.sh export-bindings`):**
   - `411 passed; 0 failed; 0 ignored; 0 measured; 164 filtered out` (ran after final edit; zero compiler warnings on `one-ship`).
2. **Read-Only Rule Gate (`./scripts/verify_rules.sh`):**
   - `[PASS] Zero TypeScript 'any' in src/`
   - `[PASS] Zero Tailwind palette classes`
   - `[PASS] Zero Result<_, String> in production Rust modules`
   - `[PASS] Zero backdrop-blur-* in components/views`
   - `[PASS] Zero hardcoded /home/butcherwutcher paths`
   - Remaining `[FAIL]` hits in `./scripts/verify_rules.sh` are pre-existing `bg-black/*` classes in unrelated modal/floating components; zero violations in modified files.
3. **Daemon Health & Build Logs (`./scripts/run_dev.sh status` & `journalctl --user -u oneship-dev-daemon-build -n 30 --no-pager`):**
   - `daemon` is `active (running)` listening on port `8765`; `oneship-dev-daemon-build` finished dev profile build in `42.76s` with zero compiler errors.
