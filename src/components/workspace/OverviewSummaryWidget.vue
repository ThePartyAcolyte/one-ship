<script setup lang="ts">
import { ref, computed } from 'vue';
import {
  BrainCircuit,
  Cloud,
  Terminal,
  GitCompare,
  GitBranch,
  ChevronDown,
  Pencil,
  Check,
  X,
  Star,
} from '@lucide/vue';
import { AppTooltip } from '../ui';
import { useCoreStore } from '../../stores/coreStore';
import { daemonCall } from '../../utils/daemonClient';
import ProjectStatusLeds from './ProjectStatusLeds.vue';
import GitBranchModal from '../modals/GitBranchModal.vue';
import type { ProjectRecord, ProjectAgentState, AgentState } from '../../types';

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
const showBranchModal = ref(false);

const isTogglingFavorite = ref(false);
async function handleToggleFavorite() {
  if (!props.project || isTogglingFavorite.value) return;
  isTogglingFavorite.value = true;
  try {
    await coreStore.toggleProjectFavorite(props.project.id, !props.project.is_favorite);
  } catch (err: unknown) {
    console.error('[OverviewSummaryWidget] Failed to toggle favorite:', err);
  } finally {
    isTogglingFavorite.value = false;
  }
}

const isEditingName = ref(false);
const draftName = ref('');
const isSavingName = ref(false);

function startEditName() {
  draftName.value = props.project.name ?? '';
  isEditingName.value = true;
}

function cancelEditName() {
  isEditingName.value = false;
}

async function saveName() {
  const trimmed = draftName.value.trim();
  if (!trimmed || isSavingName.value) return;
  isSavingName.value = true;
  try {
    await daemonCall('update_project_name', {
      projectId: props.project.id,
      name: trimmed,
    });
    isEditingName.value = false;
  } catch (err: unknown) {
    console.error('[OverviewSummaryWidget] Failed to update project name:', err);
  } finally {
    isSavingName.value = false;
  }
}

const isEditingDescription = ref(false);
const draftDescription = ref('');
const isSavingDescription = ref(false);

function startEditDescription() {
  draftDescription.value = props.project.description ?? '';
  isEditingDescription.value = true;
}

function cancelEditDescription() {
  isEditingDescription.value = false;
}

async function saveDescription() {
  if (isSavingDescription.value) return;
  isSavingDescription.value = true;
  try {
    const trimmed = draftDescription.value.trim();
    await daemonCall('update_project_description', {
      projectId: props.project.id,
      description: trimmed ? trimmed : null,
    });
    isEditingDescription.value = false;
  } catch (err: unknown) {
    console.error('[OverviewSummaryWidget] Failed to update project description:', err);
  } finally {
    isSavingDescription.value = false;
  }
}

const resolvedAgentState = computed(() => props.agentState ?? coreStore.agentStates[props.project?.id]);
const hasActiveProcesses = computed(() => {
  const agents = resolvedAgentState.value?.agents || [];
  return agents.some((a: AgentState) => a.status !== 'unlaunched' && a.status !== 'completed' && a.status !== 'failed');
});

const isProjectInitialized = computed(() => {
  return Boolean(
    props.project.skill_status?.is_installed ||
    props.project.init_state?.status === 'completed'
  );
});

const skillTooltip = computed(() => {
  if (props.project.skill_status?.is_outdated) {
    if (props.project.skill_status?.is_symlinked === false) {
      return `Migrate Platform Assets to Symlinks (v${props.project.skill_status?.installed_version || 'legacy'} -> v${props.project.skill_status?.current_version})`;
    }
    if (props.project.skill_status?.needs_migration) {
      return `Upgrade Configuration Schema (v${props.project.skill_status?.schema_version || 'legacy'} -> v${props.project.skill_status?.current_version})`;
    }
    return `Update Platform Assets (v${props.project.skill_status?.installed_version || 'legacy'} -> v${props.project.skill_status?.current_version})`;
  }
  return `Platform Assets Synced & Symlinked (v${props.project.skill_status?.installed_version || props.project.skill_status?.current_version || '0.2.5'})`;
});
</script>

<template>
  <div class="flex flex-col justify-between gap-2 h-full min-h-0 font-mono">
    <!-- Top Row: Status Icons & Agent LEDs -->
    <div class="flex items-center justify-between gap-2 shrink-0">
      <div class="flex items-center gap-1.5">
        <AppTooltip v-if="isProjectInitialized" :text="skillTooltip" position="bottom">
          <button
            type="button"
            @click="emit('syncSkills', project)"
            class="relative p-1 transition-colors cursor-pointer border"
            :class="[
              project.skill_status?.is_outdated
                ? 'border-[var(--status-warn-border)] text-[var(--status-warn-color)] hover:border-[var(--status-warn-color)]'
                : 'border-[var(--status-ok-border)] text-[var(--status-ok-color)] hover:border-[var(--status-ok-color)]'
            ]"
          >
            <BrainCircuit :size="13" />
            <span
              v-if="project.skill_status?.is_outdated"
              class="absolute -top-1 -right-1 w-2 h-2 rounded-full bg-[var(--status-warn-color)] animate-pulse"
            />
          </button>
        </AppTooltip>

        <AppTooltip :text="project.git_status?.remote_url ? 'Remote: ' + project.git_status.remote_url : 'Connect Remote'" position="bottom">
          <button
            type="button"
            @click="emit('openGithub', project)"
            class="p-1 transition-colors cursor-pointer border"
            :class="project.git_status?.remote_url
              ? 'border-[var(--status-ok-border)] text-[var(--status-ok-color)] hover:border-[var(--status-ok-color)]'
              : 'border-transparent text-[var(--dim-color)] opacity-50 hover:opacity-100 hover:text-[var(--accent-color)]'"
          >
            <Cloud :size="13" />
          </button>
        </AppTooltip>

        <AppTooltip :text="hasActiveProcesses ? 'Active Processes' : 'Supervisor Idle'" position="bottom">
          <div
            class="p-1 transition-colors border cursor-default"
            :class="hasActiveProcesses
              ? 'border-[var(--status-ok-border)] text-[var(--status-ok-color)]'
              : 'border-transparent text-[var(--dim-color)] opacity-50'"
          >
            <Terminal :size="13" />
          </div>
        </AppTooltip>
      </div>

      <ProjectStatusLeds :project-id="project.id" :project-path="project.path" orientation="horizontal" />
    </div>

    <!-- Middle: Favorite Star + Editable Title & Description -->
    <div class="flex flex-col gap-1 min-w-0">
      <div class="flex items-center gap-1.5 min-w-0">
        <AppTooltip :text="project.is_favorite ? 'Remove from favorites' : 'Pin to favorites'" position="bottom">
          <button
            type="button"
            @click="handleToggleFavorite"
            :disabled="isTogglingFavorite"
            class="p-0.5 shrink-0 cursor-pointer transition-colors"
            :class="project.is_favorite ? 'text-[var(--status-warn-color)]' : 'text-[var(--dim-color)] hover:text-[var(--status-warn-color)]'"
          >
            <Star :size="13" :class="{ 'fill-[var(--status-warn-color)]': project.is_favorite }" />
          </button>
        </AppTooltip>

        <template v-if="isEditingName">
          <input
            v-model="draftName"
            type="text"
            placeholder="Project title..."
            @keydown.enter.prevent="saveName"
            @keydown.esc.prevent="cancelEditName"
            class="flex-1 min-w-0 px-1.5 py-0.5 text-xs font-bold font-mono bg-[var(--bg-color)] border border-[var(--accent-color)] text-[var(--fg-color)] focus:outline-none"
          />
          <AppTooltip text="Save title" position="bottom">
            <button
              type="button"
              @click="saveName"
              :disabled="isSavingName || !draftName.trim()"
              class="p-0.5 text-[var(--status-ok-color)] hover:opacity-80 cursor-pointer disabled:opacity-40 shrink-0"
            >
              <Check :size="12" />
            </button>
          </AppTooltip>
          <AppTooltip text="Cancel" position="bottom">
            <button
              type="button"
              @click="cancelEditName"
              class="p-0.5 text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer shrink-0"
            >
              <X :size="12" />
            </button>
          </AppTooltip>
        </template>
        <template v-else>
          <h2 class="text-sm font-bold tracking-tight text-[var(--accent-color)] truncate">
            {{ project.name }}
          </h2>
          <AppTooltip text="Edit project title" position="bottom">
            <button
              type="button"
              @click.stop="startEditName"
              class="p-0.5 text-[var(--dim-color)]/70 hover:text-[var(--accent-color)] transition-colors cursor-pointer shrink-0"
            >
              <Pencil :size="11" />
            </button>
          </AppTooltip>
          <button
            v-if="!isProjectInitialized"
            type="button"
            @click="emit('initProject', project)"
            class="ml-auto px-1.5 py-0.5 text-[9px] font-mono font-bold tracking-wider border border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-black transition-colors cursor-pointer shrink-0"
          >
            + INITIALIZE
          </button>
        </template>
      </div>

      <div class="flex items-center gap-1.5 min-w-0">
        <template v-if="isEditingDescription">
          <input
            v-model="draftDescription"
            type="text"
            placeholder="Project description..."
            @keydown.enter.prevent="saveDescription"
            @keydown.esc.prevent="cancelEditDescription"
            class="flex-1 min-w-0 px-1.5 py-0.5 text-[10px] font-mono bg-[var(--bg-color)] border border-[var(--accent-color)] text-[var(--fg-color)] focus:outline-none"
          />
          <AppTooltip text="Save description" position="bottom">
            <button
              type="button"
              @click="saveDescription"
              :disabled="isSavingDescription"
              class="p-0.5 text-[var(--status-ok-color)] hover:opacity-80 cursor-pointer shrink-0"
            >
              <Check :size="12" />
            </button>
          </AppTooltip>
          <AppTooltip text="Cancel" position="bottom">
            <button
              type="button"
              @click="cancelEditDescription"
              class="p-0.5 text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer shrink-0"
            >
              <X :size="12" />
            </button>
          </AppTooltip>
        </template>
        <template v-else>
          <span
            class="text-[10px] font-mono truncate"
            :class="project.description ? 'text-[var(--dim-color)]' : 'text-[var(--dim-color)]/60 italic'"
          >
            {{ project.description || 'No description' }}
          </span>
          <AppTooltip text="Edit project description" position="bottom">
            <button
              type="button"
              @click.stop="startEditDescription"
              class="p-0.5 text-[var(--dim-color)]/70 hover:text-[var(--accent-color)] transition-colors cursor-pointer shrink-0"
            >
              <Pencil :size="10" />
            </button>
          </AppTooltip>
        </template>
      </div>
    </div>

    <!-- Bottom Row: Git Branch & Metrics -->
    <div class="text-[11px] flex gap-2 items-center min-w-0 overflow-x-auto scrollbar-none shrink-0">
      <template v-if="project.has_git && project.git_status">
        <AppTooltip text="Manage Git branches" position="bottom">
          <button
            type="button"
            @click.stop="showBranchModal = true"
            class="flex items-center gap-1 font-bold px-1.5 py-0.5 -ml-1.5 border border-transparent hover:border-[var(--accent-color)]/60 hover:bg-[var(--accent-color)]/10 transition-colors cursor-pointer shrink-0"
            :class="project.git_status.is_clean
              ? 'text-[var(--accent-color)]'
              : (project.git_status.has_commits ? 'text-[var(--status-warn-color)]' : 'text-[var(--dim-color)]')"
          >
            <component
              :is="project.git_status.has_remote ? GitCompare : GitBranch"
              :size="12"
            />
            <span class="truncate max-w-[140px]">{{ project.git_status.branch ?? 'detached' }}</span>
            <ChevronDown :size="10" class="opacity-60" />
          </button>
        </AppTooltip>

        <div class="flex items-center gap-1.5 text-[10px] font-mono shrink-0">
          <AppTooltip v-if="project.git_status.modified_count > 0" text="Modified" position="top">
            <span class="text-[var(--status-warn-color)] cursor-default">!{{ project.git_status.modified_count }}</span>
          </AppTooltip>
          <AppTooltip v-if="project.git_status.untracked_count > 0" text="Untracked" position="top">
            <span class="text-[var(--dim-color)] cursor-default">?{{ project.git_status.untracked_count }}</span>
          </AppTooltip>
          <AppTooltip v-if="project.git_status.staged_count > 0" text="Staged" position="top">
            <span class="text-[var(--accent-color)] cursor-default">+{{ project.git_status.staged_count }}</span>
          </AppTooltip>
          <AppTooltip v-if="project.git_status.ahead_count > 0" text="Ahead" position="top">
            <span class="text-[var(--status-info-color)] cursor-default">↑{{ project.git_status.ahead_count }}</span>
          </AppTooltip>
          <AppTooltip v-if="project.git_status.behind_count > 0" text="Behind" position="top">
            <span class="text-[var(--status-error-color)] cursor-default">↓{{ project.git_status.behind_count }}</span>
          </AppTooltip>
          <span v-if="project.git_status.is_clean" class="text-[var(--dim-color)] opacity-60 text-[9px]">clean</span>
        </div>

        <span v-if="activeListeningProjectId === project.id" class="text-[9px] text-[var(--accent-color)] animate-pulse font-bold shrink-0">
          [LISTENING]
        </span>
      </template>
      <AppTooltip v-else-if="project.has_git" :text="project.git_error ?? 'Git status unavailable'" position="top">
        <span class="text-[10px] font-bold text-[var(--status-error-color)] cursor-default">[! GIT]</span>
      </AppTooltip>
      <template v-else>
        <button
          type="button"
          @click="emit('initGit', project)"
          class="border border-[var(--border-color)] text-[9px] px-2 py-0.5 hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] transition-colors cursor-pointer text-[var(--dim-color)]"
        >
          + INIT GIT
        </button>
      </template>
    </div>

    <GitBranchModal
      v-if="showBranchModal"
      :project="project"
      @close="showBranchModal = false"
    />
  </div>
</template>
