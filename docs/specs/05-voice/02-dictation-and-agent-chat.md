# Native Gemini Dictation

## Purpose

Defines ONE-SHIP's native speech-to-text dictation: PCM capture through `pw-record`, streaming
transcription with `models/gemini-3.5-transcribe-live`, provisional interim text, context-aware
insertion into the focused text field, VAD-driven Live session rollover, and uninterrupted audio
replay when the WebSocket drops. OpenDictate is no longer part of this path; it remains an external
tool the user may keep for other applications.

| Property | Value |
| :--- | :--- |
| Document version | 2.3.1 |
| Domain | Gemini Live speech-to-text, VAD session management, focused-field insertion |
| Maintainer | ONE-SHIP / Omarchy development team |

## Scope

In scope: `src-tauri/src/dictation.rs`, `src-tauri/src/audio_capture.rs`, `src/stores/dictationStore.ts`, `src/composables/useVoiceInput.ts`, `src/utils/dictationTargets.ts`, `FloatingChatOverlay.vue`, `ProjectWorkspace.vue`, `useDocumentComments.ts`, and the `[plugins.dictation]` configuration section.

Out of scope: the conversational Global Voice Controller (see [Global Voice Controller](03-global-voice-controller.md)), OpenDictate's D-Bus protocol, local Whisper models, and LLM post-processing/cleanup passes.

## Model / Design

### Single model and audio path

```text
[Alt+V in a text field / mic button]
        │
        ▼
[useVoiceInput] ── focused target or chat/comment target
        │
        ▼
[dictationStore] ── daemonCall("start_dictation")
        │
        ▼
[oneship-daemon: dictation.rs]
        ├── pw-record (16 kHz PCM16 mono, 50 ms frames)
        ├── append every frame to <state>/one-ship/dictation/<session>.pcm
        ├── VAD (normalized RMS) + Live session watchdog
        └── WSS BidiGenerateContent (models/gemini-3.5-transcribe-live)
                inputAudioTranscription.mode = SMART | VERBATIM
                realtimeInputConfig.automaticActivityDetection.disabled = true
        │
        ▼
[SSE] dictation-interim-text / dictation-finished / dictation-cancelled / dictation-state
        │
        ▼
[dictationStore] replaces only its provisional span in the target field
```

Only `gemini-3.5-transcribe-live` is used. There is no batch transcription model and no fallback to
another provider. Recovery also uses the Live interface.

### Contextual voice shortcut

`Alt+V` is shared with the Global Voice Controller:

| Active state | Focused element | Action |
| :--- | :--- | :--- |
| None | Text input/textarea | Start native dictation |
| None | Anything else | Start Voice Controller PTT |
| Dictation active | Any | Stop and commit the dictation |
| PTT active | Any | Stop/send the voice turn |
| Dictation processing | Any | Ignore with a status message |

`Escape` and `Alt+Backspace` cancel the active session. A process-wide `CaptureLease`
(`audio_capture.rs`) prevents dictation and PTT from capturing the microphone simultaneously; the
second caller receives `MicrophoneBusy`.

### Focused-field insertion

`dictationTargets.ts` captures the focused `<input>`/`<textarea>`, its base value, and its selection.
Interim text replaces only the provisional span; the final transcript replaces that span once. A
reactive value target is used by the document-comment popover. If the user edits the field during
dictation, the session is cancelled and the user's edit is preserved.

### Transcript reconciliation

The Live API re-emits the complete transcript with corrections, especially in `SMART` mode. The
daemon therefore keeps two independent buffers:

* **Authoritative text:** `committed_text` plus `current_turn`, fed exclusively by
  `inputTranscription` events.
* **Provisional text:** `interim_text`, fed by `interimInputTranscription` and never committed.

A new authoritative event is treated as a full-session revision when its word coverage of the
committed transcript is at least 70% and its length is comparable; it then replaces the committed
text instead of being appended. Prefix checks alone are insufficient because corrections often
change the first words. Interim hypotheses only replace the provisional display span; the next
authoritative event or `turnComplete` discards them. This prevents the repeated “audio doblado”
transcript produced when each corrected full-text revision was concatenated.

### Auto-send

`[plugins.dictation].auto_send` (default `true`) is toggled from the top-bar microphone button
(green when enabled). When the final transcript targets an agent chat prompt and the field is still
intact, the daemon result is followed by `dialogueStore.submitAgentPrompt`. Comments and generic
fields are never submitted.

## Live stability

1. **No duration limit:** dictation runs until the user stops or cancels it. There is no
   `max_session_seconds`, no auto-stop, and no forced rollover while the user is speaking.
2. **Disk-backed audio:** every PCM frame is appended to the session file. The file is removed after
   a successful final transcript or a cancel; a failed session keeps it and reports its path.
3. **VAD rollover:** after `rollover_min_session_seconds` and `rollover_silence_ms` of continuous
   silence, the daemon finalizes the current Live session and opens a fresh one before the server
   closes it. Audio captured during the transition is buffered and sent to the new session.
4. **Reconnect and replay:** on a WebSocket error, close, `goAway`, outbound queue overflow, or
   speech stall, the daemon reconnects (up to `reconnect_attempts_count`) and replays the
   unconfirmed audio from the last confirmed offset through the new Live session at real-time pace.
   The transcript may lag behind the microphone while replaying, but no audio is dropped.
5. **Watchdog:** recent speech with no transcription for `watchdog_silence_seconds` triggers the
   same replay path.
6. **Finalization:** stopping sends `activityEnd` + `audioStreamEnd`, drains the pending audio, and
   waits up to `finalize_timeout_seconds` for the final segment. A timeout returns the accumulated
   transcript and preserves the PCM file; it never deletes audio.
7. **Bounded memory:** PCM lives on disk; only fixed-size frame buffers and bounded queues stay in
   memory. Disk exhaustion surfaces as an explicit I/O error.

## Contracts

### Daemon commands

| Command | Arguments | Result |
| :--- | :--- | :--- |
| `start_dictation` | `{}` | `DictationSessionInfo { sessionId, mode, model }` |
| `stop_dictation` | `{ sessionId }` | `DictationResult { sessionId, text, outcome, recoveryFilePath }` |
| `cancel_dictation` | `{ sessionId }` | `null` |
| `get_dictation_status` | `{}` | `DictationStatus` |
| `set_dictation_auto_send` | `{ enabled }` | `DictationStatus` |
| `get_gemini_api_key_status` | `{}` | `GeminiApiKeyStatus` |
| `set_gemini_api_key` | `{ apiKey }` | `GeminiApiKeyStatus` |
| `clear_gemini_api_key` | `{}` | `GeminiApiKeyStatus` |

### SSE events

| Event | Payload |
| :--- | :--- |
| `dictation-state` | `{ sessionId, state, lastError, recoveryFilePath }` |
| `dictation-interim-text` | `{ sessionId, text }` |
| `dictation-finished` | `{ sessionId, text, outcome, recoveryFilePath }` |
| `dictation-cancelled` | `{ sessionId }` |

### Runtime states

`idle`, `connecting`, `listening`, `processing`, `error`.

### Configuration (`[plugins.dictation]`, `scope = "global_only"`)

| Key | Type | Default |
| :--- | :--- | :---: |
| `enabled` | boolean | `true` |
| `mode` | `smart` \| `verbatim` | `smart` |
| `auto_send` | boolean | `true` |
| `speech_energy_threshold` | number | `0.03` |
| `rollover_min_session_seconds` | number | `90` |
| `rollover_silence_ms` | number | `800` |
| `rollover_finalize_timeout_seconds` | number | `5` |
| `watchdog_silence_seconds` | number | `10` |
| `finalize_timeout_seconds` | number | `120` |
| `reconnect_attempts_count` | number | `5` |
| `recovery_min_interval_seconds` | number | `5` |
| `stale_audio_retention_hours` | number | `24` |

The Gemini API key is stored in the OS keyring (`service=one-ship`,
`username=gemini_api_key`) through `secret-tool`. It is never written to `config.toml` or returned
to the Vue frontend. A legacy plaintext key is migrated once and removed from the TOML.

## Implementation Status

| Item | Status | Evidence |
| :--- | :--- | :--- |
| Live transcription engine, VAD rollover, replay, watchdog | Implemented | `src-tauri/src/dictation.rs` |
| Transcript reconciliation (interim display-only, full-session revision replacement) | Implemented | `dictation.rs` (`apply_transcription`, `word_coverage`), regression tests |
| Microphone lease shared with PTT | Implemented | `src-tauri/src/audio_capture.rs`, `voice_controller.rs` |
| Contextual `Alt+V`, focused-field insertion, auto-send | Implemented | `useVoiceInput.ts`, `dictationStore.ts`, `dictationTargets.ts` |
| Keyring API key storage and redaction | Implemented | `secret_store.rs`, `config.rs`, `SettingsModal.vue` |
| OpenDictate integration removal | Implemented | `kopeng/opendictate.rs` deleted; no D-Bus monitor or commands |

## Related Specs

* [Global Voice Controller](03-global-voice-controller.md)
* [Unified Capability Core and Voice Orchestrator Harness](04-unified-capability-core-and-voice-harness.md)
* [Configuration Schema and Cascade](../01-architecture/02-configuration-schema-and-cascade.md)
* [README](README.md)

Supersedes legacy: `04_dictation_and_nlp.md` and the removed OpenDictate IPC spec.
