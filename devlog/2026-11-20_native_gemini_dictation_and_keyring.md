# 2026-11-20 — Native Gemini Live Dictation and OS Keyring Credentials

**Topic:** Remove ONE-SHIP's OpenDictate integration, add native Gemini Live dictation into focused
text fields, share `Alt+V` with the Voice Controller, add VAD-driven Live rollover/replay, and move
the Gemini API key to the OS keyring.

## Summary

- Added `src-tauri/src/dictation.rs`: `models/gemini-3.5-transcribe-live`, PCM capture through
  `pw-record`, disk-backed session audio, VAD rollover, reconnect/replay, watchdog, cumulative
  interim text, and `SMART`/`VERBATIM` modes.
- Transcript reconciliation now treats interim hypotheses as display-only and detects full-session
  revisions by word coverage, not only by a literal prefix. This prevents the transcript from
  appending each corrected full-text revision (the "audio doblado" symptom).
- Added `src-tauri/src/audio_capture.rs`: process-wide microphone lease shared with the Global Voice
  Controller.
- Added `src-tauri/src/secret_store.rs`: Gemini API key storage in the FreeDesktop Secret Service
  via `secret-tool`, with a bounded timeout.
- `config.rs` now redacts the key from `get_global_config`, rejects plaintext key writes in
  `save_global_config`, migrates a legacy `config.toml` key once, and exposes
  `get_gemini_api_key_status` / `set_gemini_api_key` / `clear_gemini_api_key`.
- Removed `kopeng/opendictate.rs`, its three daemon commands, six SSE events, D-Bus monitor,
  diagnostic flags, `voiceStore.ts`, and the installer's `FEAT_VOICE`/OpenDictate clone flow.
- Added native dictation UI: contextual `Alt+V`, chat/comment microphones, top-bar auto-send toggle,
  keyring-backed API key field, and `[plugins.dictation]` settings.
- Updated living specs, README, installer, TODO and technical debt references.
- Follow-up: the transcript reconciliation bug found during runtime validation is documented in
  [`2026-11-20_dictation_transcript_reconciliation.md`](2026-11-20_dictation_transcript_reconciliation.md).

## Behavior

- `Alt+V` inside an `<input>`/`<textarea>` starts dictation; outside one it starts Voice Controller
  PTT. An active session always wins, so the same shortcut stops/cancels it.
- Dictation has no duration limit and no auto-stop. PCM is appended to
  `$XDG_STATE_HOME/one-ship/dictation/<session>.pcm`; the file is removed after a successful final
  transcript or cancel, and kept with a reported path after an unrecoverable Live failure.
- When the Live WebSocket drops, the daemon reconnects up to five times and replays the unconfirmed
  audio from the last confirmed offset through a fresh Live session at real-time pace. The
  transcript may lag, but no audio is discarded.
- After `rollover_min_session_seconds` and `rollover_silence_ms` of silence, the daemon finalizes
  the current Live session and opens a fresh one proactively. The transition never cuts speech.
- `auto_send` defaults to `true`, is toggled from the top-bar microphone, and only submits when the
  target is an agent chat prompt.

## Files

**Backend**

- Added: `src-tauri/src/dictation.rs`, `src-tauri/src/audio_capture.rs`, `src-tauri/src/secret_store.rs`.
- Modified: `src-tauri/src/config.rs`, `api.rs`, `events.rs`, `lib.rs`, `bin/daemon.rs`,
  `kopeng/mod.rs`, `kopeng/diagnostic.rs`, `voice_controller.rs`, `Cargo.toml`.
- Deleted: `src-tauri/src/kopeng/opendictate.rs`, stale `src-tauri/bindings/` and
  `src-tauri/src/types/`, and the old generated OpenDictate TypeScript files.

**Frontend**

- Added: `src/stores/dictationStore.ts`, `src/composables/useVoiceInput.ts`,
  `src/utils/dictationTargets.ts`.
- Modified: `src/App.vue`, `src/components/AppHeader.vue`,
  `src/components/dialogue/FloatingChatOverlay.vue`,
  `src/components/workspace/ProjectWorkspace.vue`, `src/composables/useDocumentComments.ts`,
  `src/components/modals/SettingsModal.vue`, `src/stores/coreStore.ts`, `src/stores/index.ts`,
  `src/types/daemonApi.ts`, `src/types/index.ts`, `src/utils/daemonEvents.ts`,
  `src/utils/integrationGuides.ts`.
- Deleted: `src/stores/voiceStore.ts`.

**Distribution & docs**

- `install.sh`: removed `FEAT_VOICE`, `--features voice`, and the OpenDictate repository clone/install
  step. OpenDictate itself is not uninstalled.
- Rewrote `docs/specs/05-voice/02-dictation-and-agent-chat.md` and the `05-voice` index; removed the
  OpenDictate IPC spec; updated architecture, diagnostics, chat overlay, artifacts, README, TODO and
  technical debt references.
- Bumped the application to `0.2.2`.

## Verification

### Bindings and Rust tests

Command:

```text
./scripts/run_dev.sh export-bindings
```

Result (tail):

```text
test result: ok. 422 passed; 0 failed; 0 ignored; 0 measured; 182 filtered out; finished in 0.42s
```

The run compiled `one-ship v0.2.2` and exported the new `Dictation*`, `GeminiApiKeyStatus`,
`SetGeminiApiKeyArgs` and `SetDictationAutoSendArgs` bindings.

### Dictation transcript tests

Command:

```text
src-tauri/target/debug/deps/oneship_lib-bbf4e676e9eeb6b2 dictation::tests --nocapture
```

Result:

```text
running 9 tests
test dictation::tests::api_errors_are_classified_for_the_frontend ... ok
test dictation::tests::setup_payload_uses_transcribe_live_and_requested_mode ... ok
test dictation::tests::authoritative_full_session_revisions_replace_committed_text ... ok
test dictation::tests::new_utterances_without_shared_content_are_appended ... ok
test dictation::tests::cumulative_smart_revisions_replace_instead_of_appending ... ok
test dictation::tests::interim_hypotheses_are_display_only_and_never_committed ... ok
test dictation::tests::cumulative_turn_updates_do_not_duplicate ... ok
test dictation::tests::full_session_revisions_with_early_corrections_replace_base_text ... ok
test dictation::tests::overlapping_and_repeated_segments_are_deduplicated ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 599 filtered out
```

### Rule gate

Command:

```text
./scripts/verify_rules.sh
```

Result (tail):

```text
[FAIL] Zero raw bg-black/* or bg-[#...] outside ui.css
... existing RollingStatusLog.vue, ConfirmDeleteModal.vue, ScriptLaunchModal.vue,
    RemovalFailedModal.vue, ApprovalRequestModal.vue, DocstyleSelectionModal.vue,
    TaskReportModal.vue hits ...
[FAIL] Unstructured Result<_, String> found outside tests:
src-tauri/src/config.rs:2044:pub fn migrate_toml_snapshots_to_vcs(content: &str) -> Result<Option<String>, String>
=== Summary: 2 hard failure(s) ===
```

Both failures are pre-existing and outside this work cycle. No new `any`, native `title=`,
`backdrop-blur`, hardcoded `/home/`, or lock `.unwrap()` violations were introduced.

### OpenDictate removal

Command:

```text
grep -rn "OpenDictate\|opendictate" . --include=*.md --include=*.sh --include=*.ts \
  --include=*.vue --include=*.rs --include=*.json | grep -v /devlog/ | grep -v /docs/legacy/
```

Result: only the intentional historical notes in
`docs/specs/05-voice/02-dictation-and-agent-chat.md`. No runtime, installer, or UI reference remains.

### Daemon health

Command:

```text
./scripts/run_dev.sh status
```

Result: `daemon` active/running and listening; `daemon_watch` and `vite` active. The `gui` unit still
reports the earlier pre-fix compile log (`secret_store` missing from `crate`), but that error was
fixed by adding the module to `lib.rs`; the GUI unit was not restarted manually because AGENTS.md
forbids self-managed daemon/GUI lifecycle changes.

## Remaining runtime checks

- Re-enter or confirm the Gemini API key through Settings → Voice Controller after the keyring
  migration; verify no `gemini_api_key` value remains in `config.toml` and `get_global_config` omits
  it.
- Verify a 10-minute dictation session, a forced WebSocket close with replay, a VAD rollover at
  silence, cancellation, auto-send on a chat prompt, and the comment popover target.
- Run the GUI build/type check when the next authorized build cycle is started; `vue-tsc` was not
  executed separately to respect AGENTS.md §4.
