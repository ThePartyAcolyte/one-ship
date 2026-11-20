import { defineStore } from 'pinia';
import { nextTick } from 'vue';
import { daemonCall } from '../utils/daemonClient';
import { listenDaemonEvent } from '../utils/daemonEvents';
import { reportAsyncFailure } from '../utils/reportAsyncFailure';
import { useCoreStore } from './coreStore';
import { useDialogueStore } from './dialogueStore';
import type { DictationTarget, DictationTargetMeta } from '../utils/dictationTargets';
import type { DictationStatus } from '../types/generated/DictationStatus';
import type { DictationState } from '../types/generated/DictationState';

let _dictationUnlisteners: Array<() => void> = [];

function logDictation(level: 'INFO' | 'WARN' | 'ERROR', message: string) {
  try {
    useCoreStore().addStatusLog(`[Dictation] ${message}`);
  } catch (err: unknown) {
    console.debug('[dictationStore] status log unavailable:', err);
  }
  daemonCall('write_client_log', { level, module: 'dictation', message }).catch((err: unknown) => {
    console.warn('[dictationStore] write_client_log failed:', err);
  });
}

export const useDictationStore = defineStore('dictation', {
  state: () => ({
    status: null as DictationStatus | null,
    state: 'idle' as DictationState,
    autoSend: true,
    activeSessionId: null as string | null,
    activeTarget: null as DictationTarget | null,
    activeTargetKey: null as string | null,
    interimText: '',
    lastError: null as string | null,
    recoveryFilePath: null as string | null,
    finalizedSessionId: null as string | null,
    listenersInitialized: false,
  }),

  getters: {
    enabled: (state): boolean => state.status?.enabled ?? false,
    available: (state): boolean => state.status?.available ?? false,
    configured: (state): boolean => state.status?.configured ?? false,
    keyringAvailable: (state): boolean => state.status?.keyringAvailable ?? false,
    canDictate: (state): boolean => {
      return (state.status?.enabled ?? false)
        && (state.status?.available ?? false)
        && (state.status?.configured ?? false);
    },
    isActive(state): boolean {
      return state.activeSessionId !== null
        && (state.state === 'connecting' || state.state === 'listening' || state.state === 'processing');
    },
    isActiveFor(state): (key: string) => boolean {
      return (key: string): boolean => state.activeSessionId !== null && state.activeTargetKey === key;
    },
  },

  actions: {
    async initListeners() {
      if (this.listenersInitialized) return;
      this.listenersInitialized = true;

      const unState = listenDaemonEvent('dictation-state', (event) => {
        const payload = event.payload;
        if (payload.sessionId && this.activeSessionId && payload.sessionId !== this.activeSessionId) {
          return;
        }
        this.state = payload.state;
        this.lastError = payload.lastError;
        this.recoveryFilePath = payload.recoveryFilePath;
        if (payload.state === 'error' && this.activeTarget) {
          if (this.interimText.trim().length === 0) {
            this.activeTarget.restore();
          }
          this.activeTarget = null;
          this.activeTargetKey = null;
          this.activeSessionId = null;
          this.interimText = '';
        }
        if (payload.state === 'idle' && !this.activeSessionId) {
          this.activeTarget = null;
          this.activeTargetKey = null;
        }
      });
      _dictationUnlisteners.push(unState);

      const unInterim = listenDaemonEvent('dictation-interim-text', (event) => {
        const payload = event.payload;
        if (!this.activeSessionId || payload.sessionId !== this.activeSessionId) return;
        if (!this.activeTarget) return;
        if (!this.activeTarget.apply(payload.text)) {
          logDictation('WARN', 'The target field changed during dictation; cancelling to preserve the user edit');
          const sessionId = this.activeSessionId;
          this.activeSessionId = null;
          this.activeTarget = null;
          this.activeTargetKey = null;
          this.interimText = '';
          this.state = 'idle';
          if (sessionId) {
            daemonCall('cancel_dictation', { sessionId }).catch((err: unknown) => {
              logDictation('WARN', `Cancel after target edit failed: ${String(err)}`);
            });
          }
          return;
        }
        this.interimText = payload.text;
      });
      _dictationUnlisteners.push(unInterim);

      const unFinished = listenDaemonEvent('dictation-finished', (event) => {
        const payload = event.payload;
        if (!this.activeSessionId || payload.sessionId !== this.activeSessionId) return;
        this.completeSession(payload.text, payload.recoveryFilePath).catch(reportAsyncFailure);
      });
      _dictationUnlisteners.push(unFinished);

      const unCancelled = listenDaemonEvent('dictation-cancelled', (event) => {
        const payload = event.payload;
        if (!this.activeSessionId || payload.sessionId !== this.activeSessionId) return;
        this.restoreActiveTarget();
        this.resetActive();
      });
      _dictationUnlisteners.push(unCancelled);

      const unResync = listenDaemonEvent('resync', () => {
        this.refreshStatus().catch(reportAsyncFailure);
      });
      _dictationUnlisteners.push(unResync);

      await this.refreshStatus();
    },

    async refreshStatus() {
      try {
        const status = await daemonCall('get_dictation_status', {});
        this.status = status;
        this.state = status.state;
        this.autoSend = status.autoSend;
        this.lastError = status.lastError;
        this.recoveryFilePath = status.recoveryFilePath;
      } catch (err: unknown) {
        this.lastError = String(err);
        console.warn('[dictationStore] refreshStatus failed:', err);
      }
    },

    async setAutoSend(enabled: boolean) {
      try {
        const status = await daemonCall('set_dictation_auto_send', { enabled });
        this.status = status;
        this.autoSend = status.autoSend;
        this.state = status.state;
      } catch (err: unknown) {
        this.lastError = String(err);
        logDictation('ERROR', `Could not change auto-send: ${String(err)}`);
      }
    },

    async start(target: DictationTarget) {
      if (this.isActive) return;
      if (!this.canDictate) {
        const message = !this.configured
          ? 'Configure the Google AI Studio API key in Settings > Voice Controller'
          : !this.available
            ? 'PipeWire audio utilities (pw-record) are not available'
            : 'Native dictation is disabled';
        this.lastError = message;
        logDictation('WARN', message);
        return;
      }

      try {
        const info = await daemonCall('start_dictation', {});
        this.activeSessionId = info.sessionId;
        this.activeTarget = target;
        this.activeTargetKey = target.key;
        this.interimText = '';
        this.lastError = null;
        this.recoveryFilePath = null;
        this.finalizedSessionId = null;
        this.state = 'connecting';
        target.focus();
      } catch (err: unknown) {
        this.lastError = String(err);
        this.activeTarget = null;
        this.activeTargetKey = null;
        this.activeSessionId = null;
        logDictation('ERROR', `Could not start dictation: ${String(err)}`);
      }
    },

    async startDom(
      key: string,
      element: HTMLInputElement | HTMLTextAreaElement,
      meta: DictationTargetMeta,
    ) {
      const { createDomTarget } = await import('../utils/dictationTargets');
      await this.start(createDomTarget(key, element, meta));
    },

    async stop() {
      const sessionId = this.activeSessionId;
      if (!sessionId) return;
      this.state = 'processing';
      try {
        const result = await daemonCall('stop_dictation', { sessionId });
        if (this.finalizedSessionId === sessionId) return;
        await this.completeSession(result.text, result.recoveryFilePath);
      } catch (err: unknown) {
        if (this.finalizedSessionId === sessionId) return;
        this.lastError = String(err);
        logDictation('ERROR', `Dictation finalization failed: ${String(err)}`);
        this.resetActive();
      }
    },

    async cancel() {
      const sessionId = this.activeSessionId;
      if (!sessionId) {
        this.restoreActiveTarget();
        this.resetActive();
        return;
      }
      this.restoreActiveTarget();
      this.resetActive();
      try {
        await daemonCall('cancel_dictation', { sessionId });
      } catch (err: unknown) {
        this.lastError = String(err);
        logDictation('WARN', `Cancel request failed: ${String(err)}`);
      }
    },

    async completeSession(text: string, recoveryFilePath: string | null) {
      const sessionId = this.activeSessionId;
      if (!sessionId || this.finalizedSessionId === sessionId) return;
      this.finalizedSessionId = sessionId;

      const target = this.activeTarget;
      this.recoveryFilePath = recoveryFilePath;
      if (target) {
        const finalText = text.trim();
        const applied = target.apply(finalText);
        if (!applied) {
          logDictation('WARN', 'The target field changed before the final transcript was applied');
        }
        const meta = target.meta;
        if (
          applied
          && this.autoSend
          && meta.kind === 'chat'
          && meta.agentId
          && finalText.length > 0
        ) {
          await nextTick();
          const dialogueStore = useDialogueStore();
          await dialogueStore.submitAgentPrompt(
            meta.agentId,
            undefined,
            meta.projectPath ?? undefined,
          );
          logDictation('INFO', `Auto-sent dictated prompt to ${meta.agentId}`);
        }
      }
      this.resetActive();
    },

    restoreActiveTarget() {
      if (this.activeTarget) {
        this.activeTarget.restore();
      }
    },

    resetActive() {
      this.state = 'idle';
      this.activeSessionId = null;
      this.activeTarget = null;
      this.activeTargetKey = null;
      this.interimText = '';
    },

    teardown() {
      if (this.activeSessionId) {
        daemonCall('cancel_dictation', { sessionId: this.activeSessionId }).catch((err: unknown) => {
          console.warn('[dictationStore] teardown cancel failed:', err);
        });
      }
      for (const unlisten of _dictationUnlisteners) {
        try {
          unlisten();
        } catch (err: unknown) {
          console.warn('[dictationStore] listener teardown failed:', err);
        }
      }
      _dictationUnlisteners = [];
      this.listenersInitialized = false;
      this.resetActive();
      this.lastError = null;
      this.recoveryFilePath = null;
    },
  },
});
