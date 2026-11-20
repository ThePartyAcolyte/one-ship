# 2026-11-17 — Forge Provider Abstraction, Project Surface Nodes & Cross-Project Gantt Overlay

## Summary

Implemented Spec 39 (WP1–WP6) and Spec 38 generalized: replaced the temporary frontend surface mapping with real daemon nodes (`project/board`, `project/schedule`, `project/gantt`, `project/forge`), added persisted-path migration and the `project.last_tasks_node` slot, extended `GitStatus` with a full `remotes` registry and provider classification, introduced `kopeng::forge` with a synchronous `ForgeProvider` trait, GitHub and GitLab (`glab api`) adapters, provider-neutral `forge_*` daemon commands, `forge_inbox_items` persistence with `forge-inbox-updated` SSE events, and a cross-project Gantt overlay filter in `ProjectManagementTab.vue`.

## Changes Implemented

### 1. Surface Tree & `project.last_tasks_node` (WP1)
- `src-tauri/src/ui_state.rs`: renamed `project/timeline` → `project/schedule`, added `project/gantt`, renamed `project/github` → `project/forge` (`required_capability = Some("has_git")`), added `project.last_tasks_node` enum slot (`board | schedule | gantt`, default `board`), and added `migrate_legacy_node_path` in `hydrate_from_db_locked` (with `INFO` logging and strict rejection on `dispatch_action`).
- `src/stores/workspaceLayoutStore.ts`: added `projectSlotValue(projectId, key)` getter.
- `src/components/workspace/ProjectManagementTab.vue`: migrated to `useUiSurfaceNode<'board' | 'schedule' | 'gantt' | 'forge'>('project', 'board')`, deleted `showGantt` / `lastTasksView` / `localStorage` mapping, wired `project.last_tasks_node` on Tasks ↔ Forge switches, and added a cross-project Gantt overlay scope filter (`This Project` | `All PM Projects` | `+ <Project>`) backed by `projectMgmtStore.fetchOverlayProjectTasks`.

### 2. Git Remote Metadata & Provider Detection (WP2)
- `src-tauri/src/kopeng/git.rs`: added `ForgeProviderKind` (`Github`, `Gitlab`, `Unknown`) and `GitRemote { name, url, provider }`, added `GitStatus.remotes: Vec<GitRemote>`, read all remotes via `git config --get-regexp '^remote\..*\.url$'`, broadened `has_remote`, and added unit tests.

### 3. Provider-Neutral Forge Contract & Adapters (WP3 & WP4)
- `src-tauri/src/kopeng/forge/mod.rs`: defined `ForgeError` (`thiserror` + `ApiError`), neutral collaboration structs (`ForgeCollabContext`, `ForgeIssueSummary`, `ForgeIssueDetail`, `ForgePullRequestSummary`, `ForgePullRequestDetail`, `ForgeReleaseSummary`), `ForgeProvider` trait, deterministic provider resolution (`upstream` > `origin` > first remote; zero silent fallbacks), and `spawn_blocking` daemon commands.
- `src-tauri/src/kopeng/forge/github.rs`: `GithubForgeProvider` delegating to `kopeng::github::*_blocking`.
- `src-tauri/src/kopeng/forge/gitlab.rs`: `GitlabForgeProvider` backed by `glab api` with `GIT_TERMINAL_PROMPT=0`, typed `ProviderUnavailable` / `UnsupportedCapability` errors, and fake-`glab` unit tests.
- `src-tauri/src/api.rs`, `src/types/daemonApi.ts`, `src-tauri/src/kopeng/project_mgmt.rs`, `src-tauri/src/voice_controller.rs`: replaced the 8 `*_github_*` collaboration commands with `forge_*` commands (`take_forge_issue`, `get_forge_context`, `list_forge_issues`, `get_forge_issue`, `list_forge_pull_requests`, `get_forge_pull_request`, `list_forge_releases`, `create_forge_draft_pull_request`) and moved voice surface actions to `project/forge`.
- Renamed `githubCollabStore.ts` → `forgeCollabStore.ts` and `GithubCollabPanel.vue` → `ForgeCollabPanel.vue`.

### 4. Forge Inbox & Triage Flags (WP5)
- `src-tauri/src/db.rs`: added `forge_inbox_items` table and CRUD helpers.
- `src-tauri/src/kopeng/forge/inbox.rs`: implemented `list_forge_inbox`, `mark_forge_item_read`, `set_forge_item_flag`, `get_forge_unread_counts` with Spec 38 read/updated/new semantics.
- `src-tauri/src/events.rs` & `src/utils/daemonEvents.ts`: registered `forge-inbox-updated` (`ForgeInboxUpdatedPayload`).
- `ForgeCollabPanel.vue` & `forgeCollabStore.ts`: added unread badges, `Inbox` / `Flagged` filters, flag toggle, mark-read toggle, and mark-read on item open.

### 5. Living Documentation (WP6)
- Updated Specs 11, 34, 35, 36, 38, removed Technical Debt #41–#44, updated `TODO.md`, and exported `ts-rs` bindings.
