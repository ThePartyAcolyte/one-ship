<script setup lang="ts">
import { computed } from 'vue';
import {
  Flag,
  Plus,
  Filter,
  AlertTriangle,
  CheckSquare,
  ListTodo,
} from '@lucide/vue';
import { AppTooltip, AppEmptyState } from '../ui';
import { useMilestoneStore } from '../../stores/milestoneStore';
import type { ProjectMilestoneSummary } from '../../types/generated/ProjectMilestoneSummary';

const props = defineProps<{
  selectedMilestoneId: number | null;
  milestoneFilterId: number | null;
}>();

const emit = defineEmits<{
  (e: 'selectMilestone', milestoneId: number): void;
  (e: 'toggleFilter', milestoneId: number): void;
  (e: 'createMilestone'): void;
}>();

const milestoneStore = useMilestoneStore();

const trackingMode = computed(() => milestoneStore.trackingMode);
const milestones = computed(() => milestoneStore.milestones);
const activeMilestones = computed(() =>
  milestones.value.filter(m => m.milestone.status !== 'archived')
);

function statusBadgeClass(status: string): string {
  switch (status) {
    case 'completed':
      return 'border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)]';
    case 'active':
      return 'border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)]';
    case 'archived':
      return 'border-[var(--border-color)] text-[var(--dim-color)] opacity-60';
    default:
      return 'border-[var(--border-color)] text-[var(--dim-color)]';
  }
}

function hillCoordinates(pos: number): { x: number; y: number } {
  const clamped = Math.min(100, Math.max(0, pos));
  const t = clamped / 100;
  // Smooth bell curve from (20, 74) peaking at (160, 14) to (300, 74)
  const x = 20 + t * 280;
  const y = 74 - Math.sin(t * Math.PI) * 58;
  return { x, y };
}

function formatEpochShort(epoch: number | null): string {
  if (!epoch) return '—';
  const d = new Date(epoch * 1000);
  const m = d.toLocaleString('en-US', { month: 'short', timeZone: 'UTC' });
  const day = d.getUTCDate();
  return `${m} ${day}`;
}

function hasWarning(s: ProjectMilestoneSummary): boolean {
  return (
    (s.scheduleDriftDays !== null && s.scheduleDriftDays > 0) ||
    s.uphillRiskFlag ||
    s.criticalTechDebtCount > 0
  );
}

function formatTokens(count: number): string {
  if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1)}M tok`;
  if (count >= 1_000) return `${(count / 1_000).toFixed(1)}k tok`;
  return `${count} tok`;
}
</script>

<template>
  <div class="flex-1 min-w-0 min-h-0 overflow-y-auto p-3 space-y-3 font-mono">
    <!-- Top Summary Strip -->
    <div class="p-3 border border-[var(--border-color)] ui-surface-card flex flex-wrap items-center justify-between gap-3">
      <div class="flex items-center gap-3 min-w-0">
        <Flag :size="14" class="text-[var(--accent-color)] shrink-0" />
        <div class="min-w-0">
          <div class="flex items-center gap-2">
            <span class="text-xs font-bold text-[var(--fg-color)]">
              {{ Math.round(milestoneStore.overallProgressPercent) }}% Overall
            </span>
            <span class="text-[8px] uppercase px-1.5 py-0.5 border border-[var(--border-color)] text-[var(--dim-color)]">
              Mode: {{ trackingMode }}
            </span>
            <span class="text-[8px] uppercase px-1.5 py-0.5 border border-[var(--border-color)] text-[var(--dim-color)]">
              {{ formatTokens(milestoneStore.totalTokensUsedCount) }}
            </span>
          </div>
          <div class="w-48 h-1.5 bg-[var(--ui-bg-list)] border border-[var(--border-color)] mt-1 overflow-hidden">
            <div
              class="h-full bg-[var(--accent-color)] transition-all"
              :style="{ width: `${Math.min(100, Math.max(0, milestoneStore.overallProgressPercent))}%` }"
            />
          </div>
        </div>
      </div>

      <button
        type="button"
        @click="emit('createMilestone')"
        class="h-6 px-2 text-[9px] font-bold uppercase tracking-wider border border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] hover:bg-[var(--accent-color)]/25 transition-colors flex items-center gap-1 cursor-pointer shrink-0"
      >
        <Plus :size="11" />
        <span>New Milestone</span>
      </button>
    </div>

    <!-- Shape Up Hill Chart (when trackingMode === 'shape_up' and milestones exist) -->
    <div
      v-if="trackingMode === 'shape_up' && activeMilestones.length > 0"
      class="p-3 border border-[var(--border-color)] ui-surface-card space-y-1.5"
    >
      <div class="flex items-center justify-between text-[9px] uppercase tracking-wider text-[var(--dim-color)]">
        <span>Uphill (Figuring Out)</span>
        <span>Hill Chart</span>
        <span>Downhill (Executing)</span>
      </div>
      <svg viewBox="0 0 320 86" class="w-full h-24 overflow-visible">
        <line x1="160" y1="6" x2="160" y2="78" stroke="var(--border-color)" stroke-dasharray="3 3" stroke-width="1" />
        <path
          d="M 20 74 Q 90 74 160 16 Q 230 74 300 74"
          fill="none"
          stroke="var(--border-color)"
          stroke-width="1.5"
        />
        <g
          v-for="s in activeMilestones"
          :key="s.milestone.id"
          @click="emit('selectMilestone', s.milestone.id)"
          class="cursor-pointer"
        >
          <circle
            :cx="hillCoordinates(s.milestone.hill_position_percent).x"
            :cy="hillCoordinates(s.milestone.hill_position_percent).y"
            r="5"
            :fill="selectedMilestoneId === s.milestone.id ? 'var(--accent-color)' : 'var(--fg-color)'"
            stroke="var(--bg-color)"
            stroke-width="1.5"
          />
          <text
            :x="hillCoordinates(s.milestone.hill_position_percent).x"
            :y="hillCoordinates(s.milestone.hill_position_percent).y - 9"
            text-anchor="middle"
            fill="var(--fg-color)"
            font-size="7.5"
            font-family="monospace"
          >
            {{ s.milestone.title.slice(0, 14) }}
          </text>
        </g>
      </svg>
    </div>

    <!-- Weighted Value Allocation Bar (when trackingMode === 'weighted') -->
    <div
      v-if="trackingMode === 'weighted' && activeMilestones.length > 0"
      class="p-3 border border-[var(--border-color)] ui-surface-card space-y-1.5"
    >
      <div class="flex items-center justify-between text-[9px] uppercase tracking-wider text-[var(--dim-color)]">
        <span>Baseline Value Allocation (Normalized 100%)</span>
        <span>{{ activeMilestones.length }} active phase(s)</span>
      </div>
      <div class="h-3 w-full flex border border-[var(--border-color)] bg-[var(--ui-bg-list)] overflow-hidden">
        <AppTooltip
          v-for="s in activeMilestones"
          :key="s.milestone.id"
          :text="`${s.milestone.title}: ${s.effectiveWeightPercent.toFixed(1)}% weight (${Math.round(s.progressPercent)}% done)`"
        >
          <button
            type="button"
            @click="emit('selectMilestone', s.milestone.id)"
            class="h-full border-r border-[var(--bg-color)] last:border-r-0 transition-opacity hover:opacity-90 cursor-pointer relative"
            :style="{ width: `${Math.max(2, s.effectiveWeightPercent)}%` }"
          >
            <div
              class="h-full bg-[var(--accent-color)]"
              :style="{ width: `${Math.min(100, Math.max(0, s.progressPercent))}%`, opacity: selectedMilestoneId === s.milestone.id ? 1 : 0.75 }"
            />
          </button>
        </AppTooltip>
      </div>
    </div>

    <!-- Empty State -->
    <AppEmptyState
      v-if="milestones.length === 0"
      title="No milestones defined yet"
      description="Create strategic milestones to structure architecture, deliverables, and emergent tasks."
    />

    <!-- Milestones Grid -->
    <div v-else class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-2.5">
      <div
        v-for="s in milestones"
        :key="s.milestone.id"
        @click="emit('selectMilestone', s.milestone.id)"
        class="p-3 border ui-surface-card hover:border-[var(--accent-color)] transition-colors cursor-pointer flex flex-col gap-2"
        :class="selectedMilestoneId === s.milestone.id ? 'border-[var(--accent-color)]' : 'border-[var(--border-color)]'"
      >
        <div class="flex items-start justify-between gap-2">
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-1.5">
              <span
                v-if="hasWarning(s)"
                class="w-1.5 h-1.5 rounded-full bg-[var(--status-warn-color)] shrink-0"
              />
              <span class="text-xs font-bold text-[var(--fg-color)] truncate">{{ s.milestone.title }}</span>
            </div>
            <p v-if="s.milestone.description" class="text-[10px] text-[var(--dim-color)] line-clamp-2 mt-0.5">
              {{ s.milestone.description }}
            </p>
          </div>
          <span class="text-[8px] uppercase px-1.5 py-0.5 border shrink-0" :class="statusBadgeClass(s.milestone.status)">
            {{ s.milestone.status }}
          </span>
        </div>

        <!-- Progress Bar -->
        <div class="space-y-1">
          <div class="flex items-center justify-between text-[9px]">
            <span class="text-[var(--dim-color)] flex items-center gap-1">
              <CheckSquare :size="10" />
              <span>{{ s.tasksDoneCount }}/{{ s.tasksTotalCount }} tasks</span>
            </span>
            <span class="font-bold text-[var(--fg-color)]">{{ Math.round(s.progressPercent) }}%</span>
          </div>
          <div class="h-1.5 w-full bg-[var(--ui-bg-list)] border border-[var(--border-color)] overflow-hidden">
            <div
              class="h-full bg-[var(--accent-color)] transition-all"
              :style="{ width: `${Math.min(100, Math.max(0, s.progressPercent))}%` }"
            />
          </div>
        </div>

        <!-- Mode & Compass Metadata -->
        <div class="flex flex-wrap items-center justify-between gap-1.5 pt-1 border-t border-[var(--border-color)]/60 text-[8px] uppercase">
          <div class="flex flex-wrap items-center gap-1">
            <span
              v-if="trackingMode === 'weighted'"
              class="px-1 py-0.5 border border-[var(--border-color)] text-[var(--fg-color)]"
            >
              {{ s.effectiveWeightPercent.toFixed(1) }}% share
            </span>
            <span
              v-if="trackingMode === 'shape_up'"
              class="px-1 py-0.5 border border-[var(--border-color)] text-[var(--fg-color)]"
            >
              Hill {{ s.milestone.hill_position_percent }}%
            </span>
            <span
              v-if="s.milestone.appetite_days"
              class="px-1 py-0.5 border border-[var(--border-color)] text-[var(--dim-color)]"
            >
              {{ s.milestone.appetite_days }}d
            </span>
            <span
              v-if="s.scheduleDriftDays !== null && s.scheduleDriftDays > 0"
              class="px-1 py-0.5 border border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)]"
            >
              +{{ s.scheduleDriftDays }}d drift
            </span>
            <span
              v-if="s.openTechDebtCount > 0"
              class="px-1 py-0.5 border border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)] flex items-center gap-0.5"
            >
              <AlertTriangle :size="9" />
              <span>{{ s.openTechDebtCount }} debt</span>
            </span>
            <span
              v-if="s.linkedTodosTotalCount > 0"
              class="px-1 py-0.5 border border-[var(--border-color)] text-[var(--dim-color)] flex items-center gap-0.5"
            >
              <ListTodo :size="9" />
              <span>{{ s.linkedTodosCompletedCount }}/{{ s.linkedTodosTotalCount }}</span>
            </span>
          </div>

          <div class="flex items-center gap-1.5 text-[var(--dim-color)]">
            <span v-if="s.milestone.planned_due_epoch_seconds">
              Due {{ formatEpochShort(s.milestone.planned_due_epoch_seconds) }}
            </span>
            <AppTooltip text="Filter Board / Schedule / Gantt by this milestone">
              <button
                type="button"
                @click.stop="emit('toggleFilter', s.milestone.id)"
                class="ui-icon-btn"
                :class="milestoneFilterId === s.milestone.id ? 'text-[var(--accent-color)] border-[var(--accent-color)]' : ''"
              >
                <Filter :size="10" />
              </button>
            </AppTooltip>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
