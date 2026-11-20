# 2026-11-05 — Phase 5 Module 23 (`kopeng/diagnostic.rs` Typed Errors)

Follow-up of `2026-11-05_phase5_persona_errors.md`.

## `DiagnosticError`
New enum with `Serialize`: `Spawn`, `UnknownTool`, `NoTools`, `Install` and `Task`. The 2 `Result<_, String>` are gone.

## Decision
`install_missing_tool(s)` had no caller (TODO item 9, TECHNICAL_DEBT #9). The user chose to wire them into the diagnostics popover. A single `install_missing_tools(tool_ids)` remains, and the special `mcp` case is removed (the daemon syncs the agy MCP config at startup).

## Fixes
- **Main thread blocked:** `run_diagnostics` was synchronous and ran `git`, `gh` and `gdbus` on the main thread. The D-Bus call had no timeout (gdbus waits 25 s by default). The command is now async with `spawn_blocking`, and `gdbus` takes `--timeout` from the new `system.dbus_call_timeout_seconds` (default 2).
- **Run failures shown as missing:** any spawn error counted as a missing tool (`unwrap_or(false)`). Only `NotFound` counts as missing now; any other failure is an error.
- **Stale socket:** OpenDictate counted as available whenever `/tmp/opendictate.socket` existed, without asking the service. It is now checked only with D-Bus `GetStatus`.
- **`mcp_configured` removed:** no UI read it, it duplicated the daemon's startup sync, and it read `HOME` with an empty fallback. In `bin/daemon.rs`, that startup sync was `let _ =` and is now `log_failure` (ERROR).
- **Hidden error:** a diagnostics error was only `console.error`ed, and the popover then showed every check as MISSING. The popover now shows the error.
- **Installer:**
  - An empty list or an unknown tool is an error (an empty list used to return Ok).
  - It no longer returns a message string.
  - It logs at `DEBUG` on entry.

## Frontend
- `types/index.ts`: `DiagnosticState`.
- `App.vue`:
  - diagnostics start as `null`, with an error state;
  - `installTools` re-runs the diagnostics afterwards;
  - typed timer.
- `AppHeader.vue`:
  - install icon buttons (with `AppTooltip`) next to a missing git / gh;
  - an error line;
  - typed props and the `installTools` emit.

## Docs
- Spec 11: the diagnostics checks (`gh auth token`, not `gh auth status`) and the installer.
- Spec 10: the new key.
- TODO item 9: `install_missing_tool(s)` removed from it.
- TECHNICAL_DEBT #9: reduced to GitHub/GitLab parity.

## Verification
- `cargo check --all-targets` clean.
- `clippy`: 17 warnings, none in `diagnostic.rs`.
- `cargo test --lib`: 102 passed, 2 of them new (package resolution, missing program reported as absent). The same 2 `kopeng::resolution` failures remain.
- `vue-tsc` clean.
- `Result<…, String>`: 54.
- Not run in the app.
