<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue';
import { Loader2, X, AlertTriangle } from '@lucide/vue';
import DocstyleSelectionModal from '../modals/DocstyleSelectionModal.vue';
import OverviewWidgetGrid from './OverviewWidgetGrid.vue';
import { AppTooltip, AppInitBadge, type ProjectInitFlag } from '../ui';
import { useCoreStore } from '../../stores/coreStore';
import { useDialogueStore } from '../../stores/dialogueStore';
import { useWorkspaceLayoutStore } from '../../stores/workspaceLayoutStore';
import { daemonCall } from '../../utils/daemonClient';
import type { ProjectRecord, ProjectAgentState } from '../../types';
import type { ProjectOverviewSummary } from '../../types/generated/ProjectOverviewSummary';
import type { OverviewWidgetLayoutItem } from '../../types/generated/OverviewWidgetLayoutItem';

const props = defineProps<{
  project: ProjectRecord;
  activeListeningProjectId?: number | null;
  agentState?: ProjectAgentState;
}>();

const emit = defineEmits<{
  (e: 'initProject', project: ProjectRecord): void;
  (e: 'syncSkills', project: ProjectRecord): void;
  (e: 'openGithub', project: ProjectRecord): void;
  (e: 'initGit', project: ProjectRecord): void;
}>();

const coreStore = useCoreStore();
const dialogueStore = useDialogueStore();
const layoutStore = useWorkspaceLayoutStore();

const showDocstyleModal = ref(false);
const activeDocstyleFlag = ref<ProjectInitFlag | null>(null);
const overviewSummary = ref<ProjectOverviewSummary | null>(null);
const isLoadingOverview = ref(false);
const isIndexingKnowledge = ref(false);
const isEditingOverview = ref(false);

async function fetchOverviewSummary(projectId: number) {
  isLoadingOverview.value = true;
  try {
    overviewSummary.value = await daemonCall('get_project_overview_summary', { projectId });
  } catch (err: unknown) {
    console.error('[ProjectOverviewTab] Failed to load overview summary:', err);
  } finally {
    isLoadingOverview.value = false;
  }
}

async function handleSaveWidgetLayout(layout: OverviewWidgetLayoutItem[]) {
  if (!props.project?.id) return;
  try {
    await daemonCall('save_project_overview_layout', {
      projectId: props.project.id,
      layout,
    });
  } catch (err: unknown) {
    console.error('[ProjectOverviewTab] Failed to save widget layout:', err);
  }
}

async function handleIndexKnowledge() {
  if (!props.project?.id || isIndexingKnowledge.value) return;
  isIndexingKnowledge.value = true;
  try {
    await daemonCall('index_project_knowledge', { projectId: props.project.id });
    await fetchOverviewSummary(props.project.id);
  } catch (err: unknown) {
    console.error('[ProjectOverviewTab] Failed to index knowledge:', err);
  } finally {
    isIndexingKnowledge.value = false;
  }
}

function handleNavigateTab(tab: string) {
  if (!props.project?.id) return;
  void layoutStore.navigateToNode(tab, props.project.id);
}

onMounted(() => {
  if (props.project?.id) {
    coreStore.fetchInitFlags(props.project.id);
    void fetchOverviewSummary(props.project.id);
  }
});

watch(() => props.project?.id, (newId) => {
  if (newId) {
    coreStore.fetchInitFlags(newId);
    void fetchOverviewSummary(newId);
  } else {
    overviewSummary.value = null;
  }
});

const pendingFlags = computed(() => {
  if (!props.project?.id) return [];
  const flags = coreStore.projectInitFlags[props.project.id] || [];
  return flags.filter(f => f.status === 'pending');
});

async function handleResolveFlag(flag: ProjectInitFlag) {
  if (!props.project) return;
  if (flag.flag_type === 'docs') {
    activeDocstyleFlag.value = flag;
    showDocstyleModal.value = true;
    return;
  }

  if (flag.id == null) {
    console.error(`Flag ${flag.flag_type} has no id and cannot be resolved`);
    return;
  }
  try {
    const agentId = await daemonCall('launch_flag_resolution_agent', {
      projectId: props.project.id,
      flagId: flag.id,
      chosenOption: null,
      cleanupStrategy: null,
      runner: null,
      model: null,
      effort: null,
    });

    await dialogueStore.openOverlay(agentId, props.project.path);
  } catch (err: unknown) {
    console.error(`Failed to launch resolution agent for ${flag.flag_type}:`, err);
  }
}

async function handleDocstyleResolved(payload: {
  flag: ProjectInitFlag;
  chosenOption: string;
  cleanupStrategy: 'purge' | 'annotate' | 'preserve';
}) {
  showDocstyleModal.value = false;
  if (!props.project) return;

  if (payload.flag.id == null) {
    console.error('Docs flag has no id and cannot be resolved');
    return;
  }
  try {
    const agentId = await daemonCall('launch_flag_resolution_agent', {
      projectId: props.project.id,
      flagId: payload.flag.id,
      chosenOption: payload.chosenOption,
      cleanupStrategy: payload.cleanupStrategy,
      runner: null,
      model: null,
      effort: null,
    });

    await dialogueStore.openOverlay(agentId, props.project.path);
  } catch (err: unknown) {
    console.error('Failed to launch docs resolution agent:', err);
  }
}

async function handleDismissFlag(flag: ProjectInitFlag) {
  showDocstyleModal.value = false;
  if (!props.project?.id) return;
  try {
    await daemonCall('resolve_project_init_flag', { id: flag.id, status: 'dismissed' });
    coreStore.fetchInitFlags(props.project.id);
  } catch (e) {
    console.error(e);
  }
}

// Reactive check for initialization state (unattended or semi-attended background task)
const isInitializing = computed(() => {
  if (!props.project?.id) return false;
  const initRecord = coreStore.initializingProjects[props.project.id];
  if (initRecord) {
    return initRecord.isInitializing && initRecord.status === 'running';
  }
  const agents = (props.agentState || coreStore.agentStates[props.project.id])?.agents || [];
  const initAgent = agents.find((a) => a.id === 'initializer' || a.id === 'init');
  return !!(initAgent && initAgent.status === 'working');
});

const isRollingBack = ref(false);

const initStatusMessage = computed(() => {
  if (!props.project?.id) return '';
  const initRecord = coreStore.initializingProjects[props.project.id];
  if (initRecord?.message) return initRecord.message;
  const outcome = coreStore.getAgentOutcome('initializer', props.project?.path) || coreStore.getAgentOutcome('init', props.project?.path);
  if (outcome?.summary) return outcome.summary;
  return 'Bootstrapping project configuration & agent personas...';
});

async function handleCancelAndRollback() {
  if (isRollingBack.value || !props.project) return;
  isRollingBack.value = true;
  try {
    await coreStore.rollbackProjectInit(props.project.path);
    coreStore.clearProjectInitializing(props.project.id);
  } finally {
    isRollingBack.value = false;
  }
}
</script>

<template>
  <div class="flex-1 min-h-0 h-full flex flex-col overflow-hidden select-text relative font-mono">
    <template v-if="project">
      <!-- Full-bleed Scrollable Body (Spec 13 §4.1) -->
      <div class="flex-1 min-h-0 overflow-y-auto p-3 flex flex-col gap-2">
        <!-- Initialization Flags Banner (If any pending flags exist, aligned to GridLayout 8px margin) -->
        <div
          v-if="pendingFlags.length > 0"
          class="mx-[8px] border border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] p-2.5 flex flex-col gap-2"
        >
          <div class="flex items-center gap-1.5 text-[var(--status-warn-color)] font-bold tracking-wider uppercase text-[9px]">
            <AlertTriangle class="w-3.5 h-3.5" />
            <span>INITIALIZATION FLAGS ({{ pendingFlags.length }})</span>
          </div>
          <div class="flex flex-wrap items-center gap-2">
            <AppInitBadge
              v-for="flag in pendingFlags"
              :key="flag.id"
              :flag="flag"
              @resolve="handleResolveFlag"
              @dismiss="handleDismissFlag"
            />
          </div>
        </div>

        <!-- Dynamic 4-Column Plugin Widget Dashboard (Spec 37 §4) -->
        <OverviewWidgetGrid
          :project="project"
          :active-listening-project-id="activeListeningProjectId ?? null"
          :agent-state="agentState || coreStore.agentStates[project.id]"
          :summary="overviewSummary"
          :is-loading="isLoadingOverview"
          :is-indexing-knowledge="isIndexingKnowledge"
          :is-editing="isEditingOverview"
          @toggle-edit="isEditingOverview = !isEditingOverview"
          @save-layout="handleSaveWidgetLayout"
          @navigate-tab="handleNavigateTab"
          @index-knowledge="handleIndexKnowledge"
          @init-project="emit('initProject', $event)"
          @sync-skills="emit('syncSkills', $event)"
          @open-github="emit('openGithub', $event)"
          @init-git="emit('initGit', $event)"
        />
      </div>

      <!-- Brutalist Under Construction Blocking Overlay (When Initializing) -->
      <div 
        v-if="isInitializing" 
        class="absolute inset-0 z-30 ui-overlay-backdrop flex flex-col justify-between p-3 select-none"
      >
        <!-- Top Hazard Tape Stripe -->
        <div class="h-2 w-full hazard-stripes"></div>

        <!-- Central Status & Spinner Card -->
        <div class="flex flex-col items-center justify-center gap-3 my-auto text-center px-4">
          <div class="flex items-center gap-2 px-3 py-1 bg-[var(--status-warn-bg)] border border-[var(--status-warn-border)] text-[var(--status-warn-color)] font-mono text-xs font-semibold tracking-wider uppercase">
            <Loader2 class="w-3.5 h-3.5 animate-spin text-[var(--status-warn-color)]" />
            <span>ORCHESTRATION UNDER CONSTRUCTION</span>
          </div>
          
          <p class="font-mono text-xs text-[var(--fg-color)] max-w-md ui-surface-title border border-[var(--border-color)] px-3 py-2 text-left">
            <span class="text-[var(--accent-color)] font-bold">></span> {{ initStatusMessage }}
          </p>

          <!-- Actions: Cancel & Rollback -->
          <div class="flex items-center gap-2 mt-2">
            <AppTooltip text="Abort and rollback partially generated initialization files">
              <button
                @click="handleCancelAndRollback"
                :disabled="isRollingBack"
                class="text-[10px] font-mono text-[var(--status-error-color)] hover:opacity-80 flex items-center gap-1 border border-[var(--status-error-border)] px-2 py-0.5 transition-colors cursor-pointer bg-[var(--status-error-bg)]"
              >
                <X class="w-3 h-3" />
                <span>{{ isRollingBack ? 'Rolling back...' : '[✕] Cancel & Rollback' }}</span>
              </button>
            </AppTooltip>
          </div>
        </div>

        <!-- Bottom Hazard Tape Stripe -->
        <div class="h-2 w-full hazard-stripes"></div>
      </div>

      <!-- Docstyle Selection Modal for Phase 2 Docs Flag Resolution -->
      <DocstyleSelectionModal
        :show="showDocstyleModal"
        :project="project"
        :flag="activeDocstyleFlag"
        @close="showDocstyleModal = false"
        @dismiss="handleDismissFlag"
        @resolve="handleDocstyleResolved"
      />
    </template>
  </div>
</template>

<style scoped>
.hazard-stripes {
  background: repeating-linear-gradient(
    45deg,
    #000,
    #000 10px,
    #fbbf24 10px,
    #fbbf24 20px
  );
}
</style>
