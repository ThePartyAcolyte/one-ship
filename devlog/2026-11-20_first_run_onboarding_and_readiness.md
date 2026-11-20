# Devlog: First-Run Onboarding, Readiness & External Tools Guide (2026-11-20)

## Summary

Implemented the multi-layer onboarding, workstation readiness verification, and external companion tool guide (`solidcraft`, `pcbcraft`, `oneship-designer`, `rea`, `electronics`, `opendictate`) based on clean-install Ubuntu 26.04 VM feedback.

## Changes Implemented

1. **Backend Readiness & Workspace Root Provisioning:**
   - [`src-tauri/src/config.rs`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/config.rs#L2806-L2841): added `set_default_projects_root_path` to persist `[system].default_projects_root_path` in `config.toml` and refresh `GLOBAL_CONFIG_CACHE`.
   - [`src-tauri/src/kopeng/diagnostic.rs`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/diagnostic.rs): extended `DiagnosticState` with `git_user_name`, `git_user_email`, `default_projects_root_path`, and `projects_root_exists`; added typed commands `set_git_global_identity` and `ensure_projects_root_dir`.
   - [`src-tauri/src/kopeng/github.rs`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/github.rs#L366-L378): updated `clone_and_add_project` to create missing parent workspace directories (`create_dir_all`) before `git clone`.
   - [`src-tauri/src/api.rs`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/api.rs) & [`src/types/daemonApi.ts`](file:///home/butcherwutcher/Projects/dev/one-ship/src/types/daemonApi.ts): registered `set_git_global_identity` and `ensure_projects_root_dir` with regenerated `ts-rs` bindings (`417/417` passed).

2. **First-Run Onboarding UI, Health Summary & Project Modal Guards:**
   - [`src/utils/integrationGuides.ts`](file:///home/butcherwutcher/Projects/dev/one-ship/src/utils/integrationGuides.ts): centralized install commands and upstream repo/release references for `desigma` (`oneship-designer`), `pcbcraft`, `solidcraft`, `rea`, `electronics`, `voice_controller` (`opendictate`), and CLI runners (`claude`, `opencode`, `agy`).
   - [`src/components/modals/SetupChecklistModal.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/modals/SetupChecklistModal.vue): created the 4-step readiness checklist modal (Default Projects Directory, Git Identity & `gh`, AI Agent Runners, Optional Companion Tools) auto-opening once on incomplete first boot.
   - [`src/components/modals/SettingsModal.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/modals/SettingsModal.vue): surfaced install commands and repo/release links for missing integrations in the **Integrations** tab.
   - [`src/components/modals/AddProjectModal.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/modals/AddProjectModal.vue): disabled `Launch Project Initializer` automatically when zero runners are installed and surfaced clear warnings on *New Project* and *Clone* tabs.
   - [`src/components/AppHeader.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/AppHeader.vue) & [`src/App.vue`](file:///home/butcherwutcher/Projects/dev/one-ship/src/App.vue): replaced the git-only `isAllDiagnosticsOk` check with full readiness validation (`git_installed`, Git identity, `projects_root_exists`, `installed_runners.length > 0`) and added **Setup & Readiness** menu entry.

3. **Installer (`install.sh`) & Spec Sync:**
   - [`install.sh`](file:///home/butcherwutcher/Projects/dev/one-ship/install.sh): added `github-cli`/`gh` to base packages, added interactive projects-root directory prompt/creation + global Git identity prompt inside `seed_initial_config()`, and printed AI runner installation hints if `agy`/`claude`/`opencode` are missing.
   - [`docs/specs/01-architecture/07-linux-packaging-and-host-capabilities.md`](file:///home/butcherwutcher/Projects/dev/one-ship/docs/specs/01-architecture/07-linux-packaging-and-host-capabilities.md): synchronized onboarding/readiness contracts.

## Verification

- **`./scripts/run_dev.sh export-bindings`:** `417 passed; 0 failed` (`DiagnosticState.ts`, `SetGitGlobalIdentityArgs.ts`, `EnsureProjectsRootDirArgs.ts` exported).
- **`./node_modules/.bin/vue-tsc --noEmit`:** exited `0` (0 TypeScript errors).
- **`bash -n install.sh`:** exited `0`.
- **`./scripts/verify_rules.sh`:** 0 new violations (only the 2 pre-existing unrelated hard failures: Debt `#45` and `config.rs:1920`).
- **`./scripts/run_dev.sh status` & `journalctl --user -u oneship-dev-daemon-build`:** daemon build `Finished 'dev' profile in 2m 02s`; `daemon` (`1651527`) and `gui` (`1657038`) active.
