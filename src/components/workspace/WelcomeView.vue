<script setup lang="ts">
import { computed, ref } from 'vue';
import {
  Plus,
  FolderGit2,
  ArrowRight,
  AlertTriangle,
  BookOpen,
  Terminal,
  GitBranch,
  FolderOpen,
  Star,
  Archive,
  ArchiveRestore,
  Cpu,
} from '@lucide/vue';
import type { DiagnosticState, ProjectRecord, ProjectAgentState } from '../../types';
import { useCoreStore } from '../../stores/coreStore';
import { AppToggle, AppTooltip } from '../ui';
import { reportAsyncFailure } from '../../utils/reportAsyncFailure';

const props = defineProps<{
  projects: ProjectRecord[];
  agentStates: Record<number, ProjectAgentState>;
  diagnostics?: DiagnosticState | null;
}>();

const emit = defineEmits<{
  (e: 'addProject'): void;
  (e: 'openProject', project: ProjectRecord): void;
  (e: 'continueToWorkspace'): void;
  (e: 'openSetup'): void;
}>();

const coreStore = useCoreStore();
const isSavingPref = ref(false);
const prefError = ref<string | null>(null);
const unarchivingProjectId = ref<number | null>(null);
const projectActionError = ref<string | null>(null);

const hasProjects = computed(() => props.projects.length > 0);
const activeProjects = computed(() => props.projects.filter((p) => !p.is_archived));
const archivedProjects = computed(() => props.projects.filter((p) => p.is_archived));

const installedRunners = computed(() => coreStore.installedRunners ?? []);
const hasAnyRunner = computed(() => installedRunners.value.length > 0);
const hasGitIdentity = computed(() =>
  Boolean(props.diagnostics?.git_user_name?.trim() && props.diagnostics?.git_user_email?.trim())
);
const projectsRootReady = computed(() => Boolean(props.diagnostics?.projects_root_exists));
const daemonConnected = computed(() => coreStore.daemonStatus.state === 'connected');

const missingReadinessItems = computed(() => {
  const items = [
    {
      id: 'daemon',
      label: 'MCP Daemon',
      ok: daemonConnected.value,
      detail: coreStore.daemonStatus.state,
      icon: Cpu,
    },
    {
      id: 'root',
      label: 'Projects Directory',
      ok: projectsRootReady.value,
      detail: 'Directory missing',
      icon: FolderOpen,
    },
    {
      id: 'git',
      label: 'Git & Identity',
      ok: Boolean(props.diagnostics?.git_installed && hasGitIdentity.value),
      detail: !props.diagnostics?.git_installed ? 'Git not installed' : 'Identity missing',
      icon: GitBranch,
    },
    {
      id: 'runners',
      label: 'AI Agent Runners',
      ok: hasAnyRunner.value,
      detail: 'None detected',
      icon: Terminal,
    },
  ];
  return items.filter((item) => !item.ok);
});

const activeAgentsCount = computed(() => {
  let count = 0;
  for (const p of activeProjects.value) {
    const st = props.agentStates[p.id];
    if (st && st.status !== 'idle') {
      count += 1;
    }
  }
  return count;
});

function activeAgentLabel(projectId: number): string | null {
  const st = props.agentStates[projectId];
  if (!st || st.status === 'idle') return null;
  return st.status;
}

async function unarchiveProject(project: ProjectRecord) {
  if (unarchivingProjectId.value !== null) return;
  unarchivingProjectId.value = project.id;
  projectActionError.value = null;
  try {
    await coreStore.toggleProjectArchived(project.id, false);
  } catch (err: unknown) {
    projectActionError.value = err instanceof Error ? err.message : String(err);
    reportAsyncFailure('WelcomeView.unarchiveProject', err);
  } finally {
    unarchivingProjectId.value = null;
  }
}

async function toggleShowOnStartup(nextEnabled: boolean) {
  if (isSavingPref.value) return;
  isSavingPref.value = true;
  prefError.value = null;
  try {
    await coreStore.setShowWelcomeOnStartup(nextEnabled);
  } catch (err: unknown) {
    prefError.value = err instanceof Error ? err.message : String(err);
    reportAsyncFailure('WelcomeView.toggleShowOnStartup', err);
  } finally {
    isSavingPref.value = false;
  }
}
</script>

<template>
  <div class="flex-1 flex flex-col min-h-0 min-w-0 bg-[var(--bg-color)] text-[var(--fg-color)] font-mono overflow-hidden">
    <!-- Scrollable Center Stage -->
    <div class="flex-1 overflow-y-auto p-5 sm:p-8">
      <div class="max-w-5xl mx-auto flex flex-col gap-6">
        <!-- Hero Banner -->
        <section class="border border-[var(--border-color)] ui-surface-card p-5 sm:p-6 flex flex-col md:flex-row md:items-center md:justify-between gap-4">
          <div class="flex flex-col gap-1.5 min-w-0">
            <div class="flex items-center gap-2.5">
              <span class="text-[10px] font-bold uppercase tracking-widest px-1.5 py-0.5 border border-[var(--accent-color)] text-[var(--accent-color)]">
                Welcome
              </span>
              <span class="text-[10px] text-[var(--dim-color)] uppercase tracking-wider">
                ONE-SHIP Engineering Workspace
              </span>
            </div>
            <h1 class="text-lg sm:text-xl font-bold tracking-tight text-[var(--fg-color)]">
              {{ activeProjects.length > 0 ? 'Select a Project or Start a New One' : 'Set Up Your First Project' }}
            </h1>
            <p class="text-xs text-[var(--dim-color)] max-w-2xl leading-relaxed">
              {{
                activeProjects.length > 0
                  ? 'Resume work on a tracked repository or register a new project.'
                  : 'No active projects are available yet. Track an existing local repository, scaffold a new greenfield workspace, or unarchive a project below.'
              }}
            </p>
          </div>

          <div class="flex flex-wrap items-center gap-2 shrink-0">
            <button
              type="button"
              @click="emit('addProject')"
              class="px-3.5 py-2 bg-[var(--accent-color)] text-[var(--bg-color)] text-xs font-bold uppercase tracking-wider flex items-center gap-2 cursor-pointer hover:opacity-90 transition-opacity"
            >
              <Plus :size="14" />
              <span>{{ hasProjects ? 'Add / New Project' : 'Create or Track Project' }}</span>
            </button>

            <button
              v-if="hasProjects"
              type="button"
              @click="emit('continueToWorkspace')"
              class="px-3.5 py-2 border border-[var(--border-color)] hover:border-[var(--accent-color)] text-[var(--fg-color)] hover:text-[var(--accent-color)] text-xs font-bold uppercase tracking-wider flex items-center gap-2 cursor-pointer transition-colors"
            >
              <span>Workspace</span>
              <ArrowRight :size="13" />
            </button>
          </div>
        </section>

        <!-- Main Two-Column Grid -->
        <div class="grid grid-cols-1 lg:grid-cols-12 gap-5 items-start">
          <!-- Left Column: Projects -->
          <section class="lg:col-span-7 border border-[var(--border-color)] ui-surface-card flex flex-col min-w-0">
            <div class="px-4 py-2.5 border-b border-[var(--border-color)] ui-surface-title flex items-center justify-between gap-2">
              <div class="flex items-center gap-2">
                <FolderGit2 :size="13" class="text-[var(--accent-color)]" />
                <h2 class="text-[11px] font-bold uppercase tracking-wider">
                  Projects
                </h2>
              </div>
              <div class="flex items-center gap-3 text-[10px] text-[var(--dim-color)]">
                <span>{{ activeProjects.length }} active</span>
                <span v-if="archivedProjects.length > 0" class="flex items-center gap-1">
                  <Archive :size="10" />
                  {{ archivedProjects.length }} archived
                </span>
                <span v-if="activeAgentsCount > 0" class="text-[var(--status-ok-color)] font-bold">
                  {{ activeAgentsCount }} running
                </span>
              </div>
            </div>

            <div
              v-if="projectActionError"
              class="px-4 py-2 border-b border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] text-[10px]"
            >
              {{ projectActionError }}
            </div>

            <!-- Empty Projects Onboarding -->
            <div v-if="!hasProjects" class="p-6 flex flex-col items-center text-center gap-3">
              <div class="w-10 h-10 border border-dashed border-[var(--accent-color)]/60 flex items-center justify-center text-[var(--accent-color)]">
                <FolderGit2 :size="18" />
              </div>
              <div class="flex flex-col gap-1 max-w-md">
                <p class="text-xs font-bold text-[var(--fg-color)]">
                  Your workspace is empty
                </p>
                <p class="text-[11px] text-[var(--dim-color)] leading-relaxed">
                  Add an existing Git repository or directory, or scaffold a new greenfield project. Once a project is registered, the project rail, tabs, and agent taskbar become active.
                </p>
              </div>
              <button
                type="button"
                @click="emit('addProject')"
                class="mt-1 px-3 py-1.5 border border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-[var(--bg-color)] text-[10px] font-bold uppercase tracking-wider cursor-pointer transition-colors"
              >
                + Track First Project
              </button>
            </div>

            <!-- Tracked Projects List (Active first, then Archived separator) -->
            <div v-else class="max-h-[420px] overflow-y-auto">
              <div v-if="activeProjects.length === 0" class="px-4 py-4 text-[11px] text-[var(--dim-color)]">
                All registered projects are archived. Unarchive one below or add a new project.
              </div>

              <div v-else class="divide-y divide-[var(--border-color)]/60">
                <button
                  v-for="project in activeProjects"
                  :key="project.id"
                  type="button"
                  @click="emit('openProject', project)"
                  class="w-full px-4 py-3 text-left flex items-center justify-between gap-3 hover:bg-[var(--accent-color)]/10 transition-colors cursor-pointer group"
                >
                  <div class="flex flex-col gap-0.5 min-w-0">
                    <div class="flex items-center gap-2 min-w-0">
                      <Star v-if="project.is_favorite" :size="11" class="text-[var(--accent-color)] shrink-0" />
                      <span class="text-xs font-bold text-[var(--fg-color)] group-hover:text-[var(--accent-color)] truncate">
                        {{ project.name }}
                      </span>
                    </div>
                    <span class="text-[10px] text-[var(--dim-color)] truncate">
                      {{ project.path }}
                    </span>
                  </div>

                  <div class="flex items-center gap-2.5 shrink-0">
                    <span
                      v-if="activeAgentLabel(project.id)"
                      class="text-[9px] px-1.5 py-0.5 border border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)] uppercase font-bold"
                    >
                      {{ activeAgentLabel(project.id) }}
                    </span>
                    <ArrowRight :size="12" class="text-[var(--dim-color)] group-hover:text-[var(--accent-color)]" />
                  </div>
                </button>
              </div>

              <!-- Archived Projects Separator & List -->
              <template v-if="archivedProjects.length > 0">
                <div class="px-4 py-1.5 border-y border-[var(--border-color)] bg-[var(--ui-bg-title)] flex items-center gap-2 text-[9px] font-bold uppercase tracking-widest text-[var(--dim-color)]">
                  <Archive :size="10" />
                  <span>Archived Projects</span>
                </div>
                <div class="divide-y divide-[var(--border-color)]/40">
                  <div
                    v-for="project in archivedProjects"
                    :key="project.id"
                    class="px-4 py-2.5 flex items-center justify-between gap-3 opacity-80"
                  >
                    <div class="flex flex-col gap-0.5 min-w-0">
                      <span class="text-xs font-bold text-[var(--dim-color)] truncate">
                        {{ project.name }}
                      </span>
                      <span class="text-[10px] text-[var(--dim-color)] truncate">
                        {{ project.path }}
                      </span>
                    </div>

                    <button
                      type="button"
                      @click="unarchiveProject(project)"
                      :disabled="unarchivingProjectId === project.id"
                      class="px-2.5 py-1 border border-[var(--border-color)] hover:border-[var(--accent-color)] text-[var(--fg-color)] hover:text-[var(--accent-color)] text-[9px] font-bold uppercase tracking-wider flex items-center gap-1.5 cursor-pointer transition-colors shrink-0 disabled:opacity-40"
                    >
                      <ArchiveRestore :size="11" />
                      <span>Unarchive</span>
                    </button>
                  </div>
                </div>
              </template>
            </div>
          </section>

          <!-- Right Column: Missing Workstation Items (only if incomplete) + Documentation -->
          <div class="lg:col-span-5 flex flex-col gap-5 min-w-0">
            <section
              v-if="missingReadinessItems.length > 0"
              class="border border-[var(--status-warn-border)] ui-surface-card flex flex-col"
            >
              <div class="px-4 py-2.5 border-b border-[var(--border-color)] ui-surface-title flex items-center gap-2">
                <AlertTriangle :size="13" class="text-[var(--status-warn-color)]" />
                <h2 class="text-[11px] font-bold uppercase tracking-wider">
                  Missing Configuration
                </h2>
              </div>

              <div class="p-3.5 flex flex-col gap-2">
                <div
                  v-for="item in missingReadinessItems"
                  :key="item.id"
                  class="px-2.5 py-2 border border-[var(--border-color)]/60 flex items-center justify-between gap-2 text-[10px]"
                >
                  <div class="flex items-center gap-2 min-w-0">
                    <component :is="item.icon" :size="12" class="text-[var(--dim-color)] shrink-0" />
                    <span class="font-bold text-[var(--fg-color)] shrink-0">{{ item.label }}</span>
                  </div>
                  <div class="flex items-center gap-1.5 min-w-0">
                    <span class="text-[var(--status-warn-color)] truncate max-w-[140px]">{{ item.detail }}</span>
                    <AlertTriangle :size="12" class="text-[var(--status-warn-color)] shrink-0" />
                  </div>
                </div>

                <button
                  type="button"
                  @click="emit('openSetup')"
                  class="mt-1 w-full px-2.5 py-1.5 border border-[var(--border-color)] hover:border-[var(--accent-color)] text-[10px] font-bold uppercase tracking-wider cursor-pointer transition-colors"
                >
                  Open Setup Checklist
                </button>
              </div>
            </section>

            <!-- Documentation & Manual (In Development) -->
            <section class="border border-[var(--border-color)] ui-surface-card flex flex-col">
              <div class="px-4 py-2.5 border-b border-[var(--border-color)] ui-surface-title flex items-center justify-between gap-2">
                <div class="flex items-center gap-2">
                  <BookOpen :size="13" class="text-[var(--accent-color)]" />
                  <h2 class="text-[11px] font-bold uppercase tracking-wider">
                    Documentation &amp; Manual
                  </h2>
                </div>
                <span class="text-[9px] px-1.5 py-0.5 border border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)] font-bold uppercase">
                  In Development
                </span>
              </div>
              <div class="p-3.5 flex flex-col gap-2.5 text-[11px] text-[var(--dim-color)]">
                <p class="leading-relaxed">
                  Interactive user manual, agent workflow guides, and hardware integration docs are currently being authored.
                </p>
                <div class="flex items-center justify-between gap-2 pt-1 border-t border-[var(--border-color)]/50 text-[10px]">
                  <span class="text-[var(--dim-color)]">Manual &amp; Guides</span>
                  <AppTooltip text="Documentation portal is under development">
                    <button
                      type="button"
                      disabled
                      class="px-2.5 py-1 border border-[var(--border-color)] text-[var(--dim-color)] opacity-60 cursor-not-allowed uppercase font-bold text-[9px]"
                    >
                      Coming Soon
                    </button>
                  </AppTooltip>
                </div>
              </div>
            </section>
          </div>
        </div>
      </div>
    </div>

    <!-- Bottom Preference Bar -->
    <footer class="h-10 px-4 sm:px-6 border-t border-[var(--border-color)] ui-surface-title flex items-center justify-between gap-3 shrink-0 text-[10px]">
      <div class="flex items-center gap-3 min-w-0">
        <AppToggle
          :model-value="coreStore.showWelcomeOnStartup"
          :disabled="isSavingPref"
          @update:model-value="toggleShowOnStartup"
        />
        <span class="text-[var(--fg-color)] select-none">
          Show welcome screen on startup
        </span>
        <span v-if="!hasProjects" class="text-[var(--dim-color)] hidden sm:inline">
          (Required while no projects are registered)
        </span>
        <span v-if="prefError" class="text-[var(--status-error-color)] truncate">
          {{ prefError }}
        </span>
      </div>
      <div class="text-[var(--dim-color)] shrink-0">
        {{ projects.length }} {{ projects.length === 1 ? 'project' : 'projects' }} registered
      </div>
    </footer>
  </div>
</template>
