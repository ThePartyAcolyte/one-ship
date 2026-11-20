# Devlog: Runner Models & Effort Synchronization (Agy & Claude)

**Date:** 2026-10-26  
**Topic:** Runner Schemas, Model Catalog Sanitization & Effort Synchronization  
**Status:** Completed  

---

## 1. Problem Statement & Ground Truth Audit

The application runner specifications (`templates/runners/agy.toml`, `templates/runners/claude.toml`, `src-tauri/src/config.rs`, and `FloatingChatOverlay.vue`) contained obsolete and hallucinated model entries (e.g. `claude-haiku-5-5`, `claude-opus-4-8`, `gemini-3.7-flash`, etc.).

Direct host inspection was performed against live binaries:
1. `agy models`:
   - `gemini-4-argon` (`low`, `medium`, `high`)
   - `gemini-3.8-flash` (`low`, `medium`, `high`)
   - `claude-opus-5-5` (`low`, `medium`, `high`)
   - `claude-sonnet-5-5` (`low`, `medium`, `high`)
   - `gpt-oss-120b-medium` (fixed, no effort tiers)
2. `claude` (Anthropic Claude Code CLI v2.1.296):
   - Documented aliases in `claude --help`: `fable` (`claude-fable-5-1`), `opus` (`claude-opus-5-5`), `sonnet` (`claude-sonnet-5-5`).
   - Effort flag: `--effort <level>` with levels `low`, `medium`, `high`, `xhigh`, `max`.
   - The non-existent `haiku` model was purged.

---

## 2. Architectural Changes

### A. Runner Specifications (`templates/runners/`)
- **`templates/runners/agy.toml`**:
  Updated to strictly contain `gemini-4-argon`, `gemini-3.8-flash`, `claude-opus-5-5`, `claude-sonnet-5-5` (all with effort levels `low`, `medium`, `high`) and `gpt-oss-120b-medium`. Obsolete Gemini 3.1/3.6/3.7 and Claude 4.6 entries removed.
- **`templates/runners/claude.toml`**:
  Configured with `claude-opus-5-5`, `claude-sonnet-5-5`, and `claude-fable-5-1`, preserving the `--effort` runner flag and the 5 effort levels (`low`, `medium`, `high`, `xhigh`, `max`). Purged hallucinated `haiku`, legacy `opus-4-8`, `sonnet-4-6`, etc.

### B. Backend Configuration & Process Supervision (`src-tauri/`)
- **`src-tauri/src/config.rs`**:
  - Synchronized `init_model` and `default_model` options in `get_all_plugin_schemas()`.
  - Synchronized in-code fallback schemas in `load_runner_schema` for `agy` and `claude`.
  - Updated unit tests `test_load_agy_runner_schema` and `test_load_claude_runner_schema` to validate live model identities, effort availability, and absence of `haiku`.
- **`src-tauri/src/supervisor.rs`**:
  - Dynamically verifies `schema.flags.effort` before formatting CLI arguments for `claude` and other runners.

### C. Frontend Dialogue Overlay (`src/components/dialogue/FloatingChatOverlay.vue`)
- Synchronized fallback `availableModels` in `catch` blocks for `agy` and `claude`.
- Updated `LIGHT_MODEL_BY_RUNNER` for `claude` from `'claude-haiku-5-5'` to `'claude-sonnet-5-5'`.

---

## 3. Verification

1. `cargo test --manifest-path src-tauri/Cargo.toml test_load_`:
   All runner schema tests passed (`test_load_agy_runner_schema`, `test_load_claude_runner_schema`, `test_load_opencode_runner_schema`).
2. `cargo check --manifest-path src-tauri/Cargo.toml`:
   Passed with 0 compilation errors.
3. `npm run build`:
   TypeScript type check (`vue-tsc`) and Vite bundling completed with 0 errors in 4.18s.
