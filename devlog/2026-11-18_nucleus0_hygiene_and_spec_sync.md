# 2026-11-18 — Nucleus 0: Quick Hygiene & Spec Sync

## Summary
Executed Nucleus 0 from `docs/WORK_PLAN.md` to eliminate orphaned bindings/dead GitHub wrappers and synchronize `TODO.md`, `TECHNICAL_DEBT.md`, and Specs 34/39 with the current codebase state.

## Changes
1. **Orphaned TypeScript Bindings Removed:**
   - Deleted stray `src-tauri/bindings/` directory (`src/types/generated/` is canonical).
   - Deleted 8 orphaned `*Github*Args.ts` files in `src/types/generated/` (`GetGithubCollabContextArgs.ts`, `ListGithubIssuesArgs.ts`, `GetGithubIssueArgs.ts`, `ListGithubPrsArgs.ts`, `GetGithubPrArgs.ts`, `ListGithubReleasesArgs.ts`, `CreateGithubDraftPrArgs.ts`, `TakeGithubIssueArgs.ts`).
2. **Dead Rust Wrappers Removed (`src-tauri/src/kopeng/github.rs`):**
   - Removed the 7 unused `#[ts(export)]` `*Args` structs and 7 unused `pub async fn` wrappers (`get_github_collaboration_context`, `list_github_issues`, `get_github_issue`, `list_github_prs`, `get_github_pr`, `list_github_releases`, `create_github_draft_pr`).
   - Preserved all active `*_blocking` functions, DTOs, and repository operations used by `kopeng/forge/github.rs` and `mcp.rs`.
3. **TODO, Technical Debt & Spec Synchronization:**
   - Removed completed items from `TODO.md` (Spec 38 implementation, `src-tauri/bindings/` removal).
   - Removed already-resolved entries from `TECHNICAL_DEBT.md` (#36 scrollbar utilities, #38 legacy CSS tokens).
   - Updated `docs/specs/39_forge_provider_and_project_surface_handoff.md` status header and `docs/specs/34_project_management_and_oss_collaboration.md` §6.1 to reflect `ForgeCollabPanel` and GitHub + GitLab support.
   - Created `docs/WORK_PLAN.md` organizing remaining work by subsystem and context nuclei.

## Verification
- Executed `./scripts/run_dev.sh export-bindings` to regenerate and verify `ts-rs` bindings.
- Checked `./scripts/run_dev.sh status` to confirm daemon build health.
