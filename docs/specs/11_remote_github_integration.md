# Spec: Remote VCS Integration & Repository Lifecycle
**Document Version:** 1.2.0  
**Domain:** Multi-Provider VCS (`gh`, `glab`, Generic Git) Remote Management, Discovery, Privileged Diagnostics & Linking  
**Maintainer:** ONE-SHIP / Omarchy Development Team

---

## 1. Architecture & Design Rationale

ONE-SHIP adheres to a zero-credential desktop architecture. Rather than embedding OAuth token capture, personal access token (PAT) storage, or custom encryption vaults inside the Tauri binary, ONE-SHIP delegates VCS authentication and API communication to official provider CLIs (`gh` for GitHub, `glab` for GitLab) or standard Git remote protocols:

```text
┌─────────────────────────────────────────────────────────────┐
│                      ONE-SHIP Hub UI                        │
│   [ <Cloud /> Button ] -> [ Remote VCS Modal ]              │
│   - GitHub Tab           - GitLab Tab        - Direct URL   │
└──────────────────────────────┬──────────────────────────────┘
                               │ daemonCall → POST /api/v1/call/<command>
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                 oneship-daemon (Spec 12 §7.2)               │
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

### Key Advantages
1. **Security Isolation:** ONE-SHIP never handles raw access tokens or sensitive credentials; authentication is maintained securely by the host OS keyring and official CLI tools.
2. **Deterministic & Symmetric Diagnostics:** `run_diagnostics` (`kopeng/diagnostic.rs`, header diagnostics popover) evaluates `git_installed`, `gh_installed`, `gh_authenticated`, `glab_installed`, `glab_authenticated`, `opendictate_available`, `package_manager`, and `can_auto_install_tools`. Only `git_installed` is treated as the foundational VCS requirement, preventing false system-wide warning indicators if a user exclusively uses GitLab or self-hosted Git.
3. **Contextual In-Modal & Toolchain Installation:** If a developer switches to the GitHub or GitLab tab and the corresponding CLI (`gh` or `glab`) is absent from `$PATH`, the modal renders an inline contextual prompt to install the tool via `pkexec` using the detected Linux distribution's package manager (`pacman`, `apt-get`, `dnf`, `zypper`). `install_missing_tools` (`kopeng/diagnostic.rs`) supports privileged installation of VCS, RE, and embedded toolchain packages across supported distros. Upon Polkit confirmation, diagnostics re-run without requiring an application restart.
4. **Generic Remote Agnosticism:** Direct Git URL support enables linking self-hosted Git, Bitbucket, Gitea, or custom SSH/HTTPS remotes without third-party CLI dependencies.

---

## 2. Command Primitives & Execution Protocol

### 2.1 Repositories Discovery & Search
- **GitHub:** Retrieved via `gh repo list` with structured JSON output:
  ```bash
  gh repo list --json name,nameWithOwner,url,isPrivate,viewerPermission --limit <vcs.github_repo_list_limit_count>
  ```
- **GitLab:** Retrieved via `glab api` with structured JSON output:
  ```bash
  glab api "projects?membership=true&simple=true&per_page=<vcs.gitlab_repo_list_limit_count>"
  ```
- **Search Filtering:** Both lists are filtered client-side by repository/namespace substring.

### 2.2 Access Permission Verification Matrix
Before linking a remote repository to a local project directory, permissions are verified:
- **GitHub (`viewerPermission`):**
  - `ADMIN` / `WRITE`: Direct linking enabled.
  - `READ` / `TRIAGE`: Warning prompt displayed advising that direct pushes will be rejected by remote unless forked (`gh repo fork`), with an option to link anyway for pull-only workflows.
- **GitLab:**
  - Evaluated from membership projects list.

### 2.3 Universal Remote Linkage (`origin`)
To link an existing local Git directory to a remote repository URL:
1. **Validate URL:** Non-empty, no whitespace, does not start with `-` (`InvalidRemoteUrl`).
2. **Ensure Git Repository:** If `.git` is absent, runs `git init` first.
3. **Configure Remote:**
   - If `origin` exists: `git remote set-url origin <remote_url>`
   - If `origin` does not exist: `git remote add origin <remote_url>`

### 2.4 Creating a New Remote Repository
- **GitHub:**
  ```bash
  gh repo create <name> --public/--private --source=. --remote=origin
  ```
- **GitLab:**
  ```bash
  glab repo create <name> --public/--private --default-branch=main
  ```
  Followed by origin configuration linking to the newly created repository web URL.

### 2.5 Remote Repository Import & Access Verification
For initializing new projects directly from GitHub or generic Git remotes:
1. **Access Verification (`check_git_remote_access`):**
   - Validates URL syntax (`validate_remote_url`).
   - If GitHub URL (`github.com`), queries `gh repo view <url> --json name,nameWithOwner,isPrivate,viewerPermission,defaultBranchRef`.
   - Fallback executes non-blocking `GIT_TERMINAL_PROMPT=0 git ls-remote --exit-code <url> HEAD`.
   - Evaluates reachability, privacy, branch, and viewer permissions without prompting interactively for credentials.
2. **Clone & Project Registration (`clone_and_add_project`):**
   - Validates target directory name and parent location.
   - Executes `GIT_TERMINAL_PROMPT=0 git clone <url> <target_path>`.
   - Registers cloned directory into the SQLite database registry (`add_project`).
   - Prompts or triggers Project Initializer for brownfield stack reconnaissance.

---

## 3. Daemon Commands (Operator RPC API)

The commands are implemented in `src-tauri/src/kopeng/{github,gitlab,diagnostic}.rs` and served by the daemon's operator RPC API (`api.rs`, Spec 12 §7.2); the UI calls them via `daemonCall`:

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

### 3.1 Remote Registry & Provider-Neutral Forge Collaboration (Spec 39)
- `GitStatus.remotes: Vec<GitRemote>` enumerates all configured Git remotes (`git config --get-regexp '^remote\..*\.url$'`) and classifies each URL into `ForgeProviderKind` (`github`, `gitlab`, `unknown`) without guessing unknown self-hosted hosts. `GitStatus.has_remote` is true whenever `!remotes.is_empty()` or an upstream branch is tracked.
- Collaboration reads (`get_forge_context`, `list_forge_issues`, `get_forge_issue`, `list_forge_pull_requests`, `get_forge_pull_request`, `list_forge_releases`, `create_forge_draft_pull_request`, `take_forge_issue`) and inbox commands (`list_forge_inbox`, `mark_forge_item_read`, `set_forge_item_flag`, `get_forge_unread_counts`) route through `kopeng::forge::ForgeProvider` (`GithubForgeProvider` via `gh`, `GitlabForgeProvider` via `glab api`).

---

## 4. User Experience & Interface Specifications

### 4.1 Toolbar Integration (`<Cloud />`)
Located in the project card upper toolbar and workspace overview:
* **Unlinked State:** Dim gray icon (`text-[var(--dim-color)] opacity-50`), tooltip `"Connect Remote"`.
* **Linked State:** Accent color highlight (`text-[var(--status-ok-color)]`), tooltip showing `"Remote: <url>"`.
* **Click Behavior:** Opens the Remote VCS Modal.

### 4.2 Modal Layout & State Machine
1. **Header & Active Remote Origin Banner:**
   - Displays current `origin` URL with provider badge (`[GITHUB]`, `[GITLAB]`, or `[GIT REMOTE]`).
   - External browser link icon for HTTP/HTTPS origins.
2. **Provider Selector Tabs:**
   - `[ GitHub ]`, `[ GitLab ]`, `[ Direct Git URL ]`.
   - Auto-selects based on existing origin domain (e.g. `gitlab.com` switches directly to GitLab tab).
3. **Contextual Installation Banners:**
   - If the selected provider CLI is not found (`gh` on GitHub, `glab` on GitLab), renders an inline banner with 1-click install (`pkexec pacman -S`).
4. **Action Tabs (Link Existing vs Create New):**
   - Instant search and filtering of user repositories.
   - Creation form with name sanitization and private/public toggle.
   - Direct URL input for arbitrary self-hosted Git endpoints.
