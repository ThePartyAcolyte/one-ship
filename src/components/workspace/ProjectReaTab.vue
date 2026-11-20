<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue';
import { daemonCall } from '../../utils/daemonClient';
import { onDaemonEvent } from '../../utils/daemonEvents';
import {
  Play,
  Square,
  FolderSearch,
  GitFork,
  Code,
  Network,
  Sparkles,
  Check,
  AlertTriangle,
  Copy,
  Layers,
  Terminal,
  Wrench,
  Download,
  Loader2,
  X,
} from '@lucide/vue';
import { AppTooltip, AppToggle } from '../ui';
import { formatBytes } from '../../utils/format';
import { useCoreStore } from '../../stores/coreStore';
import { useDialogueStore } from '../../stores/dialogueStore';
import { useWorkspaceLayoutStore } from '../../stores/workspaceLayoutStore';

export interface ReaTabProjectProp {
  id: number;
  name: string;
  path: string;
  has_rea_enabled?: boolean;
}

export interface ReaToolStatus {
  id: string;
  name: string;
  binary: string;
  category: string;
  is_installed: boolean;
  version?: string | null;
  install_hint: string;
  auto_installable: boolean;
}

export interface ReaEngineStatus {
  project_path: string;
  is_enabled: boolean;
  is_running: boolean;
  pid?: number | null;
  http_port: number;
  gateway_url: string;
  service_key: string;
  in_flight_requests_count: number;
  queued_requests_count: number;
  last_activity_epoch_ms: number;
  idle_timeout_seconds: number;
  data_dir_size_bytes: number;
  targets_count: number;
}

export interface ReaTargetArtifact {
  name: string;
  rel_path: string;
  size_bytes: number;
  sha256_hex: string;
  detected_format: string;
  modified_epoch_seconds: number;
}

export interface ReaQueuedJob {
  job_id: string;
  project_path: string;
  project_name: string;
  agent_id: string;
  tool_name: string;
  is_heavy: boolean;
  status: string;
  enqueued_at_ms: number;
  started_at_ms?: number | null;
  completed_at_ms?: number | null;
}

export interface ReaSubsystemNode {
  id: string;
  name: string;
  description: string;
  symbols: string[];
}

export interface ReaCallFlowEdge {
  id: string;
  from_symbol: string;
  to_symbol: string;
  offset_hex: string | null;
  summary: string;
}

export interface ReaDecompiledHighlight {
  id: string;
  symbol: string;
  offset_hex: string | null;
  subsystem: string | null;
  signature: string | null;
  pseudocode: string;
  explanation: string;
}

export interface ReaProtocolItem {
  id: string;
  category: string | null;
  endpoint_or_key: string;
  protocol: string;
  offset_hex: string | null;
  description: string;
}

export interface ReaVisualViewsBundle {
  subsystems: ReaSubsystemNode[];
  call_edges: ReaCallFlowEdge[];
  decompiled_functions: ReaDecompiledHighlight[];
  protocol_items: ReaProtocolItem[];
}

const props = defineProps<{
  project: ReaTabProjectProp;
}>();

const coreStore = useCoreStore();
const dialogueStore = useDialogueStore();
const layoutStore = useWorkspaceLayoutStore();

type VisualLensTab = 'architecture' | 'pseudocode' | 'protocols';
const activeLens = ref<VisualLensTab>('architecture');

const engineStatus = ref<ReaEngineStatus | null>(null);
const toolchain = ref<ReaToolStatus[]>([]);
const targets = ref<ReaTargetArtifact[]>([]);
const queueJobs = ref<ReaQueuedJob[]>([]);
const visualViews = ref<ReaVisualViewsBundle>({
  subsystems: [],
  call_edges: [],
  decompiled_functions: [],
  protocol_items: [],
});

const selectedFunctionId = ref<string | null>(null);
const isLoading = ref(false);
const isTogglingEngine = ref(false);
const showToolchainModal = ref(false);
const copiedHintId = ref<string | null>(null);
const installingToolId = ref<string | null>(null);
const toolchainInstallError = ref<string | null>(null);

let unlistenQueue: (() => void) | null = null;

const selectedFunction = computed<ReaDecompiledHighlight | null>(() => {
  const list = visualViews.value.decompiled_functions;
  if (list.length === 0) return null;
  if (selectedFunctionId.value) {
    const found = list.find(f => f.id === selectedFunctionId.value);
    if (found) return found;
  }
  return list[0];
});

const activeAndQueuedJobsCount = computed<number>(() => {
  return queueJobs.value.filter(j => j.status === 'queued' || j.status === 'running').length;
});

const installedToolsCount = computed<number>(() => {
  return toolchain.value.filter(t => t.is_installed).length;
});

const allToolsInstalled = computed<boolean>(() => {
  return toolchain.value.length > 0 && installedToolsCount.value === toolchain.value.length;
});

const autoInstallableToolIds = computed<string[]>(() => {
  return toolchain.value
    .filter(t => !t.is_installed && t.auto_installable)
    .map(t => t.id);
});

async function refreshAllReaState() {
  if (!props.project?.path) return;
  isLoading.value = true;
  try {
    const [statusRes, targetsRes, viewsRes, queueRes, toolsRes] = await Promise.all([
      daemonCall('get_rea_engine_status', { projectPath: props.project.path }),
      daemonCall('list_rea_targets', { projectPath: props.project.path }),
      daemonCall('get_rea_visual_views', { projectPath: props.project.path }),
      daemonCall('get_rea_queue_snapshot', {}),
      daemonCall('check_rea_toolchain', { projectPath: props.project.path }),
    ]);
    engineStatus.value = statusRes;
    targets.value = targetsRes;
    visualViews.value = viewsRes;
    queueJobs.value = queueRes;
    toolchain.value = toolsRes;
    if (viewsRes.decompiled_functions.length > 0 && !selectedFunctionId.value) {
      selectedFunctionId.value = viewsRes.decompiled_functions[0].id;
    }
  } catch (err) {
    console.error('[ProjectReaTab] Failed to refresh REA state:', err);
  } finally {
    isLoading.value = false;
  }
}

async function handleTogglePluginEnabled(enabled: boolean) {
  if (!props.project?.path) return;
  try {
    const updated = await daemonCall('toggle_project_rea_plugin', {
      projectPath: props.project.path,
      enabled,
    });
    engineStatus.value = updated;
    coreStore.addStatusLog(
      enabled
        ? `Enabled Reverse Engineering Lab (REA) in ${props.project.name}`
        : `Disabled Reverse Engineering Lab (REA) in ${props.project.name}`
    );
    coreStore.setProjects(await daemonCall('get_projects', {}));
  } catch (err) {
    console.error('[ProjectReaTab] Failed to toggle REA plugin:', err);
  }
}

async function handleStartStopEngine() {
  if (!props.project?.path || isTogglingEngine.value) return;
  isTogglingEngine.value = true;
  try {
    if (engineStatus.value?.is_running) {
      engineStatus.value = await daemonCall('stop_rea_engine', {
        projectPath: props.project.path,
      });
      coreStore.addStatusLog(`Stopped REA HTTP engine for ${props.project.name} (0 MB footprint)`);
    } else {
      engineStatus.value = await daemonCall('start_rea_engine', {
        projectPath: props.project.path,
      });
      coreStore.addStatusLog(`Started REA HTTP engine for ${props.project.name} on port ${engineStatus.value.http_port}`);
    }
  } catch (err) {
    console.error('[ProjectReaTab] Engine start/stop error:', err);
  } finally {
    isTogglingEngine.value = false;
  }
}

function resolvePreferredReAgentId(): string {
  const projState = coreStore.agentStates[props.project.id];
  const agentsList: Array<{ id: string }> = projState?.agents || [];
  const hasReverser = agentsList.some(a => a.id.toLowerCase() === 'reverser');
  if (hasReverser) return 'reverser';
  const hasArchitect = agentsList.some(a => a.id.toLowerCase() === 'architect');
  if (hasArchitect) return 'architect';
  return agentsList[0]?.id || 'architect';
}

function dispatchAgentReversePrompt(promptText: string) {
  if (!props.project?.path) return;
  const agentId = resolvePreferredReAgentId();
  layoutStore.openFloatingChat(agentId, props.project.path);
  dialogueStore.sendUserMessage(agentId, promptText, props.project.path);
}

function handleAnalyzeTarget(target: ReaTargetArtifact) {
  const prompt = `Analyze target binary \`${target.rel_path}\` (${target.detected_format}, SHA256: \`${target.sha256_hex.slice(0, 16)}...\`) using the \`rea\` MCP server tools. Map its core subsystems, call-flow edges, decompiled C/C++ pseudocode highlights, and protocol/IPC endpoints into \`.one-ship/rea/views/architecture_map.json\`, \`decompiled_highlights.json\`, and \`protocols_and_endpoints.json\`.`;
  dispatchAgentReversePrompt(prompt);
}

function handleAskAboutEdge(edge: ReaCallFlowEdge) {
  const location = edge.offset_hex ? ` at offset \`${edge.offset_hex}\`` : '';
  const prompt = `Explain the call path from \`${edge.from_symbol}\` to \`${edge.to_symbol}\`${location} in detail, and show how we can intercept, hook, or re-implement this behavior cleanly.`;
  dispatchAgentReversePrompt(prompt);
}

function handleAskAboutFunction(fnItem: ReaDecompiledHighlight) {
  const location = fnItem.offset_hex ? ` at \`${fnItem.offset_hex}\`` : '';
  const subsystem = fnItem.subsystem ? ` (subsystem: ${fnItem.subsystem})` : '';
  const prompt = `Based on the decompiled function \`${fnItem.symbol}\`${location}${subsystem}, write a clean-room specification or hook implementation for our project.`;
  dispatchAgentReversePrompt(prompt);
}

async function copyInstallCommand(tool: ReaToolStatus) {
  try {
    await navigator.clipboard.writeText(tool.install_hint);
    copiedHintId.value = tool.id;
    setTimeout(() => {
      if (copiedHintId.value === tool.id) copiedHintId.value = null;
    }, 2000);
  } catch (err) {
    console.warn('Clipboard copy failed:', err);
  }
}

async function refreshToolchain() {
  if (!props.project?.path) return;
  toolchain.value = await daemonCall('check_rea_toolchain', { projectPath: props.project.path });
}

async function handleInstallTool(tool: ReaToolStatus) {
  if (installingToolId.value !== null || !tool.auto_installable) return;
  installingToolId.value = tool.id;
  toolchainInstallError.value = null;
  try {
    await daemonCall('install_missing_tools', { toolIds: [tool.id] });
    await refreshToolchain();
    coreStore.addStatusLog(`Installed RE tool '${tool.name}'.`);
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : String(err);
    toolchainInstallError.value = message;
    coreStore.addStatusLog(`[ERROR] RE tool '${tool.name}' installation failed: ${message}`);
  } finally {
    installingToolId.value = null;
  }
}

async function handleInstallAllMissingTools() {
  if (installingToolId.value !== null) return;
  const missing = autoInstallableToolIds.value;
  if (missing.length === 0) return;
  installingToolId.value = 'all';
  toolchainInstallError.value = null;
  try {
    await daemonCall('install_missing_tools', { toolIds: missing });
    await refreshToolchain();
    coreStore.addStatusLog(`Installed missing RE tools: ${missing.join(', ')}.`);
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : String(err);
    toolchainInstallError.value = message;
    coreStore.addStatusLog(`[ERROR] Missing RE tools installation failed: ${message}`);
  } finally {
    installingToolId.value = null;
  }
}

function openEngineLogsInTerminal() {
  layoutStore.setWorkspaceTab('terminal');
}

watch(
  () => props.project?.path,
  (newPath) => {
    if (newPath) {
      selectedFunctionId.value = null;
      refreshAllReaState();
    }
  },
  { immediate: true }
);

function handleContextRefresh(e: Event) {
  const tab = (e as CustomEvent<{ tab?: string }>).detail?.tab;
  if (tab === 'rea') {
    void refreshAllReaState();
  }
}

onMounted(async () => {
  window.addEventListener('oneship:context-refresh', handleContextRefresh);
  unlistenQueue = onDaemonEvent('rea-queue-updated', (payload) => {
    if (payload?.jobs) {
      queueJobs.value = [...payload.jobs].reverse();
    }
  });
});

onUnmounted(() => {
  window.removeEventListener('oneship:context-refresh', handleContextRefresh);
  if (unlistenQueue) {
    unlistenQueue();
    unlistenQueue = null;
  }
});
</script>

<template>
  <div class="flex flex-col flex-1 min-h-0 h-full bg-[var(--bg-color)] font-mono text-xs select-none overflow-hidden">
    <!-- ── 1. ENGINE, FOOTPRINT & TOOLCHAIN BAR ──────────────────── -->
    <div class="h-9 px-3 border-b border-[var(--border-color)] ui-surface-title flex items-center justify-between gap-3 shrink-0">
      <div class="flex items-center gap-2 min-w-0 overflow-x-auto scrollbar-none">
        <!-- Engine Footprint & Liveness Badge -->
        <AppTooltip :text="engineStatus?.is_running
          ? `REA HTTP engine listening on :${engineStatus.http_port} (PID ${engineStatus.pid})`
          : 'Engine parked. It wakes on demand for the next agent request.'">
          <div
            class="h-6 px-2 flex items-center gap-1.5 border text-[9px] font-bold uppercase tracking-wider shrink-0"
            :style="engineStatus?.is_running
              ? 'border-color: var(--status-ok-color); color: var(--status-ok-color); background: color-mix(in srgb, var(--status-ok-color) 10%, transparent)'
              : 'border-color: var(--border-color); color: var(--dim-color)'"
          >
            <span
              class="w-1.5 h-1.5"
              :style="engineStatus?.is_running ? 'background: var(--status-ok-color)' : 'background: var(--dim-color)'"
            />
            <span v-if="engineStatus?.is_running">ENGINE :{{ engineStatus.http_port }} · PID {{ engineStatus.pid }}</span>
            <span v-else>SLEEPING</span>
          </div>
        </AppTooltip>

        <!-- On-Demand Start / Stop Button -->
        <AppTooltip :text="engineStatus?.is_running ? 'Stop REA HTTP Engine & free RAM immediately' : 'Pre-warm REA HTTP Engine for this project'">
          <button
            type="button"
            @click="handleStartStopEngine"
            :disabled="isTogglingEngine"
            class="h-6 px-2 text-[9px] uppercase font-bold border border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] flex items-center gap-1 cursor-pointer transition-colors disabled:opacity-40 shrink-0"
          >
            <Square v-if="engineStatus?.is_running" :size="10" />
            <Play v-else :size="10" />
            <span>{{ engineStatus?.is_running ? 'Stop' : 'Wake' }}</span>
          </button>
        </AppTooltip>

        <!-- View Console Logs in Scripts Tab -->
        <AppTooltip v-if="engineStatus?.is_running" text="Inspect rea_mcp process output in Scripts Hub">
          <button
            type="button"
            @click="openEngineLogsInTerminal"
            class="h-6 px-2 text-[9px] uppercase border border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] flex items-center gap-1 cursor-pointer transition-colors shrink-0"
          >
            <Terminal :size="10" />
            <span>Logs</span>
          </button>
        </AppTooltip>
      </div>

      <!-- Right Controls: Storage, Toolchain, Plugin Enable Toggle, Refresh -->
      <div class="flex items-center gap-2 text-[9px] shrink-0">
        <AppTooltip text="Isolated evidence & decompiler cache in .one-ship/rea/">
          <span class="text-[var(--dim-color)] whitespace-nowrap">
            Cache: <strong class="text-[var(--fg-color)] tabular-nums">{{ formatBytes(engineStatus?.data_dir_size_bytes || 0) }}</strong>
          </span>
        </AppTooltip>

        <AppTooltip text="Inspect the host reverse-engineering toolchain and install missing tools">
          <button
            type="button"
            class="ui-icon-btn"
            :class="allToolsInstalled
              ? '!border-[var(--status-ok-border)] !text-[var(--status-ok-color)]'
              : '!border-[var(--status-warn-border)] !text-[var(--status-warn-color)]'"
            @click="showToolchainModal = true"
          >
            <Wrench :size="12" />
          </button>
        </AppTooltip>

        <div class="h-6 flex items-center gap-1.5 pl-2 border-l border-[var(--border-color)]">
          <span class="text-[var(--dim-color)] uppercase">MCP</span>
          <AppToggle
            :model-value="Boolean(engineStatus?.is_enabled)"
            @update:model-value="handleTogglePluginEnabled"
          />
        </div>
      </div>
    </div>

    <!-- ── 2. MAIN WORKSPACE SPLIT: TARGETS/QUEUE (LEFT) + VISUAL EXPLORER (RIGHT) ── -->
    <div class="flex flex-1 min-h-0 overflow-hidden">
      <!-- LEFT RAIL: TARGET ARTIFACTS (`targets/`) & MULTI-PROJECT FIFO QUEUE -->
      <div class="w-72 sm:w-80 shrink-0 border-r border-[var(--border-color)] bg-[var(--ui-bg-card)]/20 flex flex-col min-h-0 overflow-hidden">
        <!-- Targets Header -->
        <div class="px-3 py-2 border-b border-[var(--border-color)] bg-black/20 flex items-center justify-between">
          <div class="flex items-center gap-1.5 text-[10px] font-bold uppercase tracking-wider text-[var(--fg-color)]">
            <FolderSearch :size="12" class="text-[var(--accent-color)]" />
            <span>Targets ({{ targets.length }})</span>
          </div>
          <span class="text-[9px] text-[var(--dim-color)]">targets/</span>
        </div>

        <!-- Targets List -->
        <div class="flex-1 overflow-y-auto p-2 flex flex-col gap-2 min-h-0">
          <div
            v-if="targets.length === 0"
            class="p-4 border border-dashed border-[var(--border-color)] text-center flex flex-col items-center gap-2 text-[var(--dim-color)]"
          >
            <span class="text-[10px] font-bold text-[var(--fg-color)]">No Target Binaries Yet</span>
            <p class="text-[9px] leading-relaxed">
              Place binaries, firmware images, Electron <code>.asar</code> bundles, or <code>xochitl</code> ELF files inside <code>targets/</code> to analyze them with REA.
            </p>
          </div>

          <div
            v-for="target in targets"
            :key="target.rel_path"
            class="p-2.5 border border-[var(--border-color)] bg-[var(--ui-bg-card)]/50 hover:border-[var(--accent-color)]/70 transition-colors flex flex-col gap-1.5"
          >
            <div class="flex items-center justify-between gap-2">
              <span class="font-bold text-[11px] text-[var(--fg-color)] truncate">{{ target.name }}</span>
              <span class="text-[9px] text-[var(--dim-color)] shrink-0">{{ formatBytes(target.size_bytes) }}</span>
            </div>

            <div class="flex items-center justify-between gap-1 text-[9px]">
              <span class="px-1.5 py-0.5 bg-[var(--accent-color)]/15 text-[var(--accent-color)] font-bold">
                {{ target.detected_format }}
              </span>
              <span class="text-[var(--dim-color)] font-mono">
                SHA256: {{ target.sha256_hex.slice(0, 10) }}
              </span>
            </div>

            <button
              @click="handleAnalyzeTarget(target)"
              class="mt-1 w-full py-1 px-2 border border-[var(--accent-color)]/60 bg-[var(--accent-color)]/10 hover:bg-[var(--accent-color)] hover:text-[var(--bg-color)] text-[var(--accent-color)] font-bold text-[10px] transition-colors cursor-pointer flex items-center justify-center gap-1.5"
            >
              <Sparkles :size="11" />
              <span>Analyze Target with Agent</span>
            </button>
          </div>
        </div>

        <!-- Multi-Project Asynchronous FIFO Queue Panel -->
        <div class="border-t border-[var(--border-color)] bg-black/30 flex flex-col max-h-52 shrink-0">
          <div class="px-3 py-1.5 border-b border-[var(--border-color)]/60 flex items-center justify-between text-[10px]">
            <div class="flex items-center gap-1.5 font-bold uppercase tracking-wider text-[var(--dim-color)]">
              <Layers :size="11" class="text-[var(--accent-color)]" />
              <span>Async FIFO Queue</span>
            </div>
            <span
              class="px-1.5 py-0.2 text-[9px] font-bold"
              :style="activeAndQueuedJobsCount > 0 ? 'color: var(--status-warn-color)' : 'color: var(--dim-color)'"
            >
              {{ activeAndQueuedJobsCount }} active
            </span>
          </div>

          <div class="overflow-y-auto p-2 flex flex-col gap-1 text-[9px]">
            <div v-if="queueJobs.length === 0" class="py-3 text-center text-[var(--dim-color)]">
              Queue idle — background projects never interrupted on switch.
            </div>
            <div
              v-for="job in queueJobs.slice(0, 12)"
              :key="job.job_id"
              class="px-2 py-1 border border-[var(--border-color)]/50 bg-[var(--bg-color)]/70 flex items-center justify-between gap-2"
            >
              <div class="flex flex-col min-w-0">
                <span class="font-bold text-[var(--fg-color)] truncate">
                  [{{ job.project_name }}] {{ job.tool_name }}
                </span>
                <span class="text-[8px] text-[var(--dim-color)]">
                  agent: {{ job.agent_id }} · {{ job.job_id }}
                </span>
              </div>
              <span
                class="uppercase font-bold px-1 py-0.5 text-[8px] shrink-0"
                :style="job.status === 'running'
                  ? 'color: var(--status-warn-color)'
                  : job.status === 'queued'
                    ? 'color: var(--accent-color)'
                    : job.status === 'completed'
                      ? 'color: var(--status-ok-color)'
                      : 'color: var(--status-error-color)'"
              >
                {{ job.status }}
              </span>
            </div>
          </div>
        </div>
      </div>

      <!-- RIGHT STAGE: VISUAL REVERSE ENGINEERING EXPLORER -->
      <div class="flex-1 flex flex-col min-w-0 min-h-0 overflow-hidden">
        <!-- Sub-Navigation Tabs -->
        <div class="px-3 py-1.5 border-b border-[var(--border-color)] bg-[var(--ui-bg-card)]/30 flex items-center justify-between gap-2 shrink-0">
          <div class="flex items-center gap-1 text-[10px]">
            <button
              @click="activeLens = 'architecture'"
              class="px-2.5 py-1 border transition-colors uppercase font-bold tracking-wider flex items-center gap-1.5 cursor-pointer"
              :class="activeLens === 'architecture'
                ? 'border-[var(--accent-color)] bg-[var(--bg-color)] text-[var(--accent-color)]'
                : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
            >
              <GitFork :size="11" />
              <span>Architecture &amp; Call Flow ({{ visualViews.subsystems.length + visualViews.call_edges.length }})</span>
            </button>

            <button
              @click="activeLens = 'pseudocode'"
              class="px-2.5 py-1 border transition-colors uppercase font-bold tracking-wider flex items-center gap-1.5 cursor-pointer"
              :class="activeLens === 'pseudocode'
                ? 'border-[var(--accent-color)] bg-[var(--bg-color)] text-[var(--accent-color)]'
                : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
            >
              <Code :size="11" />
              <span>Decompiled Pseudocode ({{ visualViews.decompiled_functions.length }})</span>
            </button>

            <button
              @click="activeLens = 'protocols'"
              class="px-2.5 py-1 border transition-colors uppercase font-bold tracking-wider flex items-center gap-1.5 cursor-pointer"
              :class="activeLens === 'protocols'
                ? 'border-[var(--accent-color)] bg-[var(--bg-color)] text-[var(--accent-color)]'
                : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
            >
              <Network :size="11" />
              <span>Protocols &amp; Endpoints ({{ visualViews.protocol_items.length }})</span>
            </button>
          </div>
        </div>

        <!-- LENS 1: ARCHITECTURE & CALL FLOW MAP -->
        <div v-if="activeLens === 'architecture'" class="flex-1 overflow-y-auto p-4 flex flex-col gap-5">
          <div
            v-if="visualViews.subsystems.length === 0 && visualViews.call_edges.length === 0"
            class="flex-1 flex flex-col items-center justify-center p-8 border border-dashed border-[var(--border-color)] text-center gap-3"
          >
            <GitFork :size="24" class="text-[var(--dim-color)]" />
            <div class="text-xs font-bold uppercase tracking-wider text-[var(--fg-color)]">
              No Architecture Map Generated Yet
            </div>
            <p class="text-[11px] text-[var(--dim-color)] max-w-lg leading-relaxed">
              Click <strong>Analyze Target with Agent</strong> on any binary in <code>targets/</code> (e.g. <code>xochitl</code> from reMarkable 2). The agent will decompile symbols via REA and populate interactive subsystem cards and call-flow graphs here.
            </p>
          </div>

          <template v-else>
            <!-- Discovered Subsystems Grid -->
            <div v-if="visualViews.subsystems.length > 0" class="flex flex-col gap-2">
              <div class="text-[10px] font-bold uppercase tracking-wider text-[var(--dim-color)]">
                Discovered Binary Subsystems
              </div>
              <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                <div
                  v-for="sub in visualViews.subsystems"
                  :key="sub.id"
                  class="p-3 border border-[var(--border-color)] bg-[var(--ui-bg-card)]/40 flex flex-col gap-2"
                >
                  <div class="font-bold text-xs text-[var(--accent-color)]">{{ sub.name }}</div>
                  <p class="text-[11px] text-[var(--fg-color)] leading-relaxed">{{ sub.description }}</p>
                  <div v-if="sub.symbols.length > 0" class="flex flex-wrap gap-1 pt-1">
                    <span
                      v-for="sym in sub.symbols"
                      :key="sym"
                      class="px-1.5 py-0.5 bg-black/40 border border-[var(--border-color)]/60 text-[9px] text-[var(--dim-color)]"
                    >
                      {{ sym }}
                    </span>
                  </div>
                </div>
              </div>
            </div>

            <!-- Directed Call-Flow Graph Edges -->
            <div v-if="visualViews.call_edges.length > 0" class="flex flex-col gap-2">
              <div class="text-[10px] font-bold uppercase tracking-wider text-[var(--dim-color)]">
                Verified Call-Flow &amp; Cross-References (xrefs)
              </div>
              <div class="flex flex-col gap-2">
                <div
                  v-for="edge in visualViews.call_edges"
                  :key="edge.id"
                  class="p-3 border border-[var(--border-color)] bg-[var(--ui-bg-card)]/30 flex flex-col sm:flex-row sm:items-center justify-between gap-3"
                >
                  <div class="flex flex-col gap-1 min-w-0">
                    <div class="flex items-center gap-2 flex-wrap text-[11px] font-bold">
                      <span class="px-2 py-0.5 bg-black/50 border border-[var(--border-color)] text-[var(--fg-color)]">
                        {{ edge.from_symbol }}
                      </span>
                      <span class="text-[var(--accent-color)]">──►</span>
                      <span class="px-2 py-0.5 bg-[var(--accent-color)]/15 border border-[var(--accent-color)]/50 text-[var(--accent-color)]">
                        {{ edge.to_symbol }}
                      </span>
                      <span v-if="edge.offset_hex" class="text-[10px] text-[var(--dim-color)] font-normal">
                        @ {{ edge.offset_hex }}
                      </span>
                    </div>
                    <p class="text-[10px] text-[var(--dim-color)] leading-relaxed">
                      {{ edge.summary }}
                    </p>
                  </div>

                  <AppTooltip text="Ask agent how to intercept or re-implement this call path">
                    <button
                      @click="handleAskAboutEdge(edge)"
                      class="px-2.5 py-1 border border-[var(--border-color)] hover:border-[var(--accent-color)] text-[10px] text-[var(--fg-color)] hover:text-[var(--accent-color)] shrink-0 cursor-pointer"
                    >
                      Inspect with Agent
                    </button>
                  </AppTooltip>
                </div>
              </div>
            </div>
          </template>
        </div>

        <!-- LENS 2: DECOMPILED PSEUDOCODE INSPECTOR -->
        <div v-else-if="activeLens === 'pseudocode'" class="flex-1 flex min-h-0 overflow-hidden">
          <div
            v-if="visualViews.decompiled_functions.length === 0"
            class="flex-1 flex flex-col items-center justify-center p-8 m-4 border border-dashed border-[var(--border-color)] text-center gap-3"
          >
            <Code :size="24" class="text-[var(--dim-color)]" />
            <div class="text-xs font-bold uppercase tracking-wider text-[var(--fg-color)]">
              No Decompiled Functions Saved Yet
            </div>
            <p class="text-[11px] text-[var(--dim-color)] max-w-lg leading-relaxed">
              When the agent decompiles key functions via Ghidra/REA, readable C/C++/TS pseudocode and plain-language explanations are stored in <code>.one-ship/rea/views/decompiled_highlights.json</code>.
            </p>
          </div>

          <template v-else>
            <!-- Function Selector Rail -->
            <div class="w-64 shrink-0 border-r border-[var(--border-color)] overflow-y-auto p-2 flex flex-col gap-1.5">
              <button
                v-for="fnItem in visualViews.decompiled_functions"
                :key="fnItem.id"
                @click="selectedFunctionId = fnItem.id"
                class="p-2 border text-left transition-colors cursor-pointer flex flex-col gap-0.5"
                :class="selectedFunction?.id === fnItem.id
                  ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)]'
                  : 'border-[var(--border-color)]/60 bg-[var(--ui-bg-card)]/30 text-[var(--fg-color)] hover:border-[var(--accent-color)]/50'"
              >
                <span class="font-bold text-[10px] truncate">{{ fnItem.symbol }}</span>
                <div class="flex items-center justify-between text-[9px] text-[var(--dim-color)]">
                  <span>{{ fnItem.subsystem ?? '—' }}</span>
                  <span>{{ fnItem.offset_hex ?? '—' }}</span>
                </div>
              </button>
            </div>

            <!-- Selected Function Pseudocode & Explanation -->
            <div v-if="selectedFunction" class="flex-1 overflow-y-auto p-4 flex flex-col gap-4">
              <div class="flex items-start justify-between gap-3 border-b border-[var(--border-color)] pb-3">
                <div class="flex flex-col gap-1">
                  <div class="flex items-center gap-2">
                    <span class="font-bold text-sm text-[var(--accent-color)]">{{ selectedFunction.symbol }}</span>
                    <span v-if="selectedFunction.offset_hex" class="px-1.5 py-0.5 border border-[var(--border-color)] text-[9px] text-[var(--dim-color)]">
                      {{ selectedFunction.offset_hex }}
                    </span>
                  </div>
                  <code v-if="selectedFunction.signature" class="text-[10px] text-[var(--dim-color)]">{{ selectedFunction.signature }}</code>
                </div>

                <button
                  @click="handleAskAboutFunction(selectedFunction)"
                  class="px-3 py-1 border border-[var(--accent-color)] bg-[var(--accent-color)]/10 hover:bg-[var(--accent-color)] hover:text-[var(--bg-color)] text-[var(--accent-color)] font-bold text-[10px] cursor-pointer shrink-0 flex items-center gap-1.5"
                >
                  <Sparkles :size="11" />
                  <span>Hook / Re-implement with Agent</span>
                </button>
              </div>

              <!-- Plain-Language Explanation Box -->
              <div class="p-3 border border-[var(--border-color)] bg-[var(--ui-bg-card)]/40 flex flex-col gap-1">
                <span class="text-[9px] font-bold uppercase tracking-wider text-[var(--accent-color)]">
                  Architectural Explanation (Evidence-Verified)
                </span>
                <p class="text-[11px] text-[var(--fg-color)] leading-relaxed">
                  {{ selectedFunction.explanation }}
                </p>
              </div>

              <!-- Decompiled Pseudocode Block -->
              <div class="flex flex-col gap-1.5">
                <span class="text-[9px] font-bold uppercase tracking-wider text-[var(--dim-color)]">
                  Decompiled Pseudocode
                </span>
                <pre class="p-3 border border-[var(--border-color)] bg-black/60 text-[11px] text-[var(--fg-color)] overflow-x-auto leading-relaxed"><code>{{ selectedFunction.pseudocode }}</code></pre>
              </div>
            </div>
          </template>
        </div>

        <!-- LENS 3: PROTOCOLS, CLOUD ENDPOINTS & SCHEMAS -->
        <div v-else-if="activeLens === 'protocols'" class="flex-1 overflow-y-auto p-4 flex flex-col gap-3">
          <div
            v-if="visualViews.protocol_items.length === 0"
            class="flex-1 flex flex-col items-center justify-center p-8 border border-dashed border-[var(--border-color)] text-center gap-3"
          >
            <Network :size="24" class="text-[var(--dim-color)]" />
            <div class="text-xs font-bold uppercase tracking-wider text-[var(--fg-color)]">
              No Protocol or Endpoint Artifacts Extracted Yet
            </div>
            <p class="text-[11px] text-[var(--dim-color)] max-w-lg leading-relaxed">
              Discovered REST/WebSocket cloud routes, IPC messages, SQLite schemas, and custom binary file formats (e.g. reMarkable <code>.rm</code> page structures) will be listed here.
            </p>
          </div>

          <div v-else class="border border-[var(--border-color)] overflow-hidden">
            <table class="w-full text-left border-collapse text-[10px]">
              <thead>
                <tr class="border-b border-[var(--border-color)] bg-[var(--ui-bg-card)] text-[var(--dim-color)] uppercase">
                  <th class="py-2 px-3">Category</th>
                  <th class="py-2 px-3">Endpoint / Key / Structure</th>
                  <th class="py-2 px-3">Protocol</th>
                  <th class="py-2 px-3">Offset</th>
                  <th class="py-2 px-3">Description</th>
                </tr>
              </thead>
              <tbody>
                <tr
                  v-for="proto in visualViews.protocol_items"
                  :key="proto.id"
                  class="border-b border-[var(--border-color)]/40 hover:bg-white/5"
                >
                  <td class="py-2 px-3 font-bold uppercase text-[var(--accent-color)]">{{ proto.category ?? '—' }}</td>
                  <td class="py-2 px-3 font-bold text-[var(--fg-color)]">{{ proto.endpoint_or_key }}</td>
                  <td class="py-2 px-3 text-[var(--dim-color)]">{{ proto.protocol }}</td>
                  <td class="py-2 px-3 text-[var(--dim-color)]">{{ proto.offset_hex ?? '—' }}</td>
                  <td class="py-2 px-3 text-[var(--fg-color)]">{{ proto.description }}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </div>

    <!-- ── TOOLCHAIN INSPECTION & INSTALLATION MODAL ─────────────────── -->
    <div
      v-if="showToolchainModal"
      class="fixed inset-0 z-50 ui-overlay-backdrop flex items-center justify-center p-4 select-none"
    >
      <div class="border border-[var(--border-color)] bg-[var(--bg-color)] w-full max-w-lg flex flex-col max-h-[85vh] shadow-2xl font-mono text-xs">
        <div class="h-9 px-4 border-b border-[var(--border-color)] ui-surface-title flex items-center justify-between shrink-0">
          <div class="flex items-center gap-2">
            <Wrench :size="14" class="text-[var(--accent-color)]" />
            <span class="text-[10px] font-bold uppercase tracking-wider">RE Toolchain</span>
          </div>
          <button
            type="button"
            class="ui-icon-btn"
            @click="showToolchainModal = false"
          >
            <X :size="12" />
          </button>
        </div>

        <div
          v-if="toolchainInstallError"
          class="px-4 py-2 border-b border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] text-[10px]"
        >
          {{ toolchainInstallError }}
        </div>

        <div class="p-3 flex flex-col gap-2 overflow-y-auto">
          <div
            v-for="tool in toolchain"
            :key="tool.id"
            class="p-2.5 border border-[var(--border-color)] ui-surface-card flex items-center justify-between gap-3"
          >
            <div class="flex flex-col gap-0.5 min-w-0">
              <div class="flex items-center gap-2">
                <Check v-if="tool.is_installed" :size="11" class="text-[var(--status-ok-color)] shrink-0" />
                <AlertTriangle v-else :size="11" class="text-[var(--status-warn-color)] shrink-0" />
                <span class="text-[11px] font-bold truncate">{{ tool.name }}</span>
                <span class="text-[8px] px-1 border border-[var(--border-color)] text-[var(--dim-color)] uppercase">{{ tool.category }}</span>
              </div>
              <span v-if="tool.is_installed" class="text-[9px] text-[var(--status-ok-color)] truncate">
                {{ tool.version || 'Installed' }}
              </span>
              <span v-else class="text-[9px] text-[var(--dim-color)] truncate">
                {{ tool.install_hint }}
              </span>
            </div>

            <div class="shrink-0 flex items-center gap-1">
              <span
                v-if="tool.is_installed"
                class="px-1.5 py-0.5 border border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)] text-[8px] font-bold uppercase"
              >
                Installed
              </span>
              <template v-else>
                <AppTooltip text="Copy install command">
                  <button
                    type="button"
                    class="ui-icon-btn"
                    @click="copyInstallCommand(tool)"
                  >
                    <Check v-if="copiedHintId === tool.id" :size="11" class="text-[var(--status-ok-color)]" />
                    <Copy v-else :size="11" />
                  </button>
                </AppTooltip>
                <button
                  v-if="tool.auto_installable"
                  type="button"
                  @click="handleInstallTool(tool)"
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

        <div class="px-4 py-2.5 border-t border-[var(--border-color)] ui-surface-title flex items-center justify-between gap-2 shrink-0">
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
            @click="showToolchainModal = false"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
