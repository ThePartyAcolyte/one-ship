# Spec 39: Forge Provider Abstraction & Project Surface
**Document Version:** 0.2.1  
**Status:** IMPLEMENTED (WP1–WP5: surface tree `project/{board,schedule,gantt,forge}`, `kopeng::forge` GitHub + GitLab providers, and `forge_inbox_items` Spec 38). Remaining follow-ups: §8 GitLab draft MR capability, §13.7 `[plugins.vcs] forge_provider` override, §14 `AppDatePicker`.  
**Domain:** Daemon Surface Tree, Schedule/Gantt Persistence, Provider-Neutral Forge Reads, GitLab Collaboration, Forge Inbox  
**Maintainer:** ONE-SHIP / Omarchy Development Team

---

## 1. Purpose & Pre-Migration Baseline (Historical)

> [!NOTE]
> The temporary frontend mapping described below was superseded by the implemented WP1–WP5 daemon surface (`project/board`, `project/schedule`, `project/gantt`, `project/forge`), `kopeng::forge` providers, and `forge_inbox_items`. Kept below solely as historical context for the migration.

Prior to WP1–WP5, the `Project` tab implemented `Tasks` and `Forge` via a temporary mapping over the daemon surface tree:

| UI | Current surface path | Implementation |
| :--- | :--- | :--- |
| `Tasks > Board` | `project/board` | Real node |
| `Tasks > Schedule` | `project/timeline` | Real node, labelled "Timeline" |
| `Tasks > Gantt` | `project/timeline` | Frontend-local toggle, stored in `localStorage` |
| `Forge` | `project/github` | GitHub-specific node and panel |

Current frontend-only state:
- `showGantt`, `lastTasksView`, `readGanttMode`, `writeGanttMode` and the `tasksView` computed live in `ProjectManagementTab.vue`.
- The Gantt selection is stored per project in `localStorage` with `oneship:project:gantt_mode:<projectId>`.
- Forge is shown for any git project (`project.has_git`); without a remote it renders a link-repository empty state.
- The provider is not abstracted: `GithubCollabPanel.vue` and `githubCollabStore.ts` call the `github_*` daemon commands.
- The GitHub surface node (`project/github`) is labelled "GitHub" and exposes GitHub-specific actions.
- The external inbox/read-state layer defined by Spec 38 is not implemented.

Current backend support relevant to this handoff:
- `kopeng/github.rs` already exposes blocking collaboration functions (`resolve_collab_context_blocking`, `list_github_issues_blocking`, `get_github_issue_blocking`, `list_github_prs_blocking`, `get_github_pr_blocking`, `list_github_releases_blocking`).
- `kopeng/gitlab.rs` only covers repository discovery/creation (`search_gitlab_repos`, `create_gitlab_repo`); it has no issue/MR/release reads.
- `RemoteVcsModal.vue` already supports GitHub, GitLab and direct remotes for repository linking/creation.
- `GitStatus.has_remote` currently means "origin URL exists or the current branch tracks an upstream"; it is not a general remote registry.

This spec defines the daemon changes that remove the temporary frontend mapping and complete the provider-neutral Forge layer. It does not authorize implementation by itself; the open decisions in §13 must be confirmed with the user first (AGENTS.md §1.1).

---

## 2. Target Surface Tree

```text
project                         Tab, required has_project_management_enabled
├── board                       Subtab, default child
├── schedule                    Subtab
├── gantt                       Subtab
└── forge                       Subtab, required has_git
    ├── issues                  View
    ├── prs                     View
    └── releases                View
```

Decisions:
- Rename `timeline` to `schedule`; the UI no longer calls the node "Timeline".
- Add `gantt` as a real node. The Gantt is a presentation of the same task data but a distinct navigation target so session restore and the voice controller can address it.
- Rename `github` to `forge`; the provider is resolved from the project's remotes, not from the node id.
- `forge` keeps `required_capability = has_git`; a project without `.git` has no external flow.
- `project` keeps `default_child_id = board`.
- Surface actions move from `project/github` to `project/forge`. Their ids (`list_issues`, `get_issue`, `list_prs`, `get_pr`, `list_releases`) stay; descriptions become provider-neutral. Spec 38 actions (`mark_item_read`, `set_item_flag`, list filters) are added to the same node.
- Voice examples that reference `project/github/prs` become `project/forge/prs`.

The frontend must stop mapping two UI views onto `project/timeline` and one mode onto `project/github`:
- `ProjectManagementTab.vue` uses `useUiSurfaceNode<'board' | 'schedule' | 'gantt' | 'forge'>('project', 'board')`.
- Mode resolution: `forge` (and its children) → Forge mode; `board`/`schedule`/`gantt` → Tasks mode.
- `showGantt`, `readGanttMode`, `writeGanttMode`, `lastTasksView` and the localStorage key are deleted.

---

## 3. Persisted Path Migration

`ui_project_state.active_node_path` is persisted in SQLite. Renaming nodes must migrate stored rows without losing the user's last view.

| Legacy persisted path | New path |
| :--- | :--- |
| `project/timeline` | `project/schedule` |
| `project/github` | `project/forge` |
| `project/github/issues` | `project/forge/issues` |
| `project/github/prs` | `project/forge/prs` |
| `project/github/releases` | `project/forge/releases` |

Implementation constraints:
1. Apply the alias map in `UiStateCache::load_from_db` before `resolve_and_validate_node_path`, once per project, and log an `INFO` migration line with the project id and both paths.
2. Do not accept legacy paths in `dispatch_action`; incoming navigation uses only the new ids (zero backward compatibility).
3. The alias map is a data migration, not a compatibility shim. Keep it small and documented; remove it only if the project explicitly abandons persisted state.
4. `project/timeline` currently sanitizes to `overview` when invalid. After the rename, a migrated path must resolve to `project/schedule`, never fall back to `overview`.
5. Update the `ui_state.rs` registry tests (`project/timeline`, `project/github` expectations around the resolver tests) to the new tree and add a migration test that seeds a legacy `DbUiProjectState`.

No database schema change is required for the path rename.

---

## 4. `project.last_tasks_node` Slot

Forge is a mode over `project/forge`; while it is active, the previous Tasks child (board/schedule/gantt) is not the active surface path. The UI needs a persisted, inspectable value to return to the last Tasks view.

Add to the `project` node:

```rust
UiSlotDef {
    key: "project.last_tasks_node".to_string(),
    value_kind: UiSlotValueKind::Enum(vec![
        "board".to_string(),
        "schedule".to_string(),
        "gantt".to_string(),
    ]),
    default_value: Some("board".to_string()),
}
```

Rules:
- Written when the user switches from Tasks to Forge (`SetSlotValue`).
- Read when the user switches from Forge back to Tasks; `board` is the only default.
- An invalid value is rejected by the existing enum validation (`UiStateError::InvalidSlotValue`), never silently coerced.
- The daemon does not auto-remember the path; the explicit slot is visible to the voice controller and survives restarts.
- Frontend: add a typed getter (e.g. `workspaceLayoutStore.projectSlotValue(projectId, key)`) instead of reading `uiSnapshot` ad hoc in components.
- `workspaceLayoutStore.applyDaemonSnapshot` already restores `slotValues`; no new slot-specific branch is needed unless a store-level value is introduced.
- Add a slot definition test and a dispatch test that sets/reads the value and rejects `"inbox"` or other invalid values.

---

## 5. Git Remote Metadata & Provider Detection

`GitStatus` is currently origin-centric. For a provider-neutral Forge layer it must expose every configured remote and classify its provider.

Add to `kopeng/git.rs`:

```rust
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GitRemote {
    pub name: String,
    pub url: String,
    pub provider: ForgeProviderKind,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ForgeProviderKind {
    Github,
    Gitlab,
    Unknown,
}
```

Changes:
- `GitStatus` gains `remotes: Vec<GitRemote>`.
- `has_remote` becomes `!remotes.is_empty()` (not only `origin` or a tracked branch).
- Keep `remote_url` as the `origin` URL for existing UI consumers; add `preferred_remote_url` only if a consumer needs the collaboration target. The Forge context resolves the actual target (`upstream` > parent > `origin`).
- Read all remotes with `git config --get-regexp '^remote\..*\.url$'`; trim duplicate names (keep the first URL) and classify:
  - `github.com` → `Github`
  - `gitlab.com` or a host containing `gitlab` → `Gitlab`
  - everything else → `Unknown`
- Self-hosted GitLab/GitHub instances cannot be inferred reliably from the hostname. Do not guess; leave them `Unknown` unless an explicit provider override is added to `rula.toml` (open decision §13).
- Update the `get_git_status` tests (`src-tauri/src/kopeng/git.rs` test around the current `has_remote` assertion) for origin, upstream-only, multiple remotes and no remotes.
- Regenerate `src/types/generated/GitStatus.ts` and the new `GitRemote.ts` / `ForgeProviderKind.ts` with `export_bindings`.
- Update every frontend `git_status.has_remote` consumer to confirm the broader semantics; `ProjectManagementTab` keeps `has_git` for mode visibility and uses `has_remote` only for the panel/empty-state choice.
- Update Spec 09 (VCS) and Spec 11 (remote VCS) with the remote registry and provider classification.

---

## 6. Provider-Neutral Forge Contract

Add a neutral module, recommended layout:

```text
src-tauri/src/kopeng/forge/
├── mod.rs        neutral types, provider resolution, ForgeError
├── github.rs     adapter over kopeng::github blocking functions
└── gitlab.rs     adapter over glab api
```

### 6.1 Neutral types (`ts-rs`)

- `ForgeProviderKind` (from §5).
- `ForgeCollabContext`: provider, provider label, origin/upstream/parent URLs and repos, fork flag, target repo, default branch, ahead/behind counts, viewer permission, `supports_draft_pull_request`, `supports_inbox`, `error`.
- `ForgeIssueSummary` / `ForgeIssueDetail`.
- `ForgePullRequestSummary` / `ForgePullRequestDetail` (internal neutral name; the UI may label GitHub "Pull Requests" and GitLab "Merge Requests" from a provider label, or keep one uniform label — open decision §13).
- `ForgeReleaseSummary`.
- Args structs (daemon command layer, carry `project_id`): `GetForgeContextArgs`, `ListForgeIssuesArgs`, `GetForgeIssueArgs`, `ListForgePullRequestsArgs`, `GetForgePullRequestArgs`, `ListForgeReleasesArgs`, `CreateForgeDraftPullRequestArgs`, `TakeForgeIssueArgs`.
- Provider options (no `project_id`): `ForgeIssueQuery`, `ForgePullRequestQuery`, `ForgeDraftPullRequest`.
- The daemon resolves the project once and passes a `ForgeProjectRef { project_id, path }` to the provider. A provider never resolves a project by itself, so it cannot read another project's remote data.

The GitHub summaries and details are the reference shape. GitLab mappings:
- Issue `iid` → `number`; `author.username` → `author`; `labels` → `string[]`; `user_notes_count` → `commentsCount`; `web_url` → `url`; `updated_at` → `updatedAt`.
- MR `iid` → number; `draft`/`work_in_progress` → `isDraft`; `source_branch`/`target_branch` → head/base; `detailed_merge_status`/`merge_status` → `mergeable`; `head_pipeline.status` → `checksSummary` (`success` → `passing`, `failed` → `failing`, `running`/`pending` → `pending`, otherwise `unknown`); `web_url` → `url`.
- Release `tag_name`/`name`/`released_at`/`_links.self` → tag/name/publishedAt/url.

### 6.2 Trait

Use a synchronous trait and wrap calls in `tokio::task::spawn_blocking` at the daemon command layer (consistent with the existing `gh`/`glab` design and AGENTS.md async-purity rule):

```rust
pub struct ForgeProjectRef<'a> {
    pub project_id: usize,
    pub path: &'a Path,
}

pub trait ForgeProvider: Send + Sync {
    fn kind(&self) -> ForgeProviderKind;
    fn collab_context(&self, project: ForgeProjectRef<'_>, force_refresh: bool) -> Result<ForgeCollabContext, ForgeError>;
    fn list_issues(&self, project: ForgeProjectRef<'_>, query: &ForgeIssueQuery) -> Result<Vec<ForgeIssueSummary>, ForgeError>;
    fn get_issue(&self, project: ForgeProjectRef<'_>, number: i64, force_refresh: bool) -> Result<ForgeIssueDetail, ForgeError>;
    fn list_pull_requests(&self, project: ForgeProjectRef<'_>, query: &ForgePullRequestQuery) -> Result<Vec<ForgePullRequestSummary>, ForgeError>;
    fn get_pull_request(&self, project: ForgeProjectRef<'_>, number: i64, force_refresh: bool) -> Result<ForgePullRequestDetail, ForgeError>;
    fn list_releases(&self, project: ForgeProjectRef<'_>, force_refresh: bool) -> Result<Vec<ForgeReleaseSummary>, ForgeError>;
    fn create_draft_pull_request(&self, project: ForgeProjectRef<'_>, request: &ForgeDraftPullRequest) -> Result<String, ForgeError>;
}
```

Provider resolution reads the remotes from §5; the GitHub adapter delegates to the existing `*_blocking` functions. Unsupported methods return `ForgeError::UnsupportedCapability { provider, operation }`.

### 6.3 Errors

Define `ForgeError` with `thiserror` and `ApiError`:
- `ProviderUnavailable { provider }` (CLI missing, auth missing)
- `UnsupportedCapability { provider, operation }`
- `Gh`, `Glab`, `UnexpectedOutput`, `Task`, `Git`, `Registry`
- `InvalidProviderResponse`

No `Result<T, String>`, no silent fallbacks, no automatic provider switching.

---

## 7. Daemon Commands, Events & Surface Actions

### 7.1 Commands

Replace the collaboration commands with neutral names:

| Removed | Replacement |
| :--- | :--- |
| `get_github_collaboration_context` | `get_forge_context` |
| `list_github_issues` | `list_forge_issues` |
| `get_github_issue` | `get_forge_issue` |
| `list_github_prs` | `list_forge_pull_requests` |
| `get_github_pr` | `get_forge_pull_request` |
| `list_github_releases` | `list_forge_releases` |
| `create_github_draft_pr` | `create_forge_draft_pull_request` |
| `take_github_issue` | `take_forge_issue` |

Keep repository lifecycle commands unchanged: `search_github_repos`, `create_github_repo`, `search_gitlab_repos`, `create_gitlab_repo`, `link_project_remote`, `check_git_remote_access`, `configure_upstream_remote`, `sync_upstream_fetch`.

Every new command must be registered in:
- `src-tauri/src/api.rs` `API_COMMANDS` and `dispatch`.
- `src/types/daemonApi.ts` `DaemonCommands`.
- The `api_commands_match_the_frontend_map` test.
- Its argument/result types derive `ts_rs::TS`; regenerate with `./scripts/run_dev.sh export-bindings`.

### 7.2 Events

- Read-only Forge calls need no new event; the frontend refreshes on demand and after mutations.
- The inbox (§9) adds `forge-inbox-updated` with `{ project_id, provider, unread_count, updated_count }`.
- Register the event in `src-tauri/src/events.rs`, `DAEMON_EVENT_TYPES` and `src/utils/daemonEvents.ts`; document it in Spec 12/35.

### 7.3 Surface Actions & MCP Alignment (Updated by Spec 40 Stage 2A)

- Capability actions live in `src-tauri/src/capabilities/forge.rs` (`domain = "forge"`, `linked_surface_path = Some("project/forge")`) while UI tabs continue rendering `project/forge`.
- Expose `get_context`, `list_issues`, `get_issue`, `list_prs`, `get_pr`, `list_releases`, `list_inbox`, `mark_item_read`, `set_item_flag`, and `get_unread_counts`.
- Legacy `github_*` and `propose_github_draft_pr` MCP tools were replaced atomically in `src-tauri/src/mcp.rs` and `templates/skills/oneship-project-mgmt/SKILL.md` with provider-neutral `forge_*` tools (`forge_get_context`, `forge_list_issues`, `forge_get_issue`, `forge_list_prs`, `forge_get_pr`, `forge_list_releases`, `forge_list_inbox`, `forge_mark_item_read`, `forge_set_item_flag`, `forge_get_unread_counts`) and `propose_forge_draft_pr`.

---

## 8. GitLab Adapter

Existing support: `kopeng/gitlab.rs` uses `glab` for `search_gitlab_repos` and `create_gitlab_repo`; diagnostics already expose `glab_installed` and `glab_authenticated`.

Add read-only collaboration through `glab api`:
- Resolve the project id: `glab api projects/<url-encoded path>` and read `id`.
- Issues: `GET projects/:id/issues?state=opened|closed|all&per_page=...&labels=...&search=...`.
- Issue detail: `GET projects/:id/issues/:iid` plus notes (`/notes`).
- Merge requests: `GET projects/:id/merge_requests?state=opened|closed|merged|all&per_page=...`; `author_id` for "my MRs" after `GET user`.
- MR detail: `GET projects/:id/merge_requests/:iid` plus notes/changes as needed.
- Releases: `GET projects/:id/releases?per_page=...`.
- Draft PR/MR creation: capability-gated; `glab mr create --draft` or the API; do not enable until the branch-push workflow is validated.

Constraints:
- All `glab` calls run on the blocking pool via the existing `on_project` pattern.
- Use `GIT_TERMINAL_PROMPT=0`/`glab` non-interactive behavior; never open a TTY.
- A missing `glab` or an unauthenticated session returns `ForgeError::ProviderUnavailable` with an actionable message.
- Do not silently fall back to GitHub or to the REST API with different credentials.
- Self-hosted GitLab: the base host comes from the remote URL; `glab` must be configured for that host. If not, return an explicit error.
- Add tests with a fake `glab` executable on `PATH` (the existing `github` tests likely use a fake `gh`; follow that pattern).

---

## 9. Forge Inbox & Triage Flags (Spec 38)

Spec 38 defines the read/unread and flag semantics. Generalize it for the provider-neutral layer:
- Table `forge_inbox_items` (or extend the planned `github_inbox_items` with a `provider` column):
  `(project_id, provider, repo, item_kind, item_number)` unique; `github_node_id` becomes provider node id or nullable; `last_seen_updated_at`, `is_read`, `triage_flag`, `updated_epoch_seconds`.
- Provider-neutral commands:
  - `list_forge_inbox(project_id, provider?, unread_only?)`
  - `mark_forge_item_read(project_id, provider, item_kind, number, read)`
  - `set_forge_item_flag(project_id, provider, item_kind, number, flag)`
  - `get_forge_unread_counts(project_id)`
- Event `forge-inbox-updated` (§7.2).
- Surface actions `mark_item_read` / `set_item_flag` on `project/forge`.
- Read semantics per Spec 38 §2: no row = new; `is_read=1` and upstream updated > last seen = updated; read otherwise.
- Frontend: badges on Forge/Issues/PRs, `Inbox`/`Flagged` filters, mark read on open, no localStorage MVP (it was never implemented).
- MCP is optional; if added, use neutral tool names and the same project-ownership checks.

---

## 10. Frontend Follow-ups (Required for the Backend Changes)

These are not backend work but must land atomically with the daemon contract (AGENTS.md §10.10):

1. Rename `GithubCollabPanel.vue` → `ForgeCollabPanel.vue` and `githubCollabStore.ts` → `forgeCollabStore.ts`; update imports and the `@task-taken` flow.
2. `ProjectManagementTab.vue`:
   - use `useUiSurfaceNode<'board' | 'schedule' | 'gantt' | 'forge'>('project', 'board')`;
   - delete `showGantt`, `lastTasksView`, `readGanttMode`, `writeGanttMode`, the localStorage key and the `tasksView` mapping;
   - render Board/Schedule/Gantt from the real child ids;
   - Forge mode = active child `forge`/descendants;
   - read/write `project.last_tasks_node` for the Forge round-trip;
   - keep the `!isForgeActive` gating for Tasks content (the current precedence bug fix remains a valid invariant).
3. `ForgeCollabPanel.vue`:
   - show a provider badge (`GitHub`, `GitLab`, `Unknown`), the provider label for PRs/MRs, and capability-driven actions;
   - show `Unsupported provider`/`Provider unavailable` states from `ForgeError`;
   - render content only: the topology line and provider badge live in `ForgeTopologyTools.vue` inside the single Project panel bar (Spec 34 §6.5).
4. `RemoteVcsModal.vue` already handles GitHub/GitLab/direct remotes; rename user-facing "GitHub Remote" copy to "Remote Repository" if the modal is reused from Forge.
5. Remove the Technical Debt #42 note and the Spec 35 frontend-local mapping once the nodes exist; update Spec 34 §6.

---

## 11. Ordered Work Packages

1. **WP1 — Surface tree and last-Tasks persistence (no provider changes)**
   - Rename `timeline`→`schedule`, `github`→`forge`; add `gantt` and `project.last_tasks_node`.
   - Migrate persisted paths; update registry tests; regenerate bindings.
   - Update `ProjectManagementTab` to the real nodes; remove localStorage and local mappings.
   - Acceptance: Board/Schedule/Gantt are separate nodes; Forge round-trip restores the last Tasks node after restart; old persisted paths migrate; old incoming paths are rejected.
2. **WP2 — Remote metadata and provider detection**
   - Add `GitRemote`/`remotes`, broaden `has_remote`, classify providers; update hydration, tests, generated types, Spec 09/11.
   - Acceptance: archives with origin, upstream-only, multiple and no remotes report correctly.
3. **WP3 — Forge neutral backend + GitHub adapter**
   - Add the neutral types/trait/errors and `forge_*` commands; GitHub adapter delegates to existing blocking functions.
   - Move surface actions to `project/forge`; update voice/MCP references or keep GitHub-specific tools documented.
   - Acceptance: GitHub behavior is identical to today through the new commands; frontend no longer calls `github_*` for collaboration reads.
4. **WP4 — GitLab adapter**
   - Resolve project id, list/get issues, MRs, releases through `glab api`; typed capability errors; fake-`glab` tests.
   - Acceptance: a GitLab remote renders Issues/MRs/Releases; an unauthenticated/missing `glab` shows the provider error; GitHub is unaffected.
5. **WP5 — Forge inbox and flags**
   - Implement Spec 38 generalized; commands, event, surface actions, frontend badges/filters/mark-read.
   - Acceptance: read/updated/new semantics persist across restarts; counts are correct; no localStorage.
6. **WP6 — Docs, bindings and regression**
   - Update Specs 11, 34, 35, 36, 38; remove DT #41/#42; update TODO/devlog; run `export-bindings`; run the requested tests.

---

## 12. Acceptance Criteria & Validation

- `cargo test` (when requested) passes, including:
  - `api_commands_match_the_frontend_map`;
  - surface registry resolution for `project/schedule`, `project/gantt`, `project/forge/*`;
  - persisted-path migration from the legacy tree;
  - `project.last_tasks_node` enum validation;
  - Git remote/provider classification;
  - fake-CLI provider tests for GitHub and GitLab.
- `./scripts/run_dev.sh export-bindings` is run after every `#[ts(export)]` change.
- No blocking `std::process::Command` or `std::fs` in async contexts; provider calls use `spawn_blocking`.
- No `Result<T, String>`, `.unwrap()` in production paths, or silent provider fallbacks.
- No new first-level workspace tab; Forge remains inside `Project`.
- Specs 34/35 describe the real nodes; DT #42 is removed; DT #41 is closed only when the GitLab adapter works.
- Runtime checks: Forge on GitHub, Forge on GitLab, Forge without a remote, Gantt restore after restart, last Tasks node restore, inbox read/unread across restarts, narrow-window SubtabBar.

---

## 13. Open Decisions (Confirm Before Coding)

1. **Node renames:** `project/timeline` → `project/schedule` and `project/github` → `project/forge` (recommended), or keep the ids and change only labels? The UI and voice already use Schedule/Forge.
2. **Command naming:** neutral `forge_*` commands (recommended) or keep `github_*` and pass a provider? Neutral commands avoid a provider-specific public API.
3. **GitLab transport:** `glab api` (recommended; already used for repo management) or GitLab REST with a token (conflicts with the zero-credential architecture).
4. **Inbox storage:** provider-neutral `forge_inbox_items` with `provider` (recommended) or GitHub-only per Spec 38.
5. **Last Tasks node:** the `project.last_tasks_node` slot (recommended) or a daemon auto-memory rule.
6. **GitLab labels:** show "Merge Requests" for GitLab (provider-aware label) or keep "Pull Requests" uniformly.
7. **Self-hosted providers:** explicit `[plugins.vcs] forge_provider` override for unknown hosts, or leave them `Unknown` for now.
8. **MCP tools:** keep `github_*` read-only tools for GitHub and add `forge_*` later, or rename them with the adapter (requires a skill/prompt update in the same cycle).

---

## 14. Out of Scope

- A first-level `Forge`/`Upstream` workspace tab.
- A provider selector inside the Forge view; the provider is detected from the remote.
- Multiple providers for the same project.
- Writing issue/MR bodies or comments from the UI.
- Replacing the native date inputs (`AppDatePicker` remains a frontend TODO).
- Inbox UI implementation details beyond the contract in Spec 38.

---

## 15. References

- Spec 09: VCS & logging architecture.
- Spec 11: Remote VCS integration & repository lifecycle.
- Spec 12: Daemon API, events and ts-rs export.
- Spec 34 §6: current `Tasks | Forge` frontend mapping.
- Spec 35 §6.3: current frontend-local surface presentation and the daemon projection rules.
- Spec 36: voice-controlled surface navigation.
- Spec 38: GitHub inbox and triage flags.
- `devlog/2026-11-17_project_tasks_forge_schedule_gantt.md`: current UI implementation.
- `devlog/2026-11-17_project_forge_backend_handoff.md`: session handoff and agent prompt.
- `TODO.md` → **Backend Handoff — Forge Provider & Project Surface (Spec 39)**.
- `TECHNICAL_DEBT.md` #41–#44.
