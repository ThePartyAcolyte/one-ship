# Devlog: Multi-Account Quota Tracking & Statusline Session Attribution

**Date:** 2026-09-14  
**Topic:** Fixing Multi-Account Quota Tracking, Statusline Schema Resolution, and Real-Time Turn Synchronization

---

## 1. Context & Identified Problem
In the multi-account quota monitoring and kickstarter system:
1. Executing a Kickstart probe on an inactive account erroneously updated the active account or duplicated the active account's metrics into the inactive account's SQLite record.
2. The direct statusline parser (`sync_quota_from_statusline_file` in `src-tauri/src/kopeng/stats.rs`) queried `session_contexts[cid]` instead of `session_windows[cid]` when looking up conversation-level quota arrays in `~/.local/state/herdr/plugins/herdr-agent-quota/agy-statusline.json`. Because `session_contexts` contains context cache objects rather than window arrays, session lookups failed and fell back to `val.get("windows")` (the global root window representing the active account's latest session).
3. `execute_post_agent_response` in `src-tauri/src/mcp.rs` did not trigger quota sync or emit `AccountQuotaUpdated` events upon turn completion, resulting in stale UI gauges until manual refreshes.

---

## 2. Technical Evaluation & Decisions

- **Herder Plugin Evaluation:** Evaluated building a dedicated Herder plugin/hook (`herdr-kapawu-bridge`) vs. utilizing the direct native Antigravity statusLine channel.
  - **Decision:** Selected Option 2 (Direct Statusline Reader with Deterministic Session Attribution) for immediate zero-dependency, self-contained execution within the Wang-Kapawu daemon.
- **Canonical Model Pools:** Maintained canonical grouping mapping all Gemini family models (`gemini-3.8-flash`, `gemini-3.7-flash`, `gemini-3.1-pro`, etc.) to `("Agy", "gemini-pool")` and third-party models (`claude-sonnet-4-6`, `claude-opus-4-6-thinking`, `gpt-oss-120b-medium`) to `("Anthropic", "3p-pool")`.

---

## 3. Implemented Changes

1. **`src-tauri/src/kopeng/stats.rs`:**
   - Factored out `parse_statusline_json_for_session` to parse `session_windows` and `session_models` keyed deterministically by `conversation_id`.
   - Fixed `sync_quota_from_statusline_file` to strictly query `session_windows` before considering root fallback, preventing cross-account data leakage.
   - Updated `probe_account_quota` so both Gemini and Claude probes correctly update `herder_probe_succeeded` and extract `conversation_id` from headless outputs when running fallback.
   - Added unit test `test_parse_statusline_json_for_session` covering discrete multi-account session parsing and root fallback behavior.

2. **`src-tauri/src/mcp.rs`:**
   - In `execute_post_agent_response`, added automatic `sync_quota_from_statusline_file(agy_conversation_id.as_deref(), None)`.
   - Added `AccountQuotaUpdated` event emission over `tx_ext` on turn completion.

---

## 4. Verification & Validation
- **Backend Unit Tests:** `cargo test --lib kopeng::stats::tests` executed 6/6 tests successfully. Global `cargo test --lib` executed 15/15 tests successfully.
- **Frontend Build:** `npm run build` (`vue-tsc --noEmit && vite build`) compiled clean with zero type errors.
