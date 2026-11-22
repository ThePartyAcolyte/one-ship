# 2026-11-22 — Welcome View & Header Voice Controller Toggle

## Summary
- Added a dedicated startup Welcome / Home screen (`WelcomeView.vue`) that replaces the empty workspace chrome (`ProjectVerticalRail`, workspace sub-tabs, and `AgentTaskbar`) when no projects are registered or when enabled on startup, while keeping `AppHeader` accessible.
- Persisted the startup preference via `[system].show_welcome_on_startup` in `config.toml` (`config.rs`, `api.rs`, `daemonApi.ts`), with a mandatory override when `projects.length === 0`.
- Coordinated startup onboarding so `SetupChecklistModal` does not auto-open on top of `WelcomeView`; instead, `WelcomeView` displays a **Missing Configuration** card only when workstation prerequisites are incomplete.
- Listed active projects first (without redundant `Git` or `idle` badges), separated archived projects under an **Archived Projects** divider with an **Unarchive** action, and included a **Documentation & Manual (In Development)** card.
- Consolidated the global Voice Controller toggle into the `AppHeader` Voice Controller pill button (green active state, click to toggle when idle) and removed the duplicate toggle row from the application hamburger menu.

## Files Modified / Created
- [config.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/config.rs)
- [api.rs](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/api.rs)
- [daemonApi.ts](file:///home/butcherwutcher/Projects/dev/one-ship/src/types/daemonApi.ts)
- [SetShowWelcomeOnStartupArgs.ts](file:///home/butcherwutcher/Projects/dev/one-ship/src/types/generated/SetShowWelcomeOnStartupArgs.ts)
- [coreStore.ts](file:///home/butcherwutcher/Projects/dev/one-ship/src/stores/coreStore.ts)
- [WelcomeView.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/workspace/WelcomeView.vue)
- [AppHeader.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/AppHeader.vue)
- [App.vue](file:///home/butcherwutcher/Projects/dev/one-ship/src/App.vue)

## Verification

### 1. Contract & Bindings Export (`./scripts/run_dev.sh export-bindings`)
```text
test config::export_bindings_setshowwelcomeonstartupargs ... ok
test result: ok. 425 passed; 0 failed; 0 ignored; 0 measured; 186 filtered out; finished in 0.40s
```

### 2. Targeted Rule & Silent Fallback Check (`grep -nE`)
```text
grep -nE "unwrap_or|unwrap_or_default|\.ok\(\)|let _ =|catch\s*(\([^)]*\))?\s*\{\s*\}|title=|bg-black/|bg-\[#|text-amber-|backdrop-blur|:\s*any\b|<any>" src/components/workspace/WelcomeView.vue src/components/AppHeader.vue
(no matches, exit 0)
```

### 3. Read-Only Rule Gate (`./scripts/verify_rules.sh`)
```text
=== ONE-SHIP Read-Only Rule Verification ===
[PASS] Zero TypeScript 'any' in src/
[WARN] Native title= attributes found (verify these are component props, not HTML attributes)
[WARN] Lock .unwrap()/.expect() found (must only exist inside #[cfg(test)])
[PASS] Zero Tailwind palette classes (text-/bg-/border-{color}-*)
[FAIL] Zero raw bg-black/* or bg-[#...] outside ui.css (pre-existing in RollingStatusLog.vue and modals)
[FAIL] Unstructured Result<_, String> found outside tests (pre-existing in src-tauri/src/config.rs:2218)
[PASS] Zero backdrop-blur-* in components/views
[PASS] Zero hardcoded /home/butcherwutcher paths
[PASS] English-only active documentation and templates
=== Summary: 2 hard failure(s) ===
```
Zero new violations introduced in any created or modified files.

### 4. Daemon & GUI Health (`./scripts/run_dev.sh status`)
```json
{"units":{"daemon":{"active_state":"active","sub_state":"running"},"gui":{"active_state":"active","sub_state":"running"}}}
```
