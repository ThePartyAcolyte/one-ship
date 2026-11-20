# Development Log: OpenDictate Live Speech Streaming & Automatic Click Reservation

**Date:** 2026-09-25  
**Topic:** OpenDictate D-Bus Live Streaming Integration, Click-Driven Reservation, and Navigation Resiliency  
**Version:** v0.2.2  

---

## 1. Context & Objectives

One-Ship / Wang-Kapawu integrates speech-to-text dictation with the host system's OpenDictate daemon (`org.kirulab.OpenDictate`). Previous iterations relied on synthetic clipboard pasting (`wl-copy`) and keystroke emulation (`wtype`), which suffered from Wayland modifier race conditions and focus-stealing issues in WebKitGTK.

The objective of this work cycle was to integrate the native real-time streaming D-Bus contract defined in `dictate-whisper/docs/DBUS_API_CONTRACT.md`:
1. Arm an OpenDictate capture reservation automatically whenever an agent prompt input (`<AppTextarea>`) is clicked or focused.
2. Stream transcribed text in real-time (`InterimText`) directly into the agent's input draft without clipboard tampering or keystroke emulation.
3. Decouple transcription lifecycle entirely: capture initiation, pause, cancel, and stop are controlled by the user via global OS shortcuts (e.g. `Super+D`) and OpenDictate.
4. Guarantee navigation resiliency: if the user clicks away, navigates to documentation, or inspects files while speaking, the transcription continues accumulating specifically into the target agent's draft.
5. Provide a subtle, non-intrusive visual indicator (watermark microphone icon, `pointer-events-none`) reflecting `ready` (armed) and `rec` (recording) states.

---

## 2. Technical Implementation Summary

### A. Backend Rust Subsystem (`src-tauri/src/kopeng/opendictate.rs` & `src-tauri/src/lib.rs`)
- **Robust GVariant Tuple Parser (`parse_gvariant_tuple`):**
  Implemented a deterministic token and string parser that handles both single and double quotes, nested quotes, and escape sequences (`\'`, `\"`, `\n`, `\\`), replacing brittle string splitters.
- **Persistent Signal Monitor (`spawn_dbus_monitor`):**
  Spawned a persistent background reader on `gdbus monitor --session --dest org.kirulab.OpenDictate` in `lib.rs` setup, dispatching typed `DaemonEvent` messages:
  - `opendictate_session_started`: audio capture active.
  - `opendictate_interim_text`: cumulative real-time transcription buffer.
  - `opendictate_session_finished`: final transcript (preferring LLM-cleaned text if available).
  - `opendictate_session_cancelled` & `opendictate_session_released`: session abort/teardown.
- **Structured Reservation Command (`reserve_voice_capture`):**
  Accepts `VoiceReservationRequest` specifying `session_id`, `agent_id`, `project_id`, `project_name`, and `hide_bubble: true` to prevent external GTK OSD popups.

### B. Frontend Architecture & Pinia Store (`src/stores/voiceStore.ts` & `dialogueStore.ts`)
- **Single-Responsibility Voice Domain Store (`voiceStore.ts`):**
  Tracks `isArmed`, `isRecording`, `activeSessionId`, and `targetAgentId`.
  Maintains `baseText` to ensure pre-existing typed text is never overwritten by incoming speech.
  Registers native Tauri event listeners with proper unlisten lifecycle tracking.
- **Agent Input Drafts in `dialogueStore.ts`:**
  Added `agentInputDrafts: Record<string, string>` with `getAgentDraft(agentId)` and `setAgentDraft(agentId, text)`, decoupling the text buffer from unmounted DOM components.

### C. UI & Brutalist Visual Watermark (`ConversationPanel.vue` & `FloatingChatOverlay.vue`)
- **Eliminated Manual Action Button:**
  Removed the dummy `Mic` action button from the input row; all lifecycle controls are externalized to user shortcuts.
- **Two-Way Reactive Draft Binding:**
  Bound `inputText` as a writable computed property synchronizing with `dialogueStore.getAgentDraft(agentId)`.
- **Subtle Visual Watermark Indicator:**
  Embedded an unobtrusive, non-interactive (`pointer-events-none`) pulsing microphone icon in the top-right corner of the textarea, displaying `ready` when armed and `rec` when recording.
- **Event Forwarding in `AppTextarea.vue`:**
  Added `@click` and `@focus` event emission to reliably trigger `armReservationForAgent` on first click.

---

## 3. Verification & Compliance

- **Compilation Check:**
  - `cargo check`: Passed with 0 errors.
  - `vue-tsc --noEmit && vite build`: Passed with 0 errors.
- **Technical Debt Register:**
  Updated `TECHNICAL_DEBT.md` item #23 to `RESOLVED - v0.2.2`.
