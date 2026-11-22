<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import {
  AudioLines,
  Mic,
  Settings,
  History,
  ServerCrash,
  Search,
  Menu,
  Info,
  ShieldCheck,
  AlertTriangle,
  Download,
  Minus,
  Square,
  Copy,
  X,
  Home,
} from '@lucide/vue';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { DiagnosticState } from '../types';
import { useCoreStore, type SystemTelemetryData } from '../stores/coreStore';
import { useDictationStore } from '../stores/dictationStore';
import {
  readVoiceWaveform,
  useVoiceControllerStore,
  WAVEFORM_SAMPLES_COUNT,
} from '../stores/voiceControllerStore';
import { daemonCall } from '../utils/daemonClient';
import { reportAsyncFailure } from '../utils/reportAsyncFailure';
import AppTooltip from './ui/AppTooltip.vue';
import TelemetryCharts from './telemetry/TelemetryCharts.vue';

const props = defineProps<{
  globalMetrics: SystemTelemetryData;
  diagnostics?: DiagnosticState | null;
  diagnosticsError?: string | null;
  isInstallingTools?: boolean;
  isDiagnosticsOk?: boolean;
}>();

const emit = defineEmits<{
  (e: 'toggleLanguage'): void;
  (e: 'installTools', toolIds: string[]): void;
  (e: 'openSettings'): void;
  (e: 'openSetup'): void;
  (e: 'openLogs'): void;
  (e: 'openAbout'): void;
  (e: 'openWelcome'): void;
  (e: 'openOmnisearch', initialQuery?: string): void;
}>();

const { t } = useI18n();
const coreStore = useCoreStore();
const voiceController = useVoiceControllerStore();
const dictation = useDictationStore();

const headerSearchQuery = ref('');
const isAppMenuOpen = ref(false);
const appMenuRef = ref<HTMLElement | null>(null);
const isTogglingVoice = ref(false);
const isWindowMaximized = ref(false);

const showWindowControls = computed(() => !(coreStore.hostCapabilities?.hyprland_available ?? false));

async function syncMaximizedState() {
  try {
    isWindowMaximized.value = await getCurrentWindow().isMaximized();
  } catch (err) {
    reportAsyncFailure('AppHeader.syncMaximizedState', err);
  }
}

function isInteractiveHeaderTarget(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  return Boolean(el?.closest('button, input, a, [role="button"], .ui-search-box'));
}

function handleHeaderMouseDown(e: MouseEvent) {
  if (!showWindowControls.value || e.button !== 0 || isInteractiveHeaderTarget(e.target)) return;
  getCurrentWindow().startDragging().catch(reportAsyncFailure);
}

function handleHeaderDoubleClick(e: MouseEvent) {
  if (!showWindowControls.value || isInteractiveHeaderTarget(e.target)) return;
  void toggleWindowMaximize();
}

function minimizeWindow() {
  getCurrentWindow().minimize().catch(reportAsyncFailure);
}

async function toggleWindowMaximize() {
  try {
    await getCurrentWindow().toggleMaximize();
    await syncMaximizedState();
  } catch (err) {
    reportAsyncFailure('AppHeader.toggleWindowMaximize', err);
  }
}

function closeWindow() {
  getCurrentWindow().close().catch(reportAsyncFailure);
}

function handleHeaderSearchSubmit() {
  const q = headerSearchQuery.value;
  headerSearchQuery.value = '';
  emit('openOmnisearch', q);
}

function handleHeaderSearchFocus() {
  emit('openOmnisearch', headerSearchQuery.value);
  headerSearchQuery.value = '';
}

function toggleAppMenu() {
  isAppMenuOpen.value = !isAppMenuOpen.value;
}

function closeAppMenu() {
  isAppMenuOpen.value = false;
}

function handleMenuAction(action: 'welcome' | 'logs' | 'settings' | 'setup' | 'about') {
  closeAppMenu();
  if (action === 'welcome') emit('openWelcome');
  else if (action === 'logs') emit('openLogs');
  else if (action === 'settings') emit('openSettings');
  else if (action === 'setup') emit('openSetup');
  else if (action === 'about') emit('openAbout');
}

async function toggleGlobalVoiceController(nextEnabled: boolean) {
  if (isTogglingVoice.value) return;
  isTogglingVoice.value = true;
  try {
    await daemonCall('set_voice_controller_enabled', { enabled: nextEnabled });
    await voiceController.refreshStatus();
  } catch (err) {
    reportAsyncFailure('AppHeader.toggleGlobalVoiceController', err);
  } finally {
    isTogglingVoice.value = false;
  }
}

async function handleVoicePillClick() {
  if (isVoiceExpanded.value) {
    await voiceController.refreshStatus();
    return;
  }
  await toggleGlobalVoiceController(!voiceController.enabled);
}

async function toggleDictationAutoSend() {
  await dictation.setAutoSend(!dictation.autoSend);
}

function handleDocumentClick(e: MouseEvent) {
  if (!isAppMenuOpen.value) return;
  const target = e.target as Node | null;
  if (appMenuRef.value && target && !appMenuRef.value.contains(target)) {
    closeAppMenu();
  }
}

function handleDocumentKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && isAppMenuOpen.value) {
    closeAppMenu();
  }
}

onMounted(() => {
  document.addEventListener('click', handleDocumentClick);
  document.addEventListener('keydown', handleDocumentKeydown);
  if (showWindowControls.value) {
    void syncMaximizedState();
  }
});

onUnmounted(() => {
  document.removeEventListener('click', handleDocumentClick);
  document.removeEventListener('keydown', handleDocumentKeydown);
  stopWaveformLoop();
});

const systemHealthSummary = computed(() => {
  if (daemonError.value) {
    return { ok: false, label: 'Daemon Offline' };
  }
  if (props.diagnosticsError) {
    return { ok: false, label: 'Diagnostics Error' };
  }
  if (!props.diagnostics?.git_installed) {
    return { ok: false, label: 'Missing Git' };
  }
  if (coreStore.installedRunners !== null && coreStore.installedRunners.length === 0) {
    return { ok: false, label: 'No Agent Runners' };
  }
  if (!props.isDiagnosticsOk) {
    return { ok: false, label: 'Setup Incomplete' };
  }
  return { ok: true, label: 'All Systems Nominal' };
});

const voiceTooltip = computed(() => {
  if (dictation.state === 'error' && dictation.lastError) {
    return `Dictation error: ${dictation.lastError}`;
  }
  if (dictation.activeSessionId) {
    if (dictation.state === 'processing') return 'Finalizing dictation...';
    return 'Dictating into the focused field (Alt+V to stop, Alt+Backspace to cancel)';
  }
  if (!voiceController.enabled) return 'Voice Controller: OFF (click to enable)';
  if (!voiceController.configured) return 'Voice Controller: Set Google AI Studio API Key in Settings';
  if (voiceController.lastError && voiceController.controllerState === 'error') {
    return `Voice Controller Error: ${voiceController.lastError}`;
  }
  switch (voiceController.controllerState) {
    case 'connecting':
      return 'Connecting to Gemini Live...';
    case 'listening':
      return 'Listening (Alt+V to send, Alt+Backspace to cancel)';
    case 'thinking':
      return 'Thinking...';
    case 'speaking':
      return 'Speaking (Alt+V to interrupt)';
    default:
      return 'Voice Controller: ON (click to disable · Alt+V to talk/dictate)';
  }
});

const isVoiceExpanded = computed(() => {
  return (
    dictation.activeSessionId !== null ||
    voiceController.isPttHeld ||
    voiceController.controllerState === 'connecting' ||
    voiceController.controllerState === 'listening' ||
    voiceController.controllerState === 'thinking' ||
    voiceController.controllerState === 'speaking'
  );
});

const voiceBadgeLabel = computed(() => {
  if (dictation.activeSessionId) {
    return dictation.state === 'processing' ? 'WAIT' : 'DICT';
  }
  if (voiceController.isPttHeld || voiceController.controllerState === 'listening') return 'REC';
  switch (voiceController.controllerState) {
    case 'connecting':
      return 'CONN';
    case 'thinking':
      return 'THINK';
    case 'speaking':
      return 'AI';
    case 'error':
      return 'ERR';
    default:
      return '';
  }
});

const waveformCanvasRef = ref<HTMLCanvasElement | null>(null);
const waveformBuffer = new Float32Array(WAVEFORM_SAMPLES_COUNT);
let rafId: number | null = null;

function stopWaveformLoop() {
  if (rafId !== null) {
    cancelAnimationFrame(rafId);
    rafId = null;
  }
}

function drawWaveformFrame() {
  const canvas = waveformCanvasRef.value;
  if (!canvas || !isVoiceExpanded.value) {
    rafId = null;
    return;
  }
  const ctx = canvas.getContext('2d');
  if (ctx) {
    const w = canvas.width;
    const h = canvas.height;
    const midY = h / 2;
    ctx.clearRect(0, 0, w, h);

    readVoiceWaveform(voiceController.controllerState, voiceController.isPttHeld, waveformBuffer);
    const styles = getComputedStyle(canvas);
    const strokeColor =
      voiceController.controllerState === 'speaking'
        ? styles.getPropertyValue('--status-ok-color').trim() || '#22c55e'
        : voiceController.isPttHeld || voiceController.controllerState === 'listening'
          ? styles.getPropertyValue('--accent-color').trim() || '#f59e0b'
          : styles.getPropertyValue('--status-warn-color').trim() || '#eab308';

    ctx.strokeStyle = strokeColor;
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    const len = waveformBuffer.length;
    for (let i = 0; i < len; i++) {
      const x = (i / (len - 1)) * w;
      const amp = Math.max(-1, Math.min(1, waveformBuffer[i] * 2.5));
      const y = midY - amp * (midY - 2);
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();
  }
  rafId = requestAnimationFrame(drawWaveformFrame);
}

watch(
  isVoiceExpanded,
  (expanded) => {
    if (expanded) {
      if (rafId === null) {
        rafId = requestAnimationFrame(drawWaveformFrame);
      }
    } else {
      stopWaveformLoop();
    }
  },
  { immediate: true }
);

const voiceBtnClass = computed(() => {
  if (!voiceController.enabled || !voiceController.configured) {
    return 'border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--accent-color)]';
  }
  switch (voiceController.controllerState) {
    case 'listening':
      return 'border-[var(--accent-color)] text-[var(--accent-color)] bg-[var(--accent-color)]/20 animate-pulse';
    case 'thinking':
    case 'connecting':
      return 'border-[var(--status-warn-border)] text-[var(--status-warn-color)] bg-[var(--status-warn-bg)]';
    case 'speaking':
      return 'border-[var(--status-ok-border)] text-[var(--status-ok-color)] bg-[var(--status-ok-bg)]';
    case 'error':
      return 'border-[var(--status-error-border)] text-[var(--status-error-color)] bg-[var(--status-error-bg)]';
    default:
      return 'border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)]';
  }
});

/** The MCP daemon is required; any state other than connected/connecting is an error. */
const daemonError = computed(() => {
  const status = coreStore.daemonStatus;
  return status.state === 'unreachable' || status.state === 'build_mismatch' ? status.detail : null;
});
</script>

<template>
  <header
    class="flex justify-between items-center border-b border-[var(--border-color)] pb-2.5 mb-2.5 text-xs uppercase shrink-0 relative gap-2 sm:gap-4"
    :class="showWindowControls ? 'select-none' : ''"
    @mousedown="handleHeaderMouseDown"
    @dblclick="handleHeaderDoubleClick"
  >
    
    <!-- ── LEFT: Brand & Primary View Switcher ─────────────────── -->
    <div class="flex items-center gap-2 sm:gap-3 shrink-0">
      <AppTooltip text="Toggle Language / Belta Dialect">
        <button 
          class="font-title text-base sm:text-lg tracking-wider cursor-pointer hover:text-[var(--accent-color)] transition-colors text-left flex items-center leading-none" 
          @click="emit('toggleLanguage')"
        >
          <span>ONE-SHIP</span>
        </button>
      </AppTooltip>
    </div>

    <!-- ── CENTER: Micro-Charts Telemetry Widgets ─────────────── -->
    <div class="flex items-center justify-center flex-1 min-w-0 px-1">
      <TelemetryCharts :metrics="globalMetrics" />
    </div>

    <!-- ── RIGHT: Integrated Omnisearch, Voice Pill & App Menu ─── -->
    <div class="flex items-center gap-1.5 justify-end shrink-0">
      <!-- Integrated Header Search Box -->
      <div
        class="ui-search-box h-6 px-2 w-44 sm:w-56 md:w-64 cursor-text normal-case"
        @click="handleHeaderSearchFocus"
      >
        <Search :size="12" class="ui-search-box__icon" />
        <input
          v-model="headerSearchQuery"
          type="text"
          placeholder="Search knowledge..."
          class="w-full bg-transparent border-none outline-none text-[10px] text-[var(--fg-color)] placeholder:text-[var(--dim-color)]/60 font-mono"
          @focus="handleHeaderSearchFocus"
          @keydown.enter.prevent="handleHeaderSearchSubmit"
        />
        <kbd class="ui-kbd">Ctrl+K</kbd>
      </div>

      <!-- Dictation Auto-send Toggle -->
      <AppTooltip :text="dictation.autoSend ? 'Auto-send dictated agent prompts: ON' : 'Auto-send dictated agent prompts: OFF'">
        <button
          type="button"
          @click="toggleDictationAutoSend"
          class="h-6 w-6 border transition-colors flex items-center justify-center cursor-pointer shrink-0 select-none"
          :class="dictation.autoSend
            ? 'border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)]'
            : 'border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--accent-color)]'"
        >
          <Mic :size="13" />
        </button>
      </AppTooltip>

      <!-- Global Voice Controller Toggle & Oscilloscope Waveform Pill (Spec 36, Alt+V) -->
      <AppTooltip :text="voiceTooltip">
        <button 
          type="button"
          :disabled="isTogglingVoice"
          @click="handleVoicePillClick"
          class="h-6 border transition-all duration-150 flex items-center justify-center gap-1.5 cursor-pointer shrink-0 select-none overflow-hidden disabled:opacity-50"
          :class="[voiceBtnClass, isVoiceExpanded ? 'w-36 px-1.5' : 'w-6']"
        >
          <AudioLines :size="13" class="shrink-0" />
          <span v-if="isVoiceExpanded && voiceBadgeLabel" class="text-[9px] font-bold tracking-wider shrink-0">
            {{ voiceBadgeLabel }}
          </span>
          <canvas
            v-show="isVoiceExpanded"
            ref="waveformCanvasRef"
            width="72"
            height="18"
            class="w-[72px] h-[18px] block shrink-0"
          />
        </button>
      </AppTooltip>

      <!-- MCP Daemon Error (required subsystem) -->
      <AppTooltip v-if="daemonError" :text="daemonError">
        <div class="h-6 w-6 border border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] flex items-center justify-center shrink-0 animate-pulse">
          <ServerCrash :size="13" />
        </div>
      </AppTooltip>

      <!-- Application Hamburger Menu -->
      <div ref="appMenuRef" class="relative">
        <AppTooltip text="Application Menu">
          <button
            type="button"
            @click.stop="toggleAppMenu"
            class="ui-icon-btn"
            :class="isAppMenuOpen ? '!border-[var(--accent-color)] !text-[var(--accent-color)]' : ''"
          >
            <Menu :size="13" />
          </button>
        </AppTooltip>

        <div
          v-if="isAppMenuOpen"
          class="absolute right-0 top-full mt-1.5 w-64 bg-[var(--ui-bg-title)] border border-[var(--border-color)] shadow-2xl z-50 font-mono text-[10px] normal-case select-none"
        >
          <!-- System Health Banner -->
          <div
            class="px-3 py-2 border-b border-[var(--border-color)] ui-surface-card flex items-center justify-between gap-2 cursor-pointer hover:border-[var(--accent-color)]"
            @click="handleMenuAction('setup')"
          >
            <div class="flex items-center gap-1.5 min-w-0">
              <ShieldCheck v-if="systemHealthSummary.ok" :size="12" class="text-[var(--status-ok-color)] shrink-0" />
              <AlertTriangle v-else :size="12" class="text-[var(--status-warn-color)] shrink-0" />
              <span
                class="truncate font-bold uppercase text-[9px] tracking-wider"
                :class="systemHealthSummary.ok ? 'text-[var(--status-ok-color)]' : 'text-[var(--status-warn-color)]'"
              >
                {{ systemHealthSummary.label }}
              </span>
            </div>
            <AppTooltip
              v-if="diagnostics && !diagnostics.git_installed && diagnostics.can_auto_install_tools"
              :text="`Install git (pkexec ${diagnostics.package_manager ?? 'package manager'})`"
              position="left"
            >
              <button
                type="button"
                @click.stop="emit('installTools', ['git'])"
                :disabled="isInstallingTools"
                class="text-[var(--dim-color)] hover:text-[var(--accent-color)] disabled:opacity-40 cursor-pointer"
              >
                <Download :size="11" />
              </button>
            </AppTooltip>
          </div>

          <!-- Navigation Items -->
          <div class="py-1 flex flex-col">
            <button
              type="button"
              @click="handleMenuAction('welcome')"
              class="w-full px-3 py-2 flex items-center gap-2.5 text-left text-[var(--fg-color)] hover:bg-[var(--accent-color)]/10 hover:text-[var(--accent-color)] transition-colors cursor-pointer"
            >
              <Home :size="12" class="text-[var(--accent-color)] shrink-0" />
              <span>Welcome Screen</span>
            </button>

            <button
              type="button"
              @click="handleMenuAction('logs')"
              class="w-full px-3 py-2 flex items-center gap-2.5 text-left text-[var(--fg-color)] hover:bg-[var(--accent-color)]/10 hover:text-[var(--accent-color)] transition-colors cursor-pointer"
            >
              <History :size="12" class="text-[var(--accent-color)] shrink-0" />
              <span>System Logs</span>
            </button>

            <button
              type="button"
              @click="handleMenuAction('setup')"
              class="w-full px-3 py-2 flex items-center gap-2.5 text-left text-[var(--fg-color)] hover:bg-[var(--accent-color)]/10 hover:text-[var(--accent-color)] transition-colors cursor-pointer"
            >
              <ShieldCheck :size="12" class="text-[var(--accent-color)] shrink-0" />
              <span>Setup &amp; Readiness</span>
            </button>

            <button
              type="button"
              @click="handleMenuAction('settings')"
              class="w-full px-3 py-2 flex items-center gap-2.5 text-left text-[var(--fg-color)] hover:bg-[var(--accent-color)]/10 hover:text-[var(--accent-color)] transition-colors cursor-pointer"
            >
              <Settings :size="12" class="text-[var(--accent-color)] shrink-0" />
              <span>{{ t('settings.global') }}</span>
            </button>

            <button
              type="button"
              @click="handleMenuAction('about')"
              class="w-full px-3 py-2 flex items-center gap-2.5 text-left text-[var(--fg-color)] hover:bg-[var(--accent-color)]/10 hover:text-[var(--accent-color)] transition-colors cursor-pointer border-t border-[var(--border-color)]/40 mt-0.5 pt-2"
            >
              <Info :size="12" class="text-[var(--accent-color)] shrink-0" />
              <span>About ONE-SHIP</span>
            </button>
          </div>
        </div>
      </div>

      <!-- Custom Frameless Window Controls (floating Linux desktops; hidden on Hyprland) -->
      <div v-if="showWindowControls" class="flex items-center gap-1 pl-1.5 ml-0.5 border-l border-[var(--border-color)]/60 shrink-0">
        <AppTooltip text="Minimize">
          <button type="button" @click="minimizeWindow" class="ui-icon-btn" aria-label="Minimize window">
            <Minus :size="12" />
          </button>
        </AppTooltip>
        <AppTooltip :text="isWindowMaximized ? 'Restore' : 'Maximize'">
          <button type="button" @click="toggleWindowMaximize" class="ui-icon-btn" aria-label="Toggle maximize window">
            <Copy v-if="isWindowMaximized" :size="11" />
            <Square v-else :size="11" />
          </button>
        </AppTooltip>
        <AppTooltip text="Close">
          <button
            type="button"
            @click="closeWindow"
            class="ui-icon-btn hover:!border-[var(--status-error-border)] hover:!text-[var(--status-error-color)]"
            aria-label="Close window"
          >
            <X :size="12" />
          </button>
        </AppTooltip>
      </div>
    </div>

  </header>
</template>
