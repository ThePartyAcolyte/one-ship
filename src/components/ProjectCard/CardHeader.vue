<script setup lang="ts">
import { ref, computed } from 'vue';
import { 
  BrainCircuit, 
  Cloud, 
  GitCommitHorizontal, 
  MoreVertical, 
  Settings, 
  Trash2, 
  GitCompare, 
  GitBranch,
  ChevronDown,
  Terminal,
  History,
  Pencil,
  Check,
  X
} from '@lucide/vue';
import { useI18n } from 'vue-i18n';
import { AppTooltip, AppInput } from '../ui';
import { useCoreStore } from '../../stores/coreStore';
import { daemonCall } from '../../utils/daemonClient';
import { reportAsyncFailure } from '../../utils/reportAsyncFailure';
import ProjectStatusLeds from '../workspace/ProjectStatusLeds.vue';
import GitBranchModal from '../modals/GitBranchModal.vue';
import type { ProjectRecord, ProjectAgentState, AgentState } from '../../types';

const props = defineProps<{
  project: ProjectRecord;
  isActive?: boolean;
  activeListeningProjectId: number | null;
  voiceMode: boolean;
  activeProjectMenuId: number | null;
  agentState?: ProjectAgentState;
}>();

const coreStore = useCoreStore();
const showBranchModal = ref(false);
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
    reportAsyncFailure('CardHeader.saveDescription', err);
  } finally {
    isSavingDescription.value = false;
  }
}
const agentState = computed(() => props.agentState ?? coreStore.agentStates[props.project?.id]);
const hasActiveProcesses = computed(() => {
  const agents = agentState.value?.agents || [];
  return agents.some((a: AgentState) => a.status !== 'unlaunched' && a.status !== 'completed' && a.status !== 'failed');
});

const emit = defineEmits<{
  (e: 'initProject', project: ProjectRecord): void;
  (e: 'syncSkills', project: ProjectRecord): void;
  (e: 'openGithub', project: ProjectRecord): void;
  (e: 'triggerVcs', project: ProjectRecord): void;
  (e: 'openSnapshots', project: ProjectRecord): void;
  (e: 'toggleMenu', projectId: number): void;
  (e: 'openSettings', project: ProjectRecord): void;
  (e: 'deleteProject', project: ProjectRecord): void;
  (e: 'initGit', project: ProjectRecord): void;
}>();

const { t } = useI18n();

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
  <div class="flex flex-col border-b border-[var(--border-color)] pb-3">
    <!-- Toolbar Superior -->
    <div class="flex justify-between w-full mb-1.5">
      <div class="flex gap-2">
        <!-- Platform Skill & Assets Synchronization (Only when project is initialized) -->
        <AppTooltip v-if="isProjectInitialized" :text="skillTooltip" position="bottom">
          <button 
            @click="emit('syncSkills', project)"
            class="relative p-1 transition-colors cursor-pointer border"
            :class="[
              project.skill_status?.is_outdated
                ? 'border-[var(--status-warn-border)] text-[var(--status-warn-color)] hover:border-[var(--status-warn-color)]'
                : 'border-[var(--status-ok-border)] text-[var(--status-ok-color)] hover:border-[var(--status-ok-color)]'
            ]"
          >
            <BrainCircuit :size="16" />
            <!-- Notification badge for outdated skills -->
            <span 
              v-if="project.skill_status?.is_outdated" 
              class="absolute -top-1 -right-1 w-2 h-2 rounded-full bg-[var(--status-warn-color)] animate-pulse"
            />
          </button>
        </AppTooltip>

        <!-- GitHub / Remote -->
        <AppTooltip :text="project.git_status?.remote_url ? 'Remote: ' + project.git_status.remote_url : 'Connect Remote'" position="bottom">
          <button 
            @click="emit('openGithub', project)"
            class="p-1 transition-colors cursor-pointer border"
            :class="project.git_status?.remote_url 
              ? 'border-[var(--status-ok-border)] text-[var(--status-ok-color)] hover:border-[var(--status-ok-color)]' 
              : 'border-transparent text-[var(--dim-color)] opacity-50 hover:opacity-100 hover:text-[var(--accent-color)]'"
          >
            <Cloud :size="16" />
          </button>
        </AppTooltip>

        <!-- Process Supervisor Indicator -->
        <AppTooltip :text="hasActiveProcesses ? 'Active Processes' : 'Supervisor Idle'" position="bottom">
          <div 
            class="p-1 transition-colors border cursor-default"
            :class="hasActiveProcesses 
              ? 'border-[var(--status-ok-border)] text-[var(--status-ok-color)] hover:border-[var(--status-ok-color)]' 
              : 'border-transparent text-[var(--dim-color)] opacity-50'"
          >
            <Terminal :size="16" />
          </div>
        </AppTooltip>
      </div>

      <div class="flex gap-2">
        <!-- VCS Agent Action -->
        <AppTooltip v-if="project.has_git" :text="t('vcs.action')" position="bottom">
          <button 
            @click="emit('triggerVcs', project)" 
            class="p-1 transition-colors cursor-pointer border"
            :class="[
              project.git_status && !project.git_status.is_clean 
                ? 'border-[var(--status-warn-border)] text-[var(--status-warn-color)] hover:border-[var(--status-warn-color)]' 
                : 'border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)]'
            ]"
          >
            <GitCommitHorizontal :size="16" />
          </button>
        </AppTooltip>

        <!-- Snapshots & Rollback -->
        <AppTooltip v-if="project.has_git" text="Snapshots & Safety Rollback" position="bottom">
          <button 
            @click="emit('openSnapshots', project)" 
            class="p-1 transition-colors cursor-pointer border border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)]"
          >
            <History :size="16" />
          </button>
        </AppTooltip>

        <!-- Context Menu (3 Dots) -->
        <div class="relative project-menu-container">
          <AppTooltip text="Options" position="bottom">
            <button 
              @click.stop="emit('toggleMenu', project.id)" 
              class="p-1 transition-colors text-[var(--dim-color)] hover:text-[var(--accent-color)] cursor-pointer border border-[var(--border-color)] hover:border-[var(--accent-color)]"
            >
              <MoreVertical :size="16" />
            </button>
          </AppTooltip>

          <!-- Dropdown Submenu -->
          <div 
            v-if="activeProjectMenuId === project.id"
            class="absolute right-0 top-7 w-44 bg-[var(--ui-bg-title)] border border-[var(--border-color)] shadow-2xl z-50 flex flex-col text-xs font-mono py-1"
          >
            <button 
              @click="emit('openSettings', project)"
              class="px-3 py-1.5 text-left text-[var(--fg-color)] hover:bg-[var(--accent-color)] hover:text-black transition-colors cursor-pointer flex items-center gap-2"
            >
              <Settings :size="13" />
              <span>{{ t('settings.project') }}</span>
            </button>
            <button 
              v-if="project.has_git"
              @click="emit('openSnapshots', project)"
              class="px-3 py-1.5 text-left text-[var(--fg-color)] hover:bg-[var(--accent-color)] hover:text-black transition-colors cursor-pointer flex items-center gap-2"
            >
              <History :size="13" />
              <span>Snapshots & Diff</span>
            </button>
            <div class="border-t border-[var(--border-color)] my-1"></div>
            <button 
              @click="emit('deleteProject', project)"
              class="px-3 py-1.5 text-left text-[var(--status-error-color)] hover:bg-[var(--status-error-color)] hover:text-black transition-colors cursor-pointer flex items-center gap-2"
            >
              <Trash2 :size="13" />
              <span>{{ t('card.remove_project') }}</span>
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- Title and Git Status -->
    <div class="flex items-start justify-between gap-2">
      <div class="flex flex-col min-w-0">
        <div class="flex items-center gap-2 flex-wrap">
          <h2 class="text-lg font-bold tracking-tight text-[var(--accent-color)] truncate">{{ project.name }}</h2>
          <button
            v-if="!isProjectInitialized"
            @click="emit('initProject', project)"
            class="px-2 py-0.5 text-[10px] font-mono font-bold tracking-wider border border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-black transition-colors cursor-pointer"
          >
            + INITIALIZE
          </button>
          <ProjectStatusLeds :project-id="project.id" :project-path="project.path" orientation="horizontal" />
        </div>
        <div class="mt-0.5 flex items-center gap-1.5 min-w-0">
          <template v-if="isEditingDescription">
            <AppInput
              v-model="draftDescription"
              type="text"
              placeholder="Project description..."
              @keydown.enter.prevent="saveDescription"
              @keydown.esc.prevent="cancelEditDescription"
              class="flex-1 min-w-0 !h-auto !px-1.5 !py-0.5 text-[11px] !bg-[var(--bg-color)] !border-[var(--accent-color)]"
            />
            <AppTooltip text="Save description" position="bottom">
              <button
                type="button"
                @click="saveDescription"
                :disabled="isSavingDescription"
                class="p-0.5 text-[var(--status-ok-color)] hover:opacity-80 cursor-pointer"
              >
                <Check :size="12" />
              </button>
            </AppTooltip>
            <AppTooltip text="Cancel" position="bottom">
              <button
                type="button"
                @click="cancelEditDescription"
                class="p-0.5 text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer"
              >
                <X :size="12" />
              </button>
            </AppTooltip>
          </template>
          <template v-else>
            <span
              class="text-[11px] font-mono truncate"
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
                <Pencil :size="11" />
              </button>
            </AppTooltip>
          </template>
        </div>
        <div class="text-xs mt-1 flex gap-3 items-center">
          <!-- Dual-Signification Git Status (clickable to manage branches) -->
          <template v-if="project.has_git && project.git_status">
            <AppTooltip text="Manage Git branches" position="bottom">
              <button
                type="button"
                @click.stop="showBranchModal = true"
                class="flex items-center gap-1.5 font-bold px-1.5 py-0.5 -ml-1.5 border border-transparent hover:border-[var(--accent-color)]/60 hover:bg-[var(--accent-color)]/10 transition-colors cursor-pointer"
                :class="project.git_status.is_clean 
                  ? 'text-[var(--accent-color)]' 
                  : (project.git_status.has_commits ? 'text-[var(--status-warn-color)]' : 'text-[var(--dim-color)]')"
              >
                <component 
                  :is="project.git_status.has_remote ? GitCompare : GitBranch" 
                  :size="13" 
                />
                <span>{{ project.git_status.branch ?? 'detached' }}</span>
                <ChevronDown :size="11" class="opacity-60" />
              </button>
            </AppTooltip>

            <!-- Symbolic Metrics -->
            <div class="flex items-center gap-2 text-[11px] font-mono">
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
              <span v-if="project.git_status.is_clean" class="text-[var(--dim-color)] opacity-60 text-[10px]">clean</span>
            </div>

            <!-- Listening indicator when Voice Mode is active -->
            <span v-if="activeListeningProjectId === project.id" class="text-[10px] text-[var(--accent-color)] animate-pulse font-bold">
              [LISTENING]
            </span>
          </template>
          <AppTooltip v-else-if="project.has_git" :text="project.git_error ?? 'Git status unavailable'" position="top">
            <span class="text-[10px] font-bold text-[var(--status-error-color)] cursor-default">[! GIT]</span>
          </AppTooltip>
          <template v-else>
            <button
              @click="emit('initGit', project)"
              class="border border-[var(--border-color)] text-[10px] px-2 py-0.5 hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] transition-colors cursor-pointer text-[var(--dim-color)]"
            >
              + INIT GIT
            </button>
          </template>
        </div>
      </div>
    </div>
    <GitBranchModal
      v-if="showBranchModal"
      :project="project"
      @close="showBranchModal = false"
    />
  </div>
</template>
