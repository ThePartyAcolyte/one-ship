# Forge Provider Abstraction & Project Surface

## Purpose

Define ONE-SHIP's provider-neutral Forge collaboration architecture (`kopeng::forge`), the daemon surface tree for the **Project** workspace tab (`project/{milestones,board,schedule,gantt,forge}`), persisted Tasks-mode round-trip state (`project.last_tasks_node`), and the GitLab (`glab`) collaboration adapter.

## Scope

In scope:
- The `project` daemon surface tree, legacy path migration/sanitization rules, and the `project.last_tasks_node` UI state slot.
- The provider-neutral `ForgeProvider` trait, typed `ForgeError`, neutral `ts-rs` data contracts, and GitHub/GitLab field mappings.
- Provider-neutral daemon commands (`forge_*`), capability actions (`domain = "forge"`), and MCP tools (`forge_*`, `propose_forge_draft_pr`).
- The GitLab (`glab api`) collaboration adapter and capability gating.
- Frontend alignment (`ProjectManagementTab.vue`, `ForgeCollabPanel.vue`, `ForgeTopologyTools.vue`, `forgeCollabStore.ts`).

Out of scope:
- Git remote enumeration, `ForgeProviderKind` classification, and repository search/create/link/clone ([Remote Repositories & Diagnostics](04-remote-repositories-and-diagnostics.md)).
- Forge inbox persistence, read/unread semantics, and triage flags ([Forge Inbox & Triage](06-forge-inbox-and-triage.md)).
- Kanban/Schedule/Gantt/Milestones domain logic ([Project Management & Milestones](07-project-management-and-milestones.md)).
- A first-level `Forge` workspace tab, per-project provider overrides in UI, multi-provider projects, or issue/MR body and comment authoring from the UI.

## Model / Design

### 1. Pre-Migration Baseline (Historical Context)

Prior to the provider-neutral Forge refactoring, the `Project` workspace tab implemented `Tasks` and `Forge` via temporary frontend-local state over an older daemon surface tree:

| Legacy UI | Legacy Surface Path | Legacy Mechanism |
| :--- | :--- | :--- |
| `Tasks > Board` | `project/board` | Real surface node |
| `Tasks > Schedule` | `project/timeline` | Real surface node labeled `"Timeline"` |
| `Tasks > Gantt` | `project/timeline` | Frontend toggle (`showGantt`) stored in `localStorage` (`oneship:project:gantt_mode:<projectId>`) |
| `Forge` | `project/github` | GitHub-specific node, `GithubCollabPanel.vue`, and `githubCollabStore.ts` |

That temporary mapping was removed so session persistence, the Global Voice Controller, and collaboration views address real daemon surface nodes and a provider-neutral backend.

### 2. Project Surface Tree

The daemon surface registry (`src-tauri/src/ui_state.rs`) defines the `project` tab hierarchy:

```text
project                         Tab, required has_project_management_enabled
├── milestones                  Subtab
├── board                       Subtab, default child
├── schedule                    Subtab
├── gantt                       Subtab
└── forge                       Subtab, required has_git
    ├── issues                  View (default child of forge)
    ├── prs                     View
    └── releases                View
```

Design rules:
- `timeline` was renamed to `schedule`; the UI never labels the node `"Timeline"`.
- `gantt` is a dedicated node sharing the same underlying task data as `schedule`, making it independently addressable by session restore and voice navigation.
- `github` was renamed to `forge`; the active provider is resolved from the project's Git remotes rather than encoded in the node ID.
- `forge` requires `has_git`; repositories without `.git` have no external Forge workflow.
- `project` uses `default_child_id = "board"`.
- Surface actions live under `project/forge` (and capability domain `"forge"`), exposing `get_context`, `list_issues`, `get_issue`, `list_prs`, `get_pr`, `list_releases`, `list_inbox`, `mark_item_read`, `set_item_flag`, and `get_unread_counts` with provider-neutral descriptions.
- `ProjectManagementTab.vue` binds to `useUiSurfaceNode<'milestones' | 'board' | 'schedule' | 'gantt' | 'forge'>('project', 'board')`: `forge` (and its descendants) activate Forge mode; `milestones`/`board`/`schedule`/`gantt` activate Tasks mode. All `showGantt`, `readGanttMode`, `writeGanttMode`, `lastTasksView`, and `localStorage` keys are removed.

### 3. Persisted Path Handling & `project.last_tasks_node`

#### Legacy Path Handling

Because `ui_project_state.active_node_path` is persisted in SQLite, incoming commands and persisted state enforce strict zero-backward-compatibility rules on node IDs:

| Legacy Path | Target Path |
| :--- | :--- |
| `project/timeline` | `project/schedule` |
| `project/github` | `project/forge` |
| `project/github/issues` | `project/forge/issues` |
| `project/github/prs` | `project/forge/prs` |
| `project/github/releases` | `project/forge/releases` |

- `resolve_and_validate_node_path` strictly rejects legacy paths (`project/timeline`, `project/github`) on incoming navigation/dispatch.
- The original handoff design specified an alias-map migration in `UiStateCache::load_from_db` with an `INFO` log line so `project/timeline` would migrate to `project/schedule` instead of sanitizing to `overview`. In the current `src-tauri/src/ui_state.rs` implementation, `hydrate_sanitized_node_path` sanitizes invalid/legacy persisted paths directly to `DEFAULT_NODE_PATH` (`overview`).

#### `project.last_tasks_node` Slot

While `project/forge` is active, the user's previous Tasks view (`milestones`, `board`, `schedule`, or `gantt`) is not the active surface path. The `project` node declares an explicit inspectable slot:

```rust
UiSlotDef {
    key: "project.last_tasks_node".to_string(),
    label: "Last Tasks View".to_string(),
    value_kind: UiSlotValueKind::Enum(vec![
        "milestones".to_string(),
        "board".to_string(),
        "schedule".to_string(),
        "gantt".to_string(),
    ]),
    default_value: Some("board".to_string()),
}
```

Slot rules:
- Written via `SetSlotValue` when switching from Tasks to Forge; read when switching from Forge back to Tasks (`"board"` is the default).
- Invalid values (e.g., `"inbox"`) fail validation with `UiStateError::InvalidSlotValue` and are never silently coerced.
- The frontend reads slots via a typed store helper (`workspaceLayoutStore.projectSlotValue(projectId, key)`), surviving restarts and remaining inspectable by the voice controller.

### 4. Provider-Neutral Forge Module (`kopeng::forge`)

```text
src-tauri/src/kopeng/forge/
├── mod.rs        Neutral types, ForgeProvider trait, provider resolution, ForgeError
├── github.rs     Adapter over kopeng::github blocking functions
├── gitlab.rs     Adapter over glab api
└── inbox.rs      Provider-neutral inbox read/unread and triage flags
```

#### Remote Selection & Project Scoping

- The daemon command layer resolves the project once and passes `ForgeProjectRef { project_id, path }` to the provider. Providers never resolve projects themselves and therefore cannot access another project's remotes.
- `select_primary_remote(remotes)` resolves the effective remote in priority order: `upstream` > `origin` > first configured remote. Provider classification (`Github`, `Gitlab`, `Unknown`) follows [Remote Repositories & Diagnostics](04-remote-repositories-and-diagnostics.md).
- Self-hosted GitLab/GitHub hosts that do not match `github.com` or `*gitlab*` remain `ForgeProviderKind::Unknown` unless an explicit override (`[plugins.vcs] forge_provider`) is implemented in the future. No heuristic hostname guessing is permitted.

#### GitLab Field Mappings

GitHub issue/PR/release structures define the canonical shape; the GitLab adapter (`gitlab.rs`) maps REST payloads returned by `glab api` onto the neutral types:

| Domain | GitLab Source Field | Neutral Field | Notes |
| :--- | :--- | :--- | :--- |
| Issue | `iid` | `number` | Project-scoped issue ID |
| Issue | `author.username` | `author` | — |
| Issue | `labels` | `labels` (`Vec<String>`) | — |
| Issue | `user_notes_count` | `commentsCount` | — |
| Issue | `web_url` / `updated_at` | `url` / `updatedAt` | — |
| Merge Request | `iid` | `number` | — |
| Merge Request | `draft` / `work_in_progress` | `isDraft` | — |
| Merge Request | `source_branch` / `target_branch` | `headRefName` / `baseRefName` | — |
| Merge Request | `detailed_merge_status` / `merge_status` | `mergeable` | — |
| Merge Request | `head_pipeline.status` | `checksSummary` | `success` → `passing`, `failed` → `failing`, `running`/`pending` → `pending`, else `unknown` |
| Merge Request | `web_url` | `url` | — |
| Release | `tag_name`, `name`, `released_at`, `_links.self` | `tagName`, `name`, `publishedAt`, `url` | — |

### 5. GitLab Adapter (`kopeng::forge::gitlab`)

- **Project resolution:** extracts `(host, namespace/project)` from HTTPS, SSH, or SCP-style remote URLs (`extract_gitlab_host_and_path`), percent-encodes the path, and calls `glab api projects/<encoded_path>` to obtain the numeric GitLab project ID. For self-hosted hosts, `GITLAB_HOST` is set on the `glab` child environment; missing auth or missing `glab` returns `ForgeError::ProviderUnavailable`.
- **Endpoints via `glab api`:**
  - Issues: `GET projects/:id/issues?state=opened|closed|all&per_page=...&labels=...&search=...`
  - Issue detail: `GET projects/:id/issues/:iid` plus `/notes`
  - Merge requests: `GET projects/:id/merge_requests?state=opened|closed|merged|all&per_page=...` (resolves `author_id` via `GET user` when `author_me = true`)
  - MR detail: `GET projects/:id/merge_requests/:iid` plus notes/changes
  - Releases: `GET projects/:id/releases?per_page=...`
- **Execution constraints:**
  - Runs strictly inside `tokio::task::spawn_blocking` via `run_sync_command_with_timeout` (`vcs.forge_cli_timeout_seconds`) with `Stdio::null()` stdin and concurrent stdout/stderr pipe draining.
  - Forces `GIT_TERMINAL_PROMPT=0`; never opens a TTY or silently falls back to GitHub or unauthenticated REST.
  - Draft MR creation (`create_draft_pull_request`) is capability-gated and returns `ForgeError::UnsupportedCapability { provider: Gitlab, operation: "create_draft_pull_request" }` until the branch-push workflow for `glab mr create --draft` is validated.
  - Unit tests use a fake `glab` executable injected onto `PATH`.

### 6. Frontend Integration

- `ForgeCollabPanel.vue` and `forgeCollabStore.ts` replaced the legacy `GithubCollabPanel.vue` and `githubCollabStore.ts`.
- `ForgeCollabPanel.vue` is content-only: it reads the active `project/forge` subview (`issues`, `prs`, `releases`) without rendering a private tab bar, displays capability-driven UI and explicit `ForgeError` states (`ProviderUnavailable`, `UnsupportedCapability`), and emits `@task-taken` into the Tasks workflow.
- `ForgeTopologyTools.vue` renders the provider badge (`GitHub`, `GitLab`, `Unknown`) and repository topology inside the shared Project `SubtabBar` (defined in [Project Management & Milestones](07-project-management-and-milestones.md)).
- `RemoteVcsModal.vue` uses provider-neutral `"Remote Repository"` copy when opened from Forge.

## Contracts

### `ForgeProvider` Trait & Scoping (`src-tauri/src/kopeng/forge/mod.rs`)

```rust
pub struct ForgeProjectRef<'a> {
    pub project_id: usize,
    pub path: &'a Path,
}

pub trait ForgeProvider: Send + Sync {
    fn kind(&self) -> ForgeProviderKind;
    fn collab_context(
        &self,
        project: ForgeProjectRef<'_>,
        force_refresh: bool,
    ) -> Result<ForgeCollabContext, ForgeError>;
    fn list_issues(
        &self,
        project: ForgeProjectRef<'_>,
        query: &ForgeIssueQuery,
    ) -> Result<Vec<ForgeIssueSummary>, ForgeError>;
    fn get_issue(
        &self,
        project: ForgeProjectRef<'_>,
        number: i64,
        force_refresh: bool,
    ) -> Result<ForgeIssueDetail, ForgeError>;
    fn list_pull_requests(
        &self,
        project: ForgeProjectRef<'_>,
        query: &ForgePullRequestQuery,
    ) -> Result<Vec<ForgePullRequestSummary>, ForgeError>;
    fn get_pull_request(
        &self,
        project: ForgeProjectRef<'_>,
        number: i64,
        force_refresh: bool,
    ) -> Result<ForgePullRequestDetail, ForgeError>;
    fn list_releases(
        &self,
        project: ForgeProjectRef<'_>,
        force_refresh: bool,
    ) -> Result<Vec<ForgeReleaseSummary>, ForgeError>;
    fn create_draft_pull_request(
        &self,
        project: ForgeProjectRef<'_>,
        request: &ForgeDraftPullRequest,
    ) -> Result<String, ForgeError>;
}
```

### Neutral `ts-rs` Exported Types

- **Topology & Context:** `ForgeCollabContext` (`provider`, `providerLabel`, `pullRequestLabel`, `hasOrigin`, `originUrl`, `originRepo`, `hasUpstream`, `upstreamUrl`, `upstreamRepo`, `isFork`, `parentRepo`, `parentUrl`, `targetRepo`, `defaultBranch`, `aheadUpstreamCount`, `behindUpstreamCount`, `viewerPermission`, `supportsDraftPullRequest`, `supportsInbox`, `error`).
- **Issues:** `ForgeIssueSummary`, `ForgeCommentItem`, `ForgeIssueDetail`.
- **Pull / Merge Requests:** `ForgePullRequestSummary`, `ForgeReviewItem`, `ForgeCheckItem`, `ForgePullRequestDetail`.
- **Releases:** `ForgeReleaseSummary`.
- **Command Arguments (`deny_unknown_fields`, `camelCase`):** `GetForgeContextArgs`, `ListForgeIssuesArgs`, `GetForgeIssueArgs`, `ListForgePullRequestsArgs`, `GetForgePullRequestArgs`, `ListForgeReleasesArgs`, `CreateForgeDraftPullRequestArgs`, `TakeForgeIssueArgs` (`kopeng::project_mgmt`).
- **Provider Queries:** `ForgeIssueQuery`, `ForgePullRequestQuery`, `ForgeDraftPullRequest`.

### Error Contract (`ForgeError`, implements `ApiError`)

| Variant | HTTP Status | Error Code |
| :--- | :--- | :--- |
| `ProviderUnavailable { provider, reason }` | `503 Service Unavailable` | `ForgeError::ProviderUnavailable` |
| `UnsupportedCapability { provider, operation }` | `501 Not Implemented` | `ForgeError::UnsupportedCapability` |
| `NoRemoteConfigured` / `MissingTargetRepo` | `400 Bad Request` | `ForgeError::NoRemoteConfigured` / `MissingTargetRepo` |
| `Glab` / `UnexpectedOutput` / `InvalidProviderResponse` | `502 Bad Gateway` | `ForgeError::Glab` / `UnexpectedOutput` / `InvalidProviderResponse` |
| `Task` | `500 Internal Server Error` | `ForgeError::Task` |
| `Gh` / `Git` / `Registry` / `Database` | Delegated | Delegated |

### Daemon Commands, Capabilities & MCP Tools

Legacy GitHub-specific collaboration commands were replaced atomically across `api::API_COMMANDS`, `src/types/daemonApi.ts`, `capabilities/forge.rs`, and `mcp.rs`:

| Removed Legacy Command | Provider-Neutral Daemon Command |
| :--- | :--- |
| `get_github_collaboration_context` | `get_forge_context` |
| `list_github_issues` | `list_forge_issues` |
| `get_github_issue` | `get_forge_issue` |
| `list_github_prs` | `list_forge_pull_requests` |
| `get_github_pr` | `get_forge_pull_request` |
| `list_github_releases` | `list_forge_releases` |
| `create_github_draft_pr` | `create_forge_draft_pull_request` |
| `take_github_issue` | `take_forge_issue` |

Repository lifecycle commands remain provider-specific (`search_github_repos`, `create_github_repo`, `search_gitlab_repos`, `create_gitlab_repo`, `link_project_remote`, `check_git_remote_access`, `configure_upstream_remote`, `sync_upstream_fetch`).

- **Unified Capability Core (`src-tauri/src/capabilities/forge.rs`):** registers `domain_id = "forge"`, `required_capability = Some("has_git")`, `linked_surface_path = Some("project/forge")`, exposing `get_context`, `list_issues`, `get_issue`, `list_prs`, `get_pr`, `list_releases`, `list_inbox`, `mark_item_read`, `set_item_flag`, and `get_unread_counts`.
- **MCP Tools (`src-tauri/src/mcp.rs`):** `forge_get_context`, `forge_list_issues`, `forge_get_issue`, `forge_list_prs`, `forge_get_pr`, `forge_list_releases`, `forge_list_inbox`, `forge_mark_item_read`, `forge_set_item_flag`, `forge_get_unread_counts`, and `propose_forge_draft_pr` (user-approval-gated draft PR/MR creation).
- **Config Keys:** `vcs.forge_cli_timeout_seconds` (CLI timeout for `gh`/`glab`/`git` forge calls).

## Implementation Status

| Area | Status | Notes |
| :--- | :--- | :--- |
| `project/{milestones,board,schedule,gantt,forge}` surface tree & `project.last_tasks_node` slot (`ui_state.rs`) | Implemented | — |
| Legacy persisted path alias migration (`project/timeline` → `project/schedule`, `project/github` → `project/forge`) | Partial | Direct dispatch rejects legacy paths; `hydrate_sanitized_node_path` sanitizes them to `overview` rather than remapping via an alias map |
| `ForgeProvider` trait, `ForgeError`, neutral `ts-rs` types, GitHub adapter (`kopeng/forge/{mod,github}.rs`) | Implemented | — |
| GitLab read adapter (`kopeng/forge/gitlab.rs`) & fake-`glab` tests | Implemented | — |
| Capability provider (`capabilities/forge.rs`), MCP tools (`forge_*`, `propose_forge_draft_pr`), `api.rs` commands | Implemented | — |
| Frontend (`ProjectManagementTab.vue`, `ForgeCollabPanel.vue`, `ForgeTopologyTools.vue`, `forgeCollabStore.ts`) | Implemented | — |
| GitLab draft MR creation (`glab mr create --draft`) | Roadmap | `GitlabForgeProvider::create_draft_pull_request` returns `UnsupportedCapability` |
| Self-hosted `[plugins.vcs] forge_provider` override | Roadmap | Unknown hostnames stay `ForgeProviderKind::Unknown` |

## Related Specs

- [Remote Repositories & Diagnostics](04-remote-repositories-and-diagnostics.md)
- [Forge Inbox & Triage](06-forge-inbox-and-triage.md)
- [Project Management & Milestones](07-project-management-and-milestones.md)
- [Project Category Index](README.md)

Supersedes legacy: `39_forge_provider_and_project_surface_handoff.md` (excluding remote registry portion covered in `04-remote-repositories-and-diagnostics.md`)
