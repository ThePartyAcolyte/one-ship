# Remote Repositories & Diagnostics

## Purpose

Define how ONE-SHIP discovers, creates, links, imports and verifies remote repositories across GitHub, GitLab and generic Git hosts without handling credentials, plus the tool diagnostics and installer that support it.

## Scope

In scope: zero-credential architecture, provider CLI usage, repository search/create/link/clone, access verification, diagnostics and tool installation, remote registry and provider classification, Remote VCS modal and toolbar.

Out of scope: issue/PR/release collaboration ([Forge Provider Abstraction](05-forge-provider-abstraction.md)).

## Model / Design

### 1. Architecture & Design Rationale

ONE-SHIP never embeds OAuth capture, PAT storage or custom encryption vaults. VCS authentication and API communication are delegated to official provider CLIs (`gh`, `glab`) or standard Git remote protocols.

```text
┌─────────────────────────────────────────────────────────────┐
│                      ONE-SHIP Hub UI                        │
│   [ <Cloud /> Button ] -> [ Remote VCS Modal ]              │
│   - GitHub Tab           - GitLab Tab        - Direct URL   │
└──────────────────────────────┬──────────────────────────────┘
                               │ daemonCall → POST /api/v1/call/<command>
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                       oneship-daemon                        │
│  - search_github_repos       - search_gitlab_repos          │
│  - create_github_repo        - create_gitlab_repo           │
│  - link_project_remote       - install_missing_tools        │
└──────────────────────────────┬──────────────────────────────┘
                               │ System Process Execution (tokio/std::process)
                               ▼
┌─────────────────────────────────────────────────────────────┐
│               Host VCS Providers & System Git               │
│  - GitHub CLI (`gh`): reads local session (`gh auth token`) │
│  - GitLab CLI (`glab`): reads config (`glab auth status`)   │
│  - System Git (`git`): configures `origin` add/set-url      │
│  - Privileged Installer: `pkexec pacman -S --noconfirm`     │
└─────────────────────────────────────────────────────────────┘
```

Key advantages:

1. **Security isolation:** raw tokens/credentials never reach ONE-SHIP; authentication stays in the host keyring and official CLIs.
2. **Deterministic, symmetric diagnostics:** `run_diagnostics` (`kopeng/diagnostic.rs`, header diagnostics popover) evaluates `git_installed`, `gh_installed`, `gh_authenticated`, `glab_installed`, `glab_authenticated`, `package_manager`, `can_auto_install_tools`. Only `git_installed` is the foundational VCS requirement, preventing false system-wide warnings for GitLab-only or self-hosted users.
3. **Contextual in-modal toolchain installation:** if the GitHub or GitLab tab is selected and `gh`/`glab` is not on `$PATH`, the modal shows an inline prompt to install via `pkexec` using the detected package manager (`pacman`, `apt-get`, `dnf`, `zypper`). `install_missing_tools` supports privileged installation of VCS, RE and embedded toolchain packages across supported distros. After Polkit confirmation, diagnostics re-run without an app restart.
4. **Generic remote agnosticism:** direct Git URLs link self-hosted Git, Bitbucket, Gitea or custom SSH/HTTPS remotes without third-party CLIs.

### 2. Command Primitives & Execution Protocol

**2.1 Repository discovery & search**

- GitHub: `gh repo list --json name,nameWithOwner,url,isPrivate,viewerPermission --limit <vcs.github_repo_list_limit_count>`
- GitLab: `glab api "projects?membership=true&simple=true&per_page=<vcs.gitlab_repo_list_limit_count>"`
- Both lists are filtered client-side by repository/namespace substring.

**2.2 Access permission matrix (before linking)**

- GitHub `viewerPermission`: `ADMIN` / `WRITE` → direct linking enabled; `READ` / `TRIAGE` → warning that direct pushes will be rejected unless forked (`gh repo fork`), with an option to link anyway for pull-only workflows.
- GitLab: evaluated from the membership projects list.

**2.3 Universal remote linkage (`origin`)**

1. Validate URL: non-empty, no whitespace, does not start with `-` (`InvalidRemoteUrl`).
2. Ensure a Git repository: if `.git` is absent, run `git init` first.
3. Configure remote: `git remote set-url origin <remote_url>` if `origin` exists, otherwise `git remote add origin <remote_url>`.

**2.4 Creating a new remote repository**

- GitHub: `gh repo create <name> --public/--private --source=. --remote=origin`
- GitLab: `glab repo create <name> --public/--private --default-branch=main`, followed by `origin` configuration pointing to the new repository web URL.

**2.5 Remote import & access verification** (initializing new projects from GitHub or generic remotes)

1. `check_git_remote_access`: validates URL syntax (`validate_remote_url`); for `github.com` URLs queries `gh repo view <url> --json name,nameWithOwner,isPrivate,viewerPermission,defaultBranchRef`; otherwise falls back to non-blocking `GIT_TERMINAL_PROMPT=0 git ls-remote --exit-code <url> HEAD`. Evaluates reachability, privacy, branch and viewer permission without interactive credential prompts.
2. `clone_and_add_project`: validates target directory name and parent; runs `GIT_TERMINAL_PROMPT=0 git clone <url> <target_path>`; registers the directory (`add_project`); prompts or triggers the Project Initializer for brownfield stack reconnaissance.

### 3. Remote Registry & Provider Classification

- `GitStatus.remotes: Vec<GitRemote>` enumerates all configured remotes via `git config --get-regexp '^remote\..*\.url$'`; duplicate names are trimmed (first URL kept).
- Each URL is classified into `ForgeProviderKind`: `github.com` → `github`; `gitlab.com` or a host containing `gitlab` → `gitlab`; everything else → `unknown`. Self-hosted instances are never guessed from hostnames and stay `unknown`.
- `GitStatus.has_remote` is true when `!remotes.is_empty()` or an upstream branch is tracked (previously: only `origin` URL or tracked branch).
- `remote_url` stays the `origin` URL for existing consumers; the actual collaboration target is resolved by the forge context (`upstream` > parent > `origin`). `preferred_remote_url` is to be added only if a consumer needs it.
- Types:

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

- Tests cover origin, upstream-only, multiple remotes and no remotes; bindings (`GitStatus.ts`, `GitRemote.ts`, `ForgeProviderKind.ts`) are regenerated with `export_bindings`. Frontend `git_status.has_remote` consumers use the broader semantics; `ProjectManagementTab` keeps `has_git` for mode visibility and uses `has_remote` only for the panel/empty-state choice.

### 4. User Experience

**Toolbar (`<Cloud />`)** in the project card upper toolbar and workspace overview:

- Unlinked: dim gray (`text-[var(--dim-color)] opacity-50`), tooltip `"Connect Remote"`.
- Linked: accent (`text-[var(--status-ok-color)]`), tooltip `"Remote: <url>"`.
- Click opens the Remote VCS Modal.

**Modal layout & state machine (`RemoteVcsModal.vue`)**

1. Header & active remote banner: current `origin` URL with provider badge (`[GITHUB]`, `[GITLAB]`, `[GIT REMOTE]`), and an external browser link for HTTP/HTTPS origins.
2. Provider tabs: `[ GitHub ]`, `[ GitLab ]`, `[ Direct Git URL ]`, auto-selected from the origin domain (e.g. `gitlab.com` → GitLab).
3. Contextual installation banner when the selected CLI is missing, with one-click install (`pkexec pacman -S`).
4. Action tabs (Link Existing vs Create New): instant repo search/filter; creation form with name sanitization and private/public toggle; direct URL input for arbitrary endpoints.
5. When the modal is reused from Forge, user-facing "GitHub Remote" copy becomes "Remote Repository".

## Contracts

Served by the daemon operator RPC API (`api.rs`), implemented in `src-tauri/src/kopeng/{github,gitlab,diagnostic}.rs`; UI calls via `daemonCall`.

| Command | Arguments (`camelCase`) | Result |
| :--- | :--- | :--- |
| `run_diagnostics` | `{}` | `DiagnosticState` |
| `install_missing_tools` | `{ toolIds: string[] }` | `null` |
| `search_github_repos` | `{}` | `GithubRepo[]` |
| `check_git_remote_access` | `{ repoUrl }` | `GitRemoteAccessCheck` |
| `clone_and_add_project` | `{ repoUrl, parentPath, name }` | `HydratedProject` |
| `create_github_repo` | `{ projectId, name, isPrivate }` | repository URL (`string`) |
| `search_gitlab_repos` | `{}` | `GitlabRepo[]` |
| `create_gitlab_repo` | `{ projectId, name, isPrivate }` | repository URL (`string`) |
| `link_project_remote` | `{ projectId, remoteUrl }` | `null` |

Config keys: `vcs.github_repo_list_limit_count`, `vcs.gitlab_repo_list_limit_count`.

Repository lifecycle commands stay provider-specific by design: `search_github_repos`, `create_github_repo`, `search_gitlab_repos`, `create_gitlab_repo`, `link_project_remote`, `check_git_remote_access`, `configure_upstream_remote`, `sync_upstream_fetch`.

## Implementation Status

| Area | Status |
| :--- | :--- |
| Repo search/create/link/clone/diagnostics (`kopeng/github.rs`, `gitlab.rs`, `diagnostic.rs`) | Implemented |
| Remote registry / provider classification | Implemented (`forge` module consumes it) |
| Self-hosted provider override (`[plugins.vcs] forge_provider`) | Roadmap (no occurrence in `src-tauri/src`) |

## Related Specs

- [Forge Provider Abstraction & Collaboration](05-forge-provider-abstraction.md)
- [Git Workspace, Branches & Snapshots Cockpit](02-git-workspace-and-branches.md)
- [Greenfield Project Creation](10-greenfield-project-creation.md)
- [Project Category Index](README.md)

Supersedes legacy: `11_remote_github_integration.md`, `39_forge_provider_and_project_surface_handoff.md` (remote registry portion)
