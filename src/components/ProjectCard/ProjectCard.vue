<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue';
import { Loader2, X, AlertTriangle } from '@lucide/vue';
import CardHeader from './CardHeader.vue';
import DocstyleSelectionModal from '../modals/DocstyleSelectionModal.vue';
import { AppTooltip, AppInitBadge } from '../ui';
import { useCoreStore } from '../../stores/coreStore';
import { useDialogueStore } from '../../stores/dialogueStore';
import { daemonCall } from '../../utils/daemonClient';
import type { ProjectRecord, ProjectAgentState, ProjectInitFlagRecord } from '../../types';

const props = defineProps<{
  project: ProjectRecord;
  activeListeningProjectId: number | null;
  voiceMode: boolean;
  activeProjectMenuId: number | null;
  agentState?: ProjectAgentState;
  agentOutcomes?: Record<string, { status: string; timestamp: number }>;
  runningServices?: string[];
}>();

const emit = defineEmits<{
  (e: 'initProject', project: ProjectRecord): void;
  (e: 'syncSkills', project: ProjectRecord): void;
  (e: 'openGithub', project: ProjectRecord): void;
  (e: 'triggerVcs', project: ProjectRecord): void;
  (e: 'toggleMenu', projectId: number): void;
  (e: 'openSettings', project: ProjectRecord): void;
  (e: 'deleteProject', project: ProjectRecord): void;
  (e: 'initGit', project: ProjectRecord): void;
}>();

const coreStore = useCoreStore();
const dialogueStore = useDialogueStore();

const showDocstyleModal = ref(false);
const activeDocstyleFlag = ref<ProjectInitFlagRecord | null>(null);

onMounted(() => {
  coreStore.fetchInitFlags(props.project.id);
});

watch(() => props.project.id, (newId) => {
  coreStore.fetchInitFlags(newId);
});

const pendingFlags = computed<ProjectInitFlagRecord[]>(() => {
  const flags = coreStore.projectInitFlags[props.project.id] || [];
  return flags.filter((f) => f.status === 'pending');
});

async function handleResolveFlag(flag: ProjectInitFlagRecord) {
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
  flag: ProjectInitFlagRecord;
  chosenOption: string;
  cleanupStrategy: 'purge' | 'annotate' | 'preserve';
}) {
  showDocstyleModal.value = false;

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

async function handleDismissFlag(flag: ProjectInitFlagRecord) {
  showDocstyleModal.value = false;
  try {
    await daemonCall('resolve_project_init_flag', { id: flag.id, status: 'dismissed' });
    coreStore.fetchInitFlags(props.project.id);
  } catch (e) {
    console.error(e);
  }
}

// Reactive check for initialization state (unattended or semi-attended background task)
const isInitializing = computed(() => {
  const initRecord = coreStore.initializingProjects[props.project.id];
  if (initRecord) {
    return initRecord.isInitializing && initRecord.status === 'running';
  }
  // Fallback: check if an init / initializer agent is actively working
  const agents = props.agentState?.agents || [];
  const initAgent = agents.find(a => a.id === 'initializer' || a.id === 'init');
  return !!(initAgent && initAgent.status === 'working');
});

const isRollingBack = ref(false);

const initStatusMessage = computed(() => {
  const initRecord = coreStore.initializingProjects[props.project.id];
  if (initRecord?.message) return initRecord.message;
  const outcome = coreStore.getAgentOutcome('initializer', props.project.path) || coreStore.getAgentOutcome('init', props.project.path);
  if (outcome?.summary) return outcome.summary;
  return 'Bootstrapping project configuration & agent personas...';
});

const isActive = computed(() => coreStore.activeProjectId === props.project.id);

function handleCardClick(e: MouseEvent) {
  const target = e.target as HTMLElement;
  if (target.closest('button, a, input, select, textarea, .project-menu-container, [role="button"]')) {
    return;
  }
  coreStore.setActiveProject(props.project.id);
}

async function handleCancelAndRollback() {
  if (isRollingBack.value) return;
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
  <div 
    @click="handleCardClick"
    class="relative p-4 flex flex-col gap-4 overflow-hidden transition-all duration-200 cursor-pointer"
    :class="isActive 
      ? 'border-2 border-[var(--accent-color)] bg-[var(--ui-bg-card)]/30 shadow-[0_0_16px_color-mix(in_srgb,var(--accent-color)_12%,transparent)]' 
      : 'border border-[var(--border-color)] bg-transparent hover:border-[var(--accent-color)]/40'"
  >
    <!-- Active Project Banner / Indicator -->
    <div v-if="isActive" class="absolute top-0 left-0 right-0 h-0.5 bg-[var(--accent-color)]"></div>

    <!-- Header -->
    <CardHeader 
      :project="project"
      :is-active="isActive"
      :active-listening-project-id="activeListeningProjectId"
      :voice-mode="voiceMode"
      :active-project-menu-id="activeProjectMenuId"
      :agent-state="agentState || coreStore.agentStates[project.id]"
      @init-project="emit('initProject', $event)"
      @sync-skills="emit('syncSkills', $event)"
      @open-github="emit('openGithub', $event)"
      @trigger-vcs="emit('triggerVcs', $event)"
      @toggle-menu="emit('toggleMenu', $event)"
      @open-settings="emit('openSettings', $event)"
      @delete-project="emit('deleteProject', $event)"
      @init-git="emit('initGit', $event)"
    />

    <!-- Initialization Flags Pending -->
    <div v-if="pendingFlags.length > 0" class="border border-[var(--border-color)] bg-black/40 p-2.5 flex flex-col gap-2">
      <div class="flex items-center gap-1.5 text-[var(--dim-color)] font-bold tracking-wider uppercase text-[10px]">
        <AlertTriangle class="w-3.5 h-3.5 text-[var(--status-warn-color)]" />
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



    <!-- Brutalist Under Construction Blocking Overlay -->
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
        
        <p class="font-mono text-xs text-[var(--fg-color)] max-w-md bg-black/60 border border-[var(--border-color)] px-3 py-2 text-left">
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
