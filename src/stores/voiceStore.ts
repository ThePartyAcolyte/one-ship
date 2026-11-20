import { defineStore } from 'pinia';
import { reportAsyncFailure } from '../utils/reportAsyncFailure';
import { daemonCall } from '../utils/daemonClient';
import { listenDaemonEvent } from '../utils/daemonEvents';
import type { OpenDictateStatus } from '../types/generated/OpenDictateStatus';
import { useDialogueStore } from './dialogueStore';
import { useWorkspaceLayoutStore } from './workspaceLayoutStore';
import { useCoreStore } from './coreStore';

interface PendingReservationParams {
  agentId: string;
  projectId?: number;
  projectName?: string;
  projectPath?: string;
  appName?: string;
}

let _voiceUnlisteners: Array<() => void> = [];

export const useVoiceStore = defineStore('voice', {
  state: () => ({
    isArmed: false,
    isRecording: false,
    isBusy: false,
    isWaitingForFree: false,
    isAutoSendEnabled: false,
    activeSessionId: null as string | null,
    targetAgentId: null as string | null,
    targetProjectPath: null as string | null,
    baseText: '',
    interimText: '',
    lastFinishedText: '',
    lastError: null as string | null,
    listenersInitialized: false,
    pendingParams: null as PendingReservationParams | null,
    pollTimerId: null as number | null,
  }),

  actions: {
    /**
     * Queries OpenDictate status to refresh isAutoSendEnabled state.
     */
    async refreshStatus() {
      try {
        const status = await daemonCall('get_opendictate_status', {});
        this.isAutoSendEnabled = status.autosend_enabled;
        return status;
      } catch (err) {
        console.warn('[voiceStore] refreshStatus failed:', err);
        return null;
      }
    },

    /**
     * Subscribes to the OpenDictate events the daemon's D-Bus monitor emits (`/api/events`).
     * Guaranteed to attach only once across the application lifecycle.
     */
    async initListeners() {
      if (this.listenersInitialized) return;
      this.listenersInitialized = true;
      this.refreshStatus().catch(reportAsyncFailure);

      const unStarted = listenDaemonEvent('opendictate_session_started', (event) => {
        if (!event.payload?.session_id || event.payload.session_id !== this.activeSessionId) return;
        this.isRecording = true;
        const dialogueStore = useDialogueStore();
        if (this.targetAgentId) {
          this.baseText = dialogueStore.getAgentDraft(this.targetAgentId, this.targetProjectPath);
        }
      });
      _voiceUnlisteners.push(unStarted);

      const unInterim = listenDaemonEvent('opendictate_interim_text', (event) => {
        if (!event.payload?.session_id || event.payload.session_id !== this.activeSessionId) return;
        this.interimText = event.payload.text;
        const dialogueStore = useDialogueStore();
        if (this.targetAgentId) {
          const prefix = this.baseText ? this.baseText.trimEnd() + ' ' : '';
          dialogueStore.setAgentDraft(this.targetAgentId, prefix + event.payload.text, this.targetProjectPath);
        }
      });
      _voiceUnlisteners.push(unInterim);

      const unFinished = listenDaemonEvent('opendictate_session_finished', async (event) => {
        const sid = event.payload?.session_id;
        if (sid && sid === this.activeSessionId) {
          const dialogueStore = useDialogueStore();
          const finalChunk = event.payload.text || event.payload.raw_text || '';
          const targetAgent = this.targetAgentId;
          const targetPath = this.targetProjectPath;
          const shouldAutoSend = event.payload?.auto_send ?? this.isAutoSendEnabled;

          if (targetAgent && finalChunk) {
            const prefix = this.baseText ? this.baseText.trimEnd() + ' ' : '';
            dialogueStore.setAgentDraft(targetAgent, prefix + finalChunk, targetPath);
          }

          this.lastFinishedText = finalChunk;
          this.isArmed = false;
          this.isRecording = false;
          this.isBusy = false;
          this.activeSessionId = null;
          this.targetAgentId = null;
          this.targetProjectPath = null;
          this.baseText = '';
          this.interimText = '';

          if (shouldAutoSend && targetAgent && finalChunk.trim().length > 0) {
            await dialogueStore.submitAgentPrompt(targetAgent, undefined, targetPath || undefined);
          }
        } else if (this.isWaitingForFree) {
          // An external session finished; attempt to claim the newly-freed turn
          this.tryClaimPendingReservation();
        }
      });
      _voiceUnlisteners.push(unFinished);

      const unCancelled = listenDaemonEvent('opendictate_session_cancelled', (event) => {
        const sid = event.payload?.session_id;
        if (sid && sid === this.activeSessionId) {
          const dialogueStore = useDialogueStore();
          // If capture was cancelled mid-recording, restore whatever was written before recording started
          if (this.isRecording && this.targetAgentId && this.baseText) {
            dialogueStore.setAgentDraft(this.targetAgentId, this.baseText, this.targetProjectPath);
          }
          this.isArmed = false;
          this.isRecording = false;
          this.isBusy = false;
          this.activeSessionId = null;
          this.targetAgentId = null;
          this.targetProjectPath = null;
          this.baseText = '';
          this.interimText = '';
        } else if (this.isWaitingForFree) {
          // An external session was cancelled; claim the slot
          this.tryClaimPendingReservation();
        }
      });
      _voiceUnlisteners.push(unCancelled);

      const unReleased = listenDaemonEvent('opendictate_session_released', (event) => {
        const sid = event.payload?.session_id;
        if (sid && sid === this.activeSessionId) {
          this.isArmed = false;
          this.isRecording = false;
          this.isBusy = false;
          this.activeSessionId = null;
          this.targetAgentId = null;
          this.targetProjectPath = null;
          this.baseText = '';
          this.interimText = '';
        } else if (this.isWaitingForFree) {
          // An external reservation was released; claim the slot
          this.tryClaimPendingReservation();
        }
      });
      _voiceUnlisteners.push(unReleased);

      const handleWindowBlur = () => {
        if (this.isArmed && !this.isRecording) {
          this.cancelReservation();
        }
        if (this.isWaitingForFree) {
          this.stopWaitingForFree();
        }
      };
      window.addEventListener('blur', handleWindowBlur);
      _voiceUnlisteners.push(() => {
        window.removeEventListener('blur', handleWindowBlur);
      });
    },

    /**
     * Computes the discrete visual state of the microphone indicator for an agent in a specific project.
     */
    getMicStatus(agentId?: string | null, projectPath?: string | null): 'recording' | 'reserved' | 'busy' | 'idle' {
      if (!agentId || !this.targetAgentId || this.targetAgentId !== agentId.toLowerCase()) {
        return 'idle';
      }
      if (agentId.toLowerCase() !== 'system' && this.targetProjectPath !== (projectPath || null)) {
        return 'idle';
      }
      if (this.isRecording) return 'recording';
      if (this.isArmed) return 'reserved';
      if (this.isBusy || this.isWaitingForFree) return 'busy';
      return 'idle';
    },

    /**
     * Stops the background poll and clears pending waiting state.
     */
    stopWaitingForFree() {
      if (this.pollTimerId !== null) {
        clearInterval(this.pollTimerId);
        this.pollTimerId = null;
      }
      this.isWaitingForFree = false;
      this.isBusy = false;
      this.pendingParams = null;
    },

    /**
     * Starts monitoring OpenDictate until it transitions from busy to idle.
     */
    startWaitingForFree(params: PendingReservationParams) {
      this.pendingParams = params;
      this.isWaitingForFree = true;
      this.isBusy = true;
      this.targetAgentId = params.agentId.toLowerCase();
      this.targetProjectPath = params.projectPath || null;

      if (this.pollTimerId === null) {
        this.pollTimerId = window.setInterval(async () => {
          if (!this.isWaitingForFree || !this.pendingParams) {
            this.stopWaitingForFree();
            return;
          }
          try {
            const status: OpenDictateStatus = await daemonCall('get_opendictate_status', {});
            if (status.is_available && !status.is_busy) {
              await this.tryClaimPendingReservation();
            }
          } catch (err) {
            console.warn('[voiceStore] OpenDictate poll failed:', err);
          }
        }, 1200);
      }
    },

    /**
     * Attempts to claim an OpenDictate reservation using stored pending parameters.
     */
    async tryClaimPendingReservation() {
      if (!this.pendingParams || !this.isWaitingForFree) return;
      const params = this.pendingParams;
      const normalizedAgentId = params.agentId.toLowerCase();
      const dialogueStore = useDialogueStore();
      const sessionId = `oneship-${normalizedAgentId}-${Date.now()}`;

      try {
        const res = await daemonCall('reserve_voice_capture', {
          sessionId,
          agentId: normalizedAgentId,
          projectId: params.projectId ?? null,
          projectName: params.projectName ?? null,
          appName: params.appName || `ONE-SHIP // ${params.agentId}`,
          accentColor: null,
          hideBubble: true,
        });

        if (res.success) {
          this.activeSessionId = sessionId;
          this.targetAgentId = normalizedAgentId;
          this.targetProjectPath = params.projectPath || null;
          this.isArmed = true;
          this.isRecording = false;
          this.isBusy = false;
          this.isWaitingForFree = false;
          this.interimText = '';
          this.baseText = dialogueStore.getAgentDraft(normalizedAgentId, this.targetProjectPath);
          this.lastError = null;
          if (this.pollTimerId !== null) {
            clearInterval(this.pollTimerId);
            this.pollTimerId = null;
          }
        } else if (!res.service_available) {
          this.stopWaitingForFree();
          const coreStore = useCoreStore();
          if (coreStore.hostCapabilities) {
            coreStore.hostCapabilities.opendictate_available = false;
          }
        }
      } catch (err: unknown) {
        this.lastError = String(err);
        this.stopWaitingForFree();
      }
    },

    /**
     * Cancels the active reservation if armed and speech recording has not started.
     * Prevents locking OpenDictate when focus moves away without speaking.
     */
    cancelIfIdle() {
      if (this.isWaitingForFree) {
        this.stopWaitingForFree();
      }
      if (this.isArmed && !this.isRecording) {
        this.cancelReservation();
      }
    },

    /**
     * Arms an OpenDictate speech capture session targeted to a specific agent's prompt input.
     * Does nothing if already armed or recording for the exact same (projectPath, agentId).
     */
    async armReservationForAgent(
      agentId: string,
      projectId?: number,
      projectName?: string,
      appName?: string,
      projectPath?: string
    ) {
      if (!agentId) return;
      const coreStore = useCoreStore();
      if (coreStore.hostCapabilities && !coreStore.hostCapabilities.opendictate_available) {
        return;
      }
      await this.initListeners();
      this.refreshStatus().catch(reportAsyncFailure);

      const dialogueStore = useDialogueStore();
      const layoutStore = useWorkspaceLayoutStore();
      const normalizedAgentId = agentId.toLowerCase();
      const resolvedProjectPath =
        projectPath ||
        dialogueStore.activeProjectPath ||
        layoutStore.floatingChatProjectPath ||
        dialogueStore.resolveAgentProjectPath(agentId) ||
        null;

      // Avoid duplicate D-Bus calls if already armed for the exact same (projectPath, agentId)
      if (
        this.isArmed &&
        this.targetAgentId === normalizedAgentId &&
        this.targetProjectPath === resolvedProjectPath &&
        this.activeSessionId
      ) {
        return;
      }
      if (
        this.isRecording &&
        this.targetAgentId === normalizedAgentId &&
        this.targetProjectPath === resolvedProjectPath
      ) {
        return;
      }

      const sessionId = `oneship-${normalizedAgentId}-${Date.now()}`;

      this.activeSessionId = sessionId;
      this.targetAgentId = normalizedAgentId;
      this.targetProjectPath = resolvedProjectPath;
      this.isArmed = true;
      this.isRecording = false;
      this.isBusy = false;
      this.interimText = '';
      this.baseText = dialogueStore.getAgentDraft(normalizedAgentId, resolvedProjectPath);
      this.lastError = null;

      try {
        const res = await daemonCall('reserve_voice_capture', {
          sessionId,
          agentId: normalizedAgentId,
          projectId: projectId ?? null,
          projectName: projectName ?? null,
          appName: appName || `ONE-SHIP // ${agentId}`,
          accentColor: null,
          hideBubble: true,
        });

        if (!res.success) {
          this.isArmed = false;
          this.activeSessionId = null;
          this.lastError = res.message;
          if (res.service_available) {
            // OpenDictate is running but busy recording another session.
            this.startWaitingForFree({
              agentId,
              projectId,
              projectName,
              projectPath: resolvedProjectPath || undefined,
              appName,
            });
          } else {
            this.isBusy = false;
            if (coreStore.hostCapabilities) {
              coreStore.hostCapabilities.opendictate_available = false;
            }
          }
        } else {
          this.isBusy = false;
          this.isWaitingForFree = false;
          if (this.pollTimerId !== null) {
            clearInterval(this.pollTimerId);
            this.pollTimerId = null;
          }
        }
      } catch (err: unknown) {
        this.isArmed = false;
        this.isBusy = false;
        this.activeSessionId = null;
        this.lastError = String(err);
      }
    },

    /**
     * Explicitly cancels the active reservation if armed.
     */
    async cancelReservation() {
      this.stopWaitingForFree();

      if (!this.activeSessionId) {
        this.isArmed = false;
        this.isRecording = false;
        this.isBusy = false;
        this.targetAgentId = null;
        this.targetProjectPath = null;
        return;
      }

      const sid = this.activeSessionId;
      this.isArmed = false;
      this.isRecording = false;
      this.isBusy = false;
      this.activeSessionId = null;
      this.targetAgentId = null;
      this.targetProjectPath = null;
      this.baseText = '';
      this.interimText = '';

      try {
        await daemonCall('cancel_voice_capture', { sessionId: sid });
      } catch (err) {
        console.warn('[voiceStore] cancel_voice_capture failed:', err);
      }
    },

    /**
     * Cleanly tears down all native event listeners.
     */
    teardown() {
      this.stopWaitingForFree();
      for (const unlisten of _voiceUnlisteners) {
        try {
          unlisten();
        } catch (err) {
          console.warn('[voiceStore] Listener teardown failed:', err);
        }
      }
      _voiceUnlisteners = [];
      this.listenersInitialized = false;
      this.isArmed = false;
      this.isRecording = false;
      this.isBusy = false;
      this.activeSessionId = null;
      this.targetAgentId = null;
      this.targetProjectPath = null;
    },
  },
});
