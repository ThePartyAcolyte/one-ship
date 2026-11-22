<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { X, Flag, Loader2 } from '@lucide/vue';
import { AppTooltip, AppInput, AppDatePicker, AppSelect, AppTextarea, type SelectOption } from '../ui';
import {
  useMilestoneStore,
  type MilestoneStatus,
} from '../../stores/milestoneStore';

const props = defineProps<{
  projectPath: string;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'created', milestoneId: number): void;
}>();

const milestoneStore = useMilestoneStore();

const title = ref('');
const description = ref('');
const status = ref<MilestoneStatus>('planned');
const weightPercent = ref('25');
const hillPositionPercent = ref('10');
const appetiteDays = ref('');
const plannedStartDate = ref('');
const plannedDueDate = ref('');
const isCreating = ref(false);

const statusOptions: SelectOption[] = [
  { label: 'Planned', value: 'planned' },
  { label: 'Active', value: 'active' },
  { label: 'Completed', value: 'completed' },
];

const trackingMode = computed(() => milestoneStore.trackingMode);

function dateInputToEpoch(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  const parts = trimmed.split('-').map(Number);
  if (parts.length !== 3 || parts.some(Number.isNaN)) return null;
  return Math.floor(Date.UTC(parts[0], parts[1] - 1, parts[2], 12, 0, 0) / 1000);
}

async function handleCreate() {
  const cleanTitle = title.value.trim();
  if (!cleanTitle || isCreating.value) return;
  isCreating.value = true;
  try {
    const parsedWeight = Number(weightPercent.value);
    const parsedHill = Number(hillPositionPercent.value);
    const parsedAppetite = appetiteDays.value.trim() ? Number(appetiteDays.value) : null;

    const created = await milestoneStore.createMilestone({
      projectPath: props.projectPath,
      title: cleanTitle,
      description: description.value.trim() || null,
      status: status.value,
      weightPercent: Number.isFinite(parsedWeight) ? Math.max(0, parsedWeight) : 0,
      hillPositionPercent: Number.isFinite(parsedHill) ? Math.min(100, Math.max(0, Math.round(parsedHill))) : 0,
      appetiteDays: parsedAppetite !== null && Number.isFinite(parsedAppetite) && parsedAppetite > 0
        ? Math.round(parsedAppetite)
        : null,
      plannedStartEpochSeconds: dateInputToEpoch(plannedStartDate.value),
      plannedDueEpochSeconds: dateInputToEpoch(plannedDueDate.value),
    });
    if (created) {
      emit('created', created.id);
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
      <div class="ui-surface-modal border border-[var(--border-color)] w-full max-w-xl flex flex-col shadow-2xl text-[var(--fg-color)]">
        <div class="px-4 py-2.5 border-b border-[var(--border-color)] bg-[var(--ui-bg-card)]/60 flex items-center justify-between gap-2">
          <div class="flex items-center gap-2 text-xs font-bold uppercase tracking-wider text-[var(--accent-color)] min-w-0">
            <Flag :size="14" class="shrink-0" />
            <span>New Milestone</span>
            <span class="text-[var(--dim-color)] font-normal truncate">// {{ trackingMode }}</span>
          </div>
          <AppTooltip text="Close (Esc)">
            <button
              type="button"
              @click="emit('close')"
              class="p-1 text-[var(--dim-color)] hover:text-[var(--fg-color)] transition-colors"
            >
              <X :size="14" />
            </button>
          </AppTooltip>
        </div>

        <form @submit.prevent="handleCreate" class="p-4 space-y-3 overflow-y-auto max-h-[75vh]">
          <div>
            <label class="block text-[9px] uppercase tracking-wider text-[var(--dim-color)] mb-1">
              Milestone Title
            </label>
            <AppInput
              v-model="title"
              placeholder="e.g. Phase 1: Core Architecture & Auth"
              autofocus
            />
          </div>

          <div class="grid grid-cols-2 gap-3">
            <div>
              <label class="block text-[9px] uppercase tracking-wider text-[var(--dim-color)] mb-1">
                Status
              </label>
              <AppSelect v-model="status" :options="statusOptions" />
            </div>

            <div v-if="trackingMode === 'weighted'">
              <label class="block text-[9px] uppercase tracking-wider text-[var(--dim-color)] mb-1">
                Weight % (Relative)
              </label>
              <AppInput
                v-model="weightPercent"
                type="number"
                placeholder="25"
              />
            </div>

            <div v-else-if="trackingMode === 'shape_up'">
              <label class="block text-[9px] uppercase tracking-wider text-[var(--dim-color)] mb-1">
                Appetite (Days)
              </label>
              <AppInput
                v-model="appetiteDays"
                type="number"
                placeholder="14"
              />
            </div>
          </div>

          <div v-if="trackingMode === 'shape_up'">
            <div class="flex items-center justify-between text-[9px] uppercase tracking-wider text-[var(--dim-color)] mb-1">
              <span>Hill Position ({{ Number(hillPositionPercent) < 50 ? 'Uphill — Figuring out' : 'Downhill — Executing' }})</span>
              <span class="text-[var(--fg-color)] font-bold">{{ hillPositionPercent }}%</span>
            </div>
            <input
              v-model="hillPositionPercent"
              type="range"
              min="0"
              max="100"
              class="w-full accent-[var(--accent-color)]"
            />
          </div>

          <div class="grid grid-cols-2 gap-3">
            <div>
              <label class="block text-[9px] uppercase tracking-wider text-[var(--dim-color)] mb-1">
                Baseline Start
              </label>
              <AppDatePicker v-model="plannedStartDate" />
            </div>
            <div>
              <label class="block text-[9px] uppercase tracking-wider text-[var(--dim-color)] mb-1">
                Baseline Due
              </label>
              <AppDatePicker v-model="plannedDueDate" />
            </div>
          </div>

          <div>
            <label class="block text-[9px] uppercase tracking-wider text-[var(--dim-color)] mb-1">
              Objective & Acceptance Criteria
            </label>
            <AppTextarea
              v-model="description"
              :rows="3"
              placeholder="Architectural scope, client deliverables, or definition of done..."
            />
          </div>

          <div class="pt-2 border-t border-[var(--border-color)] flex items-center justify-end gap-2">
            <button
              type="button"
              @click="emit('close')"
              class="h-7 px-3 text-[10px] font-bold uppercase border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] transition-colors"
            >
              Cancel
            </button>
            <button
              type="submit"
              :disabled="!title.trim() || isCreating"
              class="h-7 px-3 text-[10px] font-bold uppercase border border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] hover:bg-[var(--accent-color)]/25 disabled:opacity-40 transition-colors flex items-center gap-1.5"
            >
              <Loader2 v-if="isCreating" :size="12" class="animate-spin" />
              <span>Create Milestone</span>
            </button>
          </div>
        </form>
      </div>
    </div>
  </Teleport>
</template>
