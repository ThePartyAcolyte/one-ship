# Development Log: Process-Attested MCP Caller Identity

**Date:** 2026-10-07  
**Topic:** Replace CWD-based caller inference and permissive project resolution in the MCP daemon with an identity attested by the kernel and the supervisor (spec 12 §6).

---

## 1. Problem

The MCP endpoint is global (`~/.gemini/config/mcp_config.json` is written once and shared by all agents), so the daemon could not tell which agent called. It inferred the caller from the working directory of the socket-owning process and then fell back through name, rula.toml and active-initializer heuristics (`find_project_for_agent`, `resolve_project_with_fallback`, `infer_agent_id`). Agents also had to pass their own `agent_id`/`project` in most calls, so omitted or wrong values produced mis-attributed requests. Task executions were matched to processes by key suffix (`ends_with(":{exec_id}")`). All of this violates AGENTS.md rule 11.

## 2. Design

Identity is injected by ONE-SHIP, independent of runner and client configuration:

1. `spawn_agent` registers the process in SQLite (`agent_instances`, with stable personas in `agent_personas`) and injects `ONESHIP_AGENT_INSTANCE=<uuid>` into the child environment.
2. On every `tools/call`, the daemon matches the TCP peer socket (full 4-tuple against `/proc/net/tcp`) to its owning PID, reads the variable from `/proc/<pid>/environ`, verifies the PID equals or descends from the registered one, and builds a `CallerContext`.
3. Failures are typed (`CallerError`, `thiserror`); unmanaged callers keep the previous scoping rejection.

## 3. Changes

- `db.rs`: tables `agent_personas`, `agent_instances` and typed CRUD (`KopengError`).
- `caller_identity.rs` (new): `CallerError`, `CallerContext`, `resolve_caller`, `close_stale_agent_instances`; unit tests for address formatting, loopback guard, ancestor walk and missing identity.
- `supervisor.rs`: identity resolved before spawn (unregistered project paths are rejected, except the reserved `system` agent), env injection, registration with kill-on-failure, termination on process exit; fallback response persistence uses an exact registered-path lookup.
- `mcp.rs`: router carries the server port; `tools/call` requires a resolved caller; every handler migrated to `&CallerContext`; `find_registered_project` (exact id/path/name/rula uid, ambiguity is an error); task cancel/query use `format_task_key` instead of suffix matching; per-agent audit-journal scan across projects removed; deleted `resolve_caller_cwd_from_peer_port`, `find_project_for_agent`, `resolve_project_with_fallback`, `resolve_project_by_name_or_path`, `infer_agent_id`, `validate_agent_id`.
- Tool schemas: removed `agent_id`, `originator_agent`, `project_uid`, `project_path` (self-attribution) and `resolved_by`; `project` is an optional explicit target.
- `config.rs`: `supervisor.identity_ancestor_walk_max_depth_count` (default 16).
- `kopeng/prompt.rs`, `kopeng/flag_resolver.rs`, `templates/prompts/*`, `templates/skills/kapawu-initializer/SKILL.md`: agents are no longer told to pass their name or project.
- Specs 12 (§4, §6), 18 and 26 updated.
- `bin/daemon.rs`, `lib.rs`: pass the server port, close stale instances at boot (liveness-checked so a running sibling supervisor owner is not affected).

## 4. Verification

`cargo check --all-targets` clean; `caller_identity` and `prompt` unit tests pass. **Not verified at runtime:** no application process was launched (AGENTS.md rule 6). The checklist in spec 12 §6.8 must be run against `agy`, `claude` and `opencode`.

## 5. Behavior changes to be aware of

- `spawn_agent` fails for project paths that are not registered (except `system`).
- `query_system_logs` without `project` now defaults to the caller's project; global logs require `project: "system"`.
- Running agents started before this change lack `ONESHIP_AGENT_INSTANCE` and are rejected as unmanaged until restarted.

## 6. Phase 3 (2026-10-08): remaining Rule 11 items

- `supervisor.rs`: `resolve_system_working_dir` is now `Result<PathBuf, String>` and fixed (`~/Projects`, else home), independent of registered projects; it no longer takes `projects[0]` nor a single project's path, so the `system` agent can never acquire a project identity. All callers propagate the error; `get_system_agent_state_cmd` reports status `error` and logs.
- Frontend: `activeProject` getters (`coreStore`, `useProjectStore`) return the selected project or `null`, never the first of the list; `setProjects` clears a stale selection instead of selecting `projects[0]`. **UX consequence:** on first run (or after the selected project is deleted) no project is selected until the user picks one.
- Frontend: all 40 `.catch(() => {})` replaced by `reportAsyncFailure` (`src/utils/reportAsyncFailure.ts`); the five per-project fetches in `coreStore.setActiveProject` log operation and project id.
- `mcp.rs`: `log_failure` helper; ~25 discarded results (DB writes, status file, callback prompts, cleanup, snapshots) now log at `ERROR`. Broadcast `send` failures (no subscribers) are intentionally left.
- Checks: `cargo check --all-targets` and `vue-tsc --noEmit` clean.

## 7. Phase 3b (2026-10-08): swallowed errors outside `mcp.rs`

- `logger::log_failure(module, operation, result)` is the shared helper; `mcp.rs` delegates to it.
- `supervisor.rs`: spawn now fails when an explicitly requested account does not exist or its credentials cannot be decrypted (previously `.ok()` silently spawned under another account); the OpenCode default DeepSeek account path propagates DB and decryption errors too. ~20 discarded results (prompt delivery, queue dispatch, keyring writes, init-state and conversation writes, rearm enqueue) now log at `ERROR`. Process-kill/stdin-flush/channel-send results are intentionally left.
- `db.rs`: `db_remove_project` is one transaction with propagated errors; the dependent tables cascade through their foreign keys, so only `task_executions` and `audit_journal` (no FK) are deleted explicitly. This removes the delete against the nonexistent table `project_snapshots` and stops orphaning `audit_journal` rows. `db_get_init_state` propagates row-read errors instead of defaulting to empty strings.
- Frontend: `fetchProjects` rethrows; quota config sync, visual builder undo limit and agent-state refresh log their errors. Pointer-capture `catch {}` in `App.vue` is kept (DOM race, harmless).
- Checks: `cargo check --all-targets`, `vue-tsc --noEmit` clean.

## 8. Left for follow-up

- Swallowed errors not yet reviewed: `kopeng/accounts.rs` (`.ok()` x11, `let _ =` x15), `config.rs` (`let _ =` x5, `sync_antigravity_global_mcp_config` ignores write failures), `conversation.rs`, remaining `.ok()` row reads in `db.rs` (token/metrics columns), the idempotent `ALTER TABLE` migrations in `db.rs` (expected to fail when the column exists).
- `coreStore` and `useProjectStore` both hold `activeProjectId` (duplicated state).
- MCP port is resolved from three different sources (`8765` literal in the in-process server, `system.mcp_port` in skill injection, `mcp.mcp_daemon_port` in the standalone daemon).
- `agent_conversations`, delegations and flags are still keyed by agent display name; migrating them to `agent_uid` is a separate change.
- `rula_project_uid` reads `rula.toml` synchronously inside async handlers (AGENTS.md rule 9.2).
