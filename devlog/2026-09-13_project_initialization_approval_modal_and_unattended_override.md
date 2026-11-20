# Devlog: 2026-09-13 - Project Initialization Approval Modal & Unattended Invariant Hardening

## 1. Context & Objectives
1. Investigate and diagnose system memory/swap saturation.
2. Resolve broken project initialization in Wang-Kapawu across semi-attended and unattended modes.

---

## 2. Root Cause Analysis

### Memory Saturation
- **`opendictate` (PID 2006)** had accumulated 12.4 GB of virtual memory (VmSwap) inside `/dev/zram0` (compressed in RAM), consuming ~7.5 GB of actual physical RAM.
- Multiple orphaned background `agy` agent sessions were active (such as dangling initialization jobs).
- `wang-kapawu` core and `vite` processes were verified healthy and minimal (~70 MB combined).

### Initialization Failures
1. **Semi-Attended Mode:**
   - The initialization agent invoked `request_user_approval` via MCP.
   - The backend stored the record in `pending_approvals` and emitted `ApprovalRequested` SSE.
   - `coreStore` recorded `pendingApproval`.
   - **Defect:** No Vue component rendered `pendingApproval`, leaving the backend channel `rx.await` hanging indefinitely.
2. **Unattended Mode:**
   - When running headless non-interactive commands via `agy -p`, the agent inherited the global `<RULE[user_global]>` requiring technical proposals before writing files.
   - Because the agent ran without interactive stdin, the proposal rule stalled automated file creation.

---

## 3. Implemented Changes

### A. Approval Request Modal
- Created [`src/components/modals/ApprovalRequestModal.vue`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/modals/ApprovalRequestModal.vue):
  - Displays phase title, target project path, and rendered Markdown payload via `marked.parse`.
  - Provides feedback input via `AppTextarea`.
  - Provides `[Approve Phase]`, `[Request Changes]`, and `[Reject]` controls wired to `coreStore.submitApprovalResponse`.
- Mounted `<ApprovalRequestModal />` in [`src/App.vue`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/App.vue).
- Updated [`src/stores/coreStore.ts`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/stores/coreStore.ts) to track and hydrate `project_path` in `pendingApproval`.

### B. Initialization Prompt & MCP Hardening
- Updated [`src-tauri/src/kopeng/skill.rs`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/kopeng/skill.rs):
  - Injected explicit `MANDATORY OVERRIDE` into unattended prompts declaring that user authorization for writing `rula.toml` and `AGENTS.md` is pre-approved.
  - Formatted semi-attended phase prompts to pass `project_path` directly into `request_user_approval`.
- Updated [`src-tauri/src/mcp.rs`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src-tauri/src/mcp.rs) to include `project_path` in `request_user_approval` tool definition.

---

## 4. Verification
- Terminated dangling initialization process PID `1138227`.
- `cargo check --manifest-path src-tauri/Cargo.toml`: Passed with 0 errors.
- `npm run build` (`vue-tsc --noEmit && vite build`): Passed with 0 errors.
