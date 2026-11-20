# Devlog: Remote GitHub Integration & Repository Lifecycle
**Date:** 2026-09-02  
**Topic:** Remote GitHub CLI (`gh`) Integration, Permissions Verification, and Remote Connection Modal

## Overview
Implemented the Remote GitHub Integration for Wang-Kapawu. The architecture delegates all authentication, query, and repository lifecycle operations to the official GitHub CLI (`gh`), maintaining a zero-credential footprint in the application.

## Key Changes
1. **Live Specification (`docs/specs/11_remote_github_integration.md`)**:
   - Documented the `gh` CLI architecture and zero-credential rationale.
   - Specified repository discovery via `gh repo list --json name,nameWithOwner,url,isPrivate,viewerPermission --limit 30`.
   - Defined permission matrix (`ADMIN` / `WRITE` for direct push vs `READ` for fork prompt).
   - Documented local-to-remote linking (`git remote add origin` / `git remote set-url origin`) and remote creation (`gh repo create <name> --public/--private --source=. --remote=origin`).

2. **Backend Plugin (`src-tauri/src/kopeng/github.rs` & `src-tauri/src/kopeng/git.rs`)**:
   - Created `GithubRepo` struct with camelCase serde serialization.
   - Implemented `search_github_repos`: runs `gh repo list` and supports optional substring query filtering.
   - Implemented `link_github_remote`: auto-initializes `.git` if missing, detects existing `origin`, and performs `git remote set-url` or `git remote add`.
   - Implemented `create_github_repo`: invokes `gh repo create` with `--source=. --remote=origin`.
   - Implemented `get_git_remote_url`: queries `git remote get-url origin`.
   - Extended `GitStatus` in `git.rs` with `pub remote_url: Option<String>`.
   - Registered all commands in `src-tauri/src/lib.rs`.

3. **Frontend Modal & Project Card (`src/App.vue`)**:
   - Wired `<Cloud />` toolbar icon in project cards: highlighted emerald with remote URL tooltip when linked, dim when unlinked.
   - Implemented GitHub Remote Connection modal:
     - Configured remote status banner with direct external link to GitHub.
     - "Link Existing Repo" tab with instant search filter, visibility pills (`[PRIVATE]`/`[PUBLIC]`), permission badges, and 1-click Link action.
     - Permission check: displays warning with fork recommendation when attempting to link a `READ`-only repository.
     - "Create New Repo" tab: pre-populated repo name, visibility radio buttons, preview command, and creation action.
   - Refreshes project registry and git status automatically after link or create.

4. **Verification**:
   - `npm run build` ran clean with 0 errors.
   - `cargo check --manifest-path src-tauri/Cargo.toml` passed with 0 errors/warnings.
