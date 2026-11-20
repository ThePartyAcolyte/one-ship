# Devlog: Agent Stop Button Accuracy, OpenDictate Visual State Machine & Input Layout Homogeneity

**Date:** 2026-09-27  
**Scope:** Backend supervisor process status protocol, Pinia dialogue & voice stores, prompt textarea alignment, OpenDictate D-Bus status querying and auto-capture lifecycle.

---

## 1. Problem Overview & Root Cause Analysis

1. **Inaccurate Stop Button on Resumed / Idling Agents:**
   - When launching an agent or resuming a conversation context (`resume_agent_session`), the REPL subprocess emitted an `init` event containing the session ID.
   - In `src-tauri/src/supervisor.rs`, this was forwarded via `agent-step-update` with `"state": "ACTIVE"`. Because initialization is an idle readiness event rather than a working execution step, and no subsequent `DONE` event is emitted until user instructions are sent, `dialogueStore.activeSteps[agentId]` remained locked in an active state indefinitely.
   - The UI computed `isWorking` as true whenever `currentLiveStep.state === 'ACTIVE'`, displaying a phantom Stop button and an active thought bubble for agents that were actually `Idle`.

2. **OpenDictate Input Visual Feedback & Auto-Capture:**
   - The microphone icon in the chat prompt box previously displayed textual badges (`"ready"`, `"rec"`), violating brutalist minimalist design guidelines prohibiting superfluous text.
   - The icon was pinned to the top-right rather than vertically centered, and lacked feedback when OpenDictate was busy with another session.
   - When a user focused the input while OpenDictate was busy, no mechanism existed to automatically claim the next speech turn once OpenDictate returned to an idle state.

3. **Mismatched Input Button Heights:**
   - The Attach file (`Paperclip`) and Send/Stop buttons had variable padding and alignments (`items-end`), causing them to differ in rendered height from the prompt textarea.

---

## 2. Architectural Changes & Key Decisions

1. **Supervisor Initialization State Fix (`supervisor.rs`):**
   - Updated both OpenCode (line 1382) and AGY (line 1674) `init` event forwarders to emit `"state": "DONE"` with `"step_type": "init"`.
   - This notifies the UI that session setup has concluded cleanly without leaving an active working step in the frontend pipeline.

2. **Defense-in-Depth in Frontend State (`dialogueStore.ts`, `ConversationPanel.vue`, `FloatingChatOverlay.vue`):**
   - In `dialogueStore.ts`: `resumeSession` immediately clears `activeSteps[lowerId]`, and `loadActiveConversation` purges any stale steps whose `stepType === 'init'`.
   - In `ConversationPanel.vue` and `FloatingChatOverlay.vue`: `isWorking` explicitly filters out `currentLiveStep.stepType === 'init'`.

3. **OpenDictate D-Bus Status Query & Auto-Capture (`opendictate.rs`, `voiceStore.ts`):**
   - Added `get_opendictate_status` Tauri IPC command in `src-tauri/src/kopeng/opendictate.rs` to query `org.kirulab.OpenDictate.GetStatus`.
   - In `voiceStore.ts`:
     - Introduced `isBusy`, `isWaitingForFree`, and `pendingParams` state tracking.
     - When `reserve_voice_capture` fails because OpenDictate is busy, `voiceStore` enters `isWaitingForFree = true`.
     - Subscribed to D-Bus signal forwarders (`opendictate_session_finished`, `opendictate_session_cancelled`, `opendictate_session_released`) and initiated a 1200ms background poll.
     - As soon as OpenDictate becomes free, `tryClaimPendingReservation()` automatically secures the next turn for the focused input.
     - `cancelIfIdle()` tears down the polling timer and releases any un-dictated reservations upon focus blur.

4. **Microphone Indicator Refinement & Layout Homogeneity:**
   - Removed all textual labels and tooltips from the microphone icon.
   - Positioned the icon at `absolute right-2.5 top-1/2 -translate-y-1/2` with `pr-8` on the textarea to prevent text overlap.
   - Formatted the icon into 4 distinct visual states:
     - `idle`: Dim opacity (`opacity-25 text-[var(--dim-color)]`).
     - `busy`: Subtle warning color (`opacity-60 text-[var(--status-warn-color)]`).
     - `reserved`: Pulsing accent color (`opacity-90 text-[var(--accent-color)] animate-pulse`).
     - `recording`: High-visibility error/recording color with pulse (`text-[var(--status-error-color)] opacity-100 animate-pulse`).
   - Standardized input row flexbox with `items-stretch` and fixed button dimensions (`w-[34px] h-full min-h-[34px] flex items-center justify-center`), ensuring exact 1:1 height parity across the file attachment button, prompt textarea, and send/stop button.

---

## 3. Verification & Build Diagnostics

- Rust compilation verified via `cargo check --manifest-path src-tauri/Cargo.toml` (0 warnings, 0 errors).
- TypeScript & Vue frontend compilation validated via `npm run build` (`vue-tsc --noEmit && vite build` succeeded cleanly).
