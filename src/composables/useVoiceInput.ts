import { useCoreStore } from '../stores/coreStore';
import { useDictationStore } from '../stores/dictationStore';
import { useVoiceControllerStore } from '../stores/voiceControllerStore';
import { createDomTarget, isDictationElement } from '../utils/dictationTargets';

/**
 * Contextual voice entry point. `Alt+V` dictates when a ONE-SHIP text field is focused and
 * toggles the conversational Voice Controller everywhere else. An already active session always
 * wins, so the same shortcut stops/cancels whatever is running.
 */
export function useVoiceInput() {
  const dictation = useDictationStore();
  const voiceController = useVoiceControllerStore();
  const coreStore = useCoreStore();

  function log(message: string) {
    coreStore.addStatusLog(`[VoiceInput] ${message}`);
  }

  async function toggle(): Promise<void> {
    if (dictation.state === 'processing') {
      log('Finalizing the current dictation before starting another session');
      return;
    }
    if (dictation.activeSessionId) {
      await dictation.stop();
      return;
    }
    if (voiceController.isPttHeld) {
      voiceController.toggleRecording();
      return;
    }

    const element = document.activeElement;
    if (isDictationElement(element)) {
      if (!dictation.canDictate) {
        log(
          !dictation.configured
            ? 'Configure the Google AI Studio API key in Settings > Voice Controller'
            : !dictation.available
              ? 'PipeWire audio utilities (pw-record) are not available'
              : 'Native dictation is disabled',
        );
        return;
      }
      const target = createDomTarget('field', element, { kind: 'field' });
      await dictation.start(target);
      return;
    }

    await voiceController.toggleRecording();
  }

  function cancel(): boolean {
    if (dictation.activeSessionId) {
      dictation.cancel().catch((err: unknown) => {
        log(`Cancel failed: ${String(err)}`);
      });
      return true;
    }
    if (voiceController.isPttHeld) {
      voiceController.cancelPushToTalk();
      return true;
    }
    return false;
  }

  return { toggle, cancel };
}
