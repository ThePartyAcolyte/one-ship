import { defineStore } from 'pinia';
import { daemonBaseUrl, daemonCall } from '../utils/daemonClient';
import { listenDaemonEvent } from '../utils/daemonEvents';
import { reportAsyncFailure } from '../utils/reportAsyncFailure';
import type { VoiceControllerState } from '../types/generated/VoiceControllerState';
import type { VoiceControllerStatus } from '../types/generated/VoiceControllerStatus';
import { useCoreStore } from './coreStore';

const VOICE_CONNECT_TIMEOUT_MS = 5000;
export const WAVEFORM_SAMPLES_COUNT = 48;

const _inputWaveformSamples = new Float32Array(WAVEFORM_SAMPLES_COUNT);
const _outputWaveformSamples = new Float32Array(WAVEFORM_SAMPLES_COUNT);

let _controllerUnlisteners: Array<() => void> = [];
let _ws: WebSocket | null = null;

function withTimeout<T>(promise: Promise<T>, timeoutMs: number, label: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = window.setTimeout(() => {
      reject(new Error(`${label} timed out after ${timeoutMs}ms`));
    }, timeoutMs);
    promise.then(
      (val) => {
        clearTimeout(timer);
        resolve(val);
      },
      (err) => {
        clearTimeout(timer);
        reject(err);
      }
    );
  });
}

function logVoiceClient(level: 'INFO' | 'WARN' | 'ERROR', message: string) {
  try {
    useCoreStore().addStatusLog(`[Voice] ${message}`);
  } catch {
    // Store may not be active during teardown.
  }
  daemonCall('write_client_log', { level, module: 'voice_controller', message }).catch((err) => {
    console.warn('[VoiceController] write_client_log failed:', err);
  });
}

/**
 * Populates `out` with normalized `[-1, 1]` oscilloscope samples for the active audio direction.
 */
export function readVoiceWaveform(
  mode: VoiceControllerState,
  isPttHeld: boolean,
  out: Float32Array
): void {
  const len = out.length;
  if (isPttHeld || mode === 'listening') {
    const srcLen = _inputWaveformSamples.length;
    for (let i = 0; i < len; i++) {
      const idx = Math.min(srcLen - 1, Math.floor((i * srcLen) / len));
      out[i] = _inputWaveformSamples[idx];
    }
    return;
  }
  if (mode === 'speaking') {
    const srcLen = _outputWaveformSamples.length;
    for (let i = 0; i < len; i++) {
      const idx = Math.min(srcLen - 1, Math.floor((i * srcLen) / len));
      out[i] = _outputWaveformSamples[idx];
    }
    return;
  }
  out.fill(0);
}

export const useVoiceControllerStore = defineStore('voiceController', {
  state: () => ({
    configured: false,
    enabled: true,
    controllerState: 'idle' as VoiceControllerState,
    thinkingLevel: 'low',
    lastError: null as string | null,
    isPttHeld: false,
    listenersInitialized: false,
  }),

  actions: {
    applyStatus(status: VoiceControllerStatus) {
      this.configured = status.configured;
      this.enabled = status.enabled;
      this.controllerState = status.state;
      this.thinkingLevel = status.thinkingLevel;
      this.lastError = status.lastError;
      if (status.state === 'idle' || status.state === 'error') {
        _outputWaveformSamples.fill(0);
      }
    },

    async refreshStatus() {
      try {
        const status = await daemonCall('get_voice_controller_status', {});
        this.applyStatus(status);
      } catch (err) {
        console.warn('[voiceControllerStore] refreshStatus failed:', err);
      }
    },

    async initListeners() {
      if (!this.listenersInitialized) {
        this.listenersInitialized = true;
        const unlisten = listenDaemonEvent('voice-controller-state', (event) => {
          this.applyStatus(event.payload);
        });
        _controllerUnlisteners.push(unlisten);
      }
      await this.refreshStatus();
    },

    async ensureConnected(): Promise<WebSocket> {
      if (_ws && _ws.readyState === WebSocket.OPEN) {
        return _ws;
      }
      if (_ws && _ws.readyState === WebSocket.CONNECTING) {
        return await withTimeout(
          new Promise<WebSocket>((resolve, reject) => {
            const ws = _ws!;
            const onOpen = () => {
              ws.removeEventListener('open', onOpen);
              ws.removeEventListener('error', onErr);
              resolve(ws);
            };
            const onErr = () => {
              ws.removeEventListener('open', onOpen);
              ws.removeEventListener('error', onErr);
              reject(new Error('Voice controller WebSocket connection failed'));
            };
            ws.addEventListener('open', onOpen);
            ws.addEventListener('error', onErr);
          }),
          VOICE_CONNECT_TIMEOUT_MS,
          'Voice WebSocket connect'
        );
      }

      const base = await daemonBaseUrl();
      const wsUrl = `${base.replace(/^http/, 'ws')}/api/v1/voice/stream`;
      const ws = new WebSocket(wsUrl);
      _ws = ws;

      ws.onmessage = (ev: MessageEvent<ArrayBuffer | string>) => {
        if (typeof ev.data === 'string') {
          try {
            const parsed = JSON.parse(ev.data) as { event?: string; samples?: number[] };
            if (parsed.event === 'interrupted') {
              _outputWaveformSamples.fill(0);
            } else if (parsed.event === 'input_waveform' && Array.isArray(parsed.samples)) {
              const count = Math.min(WAVEFORM_SAMPLES_COUNT, parsed.samples.length);
              for (let i = 0; i < count; i++) {
                _inputWaveformSamples[i] = parsed.samples[i] ?? 0;
              }
            } else if (parsed.event === 'output_waveform' && Array.isArray(parsed.samples)) {
              const count = Math.min(WAVEFORM_SAMPLES_COUNT, parsed.samples.length);
              for (let i = 0; i < count; i++) {
                _outputWaveformSamples[i] = parsed.samples[i] ?? 0;
              }
            }
          } catch {
            // Ignore malformed control frame.
          }
        }
      };

      ws.onclose = (ev: CloseEvent) => {
        if (_ws === ws) {
          _ws = null;
        }
        if (this.isPttHeld) {
          logVoiceClient('WARN', `Voice WebSocket closed while recording (code=${ev.code}, reason=${ev.reason || 'none'})`);
        }
        this.isPttHeld = false;
        _inputWaveformSamples.fill(0);
        _outputWaveformSamples.fill(0);
      };

      await withTimeout(
        new Promise<void>((resolve, reject) => {
          const onOpen = () => {
            ws.removeEventListener('open', onOpen);
            ws.removeEventListener('error', onErr);
            resolve();
          };
          const onErr = () => {
            ws.removeEventListener('open', onOpen);
            ws.removeEventListener('error', onErr);
            reject(new Error('Failed to open voice controller WebSocket'));
          };
          ws.addEventListener('open', onOpen);
          ws.addEventListener('error', onErr);
        }),
        VOICE_CONNECT_TIMEOUT_MS,
        'Voice WebSocket connect'
      );

      return ws;
    },

    async toggleRecording() {
      if (this.isPttHeld) {
        this.stopPushToTalk();
      } else {
        await this.startPushToTalk();
      }
    },

    async startPushToTalk() {
      if (this.isPttHeld) return;
      this.isPttHeld = true;
      this.lastError = null;
      _outputWaveformSamples.fill(0);

      try {
        await this.refreshStatus();
        if (!this.isPttHeld) return;

        if (!this.enabled || !this.configured) {
          const msg = !this.configured
            ? 'Configure Google AI Studio API Key in Settings -> Voice Controller'
            : 'Voice Controller is disabled';
          this.isPttHeld = false;
          this.lastError = msg;
          this.controllerState = 'error';
          logVoiceClient('WARN', msg);
          return;
        }

        const ws = await this.ensureConnected();
        if (this.isPttHeld && ws.readyState === WebSocket.OPEN) {
          ws.send(JSON.stringify({ action: 'ptt_start' }));
          logVoiceClient('INFO', 'Recording started via pw-record (Alt+V to send, Alt+Backspace/Esc to cancel)');
        }
      } catch (err: unknown) {
        if (!this.isPttHeld) {
          return;
        }
        this.isPttHeld = false;
        this.lastError = String(err);
        this.controllerState = 'error';
        logVoiceClient('ERROR', `startPushToTalk failed: ${this.lastError}`);
        reportAsyncFailure(err);
      }
    },

    stopPushToTalk() {
      if (!this.isPttHeld) return;
      this.isPttHeld = false;
      _inputWaveformSamples.fill(0);
      if (_ws && _ws.readyState === WebSocket.OPEN) {
        _ws.send(JSON.stringify({ action: 'ptt_stop' }));
        logVoiceClient('INFO', 'Recording sent to Gemini Live');
      }
    },

    cancelPushToTalk() {
      if (!this.isPttHeld) return;
      this.isPttHeld = false;
      _inputWaveformSamples.fill(0);
      _outputWaveformSamples.fill(0);
      if (_ws) {
        try {
          if (_ws.readyState === WebSocket.OPEN) {
            _ws.send(JSON.stringify({ action: 'ptt_cancel' }));
          }
          _ws.close();
        } catch {
          // Ignore close error.
        }
        _ws = null;
      }
      this.controllerState = 'idle';
      logVoiceClient('INFO', 'Recording cancelled');
    },

    teardown() {
      this.isPttHeld = false;
      _inputWaveformSamples.fill(0);
      _outputWaveformSamples.fill(0);

      if (_ws) {
        try {
          if (_ws.readyState === WebSocket.OPEN) {
            _ws.send(JSON.stringify({ action: 'disconnect' }));
          }
          _ws.close();
        } catch {
          // Ignore close error.
        }
        _ws = null;
      }

      for (const unlisten of _controllerUnlisteners) {
        try {
          unlisten();
        } catch (err) {
          console.warn('[voiceControllerStore] Listener teardown failed:', err);
        }
      }
      _controllerUnlisteners = [];
      this.listenersInitialized = false;
    },
  },
});
