# Devlog: Multi-Account Session Relaunch, In-Chat Mismatch Detection & Quota Telemetry

**Date:** 2026-09-20
**Topic:** Multi-Account Session Management, ProcessSupervisor Native Relaunch, In-Chat Credential Mismatch & Quota Observability
**Status:** Completed

---

## 1. Context & Motivation

1. **Multi-Account Switching Flexibility:**
   Previously, switching Google accounts in Wang-Kapawu was a binary action that terminated all active agent sessions. The user required the ability to switch the active global account without terminating ongoing conversations, allowing active agents to finish their tasks under their startup credentials while newly launched chats start with the newly selected account.
2. **Post-Herder Process Supervision:**
   Following the complete decoupling of Herder from the architecture, the account switcher in `kopeng/accounts.rs` contained obsolete references to OS `SIGTERM` signals and terminal injection. It needed migration to native `ProcessSupervisor` lifecycle methods (`stop_process` and `launch_agent`).
3. **In-Chat Mismatch Detection & On-Demand Reload:**
   When an active chat continues running under an earlier account, the chat overlay must visibly indicate that its credentials differ from the global active account, providing a one-click button to reload the session under the new credentials while preserving conversation thread context.
4. **Context & Token Observability in Chat:**
   Real-time visibility into context window consumption (`CTX %`), token counts (fresh input vs. cache read), and 5h/7d rate limit quotas within `FloatingChatOverlay.vue`.

---

## 2. Architectural Changes

### 2.1 Backend (`src-tauri/`)

- **`supervisor.rs`**:
  - Added `account_id: Option<String>` and `account_email: Option<String>` to `ManagedProcess` and `AgentState`.
  - When spawning an agent, the supervisor queries SQLite for the active account and records it on the managed process.
  - In `compute_project_state`, propagated `account_id` and `account_email` across all agent state projections (Rula agents, dynamic specialists, initializers, and the platform system agent).
  - Enhanced `restart_agent` to handle both project agents and the global `"system"` agent cleanly.

- **`kopeng/accounts.rs`**:
  - Refactored `switch_account` into an async function accepting `(target_account_id: &str, relaunch_active: bool)`.
  - When `relaunch_active` is `false`, it swaps keyring credentials and sets the active account in SQLite under the async keyring mutex, leaving active agent processes running.
  - When `relaunch_active` is `true`, it stops active processes via `ProcessSupervisor::stop_process`, swaps credentials, and relaunches them via `ProcessSupervisor::launch_agent`, resuming their stored conversation IDs (`--conversation <agy_id>`).
  - Updated `switch_account_cmd` Tauri command handler to accept `relaunch_active: Option<bool>`.

- **`kopeng/stats.rs`**:
  - In `get_agent_metrics`, added fallback hydration from the `account_quotas` table in SQLite for the active account if statusline metadata lacked 5h or 7d window metrics.

### 2.2 Frontend (`src/`)

- **`stores/coreStore.ts`**:
  - Updated `requestAccountSwitch`, `confirmAccountSwitch(relaunchActive: boolean)`, and `executeAccountSwitch(accountId: string, relaunchActive: boolean)` to pass `relaunchActive` to `switch_account_cmd`.

- **`components/modals/SwitchConfirmationModal.vue`**:
  - Replaced the binary modal with three explicit choices:
    1. **Cancel**: Aborts the operation.
    2. **Keep Active Chats** (`relaunch_active = false`): Switches credentials for future agent launches while preserving running conversations.
    3. **Switch & Restart Chats** (`relaunch_active = true`): Restarts active sessions under the new account while maintaining conversation continuity.

- **`components/dialogue/FloatingChatOverlay.vue`**:
  - Added `isAccountMismatched` computed check comparing running agent account vs global active account.
  - Added amber notification banner displaying running account vs new global login, with a `[RELOAD WITH NEW LOGIN]` action button that executes `restart_agent` and updates agent state.
  - Added brutalist Context Window (`CTX XX.X%` with mini-progress bar), Token Breakdown (fresh tokens, cache hit %), and 5h/7d quota indicators.
  - Wrapped all hover tooltips in `<AppTooltip>` complying with strict UI rules.

### 2.3 Documentation (`docs/specs/`)

- **`docs/specs/17_multi_account_management.md`**:
  - Bumped version to `0.2.5`.
  - Removed obsolete Herder references (`HERDR_PANE_ID`, `pane_id`, `pending_resume` table).
  - Documented native `ProcessSupervisor` session discovery, 3-option modal, and process-level account attribution.
- **`docs/specs/18_mcp_dialogue_channel_and_chat_overlay.md`**:
  - Added Section 4.4 and Phase 2F documenting `FloatingChatOverlay.vue`, account mismatch banner, and live telemetry bar.
- **`docs/specs/19_agent_quota_and_context_observability.md`**:
  - Documented fallback hydration from `account_quotas` in Section 3.1 and UI telemetry bar integration in Section 5.

---

## 3. Verification & Validation

- `cargo check` in `src-tauri`: Verified with exit code `0`.
- `npm run build` (`vue-tsc --noEmit && vite build`): Verified with exit code `0`.
