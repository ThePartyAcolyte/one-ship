<script setup lang="ts">
import { ref, watch, nextTick, onMounted, onUnmounted, computed } from 'vue';
import { X, History, Search, Check, Bot, Trash2, ChevronDown, Loader2 } from '@lucide/vue';
import { AppTooltip, AppSelect, AppInput } from '../ui';
import { useI18n } from 'vue-i18n';
import { onUiEvent } from '../../utils/uiEvents';
import { daemonCall } from '../../utils/daemonClient';
import type { SystemLogRecord } from '../../types';
import { useCoreStore } from '../../stores/coreStore';
import { useWorkspaceLayoutStore } from '../../stores/workspaceLayoutStore';

defineProps<{
  statusLogs: string[];
  hideCollapsed?: boolean;
}>();

const { t } = useI18n();
const coreStore = useCoreStore();
const layoutStore = useWorkspaceLayoutStore();
const isExpanded = computed({
  get: () => layoutStore.isStatusLogOpen,
  set: (val: boolean) => layoutStore.setStatusLogOpen(val),
});

const dbLogs = ref<SystemLogRecord[]>([]);
const limit = 80;
const MAX_LOGS_IN_MEMORY = 1000;
const MAX_AUTO_FILL_PAGES = 5;
const hasMoreBackward = ref(true);
const isLoadingBackward = ref(false);
const isInitialLoading = ref(false);
const logContainer = ref<HTMLElement | null>(null);
const searchInputRef = ref<{ focus: () => void } | null>(null);

// Filtering
const searchQuery = ref('');
const filterLevel = ref('ALL');
const filterOrigin = ref('all');
const filterSource = ref('ALL');
const availableSources = ref<string[]>([]);

const levelOptions = [
  { label: 'ALL', value: 'ALL' },
  { label: 'DEBUG', value: 'DEBUG' },
  { label: 'INFO', value: 'INFO' },
  { label: 'WARN', value: 'WARN' },
  { label: 'ERROR', value: 'ERROR' },
];

const originOptions = computed(() => {
  const opts = [
    { label: 'ALL ORIGINS', value: 'all' },
    { label: 'SYSTEM CORE', value: 'system' },
  ];
  for (const p of coreStore.projects) {
    opts.push({
      label: p.name.toUpperCase(),
      value: `project:${p.id}`,
    });
  }
  return opts;
});

const SUBSYSTEM_LABELS: Record<string, string> = {
  db: 'Database',
  duckdb: 'DuckDB',
  telemetry: 'Telemetry',
  watcher: 'Watcher',
  supervisor: 'Supervisor',
  orchestrator: 'Orchestrator',
  voice_controller: 'Voice Controller',
  api: 'Daemon API',
  mcp: 'MCP Server',
  registry: 'Registry',
  git: 'Git',
  runner: 'Runner',
  semantic: 'Semantic Search',
};

const sourceOptions = computed(() => {
  const merged = new Set<string>([
    'telemetry',
    'watcher',
    'db',
    'supervisor',
    'orchestrator',
    'voice_controller',
    ...availableSources.value,
  ]);
  const sorted = Array.from(merged).sort((a, b) => a.localeCompare(b));
  return [
    { label: 'ALL SUBSYSTEMS', value: 'ALL' },
    ...sorted.map((s) => ({
      label: (SUBSYSTEM_LABELS[s] ?? s).toUpperCase(),
      value: s,
    })),
  ];
});

const oldestId = computed(() => dbLogs.value[0]?.id ?? null);
const latestId = computed(() => dbLogs.value[dbLogs.value.length - 1]?.id ?? null);

let searchTimeout: ReturnType<typeof setTimeout> | null = null;

async function fetchLogSources() {
  try {
    availableSources.value = await daemonCall('get_system_log_sources', {});
  } catch (e) {
    console.warn('[RollingStatusLog] Failed to fetch log sources:', e);
  }
}

function buildFilterParams() {
  const sq = searchQuery.value.trim();
  return {
    origin: filterOrigin.value === 'all' ? null : filterOrigin.value,
    source: filterSource.value === 'ALL' ? null : filterSource.value,
    searchQuery: sq.length > 0 ? sq : null,
    level: filterLevel.value === 'ALL' ? null : filterLevel.value,
  };
}

// Initial or filter-reset fetch
async function initialFetch() {
  isInitialLoading.value = true;
  hasMoreBackward.value = true;
  try {
    const logs: SystemLogRecord[] = await daemonCall('get_system_logs', {
      ...buildFilterParams(),
      limitCount: limit,
      beforeId: null,
      afterId: null,
    });

    // get_system_logs returns DESC (newest first). Reverse to display chronologically.
    dbLogs.value = logs.reverse();
    hasMoreBackward.value = logs.length === limit;

    await nextTick();
    if (logContainer.value) {
      logContainer.value.scrollTop = logContainer.value.scrollHeight;
    }

    // Ensure the container overflows so the user can scroll up to load older logs.
    let fillAttempts = 0;
    while (
      isExpanded.value &&
      hasMoreBackward.value &&
      fillAttempts < MAX_AUTO_FILL_PAGES &&
      logContainer.value &&
      logContainer.value.scrollHeight <= logContainer.value.clientHeight + 20
    ) {
      fillAttempts++;
      await loadOlderLogs();
      await nextTick();
    }

    if (logContainer.value) {
      logContainer.value.scrollTop = logContainer.value.scrollHeight;
    }
  } catch (e) {
    console.error('Failed to fetch logs:', e);
  } finally {
    isInitialLoading.value = false;
  }
}

async function handleClearLogs() {
  try {
    await daemonCall('clear_system_logs', {});
    dbLogs.value = [];
    hasMoreBackward.value = false;
    coreStore.statusLogs = [];
  } catch (e) {
    console.error('Failed to clear logs:', e);
  }
}

// Backward lazy load (scrolling up)
async function loadOlderLogs() {
  if (isLoadingBackward.value || !hasMoreBackward.value || !oldestId.value) return;
  isLoadingBackward.value = true;
  try {
    const older: SystemLogRecord[] = await daemonCall('get_system_logs', {
      ...buildFilterParams(),
      limitCount: limit,
      beforeId: oldestId.value,
      afterId: null,
    });

    if (older.length < limit) {
      hasMoreBackward.value = false;
    }

    if (older.length > 0) {
      const chronological = older.reverse();
      const oldScrollHeight = logContainer.value?.scrollHeight || 0;
      const oldScrollTop = logContainer.value?.scrollTop || 0;

      let merged = [...chronological, ...dbLogs.value];
      if (merged.length > MAX_LOGS_IN_MEMORY) {
        merged = merged.slice(0, MAX_LOGS_IN_MEMORY);
      }
      dbLogs.value = merged;

      await nextTick();
      if (logContainer.value) {
        const newScrollHeight = logContainer.value.scrollHeight;
        logContainer.value.scrollTop = newScrollHeight - oldScrollHeight + oldScrollTop;
      }
    }
  } catch (e) {
    console.error('Failed to load older logs:', e);
  } finally {
    isLoadingBackward.value = false;
  }
}

// Background polling for new logs (at bottom)
async function pollNewLogs() {
  if (!latestId.value || isInitialLoading.value) return;
  try {
    const newer: SystemLogRecord[] = await daemonCall('get_system_logs', {
      ...buildFilterParams(),
      limitCount: limit,
      beforeId: null,
      afterId: latestId.value,
    });

    if (newer.length > 0) {
      const isAtBottom = logContainer.value
        ? logContainer.value.scrollHeight - logContainer.value.scrollTop - logContainer.value.clientHeight < 60
        : true;

      let merged = [...dbLogs.value, ...newer];
      if (merged.length > MAX_LOGS_IN_MEMORY) {
        merged = merged.slice(merged.length - MAX_LOGS_IN_MEMORY);
      }
      dbLogs.value = merged;

      if (isAtBottom) {
        nextTick(() => {
          if (logContainer.value) {
            logContainer.value.scrollTop = logContainer.value.scrollHeight;
          }
        });
      }
    }
  } catch (err) {
    console.warn('[RollingStatusLog] Log polling failed:', err);
  }
}

function handleScroll(e: Event) {
  const target = e.target as HTMLElement;
  if (target.scrollTop <= 50 && !isInitialLoading.value) {
    loadOlderLogs();
  }
}

function triggerFilterUpdate() {
  hasMoreBackward.value = true;
  dbLogs.value = [];
  initialFetch();
}

watch([filterLevel, filterOrigin, filterSource], () => {
  if (isExpanded.value) triggerFilterUpdate();
});

watch(searchQuery, () => {
  if (isExpanded.value) {
    if (searchTimeout) clearTimeout(searchTimeout);
    searchTimeout = setTimeout(triggerFilterUpdate, 300);
  }
});

let expandRefreshInterval: ReturnType<typeof setInterval> | null = null;

watch(isExpanded, (newVal) => {
  showSelectionPill.value = false;
  showAgentDropdown.value = false;
  selectedLogText.value = '';
  if (newVal) {
    fetchLogSources();
    triggerFilterUpdate();
    expandRefreshInterval = setInterval(pollNewLogs, 2500);
    nextTick(() => {
      searchInputRef.value?.focus();
    });
  } else {
    if (expandRefreshInterval) clearInterval(expandRefreshInterval);
  }
});

function toggleLog() {
  isExpanded.value = !isExpanded.value;
}

// Copy-on-Select & Send-to-Agent State
const selectedLogText = ref('');
const copiedNotification = ref(false);
const copiedCharCount = ref(0);
const sentAgentNotification = ref<string | null>(null);
let copiedTimer: ReturnType<typeof setTimeout> | null = null;
let sentTimer: ReturnType<typeof setTimeout> | null = null;

const showSelectionPill = ref(false);
const selectionPillPos = ref({ top: 0, left: 0 });
const showAgentDropdown = ref(false);

const availableAgents = computed(() => {
  const agentsMap = new Map<
    string,
    { key: string; id: string; role: string; projectName: string; projectPath: string; status: string }
  >();
  for (const p of coreStore.projects) {
    const st = coreStore.agentStates[p.id];
    if (st?.agents) {
      for (const ag of st.agents) {
        const compositeKey = `${p.id}:${ag.id}`;
        if (!agentsMap.has(compositeKey)) {
          agentsMap.set(compositeKey, {
            key: compositeKey,
            id: ag.id,
            role: ag.role,
            projectName: p.name,
            projectPath: p.path,
            status: ag.status,
          });
        }
      }
    }
  }
  return Array.from(agentsMap.values());
});

async function handleLogSelection(e: MouseEvent) {
  const target = e.target as HTMLElement;
  if (target.closest('#log-selection-pill')) {
    return;
  }

  const selection = window.getSelection();
  if (!selection || selection.isCollapsed) {
    showSelectionPill.value = false;
    showAgentDropdown.value = false;
    selectedLogText.value = '';
    return;
  }

  const text = selection.toString().trim();
  if (!text) {
    showSelectionPill.value = false;
    showAgentDropdown.value = false;
    selectedLogText.value = '';
    return;
  }

  selectedLogText.value = text;
  copiedCharCount.value = text.length;

  try {
    await navigator.clipboard.writeText(text);
    copiedNotification.value = true;
    if (copiedTimer) clearTimeout(copiedTimer);
    copiedTimer = setTimeout(() => {
      copiedNotification.value = false;
    }, 2500);
  } catch (err) {
    console.error('Failed to copy to clipboard:', err);
  }

  try {
    const range = selection.getRangeAt(0);
    const rect = range.getBoundingClientRect();
    if (rect.width > 0 && rect.height > 0) {
      let top = rect.bottom + 6;
      let left = rect.left;

      if (top + 45 > window.innerHeight) {
        top = Math.max(10, rect.top - 38);
      }
      if (left + 260 > window.innerWidth) {
        left = Math.max(10, window.innerWidth - 270);
      }

      selectionPillPos.value = { top, left };
      showSelectionPill.value = true;
      showAgentDropdown.value = false;
    }
  } catch {
    // Ignore range calculation errors
  }
}

async function sendToAgent(agentId: string, projectPath: string) {
  if (!selectedLogText.value || !projectPath) return;
  try {
    const promptText = `[LOG SNIPPET from ONE-SHIP Log Viewer]\n\`\`\`\n${selectedLogText.value}\n\`\`\``;
    await daemonCall('prompt_agent', { projectPath, target: agentId, prompt: promptText });
    coreStore.addStatusLog(`Dispatched log snippet to agent '${agentId}'`);

    sentAgentNotification.value = agentId.toUpperCase();
    showSelectionPill.value = false;
    showAgentDropdown.value = false;

    if (sentTimer) clearTimeout(sentTimer);
    sentTimer = setTimeout(() => {
      sentAgentNotification.value = null;
    }, 3000);
  } catch (err: unknown) {
    console.error('Failed to send log snippet to agent:', err);
    const msg = err instanceof Error ? err.message : String(err);
    coreStore.addStatusLog(`Failed to send log snippet to ${agentId}: ${msg}`);
  }
}

function handleOpenStatusLog() {
  isExpanded.value = true;
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && isExpanded.value) {
    e.stopImmediatePropagation();
    e.preventDefault();
    isExpanded.value = false;
  }
}

let unlistenOpenStatusLog: (() => void) | null = null;

onMounted(() => {
  unlistenOpenStatusLog = onUiEvent('open-status-log', handleOpenStatusLog);
  window.addEventListener('keydown', handleKeydown, { capture: true });
});

onUnmounted(() => {
  if (unlistenOpenStatusLog) unlistenOpenStatusLog();
  window.removeEventListener('keydown', handleKeydown, { capture: true });
  if (expandRefreshInterval) clearInterval(expandRefreshInterval);
  if (searchTimeout) clearTimeout(searchTimeout);
  if (copiedTimer) clearTimeout(copiedTimer);
  if (sentTimer) clearTimeout(sentTimer);
});

function getLevelColor(level: string) {
  if (level === 'ERROR') return 'text-[var(--status-error-color)]';
  if (level === 'WARN') return 'text-[var(--status-warn-color)]';
  if (level === 'DEBUG') return 'text-[var(--status-info-color)] opacity-70';
  return 'text-[var(--accent-color)]';
}
</script>

<template>
  <div
    v-if="isExpanded"
    class="fixed inset-0 ui-overlay-backdrop z-50 p-4 flex items-center justify-center font-mono select-none"
    @click.self="toggleLog"
  >
    <div class="ui-surface-base border border-[var(--border-color)] w-full h-full max-w-6xl max-h-[88vh] flex flex-col shadow-2xl overflow-hidden text-[10px]">
      <!-- Top Search & Title Bar (Omnisearch-aligned) -->
      <div class="ui-search-box !border-x-0 !border-t-0 px-4 py-2.5 gap-3 shrink-0">
        <div class="flex items-center gap-2 text-[var(--dim-color)] font-bold text-xs uppercase tracking-widest shrink-0">
          <History :size="15" class="text-[var(--accent-color)]" />
          <span class="hidden sm:inline">{{ t('log.title') }}</span>
        </div>

        <div class="h-4 w-px bg-[var(--border-color)] shrink-0" />

        <Search :size="15" class="ui-search-box__icon" />
        <AppInput
          ref="searchInputRef"
          v-model="searchQuery"
          type="text"
          placeholder="Search system logs by message or subsystem..."
          class="!bg-transparent !border-none !h-auto !px-0 !py-0"
        />

        <!-- Feedback Badges & Actions -->
        <div class="flex items-center gap-2 shrink-0">
          <Loader2 v-if="isInitialLoading" class="w-3.5 h-3.5 animate-spin text-[var(--accent-color)]" />

          <span
            v-if="copiedNotification"
            class="text-[9px] px-2 py-0.5 border border-[var(--status-ok-border)] text-[var(--status-ok-color)] bg-[var(--status-ok-bg)] uppercase tracking-wider flex items-center gap-1"
          >
            <Check :size="10" />
            <span>COPIED {{ copiedCharCount }}</span>
          </span>

          <span
            v-if="sentAgentNotification"
            class="text-[9px] px-2 py-0.5 border border-[var(--accent-color)]/40 text-[var(--accent-color)] bg-[var(--accent-color)]/10 uppercase tracking-wider flex items-center gap-1"
          >
            <Bot :size="10" />
            <span>SENT TO {{ sentAgentNotification }}</span>
          </span>

          <AppTooltip text="Clear system logs" position="bottom">
            <button
              type="button"
              @click="handleClearLogs"
              class="ui-icon-btn ui-icon-btn--danger"
            >
              <Trash2 :size="12" />
            </button>
          </AppTooltip>

          <kbd class="ui-kbd">ESC</kbd>

          <AppTooltip :text="t('log.close')" position="left">
            <button
              type="button"
              @click="toggleLog"
              class="text-[var(--dim-color)] hover:text-[var(--fg-color)] transition-colors cursor-pointer"
            >
              <X :size="16" />
            </button>
          </AppTooltip>
        </div>
      </div>

      <!-- Compact Filter Bar -->
      <div class="flex flex-wrap items-center justify-between gap-2 px-4 py-2 border-b border-[var(--border-color)] bg-[var(--ui-bg-card)]/20 shrink-0">
        <!-- Level Filter Pills -->
        <div class="flex items-center gap-1.5 overflow-x-auto">
          <span class="text-[var(--dim-color)] text-[9px] uppercase tracking-wider mr-1">Level:</span>
          <button
            v-for="lvl in levelOptions"
            :key="lvl.value"
            type="button"
            @click="filterLevel = lvl.value"
            class="ui-filter-pill"
            :class="filterLevel === lvl.value ? 'ui-filter-pill--active' : ''"
          >
            {{ lvl.label }}
          </button>
        </div>

        <!-- Compact Origin & Subsystem Selects -->
        <div class="flex items-center gap-2 shrink-0">
          <div class="w-36">
            <AppSelect
              v-model="filterOrigin"
              :options="originOptions"
              compact
            />
          </div>
          <div class="w-40">
            <AppSelect
              v-model="filterSource"
              :options="sourceOptions"
              compact
            />
          </div>
        </div>
      </div>

      <!-- Body -->
      <div
        ref="logContainer"
        class="flex-1 overflow-y-auto p-4 flex flex-col leading-relaxed select-text ui-surface-list"
        @scroll="handleScroll"
        @mouseup="handleLogSelection"
      >
        <div v-if="!hasMoreBackward" class="text-center text-[var(--dim-color)] italic py-2 opacity-50 border-b border-black/10 mb-2">
          {{ t('log.end_of_logs') }}
        </div>
        <div v-else-if="isLoadingBackward" class="text-center text-[var(--accent-color)] py-2 animate-pulse font-mono text-[9px]">
          FETCHING OLDER LOGS...
        </div>
        <button
          v-else-if="dbLogs.length > 0"
          type="button"
          @click="loadOlderLogs"
          class="mx-auto mb-2 px-2.5 py-0.5 border border-[var(--border-color)] text-[9px] uppercase tracking-wider text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] transition-colors cursor-pointer"
        >
          Load older logs
        </button>

        <div v-if="isInitialLoading && dbLogs.length === 0" class="text-center text-[var(--accent-color)] py-4 animate-pulse">
          Fetching logs...
        </div>

        <div
          v-for="log in dbLogs"
          :key="log.id"
          class="text-[var(--fg-color)] break-words hover:bg-white/[0.02] px-1 -mx-1 rounded-sm select-text"
        >
          <span class="text-[var(--dim-color)] opacity-75 mr-2">[{{ log.timestamp.replace('T', ' ').substring(0, 19) }}]</span>
          <span :class="[getLevelColor(log.level), 'font-bold mr-2 uppercase tracking-wide']">[{{ log.level }}]</span>
          <span class="text-[var(--dim-color)] opacity-60 mr-2">[{{ log.source }}]</span>
          <span class="opacity-90">{{ log.message }}</span>
        </div>
      </div>

      <!-- Footer Info -->
      <div class="ui-modal-footer">
        <div class="flex items-center gap-3">
          <span>Scroll up to load history</span>
          <span>Select text to copy or send to agent</span>
        </div>
        <span>{{ dbLogs.length }} log{{ dbLogs.length === 1 ? '' : 's' }} loaded</span>
      </div>

      <!-- Floating Selection Action Bar -->
      <div
        v-if="showSelectionPill && selectedLogText"
        id="log-selection-pill"
        class="fixed z-60 bg-[var(--bg-color)] border border-[var(--border-color)] shadow-2xl p-1 flex items-center gap-1.5 font-mono text-[10px] select-none"
        :style="{ top: `${selectionPillPos.top}px`, left: `${selectionPillPos.left}px` }"
      >
        <div class="flex items-center gap-1 text-[var(--status-ok-color)] bg-[var(--status-ok-bg)] border border-[var(--status-ok-border)] px-2 py-0.5">
          <Check :size="11" />
          <span>COPIED</span>
        </div>

        <div class="relative">
          <button
            type="button"
            @click.stop="showAgentDropdown = !showAgentDropdown"
            class="flex items-center gap-1 bg-black/40 hover:bg-[var(--accent-color)]/20 border border-[var(--border-color)] hover:border-[var(--accent-color)] text-[var(--fg-color)] hover:text-[var(--accent-color)] px-2 py-0.5 transition-colors cursor-pointer"
          >
            <Bot :size="11" />
            <span>SEND TO AGENT</span>
            <ChevronDown :size="10" />
          </button>

          <div
            v-if="showAgentDropdown"
            class="absolute left-0 mt-1 w-52 bg-[var(--ui-bg-title)] border border-[var(--border-color)] shadow-2xl flex flex-col py-1 z-70 max-h-48 overflow-y-auto font-mono text-[10px]"
          >
            <div v-if="availableAgents.length === 0" class="px-3 py-1.5 text-[var(--dim-color)] opacity-50 italic">
              No active agents available
            </div>
            <div
              v-for="ag in availableAgents"
              :key="ag.key"
              @click.stop="sendToAgent(ag.id, ag.projectPath)"
              class="px-2.5 py-1.5 hover:bg-[var(--accent-color)]/15 cursor-pointer flex flex-col gap-0.5 transition-colors border-b border-[var(--border-color)]/20 last:border-b-0"
            >
              <div class="flex items-center justify-between">
                <span class="font-bold text-[var(--accent-color)]">{{ ag.id }}</span>
                <span
                  class="text-[8px] px-1 py-0.1 border uppercase"
                  :class="ag.status === 'working' ? 'border-[var(--status-ok-border)] text-[var(--status-ok-color)]' : 'border-[var(--border-color)] text-[var(--dim-color)]'"
                >
                  {{ ag.status }}
                </span>
              </div>
              <span class="text-[8px] text-[var(--dim-color)] truncate">{{ ag.role }}</span>
            </div>
          </div>
        </div>

        <button
          type="button"
          @click.stop="showSelectionPill = false; showAgentDropdown = false"
          class="p-0.5 text-[var(--dim-color)] hover:text-[var(--fg-color)] transition-colors cursor-pointer"
        >
          <X :size="11" />
        </button>
      </div>
    </div>
  </div>
</template>
