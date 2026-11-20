# Devlog — 2026-11-19: WP-B Remediation (Electronics UI Tokens, Dead Code Purge, Timeouts & Semantic Pipeline)

## Summary
Executed Work Package **WP-B** from [`docs/audit/post_audit_2026-11-19_findings.md`](../docs/audit/post_audit_2026-11-19_findings.md), resolving **1 High** (`PA-A05`) and **11 Medium** (`PA-M06`–`PA-M08`, `PA-M11`, `PA-M13`–`PA-M18`, plus partial `PA-M12` and `PA-B10`) findings across Electronics UI, Forge/Electronics CLI timeouts, dead-code/shim removal, `ProjectConfigPanel.vue`, `ProjectGitSnapshotsTab.vue`, and the semantic indexing pipeline.

## Changes Implemented

1. **UI Token & Feedback Fixes (`OscilloscopeView.vue`, `ProjectElectronicsTab.vue`, `ProjectGitSnapshotsTab.vue`, `ProjectConfigPanel.vue`):**
   - Migrated all 57 Tailwind palette classes, `bg-black/*`, `bg-[#06070c]`, and `bg-[#090a0f]` in `OscilloscopeView.vue` to Spec 13 semantic tokens (`--ui-bg-*`, `--status-*`, `--fg-color`, `--dim-color`, `--border-color`) and scoped CSS variables (`--osc-ch1`, `--osc-ch2`, `--osc-trig`, `--osc-math`, `--osc-t1`, `--osc-t2`, `--osc-v1`, `--osc-v2`) preserving instrument channel colors (`PA-A05`).
   - Replaced the two hardcoded `bg-[#09090b]` terminal backgrounds in `ProjectElectronicsTab.vue` with `bg-[var(--ui-bg-base)]` (`PA-M17`).
   - Added the `Showing first {{ MAX_RENDERED_FILES_COUNT }} of ... staged files` warning banner to the Staged section in `ProjectGitSnapshotsTab.vue` (`PA-M16`).
   - Updated `ProjectConfigPanel.vue` (`loadSettings` and `toggleCategory`) to surface errors via `reportAsyncFailure` and `settingsError` instead of silent `catch {}` or `console.error`-only handling (`PA-M18`).

2. **Dead Code & Legacy Shim Removal (`desktop.rs`, `api.rs`, `daemonApi.ts`, `ui_state.rs`, `Specs 08 & 12`):**
   - Deleted unused `open_system_terminal`, `OpenSystemTerminalArgs`, and `DesktopError` from `desktop.rs`, `api.rs`, `src/types/daemonApi.ts`, and synchronized `docs/specs/08_native_process_supervisor.md` and `docs/specs/12_streamable_http_mcp_daemon.md` (`PA-M14`). *(Note: previously generated `src/types/generated/OpenSystemTerminalArgs.ts` remains unimported on disk since shell deletion is forbidden by `AGENTS.md` §1.3.)*
   - Removed `migrate_legacy_node_path` compatibility shim and its migration test from `src-tauri/src/ui_state.rs`, verifying stale legacy paths sanitize directly to `DEFAULT_NODE_PATH` (`PA-M15`).

3. **Explicit Target Repo & Bounded Process Timeouts (`config.rs`, `forge/mod.rs`, `forge/inbox.rs`, `forge/gitlab.rs`, `electronics/firmware.rs`):**
   - Added `ForgeError::MissingTargetRepo` (`StatusCode::BAD_REQUEST`, `"ForgeError::MissingTargetRepo"`) and replaced `ctx.target_repo.unwrap_or_default()` in `forge/inbox.rs` so projects with a configured remote whose `owner/repo` cannot be resolved fail explicitly rather than writing `repo = ""` rows (`PA-M06`).
   - Registered `[plugins.vcs].forge_cli_timeout_seconds` (`60`) and `[plugins.electronics].firmware_upload_timeout_seconds` (`180`) in `config.rs` (`scope: "global_only"`).
   - Added `run_sync_command_with_timeout` in `forge/mod.rs` (draining stdout/stderr on worker threads and killing on deadline) and wired it into `git_remote_url`, `git_rev_list_counts`, and `run_glab_api` in `forge/gitlab.rs` with `GIT_TERMINAL_PROMPT=0` (`PA-M07`).
   - Wrapped `mpremote` and `pio` invocations in `electronics/firmware.rs` with `tokio::time::timeout` and `kill_on_drop(true)`, returning actionable `ElectronicsError::FlashFailed` messages on timeout (`PA-M08`).

4. **Semantic Pipeline Purity, Config Knobs & Orphan Protection (`config.rs`, `semantic/{mod.rs,chunker.rs,throttler.rs,engine.rs}`):**
   - Removed `unsafe { libc::nice(19); }` calls from Tokio `spawn_blocking` threads in `semantic/mod.rs` (`PA-M11`).
   - Registered 7 runtime semantic knobs in `config.rs` (`scope: "global_only"`): `embedding_batch_size_count` (`10`), `embedding_batch_sleep_ms` (`20`), `throttler_min_free_mem_mbytes` (`2048`), `throttler_critical_free_mem_mbytes` (`512`), `throttler_max_cpu_percent` (`45`), `onnx_intra_threads_count` (`2`), and `max_relations_per_result_count` (`6`), and wired them into `semantic/mod.rs`, `throttler.rs`, and `engine.rs` (partial `PA-M12`).
   - Added `retain_existing_records_for_file` in `semantic/mod.rs` so transient `read_to_string`, `fs::read`, or PDF extraction errors retain previously indexed vectors rather than purging them as orphans (`PA-M13`).
   - Enforced `exceeds_max_file_size` for `.pdf` files with `WARN` logging on `metadata` failure (`PA-B10` partial), and replaced `chunker::chunk_pdf_file` with `chunker::chunk_pdf_bytes` to avoid reading PDFs twice from disk (`PA-M13`).

5. **Audit Tracker (`docs/audit/post_audit_2026-11-19_findings.md`):**
   - Marked `PA-A05`, `PA-M06`–`PA-M08`, `PA-M11`, `PA-M13`–`PA-M18` as `RESOLVED (WP-B)` and `PA-M12` as `PARTIAL (WP-B)`.

## Verification

- `./scripts/run_dev.sh export-bindings`:
  - `test result: ok. 402 passed; 0 failed; 0 ignored; 0 measured; 155 filtered out; finished in 0.34s`
- `./scripts/verify_rules.sh | grep -E "^\[|^==="`:
  - `[PASS] Zero TypeScript 'any' in src/`
  - `[PASS] Zero Tailwind palette classes (text-/bg-/border-{color}-*)` *(resolved from 57 hits in `OscilloscopeView.vue` to 0)*
  - `[FAIL] Zero raw bg-black/* or bg-[#...] outside ui.css` *(remaining hits are legacy modals/tabs tracked under `PA-B12`)*
  - `[PASS] Zero Result<_, String> in production Rust modules`
  - `[PASS] Zero backdrop-blur-* in components/views`
  - `[PASS] Zero hardcoded /home/butcherwutcher paths`
- Targeted check on `OscilloscopeView.vue` and `ProjectElectronicsTab.vue`:
  - `grep -nE '(text|bg|border)-(amber|emerald|rose|orange|fuchsia|sky|red|green|blue|yellow|zinc|slate|gray|neutral|purple|indigo|teal|cyan|pink)-[0-9]+|bg-black/|bg-\[#|text-\[7px\]' src/components/workspace/electronics/OscilloscopeView.vue src/components/workspace/ProjectElectronicsTab.vue` -> `CLEAN: zero matches`
- `./scripts/run_dev.sh status`:
  - `daemon` (`active/running`, port `8765`), `daemon_watch` (`active/running`), `vite` (`active/running`).
- `journalctl --user -u oneship-dev-daemon-build -n 12 --no-pager`:
  - `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 1m 03s` (`oneship-dev-daemon-build.service` completed cleanly after removing unused `desktop` import in `api.rs`).

## Next Steps
- **WP-C** (documentation/debt hygiene, remaining low-severity items, and `chunker.rs` internal constants if desired) upon maintainer request.
