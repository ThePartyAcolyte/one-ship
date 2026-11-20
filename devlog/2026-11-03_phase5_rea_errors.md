# 2026-11-03 — Phase 5 Module 15 (`kopeng/rea.rs` Typed Errors)

Follow-up of `2026-11-03_phase5_config_errors.md`.

## `ReaError`
- Existing enum extended: `Io { context, source }` (was a `details` string), `EngineNotInstalled`, `InvalidSetting`, `InvalidView`, `Task`, `Registry`, `Config`, `Skill`; implements `Serialize`. The 8 Tauri commands return it (the 5 `Result<_, String>` are gone); `get_rea_queue_snapshot` returns the snapshot directly (it could not fail).

## Fixes
- Scoping (Rule 11): every project command accepted any path; they require a registered project (`require_registered_project`).
- Gateway origin: caller-identity failures were swallowed (`.ok().and_then(r.ok())`) and the request fell through to the URL's `project_hex`; now only `CallerError::UnmanagedCaller` may use it and any other failure rejects. The plain `project_path` query alias is removed, malformed hex is an error, and `decode_hex_string` no longer slices `&str` at byte offsets (a non-ASCII parameter panicked the daemon).
- Engine start: when `rea` was not on `PATH` it ran `npx -y rea-agents@latest` (an unpinned download at runtime); it now fails with `EngineNotInstalled` (Spec 26 already specified `rea mcp`). `which` errors were read as "not installed"; the presence check uses `rea --version`.
- Engine port: `id % 200` made projects 3 and 203 share a port, and an unregistered or unreadable registry fell back to a path hash; the port is `8790 + id` for registered projects only.
- `toggle_project_rea_plugin`: an unreadable or unparsable `rula.toml` was replaced by a table holding only `[plugins.rea]` (dropping scripts, agents, project); it now requires a valid `rula.toml` (`config::read_project_rula`). Disabling discarded the engine stop result.
- `ghidra_install_dir`: an empty value silently became `/opt/ghidra`; it is an `InvalidSetting` error, and the toolchain check without a project reads the configured global value instead of the literal.
- The idle reaper discarded stop failures; they are logged.
- Scans: `compute_dir_size_bytes`, the targets count, `classify_binary_magic`, the SHA-256 of targets ("unavailable") and the status worker join (`(0, 0)`) read errors as zero/empty; they are returned. The 256 MB hash limit is a named constant.
- Visual views: unreadable or invalid JSON files were skipped silently; `read_view` returns `InvalidView` / I/O errors. The three parsers were rewritten with `read_view` / `view_items` / `str_field` (same output).

## View contract (maintainer follow-up)
The first pass kept the field placeholders; the stated reason (frontend types) did not justify leaving them, and they were removed in the same session:
- One shape per file (`{ subsystems, call_edges }`, `{ functions }`, `{ items }`); bare arrays and the `edges` alias are rejected.
- Identifying fields are required (`name`, `from_symbol`/`to_symbol`, `symbol`, `endpoint_or_key`); a missing one is `InvalidView` naming file, list, index and field (`ViewFile::required`). Wrongly typed fields are errors too.
- `offset_hex`, `subsystem`, `signature`, `category` are `Option` in Rust and `string | null` in `ProjectReaTab.vue`, which shows `—` (or omits the badge / prompt fragment). Placeholders `"0x0"`, `"sub_unknown"`, `"Core"`, `"caller"`/`"callee"`, `"protocol"`, `"Subsystem"` are gone; `signature` no longer copies `symbol`.
- The exact shape is documented in the `oneship-rea` skill (Phase 4) and spec 26 §5. Test: `view_items_require_their_identifying_fields`.

## Left as is (noted)
`forward_jsonrpc_to_rea_engine` shells out to `curl` with a fixed `--max-time 320`.

## Verification
`cargo check --all-targets` clean; `clippy` no warnings in `rea.rs`; `cargo test --lib`: 74 passed (3 new: hex decoding, strict views, symlink-safe directory size), same 2 pre-existing `kopeng::resolution` failures. Backend `Result<_, String>`: 60. Spec 26 §2.1 added. Not run in the app.
