# 2026-11-01 — Phase 5 Modules 5–7 (`kopeng/documents.rs`, `kopeng/skill.rs`, `kopeng/visual_builder.rs` Typed Errors)

Follow-up of `2026-11-01_port_unification_terminal_scoping_and_phase5_accounts_conversation.md`.

## `DocumentsError`
- Variants: `ProjectNotFound`, `PathEscapesRoot`, `FileNotFound`, `Io { context, source }`, `InvalidRequest`, `Task`, `Database`.
- Implements `serde::Serialize` as its message, so the module's Tauri commands return `Result<_, DocumentsError>` directly (no `String` conversion layer; the UI still receives text). This is the pattern to use for the remaining modules whose commands hold the logic.
- Helpers: `io_err(context)` (`map_err` adapter), `require_project_root`, `resolve_existing_project_file` (replaces the traversal/canonicalization block duplicated in `read_project_document` and `open_file_in_external_editor`).

## Behavior fixes found during conversion
- `get_unique_target_path` was an unbounded loop; now capped at `MAX_UNIQUE_NAME_ATTEMPTS_COUNT` (10 000) with an error (AGENTS.md §8.4).
- `move_project_items` falls back to copy + delete only on `EXDEV` (cross-filesystem); any other rename error is reported. `rename_project_item` (always within one project) no longer copies on rename failure.
- `paste_wayland_clipboard_image` fails when `wl-paste` cannot run (it returned "no image" silently); `Ok(None)` now means only that the clipboard holds no image.
- `attach_file_to_project` propagates canonicalization failures instead of comparing uncanonicalized paths.
- `get_project_documents` reports a scratchpad creation failure instead of discarding it.

## Verification (documents)
`cargo check`/`clippy` clean (35 pre-existing style warnings; none in `documents.rs`), `vue-tsc` clean. `cargo test --lib`: 54 passed, 2 failed in `kopeng::resolution` (pre-existing: the tests expect `gemini-3.8-flash` but `templates/runners/agy.toml`, modified before the audit, lists `gemini-4-argon` first; tracked in TODO).

## `kopeng/skill.rs` (`SkillError`)
Decision (maintainer): the canonical `templates/` tree is located at compile time (`TEMPLATES_ROOT = $CARGO_MANIFEST_DIR/../templates`), with no other source.

- Variants: `TemplatesNotFound`, `UnknownSkill`, `UnknownTemplate`, `InvalidName`, `ProtectedSkill`, `NotFound`, `ProjectNotFound`, `HomeUnavailable`, `NotAnObject`, `Io`, `Json`, `Remote`, `ProjectFiles` (wraps the `String` errors of `manifest.rs` / `migration.rs` until those modules are converted). Implements `Serialize`; all 9 Tauri commands return it. `SupervisorError::Provisioning(#[from] SkillError)` added.
- Fallbacks removed:
  - template lookup in CWD-relative paths, `~/.config/one-ship/templates`, a hardcoded `/home/...` path and executable ancestors;
  - the file-copy path of `inject_platform_skill` / `inject_specialized_skill` and all 15 `include_str!` skill copies;
  - embedded prompt templates and the undocumented `~/.config/one-ship/prompts` override. `load_prompt_template` is now async (`tokio::fs`) and fails on an unknown name; previously `flag_resolve_web_builder` / `flag_resolve_dev_server` had no embedded copy and would have produced an empty prompt;
  - `CURRENT_SKILL_VERSION` and the runtime read of the version from `oneship-core/SKILL.md`: `CURRENT_PLATFORM_VERSION` is the only version, and a test asserts the SKILL.md frontmatter matches;
  - `inject_project_mcp_plugin` replaced a corrupt or non-object `opencode.json` / `.mcp.json` with `{}` (erasing other MCP servers); it now fails. Its file writes, the `CLAUDE.md` symlink and the `agy` instructions file no longer discard errors;
  - `launch_flag_resolution_agent` mapped unknown flag types to the docs template; it now returns an error (the six MCP-declared flag types are all mapped).
- Security: skill names from IPC/MCP were joined into paths unchecked, so `remove_project_skill_cmd(.., "../../src")` could delete project directories. `validate_skill_name` now accepts only one path component; `cleanup_ephemeral_skill` only removes declared specialized skills. The SkillsMP query is passed with `curl -G --data-urlencode` instead of being concatenated into the URL.
- Callers: `initializer.rs`, `flag_resolver.rs`, `rea.rs`, `supervisor.rs` propagate skill failures instead of `let _ =`. The initializer and flag resolver no longer call `prepare_project_agents_md` / `inject_project_mcp_plugin` before `inject_platform_skill`, which already runs both. Background cleanups in the initializer monitor loop use `log_failure`. Greenfield REA scaffolding propagates `rula.toml` and REA directory failures.
- Tests added: canonical version match, every declared skill has a template, skill-name validation.

## Verification (skill)
`cargo check` clean; `clippy` 33 warnings (all pre-existing, none in `skill.rs`). `cargo test --lib`: 57 passed, same 2 pre-existing `kopeng::resolution` failures. Specs 10 (§5.1 single template source) and 25 (symlink, not copy) updated.

## `kopeng/visual_builder.rs` (`VisualBuilderError`)
Decision (maintainer): the Visual Builder dev server and frontend layout are declared in `rula.toml`, on the script itself.

- Contract: exactly one `[scripts.persistent.<id>]` carries a `[scripts.persistent.<id>.visual_builder]` table with `css_entry`; the script's existing `port` key is required, and its `cwd` is the frontend root (`public/`, `src/`, the single `tailwind.config.{js,ts,cjs,mjs}`). No marked script → `missing_script` (State C, `no_dev_server` flag); anything malformed → `InvalidConfig`.
- Heuristics removed: dev script detection by command substrings (`dev`/`start`/`vite`/`next`), port guessing (3000, or 5173 when a `vite.config.*` exists), `frontend/public` vs `public` probing, the 4 Tailwind config and 2 `src/` candidates, the 6 CSS candidates of `touch_project_css` (now `touch_css_entry`, whose errors propagate).
- Scoping (security): JSX mutations wrote to any absolute path, and `restore_source_file_snapshot` overwrote any existing file with the given content. Every command now requires a registered project (exact match) and the file must canonicalize inside it. The snapshot no longer guesses the project (registry prefix → `rula.toml` walk → parent directory → `"default"`). `restore_source_file_snapshot` takes `project_path`; `ProjectVisualBuilderTab.vue` sends it.
- Bugs fixed: `strip_and_replace_attribute` rebuilt the tag byte by byte (`bytes[i] as char`), corrupting every non-ASCII character in the tag (e.g. `alt="Café"`), and sliced `&str` at byte offsets that could panic on multi-byte text; the image import name loop was unbounded (now 10 000 attempts); `list_project_images` created `public/images` as a side effect and swallowed `read_dir` errors; color merge wrote files as it went (now computes every rewrite before the first write).
- Text edits (maintainer decision: fail): `update_jsx_element_text` no longer replaces the element's whole inner body when the old text is not found, nor replaces the first occurrence anywhere in the file when the element cannot be located, nor searches past an element without a closing tag. `start_line` is required; the old text must occur in the located element's body (empty old text only for an empty body); otherwise `TagNotFound` / `TextNotFound` / `InvalidRequest` and nothing is written. `ProjectVisualBuilderTab.vue` reloads the canvas after a rejected edit, since it shows the new text optimistically. Consequence: editing rich text whose rendered text differs from the source (nested tags, expressions) now fails instead of flattening the element. `update_tailwind_theme_color` keeps its documented upsert (adds the key when absent).
- Layers updated: `rula_schema.md` §5.1.1 (`oneship-core` skill), prompts `flag_resolve_dev_server` (rewritten around the contract; reads the port from the dev server config instead of assuming defaults), `flag_resolve_web_builder`, `greenfield_initializer`, the inline greenfield instruction in `initializer.rs`, spec 27 §1.2/§3/§4.1/§5/§9.1, the State C text in `ProjectVisualBuilderTab.vue`. Project files updated at the maintainer's request: `estudiorc/rula.toml` and `watchtower/rula.toml` (`port = 5173`, `css_entry = "src/index.css"`; verified: TOML parses, CSS entry exists, one `tailwind.config.js`).
- Tests added: text edit confined to the located element; non-ASCII attribute replacement; config contract (no marked script → `None`, missing `port` → error, valid config resolves port/script/public dir).

## Verification (visual_builder)
`cargo check` clean; `clippy` 30 warnings (pre-existing, none in `visual_builder.rs`); `cargo test --lib`: 60 passed, same 2 pre-existing `kopeng::resolution` failures; `vue-tsc` clean. `Result<_, String>` in the backend: 108 (245 at the audit).
