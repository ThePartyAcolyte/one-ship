> [!WARNING]
> **LEGACY SPECIFICATION:** Superseded by [05-voice/02-dictation-and-agent-chat.md](../../specs/05-voice/02-dictation-and-agent-chat.md). Preserved for historical provenance only.

# Spec 04: Dictation and NLP (OpenDictate Integration)
**Document Version:** 2.2.0  
**Domain:** Voice Capture, D-Bus IPC, Real-Time Speech Streaming & Agent Chat Routing  
**Maintainer:** ONE-SHIP / Omarchy Development Team

---

## 1. D-Bus Session Bus Architecture

`oneship-daemon` (`src-tauri/src/kopeng/opendictate.rs`) communicates with OpenDictate over the Freedesktop D-Bus session bus under destination `org.kirulab.OpenDictate` at path `/org/kirulab/OpenDictate`, streaming events to the GUI over `/api/events` SSE (`listenDaemonEvent` in `src/stores/voiceStore.ts`).

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                 OPENDICTATE REAL-TIME STREAMING LIFECYCLE                   │
│                                                                             │
│  [ Agent Chat Textarea ] ──► Click / Focus Input Field or Mic Button        │
│               │                                                             │
│               ▼                                                             │
│  1. Generates deterministic session ID:                                     │
│     `oneship-${normalizedAgentId}-${Date.now()}`                            │
│  2. Invokes daemon command `reserve_voice_capture` -> D-Bus:                │
│     ReserveCaptureSession({ session_id, app_name, hide_bubble: true })      │
│               │                                                             │
│               ▼                                                             │
│  [ UI: Enters ARMED state (isArmed = true, mic indicator pulses) ]          │
│               │                                                             │
│       ┌───────┴─────────────────────────────────────────┐                   │
│       ▼ (User blurs textarea before speaking)           ▼ (User begins talking)│
│  `cancel_voice_capture` ->                        OpenDictate emits:        │
│  ReleaseReservedSession(session_id)               SessionStarted(session_id)│
│  Reservation cancelled cleanly                           │                  │
│  (No foreign app transcription hijacking)                ▼                  │
│                                                  [ RECORDING state ]        │
│                                                  Focus loss ignored;        │
│                                                  hands-free dictation       │
│                                                         │                   │
│                                                         ▼                   │
│                                                  OpenDictate emits:         │
│                                                  InterimText(session_id, ..)│
│                                                  - Real-time in-place       │
│                                                    cumulative rewrite       │
│                                                         │                   │
│                                                         ▼                   │
│                                                  OpenDictate emits:         │
│                                                  SessionFinished(..)        │
│                                                  - Commits final transcript │
│                                                  - Auto-sends if enabled    │
│                                                  - Unarms session           │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Zero-Modal Asynchronous Workflow & Click-to-Reserve

* **Click-to-Reserve in Agent Textareas:** Clicking or focusing an agent chat prompt input (`AppTextarea.vue` / `FloatingChatOverlay.vue`) or clicking the interactive microphone toggle invokes `reserve_voice_capture`. The session ID (`oneship-${normalizedAgentId}-${Date.now()}`) is bound to the target agent and project path.
* **External OSD Bubble Suppression (`hide_bubble: true`):** When reserving capture, ONE-SHIP explicitly passes `hide_bubble: true`. OpenDictate suppresses its desktop floating bubble/OSD, allowing transcribed text to stream seamlessly into ONE-SHIP's textarea.
* **Interactive Microphone Indicator & Tooltip:**
  * Inside `AppTextarea.vue`, a passive microphone status indicator reflects the active voice state (`idle`, `busy`, `reserved`, `recording`).
  * In `FloatingChatOverlay.vue`, the input toolbar renders an interactive microphone button wrapped in `<AppTooltip>` (`Voice dictation (Click to arm)`, `Microphone armed — speak now (Click to release)`, `Recording voice... [Auto-send]`, or `OpenDictate offline`).
* **Auto-Send Detection (`/tmp/opendictate_state.json`):** When `SessionFinished` arrives, `opendictate.rs` inspects `/tmp/opendictate_state.json` (`autosend_enabled` / `config.auto_send`) and attaches `auto_send: bool` to `SessionFinishedPayload`. When `auto_send` is `true`, `voiceStore.ts` automatically submits the completed transcript to the target agent via `dialogueStore.submitAgentPrompt`.
* **External Voice Lifecycle Control:** The user initiates, stops, or cancels voice dictation using standard system-wide shortcuts (e.g. `Super+D`), OpenDictate top bar triggers, or by clicking the armed mic icon to release the reservation.

---

## 3. Blur-to-Cancel Invariant & In-Flight Protection

To prevent accidental reservation hijacking without degrading hands-free multitasking:

1. **Idle Blur Cancellation (`isArmed && !isRecording`):**
   * If the user clicks into an agent's chat textarea and then clicks elsewhere or switches applications *before* uttering speech, `cancelIfIdle()` triggers.
   * ONE-SHIP immediately calls `cancel_voice_capture` (`ReleaseReservedSession(session_id)`) via D-Bus and unarms `voiceStore`.
   * This guarantees that if the user subsequently begins dictation in another application (e.g. terminal, browser, editor), OpenDictate is not locked to ONE-SHIP.
2. **In-Flight Recording Protection (`isRecording === true`):**
   * Once OpenDictate emits `SessionStarted`, the session transitions to `isRecording = true`.
   * Subsequent `blur` events (clicking outside the window, inspecting files, switching tabs, reading diffs) are strictly **ignored**.
   * Transcription continues streaming into the originating agent's draft buffer hands-free until dictation is concluded.

---

## 4. Real-Time Cumulative Streaming & Text Preservation

* **`baseText` Preservation:** When audio capture begins (`SessionStarted`), the current text in the textarea is captured as `baseText`.
* **Cumulative Rewrite Buffer (`InterimText`):**
  * OpenDictate emits `InterimText(session_id, interim_text)` as the speech engine refines its acoustic hypotheses.
  * `interim_text` is the complete cumulative transcription of the current utterance (not an incremental delta token).
  * ONE-SHIP updates the textarea draft reactively: `${baseText}${baseText.length > 0 ? ' ' : ''}${interimText}`.
  * Typing or editing is cleanly merged without cursor tearing or DOM corruption.
* **Zero Side-Effects Guarantee:**
  * OpenDictate does not manipulate the system clipboard (`wl-copy`) or inject synthetic keypresses (`ydotool`).
  * Text delivery is pure data IPC directly into Pinia state (`dialogueStore.agentInputDrafts`).

---

## 5. Real-Time D-Bus Signal Listener & Daemon SSE Bridge

A persistent background loop in `src-tauri/src/kopeng/opendictate.rs` monitors `org.kirulab.OpenDictate` via `gdbus monitor` and broadcasts `DaemonEvent` payloads over `/api/events` SSE:
* **`SessionReserved(session_id, app_name, accent_color)`:** Emits `opendictate_session_reserved` (`SessionReservedPayload`). Synchronizes armed state or evicts local reservation if claimed by an external application.
* **`SessionStarted(session_id)`:** Emits `opendictate_session_started`. Sets `isRecording = true`.
* **`InterimText(session_id, interim_text)`:** Emits `opendictate_interim_text`. Updates live draft buffer in real-time.
* **`SessionFinished(session_id, raw, final, status)`:** Emits `opendictate_session_finished` (including `auto_send: bool`). Replaces interim text with final transcript, optionally auto-submits, and clears session.
* **`SessionCancelled(session_id)`:** Emits `opendictate_session_cancelled`. Restores `baseText` and unarms session.
* **`SessionReservationReleased(session_id)`:** Emits `opendictate_session_released`. Cleanly unarms local state if evicted or released remotely.

---

## 6. Continuous Focus Auto-Capture & Visual State Taxonomy

1. **`idle` (Standby / Unfocused):**
   * Subtle dim appearance (`opacity-25 text-[var(--dim-color)]`).
   * Indicates speech engine is ready for reservation upon focus or explicit interaction.
2. **`busy` (Blocked / Waiting for Turn):**
   * Muted warning color (`opacity-60 text-[var(--status-warn-color)]`).
   * Occurs when user focuses the prompt input while OpenDictate is actively capturing another session.
   * `voiceStore` enters `isWaitingForFree = true`, monitors D-Bus completion signals, and polls `get_opendictate_status` every 1200ms.
   * As soon as OpenDictate transitions to `is_busy: false`, the next turn is automatically claimed without user intervention.
3. **`reserved` (Armed & Ready):**
   * Pulsating accent color (`opacity-90 text-[var(--accent-color)] animate-pulse`).
   * Confirms reservation is locked in OpenDictate; user may speak immediately.
4. **`recording` (Active Speech Capture):**
   * Vivid recording color (`opacity-100 text-[var(--status-error-color)] animate-pulse`).
   * Cumulative speech hypothesis streaming into input draft via `opendictate_interim_text`.
5. **Teardown on Blur:**
   * When input focus blurs while idle, `cancelIfIdle()` immediately clears waiting timers and releases un-dictated reservations.
