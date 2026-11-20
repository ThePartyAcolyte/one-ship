<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue';
import {
  ExternalLink,
  CheckCircle2,
  AlertCircle,
  Search,
  X,
  Plus,
  Play,
  Bookmark,
  Check,
  Inbox,
} from '@lucide/vue';
import {
  AppTooltip,
  AppInput,
  AppSelect,
  AppTextarea,
  AppToggle,
  type SelectOption,
} from '../ui';
import { useForgeCollabStore } from '../../stores/forgeCollabStore';
import { useUiSurfaceNode } from '../../composables/useUiSurfaceNode';
import type { ProjectRecord } from '../../types';
import type { ForgeInboxItem } from '../../types/generated/ForgeInboxItem';

const props = defineProps<{
  project: ProjectRecord;
}>();

const emit = defineEmits<{
  (e: 'task-taken', taskId: number): void;
}>();

const store = useForgeCollabStore();

type ForgeSubTab = 'issues' | 'prs' | 'releases';
const forgeSurface = useUiSurfaceNode<ForgeSubTab>('project/forge', 'issues');
const activeTab = computed({
  get: () => forgeSurface.activeChildId.value,
  set: (val) => forgeSurface.selectChild(val),
});

// Issue filters
const issueState = ref('open');
const issueLabel = ref('');
const issueSearch = ref('');
const issueInboxFilter = ref<'all' | 'inbox' | 'flagged'>('all');
const selectedIssueNumber = ref<number | null>(null);

// Take Issue options (never auto-switches active branch)
const takeCreateBranch = ref(false);
const takeBranchName = ref('');
const takeCreateTodo = ref(true);
const takenTaskMessage = ref<string | null>(null);

// PR/MR filters & Draft PR form
const prState = ref('open');
const prAuthorMe = ref(false);
const prInboxFilter = ref<'all' | 'inbox' | 'flagged'>('all');
const selectedPrNumber = ref<number | null>(null);
const showDraftPrForm = ref(false);
const draftTargetRepo = ref('');
const draftBaseBranch = ref('');
const draftHeadBranch = ref('');
const draftTitle = ref('');
const draftBody = ref('');
const createdDraftPrUrl = ref<string | null>(null);

const issueStateOptions: SelectOption[] = [
  { label: 'Open', value: 'open' },
  { label: 'Closed', value: 'closed' },
  { label: 'All', value: 'all' },
];

const prStateOptions: SelectOption[] = [
  { label: 'Open', value: 'open' },
  { label: 'Closed', value: 'closed' },
  { label: 'Merged', value: 'merged' },
  { label: 'All', value: 'all' },
];

const quickLabels = ['', 'good first issue', 'help wanted', 'bug', 'enhancement'];

const ctx = computed(() => store.collabContext);
const providerLabel = computed(() => ctx.value?.providerLabel ?? 'Forge');
const prTabLabel = computed(() => ctx.value?.pullRequestLabel ?? 'Pull Requests');
const supportsDraftPr = computed(() => ctx.value?.supportsDraftPullRequest ?? false);

const currentLocalBranch = computed(() => props.project.git_status?.branch ?? '');

const inboxByKey = computed(() => {
  const map = new Map<string, ForgeInboxItem>();
  for (const item of store.inboxItems) {
    map.set(`${item.itemKind}:${item.itemNumber}`, item);
  }
  return map;
});

function inboxEntryFor(kind: 'issue' | 'pr', number: number): ForgeInboxItem | null {
  return inboxByKey.value.get(`${kind}:${number}`) ?? null;
}

const displayedIssues = computed(() => {
  if (issueInboxFilter.value === 'all') return store.issues;
  return store.issues.filter((issue) => {
    const entry = inboxEntryFor('issue', issue.number);
    if (issueInboxFilter.value === 'flagged') {
      return entry?.triageFlag === 'flagged';
    }
    return !entry || entry.inboxStatus !== 'read';
  });
});

const displayedPrs = computed(() => {
  if (prInboxFilter.value === 'all') return store.prs;
  return store.prs.filter((pr) => {
    const entry = inboxEntryFor('pr', pr.number);
    if (prInboxFilter.value === 'flagged') {
      return entry?.triageFlag === 'flagged';
    }
    return !entry || entry.inboxStatus !== 'read';
  });
});

function suggestBranchName(issueNumber: number): string {
  return `fix/issue-${issueNumber}`;
}

async function loadInitial(forceRefresh = false) {
  store.clearForgeState();
  selectedIssueNumber.value = null;
  selectedPrNumber.value = null;
  takenTaskMessage.value = null;
  createdDraftPrUrl.value = null;
  const loadedCtx = await store.fetchCollabContext(props.project.id, forceRefresh);
  if (loadedCtx?.targetRepo && !loadedCtx.error) {
    await Promise.all([
      loadIssues(forceRefresh),
      loadPrs(forceRefresh),
      store.fetchInbox(props.project.id),
      store.fetchUnreadCounts(props.project.id),
    ]);
    // A restored `project/forge/releases` node must load its data on mount; the
    // tab selection no longer flows through a local click handler.
    if (activeTab.value === 'releases' && store.releases.length === 0) {
      await loadReleases(forceRefresh);
    }
  }
}

async function loadIssues(forceRefresh = false) {
  await store.fetchIssues(props.project.id, {
    state: issueState.value,
    label: issueLabel.value || undefined,
    search: issueSearch.value.trim() || undefined,
    forceRefresh,
  });
}

async function loadPrs(forceRefresh = false) {
  await store.fetchPrs(props.project.id, {
    state: prState.value,
    authorMe: prAuthorMe.value,
    forceRefresh,
  });
}

async function loadReleases(forceRefresh = false) {
  await store.fetchReleases(props.project.id, forceRefresh);
}

async function handleSelectIssue(issueNumber: number) {
  selectedIssueNumber.value = issueNumber;
  takenTaskMessage.value = null;
  takeBranchName.value = takeCreateBranch.value ? suggestBranchName(issueNumber) : '';
  const summary = store.issues.find((i) => i.number === issueNumber);
  await store.fetchIssueDetail(props.project.id, issueNumber);
  await store.markItemRead({
    projectId: props.project.id,
    provider: ctx.value?.provider ?? null,
    itemKind: 'issue',
    number: issueNumber,
    read: true,
    updatedAt: summary?.updatedAt ?? store.selectedIssueDetail?.updatedAt ?? null,
  });
}

async function handleSelectPr(prNumber: number) {
  selectedPrNumber.value = prNumber;
  const summary = store.prs.find((p) => p.number === prNumber);
  await store.fetchPrDetail(props.project.id, prNumber);
  await store.markItemRead({
    projectId: props.project.id,
    provider: ctx.value?.provider ?? null,
    itemKind: 'pr',
    number: prNumber,
    read: true,
    updatedAt: summary?.updatedAt ?? null,
  });
}

async function toggleFlag(kind: 'issue' | 'pr', number: number) {
  const current = inboxEntryFor(kind, number)?.triageFlag ?? null;
  await store.setItemFlag({
    projectId: props.project.id,
    provider: ctx.value?.provider ?? null,
    itemKind: kind,
    number,
    flag: current === 'flagged' ? null : 'flagged',
  });
}

async function toggleRead(kind: 'issue' | 'pr', number: number, updatedAt: string) {
  const entry = inboxEntryFor(kind, number);
  const nextRead = !(entry?.isRead ?? false);
  await store.markItemRead({
    projectId: props.project.id,
    provider: ctx.value?.provider ?? null,
    itemKind: kind,
    number,
    read: nextRead,
    updatedAt,
  });
}

// The Project panel bar (Spec 34 §6.5) owns the Issues | Pull Requests | Releases
// selection through `project/forge`; releases are still fetched lazily on first visit.
watch(activeTab, (tab) => {
  if (tab === 'releases' && store.releases.length === 0) {
    loadReleases(false);
  }
});

async function handleRefreshActive() {
  await store.fetchCollabContext(props.project.id, true);
  if (activeTab.value === 'issues') {
    await Promise.all([loadIssues(true), store.fetchInbox(props.project.id)]);
    if (selectedIssueNumber.value !== null) {
      await store.fetchIssueDetail(props.project.id, selectedIssueNumber.value, true);
    }
  } else if (activeTab.value === 'prs') {
    await Promise.all([loadPrs(true), store.fetchInbox(props.project.id)]);
    if (selectedPrNumber.value !== null) {
      await store.fetchPrDetail(props.project.id, selectedPrNumber.value, true);
    }
  } else {
    await loadReleases(true);
  }
  await store.fetchUnreadCounts(props.project.id);
}

async function handleTakeIssue() {
  if (selectedIssueNumber.value === null || store.isTakingIssue) return;
  const trimmed = takeBranchName.value.trim();
  const branch = takeCreateBranch.value
    ? trimmed || suggestBranchName(selectedIssueNumber.value)
    : trimmed || null;
  const taskId = await store.takeIssue({
    projectId: props.project.id,
    projectPath: props.project.path,
    issueNumber: selectedIssueNumber.value,
    createBranch: takeCreateBranch.value,
    branchName: branch,
    createLinkedTodo: takeCreateTodo.value,
  });
  if (taskId !== null) {
    takenTaskMessage.value = `Created task #${taskId} from issue #${selectedIssueNumber.value}`;
    emit('task-taken', taskId);
  }
}

watch(takeCreateBranch, (enabled) => {
  if (enabled && !takeBranchName.value.trim() && selectedIssueNumber.value !== null) {
    takeBranchName.value = suggestBranchName(selectedIssueNumber.value);
  }
});

function openDraftPrForm() {
  if (!supportsDraftPr.value) return;
  createdDraftPrUrl.value = null;
  draftTargetRepo.value = ctx.value?.targetRepo ?? ctx.value?.originRepo ?? '';
  draftBaseBranch.value = ctx.value?.defaultBranch ?? 'main';
  draftHeadBranch.value = currentLocalBranch.value || '';
  draftTitle.value = '';
  draftBody.value = '';
  showDraftPrForm.value = true;
}

async function handleCreateDraftPr() {
  if (
    !draftTargetRepo.value.trim() ||
    !draftBaseBranch.value.trim() ||
    !draftHeadBranch.value.trim() ||
    !draftTitle.value.trim() ||
    store.isCreatingDraftPr
  ) {
    return;
  }
  const url = await store.createDraftPr({
    projectId: props.project.id,
    targetRepo: draftTargetRepo.value.trim(),
    baseBranch: draftBaseBranch.value.trim(),
    headBranch: draftHeadBranch.value.trim(),
    title: draftTitle.value.trim(),
    body: draftBody.value,
  });
  if (url) {
    createdDraftPrUrl.value = url;
    showDraftPrForm.value = false;
  }
}

function formatIsoShort(iso: string): string {
  if (!iso) return '';
  return iso.slice(0, 10);
}

function checksBadgeClass(summary: string): string {
  if (summary === 'passing') {
    return 'border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)]';
  }
  if (summary === 'failing') {
    return 'border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)]';
  }
  if (summary === 'pending') {
    return 'border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)]';
  }
  return 'border-[var(--border-color)] text-[var(--dim-color)]';
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'F5') {
    e.preventDefault();
    if (!store.isLoadingForge) {
      handleRefreshActive();
    }
  }
}

watch(
  () => props.project.id,
  () => {
    loadInitial(false);
  }
);

onMounted(() => {
  store.initInboxListener();
  window.addEventListener('keydown', handleKeydown);
  loadInitial(false);
});

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown);
  store.teardownInboxListener();
});
</script>

<template>
  <div class="flex flex-col flex-1 min-h-0 overflow-hidden bg-[var(--bg-color)]">
    <!-- Error / Provider Unavailable Banner -->
    <div
      v-if="ctx?.error || store.forgeError"
      class="px-3 py-1.5 border-b border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] text-[11px] font-mono flex items-center gap-2 shrink-0"
    >
      <AlertCircle :size="12" class="shrink-0" />
      <span class="truncate">{{ store.forgeError || ctx?.error }}</span>
    </div>

    <!-- Confirmation Banner for Taken Issue or Created Draft PR -->
    <div
      v-if="takenTaskMessage"
      class="px-3 py-1.5 border-b border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)] text-[11px] font-mono flex items-center justify-between gap-2 shrink-0"
    >
      <div class="flex items-center gap-2">
        <CheckCircle2 :size="12" class="shrink-0" />
        <span>{{ takenTaskMessage }}</span>
      </div>
      <button type="button" @click="takenTaskMessage = null" class="cursor-pointer opacity-70 hover:opacity-100">
        <X :size="12" />
      </button>
    </div>

    <div
      v-if="createdDraftPrUrl"
      class="px-3 py-1.5 border-b border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)] text-[11px] font-mono flex items-center justify-between gap-2 shrink-0"
    >
      <div class="flex items-center gap-2 min-w-0">
        <CheckCircle2 :size="12" class="shrink-0" />
        <span>Draft PR created:</span>
        <a
          :href="createdDraftPrUrl"
          target="_blank"
          rel="noopener noreferrer"
          class="underline font-bold truncate"
        >
          {{ createdDraftPrUrl }}
        </a>
      </div>
      <button type="button" @click="createdDraftPrUrl = null" class="cursor-pointer opacity-70 hover:opacity-100">
        <X :size="12" />
      </button>
    </div>

    <!-- ISSUES VIEW -->
    <div v-if="activeTab === 'issues'" class="flex flex-col flex-1 min-h-0 overflow-hidden">
      <!-- Issue Filters -->
      <div class="px-3 py-2 border-b border-[var(--border-color)] bg-[var(--ui-bg-card)]/20 flex flex-wrap items-center gap-2 shrink-0">
        <div class="w-28 shrink-0">
          <AppSelect
            :model-value="issueState"
            :options="issueStateOptions"
            @update:model-value="val => { issueState = val; loadIssues(false); }"
          />
        </div>
        <div class="flex items-center gap-1">
          <button
            type="button"
            @click="issueInboxFilter = 'all'"
            class="px-2 py-1 text-[9px] font-mono uppercase border cursor-pointer"
            :class="issueInboxFilter === 'all'
              ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] font-bold'
              : 'border-[var(--border-color)] text-[var(--dim-color)]'"
          >
            All
          </button>
          <button
            type="button"
            @click="issueInboxFilter = 'inbox'"
            class="px-2 py-1 text-[9px] font-mono uppercase border cursor-pointer flex items-center gap-1"
            :class="issueInboxFilter === 'inbox'
              ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] font-bold'
              : 'border-[var(--border-color)] text-[var(--dim-color)]'"
          >
            <Inbox :size="10" />
            <span>Inbox</span>
          </button>
          <button
            type="button"
            @click="issueInboxFilter = 'flagged'"
            class="px-2 py-1 text-[9px] font-mono uppercase border cursor-pointer flex items-center gap-1"
            :class="issueInboxFilter === 'flagged'
              ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] font-bold'
              : 'border-[var(--border-color)] text-[var(--dim-color)]'"
          >
            <Bookmark :size="10" />
            <span>Flagged</span>
          </button>
        </div>
        <div class="flex-1 min-w-[160px] max-w-xs relative flex items-center">
          <Search :size="12" class="text-[var(--dim-color)] absolute left-2 pointer-events-none" />
          <AppInput
            v-model="issueSearch"
            @keydown.enter="loadIssues(false)"
            type="text"
            placeholder="Search issues (Enter)..."
            class="pl-7 pr-6"
          />
          <button
            v-if="issueSearch"
            type="button"
            @click="issueSearch = ''; loadIssues(false)"
            class="absolute right-1.5 text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer"
          >
            <X :size="12" />
          </button>
        </div>
        <div class="flex flex-wrap items-center gap-1">
          <button
            v-for="lbl in quickLabels"
            :key="lbl || '_all'"
            type="button"
            @click="issueLabel = lbl; loadIssues(false)"
            class="px-2 py-1 text-[9px] font-mono uppercase border transition-colors cursor-pointer"
            :class="issueLabel === lbl
              ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] font-bold'
              : 'border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
          >
            {{ lbl || 'all labels' }}
          </button>
        </div>
      </div>

      <!-- Split List + Detail -->
      <div class="flex-1 min-h-0 flex overflow-hidden">
        <!-- Left Issue List -->
        <div class="w-full md:w-[380px] lg:w-[420px] shrink-0 border-r border-[var(--border-color)] overflow-y-auto p-2 flex flex-col gap-1.5">
          <div
            v-if="displayedIssues.length === 0 && !store.isLoadingForge"
            class="p-4 text-xs font-mono text-[var(--dim-color)]"
          >
            No matching issues found.
          </div>
          <div
            v-for="issue in displayedIssues"
            :key="issue.number"
            @click="handleSelectIssue(issue.number)"
            class="text-left p-2.5 border transition-colors cursor-pointer flex flex-col gap-1"
            :class="selectedIssueNumber === issue.number
              ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/10'
              : 'border-[var(--border-color)] bg-[var(--ui-bg-card)]/30 hover:border-[var(--accent-color)]/60'"
          >
            <div class="flex items-center justify-between gap-2 text-[10px] font-mono">
              <div class="flex items-center gap-1.5">
                <span class="font-bold text-[var(--accent-color)]">#{{ issue.number }}</span>
                <span
                  v-if="inboxEntryFor('issue', issue.number)?.inboxStatus !== 'read'"
                  class="px-1 border border-[var(--status-info-border)] bg-[var(--status-info-bg)] text-[var(--status-info-color)] uppercase text-[8px]"
                >
                  {{ inboxEntryFor('issue', issue.number)?.inboxStatus === 'updated' ? 'UPD' : 'NEW' }}
                </span>
              </div>
              <div class="flex items-center gap-1.5 text-[var(--dim-color)]">
                <span v-if="issue.commentsCount > 0">{{ issue.commentsCount }} comments</span>
                <span>{{ formatIsoShort(issue.updatedAt) }}</span>
                <AppTooltip text="Toggle flag">
                  <button
                    type="button"
                    @click.stop="toggleFlag('issue', issue.number)"
                    class="p-0.5 cursor-pointer hover:text-[var(--accent-color)]"
                    :class="inboxEntryFor('issue', issue.number)?.triageFlag === 'flagged' ? 'text-[var(--accent-color)]' : ''"
                  >
                    <Bookmark :size="11" />
                  </button>
                </AppTooltip>
                <AppTooltip :text="inboxEntryFor('issue', issue.number)?.isRead ? 'Mark unread' : 'Mark read'">
                  <button
                    type="button"
                    @click.stop="toggleRead('issue', issue.number, issue.updatedAt)"
                    class="p-0.5 cursor-pointer hover:text-[var(--status-ok-color)]"
                    :class="inboxEntryFor('issue', issue.number)?.isRead ? 'text-[var(--status-ok-color)]' : ''"
                  >
                    <Check :size="11" />
                  </button>
                </AppTooltip>
              </div>
            </div>
            <div class="text-xs font-bold text-[var(--fg-color)] leading-snug">
              {{ issue.title }}
            </div>
            <div class="flex flex-wrap items-center gap-1">
              <span class="text-[9px] font-mono text-[var(--dim-color)]">@{{ issue.author }}</span>
              <span
                v-for="l in issue.labels.slice(0, 4)"
                :key="l"
                class="text-[9px] font-mono px-1 border border-[var(--border-color)] text-[var(--dim-color)]"
              >
                {{ l }}
              </span>
            </div>
          </div>
        </div>

        <!-- Right Issue Detail + Take Action -->
        <div class="flex-1 min-w-0 overflow-y-auto p-4 flex flex-col gap-4">
          <div
            v-if="!store.selectedIssueDetail && !store.isLoadingForgeDetail"
            class="text-xs font-mono text-[var(--dim-color)] m-auto"
          >
            Select an issue to inspect context or take it into Project Management.
          </div>
          <div
            v-else-if="store.isLoadingForgeDetail"
            class="text-xs font-mono text-[var(--dim-color)] m-auto"
          >
            Loading issue details...
          </div>
          <template v-else-if="store.selectedIssueDetail">
            <div class="flex flex-wrap items-start justify-between gap-2 border-b border-[var(--border-color)] pb-3">
              <div class="flex flex-col gap-1 min-w-0">
                <div class="flex items-center gap-2 text-[10px] font-mono text-[var(--dim-color)]">
                  <span class="font-bold text-[var(--accent-color)]">#{{ store.selectedIssueDetail.number }}</span>
                  <span class="uppercase px-1.5 border border-[var(--border-color)]">{{ store.selectedIssueDetail.state }}</span>
                  <span>by @{{ store.selectedIssueDetail.author }}</span>
                  <span>{{ formatIsoShort(store.selectedIssueDetail.createdAt) }}</span>
                </div>
                <h3 class="text-sm font-bold text-[var(--fg-color)]">{{ store.selectedIssueDetail.title }}</h3>
                <div v-if="store.selectedIssueDetail.labels.length > 0" class="flex flex-wrap gap-1 pt-0.5">
                  <span
                    v-for="lbl in store.selectedIssueDetail.labels"
                    :key="lbl"
                    class="text-[9px] font-mono px-1.5 py-0.5 border border-[var(--border-color)] text-[var(--dim-color)]"
                  >
                    {{ lbl }}
                  </span>
                </div>
              </div>
              <a
                :href="store.selectedIssueDetail.url"
                target="_blank"
                rel="noopener noreferrer"
                class="px-2 py-1 text-[10px] font-mono border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--accent-color)] flex items-center gap-1 shrink-0"
              >
                <ExternalLink :size="11" />
                <span>{{ providerLabel }}</span>
              </a>
            </div>

            <!-- Take Issue Action Box -->
            <div class="p-3 border border-[var(--accent-color)]/40 bg-[var(--ui-bg-card)]/40 flex flex-col gap-2">
              <div class="flex items-center justify-between">
                <span class="text-[10px] font-bold uppercase tracking-wider text-[var(--accent-color)]">
                  Take Issue into Workspace
                </span>
                <AppTooltip text="Creates a linked Project Task, plus optional Git branch and Journal todo">
                  <button
                    type="button"
                    :disabled="store.isTakingIssue"
                    @click="handleTakeIssue"
                    class="px-3 py-1 text-[10px] font-bold uppercase border border-[var(--accent-color)] bg-[var(--accent-color)] text-black hover:opacity-90 cursor-pointer flex items-center gap-1 disabled:opacity-40"
                  >
                    <Play :size="10" />
                    <span>{{ store.isTakingIssue ? 'Taking...' : 'Take Issue' }}</span>
                  </button>
                </AppTooltip>
              </div>
              <div class="grid grid-cols-1 md:grid-cols-2 gap-2 text-[11px] font-mono items-center">
                <div class="flex items-center gap-2">
                  <AppToggle v-model="takeCreateBranch" />
                  <AppTooltip text="Creates branch without switching active HEAD">
                    <span class="text-[var(--fg-color)] shrink-0">Create branch:</span>
                  </AppTooltip>
                  <div class="flex-1 min-w-0">
                    <AppInput
                      v-model="takeBranchName"
                      placeholder="Optional branch (e.g. fix/issue-...)"
                    />
                  </div>
                </div>
                <div class="flex items-center gap-2">
                  <AppToggle v-model="takeCreateTodo" />
                  <span class="text-[var(--fg-color)]">Create linked Journal todo</span>
                </div>
              </div>
            </div>

            <!-- Issue Body -->
            <div class="p-3 border border-[var(--border-color)] bg-[var(--bg-color)]/60 text-xs font-mono text-[var(--fg-color)] whitespace-pre-wrap break-words leading-relaxed">
              {{ store.selectedIssueDetail.body || '(No description provided)' }}
            </div>

            <!-- Comments -->
            <div v-if="store.selectedIssueDetail.comments.length > 0" class="flex flex-col gap-2">
              <span class="text-[10px] font-bold uppercase tracking-wider text-[var(--dim-color)]">
                Comments ({{ store.selectedIssueDetail.comments.length }})
              </span>
              <div
                v-for="comment in store.selectedIssueDetail.comments"
                :key="comment.createdAt + '-' + comment.author"
                class="p-2.5 border border-[var(--border-color)] bg-[var(--ui-bg-card)]/20 flex flex-col gap-1"
              >
                <div class="flex items-center justify-between text-[10px] font-mono text-[var(--dim-color)]">
                  <span class="font-bold text-[var(--fg-color)]">@{{ comment.author }}</span>
                  <span>{{ formatIsoShort(comment.createdAt) }}</span>
                </div>
                <div class="text-xs font-mono text-[var(--fg-color)] whitespace-pre-wrap break-words">
                  {{ comment.body }}
                </div>
              </div>
            </div>
          </template>
        </div>
      </div>
    </div>

    <!-- PULL REQUESTS / MERGE REQUESTS VIEW -->
    <div v-else-if="activeTab === 'prs'" class="flex flex-col flex-1 min-h-0 overflow-hidden">
      <!-- PR Filters + New Draft PR Trigger -->
      <div class="px-3 py-2 border-b border-[var(--border-color)] bg-[var(--ui-bg-card)]/20 flex flex-wrap items-center justify-between gap-2 shrink-0">
        <div class="flex flex-wrap items-center gap-2">
          <div class="w-28 shrink-0">
            <AppSelect
              :model-value="prState"
              :options="prStateOptions"
              @update:model-value="val => { prState = val; loadPrs(false); }"
            />
          </div>
          <div class="flex items-center gap-1">
            <button
              type="button"
              @click="prAuthorMe = false; loadPrs(false)"
              class="px-2 py-1 text-[10px] font-mono uppercase border cursor-pointer"
              :class="!prAuthorMe
                ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] font-bold'
                : 'border-[var(--border-color)] text-[var(--dim-color)]'"
            >
              All
            </button>
            <button
              type="button"
              @click="prAuthorMe = true; loadPrs(false)"
              class="px-2 py-1 text-[10px] font-mono uppercase border cursor-pointer"
              :class="prAuthorMe
                ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] font-bold'
                : 'border-[var(--border-color)] text-[var(--dim-color)]'"
            >
              Mine
            </button>
          </div>
          <div class="flex items-center gap-1">
            <button
              type="button"
              @click="prInboxFilter = 'all'"
              class="px-2 py-1 text-[9px] font-mono uppercase border cursor-pointer"
              :class="prInboxFilter === 'all'
                ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] font-bold'
                : 'border-[var(--border-color)] text-[var(--dim-color)]'"
            >
              All Inbox
            </button>
            <button
              type="button"
              @click="prInboxFilter = 'inbox'"
              class="px-2 py-1 text-[9px] font-mono uppercase border cursor-pointer flex items-center gap-1"
              :class="prInboxFilter === 'inbox'
                ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] font-bold'
                : 'border-[var(--border-color)] text-[var(--dim-color)]'"
            >
              <Inbox :size="10" />
              <span>Unread</span>
            </button>
            <button
              type="button"
              @click="prInboxFilter = 'flagged'"
              class="px-2 py-1 text-[9px] font-mono uppercase border cursor-pointer flex items-center gap-1"
              :class="prInboxFilter === 'flagged'
                ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] font-bold'
                : 'border-[var(--border-color)] text-[var(--dim-color)]'"
            >
              <Bookmark :size="10" />
              <span>Flagged</span>
            </button>
          </div>
        </div>

        <AppTooltip v-if="supportsDraftPr" text="Open a new Draft Pull Request">
          <button
            type="button"
            @click="openDraftPrForm"
            class="px-2.5 py-1 text-[10px] font-bold uppercase border border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-black cursor-pointer flex items-center gap-1"
          >
            <Plus :size="11" />
            <span>Draft PR</span>
          </button>
        </AppTooltip>
      </div>

      <!-- Inline Draft PR Creator -->
      <div
        v-if="showDraftPrForm && supportsDraftPr"
        class="p-3 border-b border-[var(--border-color)] bg-[var(--ui-bg-card)]/50 flex flex-col gap-2 shrink-0"
      >
        <div class="flex items-center justify-between">
          <span class="text-[10px] font-bold uppercase text-[var(--accent-color)]">
            Create Draft Pull Request
          </span>
          <button type="button" @click="showDraftPrForm = false" class="text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer">
            <X :size="13" />
          </button>
        </div>
        <div class="grid grid-cols-1 md:grid-cols-3 gap-2">
          <AppInput v-model="draftTargetRepo" placeholder="Target repo (owner/repo)" />
          <AppInput v-model="draftBaseBranch" placeholder="Base branch (e.g. main)" />
          <AppInput v-model="draftHeadBranch" placeholder="Head branch (e.g. fix/issue-12)" />
        </div>
        <AppInput v-model="draftTitle" placeholder="Draft PR title..." />
        <AppTextarea v-model="draftBody" :rows="3" placeholder="Summary, Closes #..., verification notes..." />
        <div class="flex items-center justify-end gap-2">
          <button
            type="button"
            :disabled="store.isCreatingDraftPr || !draftTitle.trim() || !draftTargetRepo.trim() || !draftBaseBranch.trim() || !draftHeadBranch.trim()"
            @click="handleCreateDraftPr"
            class="px-3 py-1 text-[10px] font-bold uppercase border border-[var(--accent-color)] bg-[var(--accent-color)] text-black hover:opacity-90 cursor-pointer disabled:opacity-40"
          >
            {{ store.isCreatingDraftPr ? 'Creating...' : 'Create Draft PR' }}
          </button>
        </div>
      </div>

      <!-- Split List + Detail -->
      <div class="flex-1 min-h-0 flex overflow-hidden">
        <div class="w-full md:w-[380px] lg:w-[420px] shrink-0 border-r border-[var(--border-color)] overflow-y-auto p-2 flex flex-col gap-1.5">
          <div
            v-if="displayedPrs.length === 0 && !store.isLoadingForge"
            class="p-4 text-xs font-mono text-[var(--dim-color)]"
          >
            No {{ prTabLabel.toLowerCase() }} found.
          </div>
          <div
            v-for="pr in displayedPrs"
            :key="pr.number"
            @click="handleSelectPr(pr.number)"
            class="text-left p-2.5 border transition-colors cursor-pointer flex flex-col gap-1"
            :class="selectedPrNumber === pr.number
              ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/10'
              : 'border-[var(--border-color)] bg-[var(--ui-bg-card)]/30 hover:border-[var(--accent-color)]/60'"
          >
            <div class="flex items-center justify-between gap-2 text-[10px] font-mono">
              <div class="flex items-center gap-1.5">
                <span class="font-bold text-[var(--accent-color)]">#{{ pr.number }}</span>
                <span
                  v-if="inboxEntryFor('pr', pr.number)?.inboxStatus !== 'read'"
                  class="px-1 border border-[var(--status-info-border)] bg-[var(--status-info-bg)] text-[var(--status-info-color)] uppercase text-[8px]"
                >
                  {{ inboxEntryFor('pr', pr.number)?.inboxStatus === 'updated' ? 'UPD' : 'NEW' }}
                </span>
                <span v-if="pr.isDraft" class="px-1 border border-[var(--border-color)] text-[var(--dim-color)] uppercase text-[9px]">
                  DRAFT
                </span>
                <span class="px-1 border uppercase text-[9px]" :class="checksBadgeClass(pr.checksSummary)">
                  {{ pr.checksSummary }}
                </span>
              </div>
              <div class="flex items-center gap-1.5 text-[var(--dim-color)]">
                <span>{{ formatIsoShort(pr.updatedAt) }}</span>
                <AppTooltip text="Toggle flag">
                  <button
                    type="button"
                    @click.stop="toggleFlag('pr', pr.number)"
                    class="p-0.5 cursor-pointer hover:text-[var(--accent-color)]"
                    :class="inboxEntryFor('pr', pr.number)?.triageFlag === 'flagged' ? 'text-[var(--accent-color)]' : ''"
                  >
                    <Bookmark :size="11" />
                  </button>
                </AppTooltip>
                <AppTooltip :text="inboxEntryFor('pr', pr.number)?.isRead ? 'Mark unread' : 'Mark read'">
                  <button
                    type="button"
                    @click.stop="toggleRead('pr', pr.number, pr.updatedAt)"
                    class="p-0.5 cursor-pointer hover:text-[var(--status-ok-color)]"
                    :class="inboxEntryFor('pr', pr.number)?.isRead ? 'text-[var(--status-ok-color)]' : ''"
                  >
                    <Check :size="11" />
                  </button>
                </AppTooltip>
              </div>
            </div>
            <div class="text-xs font-bold text-[var(--fg-color)] leading-snug">
              {{ pr.title }}
            </div>
            <div class="text-[9px] font-mono text-[var(--dim-color)] truncate">
              @{{ pr.author }} · {{ pr.headRefName }} -> {{ pr.baseRefName }}
            </div>
          </div>
        </div>

        <div class="flex-1 min-w-0 overflow-y-auto p-4 flex flex-col gap-4">
          <div
            v-if="!store.selectedPrDetail && !store.isLoadingForgeDetail"
            class="text-xs font-mono text-[var(--dim-color)] m-auto"
          >
            Select an item to inspect checks, reviews, and diff stats.
          </div>
          <div
            v-else-if="store.isLoadingForgeDetail"
            class="text-xs font-mono text-[var(--dim-color)] m-auto"
          >
            Loading details...
          </div>
          <template v-else-if="store.selectedPrDetail">
            <div class="flex flex-wrap items-start justify-between gap-2 border-b border-[var(--border-color)] pb-3">
              <div class="flex flex-col gap-1 min-w-0">
                <div class="flex flex-wrap items-center gap-2 text-[10px] font-mono text-[var(--dim-color)]">
                  <span class="font-bold text-[var(--accent-color)]">#{{ store.selectedPrDetail.number }}</span>
                  <span class="uppercase px-1.5 border border-[var(--border-color)]">{{ store.selectedPrDetail.state }}</span>
                  <span v-if="store.selectedPrDetail.isDraft" class="uppercase px-1.5 border border-[var(--border-color)]">DRAFT</span>
                  <span>{{ store.selectedPrDetail.headRefName }} -> {{ store.selectedPrDetail.baseRefName }}</span>
                  <span>
                    {{ store.selectedPrDetail.changedFilesCount }} files
                    (+{{ store.selectedPrDetail.additionsCount }} / -{{ store.selectedPrDetail.deletionsCount }})
                  </span>
                </div>
                <h3 class="text-sm font-bold text-[var(--fg-color)]">{{ store.selectedPrDetail.title }}</h3>
              </div>
              <a
                :href="store.selectedPrDetail.url"
                target="_blank"
                rel="noopener noreferrer"
                class="px-2 py-1 text-[10px] font-mono border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--accent-color)] flex items-center gap-1 shrink-0"
              >
                <ExternalLink :size="11" />
                <span>{{ providerLabel }}</span>
              </a>
            </div>

            <div class="p-3 border border-[var(--border-color)] bg-[var(--bg-color)]/60 text-xs font-mono text-[var(--fg-color)] whitespace-pre-wrap break-words leading-relaxed">
              {{ store.selectedPrDetail.body || '(No description)' }}
            </div>

            <!-- Checks -->
            <div v-if="store.selectedPrDetail.checks.length > 0" class="flex flex-col gap-1.5">
              <span class="text-[10px] font-bold uppercase tracking-wider text-[var(--dim-color)]">
                CI Checks ({{ store.selectedPrDetail.checks.length }})
              </span>
              <div class="grid grid-cols-1 sm:grid-cols-2 gap-1.5">
                <div
                  v-for="chk in store.selectedPrDetail.checks"
                  :key="chk.name"
                  class="px-2 py-1.5 border border-[var(--border-color)] bg-[var(--ui-bg-card)]/20 flex items-center justify-between gap-2 text-[10px] font-mono"
                >
                  <span class="truncate text-[var(--fg-color)]">{{ chk.name }}</span>
                  <span class="uppercase text-[var(--dim-color)] shrink-0">
                    {{ chk.conclusion || chk.status }}
                  </span>
                </div>
              </div>
            </div>

            <!-- Reviews -->
            <div v-if="store.selectedPrDetail.reviews.length > 0" class="flex flex-col gap-1.5">
              <span class="text-[10px] font-bold uppercase tracking-wider text-[var(--dim-color)]">
                Reviews ({{ store.selectedPrDetail.reviews.length }})
              </span>
              <div
                v-for="rev in store.selectedPrDetail.reviews"
                :key="rev.submittedAt + '-' + rev.author + '-' + rev.state"
                class="p-2 border border-[var(--border-color)] bg-[var(--ui-bg-card)]/20 flex flex-col gap-1 text-xs font-mono"
              >
                <div class="flex items-center justify-between text-[10px] text-[var(--dim-color)]">
                  <span class="font-bold text-[var(--fg-color)]">@{{ rev.author }}</span>
                  <span class="uppercase">{{ rev.state }}</span>
                </div>
                <div v-if="rev.body" class="whitespace-pre-wrap break-words text-[var(--fg-color)]">{{ rev.body }}</div>
              </div>
            </div>

            <!-- Comments -->
            <div v-if="store.selectedPrDetail.comments.length > 0" class="flex flex-col gap-1.5">
              <span class="text-[10px] font-bold uppercase tracking-wider text-[var(--dim-color)]">
                Comments ({{ store.selectedPrDetail.comments.length }})
              </span>
              <div
                v-for="comment in store.selectedPrDetail.comments"
                :key="comment.createdAt + '-' + comment.author"
                class="p-2 border border-[var(--border-color)] bg-[var(--ui-bg-card)]/20 flex flex-col gap-1 text-xs font-mono"
              >
                <div class="flex items-center justify-between text-[10px] text-[var(--dim-color)]">
                  <span class="font-bold text-[var(--fg-color)]">@{{ comment.author }}</span>
                  <span>{{ formatIsoShort(comment.createdAt) }}</span>
                </div>
                <div class="whitespace-pre-wrap break-words text-[var(--fg-color)]">{{ comment.body }}</div>
              </div>
            </div>
          </template>
        </div>
      </div>
    </div>

    <!-- RELEASES VIEW -->
    <div v-else class="flex-1 min-h-0 overflow-y-auto p-4 flex flex-col gap-2">
      <div
        v-if="store.releases.length === 0 && !store.isLoadingForge"
        class="text-xs font-mono text-[var(--dim-color)]"
      >
        No releases published on target repository.
      </div>
      <div
        v-for="rel in store.releases"
        :key="rel.tagName"
        class="p-3 border border-[var(--border-color)] bg-[var(--ui-bg-card)]/30 flex items-center justify-between gap-3"
      >
        <div class="flex flex-col gap-0.5 min-w-0">
          <div class="flex items-center gap-2 text-xs font-mono">
            <span class="font-bold text-[var(--accent-color)]">{{ rel.tagName }}</span>
            <span class="font-bold text-[var(--fg-color)] truncate">{{ rel.name }}</span>
            <span v-if="rel.isPrerelease" class="text-[9px] px-1 border border-[var(--status-warn-border)] text-[var(--status-warn-color)] uppercase">
              prerelease
            </span>
            <span v-if="rel.isDraft" class="text-[9px] px-1 border border-[var(--border-color)] text-[var(--dim-color)] uppercase">
              draft
            </span>
          </div>
          <span class="text-[10px] font-mono text-[var(--dim-color)]">
            Published {{ formatIsoShort(rel.publishedAt) }}
          </span>
        </div>
        <a
          :href="rel.url"
          target="_blank"
          rel="noopener noreferrer"
          class="px-2 py-1 text-[10px] font-mono border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--accent-color)] flex items-center gap-1 shrink-0"
        >
          <ExternalLink :size="11" />
          <span>Release</span>
        </a>
      </div>
    </div>
  </div>
</template>
