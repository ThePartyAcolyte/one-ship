<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue';
import { daemonCall } from '../../utils/daemonClient';
import type { GitCommitInfo, ProjectRecord, Snapshot, VcsAction, WorkingTreeFile } from '../../types';
import {
  GitCommitHorizontal,
  RotateCcw,
  Plus,
  Minus,
  Trash2,
  FileText,
  CheckSquare,
  Square,
  User,
  Clock,
  Loader2,
  X,
  AlertCircle,
  FileCode,
  CheckCircle2,
  Eye,
  Sparkles,
  ArrowUpDown,
  Check,
  Bot,
  Image as ImageIcon,
} from '@lucide/vue';
import { AppTooltip, AppInput, AppTextarea, AppEmptyState } from '../ui';
import GitBranchSelector from './GitBranchSelector.vue';
import { useWorkspaceLayoutStore } from '../../stores/workspaceLayoutStore';
import { formatIsoTimestamp as formatIso } from '../../utils/format';

type VcsActionItem = VcsAction;

interface FileEntry {
  path: string;
  status?: string;
  is_staged?: boolean;
  staged_code?: string;
  unstaged_code?: string;
  is_deleted?: boolean;
  is_untracked?: boolean;
  exists_on_disk?: boolean;
  is_domain_asset?: boolean;
  old_path?: string | null;
}

interface ParsedDiffLine {
  type: 'header' | 'hunk' | 'added' | 'removed' | 'context';
  oldLineNo?: number;
  newLineNo?: number;
  content: string;
}

type TimelineItem = 
  | { kind: 'snapshot'; dateMs: number; data: Snapshot }
  | { kind: 'commit'; dateMs: number; data: GitCommitInfo };

const props = defineProps<{
  project: ProjectRecord;
  activeListeningProjectId?: number | null;
  voiceMode?: boolean;
}>();

const emit = defineEmits<{
  (e: 'restored', files: string[]): void;
  (e: 'dispatched', instruction: string): void;
}>();

const layoutStore = useWorkspaceLayoutStore();

// State
const snapshots = ref<Snapshot[]>([]);
const commits = ref<GitCommitInfo[]>([]);
const workingTreeFiles = ref<WorkingTreeFile[]>([]);
const vcsActions = ref<VcsActionItem[]>([]);

const timelineFilter = ref<'all' | 'snapshots' | 'commits'>('all');
const snapshotDomainFilter = ref<'all' | 'code' | 'pcbcraft' | 'solidcraft' | 'desigma'>('all');
const milestoneDiffMode = ref<'working_tree' | 'previous_milestone'>('working_tree');
const selectedItemKey = ref<string>('working_tree');
const selectedFile = ref<string | null>(null);
const selectedFilesToRestore = ref<Set<string>>(new Set());

const currentDiff = ref<string>('');
const isDiffLoading = ref(false);
const isLoading = ref(false);
const isProcessing = ref(false);
const isDispatching = ref(false);

const commitMessage = ref('');
const isGeneratingCommitMsg = ref(false);
const isCommitting = ref(false);
const isSyncing = ref(false);
const statusMessage = ref<string | null>(null);

const filePaneWidthPx = ref(288);
const isResizingFilePane = ref(false);
let resizeStartX = 0;
let resizeStartWidthPx = 288;

function onFilePaneResizeMove(e: PointerEvent) {
  if (!isResizingFilePane.value) return;
  const delta = e.clientX - resizeStartX;
  filePaneWidthPx.value = Math.max(220, Math.min(560, resizeStartWidthPx + delta));
}

function stopFilePaneResize() {
  if (!isResizingFilePane.value) return;
  isResizingFilePane.value = false;
  window.removeEventListener('pointermove', onFilePaneResizeMove);
  window.removeEventListener('pointerup', stopFilePaneResize);
}

function startFilePaneResize(e: PointerEvent) {
  e.preventDefault();
  isResizingFilePane.value = true;
  resizeStartX = e.clientX;
  resizeStartWidthPx = filePaneWidthPx.value;
  window.addEventListener('pointermove', onFilePaneResizeMove);
  window.addEventListener('pointerup', stopFilePaneResize);
}

function splitFilePath(fullPath: string): { name: string; dir: string } {
  const idx = fullPath.lastIndexOf('/');
  if (idx === -1) return { name: fullPath, dir: '' };
  return {
    name: fullPath.slice(idx + 1),
    dir: fullPath.slice(0, idx),
  };
}

const MAX_RENDERED_FILES_COUNT = 250;

const stagedWorkingTreeFiles = computed(() =>
  workingTreeFiles.value.filter(f => f.is_staged)
);
const unstagedWorkingTreeFiles = computed(() =>
  workingTreeFiles.value.filter(f => !f.is_staged || (f.unstaged_code !== ' ' && f.unstaged_code !== '?'))
);
const visibleStagedFiles = computed(() =>
  stagedWorkingTreeFiles.value.slice(0, MAX_RENDERED_FILES_COUNT)
);
const visibleUnstagedFiles = computed(() =>
  unstagedWorkingTreeFiles.value.slice(0, MAX_RENDERED_FILES_COUNT)
);
const visibleCurrentFileList = computed(() =>
  currentFileList.value.slice(0, MAX_RENDERED_FILES_COUNT)
);

// Submodal for adding action to rula.toml
const showAddActionModal = ref(false);
const newActionName = ref('');
const newActionPrompt = ref('');

const availableSnapshotDomains = computed(() => {
  const counts: Record<string, number> = { code: 0, pcbcraft: 0, solidcraft: 0, desigma: 0 };
  for (const snap of snapshots.value) {
    const dom = snap.domain || 'code';
    counts[dom] = (counts[dom] ?? 0) + 1;
  }
  return counts;
});

function formatDomainBadge(domain?: string): string {
  if (domain === 'pcbcraft') return 'PCB';
  if (domain === 'solidcraft') return 'CAD';
  if (domain === 'desigma') return 'DESIGN';
  return 'CODE';
}

// Computed timeline items interleaved chronologically (newest first)
const timelineItems = computed<TimelineItem[]>(() => {
  const items: TimelineItem[] = [];

  if (timelineFilter.value === 'all' || timelineFilter.value === 'snapshots') {
    for (const snap of snapshots.value) {
      const dom = snap.domain || 'code';
      if (snapshotDomainFilter.value !== 'all' && dom !== snapshotDomainFilter.value) {
        continue;
      }
      const dateMs = new Date(snap.created_at).getTime() || 0;
      items.push({ kind: 'snapshot', dateMs, data: snap });
    }
  }

  if (timelineFilter.value === 'all' || timelineFilter.value === 'commits') {
    for (const commit of commits.value) {
      const dateMs = new Date(commit.timestamp).getTime() || 0;
      items.push({ kind: 'commit', dateMs, data: commit });
    }
  }

  items.sort((a, b) => b.dateMs - a.dateMs);
  return items;
});

// Currently active selection details
const isWorkingTreeSelected = computed(() => selectedItemKey.value === 'working_tree');
const activeSnapshot = computed<Snapshot | null>(() => {
  if (selectedItemKey.value.startsWith('snap_')) {
    const id = selectedItemKey.value.replace('snap_', '');
    return snapshots.value.find(s => s.id === id) || null;
  }
  return null;
});

const previousMilestoneSnapshot = computed<Snapshot | null>(() => {
  const current = activeSnapshot.value;
  if (!current || !current.label.startsWith('milestone:')) {
    return null;
  }
  const currentMs = new Date(current.created_at).getTime() || 0;
  const olderMilestones = snapshots.value
    .filter(s => s.id !== current.id && (s.domain || 'code') === 'code' && s.label.startsWith('milestone:'))
    .filter(s => (new Date(s.created_at).getTime() || 0) <= currentMs)
    .sort((a, b) => (new Date(b.created_at).getTime() || 0) - (new Date(a.created_at).getTime() || 0));
  return olderMilestones[0] || null;
});

const activeCommit = computed<GitCommitInfo | null>(() => {
  if (selectedItemKey.value.startsWith('commit_')) {
    const hash = selectedItemKey.value.replace('commit_', '');
    return commits.value.find(c => c.hash === hash) || null;
  }
  return null;
});

// Current file list for inspector
const currentFileList = ref<FileEntry[]>([]);

const selectedFileEntry = computed<FileEntry | null>(() => {
  if (!selectedFile.value) return null;
  return currentFileList.value.find(f => f.path === selectedFile.value) || null;
});

function getFileStatusBadge(file: FileEntry) {
  if (file.is_deleted || file.status === 'deleted' || file.exists_on_disk === false) {
    return {
      text: 'D',
      class: 'border-[var(--status-error-border)] text-[var(--status-error-color)] bg-[var(--status-error-bg)]',
      title: 'Deleted (not present on disk)',
    };
  }
  if (file.is_untracked || file.status === 'untracked') {
    return {
      text: 'U',
      class: 'border-[var(--status-info-border)] text-[var(--status-info-color)] bg-[var(--status-info-bg)]',
      title: 'Untracked',
    };
  }
  if (file.status === 'added') {
    return {
      text: 'A',
      class: 'border-[var(--status-ok-border)] text-[var(--status-ok-color)] bg-[var(--status-ok-bg)]',
      title: 'Added',
    };
  }
  if (file.status === 'renamed') {
    return {
      text: 'R',
      class: 'border-[var(--accent-color)]/50 text-[var(--accent-color)] bg-[var(--accent-color)]/10',
      title: file.old_path ? `Renamed from ${file.old_path}` : 'Renamed',
    };
  }
  return {
    text: 'M',
    class: 'border-[var(--status-warn-border)] text-[var(--status-warn-color)] bg-[var(--status-warn-bg)]',
    title: 'Modified',
  };
}

// Reactive computed diff lines with accurate line numbers for dual gutters.
// A working tree can contain very large generated/design files; rendering one row
// per diff line without a bound can exhaust the WebKit renderer (large diffs have
// been observed in the multi-megabyte range). Keep the viewer bounded and ask the
// operator to select a file when the global diff is truncated.
const MAX_DIFF_RENDER_LINES_COUNT = 4_000;
const MAX_DIFF_LINE_CHARS_COUNT = 2_000;
const MAX_DIFF_PARSE_CHARS_COUNT = 4_000_000;

const isDiffTextTruncated = computed(
  () => currentDiff.value.length > MAX_DIFF_PARSE_CHARS_COUNT
);

const diffTextForDisplay = computed(() => {
  if (!currentDiff.value) return '';
  return currentDiff.value.length > MAX_DIFF_PARSE_CHARS_COUNT
    ? currentDiff.value.slice(0, MAX_DIFF_PARSE_CHARS_COUNT)
    : currentDiff.value;
});

const diffRawLines = computed<string[]>(() =>
  diffTextForDisplay.value ? diffTextForDisplay.value.split('\n') : []
);

const diffTotalLinesCount = computed(() => diffRawLines.value.length);

const isDiffTruncated = computed(
  () => diffTotalLinesCount.value > MAX_DIFF_RENDER_LINES_COUNT
);

const diffHasLongLines = computed(() =>
  diffRawLines.value
    .slice(0, MAX_DIFF_RENDER_LINES_COUNT)
    .some((line) => line.length > MAX_DIFF_LINE_CHARS_COUNT)
);

const parsedDiffLines = computed<ParsedDiffLine[]>(() => {
  if (!currentDiff.value) return [];
  const lines = diffRawLines.value.slice(0, MAX_DIFF_RENDER_LINES_COUNT);
  const result: ParsedDiffLine[] = [];
  let currentOldLine = 0;
  let currentNewLine = 0;
  let inHunk = false;

  const hunkRegex = /^@@\s+-(\d+)(?:,\d+)?\s+\+(\d+)(?:,\d+)?\s+@@/;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const content = line.length > MAX_DIFF_LINE_CHARS_COUNT
      ? `${line.slice(0, MAX_DIFF_LINE_CHARS_COUNT)} … [line truncated]`
      : line;

    // Check for hunk header
    const hunkMatch = line.match(hunkRegex);
    if (hunkMatch) {
      currentOldLine = parseInt(hunkMatch[1], 10);
      currentNewLine = parseInt(hunkMatch[2], 10);
      inHunk = true;
      result.push({
        type: 'hunk',
        content,
      });
      continue;
    }

    if (!inHunk) {
      result.push({
        type: 'header',
        content,
      });
      continue;
    }

    if (line.startsWith('+') && !line.startsWith('+++')) {
      result.push({
        type: 'added',
        newLineNo: currentNewLine++,
        content,
      });
    } else if (line.startsWith('-') && !line.startsWith('---')) {
      result.push({
        type: 'removed',
        oldLineNo: currentOldLine++,
        content,
      });
    } else if (line.startsWith('\\')) {
      result.push({
        type: 'header',
        content,
      });
    } else {
      // Context line
      result.push({
        type: 'context',
        oldLineNo: currentOldLine++,
        newLineNo: currentNewLine++,
        content,
      });
    }
  }

  return result;
});

// Centralized navigation handler to open file in Reader/Editor tab
async function openFileInReader(filePath: string) {
  if (!props.project || !filePath) return;
  const entry = currentFileList.value.find(f => f.path === filePath);
  if (entry && (entry.is_deleted || entry.exists_on_disk === false)) {
    statusMessage.value = `Cannot open '${filePath}': file does not exist on disk.`;
    return;
  }
  await layoutStore.navigateTo({
    projectId: props.project.id,
    projectPath: props.project.path,
    tab: 'files',
    filePath,
  });
}

// Refresh all telemetry and history
async function refreshAll() {
  if (!props.project) return;
  isLoading.value = true;
  try {
    await Promise.all([
      loadSnapshots(),
      loadGitLog(),
      loadWorkingTreeFiles(),
      loadVcsActions(),
    ]);

    // Default select working tree if dirty, or latest item
    if (workingTreeFiles.value.length > 0) {
      selectWorkingTree();
    } else if (timelineItems.value.length > 0) {
      const first = timelineItems.value[0];
      if (first.kind === 'snapshot') {
        selectSnapshot(first.data);
      } else {
        selectCommit(first.data);
      }
    } else {
      selectWorkingTree();
    }
  } catch (err: unknown) {
    statusMessage.value = `Refresh error: ${err}`;
  } finally {
    isLoading.value = false;
  }
}

async function loadSnapshots() {
  if (!props.project) return;
  try {
    const list = await daemonCall('list_project_snapshots', {
      projectId: props.project.id,
      conversationId: null,
      agentId: null,
      limitCount: 100,
    });
    snapshots.value = list;
  } catch (err: unknown) {
    console.error('Failed to load snapshots:', err);
  }
}

async function loadGitLog() {
  if (!props.project) return;
  try {
    const list = await daemonCall('get_project_git_log', {
      projectId: props.project.id,
      limitCount: 50,
    });
    commits.value = list;
  } catch (err: unknown) {
    console.error('Failed to load git log:', err);
  }
}

async function loadWorkingTreeFiles() {
  if (!props.project) return;
  try {
    const files = await daemonCall('get_working_tree_files', {
      projectId: props.project.id,
    });
    workingTreeFiles.value = files;
    if (isWorkingTreeSelected.value) {
      currentFileList.value = files.map(f => ({
        path: f.path,
        status: f.status,
        is_staged: f.is_staged,
        staged_code: f.staged_code,
        unstaged_code: f.unstaged_code,
        is_deleted: f.is_deleted,
        is_untracked: f.is_untracked,
        exists_on_disk: f.exists_on_disk,
        is_domain_asset: f.is_domain_asset,
        old_path: f.old_path,
      }));
    }
  } catch (err: unknown) {
    console.error('Failed to load working tree files:', err);
  }
}

async function loadVcsActions() {
  if (!props.project) return;
  try {
    const actions = await daemonCall('get_vcs_actions', {
      projectId: props.project.id,
    });
    vcsActions.value = actions;
  } catch (err: unknown) {
    console.error('Failed to load vcs actions:', err);
  }
}

// Selection handlers
function selectWorkingTree() {
  selectedItemKey.value = 'working_tree';
  selectedFile.value = null;
  currentFileList.value = workingTreeFiles.value.map(f => ({
    path: f.path,
    status: f.status,
    is_staged: f.is_staged,
    staged_code: f.staged_code,
    unstaged_code: f.unstaged_code,
    is_deleted: f.is_deleted,
    is_untracked: f.is_untracked,
    exists_on_disk: f.exists_on_disk,
    is_domain_asset: f.is_domain_asset,
    old_path: f.old_path,
  }));
  currentDiff.value = '';
}

function selectSnapshot(snap: Snapshot) {
  selectedItemKey.value = `snap_${snap.id}`;
  milestoneDiffMode.value = 'working_tree';
  selectedFilesToRestore.value = new Set(snap.files_changed || []);
  selectedFile.value = null;
  currentFileList.value = (snap.files_changed || []).map(p => ({ path: p }));
  currentDiff.value = '';
}

function setMilestoneDiffMode(mode: 'working_tree' | 'previous_milestone') {
  if (milestoneDiffMode.value === mode) return;
  milestoneDiffMode.value = mode;
  if (selectedFile.value) {
    loadDiff();
  }
}

async function selectCommit(commit: GitCommitInfo) {
  selectedItemKey.value = `commit_${commit.hash}`;
  selectedFile.value = null;
  currentDiff.value = '';
  try {
    const files = await daemonCall('get_commit_files_changed', {
      projectId: props.project.id,
      commitHash: commit.hash,
    });
    currentFileList.value = files.map(f => ({
      path: f.path,
      status: f.status,
      is_deleted: f.status === 'deleted',
      is_domain_asset: f.is_domain_asset,
    }));
  } catch (err) {
    console.error('[ProjectGitSnapshotsTab] Failed to load commit files:', err);
    currentFileList.value = [];
  }
}

// Diff loader: only loads diff when a specific non-asset file is selected.
async function loadDiff() {
  if (!props.project || !selectedFile.value || selectedFileEntry.value?.is_domain_asset) {
    currentDiff.value = '';
    return;
  }
  isDiffLoading.value = true;
  try {
    if (isWorkingTreeSelected.value) {
      const diff = await daemonCall('get_working_tree_diff', {
        projectId: props.project.id,
        filePath: selectedFile.value,
      });
      currentDiff.value = diff;
    } else if (activeSnapshot.value) {
      const prev =
        milestoneDiffMode.value === 'previous_milestone'
          ? previousMilestoneSnapshot.value
          : null;
      const diff = await daemonCall('get_snapshot_diff', {
        projectId: props.project.id,
        snapshotId: prev ? prev.id : activeSnapshot.value.id,
        compareWith: prev ? activeSnapshot.value.id : null,
        filePath: selectedFile.value,
      });
      currentDiff.value = diff;
    } else if (activeCommit.value) {
      const diff = await daemonCall('get_git_commit_diff', {
        projectId: props.project.id,
        commitHash: activeCommit.value.hash,
        filePath: selectedFile.value,
      });
      currentDiff.value = diff;
    } else {
      currentDiff.value = '';
    }
  } catch (err: unknown) {
    currentDiff.value = `[Diff error: ${err}]`;
  } finally {
    isDiffLoading.value = false;
  }
}

function selectDiffFile(file: string | null) {
  selectedFile.value = file;
  loadDiff();
}

// Manual Git staging, commit, sync, and AI commit message handlers
async function handleStageFile(filePath: string) {
  if (!props.project || isProcessing.value) return;
  isProcessing.value = true;
  try {
    await daemonCall('stage_git_files', { projectId: props.project.id, paths: [filePath] });
    await loadWorkingTreeFiles();
  } catch (err: unknown) {
    statusMessage.value = `Stage failed: ${err}`;
  } finally {
    isProcessing.value = false;
  }
}

async function handleStageAll() {
  if (!props.project || isProcessing.value) return;
  isProcessing.value = true;
  try {
    await daemonCall('stage_git_files', { projectId: props.project.id, paths: null });
    await loadWorkingTreeFiles();
  } catch (err: unknown) {
    statusMessage.value = `Stage all failed: ${err}`;
  } finally {
    isProcessing.value = false;
  }
}

async function handleUnstageFile(filePath: string) {
  if (!props.project || isProcessing.value) return;
  isProcessing.value = true;
  try {
    await daemonCall('unstage_git_files', { projectId: props.project.id, paths: [filePath] });
    await loadWorkingTreeFiles();
  } catch (err: unknown) {
    statusMessage.value = `Unstage failed: ${err}`;
  } finally {
    isProcessing.value = false;
  }
}

async function handleUnstageAll() {
  if (!props.project || isProcessing.value) return;
  isProcessing.value = true;
  try {
    await daemonCall('unstage_git_files', { projectId: props.project.id, paths: null });
    await loadWorkingTreeFiles();
  } catch (err: unknown) {
    statusMessage.value = `Unstage all failed: ${err}`;
  } finally {
    isProcessing.value = false;
  }
}

async function handleGenerateCommitMessage() {
  if (!props.project || isGeneratingCommitMsg.value) return;
  if (stagedWorkingTreeFiles.value.length === 0) {
    statusMessage.value = 'Stage at least one file before generating a commit message.';
    return;
  }
  isGeneratingCommitMsg.value = true;
  try {
    const generated = await daemonCall('generate_commit_message', { projectId: props.project.id });
    commitMessage.value = generated;
    statusMessage.value = 'Commit message generated from staged changes.';
  } catch (err: unknown) {
    statusMessage.value = `Commit message generation failed: ${err}`;
  } finally {
    isGeneratingCommitMsg.value = false;
  }
}

async function handleCommit() {
  if (!props.project || isCommitting.value) return;
  const msg = commitMessage.value.trim();
  if (!msg) {
    statusMessage.value = 'Enter a commit message first.';
    return;
  }
  if (stagedWorkingTreeFiles.value.length === 0) {
    statusMessage.value = 'No staged changes to commit.';
    return;
  }
  isCommitting.value = true;
  try {
    const hash = await daemonCall('commit_git_changes', {
      projectId: props.project.id,
      message: msg,
    });
    commitMessage.value = '';
    selectedFile.value = null;
    currentDiff.value = '';
    statusMessage.value = `Committed staged changes as #${hash}.`;
    await Promise.all([loadWorkingTreeFiles(), loadGitLog()]);
  } catch (err: unknown) {
    statusMessage.value = `Commit failed: ${err}`;
  } finally {
    isCommitting.value = false;
  }
}

async function handleSync(mode: 'pull' | 'push' | 'sync') {
  if (!props.project || isSyncing.value) return;
  isSyncing.value = true;
  try {
    const out = await daemonCall('sync_git_branch', { projectId: props.project.id, mode });
    statusMessage.value = out || `Branch ${mode} completed.`;
    await Promise.all([loadWorkingTreeFiles(), loadGitLog()]);
  } catch (err: unknown) {
    statusMessage.value = `Sync (${mode}) failed: ${err}`;
  } finally {
    isSyncing.value = false;
  }
}

// Snapshot surgical restore handlers
function toggleFileSelection(file: string) {
  const set = new Set(selectedFilesToRestore.value);
  if (set.has(file)) {
    set.delete(file);
  } else {
    set.add(file);
  }
  selectedFilesToRestore.value = set;
}

function toggleSelectAll() {
  if (!activeSnapshot.value) return;
  const allFiles = activeSnapshot.value.files_changed || [];
  if (selectedFilesToRestore.value.size === allFiles.length) {
    selectedFilesToRestore.value = new Set();
  } else {
    selectedFilesToRestore.value = new Set(allFiles);
  }
}

async function handleRestoreSelected() {
  if (!activeSnapshot.value || !props.project) return;
  const files = Array.from(selectedFilesToRestore.value);
  if (files.length === 0) {
    statusMessage.value = 'No files selected for surgical rollback.';
    return;
  }
  isProcessing.value = true;
  try {
    const restored = await daemonCall('restore_project_snapshot', {
      projectId: props.project.id,
      snapshotId: activeSnapshot.value.id,
      files,
    });
    statusMessage.value = `Surgically restored ${restored.length} file(s) from snapshot ${activeSnapshot.value.id}.`;
    emit('restored', restored);
    await refreshAll();
  } catch (err: unknown) {
    statusMessage.value = `Rollback failed: ${err}`;
  } finally {
    isProcessing.value = false;
  }
}

async function handleFullRollback() {
  if (!activeSnapshot.value || !props.project) return;
  isProcessing.value = true;
  try {
    const restored = await daemonCall('restore_project_snapshot', {
      projectId: props.project.id,
      snapshotId: activeSnapshot.value.id,
      files: null,
    });
    statusMessage.value = `Full rollback executed to snapshot ${activeSnapshot.value.id}.`;
    emit('restored', restored);
    await refreshAll();
  } catch (err: unknown) {
    statusMessage.value = `Full rollback failed: ${err}`;
  } finally {
    isProcessing.value = false;
  }
}

async function handleDeleteSnapshot(snap: Snapshot) {
  if (!props.project) return;
  isProcessing.value = true;
  try {
    await daemonCall('delete_project_snapshot', {
      projectId: props.project.id,
      snapshotId: snap.id,
    });
    statusMessage.value = `Snapshot ${snap.id} deleted.`;
    await loadSnapshots();
  } catch (err: unknown) {
    statusMessage.value = `Failed to delete snapshot: ${err}`;
  } finally {
    isProcessing.value = false;
  }
}

// Agent dispatch
async function resolveDefaultGitAgentId(): Promise<string> {
  if (!props.project) return '';
  const scoped = await daemonCall('get_scoped_project_config', {
    projectPath: props.project.path,
  });
  const raw = scoped['vcs.default_agent_id']?.value;
  return typeof raw === 'string' ? raw.trim() : '';
}

async function handleDispatchInstruction(instructionToSend: string) {
  const text = instructionToSend.trim();
  if (!text || !props.project) return;
  isDispatching.value = true;
  try {
    const agentId = await daemonCall('dispatch_git_instruction', {
      projectId: props.project.id,
      instruction: text,
    });
    layoutStore.openFloatingChat(agentId, props.project.path);
    statusMessage.value = `Dispatched to '${agentId}': "${text}"`;
    emit('dispatched', text);
  } catch (err: unknown) {
    statusMessage.value = `Dispatch error: ${err}`;
  } finally {
    isDispatching.value = false;
  }
}

async function handleOpenGitAgent() {
  if (!props.project || isDispatching.value) return;
  isDispatching.value = true;
  try {
    const agentId = await resolveDefaultGitAgentId();
    if (!agentId) {
      statusMessage.value = 'No default Git agent configured. Select one in Project Settings -> Git & VCS.';
      return;
    }
    await daemonCall('launch_agent', {
      projectPath: props.project.path,
      projectName: props.project.name,
      agentId,
      runner: null,
      model: null,
      effort: null,
      accountId: null,
      cleanContext: null,
    });
    layoutStore.openFloatingChat(agentId, props.project.path);
    statusMessage.value = `Opened Git agent '${agentId}'.`;
  } catch (err: unknown) {
    statusMessage.value = `Failed to open Git agent: ${err}`;
  } finally {
    isDispatching.value = false;
  }
}

// Save custom VCS action into rula.toml
async function handleSaveAction() {
  if (!newActionName.value.trim() || !newActionPrompt.value.trim() || !props.project) return;
  try {
    await daemonCall('save_vcs_action', {
      projectId: props.project.id,
      name: newActionName.value.trim(),
      prompt: newActionPrompt.value.trim(),
    });
    statusMessage.value = `Added action '${newActionName.value.trim()}' to rula.toml`;
    showAddActionModal.value = false;
    newActionName.value = '';
    newActionPrompt.value = '';
    await loadVcsActions();
  } catch (err: unknown) {
    statusMessage.value = `Failed to save action: ${err}`;
  }
}

function getLabelBadgeColor(label: string) {
  if (label.startsWith('milestone')) return 'border-[var(--status-ok-border)] text-[var(--status-ok-color)] bg-[var(--status-ok-bg)]';
  if (label.startsWith('deploy')) return 'border-[var(--status-info-border)] text-[var(--status-info-color)] bg-[var(--status-info-bg)]';
  if (label === 'pre' || label === 'post') return 'border-[var(--status-warn-border)] text-[var(--status-warn-color)] bg-[var(--status-warn-bg)]';
  return 'border-[var(--border-color)] text-[var(--fg-color)] bg-[var(--ui-bg-card)]';
}

watch(() => props.project?.id, () => {
  refreshAll();
}, { immediate: true });

function handleKeyDown(e: KeyboardEvent) {
  if (e.key === 'Escape' && showAddActionModal.value) {
    showAddActionModal.value = false;
    newActionName.value = '';
    newActionPrompt.value = '';
  } else if (e.key === 'F5') {
    e.preventDefault();
    refreshAll();
  }
}

onMounted(() => {
  window.addEventListener('keydown', handleKeyDown);
});

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeyDown);
  stopFilePaneResize();
});
</script>

<template>
  <div class="flex-1 flex flex-col min-h-0 h-full bg-[var(--bg-color)] font-mono select-none overflow-hidden">

    <!-- ── MAIN DUAL-PANEL COCKPIT BODY ──────────────────────────── -->
    <div class="flex-1 flex min-h-0 divide-x divide-[var(--border-color)] overflow-hidden">
      
      <!-- ── LEFT: UNIFIED TIMELINE STREAM (w-80) ──────────────────── -->
      <div class="w-80 shrink-0 flex flex-col min-h-0 ui-surface-list">
        
        <!-- Filter Header Bar -->
        <div class="h-9 px-2 border-b border-[var(--border-color)] flex items-center justify-between ui-surface-title shrink-0 text-[9px]">
          <div class="flex items-center gap-1">
            <button 
              @click="timelineFilter = 'all'"
              class="h-6 px-2 border cursor-pointer uppercase tracking-wider transition-colors"
              :class="timelineFilter === 'all' 
                ? 'border-[var(--accent-color)] text-[var(--accent-color)] font-bold bg-[var(--accent-color)]/10' 
                : 'border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
            >
              All
            </button>
            <button 
              @click="timelineFilter = 'snapshots'"
              class="h-6 px-2 border cursor-pointer uppercase tracking-wider transition-colors"
              :class="timelineFilter === 'snapshots' 
                ? 'border-[var(--accent-color)] text-[var(--accent-color)] font-bold bg-[var(--accent-color)]/10' 
                : 'border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
            >
              Snaps ({{ snapshots.length }})
            </button>
            <button 
              @click="timelineFilter = 'commits'"
              class="h-6 px-2 border cursor-pointer uppercase tracking-wider transition-colors"
              :class="timelineFilter === 'commits' 
                ? 'border-[var(--accent-color)] text-[var(--accent-color)] font-bold bg-[var(--accent-color)]/10' 
                : 'border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
            >
              Commits ({{ commits.length }})
            </button>
          </div>

          <span v-if="isLoading" class="flex items-center gap-1 text-[var(--accent-color)]">
            <Loader2 :size="10" class="animate-spin" />
            <span>syncing</span>
          </span>
        </div>

        <!-- Snapshot Domain Filter Row -->
        <div
          v-if="timelineFilter !== 'commits' && (availableSnapshotDomains.pcbcraft > 0 || availableSnapshotDomains.solidcraft > 0 || availableSnapshotDomains.desigma > 0)"
          class="px-2 py-1 border-b border-[var(--border-color)]/60 ui-surface-title flex items-center gap-1 overflow-x-auto scrollbar-none shrink-0 text-[8px]"
        >
          <button
            v-for="opt in [
              { id: 'all', label: 'All' },
              { id: 'code', label: `Code (${availableSnapshotDomains.code})` },
              { id: 'pcbcraft', label: `PCB (${availableSnapshotDomains.pcbcraft})` },
              { id: 'solidcraft', label: `CAD (${availableSnapshotDomains.solidcraft})` },
              { id: 'desigma', label: `Design (${availableSnapshotDomains.desigma})` },
            ]"
            :key="opt.id"
            type="button"
            class="h-5 px-1.5 border uppercase cursor-pointer transition-colors shrink-0"
            :class="snapshotDomainFilter === opt.id
              ? 'border-[var(--accent-color)] text-[var(--accent-color)] font-bold bg-[var(--accent-color)]/10'
              : 'border-[var(--border-color)]/60 text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
            @click="snapshotDomainFilter = opt.id as typeof snapshotDomainFilter"
          >
            {{ opt.label }}
          </button>
        </div>

        <!-- Timeline Items List -->
        <div class="flex-1 overflow-y-auto p-2 flex flex-col gap-1.5 min-h-0">
          
          <!-- Persistent Top Item: Live Working Tree Changes -->
          <div 
            @click="selectWorkingTree()"
            class="border p-2 cursor-pointer transition-colors flex flex-col gap-1 relative text-[10px]"
            :class="isWorkingTreeSelected 
              ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/10 text-[var(--fg-color)]' 
              : 'border-[var(--border-color)] ui-surface-card hover:border-[var(--accent-color)]/60 text-[var(--dim-color)]'"
          >
            <div class="flex items-center justify-between">
              <span class="font-bold text-[var(--fg-color)] flex items-center gap-1.5">
                <span class="w-2 h-2 rounded-full" :class="workingTreeFiles.length > 0 ? 'bg-[var(--status-warn-color)] animate-pulse' : 'bg-[var(--status-ok-color)]'"></span>
                WORKING TREE
              </span>
              <span 
                class="px-1.5 py-0.2 text-[8px] font-bold uppercase border"
                :class="workingTreeFiles.length > 0 ? 'border-[var(--status-warn-border)] text-[var(--status-warn-color)]' : 'border-[var(--status-ok-border)] text-[var(--status-ok-color)]'"
              >
                {{ workingTreeFiles.length }} uncommitted
              </span>
            </div>
            <div class="text-[9px] text-[var(--dim-color)] truncate">
              {{ workingTreeFiles.length === 0 ? 'Working tree clean against HEAD' : `${workingTreeFiles.length} file(s) staged or modified on disk` }}
            </div>
          </div>

          <div class="border-b border-[var(--border-color)]/50 my-0.5"></div>

          <!-- Chronological Timeline Stream -->
          <div v-if="timelineItems.length === 0 && !isLoading" class="text-[10px] text-[var(--dim-color)] p-6 text-center italic">
            No history or snapshots recorded yet.
          </div>

          <div 
            v-for="item in timelineItems" 
            :key="item.kind === 'snapshot' ? `snap_${item.data.id}` : `commit_${item.data.hash}`"
            @click="item.kind === 'snapshot' ? selectSnapshot(item.data) : selectCommit(item.data)"
            class="border p-2 cursor-pointer transition-all flex flex-col gap-1 relative text-[10px]"
            :class="selectedItemKey === (item.kind === 'snapshot' ? `snap_${item.data.id}` : `commit_${item.data.hash}`)
              ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/10 text-[var(--fg-color)] shadow-[0_0_8px_color-mix(in_srgb,var(--accent-color)_10%,transparent)]' 
              : 'border-[var(--border-color)]/80 bg-[var(--bg-color)]/60 hover:border-[var(--border-color)] hover:bg-[var(--ui-bg-card)]/50 text-[var(--dim-color)]'"
          >
            <!-- Top badge row -->
            <div class="flex items-center justify-between gap-1">
              <div class="flex items-center gap-1 min-w-0">
                <span 
                  v-if="item.kind === 'snapshot'"
                  class="px-1.5 py-0.2 text-[8px] uppercase font-bold border shrink-0" 
                  :class="getLabelBadgeColor(item.data.label)"
                >
                  {{ item.data.label }}
                </span>
                <span
                  v-if="item.kind === 'snapshot' && item.data.domain && item.data.domain !== 'code'"
                  class="px-1 py-0.2 text-[8px] uppercase font-bold border border-[var(--accent-color)]/60 text-[var(--accent-color)] bg-[var(--accent-color)]/10 shrink-0"
                >
                  {{ formatDomainBadge(item.data.domain) }}
                </span>
                <span 
                  v-if="item.kind !== 'snapshot'"
                  class="px-1.5 py-0.2 text-[8px] uppercase font-bold border border-[var(--status-info-border)] text-[var(--status-info-color)] bg-[var(--status-info-bg)] flex items-center gap-1"
                >
                  <GitCommitHorizontal :size="10" />
                  <span>GIT COMMIT</span>
                </span>
              </div>

              <span class="text-[9px] text-[var(--dim-color)] flex items-center gap-1 shrink-0">
                <Clock :size="9" />
                {{ formatIso(item.kind === 'snapshot' ? item.data.created_at : item.data.timestamp) }}
              </span>
            </div>

            <!-- Content title row -->
            <div class="flex items-center justify-between text-[10px] min-w-0">
              <span class="font-bold text-[var(--fg-color)] truncate flex items-center gap-1">
                <span class="text-[var(--accent-color)] shrink-0">
                  {{ item.kind === 'snapshot' ? item.data.id : `#${item.data.short_hash}` }}
                </span>
                <AppTooltip :text="item.kind === 'snapshot' ? (item.data.asset_key || item.data.label) : item.data.message">
                  <span class="truncate font-mono inline-block">
                    {{ item.kind === 'snapshot' ? (item.data.asset_key ? `${item.data.asset_key} (${item.data.agent_id})` : `Snapshot (${item.data.agent_id})`) : item.data.message }}
                  </span>
                </AppTooltip>
              </span>
            </div>

            <!-- Footer row -->
            <div class="flex items-center justify-between text-[9px] text-[var(--dim-color)] border-t border-[var(--border-color)]/40 pt-1 mt-0.5">
              <AppTooltip :text="item.kind === 'snapshot' ? item.data.agent_id : item.data.author_name">
                <span class="flex items-center gap-1 truncate max-w-[170px]">
                  <User :size="9" />
                  {{ item.kind === 'snapshot' ? item.data.agent_id : item.data.author_name }}
                </span>
              </AppTooltip>

              <div class="flex items-center gap-1.5">
                <span v-if="item.kind === 'snapshot'" class="flex items-center gap-0.5">
                  <FileText :size="9" />
                  {{ item.data.files_changed?.length || 0 }} files
                </span>

                <AppTooltip v-if="item.kind === 'snapshot'" text="Delete snapshot">
                  <button 
                    @click.stop="handleDeleteSnapshot(item.data)"
                    class="p-0.5 hover:text-[var(--status-error-color)] opacity-60 hover:opacity-100 transition-opacity cursor-pointer ml-1"
                  >
                    <Trash2 :size="11" />
                  </button>
                </AppTooltip>
              </div>
            </div>
          </div>

        </div>
      </div>

      <!-- ── RIGHT: INSPECTOR & LIVE DIFF STAGE ────────────────────── -->
      <div class="flex-1 flex flex-col min-h-0 bg-[var(--bg-color)] overflow-hidden">
        
        <!-- Top Details / Action Bar for Selected Item -->
        <div class="h-9 px-3 border-b border-[var(--border-color)] flex justify-between items-center ui-surface-title shrink-0 gap-2 overflow-x-auto scrollbar-none">
          
          <div class="flex items-center gap-2 min-w-0">
            <GitBranchSelector :project="project" class="shrink-0" />
            <span class="h-4 w-px bg-[var(--border-color)] shrink-0"></span>

            <!-- Mode 1: Working Tree Selected -->
            <div v-if="isWorkingTreeSelected" class="flex items-center gap-2 text-[10px] min-w-0">
              <span class="font-bold text-[var(--accent-color)] shrink-0">WORKING TREE</span>
              <span class="px-1.5 py-0.5 border text-[8px] border-[var(--status-warn-border)] text-[var(--status-warn-color)] bg-[var(--status-warn-bg)] uppercase shrink-0">
                {{ stagedWorkingTreeFiles.length }} staged
              </span>
            </div>

            <!-- Mode 2: Snapshot Selected -->
            <div v-else-if="activeSnapshot" class="flex items-center gap-2 text-[10px] min-w-0">
              <span class="font-bold text-[var(--accent-color)] shrink-0">{{ activeSnapshot.id }}</span>
              <span class="px-1.5 py-0.5 border text-[8px] shrink-0" :class="getLabelBadgeColor(activeSnapshot.label)">
                {{ activeSnapshot.label }}
              </span>
              <span class="text-[var(--dim-color)] text-[10px] shrink-0">
                Agent: <strong class="text-[var(--fg-color)]">{{ activeSnapshot.agent_id }}</strong>
              </span>
              <div
                v-if="previousMilestoneSnapshot"
                class="flex items-center border border-[var(--border-color)] shrink-0 text-[8px]"
              >
                <AppTooltip text="Diff selected file against the live working tree" position="bottom">
                  <button
                    type="button"
                    @click="setMilestoneDiffMode('working_tree')"
                    class="h-5 px-1.5 uppercase cursor-pointer transition-colors"
                    :class="milestoneDiffMode === 'working_tree'
                      ? 'bg-[var(--accent-color)]/15 text-[var(--accent-color)] font-bold'
                      : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
                  >
                    vs Working Tree
                  </button>
                </AppTooltip>
                <AppTooltip :text="`Diff introduced relative to previous milestone (${previousMilestoneSnapshot.label})`" position="bottom">
                  <button
                    type="button"
                    @click="setMilestoneDiffMode('previous_milestone')"
                    class="h-5 px-1.5 uppercase cursor-pointer transition-colors border-l border-[var(--border-color)]"
                    :class="milestoneDiffMode === 'previous_milestone'
                      ? 'bg-[var(--accent-color)]/15 text-[var(--accent-color)] font-bold'
                      : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
                  >
                    vs {{ previousMilestoneSnapshot.label }}
                  </button>
                </AppTooltip>
              </div>
            </div>

            <!-- Mode 3: Git Commit Selected -->
            <div v-else-if="activeCommit" class="flex items-center gap-2 text-[10px] min-w-0 truncate">
              <span class="font-bold text-[var(--status-info-color)] shrink-0">#{{ activeCommit.short_hash }}</span>
              <span class="text-[var(--dim-color)] text-[10px] shrink-0">
                {{ activeCommit.author_name }}
              </span>
              <AppTooltip :text="activeCommit.message" position="bottom">
                <span class="text-[var(--dim-color)] text-[10px] truncate max-w-md inline-block">
                  "{{ activeCommit.message }}"
                </span>
              </AppTooltip>
            </div>
          </div>

          <!-- Right Action Buttons -->
          <div class="flex items-center gap-1.5 shrink-0">
            
            <!-- Snapshot Actions (Rollback buttons) -->
            <template v-if="activeSnapshot">
              <AppTooltip text="Surgically restore ONLY the selected files below from this snapshot" position="bottom">
                <button 
                  @click="handleRestoreSelected"
                  :disabled="isProcessing || selectedFilesToRestore.size === 0"
                  class="h-6 px-2 bg-[var(--accent-color)]/15 border border-[var(--accent-color)] text-[9px] text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-[var(--bg-color)] transition-colors cursor-pointer flex items-center gap-1 disabled:opacity-40 font-bold uppercase tracking-wider"
                >
                  <RotateCcw :size="10" />
                  <span>RESTORE SELECTED ({{ selectedFilesToRestore.size }})</span>
                </button>
              </AppTooltip>

              <AppTooltip text="Restore ALL files in the repository to this snapshot state" position="bottom">
                <button 
                  @click="handleFullRollback"
                  :disabled="isProcessing"
                  class="h-6 px-2 border border-[var(--status-warn-border)] text-[9px] text-[var(--status-warn-color)] hover:bg-[var(--status-warn-color)] hover:text-[var(--bg-color)] transition-colors cursor-pointer flex items-center gap-1 disabled:opacity-40 uppercase font-bold tracking-wider"
                >
                  <RotateCcw :size="10" />
                  <span>FULL ROLLBACK</span>
                </button>
              </AppTooltip>
            </template>

            <!-- Working Tree Actions: Quick Actions + Add + Sync + Open Agent -->
            <template v-else-if="isWorkingTreeSelected">
              <AppTooltip
                v-for="act in vcsActions"
                :key="act.name"
                :text="act.prompt"
                position="bottom"
              >
                <button
                  @click="handleDispatchInstruction(act.prompt)"
                  :disabled="isDispatching"
                  class="h-6 px-2 border border-[var(--border-color)] text-[9px] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] transition-colors cursor-pointer uppercase tracking-wider disabled:opacity-40"
                >
                  {{ act.name }}
                </button>
              </AppTooltip>

              <AppTooltip text="Add quick Git action to rula.toml" position="bottom">
                <button
                  @click="showAddActionModal = true"
                  aria-label="Add quick Git action"
                  class="ui-icon-btn border border-[var(--border-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)]"
                >
                  <Plus :size="10" />
                </button>
              </AppTooltip>

              <span class="h-4 w-px bg-[var(--border-color)] mx-0.5"></span>

              <AppTooltip text="Pull (rebase) and push current branch with remote" position="bottom">
                <button 
                  @click="handleSync('sync')"
                  :disabled="isSyncing"
                  class="h-6 px-2 border border-[var(--border-color)] text-[9px] text-[var(--fg-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] transition-colors cursor-pointer flex items-center gap-1 font-bold uppercase tracking-wider disabled:opacity-40"
                >
                  <Loader2 v-if="isSyncing" :size="10" class="animate-spin" />
                  <ArrowUpDown v-else :size="10" />
                  <span>SYNC</span>
                </button>
              </AppTooltip>

              <AppTooltip text="Open Git agent session for interactive VCS tasks" position="bottom">
                <button 
                  @click="handleOpenGitAgent"
                  :disabled="isDispatching"
                  class="h-6 px-2 border border-[var(--accent-color)] text-[9px] text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-[var(--bg-color)] transition-colors cursor-pointer flex items-center gap-1 font-bold uppercase tracking-wider disabled:opacity-40"
                >
                  <Loader2 v-if="isDispatching" :size="10" class="animate-spin" />
                  <Bot v-else :size="10" />
                  <span>OPEN AGENT</span>
                </button>
              </AppTooltip>
            </template>

          </div>

        </div>

        <!-- Content Area: Dual Sub-Panel (Resizable Files/Staging on left, Diff Viewer on right) -->
        <div class="flex-1 flex min-h-0 overflow-hidden">
          
          <!-- Sub-Panel Left: Touched / Changed Files & Staging -->
          <div
            class="shrink-0 flex flex-col min-h-0 ui-surface-list overflow-x-hidden"
            :style="{ width: `${filePaneWidthPx}px` }"
          >
            
            <!-- Working Tree Commit Composer -->
            <div v-if="isWorkingTreeSelected" class="p-2 border-b border-[var(--border-color)] ui-surface-card flex flex-col gap-1.5 shrink-0">
              <div class="relative">
                <AppTextarea
                  v-model="commitMessage"
                  :rows="2"
                  placeholder="Commit message (Conventional Commits)..."
                  class="!text-[10px] !pr-7"
                  @keydown.ctrl.enter.prevent="handleCommit"
                />
                <AppTooltip text="Generate commit message from staged diff via AI" position="bottom">
                  <button
                    @click="handleGenerateCommitMessage"
                    :disabled="isGeneratingCommitMsg || stagedWorkingTreeFiles.length === 0"
                    aria-label="Generate commit message via AI"
                    class="absolute top-1.5 right-1.5 ui-icon-btn border border-[var(--border-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] disabled:opacity-35"
                  >
                    <Loader2 v-if="isGeneratingCommitMsg" :size="11" class="animate-spin" />
                    <Sparkles v-else :size="11" />
                  </button>
                </AppTooltip>
              </div>
              <div class="flex items-center gap-1.5">
                <button
                  @click="handleCommit"
                  :disabled="isCommitting || !commitMessage.trim() || stagedWorkingTreeFiles.length === 0"
                  class="flex-1 h-7 px-2 border border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-[var(--bg-color)] text-[9px] font-bold uppercase tracking-wider flex items-center justify-center gap-1 transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
                >
                  <Loader2 v-if="isCommitting" :size="11" class="animate-spin" />
                  <Check v-else :size="11" />
                  <span>COMMIT ({{ stagedWorkingTreeFiles.length }})</span>
                </button>
              </div>
            </div>

            <!-- Working Tree Staged / Unstaged Sections -->
            <template v-if="isWorkingTreeSelected">
              <div class="flex-1 overflow-y-auto flex flex-col min-h-0">
                <div v-if="workingTreeFiles.length === 0" class="p-4 text-[10px] text-[var(--dim-color)] italic text-center">
                  No modified files in working tree.
                </div>

                <template v-else>
                  <!-- STAGED CHANGES -->
                  <div class="px-2 py-1 border-b border-[var(--border-color)] flex items-center justify-between text-[9px] uppercase tracking-wider text-[var(--dim-color)] ui-surface-title shrink-0">
                    <span>Staged Changes ({{ stagedWorkingTreeFiles.length }})</span>
                    <AppTooltip v-if="stagedWorkingTreeFiles.length > 0" text="Unstage all files" position="bottom">
                      <button
                        @click="handleUnstageAll"
                        :disabled="isProcessing"
                        aria-label="Unstage all files"
                        class="ui-icon-btn hover:text-[var(--accent-color)]"
                      >
                        <Minus :size="11" />
                      </button>
                    </AppTooltip>
                  </div>

                  <div v-if="stagedWorkingTreeFiles.length > 0" class="p-1 flex flex-col gap-0.5 border-b border-[var(--border-color)]/60">
                    <div
                      v-if="stagedWorkingTreeFiles.length > MAX_RENDERED_FILES_COUNT"
                      class="px-2 py-1 mb-0.5 border border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)] text-[9px] leading-tight"
                    >
                      Showing first {{ MAX_RENDERED_FILES_COUNT }} of {{ stagedWorkingTreeFiles.length.toLocaleString() }} staged files (all staged files are included on commit).
                    </div>
                    <div
                      v-for="file in visibleStagedFiles"
                      :key="`staged_${file.path}`"
                      @click="selectDiffFile(file.path)"
                      class="flex items-center justify-between gap-1.5 px-2 py-1 text-[10px] border border-transparent hover:border-[var(--border-color)] hover:bg-[var(--ui-bg-card)] transition-colors cursor-pointer group min-w-0"
                      :class="selectedFile === file.path ? 'bg-[var(--accent-color)]/10 border-[var(--accent-color)]/50 font-bold' : ''"
                    >
                      <div class="flex items-center gap-1.5 min-w-0 flex-1 overflow-hidden">
                        <span
                          class="px-1 py-0.5 text-[8px] font-bold uppercase border shrink-0 font-mono leading-none inline-block"
                          :class="getFileStatusBadge(file).class"
                        >
                          {{ file.staged_code !== ' ' ? file.staged_code : getFileStatusBadge(file).text }}
                        </span>
                        <span
                          v-if="file.is_domain_asset"
                          class="px-1 py-0.5 text-[8px] font-bold uppercase border border-[var(--border-color)] text-[var(--dim-color)] shrink-0 leading-none"
                        >
                          ASSET
                        </span>
                        <div class="min-w-0 flex-1 overflow-hidden">
                          <div class="truncate text-[10px] font-mono text-[var(--fg-color)] leading-tight">
                            {{ splitFilePath(file.path).name }}
                          </div>
                          <div v-if="splitFilePath(file.path).dir" class="truncate text-[8px] font-mono text-[var(--dim-color)] leading-tight">
                            {{ splitFilePath(file.path).dir }}
                          </div>
                        </div>
                      </div>
                      <div class="flex items-center gap-0.5 shrink-0">
                        <AppTooltip text="Unstage file" position="top">
                          <button
                            @click.stop="handleUnstageFile(file.path)"
                            aria-label="Unstage file"
                            class="ui-icon-btn opacity-70 group-hover:opacity-100 hover:text-[var(--accent-color)]"
                          >
                            <Minus :size="11" />
                          </button>
                        </AppTooltip>
                      </div>
                    </div>
                  </div>

                  <!-- UNSTAGED / UNTRACKED CHANGES -->
                  <div class="px-2 py-1 border-b border-[var(--border-color)] flex items-center justify-between text-[9px] uppercase tracking-wider text-[var(--dim-color)] ui-surface-title shrink-0">
                    <span>Changes ({{ unstagedWorkingTreeFiles.length }})</span>
                    <AppTooltip v-if="unstagedWorkingTreeFiles.length > 0" text="Stage all changes" position="bottom">
                      <button
                        @click="handleStageAll"
                        :disabled="isProcessing"
                        aria-label="Stage all changes"
                        class="ui-icon-btn hover:text-[var(--accent-color)]"
                      >
                        <Plus :size="11" />
                      </button>
                    </AppTooltip>
                  </div>

                  <div v-if="unstagedWorkingTreeFiles.length > 0" class="p-1 flex flex-col gap-0.5">
                    <div
                      v-if="unstagedWorkingTreeFiles.length > MAX_RENDERED_FILES_COUNT"
                      class="px-2 py-1 mb-0.5 border border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)] text-[9px] leading-tight"
                    >
                      Showing first {{ MAX_RENDERED_FILES_COUNT }} of {{ unstagedWorkingTreeFiles.length.toLocaleString() }} files. Add a .gitignore to exclude vendor/build folders.
                    </div>
                    <div
                      v-for="file in visibleUnstagedFiles"
                      :key="`unstaged_${file.path}`"
                      @click="selectDiffFile(file.path)"
                      class="flex items-center justify-between gap-1.5 px-2 py-1 text-[10px] border border-transparent hover:border-[var(--border-color)] hover:bg-[var(--ui-bg-card)] transition-colors cursor-pointer group min-w-0"
                      :class="selectedFile === file.path ? 'bg-[var(--accent-color)]/10 border-[var(--accent-color)]/50 font-bold' : ''"
                    >
                      <div class="flex items-center gap-1.5 min-w-0 flex-1 overflow-hidden">
                        <AppTooltip :text="getFileStatusBadge(file).title">
                          <span 
                            class="px-1 py-0.5 text-[8px] font-bold uppercase border shrink-0 font-mono leading-none inline-block"
                            :class="getFileStatusBadge(file).class"
                          >
                            {{ getFileStatusBadge(file).text }}
                          </span>
                        </AppTooltip>
                        <span
                          v-if="file.is_domain_asset"
                          class="px-1 py-0.5 text-[8px] font-bold uppercase border border-[var(--border-color)] text-[var(--dim-color)] shrink-0 leading-none"
                        >
                          ASSET
                        </span>
                        <div class="min-w-0 flex-1 overflow-hidden">
                          <div
                            class="truncate text-[10px] font-mono leading-tight"
                            :class="file.is_deleted || file.exists_on_disk === false ? 'line-through text-[var(--status-error-color)]/80' : 'text-[var(--fg-color)]'"
                          >
                            {{ splitFilePath(file.path).name }}
                          </div>
                          <div v-if="splitFilePath(file.path).dir" class="truncate text-[8px] font-mono text-[var(--dim-color)] leading-tight">
                            {{ splitFilePath(file.path).dir }}
                          </div>
                        </div>
                      </div>
                      <div class="flex items-center gap-0.5 shrink-0">
                        <AppTooltip text="Stage file" position="top">
                          <button
                            @click.stop="handleStageFile(file.path)"
                            aria-label="Stage file"
                            class="ui-icon-btn opacity-70 group-hover:opacity-100 hover:text-[var(--accent-color)]"
                          >
                            <Plus :size="11" />
                          </button>
                        </AppTooltip>
                      </div>
                    </div>
                  </div>
                </template>
              </div>
            </template>

            <!-- Snapshot / Commit File List -->
            <template v-else>
              <div class="h-8 px-2 border-b border-[var(--border-color)] flex justify-between items-center text-[9px] uppercase tracking-wider text-[var(--dim-color)] ui-surface-title shrink-0">
                <span>Files Changed ({{ currentFileList.length }})</span>
                <button 
                  v-if="activeSnapshot"
                  @click="toggleSelectAll" 
                  class="hover:text-[var(--accent-color)] cursor-pointer"
                >
                  {{ selectedFilesToRestore.size === currentFileList.length ? 'Deselect All' : 'Select All' }}
                </button>
              </div>

              <div class="flex-1 overflow-y-auto p-1 flex flex-col gap-0.5 min-h-0">
                <div 
                  v-if="currentFileList.length === 0"
                  class="p-4 text-[10px] text-[var(--dim-color)] italic text-center"
                >
                  No files recorded for this item.
                </div>

                <div
                  v-if="currentFileList.length > MAX_RENDERED_FILES_COUNT"
                  class="px-2 py-1 mb-0.5 border border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)] text-[9px] leading-tight"
                >
                  Showing first {{ MAX_RENDERED_FILES_COUNT }} of {{ currentFileList.length.toLocaleString() }} files.
                </div>
                <div 
                  v-for="file in visibleCurrentFileList" 
                  :key="file.path"
                  class="flex items-center justify-between px-2 py-1 text-[10px] border border-transparent hover:border-[var(--border-color)] hover:bg-[var(--ui-bg-card)] transition-colors cursor-pointer group"
                  :class="selectedFile === file.path ? 'bg-[var(--accent-color)]/10 border-[var(--accent-color)]/50 font-bold' : ''"
                  @click="selectDiffFile(file.path)"
                >
                  <div class="flex items-center gap-1.5 min-w-0 flex-1">
                    <button 
                      v-if="activeSnapshot"
                      @click.stop="toggleFileSelection(file.path)"
                      aria-label="Toggle file for restore"
                      class="text-[var(--dim-color)] hover:text-[var(--accent-color)] cursor-pointer shrink-0"
                    >
                      <CheckSquare v-if="selectedFilesToRestore.has(file.path)" :size="13" class="text-[var(--accent-color)]" />
                      <Square v-else :size="13" />
                    </button>

                    <AppTooltip v-if="!activeSnapshot" :text="getFileStatusBadge(file).title">
                      <span 
                        class="px-1 py-0.5 text-[8px] font-bold uppercase border shrink-0 font-mono leading-none inline-block"
                        :class="getFileStatusBadge(file).class"
                      >
                        {{ getFileStatusBadge(file).text }}
                      </span>
                    </AppTooltip>

                    <FileCode v-else :size="11" class="text-[var(--dim-color)] shrink-0" />

                    <span
                      v-if="file.is_domain_asset"
                      class="px-1 py-0.5 text-[8px] font-bold uppercase border border-[var(--border-color)] text-[var(--dim-color)] shrink-0 leading-none"
                    >
                      ASSET
                    </span>

                    <div class="min-w-0 flex-1 overflow-hidden">
                      <div
                        class="truncate text-[10px] font-mono leading-tight"
                        :class="file.is_deleted || file.status === 'deleted' || file.exists_on_disk === false ? 'line-through text-[var(--status-error-color)]/80' : 'text-[var(--fg-color)]'"
                      >
                        {{ splitFilePath(file.path).name }}
                      </div>
                      <div v-if="splitFilePath(file.path).dir" class="truncate text-[8px] font-mono text-[var(--dim-color)] leading-tight">
                        {{ splitFilePath(file.path).dir }}
                      </div>
                    </div>
                  </div>

                  <div class="flex items-center gap-1 shrink-0">
                    <AppTooltip 
                      :text="file.is_deleted || file.exists_on_disk === false ? 'File deleted on disk (cannot open)' : 'Open in Explorer'" 
                      position="top"
                    >
                      <button 
                        :disabled="file.is_deleted || file.exists_on_disk === false"
                        @click.stop="openFileInReader(file.path)"
                        aria-label="Open file in Explorer"
                        class="ui-icon-btn"
                        :class="file.is_deleted || file.exists_on_disk === false 
                          ? 'opacity-20 cursor-not-allowed text-[var(--status-error-color)]' 
                          : 'hover:text-[var(--accent-color)] text-[var(--dim-color)] opacity-40 hover:opacity-100'"
                      >
                        <Eye :size="11" />
                      </button>
                    </AppTooltip>
                  </div>
                </div>
              </div>

              <div v-if="activeSnapshot" class="h-7 px-2 border-t border-[var(--border-color)] flex justify-end items-center text-[9px] text-[var(--dim-color)] ui-surface-title shrink-0">
                <span>{{ selectedFilesToRestore.size }} selected</span>
              </div>
            </template>
          </div>

          <!-- Resizable Divider -->
          <div
            @pointerdown="startFilePaneResize"
            class="w-1 shrink-0 cursor-col-resize bg-[var(--border-color)] hover:bg-[var(--accent-color)] transition-colors"
            :class="isResizingFilePane ? 'bg-[var(--accent-color)]' : ''"
          ></div>

          <!-- Sub-Panel Right: Live File Diff Viewer -->
          <div class="flex-1 flex flex-col min-h-0 bg-[var(--bg-color)]">
            <div class="h-8 px-3 border-b border-[var(--border-color)] flex justify-between items-center text-[9px] text-[var(--dim-color)] ui-surface-title shrink-0">
              <span class="uppercase tracking-wider truncate">
                {{ selectedFile ? `Diff: ${selectedFile}` : 'File Diff Inspector' }}
              </span>
              <div class="flex items-center gap-2 shrink-0">
                <template v-if="selectedFile">
                  <div 
                    v-if="selectedFileEntry?.is_deleted || selectedFileEntry?.status === 'deleted' || selectedFileEntry?.exists_on_disk === false"
                    class="h-5 px-2 border border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] text-[8px] uppercase font-bold flex items-center gap-1"
                  >
                    <AlertCircle :size="10" />
                    <span>DELETED ON DISK</span>
                  </div>

                  <AppTooltip v-else :text="`Open '${selectedFile}' in Reader`" position="bottom">
                    <button 
                      @click="openFileInReader(selectedFile)"
                      class="h-6 px-2 border border-[var(--border-color)] hover:border-[var(--accent-color)] text-[9px] text-[var(--dim-color)] hover:text-[var(--accent-color)] flex items-center gap-1 font-bold uppercase transition-colors cursor-pointer"
                    >
                      <Eye :size="10" />
                      <span>Open in Reader</span>
                    </button>
                  </AppTooltip>
                </template>

                <span v-if="isDiffLoading" class="flex items-center gap-1 text-[var(--accent-color)]">
                  <Loader2 :size="10" class="animate-spin" />
                  <span>loading diff</span>
                </span>
              </div>
            </div>

            <div
              v-if="isDiffTextTruncated || isDiffTruncated || diffHasLongLines"
              class="px-3 py-1.5 border-b border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)] text-[9px] font-mono shrink-0 leading-relaxed"
            >
              <span v-if="isDiffTextTruncated">
                The diff exceeds {{ MAX_DIFF_PARSE_CHARS_COUNT.toLocaleString() }} characters; only the beginning is shown.
              </span>
              <span v-if="isDiffTruncated">
                Showing the first {{ MAX_DIFF_RENDER_LINES_COUNT.toLocaleString() }} of {{ diffTotalLinesCount.toLocaleString() }} diff lines.
              </span>
              <span v-if="diffHasLongLines">
                Lines longer than {{ MAX_DIFF_LINE_CHARS_COUNT.toLocaleString() }} characters are truncated.
              </span>
            </div>

            <div class="flex-1 overflow-auto text-[10px] font-mono select-text ui-surface-list">
              <AppEmptyState
                v-if="!selectedFile"
                :icon="FileCode"
                title="No file selected"
              >
                <template #description>
                  Select a file from the list to inspect its diff.
                </template>
              </AppEmptyState>

              <AppEmptyState
                v-else-if="selectedFileEntry?.is_domain_asset"
                :icon="ImageIcon"
                title="Design / PCB / CAD asset"
              >
                <template #description>
                  Diff preview is disabled for visual and hardware assets. The file remains tracked in Git.
                </template>
              </AppEmptyState>

              <div v-else-if="parsedDiffLines.length === 0 && !isDiffLoading" class="text-[10px] text-[var(--dim-color)] italic p-4">
                No differences found for this file.
              </div>

              <div v-else class="min-w-full inline-block font-mono leading-tight py-1">
                <div
                  v-for="(diffLine, lineNum) in parsedDiffLines"
                  :key="`${diffLine.type}-${diffLine.oldLineNo ?? 'x'}-${diffLine.newLineNo ?? 'x'}-${lineNum}`"
                  class="flex items-stretch hover:bg-white/[0.04] transition-colors"
                  :class="[
                    diffLine.type === 'added' ? 'bg-[var(--status-ok-bg)] text-[var(--status-ok-color)]' : '',
                    diffLine.type === 'removed' ? 'bg-[var(--status-error-bg)] text-[var(--status-error-color)]' : '',
                    diffLine.type === 'hunk' ? 'bg-[var(--status-info-bg)] text-[var(--status-info-color)] my-0.5 border-y border-[var(--status-info-border)]' : '',
                    diffLine.type === 'header' ? 'text-[var(--dim-color)] ui-surface-title font-bold' : '',
                    diffLine.type === 'context' ? 'text-[var(--fg-color)]/85' : ''
                  ]"
                >
                  <!-- Old Line Number Column -->
                  <span class="w-9 shrink-0 text-right pr-2 select-none text-[8px] opacity-40 ui-surface-title border-r border-[var(--border-color)]/30 font-mono flex items-center justify-end">
                    {{ diffLine.oldLineNo ?? '' }}
                  </span>

                  <!-- New Line Number Column -->
                  <span class="w-9 shrink-0 text-right pr-2 select-none text-[8px] opacity-40 ui-surface-title border-r border-[var(--border-color)]/30 font-mono flex items-center justify-end">
                    {{ diffLine.newLineNo ?? '' }}
                  </span>

                  <!-- Gutter Indicator (+ / - / @ / space) -->
                  <span class="w-4 shrink-0 text-center select-none opacity-60 font-bold flex items-center justify-center text-[9px]">
                    {{ diffLine.type === 'added' ? '+' : (diffLine.type === 'removed' ? '-' : (diffLine.type === 'hunk' ? '@' : ' ')) }}
                  </span>

                  <!-- Code Line Content -->
                  <span class="flex-1 px-1 whitespace-pre break-all">
                    {{ diffLine.type === 'added' || diffLine.type === 'removed' ? diffLine.content.slice(1) : diffLine.content }}
                  </span>
                </div>
              </div>
            </div>
          </div>

        </div>

      </div>

    </div>

    <!-- ── STATUS FOOTER ─────────────────────────────────────────── -->
    <div v-if="statusMessage" class="px-3 py-1.5 border-t border-[var(--border-color)] bg-[var(--ui-bg-card)]/70 text-[10px] text-[var(--accent-color)] flex justify-between items-center shrink-0">
      <span>{{ statusMessage }}</span>
      <button @click="statusMessage = null" class="text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer">
        <X :size="12" />
      </button>
    </div>

    <!-- ── SUBMODAL: ADD VCS ACTION TO RULA.TOML ─────────────────── -->
    <div v-if="showAddActionModal" class="fixed inset-0 ui-overlay-backdrop flex items-center justify-center z-60 p-4">
      <div class="bg-[var(--bg-color)] border border-[var(--border-color)] p-5 max-w-sm w-full flex flex-col gap-3 shadow-2xl">
        <h3 class="font-bold uppercase tracking-widest text-[var(--accent-color)] text-xs border-b border-[var(--border-color)] pb-1.5 flex items-center gap-1.5">
          <Plus :size="13" />
          <span>ADD ACTION // rula.toml</span>
        </h3>

        <div class="flex flex-col gap-1">
          <label class="text-[10px] uppercase text-[var(--dim-color)]">Action Name</label>
          <AppInput 
            v-model="newActionName"
            type="text" 
            placeholder="e.g. Sync Main"
          />
        </div>

        <div class="flex flex-col gap-1">
          <label class="text-[10px] uppercase text-[var(--dim-color)]">Agent Prompt</label>
          <AppTextarea 
            v-model="newActionPrompt"
            :rows="2"
            placeholder="e.g. Rebase onto origin/main and resolve conflicts..."
          />
        </div>

        <div class="flex justify-end gap-2 mt-2 border-t border-[var(--border-color)] pt-2">
          <button 
            @click="showAddActionModal = false; newActionName = ''; newActionPrompt = ''"
            class="px-3 py-1 text-[10px] font-bold uppercase border border-[var(--border-color)] hover:bg-[var(--fg-color)] hover:text-[var(--bg-color)] transition-colors cursor-pointer"
          >
            Cancel
          </button>
          <button 
            @click="handleSaveAction" 
            :disabled="!newActionName.trim() || !newActionPrompt.trim()"
            class="px-3 py-1 text-[10px] font-bold uppercase border transition-colors"
            :class="newActionName.trim() && newActionPrompt.trim()
              ? 'border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-black cursor-pointer' 
              : 'border-[var(--border-color)] text-[var(--dim-color)] opacity-50 cursor-not-allowed'"
          >
            Save Action
          </button>
        </div>
      </div>
    </div>

  </div>
</template>
