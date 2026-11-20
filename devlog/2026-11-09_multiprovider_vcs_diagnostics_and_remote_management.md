# Devlog: Multi-Provider VCS Diagnostics & Remote Management (Phase 11)

**Date:** 2026-11-09  
**Topic:** Multi-Provider VCS Diagnostics (`gh`, `glab`, Generic Git) & In-Modal Privileged Installation  
**Status:** Completed  

---

## 1. Context & Motivation

Previously, repository remote management and diagnostics were tightly coupled to the GitHub CLI (`gh`):
1. **Asymmetric System Health (Technical Debt #9):** `isAllDiagnosticsOk` in the application header required `gh_authenticated = true`. Developers who exclusively used GitLab or self-hosted Git repositories suffered permanent warning pulses on the health indicator.
2. **Missing GitLab Parity (Technical Debt #3):** Projects hosted on GitLab could not be discovered or created natively through ONE-SHIP's remote connection workflow.
3. **Hardcoded Tooling Checks & Disconnected Installation Flow (Technical Debt #2 & TODO Phase 0):** While `pkexec pacman -S` was available in the header diagnostics popover, encountering a missing CLI inside the "Connect Remote" modal provided no contextual resolution path.

---

## 2. Changes Summary

### Backend
1. **`src-tauri/src/kopeng/diagnostic.rs`**:
   - Extended `DiagnosticState` with `glab_installed: bool` and `glab_authenticated: bool`.
   - Updated `collect_diagnostics` to check `glab --version` and `glab auth status`.
   - Verified that `resolve_pacman_package` cleanly handles `"glab" | "gitlab-cli"`.
2. **`src-tauri/src/kopeng/gitlab.rs` (New Module)**:
   - Implemented `search_gitlab_repos` querying `glab api "projects?membership=true&simple=true&per_page=..."` and returning strongly typed `GitlabRepo[]`.
   - Implemented `create_gitlab_repo` (`glab repo create <name> --source=. --remote=origin [--private/--public]`).
   - Defined `GitlabError` implementing `ApiError` with HTTP status mapping.
   - Added unit test `repo_names_validation`.
3. **`src-tauri/src/kopeng/git.rs` & `github.rs`**:
   - Elevated `ensure_repository` and `has_origin` into `kopeng::git` as shared reusable helpers.
   - Replaced provider-coupled `link_github_remote` with canonical, universal `link_project_remote` in `github.rs` (`git remote add/set-url origin <url>`).
4. **`src-tauri/src/api.rs`**:
   - Registered `search_gitlab_repos`, `create_gitlab_repo`, and `link_project_remote` in `API_COMMANDS` and `dispatch`.
5. **`src-tauri/src/config.rs` & User Config**:
   - Added `gitlab_repo_list_limit_count` (default: 100) to `ConfigFieldSchema` and `DEFAULT_GLOBAL_CONFIG_TOML`.
   - Updated active `~/.config/one-ship/config.toml`.

### Frontend
1. **`src/types/daemonApi.ts`**:
   - Mirrored new daemon commands in `DaemonCommands`: `search_gitlab_repos`, `create_gitlab_repo`, and `link_project_remote`.
2. **`src/components/AppHeader.vue`**:
   - Added symmetrical GitLab CLI (`glab`) installation button and GitLab Auth status to the diagnostics popover.
3. **`src/App.vue`**:
   - Decoupled `isAllDiagnosticsOk` from GitHub auth; now evaluates `git_installed` as the core baseline VCS invariant.
   - Replaced `GithubRemoteModal` with `RemoteVcsModal`, passing reactive diagnostics and tool installation events.
4. **`src/components/modals/RemoteVcsModal.vue` (New Component)**:
   - Unified multi-provider remote modal supporting **GitHub**, **GitLab**, and **Direct Git URL**.
   - Auto-detects provider based on the project's configured `remote_url`.
   - Renders contextual 1-click installation banners (`pkexec pacman -S --noconfirm`) if `gh` or `glab` is not found, auto-refreshing the view upon installation.
   - Supports search, permission verification, and repository creation for both GitHub and GitLab, as well as arbitrary SSH/HTTPS Git endpoints.
5. **Dead Code Purge**:
   - Removed obsolete `src/components/modals/GithubRemoteModal.vue` and unused `LinkGithubRemoteArgs.ts`.

### Specifications & Debt Tracking
1. **`docs/specs/11_remote_github_integration.md`**:
   - Upgraded to Version 1.2.0: Multi-Provider VCS Integration (`gh`, `glab`, Generic Git).
2. **`TECHNICAL_DEBT.md`**:
   - Resolved and removed items #2, #3, and #9.
3. **`TODO.md`**:
   - Archived completed Phase 0 item.

---

## 3. Verification

- `cargo check --manifest-path src-tauri/Cargo.toml --all-targets`: Passed (0 errors).
- `cargo test --manifest-path src-tauri/Cargo.toml --lib`: 345 passed, 0 failed.
- `(cd src-tauri && cargo test --lib export_bindings)`: 220 passed, all TS bindings generated.
- `npx vue-tsc --noEmit`: Clean type check (exit code 0).
- `npm run build`: Production build succeeded in 4.42s (0 errors).
