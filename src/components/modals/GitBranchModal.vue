<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import {
  GitBranch,
  X,
  Check,
  Plus,
  ChevronDown,
  ChevronRight,
  Search,
  AlertCircle,
  Loader2,
} from '@lucide/vue';
import { AppInput, AppSelect, AppToggle, AppTooltip, type SelectOption } from '../ui';
import { useProjectMgmtStore } from '../../stores/projectMgmtStore';
import { useForgeCollabStore } from '../../stores/forgeCollabStore';
import type { ProjectRecord } from '../../types';

const props = defineProps<{
  project: ProjectRecord;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
}>();

const store = useProjectMgmtStore();
const githubStore = useForgeCollabStore();

const searchQuery = ref('');
const showOriginRemotes = ref(false);
const newBranchName = ref('');
const startPoint = ref('');
const switchAfterCreate = ref(true);
const localError = ref<string | null>(null);

const branches = computed(() => store.branchesByProjectId[props.project.id] || []);
const isSwitching = computed(() => Boolean(store.isSwitchingBranchByProjectId[props.project.id]));
const collabCtx = computed(() => githubStore.collabContextByProjectId[props.project.id] ?? null);

const localBranches = computed(() => {
  const q = searchQuery.value.trim().toLowerCase();
  return branches.value.filter(b => {
    if (b.isRemote) return false;
    if (!q) return true;
    return b.name.toLowerCase().includes(q) || (b.upstream?.toLowerCase().includes(q) ?? false);
  });
});

const localBranchNames = computed(() => new Set(branches.value.filter(b => !b.isRemote).map(b => b.name)));

// Strictly exclude `upstream/*` and remotes that already exist as local branches.
const untrackedOriginBranches = computed(() => {
  const q = searchQuery.value.trim().toLowerCase();
  return branches.value.filter(b => {
    if (!b.isRemote) return false;
    const remote = b.remoteName ?? (b.name.includes('/') ? b.name.split('/')[0] : '');
    if (remote !== 'origin') return false;
    const stripped = b.name.startsWith('origin/') ? b.name.slice('origin/'.length) : b.name;
    if (localBranchNames.value.has(stripped)) return false;
    if (!q) return true;
    return b.name.toLowerCase().includes(q);
  });
});

const baseBranchOptions = computed<SelectOption[]>(() => {
  const opts: SelectOption[] = [{ label: 'Current HEAD', value: '' }];
  const ctx = collabCtx.value;
  if (ctx?.hasUpstream && ctx.defaultBranch) {
    const refName = `upstream/${ctx.defaultBranch}`;
    opts.push({ label: `${refName} (Upstream default)`, value: refName });
  }
  return opts;
});

async function handleSelectLocal(branchName: string, isCurrent: boolean) {
  if (isCurrent || isSwitching.value) return;
  localError.value = null;
  const ok = await store.switchBranch(props.project.id, branchName, false);
  if (ok) {
    emit('close');
  } else {
    localError.value = store.branchError || 'Failed to switch branch';
  }
}

async function handleSelectRemote(branchName: string) {
  if (isSwitching.value) return;
  localError.value = null;
  const ok = await store.switchBranch(props.project.id, branchName, true);
  if (ok) {
    emit('close');
  } else {
    localError.value = store.branchError || 'Failed to checkout origin branch';
  }
}

async function handleCreateBranch() {
  const name = newBranchName.value.trim();
  if (!name || isSwitching.value) return;
  localError.value = null;
  const ok = await store.createBranch(
    props.project.id,
    name,
    startPoint.value || null,
    switchAfterCreate.value
  );
  if (ok) {
    newBranchName.value = '';
    if (switchAfterCreate.value) {
      emit('close');
    }
  } else {
    localError.value = store.branchError || 'Failed to create branch';
  }
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.preventDefault();
    emit('close');
  }
}

onMounted(() => {
  window.addEventListener('keydown', handleKeydown);
  store.fetchBranches(props.project.id);
  if (!collabCtx.value) {
    githubStore.fetchCollabContext(props.project.id, false);
  }
});

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown);
});
</script>

<template>
  <Teleport to="body">
    <div
      class="fixed inset-0 ui-overlay-backdrop flex items-center justify-center z-50 p-4 font-mono"
      @click.self="emit('close')"
    >
      <div class="bg-[var(--bg-color)] border border-[var(--border-color)] w-full max-w-lg max-h-[85vh] flex flex-col shadow-2xl text-[var(--fg-color)]">
        <!-- Header -->
        <div class="px-4 py-2.5 border-b border-[var(--border-color)] bg-[var(--ui-bg-card)]/60 flex items-center justify-between gap-2 shrink-0">
          <div class="flex items-center gap-2 text-xs font-bold uppercase tracking-wider text-[var(--accent-color)] min-w-0">
            <GitBranch :size="14" class="shrink-0" />
            <span>Branches</span>
            <span class="text-[var(--dim-color)] font-normal lowercase truncate">// {{ project.name }}</span>
          </div>
          <AppTooltip text="Close (Esc)">
            <button
              type="button"
              @click="emit('close')"
              class="text-[var(--dim-color)] hover:text-[var(--fg-color)] p-1 cursor-pointer"
            >
              <X :size="14" />
            </button>
          </AppTooltip>
        </div>

        <!-- Error Banner -->
        <div
          v-if="localError"
          class="px-4 py-2 border-b border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] text-[11px] flex items-center gap-2 shrink-0"
        >
          <AlertCircle :size="13" class="shrink-0" />
          <span class="break-words">{{ localError }}</span>
        </div>

        <!-- Search Filter -->
        <div class="px-4 py-2 border-b border-[var(--border-color)] shrink-0">
          <div class="relative flex items-center">
            <Search :size="12" class="text-[var(--dim-color)] absolute left-2.5 pointer-events-none" />
            <AppInput
              v-model="searchQuery"
              type="text"
              placeholder="Filter branches..."
              class="!h-[30px] pl-8 pr-7"
            />
            <button
              v-if="searchQuery"
              type="button"
              @click="searchQuery = ''"
              class="absolute right-2 text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer"
            >
              <X :size="12" />
            </button>
          </div>
        </div>

        <!-- Branch Lists -->
        <div class="flex-1 min-h-0 overflow-y-auto p-4 flex flex-col gap-4">
          <!-- Local Branches -->
          <div class="flex flex-col gap-1.5">
            <span class="text-[10px] font-bold uppercase tracking-wider text-[var(--dim-color)]">
              Local Branches ({{ localBranches.length }})
            </span>
            <div v-if="localBranches.length === 0" class="text-xs text-[var(--dim-color)] py-2">
              No matching local branches.
            </div>
            <button
              v-for="b in localBranches"
              :key="b.name"
              type="button"
              :disabled="isSwitching"
              @click="handleSelectLocal(b.name, b.isCurrent)"
              class="w-full text-left px-3 py-2 border transition-colors flex items-center justify-between gap-2 text-xs cursor-pointer disabled:opacity-50"
              :class="b.isCurrent
                ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/10 text-[var(--fg-color)]'
                : 'border-[var(--border-color)] bg-[var(--ui-bg-card)]/25 hover:border-[var(--accent-color)]/60 text-[var(--fg-color)]'"
            >
              <div class="flex items-center gap-2 min-w-0">
                <Check v-if="b.isCurrent" :size="12" class="text-[var(--accent-color)] shrink-0" />
                <GitBranch v-else :size="12" class="text-[var(--dim-color)] shrink-0" />
                <span class="font-bold truncate">{{ b.name }}</span>
              </div>
              <div class="flex items-center gap-2 shrink-0 text-[10px]">
                <span v-if="b.upstream" class="text-[var(--dim-color)]">{{ b.upstream }}</span>
                <span
                  v-if="b.isCurrent"
                  class="px-1.5 py-0.2 border border-[var(--accent-color)] text-[var(--accent-color)] uppercase text-[9px] font-bold"
                >
                  ACTIVE
                </span>
              </div>
            </button>
          </div>

          <!-- Untracked Origin Branches (strictly excludes upstream/*) -->
          <div v-if="untrackedOriginBranches.length > 0" class="flex flex-col gap-1.5">
            <button
              type="button"
              @click="showOriginRemotes = !showOriginRemotes"
              class="flex items-center gap-1.5 text-[10px] font-bold uppercase tracking-wider text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer w-fit"
            >
              <ChevronDown v-if="showOriginRemotes" :size="12" />
              <ChevronRight v-else :size="12" />
              <span>Remote Branches on origin ({{ untrackedOriginBranches.length }})</span>
            </button>

            <div v-if="showOriginRemotes" class="flex flex-col gap-1 pl-2 border-l border-[var(--border-color)]">
              <button
                v-for="r in untrackedOriginBranches"
                :key="r.name"
                type="button"
                :disabled="isSwitching"
                @click="handleSelectRemote(r.name)"
                class="w-full text-left px-3 py-1.5 border border-[var(--border-color)] bg-[var(--ui-bg-card)]/20 hover:border-[var(--accent-color)]/60 text-xs flex items-center justify-between gap-2 cursor-pointer disabled:opacity-50"
              >
                <span class="truncate">{{ r.name }}</span>
                <span class="text-[9px] text-[var(--dim-color)] uppercase shrink-0">track</span>
              </button>
            </div>
          </div>
        </div>

        <!-- Create New Branch Footer -->
        <div class="p-4 border-t border-[var(--border-color)] bg-[var(--ui-bg-card)]/35 flex flex-col gap-2.5 shrink-0">
          <div class="flex items-center justify-between">
            <span class="text-[10px] font-bold uppercase tracking-wider text-[var(--accent-color)]">
              Create New Branch
            </span>
            <div class="flex items-center gap-2 text-[10px]">
              <AppToggle v-model="switchAfterCreate" />
              <span class="text-[var(--dim-color)]">Switch immediately</span>
            </div>
          </div>
          <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
            <AppInput
              v-model="newBranchName"
              placeholder="fix/issue-42"
              @keydown.enter.prevent="handleCreateBranch"
            />
            <AppSelect
              :model-value="startPoint"
              :options="baseBranchOptions"
              @update:model-value="startPoint = $event"
            />
          </div>
          <div class="flex justify-end">
            <button
              type="button"
              :disabled="!newBranchName.trim() || isSwitching"
              @click="handleCreateBranch"
              class="px-3 py-1.5 text-[10px] font-bold uppercase border border-[var(--accent-color)] bg-[var(--accent-color)] text-black hover:opacity-90 cursor-pointer disabled:opacity-40 flex items-center gap-1.5"
            >
              <Loader2 v-if="isSwitching" :size="11" class="animate-spin" />
              <Plus v-else :size="11" />
              <span>{{ switchAfterCreate ? 'Create & Switch' : 'Create Branch' }}</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  </Teleport>
</template>
