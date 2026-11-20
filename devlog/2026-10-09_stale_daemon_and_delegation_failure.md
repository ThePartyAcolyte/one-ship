# Development Log: Stale Daemon, Lost Agent Messages, Silent Delegation Failure

**Date:** 2026-10-09
**Topic:** Incident in project `l10n_cl_banking` (agents `bosmang`, `frontend`), investigated from `oneship.log` and `oneship.db`.

## Findings
- `oneship-daemon` was built 2026-10-05 and still running; the database had been migrated by Spec 30 Phase B on 2026-10-08. The daemon's `db_get_or_create_active_conversation` selected the dropped column `agy_conversation_id` (`no such column`), so every `post_agent_response` and fallback error turn was dropped: `agent_chat_turns` held only user turns. The daemon also predated process-attested identity, so the delegation was recorded as `normalizer -> frontend` instead of `bosmang`.
- `frontend` received the delegated prompt and failed 4 s later with `Individual quota reached… Resets in 131h` (account `d9bfc59a`). The error turn could not be stored (above), the delegation stayed `pending`, and the originator was never told.
- UI: `launchCurrentAgent` sent the selector state (`runner: 'agy'`, the default shown before the persona loaded) as an explicit launch argument, overriding the `claude` persona and creating an empty Conversation that was immediately `superseded`. Overlapping config hydrations cleared a boolean flag early, so value assignments were treated as user edits and restarted the agent right after launch.

## Done
- `post_agent_response` returns `isError` when the Conversation or the chat turn cannot be stored; the fallback persistence logs `ERROR`.
- `report_agent_failure` (supervisor): `system` turn in the target chat, delegation marked `failed`, originator notified; wired to the turn-error and process-exit sites.
- `BUILD_ID` (`build.rs`), `GET /api/build`, `serverInfo.build_id`, `get_build_id_cmd`, `coreStore.checkDaemonBuild()`; `init_database` rejects a newer `user_version`.
- `FloatingChatOverlay.vue`: launches pass no runner/model/effort/account; the redundant pre-launch persist is removed; hydration uses an in-flight counter.
- Spec 12 updated. `cargo check --all-targets` and `vue-tsc --noEmit` clean; nothing exercised at runtime.

## Second pass (same day, after the first fixes)
- `frontend` was switched to `claude` (the persona's runner). The supervisor spawned `claude --dangerously-skip-permissions` with no headless flags, and it exited with code 1 three times. The earlier "working" `frontend` only ran because the launch race forced `agy`. `spawn_agent` now refuses `claude` explicitly; TECHNICAL_DEBT #24.
- Regression from `report_agent_failure`: it notified the originator recorded in the old delegation (`normalizer`, wrong because of the stale daemon) through `prompt_agent`, which launched `normalizer`; it also closed an unrelated delegation from 2026-10-04 (`task-1791299677`). Now it only touches a delegation dispatched after the failing process started, never launches the originator, and includes the last output lines of the dead process in the chat turn.

## Third pass (same day)
- **"Error state in every project"**: `agent-error` carried no `project_path` and `dialogueStore.agentErrors` was keyed by agent name alone, so one failure of `frontend` lit the rose LED of every project that has an agent called `frontend`. Now `agent-error` and the exit-time `agent-status-report` events carry `project_path`; `agentErrors` and `pendingRetryPrompts` use the `<project>::<agent>` key; an event without `project_path` is reported in the status log and not attributed; `clearAgentError`/`getAgentError` require the project.
- **Log rotation**: `get_git_status` logged a `WARN` per dirty project on every hydration poll (~780 lines per 15 min per project), which rotated the incident logs out within minutes. A dirty tree is not an anomaly; the `DEBUG` line with the counts remains.
- **Stale replay**: the agent state watcher started with timestamp 0, so every start re-broadcast `last_agent_status.json` (a `workflows` completion from 2026-10-04). It is now seeded with the file's timestamp.
- **Lost bosmang messages recovered from `agy`**: the store `854f2bbe-….db` holds `steps` as protobuf; the `post_agent_response` calls are `call_mcp_tool` steps (type 15, field 20.7) with JSON arguments. Four were found (ts 1791518213, 1791518752, 1791521030, 1791521995). After the user's last message the agent kept working (SCSS/JS/XML edits in `l10n_cl_banking`, steps 142-203) and never posted a reply. The Native Session row for `854f2bbe` is still the resumable one, so a relaunch of `bosmang` resumes with full context.
- Restoring those rows into `oneship.db` (script `restore_bosmang.py` in the session scratchpad) was **not applied**: the write to the live database was denied by the permission classifier.

## Fourth pass: blank agent terminal, activity in every project
- **Blank terminal**: `FloatingChatOverlay` and `ArtifactPanel` called `read_terminal_output` with the bare agent name and no `project_path`; since the loose suffix match was removed, the key `bosmang` matched no process and the pane stayed empty. Both now pass the project and the command rejects a bare name without one.
- **Activity in every project**: `agent-step-update` wrote `activeSteps[<agent>]` besides the composite key and `getActiveStep` fell back to it, so the sidebar LED of every project with a same-named agent showed "working"; `getConversation` had the same fallback (pending-approval and unread LEDs). The aliases are removed. `closeAndKillAgent` also reset the status of the agent in every project; it now touches only the given project. Step DONE events emitted without `project_path` (process termination, exit, flag resolution) now carry it.
- The thought bubble and tool steps still depend on `agent-step-update`; they were reaching the right key but were also overwriting the alias read by other projects.
- Dead code `AgentList.vue` and `AgentDialogueWorkspace.vue` (no importer) deleted; `CLAUDE.md -> AGENTS.md` symlink created after the user granted permission rules.
- A no-op `sed -i` on `/dev/null` was run by mistake while inspecting `AgentTaskbar.vue`; it changed nothing.

## Fifth pass: name-keyed state removed
- `agentProjectMap` removed. `resolveAgentProjectPath` no longer scans projects, the active conversation record or initializing projects; it answers from the open chat, `activeProjectPath` (set by an explicit selection) or the active project when the agent belongs to it. `getActiveProjectPath` lost its own copy of those heuristics.
- Launch guard keyed per `(project, agent)`; `ConversationPanel.launchCurrentAgent` launched in the first project with a same-named agent and now uses the chat's project. `ensureAgentRunning` no longer falls back to the active project when the given path matches none.
- `agentOutcomes` keyed per `(project, agent)` with `getAgentOutcome`; `last_agent_status.json`, `write_init_status` and `execute_report_task_status` write `project_path`; the initializer observer only updates the project the report belongs to (it updated every initializing project).
- `cargo check --all-targets` and `vue-tsc --noEmit` clean; nothing exercised at runtime.

## Open
- The bosmang restore is blocked by the permission classifier: bosmang has been used since (new turns 20-36 with a pending approval on turn 36), the script `restore_bosmang.py` renumbers the conversation's turn ids, so run it with the app closed or reload the chat after it. A backup of the turns is written before it runs.
- Restore of the four `bosmang` agent turns, close of the stray `normalizer` and `frontend` Conversations, and the delegation records need a maintainer-run script against `oneship.db` (see the third pass).
- `agentOutcomes` (coreStore) is still keyed by agent name and consumed for `initializer` without a project; it shares the cross-project flaw fixed for errors.
- `conv-8-frontend-2f971a4d` still carries `runner=agy` from the faulty launch (persona says `claude`); change the runner in the chat selector to correct it.
- `run_dev.sh` still starts the daemon once; it is not rebuilt on source changes (only detected now).
- `find_agy_session_for_agent` remains a loose lookup in `post_agent_response`.
