# Devlog — 2026-11-19: WP-C Remediation (Debt/Doc Sync, Git Timeouts, Orphan Cleanup & Low-Severity Fixes)

## Summary
Executed Work Package **WP-C** from [`docs/audit/post_audit_2026-11-19_findings.md`](../docs/audit/post_audit_2026-11-19_findings.md), closing **1 Medium** (`PA-M19`) and **10 Low** (`PA-B02`, `PA-B04`, `PA-B05`, `PA-B07`–`PA-B09`, `PA-B11`, `PA-B12` partial, `PA-B14`, `PA-B15`) findings, removing the orphaned generated binding `OpenSystemTerminalArgs.ts`, and registering deferred legacy UI/structural items cleanly under `TECHNICAL_DEBT.md #45` and `#46`.

## Changes Implemented

1. **Orphan Binding Removal:**
   - Deleted `src/types/generated/OpenSystemTerminalArgs.ts` (authorized by maintainer) and verified 0 unreferenced files remain in `src/types/generated/*.ts`.

2. **Backend Cleanup & Git Safety (`main.rs`, `kopeng/git.rs`, `supervisor.rs`):**
   - Removed dead `GTK_OVERLAY_SCROLLING=0` env var from `src-tauri/src/main.rs` (`PA-B07`).
   - Updated `git()` in `src-tauri/src/kopeng/git.rs` to set `GIT_TERMINAL_PROMPT=0` and execute via `run_sync_command_with_timeout` (`[plugins.vcs].forge_cli_timeout_seconds`, default `60`s); raised `generate_commit_message` prompt timeout to `>= 90_000 ms` and replaced `trim_start_matches("text"/"git")` with opening-fence-only stripping (`PA-B08`).
   - Bounded `dispatch_git_instruction` `INFO` logging in `src-tauri/src/supervisor.rs` to `instruction.len()`, switched runner detection to `Path::file_name()`, and propagated stdin write/flush errors on ACP prompt delivery (`PA-B09`).

3. **Frontend Component & Error Handling (`CardHeader.vue`, `voiceControllerStore.ts`, `ProjectSkillsTab.vue`, `QuotaGaugePreviewSelector.vue`, `AppExecutionConfigPicker.vue`, `ProjectManagementTab.vue`):**
   - Migrated description editing in `CardHeader.vue` from raw `<input>` to `<AppInput>` and added `catch` + `reportAsyncFailure` in `saveDescription` (`PA-B02`).
   - Replaced silent `.catch(() => {})` in `voiceControllerStore.ts` (`logVoiceClient`) and silent `localStorage` `catch {}` blocks in `ProjectSkillsTab.vue` with `console.warn` (`PA-B04`, `PA-B11`).
   - Migrated all 10 `text-[7px]` occurrences in `QuotaGaugePreviewSelector.vue` (1) and `AppExecutionConfigPicker.vue` (9) to `text-[8px]` (`PA-B12` partial; 7 remaining hits in `ProjectWorkspace.vue` and `ProjectTerminalGrid.vue` tracked in `TECHNICAL_DEBT.md #45`).
   - Updated stale `project/timeline` comment in `ProjectManagementTab.vue:281` to `project/gantt` / `project/schedule` (`PA-B15`).

4. **Specs, Skills, `TODO.md` & `TECHNICAL_DEBT.md` Synchronization (`PA-M19`, `PA-B05`, `PA-B14`, `PA-B15`):**
   - Added `version: "0.2.6"` frontmatter to `templates/skills/{oneship-project-mgmt,oneship-desigma,oneship-pcbcraft}/SKILL.md` (`PA-B14`).
   - Updated `docs/specs/13_design_system_and_ui_components.md` version to `0.2.14` (pre-1.0 compliance), marked `docs/specs/39_forge_provider_and_project_surface_handoff.md` §1 as historical pre-migration baseline, and documented `get_system_log_sources` in `Specs 09` and `12` (`PA-B05`, `PA-B15`).
   - Cleaned stale `open_system_terminal`, `GithubCollabPanel`, `project/github` fallback-to-Board, and dangling `#41`–`#44` references from `TODO.md`, converted `Date-Range Validation` to a runtime check, and added the pending manual scrollbar verification item (`PA-M19`, `PA-B15`).
   - Added `TECHNICAL_DEBT.md #45` (legacy `bg-black/*`, `hover:bg-white/5`, and remaining `text-[7px]`) and `#46` (semantic audit REST endpoint, legacy SQLite column suffixes, internal chunker heuristics, monolithic SFC decomposition, and `AppHeader.vue` canvas fallback hexes) without reusing `#1`–`#44`.

## Verification

- `./scripts/verify_rules.sh | grep -E "^\[|^==="`:
  - `[PASS] Zero TypeScript 'any' in src/`
  - `[PASS] Zero Tailwind palette classes (text-/bg-/border-{color}-*)`
  - `[FAIL] Zero raw bg-black/* or bg-[#...] outside ui.css` *(remaining hits tracked in `TECHNICAL_DEBT.md #45`)*
  - `[PASS] Zero Result<_, String> in production Rust modules`
  - `[PASS] Zero backdrop-blur-* in components/views`
  - `[PASS] Zero hardcoded /home/butcherwutcher paths`
- Targeted WP-C checks:
  - `ls -1 src/types/generated/*.ts | while read f; do ...` -> `0` unreferenced generated files.
  - `grep -rn "GTK_OVERLAY_SCROLLING" src-tauri/src/` -> `CLEAN: zero GTK_OVERLAY_SCROLLING`.
  - `grep -nE "GithubCollabPanel|open_system_terminal|#41|#42|#43|#44" TODO.md` -> `CLEAN: TODO.md synced`.
  - `grep -nE "catch\s*(\([^)]*\))?\s*\{\s*\}|\.catch\(\(\)\s*=>" CardHeader.vue voiceControllerStore.ts ProjectSkillsTab.vue` -> `CLEAN: zero silent JS catches`.
  - `grep -rn "text-\[7px\]" src/` -> `0` hits in `QuotaGaugePreviewSelector.vue` / `AppExecutionConfigPicker.vue`; 7 remaining legacy hits (`ProjectWorkspace.vue`: 4, `ProjectTerminalGrid.vue`: 3) recorded in `TECHNICAL_DEBT.md #45`.
- `./scripts/run_dev.sh status` & `journalctl --user -u oneship-dev-daemon-build -n 15 --no-pager`:
  - `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 33.73s`; `daemon` (`active/running`, port `8765`), `daemon_watch` (`active/running`), and `vite` (`active/running`).
