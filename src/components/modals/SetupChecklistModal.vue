<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue';
import {
  X,
  Check,
  AlertTriangle,
  FolderOpen,
  GitBranch,
  Terminal,
  Wrench,
  RefreshCw,
  Download,
  Settings,
  Loader2,
} from '@lucide/vue';
import { open } from '@tauri-apps/plugin-dialog';
import type { DiagnosticState } from '../../types';
import { useCoreStore } from '../../stores/coreStore';
import { daemonCall } from '../../utils/daemonClient';
import { AppInput, AppTooltip } from '../ui';
import { INTEGRATION_INSTALL_GUIDES, RUNNER_INSTALL_GUIDES } from '../../utils/integrationGuides';

export const ONBOARDING_DISMISSED_KEY = 'oneship-onboarding-dismissed-v1';

const props = defineProps<{
  show: boolean;
  diagnostics: DiagnosticState | null;
  isInstallingTools?: boolean;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'refreshDiagnostics'): void;
  (e: 'installTools', toolIds: string[]): void;
  (e: 'openIntegrations'): void;
}>();

const coreStore = useCoreStore();

const projectsRootInput = ref('');
const isSavingRoot = ref(false);
const rootStatusMsg = ref<string | null>(null);

const gitNameInput = ref('');
const gitEmailInput = ref('');
const isSavingGitIdentity = ref(false);
const gitStatusMsg = ref<string | null>(null);

const isRefreshingRunners = ref(false);
const dontShowAgain = ref(false);
const actionError = ref<string | null>(null);

const installedRunners = computed(() => coreStore.installedRunners ?? []);
const hasAnyRunner = computed(() => installedRunners.value.length > 0);
const hasGitIdentity = computed(() =>
  Boolean(props.diagnostics?.git_user_name?.trim() && props.diagnostics?.git_user_email?.trim())
);
const projectsRootReady = computed(() => Boolean(props.diagnostics?.projects_root_exists));
const integrationsList = computed(() => coreStore.hostCapabilities?.integrations ?? []);

watch(
  () => [props.show, props.diagnostics] as const,
  ([isOpen, diag]) => {
    if (!isOpen) return;
    actionError.value = null;
    rootStatusMsg.value = null;
    gitStatusMsg.value = null;
    dontShowAgain.value = localStorage.getItem(ONBOARDING_DISMISSED_KEY) === '1';
    if (diag) {
      if (!projectsRootInput.value) {
        projectsRootInput.value = diag.default_projects_root_path || '';
      }
      if (!gitNameInput.value && diag.git_user_name) {
        gitNameInput.value = diag.git_user_name;
      }
      if (!gitEmailInput.value && diag.git_user_email) {
        gitEmailInput.value = diag.git_user_email;
      }
    }
  },
  { immediate: true }
);

async function browseProjectsRoot() {
  const selected = await open({
    directory: true,
    multiple: false,
    title: 'Select Default Projects Directory',
  });
  if (selected && typeof selected === 'string') {
    projectsRootInput.value = selected;
  }
}

async function saveProjectsRoot() {
  const path = projectsRootInput.value.trim();
  if (!path) return;
  isSavingRoot.value = true;
  actionError.value = null;
  rootStatusMsg.value = null;
  try {
    const resolved = await daemonCall('ensure_projects_root_dir', { path });
    projectsRootInput.value = resolved;
    rootStatusMsg.value = `Workspace directory ready: ${resolved}`;
    emit('refreshDiagnostics');
  } catch (err: unknown) {
    actionError.value = err instanceof Error ? err.message : String(err);
  } finally {
    isSavingRoot.value = false;
  }
}

async function saveGitIdentity() {
  isSavingGitIdentity.value = true;
  actionError.value = null;
  gitStatusMsg.value = null;
  try {
    await daemonCall('set_git_global_identity', {
      name: gitNameInput.value,
      email: gitEmailInput.value,
    });
    gitStatusMsg.value = 'Saved global git user.name and user.email.';
    emit('refreshDiagnostics');
  } catch (err: unknown) {
    actionError.value = err instanceof Error ? err.message : String(err);
  } finally {
    isSavingGitIdentity.value = false;
  }
}

async function refreshAll() {
  isRefreshingRunners.value = true;
  actionError.value = null;
  try {
    await coreStore.fetchHostCapabilities();
    emit('refreshDiagnostics');
  } finally {
    isRefreshingRunners.value = false;
  }
}

function toggleDontShowAgain(checked: boolean) {
  dontShowAgain.value = checked;
  if (checked) {
    localStorage.setItem(ONBOARDING_DISMISSED_KEY, '1');
  } else {
    localStorage.removeItem(ONBOARDING_DISMISSED_KEY);
  }
}

function handleKeydown(e: KeyboardEvent) {
  if (props.show && e.key === 'Escape') {
    e.stopImmediatePropagation();
    emit('close');
  }
}

onMounted(() => {
  window.addEventListener('keydown', handleKeydown, { capture: true });
});

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown, { capture: true });
});
</script>

<template>
  <div
    v-if="show"
    class="fixed inset-0 z-50 flex items-center justify-center bg-[var(--bg-color)]/80 p-4 font-mono"
    @click.self="emit('close')"
  >
    <div class="w-full max-w-3xl max-h-[88vh] flex flex-col border border-[var(--border-color)] ui-surface-modal shadow-2xl overflow-hidden">
      <!-- Header -->
      <div class="px-4 py-3 border-b border-[var(--border-color)] ui-surface-card flex items-center justify-between gap-3 shrink-0">
        <div class="flex flex-col gap-0.5">
          <h2 class="text-xs font-bold uppercase tracking-wider text-[var(--fg-color)]">
            ONE-SHIP Workstation Setup &amp; Readiness
          </h2>
          <p class="text-[10px] text-[var(--dim-color)]">
            Configure your workspace directory, Git identity, AI runners, and optional companion tools.
          </p>
        </div>
        <div class="flex items-center gap-2">
          <AppTooltip text="Re-scan host tools">
            <button
              type="button"
              @click="refreshAll"
              :disabled="isRefreshingRunners"
              class="ui-icon-btn"
              aria-label="Refresh host readiness"
            >
              <RefreshCw :size="12" :class="isRefreshingRunners ? 'animate-spin' : ''" />
            </button>
          </AppTooltip>
          <button type="button" @click="emit('close')" class="ui-icon-btn" aria-label="Close setup checklist">
            <X :size="13" />
          </button>
        </div>
      </div>

      <!-- Body -->
      <div class="p-4 overflow-y-auto flex-1 flex flex-col gap-3 text-xs">
        <div
          v-if="actionError"
          class="p-2.5 border border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] text-[10px]"
        >
          {{ actionError }}
        </div>

        <!-- Step 1: Projects Root Directory -->
        <section class="p-3 border border-[var(--border-color)] ui-surface-card flex flex-col gap-2">
          <div class="flex items-center justify-between gap-2">
            <div class="flex items-center gap-2">
              <FolderOpen :size="13" class="text-[var(--accent-color)]" />
              <span class="font-bold uppercase tracking-wider text-[11px]">1. Default Projects Directory</span>
            </div>
            <span
              class="text-[9px] px-1.5 py-0.5 border font-bold uppercase"
              :class="projectsRootReady
                ? 'border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)]'
                : 'border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)]'"
            >
              {{ projectsRootReady ? 'Ready' : 'Directory Missing' }}
            </span>
          </div>
          <p class="text-[10px] text-[var(--dim-color)]">
            Where cloned and greenfield projects are stored by default.
          </p>
          <div class="flex items-center gap-2">
            <div class="flex-1">
              <AppInput v-model="projectsRootInput" placeholder="/home/user/Projects" />
            </div>
            <button
              type="button"
              @click="browseProjectsRoot"
              class="px-2.5 py-1.5 border border-[var(--border-color)] hover:border-[var(--accent-color)] text-[10px] font-bold uppercase cursor-pointer"
            >
              Browse
            </button>
            <button
              type="button"
              @click="saveProjectsRoot"
              :disabled="isSavingRoot || !projectsRootInput.trim()"
              class="px-3 py-1.5 bg-[var(--accent-color)] text-[var(--bg-color)] text-[10px] font-bold uppercase cursor-pointer disabled:opacity-40 flex items-center gap-1"
            >
              <Loader2 v-if="isSavingRoot" :size="11" class="animate-spin" />
              <span>Create &amp; Save</span>
            </button>
          </div>
          <p v-if="rootStatusMsg" class="text-[10px] text-[var(--status-ok-color)]">{{ rootStatusMsg }}</p>
        </section>

        <!-- Step 2: Git Identity & GitHub CLI -->
        <section class="p-3 border border-[var(--border-color)] ui-surface-card flex flex-col gap-2.5">
          <div class="flex items-center justify-between gap-2">
            <div class="flex items-center gap-2">
              <GitBranch :size="13" class="text-[var(--accent-color)]" />
              <span class="font-bold uppercase tracking-wider text-[11px]">2. Git Identity &amp; GitHub CLI</span>
            </div>
            <span
              class="text-[9px] px-1.5 py-0.5 border font-bold uppercase"
              :class="diagnostics?.git_installed && hasGitIdentity
                ? 'border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)]'
                : 'border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)]'"
            >
              {{ !diagnostics?.git_installed ? 'Git Missing' : (hasGitIdentity ? 'Configured' : 'Identity Missing') }}
            </span>
          </div>

          <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
            <div class="flex flex-col gap-1">
              <span class="text-[9px] uppercase font-bold text-[var(--dim-color)]">Git user.name</span>
              <AppInput v-model="gitNameInput" placeholder="Your Name" />
            </div>
            <div class="flex flex-col gap-1">
              <span class="text-[9px] uppercase font-bold text-[var(--dim-color)]">Git user.email</span>
              <AppInput v-model="gitEmailInput" placeholder="you@example.com" />
            </div>
          </div>

          <div class="flex flex-wrap items-center justify-between gap-2">
            <div class="text-[10px] text-[var(--dim-color)] flex items-center gap-2">
              <span>GitHub CLI (<code>gh</code>):</span>
              <strong :class="diagnostics?.gh_authenticated ? 'text-[var(--status-ok-color)]' : 'text-[var(--status-warn-color)]'">
                {{
                  !diagnostics?.gh_installed
                    ? 'Not installed'
                    : (diagnostics.gh_authenticated ? 'Authenticated' : 'Installed · run `gh auth login` in terminal')
                }}
              </strong>
              <button
                v-if="diagnostics && !diagnostics.gh_installed && diagnostics.can_auto_install_tools"
                type="button"
                @click="emit('installTools', ['gh'])"
                :disabled="isInstallingTools"
                class="px-2 py-0.5 border border-[var(--border-color)] hover:border-[var(--accent-color)] text-[9px] font-bold uppercase flex items-center gap-1 cursor-pointer disabled:opacity-40"
              >
                <Download :size="10" />
                <span>Install gh</span>
              </button>
            </div>
            <button
              type="button"
              @click="saveGitIdentity"
              :disabled="isSavingGitIdentity || !diagnostics?.git_installed || !gitNameInput.trim() || !gitEmailInput.trim()"
              class="px-3 py-1.5 border border-[var(--border-color)] hover:border-[var(--accent-color)] text-[10px] font-bold uppercase cursor-pointer disabled:opacity-40"
            >
              Save Git Identity
            </button>
          </div>
          <p v-if="gitStatusMsg" class="text-[10px] text-[var(--status-ok-color)]">{{ gitStatusMsg }}</p>
        </section>

        <!-- Step 3: AI Agent Runners -->
        <section class="p-3 border border-[var(--border-color)] ui-surface-card flex flex-col gap-2">
          <div class="flex items-center justify-between gap-2">
            <div class="flex items-center gap-2">
              <Terminal :size="13" class="text-[var(--accent-color)]" />
              <span class="font-bold uppercase tracking-wider text-[11px]">3. AI Agent Runners</span>
            </div>
            <span
              class="text-[9px] px-1.5 py-0.5 border font-bold uppercase"
              :class="hasAnyRunner
                ? 'border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)]'
                : 'border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)]'"
            >
              {{ hasAnyRunner ? `Detected: ${installedRunners.join(', ')}` : 'None Installed' }}
            </span>
          </div>
          <p v-if="!hasAnyRunner" class="text-[10px] text-[var(--status-warn-color)]">
            Install at least one supported CLI runner to initialize projects or launch autonomous agents:
          </p>
          <div class="grid grid-cols-1 gap-1.5">
            <div
              v-for="r in RUNNER_INSTALL_GUIDES"
              :key="r.id"
              class="px-2.5 py-1.5 border border-[var(--border-color)]/60 flex items-center justify-between gap-2 text-[10px]"
            >
              <div class="flex items-center gap-2 min-w-0">
                <Check v-if="installedRunners.includes(r.id)" :size="12" class="text-[var(--status-ok-color)] shrink-0" />
                <AlertTriangle v-else :size="12" class="text-[var(--dim-color)] shrink-0" />
                <span class="font-bold text-[var(--fg-color)]">{{ r.label }}</span>
              </div>
              <code class="text-[9px] text-[var(--dim-color)] truncate">{{ r.hint }}</code>
            </div>
          </div>
        </section>

        <!-- Step 4: External Companion Tools -->
        <section class="p-3 border border-[var(--border-color)] ui-surface-card flex flex-col gap-2">
          <div class="flex items-center justify-between gap-2">
            <div class="flex items-center gap-2">
              <Wrench :size="13" class="text-[var(--accent-color)]" />
              <span class="font-bold uppercase tracking-wider text-[11px]">4. Optional Companion Tools</span>
            </div>
            <button
              type="button"
              @click="emit('openIntegrations')"
              class="px-2 py-1 border border-[var(--border-color)] hover:border-[var(--accent-color)] text-[9px] font-bold uppercase flex items-center gap-1 cursor-pointer"
            >
              <Settings :size="10" />
              <span>Integrations Settings</span>
            </button>
          </div>
          <p class="text-[10px] text-[var(--dim-color)]">
            Missing subsystems stay hidden automatically. Run these commands from your ONE-SHIP checkout to install any optional workbench:
          </p>
          <div class="grid grid-cols-1 gap-1.5">
            <div
              v-for="item in integrationsList"
              :key="item.plugin_id"
              class="p-2 border border-[var(--border-color)]/60 flex flex-col gap-1 text-[10px]"
            >
              <div class="flex items-center justify-between gap-2">
                <span class="font-bold text-[var(--fg-color)]">{{ item.label }}</span>
                <span
                  class="text-[8px] px-1.5 py-0.5 border font-bold uppercase"
                  :class="item.available
                    ? 'border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)]'
                    : 'border-[var(--border-color)] text-[var(--dim-color)]'"
                >
                  {{ item.available ? 'Ready' : 'Not Installed' }}
                </span>
              </div>
              <div v-if="INTEGRATION_INSTALL_GUIDES[item.plugin_id]" class="flex flex-wrap items-center justify-between gap-2 text-[9px] text-[var(--dim-color)]">
                <code>{{ INTEGRATION_INSTALL_GUIDES[item.plugin_id].installCmd }}</code>
                <span>{{ INTEGRATION_INSTALL_GUIDES[item.plugin_id].sourceLabel }}</span>
              </div>
            </div>
          </div>
        </section>
      </div>

      <!-- Footer -->
      <div class="px-4 py-3 border-t border-[var(--border-color)] ui-surface-card flex items-center justify-between gap-3 shrink-0">
        <label class="flex items-center gap-2 text-[10px] text-[var(--dim-color)] cursor-pointer select-none">
          <input
            type="checkbox"
            :checked="dontShowAgain"
            @change="toggleDontShowAgain(($event.target as HTMLInputElement).checked)"
            class="accent-[var(--accent-color)]"
          />
          <span>Don't show automatically on startup</span>
        </label>
        <button
          type="button"
          @click="emit('close')"
          class="px-4 py-1.5 bg-[var(--accent-color)] text-[var(--bg-color)] text-xs font-bold uppercase cursor-pointer hover:opacity-90"
        >
          Done
        </button>
      </div>
    </div>
  </div>
</template>
