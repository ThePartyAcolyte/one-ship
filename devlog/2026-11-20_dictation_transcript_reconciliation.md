# 2026-11-20 — Dictation Transcript Reconciliation Fix

**Topic:** Fix duplicated dictation text caused by committing interim hypotheses and appending
corrected full-session revisions instead of replacing them.

## Symptom

A real dictation session produced a transcript where the same passage appeared repeatedly with
small corrections, for example:

```text
2400 Ya este es un Ya esta es una prueba. de dictado eh en tiempo real usando el motor local
de dictado Adiós Ya, esta es una prueba de dictado eh en tiempo real usando el motor local de
dictado. Adiós OpenDictate. Te extrañaremos. Ajá, tenemos audio doblado. A Ya, esta es una
prueba de dictado eh en tiempo real usando el motor local de dictado...
```

The API re-emits the complete transcript with corrections during a session. The daemon was
concatenating those revisions instead of replacing the previous text.

## Root cause

1. `interimInputTranscription` events were written into `current_turn` and then committed together
   with the authoritative transcript. Interim hypotheses are speculative and must never be
   committed.
2. Cumulative-revision detection only recognized literal prefixes or the first 5–10 words. A
   correction early in the text (“Ya este es un…” → “Ya esta es una…”) failed every prefix check,
   so `join_segments` appended the whole full-session revision.

## Fix

`src-tauri/src/dictation.rs`:

- Added `TranscriptState::interim_text`; interim events only update the provisional display and are
  cleared by the next authoritative event or `turnComplete`.
- Added `TranscriptState::apply_transcription`: a full-session revision replaces `committed_text`;
  a segment-sized update merges into `current_turn`; `finished` commits the turn.
- Added `word_coverage`: a multiset word-overlap ratio. `is_cumulative_revision` now treats an
  incoming text as a revision when it is comparable in length and covers at least 70% of the base
  words, even if corrections changed the opening words.
- On recovery/rollover, both `current_turn` and `interim_text` are cleared before replay.
- `full_text()` combines authoritative text with the provisional interim only for display.

`docs/specs/05-voice/02-dictation-and-agent-chat.md` now documents the authoritative/provisional
split and the word-coverage revision rule.

## Regression tests

Added and ran:

```text
src-tauri/target/debug/deps/oneship_lib-<build> dictation::tests --nocapture
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

`./scripts/run_dev.sh export-bindings` also passed:

```text
test result: ok. 422 passed; 0 failed; 0 ignored; 0 measured; 186 filtered out
```

## Runtime confirmation

The user confirmed the corrected build works in the running application after restarting the
daemon/GUI: the repeated “audio doblado” text no longer appears.

## Remaining checks

- Force a WebSocket disconnect and confirm the replay path still produces a single clean final
  transcript.
- Confirm VAD rollover during a long session does not introduce duplicated text.
- Confirm a 10-minute continuous dictation.
