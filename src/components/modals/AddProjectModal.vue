<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue';
import { Folder, Sparkles, FolderOpen, X, Loader2, GitBranch, ArrowRight, Cpu, ShieldCheck, AlertTriangle } from '@lucide/vue';
import { daemonCall } from '../../utils/daemonClient';
import { open } from '@tauri-apps/plugin-dialog';
import { AppInput, AppSelect, AppToggle } from '../ui';
import { useCoreStore } from '../../stores/coreStore';
import { useAgentPairPicker } from '../../utils/runnerCatalog';
import type { ProjectRecord } from '../../types';
import type { GitRemoteAccessCheck } from '../../types/generated/GitRemoteAccessCheck';

const props = defineProps<{
  show: boolean;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'added', project: ProjectRecord, shouldInit?: boolean): void;
  (e: 'created-greenfield', project: ProjectRecord): void;
}>();

const coreStore = useCoreStore();

type AddMode = 'existing' | 'new' | 'clone';
const activeMode = ref<AddMode>('new');
const isSubmitting = ref(false);
const errorMessage = ref<string | null>(null);

// Existing Project State
const existingPath = ref('');

// New Project State
const projectName = ref('');
const parentPath = ref('');
const initGit = ref(true);
const enableRea = ref(false);
const trackingMode = ref('');

const trackingModeOptions = [
  { label: 'Default (Enable Later)', value: '' },
  { label: 'Personal / Emergent (Linear)', value: 'linear' },
  { label: 'R&D / Exploratory (Shape Up)', value: 'shape_up' },
  { label: 'Commercial / Client (Weighted)', value: 'weighted' },
];

// Clone Project State
const cloneRepoUrl = ref('');
const cloneParentPath = ref('');
const cloneProjectName = ref('');
const cloneLaunchInit = ref(true);
const isCheckingAccess = ref(false);
const accessCheckResult = ref<GitRemoteAccessCheck | null>(null);
const accessCheckError = ref<string | null>(null);
const prevExtractedName = ref('');

// The greenfield initializer's runner/model/effort come only from the runner manifests; the
// initializer pair of the configuration is preselected.
const pairPicker = useAgentPairPicker();
const {
  runner: selectedRunner,
  model: selectedModel,
  effort: selectedEffort,
  runnerOptions,
  modelOptions,
  effortOptions,
} = pairPicker;

const hasInstalledRunner = computed(() => (coreStore.installedRunners?.length ?? 0) > 0);
const missingRunnerGate = computed(() => coreStore.hostCapabilities !== null && !coreStore.hasAnyRunner);
const missingGitIdentityGate = computed(() => coreStore.diagnostics !== null && !coreStore.hasGitIdentity);

function openSetupAndClose() {
  coreStore.requestOpenSetupModal();
  emit('close');
}

// Computed target full path
const computedTargetPath = computed(() => {
  const name = projectName.value.trim();
  const parent = parentPath.value.trim();
  if (!parent) return name;
  if (parent.endsWith('/') || parent.endsWith('\\')) {
    return `${parent}${name}`;
  }
  return `${parent}/${name}`;
});

const computedCloneTargetPath = computed(() => {
  const name = cloneProjectName.value.trim();
  const parent = cloneParentPath.value.trim();
  if (!parent) return name;
  if (parent.endsWith('/') || parent.endsWith('\\')) {
    return `${parent}${name}`;
  }
  return `${parent}/${name}`;
});

function extractNameFromUrl(url: string): string {
  const trimmed = url.trim().replace(/\/+$/, '');
  const noGit = trimmed.endsWith('.git') ? trimmed.slice(0, -4) : trimmed;
  const parts = noGit.split(/[/:]/);
  const last = parts[parts.length - 1] || '';
  return last.startsWith('-') ? '' : last;
}

watch(cloneRepoUrl, (newUrl) => {
  accessCheckResult.value = null;
  accessCheckError.value = null;
  const extracted = extractNameFromUrl(newUrl);
  if (extracted && (!cloneProjectName.value || cloneProjectName.value === prevExtractedName.value)) {
    cloneProjectName.value = extracted;
    prevExtractedName.value = extracted;
  }
});

async function browseExistingDirectory() {
  errorMessage.value = null;
  const selected = await open({
    directory: true,
    multiple: false,
    title: "Select Existing Project Directory"
  });

  if (selected && typeof selected === "string") {
    existingPath.value = selected;
  }
}

async function browseParentDirectory() {
  const selected = await open({
    directory: true,
    multiple: false,
    title: "Select Parent Workspace Directory"
  });

  if (selected && typeof selected === "string") {
    parentPath.value = selected;
  }
}

async function handleAddExisting() {
  if (!existingPath.value.trim()) {
    errorMessage.value = "Please select a project directory";
    return;
  }

  isSubmitting.value = true;
  errorMessage.value = null;
  try {
    const project = await daemonCall("add_project", { path: existingPath.value.trim() });
    emit('added', project);
    emit('close');
  } catch (err: unknown) {
    errorMessage.value = String(err);
  } finally {
    isSubmitting.value = false;
  }
}

async function handleCreateGreenfield() {
  const name = projectName.value.trim();
  const parent = parentPath.value.trim();

  if (!pairPicker.isValid.value) {
    errorMessage.value = "Choose a runner and one of its models";
    return;
  }

  if (!name) {
    errorMessage.value = "Project name is required";
    return;
  }
  if (name.includes('/') || name.includes('\\')) {
    errorMessage.value = "Project name cannot contain path separators";
    return;
  }
  if (!parent) {
    errorMessage.value = "Parent location is required";
    return;
  }

  isSubmitting.value = true;
  errorMessage.value = null;

  try {
    const project = await daemonCall("create_greenfield_project", {
      name,
      parentPath: parent,
      initGit: initGit.value,
      enableRea: enableRea.value,
      trackingMode: trackingMode.value || null,
      runner: selectedRunner.value,
      model: selectedModel.value,
      effort: selectedEffort.value || null,
    });
    emit('created-greenfield', project);
    emit('close');
  } catch (err: unknown) {
    errorMessage.value = String(err);
  } finally {
    isSubmitting.value = false;
  }
}

async function browseCloneParentDirectory() {
  const selected = await open({
    directory: true,
    multiple: false,
    title: "Select Parent Workspace Directory"
  });

  if (selected && typeof selected === "string") {
    cloneParentPath.value = selected;
  }
}

async function handleCheckAccess() {
  const url = cloneRepoUrl.value.trim();
  if (!url) {
    accessCheckError.value = "Enter a repository URL";
    return;
  }
  isCheckingAccess.value = true;
  accessCheckResult.value = null;
  accessCheckError.value = null;
  try {
    const res = await daemonCall("check_git_remote_access", { repoUrl: url });
    accessCheckResult.value = res;
    if (res.repoName && (!cloneProjectName.value || cloneProjectName.value === prevExtractedName.value)) {
      cloneProjectName.value = res.repoName;
      prevExtractedName.value = res.repoName;
    }
    if (!res.accessible && res.errorMessage) {
      accessCheckError.value = res.errorMessage;
    }
  } catch (err: unknown) {
    accessCheckError.value = String(err);
  } finally {
    isCheckingAccess.value = false;
  }
}

async function handleCloneAndAdd() {
  const url = cloneRepoUrl.value.trim();
  const parent = cloneParentPath.value.trim();
  const name = cloneProjectName.value.trim();

  if (!url) {
    errorMessage.value = "Repository URL is required";
    return;
  }
  if (!name) {
    errorMessage.value = "Project name is required";
    return;
  }
  if (name.includes('/') || name.includes('\\')) {
    errorMessage.value = "Project name cannot contain path separators";
    return;
  }
  if (!parent) {
    errorMessage.value = "Parent location is required";
    return;
  }

  isSubmitting.value = true;
  errorMessage.value = null;

  try {
    const project = await daemonCall("clone_and_add_project", {
      repoUrl: url,
      parentPath: parent,
      name,
    });
    emit('added', project, cloneLaunchInit.value);
    emit('close');
  } catch (err: unknown) {
    errorMessage.value = String(err);
  } finally {
    isSubmitting.value = false;
  }
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && props.show) {
    emit('close');
  }
}

watch(() => props.show, async (isOpen) => {
  if (isOpen) {
    errorMessage.value = null;
    isSubmitting.value = false;
    existingPath.value = '';
    projectName.value = '';
    enableRea.value = false;
    trackingMode.value = '';
    cloneRepoUrl.value = '';
    cloneProjectName.value = '';
    cloneLaunchInit.value = hasInstalledRunner.value;
    accessCheckResult.value = null;
    accessCheckError.value = null;
    prevExtractedName.value = '';
    
    // Set smart default parent directory
    if (!parentPath.value) {
      if (coreStore.projects.length > 0) {
        const lastProj = coreStore.projects[0];
        const parts = lastProj.path.split('/');
        parts.pop();
        parentPath.value = parts.join('/');
      }
      if (!parentPath.value) {
        try {
          const schemas = await daemonCall('get_plugin_schemas', {});
          const sys = schemas.find(p => p.plugin_id === 'system');
          const rootField = sys?.fields.find(f => f.key === 'default_projects_root_path');
          if (typeof rootField?.default_value === 'string') {
            parentPath.value = rootField.default_value;
          }
        } catch (err) {
          console.warn('[AddProjectModal] Failed to resolve default_projects_root_path:', err);
        }
      }
    }
    cloneParentPath.value = parentPath.value;

    try {
      await pairPicker.loadConfiguredPair('init');
    } catch (e: unknown) {
      errorMessage.value = `Cannot load the initializer runner/model: ${String(e)}`;
    }
  }
});

onMounted(() => {
  window.addEventListener('keydown', handleKeydown);
});

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown);
});
</script>

<template>
  <div 
    v-if="show" 
    class="fixed inset-0 ui-overlay-backdrop z-50 flex items-center justify-center p-4 font-mono select-none"
  >
    <div 
      class="w-full max-w-xl ui-surface-modal border border-[var(--border-color)] shadow-2xl flex flex-col max-h-[90vh] overflow-hidden"
    >
      <!-- Modal Header -->
      <div class="flex items-center justify-between px-4 py-3 border-b border-[var(--border-color)] bg-[var(--ui-bg-card)]">
        <div class="flex items-center gap-2">
          <Folder :size="16" class="text-[var(--accent-color)]" />
          <span class="text-xs font-bold uppercase tracking-wider text-[var(--fg-color)]">
            Add Project
          </span>
        </div>
        <button 
          @click="emit('close')"
          class="text-[var(--dim-color)] hover:text-[var(--fg-color)] transition-colors p-1"
        >
          <X :size="14" />
        </button>
      </div>

      <!-- Mode Selector Tabs -->
      <div class="grid grid-cols-3 border-b border-[var(--border-color)] bg-[var(--bg-color)]">
        <button 
          type="button"
          @click="activeMode = 'new'"
          class="flex items-center justify-center gap-1.5 py-2.5 px-3 text-xs font-bold tracking-wider transition-colors border-r border-[var(--border-color)] cursor-pointer"
          :class="activeMode === 'new' 
            ? 'bg-[var(--accent-color)]/10 text-[var(--accent-color)] border-b-2 border-b-[var(--accent-color)]' 
            : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
        >
          <Sparkles :size="13" />
          <span>New Project</span>
        </button>
        <button 
          type="button"
          @click="activeMode = 'existing'"
          class="flex items-center justify-center gap-1.5 py-2.5 px-3 text-xs font-bold tracking-wider transition-colors border-r border-[var(--border-color)] cursor-pointer"
          :class="activeMode === 'existing' 
            ? 'bg-[var(--accent-color)]/10 text-[var(--accent-color)] border-b-2 border-b-[var(--accent-color)]' 
            : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
        >
          <FolderOpen :size="13" />
          <span>Existing Project</span>
        </button>
        <button 
          type="button"
          @click="activeMode = 'clone'"
          class="flex items-center justify-center gap-1.5 py-2.5 px-3 text-xs font-bold tracking-wider transition-colors cursor-pointer"
          :class="activeMode === 'clone' 
            ? 'bg-[var(--accent-color)]/10 text-[var(--accent-color)] border-b-2 border-b-[var(--accent-color)]' 
            : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
        >
          <GitBranch :size="13" />
          <span>Clone Repo</span>
        </button>
      </div>

      <!-- Modal Body -->
      <div class="p-4 overflow-y-auto flex-1 flex flex-col gap-4 text-xs">
        <!-- Error Banner -->
        <div 
          v-if="errorMessage" 
          class="p-3 bg-[var(--status-error-bg)] border border-[var(--status-error-border)] text-[var(--status-error-color)] text-xs flex items-center justify-between"
        >
          <span>{{ errorMessage }}</span>
          <button @click="errorMessage = null" class="text-[var(--status-error-color)] hover:opacity-80">
            <X :size="12" />
          </button>
        </div>

        <!-- Missing Runner Gate Banner -->
        <div
          v-if="missingRunnerGate"
          class="p-2.5 bg-[var(--status-warn-bg)] border border-[var(--status-warn-border)] text-[var(--status-warn-color)] text-[11px] flex items-center justify-between gap-2"
        >
          <div class="flex items-center gap-2 min-w-0">
            <AlertTriangle :size="14" class="shrink-0" />
            <span>Install at least one AI CLI runner (<code>agy</code>, <code>claude</code>, or <code>opencode</code>) before adding or initializing projects.</span>
          </div>
          <button
            type="button"
            @click="openSetupAndClose"
            class="px-2 py-1 border border-[var(--status-warn-border)] bg-[var(--bg-color)] text-[var(--status-warn-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] text-[10px] font-bold uppercase shrink-0 cursor-pointer"
          >
            Setup &amp; Readiness
          </button>
        </div>

        <!-- MODE: NEW PROJECT (GREENFIELD) -->
        <div v-if="activeMode === 'new'" class="flex flex-col gap-4">
          <div class="text-[11px] text-[var(--dim-color)] leading-relaxed">
            Bootstrap a new project from scratch. ONE-SHIP will create the directory, inject platform skills, and launch the conversational Lead Architect to define scope and documentation.
          </div>

          <!-- Project Name -->
          <div class="flex flex-col gap-1.5">
            <label class="font-bold text-[var(--fg-color)] uppercase text-[10px] tracking-wider">
              Project Name
            </label>
            <AppInput 
              v-model="projectName" 
              placeholder="e.g. telemetry-daemon" 
              autofocus
            />
          </div>

          <!-- Parent Directory Location -->
          <div class="flex flex-col gap-1.5">
            <label class="font-bold text-[var(--fg-color)] uppercase text-[10px] tracking-wider">
              Location
            </label>
            <div class="flex items-center gap-2">
              <div class="flex-1">
                <AppInput 
                  v-model="parentPath" 
                  placeholder="/home/user/Projects" 
                />
              </div>
              <button 
                type="button"
                @click="browseParentDirectory"
                class="px-3 py-1.5 border border-[var(--border-color)] bg-[var(--ui-bg-card)] hover:border-[var(--accent-color)] text-[var(--fg-color)] text-xs font-bold transition-colors cursor-pointer flex items-center gap-1.5"
              >
                <FolderOpen :size="13" />
                <span>Browse</span>
              </button>
            </div>
          </div>

          <!-- Calculated Target Preview -->
          <div v-if="projectName.trim()" class="p-2.5 bg-[var(--ui-bg-card)] border border-[var(--border-color)]/60 text-[11px] flex flex-col gap-1">
            <span class="text-[var(--dim-color)] text-[10px] uppercase font-bold tracking-wider">Target Directory</span>
            <span class="text-[var(--accent-color)] font-mono break-all">{{ computedTargetPath }}</span>
          </div>

          <!-- Git Initialization Toggle -->
          <div class="flex items-center justify-between p-2.5 bg-[var(--ui-bg-card)] border border-[var(--border-color)]/60">
            <div class="flex items-center gap-2">
              <GitBranch :size="14" class="text-[var(--accent-color)]" />
              <div class="flex flex-col">
                <span class="font-bold text-[var(--fg-color)] text-[11px]">Initialize Git Repository</span>
                <span class="text-[10px] text-[var(--dim-color)]">
                  {{ missingGitIdentityGate
                    ? 'Unavailable: configure Git user.name and user.email in Setup & Readiness'
                    : 'Run `git init` in the project root' }}
                </span>
              </div>
            </div>
            <AppToggle v-model="initGit" :disabled="missingGitIdentityGate" />
          </div>

          <!-- Reverse Engineering Lab (REA) Toggle -->
          <div class="flex items-center justify-between p-2.5 bg-[var(--ui-bg-card)] border border-[var(--border-color)]/60">
            <div class="flex items-center gap-2">
              <Cpu :size="14" class="text-[var(--accent-color)]" />
              <div class="flex flex-col">
                <span class="font-bold text-[var(--fg-color)] text-[11px]">Reverse Engineering Lab (REA)</span>
                <span class="text-[10px] text-[var(--dim-color)]">Scaffolds targets/, isolated REA MCP gateway &amp; reverser specialist</span>
              </div>
            </div>
            <AppToggle v-model="enableRea" />
          </div>

          <!-- Project Nature / Milestone Tracking Mode -->
          <div class="flex flex-col gap-1 p-2.5 bg-[var(--ui-bg-card)] border border-[var(--border-color)]/60">
            <label class="text-[10px] uppercase font-bold tracking-wider text-[var(--dim-color)]">
              Project Planning &amp; Milestone Mode
            </label>
            <AppSelect
              v-model="trackingMode"
              :options="trackingModeOptions"
            />
          </div>

          <!-- Architect Agent Settings -->
          <div class="border border-[var(--border-color)]/60 p-3 bg-[var(--ui-bg-card)]/40 flex flex-col gap-3">
            <div class="text-[10px] uppercase font-bold tracking-wider text-[var(--dim-color)]">
              Lead Architect Agent
            </div>
            <div
              v-if="!hasInstalledRunner"
              class="p-2 border border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)] text-[10px]"
            >
              No agent runners detected (`agy`, `claude`, `opencode`). Install a CLI runner first or import an existing repository via Existing / Clone.
            </div>
            <div class="grid grid-cols-2 gap-3">
              <div class="flex flex-col gap-1">
                <label class="text-[10px] text-[var(--dim-color)] uppercase">Runner</label>
                <AppSelect 
                  v-model="selectedRunner" 
                  :options="runnerOptions" 
                />
              </div>
              <div class="flex flex-col gap-1">
                <label class="text-[10px] text-[var(--dim-color)] uppercase">Model</label>
                <AppSelect 
                  v-model="selectedModel"
                  :options="modelOptions"
                />
              </div>
            </div>
            <div class="flex flex-col gap-1">
              <label class="text-[10px] text-[var(--dim-color)] uppercase">Reasoning Effort</label>
              <AppSelect
                v-if="effortOptions.length > 0"
                v-model="selectedEffort"
                :options="effortOptions"
              />
              <span v-else class="text-[10px] text-[var(--dim-color)]">This model has no effort setting</span>
            </div>
          </div>
        </div>

        <!-- MODE: EXISTING PROJECT (BROWNFIELD) -->
        <div v-if="activeMode === 'existing'" class="flex flex-col gap-4">
          <div class="text-[11px] text-[var(--dim-color)] leading-relaxed">
            Select an existing directory containing project source code. ONE-SHIP will register it and trigger non-invasive reconnaissance.
          </div>

          <div class="flex flex-col gap-2">
            <label class="font-bold text-[var(--fg-color)] uppercase text-[10px] tracking-wider">
              Project Directory
            </label>
            <div class="flex items-center gap-2">
              <div class="flex-1">
                <AppInput 
                  v-model="existingPath" 
                  placeholder="Select a folder..." 
                  readonly
                />
              </div>
              <button 
                type="button"
                @click="browseExistingDirectory"
                class="px-3 py-1.5 border border-[var(--border-color)] bg-[var(--ui-bg-card)] hover:border-[var(--accent-color)] text-[var(--fg-color)] text-xs font-bold transition-colors cursor-pointer flex items-center gap-1.5"
              >
                <FolderOpen :size="13" />
                <span>Browse</span>
              </button>
            </div>
          </div>
        </div>

        <!-- MODE: CLONE REPOSITORY (REMOTE GIT / GITHUB) -->
        <div v-if="activeMode === 'clone'" class="flex flex-col gap-4">
          <div class="text-[11px] text-[var(--dim-color)] leading-relaxed">
            Clone a project from GitHub or any remote Git repository (HTTPS or SSH). ONE-SHIP verifies repository access, clones the repository into your workspace, and registers it.
          </div>

          <!-- Repository URL & Access Verification -->
          <div class="flex flex-col gap-1.5">
            <label class="font-bold text-[var(--fg-color)] uppercase text-[10px] tracking-wider">
              Repository URL
            </label>
            <div class="flex items-center gap-2">
              <div class="flex-1">
                <AppInput 
                  v-model="cloneRepoUrl" 
                  placeholder="https://github.com/owner/repo.git or git@github.com:..." 
                  @keydown.enter.prevent="handleCheckAccess"
                />
              </div>
              <button 
                type="button"
                @click="handleCheckAccess"
                :disabled="isCheckingAccess || !cloneRepoUrl.trim()"
                class="px-3 py-1.5 border border-[var(--border-color)] bg-[var(--ui-bg-card)] hover:border-[var(--accent-color)] text-[var(--fg-color)] text-xs font-bold transition-colors cursor-pointer flex items-center gap-1.5 disabled:opacity-40 disabled:cursor-not-allowed"
              >
                <Loader2 v-if="isCheckingAccess" :size="13" class="animate-spin" />
                <ShieldCheck v-else :size="13" />
                <span>Verify Access</span>
              </button>
            </div>
          </div>

          <!-- Access Check Result Feedback -->
          <div 
            v-if="accessCheckResult?.accessible" 
            class="p-2.5 bg-[var(--ui-bg-card)] border border-[var(--status-ok-border)] text-xs flex flex-col gap-1.5"
          >
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-2">
                <span class="px-1.5 py-0.5 text-[9px] font-bold bg-[var(--status-ok-bg)] text-[var(--status-ok-color)] border border-[var(--status-ok-border)] uppercase">
                  Accessible
                </span>
                <span 
                  v-if="accessCheckResult.isPrivate !== undefined && accessCheckResult.isPrivate !== null" 
                  class="px-1.5 py-0.5 text-[9px] font-bold uppercase" 
                  :class="accessCheckResult.isPrivate ? 'bg-[var(--status-warn-bg)] text-[var(--status-warn-color)] border border-[var(--status-warn-border)]' : 'bg-[var(--dim-color)]/20 text-[var(--fg-color)] border border-[var(--border-color)]'"
                >
                  {{ accessCheckResult.isPrivate ? 'Private' : 'Public' }}
                </span>
                <span 
                  v-if="accessCheckResult.viewerPermission" 
                  class="px-1.5 py-0.5 text-[9px] font-bold bg-[var(--accent-color)]/10 text-[var(--accent-color)] border border-[var(--accent-color)]/30 uppercase"
                >
                  {{ accessCheckResult.viewerPermission }}
                </span>
              </div>
              <span v-if="accessCheckResult.defaultBranch" class="text-[10px] text-[var(--dim-color)] font-mono">
                branch: {{ accessCheckResult.defaultBranch }}
              </span>
            </div>
          </div>

          <div 
            v-else-if="accessCheckError" 
            class="p-2.5 bg-[var(--status-error-bg)] border border-[var(--status-error-border)] text-[var(--status-error-color)] text-xs flex flex-col gap-1"
          >
            <div class="font-bold flex items-center gap-1.5">
              <AlertTriangle :size="13" />
              <span>Access Verification Failed</span>
            </div>
            <span class="text-[11px] leading-relaxed break-words">{{ accessCheckError }}</span>
          </div>

          <!-- Location (Parent Directory) -->
          <div class="flex flex-col gap-1.5">
            <label class="font-bold text-[var(--fg-color)] uppercase text-[10px] tracking-wider">
              Location
            </label>
            <div class="flex items-center gap-2">
              <div class="flex-1">
                <AppInput 
                  v-model="cloneParentPath" 
                  placeholder="/home/user/Projects" 
                />
              </div>
              <button 
                type="button"
                @click="browseCloneParentDirectory"
                class="px-3 py-1.5 border border-[var(--border-color)] bg-[var(--ui-bg-card)] hover:border-[var(--accent-color)] text-[var(--fg-color)] text-xs font-bold transition-colors cursor-pointer flex items-center gap-1.5"
              >
                <FolderOpen :size="13" />
                <span>Browse</span>
              </button>
            </div>
          </div>

          <!-- Project Directory Name -->
          <div class="flex flex-col gap-1.5">
            <label class="font-bold text-[var(--fg-color)] uppercase text-[10px] tracking-wider">
              Project Name
            </label>
            <AppInput 
              v-model="cloneProjectName" 
              placeholder="e.g. repo-name" 
            />
          </div>

          <!-- Target Directory Preview -->
          <div v-if="cloneProjectName.trim()" class="p-2.5 bg-[var(--ui-bg-card)] border border-[var(--border-color)]/60 text-[11px] flex flex-col gap-1">
            <span class="text-[var(--dim-color)] text-[10px] uppercase font-bold tracking-wider">Target Directory</span>
            <span class="text-[var(--accent-color)] font-mono break-all">{{ computedCloneTargetPath }}</span>
          </div>

          <!-- Launch Initializer Toggle -->
          <div class="flex items-center justify-between p-2.5 bg-[var(--ui-bg-card)] border border-[var(--border-color)]/60">
            <div class="flex items-center gap-2">
              <Sparkles :size="14" class="text-[var(--accent-color)]" />
              <div class="flex flex-col">
                <span class="font-bold text-[var(--fg-color)] text-[11px]">Launch Project Initializer</span>
                <span class="text-[10px] text-[var(--dim-color)]">
                  {{ hasInstalledRunner
                    ? 'Start stack reconnaissance and persona setup after cloning'
                    : 'Unavailable: no agent runners installed on this machine' }}
                </span>
              </div>
            </div>
            <AppToggle v-model="cloneLaunchInit" :disabled="!hasInstalledRunner" />
          </div>
        </div>
      </div>

      <!-- Modal Footer -->
      <div class="px-4 py-3 border-t border-[var(--border-color)] bg-[var(--ui-bg-card)] flex items-center justify-end gap-2">
        <button 
          type="button"
          @click="emit('close')"
          :disabled="isSubmitting"
          class="px-4 py-1.5 text-xs font-bold text-[var(--dim-color)] hover:text-[var(--fg-color)] transition-colors border border-transparent hover:border-[var(--border-color)] cursor-pointer"
        >
          Cancel
        </button>

        <button 
          v-if="activeMode === 'new'"
          type="button"
          @click="handleCreateGreenfield"
          :disabled="isSubmitting || missingRunnerGate || !projectName.trim() || !parentPath.trim()"
          class="px-4 py-1.5 text-xs font-bold bg-[var(--accent-color)] text-[var(--bg-color)] hover:opacity-90 disabled:opacity-40 disabled:cursor-not-allowed transition-all flex items-center gap-1.5 cursor-pointer"
        >
          <Loader2 v-if="isSubmitting" :size="14" class="animate-spin" />
          <Sparkles v-else :size="14" />
          <span>Create Project</span>
        </button>

        <button 
          v-if="activeMode === 'existing'"
          type="button"
          @click="handleAddExisting"
          :disabled="isSubmitting || missingRunnerGate || !existingPath.trim()"
          class="px-4 py-1.5 text-xs font-bold bg-[var(--accent-color)] text-[var(--bg-color)] hover:opacity-90 disabled:opacity-40 disabled:cursor-not-allowed transition-all flex items-center gap-1.5 cursor-pointer"
        >
          <Loader2 v-if="isSubmitting" :size="14" class="animate-spin" />
          <ArrowRight v-else :size="14" />
          <span>Add Project</span>
        </button>

        <button 
          v-if="activeMode === 'clone'"
          type="button"
          @click="handleCloneAndAdd"
          :disabled="isSubmitting || missingRunnerGate || missingGitIdentityGate || !cloneRepoUrl.trim() || !cloneProjectName.trim() || !cloneParentPath.trim()"
          class="px-4 py-1.5 text-xs font-bold bg-[var(--accent-color)] text-[var(--bg-color)] hover:opacity-90 disabled:opacity-40 disabled:cursor-not-allowed transition-all flex items-center gap-1.5 cursor-pointer"
        >
          <Loader2 v-if="isSubmitting" :size="14" class="animate-spin" />
          <GitBranch v-else :size="14" />
          <span>Clone Project</span>
        </button>
      </div>
    </div>
  </div>
</template>
