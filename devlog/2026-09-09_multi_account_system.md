# Devlog: Multi-Account Management System (Spec 17)
**Date:** 2026-09-09  
**Domain:** Authentication, Session Management, Account Lifecycle  

---

## Completed Tasks

1. **Crypto & Machine-Bound Token Storage:**
   - Integrated `aes-gcm`, `hkdf`, `sha2`, `rand`, `base64`, `uuid` into `src-tauri/Cargo.toml`.
   - Built HKDF-SHA256 master key derivation from `~/.gemini/installation_id` (salt `b"wang-kapawu-accounts-v1"`, info `b"account-token-encryption"`).
   - Implemented AES-256-GCM authenticated encryption/decryption with 12-byte random nonces for refresh tokens.
   - Built RS256 JWT payload base64url parsing to extract user identity claims (`email`).

2. **SQLite Multi-Account Persistence (`src/db.rs`):**
   - Created `accounts` table (`id`, `label`, `email`, `is_active`, `enc_token`, `added_at`).
   - Created `pending_resume` table (`pane_id`, `conversation_id`, `captured_at`).
   - Added DB indexes for high-throughput queries (`idx_accounts_email`, `idx_accounts_is_active`).
   - Provided full CRUD interfaces for account registration, deletion, renaming, and session recovery persistence.

3. **Keyring Driver & Passive Watcher (`src/kopeng/accounts.rs`, `src/watcher.rs`):**
   - Interfaced with FreeDesktop Secret Service via `/usr/bin/secret-tool` (`read_keyring_token`, `write_keyring_token`, `clear_keyring_token`).
   - Implemented Linux `/proc` scanner (`get_active_agy_sessions`) extracting active PIDs, `HERDR_PANE_ID`, and `--conversation` UUIDs.
   - Integrated keyring file watching (`Default_keyring.keyring`) to automatically discover new OAuth logins and sync active credentials without polling.

4. **Atomic Switching & Add Account Workflow:**
   - Implemented `switch_account`: snapshot active sessions -> graceful SIGTERM with presence lock detection (10s fallback SIGKILL) -> keyring credential swap -> in-place `agy --conversation <uuid>` restart via Herder socket -> clear pending resumes.
   - Implemented `initiate_add_account`: backup current credentials -> clear keyring -> spawn isolated terminal -> async detector -> automatic credential restoration and cleanup.
   - Added presence guard check: `check_active_presence_sessions()` detecting lock files `< 30s` old.

5. **Frontend UI & State Management (`src/components/`, `src/stores/coreStore.ts`):**
   - Created reactive Account Switcher dropdown in `AppHeader.vue` displaying active account, registered accounts list, rename/remove actions, and `+ Add Account` button.
   - Created `SwitchConfirmationModal.vue` warning user when switching accounts while active presence locks exist.
   - Verified end-to-end frontend and backend builds (`cargo test --lib`, `vue-tsc --noEmit`, `vite build`).
