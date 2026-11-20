# Audit — Conversation & Context Continuity
**Date:** 2026-10-08 | **Scope:** conversation lifecycle, runner/account/model resolution, context handoff, session journal, snapshots  
**Target design:** `docs/specs/30_agent_thread_conversation_and_context_continuity.md`  
**Method:** read-only inspection of code and of the live SQLite database (`~/.config/one-ship/oneship.db`). Each finding lists its evidence; items marked *unverified* were inferred, not observed.

> Status values: `OPEN` (no work started), `SPEC'D` (addressed by a phase of Spec 30), `DONE`.

---

## Findings

| ID | Sev. | Finding | Evidence | Spec 30 | Status |
|---|---|---|---|---|---|
| F1 | 🔴 | Changing runner **archives** the active conversation and starts clean; no context is carried over. | `db.rs:3345-3356` (`db_get_active_agent_session`: runner mismatch → `UPDATE … status='archived'`, returns `None`) | §4 hybrid rule, Phase B/C | **DONE 2026-10-08 (Phase B)** — runner change closes the Conversation as `superseded` and opens a clean one; `db_get_active_agent_session` no longer archives. Context carry-over awaits Reopen/Brief (Phase C/E) |
| F2 | 🟠 | Every spawn sends the persona prompt and instructs the model to greet via `post_agent_response`: one model turn per start before the user speaks. | `supervisor.rs` `launch_agent` (`should_inject_persona`); `kopeng/prompt.rs` "MANDATORY STARTUP DIRECTIVES" | §4, Phase A | **DONE 2026-10-08** — spawn sends nothing; preamble rides the first `send_input`; `insert_readiness_stub` adds the synthetic turn; greeting directive removed. Not yet exercised at runtime |
| F3 | 🟠 | Native resume exists only for `agy` (`--conversation`) and OpenCode ACP (`session/load`). The `claude` runner has no resume path. | `supervisor.rs:1141` (`--conversation`), `supervisor.rs:1419` (`session/load`); no Claude resume flag anywhere | §4, Phase B | OPEN |
| F4 | 🟠 | One column, `agy_conversation_id`, plays the role of "native session id" for all runners and is also written for OpenCode sessions. | `db.rs` `AgentConversationRecord`; `supervisor.rs:1458` (ACP session stored in `st.agy_conversation_id`) | §3 `conversation_native_sessions` | **DONE 2026-10-08 (Phase B)** — `conversation_native_sessions` table; the `agy_conversation_id` column is gone from conversations; OpenCode sessions are now recorded too. The Minutes journal and `agent_session_metrics` still key on `agy_conversation_id` (Phase D) |
| F5 | 🔴 | A conversation's `runner` is `NOT NULL DEFAULT 'agy'`, so it is never empty and always outranks the persona (`rula.toml`). Editing a persona's runner never changes an existing conversation. | `db.rs` table DDL; `supervisor.rs:2665` precedence (explicit → conversation → persona); frontend hydration `conv?.runner \|\| info?.runner` (`FloatingChatOverlay.vue`). Observed: `conv-8-frontend` has `runner=agy` | §7, Phase B | **DONE 2026-10-08 (Phase B)** — overrides are nullable; `apply_conversation_config` stores a value only if it differs from the persona; frontend hydration unchanged (`conv \|\| persona`) and now correct. Not yet exercised at runtime |
| F6 | 🟠 | Account is stored per conversation, but spawning an `agy` agent rewrites the single global keyring credential and marks that account active; a keyring watcher then syncs the DB flag back. With several conversations the "active" account changes with the last spawn. | `supervisor.rs:1197` (`db_set_active_account` on spawn); log lines "Keyring modification detected"; observed: project 3 conversation has no account, project 8 conversation pins `thepartyacolyte` | §7 (partial); needs separate account-isolation design | OPEN |
| F7 | 🟠 | The Minutes journal is keyed by `agy_conversation_id`, not by the ONE-SHIP conversation, so a Conversation cannot be joined to its Minutes for a Brief, and non-`agy` runners write synthetic ids (`sess-<project>-<agent>`). | `db.rs` `SessionJournalRecord`; `mcp.rs` `execute_record_session_minutes` (fallback id `sess-{}-{}`) | §3, Phase D | **DONE 2026-10-08 (Phase D)** — journal has `conversation_id`; synthetic ids removed |
| F8 | 🟡 | The only handoff is a markdown built in the UI from Minutes; there is no backend Brief and no bound on its size. | `ContextTransferModal.vue:54` (`generatedHandoffMarkdown`) | §5, Phase C | **DONE 2026-10-08 (Phase C)** — backend `brief.rs`; modal consumes it. State report/open items deferred (Spec 30 "Phase C as built") |
| F9 | 🟠 | No summary field on a Conversation, and no search or reopen capability for agents or the user. Semantic search (Spec 22 Phase 6) is still an open TODO. | `AgentConversationRecord` has no summary; `TODO.md` Phase 8 item; no `search_conversations` tool in `mcp.rs` | §6, §8, Phase D/E | **DONE 2026-10-09 (Phases D, E)** — summary field, FTS search, reopen |
| F10 | 🟡 | The MCP tool `resume_agent_session` takes an `agy` conversation id; it cannot target a ONE-SHIP Conversation or another runner. | `mcp.rs` `execute_resume_agent_session` → `restart_agent_with_session` | §8 (`reopen_conversation`) | **DONE 2026-10-09 (Phase E)** — `resume_agent_session` removed, replaced by `reopen_conversation` |
| F11 | 🟠 | Snapshots are triggered by `agy` presence lock files and keyed by the `agy` conversation id; Claude and OpenCode conversations get no PRE/POST snapshots. | Spec 16 §triggers (`~/.gemini/antigravity-cli/presence/<id>.lock`); `snapshots.rs` `conversation_id` | §10 decision 5 | OPEN |
| F12 | 🟡 | Terminology collision: "conversation" denotes a ONE-SHIP topic cycle (`agent_conversations`), an `agy` native session id (`agy_conversation_id`, snapshot `conversation_id`, MCP `conversation_id` arguments) and the UI chat stream. "Session" is used for the same thing in Spec 22, the journal and `resume_agent_session`. | `docs/specs/22…`, `db.rs`, `mcp.rs` tool schemas | §2 | OPEN |
| F13 | 🟡 | Persona edits in `rula.toml` only reach an agent at the next spawn that injects a preamble; resumed sessions keep the old persona text. Behavior is implicit, not documented. | `supervisor.rs` `launch_agent` (`should_inject_persona` false when resuming) | §4 | OPEN (documented in Spec 30 §4: preamble only for sessions without history) |
| F14 | 🔵 | Selecting an account for the `claude` runner has no effect (the CLI uses its own login). Now logged at `WARN`, but the UI presents it as a real choice. | `supervisor.rs` `spawn_agent` WARN; chat account selector | §7 | PARTIAL |

---

## New observations while implementing Phase A

| ID | Sev. | Finding | Evidence |
|---|---|---|---|
| F15 | 🟠 | For a runner with no native resume (`claude`), `session_resume_id` is `None` but `has_existing_turns` can be true, so a relaunch receives **no** Persona Preamble and has no memory: the persona is silently lost. | `supervisor.rs` `launch_agent` condition `session_resume_id.is_none() && !has_existing_turns` |
| F16 | 🟠 | `prompt_agent` with an omitted `project_path` picks the project by matching running processes by agent name, a Rule 11 violation. | `supervisor.rs` `prompt_agent` (fallback branch) |

Both were inputs for Phase B (Native Session ownership of "has history"). **F15 DONE 2026-10-08** (Persona Preamble is injected whenever there is no session to resume). **F16 DONE 2026-10-08** — `prompt_agent` returns an error when `project_path` is omitted for a non-`system` target; the process-name matching branch is deleted. Callers (UI, `report_task_status` callbacks, `delegate_agent_task`) already pass the project; a callback with no resolvable project (caller is the project-less `system` agent) now logs the error instead of guessing.

## New observations while implementing Phase B

| ID | Sev. | Finding | Evidence |
|---|---|---|---|
| F17 | 🟠 | Several existing unit tests call `init_database()` and write to the maintainer's real database (`~/.config/one-ship/oneship.db`). Running `cargo test --lib` therefore triggered the Spec 30 schema reset (backup written) on the live database instead of a temporary one. | `db.rs` tests (`test_init_flags_crud_with_options`, …); observed 2026-10-08: `oneship.pre-spec30-1791513078.db` created by a test run |
| F18 | 🟡 | `oneship.db` is about 3.3 GB with a 1.6 GB WAL, while a `VACUUM INTO` copy is 8.5 MB: the file is mostly free pages and the WAL is not being truncated. Cause not investigated. | file sizes in `~/.config/one-ship/` |

## Verified Observations (database, 2026-10-08)

- `agent_conversations`: the active `frontend` conversation of project 8 has `runner=agy`, `model=gemini-3.8-flash`, `account_id=b637…` (`primary@example.com`); the active one of project 3 has an empty `account_id`.
- `accounts`: two Google accounts (one active), one DeepSeek key, one Claude account (`provider='claude'`).
- `agent_instances` (Spec 12 §6): rows are created for `agy` agents and the live process carries `ONESHIP_AGENT_INSTANCE`.

## Risks of the Target Design

| Risk | Mitigation |
|---|---|
| A Brief reconstructs the conversation, not the runner's tool state or file reads | Brief header states this; keep native Resume when the runner is unchanged |
| Token cost of long Briefs | Hard caps (`brief_recent_turns_count`, `brief_max_bytes`) and priority-ordered truncation |
| Agents that never write a summary leave Conversations unsearchable | Fall back to topic + recent turns + Minutes digest; lexical search first |
| Existing conversations pinned to `agy` by the old default | One-time migration rule (Spec 30 §10.4) |
| Account isolation (F6) is not solved by this spec | Track separately: `agy` spawn rewrites one global keyring entry; whether concurrent `agy` agents keep distinct identities is **unverified** (maintainer recalls they do) |
| Large cross-layer change | Phased plan A–E, each phase atomic across backend, frontend, prompts and specs |

## Suggested Order

1. Approve terminology (Spec 30 §10.1) and merge it into `docs/01_ubiquitous_language.md`.
2. Phase A (silent spawn, Readiness Stub, Persona Preamble on first user message).
3. Phase B (Native Session table, nullable overrides, precedence, runner switch without archiving).
4. Phases C–E.
5. In parallel, decide F6 (account isolation) separately.
