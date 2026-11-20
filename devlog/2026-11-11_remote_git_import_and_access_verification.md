# Devlog: Remote Git / GitHub Repository Import & Access Verification

**Date:** 2026-11-11  
**Topic:** Remote Git / GitHub Repository Import & Access Verification  
**Author:** Pair Programming Session  

---

## 1. Summary of Work Done

Implemented the ability to initialize and import projects directly from GitHub or any remote Git repository (HTTPS or SSH) by supplying the repository URL.

### Backend Daemon (`src-tauri`):
1. **Access Verification (`check_git_remote_access`)**:
   - Implemented in `src-tauri/src/kopeng/github.rs`.
   - Validates the remote URL syntax (`validate_remote_url`).
   - If the URL targets GitHub (`github.com`), queries `gh repo view <url> --json name,nameWithOwner,isPrivate,viewerPermission,defaultBranchRef` to obtain repository name, privacy state, default branch, and viewer permissions (`READ`, `WRITE`, `ADMIN`).
   - For non-GitHub or generic SSH/HTTPS URLs (or when `gh` is unauthenticated), executes `GIT_TERMINAL_PROMPT=0 git ls-remote --exit-code <url> HEAD` to verify read reachability without prompting interactively or blocking daemon threads.
   - Returns typed `GitRemoteAccessCheck` (`accessible`, `isPrivate`, `repoName`, `defaultBranch`, `viewerPermission`, `errorMessage`).
2. **Clone & Project Registration (`clone_and_add_project`)**:
   - Implemented in `src-tauri/src/kopeng/github.rs`.
   - Validates target directory name and checks that parent path is an absolute, existing directory.
   - Verifies target directory does not already exist.
   - Clones using non-blocking `GIT_TERMINAL_PROMPT=0 git clone <url> <target_path>`. On failure, cleans up partial directories and returns a typed error.
   - Automatically registers the newly cloned project in the SQLite registry (`add_project`), returning hydrated `HydratedProject`.
3. **RPC Dispatch & Invariants**:
   - Registered `check_git_remote_access` and `clone_and_add_project` in `api::API_COMMANDS` and `api::dispatch` in `src-tauri/src/api.rs`.
   - Mirrored both commands in `src/types/daemonApi.ts`.
   - Exported TypeScript types: `CheckGitRemoteAccessArgs.ts`, `GitRemoteAccessCheck.ts`, and `CloneAndAddProjectArgs.ts`.

### Frontend UI (`src`):
1. **`AddProjectModal.vue`**:
   - Added a 3rd tab: `Clone Repo` alongside `New Project` and `Existing Project`.
   - Provided repository URL input with auto-extraction of the repository name into the project directory field.
   - Added `Verify Access` button with real-time feedback:
     - Green badge: `Accessible` with privacy tag (`[PUBLIC]` or `[PRIVATE]`), viewer permission tag (`[ADMIN]`, `[WRITE]`, `[READ]`), and default branch indicator.
     - Red alert banner: when access is denied or the repository does not exist, explaining the cause.
   - Integrated parent directory selector with browse dialog.
   - Added target directory live preview.
   - Added "Launch Project Initializer" toggle, allowing brownfield stack reconnaissance to be launched immediately upon clone.
2. **`App.vue`**:
   - Updated `handleProjectAdded` to accept `shouldInit?: boolean` and invoke `triggerInitProject(project)` when requested.

### Specifications & Documentation:
- Updated `docs/specs/11_remote_github_integration.md` to document the remote repository import lifecycle and command schemas.

---

## 2. Verification & Validation
- **TypeScript & Frontend Build:** `vue-tsc --noEmit` and `npm run build` completed with 0 errors.
- **Backend Rust Build:** Verified clean compilation of `oneship_lib` and `oneship_daemon`.
