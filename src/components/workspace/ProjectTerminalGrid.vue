<script setup lang="ts">
import { ref, shallowRef, computed, watch, onMounted, onUnmounted, nextTick } from 'vue';
import {
  Terminal,
  Play,
  Square,
  RotateCw,
  Filter,
  Copy,
  Check,
  Trash2,
  ArrowDown,
  Plus,
  Activity,
  Search,
  Loader2,
  CornerDownLeft,
  Settings2,
  History,
  X
} from '@lucide/vue';
import { daemonCall } from '../../utils/daemonClient';
import { onDaemonEvent } from '../../utils/daemonEvents';
import { useCoreStore } from '../../stores/coreStore';
import { useWorkspaceLayoutStore } from '../../stores/workspaceLayoutStore';
import { AppInput } from '../ui';
import { emitUiEvent } from '../../utils/uiEvents';
import { AppTooltip, AppContextMenu, type ContextMenuItem } from '../ui';
import { parseAnsiToHtml } from '../../utils/ansi';
import ScriptLaunchModal from '../modals/ScriptLaunchModal.vue';
import type { ProjectRecord, ScriptEntry, ScriptOption, ScriptOptionValues } from '../../types';

const props = defineProps<{
  project: ProjectRecord | null | undefined;
}>();

const coreStore = useCoreStore();
const layoutStore = useWorkspaceLayoutStore();


// Context Menu State
const contextMenuVisible = ref(false);
const contextMenuX = ref(0);
const contextMenuY = ref(0);
const contextMenuItems = ref<ContextMenuItem[]>([]);

// ── TYPES & INTERFACES ──────────────────────────────────────────
export interface ConsoleTarget {
  id: string;
  key: string;
  name: string;
  category: 'service' | 'task' | 'shell' | 'system';
  status: 'running' | 'stopped' | 'working' | 'idle' | 'failed' | 'completed';
  command?: string;
  args?: string[];
  options?: ScriptOption[];
  lastExitCode?: number | null;
  lastDurationMs?: number | null;
}

// ── SCRIPT & TARGET DISCOVERY ───────────────────────────────────
const searchQuery = ref('');

const allScripts = computed<ScriptEntry[]>(() => props.project?.scripts || []);

const persistentScripts = computed<ScriptEntry[]>(() => {
  return allScripts.value.filter((s) => s.is_persistent);
});

const deterministicTasks = computed<ScriptEntry[]>(() => {
  return allScripts.value.filter((s) => !s.is_persistent);
});

function isServiceRunning(serviceId: string): boolean {
  if (!props.project?.id) return false;
  return coreStore.runningServices.includes(`${props.project.id}:${serviceId}`);
}

// Service transition tracking (restarting, stopping)
interface ServiceTransition {
  mode: 'stopping' | 'restarting';
  attempt: number;
  maxAttempts: number;
}
const serviceTransitions = ref<Record<string, ServiceTransition>>({});

function getServiceKey(serviceId: string): string {
  return `${props.project?.id}:${serviceId}`;
}

function isServiceTransitioning(serviceId: string): boolean {
  return !!serviceTransitions.value[getServiceKey(serviceId)];
}

// Task execution states
const runningTasks = ref<Record<string, boolean>>({});
// `status` is the execution record's `completed` / `failed`; `exit_code` is null when the
// process never started.
const lastTaskResults = ref<Record<string, {
  status: string;
  duration_ms: number | null;
  exit_code: number | null;
  exec_id: number;
}>>({});


// Filtered scripts by search query
const filteredPersistentScripts = computed(() => {
  if (!searchQuery.value.trim()) return persistentScripts.value;
  const q = searchQuery.value.toLowerCase();
  return persistentScripts.value.filter((s) =>
    s.name.toLowerCase().includes(q) ||
    s.command.toLowerCase().includes(q) ||
    s.id.toLowerCase().includes(q)
  );
});

const filteredDeterministicTasks = computed(() => {
  if (!searchQuery.value.trim()) return deterministicTasks.value;
  const q = searchQuery.value.toLowerCase();
  return deterministicTasks.value.filter((t) =>
    t.name.toLowerCase().includes(q) ||
    t.command.toLowerCase().includes(q) ||
    t.id.toLowerCase().includes(q)
  );
});

// ── SELECTED TARGET RESOLUTION ──────────────────────────────────
const selectedTargetKey = ref<string>('system');

const currentTarget = computed<ConsoleTarget>(() => {
  if (!props.project) {
    return {
      id: 'system',
      key: 'system',
      name: 'PLATFORM: supervisor',
      category: 'system',
      status: 'idle',
    };
  }

  // 1. Interactive Shells
  const shell = shellSessions.value.find(s => s.id === selectedTargetKey.value);
  if (shell) {
    return {
      id: shell.id,
      key: shell.serviceKey,
      name: `SHELL: ${shell.label}`,
      category: 'shell',
      status: 'running',
    };
  }

  // 2. Persistent Services
  const svc = persistentScripts.value.find((s) => s.id === selectedTargetKey.value);
  if (svc) {
    const running = isServiceRunning(svc.id);
    return {
      id: svc.id,
      key: `${props.project.path}:svc:${svc.id}`,
      name: `SERVICE: ${svc.name || svc.id}`,
      category: 'service',
      status: running ? 'running' : 'stopped',
      command: `${svc.command} ${(svc.args || []).join(' ')}`,
      args: svc.args,
      options: svc.options,
    };
  }

  // 3. Deterministic Tasks
  const task = deterministicTasks.value.find((t) => t.id === selectedTargetKey.value);
  if (task) {
    const isRunning = !!runningTasks.value[task.id];
    const last = lastTaskResults.value[task.id];
    return {
      id: task.id,
      key: `${props.project.path}:tsk:${task.id}`,
      name: `TASK: ${task.name || task.id}`,
      category: 'task',
      status: isRunning ? 'working' : (last?.status === 'completed' ? 'completed' : (last?.status === 'failed' ? 'failed' : 'idle')),
      command: `${task.command} ${(task.args || []).join(' ')}`,
      args: task.args,
      options: task.options,
      lastExitCode: last?.exit_code,
      lastDurationMs: last?.duration_ms,
    };
  }

  // 4. System Supervisor fallback
  return {
    id: 'system',
    key: 'system',
    name: 'PLATFORM: supervisor',
    category: 'system',
    status: 'idle',
  };
});

function selectTarget(key: string) {
  selectedTargetKey.value = key;
  fetchCurrentOutput();
}

// ── LOG OUTPUT & STREAMING STATE ────────────────────────────────
const rawOutput = ref<string>('');
const isOutputCleared = ref<boolean>(false);
const filterErrorsOnly = ref<boolean>(false);
const stickToBottom = ref<boolean>(true);
const terminalViewportRef = ref<HTMLElement | null>(null);

const formattedOutputHtml = computed(() => {
  if (isOutputCleared.value) return '// Terminal output cleared';
  if (!rawOutput.value) return '// Waiting for output stream...';
  return parseAnsiToHtml(rawOutput.value);
});

async function fetchCurrentOutput() {
  if (!props.project) {
    rawOutput.value = '';
    return;
  }

  try {
    const out = await daemonCall('read_terminal_output', {
      projectPath: props.project?.path ?? null,
      paneId: currentTarget.value.key,
      linesCount: 300,
      filter: filterErrorsOnly.value ? 'errors_only' : 'all',
    });

    if (!isOutputCleared.value) {
      rawOutput.value = out || `// No output logged yet for '${currentTarget.value.name}'`;
    }

    if (stickToBottom.value) {
      scrollToBottom();
    }
  } catch (err: unknown) {
    if (!isOutputCleared.value) {
      rawOutput.value = `// Failed to read terminal output:\n// ${err instanceof Error ? err.message : String(err)}`;
    }
  }
}

function scrollToBottom() {
  nextTick(() => {
    if (terminalViewportRef.value) {
      terminalViewportRef.value.scrollTop = terminalViewportRef.value.scrollHeight;
    }
  });
}

function handleViewportScroll() {
  if (!terminalViewportRef.value) return;
  const { scrollTop, scrollHeight, clientHeight } = terminalViewportRef.value;
  // If user scrolled up by more than 40px, disable stickToBottom
  const atBottom = scrollHeight - (scrollTop + clientHeight) < 40;
  stickToBottom.value = atBottom;
}

function toggleStickToBottom() {
  stickToBottom.value = !stickToBottom.value;
  if (stickToBottom.value) {
    scrollToBottom();
  }
}

function toggleErrorFilter() {
  filterErrorsOnly.value = !filterErrorsOnly.value;
  isOutputCleared.value = false;
  fetchCurrentOutput();
}

function clearConsole() {
  isOutputCleared.value = true;
  rawOutput.value = '';
}

const copiedSuccess = ref(false);
let copyTimer: ReturnType<typeof setTimeout> | null = null;

async function copyOutput() {
  if (!rawOutput.value) return;
  try {
    await navigator.clipboard.writeText(rawOutput.value);
    copiedSuccess.value = true;
    if (copyTimer) clearTimeout(copyTimer);
    copyTimer = setTimeout(() => {
      copiedSuccess.value = false;
    }, 2000);
  } catch (e) {
    console.error('Failed to copy terminal output:', e);
  }
}

// ── BIDIRECTIONAL INTERACTIVE INPUT ─────────────────────────────
const cmdInput = ref('');
const isSendingInput = ref(false);
const cmdHistory = ref<string[]>([]);
const historyIndex = ref(-1);

async function handleSendInput() {
  const text = cmdInput.value;
  if (!text.trim() || !props.project) return;

  cmdHistory.value.push(text);
  historyIndex.value = -1;
  cmdInput.value = '';
  isSendingInput.value = true;
  isOutputCleared.value = false;

  try {
    await daemonCall('send_terminal_input', {
      projectPath: props.project.path,
      processKey: currentTarget.value.key,
      input: text,
    });
    // Short step delay for command response
    setTimeout(fetchCurrentOutput, 150);
  } catch (err: unknown) {
    coreStore.addStatusLog(`[ERROR] Stdin failed: ${err instanceof Error ? err.message : String(err)}`);
  } finally {
    isSendingInput.value = false;
  }
}

function handleHistoryNavigation(direction: 'up' | 'down') {
  if (cmdHistory.value.length === 0) return;

  if (direction === 'up') {
    if (historyIndex.value === -1) {
      historyIndex.value = cmdHistory.value.length - 1;
    } else if (historyIndex.value > 0) {
      historyIndex.value--;
    }
    cmdInput.value = cmdHistory.value[historyIndex.value] || '';
  } else {
    if (historyIndex.value !== -1 && historyIndex.value < cmdHistory.value.length - 1) {
      historyIndex.value++;
      cmdInput.value = cmdHistory.value[historyIndex.value] || '';
    } else {
      historyIndex.value = -1;
      cmdInput.value = '';
    }
  }
}

// Send SIGINT / Interrupt to active shell or service
async function handleInterruptProcess() {
  if (currentTarget.value.category === 'shell') {
    const session = shellSessions.value.find(s => s.id === currentTarget.value.id);
    if (session) await closeShell(session);
  } else if (currentTarget.value.category === 'service') {
    const svc = persistentScripts.value.find((s) => s.id === currentTarget.value.id);
    if (svc) stopService(svc);
  }
}

// ── SCRIPT LAUNCH & CONFIG MODAL ────────────────────────────────
const modalShow = ref(false);
const modalScript = shallowRef<ScriptEntry | null>(null);
const selectedOptions = ref<Record<string, ScriptOptionValues>>({});

function openScriptConfig(script: ScriptEntry) {
  modalScript.value = script;
  modalShow.value = true;
}

function onModalLaunch(opts: ScriptOptionValues) {
  if (!modalScript.value) return;
  selectedOptions.value[modalScript.value.id] = opts;
  modalShow.value = false;
  if (modalScript.value.is_persistent) {
    startService(modalScript.value, opts);
  } else {
    runTask(modalScript.value, opts);
  }
}

function onModalSave(opts: ScriptOptionValues) {
  if (!modalScript.value) return;
  selectedOptions.value[modalScript.value.id] = opts;
}

// ── SERVICE EXECUTION CONTROLS ──────────────────────────────────
interface PortConflictInfo {
  service: ScriptEntry;
  customOptions?: ScriptOptionValues;
  port: number;
  pid: number;
  cmdline: string;
}

const portConflict = shallowRef<PortConflictInfo | null>(null);
const isReleasingPort = ref(false);

async function resolvePortConflict() {
  if (!portConflict.value) return;
  isReleasingPort.value = true;
  const target = portConflict.value;
  try {
    coreStore.addStatusLog(`[INFO] Terminating PID ${target.pid} to release port ${target.port}...`);
    await daemonCall('release_port', {
      port: target.port,
      targetPid: target.pid,
    });
    coreStore.addStatusLog(`[INFO] Port ${target.port} released.`);
    portConflict.value = null;
    await startService(target.service, target.customOptions);
  } catch (err: unknown) {
    console.error('Failed to release port', err);
    coreStore.addStatusLog(`[ERROR] Failed to release port ${target.port}: ${String(err)}`);
  } finally {
    isReleasingPort.value = false;
  }
}

function cancelPortConflict() {
  portConflict.value = null;
}

// Command, arguments and option flags are resolved by the backend from rula.toml; options the
// user has not set take their declared default. Starting a running service replaces it.
async function startService(svc: ScriptEntry, customOptions?: ScriptOptionValues) {
  if (!props.project) return;
  const options = customOptions || selectedOptions.value[svc.id] || {};
  selectTarget(svc.id);
  isOutputCleared.value = false;

  try {
    const outcome = await daemonCall('start_service', {
      projectId: props.project.id,
      serviceId: svc.id,
      options,
    });
    if (outcome.kind === 'port_occupied') {
      portConflict.value = { service: svc, customOptions, port: outcome.port, pid: outcome.pid, cmdline: outcome.cmdline };
      coreStore.addStatusLog(`[WARN] Service '${svc.name}' not started: port ${outcome.port} is held by PID ${outcome.pid}.`);
      return;
    }
    coreStore.addStatusLog(`[INFO] Started service '${svc.name}' (PID ${outcome.pid}).`);
    setTimeout(fetchCurrentOutput, 200);
  } catch (err: unknown) {
    console.error('Failed to start service', err);
    coreStore.addStatusLog(`[ERROR] Failed to start service '${svc.name}': ${String(err)}`);
  }
}

async function stopService(svc: ScriptEntry) {
  if (!props.project) return;
  const key = getServiceKey(svc.id);
  if (serviceTransitions.value[key]) return;

  serviceTransitions.value[key] = {
    mode: 'stopping',
    attempt: 1,
    maxAttempts: 3,
  };

  try {
    await daemonCall('stop_service', {
      projectId: props.project.id,
      serviceId: svc.id,
    });
    coreStore.addStatusLog(`[INFO] Stopped service '${svc.name}'.`);
  } catch (err: unknown) {
    console.error('Failed to stop service', err);
    coreStore.addStatusLog(`[ERROR] Failed to stop service '${svc.name}': ${String(err)}`);
  } finally {
    delete serviceTransitions.value[key];
    setTimeout(fetchCurrentOutput, 300);
  }
}

async function restartService(svc: ScriptEntry) {
  if (!props.project) return;
  const key = getServiceKey(svc.id);
  if (serviceTransitions.value[key]) return;

  serviceTransitions.value[key] = {
    mode: 'restarting',
    attempt: 1,
    maxAttempts: 3,
  };

  coreStore.addStatusLog(`[INFO] Restarting service '${svc.name}'...`);
  try {
    await startService(svc);
  } finally {
    delete serviceTransitions.value[key];
  }
}

// ── DETERMINISTIC TASK EXECUTION ────────────────────────────────
// Command, arguments and option flags are resolved by the backend from rula.toml; options the
// user has not set take their declared default.
async function runTask(task: ScriptEntry, customOptions?: ScriptOptionValues) {
  if (runningTasks.value[task.id] || !props.project) return;
  runningTasks.value[task.id] = true;
  selectTarget(task.id);
  isOutputCleared.value = false;

  try {
    const report = await daemonCall('run_project_task', {
      projectId: props.project.id,
      taskId: task.id,
      options: customOptions || selectedOptions.value[task.id] || {},
    });

    lastTaskResults.value[task.id] = {
      status: report.status,
      duration_ms: report.duration_ms ?? null,
      exit_code: report.exit_code ?? null,
      exec_id: report.id,
    };
    fetchCurrentOutput();
  } catch (err: unknown) {
    console.error('Task execution error:', err);
    coreStore.addStatusLog(`[ERROR] Task '${task.name}' failed: ${String(err)}`);
  } finally {
    runningTasks.value[task.id] = false;
  }
}

// ── RIGHT-CLICK CONTEXT MENUS ───────────────────────────────────
function openScriptContextMenu(e: MouseEvent, svc: ScriptEntry) {
  e.preventDefault();
  e.stopPropagation();

  contextMenuX.value = e.clientX;
  contextMenuY.value = e.clientY;

  const items: ContextMenuItem[] = [];
  const running = isServiceRunning(svc.id);
  const transitioning = isServiceTransitioning(svc.id);

  if (running) {
    items.push({
      label: 'Focus Console',
      icon: Terminal,
      action: () => selectTarget(svc.id)
    });
    items.push({
      label: 'Performance Telemetry',
      icon: Activity,
      action: () => layoutStore.setWorkspaceTab('profiler')
    });
    items.push({
      label: 'Restart Service',
      icon: RotateCw,
      disabled: transitioning,

      action: () => restartService(svc)
    });
    items.push({
      label: 'Stop Service',
      icon: Square,
      danger: true,
      disabled: transitioning,
      action: () => stopService(svc)
    });
  } else {
    items.push({
      label: 'Start Service',
      icon: Play,
      disabled: transitioning,
      action: () => startService(svc)
    });
  }

  if ((svc.options?.length ?? 0) > 0) {
    items.push({
      label: 'Configure Options',
      icon: Settings2,
      action: () => openScriptConfig(svc)
    });
  }

  items.push({
    separator: true,
    label: '',
    action: () => {}
  });

  items.push({
    label: 'Open History',
    icon: History,
    action: () => emitUiEvent('oneship-open-history', props.project ?? undefined)
  });

  contextMenuItems.value = items;
  contextMenuVisible.value = true;
}

function openTaskContextMenu(e: MouseEvent, task: ScriptEntry) {
  e.preventDefault();
  e.stopPropagation();

  contextMenuX.value = e.clientX;
  contextMenuY.value = e.clientY;

  const items: ContextMenuItem[] = [];
  const isRunning = !!runningTasks.value[task.id];

  items.push({
    label: `Run Task (${task.id})`,
    icon: Play,
    disabled: isRunning,
    action: () => runTask(task)
  });

  const last = lastTaskResults.value[task.id];
  if (last && props.project) {
    items.push({
      label: 'View Last Report',
      icon: History,
      action: () => emitUiEvent('oneship-open-task-report', { project: props.project!, execId: last.exec_id })
    });
  }

  if ((task.options?.length ?? 0) > 0) {
    items.push({
      label: 'Configure Flags',
      icon: Settings2,
      action: () => openScriptConfig(task)
    });
  }

  items.push({
    separator: true,
    label: '',
    action: () => {}
  });

  items.push({
    label: 'Open History',
    icon: History,
    action: () => emitUiEvent('oneship-open-history', props.project ?? undefined)
  });

  contextMenuItems.value = items;
  contextMenuVisible.value = true;
}

// ── INTERACTIVE SHELL LAUNCHER STATE ─────────────────────────────
const isStartingShell = ref(false);

// ── MULTI-SHELL REGISTRY ─────────────────────────────────────────
// Each shell instance is identified by a unique service_id: shell_<timestamp_ms>.
// The supervisor is the source of truth (`list_interactive_shells`): shells keep running while
// another project or tab is shown, and are listed again when the grid is shown.
interface ShellSession {
  id: string;           // e.g. "shell_1748200000000"
  label: string;        // e.g. "Shell 1"
  serviceKey: string;   // e.g. "<project_path>:svc:shell_1748200000000"
}

const shellSessions = ref<ShellSession[]>([]);

async function loadShellSessions() {
  const project = props.project;
  if (!project) {
    shellSessions.value = [];
    return;
  }
  try {
    const ids = await daemonCall('list_interactive_shells', { projectId: project.id });
    if (props.project?.id !== project.id) return;
    shellSessions.value = ids.map((id, index) => ({
      id,
      label: `Shell ${index + 1}`,
      serviceKey: `${project.path}:svc:${id}`,
    }));
  } catch (err: unknown) {
    coreStore.addStatusLog(`[ERROR] Failed to list shells: ${String(err)}`);
  }
}

async function spawnNewShell() {
  if (!props.project) return;
  const id = `shell_${Date.now()}`;
  isStartingShell.value = true;
  isOutputCleared.value = false;

  try {
    await daemonCall('launch_interactive_shell', {
      projectId: props.project.id,
      shellId: id,
    });
    await loadShellSessions();
    selectTarget(id);
    const label = shellSessions.value.find(s => s.id === id)?.label ?? id;
    coreStore.addStatusLog(`[INFO] ${label} launched.`);
    setTimeout(fetchCurrentOutput, 250);
  } catch (err: unknown) {
    coreStore.addStatusLog(`[ERROR] Failed to launch shell: ${String(err)}`);
  } finally {
    isStartingShell.value = false;
  }
}

// Stopping a shell that already exited succeeds; any other failure keeps the session listed.
async function closeShell(session: ShellSession) {
  if (!props.project) return;
  try {
    await daemonCall('stop_service', {
      projectId: props.project.id,
      serviceId: session.id,
    });
  } catch (err: unknown) {
    coreStore.addStatusLog(`[ERROR] Failed to close ${session.label}: ${String(err)}`);
    return;
  }
  await loadShellSessions();
  // If the closed shell was selected, fall back to the first remaining shell or system
  if (selectedTargetKey.value === session.id) {
    const next = shellSessions.value[0];
    selectedTargetKey.value = next ? next.id : 'system';
    fetchCurrentOutput();
  }
  coreStore.addStatusLog(`[INFO] ${session.label} closed.`);
}

// ── LIFECYCLE & REACTIVE OUTPUT ──────────────────────────────────
let pollInterval: ReturnType<typeof setInterval> | null = null;
const unlisteners = ref<Array<() => void>>([]);

const MAX_RAW_OUTPUT_CHARS_COUNT = 512 * 1024;

// Append a single raw line to the current output buffer without a full fetch cycle
function appendLiveLine(line: string) {
  if (isOutputCleared.value) return;
  const next = rawOutput.value
    ? rawOutput.value + '\n' + line
    : line;
  rawOutput.value = next.length > MAX_RAW_OUTPUT_CHARS_COUNT
    ? next.slice(next.length - MAX_RAW_OUTPUT_CHARS_COUNT)
    : next;
  if (stickToBottom.value) scrollToBottom();
}

// Subscribe to real-time daemon events (`/api/events`).
// task-output-chunk: one line per task stdout/stderr (emitted by spawn_task).
// agent-step-update: one event per agent step (tool call, response, etc.).
function subscribeToLiveEvents() {
  // Unregister previous listeners before re-subscribing
  for (const fn of unlisteners.value) fn();
  unlisteners.value = [];

  const taskChunkUn = onDaemonEvent('task-output-chunk', (payload) => {
    if (!props.project) return;
    const { task_id, line, is_stderr } = payload;
    // Only forward if this task belongs to the currently viewed target
    if (selectedTargetKey.value === task_id) {
      appendLiveLine(is_stderr ? `[STDERR] ${line}` : line);
    }
  });

  const agentStepUn = onDaemonEvent('agent-step-update', (payload) => {
    if (!props.project) return;
    const { project_path, action_summary } = payload;
    // Forward agent step summaries only when viewing the system supervisor target
    if (
      project_path === props.project.path &&
      selectedTargetKey.value === 'system' &&
      action_summary
    ) {
      appendLiveLine(`[AGENT] ${action_summary}`);
    }
  });

  unlisteners.value = [taskChunkUn, agentStepUn];
}

watch(
  () => props.project?.id,
  (newId, oldId) => {
    if (newId !== oldId) {
      rawOutput.value = '';
      isOutputCleared.value = false;
      selectedTargetKey.value = 'system';
      loadShellSessions();
      fetchCurrentOutput();
      subscribeToLiveEvents();
    }
  }
);

watch(
  () => selectedTargetKey.value,
  () => {
    isOutputCleared.value = false;
    fetchCurrentOutput();
  }
);

onMounted(async () => {
  loadShellSessions();
  fetchCurrentOutput();
  // Polling is now a 3s sync heartbeat — live output arrives via event listeners
  pollInterval = setInterval(fetchCurrentOutput, 3000);
  subscribeToLiveEvents();
});

onUnmounted(() => {
  if (pollInterval) clearInterval(pollInterval);
  if (copyTimer) clearTimeout(copyTimer);
  for (const fn of unlisteners.value) fn();
  unlisteners.value = [];
});
</script>

<template>
  <div v-if="!project" class="flex flex-1 h-full items-center justify-center p-8 text-center text-[var(--dim-color)] text-[11px] font-mono select-none bg-[var(--bg-color)]">
    // NO ACTIVE PROJECT FOR SCRIPTS CONSOLE
  </div>

  <div v-else class="flex flex-1 h-full min-h-0 bg-[var(--bg-color)] font-mono overflow-hidden select-text">
    <!-- ── LEFT PANE: Scripts & Session Explorer (35% width, min 300px) ─────────────── -->
    <div class="w-80 md:w-96 border-r border-[var(--border-color)] bg-[var(--ui-bg-card)]/20 flex flex-col shrink-0 min-h-0">
      <!-- Rail Header: History + Shell on the first row, filter on the second (Spec 13 §3.13) -->
      <div class="shrink-0 border-b border-[var(--border-color)] ui-surface-title">
        <div class="h-9 flex items-center justify-end gap-1.5 px-2">
          <!-- View Execution History -->
          <AppTooltip text="View execution history and reports">
            <button
              type="button"
              @click="emitUiEvent('oneship-open-history', project)"
              class="h-6 px-2 border border-[var(--border-color)] text-[9px] uppercase font-bold text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] transition-colors cursor-pointer flex items-center gap-1"
            >
              <History :size="10" />
              <span>History</span>
            </button>
          </AppTooltip>

          <!-- Add / Focus Interactive Shell -->
          <AppTooltip text="Spawn or focus interactive bash session">
            <button
              type="button"
              @click="spawnNewShell"
              :disabled="isStartingShell"
              class="h-6 px-2 border border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] text-[9px] uppercase font-bold hover:bg-[var(--accent-color)]/25 transition-colors cursor-pointer flex items-center gap-1 disabled:opacity-40"
            >
              <Plus :size="10" />
              <span>Shell</span>
            </button>
          </AppTooltip>
        </div>

        <!-- Quick Filter Search Input -->
        <div class="px-2 pb-2">
          <div class="ui-search-box h-6 px-1.5">
            <Search :size="10" class="ui-search-box__icon" />
            <AppInput
              v-model="searchQuery"
              type="text"
              placeholder="Filter scripts or commands..."
              class="!bg-transparent !border-none !h-auto !px-0 !py-0 text-[10px]"
            />
            <button
              v-if="searchQuery"
              type="button"
              class="text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer shrink-0"
              @click="searchQuery = ''"
            >
              <X :size="10" />
            </button>
          </div>
        </div>
      </div>

      <!-- Scripts List (Scrollable) -->
      <div class="flex-1 min-h-0 overflow-y-auto divide-y divide-[var(--border-color)]/40">
        <!-- ── SESSIONS: Interactive Shells & Platform Supervisor ── -->
          <div class="p-2">
          <div class="text-[8px] uppercase tracking-wider text-[var(--dim-color)] font-bold mb-1.5 px-1 flex items-center justify-between">
            <span>SESSIONS</span>
            <span class="text-[8px] opacity-40">{{ shellSessions.length }} shell{{ shellSessions.length !== 1 ? 's' : '' }}</span>
          </div>

          <!-- Dynamic shell instances -->
          <div
            v-for="session in shellSessions"
            :key="session.id"
            @click="selectTarget(session.id)"
            class="p-2 border transition-all cursor-pointer mb-1 flex items-center justify-between gap-2"
            :class="selectedTargetKey === session.id
              ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/10 text-[var(--fg-color)] shadow-sm'
              : 'border-[var(--border-color)] bg-[var(--bg-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)]/60 hover:text-[var(--fg-color)]'"
          >
            <div class="flex items-center gap-2 min-w-0">
              <span class="w-2 h-2 rounded-full shrink-0 bg-[var(--status-ok-color)] animate-pulse"></span>
              <div class="min-w-0">
                <div class="text-[10px] font-bold uppercase truncate flex items-center gap-1.5">
                  <span>{{ session.label }}</span>
                  <span class="text-[8px] px-1 py-0.1 border border-[var(--border-color)] text-[var(--dim-color)] font-normal">bash -i</span>
                </div>
                <div class="text-[9px] text-[var(--dim-color)] truncate">{{ session.id }}</div>
              </div>
            </div>

            <AppTooltip text="Close shell" position="left">
              <button
                @click.stop="closeShell(session)"
                class="p-1 border border-[var(--status-error-border)] text-[var(--status-error-color)]/70 hover:border-[var(--status-error-color)] hover:text-[var(--status-error-color)] transition-colors cursor-pointer shrink-0"
              >
                <Trash2 :size="9" />
              </button>
            </AppTooltip>
          </div>

          <!-- Empty state: no shells yet -->
          <div
            v-if="shellSessions.length === 0"
            class="text-[9px] text-[var(--dim-color)] italic p-2 border border-dashed border-[var(--border-color)] text-center"
          >
            No active shells — press <span class="text-[var(--accent-color)] font-bold">+ Shell</span> to open one
          </div>

          <!-- Supervisor Fallback Item -->
          <div
            @click="selectTarget('system')"
            class="mt-1 p-1.5 border transition-all cursor-pointer flex items-center justify-between gap-2 text-[9px]"
            :class="selectedTargetKey === 'system'
              ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/10 text-[var(--fg-color)]'
              : 'border-dashed border-[var(--border-color)] bg-[var(--bg-color)]/60 text-[var(--dim-color)] hover:border-[var(--accent-color)]/50 hover:text-[var(--fg-color)]'"
          >
            <div class="flex items-center gap-1.5 min-w-0">
              <span class="w-1.5 h-1.5 rounded-full bg-[var(--dim-color)] shrink-0"></span>
              <span class="uppercase font-bold truncate">PLATFORM: supervisor</span>
            </div>
            <span class="text-[8px] text-[var(--dim-color)]">system</span>
          </div>
        </div>

        <!-- ── SECTION 1: PERSISTENT SERVICES ── -->
        <div class="p-2">
          <div class="text-[8px] uppercase tracking-wider text-[var(--dim-color)] font-bold mb-1.5 px-1 flex items-center justify-between">
            <span>PERSISTENT SERVICES ({{ persistentScripts.length }})</span>
          </div>

          <div v-if="filteredPersistentScripts.length === 0" class="text-[9px] text-[var(--dim-color)] italic p-2">
            No persistent services configured.
          </div>

          <div
            v-for="svc in filteredPersistentScripts"
            :key="svc.id"
            @click="selectTarget(svc.id)"
            @contextmenu="openScriptContextMenu($event, svc)"
            class="p-2 border transition-all cursor-pointer mb-1.5 flex flex-col gap-1.5"
            :class="selectedTargetKey === svc.id
              ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/10 text-[var(--fg-color)] shadow-sm'
              : 'border-[var(--border-color)] bg-[var(--bg-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)]/60 hover:text-[var(--fg-color)]'"
          >
            <!-- Card Header -->
            <div class="flex items-center justify-between gap-1.5">
              <div class="flex items-center gap-1.5 min-w-0">
                <span
                  class="w-2 h-2 rounded-full shrink-0"
                  :class="isServiceRunning(svc.id) ? 'bg-[var(--status-ok-color)] animate-pulse' : 'bg-[var(--dim-color)]'"
                ></span>
                <span class="text-[10px] font-bold uppercase truncate tracking-tight text-[var(--fg-color)]">
                  {{ svc.name || svc.id }}
                </span>
                <span 
                  class="text-[7px] px-1 py-0.1 border"
                  :class="isServiceRunning(svc.id) ? 'border-[var(--status-ok-border)] text-[var(--status-ok-color)] font-bold' : 'border-[var(--border-color)] text-[var(--dim-color)]'"
                >
                  {{ isServiceRunning(svc.id) ? 'RUNNING' : 'STOPPED' }}
                </span>
                <AppTooltip v-if="svc.profiler" :text="`Runs wrapped by the ${svc.profiler} profiler engine`">
                  <span class="text-[7px] px-1 py-0.1 border border-[var(--accent-color)]/50 text-[var(--accent-color)] font-bold shrink-0">
                    {{ svc.profiler.toUpperCase() }}
                  </span>
                </AppTooltip>
              </div>

              <!-- Action Buttons -->
              <div class="flex items-center gap-1 shrink-0" @click.stop>
                <!-- Options Modal -->
                <AppTooltip v-if="svc.options?.length > 0" text="Configure launch flags">
                  <button
                    @click="openScriptConfig(svc)"
                    class="p-1 border border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] transition-colors cursor-pointer"
                  >
                    <Settings2 :size="9" />
                  </button>
                </AppTooltip>

                <!-- Restart Button -->
                <AppTooltip v-if="isServiceRunning(svc.id)" text="Restart service">
                  <button
                    @click="restartService(svc)"
                    :disabled="isServiceTransitioning(svc.id)"
                    class="p-1 border border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] transition-colors cursor-pointer disabled:opacity-40"
                  >
                    <RotateCw :size="9" :class="serviceTransitions[getServiceKey(svc.id)]?.mode === 'restarting' ? 'animate-spin text-[var(--accent-color)]' : ''" />
                  </button>
                </AppTooltip>

                <!-- Start / Stop Toggle -->
                <AppTooltip :text="isServiceRunning(svc.id) ? 'Stop service' : 'Start service'">
                  <button
                    v-if="!isServiceRunning(svc.id)"
                    @click="startService(svc)"
                    class="px-1.5 py-0.5 border border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)] hover:bg-[var(--status-ok-color)]/20 text-[8px] uppercase font-bold transition-colors cursor-pointer flex items-center gap-0.5"
                  >
                    <Play :size="8" />
                    <span>START</span>
                  </button>
                  <button
                    v-else
                    @click="stopService(svc)"
                    :disabled="isServiceTransitioning(svc.id)"
                    class="px-1.5 py-0.5 border border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] hover:bg-[var(--status-error-color)]/20 text-[8px] uppercase font-bold transition-colors cursor-pointer flex items-center gap-0.5 disabled:opacity-40"
                  >
                    <Square :size="8" />
                    <span>STOP</span>
                  </button>
                </AppTooltip>
              </div>
            </div>

            <!-- Command Line Preview -->
            <div class="text-[8px] text-[var(--dim-color)] font-mono truncate px-1 py-0.5 bg-[var(--ui-bg-card)]/60 border border-[var(--border-color)]/30">
              $ {{ svc.command }} {{ (svc.args || []).join(' ') }}
            </div>
          </div>
        </div>

        <!-- ── SECTION 2: DETERMINISTIC TASKS ── -->
        <div class="p-2">
          <div class="text-[8px] uppercase tracking-wider text-[var(--dim-color)] font-bold mb-1.5 px-1 flex items-center justify-between">
            <span>DETERMINISTIC TASKS ({{ deterministicTasks.length }})</span>
          </div>

          <div v-if="filteredDeterministicTasks.length === 0" class="text-[9px] text-[var(--dim-color)] italic p-2">
            No deterministic tasks declared.
          </div>

          <div
            v-for="task in filteredDeterministicTasks"
            :key="task.id"
            @click="selectTarget(task.id)"
            @contextmenu="openTaskContextMenu($event, task)"
            class="p-2 border transition-all cursor-pointer mb-1.5 flex flex-col gap-1.5"
            :class="selectedTargetKey === task.id
              ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/10 text-[var(--fg-color)] shadow-sm'
              : 'border-[var(--border-color)] bg-[var(--bg-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)]/60 hover:text-[var(--fg-color)]'"
          >
            <!-- Task Header -->
            <div class="flex items-center justify-between gap-1.5">
              <div class="flex items-center gap-1.5 min-w-0">
                <span
                  class="w-1.5 h-1.5 rounded-full shrink-0"
                  :class="runningTasks[task.id] ? 'bg-[var(--status-warn-color)] animate-pulse' : (lastTaskResults[task.id]?.status === 'completed' ? 'bg-[var(--status-ok-color)]' : 'bg-[var(--dim-color)]')"
                ></span>
                <span class="text-[10px] font-bold uppercase truncate text-[var(--fg-color)]">
                  {{ task.name || task.id }}
                </span>

                <!-- Exit code & duration pill -->
                <span
                  v-if="lastTaskResults[task.id]"
                  class="text-[7px] px-1 py-0.1 border"
                  :class="lastTaskResults[task.id].exit_code === 0 ? 'border-[var(--status-ok-border)] text-[var(--status-ok-color)]' : 'border-[var(--status-error-border)] text-[var(--status-error-color)]'"
                >
                  {{ lastTaskResults[task.id].exit_code === null ? 'NOT STARTED' : `EXIT ${lastTaskResults[task.id].exit_code} (${lastTaskResults[task.id].duration_ms}ms)` }}
                </span>
              </div>

              <!-- Action Controls -->
              <div class="flex items-center gap-1 shrink-0" @click.stop>
                <AppTooltip v-if="task.options?.length > 0" text="Configure task options">
                  <button
                    @click="openScriptConfig(task)"
                    class="p-1 border border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] transition-colors cursor-pointer"
                  >
                    <Settings2 :size="9" />
                  </button>
                </AppTooltip>

                <button
                  @click="runTask(task)"
                  :disabled="runningTasks[task.id]"
                  class="px-2 py-0.5 border border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] hover:bg-[var(--accent-color)]/25 text-[8px] uppercase font-bold transition-colors cursor-pointer flex items-center gap-1 disabled:opacity-40"
                >
                  <Loader2 v-if="runningTasks[task.id]" :size="8" class="animate-spin" />
                  <Play v-else :size="8" />
                  <span>{{ runningTasks[task.id] ? 'RUNNING' : 'RUN' }}</span>
                </button>
              </div>
            </div>

            <!-- Command Line Preview -->
            <div class="text-[8px] text-[var(--dim-color)] font-mono truncate px-1 py-0.5 bg-[var(--ui-bg-card)]/60 border border-[var(--border-color)]/30">
              $ {{ task.command }} {{ (task.args || []).join(' ') }}
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- ── RIGHT MAIN PANE: Interactive Terminal & Console Viewport ───────────── -->
    <div class="flex-1 flex flex-col min-w-0 h-full overflow-hidden bg-[#09090b]">
      <!-- Console Header Bar -->
      <div class="h-9 flex items-center justify-between px-3 border-b border-[var(--border-color)] ui-surface-title shrink-0 gap-2">
        <!-- Target Info -->
        <div class="flex items-center gap-2 min-w-0">
          <span
            class="w-2 h-2 rounded-full shrink-0"
            :class="currentTarget.status === 'running' || currentTarget.status === 'working' ? 'bg-[var(--status-ok-color)] animate-pulse' : 'bg-[var(--dim-color)]'"
          ></span>
          <span class="text-[11px] font-bold text-[var(--fg-color)] uppercase tracking-wider truncate">
            {{ currentTarget.name }}
          </span>
          <span 
            class="text-[8px] px-1.5 py-0.2 border uppercase font-mono tracking-tight"
            :class="currentTarget.status === 'running' || currentTarget.status === 'working' ? 'border-[var(--status-ok-border)] text-[var(--status-ok-color)]' : 'border-[var(--border-color)] text-[var(--dim-color)]'"
          >
            {{ currentTarget.status }}
          </span>
          <span v-if="currentTarget.command" class="text-[9px] text-[var(--dim-color)] hidden lg:inline truncate max-w-sm">
            {{ currentTarget.command }}
          </span>
        </div>

        <!-- Action Controls Toolbar -->
        <div class="flex items-center gap-1.5 shrink-0">
          <!-- Filter: ALL vs ERRORS -->
          <AppTooltip :text="filterErrorsOnly ? 'Showing Errors Only (Click to show all logs)' : 'Show All Logs (Click to show errors only)'">
            <button
              @click="toggleErrorFilter"
              class="h-6 px-2 text-[9px] uppercase tracking-tight border transition-colors cursor-pointer flex items-center gap-1"
              :class="filterErrorsOnly 
                ? 'border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)] font-bold' 
                : 'border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)]'"
            >
              <Filter :size="9" />
              <span>{{ filterErrorsOnly ? 'ERRORS' : 'ALL' }}</span>
            </button>
          </AppTooltip>

          <!-- Stick to Bottom (Autoscroll Lock) -->
          <AppTooltip :text="stickToBottom ? 'Autoscroll Locked (Follow output)' : 'Autoscroll Unlocked (Click to follow)'">
            <button
              @click="toggleStickToBottom"
              class="h-6 px-2 text-[9px] uppercase tracking-tight border transition-colors cursor-pointer flex items-center gap-1"
              :class="stickToBottom
                ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/20 text-[var(--accent-color)] font-bold'
                : 'border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)]'"
            >
              <ArrowDown :size="9" />
              <span>AUTO</span>
            </button>
          </AppTooltip>

          <!-- Clear Console Buffer -->
          <AppTooltip text="Clear console display buffer">
            <button
              @click="clearConsole"
              class="ui-icon-btn"
            >
              <Trash2 :size="11" />
            </button>
          </AppTooltip>

          <!-- Copy Console Output -->
          <AppTooltip text="Copy output to clipboard">
            <button
              @click="copyOutput"
              class="ui-icon-btn"
            >
              <Check v-if="copiedSuccess" :size="11" class="text-[var(--status-ok-color)]" />
              <Copy v-else :size="11" />
            </button>
          </AppTooltip>

          <!-- Interrupt / Stop (Ctrl+C) -->
          <AppTooltip v-if="currentTarget.status === 'running' || currentTarget.status === 'working'" text="Send SIGINT / Interrupt (Ctrl+C)">
            <button
              @click="handleInterruptProcess"
              class="h-6 px-2 border border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] hover:bg-[var(--status-error-color)]/25 text-[9px] uppercase font-bold transition-colors cursor-pointer flex items-center gap-1"
            >
              <Square :size="8" />
              <span>SIGINT</span>
            </button>
          </AppTooltip>
        </div>
      </div>

      <!-- Terminal Output Viewport with ANSI Color Rendering -->
      <div
        ref="terminalViewportRef"
        @scroll="handleViewportScroll"
        class="flex-1 p-3.5 overflow-y-auto font-mono text-[11px] leading-relaxed select-text text-[var(--fg-color)] whitespace-pre-wrap break-words bg-[#09090b]"
        v-html="formattedOutputHtml"
      ></div>

      <!-- ── BOTTOM BAR: Bidirectional Interactive Stdin Prompt ── -->
      <div class="px-3 py-2 border-t border-[var(--border-color)] bg-[var(--ui-bg-card)]/40 flex items-center gap-2 shrink-0">
        <!-- Monospace Command Prompt Indicator -->
        <div class="flex items-center gap-1 text-[10px] font-bold text-[var(--accent-color)] shrink-0 select-none">
          <span>{{ project.name }}</span>
          <span class="text-[var(--dim-color)]">$</span>
        </div>

        <!-- Command Input Field -->
        <div class="flex-1 relative flex items-center">
          <AppInput
            v-model="cmdInput"
            @keydown.enter="handleSendInput"
            @keydown.up.prevent="handleHistoryNavigation('up')"
            @keydown.down.prevent="handleHistoryNavigation('down')"
            type="text"
            :disabled="isSendingInput"
            :placeholder="currentTarget.category === 'shell' ? 'Enter bash command (e.g. ls, cargo check, git status)...' : `Send stdin to ${currentTarget.name}...`"
            class="!h-auto !py-1 text-[11px]"
          />
        </div>

        <!-- Send / Return Button -->
        <button
          @click="handleSendInput"
          :disabled="!cmdInput.trim() || isSendingInput"
          class="px-2.5 py-1 border border-[var(--accent-color)] bg-[var(--accent-color)]/20 text-[var(--accent-color)] hover:bg-[var(--accent-color)]/30 text-[9px] uppercase font-bold tracking-wider flex items-center gap-1 transition-colors cursor-pointer disabled:opacity-30 disabled:cursor-not-allowed shrink-0"
        >
          <CornerDownLeft :size="10" />
          <span>Enter</span>
        </button>
      </div>
    </div>

    <!-- ── SCRIPT LAUNCH MODAL (Configurable flags & options) ── -->
    <ScriptLaunchModal
      :show="modalShow"
      :project="project"
      :script="modalScript"
      :initial-options="modalScript ? selectedOptions[modalScript.id] : undefined"
      :is-running="modalScript ? (modalScript.is_persistent ? isServiceRunning(modalScript.id) : !!runningTasks[modalScript.id]) : false"
      @close="modalShow = false"
      @launch="onModalLaunch"
      @save="onModalSave"
    />

    <!-- ── PORT CONFLICT RESOLUTION MODAL ── -->
    <div
      v-if="portConflict"
      class="fixed inset-0 ui-overlay-backdrop flex items-center justify-center z-50 p-4 font-mono select-none"
      @click.self="cancelPortConflict"
    >
      <div class="bg-[var(--bg-color)] border border-[var(--status-warn-border,rgba(234,179,8,0.4))] p-5 max-w-md w-full flex flex-col gap-4 shadow-2xl">
        <!-- Header -->
        <div class="flex justify-between items-center border-b border-[var(--border-color)] pb-3">
          <div class="flex items-center gap-2">
            <AlertTriangle class="text-[var(--status-warn-color,#eab308)] shrink-0" :size="16" />
            <h3 class="text-xs font-bold uppercase tracking-widest text-[var(--status-warn-color,#eab308)]">
              Port Conflict
            </h3>
          </div>
          <AppTooltip text="Close">
            <button
              @click="cancelPortConflict"
              class="text-[var(--dim-color)] hover:text-[var(--fg-color)] transition-colors cursor-pointer"
            >
              <X :size="14" />
            </button>
          </AppTooltip>
        </div>

        <!-- Content -->
        <div class="flex flex-col gap-2.5 text-xs">
          <p class="text-[var(--fg-color)]">
            Cannot start service <span class="font-bold text-white">{{ portConflict.service.name }}</span> because port <span class="font-bold text-[var(--accent-color)]">{{ portConflict.port }}</span> is currently occupied.
          </p>
          <div class="bg-[#0c0c0e] border border-[var(--border-color)] p-2.5 flex flex-col gap-1.5 text-[11px]">
            <div class="flex items-center gap-2 text-[var(--dim-color)]">
              <span class="text-[var(--dim-color)] uppercase tracking-wider text-[10px]">Occupant PID:</span>
              <span class="font-bold text-[var(--fg-color)]">{{ portConflict.pid }}</span>
            </div>
            <div class="flex flex-col gap-0.5 text-[var(--dim-color)]">
              <span class="text-[var(--dim-color)] uppercase tracking-wider text-[10px]">Command:</span>
              <span class="font-mono text-[var(--fg-color)] break-all">{{ portConflict.cmdline }}</span>
            </div>
          </div>
          <p class="text-[11px] text-[var(--dim-color)]">
            Terminate the occupying process and start the service?
          </p>
        </div>

        <!-- Actions -->
        <div class="flex items-center justify-end gap-2 border-t border-[var(--border-color)] pt-3">
          <button
            @click="cancelPortConflict"
            :disabled="isReleasingPort"
            class="px-3 py-1.5 border border-[var(--border-color)] bg-transparent hover:bg-white/5 text-[var(--dim-color)] hover:text-[var(--fg-color)] text-xs font-mono uppercase tracking-wider transition-colors cursor-pointer disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            @click="resolvePortConflict"
            :disabled="isReleasingPort"
            class="px-3 py-1.5 border border-[var(--status-error-border,rgba(244,63,94,0.4))] bg-[var(--status-error-bg,rgba(244,63,94,0.1))] hover:opacity-90 text-[var(--status-error-color,#f43f5e)] text-xs font-mono uppercase font-bold tracking-wider flex items-center gap-1.5 transition-colors cursor-pointer disabled:opacity-50"
          >
            <Loader2 v-if="isReleasingPort" class="animate-spin" :size="12" />
            <Square v-else :size="12" />
            <span>{{ isReleasingPort ? 'Releasing...' : 'Kill Process & Start' }}</span>
          </button>
        </div>
      </div>
    </div>

    <!-- ── CONTEXT MENU ── -->
    <AppContextMenu
      :show="contextMenuVisible"
      :x="contextMenuX"
      :y="contextMenuY"
      :items="contextMenuItems"
      @close="contextMenuVisible = false"
    />
  </div>
</template>
