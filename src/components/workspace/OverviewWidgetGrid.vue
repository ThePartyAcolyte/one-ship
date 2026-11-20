<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { GridLayout, GridItem } from 'grid-layout-plus';
import {
  GripVertical,
  Eye,
  EyeOff,
  ArrowUpRight,
  Kanban,
  GitPullRequest,
  Sparkles,
  BookOpen,
  Activity,
  Palette,
  Box,
  Cpu,
  RefreshCw,
  LayoutDashboard,
  RotateCcw,
} from '@lucide/vue';
import { AppTooltip } from '../ui';
import OverviewSummaryWidget from './OverviewSummaryWidget.vue';
import type { ProjectRecord, ProjectAgentState } from '../../types';
import type { ProjectOverviewSummary } from '../../types/generated/ProjectOverviewSummary';
import type { OverviewWidgetLayoutItem } from '../../types/generated/OverviewWidgetLayoutItem';

const props = defineProps<{
  project: ProjectRecord;
  activeListeningProjectId?: number | null;
  agentState?: ProjectAgentState;
  summary: ProjectOverviewSummary | null;
  isLoading: boolean;
  isIndexingKnowledge: boolean;
  isEditing: boolean;
}>();

const emit = defineEmits<{
  (e: 'saveLayout', layout: OverviewWidgetLayoutItem[]): void;
  (e: 'navigateTab', tab: string): void;
  (e: 'indexKnowledge'): void;
  (e: 'toggleEdit'): void;
  (e: 'initProject', project: ProjectRecord): void;
  (e: 'syncSkills', project: ProjectRecord): void;
  (e: 'openGithub', project: ProjectRecord): void;
  (e: 'initGit', project: ProjectRecord): void;
}>();

interface WidgetMeta {
  id: string;
  title: string;
  targetTab: string;
  defaultW: number;
  defaultH: number;
}

const WIDGET_REGISTRY: WidgetMeta[] = [
  { id: 'project_summary', title: 'Project Summary', targetTab: 'overview', defaultW: 2, defaultH: 2 },
  { id: 'project_mgmt', title: 'Project Management', targetTab: 'project', defaultW: 2, defaultH: 2 },
  { id: 'forge', title: 'Forge Collaboration', targetTab: 'project', defaultW: 2, defaultH: 1 },
  { id: 'knowledge', title: 'Semantic Knowledge', targetTab: 'knowledge', defaultW: 2, defaultH: 1 },
  { id: 'journal', title: 'Devlog & Journal', targetTab: 'devlog', defaultW: 2, defaultH: 1 },
  { id: 'profiler', title: 'Profiler', targetTab: 'profiler', defaultW: 1, defaultH: 1 },
  { id: 'desigma', title: 'Designer', targetTab: 'design', defaultW: 1, defaultH: 1 },
  { id: 'solidcraft', title: '3D CAD', targetTab: 'cad', defaultW: 1, defaultH: 1 },
  { id: 'pcbcraft', title: 'PCB & EDA', targetTab: 'pcb', defaultW: 1, defaultH: 1 },
];

const localItems = ref<OverviewWidgetLayoutItem[]>([]);

const enabledWidgetIds = computed<Set<string>>(() => {
  const s = props.summary;
  const ids = new Set<string>(['project_summary', 'knowledge', 'journal']);
  if (!s) return ids;
  if (s.projectMgmt) ids.add('project_mgmt');
  if (s.forge) ids.add('forge');
  if (s.profiler) ids.add('profiler');
  if (s.desigma) ids.add('desigma');
  if (s.solidcraft) ids.add('solidcraft');
  if (s.pcbcraft) ids.add('pcbcraft');
  return ids;
});

function buildDefaultLayout(): OverviewWidgetLayoutItem[] {
  const active = WIDGET_REGISTRY.filter((w) => enabledWidgetIds.value.has(w.id));
  const items: OverviewWidgetLayoutItem[] = [];
  let cursorX = 0;
  let cursorY = 0;
  let rowMaxH = 1;

  for (const meta of active) {
    if (cursorX + meta.defaultW > 4) {
      cursorX = 0;
      cursorY += rowMaxH;
      rowMaxH = 1;
    }
    items.push({
      id: meta.id,
      x: cursorX,
      y: cursorY,
      w: meta.defaultW,
      h: meta.defaultH,
      visible: true,
    });
    cursorX += meta.defaultW;
    rowMaxH = Math.max(rowMaxH, meta.defaultH);
  }
  return items;
}

function syncFromSummary() {
  const saved = props.summary?.layout ?? [];
  const enabled = enabledWidgetIds.value;
  if (saved.length === 0) {
    localItems.value = buildDefaultLayout();
    return;
  }

  const byId = new Map<string, OverviewWidgetLayoutItem>();
  for (const item of saved) {
    if (enabled.has(item.id)) {
      byId.set(item.id, {
        ...item,
        visible: item.id === 'project_summary' ? true : item.visible,
        w: Math.max(1, Math.min(4, item.w)),
        h: Math.max(1, Math.min(3, item.h)),
        x: Math.max(0, Math.min(3, item.x)),
      });
    }
  }

  let maxY = 0;
  for (const item of byId.values()) {
    maxY = Math.max(maxY, item.y + item.h);
  }
  // Keep project_summary positioned at top-left (0, 0) when newly injected into an existing layout
  if (!byId.has('project_summary') && enabled.has('project_summary')) {
    const summaryMeta = WIDGET_REGISTRY.find((m) => m.id === 'project_summary')!;
    for (const existing of byId.values()) {
      existing.y += summaryMeta.defaultH;
    }
    byId.set('project_summary', {
      id: 'project_summary',
      x: 0,
      y: 0,
      w: summaryMeta.defaultW,
      h: summaryMeta.defaultH,
      visible: true,
    });
    maxY += summaryMeta.defaultH;
  }

  for (const meta of WIDGET_REGISTRY) {
    if (enabled.has(meta.id) && !byId.has(meta.id)) {
      byId.set(meta.id, {
        id: meta.id,
        x: 0,
        y: maxY,
        w: meta.defaultW,
        h: meta.defaultH,
        visible: true,
      });
      maxY += meta.defaultH;
    }
  }

  localItems.value = Array.from(byId.values());
}

watch(() => props.summary, syncFromSummary, { immediate: true });

const visibleGridItems = computed(() =>
  localItems.value
    .filter((item) => item.visible && enabledWidgetIds.value.has(item.id))
    .map((item) => ({ ...item, i: item.id })),
);

const availableMetas = computed(() =>
  WIDGET_REGISTRY.filter((m) => m.id !== 'project_summary' && enabledWidgetIds.value.has(m.id)),
);

function getMeta(id: string): WidgetMeta {
  return WIDGET_REGISTRY.find((m) => m.id === id) ?? {
    id,
    title: id,
    targetTab: 'overview',
    defaultW: 1,
    defaultH: 1,
  };
}

function isWidgetVisible(id: string): boolean {
  return localItems.value.find((i) => i.id === id)?.visible ?? true;
}

function toggleWidgetVisibility(id: string) {
  const item = localItems.value.find((i) => i.id === id);
  if (!item) return;
  item.visible = !item.visible;
  emit('saveLayout', localItems.value);
}

function applyPresetSize(id: string, w: number, h: number) {
  const item = localItems.value.find((i) => i.id === id);
  if (!item) return;
  item.w = w;
  item.h = h;
  if (item.x + w > 4) {
    item.x = Math.max(0, 4 - w);
  }
  emit('saveLayout', localItems.value);
}

function handleLayoutUpdated(newLayout: Array<{ i: string | number; x: number; y: number; w: number; h: number }>) {
  for (const updated of newLayout) {
    const target = localItems.value.find((item) => item.id === String(updated.i));
    if (target) {
      target.x = updated.x;
      target.y = updated.y;
      target.w = updated.w;
      target.h = updated.h;
    }
  }
  if (props.isEditing) {
    emit('saveLayout', localItems.value);
  }
}

function resetLayout() {
  localItems.value = buildDefaultLayout();
  emit('saveLayout', localItems.value);
}

defineExpose({ resetLayout });

function formatEpoch(secs: number | null | undefined): string {
  if (!secs) return '—';
  return new Date(secs * 1000).toLocaleDateString();
}
</script>

<template>
  <div class="flex flex-col gap-2.5 w-full">
    <!-- Customize Visibility Strip (Only shown in Customize mode) -->
    <div
      v-if="isEditing"
      class="mx-[8px] flex items-center gap-1.5 px-2.5 py-1.5 border border-[var(--border-color)] ui-surface-list overflow-x-auto scrollbar-none"
    >
      <span class="text-[9px] uppercase font-bold text-[var(--dim-color)] shrink-0 mr-1">Widgets:</span>
      <button
        v-for="meta in availableMetas"
        :key="meta.id"
        type="button"
        @click="toggleWidgetVisibility(meta.id)"
        class="h-6 px-2 text-[9px] font-bold uppercase border flex items-center gap-1 shrink-0 cursor-pointer transition-colors"
        :class="isWidgetVisible(meta.id)
          ? 'border-[var(--accent-color)]/70 bg-[var(--accent-color)]/10 text-[var(--fg-color)]'
          : 'border-[var(--border-color)] text-[var(--dim-color)] opacity-60 hover:opacity-100'"
      >
        <Eye v-if="isWidgetVisible(meta.id)" :size="10" class="text-[var(--accent-color)]" />
        <EyeOff v-else :size="10" />
        <span>{{ meta.title }}</span>
      </button>

      <AppTooltip text="Reset widgets to default 4-column layout" position="bottom">
        <button
          type="button"
          @click="resetLayout"
          class="ml-auto h-6 px-2 text-[9px] font-bold uppercase border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--accent-color)] flex items-center gap-1 shrink-0 cursor-pointer transition-colors"
        >
          <RotateCcw :size="10" />
          <span>Reset</span>
        </button>
      </AppTooltip>
    </div>

    <!-- 4-Column Widget Grid -->
    <GridLayout
      v-if="visibleGridItems.length > 0"
      :layout="visibleGridItems"
      :col-num="4"
      :row-height="104"
      :margin="[8, 8]"
      :is-draggable="isEditing"
      :is-resizable="isEditing"
      :vertical-compact="true"
      :use-css-transforms="true"
      @layout-updated="handleLayoutUpdated"
      class="overview-grid-layout"
    >
      <GridItem
        v-for="item in visibleGridItems"
        :key="item.id"
        :x="item.x"
        :y="item.y"
        :w="item.w"
        :h="item.h"
        :min-w="1"
        :max-w="4"
        :min-h="1"
        :max-h="3"
        :i="item.i"
        drag-allow-from=".widget-drag-handle"
        class="border border-[var(--border-color)] ui-surface-card flex flex-col overflow-hidden transition-colors"
        :class="isEditing ? 'hover:border-[var(--accent-color)]' : ''"
      >
        <!-- Widget Header -->
        <div
          class="h-7 px-2.5 border-b border-[var(--border-color)] ui-surface-title flex items-center justify-between gap-1.5 shrink-0 select-none"
          :class="isEditing ? 'widget-drag-handle cursor-grab active:cursor-grabbing' : ''"
        >
          <div class="flex items-center gap-1.5 min-w-0">
            <GripVertical v-if="isEditing" :size="11" class="text-[var(--accent-color)] shrink-0" />
            <LayoutDashboard v-if="item.id === 'project_summary'" :size="11" class="text-[var(--accent-color)] shrink-0" />
            <Kanban v-else-if="item.id === 'project_mgmt'" :size="11" class="text-[var(--accent-color)] shrink-0" />
            <GitPullRequest v-else-if="item.id === 'forge'" :size="11" class="text-[var(--accent-color)] shrink-0" />
            <Sparkles v-else-if="item.id === 'knowledge'" :size="11" class="text-[var(--accent-color)] shrink-0" />
            <BookOpen v-else-if="item.id === 'journal'" :size="11" class="text-[var(--accent-color)] shrink-0" />
            <Activity v-else-if="item.id === 'profiler'" :size="11" class="text-[var(--accent-color)] shrink-0" />
            <Palette v-else-if="item.id === 'desigma'" :size="11" class="text-[var(--accent-color)] shrink-0" />
            <Box v-else-if="item.id === 'solidcraft'" :size="11" class="text-[var(--accent-color)] shrink-0" />
            <Cpu v-else-if="item.id === 'pcbcraft'" :size="11" class="text-[var(--accent-color)] shrink-0" />
            <span class="text-[9px] font-bold uppercase tracking-wider text-[var(--fg-color)] truncate">
              {{ getMeta(item.id).title }}
            </span>
          </div>

          <div v-if="isEditing" class="flex items-center gap-1 shrink-0">
            <button
              v-for="preset in ([ [1,1], [2,1], [2,2], [4,1] ] as const)"
              :key="`${preset[0]}x${preset[1]}`"
              type="button"
              @click.stop="applyPresetSize(item.id, preset[0], preset[1])"
              class="px-1 h-4 text-[8px] border cursor-pointer"
              :class="item.w === preset[0] && item.h === preset[1]
                ? 'border-[var(--accent-color)] text-[var(--accent-color)]'
                : 'border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
            >
              {{ preset[0] }}×{{ preset[1] }}
            </button>
            <button
              v-if="item.id === 'project_summary'"
              type="button"
              @click.stop="emit('toggleEdit')"
              class="ml-1 px-1.5 h-4 text-[8px] font-bold uppercase border border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] cursor-pointer transition-colors"
            >
              Done
            </button>
          </div>
          <AppTooltip v-else-if="item.id === 'project_summary'" text="Customize dashboard layout">
            <button
              type="button"
              @click="emit('toggleEdit')"
              class="h-5 px-1.5 text-[8px] font-bold uppercase border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--accent-color)] cursor-pointer transition-colors"
            >
              Customize
            </button>
          </AppTooltip>
          <AppTooltip v-else :text="`Open ${getMeta(item.id).title}`">
            <button
              type="button"
              @click="emit('navigateTab', getMeta(item.id).targetTab)"
              class="p-0.5 text-[var(--dim-color)] hover:text-[var(--accent-color)] cursor-pointer transition-colors"
            >
              <ArrowUpRight :size="12" />
            </button>
          </AppTooltip>
        </div>

        <!-- Widget Body -->
        <div class="flex-1 min-h-0 p-2.5 flex flex-col justify-between gap-2 overflow-hidden text-[10px] font-mono">
          <!-- 0. Project Summary -->
          <OverviewSummaryWidget
            v-if="item.id === 'project_summary'"
            :project="project"
            :active-listening-project-id="activeListeningProjectId"
            :agent-state="agentState"
            @init-project="emit('initProject', $event)"
            @sync-skills="emit('syncSkills', $event)"
            @open-github="emit('openGithub', $event)"
            @init-git="emit('initGit', $event)"
          />

          <!-- 1. Project Management -->
          <template v-else-if="item.id === 'project_mgmt' && summary?.projectMgmt">
            <div class="flex items-baseline gap-4">
              <div>
                <div class="text-[8px] uppercase text-[var(--dim-color)]">Active Tasks</div>
                <div class="text-lg font-bold text-[var(--fg-color)]">{{ summary.projectMgmt.activeTasksCount }}</div>
              </div>
              <div>
                <div class="text-[8px] uppercase text-[var(--dim-color)]">Overdue</div>
                <div
                  class="text-lg font-bold"
                  :class="summary.projectMgmt.overdueTasksCount > 0 ? 'text-[var(--status-error-color)]' : 'text-[var(--status-ok-color)]'"
                >
                  {{ summary.projectMgmt.overdueTasksCount }}
                </div>
              </div>
              <div v-if="summary.projectMgmt.milestonesCount > 0">
                <div class="text-[8px] uppercase text-[var(--dim-color)]">Milestones</div>
                <div class="text-lg font-bold text-[var(--accent-color)] flex items-baseline gap-1">
                  <span>{{ Math.round(summary.projectMgmt.overallProgressPercent) }}%</span>
                  <span
                    v-if="summary.projectMgmt.driftWarningsCount > 0"
                    class="text-[9px] text-[var(--status-warn-color)]"
                  >
                    ({{ summary.projectMgmt.driftWarningsCount }}!)
                  </span>
                </div>
              </div>
            </div>
            <div class="flex items-center gap-1.5 overflow-x-auto scrollbar-none min-h-0">
              <span
                v-for="st in summary.projectMgmt.stages"
                :key="st.slug"
                class="px-1.5 py-0.5 border border-[var(--border-color)] ui-surface-list text-[9px] text-[var(--dim-color)] shrink-0"
              >
                {{ st.label }}: <strong class="text-[var(--fg-color)]">{{ st.count }}</strong>
              </span>
            </div>
          </template>

          <!-- 2. Forge Collaboration -->
          <template v-else-if="item.id === 'forge' && summary?.forge">
            <div class="flex items-baseline gap-4">
              <div>
                <div class="text-[8px] uppercase text-[var(--dim-color)]">Unread</div>
                <div
                  class="text-base font-bold"
                  :class="summary.forge.unreadCount > 0 ? 'text-[var(--accent-color)]' : 'text-[var(--fg-color)]'"
                >
                  {{ summary.forge.unreadCount }}
                </div>
              </div>
              <div>
                <div class="text-[8px] uppercase text-[var(--dim-color)]">Flagged</div>
                <div class="text-base font-bold text-[var(--status-warn-color)]">{{ summary.forge.flaggedCount }}</div>
              </div>
              <div>
                <div class="text-[8px] uppercase text-[var(--dim-color)]">Tracked</div>
                <div class="text-base font-bold text-[var(--dim-color)]">{{ summary.forge.trackedItemsCount }}</div>
              </div>
            </div>
            <div class="text-[8px] uppercase text-[var(--dim-color)] truncate">
              Provider: <span class="text-[var(--fg-color)]">{{ summary.forge.provider || 'git' }}</span>
            </div>
          </template>

          <!-- 3. Semantic Knowledge (Debt #40 & Spec 22 §5) -->
          <template v-else-if="item.id === 'knowledge' && summary">
            <div class="flex items-center justify-between gap-2">
              <div class="flex items-baseline gap-4">
                <div>
                  <div class="text-[8px] uppercase text-[var(--dim-color)]">Chunks</div>
                  <div class="text-base font-bold text-[var(--fg-color)]">{{ summary.knowledge.indexedChunksCount }}</div>
                </div>
                <div>
                  <div class="text-[8px] uppercase text-[var(--dim-color)]">Graph Edges</div>
                  <div
                    class="text-base font-bold"
                    :class="summary.knowledge.graphEdgesCount > 0 ? 'text-[var(--accent-color)]' : 'text-[var(--dim-color)]'"
                  >
                    {{ summary.knowledge.graphEdgesCount }}
                  </div>
                </div>
              </div>
              <button
                type="button"
                :disabled="isIndexingKnowledge || summary.knowledge.isIndexing"
                @click="emit('indexKnowledge')"
                class="h-6 px-2 text-[9px] font-bold uppercase border border-[var(--border-color)] text-[var(--fg-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] flex items-center gap-1 cursor-pointer disabled:opacity-40"
              >
                <RefreshCw :size="10" :class="{ 'animate-spin': isIndexingKnowledge || summary.knowledge.isIndexing }" />
                <span>{{ isIndexingKnowledge || summary.knowledge.isIndexing ? 'Indexing...' : 'Index' }}</span>
              </button>
            </div>
            <div class="flex items-center gap-1.5 flex-wrap text-[8px] uppercase">
              <span class="px-1 py-0.5 border border-[var(--border-color)]/60 text-[var(--dim-color)]">
                SYMBOLS <span class="text-[var(--fg-color)] font-bold">{{ summary.knowledge.codeChunksCount }}</span>
              </span>
              <span class="px-1 py-0.5 border border-[var(--border-color)]/60 text-[var(--dim-color)]">
                SPECS <span class="text-[var(--fg-color)] font-bold">{{ summary.knowledge.specChunksCount }}</span>
              </span>
              <span class="px-1 py-0.5 border border-[var(--border-color)]/60 text-[var(--dim-color)]">
                ENTITIES <span class="text-[var(--fg-color)] font-bold">{{ summary.knowledge.entityChunksCount }}</span>
              </span>
            </div>
            <div class="flex items-center justify-between gap-2 text-[8px] text-[var(--dim-color)] truncate">
              <span>
                AUTO-SYNC:
                <span :class="summary.knowledge.autoIndexEnabled ? 'text-[var(--status-ok-color)] font-bold' : 'text-[var(--dim-color)]'">
                  {{ summary.knowledge.autoIndexEnabled ? 'ON' : 'OFF' }}
                </span>
                · MIN {{ summary.knowledge.minScorePercent }}%
              </span>
              <span class="truncate">
                {{ summary.knowledge.lastDurationMs != null ? `${summary.knowledge.lastDurationMs} ms` : 'Ready' }}
              </span>
            </div>
          </template>

          <!-- 4. Devlog & Journal -->
          <template v-else-if="item.id === 'journal' && summary">
            <div class="flex flex-col gap-0.5 min-w-0">
              <div class="text-[8px] uppercase text-[var(--dim-color)]">
                Latest Minutes ({{ summary.journal.totalEntriesCount }})
              </div>
              <div class="text-[10px] font-bold text-[var(--fg-color)] truncate">
                {{ summary.journal.latestTitle || 'No session minutes recorded yet' }}
              </div>
            </div>
            <div class="text-[8px] text-[var(--dim-color)] flex items-center justify-between gap-2">
              <span class="truncate">{{ summary.journal.latestAgentId ? `@${summary.journal.latestAgentId}` : '—' }}</span>
              <span>{{ formatEpoch(summary.journal.latestCreatedAtSeconds) }}</span>
            </div>
          </template>

          <!-- 5. Profiler -->
          <template v-else-if="item.id === 'profiler' && summary?.profiler">
            <div class="text-base font-bold text-[var(--fg-color)]">{{ summary.profiler.runsCount }} runs</div>
            <div class="text-[8px] text-[var(--dim-color)] truncate">
              {{ summary.profiler.latestTargetId ? `${summary.profiler.latestTargetId} · ${summary.profiler.latestStatus}` : 'No runs yet' }}
            </div>
          </template>

          <!-- 6. Designer / CAD / PCB -->
          <template v-else-if="(item.id === 'desigma' && summary?.desigma) || (item.id === 'solidcraft' && summary?.solidcraft) || (item.id === 'pcbcraft' && summary?.pcbcraft)">
            <div class="text-base font-bold text-[var(--fg-color)]">
              {{ (item.id === 'desigma' ? summary.desigma : item.id === 'solidcraft' ? summary.solidcraft : summary.pcbcraft)?.filesCount }} files
            </div>
            <div class="text-[8px] uppercase" :class="(item.id === 'desigma' ? summary.desigma : item.id === 'solidcraft' ? summary.solidcraft : summary.pcbcraft)?.isEditorRunning ? 'text-[var(--status-ok-color)]' : 'text-[var(--dim-color)]'">
              {{ (item.id === 'desigma' ? summary.desigma : item.id === 'solidcraft' ? summary.solidcraft : summary.pcbcraft)?.isEditorRunning ? 'Editor Running' : 'Editor Idle' }}
            </div>
          </template>
        </div>
      </GridItem>
    </GridLayout>
  </div>
</template>

<style scoped>
:deep(.vgl-item__placeholder) {
  background: color-mix(in srgb, var(--accent-color) 18%, transparent);
  border: 1px dashed var(--accent-color);
  border-radius: 0;
}

:deep(.vgl-item__resizer::before) {
  border-color: var(--accent-color);
}
</style>
