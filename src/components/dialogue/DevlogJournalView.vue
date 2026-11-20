<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import {
  ScrollText,
  CheckSquare,
  Square,
  AlertTriangle,
  RotateCcw,
  Plus,
  X,
  Search,
  RefreshCw,
  Check,
  FileCode,
  Layers,
  ListTodo,
  ShieldAlert,
  Filter,
  GitCommitHorizontal,
} from '@lucide/vue';
import { marked } from 'marked';
import { useDialogueStore } from '../../stores/dialogueStore';
import { useJournalStore, type SessionJournalItem } from '../../stores/journalStore';
import { useProjectExplorerStore } from '../../stores/projectExplorerStore';
import { useAgentMetricsStore } from '../../stores/agentMetricsStore';
import { useCoreStore } from '../../stores/coreStore';
import { AppTooltip, AppInput, AppSelect, AppTextarea, SubtabBar, type SelectOption, type SubtabItem } from '../ui';
import { reportAsyncFailure } from '../../utils/reportAsyncFailure';
import { formatDate } from '../../utils/format';

const props = defineProps<{
  project?: {
    id: number;
    name: string;
    path: string;
  } | null;
}>();

const dialogueStore = useDialogueStore();
const store = useJournalStore();
const explorerStore = useProjectExplorerStore();
const metricsStore = useAgentMetricsStore();
const coreStore = useCoreStore();

// Local UI state
const searchQuery = ref('');
const isRefreshing = ref(false);

// Filter states
const devlogAgentFilter = ref<string>('all');
const todoAgentFilter = ref<string>('all');
const todoStatusFilter = ref<string>('all');
const todoPriorityFilter = ref<string>('all');
const debtStatusFilter = ref<string>('all');
const debtSeverityFilter = ref<string>('all');

// Available agents derived from records
const availableDevlogAgents = computed<string[]>(() => {
  const agents = new Set<string>();
  for (const j of store.sessionJournals) {
    if (j.agent_id) agents.add(j.agent_id);
  }
  return Array.from(agents).sort();
});

const availableTodoAgents = computed<string[]>(() => {
  const agents = new Set<string>();
  for (const t of store.projectTodos) {
    if (t.agent_id) agents.add(t.agent_id);
  }
  return Array.from(agents).sort();
});

// Creation forms visibility
const showNewTodoForm = ref(false);
const showNewDebtForm = ref(false);

// New Todo Model
const newTodoTitle = ref('');
const newTodoDesc = ref('');
const newTodoCategory = ref('feature');
const newTodoPriority = ref('2');

// New Debt Model
const newDebtTitle = ref('');
const newDebtImpact = ref('');
const newDebtRemediation = ref('');
const newDebtSeverity = ref('medium');

// Current project
const currentProject = computed(() => {
  return props.project ?? coreStore.activeProject ?? null;
});

// Counts for sub-tabs
const devlogCount = computed(() => store.sessionJournals.length);
const pendingTasksCount = computed(() =>
  store.projectTodos.filter(t => t.status !== 'completed' && t.status !== 'cancelled').length
);
const openDebtCount = computed(() =>
  store.projectTechnicalDebt.filter(d => d.status !== 'resolved').length
);

const journalTabItems = computed<SubtabItem[]>(() => [
  { id: 'devlog', label: 'Devlogs', icon: ScrollText, badge: devlogCount.value },
  { id: 'todos', label: 'Tasks', icon: ListTodo, badge: pendingTasksCount.value },
  { id: 'debt', label: 'Tech Debt', icon: ShieldAlert, badge: openDebtCount.value },
]);

const journalSearchPlaceholder = computed<string>(() => {
  if (store.journalSubTab === 'todos') return 'Filter tasks...';
  if (store.journalSubTab === 'debt') return 'Filter technical debt...';
  return 'Filter devlogs...';
});

const showFiltersMenu = ref(false);
const filterMenuRef = ref<HTMLElement | null>(null);

interface JournalFilterOption {
  label: string;
  active: boolean;
  select: () => void;
}

interface JournalFilterGroup {
  label: string;
  options: JournalFilterOption[];
}

const journalFilterGroups = computed<JournalFilterGroup[]>(() => {
  const tab = store.journalSubTab;
  const groups: JournalFilterGroup[] = [];

  if (tab === 'devlog') {
    groups.push({
      label: 'Agent',
      options: [
        { label: 'All', active: devlogAgentFilter.value === 'all', select: () => { devlogAgentFilter.value = 'all'; } },
        ...availableDevlogAgents.value.map((aid) => ({
          label: aid,
          active: devlogAgentFilter.value.toLowerCase() === aid.toLowerCase(),
          select: () => { devlogAgentFilter.value = aid; },
        })),
      ],
    });
    return groups;
  }

  if (tab === 'todos') {
    groups.push({
      label: 'Status',
      options: ['all', 'pending', 'completed'].map((st) => ({
        label: st === 'all' ? 'All' : st,
        active: todoStatusFilter.value === st,
        select: () => { todoStatusFilter.value = st; },
      })),
    });
    if (availableTodoAgents.value.length > 0) {
      groups.push({
        label: 'Agent',
        options: [
          { label: 'All', active: todoAgentFilter.value === 'all', select: () => { todoAgentFilter.value = 'all'; } },
          ...availableTodoAgents.value.map((aid) => ({
            label: aid,
            active: todoAgentFilter.value.toLowerCase() === aid.toLowerCase(),
            select: () => { todoAgentFilter.value = aid; },
          })),
        ],
      });
    }
    groups.push({
      label: 'Priority',
      options: ['all', '1', '2', '3'].map((pr) => ({
        label: pr === 'all' ? 'All' : `P${pr}`,
        active: todoPriorityFilter.value === pr,
        select: () => { todoPriorityFilter.value = pr; },
      })),
    });
    return groups;
  }

  groups.push({
    label: 'Status',
    options: ['all', 'open', 'resolved'].map((st) => ({
      label: st === 'all' ? 'All' : st,
      active: debtStatusFilter.value === st,
      select: () => { debtStatusFilter.value = st; },
    })),
  });
  groups.push({
    label: 'Severity',
    options: ['all', 'critical', 'high', 'medium', 'low'].map((sev) => ({
      label: sev === 'all' ? 'All' : sev,
      active: debtSeverityFilter.value === sev,
      select: () => { debtSeverityFilter.value = sev; },
    })),
  });
  return groups;
});

const hasActiveJournalFilter = computed<boolean>(() => {
  if (store.journalSubTab === 'devlog') return devlogAgentFilter.value !== 'all';
  if (store.journalSubTab === 'todos') {
    return todoStatusFilter.value !== 'all'
      || todoPriorityFilter.value !== 'all'
      || todoAgentFilter.value !== 'all';
  }
  return debtStatusFilter.value !== 'all' || debtSeverityFilter.value !== 'all';
});

function handleDocumentPointerDown(event: PointerEvent) {
  if (!showFiltersMenu.value) return;
  const target = event.target as Node | null;
  if (target && filterMenuRef.value?.contains(target)) return;
  showFiltersMenu.value = false;
}

// Filtered Devlogs
const filteredDevlogs = computed(() => {
  const q = searchQuery.value.trim().toLowerCase();
  const selectedAgent = devlogAgentFilter.value.toLowerCase();
  return store.sessionJournals.filter(j => {
    if (selectedAgent !== 'all' && j.agent_id.toLowerCase() !== selectedAgent) {
      return false;
    }
    if (q) {
      const match =
        j.title.toLowerCase().includes(q) ||
        j.summary.toLowerCase().includes(q) ||
        j.agent_id.toLowerCase().includes(q) ||
        (j.tags && j.tags.toLowerCase().includes(q)) ||
        (j.files_touched && j.files_touched.toLowerCase().includes(q));
      if (!match) return false;
    }
    return true;
  });
});

// Filtered Tasks
const filteredTodos = computed(() => {
  const q = searchQuery.value.trim().toLowerCase();
  const selectedAgent = todoAgentFilter.value.toLowerCase();
  return store.projectTodos.filter(t => {
    if (selectedAgent !== 'all' && (t.agent_id?.toLowerCase() || '') !== selectedAgent) {
      return false;
    }
    if (todoStatusFilter.value !== 'all' && t.status !== todoStatusFilter.value) {
      return false;
    }
    if (todoPriorityFilter.value !== 'all' && String(t.priority) !== todoPriorityFilter.value) {
      return false;
    }
    if (q) {
      const match =
        t.title.toLowerCase().includes(q) ||
        (t.description && t.description.toLowerCase().includes(q)) ||
        t.category.toLowerCase().includes(q) ||
        (t.agent_id && t.agent_id.toLowerCase().includes(q));
      if (!match) return false;
    }
    return true;
  });
});

// Filtered Technical Debt
const filteredDebt = computed(() => {
  const q = searchQuery.value.trim().toLowerCase();
  return store.projectTechnicalDebt.filter(d => {
    if (debtStatusFilter.value !== 'all' && d.status !== debtStatusFilter.value) {
      return false;
    }
    if (debtSeverityFilter.value !== 'all' && d.severity !== debtSeverityFilter.value) {
      return false;
    }
    if (q) {
      const match =
        d.title.toLowerCase().includes(q) ||
        d.impact_summary.toLowerCase().includes(q) ||
        (d.remediation_plan && d.remediation_plan.toLowerCase().includes(q));
      if (!match) return false;
    }
    return true;
  });
});

// Options for selects
const todoCategoryOptions: SelectOption[] = [
  { label: 'Feature', value: 'feature' },
  { label: 'Bugfix', value: 'fix' },
  { label: 'Refactor', value: 'refactor' },
  { label: 'Documentation', value: 'docs' },
  { label: 'Testing', value: 'test' },
];

const todoPriorityOptions: SelectOption[] = [
  { label: 'P1 - High', value: '1' },
  { label: 'P2 - Medium', value: '2' },
  { label: 'P3 - Low', value: '3' },
];

const debtSeverityOptions: SelectOption[] = [
  { label: 'Critical', value: 'critical' },
  { label: 'High', value: 'high' },
  { label: 'Medium', value: 'medium' },
  { label: 'Low', value: 'low' },
];

// Helper parsers
function parseJsonOrList(val?: string): string[] {
  if (!val) return [];
  try {
    const parsed = JSON.parse(val);
    if (Array.isArray(parsed)) return parsed.map(String);
  } catch {
    // Not JSON
  }
  return val
    .split(/[\n,]+/)
    .map(s => s.trim())
    .filter(Boolean);
}

function renderMarkdown(content?: string): string {
  if (!content) return '';
  try {
    return marked.parse(content) as string;
  } catch {
    return content;
  }
}

// Actions
async function refreshAll() {
  const pPath = currentProject.value?.path;
  if (!pPath) return;
  isRefreshing.value = true;
  try {
    await Promise.all([
      store.fetchSessionJournals(pPath),
      store.fetchProjectTodos(pPath),
      store.fetchProjectTechnicalDebt(pPath),
    ]);
  } finally {
    isRefreshing.value = false;
  }
}

onMounted(() => {
  refreshAll();
  document.addEventListener('pointerdown', handleDocumentPointerDown);
});

onUnmounted(() => {
  document.removeEventListener('pointerdown', handleDocumentPointerDown);
});

watch(
  () => store.journalSubTab,
  () => {
    showFiltersMenu.value = false;
  }
);

watch(
  () => currentProject.value?.path,
  (newPath) => {
    if (newPath) {
      refreshAll();
    }
  }
);

function openTouchedFile(relPath: string) {
  const pPath = currentProject.value?.path;
  if (!pPath || !relPath) return;
  explorerStore.selectProjectDocument(pPath, relPath);
}

async function handleReopenConversation(journal: SessionJournalItem) {
  if (!journal.conversation_id) throw new Error(`Minutes #${journal.id} are not attached to a conversation`);
  const pPath = currentProject.value?.path;
  await dialogueStore.reopenConversation(journal.conversation_id, journal.agent_id, pPath);
}

async function submitNewTodo() {
  const pPath = currentProject.value?.path;
  if (!newTodoTitle.value.trim() || !pPath) return;
  await store.createProjectTodo(
    newTodoTitle.value.trim(),
    newTodoCategory.value,
    parseInt(newTodoPriority.value, 10) || 2,
    newTodoDesc.value.trim() || undefined,
    pPath
  );
  newTodoTitle.value = '';
  newTodoDesc.value = '';
  showNewTodoForm.value = false;
}

async function submitNewDebt() {
  const pPath = currentProject.value?.path;
  if (!newDebtTitle.value.trim() || !newDebtImpact.value.trim() || !pPath) return;
  await store.recordTechnicalDebt(
    newDebtTitle.value.trim(),
    newDebtImpact.value.trim(),
    newDebtRemediation.value.trim() || undefined,
    newDebtSeverity.value,
    pPath
  );
  newDebtTitle.value = '';
  newDebtImpact.value = '';
  newDebtRemediation.value = '';
  showNewDebtForm.value = false;
}
</script>

<template>
  <div class="flex-1 flex flex-col min-w-0 h-full overflow-hidden bg-[var(--bg-color)] font-mono text-[10px]">
    
    <!-- ── SUB-TAB HEADER BAR ─────────────────────────────────────── -->
    <SubtabBar
      :model-value="store.journalSubTab"
      @update:model-value="store.setJournalSubTab($event as 'devlog' | 'todos' | 'debt')"
      :items="journalTabItems"
    >
      <template #right>
        <div class="flex items-center gap-1">
          <AppTooltip v-if="store.journalSubTab === 'todos'" text="Add task item">
            <button
              type="button"
              class="ui-icon-btn"
              @click="showNewTodoForm = !showNewTodoForm"
            >
              <Plus :size="11" />
            </button>
          </AppTooltip>

          <AppTooltip v-if="store.journalSubTab === 'debt'" text="Record technical debt">
            <button
              type="button"
              class="ui-icon-btn"
              @click="showNewDebtForm = !showNewDebtForm"
            >
              <Plus :size="11" />
            </button>
          </AppTooltip>

          <AppTooltip text="Refresh records">
            <button
              type="button"
              class="ui-icon-btn"
              @click="refreshAll"
            >
              <RefreshCw :size="11" :class="isRefreshing ? 'animate-spin' : ''" />
            </button>
          </AppTooltip>
        </div>
      </template>

      <template #center>
        <div class="flex items-center gap-1 w-full min-w-0">
          <Search :size="10" class="text-[var(--accent-color)] shrink-0" />
          <AppInput
            v-model="searchQuery"
            type="text"
            :placeholder="journalSearchPlaceholder"
            class="!bg-transparent !border-none !h-6 !px-0 !py-0 text-[10px] flex-1 min-w-0"
          />
          <button
            v-if="searchQuery"
            type="button"
            class="text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer shrink-0"
            @click="searchQuery = ''"
          >
            <X :size="10" />
          </button>

          <div ref="filterMenuRef" class="relative shrink-0">
            <AppTooltip :text="hasActiveJournalFilter ? 'Filters active' : 'Filter records'">
              <button
                type="button"
                class="h-6 w-6 flex items-center justify-center cursor-pointer transition-colors relative"
                :class="hasActiveJournalFilter
                  ? 'text-[var(--accent-color)]'
                  : 'text-[var(--dim-color)] hover:text-[var(--accent-color)]'"
                @click.stop="showFiltersMenu = !showFiltersMenu"
              >
                <Filter :size="11" />
                <span v-if="hasActiveJournalFilter" class="absolute top-1 right-1 w-1 h-1 bg-[var(--accent-color)]" />
              </button>
            </AppTooltip>

            <div
              v-if="showFiltersMenu"
              class="absolute right-0 top-full mt-1 z-50 w-56 max-h-80 overflow-y-auto border border-[var(--border-color)] ui-surface-card p-2 shadow-2xl font-mono select-none"
              @click.stop
            >
              <div class="text-[8px] uppercase tracking-wider font-bold text-[var(--dim-color)] px-1 pb-1 border-b border-[var(--border-color)]/40 mb-1">
                Filters
              </div>
              <div v-for="group in journalFilterGroups" :key="group.label" class="mb-1.5 last:mb-0">
                <div class="text-[8px] uppercase tracking-wider text-[var(--dim-color)] px-1 py-0.5">
                  {{ group.label }}
                </div>
                <button
                  v-for="option in group.options"
                  :key="option.label"
                  type="button"
                  class="w-full h-6 px-2 text-left text-[9px] uppercase tracking-wider border transition-colors cursor-pointer"
                  :class="option.active
                    ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] font-bold'
                    : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:bg-white/5'"
                  @click="option.select(); showFiltersMenu = false"
                >
                  {{ option.label }}
                </button>
              </div>
            </div>
          </div>
        </div>
      </template>
    </SubtabBar>

    <!-- ── TAB 1: DEVLOG JOURNAL VIEW ────────────────────────────── -->
    <div v-if="store.journalSubTab === 'devlog'" class="flex-1 flex flex-col min-h-0 overflow-hidden">

      <div class="flex-1 overflow-y-auto p-3 space-y-3">
      
      <!-- Empty state -->
      <div v-if="filteredDevlogs.length === 0" class="h-40 flex flex-col items-center justify-center gap-2 text-[var(--dim-color)] text-center p-4">
        <ScrollText :size="20" class="opacity-20" />
        <p class="text-[10px] uppercase tracking-widest opacity-60">
          {{ searchQuery ? 'No devlogs match filter' : 'No session journals recorded' }}
        </p>
        <p class="text-[8px] opacity-40 max-w-xs">
          Agents record minutes and milestones automatically via the record_session_minutes MCP tool.
        </p>
      </div>

      <!-- Journal Cards -->
      <div
        v-for="journal in filteredDevlogs"
        :key="journal.id || journal.created_at"
        class="border border-[var(--border-color)] ui-surface-card p-3 space-y-2 hover:border-[var(--accent-color)]/60 transition-colors"
      >
        <!-- Card Header -->
        <div class="flex items-start justify-between gap-2 border-b border-[var(--border-color)]/40 pb-2">
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-1.5 flex-wrap">
              <span class="px-1.5 py-0.5 border border-[var(--accent-color)]/50 text-[var(--accent-color)] bg-[var(--accent-color)]/10 text-[8px] uppercase font-bold tracking-tight">
                {{ journal.agent_id }}
              </span>
              <span
                v-if="journal.runner || journal.model"
                class="px-1.5 py-0.5 border border-[var(--border-color)] ui-surface-base text-[var(--dim-color)] text-[8px] font-mono uppercase tracking-tight"
              >
                {{ [journal.runner, journal.model].filter(Boolean).join(' · ') }}
              </span>
              <span
                v-if="journal.cost_dollars !== undefined && journal.cost_dollars !== null && journal.cost_dollars > 0"
                class="px-1.5 py-0.5 border border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)] text-[8px] font-mono font-bold tracking-tight"
              >
                ${{ journal.cost_dollars.toFixed(4) }}
              </span>
              <span
                v-else-if="journal.total_tokens !== undefined && journal.total_tokens !== null && journal.total_tokens > 0"
                class="px-1.5 py-0.5 border border-[var(--border-color)] ui-surface-base text-[var(--fg-color)] text-[8px] font-mono font-bold tracking-tight"
              >
                {{ journal.total_tokens.toLocaleString() }} tok
              </span>
              <AppTooltip
                v-if="journal.context_pct !== undefined && journal.context_pct !== null"
                :text="journal.total_tokens ? `Session tokens delta: ${journal.total_tokens.toLocaleString()}` : ''"
              >
                <span
                  class="px-1.5 py-0.5 border text-[8px] font-mono font-bold tracking-tight"
                  :class="journal.context_pct > metricsStore.contextThresholdPct
                    ? 'border-[var(--status-warn-border)] text-[var(--status-warn-color)] bg-[var(--status-warn-bg)]'
                    : 'border-[var(--status-ok-border)] text-[var(--status-ok-color)] bg-[var(--status-ok-bg)]'"
                >
                  Ctx: {{ journal.context_pct.toFixed(0) }}%
                </span>
              </AppTooltip>
              <h3 class="font-bold text-[11px] text-[var(--fg-color)] truncate">
                {{ journal.title }}
              </h3>
            </div>
            <div class="text-[8px] text-[var(--dim-color)] mt-1 flex items-center gap-2">
              <span>{{ formatDate(journal.created_at) }}</span>
              <template v-if="journal.commit_hash">
                <span class="opacity-40">•</span>
                <AppTooltip :text="`Auto-commit: ${journal.commit_hash}`">
                  <span class="inline-flex items-center gap-1 px-1.5 py-0.5 border border-[var(--status-info-border)] text-[var(--status-info-color)] bg-[var(--status-info-bg)] font-mono">
                    <GitCommitHorizontal :size="8" />
                    #{{ journal.commit_hash.slice(0, 7) }}
                  </span>
                </AppTooltip>
              </template>
              <span class="opacity-40">•</span>
              <AppTooltip v-if="journal.agy_conversation_id" :text="journal.agy_conversation_id">
                <span class="opacity-60 font-mono truncate max-w-[160px] inline-block">
                  agy: {{ journal.agy_conversation_id }}
                </span>
              </AppTooltip>
            </div>
          </div>

          <!-- Reopen Conversation Action Button -->
          <AppTooltip v-if="journal.conversation_id" text="Reopen this conversation: the agent continues it from a Brief">
            <button
              class="h-6 px-2 border text-[9px] uppercase tracking-wider font-bold transition-colors cursor-pointer flex items-center gap-1 shrink-0 border-[var(--border-color)] hover:border-[var(--accent-color)] text-[var(--dim-color)] hover:text-[var(--accent-color)]"
              @click="handleReopenConversation(journal).catch(reportAsyncFailure)"
            >
              <RotateCcw :size="9" />
              <span>Reopen</span>
            </button>
          </AppTooltip>
        </div>

        <!-- Summary -->
        <div
          class="markdown-body text-[10px] text-[var(--fg-color)] leading-relaxed select-text"
          v-html="renderMarkdown(journal.summary)"
        />

        <!-- Decisions Section -->
        <div v-if="journal.decisions" class="border-t border-[var(--border-color)]/30 pt-2">
          <div class="text-[8px] uppercase font-bold text-[var(--dim-color)] mb-1 flex items-center gap-1">
            <Layers :size="9" />
            <span>Decisions</span>
          </div>
          <div class="space-y-1 pl-1">
            <div
              v-for="decision in parseJsonOrList(journal.decisions)"
              :key="decision"
              class="flex items-start gap-1.5 text-[9px] text-[var(--fg-color)]"
            >
              <span class="text-[var(--accent-color)] shrink-0">•</span>
              <span>{{ decision }}</span>
            </div>
          </div>
        </div>

        <!-- Touched Files -->
        <div v-if="journal.files_touched" class="border-t border-[var(--border-color)]/30 pt-2">
          <div class="text-[8px] uppercase font-bold text-[var(--dim-color)] mb-1 flex items-center gap-1">
            <FileCode :size="9" />
            <span>Touched Files</span>
          </div>
          <div class="flex flex-wrap gap-1">
            <button
              v-for="file in parseJsonOrList(journal.files_touched)"
              :key="file"
              class="px-1.5 py-0.5 border border-[var(--status-info-border)] text-[var(--status-info-color)] bg-[var(--status-info-bg)] text-[8px] font-mono hover:border-[var(--status-info-color)] transition-colors cursor-pointer flex items-center gap-1"
              @click="openTouchedFile(file)"
            >
              <FileCode :size="8" />
              <span>{{ file }}</span>
            </button>
          </div>
        </div>

        <!-- Technical Debt linked in journal -->
        <div v-if="journal.technical_debt" class="border-t border-[var(--status-warn-border)] pt-2 bg-[var(--status-warn-bg)] -mx-3 px-3 py-1.5">
          <div class="text-[8px] uppercase font-bold text-[var(--status-warn-color)] mb-0.5 flex items-center gap-1">
            <AlertTriangle :size="9" />
            <span>Recorded Technical Debt</span>
          </div>
          <p class="text-[9px] text-[var(--status-warn-color)]/80">{{ journal.technical_debt }}</p>
        </div>

        <!-- Tags -->
        <div v-if="journal.tags" class="flex flex-wrap gap-1 pt-1 border-t border-[var(--border-color)]/20">
          <span
            v-for="tag in parseJsonOrList(journal.tags)"
            :key="tag"
            class="px-1 py-0.5 border border-[var(--border-color)]/60 text-[var(--dim-color)] text-[8px] font-mono"
          >
            #{{ tag.replace(/^#/, '') }}
          </span>
        </div>

      </div>

    </div>

    </div>

    <!-- ── TAB 2: TASKS (TODOS) VIEW ─────────────────────────────── -->
    <div v-else-if="store.journalSubTab === 'todos'" class="flex-1 flex flex-col min-h-0 overflow-hidden">

      <!-- Expandable Add Task Form -->
      <div v-if="showNewTodoForm" class="p-3 border-b border-[var(--border-color)] ui-surface-card space-y-2 shrink-0">
        <div class="flex items-center justify-between">
          <span class="uppercase font-bold text-[9px] text-[var(--accent-color)]">New Task</span>
          <button class="text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer" @click="showNewTodoForm = false">
            <X :size="11" />
          </button>
        </div>
        <AppInput v-model="newTodoTitle" placeholder="Task title…" class="!h-6 !text-[10px]" />
        <AppInput v-model="newTodoDesc" placeholder="Description (optional)…" class="!h-6 !text-[10px]" />
        <div class="grid grid-cols-2 gap-2">
          <AppSelect v-model="newTodoCategory" :options="todoCategoryOptions" compact class="[&>button]:h-6" />
          <AppSelect v-model="newTodoPriority" :options="todoPriorityOptions" compact class="[&>button]:h-6" />
        </div>
        <div class="flex justify-end gap-2 pt-1">
          <button
            class="h-7 px-3 border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] text-[9px] uppercase cursor-pointer transition-colors"
            @click="showNewTodoForm = false"
          >
            Cancel
          </button>
          <button
            class="h-7 px-3 border border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] font-bold text-[9px] uppercase cursor-pointer hover:bg-[var(--accent-color)]/25 transition-colors"
            @click="submitNewTodo"
          >
            Save Task
          </button>
        </div>
      </div>

      <!-- Task Items List -->
      <div class="flex-1 overflow-y-auto p-3 space-y-2">
        
        <!-- Empty state -->
        <div v-if="filteredTodos.length === 0" class="h-32 flex flex-col items-center justify-center gap-2 text-[var(--dim-color)] text-center p-4">
          <ListTodo :size="20" class="opacity-20" />
          <p class="text-[10px] uppercase tracking-widest opacity-60">
            {{ searchQuery || todoStatusFilter !== 'all' ? 'No tasks match filter' : 'No active tasks' }}
          </p>
        </div>

        <!-- Task Item -->
        <div
          v-for="item in filteredTodos"
          :key="item.id || item.created_at"
          class="border border-[var(--border-color)] p-2.5 ui-surface-card flex items-start gap-2.5 hover:border-[var(--accent-color)]/50 transition-colors"
          :class="item.status === 'completed' ? 'opacity-60' : ''"
        >
          <!-- Status Toggle Checkbox -->
          <button
            class="mt-0.5 text-[var(--dim-color)] hover:text-[var(--accent-color)] cursor-pointer shrink-0 transition-colors"
            @click="item.id && store.toggleTodoStatus(item.id, item.status)"
          >
            <CheckSquare v-if="item.status === 'completed'" :size="13" class="text-[var(--status-ok-color)]" />
            <Square v-else :size="13" />
          </button>

          <!-- Content -->
          <div class="flex-1 min-w-0 space-y-1">
            <div class="flex items-center gap-1.5 flex-wrap">
              <!-- Priority Pill -->
              <span
                class="px-1.5 py-0.5 border text-[8px] font-bold uppercase"
                :class="item.priority === 1
                  ? 'border-[var(--status-error-border)] text-[var(--status-error-color)] bg-[var(--status-error-bg)]'
                  : (item.priority === 2
                    ? 'border-[var(--status-warn-border)] text-[var(--status-warn-color)] bg-[var(--status-warn-bg)]'
                    : 'border-[var(--border-color)] text-[var(--dim-color)] bg-[var(--bg-color)]/60')"
              >
                P{{ item.priority }}
              </span>

              <!-- Category Pill -->
              <span class="px-1 py-0.5 border border-[var(--border-color)] text-[var(--dim-color)] text-[8px] uppercase">
                {{ item.category }}
              </span>

              <!-- Title -->
              <span
                class="font-bold text-[10px] text-[var(--fg-color)]"
                :class="item.status === 'completed' ? 'line-through opacity-70' : ''"
              >
                {{ item.title }}
              </span>
            </div>

            <!-- Description -->
            <p v-if="item.description" class="text-[9px] text-[var(--dim-color)] leading-normal pl-0.5">
              {{ item.description }}
            </p>

            <!-- Metadata footer -->
            <div class="text-[8px] text-[var(--dim-color)] opacity-70 flex items-center gap-2 pl-0.5">
              <span v-if="item.agent_id">agent: {{ item.agent_id }}</span>
              <span>created: {{ formatDate(item.created_at) }}</span>
              <span v-if="item.completed_at">completed: {{ formatDate(item.completed_at) }}</span>
            </div>
          </div>
        </div>

      </div>

    </div>

    <!-- ── TAB 3: TECHNICAL DEBT VIEW ────────────────────────────── -->
    <div v-else class="flex-1 flex flex-col min-h-0 overflow-hidden">

      <!-- Expandable Add Technical Debt Form -->
      <div v-if="showNewDebtForm" class="p-3 border-b border-[var(--border-color)] ui-surface-card space-y-2 shrink-0">
        <div class="flex items-center justify-between">
          <span class="uppercase font-bold text-[9px] text-[var(--status-warn-color)]">Record Technical Debt</span>
          <button class="text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer" @click="showNewDebtForm = false">
            <X :size="11" />
          </button>
        </div>
        <AppInput v-model="newDebtTitle" placeholder="Title (e.g., Unbounded memory buffer in telemetry)…" class="!h-6 !text-[10px]" />
        <AppSelect v-model="newDebtSeverity" :options="debtSeverityOptions" compact class="[&>button]:h-6" />
        <AppTextarea v-model="newDebtImpact" placeholder="Impact summary…" :rows="2" />
        <AppTextarea v-model="newDebtRemediation" placeholder="Remediation plan (optional)…" :rows="2" />
        <div class="flex justify-end gap-2 pt-1">
          <button
            class="h-7 px-3 border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] text-[9px] uppercase cursor-pointer transition-colors"
            @click="showNewDebtForm = false"
          >
            Cancel
          </button>
          <button
            class="h-7 px-3 border border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)] font-bold text-[9px] uppercase cursor-pointer hover:border-[var(--status-warn-color)] transition-colors"
            @click="submitNewDebt"
          >
            Record Debt
          </button>
        </div>
      </div>

      <!-- Debt Items List -->
      <div class="flex-1 overflow-y-auto p-3 space-y-2.5">
        
        <!-- Empty state -->
        <div v-if="filteredDebt.length === 0" class="h-32 flex flex-col items-center justify-center gap-2 text-[var(--dim-color)] text-center p-4">
          <ShieldAlert :size="20" class="opacity-20" />
          <p class="text-[10px] uppercase tracking-widest opacity-60">
            {{ searchQuery || debtStatusFilter !== 'all' ? 'No debt items match filter' : 'No technical debt recorded' }}
          </p>
        </div>

        <!-- Debt Card -->
        <div
          v-for="item in filteredDebt"
          :key="item.id || item.identified_at"
          class="border border-[var(--border-color)] p-3 ui-surface-card space-y-2 hover:border-[var(--accent-color)]/50 transition-colors"
          :class="item.status === 'resolved' ? 'opacity-50' : ''"
        >
          <!-- Card Header -->
          <div class="flex items-start justify-between gap-2">
            <div class="space-y-1 min-w-0 flex-1">
              <div class="flex items-center gap-1.5 flex-wrap">
                <!-- Severity Pill -->
                <span
                  class="px-1.5 py-0.5 border text-[8px] font-bold uppercase"
                  :class="item.severity === 'critical'
                    ? 'border-[var(--status-error-color)] text-[var(--status-error-color)] bg-[var(--status-error-bg)]'
                    : (item.severity === 'high'
                      ? 'border-[var(--status-warn-color)] text-[var(--status-warn-color)] bg-[var(--status-warn-bg)]'
                      : (item.severity === 'medium'
                        ? 'border-[var(--status-warn-border)] text-[var(--status-warn-color)] bg-[var(--status-warn-bg)]'
                        : 'border-[var(--border-color)] text-[var(--dim-color)] bg-[var(--bg-color)]/60'))"
                >
                  {{ item.severity }}
                </span>

                <!-- Status Pill -->
                <span
                  class="px-1 py-0.5 border text-[8px] font-mono uppercase"
                  :class="item.status === 'resolved'
                    ? 'border-[var(--status-ok-border)] text-[var(--status-ok-color)] bg-[var(--status-ok-bg)]'
                    : 'border-[var(--status-warn-border)] text-[var(--status-warn-color)] bg-[var(--status-warn-bg)]'"
                >
                  {{ item.status }}
                </span>

                <!-- Title -->
                <h4 class="font-bold text-[10px] text-[var(--fg-color)] truncate">
                  {{ item.title }}
                </h4>
              </div>

              <div class="text-[8px] text-[var(--dim-color)] opacity-70">
                Identified: {{ formatDate(item.identified_at) }}
                <span v-if="item.resolved_at"> • Resolved: {{ formatDate(item.resolved_at) }}</span>
              </div>
            </div>

            <!-- Resolve Action Button -->
            <AppTooltip v-if="item.status !== 'resolved'" text="Mark as resolved">
              <button
                class="h-6 px-2 border border-[var(--status-ok-border)] text-[var(--status-ok-color)] hover:bg-[var(--status-ok-bg)] text-[9px] uppercase tracking-wider font-bold transition-colors cursor-pointer flex items-center gap-1 shrink-0"
                @click="item.id && store.resolveTechnicalDebt(item.id)"
              >
                <Check :size="9" />
                <span>Resolve</span>
              </button>
            </AppTooltip>
          </div>

          <!-- Impact Summary -->
          <div class="text-[9px] text-[var(--fg-color)] leading-relaxed bg-[var(--bg-color)]/60 p-2 border-l border-[var(--status-warn-border)]">
            <span class="text-[8px] uppercase font-bold text-[var(--status-warn-color)] block mb-0.5">Impact</span>
            {{ item.impact_summary }}
          </div>

          <!-- Remediation Plan -->
          <div v-if="item.remediation_plan" class="text-[9px] text-[var(--dim-color)] leading-relaxed pl-2 border-l border-[var(--border-color)]">
            <span class="text-[8px] uppercase font-bold text-[var(--dim-color)] block mb-0.5">Remediation Plan</span>
            {{ item.remediation_plan }}
          </div>
        </div>

      </div>

    </div>

  </div>
</template>
