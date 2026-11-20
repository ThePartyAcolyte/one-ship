<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { X, Plus, Loader2 } from '@lucide/vue';
import { AppTooltip, AppInput, AppDatePicker, AppSelect, AppTextarea, type SelectOption } from '../ui';
import { useProjectMgmtStore, type ProjectTaskDomain } from '../../stores/projectMgmtStore';
import { useMilestoneStore } from '../../stores/milestoneStore';

const props = defineProps<{
  projectPath: string;
  initialStageSlug: string;
  initialMilestoneId?: number | null;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'created', taskId: number): void;
}>();

const store = useProjectMgmtStore();
const milestoneStore = useMilestoneStore();

const title = ref('');
const description = ref('');
const domain = ref<ProjectTaskDomain>('code');
const stageSlug = ref(props.initialStageSlug);
const priority = ref('2');
const milestoneId = ref(props.initialMilestoneId ? String(props.initialMilestoneId) : '');
const weightPoints = ref('1');
const startDate = ref('');
const dueDate = ref('');
const branchName = ref('');
const isCreating = ref(false);

const domainOptions: SelectOption[] = [
  { label: 'Code', value: 'code' },
  { label: 'Hardware', value: 'hardware' },
  { label: 'Procurement', value: 'procurement' },
  { label: 'Design', value: 'design' },
  { label: 'Docs', value: 'docs' },
  { label: 'Client', value: 'client' },
  { label: 'Milestone', value: 'milestone' },
  { label: 'Ops', value: 'ops' },
];

const stageOptions = computed<SelectOption[]>(() => {
  return store.stages.map(s => ({ label: s.label, value: s.slug }));
});

const milestoneOptions = computed<SelectOption[]>(() => [
  { label: 'No Milestone', value: '' },
  ...milestoneStore.milestones.map(m => ({ label: m.milestone.title, value: String(m.milestone.id) })),
]);

const priorityOptions: SelectOption[] = [
  { label: 'P1 — High', value: '1' },
  { label: 'P2 — Medium', value: '2' },
  { label: 'P3 — Low', value: '3' },
];

const initialStageLabel = computed(() => {
  return store.stages.find(s => s.slug === stageSlug.value)?.label ?? stageSlug.value;
});

function dateInputToEpoch(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  const parts = trimmed.split('-').map(Number);
  if (parts.length !== 3 || parts.some(Number.isNaN)) return null;
  return Math.floor(Date.UTC(parts[0], parts[1] - 1, parts[2], 12, 0, 0) / 1000);
}

const dateRangeError = computed(() => {
  const start = dateInputToEpoch(startDate.value);
  const due = dateInputToEpoch(dueDate.value);
  if (start !== null && due !== null && due < start) {
    return 'Due date cannot be earlier than start date.';
  }
  return null;
});

async function handleCreate() {
  const cleanTitle = title.value.trim();
  if (!cleanTitle || dateRangeError.value || isCreating.value) return;
  isCreating.value = true;
  try {
    const id = await store.createTask({
      projectPath: props.projectPath,
      title: cleanTitle,
      description: description.value.trim() || null,
      domain: domain.value,
      status: stageSlug.value,
      priority: Number(priority.value) || 2,
      milestoneId: milestoneId.value ? Number(milestoneId.value) : null,
      weightPoints: Math.max(1, Number(weightPoints.value) || 1),
      startEpochSeconds: dateInputToEpoch(startDate.value),
      dueEpochSeconds: dateInputToEpoch(dueDate.value),
      branchName: branchName.value.trim() || null,
    });
    if (id !== null) {
      emit('created', id);
      emit('close');
    }
  } finally {
    isCreating.value = false;
  }
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
  <Teleport to="body">
    <div
      class="fixed inset-0 ui-overlay-backdrop flex items-center justify-center z-50 p-4 font-mono"
      @click.self="emit('close')"
    >
      <div class="bg-[var(--bg-color)] border border-[var(--border-color)] w-full max-w-xl flex flex-col shadow-2xl text-[var(--fg-color)]">
        <!-- Header -->
        <div class="px-4 py-2.5 border-b border-[var(--border-color)] bg-[var(--ui-bg-card)]/60 flex items-center justify-between gap-2">
          <div class="flex items-center gap-2 text-xs font-bold uppercase tracking-wider text-[var(--accent-color)] min-w-0">
            <Plus :size="14" class="shrink-0" />
            <span>New Project Task</span>
            <span class="text-[var(--dim-color)] font-normal truncate">// {{ initialStageLabel }}</span>
          </div>
          <AppTooltip text="Close (Esc)">
            <button
              type="button"
              @click="emit('close')"
              class="text-[var(--dim-color)] hover:text-[var(--fg-color)] p-1 cursor-pointer"
            >
              <X :size="14" />
            </button>
          </AppTooltip>
        </div>

        <!-- Form Body -->
        <div class="p-4 flex flex-col gap-3">
          <div class="flex flex-col gap-1">
            <label class="text-[10px] uppercase font-bold text-[var(--dim-color)]">Title</label>
            <AppInput
              v-model="title"
              placeholder="e.g. Order LDO regulators, Implement CAN parser..."
              @keydown.enter.prevent="handleCreate"
            />
          </div>

          <div class="grid grid-cols-1 sm:grid-cols-3 gap-2.5">
            <div class="flex flex-col gap-1">
              <label class="text-[10px] uppercase font-bold text-[var(--dim-color)]">Stage</label>
              <AppSelect
                :model-value="stageSlug"
                :options="stageOptions"
                @update:model-value="stageSlug = $event"
              />
            </div>
            <div class="flex flex-col gap-1">
              <label class="text-[10px] uppercase font-bold text-[var(--dim-color)]">Domain</label>
              <AppSelect
                :model-value="domain"
                :options="domainOptions"
                @update:model-value="domain = $event as ProjectTaskDomain"
              />
            </div>
            <div class="flex flex-col gap-1">
              <label class="text-[10px] uppercase font-bold text-[var(--dim-color)]">Priority</label>
              <AppSelect
                :model-value="priority"
                :options="priorityOptions"
                @update:model-value="priority = $event"
              />
            </div>
          </div>

          <div class="grid grid-cols-1 sm:grid-cols-3 gap-2.5">
            <div class="flex flex-col gap-1 sm:col-span-2">
              <label class="text-[10px] uppercase font-bold text-[var(--dim-color)]">Milestone</label>
              <AppSelect
                :model-value="milestoneId"
                :options="milestoneOptions"
                @update:model-value="milestoneId = $event"
              />
            </div>
            <div class="flex flex-col gap-1">
              <label class="text-[10px] uppercase font-bold text-[var(--dim-color)]">Weight Points</label>
              <AppInput v-model="weightPoints" type="number" placeholder="1" />
            </div>
          </div>

          <div class="flex flex-col gap-1">
            <label class="text-[10px] uppercase font-bold text-[var(--dim-color)]">Description / Notes</label>
            <AppTextarea
              v-model="description"
              :rows="4"
              placeholder="Deliverable scope, constraints, components or links..."
            />
          </div>

          <div class="grid grid-cols-1 sm:grid-cols-3 gap-2.5">
            <div class="flex flex-col gap-1">
              <label class="text-[10px] uppercase font-bold text-[var(--dim-color)]">Start Date</label>
              <AppDatePicker v-model="startDate" />
            </div>
            <div class="flex flex-col gap-1">
              <label class="text-[10px] uppercase font-bold text-[var(--dim-color)]">Due Date</label>
              <AppDatePicker v-model="dueDate" />
            </div>
            <div class="flex flex-col gap-1">
              <label class="text-[10px] uppercase font-bold text-[var(--dim-color)]">Branch (Optional)</label>
              <AppInput v-model="branchName" placeholder="feat/..." />
            </div>
          </div>
          <div v-if="dateRangeError" class="text-[10px] text-[var(--status-error-color)] font-bold">
            {{ dateRangeError }}
          </div>
        </div>

        <!-- Footer -->
        <div class="px-4 py-3 border-t border-[var(--border-color)] bg-[var(--ui-bg-card)]/40 flex items-center justify-end gap-2">
          <button
            type="button"
            @click="emit('close')"
            class="px-3 py-1.5 text-[10px] font-bold uppercase border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer"
          >
            Cancel
          </button>
          <button
            type="button"
            :disabled="!title.trim() || Boolean(dateRangeError) || isCreating"
            @click="handleCreate"
            class="px-3.5 py-1.5 text-[10px] font-bold uppercase border border-[var(--accent-color)] bg-[var(--accent-color)] text-black hover:opacity-90 cursor-pointer disabled:opacity-40 flex items-center gap-1.5"
          >
            <Loader2 v-if="isCreating" :size="11" class="animate-spin" />
            <Plus v-else :size="11" />
            <span>{{ isCreating ? 'Creating...' : 'Create Task' }}</span>
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
