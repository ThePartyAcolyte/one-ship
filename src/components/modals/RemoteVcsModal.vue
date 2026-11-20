<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { Cloud, X, ExternalLink, AlertTriangle, Download, GitBranch } from '@lucide/vue';
import { AppInput, AppTooltip } from '../ui';
import type { ProjectRecord } from '../../types';
import type { GithubRepo as GitHubRepo } from '../../types/generated/GithubRepo';
import type { GitlabRepo as GitLabRepo } from '../../types/generated/GitlabRepo';
import type { DiagnosticState } from '../../types/generated/DiagnosticState';
import { daemonCall } from '../../utils/daemonClient';

const props = defineProps<{
  project: ProjectRecord | null;
  diagnostics?: DiagnosticState | null;
  isInstallingTools?: boolean;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'remoteChanged'): void;
  (e: 'installTools', tools: string[]): void;
}>();

type VcsProvider = 'github' | 'gitlab' | 'direct';

const activeProvider = ref<VcsProvider>('github');
const activeActionTab = ref<'link' | 'create'>('link');
const currentRemoteUrl = ref<string | null>(null);

// GitHub state
const githubSearchQuery = ref('');
const githubRepos = ref<GitHubRepo[]>([]);
const githubLoading = ref(false);
const forkPromptRepo = ref<GitHubRepo | null>(null);

// GitLab state
const gitlabSearchQuery = ref('');
const gitlabRepos = ref<GitLabRepo[]>([]);
const gitlabLoading = ref(false);

// Direct Remote state
const directRemoteUrl = ref('');
const directSubmitting = ref(false);

// Create repo state (shared schema)
const newRepoName = ref('');
const newRepoIsPrivate = ref(true);
const newRepoCreating = ref(false);

// Feedback
const vcsError = ref<string | null>(null);
const vcsSuccessMsg = ref<string | null>(null);

const filteredGithubRepos = computed(() => {
  const q = githubSearchQuery.value.trim().toLowerCase();
  if (!q) return githubRepos.value;
  return githubRepos.value.filter(r => r.nameWithOwner.toLowerCase().includes(q));
});

const filteredGitlabRepos = computed(() => {
  const q = gitlabSearchQuery.value.trim().toLowerCase();
  if (!q) return gitlabRepos.value;
  return gitlabRepos.value.filter(r => r.nameWithNamespace.toLowerCase().includes(q) || r.name.toLowerCase().includes(q));
});

const detectedProviderBadge = computed(() => {
  const url = (currentRemoteUrl.value || '').toLowerCase();
  if (!url) return null;
  if (url.includes('github.com')) return 'GITHUB';
  if (url.includes('gitlab.com') || url.includes('gitlab')) return 'GITLAB';
  return 'GIT REMOTE';
});

watch(() => props.project, async (newVal) => {
  if (newVal) {
    currentRemoteUrl.value = newVal.git_status?.remote_url ?? null;
    vcsError.value = null;
    vcsSuccessMsg.value = null;
    forkPromptRepo.value = null;
    newRepoName.value = newVal.name || '';
    directRemoteUrl.value = currentRemoteUrl.value || '';
    githubSearchQuery.value = '';
    gitlabSearchQuery.value = '';
    activeActionTab.value = 'link';

    // Auto-detect provider tab based on remote_url
    const url = (currentRemoteUrl.value || '').toLowerCase();
    if (url.includes('gitlab.com') || url.includes('gitlab')) {
      activeProvider.value = 'gitlab';
    } else if (url.includes('github.com')) {
      activeProvider.value = 'github';
    } else if (url) {
      activeProvider.value = 'direct';
    } else {
      activeProvider.value = 'github';
    }

    await loadProviderRepos();
  }
}, { immediate: true });

watch(activeProvider, async () => {
  vcsError.value = null;
  vcsSuccessMsg.value = null;
  forkPromptRepo.value = null;
  await loadProviderRepos();
});

watch(() => props.diagnostics, async (newDiag, oldDiag) => {
  if (!newDiag) return;
  // If a CLI was just installed while on that tab, fetch repos immediately
  if (activeProvider.value === 'github' && newDiag.gh_installed && !oldDiag?.gh_installed) {
    await fetchGithubRepos();
  } else if (activeProvider.value === 'gitlab' && newDiag.glab_installed && !oldDiag?.glab_installed) {
    await fetchGitlabRepos();
  }
}, { deep: true });

async function loadProviderRepos() {
  if (activeProvider.value === 'github' && props.diagnostics?.gh_installed) {
    await fetchGithubRepos();
  } else if (activeProvider.value === 'gitlab' && props.diagnostics?.glab_installed) {
    await fetchGitlabRepos();
  }
}

async function fetchGithubRepos() {
  if (!props.project) return;
  githubLoading.value = true;
  vcsError.value = null;
  try {
    githubRepos.value = await daemonCall('search_github_repos', {});
  } catch (e: unknown) {
    vcsError.value = String(e);
  } finally {
    githubLoading.value = false;
  }
}

async function fetchGitlabRepos() {
  if (!props.project) return;
  gitlabLoading.value = true;
  vcsError.value = null;
  try {
    gitlabRepos.value = await daemonCall('search_gitlab_repos', {});
  } catch (e: unknown) {
    vcsError.value = String(e);
  } finally {
    gitlabLoading.value = false;
  }
}

function promptOrLinkGithubRepo(repo: GitHubRepo) {
  const perm = (repo.viewerPermission || '').toUpperCase();
  if (perm && !['ADMIN', 'WRITE'].includes(perm)) {
    forkPromptRepo.value = repo;
  } else {
    executeLinkRepo(repo.url);
  }
}

async function executeLinkRepo(url: string) {
  if (!props.project) return;
  vcsError.value = null;
  forkPromptRepo.value = null;
  directSubmitting.value = true;
  try {
    await daemonCall('link_project_remote', { projectId: props.project.id, remoteUrl: url });
    currentRemoteUrl.value = url;
    vcsSuccessMsg.value = `Successfully linked remote to ${url}`;
    emit('remoteChanged');
  } catch (e: unknown) {
    vcsError.value = String(e);
  } finally {
    directSubmitting.value = false;
  }
}

async function executeCreateRepo() {
  if (!props.project || !newRepoName.value.trim()) return;
  newRepoCreating.value = true;
  vcsError.value = null;
  try {
    let remoteUrl = '';
    if (activeProvider.value === 'github') {
      remoteUrl = await daemonCall('create_github_repo', {
        projectId: props.project.id,
        name: newRepoName.value.trim(),
        isPrivate: newRepoIsPrivate.value,
      });
    } else if (activeProvider.value === 'gitlab') {
      remoteUrl = await daemonCall('create_gitlab_repo', {
        projectId: props.project.id,
        name: newRepoName.value.trim(),
        isPrivate: newRepoIsPrivate.value,
      });
    }
    if (remoteUrl) {
      currentRemoteUrl.value = remoteUrl;
      vcsSuccessMsg.value = `Created & linked repository: ${remoteUrl}`;
      emit('remoteChanged');
    }
  } catch (e: unknown) {
    vcsError.value = String(e);
  } finally {
    newRepoCreating.value = false;
  }
}
</script>

<template>
  <div v-if="project" class="fixed inset-0 ui-overlay-backdrop flex items-center justify-center z-50 p-4 font-mono">
    <div class="bg-[var(--bg-color)] border border-[var(--border-color)] p-5 max-w-xl w-full flex flex-col gap-3 shadow-2xl">
      <!-- Header -->
      <div class="flex justify-between items-center border-b border-[var(--border-color)] pb-2.5">
        <h3 class="font-bold uppercase tracking-widest text-[var(--accent-color)] text-xs flex items-center gap-2">
          <Cloud :size="16" />
          REMOTE VCS // {{ project.name }}
        </h3>
        <AppTooltip text="Close" position="left">
          <button 
            @click="emit('close')" 
            class="text-[var(--dim-color)] hover:text-[var(--fg-color)] transition-colors p-0.5 cursor-pointer"
          >
            <X :size="15" />
          </button>
        </AppTooltip>
      </div>

      <!-- Configured Remote Origin Banner -->
      <div v-if="currentRemoteUrl" class="border border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] p-2.5 flex flex-col gap-1 text-xs">
        <div class="flex justify-between items-center">
          <div class="flex items-center gap-2">
            <span class="text-[10px] uppercase font-bold text-[var(--status-ok-color)] tracking-wider">
              Remote Origin Configured
            </span>
            <span v-if="detectedProviderBadge" class="text-[9px] px-1 py-0.2 border border-[var(--status-ok-border)] text-[var(--status-ok-color)]">
              {{ detectedProviderBadge }}
            </span>
          </div>
          <span class="text-[9px] px-1.5 py-0.2 border border-[var(--status-ok-border)] text-[var(--status-ok-color)]">ACTIVE</span>
        </div>
        <div class="flex items-center justify-between gap-2">
          <span class="font-mono text-[var(--status-ok-color)] truncate text-[11px] select-all">{{ currentRemoteUrl }}</span>
          <AppTooltip v-if="currentRemoteUrl.startsWith('http')" text="Open in Browser" position="left">
            <a :href="currentRemoteUrl" target="_blank" class="text-[var(--dim-color)] hover:text-[var(--status-ok-color)] transition-colors p-0.5 shrink-0">
              <ExternalLink :size="12" />
            </a>
          </AppTooltip>
        </div>
      </div>

      <!-- Error & Success Messages -->
      <div v-if="vcsError" class="border border-[var(--status-error-border)] bg-[var(--status-error-bg)] p-2 text-[var(--status-error-color)] text-xs break-words">
        {{ vcsError }}
      </div>
      <div v-if="vcsSuccessMsg" class="border border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] p-2 text-[var(--status-ok-color)] text-xs">
        {{ vcsSuccessMsg }}
      </div>

      <!-- Read-Only Permission Prompt (GitHub Fork Warning) -->
      <div v-if="forkPromptRepo" class="border border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] p-3 flex flex-col gap-2 text-xs">
        <div class="flex items-center gap-1.5 text-[var(--status-warn-color)] font-bold uppercase text-[11px]">
          <AlertTriangle :size="14" />
          <span>Read-Only Permission Detected</span>
        </div>
        <p class="text-[var(--fg-color)] text-[11px]">
          You have <span class="font-bold text-[var(--status-warn-color)]">READ</span> access to <span class="font-mono text-[var(--accent-color)]">{{ forkPromptRepo.nameWithOwner }}</span>. Pushing changes directly will be rejected by GitHub.
        </p>
        <p class="text-[var(--dim-color)] text-[10px]">
          We recommend forking this repository first via <code class="text-[var(--fg-color)]">gh repo fork</code>, or link anyway if you only plan to pull.
        </p>
        <div class="flex justify-end gap-2 mt-1">
          <button 
            @click="forkPromptRepo = null" 
            class="px-2.5 py-1 text-[10px] font-bold uppercase border border-[var(--border-color)] hover:bg-[var(--fg-color)] hover:text-[var(--bg-color)] cursor-pointer"
          >
            Cancel
          </button>
          <button 
            @click="executeLinkRepo(forkPromptRepo.url)" 
            class="px-2.5 py-1 text-[10px] font-bold uppercase border border-[var(--status-warn-border)] text-[var(--status-warn-color)] hover:bg-[var(--status-warn-color)] hover:text-black cursor-pointer"
          >
            Link Anyway (Pull-Only)
          </button>
        </div>
      </div>

      <!-- Provider Tabs Header -->
      <div class="flex border-b border-[var(--border-color)] text-xs gap-1">
        <button 
          @click="activeProvider = 'github'"
          class="px-3 py-1.5 font-bold uppercase transition-colors cursor-pointer border-b-2 text-[11px]"
          :class="activeProvider === 'github' 
            ? 'border-[var(--accent-color)] text-[var(--accent-color)]' 
            : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
        >
          GitHub
        </button>
        <button 
          @click="activeProvider = 'gitlab'"
          class="px-3 py-1.5 font-bold uppercase transition-colors cursor-pointer border-b-2 text-[11px]"
          :class="activeProvider === 'gitlab' 
            ? 'border-[var(--accent-color)] text-[var(--accent-color)]' 
            : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
        >
          GitLab
        </button>
        <button 
          @click="activeProvider = 'direct'"
          class="px-3 py-1.5 font-bold uppercase transition-colors cursor-pointer border-b-2 text-[11px]"
          :class="activeProvider === 'direct' 
            ? 'border-[var(--accent-color)] text-[var(--accent-color)]' 
            : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
        >
          Direct Git URL
        </button>
      </div>

      <!-- PROVIDER 1: GITHUB -->
      <div v-if="activeProvider === 'github'" class="flex flex-col gap-3">
        <!-- Contextual Tool Installation Banner -->
        <div v-if="!diagnostics?.gh_installed" class="border border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] p-3 flex items-center justify-between gap-3 text-xs">
          <div class="flex items-center gap-2">
            <AlertTriangle :size="16" class="text-[var(--status-warn-color)] shrink-0" />
            <span class="text-[11px] text-[var(--fg-color)]">
              GitHub CLI (<code class="text-[var(--accent-color)]">gh</code>) is not installed on this system.
            </span>
          </div>
          <button
            @click="emit('installTools', ['gh'])"
            :disabled="isInstallingTools"
            class="px-2.5 py-1 text-[10px] font-bold border border-[var(--status-warn-border)] bg-[var(--bg-color)] text-[var(--status-warn-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] transition-colors cursor-pointer shrink-0 flex items-center gap-1.5 disabled:opacity-40"
          >
            <Download :size="11" />
            <span>{{ isInstallingTools ? 'INSTALLING...' : 'INSTALL GITHUB-CLI' }}</span>
          </button>
        </div>

        <template v-else>
          <!-- Sub-action Tabs -->
          <div class="flex border-b border-[var(--border-color)] text-xs">
            <button 
              @click="activeActionTab = 'link'; forkPromptRepo = null"
              class="px-3 py-1 font-bold uppercase transition-colors cursor-pointer border-b-2 text-[10px]"
              :class="activeActionTab === 'link' 
                ? 'border-[var(--accent-color)] text-[var(--accent-color)]' 
                : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
            >
              {{ currentRemoteUrl ? 'Change / Link Remote' : 'Link Existing Repo' }}
            </button>
            <button 
              @click="activeActionTab = 'create'; forkPromptRepo = null"
              class="px-3 py-1 font-bold uppercase transition-colors cursor-pointer border-b-2 text-[10px]"
              :class="activeActionTab === 'create' 
                ? 'border-[var(--accent-color)] text-[var(--accent-color)]' 
                : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
            >
              Create New Repo
            </button>
          </div>

          <!-- GitHub: Link Existing -->
          <div v-if="activeActionTab === 'link' && !forkPromptRepo" class="flex flex-col gap-2.5">
            <div class="relative">
              <AppInput 
                v-model="githubSearchQuery"
                placeholder="Search your GitHub repositories..."
              />
              <button
                v-if="githubSearchQuery"
                @click="githubSearchQuery = ''"
                class="absolute right-2 top-1.5 text-xs text-[var(--dim-color)] hover:text-[var(--fg-color)] px-1"
              >
                ✕
              </button>
            </div>

            <div v-if="githubLoading" class="py-6 flex items-center justify-center text-xs text-[var(--dim-color)] animate-pulse">
              Querying GitHub CLI (gh repo list)...
            </div>
            <div v-else-if="filteredGithubRepos.length === 0" class="py-6 text-center text-xs text-[var(--dim-color)]">
              No repositories found.
            </div>
            <div v-else class="flex flex-col gap-1.5 max-h-56 overflow-y-auto pr-1">
              <div 
                v-for="repo in filteredGithubRepos" 
                :key="repo.nameWithOwner"
                class="border border-[var(--border-color)] p-2 hover:border-[var(--accent-color)]/50 transition-colors flex items-center justify-between gap-2 bg-black/20"
              >
                <div class="flex flex-col min-w-0">
                  <div class="flex items-center gap-1.5">
                    <span class="font-bold text-xs text-[var(--fg-color)] truncate">{{ repo.nameWithOwner }}</span>
                    <span 
                      class="text-[9px] px-1 py-0.2 border uppercase"
                      :class="repo.isPrivate ? 'border-[var(--status-warn-border)] text-[var(--status-warn-color)]' : 'border-[var(--status-info-border)] text-[var(--status-info-color)]'"
                    >
                      {{ repo.isPrivate ? 'Private' : 'Public' }}
                    </span>
                    <span 
                      v-if="repo.viewerPermission"
                      class="text-[9px] px-1 py-0.2 border uppercase"
                      :class="['ADMIN', 'WRITE'].includes(repo.viewerPermission.toUpperCase()) 
                        ? 'border-[var(--status-ok-border)] text-[var(--status-ok-color)]' 
                        : 'border-[var(--status-warn-border)] text-[var(--status-warn-color)]'"
                    >
                      {{ repo.viewerPermission }}
                    </span>
                  </div>
                  <span class="text-[10px] text-[var(--dim-color)] truncate">{{ repo.url }}</span>
                </div>

                <button 
                  @click="promptOrLinkGithubRepo(repo)"
                  class="shrink-0 px-2.5 py-1 text-[10px] font-bold uppercase border transition-colors cursor-pointer"
                  :class="currentRemoteUrl === repo.url 
                    ? 'border-[var(--status-ok-border)] text-[var(--status-ok-color)] bg-[var(--status-ok-bg)]' 
                    : 'border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-black'"
                >
                  {{ currentRemoteUrl === repo.url ? 'Linked' : 'Link' }}
                </button>
              </div>
            </div>
          </div>

          <!-- GitHub: Create New Repo -->
          <div v-if="activeActionTab === 'create'" class="flex flex-col gap-3">
            <div class="flex flex-col gap-1">
              <label class="text-[10px] uppercase text-[var(--dim-color)]">Repository Name</label>
              <AppInput 
                v-model="newRepoName"
                placeholder="e.g. my-awesome-project"
              />
            </div>

            <div class="flex flex-col gap-1.5">
              <label class="text-[10px] uppercase text-[var(--dim-color)]">Visibility</label>
              <div class="flex gap-4 text-xs">
                <label class="flex items-center gap-1.5 cursor-pointer">
                  <input type="radio" :value="true" v-model="newRepoIsPrivate" class="accent-[var(--accent-color)]" />
                  <span>Private (Recommended)</span>
                </label>
                <label class="flex items-center gap-1.5 cursor-pointer">
                  <input type="radio" :value="false" v-model="newRepoIsPrivate" class="accent-[var(--accent-color)]" />
                  <span>Public</span>
                </label>
              </div>
            </div>

            <div class="text-[10px] text-[var(--dim-color)] border border-[var(--border-color)] p-2 bg-black/20">
              Running: <span class="text-[var(--fg-color)]">gh repo create {{ newRepoName || '&lt;name&gt;' }} {{ newRepoIsPrivate ? '--private' : '--public' }} --source=. --remote=origin</span>
            </div>

            <button 
              @click="executeCreateRepo" 
              :disabled="newRepoCreating || !newRepoName.trim()"
              class="py-2 px-3 text-xs font-bold uppercase border transition-colors flex items-center justify-center gap-2 cursor-pointer"
              :class="newRepoCreating || !newRepoName.trim()
                ? 'border-[var(--border-color)] text-[var(--dim-color)] opacity-50 cursor-not-allowed' 
                : 'border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-black'"
            >
              <span v-if="newRepoCreating" class="animate-pulse">Creating Repository...</span>
              <span v-else>Create &amp; Link Remote</span>
            </button>
          </div>
        </template>
      </div>

      <!-- PROVIDER 2: GITLAB -->
      <div v-if="activeProvider === 'gitlab'" class="flex flex-col gap-3">
        <!-- Contextual Tool Installation Banner -->
        <div v-if="!diagnostics?.glab_installed" class="border border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] p-3 flex items-center justify-between gap-3 text-xs">
          <div class="flex items-center gap-2">
            <AlertTriangle :size="16" class="text-[var(--status-warn-color)] shrink-0" />
            <span class="text-[11px] text-[var(--fg-color)]">
              GitLab CLI (<code class="text-[var(--accent-color)]">glab</code>) is not installed on this system.
            </span>
          </div>
          <button
            @click="emit('installTools', ['glab'])"
            :disabled="isInstallingTools"
            class="px-2.5 py-1 text-[10px] font-bold border border-[var(--status-warn-border)] bg-[var(--bg-color)] text-[var(--status-warn-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] transition-colors cursor-pointer shrink-0 flex items-center gap-1.5 disabled:opacity-40"
          >
            <Download :size="11" />
            <span>{{ isInstallingTools ? 'INSTALLING...' : 'INSTALL GLAB' }}</span>
          </button>
        </div>

        <template v-else>
          <!-- Sub-action Tabs -->
          <div class="flex border-b border-[var(--border-color)] text-xs">
            <button 
              @click="activeActionTab = 'link'"
              class="px-3 py-1 font-bold uppercase transition-colors cursor-pointer border-b-2 text-[10px]"
              :class="activeActionTab === 'link' 
                ? 'border-[var(--accent-color)] text-[var(--accent-color)]' 
                : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
            >
              {{ currentRemoteUrl ? 'Change / Link Remote' : 'Link Existing Repo' }}
            </button>
            <button 
              @click="activeActionTab = 'create'"
              class="px-3 py-1 font-bold uppercase transition-colors cursor-pointer border-b-2 text-[10px]"
              :class="activeActionTab === 'create' 
                ? 'border-[var(--accent-color)] text-[var(--accent-color)]' 
                : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
            >
              Create New Repo
            </button>
          </div>

          <!-- GitLab: Link Existing -->
          <div v-if="activeActionTab === 'link'" class="flex flex-col gap-2.5">
            <div class="relative">
              <AppInput 
                v-model="gitlabSearchQuery"
                placeholder="Search your GitLab repositories..."
              />
              <button
                v-if="gitlabSearchQuery"
                @click="gitlabSearchQuery = ''"
                class="absolute right-2 top-1.5 text-xs text-[var(--dim-color)] hover:text-[var(--fg-color)] px-1"
              >
                ✕
              </button>
            </div>

            <div v-if="gitlabLoading" class="py-6 flex items-center justify-center text-xs text-[var(--dim-color)] animate-pulse">
              Querying GitLab CLI (glab api projects)...
            </div>
            <div v-else-if="filteredGitlabRepos.length === 0" class="py-6 text-center text-xs text-[var(--dim-color)]">
              No repositories found.
            </div>
            <div v-else class="flex flex-col gap-1.5 max-h-56 overflow-y-auto pr-1">
              <div 
                v-for="repo in filteredGitlabRepos" 
                :key="repo.webUrl"
                class="border border-[var(--border-color)] p-2 hover:border-[var(--accent-color)]/50 transition-colors flex items-center justify-between gap-2 bg-black/20"
              >
                <div class="flex flex-col min-w-0">
                  <div class="flex items-center gap-1.5">
                    <span class="font-bold text-xs text-[var(--fg-color)] truncate">{{ repo.nameWithNamespace }}</span>
                    <span 
                      class="text-[9px] px-1 py-0.2 border uppercase"
                      :class="repo.isPrivate ? 'border-[var(--status-warn-border)] text-[var(--status-warn-color)]' : 'border-[var(--status-info-border)] text-[var(--status-info-color)]'"
                    >
                      {{ repo.isPrivate ? 'Private' : 'Public' }}
                    </span>
                  </div>
                  <span class="text-[10px] text-[var(--dim-color)] truncate">{{ repo.webUrl }}</span>
                </div>

                <button 
                  @click="executeLinkRepo(repo.webUrl)"
                  class="shrink-0 px-2.5 py-1 text-[10px] font-bold uppercase border transition-colors cursor-pointer"
                  :class="currentRemoteUrl === repo.webUrl 
                    ? 'border-[var(--status-ok-border)] text-[var(--status-ok-color)] bg-[var(--status-ok-bg)]' 
                    : 'border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-black'"
                >
                  {{ currentRemoteUrl === repo.webUrl ? 'Linked' : 'Link' }}
                </button>
              </div>
            </div>
          </div>

          <!-- GitLab: Create New Repo -->
          <div v-if="activeActionTab === 'create'" class="flex flex-col gap-3">
            <div class="flex flex-col gap-1">
              <label class="text-[10px] uppercase text-[var(--dim-color)]">Repository Name</label>
              <AppInput 
                v-model="newRepoName"
                placeholder="e.g. my-gitlab-project"
              />
            </div>

            <div class="flex flex-col gap-1.5">
              <label class="text-[10px] uppercase text-[var(--dim-color)]">Visibility</label>
              <div class="flex gap-4 text-xs">
                <label class="flex items-center gap-1.5 cursor-pointer">
                  <input type="radio" :value="true" v-model="newRepoIsPrivate" class="accent-[var(--accent-color)]" />
                  <span>Private (Recommended)</span>
                </label>
                <label class="flex items-center gap-1.5 cursor-pointer">
                  <input type="radio" :value="false" v-model="newRepoIsPrivate" class="accent-[var(--accent-color)]" />
                  <span>Public</span>
                </label>
              </div>
            </div>

            <div class="text-[10px] text-[var(--dim-color)] border border-[var(--border-color)] p-2 bg-black/20">
              Running: <span class="text-[var(--fg-color)]">glab repo create {{ newRepoName || '&lt;name&gt;' }} {{ newRepoIsPrivate ? '--private' : '--public' }} --default-branch=main</span>
            </div>

            <button 
              @click="executeCreateRepo" 
              :disabled="newRepoCreating || !newRepoName.trim()"
              class="py-2 px-3 text-xs font-bold uppercase border transition-colors flex items-center justify-center gap-2 cursor-pointer"
              :class="newRepoCreating || !newRepoName.trim()
                ? 'border-[var(--border-color)] text-[var(--dim-color)] opacity-50 cursor-not-allowed' 
                : 'border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-black'"
            >
              <span v-if="newRepoCreating" class="animate-pulse">Creating Repository...</span>
              <span v-else>Create &amp; Link Remote</span>
            </button>
          </div>
        </template>
      </div>

      <!-- PROVIDER 3: DIRECT GIT URL -->
      <div v-if="activeProvider === 'direct'" class="flex flex-col gap-3">
        <div class="flex flex-col gap-1">
          <label class="text-[10px] uppercase text-[var(--dim-color)]">Git Remote URL</label>
          <AppInput 
            v-model="directRemoteUrl"
            placeholder="git@myserver.com:owner/repo.git or https://..."
          />
          <span class="text-[10px] text-[var(--dim-color)] mt-0.5">
            Accepts any valid SSH or HTTPS remote URL for self-hosted Git, Bitbucket, Gitea, or custom remotes.
          </span>
        </div>

        <button 
          @click="executeLinkRepo(directRemoteUrl.trim())" 
          :disabled="directSubmitting || !directRemoteUrl.trim()"
          class="py-2 px-3 text-xs font-bold uppercase border transition-colors flex items-center justify-center gap-2 cursor-pointer"
          :class="directSubmitting || !directRemoteUrl.trim()
            ? 'border-[var(--border-color)] text-[var(--dim-color)] opacity-50 cursor-not-allowed' 
            : 'border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-black'"
        >
          <GitBranch :size="13" />
          <span v-if="directSubmitting" class="animate-pulse">Configuring Origin...</span>
          <span v-else>Set Remote Origin</span>
        </button>
      </div>

      <!-- Footer -->
      <div class="flex justify-end gap-2 border-t border-[var(--border-color)] pt-2.5 mt-1">
        <button 
          @click="emit('close')" 
          class="px-4 py-1.5 text-xs font-bold uppercase border border-[var(--border-color)] hover:bg-[var(--fg-color)] hover:text-[var(--bg-color)] transition-colors cursor-pointer"
        >
          Close
        </button>
      </div>
    </div>
  </div>
</template>
