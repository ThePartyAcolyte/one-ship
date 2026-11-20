# Devlog: Kapawu Identifiers Purge & Data Migration (Phase 10)

**Date:** 2026-11-09  
**Topic:** Clean Purge of Legacy `kapawu` / `wang-kapawu` Identifiers & In-Flight Data Migration  
**Status:** Completed  

---

## 1. Context & Motivation

During early iterations of the platform, certain subsystem identifiers, namespaces, and encryption salts retained the legacy project codename `kapawu` / `wang-kapawu` (recorded as Item #25 in `TECHNICAL_DEBT.md`). Specifically:
- Account token encryption derived keys using the salt `b"wang-kapawu-accounts-v1"`.
- Git snapshot refs used the namespace `refs/kapawu/snapshots/{agent_id}/{snapshot_id}`.
- Snapshot commit messages used the prefix `kapawu-snapshot: {summary}`.
- In-memory database records stored `refs/kapawu/snapshots/...` across registered projects.

Per the Zero Backward Compatibility & Anti-Legacy Fallback principles, these were migrated cleanly to canonical `one-ship` conventions, ensuring existing on-disk repos, database records, and account tokens migrate cleanly on database startup without accumulating permanent fallback branches.

---

## 2. Changes Summary

### Backend
1. **`src-tauri/src/kopeng/accounts.rs`**:
   - Switched canonical salt to `b"one-ship-accounts-v1"`.
   - Maintained `LEGACY_ACCOUNT_SALT = b"wang-kapawu-accounts-v1"` strictly scoped to migration routine.
   - Added `rekey_account_token_if_needed(encrypted_hex, ikm)` to decrypt legacy tokens and re-encrypt under the canonical salt.
   - Added unit test `test_legacy_token_migration` verifying roundtrip key migration.

2. **`src-tauri/src/kopeng/snapshots.rs`**:
   - Updated `SNAPSHOT_REF_PREFIX` to `"refs/one-ship/snapshots"`.
   - Updated commit message prefix to `"oneship-snapshot: "`.
   - Implemented `migrate_project_snapshot_refs(project_path)` to enumerate git refs under `refs/kapawu/snapshots/*`, create equivalent refs under `refs/one-ship/snapshots/*`, and delete the obsolete refs.
   - Added unit test `test_migrate_project_snapshot_refs`.

3. **`src-tauri/src/db.rs`**:
   - Added `migrate_kapawu_identifiers(conn)` invoked automatically within `init_database()`.
   - Migrates `snapshots.ref_path` replacing `refs/kapawu/snapshots/` with `refs/one-ship/snapshots/`.
   - Re-encrypts account tokens using `rekey_account_token_if_needed`.
   - Iterates through all registered projects in `projects` table and invokes `migrate_project_snapshot_refs`.

### Frontend
1. **`src/components/workspace/ProjectGitSnapshotsTab.vue`**:
   - Updated `formatRefPath` to strip the canonical ref prefix `'refs/one-ship/snapshots/'`.

### Documentation & Debt Tracking
1. **`docs/specs/16_agent_session_snapshot_system.md`**:
   - Updated ref paths and commit messages to `refs/one-ship/snapshots/` and `oneship-snapshot: `.
2. **`docs/specs/17_multi_account_management.md`**:
   - Documented `b"one-ship-accounts-v1"` as the canonical HKDF salt.
3. **`TECHNICAL_DEBT.md`**:
   - Removed item #25 (`Eliminate Legacy 'kapawu' Identifiers in Snapshots and Account Keys`).

---

## 3. Verification

- `cargo check --manifest-path src-tauri/Cargo.toml --all-targets`: Passed (0 errors).
- `cargo test --manifest-path src-tauri/Cargo.toml --lib`: 342 passed, 0 failed.
- `npx vue-tsc --noEmit`: Clean type check (exit code 0).
