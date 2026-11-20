<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import {
  Cpu,
  Hammer,
  Upload,
  RotateCw,
  Trash2,
  Pause,
  Play,
  Send,
  CheckCircle2,
  AlertCircle,
  Terminal,
  ChevronDown,
  ChevronUp,
  Loader2,
  Activity,
  Binary,
  Download,
  Wrench,
  Sparkles,
  X,
  Check,
  Copy,
  AlertTriangle,
} from '@lucide/vue';
import { daemonCall } from '../../utils/daemonClient';
import { useUiSurfaceNode } from '../../composables/useUiSurfaceNode';
import { AppTooltip, AppSelect, AppInput, AppSegmentedToggle, SubtabBar, type SegmentedItem, type SubtabItem } from '../ui';
import OscilloscopeView from './electronics/OscilloscopeView.vue';
import LogicAnalyzerView from './electronics/LogicAnalyzerView.vue';
import type {
  ProjectRecord,
  ProjectElectronicsStatus,
  DiscoveredMcuPort,
  FirmwareOpResult,
  DiscoveredInstrument,
  PlatformioBoard,
} from '../../types';

const props = defineProps<{
  project: ProjectRecord;
}>();

type ElectronicsSubTab = 'firmware' | 'oscilloscope' | 'logic';
const electronicsSurface = useUiSurfaceNode<ElectronicsSubTab>('electronics', 'firmware');
const activeSubTab = computed({
  get: () => electronicsSurface.activeChildId.value,
  set: (val) => electronicsSurface.selectChild(val),
});
const connectedInstruments = ref<DiscoveredInstrument[]>([]);

const isLoading = ref<boolean>(true);
const status = ref<ProjectElectronicsStatus | null>(null);

const selectedMcuPort = ref<string>('');
const selectedBaudRate = ref<string>('115200');
const selectedEnvironment = ref<string>('');

const isBuilding = ref<boolean>(false);
const isFlashing = ref<boolean>(false);
const isInstallingTool = ref<boolean>(false);
const lastOpResult = ref<FirmwareOpResult | null>(null);
const lastOpError = ref<string | null>(null);
const showBuildLogs = ref<boolean>(false);

// Toolchain & Configuration Status
const isPioCliInstalled = computed<boolean>(() => {
  return status.value?.toolchains.some((t) => t.id === 'platformio' && t.isInstalled) ?? false;
});

const hasPlatformioConfig = computed<boolean>(() => {
  return status.value?.hasPlatformio ?? false;
});

const installedToolsCount = computed<number>(() => {
  return status.value?.toolchains.filter((t) => t.isInstalled).length ?? 0;
});

const totalToolsCount = computed<number>(() => {
  return status.value?.toolchains.length ?? 0;
});

const allToolsInstalled = computed<boolean>(() => {
  return installedToolsCount.value === totalToolsCount.value && totalToolsCount.value > 0;
});

const showToolchainsModal = ref<boolean>(false);
const installingToolId = ref<string | null>(null);

// Scaffolding State
const isScaffolding = ref<boolean>(false);
const boardSearchQuery = ref<string>('');
const boardsList = ref<PlatformioBoard[]>([]);
const isSearchingBoards = ref<boolean>(false);
const selectedBoard = ref<PlatformioBoard | null>(null);
const boardManuallySelected = ref<boolean>(false);
const detectedBoard = ref<PlatformioBoard | null>(null);
const selectedFramework = ref<string>('arduino');
const selectedMonitorSpeed = ref<string>('115200');
const firmwareLanguage = ref<'cpp' | 'c' | 'micropython'>('cpp');
const copiedToolId = ref<string | null>(null);

const languageItems: SegmentedItem[] = [
  { id: 'cpp', label: 'C++' },
  { id: 'c', label: 'C' },
  { id: 'micropython', label: 'Python' },
];

const isMicroPython = computed<boolean>(() => firmwareLanguage.value === 'micropython');

const hasFirmwareProject = computed<boolean>(() => {
  return Boolean(status.value?.firmwareLanguage) || hasPlatformioConfig.value;
});

const isMpremoteInstalled = computed<boolean>(() => {
  return status.value?.toolchains.some((t) => t.id === 'mpremote' && t.isInstalled) ?? false;
});

const autoInstallableToolIds = computed<string[]>(() => {
  return (status.value?.toolchains ?? [])
    .filter((t) => !t.isInstalled && t.autoInstallable)
    .map((t) => t.id);
});

const detectedBoardId = computed<string | null>(() => {
  const port = currentPortDetails.value;
  if (!port) return null;
  const haystack = `${port.productName ?? ''} ${port.description ?? ''} ${port.manufacturer ?? ''}`.toLowerCase();
  if (haystack.includes('nodemcu') || haystack.includes('esp8266')) return 'nodemcuv2';
  if (haystack.includes('esp32')) return 'esp32dev';
  if (haystack.includes('pico') || haystack.includes('rp2040')) return 'pico';
  if (haystack.includes('nano')) return 'nanoatmega328';
  if (haystack.includes('uno')) return 'uno';
  if (haystack.includes('bluepill') || haystack.includes('stm32')) return 'bluepill_f103c8';
  const vid = (port.vendorId ?? '').toLowerCase();
  if (vid === '10c4' || vid === '1a86') return 'nodemcuv2';
  if (vid === '2341' || vid === '2a03') return 'uno';
  if (vid === '2e8a') return 'pico';
  return null;
});

const frameworkOptions = computed(() => {
  const fw = selectedBoard.value?.frameworks ?? [];
  if (fw.length === 0) {
    return [{ label: 'ARDUINO', value: 'arduino' }];
  }
  return fw.map((f: string) => ({
    label: f.toUpperCase(),
    value: f,
  }));
});

// Serial Monitor reactive state
interface SerialEntry {
  id: number;
  text: string;
  type: 'in' | 'out' | 'sys';
  timestamp: string;
}

let serialEntrySequence = 0;
const serialLines = ref<SerialEntry[]>([]);
const serialInput = ref<string>('');
const isSerialPaused = ref<boolean>(false);
const isAutoScroll = ref<boolean>(true);
const terminalContainerRef = ref<HTMLDivElement | null>(null);

const baudRateOptions = [
  { label: '9600 baud', value: '9600' },
  { label: '19200 baud', value: '19200' },
  { label: '38400 baud', value: '38400' },
  { label: '57600 baud', value: '57600' },
  { label: '115200 baud', value: '115200' },
  { label: '230400 baud', value: '230400' },
  { label: '460800 baud', value: '460800' },
  { label: '921600 baud', value: '921600' },
];

const portOptions = computed(() => {
  const mcus = status.value?.connectedMcus ?? [];
  if (mcus.length === 0) {
    return [{ label: 'No hardware devices detected', value: '' }];
  }
  return mcus.map((m: DiscoveredMcuPort) => ({
    label: m.description,
    value: m.portName,
  }));
});

const environmentOptions = computed(() => {
  const envs = status.value?.environments ?? [];
  if (envs.length === 0) {
    return [{ label: 'Default Environment', value: '' }];
  }
  return envs.map((env: string) => ({
    label: `[env:${env}]`,
    value: env,
  }));
});

const currentPortDetails = computed<DiscoveredMcuPort | null>(() => {
  if (!selectedMcuPort.value || !status.value?.connectedMcus) return null;
  return status.value.connectedMcus.find((m: DiscoveredMcuPort) => m.portName === selectedMcuPort.value) ?? null;
});

async function loadStatus() {
  isLoading.value = true;
  lastOpError.value = null;
  try {
    const res = await daemonCall('get_project_electronics_status', {
      projectPath: props.project.path,
    });
    status.value = res;

    if (res.firmwareLanguage === 'micropython') {
      firmwareLanguage.value = 'micropython';
    } else if (res.firmwareLanguage === 'platformio' && firmwareLanguage.value === 'micropython') {
      firmwareLanguage.value = 'cpp';
    }

    // Auto-select first detected port if not set
    if (!selectedMcuPort.value && res.connectedMcus.length > 0) {
      selectedMcuPort.value = res.connectedMcus[0].portName;
    }
    // Auto-select first environment if not set
    if (!selectedEnvironment.value && res.environments.length > 0) {
      selectedEnvironment.value = res.environments[0];
    }

    try {
      connectedInstruments.value = await daemonCall('scan_connected_instruments', {});
    } catch {
      // Ignore background instrument scan failure
    }
  } catch (err: unknown) {
    console.error('Failed to load electronics status:', err);
    lastOpError.value = err instanceof Error ? err.message : String(err);
  } finally {
    isLoading.value = false;
  }
}

async function refreshDevices() {
  try {
    const mcus = await daemonCall('list_connected_mcus', {});
    if (status.value) {
      status.value.connectedMcus = mcus;
    }
    if (!selectedMcuPort.value && mcus.length > 0) {
      selectedMcuPort.value = mcus[0].portName;
    }

    try {
      connectedInstruments.value = await daemonCall('scan_connected_instruments', {});
    } catch {
      // Ignore instrument scan error
    }

    appendSerialMessage(`Hardware scan: ${mcus.length} MCU(s), ${connectedInstruments.value.length} instrument(s) found`, 'sys');
  } catch (err: unknown) {
    appendSerialMessage(`Scan failed: ${err instanceof Error ? err.message : String(err)}`, 'sys');
  }
}

async function handleBuildFirmware() {
  if (isBuilding.value || isFlashing.value) return;
  if (isMicroPython.value) {
    lastOpError.value = 'MicroPython projects have no compile step. Use Upload to copy main.py to the board.';
    appendSerialMessage(lastOpError.value, 'sys');
    return;
  }
  isBuilding.value = true;
  lastOpError.value = null;
  lastOpResult.value = null;
  appendSerialMessage(`Building firmware for env '${selectedEnvironment.value || 'default'}'...`, 'sys');

  try {
    const result = await daemonCall('build_project_firmware', {
      projectPath: props.project.path,
      environment: selectedEnvironment.value || null,
    });
    lastOpResult.value = result;
    appendSerialMessage(`Build successful in ${(Number(result.durationMs) / 1000).toFixed(2)}s`, 'sys');
  } catch (err: unknown) {
    const errMsg = err instanceof Error ? err.message : String(err);
    lastOpError.value = errMsg;
    appendSerialMessage(`Build error: ${errMsg}`, 'sys');
  } finally {
    isBuilding.value = false;
  }
}

async function handleFlashFirmware() {
  if (isBuilding.value || isFlashing.value) return;
  isFlashing.value = true;
  lastOpError.value = null;
  lastOpResult.value = null;
  appendSerialMessage(
    isMicroPython.value
      ? `Uploading main.py to port '${selectedMcuPort.value}'...`
      : `Uploading firmware to port '${selectedMcuPort.value || 'auto'}' (env: '${selectedEnvironment.value || 'default'}')...`,
    'sys'
  );

  try {
    const result = await daemonCall('flash_project_firmware', {
      projectPath: props.project.path,
      environment: selectedEnvironment.value || null,
      uploadPort: selectedMcuPort.value || null,
    });
    lastOpResult.value = result;
    appendSerialMessage(`Upload successful in ${(Number(result.durationMs) / 1000).toFixed(2)}s`, 'sys');
  } catch (err: unknown) {
    const errMsg = err instanceof Error ? err.message : String(err);
    lastOpError.value = errMsg;
    appendSerialMessage(`Upload failed: ${errMsg}`, 'sys');
  } finally {
    isFlashing.value = false;
  }
}

async function handleInstallPlatformio() {
  if (isInstallingTool.value) return;
  isInstallingTool.value = true;
  lastOpError.value = null;
  appendSerialMessage('Requesting PlatformIO installation via system package manager...', 'sys');
  try {
    await daemonCall('install_missing_tools', {
      toolIds: ['platformio'],
    });
    appendSerialMessage('PlatformIO installation completed.', 'sys');
    await loadStatus();
  } catch (err: unknown) {
    const errMsg = err instanceof Error ? err.message : String(err);
    lastOpError.value = errMsg;
    appendSerialMessage(`Installation failed: ${errMsg}`, 'sys');
  } finally {
    isInstallingTool.value = false;
  }
}

const MAX_SERIAL_LINES_COUNT = 10_000;

function appendSerialMessage(text: string, type: 'in' | 'out' | 'sys') {
  if (isSerialPaused.value && type === 'in') return;
  const now = new Date().toTimeString().slice(0, 8);
  serialLines.value.push({
    id: ++serialEntrySequence,
    text,
    type,
    timestamp: now,
  });
  if (serialLines.value.length > MAX_SERIAL_LINES_COUNT) {
    serialLines.value.splice(0, serialLines.value.length - MAX_SERIAL_LINES_COUNT);
  }

  if (isAutoScroll.value) {
    requestAnimationFrame(() => {
      if (terminalContainerRef.value) {
        terminalContainerRef.value.scrollTop = terminalContainerRef.value.scrollHeight;
      }
    });
  }
}

function handleSendSerial() {
  const line = serialInput.value.trim();
  if (!line) return;
  appendSerialMessage(line, 'out');
  serialInput.value = '';
}

function clearSerialLogs() {
  serialLines.value = [];
}

async function fetchBoards(query?: string) {
  isSearchingBoards.value = true;
  try {
    const list = await daemonCall('list_platformio_boards', { query: query || null });
    boardsList.value = list;
  } catch (err: unknown) {
    console.error('Failed to load boards:', err);
  } finally {
    isSearchingBoards.value = false;
  }
}

function applyBoard(board: PlatformioBoard, manual: boolean) {
  selectedBoard.value = board;
  if (manual) boardManuallySelected.value = true;
  if (board.frameworks.length > 0) {
    selectedFramework.value = board.frameworks.includes('arduino') ? 'arduino' : board.frameworks[0];
  } else {
    selectedFramework.value = 'arduino';
  }
}

function selectBoard(board: PlatformioBoard) {
  applyBoard(board, true);
}

async function loadDetectedBoard(): Promise<PlatformioBoard | null> {
  const id = detectedBoardId.value;
  if (!id) {
    detectedBoard.value = null;
    return null;
  }
  try {
    const list = await daemonCall('list_platformio_boards', { query: id });
    detectedBoard.value = list.find((b) => b.id === id) ?? null;
    return detectedBoard.value;
  } catch (err: unknown) {
    console.error('Failed to resolve detected board:', err);
    return null;
  }
}

async function applyDetectedBoard() {
  if (!detectedBoard.value) return;
  applyBoard(detectedBoard.value, false);
}

watch(selectedMcuPort, async () => {
  boardManuallySelected.value = false;
  if (hasFirmwareProject.value) return;
  const board = await loadDetectedBoard();
  if (board && !boardManuallySelected.value) {
    applyBoard(board, false);
  }
});

let searchDebounce: number | null = null;
function handleSearchBoardsInput(e: Event) {
  const q = (e.target as HTMLInputElement).value;
  boardSearchQuery.value = q;
  if (searchDebounce) clearTimeout(searchDebounce);
  searchDebounce = window.setTimeout(() => {
    fetchBoards(q);
  }, 250);
}

async function handleScaffoldFirmware() {
  if (isScaffolding.value || !selectedBoard.value) return;
  isScaffolding.value = true;
  lastOpError.value = null;
  appendSerialMessage(`Scaffolding firmware for board '${selectedBoard.value.id}' (${selectedFramework.value})...`, 'sys');

  try {
    const res = await daemonCall('scaffold_firmware_project', {
      projectPath: props.project.path,
      boardId: selectedBoard.value.id,
      platform: selectedBoard.value.platform,
      framework: isMicroPython.value ? 'micropython' : selectedFramework.value,
      monitorSpeed: parseInt(selectedMonitorSpeed.value, 10) || 115200,
      language: firmwareLanguage.value,
    });

    appendSerialMessage(
      `Firmware scaffolded successfully (${res.createdFiles.join(', ')}). Environment: [env:${res.environment}]`,
      'sys'
    );
    await loadStatus();
  } catch (err: unknown) {
    const errMsg = err instanceof Error ? err.message : String(err);
    lastOpError.value = errMsg;
    appendSerialMessage(`Scaffolding failed: ${errMsg}`, 'sys');
  } finally {
    isScaffolding.value = false;
  }
}

async function copyToolHint(tool: { id: string; installHint: string }) {
  try {
    await navigator.clipboard.writeText(tool.installHint);
    copiedToolId.value = tool.id;
    setTimeout(() => {
      if (copiedToolId.value === tool.id) copiedToolId.value = null;
    }, 2000);
  } catch (err) {
    console.error('Failed to copy install hint:', err);
  }
}

async function handleInstallSingleTool(toolId: string) {
  if (installingToolId.value) return;
  installingToolId.value = toolId;
  appendSerialMessage(`Requesting package installation for '${toolId}'...`, 'sys');
  try {
    await daemonCall('install_missing_tools', {
      toolIds: [toolId],
    });
    appendSerialMessage(`Successfully installed '${toolId}'.`, 'sys');
    await loadStatus();
  } catch (err: unknown) {
    const errMsg = err instanceof Error ? err.message : String(err);
    appendSerialMessage(`Installation of '${toolId}' failed: ${errMsg}`, 'sys');
  } finally {
    installingToolId.value = null;
  }
}

async function handleInstallAllMissingTools() {
  if (installingToolId.value || !status.value) return;
  const missing = autoInstallableToolIds.value;
  if (missing.length === 0) return;

  installingToolId.value = 'all';
  appendSerialMessage(`Requesting installation for missing tools: ${missing.join(', ')}...`, 'sys');
  try {
    await daemonCall('install_missing_tools', {
      toolIds: missing,
    });
    appendSerialMessage(`Installation of missing tools completed.`, 'sys');
    await loadStatus();
  } catch (err: unknown) {
    const errMsg = err instanceof Error ? err.message : String(err);
    appendSerialMessage(`Installation failed: ${errMsg}`, 'sys');
  } finally {
    installingToolId.value = null;
  }
}

onMounted(() => {
  loadStatus();
  fetchBoards();
  appendSerialMessage('Serial monitor ready. Select port and baud rate to communicate.', 'sys');
});

watch(() => props.project.path, () => {
  loadStatus();
});

onUnmounted(() => {
  if (searchDebounce) {
    clearTimeout(searchDebounce);
    searchDebounce = null;
  }
});
</script>

<template>
  <div class="h-full flex flex-col min-w-0 bg-[var(--bg-color)] overflow-hidden">
    <!-- Sub-navigation Bar -->
    <SubtabBar
      :model-value="activeSubTab"
      @update:model-value="activeSubTab = $event as 'firmware' | 'oscilloscope' | 'logic'"
      :items="[
        { id: 'firmware', label: 'Firmware', icon: Hammer },
        { id: 'oscilloscope', label: 'Oscilloscope', icon: Activity },
        { id: 'logic', label: 'Logic Analyzer', icon: Binary },
      ] satisfies SubtabItem[]"
    >
      <template #right>
        <div class="flex items-center gap-2 text-[11px] font-mono">
          <AppTooltip :text="`Toolchains: ${installedToolsCount}/${totalToolsCount} installed. Click to configure.`">
            <button
              type="button"
              @click="showToolchainsModal = true"
              class="ui-icon-btn"
              :class="allToolsInstalled
                ? '!border-[var(--status-ok-border)] !text-[var(--status-ok-color)]'
                : '!border-[var(--status-warn-border)] !text-[var(--status-warn-color)]'"
            >
              <Wrench :size="12" />
            </button>
          </AppTooltip>
        </div>
      </template>
    </SubtabBar>

    <!-- Sub-tab 1: Firmware Toolchain & Serial Monitor -->
    <div v-if="activeSubTab === 'firmware'" class="flex-1 p-4 flex flex-col lg:flex-row gap-4 min-h-0 overflow-y-auto">
      <!-- Left Column: Hardware Interface & Firmware Controls -->
      <div class="w-full lg:w-[380px] shrink-0 flex flex-col gap-4">
        <!-- Hardware Card -->
        <div class="border border-[var(--border-color)] ui-surface-card p-3.5 flex flex-col gap-3">
          <div class="flex items-center justify-between border-b border-[var(--border-color)] pb-2">
            <span class="font-bold text-[10px] uppercase tracking-wider text-[var(--fg-color)] flex items-center gap-1.5">
              <Cpu class="w-3.5 h-3.5 text-[var(--accent-color)]" />
              <span>Hardware</span>
            </span>
            <div class="flex items-center gap-1.5">
              <span class="text-[10px] font-mono text-[var(--dim-color)]">
                {{ status?.connectedMcus?.length ?? 0 }} Connected
              </span>
              <AppTooltip text="Rescan serial devices">
                <button
                  @click="refreshDevices"
                  :disabled="isLoading"
                  class="p-0.5 text-[var(--dim-color)] hover:text-[var(--fg-color)] transition-colors cursor-pointer disabled:opacity-50"
                >
                  <RotateCw class="w-3 h-3" :class="{ 'animate-spin': isLoading }" />
                </button>
              </AppTooltip>
            </div>
          </div>

          <div class="flex flex-col gap-2">
            <label class="text-[9px] font-bold uppercase tracking-wider text-[var(--dim-color)]">Serial Port</label>
            <AppSelect
              v-model="selectedMcuPort"
              :options="portOptions"
              compact
              class="[&>button]:h-6"
            />
          </div>

          <div class="flex flex-col gap-2">
            <label class="text-[9px] font-bold uppercase tracking-wider text-[var(--dim-color)]">Baud Rate</label>
            <AppSelect
              v-model="selectedBaudRate"
              :options="baudRateOptions"
              compact
              class="[&>button]:h-6"
            />
          </div>

          <!-- Port Metadata Breakdown -->
          <div v-if="currentPortDetails" class="bg-[var(--ui-bg-card)] p-2.5 border border-[var(--border-color)] text-[11px] font-mono flex flex-col gap-1 text-[var(--dim-color)]">
            <div class="flex justify-between">
              <span>Port:</span>
              <span class="text-[var(--fg-color)] font-bold">{{ currentPortDetails.portName }}</span>
            </div>
            <div v-if="currentPortDetails.vendorId" class="flex justify-between">
              <span>VID:PID:</span>
              <span class="text-[var(--fg-color)]">{{ currentPortDetails.vendorId }}:{{ currentPortDetails.productId }}</span>
            </div>
            <div v-if="currentPortDetails.manufacturer" class="flex justify-between">
              <span>Vendor:</span>
              <span class="text-[var(--fg-color)] truncate max-w-[200px]">{{ currentPortDetails.manufacturer }}</span>
            </div>
            <div v-if="currentPortDetails.serialNumber" class="flex justify-between">
              <span>Serial:</span>
              <span class="text-[var(--fg-color)] truncate max-w-[200px]">{{ currentPortDetails.serialNumber }}</span>
            </div>
          </div>

          <!-- Microcontroller board: catalog + suggestion from the connected USB device -->
          <div v-if="!hasFirmwareProject" class="flex flex-col gap-1.5">
            <label class="text-[9px] font-bold uppercase tracking-wider text-[var(--dim-color)] flex items-center justify-between">
              <span>Microcontroller Board</span>
              <span v-if="boardsList.length > 0" class="text-[8px] text-[var(--dim-color)]">{{ boardsList.length }} indexed</span>
            </label>

            <div class="flex items-center gap-1">
              <div class="flex-1 min-w-0">
                <AppInput
                  :model-value="boardSearchQuery"
                  @input="handleSearchBoardsInput"
                  placeholder="Search PlatformIO boards (esp32, pico, uno...)"
                  class="!h-6 !text-[10px]"
                />
              </div>
              <Loader2 v-if="isSearchingBoards" class="w-3 h-3 animate-spin text-[var(--accent-color)] shrink-0" />
            </div>

            <div class="max-h-32 overflow-y-auto border border-[var(--border-color)] ui-surface-list flex flex-col divide-y divide-[var(--border-color)]/40">
              <button
                v-for="b in boardsList"
                :key="b.id"
                type="button"
                @click="selectBoard(b)"
                class="px-2 py-1 text-left text-[10px] transition-colors flex items-center justify-between gap-2 cursor-pointer"
                :class="selectedBoard?.id === b.id ? 'bg-[var(--accent-color)]/15 text-[var(--accent-color)] font-bold' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
              >
                <span class="truncate">{{ b.name }} ({{ b.mcu }})</span>
                <span class="text-[8px] font-mono shrink-0">[{{ b.id }}]</span>
              </button>
              <div v-if="boardsList.length === 0 && !isSearchingBoards" class="p-2 text-[9px] text-[var(--dim-color)] italic text-center">
                No boards match that search.
              </div>
            </div>

            <div
              v-if="detectedBoard && (!selectedBoard || selectedBoard.id !== detectedBoard.id)"
              class="flex items-center justify-between gap-2 px-1 text-[9px]"
            >
              <span class="truncate text-[var(--dim-color)]">
                Detected USB device → <strong class="text-[var(--fg-color)]">{{ detectedBoard.name }}</strong>
              </span>
              <button
                type="button"
                @click="applyDetectedBoard"
                class="h-5 px-1.5 shrink-0 border border-[var(--accent-color)]/60 text-[var(--accent-color)] text-[8px] uppercase font-bold cursor-pointer"
              >
                Apply
              </button>
            </div>

            <div v-if="selectedBoard" class="p-2 border border-[var(--border-color)] ui-surface-card text-[9px] text-[var(--dim-color)] flex flex-wrap items-center gap-1.5">
              <span class="text-[var(--accent-color)] font-bold">{{ selectedBoard.name }}</span>
              <span>·</span>
              <span>{{ selectedBoard.vendor }}</span>
              <span>·</span>
              <span>{{ selectedBoard.platform }}</span>
              <span>·</span>
              <span>{{ selectedBoard.mcu }}</span>
            </div>
          </div>
        </div>

        <!-- Firmware Project Card -->
        <div class="border border-[var(--border-color)] ui-surface-card p-3.5 flex flex-col gap-3">
          <div class="flex items-center justify-between border-b border-[var(--border-color)] pb-2">
            <span class="font-bold text-[10px] uppercase tracking-wider text-[var(--fg-color)] flex items-center gap-1.5">
              <Sparkles class="w-3.5 h-3.5 text-[var(--accent-color)]" />
              <span>Firmware Project</span>
            </span>
            <span class="text-[9px] px-1.5 py-0.2 border border-[var(--border-color)] font-mono text-[var(--accent-color)] uppercase">
              {{ firmwareLanguage === 'micropython' ? 'micropython' : firmwareLanguage }}
            </span>
          </div>

          <!-- Case 1: Missing PlatformIO CLI on Host System -->
          <div
            v-if="status && !isPioCliInstalled && !isMicroPython"
            class="p-2.5 border border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[10px] font-mono flex flex-col gap-2"
          >
            <div class="flex items-start gap-2 text-[var(--status-warn-color)]">
              <AlertCircle class="w-4 h-4 shrink-0 mt-0.5" />
              <div>
                <span class="font-bold block">PlatformIO CLI Missing</span>
                <span class="text-[10px] text-[var(--dim-color)]">
                  PlatformIO Core is required to compile and flash C/C++ firmware in this workspace. MicroPython only needs mpremote.
                </span>
              </div>
            </div>
            <button
              v-if="status.toolchains.find((t) => t.id === 'platformio')?.autoInstallable"
              type="button"
              @click="handleInstallPlatformio"
              :disabled="isInstallingTool"
              class="h-7 px-3 border border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)] font-bold uppercase text-[9px] hover:border-[var(--status-warn-color)] transition-colors cursor-pointer flex items-center justify-center gap-1.5 disabled:opacity-50"
            >
              <Loader2 v-if="isInstallingTool" class="w-3.5 h-3.5 animate-spin" />
              <Download v-else class="w-3.5 h-3.5" />
              <span>Install PlatformIO Core</span>
            </button>
          </div>

          <!-- Case 2: project has no firmware scaffold yet -->
          <div
            v-if="status && !hasFirmwareProject"
            class="p-3 border border-[var(--border-color)] ui-surface-card text-[10px] font-mono flex flex-col gap-3"
          >
            <div class="flex items-start gap-2 text-[var(--accent-color)]">
              <Sparkles class="w-4 h-4 shrink-0 mt-0.5" />
              <div>
                <span class="font-bold block">Scaffold Firmware Project</span>
                <span class="text-[10px] text-[var(--dim-color)]">
                  Confirm the hardware board and pick a development language to generate the starter project.
                </span>
              </div>
            </div>

            <!-- Development Language -->
            <div class="flex flex-col gap-1.5">
              <label class="text-[9px] font-bold uppercase tracking-wider text-[var(--dim-color)]">Language</label>
              <AppSegmentedToggle
                :model-value="firmwareLanguage"
                :items="languageItems"
                @update:model-value="firmwareLanguage = $event as 'cpp' | 'c' | 'micropython'"
              />
            </div>

            <!-- Framework & Monitor Speed Grid -->
            <div class="grid gap-2" :class="isMicroPython ? 'grid-cols-1' : 'grid-cols-2'">
              <div v-if="!isMicroPython" class="flex flex-col gap-1">
                <label class="text-[9px] font-bold uppercase tracking-wider text-[var(--dim-color)]">Framework</label>
                <AppSelect
                  v-model="selectedFramework"
                  :options="frameworkOptions"
                  compact
                  class="[&>button]:h-6"
                />
              </div>

              <div class="flex flex-col gap-1">
                <label class="text-[9px] font-bold uppercase tracking-wider text-[var(--dim-color)]">Monitor Speed</label>
                <AppSelect
                  v-model="selectedMonitorSpeed"
                  :options="baudRateOptions"
                  compact
                  class="[&>button]:h-6"
                />
              </div>
            </div>

            <p v-if="isMicroPython" class="text-[9px] text-[var(--dim-color)] leading-relaxed border border-[var(--border-color)] bg-[var(--ui-bg-card)]/40 p-2">
              MicroPython runtime. Upload copies <code>main.py</code> and <code>boot.py</code> over the REPL; flash the MicroPython firmware once with <code>esptool</code> if the board still runs stock firmware.
            </p>

            <!-- Scaffold Action Button -->
            <button
              type="button"
              @click="handleScaffoldFirmware"
              :disabled="isScaffolding || !selectedBoard"
              class="h-8 px-3 border border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] font-bold uppercase text-[10px] hover:bg-[var(--accent-color)]/25 transition-colors cursor-pointer flex items-center justify-center gap-1.5 disabled:opacity-40"
            >
              <Loader2 v-if="isScaffolding" class="w-3.5 h-3.5 animate-spin" />
              <Sparkles v-else class="w-3.5 h-3.5" />
              <span>{{ isMicroPython ? 'Scaffold MicroPython Project' : `Scaffold ${firmwareLanguage === 'c' ? 'C' : 'C++'} Project` }}</span>
            </button>
          </div>

          <!-- Case 3: existing firmware project -->
          <p v-else class="text-[9px] text-[var(--dim-color)] leading-relaxed border border-[var(--border-color)] bg-[var(--ui-bg-card)]/40 p-2">
            Firmware project detected ({{ status?.firmwareLanguage === 'micropython' ? 'MicroPython' : 'PlatformIO' }}). Build and upload it from the Toolchain card.
          </p>
        </div>

        <!-- Toolchain & Operations Card -->
        <div class="border border-[var(--border-color)] ui-surface-card p-3.5 flex flex-col gap-3">
          <div class="flex items-center justify-between border-b border-[var(--border-color)] pb-2">
            <span class="font-bold text-[10px] uppercase tracking-wider text-[var(--fg-color)] flex items-center gap-1.5">
              <Hammer class="w-3.5 h-3.5 text-[var(--accent-color)]" />
              <span>Toolchain &amp; Operations</span>
            </span>
            <span
              v-if="selectedEnvironment"
              class="text-[9px] px-1.5 py-0.2 border border-[var(--border-color)] font-mono text-[var(--accent-color)]"
            >
              {{ selectedEnvironment }}
            </span>
          </div>

          <template v-if="hasFirmwareProject">
            <div v-if="!isMicroPython && status?.environments && status.environments.length > 0" class="flex flex-col gap-1">
              <label class="text-[9px] font-bold uppercase tracking-wider text-[var(--dim-color)]">Environment</label>
              <AppSelect
                v-model="selectedEnvironment"
                :options="environmentOptions"
                compact
                class="[&>button]:h-6"
              />
            </div>

            <p v-if="isMicroPython && !isMpremoteInstalled" class="p-2 border border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)] text-[9px] leading-relaxed">
              <code>mpremote</code> is required to upload MicroPython files. Install it with <code>python -m pip install --user mpremote</code> or open the toolchain modal.
            </p>

            <div class="grid grid-cols-2 gap-2 pt-1">
              <AppTooltip :text="isMicroPython ? 'MicroPython has no compile step' : 'Compile firmware'" class="w-full">
                <button
                  type="button"
                  @click="handleBuildFirmware"
                  :disabled="isBuilding || isFlashing || isMicroPython"
                  class="w-full h-7 text-[9px] font-bold uppercase border border-[var(--border-color)] bg-[var(--ui-bg-card)] text-[var(--fg-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] transition-colors flex items-center justify-center gap-1.5 cursor-pointer disabled:opacity-40"
                >
                  <Loader2 v-if="isBuilding" class="w-3.5 h-3.5 animate-spin" />
                  <Hammer v-else class="w-3.5 h-3.5" />
                  <span>Compile</span>
                </button>
              </AppTooltip>

              <button
                type="button"
                @click="handleFlashFirmware"
                :disabled="isBuilding || isFlashing || !selectedMcuPort || (isMicroPython && !isMpremoteInstalled)"
                class="h-7 text-[9px] font-bold uppercase border border-[var(--accent-color)] bg-[var(--accent-color)]/10 text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-[var(--bg-color)] transition-colors flex items-center justify-center gap-1.5 cursor-pointer disabled:opacity-40"
              >
                <Loader2 v-if="isFlashing" class="w-3.5 h-3.5 animate-spin" />
                <Upload v-else class="w-3.5 h-3.5" />
                <span>{{ isMicroPython ? 'Upload' : 'Flash' }}</span>
              </button>
            </div>
          </template>

          <p v-else class="text-[9px] text-[var(--dim-color)] italic">
            Scaffold a firmware project to enable build and upload.
          </p>

          <!-- Operation Results Banner -->
          <div v-if="lastOpResult || lastOpError" class="mt-1 flex flex-col gap-2">
            <div
              class="p-2.5 border text-[10px] flex items-start gap-2"
              :class="lastOpResult?.success
                ? 'border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)]'
                : 'border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)]'"
            >
              <CheckCircle2 v-if="lastOpResult?.success" class="w-4 h-4 shrink-0 mt-0.5" />
              <AlertCircle v-else class="w-4 h-4 shrink-0 mt-0.5" />
              <div class="flex-1 min-w-0">
                <span class="font-bold block">
                  {{ lastOpResult?.success ? 'Operation Succeeded' : 'Operation Failed' }}
                </span>
                <span class="text-[9px] block opacity-80" v-if="lastOpResult?.durationMs">
                  Completed in {{ (Number(lastOpResult.durationMs) / 1000).toFixed(2) }}s
                </span>
                <span class="text-[9px] block font-mono break-words" v-if="lastOpError">
                  {{ lastOpError }}
                </span>
              </div>
            </div>

            <button
              @click="showBuildLogs = !showBuildLogs"
              class="text-[9px] font-bold uppercase tracking-wider text-[var(--dim-color)] hover:text-[var(--fg-color)] flex items-center gap-1 cursor-pointer w-fit"
            >
              <span>{{ showBuildLogs ? 'Hide Compiler Log' : 'View Compiler Log' }}</span>
              <ChevronUp v-if="showBuildLogs" class="w-3 h-3" />
              <ChevronDown v-else class="w-3 h-3" />
            </button>

            <!-- Collapsible Build Output -->
            <pre
              v-if="showBuildLogs && (lastOpResult?.output || lastOpError)"
              class="p-2 bg-[var(--ui-bg-base)] border border-[var(--border-color)] text-[10px] font-mono text-[var(--fg-color)] max-h-48 overflow-y-auto whitespace-pre-wrap leading-tight"
            >{{ lastOpResult?.output || lastOpError }}</pre>
          </div>
        </div>
      </div>

      <!-- Right Column: Interactive Live Serial Monitor -->
      <div class="flex-1 border border-[var(--border-color)] bg-[var(--ui-bg-card)]/20 flex flex-col min-w-0 min-h-[400px]">
        <!-- Terminal Header Controls -->
        <div class="h-9 border-b border-[var(--border-color)] px-3 ui-surface-title flex items-center justify-between gap-3 text-[10px] shrink-0">
          <div class="flex items-center gap-2 min-w-0">
            <Terminal class="w-3.5 h-3.5 text-[var(--accent-color)] shrink-0" />
            <span class="font-bold text-[10px] uppercase tracking-wider text-[var(--fg-color)]">Serial Monitor</span>
            <span class="text-[9px] font-mono text-[var(--dim-color)] truncate">
              [{{ selectedMcuPort || 'No Port' }} @ {{ selectedBaudRate }} baud]
            </span>
          </div>

          <div class="flex items-center gap-1.5 shrink-0">
            <AppTooltip :text="isSerialPaused ? 'Resume serial output' : 'Pause serial output'">
              <button
                type="button"
                @click="isSerialPaused = !isSerialPaused"
                class="ui-icon-btn"
                :class="{ '!border-[var(--status-warn-border)] !text-[var(--status-warn-color)]': isSerialPaused }"
              >
                <Play v-if="isSerialPaused" :size="11" />
                <Pause v-else :size="11" />
              </button>
            </AppTooltip>

            <AppTooltip text="Clear serial log">
              <button
                type="button"
                @click="clearSerialLogs"
                class="ui-icon-btn"
              >
                <Trash2 :size="11" />
              </button>
            </AppTooltip>
          </div>
        </div>

        <!-- Terminal Output Area -->
        <div
          ref="terminalContainerRef"
          class="flex-1 p-3 font-mono text-[11px] overflow-y-auto bg-[var(--ui-bg-base)] flex flex-col gap-1 select-text"
        >
          <div
            v-for="entry in serialLines"
            :key="entry.id"
            class="leading-relaxed break-words"
            :class="{
              'text-[var(--fg-color)]': entry.type === 'in',
              'text-[var(--accent-color)] font-bold': entry.type === 'out',
              'text-[var(--dim-color)] italic': entry.type === 'sys'
            }"
          >
            <span class="opacity-40 select-none mr-2">[{{ entry.timestamp }}]</span>
            <span v-if="entry.type === 'out'" class="select-none text-[var(--accent-color)] mr-1">&gt;</span>
            <span v-else-if="entry.type === 'sys'" class="select-none mr-1">*</span>
            <span>{{ entry.text }}</span>
          </div>

          <div v-if="serialLines.length === 0" class="text-[var(--dim-color)] italic text-center py-12 text-[10px]">
            No serial output received yet.
          </div>
        </div>

        <!-- Terminal Input Bar -->
        <div class="border-t border-[var(--border-color)] p-2 bg-[var(--ui-bg-card)]/30 flex items-center gap-2 shrink-0">
          <AppInput
            v-model="serialInput"
            @keyup.enter="handleSendSerial"
            placeholder="Type command and press Enter to transmit..."
            class="flex-1"
          />
          <button
            @click="handleSendSerial"
            :disabled="!serialInput.trim()"
            class="px-3 py-1.5 border border-[var(--accent-color)] bg-[var(--accent-color)]/10 text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-black transition-all text-xs font-bold uppercase flex items-center gap-1 cursor-pointer disabled:opacity-40"
          >
            <Send class="w-3 h-3" />
            <span>Send</span>
          </button>
        </div>
      </div>
    </div>

    <!-- Sub-tab 2: Owon VDS1022 Digital Oscilloscope -->
    <OscilloscopeView
      v-else-if="activeSubTab === 'oscilloscope'"
      :project-path="project.path"
      :project-id="project.id"
      class="flex-1 min-h-0"
    />

    <!-- Sub-tab 3: 24MHz 8-Channel Logic Analyzer -->
    <LogicAnalyzerView
      v-else-if="activeSubTab === 'logic'"
      :project-path="project.path"
      class="flex-1 min-h-0"
    />

    <!-- Toolchains Inspection & Installation Modal -->
    <div
      v-if="showToolchainsModal"
      class="fixed inset-0 z-50 ui-overlay-backdrop flex items-center justify-center p-4 select-none"
    >
      <div class="border border-[var(--border-color)] bg-[var(--bg-color)] w-full max-w-lg flex flex-col max-h-[85vh] shadow-2xl font-mono text-xs">
        <!-- Modal Header -->
        <div class="h-9 px-4 border-b border-[var(--border-color)] ui-surface-title flex items-center justify-between shrink-0">
          <div class="flex items-center gap-2">
            <Wrench :size="14" class="text-[var(--accent-color)]" />
            <span class="text-[10px] font-bold uppercase tracking-wider">Electronics Toolchain</span>
          </div>
          <button
            type="button"
            class="ui-icon-btn"
            @click="showToolchainsModal = false"
          >
            <X :size="12" />
          </button>
        </div>

        <!-- Toolchain Items List -->
        <div class="p-3 flex flex-col gap-2 overflow-y-auto">
          <div
            v-for="tool in status?.toolchains ?? []"
            :key="tool.id"
            class="p-2.5 border border-[var(--border-color)] ui-surface-card flex items-center justify-between gap-3"
          >
            <div class="flex flex-col gap-0.5 min-w-0">
              <div class="flex items-center gap-2">
                <Check v-if="tool.isInstalled" :size="11" class="text-[var(--status-ok-color)] shrink-0" />
                <AlertTriangle v-else :size="11" class="text-[var(--status-warn-color)] shrink-0" />
                <span class="text-[11px] font-bold truncate">{{ tool.name }}</span>
                <span class="text-[8px] px-1 border border-[var(--border-color)] text-[var(--dim-color)] uppercase">{{ tool.category }}</span>
              </div>
              <span v-if="tool.isInstalled" class="text-[9px] text-[var(--status-ok-color)] truncate">
                {{ tool.version || 'Installed' }}
              </span>
              <span v-else class="text-[9px] text-[var(--dim-color)] truncate">
                {{ tool.installHint }}
              </span>
            </div>

            <!-- Install / Status Action -->
            <div class="shrink-0 flex items-center gap-1">
              <span
                v-if="tool.isInstalled"
                class="px-1.5 py-0.5 border border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)] text-[8px] font-bold uppercase"
              >
                Installed
              </span>
              <template v-else>
                <AppTooltip text="Copy install command">
                  <button
                    type="button"
                    class="ui-icon-btn"
                    @click="copyToolHint(tool)"
                  >
                    <Check v-if="copiedToolId === tool.id" :size="11" class="text-[var(--status-ok-color)]" />
                    <Copy v-else :size="11" />
                  </button>
                </AppTooltip>
                <button
                  v-if="tool.autoInstallable"
                  type="button"
                  @click="handleInstallSingleTool(tool.id)"
                  :disabled="installingToolId !== null"
                  class="h-6 px-2 text-[9px] uppercase font-bold border border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)] hover:border-[var(--status-warn-color)] flex items-center gap-1 cursor-pointer transition-colors disabled:opacity-40"
                >
                  <Loader2 v-if="installingToolId === tool.id" :size="11" class="animate-spin" />
                  <Download v-else :size="11" />
                  <span>Install</span>
                </button>
              </template>
            </div>
          </div>
        </div>

        <!-- Modal Footer -->
        <div class="h-9 px-4 border-t border-[var(--border-color)] ui-surface-title flex items-center justify-between gap-2 shrink-0">
          <button
            type="button"
            @click="handleInstallAllMissingTools"
            :disabled="installingToolId !== null || autoInstallableToolIds.length === 0"
            class="h-7 px-3 text-[9px] font-bold uppercase border border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)]/15 flex items-center gap-1.5 cursor-pointer transition-colors disabled:opacity-40"
          >
            <Loader2 v-if="installingToolId === 'all'" :size="11" class="animate-spin" />
            <Download v-else :size="11" />
            <span>Install All Missing</span>
          </button>

          <button
            type="button"
            class="h-7 px-3 text-[9px] uppercase border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer transition-colors"
            @click="showToolchainsModal = false"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
