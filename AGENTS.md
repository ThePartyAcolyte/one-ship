# Development Protocol and Agent Rules (`AGENTS.md`)

This document establishes the mandatory governance, anti-pattern gates, and verification protocol for **ONE-SHIP**. Detailed architectural, Tiger Style, UI, concurrency, and logging specifications live in [`docs/DESIGN_RULES.md`](docs/DESIGN_RULES.md) and are equally mandatory.

> [!IMPORTANT]
> **Standalone External Development Environment:**
> `one-ship` is the platform orchestrator itself, developed exclusively via external interactive CLI sessions.
> AI agents working in this repository are **NOT** managed subprocesses of ONE-SHIP and **MUST NOT** invoke `post_agent_response` or any ONE-SHIP MCP dialogue tools.
> Output all dialogue, proposals, and responses directly to the user in terminal markdown.

---

## 1. Golden Rules (Non-Negotiable)

1. **Mandatory Technical Proposals:** NEVER create, modify, or delete code, config, or docs without first presenting a structured technical proposal (with objective pros/cons) and receiving explicit user approval.
2. **English Only:** All source code, UI strings, comments, specs, and devlogs MUST be strictly in **English**. Tone must be technical, direct, and objective.
3. **Strict Git & Shell Edit Ban:**
   - **FORBIDDEN:** Executing ANY `git` command (`checkout`, `restore`, `reset`, `add`, `commit`, `stash`, `clean`, etc.) unless explicitly commanded by the user.
   - **All-Inclusive Commits (When Explicitly Commanded):** Whenever the user explicitly commands a `commit` or repository update/push, the agent MUST inspect the full working tree (`git status`, `git diff`) and any modified/new `devlog/` entries, stage **all** pending changes across the entire repository (`git add -A`) — never partial commits that leave other agents' changes behind — and write a commit message covering every change included.
   - **FORBIDDEN:** Creating or modifying files via shell (`cat << EOF`, `echo >`, `sed`, `awk`, `python -c`, `>`). Use `write_file` / `edit_file` exclusively.
4. **Self-Managed Watchers & Build Contention Ban:**
   - **FORBIDDEN:** Starting/stopping daemons, dev servers, or GUI windows (`oneship-daemon`, `vite`, `npm run dev`, `scripts/run_dev.sh all|daemon|gui|stop|rebuild-daemon`).
   - **FORBIDDEN:** Running `cargo build`, `cargo check`, `cargo test`, `npm run build`, or `vue-tsc` unless explicitly requested by the user.
   - **On-Demand Bindings:** Whenever modifying `#[ts(export)]` types, `api::API_COMMANDS`, or `events::DAEMON_EVENT_TYPES`, you MUST run `./scripts/run_dev.sh export-bindings`.
   - **Read-Only Diagnostics Only:** Use `./scripts/run_dev.sh status` or `journalctl --user -u oneship-dev-daemon-build -u oneship-dev-gui -n 50 --no-pager`.
5. **Zero Backward Compatibility & Zero Dead Code:** Active greenfield single-machine project. Delete deprecated code, unused commands, and legacy shims outright. Never leave compatibility wrappers for non-existent callers.
6. **Zero Permissive Fallbacks & Exact Scoping (Tiger Style):**
   - Never fall back to defaults (`unwrap_or`, `unwrap_or_default`, `.filter().unwrap_or(...)`, `projects[0]`, `ends_with`) when a project context, config value, or input is missing or invalid. Fail early and explicitly (`Err(...)` / thrown `Error`).
7. **Full-Stack Atomic Propagation:** Every schema, protocol, command, or UI contract change MUST propagate atomically in the same work cycle across:
   - **Backend (`src-tauri/src/`):** `api::API_COMMANDS` + `api::dispatch` (`api.rs`), `#[ts(export)]`, typed `thiserror` (`api::ApiError`).
   - **Frontend (`src/`):** `DaemonCommands` (`src/types/daemonApi.ts`), `daemonCall`, Pinia stores, components.
   - **Prompts & Skills (`templates/`):** Agent instructions and skills (`0.2.x` frontmatter).
   - **Living Specs (`docs/specs/`):** Bidirectional sync with implementation truth.
8. **Universal Headless Multi-Turn Agent Protocol (`stream-json`):** Spawn all agent subprocesses in interactive REPL mode (`--input-format stream-json --output-format stream-json`) with piped `stdin`/`stdout`. Batch flags (`-p`, `--prompt`, `--batch`) are strictly prohibited.
9. **Pre-1.0 Build Number Versioning:** Maintain `0.2.x` sequential build numbers (`0.2.1`, `0.2.2`, ...) across binaries, specs, and skills. `1.0.0` is reserved for production maturity.

---

## 2. Recurring Anti-Patterns (Strictly Prohibited)

These patterns have repeatedly caused regressions in post-audit sessions and are **strictly forbidden**:

| Anti-Pattern | Prohibited Form | Required Form |
| :--- | :--- | :--- |
| **Silent Fallbacks** | `unwrap_or(...)`, `unwrap_or_default()`, `.ok()`, `let _ =`, empty `catch {}`, `.filter().unwrap_or(default)` | Propagate `?` / typed `Err(...)`, or log explicitly via `logger::log` / `reportAsyncFailure` |
| **Invalid Config Defaulting** | Returning silent default on invalid `config.toml` / `rula.toml` field | Return typed `ConfigError` or log `WARN` + surface error |
| **Unstructured Rust Errors** | `Result<T, String>` in domain, voice, or MCP helpers | Typed `thiserror` enum implementing `api::ApiError` |
| **Client-Side Config/TOML Edits** | Regex-patching `config.toml` or `rula.toml` from Vue/TS | Typed daemon command in `api.rs` + `daemonApi.ts` |
| **Hardcoded UI Palettes & Raw Inputs** | `text-amber-*`, `bg-black/*`, `bg-[#...]`, `text-[7px]`, raw `<input>`/`<select>`, `title="..."` | Spec 13 CSS variables (`--status-*`, `--bg-*`), `AppInput`/`AppSelect`/`AppTooltip` |
| **Magic Constants Outside `config.rs`** | Hardcoded limits, buffer sizes, throttler thresholds, or timeouts in modules | Register in `get_all_plugin_schemas()` (`config.rs`) with `scope: "global_only"` |
| **Unbounded External Processes** | `cmd.output()` / `.output().await` on `git`, `glab`, `mpremote`, `pio` without timeout | Deterministic human-scale timeout + `GIT_TERMINAL_PROMPT=0` where applicable |
| **Un-transacted DB Multi-Steps** | `SELECT MAX(...)` + `INSERT`/`UPDATE`/`DELETE` without transaction | Explicit `conn.transaction()?` ... `tx.commit()?` |
| **Blocking I/O in `async fn`** | `std::fs::*`, `std::process::Command`, sync `db::*` inside `async fn` | `tokio::fs`, `tokio::process`, or `tokio::task::spawn_blocking` |
| **Bare Lock Unwraps** | `.lock().unwrap()`, `.read().unwrap()`, `.write().unwrap()` outside `#[cfg(test)]` | `.read_safe()`, `.write_safe()`, or `.unwrap_or_else(PoisonError::into_inner)` |
| **Unscoped Keys & Leaked Listeners** | Map/vector keys missing `project_id`; listeners/timers without `onUnmounted`/`$reset` | `${projectId}:${id}`; retain and invoke all unlisten/clear handles |

---

## 3. Mandatory Task-Closure Verification Checklist

Before declaring ANY task complete or writing the closing `devlog/`, the agent MUST execute these read-only checks on all modified files and paste the exact output in the devlog's `## Verification` section:

1. **Run Read-Only Rule Gate:**
   - Execute `./scripts/verify_rules.sh` (or targeted `grep -rnE` on the edited files if unrelated legacy failures exist).
   - Confirm **zero new violations** for: `any`, native `title=`, bare lock `.unwrap()`, `Result<_, String>`, Tailwind palettes / `bg-black/*` / `bg-[#...]`, `backdrop-blur`, hardcoded `/home/`.
2. **Check Silent Swallowing & Fallbacks on Edited Files:**
   - Run `grep -nE "unwrap_or|unwrap_or_default|\.ok\(\)|let _ =|catch\s*(\([^)]*\))?\s*\{\s*\}" <edited_files>` and justify or eliminate every hit.
3. **Check Contract & Bindings Sync:**
   - If any `#[ts(export)]`, `API_COMMANDS`, or `DAEMON_EVENT_TYPES` changed, run `./scripts/run_dev.sh export-bindings` and record the pass/fail count.
4. **Check Daemon Health (Read-Only):**
   - Run `./scripts/run_dev.sh status` (and `journalctl --user -u oneship-dev-daemon-build -n 30 --no-pager` if Rust was edited) to confirm no compilation or watcher errors.

---

## 4. Devlog, Spec & Technical Debt Truthfulness Rule

1. **Zero Unverified Claims:** NEVER state in a `devlog/` that a file was "migrated", "cleaned", or "resolved" unless verified by a `grep` / read-only check whose output is cited in `## Verification`.
2. **State & Debt Hygiene:**
   - **`TODO.md`:** Contains ONLY active pending items. Runtime checks remain unchecked until manually verified by the user.
   - **`TECHNICAL_DEBT.md`:** Do NOT delete or mark an entry `RESOLVED` until `./scripts/verify_rules.sh` (or targeted `grep`) confirms zero remaining occurrences. **NEVER reuse retired debt IDs** (`#1`..`#44` remain permanently retired once closed).
   - **`devlog/YYYY-MM-DD_<topic>.md`:** Record every work session with exact file paths, `Verification` command outputs, and any remaining open items.

---

## 5. Detailed Engineering References

For full rules on each domain, read the corresponding reference before editing that subsystem:
- **Detailed Design, Tiger Style, Concurrency, Frontend, Logging & Onboarding Rules:** [`docs/DESIGN_RULES.md`](docs/DESIGN_RULES.md)
- **UI Design System & Mandatory Component Checklist:** [`docs/specs/07-ui/01-design-system-foundations.md`](docs/specs/07-ui/01-design-system-foundations.md), [`02-ui-component-library-and-recipes.md`](docs/specs/07-ui/02-ui-component-library-and-recipes.md), and [`03-workspace-chrome-and-panel-contracts.md`](docs/specs/07-ui/03-workspace-chrome-and-panel-contracts.md)
