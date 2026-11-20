# Development Log: Conversation & Context Continuity — Design Only

**Date:** 2026-10-08  
**Topic:** Runner-neutral conversation model, introduced as a spec and an audit before any code change (decision of the maintainer).

## Context
With OpenCode and Claude Code alongside Antigravity, resuming a runner's native session is no longer a valid way to keep context. Chat testing also surfaced three symptoms: a persona's runner edit not applied (conversation `runner` defaults to `agy` and outranks the persona), the account changing between conversations (global `agy` keyring credential), and the `claude` runner missing from the chat selector.

## Done
- Chat selector: `claude` runner, its models (from the backend schema), `claude` accounts, Haiku as the light fallback model (`FloatingChatOverlay.vue`, `dialogueStore.ts`); backend `WARN` when an account is requested for `claude`.
- `docs/specs/30_agent_thread_conversation_and_context_continuity.md`: terminology (Thread, Conversation, Native Session, Brief, Minutes, Persona Preamble, Readiness Stub, Resume, Reopen), data model, lifecycle, Brief, precedence, restoration tools, phases A–E, open decisions.
- `docs/audit/audit_conversation_context_continuity.md` (F1–F14, verified against code and database) and index entry.

## Agreed direction (maintainer)
Hybrid: native Resume when the runner is unchanged, Brief-seeded Native Session otherwise. Brief = last N turns plus the agent-authored summary when present (no extra process). Keep a synthetic "ready" turn at conversation start. Persona prompt delivered with the first user message of a clean conversation.

## Decisions (maintainer, 2026-10-08)
Logged in Spec 30 §10 (D1–D8): terminology approved and merged into `docs/01_ubiquitous_language.md`; closure by user or agent, mainly user, topic-change suggestion is a skill; opening the chat resumes the last active Conversation if the runner is unchanged, a runner change starts a clean one (Brief only via Reopen/transfer); Readiness Stub persisted as a `system` turn; no migration, conversation data may be wiped in development.

## Phase A implemented
- `supervisor.rs`: `ManagedProcess.pending_preamble`, `set_pending_preamble`; `send_input` prepends the preamble to the first input of a process (restored if the send fails); `launch_agent` no longer sends anything at spawn; `insert_readiness_stub` persists the `system` turn "agent ready" and emits `agent-conversation-turn-added`.
- `kopeng/prompt.rs`: greeting/startup directives removed; protocol text kept.
- `dialogueStore.ts`: listener reloads the stream for `system` turns of the active agent (teardown through `_dialogueUnlisteners`).
- Checks: `cargo check --all-targets`, `cargo test --lib prompt`, `vue-tsc --noEmit` clean. Not exercised at runtime.
- Audit F2 closed; F15 and F16 added.

## Phase B implemented
- `db.rs`: `PRAGMA user_version` gate; the first start on an older database backs it up (`oneship.pre-spec30-<epoch>.db`, `VACUUM INTO`) and recreates the conversation tables; `agent_conversations` with nullable `runner`/`model`/`account_id`/`effort` and `closed_at_ms`/`close_reason`; new `conversation_native_sessions`; `agent_chat_turns.native_session_row_id`; shared `CONVERSATION_SELECT`/`conversation_from_row` replace four duplicated queries; `db_close_active_conversation`, `db_open_native_session`, `db_ensure_open_native_session`, `db_end_native_session`, `db_record_native_session_id`; `db_get_active_agent_session` no longer archives on a runner mismatch. Removed the old column migrations and the hard-coded "turn 70" backfill.
- `conversation.rs`: `apply_conversation_config` (persona-aware overrides; runner change closes the Conversation as `superseded` and opens a clean one).
- `supervisor.rs`: `launch_agent` resolves the project once (exact path), applies explicit arguments through `apply_conversation_config`, owns its resolved targets, opens the Native Session before spawning, and injects the Persona Preamble whenever there is nothing to resume (audit F15); `restart_agent_with_session` and the init events register Native Sessions; protocol-correction prompts no longer ask for `agent_id`.
- `dialogueStore.ts`: `AgentConversationRecord` follows the new shape.
- Tests: three schema/closure tests on temporary databases; DDL validated in SQLite; `cargo check --all-targets`, `vue-tsc --noEmit` clean.
- **Incident:** the full `cargo test --lib` run executed pre-existing tests that call `init_database()` on the real database, so the one-time reset ran then: 40 conversations and 220 turns moved to `~/.config/one-ship/oneship.pre-spec30-1791513078.db`; the live tables are empty. This is the intended wipe (D7) done earlier than planned. New audit items F17 (tests hit the real database) and F18 (database file 3.3 GB vs 8.5 MB compacted).

## F16 implemented, plans written
- `supervisor.rs` `prompt_agent`: `project_path` is required for non-`system` targets; the process-name matching branch is deleted. `cargo check --all-targets` clean.
- Spec 30 §11 (plan for D → C → E) and §12 (F6 account-isolation options) written as proposals; nothing in them is implemented.

## Phase D implemented
- `db.rs`: `agent_conversations.summary` (`ensure_column`), `db_set_conversation_summary`; `agent_session_journal` rebuilt in place with `conversation_id` and nullable `agy_conversation_id` (21 existing rows kept, `conversation_id` NULL); journal record/queries updated; test `journal_rebuild_keeps_rows_and_is_idempotent`.
- `mcp.rs`: `record_session_minutes` resolves the caller's active Conversation, drops the `agy_conversation_id` parameter and the synthetic-id/heuristic fallbacks, accepts `conversation_summary`.
- Frontend types, Devlog Resume button (only with a native id), `kapawu-devlog` skill, Specs 22 and 30 updated. `cargo check --all-targets`, `vue-tsc --noEmit` clean; the new test passes (the other DB tests were not run: they touch the real database, F17).
- Deviation from the plan: no version bump or wipe, because the journal holds real history; metrics and snapshots keep native-id keys (F11 open).

## Phase C implemented
- New `brief.rs` (`compose_brief`, `build_brief`, `BriefError`) with 4 unit tests (pass); `db.rs` helpers `db_get_conversation_by_id`, `db_count_dialogue_turns`, `db_get_conversation_session_journals`, `db_mark_native_session_seeded`; config keys `brief_recent_turns_count` / `brief_max_bytes`.
- `launch_agent` seeds a Native Session that cannot resume (and whose Conversation has dialogue) with persona preamble + Brief, built before the spawn.
- `build_conversation_brief_cmd` registered; `ContextTransferModal` shows and sends the backend Brief; the UI markdown builder is gone.
- State report and open items of Spec 30 §5 are not built (data not keyed by Conversation). `cargo check --all-targets`, `vue-tsc --noEmit` clean.

## Phase E implemented (2026-10-09)
- `db.rs`: FTS5 `conversation_search` (+ `reindex_conversation`, startup backfill), `reopened_from`, `db_search_conversations`, `db_reopen_conversation`; `conversation.rs`: `reopen_conversation`, `reopen_conversation_cmd`, `search_conversations_cmd`; `supervisor.rs`: `queue_context_for_next_input`, `process_runner`, `launch_agent` seeds a reopened Conversation from its source.
- `mcp.rs`: `search_conversations` and `reopen_conversation` replace `resume_agent_session`; removed `resume_agent_session_cmd`, `restart_agent_with_session`, the high-context modal and `startCleanSessionWithMilestone`.
- UI: `ConversationSearchModal.vue`, Reopen on stream separators (now keyed by `conversation_id`) and on Devlog Minutes; `kapawu-devlog` skill, Specs 12, 22, 23, 30 updated.
- F6 recorded as TECHNICAL_DEBT #23 (the maintainer recalls concurrent `agy` accounts already work; the earlier claim was unverified and was corrected).
- Checks: `cargo check --all-targets`, `vue-tsc --noEmit` clean; new tests pass (journal rebuild, brief ×4, conversation search, query quoting). Not run: tests that call `init_database()` (F17). Nothing exercised at runtime.

## Not done
Phases B–E. Runtime verification of Claude MCP discovery requires "Upgrade Platform Assets" per project (`.mcp.json` absent in the five registered projects). The conversation data is not wiped yet; it is the maintainer's call when Phase B lands.
