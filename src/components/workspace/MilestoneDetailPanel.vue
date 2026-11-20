<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue';
import {
  X,
  Flag,
  Save,
  Trash2,
  BookOpen,
  ListTodo,
  AlertTriangle,
  CheckSquare,
  MessageSquareShare,
  Loader2,
  Filter,
} from '@lucide/vue';
import {
  AppTooltip,
  AppInput,
  AppDatePicker,
  AppTextarea,
  AppSelect,
  AppEmptyState,
  type SelectOption,
} from '../ui';
import {
  useMilestoneStore,
  type MilestoneStatus,
} from '../../stores/milestoneStore';
import { useDialogueStore } from '../../stores/dialogueStore';
import { reportAsyncFailure } from '../../utils/reportAsyncFailure';
import type { ProjectMilestoneDetail } from '../../types/generated/ProjectMilestoneDetail';

const props = defineProps<{
  milestoneId: number;
  projectPath: string;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'selectTask', taskId: number): void;
  (e: 'filterByMilestone', milestoneId: number): void;
}>();

const milestoneStore = useMilestoneStore();
const dialogueStore = useDialogueStore();

type DetailTab = 'tasks' | 'minutes' | 'todos' | 'debt';
const activeTab = ref<DetailTab>('tasks');

const editTitle = ref('');
const editDescription = ref('');
const editStatus = ref<MilestoneStatus>('planned');
const editWeightPercent = ref('0');
const editHillPosition = ref('0');
const editAppetiteDays = ref('');
const editPlannedStart = ref('');
const editPlannedDue = ref('');
const isSaving = ref(false);
const confirmDelete = ref(false);

const statusOptions: SelectOption[] = [
  { label: 'Planned', value: 'planned' },
  { label: 'Active', value: 'active' },
  { label: 'Completed', value: 'completed' },
  { label: 'Archived', value: 'archived' },
];

const trackingMode = computed(() => milestoneStore.trackingMode);
const detail = computed<ProjectMilestoneDetail | null>(() => {
  const d = milestoneStore.selectedDetail;
  return d && d.summary.milestone.id === props.milestoneId ? d : null;
});

function epochToDateInput(epochSeconds: number | null): string {
  if (!epochSeconds) return '';
  const d = new Date(epochSeconds * 1000);
  const y = d.getUTCFullYear();
  const m = String(d.getUTCMonth() + 1).padStart(2, '0');
  const day = String(d.getUTCDate()).padStart(2, '0');
  return `${y}-${m}-${day}`;
}

function dateInputToEpoch(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  const parts = trimmed.split('-').map(Number);
  if (parts.length !== 3 || parts.some(Number.isNaN)) return null;
  return Math.floor(Date.UTC(parts[0], parts[1] - 1, parts[2], 12, 0, 0) / 1000);
}

function formatEpoch(epochSeconds: number): string {
  const d = new Date(epochSeconds * 1000);
  return d.toISOString().slice(0, 16).replace('T', ' ');
}

function syncForm(d: ProjectMilestoneDetail | null) {
  if (!d) return;
  const m = d.summary.milestone;
  editTitle.value = m.title;
  editDescription.value = m.description ?? '';
  editStatus.value = m.status as MilestoneStatus;
  editWeightPercent.value = String(m.weight_percent);
  editHillPosition.value = String(m.hill_position_percent);
  editAppetiteDays.value = m.appetite_days != null ? String(m.appetite_days) : '';
  editPlannedStart.value = epochToDateInput(m.planned_start_epoch_seconds);
  editPlannedDue.value = epochToDateInput(m.planned_due_epoch_seconds);
  confirmDelete.value = false;
}

watch(
  () => [props.milestoneId, props.projectPath] as const,
  ([mid, path]) => {
    void milestoneStore.fetchMilestoneDetail(path, mid);
  },
  { immediate: true }
);

watch(detail, syncForm, { immediate: true });

async function handleSave() {
  if (!detail.value || !editTitle.value.trim() || isSaving.value) return;
  isSaving.value = true;
  try {
    const parsedWeight = Number(editWeightPercent.value);
    const parsedHill = Number(editHillPosition.value);
    const parsedAppetite = editAppetiteDays.value.trim() ? Number(editAppetiteDays.value) : null;
    await milestoneStore.updateMilestone({
      projectPath: props.projectPath,
      milestoneId: props.milestoneId,
      title: editTitle.value.trim(),
      description: editDescription.value.trim() || null,
      status: editStatus.value,
      weightPercent: Number.isFinite(parsedWeight) ? Math.max(0, parsedWeight) : 0,
      hillPositionPercent: Number.isFinite(parsedHill) ? Math.min(100, Math.max(0, Math.round(parsedHill))) : 0,
      appetiteDays: parsedAppetite !== null && Number.isFinite(parsedAppetite) && parsedAppetite > 0
        ? Math.round(parsedAppetite)
        : null,
      plannedStartEpochSeconds: dateInputToEpoch(editPlannedStart.value),
      plannedDueEpochSeconds: dateInputToEpoch(editPlannedDue.value),
      sortOrder: detail.value.summary.milestone.sort_order,
    });
  } finally {
    isSaving.value = false;
  }
}

async function handleDelete() {
  if (!confirmDelete.value) {
    confirmDelete.value = true;
    return;
  }
  const ok = await milestoneStore.deleteMilestone(props.milestoneId, props.projectPath);
  if (ok) emit('close');
}

function handleReopenConversation(conversationId: string, agentId: string) {
  dialogueStore.reopenConversation(conversationId, agentId, props.projectPath).catch(reportAsyncFailure);
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.preventDefault();
    emit('close');
  }
}

onMounted(() => {
  window.addEventListener('keydown', handleKeydown);
});

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown);
});
</script>

<template>
  <div class="w-[460px] max-w-full shrink-0 h-full border-l border-[var(--border-color)] ui-surface-base flex flex-col font-mono min-h-0">
    <!-- Header -->
    <div class="px-3 py-2 border-b border-[var(--border-color)] ui-surface-title flex items-center justify-between gap-2 shrink-0">
      <div class="flex items-center gap-2 min-w-0">
        <Flag :size="13" class="text-[var(--accent-color)] shrink-0" />
        <span class="text-[10px] font-bold uppercase tracking-wider text-[var(--fg-color)] truncate">
          {{ detail?.summary.milestone.title ?? `Milestone #${milestoneId}` }}
        </span>
      </div>
      <div class="flex items-center gap-1 shrink-0">
        <AppTooltip text="Filter board by this milestone">
          <button
            type="button"
            @click="emit('filterByMilestone', milestoneId)"
            class="ui-icon-btn"
          >
            <Filter :size="12" />
          </button>
        </AppTooltip>
        <AppTooltip text="Close (Esc)">
          <button
            type="button"
            @click="emit('close')"
            class="ui-icon-btn"
          >
            <X :size="13" />
          </button>
        </AppTooltip>
      </div>
    </div>

    <div v-if="milestoneStore.isLoadingDetail && !detail" class="flex-1 flex items-center justify-center text-[10px] text-[var(--dim-color)] gap-2">
      <Loader2 :size="14" class="animate-spin" />
      <span>Loading milestone...</span>
    </div>

    <div v-else-if="detail" class="flex-1 overflow-y-auto min-h-0 flex flex-col">
      <!-- Summary & Drift Compass Strip -->
      <div class="p-3 border-b border-[var(--border-color)] ui-surface-card space-y-2">
        <div class="flex items-center justify-between gap-2 text-[10px]">
          <span class="text-[var(--dim-color)] uppercase">Progress</span>
          <span class="font-bold text-[var(--fg-color)]">
            {{ Math.round(detail.summary.progressPercent) }}%
            <span class="text-[var(--dim-color)] font-normal">
              ({{ detail.summary.tasksDoneCount }}/{{ detail.summary.tasksTotalCount }} tasks)
            </span>
          </span>
        </div>
        <div class="h-1.5 w-full bg-[var(--ui-bg-list)] border border-[var(--border-color)] overflow-hidden">
          <div
            class="h-full bg-[var(--accent-color)] transition-all"
            :style="{ width: `${Math.min(100, Math.max(0, detail.summary.progressPercent))}%` }"
          />
        </div>

        <!-- Drift Compass & Cross-Domain Counters -->
        <div class="flex flex-wrap items-center gap-1.5 pt-1">
          <span
            v-if="trackingMode === 'weighted'"
            class="px-1.5 py-0.5 text-[9px] border border-[var(--border-color)] text-[var(--fg-color)]"
          >
            Share: {{ detail.summary.effectiveWeightPercent.toFixed(1) }}%
          </span>
          <span
            v-if="detail.summary.scheduleDriftDays !== null && detail.summary.scheduleDriftDays > 0"
            class="px-1.5 py-0.5 text-[9px] border border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)]"
          >
            Drift: +{{ detail.summary.scheduleDriftDays }}d
          </span>
          <span
            v-else-if="detail.summary.scheduleDriftDays !== null"
            class="px-1.5 py-0.5 text-[9px] border border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)]"
          >
            On Schedule
          </span>
          <span
            v-if="detail.summary.uphillRiskFlag"
            class="px-1.5 py-0.5 text-[9px] border border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)]"
          >
            Uphill Risk
          </span>
          <span
            v-if="detail.summary.openTechDebtCount > 0"
            class="px-1.5 py-0.5 text-[9px] border border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)]"
          >
            Debt: {{ detail.summary.openTechDebtCount }}
          </span>
          <span class="px-1.5 py-0.5 text-[9px] border border-[var(--border-color)] text-[var(--dim-color)]">
            TODOs: {{ detail.summary.linkedTodosCompletedCount }}/{{ detail.summary.linkedTodosTotalCount }}
          </span>
          <span class="px-1.5 py-0.5 text-[9px] border border-[var(--border-color)] text-[var(--dim-color)]">
            Tokens: {{ detail.summary.tokensUsedCount.toLocaleString() }}
          </span>
        </div>
      </div>

      <!-- Editable Metadata -->
      <div class="p-3 border-b border-[var(--border-color)] space-y-2.5">
        <div>
          <label class="block text-[9px] uppercase tracking-wider text-[var(--dim-color)] mb-1">Title</label>
          <AppInput v-model="editTitle" />
        </div>

        <div class="grid grid-cols-2 gap-2">
          <div>
            <label class="block text-[9px] uppercase tracking-wider text-[var(--dim-color)] mb-1">Status</label>
            <AppSelect v-model="editStatus" :options="statusOptions" />
          </div>
          <div v-if="trackingMode === 'weighted'">
            <label class="block text-[9px] uppercase tracking-wider text-[var(--dim-color)] mb-1">Weight %</label>
            <AppInput v-model="editWeightPercent" type="number" />
          </div>
          <div v-else-if="trackingMode === 'shape_up'">
            <label class="block text-[9px] uppercase tracking-wider text-[var(--dim-color)] mb-1">Appetite (days)</label>
            <AppInput v-model="editAppetiteDays" type="number" />
          </div>
        </div>

        <div v-if="trackingMode === 'shape_up'">
          <div class="flex items-center justify-between text-[9px] uppercase text-[var(--dim-color)] mb-1">
            <span>Hill ({{ Number(editHillPosition) < 50 ? 'Uphill' : 'Downhill' }})</span>
            <span class="text-[var(--fg-color)] font-bold">{{ editHillPosition }}%</span>
          </div>
          <input
            v-model="editHillPosition"
            type="range"
            min="0"
            max="100"
            class="w-full accent-[var(--accent-color)]"
          />
        </div>

        <div class="grid grid-cols-2 gap-2">
          <div>
            <label class="block text-[9px] uppercase tracking-wider text-[var(--dim-color)] mb-1">Baseline Start</label>
            <AppDatePicker v-model="editPlannedStart" />
          </div>
          <div>
            <label class="block text-[9px] uppercase tracking-wider text-[var(--dim-color)] mb-1">Baseline Due</label>
            <AppDatePicker v-model="editPlannedDue" />
          </div>
        </div>

        <div>
          <label class="block text-[9px] uppercase tracking-wider text-[var(--dim-color)] mb-1">Objective / Acceptance</label>
          <AppTextarea v-model="editDescription" :rows="2" placeholder="Architectural or commercial scope..." />
        </div>

        <div class="flex items-center justify-between pt-1">
          <button
            type="button"
            @click="handleDelete"
            class="h-6 px-2 text-[9px] font-bold uppercase border border-[var(--status-error-border)] text-[var(--status-error-color)] hover:bg-[var(--status-error-bg)] transition-colors flex items-center gap-1"
          >
            <Trash2 :size="11" />
            <span>{{ confirmDelete ? 'Confirm Delete' : 'Delete' }}</span>
          </button>
          <button
            type="button"
            @click="handleSave"
            :disabled="isSaving || !editTitle.trim()"
            class="h-6 px-2.5 text-[9px] font-bold uppercase border border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] hover:bg-[var(--accent-color)]/25 disabled:opacity-40 transition-colors flex items-center gap-1"
          >
            <Save :size="11" />
            <span>Save</span>
          </button>
        </div>
      </div>

      <!-- Aggregated Evidence Subtabs -->
      <div class="flex border-b border-[var(--border-color)] ui-surface-title shrink-0">
        <button
          type="button"
          @click="activeTab = 'tasks'"
          class="flex-1 h-8 px-2 text-[9px] font-bold uppercase tracking-wider border-b-2 transition-colors flex items-center justify-center gap-1"
          :class="activeTab === 'tasks' ? 'border-[var(--accent-color)] text-[var(--accent-color)]' : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
        >
          <CheckSquare :size="11" />
          <span>Tasks ({{ detail.tasks.length }})</span>
        </button>
        <button
          type="button"
          @click="activeTab = 'minutes'"
          class="flex-1 h-8 px-2 text-[9px] font-bold uppercase tracking-wider border-b-2 transition-colors flex items-center justify-center gap-1"
          :class="activeTab === 'minutes' ? 'border-[var(--accent-color)] text-[var(--accent-color)]' : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
        >
          <BookOpen :size="11" />
          <span>Minutes ({{ detail.journalEntries.length }})</span>
        </button>
        <button
          type="button"
          @click="activeTab = 'todos'"
          class="flex-1 h-8 px-2 text-[9px] font-bold uppercase tracking-wider border-b-2 transition-colors flex items-center justify-center gap-1"
          :class="activeTab === 'todos' ? 'border-[var(--accent-color)] text-[var(--accent-color)]' : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
        >
          <ListTodo :size="11" />
          <span>TODOs ({{ detail.todos.length }})</span>
        </button>
        <button
          type="button"
          @click="activeTab = 'debt'"
          class="flex-1 h-8 px-2 text-[9px] font-bold uppercase tracking-wider border-b-2 transition-colors flex items-center justify-center gap-1"
          :class="activeTab === 'debt' ? 'border-[var(--accent-color)] text-[var(--accent-color)]' : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
        >
          <AlertTriangle :size="11" />
          <span>Debt ({{ detail.techDebt.length }})</span>
        </button>
      </div>

      <!-- Tab Content -->
      <div class="p-3 space-y-2 flex-1">
        <!-- Tasks -->
        <template v-if="activeTab === 'tasks'">
          <AppEmptyState v-if="detail.tasks.length === 0" title="No tasks assigned to this milestone" />
          <button
            v-for="b in detail.tasks"
            :key="b.task.id"
            type="button"
            @click="emit('selectTask', b.task.id)"
            class="w-full text-left p-2 border border-[var(--border-color)] ui-surface-card hover:border-[var(--accent-color)] transition-colors space-y-1"
          >
            <div class="flex items-center justify-between gap-2">
              <span class="text-[10px] font-bold text-[var(--fg-color)] truncate">{{ b.task.title }}</span>
              <span class="text-[8px] uppercase px-1 py-0.5 border border-[var(--border-color)] text-[var(--dim-color)] shrink-0">
                {{ b.task.status }}
              </span>
            </div>
            <div class="flex items-center gap-2 text-[9px] text-[var(--dim-color)]">
              <span class="uppercase">{{ b.task.domain }}</span>
              <span>· P{{ b.task.priority }}</span>
              <span v-if="b.task.weight_points > 1">· {{ b.task.weight_points }} pts</span>
            </div>
          </button>
        </template>

        <!-- Minutes (Direct + Transitive) -->
        <template v-else-if="activeTab === 'minutes'">
          <AppEmptyState v-if="detail.journalEntries.length === 0" title="No session minutes linked yet" />
          <div
            v-for="entry in detail.journalEntries"
            :key="entry.id ?? entry.created_at"
            class="p-2 border border-[var(--border-color)] ui-surface-card space-y-1.5"
          >
            <div class="flex items-start justify-between gap-2">
              <span class="text-[10px] font-bold text-[var(--fg-color)]">{{ entry.title }}</span>
              <AppTooltip v-if="entry.conversation_id" text="Reopen conversation">
                <button
                  type="button"
                  @click="handleReopenConversation(entry.conversation_id, entry.agent_id)"
                  class="ui-icon-btn shrink-0"
                >
                  <MessageSquareShare :size="11" />
                </button>
              </AppTooltip>
            </div>
            <p class="text-[10px] text-[var(--dim-color)] whitespace-pre-wrap leading-relaxed">{{ entry.summary }}</p>
            <div class="flex items-center justify-between text-[8px] text-[var(--dim-color)] uppercase">
              <span>@{{ entry.agent_id }}</span>
              <span>{{ formatEpoch(entry.created_at) }}</span>
            </div>
          </div>
        </template>

        <!-- Aggregated TODOs -->
        <template v-else-if="activeTab === 'todos'">
          <AppEmptyState v-if="detail.todos.length === 0" title="No TODOs linked via tasks" />
          <div
            v-for="todo in detail.todos"
            :key="todo.id ?? todo.title"
            class="p-2 border border-[var(--border-color)] ui-surface-card flex items-start justify-between gap-2"
          >
            <div class="min-w-0">
              <div
                class="text-[10px] font-bold truncate"
                :class="todo.status === 'completed' ? 'line-through text-[var(--dim-color)]' : 'text-[var(--fg-color)]'"
              >
                {{ todo.title }}
              </div>
              <div class="text-[8px] text-[var(--dim-color)] uppercase mt-0.5">
                {{ todo.category }} · P{{ todo.priority }}
              </div>
            </div>
            <span
              class="text-[8px] uppercase px-1.5 py-0.5 border shrink-0"
              :class="todo.status === 'completed'
                ? 'border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)]'
                : 'border-[var(--border-color)] text-[var(--dim-color)]'"
            >
              {{ todo.status }}
            </span>
          </div>
        </template>

        <!-- Technical Debt -->
        <template v-else>
          <AppEmptyState v-if="detail.techDebt.length === 0" title="No technical debt attributed to this milestone" />
          <div
            v-for="debt in detail.techDebt"
            :key="debt.id ?? debt.title"
            class="p-2 border border-[var(--border-color)] ui-surface-card space-y-1"
          >
            <div class="flex items-center justify-between gap-2">
              <span class="text-[10px] font-bold text-[var(--fg-color)] truncate">{{ debt.title }}</span>
              <span
                class="text-[8px] uppercase px-1.5 py-0.5 border shrink-0"
                :class="debt.status === 'resolved'
                  ? 'border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)]'
                  : 'border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)]'"
              >
                {{ debt.severity }} · {{ debt.status }}
              </span>
            </div>
            <p class="text-[9px] text-[var(--dim-color)]">{{ debt.impact_summary }}</p>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>
