# Unified Capability Core — Stage 2A (`forge`, `files`, `settings`)

**Date:** 2026-11-19  
**Specs:** Spec 34, Spec 39, Spec 40  

## Changes Implemented

1. **Capability Core Extension ([src-tauri/src/capabilities/mod.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/capabilities/mod.rs)):**
   - Registered `forge`, `files`, and `settings` modules in `PROVIDERS`.
   - Added typed `CapabilityError` variants (`Forge`, `Config`, `Semantic`, `UiState`, `Io`) with `ApiError` status/code delegation.
   - Updated `domain_enablement_respects_capabilities_independent_of_hidden_tabs` unit test to cover the 5 registered providers.

2. **Provider-Neutral Forge Capability Provider ([src-tauri/src/capabilities/forge.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/capabilities/forge.rs)):**
   - Registered `domain = "forge"` (`required_capability = Some("has_git")`, `linked_surface_path = Some("project/forge")`) wrapping `kopeng::forge` & `kopeng::forge::inbox`.
   - Actions: `get_context`, `list_issues`, `get_issue`, `list_prs`, `get_pr`, `list_releases`, `list_inbox`, `mark_item_read`, `set_item_flag`, `get_unread_counts`.

3. **Files & Knowledge Capability Provider ([src-tauri/src/capabilities/files.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/capabilities/files.rs)):**
   - Registered `domain = "files"` (`linked_surface_path = Some("files")`).
   - Actions: canonical root-contained `search_files`, `list_directory`, bounded `read_file_excerpt` (`max_lines` clamped `1..=250`, 2 MiB limit), `semantic_search` (preserving existing `crate::semantic::search_knowledge` delegation with `KnowledgeScope` filter support), `set_scope` (`files.explorer_scope` slot), and `open_file` (`files.selected_document_path` slot + `ui-surface-command` event).

4. **Settings Capability Provider ([src-tauri/src/capabilities/settings.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/capabilities/settings.rs)):**
   - Registered `domain = "settings"` (`linked_surface_path = Some("settings")`).
   - Actions: `get_project_config_schema` and strict-JSON `set_project_config_value` (removed legacy string fallback on malformed `value_json`).

5. **Voice Controller Cleanup ([src-tauri/src/voice_controller.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/voice_controller.rs)):**
   - Updated `capability_discover` example domain `'project/forge'` -> `'forge'`.
   - Removed temporary `"settings"`, `"project/forge"`, `"files"` domain injections from `ui_get_state`.
   - Removed migrated arms and file walk helpers from `execute_surface_action`, leaving only `electronics/oscilloscope` (`SurfaceActionTarget::Frontend`) on the legacy fallback path until Stage 2C.

6. **Atomic MCP `github_*` -> `forge_*` Migration ([src-tauri/src/mcp.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/mcp.rs), [templates/skills/oneship-project-mgmt/SKILL.md](file:///home/butcherwutcher/Projects/dev/one-ship/templates/skills/oneship-project-mgmt/SKILL.md), Specs 34/39/40):**
   - Replaced legacy `github_get_repo_context`, `github_list_issues`, `github_get_issue`, `github_list_prs`, `github_get_pr`, `github_list_releases`, and `propose_github_draft_pr` with provider-neutral `forge_get_context`, `forge_list_issues`, `forge_get_issue`, `forge_list_prs`, `forge_get_pr`, `forge_list_releases`, `forge_list_inbox`, `forge_mark_item_read`, `forge_set_item_flag`, `forge_get_unread_counts`, and `propose_forge_draft_pr`.
   - Wrapped external issue/PR/release/inbox payloads with `UNTRUSTED_FORGE_BANNER` and routed read/inbox tools through `call_mcp_capability`.
   - Routed `propose_forge_draft_pr` through `await_user_approval` (`approval_type = "forge_draft_pr"`) + `kopeng::forge::create_forge_draft_pull_request`.

## Verification

1. **Daemon Build & Runtime Health (`./scripts/run_dev.sh status && journalctl --user -u oneship-dev-daemon-build -n 25 --no-pager`):**
   - Build succeeded cleanly: `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 36.82s`.
   - `oneship-dev-daemon` active (`main_pid: 3180419`), listening on port `8765`.
   - Note: `oneship-dev-gui` had exited during an earlier intermediate edit before the `files.rs` `i64` fix and was not restarted per `AGENTS.md` §1.4 (no manual daemon/GUI start/stop).

2. **MCP `tools/list` Live Probe (`POST http://127.0.0.1:8765/mcp`):**
   - Confirmed all 11 new tools registered (`forge_get_context`, `forge_list_issues`, `forge_get_issue`, `forge_list_prs`, `forge_get_pr`, `forge_list_releases`, `forge_list_inbox`, `forge_mark_item_read`, `forge_set_item_flag`, `forge_get_unread_counts`, `propose_forge_draft_pr`) and zero `github_*` tools remaining.

3. **Rule Gate & Fallback Audit (`./scripts/verify_rules.sh` + `grep`):**
   - `grep -nE "unwrap_or|unwrap_or_default|\.ok\(\)|let _ =|catch..." src-tauri/src/capabilities/{mod,forge,files,settings}.rs` -> zero hits.
   - `sed -n '5240,5360p' src-tauri/src/mcp.rs | grep -nE "unwrap_or|unwrap_or_default|\.ok\(\)|let _ ="` -> zero hits.
   - `./scripts/verify_rules.sh` reported zero violations in any Rust files modified in Stage 2A (1 pre-existing failure remains on unrelated Vue modal `bg-black/*` classes).
