<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted, nextTick, type Component } from 'vue';
import { VueDraggable } from 'vue-draggable-plus';
import { reportAsyncFailure } from '../../utils/reportAsyncFailure';
import { 
  Folder, FolderPlus, FileText, Terminal, BookOpen, Search, Filter, 
  CornerLeftUp, Code, Braces, GitBranch, LayoutList, LayoutDashboard,
  Copy, Check, Paperclip, Edit3, Eye, Save, RotateCcw,
  X, AlertTriangle, Scissors, ClipboardCopy, Trash, Trash2,
  CheckCheck, Mic, MessageSquare, ChevronRight, ChevronDown,
  PanelLeftClose, PanelLeft, ExternalLink, Activity, Brain, BrainCircuit, Cpu, CircuitBoard, Layers, PenTool, Box, Kanban, Settings, Network,
  Bot, Sparkles, RefreshCw, ArrowUpRight, WrapText, ArrowLeft
} from '@lucide/vue';
import { marked } from 'marked';
import hljs from 'highlight.js';
import 'highlight.js/styles/atom-one-dark.css';
import { useDialogueStore } from '../../stores/dialogueStore';
import { useWorkspaceLayoutStore, type WorkspaceTab } from '../../stores/workspaceLayoutStore';
import { useProjectExplorerStore, type DocumentItem } from '../../stores/projectExplorerStore';
import { useJournalStore } from '../../stores/journalStore';
import { useCoreStore } from '../../stores/coreStore';
import { useHardwarePreviewStore } from '../../stores/hardwarePreviewStore';
import { useDocumentComments } from '../../composables/useDocumentComments';
import { formatBytes, formatTime as formatDate } from '../../utils/format';
import { daemonCall } from '../../utils/daemonClient';
import { onDaemonEvent } from '../../utils/daemonEvents';
import { onUiEvent } from '../../utils/uiEvents';
import { AppTooltip, AppTextarea, AppCodeEditor, AppInput, AppToggle, AppContextMenu, type ContextMenuItem } from '../ui';
import type { EditorCursorInfo, EditorContextMenuPayload, EditorDiagnosticsSummary } from '../ui/AppCodeEditor.vue';
import type { ProjectRecord, ProjectAgentState } from '../../types';
import type { ProjectExclusionStatus } from '../../types/generated/ProjectExclusionStatus';
import type { FileAnchoredItem } from '../../types/generated/FileAnchoredItem';
import type { FileCausalMinute } from '../../types/generated/FileCausalMinute';
import type { FileLineInsights } from '../../types/generated/FileLineInsights';
import type { FileLineRangeContent } from '../../types/generated/FileLineRangeContent';
import type { FileRelatedDoc } from '../../types/generated/FileRelatedDoc';
import type { FileSymbolItem } from '../../types/generated/FileSymbolItem';
import type { SymbolDefinitionLocation } from '../../types/generated/SymbolDefinitionLocation';
import DevlogJournalView from '../dialogue/DevlogJournalView.vue';
import ProjectVerticalRail from './ProjectVerticalRail.vue';
import ProjectTerminalGrid from './ProjectTerminalGrid.vue';
import ProjectOverviewTab from './ProjectOverviewTab.vue';
import ProjectConfigPanel from './ProjectConfigPanel.vue';
import ProjectGitSnapshotsTab from './ProjectGitSnapshotsTab.vue';
import ProjectProfilerTab from './ProjectProfilerTab.vue';
import ProjectSkillsTab from './ProjectSkillsTab.vue';
import ProjectReaTab from './ProjectReaTab.vue';
import ProjectElectronicsTab from './ProjectElectronicsTab.vue';
import ProjectDesignTab from './ProjectDesignTab.vue';
import ProjectCadTab from './ProjectCadTab.vue';
import ProjectPcbTab from './ProjectPcbTab.vue';
import ProjectPreviewTab from './ProjectPreviewTab.vue';
import ProjectManagementTab from './ProjectManagementTab.vue';
import ProjectKnowledgeGraphTab from './ProjectKnowledgeGraphTab.vue';
import GitBranchSelector from './GitBranchSelector.vue';
import AgentTaskbar from '../taskbar/AgentTaskbar.vue';

defineProps<{
  projects: ProjectRecord[];
  activeListeningProjectId?: number | null;
  voiceMode?: boolean;
  activeProjectMenuId?: number | null;
  agentStates?: Record<number, ProjectAgentState>;
  agentOutcomes?: Record<string, { status: string; timestamp: number }>;
  runningServices?: string[];
}>();

const emit = defineEmits<{
  (e: 'addProject'): void;
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

const coreStore = useCoreStore();
const dialogueStore = useDialogueStore();
const layoutStore = useWorkspaceLayoutStore();
const explorerStore = useProjectExplorerStore();
const journalStore = useJournalStore();
const hardwarePreviewStore = useHardwarePreviewStore();

// Custom directive for inline input auto-focus
const vFocus = {
  mounted: (el: HTMLElement) => {
    el.focus();
    if (el instanceof HTMLInputElement) {
      el.select();
    }
  }
};

// Active Workspace Primary Tab (Synced with centralized navigation)
const workspaceTab = computed<WorkspaceTab>({
  get: () => layoutStore.activeWorkspaceTab,
  set: (val: WorkspaceTab) => layoutStore.setWorkspaceTab(val),
});

// Active Project resolution
const activeProject = computed(() => {
  return coreStore.activeProject;
});

// Dynamic Workspace Tab Definitions and Order
interface TabDefinition {
  id: WorkspaceTab;
  label: string;
  icon: Component;
  isVisible: (project: ProjectRecord | null | undefined) => boolean;
}

const ALL_TABS: Record<WorkspaceTab, TabDefinition> = {
  settings: {
    id: 'settings',
    label: 'Settings',
    icon: Settings,
    isVisible: () => false, // Rendered as a fixed, non-movable gear icon to the left of Overview.
  },
  overview: {
    id: 'overview',
    label: 'Overview',
    icon: LayoutDashboard,
    isVisible: () => true,
  },
  project: {
    id: 'project',
    label: 'Project',
    icon: Kanban,
    isVisible: (p) => Boolean(p?.has_project_management_enabled && !p?.hidden_tabs?.includes('project')),
  },
  files: {
    id: 'files',
    label: 'Explorer',
    icon: FileText,
    isVisible: (p) => !p?.hidden_tabs?.includes('files'),
  },
  knowledge: {
    id: 'knowledge',
    label: 'Graph',
    icon: Network,
    isVisible: (p) => Boolean(p?.has_knowledge_graph_enabled && !p?.hidden_tabs?.includes('knowledge')),
  },
  electronics: {
    id: 'electronics',
    label: 'Electronics',
    icon: CircuitBoard,
    isVisible: (p) => Boolean(p?.has_electronics_enabled && coreStore.isIntegrationAvailable('electronics') && !p?.hidden_tabs?.includes('electronics')),
  },
  design: {
    id: 'design',
    label: 'Design',
    icon: PenTool,
    isVisible: (p) => Boolean(p?.has_desigma_enabled && coreStore.isIntegrationAvailable('desigma') && !p?.hidden_tabs?.includes('design')),
  },
  cad: {
    id: 'cad',
    label: 'CAD',
    icon: Box,
    isVisible: (p) => Boolean(p?.has_solidcraft_enabled && coreStore.isIntegrationAvailable('solidcraft') && !p?.hidden_tabs?.includes('cad')),
  },
  pcb: {
    id: 'pcb',
    label: 'PCB',
    icon: Layers,
    isVisible: (p) => Boolean(p?.has_pcbcraft_enabled && coreStore.isIntegrationAvailable('pcbcraft') && !p?.hidden_tabs?.includes('pcb')),
  },
  preview: {
    id: 'preview',
    label: 'Preview',
    icon: Eye,
    isVisible: (p) => Boolean(p?.preview_url && !p?.hidden_tabs?.includes('preview')),
  },
  profiler: {
    id: 'profiler',
    label: 'Profiling',
    icon: Activity,
    isVisible: (p) => Boolean(p?.has_profiler_enabled && !p?.hidden_tabs?.includes('profiler')),
  },
  rea: {
    id: 'rea',
    label: 'RE Lab',
    icon: Cpu,
    isVisible: (p) => Boolean(p?.has_rea_enabled && coreStore.isIntegrationAvailable('rea') && !p?.hidden_tabs?.includes('rea')),
  },
  terminal: {
    id: 'terminal',
    label: 'Scripts',
    icon: Terminal,
    isVisible: (p) => !p?.hidden_tabs?.includes('terminal'),
  },
  git: {
    id: 'git',
    label: 'Git & Snapshots',
    icon: GitBranch,
    isVisible: (p) => Boolean(p?.has_git && !p?.hidden_tabs?.includes('git')),
  },
  devlog: {
    id: 'devlog',
    label: 'Journal',
    icon: BookOpen,
    isVisible: (p) => !p?.hidden_tabs?.includes('devlog'),
  },
  skills: {
    id: 'skills',
    label: 'Skills',
    icon: BrainCircuit,
    isVisible: (p) => !p?.hidden_tabs?.includes('skills'),
  },
};

const DEFAULT_TAB_ORDER: WorkspaceTab[] = [
  'overview',
  'project',
  'files',
  'knowledge',
  'design',
  'cad',
  'pcb',
  'preview',
  'electronics',
  'profiler',
  'rea',
  'terminal',
  'git',
  'devlog',
  'skills',
];

const tabOrder = ref<WorkspaceTab[]>([...DEFAULT_TAB_ORDER]);

function loadTabOrderForProject(projectId?: number) {
  if (!projectId) {
    tabOrder.value = [...DEFAULT_TAB_ORDER];
    return;
  }
  try {
    const saved = localStorage.getItem(`oneship_tab_order_${projectId}`);
    if (saved) {
      const parsed: WorkspaceTab[] = JSON.parse(saved);
      if (Array.isArray(parsed) && parsed.length > 0) {
        const unique = parsed.filter((t) => t in ALL_TABS);
        const missing = DEFAULT_TAB_ORDER.filter((t) => !unique.includes(t));
        tabOrder.value = [...unique, ...missing];
        return;
      }
    }
  } catch (e) {
    console.warn('Failed to parse tab order from localStorage:', e);
  }
  tabOrder.value = [...DEFAULT_TAB_ORDER];
}

function saveTabOrderForProject(projectId?: number) {
  if (!projectId) return;
  try {
    localStorage.setItem(`oneship_tab_order_${projectId}`, JSON.stringify(tabOrder.value));
  } catch (e) {
    console.warn('Failed to save tab order to localStorage:', e);
  }
}

watch(
  () => activeProject.value?.id,
  (newId) => {
    loadTabOrderForProject(newId);
  },
  { immediate: true }
);

const visibleTabs = computed(() => {
  return tabOrder.value
    .map((tabId) => ALL_TABS[tabId])
    .filter((def): def is TabDefinition => Boolean(def && def.isVisible(activeProject.value)));
});

const reorderableTabs = computed<TabDefinition[]>({
  get: () => visibleTabs.value,
  set: (nextVisible) => {
    const nextVisibleIds = nextVisible.map((t) => t.id);
    const visibleSet = new Set(nextVisibleIds);
    const hiddenTabs = tabOrder.value.filter((id) => !visibleSet.has(id));
    tabOrder.value = [...nextVisibleIds, ...hiddenTabs];
    saveTabOrderForProject(activeProject.value?.id);
  },
});

// Fallback if an active tab belongs to a feature that is not enabled, unavailable on host, or hidden
watch([
  () => activeProject.value?.id,
  () => activeProject.value?.has_rea_enabled,
  () => activeProject.value?.has_profiler_enabled,
  () => activeProject.value?.has_git,
  () => activeProject.value?.has_electronics_enabled,
  () => activeProject.value?.has_desigma_enabled,
  () => activeProject.value?.has_solidcraft_enabled,
  () => activeProject.value?.has_pcbcraft_enabled,
  () => activeProject.value?.has_knowledge_graph_enabled,
  () => activeProject.value?.preview_url,
  () => activeProject.value?.hidden_tabs,
  () => coreStore.hostCapabilities,
], ([, rea, profiler, git, electronics, desigma, cad, pcb, knowledgeGraph, previewUrl, hiddenTabs]) => {
  if (workspaceTab.value === 'rea' && (!rea || !coreStore.isIntegrationAvailable('rea'))) workspaceTab.value = 'overview';
  if (workspaceTab.value === 'profiler' && !profiler) workspaceTab.value = 'overview';
  if (workspaceTab.value === 'git' && !git) workspaceTab.value = 'overview';
  if (workspaceTab.value === 'electronics' && (!electronics || !coreStore.isIntegrationAvailable('electronics'))) workspaceTab.value = 'overview';
  if (workspaceTab.value === 'design' && (!desigma || !coreStore.isIntegrationAvailable('desigma'))) workspaceTab.value = 'overview';
  if (workspaceTab.value === 'cad' && (!cad || !coreStore.isIntegrationAvailable('solidcraft'))) workspaceTab.value = 'overview';
  if (workspaceTab.value === 'pcb' && (!pcb || !coreStore.isIntegrationAvailable('pcbcraft'))) workspaceTab.value = 'overview';
  if (workspaceTab.value === 'knowledge' && !knowledgeGraph) workspaceTab.value = 'overview';
  if (workspaceTab.value === 'preview' && !previewUrl) workspaceTab.value = 'overview';
  if (hiddenTabs && hiddenTabs.includes(workspaceTab.value)) workspaceTab.value = 'overview';
});

function navigateToProjectSettings(project: ProjectRecord) {
  if (activeProject.value?.id !== project.id) {
    coreStore.setActiveProject(project.id);
  }
  workspaceTab.value = 'settings';
}

// ── FILE EXPLORER STATE & FILTERING ──────────────────────────
const explorerPath = computed<string>({
  get: () => explorerStore.explorerPath,
  set: (val: string) => explorerStore.setExplorerPath(val),
});
const isExplorerCollapsed = ref<boolean>(localStorage.getItem('oneship:files:is_explorer_collapsed') === 'true');

// Preview fullscreen owns the viewport: the rail and the agent bar are hidden so
// neither stacks above the preview nor reflows into the free space it leaves.
const isPreviewFullscreen = ref<boolean>(false);

function toggleExplorerCollapsed() {
  isExplorerCollapsed.value = !isExplorerCollapsed.value;
  try {
    localStorage.setItem('oneship:files:is_explorer_collapsed', String(isExplorerCollapsed.value));
  } catch (err) {
    console.warn('Failed to save isExplorerCollapsed:', err);
  }
}

const searchFilter = ref<string>('');
const docsFilterOnly = ref<boolean>(false);
const conversationFilterOnly = ref<boolean>(false);
const showFilterDropdown = ref<boolean>(false);

function toggleFilterDropdown(e: MouseEvent) {
  e.stopPropagation();
  showFilterDropdown.value = !showFilterDropdown.value;
}

function isMdFile(item: DocumentItem): boolean {
  if (item.is_dir) return false;
  const lower = item.name.toLowerCase();
  return lower.endsWith('.md') || lower.endsWith('.markdown') || item.doc_type === 'markdown';
}

function hasMdFilesInSubtree(item: DocumentItem): boolean {
  if (!item.is_dir) return isMdFile(item);
  if (item.children && item.children.length > 0) {
    return item.children.some(child => hasMdFilesInSubtree(child));
  }
  return false;
}

function isItemTouchedInConversation(item: DocumentItem): boolean {
  if (!item.is_dir) {
    return (
      dialogueStore.artifactsTouched.includes(item.rel_path) ||
      dialogueStore.conversationScratchpads.some(a => a.filePath === item.rel_path)
    );
  }
  if (item.children) {
    return item.children.some(child => isItemTouchedInConversation(child));
  }
  return false;
}

function countFolderFiles(item: DocumentItem): number {
  if (!item.is_dir) return 1;
  let count = 0;
  if (item.children) {
    for (const child of item.children) {
      count += countFolderFiles(child);
    }
  }
  return count;
}

// Find document item by rel_path recursively
function findItemByPath(items: DocumentItem[], targetPath: string): DocumentItem | null {
  if (!targetPath) return null;
  for (const item of items) {
    if (item.rel_path === targetPath) return item;
    if (item.is_dir && item.children && targetPath.startsWith(item.rel_path + '/')) {
      const found = findItemByPath(item.children, targetPath);
      if (found) return found;
    }
  }
  return null;
}

// Get directory children for current explorerPath
const explorerCurrentItems = computed<DocumentItem[]>(() => {
  if (!explorerPath.value) {
    return explorerStore.projectDocuments;
  }
  const folderItem = findItemByPath(explorerStore.projectDocuments, explorerPath.value);
  if (folderItem && folderItem.is_dir && folderItem.children) {
    return folderItem.children;
  }
  return [];
});

// Filtered Explorer items
const filteredExplorerItems = computed<DocumentItem[]>(() => {
  let list = explorerCurrentItems.value;

  if (conversationFilterOnly.value) {
    list = list.filter(item => isItemTouchedInConversation(item));
  }

  if (docsFilterOnly.value) {
    list = list.filter(item => item.is_dir ? hasMdFilesInSubtree(item) : isMdFile(item));
  }

  const q = searchFilter.value.trim().toLowerCase();
  if (q) {
    list = list.filter(item => item.name.toLowerCase().includes(q) || item.rel_path.toLowerCase().includes(q));
  }

  return list;
});

// Parent folder path for `..` button
const parentPath = computed<string>(() => {
  if (!explorerPath.value) return '';
  const parts = explorerPath.value.split('/');
  parts.pop();
  return parts.join('/');
});

function navigateToParent() {
  explorerPath.value = parentPath.value;
}

function onSelectExplorerFolder(item: DocumentItem) {
  explorerPath.value = item.rel_path;
}

// ── MULTI-ITEM SELECTION & CLIPBOARD ─────────────────────────
const selectedItemPaths = ref<string[]>([]);
const lastSelectedIndex = ref<number | null>(null);
const showDeleteConfirmModal = ref(false);
const isOperatingFiles = ref(false);

function isItemSelected(relPath: string): boolean {
  return selectedItemPaths.value.includes(relPath);
}

function gitStatusTextClass(item: DocumentItem): string {
  switch (item.git_status) {
    case 'conflict':
      return 'text-[var(--status-error-color)]';
    case 'modified':
      return 'text-[var(--status-warn-color)]';
    case 'untracked':
    case 'added':
      return 'text-[var(--status-ok-color)]';
    case 'renamed':
      return 'text-[var(--status-info-color)]';
    default:
      return 'text-[var(--fg-color)]';
  }
}

function gitStatusBadge(item: DocumentItem): { code: string; label: string; colorClass: string } | null {
  switch (item.git_status) {
    case 'conflict':
      return { code: '!', label: 'Git conflict', colorClass: 'text-[var(--status-error-color)]' };
    case 'modified':
      return { code: 'M', label: 'Modified in Git', colorClass: 'text-[var(--status-warn-color)]' };
    case 'untracked':
      return { code: 'U', label: 'Untracked in Git', colorClass: 'text-[var(--status-ok-color)]' };
    case 'added':
      return { code: 'A', label: 'Added (staged) in Git', colorClass: 'text-[var(--status-ok-color)]' };
    case 'renamed':
      return { code: 'R', label: 'Renamed in Git', colorClass: 'text-[var(--status-info-color)]' };
    default:
      return null;
  }
}

function onExplorerItemClick(e: MouseEvent, item: DocumentItem, index: number) {
  closeAllPopups();
  if (e.ctrlKey || e.metaKey) {
    const idx = selectedItemPaths.value.indexOf(item.rel_path);
    if (idx !== -1) {
      selectedItemPaths.value.splice(idx, 1);
    } else {
      selectedItemPaths.value.push(item.rel_path);
    }
    lastSelectedIndex.value = index;
  } else if (e.shiftKey && lastSelectedIndex.value !== null) {
    const start = Math.min(lastSelectedIndex.value, index);
    const end = Math.max(lastSelectedIndex.value, index);
    const rangePaths = filteredExplorerItems.value.slice(start, end + 1).map(i => i.rel_path);
    const combined = new Set([...selectedItemPaths.value, ...rangePaths]);
    selectedItemPaths.value = Array.from(combined);
  } else {
    selectedItemPaths.value = [item.rel_path];
    lastSelectedIndex.value = index;
  }
}

function onExplorerItemDblClick(item: DocumentItem) {
  if (item.is_dir) {
    onSelectExplorerFolder(item);
  } else {
    onSelectExplorerFile(item);
  }
}

function selectAllVisibleItems() {
  selectedItemPaths.value = filteredExplorerItems.value.map(i => i.rel_path);
}

function clearSelection() {
  selectedItemPaths.value = [];
  lastSelectedIndex.value = null;
}

function copySelectedItems() {
  if (selectedItemPaths.value.length === 0) return;
  explorerStore.setExplorerClipboard([...selectedItemPaths.value], 'copy');
}

function cutSelectedItems() {
  if (selectedItemPaths.value.length === 0) return;
  explorerStore.setExplorerClipboard([...selectedItemPaths.value], 'cut');
}

async function confirmBatchDelete() {
  if (selectedItemPaths.value.length === 0 || !activeProject.value?.path) return;
  isOperatingFiles.value = true;
  try {
    await explorerStore.deleteProjectItems(activeProject.value.path, selectedItemPaths.value);
    selectedItemPaths.value = [];
    showDeleteConfirmModal.value = false;
  } catch (err) {
    console.error('Failed to delete items:', err);
  } finally {
    isOperatingFiles.value = false;
  }
}

// ── RIGHT-CLICK CONTEXT MENU ─────────────────────────────────
const showContextMenu = ref(false);
const contextMenuPos = ref({ x: 0, y: 0 });
const contextMenuTarget = ref<{
  item: DocumentItem | null;
  type: 'file' | 'dir' | 'background';
}>({ item: null, type: 'background' });
const contextExclusionStatus = ref<ProjectExclusionStatus | null>(null);

function openContextMenu(e: MouseEvent, item: DocumentItem | null, type: 'file' | 'dir' | 'background') {
  e.preventDefault();
  e.stopPropagation();
  contextMenuTarget.value = { item, type };
  contextExclusionStatus.value = null;

  if (item && !selectedItemPaths.value.includes(item.rel_path)) {
    selectedItemPaths.value = [item.rel_path];
  }

  const maxX = window.innerWidth - 210;
  const maxY = window.innerHeight - 280;
  contextMenuPos.value = {
    x: Math.max(10, Math.min(e.clientX, maxX)),
    y: Math.max(10, Math.min(e.clientY, maxY)),
  };
  showContextMenu.value = true;
  showFilterDropdown.value = false;

  if (item && activeProject.value?.path && (type === 'file' || type === 'dir')) {
    const reqPath = activeProject.value.path;
    const reqRel = item.rel_path;
    daemonCall('get_project_exclusion_status', {
      projectPath: reqPath,
      relPath: reqRel,
      isDir: item.is_dir,
    })
      .then((status) => {
        if (contextMenuTarget.value.item?.rel_path === reqRel) {
          contextExclusionStatus.value = status;
        }
      })
      .catch(() => {
        // Non-critical preview state
      });
  }
}

function closeContextMenu() {
  showContextMenu.value = false;
}

function closeAllPopups() {
  showContextMenu.value = false;
  showFilterDropdown.value = false;
  showTabsOverflowMenu.value = false;
  editorContextMenu.value.show = false;
  showDefinitionPopover.value = false;
}

// Context Menu Action Handlers
function handleCtxNewFolder() {
  closeContextMenu();
  const targetDir = contextMenuTarget.value.item?.is_dir
    ? contextMenuTarget.value.item.rel_path
    : (explorerPath.value || '');
  inlineCreating.value = { targetDir, type: 'folder' };
  inlineCreatingName.value = '';
}

function handleCtxNewFile() {
  closeContextMenu();
  const targetDir = contextMenuTarget.value.item?.is_dir
    ? contextMenuTarget.value.item.rel_path
    : (explorerPath.value || '');
  inlineCreating.value = { targetDir, type: 'file' };
  inlineCreatingName.value = '';
}

async function openFileInExternalEditor(relPath: string) {
  const pPath = activeProject.value?.path;
  if (!pPath || !relPath) return;
  try {
    await daemonCall('open_file_in_external_editor', {
      projectPath: pPath,
      relPath,
    });
  } catch (err: unknown) {
    console.error('[ProjectWorkspace] Failed to open in external editor:', err);
    const msg = err instanceof Error ? err.message : String(err);
    coreStore.addStatusLog(`[ERROR] Failed to open file in external editor: ${msg}`);
  }
}

function handleCtxOpenExternal() {
  const item = contextMenuTarget.value.item;
  closeContextMenu();
  if (!item || item.is_dir) return;
  openFileInExternalEditor(item.rel_path);
}

function handleCtxToggleReview() {
  const item = contextMenuTarget.value.item;
  closeContextMenu();
  if (!item || item.is_dir) return;
  const isAttached = explorerStore.attachedReviewFiles.some(f => f.rel_path === item.rel_path);
  if (isAttached) {
    explorerStore.detachFileFromChat(item.rel_path);
  } else {
    explorerStore.attachFileToChat({
      rel_path: item.rel_path,
      name: item.name,
      category: item.category,
      comment_count: 0,
    });
    const targetAgentId = dialogueStore.activeAgentId || layoutStore.floatingChatAgentId || 'system';
    layoutStore.openFloatingChat(targetAgentId, activeProject.value?.path);
  }
}

function handleCtxCopy() {
  closeContextMenu();
  const paths = selectedItemPaths.value.length > 0
    ? selectedItemPaths.value
    : (contextMenuTarget.value.item ? [contextMenuTarget.value.item.rel_path] : []);
  if (paths.length === 0) return;
  explorerStore.setExplorerClipboard([...paths], 'copy');
}

function handleCtxCut() {
  closeContextMenu();
  const paths = selectedItemPaths.value.length > 0
    ? selectedItemPaths.value
    : (contextMenuTarget.value.item ? [contextMenuTarget.value.item.rel_path] : []);
  if (paths.length === 0) return;
  explorerStore.setExplorerClipboard([...paths], 'cut');
}

async function handleCtxPaste() {
  closeContextMenu();
  const targetDir = contextMenuTarget.value.item?.is_dir
    ? contextMenuTarget.value.item.rel_path
    : (explorerPath.value || '');
  if (!explorerStore.explorerClipboard || !activeProject.value?.path) return;
  isOperatingFiles.value = true;
  try {
    if (explorerStore.explorerClipboard.operation === 'copy') {
      await explorerStore.copyProjectItems(activeProject.value.path, explorerStore.explorerClipboard.items, targetDir);
    } else {
      await explorerStore.moveProjectItems(activeProject.value.path, explorerStore.explorerClipboard.items, targetDir);
      explorerStore.clearExplorerClipboard();
    }
  } catch (err) {
    console.error('Failed to paste item:', err);
  } finally {
    isOperatingFiles.value = false;
  }
}

function handleCtxRename() {
  const item = contextMenuTarget.value.item;
  closeContextMenu();
  if (!item) return;
  const parentDir = item.rel_path.includes('/')
    ? item.rel_path.split('/').slice(0, -1).join('/')
    : '';
  inlineRenaming.value = {
    relPath: item.rel_path,
    oldName: item.name,
    parentDir,
    isDir: item.is_dir,
  };
  inlineRenamingName.value = item.name;
}

function handleCtxDelete() {
  const item = contextMenuTarget.value.item;
  closeContextMenu();
  if (!item) return;
  if (selectedItemPaths.value.length === 0 || !selectedItemPaths.value.includes(item.rel_path)) {
    selectedItemPaths.value = [item.rel_path];
  }
  showDeleteConfirmModal.value = true;
}

async function handleCtxToggleExclusion(target: 'gitignore' | 'semantic') {
  const item = contextMenuTarget.value.item;
  const projectPath = activeProject.value?.path;
  closeContextMenu();
  if (!item || !projectPath) return;
  try {
    await daemonCall('toggle_project_exclusion', {
      projectPath,
      relPath: item.rel_path,
      isDir: item.is_dir,
      target,
    });
    await explorerStore.fetchProjectDocuments(projectPath);
  } catch (err) {
    reportAsyncFailure('ProjectWorkspace.handleCtxToggleExclusion', err);
  }
}

// ── INLINE CREATION & INLINE RENAMING ────────────────────────
const inlineCreating = ref<{ targetDir: string; type: 'folder' | 'file' } | null>(null);
const inlineCreatingName = ref('');
const inlineRenaming = ref<{ relPath: string; oldName: string; parentDir: string; isDir: boolean } | null>(null);
const inlineRenamingName = ref('');

async function submitInlineCreate() {
  if (!inlineCreating.value || !activeProject.value?.path) {
    inlineCreating.value = null;
    return;
  }
  const name = inlineCreatingName.value.trim();
  if (!name) {
    inlineCreating.value = null;
    return;
  }
  const { targetDir, type } = inlineCreating.value;
  inlineCreating.value = null;
  const relPath = targetDir ? `${targetDir}/${name}` : name;
  isOperatingFiles.value = true;
  try {
    if (type === 'folder') {
      await explorerStore.createProjectFolder(activeProject.value.path, relPath);
    } else {
      await explorerStore.createProjectFile(activeProject.value.path, relPath);
      await explorerStore.selectProjectDocument(activeProject.value.path, relPath);
    }
  } catch (err) {
    console.error('Failed to create item inline:', err);
  } finally {
    isOperatingFiles.value = false;
  }
}

async function submitInlineRename() {
  if (!inlineRenaming.value || !activeProject.value?.path) {
    inlineRenaming.value = null;
    return;
  }
  const newName = inlineRenamingName.value.trim();
  const { relPath, oldName, parentDir } = inlineRenaming.value;
  inlineRenaming.value = null;
  if (!newName || newName === oldName) return;

  const newRelPath = parentDir ? `${parentDir}/${newName}` : newName;
  isOperatingFiles.value = true;
  try {
    await explorerStore.renameProjectItem(activeProject.value.path, relPath, newRelPath);
  } catch (err) {
    console.error('Failed to rename item inline:', err);
  } finally {
    isOperatingFiles.value = false;
  }
}

function cancelInline() {
  inlineCreating.value = null;
  inlineRenaming.value = null;
}

// ── IN-PLACE DOCUMENT EDITOR & DRAFT STATE ───────────────────
const isEditMode = ref(false);
const editBuffer = ref('');
const isSavingDocument = ref(false);
const saveSuccess = ref(false);
let saveSuccessTimer: ReturnType<typeof setTimeout> | null = null;

// Disk Base & Conflict state
const diskBaseContent = ref('');
const diskConflict = ref(false);
const diskContent = ref('');
const showDiffView = ref(false);

// Unsaved changes warning modal state
const showUnsavedModal = ref(false);
const pendingNavigation = ref<{
  type: 'mode' | 'file' | 'closeTab';
  targetMode?: 'read' | 'edit';
  targetItem?: DocumentItem;
  targetRelPath?: string;
} | null>(null);

// Per-tab edit mode persistence & line-wrapping toggle
const tabEditModes = ref<Record<string, boolean>>({});
const editorLineWrap = ref(true);
const docTabsScrollContainer = ref<HTMLElement | null>(null);

function handleDocTabsWheel(e: WheelEvent) {
  const el = docTabsScrollContainer.value;
  if (!el) return;
  const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
  if (delta !== 0) {
    el.scrollLeft += delta;
  }
}

function hasUnsavedDraftFor(relPath: string): boolean {
  if (displayContent.value?.relPath === relPath && isDirty.value) {
    return true;
  }
  return explorerStore.unsavedDrafts[relPath] !== undefined;
}

function onSelectOpenTab(relPath: string) {
  if (!activeProject.value?.path) return;
  if (displayContent.value?.relPath === relPath) return;
  void explorerStore.activateOpenTab(activeProject.value.path, relPath, activeProject.value.id);
}

function requestCloseTab(relPath: string) {
  if (hasUnsavedDraftFor(relPath)) {
    pendingNavigation.value = { type: 'closeTab', targetRelPath: relPath };
    showUnsavedModal.value = true;
    return;
  }
  delete tabEditModes.value[relPath];
  explorerStore.closeDocumentTab(relPath);
}

const isDirty = computed(() => {
  if (!displayContent.value?.relPath) return false;
  const original = diskBaseContent.value || displayContent.value.content || '';
  return editBuffer.value !== original;
});

// Sync editBuffer with store unsavedDrafts
watch(editBuffer, (newVal) => {
  if (displayContent.value?.relPath) {
    if (isDirty.value) {
      explorerStore.setUnsavedDraft(displayContent.value.relPath, newVal);
    } else {
      explorerStore.clearUnsavedDraft(displayContent.value.relPath);
    }
  }
});

function requestSwitchMode(mode: 'read' | 'edit') {
  const relPath = displayContent.value?.relPath;
  if (!relPath) return;
  if (mode === 'read' && isEditMode.value && isDirty.value) {
    pendingNavigation.value = { type: 'mode', targetMode: 'read' };
    showUnsavedModal.value = true;
    return;
  }
  if (mode === 'edit' && !isEditMode.value) {
    if (explorerStore.unsavedDrafts[relPath] !== undefined) {
      editBuffer.value = explorerStore.unsavedDrafts[relPath];
    } else {
      editBuffer.value = displayContent.value?.content || '';
    }
    isEditMode.value = true;
    tabEditModes.value[relPath] = true;
  } else if (mode === 'read') {
    isEditMode.value = false;
    tabEditModes.value[relPath] = false;
  }
}

function toggleEditMode() {
  if (!isEditMode.value) {
    requestSwitchMode('edit');
  } else {
    requestSwitchMode('read');
  }
}

function discardChanges() {
  if (displayContent.value?.relPath) {
    explorerStore.clearUnsavedDraft(displayContent.value.relPath);
  }
  editBuffer.value = diskBaseContent.value || displayContent.value?.content || '';
}

async function handleSaveDocument() {
  if (!activeProject.value?.path || !displayContent.value?.relPath || isSavingDocument.value) return;
  isSavingDocument.value = true;
  try {
    const savedDoc = await explorerStore.saveProjectDocument(
      activeProject.value.path,
      displayContent.value.relPath,
      editBuffer.value
    );
    if (savedDoc) {
      diskBaseContent.value = savedDoc.content;
      diskConflict.value = false;
    }
    saveSuccess.value = true;
    if (saveSuccessTimer) clearTimeout(saveSuccessTimer);
    saveSuccessTimer = setTimeout(() => {
      saveSuccess.value = false;
    }, 2000);
  } catch (err) {
    console.error('Failed to save document:', err);
  } finally {
    isSavingDocument.value = false;
  }
}

function onSelectExplorerFile(item: DocumentItem) {
  if (!activeProject.value?.path) return;
  if (displayContent.value?.relPath === item.rel_path) {
    return;
  }
  executeFileSelect(item);
}

function executeFileSelect(item: DocumentItem) {
  if (activeProject.value?.path) {
    void explorerStore.activateOpenTab(activeProject.value.path, item.rel_path, activeProject.value.id);
  }
}

function handleUnsavedModalSave() {
  const nav = pendingNavigation.value;
  if (nav?.type === 'closeTab' && nav.targetRelPath && nav.targetRelPath !== displayContent.value?.relPath && activeProject.value?.path) {
    const targetPath = nav.targetRelPath;
    const draft = explorerStore.unsavedDrafts[targetPath];
    if (draft !== undefined) {
      void explorerStore.saveProjectDocument(activeProject.value.path, targetPath, draft).then(() => {
        showUnsavedModal.value = false;
        proceedPendingNavigation();
      });
      return;
    }
  }
  handleSaveDocument().then(() => {
    showUnsavedModal.value = false;
    proceedPendingNavigation();
  });
}

function handleUnsavedModalKeepDraft() {
  showUnsavedModal.value = false;
  proceedPendingNavigation();
}

function handleUnsavedModalDiscard() {
  const targetRel = pendingNavigation.value?.type === 'closeTab'
    ? pendingNavigation.value.targetRelPath
    : displayContent.value?.relPath;
  if (targetRel) {
    explorerStore.clearUnsavedDraft(targetRel);
  }
  if (targetRel === displayContent.value?.relPath) {
    editBuffer.value = diskBaseContent.value;
  }
  showUnsavedModal.value = false;
  proceedPendingNavigation();
}

function cancelPendingNavigation() {
  showUnsavedModal.value = false;
  pendingNavigation.value = null;
}

function proceedPendingNavigation() {
  const nav = pendingNavigation.value;
  pendingNavigation.value = null;
  if (!nav) return;

  if (nav.type === 'mode') {
    const relPath = displayContent.value?.relPath;
    if (nav.targetMode === 'read') {
      isEditMode.value = false;
      if (relPath) tabEditModes.value[relPath] = false;
    } else if (nav.targetMode === 'edit') {
      isEditMode.value = true;
      if (relPath) tabEditModes.value[relPath] = true;
    }
  } else if (nav.type === 'file' && nav.targetItem) {
    executeFileSelect(nav.targetItem);
  } else if (nav.type === 'closeTab' && nav.targetRelPath) {
    delete tabEditModes.value[nav.targetRelPath];
    explorerStore.closeDocumentTab(nav.targetRelPath);
  }
}

// ── CONFLICT RESOLUTION & DIFF ──────────────────────────────
function acceptDiskVersion() {
  diskBaseContent.value = diskContent.value;
  editBuffer.value = diskContent.value;
  if (displayContent.value?.relPath) {
    explorerStore.clearUnsavedDraft(displayContent.value.relPath);
  }
  diskConflict.value = false;
  showDiffView.value = false;
}

async function overwriteDiskWithDraft() {
  if (activeProject.value?.path && displayContent.value?.relPath) {
    await handleSaveDocument();
  }
  diskConflict.value = false;
  showDiffView.value = false;
}

function dismissConflictWarning() {
  diskConflict.value = false;
}

function openDiffView() {
  showDiffView.value = true;
}

interface DiffLine {
  lineNoA?: number;
  lineNoB?: number;
  textA?: string;
  textB?: string;
  type: 'same' | 'modified' | 'added' | 'removed';
}

const diffLines = computed<DiffLine[]>(() => {
  const textA = diskContent.value || diskBaseContent.value || '';
  const textB = editBuffer.value || '';
  const linesA = textA.split('\n');
  const linesB = textB.split('\n');
  const result: DiffLine[] = [];

  let iA = 0;
  let iB = 0;
  while (iA < linesA.length || iB < linesB.length) {
    const lineA = linesA[iA];
    const lineB = linesB[iB];
    if (lineA === lineB) {
      result.push({ lineNoA: iA + 1, lineNoB: iB + 1, textA: lineA, textB: lineB, type: 'same' });
      iA++;
      iB++;
    } else if (lineA !== undefined && lineB !== undefined) {
      result.push({ lineNoA: iA + 1, lineNoB: iB + 1, textA: lineA, textB: lineB, type: 'modified' });
      iA++;
      iB++;
    } else if (lineA !== undefined) {
      result.push({ lineNoA: iA + 1, lineNoB: undefined, textA: lineA, textB: '', type: 'removed' });
      iA++;
    } else {
      result.push({ lineNoA: undefined, lineNoB: iB + 1, textA: '', textB: lineB, type: 'added' });
      iB++;
    }
  }
  return result;
});

// ── ACTIVE DISPLAY CONTENT (Unification) ─────────────────────
const displayContent = computed(() => {
  if (explorerStore.selectedDocument) {
    return {
      title: explorerStore.selectedDocument.name,
      relPath: explorerStore.selectedDocument.rel_path,
      category: explorerStore.selectedDocument.category,
      type: explorerStore.selectedDocument.doc_type,
      lang: explorerStore.selectedDocument.doc_lang ?? undefined,
      content: explorerStore.selectedDocument.content,
      mtime: explorerStore.selectedDocument.mtime_seconds,
      size: explorerStore.selectedDocument.size_bytes,
    };
  }

  const artifact = dialogueStore.activeArtifact;
  if (artifact) {
    return {
      title: artifact.label,
      relPath: artifact.filePath,
      category: artifact.isEphemeral ? 'ephemeral' : (artifact.filePath?.startsWith('scratchpad') ? 'scratchpad' : 'docs'),
      type: artifact.type,
      lang: artifact.lang,
      content: artifact.content,
      mtime: artifact.timestamp,
      size: artifact.content.length,
    };
  }

  if (dialogueStore.activeConversationArtifacts.length > 0) {
    const latest = dialogueStore.activeConversationArtifacts[0];
    return {
      title: latest.label,
      relPath: latest.filePath,
      category: latest.isEphemeral
        ? 'ephemeral'
        : (latest.filePath?.startsWith('scratchpad') ? 'scratchpad' : 'docs'),
      type: latest.type,
      lang: latest.lang,
      content: latest.content,
      mtime: latest.timestamp,
      size: latest.content.length,
    };
  }

  return null;
});

watch(
  () => displayContent.value?.relPath,
  (newPath, oldPath) => {
    if (newPath !== oldPath) {
      if (!newPath) return;
      const initialDisk = displayContent.value?.content || '';
      diskBaseContent.value = initialDisk;
      diskConflict.value = false;
      showDiffView.value = false;
      isEditMode.value = Boolean(tabEditModes.value[newPath]);
      selectedInsightDoc.value = null;
      insightRangeContent.value = null;

      const draft = explorerStore.unsavedDrafts[newPath];
      if (draft !== undefined && draft !== initialDisk) {
        editBuffer.value = draft;
      } else {
        editBuffer.value = initialDisk;
      }

      if (showOutlineDrawer.value) {
        collapsedOutlineIds.value = {};
        void loadFileSymbols();
      } else {
        fileSymbols.value = [];
        collapsedOutlineIds.value = {};
      }
      if (showInsightsDrawer.value) {
        void loadLineInsights(1);
      } else {
        lineInsights.value = null;
      }

      if (pendingJumpLine.value !== null) {
        const targetLine = pendingJumpLine.value;
        pendingJumpLine.value = null;
        nextTick(() => {
          codeEditorRef.value?.scrollToLine(targetLine, isEditMode.value);
        });
      }
    }
  },
  { immediate: true }
);

watch(
  () => displayContent.value?.content,
  (newDiskContent) => {
    if (!newDiskContent) return;
    if (newDiskContent === diskBaseContent.value) return;

    if (!isDirty.value) {
      diskBaseContent.value = newDiskContent;
      editBuffer.value = newDiskContent;
      diskConflict.value = false;
    } else {
      diskContent.value = newDiskContent;
      diskConflict.value = true;
    }
  }
);

// Breadcrumbs: the open document wins; otherwise follow the explored folder.
const breadcrumbSegments = computed(() => {
  const openDoc = displayContent.value;
  const relPath = openDoc ? (openDoc.relPath ?? '') : explorerPath.value;
  const segments: Array<{ name: string; path: string; isDir: boolean }> = [
    { name: '.', path: '', isDir: true }
  ];
  if (!relPath) return segments;

  const parts = relPath.split('/');
  const folderParts = openDoc ? parts.slice(0, -1) : parts;
  let current = '';
  for (const p of folderParts) {
    current = current ? `${current}/${p}` : p;
    segments.push({
      name: p,
      path: current,
      isDir: true,
    });
  }
  return segments;
});

function onBreadcrumbClick(segment: { name: string; path: string; isDir: boolean }) {
  if (segment.isDir) {
    explorerPath.value = segment.path;
  }
}

// Copy content
const copied = ref(false);
let copyTimer: ReturnType<typeof setTimeout> | null = null;

async function copyContent(text?: string) {
  if (!text) return;
  try {
    await navigator.clipboard.writeText(text);
    copied.value = true;
    if (copyTimer) clearTimeout(copyTimer);
    copyTimer = setTimeout(() => { copied.value = false; }, 2000);
  } catch (err) {
    console.error('Failed to copy content:', err);
  }
}

// Attachment to Chat
const isCurrentDocAttached = computed(() => {
  if (!displayContent.value?.relPath) return false;
  return explorerStore.attachedReviewFiles.some(f => f.rel_path === displayContent.value?.relPath);
});

function toggleAttachCurrentDoc() {
  if (!displayContent.value?.relPath) return;
  const relPath = displayContent.value.relPath;
  if (isCurrentDocAttached.value) {
    explorerStore.detachFileFromChat(relPath);
  } else {
    explorerStore.attachFileToChat({
      rel_path: relPath,
      name: displayContent.value.title || relPath.split('/').pop() || 'file',
      category: displayContent.value.category,
      comment_count: explorerStore.activeDocumentOpenCommentCount,
    });
    const targetAgentId = dialogueStore.activeAgentId || layoutStore.floatingChatAgentId || 'system';
    layoutStore.openFloatingChat(targetAgentId, activeProject.value?.path);
  }
}

const {
  showCommentPopover,
  popoverPosition,
  selectedSnippet,
  selectedStartLine,
  selectedEndLine,
  newCommentText,
  isRecordingVoice,
  isSavingComment,
  showCommentsDrawer,
  contentContainerRef,
  handleTextSelection,
  openCommentAt,
  cancelComment,
  saveComment,
  toggleVoiceRecording,
  scrollToCommentLine,
} = useDocumentComments(activeProject, displayContent);

// ── LIGHTWEIGHT IDE: OUTLINE, GRAPH INSIGHTS, DEFINITION JUMP & AGENT CONTEXT MENU ──
const codeEditorRef = ref<InstanceType<typeof AppCodeEditor> | null>(null);
const cursorInfo = ref<EditorCursorInfo>({
  line: 1,
  col: 1,
  selectionStartLine: 1,
  selectionEndLine: 1,
  hasSelection: false,
});
const editorDiagnostics = ref<EditorDiagnosticsSummary>({
  errorCount: 0,
  warningCount: 0,
});
const editorStatusHint = ref<string | null>(null);
let editorStatusHintTimer: ReturnType<typeof setTimeout> | null = null;
const pendingJumpLine = ref<number | null>(null);

function setTransientEditorHint(message: string) {
  editorStatusHint.value = message;
  if (editorStatusHintTimer) clearTimeout(editorStatusHintTimer);
  editorStatusHintTimer = setTimeout(() => {
    editorStatusHint.value = null;
  }, 3500);
}

// Outline Drawer (Minimal Hierarchical Symbol / Heading Tree)
interface OutlineTreeNode {
  id: string;
  label: string;
  lineStart: number;
  lineEnd: number;
  children: OutlineTreeNode[];
}

interface OutlineFlatRow {
  node: OutlineTreeNode;
  depth: number;
  hasChildren: boolean;
  isCollapsed: boolean;
}

const showOutlineDrawer = ref(false);
const fileSymbols = ref<FileSymbolItem[]>([]);
const isLoadingSymbols = ref(false);
const outlineFilter = ref('');
const collapsedOutlineIds = ref<Record<string, boolean>>({});
let outlineDebounceTimer: ReturnType<typeof setTimeout> | null = null;

function toggleOutlineNode(nodeId: string) {
  collapsedOutlineIds.value[nodeId] = !collapsedOutlineIds.value[nodeId];
}

const outlineTree = computed<OutlineTreeNode[]>(() => {
  const items = fileSymbols.value;
  if (items.length === 0) return [];

  if (items[0].kind === 'section') {
    const roots: OutlineTreeNode[] = [];
    const stack: Array<{ level: number; node: OutlineTreeNode }> = [];
    items.forEach((item, idx) => {
      while (stack.length > 0 && stack[stack.length - 1].level >= item.level) {
        stack.pop();
      }
      const node: OutlineTreeNode = {
        id: `sec:${idx}:${item.lineStart}`,
        label: item.symbolName,
        lineStart: item.lineStart,
        lineEnd: item.lineEnd,
        children: [],
      };
      if (stack.length > 0) {
        stack[stack.length - 1].node.children.push(node);
      } else {
        roots.push(node);
      }
      stack.push({ level: item.level, node });
    });
    return roots;
  }

  const roots: OutlineTreeNode[] = [];
  items.forEach((item, idx) => {
    const parts = item.symbolName.split('::').filter(Boolean);
    if (parts.length === 0) return;

    let currentList = roots;
    for (let i = 0; i < parts.length - 1; i++) {
      const seg = parts[i];
      let parent = [...currentList]
        .reverse()
        .find(n => n.label === seg && item.lineStart >= n.lineStart && item.lineStart <= Math.max(n.lineEnd, item.lineStart));
      if (!parent) {
        parent = [...currentList].reverse().find(n => n.label === seg);
      }
      if (!parent) {
        parent = {
          id: `scope:${parts.slice(0, i + 1).join('::')}:${item.lineStart}`,
          label: seg,
          lineStart: item.lineStart,
          lineEnd: item.lineEnd,
          children: [],
        };
        currentList.push(parent);
      } else {
        parent.lineEnd = Math.max(parent.lineEnd, item.lineEnd);
      }
      currentList = parent.children;
    }

    const leafLabel = parts[parts.length - 1];
    const existing = currentList.find(n => n.label === leafLabel && Math.abs(n.lineStart - item.lineStart) <= 1);
    if (existing) {
      existing.lineEnd = Math.max(existing.lineEnd, item.lineEnd);
      return;
    }
    currentList.push({
      id: `sym:${idx}:${item.symbolName}:${item.lineStart}`,
      label: leafLabel,
      lineStart: item.lineStart,
      lineEnd: item.lineEnd,
      children: [],
    });
  });

  return roots;
});

const visibleOutlineRows = computed<OutlineFlatRow[]>(() => {
  const q = outlineFilter.value.trim().toLowerCase();
  const rows: OutlineFlatRow[] = [];

  function matchesSubtree(node: OutlineTreeNode): boolean {
    if (!q) return true;
    if (node.label.toLowerCase().includes(q)) return true;
    return node.children.some(matchesSubtree);
  }

  function walk(nodes: OutlineTreeNode[], depth: number) {
    for (const node of nodes) {
      if (q && !matchesSubtree(node)) continue;
      const hasChildren = node.children.length > 0;
      const isCollapsed = !q && Boolean(collapsedOutlineIds.value[node.id]);
      rows.push({ node, depth, hasChildren, isCollapsed });
      if (hasChildren && !isCollapsed) {
        walk(node.children, depth + 1);
      }
    }
  }

  walk(outlineTree.value, 0);
  return rows;
});

const activeOutlineNodeId = computed<string | null>(() => {
  const line = cursorInfo.value.line;
  let bestId: string | null = null;
  let bestSpan = Number.POSITIVE_INFINITY;

  function scan(nodes: OutlineTreeNode[]) {
    for (const node of nodes) {
      if (line >= node.lineStart && line <= node.lineEnd) {
        const span = node.lineEnd - node.lineStart;
        if (span <= bestSpan) {
          bestSpan = span;
          bestId = node.id;
        }
      }
      if (node.children.length > 0) {
        scan(node.children);
      }
    }
  }

  scan(outlineTree.value);
  return bestId;
});

async function loadFileSymbols() {
  const projectPath = activeProject.value?.path;
  const relPath = displayContent.value?.relPath;
  if (!projectPath || !relPath) {
    fileSymbols.value = [];
    return;
  }
  isLoadingSymbols.value = true;
  try {
    const content = isEditMode.value ? editBuffer.value : (displayContent.value?.content ?? null);
    fileSymbols.value = await daemonCall('get_file_symbols', {
      projectPath,
      relPath,
      content,
    });
  } catch (err) {
    reportAsyncFailure('ProjectWorkspace.loadFileSymbols', err);
    fileSymbols.value = [];
  } finally {
    isLoadingSymbols.value = false;
  }
}

function toggleOutlineDrawer() {
  showOutlineDrawer.value = !showOutlineDrawer.value;
  if (showOutlineDrawer.value) {
    showInsightsDrawer.value = false;
    showCommentsDrawer.value = false;
    void loadFileSymbols();
  }
}

watch(editBuffer, () => {
  if (!showOutlineDrawer.value || !isEditMode.value) return;
  if (outlineDebounceTimer) clearTimeout(outlineDebounceTimer);
  outlineDebounceTimer = setTimeout(() => {
    void loadFileSymbols();
  }, 500);
});

function splitSymbolScope(symbolName: string): { scope: string; name: string } {
  const sepIdx = symbolName.lastIndexOf('::');
  if (sepIdx === -1) {
    return { scope: '', name: symbolName };
  }
  return {
    scope: symbolName.slice(0, sepIdx),
    name: symbolName.slice(sepIdx + 2),
  };
}

// Causal Knowledge Insights Drawer (Deterministic Git hunks + Anchored Debt/TODOs + Related Specs)
const showInsightsDrawer = ref(false);
const lineInsights = ref<FileLineInsights | null>(null);
const isLoadingInsights = ref(false);
const isIndexingInsights = ref(false);
const selectedInsightDoc = ref<FileRelatedDoc | null>(null);
const selectedInsightMinute = ref<FileCausalMinute | null>(null);
const insightRangeContent = ref<FileLineRangeContent | null>(null);
const insightCommitDiff = ref<string | null>(null);
const isLoadingInsightDetail = ref(false);
let insightsDebounceTimer: ReturnType<typeof setTimeout> | null = null;

function closeInsightPreview() {
  selectedInsightDoc.value = null;
  selectedInsightMinute.value = null;
  insightRangeContent.value = null;
  insightCommitDiff.value = null;
}

async function openInsightDocPreview(doc: FileRelatedDoc) {
  selectedInsightMinute.value = null;
  insightCommitDiff.value = null;
  selectedInsightDoc.value = doc;
  insightRangeContent.value = null;
  const projectPath = activeProject.value?.path;
  if (!projectPath || !doc.filePath) {
    return;
  }
  isLoadingInsightDetail.value = true;
  try {
    insightRangeContent.value = await daemonCall('get_file_line_range_content', {
      projectPath,
      relPath: doc.filePath,
      lineStart: doc.lineStart ?? 1,
      lineEnd: doc.lineEnd ?? null,
    });
  } catch (err) {
    reportAsyncFailure('ProjectWorkspace.openInsightDocPreview', err);
    insightRangeContent.value = null;
  } finally {
    isLoadingInsightDetail.value = false;
  }
}

async function openMinuteDiffPreview(minute: FileCausalMinute) {
  selectedInsightDoc.value = null;
  insightRangeContent.value = null;
  selectedInsightMinute.value = minute;
  insightCommitDiff.value = null;
  const projectId = activeProject.value?.id;
  const relPath = displayContent.value?.relPath;
  if (!projectId || !minute.commitHash) {
    return;
  }
  isLoadingInsightDetail.value = true;
  try {
    insightCommitDiff.value = await daemonCall('get_git_commit_diff', {
      projectId,
      commitHash: minute.commitHash,
      filePath: relPath ?? null,
    });
  } catch (err) {
    reportAsyncFailure('ProjectWorkspace.openMinuteDiffPreview', err);
    insightCommitDiff.value = null;
  } finally {
    isLoadingInsightDetail.value = false;
  }
}

function formatAnchoredReason(item: FileAnchoredItem): string {
  switch (item.linkReason) {
    case 'line_anchor':
      return item.lineStart ? `L${item.lineStart}${item.lineEnd && item.lineEnd > item.lineStart ? `-L${item.lineEnd}` : ''}` : 'Line';
    case 'file_anchor':
      return 'File anchor';
    default:
      return 'Explicit ref';
  }
}

function formatDocReason(doc: FileRelatedDoc): string {
  if (doc.linkReason === 'via_minute') {
    return doc.viaMinuteTitle ? `Via minute · ${Math.round(doc.weight * 100)}%` : `Via minute`;
  }
  return 'References file';
}

async function loadLineInsights(targetLine?: number) {
  const projectId = activeProject.value?.id;
  const relPath = displayContent.value?.relPath;
  if (!projectId || !relPath) {
    lineInsights.value = null;
    return;
  }
  const line = targetLine ?? cursorInfo.value.line;
  isLoadingInsights.value = true;
  try {
    lineInsights.value = await daemonCall('get_file_line_insights', {
      projectId,
      relPath,
      line,
    });
  } catch (err) {
    reportAsyncFailure('ProjectWorkspace.loadLineInsights', err);
    lineInsights.value = null;
  } finally {
    isLoadingInsights.value = false;
  }
}

function toggleInsightsDrawer() {
  showInsightsDrawer.value = !showInsightsDrawer.value;
  if (showInsightsDrawer.value) {
    showOutlineDrawer.value = false;
    showCommentsDrawer.value = false;
    closeInsightPreview();
    void loadLineInsights();
  }
}

async function triggerProjectIndexForInsights() {
  const projectId = activeProject.value?.id;
  if (!projectId || isIndexingInsights.value) return;
  isIndexingInsights.value = true;
  try {
    await daemonCall('index_project_knowledge', { projectId, force: false });
    await loadLineInsights();
  } catch (err) {
    reportAsyncFailure('ProjectWorkspace.triggerProjectIndexForInsights', err);
  } finally {
    isIndexingInsights.value = false;
  }
}

function onEditorCursorChange(info: EditorCursorInfo) {
  const lineChanged = info.line !== cursorInfo.value.line;
  cursorInfo.value = info;
  if (lineChanged && showInsightsDrawer.value) {
    if (insightsDebounceTimer) clearTimeout(insightsDebounceTimer);
    insightsDebounceTimer = setTimeout(() => {
      void loadLineInsights(info.line);
    }, 350);
  }
}

function onEditorDiagnosticsChange(summary: EditorDiagnosticsSummary) {
  editorDiagnostics.value = summary;
}

async function openFileAtLine(filePath: string, line?: number | null) {
  if (!filePath || !activeProject.value?.path) return;
  if (
    filePath.startsWith('journal://') ||
    filePath === 'TODO.md' ||
    filePath === 'TECHNICAL_DEBT.md'
  ) {
    workspaceTab.value = 'devlog';
    return;
  }
  const targetLine = line && line > 0 ? line : 1;
  if (displayContent.value?.relPath === filePath) {
    jumpToEditorLine(targetLine);
    return;
  }
  pendingJumpLine.value = targetLine;
  await explorerStore.activateOpenTab(activeProject.value.path, filePath, activeProject.value.id);
}

function jumpToEditorLine(lineNumber: number) {
  if (codeEditorRef.value) {
    codeEditorRef.value.scrollToLine(lineNumber, isEditMode.value);
    return;
  }
  if (contentContainerRef.value) {
    const lineCount = displayContent.value?.content
      ? displayContent.value.content.split('\n').length
      : 100;
    const ratio = Math.min(1, Math.max(0, (lineNumber - 1) / Math.max(1, lineCount)));
    contentContainerRef.value.scrollTo({
      top: ratio * contentContainerRef.value.scrollHeight,
      behavior: 'smooth',
    });
  }
}

// Go to Definition (Tree-sitter indexed symbols)
const definitionCandidates = ref<SymbolDefinitionLocation[]>([]);
const showDefinitionPopover = ref(false);

async function handleGotoDefinition(payload: { symbol: string; line: number }) {
  const projectId = activeProject.value?.id;
  if (!projectId || !payload.symbol) return;
  try {
    const candidates = await daemonCall('resolve_symbol_definition', {
      projectId,
      symbolName: payload.symbol,
      currentRelPath: displayContent.value?.relPath ?? null,
    });
    if (candidates.length === 0) {
      setTransientEditorHint(`No definition indexed for '${payload.symbol}'`);
      return;
    }
    if (candidates.length === 1) {
      await openFileAtLine(candidates[0].filePath, candidates[0].lineStart);
      return;
    }
    definitionCandidates.value = candidates;
    showDefinitionPopover.value = true;
  } catch (err) {
    reportAsyncFailure('ProjectWorkspace.handleGotoDefinition', err);
  }
}

async function jumpToDefinitionCandidate(loc: SymbolDefinitionLocation) {
  showDefinitionPopover.value = false;
  await openFileAtLine(loc.filePath, loc.lineStart);
}

// Right-Click Editor Context Menu (Agent Actions, Comments, Definition, Insights)
const editorContextMenu = ref<{
  show: boolean;
  x: number;
  y: number;
  payload: EditorContextMenuPayload | null;
}>({
  show: false,
  x: 0,
  y: 0,
  payload: null,
});

function onCodeEditorContextMenu(payload: EditorContextMenuPayload) {
  closeAllPopups();
  editorContextMenu.value = {
    show: true,
    x: payload.x,
    y: payload.y,
    payload,
  };
}

function sendEditorContextToAgent(mode: 'ask' | 'refactor') {
  const payload = editorContextMenu.value.payload;
  const relPath = displayContent.value?.relPath;
  const projectPath = activeProject.value?.path;
  if (!payload || !relPath || !projectPath) return;

  const targetAgentId = dialogueStore.activeAgentId || layoutStore.floatingChatAgentId || 'system';
  explorerStore.attachFileToChat({
    rel_path: relPath,
    name: displayContent.value?.title || relPath.split('/').pop() || 'file',
    category: displayContent.value?.category,
    comment_count: explorerStore.activeDocumentOpenCommentCount,
  });

  const lineRef = payload.startLine === payload.endLine
    ? `L${payload.startLine}`
    : `L${payload.startLine}-L${payload.endLine}`;
  const langTag = displayContent.value?.lang || '';
  const snippetBlock = payload.selectedText
    ? `\n\`\`\`${langTag}\n${payload.selectedText}\n\`\`\``
    : (payload.wordAtCursor ? ` (symbol \`${payload.wordAtCursor}\`)` : '');

  const promptText = mode === 'refactor'
    ? `Refactor and improve \`${relPath}\` (${lineRef})${snippetBlock ? ':' + snippetBlock : ''}\n\nGoal: `
    : `Explain and analyze \`${relPath}\` (${lineRef})${snippetBlock ? ':' + snippetBlock : ''}\n\nQuestion: `;

  dialogueStore.setAgentDraft(targetAgentId, promptText, projectPath);
  layoutStore.openFloatingChat(targetAgentId, projectPath);
}

const editorContextMenuItems = computed<ContextMenuItem[]>(() => {
  const payload = editorContextMenu.value.payload;
  if (!payload) return [];

  const hasSelection = payload.selectedText.trim().length > 0;
  const targetLabel = hasSelection
    ? `L${payload.startLine}-L${payload.endLine}`
    : (payload.wordAtCursor ? `'${payload.wordAtCursor}'` : `L${payload.cursorLine}`);

  const items: ContextMenuItem[] = [
    {
      label: `Ask Agent about ${targetLabel}`,
      icon: Bot,
      action: () => sendEditorContextToAgent('ask'),
    },
    {
      label: `Refactor ${targetLabel} with Agent`,
      icon: Sparkles,
      action: () => sendEditorContextToAgent('refactor'),
    },
    { separator: true, label: '', action: () => {} },
    {
      label: `Add Review Comment (${targetLabel})`,
      icon: MessageSquare,
      action: () => {
        const snippet = payload.selectedText.trim() || payload.wordAtCursor || `Line ${payload.cursorLine}`;
        openCommentAt({
          snippet,
          startLine: payload.startLine,
          endLine: payload.endLine,
          x: payload.x,
          y: payload.y,
        });
      },
    },
    {
      label: `Graph Insights for L${payload.cursorLine}`,
      icon: Network,
      action: () => {
        showInsightsDrawer.value = true;
        showOutlineDrawer.value = false;
        showCommentsDrawer.value = false;
        void loadLineInsights(payload.cursorLine);
      },
    },
  ];

  if (payload.wordAtCursor) {
    items.push({
      label: `Go to Definition (${payload.wordAtCursor})`,
      icon: ArrowUpRight,
      action: () => {
        void handleGotoDefinition({ symbol: payload.wordAtCursor, line: payload.cursorLine });
      },
    });
  }

  if (hasSelection) {
    items.push({ separator: true, label: '', action: () => {} });
    items.push({
      label: 'Copy Selection',
      icon: Copy,
      action: () => {
        void copyContent(payload.selectedText);
      },
    });
  }

  return items;
});

// Markdown parser
marked.use({
  gfm: true,
  breaks: true,
  renderer: {
    code({ text, lang }: { text: string; lang?: string }) {
      const language = lang && hljs.getLanguage(lang) ? lang : '';
      try {
        const highlighted = language
          ? hljs.highlight(text, { language }).value
          : hljs.highlightAuto(text).value;
        return `<pre class="hljs font-mono text-[10px] p-2.5 my-2 border border-[var(--border-color)] bg-black/50 overflow-x-auto"><code class="hljs language-${language || 'plaintext'}">${highlighted}</code></pre>`;
      } catch {
        return `<pre class="hljs font-mono text-[10px] p-2.5 my-2 border border-[var(--border-color)] bg-black/50 overflow-x-auto"><code>${text}</code></pre>`;
      }
    },
  },
});

function renderMarkdown(content: string): string {
  try {
    return marked.parse(content) as string;
  } catch {
    return `<pre>${content}</pre>`;
  }
}

function typeIcon(type?: string, filename?: string) {
  const name = (filename || '').toLowerCase();
  if (name.endsWith('.vue') || name.endsWith('.rs') || name.endsWith('.ts') || name.endsWith('.js') || name.endsWith('.py') || name.endsWith('.go') || name.endsWith('.sh')) {
    return Code;
  }
  if (name.endsWith('.json') || name.endsWith('.toml') || name.endsWith('.yaml') || name.endsWith('.yml')) {
    return Braces;
  }
  if (name.endsWith('.md') || name.endsWith('.markdown')) {
    return FileText;
  }
  switch (type) {
    case 'code':     return Code;
    case 'diff':     return GitBranch;
    case 'json':     return Braces;
    case 'plan':     return LayoutList;
    case 'markdown': return FileText;
    default:         return FileText;
  }
}

watch(
  () => activeProject.value?.path,
  (newPath) => {
    if (newPath) {
      explorerStore.fetchProjectDocuments(newPath);
      journalStore.fetchSessionJournals(newPath);
      journalStore.fetchProjectTodos(newPath);
      journalStore.fetchProjectTechnicalDebt(newPath);
      explorerPath.value = '';
    } else {
      dialogueStore.clearProjectState();
      explorerPath.value = '';
    }
  },
  { immediate: true }
);

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    if (
      inlineCreating.value ||
      inlineRenaming.value ||
      showContextMenu.value ||
      showFilterDropdown.value ||
      showCommentPopover.value ||
      editorContextMenu.value.show ||
      showDefinitionPopover.value
    ) {
      e.preventDefault();
      e.stopPropagation();
      cancelInline();
      closeAllPopups();
      cancelComment();
    }
  }
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'e') {
    if (workspaceTab.value === 'files' && displayContent.value?.relPath) {
      e.preventDefault();
      toggleEditMode();
    }
  }
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
    if (isEditMode.value) {
      e.preventDefault();
      handleSaveDocument();
    }
  }
  if (e.key === 'F5') {
    e.preventDefault();
    if (workspaceTab.value === 'files') {
      void refreshExplorerFiles();
    } else {
      window.dispatchEvent(
        new CustomEvent('oneship:context-refresh', { detail: { tab: workspaceTab.value } }),
      );
    }
    return;
  }
  if (workspaceTab.value === 'files') {
    const targetEl = e.target as HTMLElement | null;
    const isEditingText =
      e.target instanceof HTMLInputElement ||
      e.target instanceof HTMLTextAreaElement ||
      Boolean(targetEl?.isContentEditable) ||
      Boolean(targetEl?.closest?.('.cm-editor'));
    if (!isEditingText) {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'a') {
        e.preventDefault();
        selectAllVisibleItems();
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'c') {
        if (selectedItemPaths.value.length > 0) {
          e.preventDefault();
          copySelectedItems();
        }
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'x') {
        if (selectedItemPaths.value.length > 0) {
          e.preventDefault();
          cutSelectedItems();
        }
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'v') {
        if (explorerStore.explorerClipboard) {
          e.preventDefault();
          handleCtxPaste();
        }
      } else if (e.key === 'Delete') {
        if (selectedItemPaths.value.length > 0) {
          e.preventDefault();
          showDeleteConfirmModal.value = true;
        }
      }
    }
  }
}

function handleSwitchToTerminal() {
  workspaceTab.value = 'terminal';
}

function handleOpenWorkspaceFile(detail: { filePath: string; line?: number }) {
  workspaceTab.value = 'files';
  if (detail?.filePath && activeProject.value?.path) {
    void openFileAtLine(detail.filePath, detail.line ?? null);
  }
}

function handleOpenWorkspaceDevlog() {
  workspaceTab.value = 'devlog';
}

const uiUnlisteners: (() => void)[] = [];
const isRefreshingExplorer = ref(false);
const tabsScrollContainer = ref<HTMLElement | null>(null);
const hasTabOverflow = ref(false);
const showTabsOverflowMenu = ref(false);
let tabsResizeObserver: ResizeObserver | null = null;

function updateTabOverflow() {
  const el = tabsScrollContainer.value;
  if (!el) {
    hasTabOverflow.value = false;
    return;
  }
  hasTabOverflow.value = el.scrollWidth > el.clientWidth + 2;
}

function handleTabsWheel(e: WheelEvent) {
  const el = tabsScrollContainer.value;
  if (!el) return;
  const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
  if (delta !== 0) {
    el.scrollLeft += delta;
  }
}

function selectOverflowTab(tabId: WorkspaceTab) {
  workspaceTab.value = tabId;
  showTabsOverflowMenu.value = false;
}

watch(workspaceTab, async (tabId) => {
  await nextTick();
  const btn = tabsScrollContainer.value?.querySelector<HTMLElement>(`[data-workspace-tab="${tabId}"]`);
  btn?.scrollIntoView({ behavior: 'smooth', inline: 'nearest', block: 'nearest' });
});

watch(
  () => visibleTabs.value.length,
  async () => {
    await nextTick();
    updateTabOverflow();
  }
);

async function refreshExplorerFiles() {
  if (!activeProject.value?.path || isRefreshingExplorer.value) return;
  isRefreshingExplorer.value = true;
  try {
    await explorerStore.fetchProjectDocuments(activeProject.value.path);
  } finally {
    isRefreshingExplorer.value = false;
  }
}

onMounted(async () => {
  window.addEventListener('keydown', handleKeydown);
  hardwarePreviewStore.initEventListeners();
  await nextTick();
  updateTabOverflow();
  if (tabsScrollContainer.value && typeof ResizeObserver !== 'undefined') {
    tabsResizeObserver = new ResizeObserver(updateTabOverflow);
    tabsResizeObserver.observe(tabsScrollContainer.value);
  }
  uiUnlisteners.push(
    onUiEvent('switch-to-workspace-terminal', handleSwitchToTerminal),
    onUiEvent('open-workspace-file', handleOpenWorkspaceFile),
    onUiEvent('open-workspace-devlog', handleOpenWorkspaceDevlog),
    onDaemonEvent('project-files-changed', (payload) => {
      if (payload.project_path === activeProject.value?.path) {
        void explorerStore.fetchProjectDocuments(payload.project_path);
      }
    }),
    onDaemonEvent('ui-surface-command', (payload) => {
      if (!activeProject.value || payload.projectId !== activeProject.value.id) return;
      if (payload.nodePath === 'files' && payload.actionId === 'open_file') {
        const params = payload.params as { path?: string } | null;
        if (params?.path) {
          void explorerStore.selectProjectDocument(activeProject.value.path, params.path, activeProject.value.id);
        }
      }
    })
  );
});

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown);
  tabsResizeObserver?.disconnect();
  tabsResizeObserver = null;
  uiUnlisteners.forEach(u => u());
  hardwarePreviewStore.disposeEventListeners();
  if (saveSuccessTimer) clearTimeout(saveSuccessTimer);
  if (copyTimer) clearTimeout(copyTimer);
  if (editorStatusHintTimer) clearTimeout(editorStatusHintTimer);
  if (outlineDebounceTimer) clearTimeout(outlineDebounceTimer);
  if (insightsDebounceTimer) clearTimeout(insightsDebounceTimer);
});
</script>

<template>
  <div 
    class="flex flex-1 h-full min-h-0 bg-[var(--bg-color)] border border-[var(--border-color)] font-mono overflow-hidden"
    @click="closeAllPopups"
  >
    
    <!-- ── 1. COMPACT VERTICAL PROJECT RAIL & DRAWER (Leftmost) ── -->
    <ProjectVerticalRail 
      v-show="!isPreviewFullscreen"
      :projects="projects"
      @add-project="emit('addProject')"
      @open-settings="navigateToProjectSettings($event)"
      @open-snapshots="workspaceTab = 'git'"
      @delete-project="emit('deleteProject', $event)"
    />

    <!-- ── 2. WORKSPACE STAGE (Main Content & Tabs) ─────────────── -->
    <div class="flex flex-col flex-1 min-w-0 min-h-0 h-full overflow-hidden relative">
      
      <!-- Workspace Sub-Tab Navigation Header -->
      <div class="flex items-center justify-between h-9 px-3 py-0 border-b border-[var(--border-color)] ui-surface-title shrink-0 gap-2 relative">
        <div
          ref="tabsScrollContainer"
          @wheel.prevent="handleTabsWheel"
          class="flex items-center gap-1 min-w-0 flex-1 overflow-x-auto scrollbar-none text-[10px] font-mono select-none"
        >
          <!-- Fixed, Non-Movable Settings Tab (Gear Icon Only, Left of Overview) -->
          <AppTooltip text="Project Settings" position="bottom">
            <button
              type="button"
              data-workspace-tab="settings"
              @click="workspaceTab = 'settings'"
              class="h-6 w-6 px-0 py-0 border transition-all flex items-center justify-center shrink-0 cursor-pointer"
              :class="workspaceTab === 'settings'
                ? 'border-[var(--accent-color)] bg-[var(--bg-color)] text-[var(--accent-color)]'
                : 'border-transparent text-[var(--dim-color)] hover:border-[var(--border-color)] hover:text-[var(--fg-color)]'"
            >
              <Settings :size="12" class="shrink-0" />
            </button>
          </AppTooltip>

          <!-- Tabs: Draggable & Reorderable (Order Persisted per Project) -->
          <VueDraggable
            v-model="reorderableTabs"
            :animation="150"
            ghost-class="opacity-50"
            class="flex items-center gap-1"
          >
            <button
              v-for="tab in reorderableTabs"
              :key="tab.id"
              :data-workspace-tab="tab.id"
              type="button"
              @click="workspaceTab = tab.id"
              class="h-6 px-2.5 py-0 border transition-all uppercase font-bold tracking-wider flex items-center gap-1 select-none whitespace-nowrap shrink-0 cursor-pointer"
              :class="workspaceTab === tab.id
                ? 'border-[var(--accent-color)] bg-[var(--bg-color)] text-[var(--accent-color)]'
                : 'border-transparent text-[var(--dim-color)] hover:border-[var(--border-color)] hover:text-[var(--fg-color)]'"
            >
              <component :is="tab.icon" :size="11" class="shrink-0" />
              <span>{{ tab.label }}</span>
            </button>
          </VueDraggable>
        </div>

        <!-- Overflow Tabs Dropdown Button -->
        <div v-if="hasTabOverflow" class="relative shrink-0" @click.stop>
          <AppTooltip text="All workspace tabs" position="bottom">
            <button
              type="button"
              @click="showTabsOverflowMenu = !showTabsOverflowMenu"
              class="h-6 w-6 px-0 py-0 border transition-colors cursor-pointer flex items-center justify-center"
              :class="showTabsOverflowMenu
                ? 'border-[var(--accent-color)] bg-[var(--bg-color)] text-[var(--accent-color)]'
                : 'border-[var(--border-color)] bg-[var(--bg-color)]/60 text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)]'"
            >
              <ChevronDown :size="12" />
            </button>
          </AppTooltip>

          <div
            v-if="showTabsOverflowMenu"
            class="absolute right-0 top-full mt-1.5 w-52 bg-[var(--ui-bg-title)] border border-[var(--border-color)] shadow-2xl z-50 p-1 flex flex-col gap-0.5 font-mono text-[10px]"
          >
            <button
              v-for="tab in visibleTabs"
              :key="`overflow-${tab.id}`"
              type="button"
              @click="selectOverflowTab(tab.id)"
              class="px-2 py-1.5 text-left uppercase font-bold tracking-wider flex items-center gap-2 transition-colors cursor-pointer whitespace-nowrap"
              :class="workspaceTab === tab.id
                ? 'bg-[var(--accent-color)]/15 text-[var(--accent-color)]'
                : 'text-[var(--dim-color)] hover:bg-[var(--ui-bg-card)] hover:text-[var(--fg-color)]'"
            >
              <component :is="tab.icon" :size="11" class="shrink-0" />
              <span class="truncate">{{ tab.label }}</span>
            </button>
          </div>
        </div>
      </div>

      <!-- ── TAB CONTENT ────────────────────────────────────────── -->
      
      <!-- EMPTY STATE: NO TRACKED PROJECTS OR NO ACTIVE PROJECT -->
      <div v-if="!activeProject || projects.length === 0" class="flex-1 flex flex-col items-center justify-center p-8 text-center select-none bg-[var(--bg-color)]">
        <div 
          class="border border-dashed border-[var(--border-color)] hover:border-[var(--accent-color)] p-8 max-w-md w-full font-mono flex flex-col items-center gap-3 cursor-pointer transition-colors group" 
          @click="emit('addProject')"
        >
          <span class="text-3xl text-[var(--dim-color)] group-hover:text-[var(--accent-color)]">+</span>
          <div class="text-[var(--accent-color)] text-xs font-bold uppercase tracking-widest">
            Track New Project
          </div>
          <p class="text-[11px] text-[var(--dim-color)] group-hover:text-[var(--fg-color)] transition-colors">
            Select a local Git repository or directory to start managing agent personas, devlogs, and workspaces.
          </p>
        </div>
      </div>

      <!-- TAB -1: DEDICATED PROJECT SETTINGS (Fixed Gear Tab, Spec 37) -->
      <div
        v-else-if="workspaceTab === 'settings'"
        class="flex flex-1 min-h-0 overflow-hidden"
      >
        <ProjectConfigPanel
          :project="activeProject"
          @saved="emit('syncSkills', activeProject)"
          @sync-skills="emit('syncSkills', $event)"
          @init-git="emit('initGit', $event)"
        />
      </div>

      <!-- TAB 0: PROJECT OVERVIEW (Summary Widget, Git, Init Flags, Plugin Grid) -->
      <ProjectOverviewTab
        v-else-if="workspaceTab === 'overview'"
        :project="activeProject"
        :active-listening-project-id="activeListeningProjectId"
        :agent-state="agentStates?.[activeProject.id]"
        @init-project="emit('initProject', $event)"
        @sync-skills="emit('syncSkills', $event)"
        @open-github="emit('openGithub', $event)"
        @init-git="emit('initGit', $event)"
      />

      <!-- TAB 1: FILES & DOCS (Explorer + Reader/Editor) -->
      <div v-else-if="workspaceTab === 'files'" class="flex flex-1 min-h-0 h-full overflow-hidden">
        
        <!-- File Explorer Sidebar (Left) -->
        <div 
          v-if="!isExplorerCollapsed"
          class="w-64 sm:w-72 shrink-0 border-r border-[var(--border-color)] bg-[var(--ui-bg-card)]/30 flex flex-col min-h-0 h-full select-none relative"
          @contextmenu="openContextMenu($event, null, 'background')"
        >
          <!-- Explorer Toolbar: branch + collapse, then search + filters (Spec 13 §3.13) -->
          <div class="shrink-0 border-b border-[var(--border-color)] ui-surface-title">
            <div class="h-9 flex items-center gap-2 px-2 min-w-0">
              <GitBranchSelector
                v-if="activeProject?.has_git"
                :project="activeProject"
              />
              <AppTooltip text="Collapse File Explorer" class="ml-auto shrink-0">
                <button
                  type="button"
                  @click="toggleExplorerCollapsed"
                  class="h-6 w-6 flex items-center justify-center text-[var(--dim-color)] hover:text-[var(--accent-color)] transition-colors cursor-pointer"
                >
                  <PanelLeftClose :size="12" />
                </button>
              </AppTooltip>
            </div>

            <div class="flex items-center gap-1 px-2 pb-2">
              <!-- Search Filter -->
              <div class="ui-search-box flex-1 min-w-0 h-6 px-1.5">
                <Search :size="10" class="ui-search-box__icon" />
                <AppInput
                  v-model="searchFilter"
                  type="text"
                  placeholder="Filter files..."
                  class="!bg-transparent !border-none !h-auto !px-0 !py-0 text-[10px]"
                />
                <button
                  v-if="searchFilter"
                  type="button"
                  class="text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer shrink-0"
                  @click="searchFilter = ''"
                >
                  <X :size="10" />
                </button>
              </div>

              <!-- Filter Popover Menu Toggle -->
              <div class="relative shrink-0">
                <button
                  type="button"
                  @click.stop="toggleFilterDropdown"
                  class="h-6 w-6 flex items-center justify-center border transition-colors cursor-pointer relative"
                  :class="conversationFilterOnly || docsFilterOnly
                    ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/20 text-[var(--accent-color)] font-bold'
                    : 'border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)]'"
                >
                  <Filter :size="11" />
                  <span v-if="conversationFilterOnly || docsFilterOnly" class="absolute top-1 right-1 w-1.5 h-1.5 bg-[var(--accent-color)]" />
                </button>

                <!-- Filter Dropdown Popover Menu -->
                <div
                  v-if="showFilterDropdown"
                  class="absolute right-0 top-full mt-1 z-50 w-56 border border-[var(--border-color)] bg-[var(--ui-bg-card)] p-2 shadow-2xl flex flex-col gap-1.5 font-mono select-none"
                  @click.stop
                >
                  <div class="text-[8px] uppercase tracking-wider font-bold text-[var(--dim-color)] px-1 pb-1 border-b border-[var(--border-color)]/40">
                    View Filters
                  </div>
                  <AppToggle v-model="conversationFilterOnly" label="This Chat" />
                  <AppToggle v-model="docsFilterOnly" label="Documentation (.md)" />
                </div>
              </div>
            </div>
          </div>

          <!-- Explorer Files List -->
          <div
            class="flex-1 overflow-y-auto p-1 flex flex-col gap-0.5 select-none"
            @click="clearSelection"
          >
            <!-- Parent Directory `..` button -->
            <button
              v-if="explorerPath"
              @click.stop="navigateToParent"
              class="w-full flex items-center gap-1.5 px-2 py-1 text-[10px] font-bold text-[var(--accent-color)] hover:bg-[var(--accent-color)]/10 text-left border border-dashed border-[var(--border-color)]/60 hover:border-[var(--accent-color)] transition-colors cursor-pointer bg-white/5 mb-0.5"
            >
              <CornerLeftUp :size="12" />
              <span>..</span>
            </button>

            <!-- Inline Transient Item Creation Input (Folder or File) -->
            <div
              v-if="inlineCreating && inlineCreating.targetDir === (explorerPath || '')"
              class="w-full px-2 py-1 border border-[var(--accent-color)] bg-[var(--accent-color)]/10 flex items-center gap-1.5 font-mono mb-1"
              @click.stop
            >
              <component
                :is="inlineCreating.type === 'folder' ? Folder : FileText"
                :size="12"
                class="shrink-0 text-[var(--accent-color)]"
              />
              <AppInput
                v-focus
                v-model="inlineCreatingName"
                type="text"
                :placeholder="inlineCreating.type === 'folder' ? 'new-folder-name' : 'new-file.md'"
                class="!bg-transparent !border-0 !border-b !border-[var(--accent-color)] !h-auto !px-0 !py-0 text-[10px] font-bold"
                @keydown.enter="submitInlineCreate"
                @keydown.esc="cancelInline"
              />
            </div>

            <!-- Directory & File Rows -->
            <div 
              v-for="(item, idx) in filteredExplorerItems"
              :key="item.rel_path"
              class="w-full flex items-center justify-between gap-2 px-2 py-1 text-[10px] text-left transition-colors cursor-default group font-mono"
              :class="[
                isItemSelected(item.rel_path)
                  ? 'border border-[var(--accent-color)]/60 bg-[var(--accent-color)]/15 font-bold'
                  : 'border border-transparent hover:border-[var(--border-color)] hover:bg-[var(--ui-bg-card)]/60',
                item.in_gitignore
                  ? 'text-[var(--dim-color)] opacity-85'
                  : gitStatusTextClass(item)
              ]"
              @click.stop="clearSelection"
              @contextmenu.prevent.stop="openContextMenu($event, item, item.is_dir ? 'dir' : 'file')"
            >
              <!-- Left side: Clickable Icon & Title/Input -->
              <div class="flex items-center min-w-0 flex-1">
                <div
                  class="inline-flex items-center gap-1.5 min-w-0 max-w-full cursor-pointer"
                  @click.stop="onExplorerItemClick($event, item, idx)"
                  @dblclick.stop="onExplorerItemDblClick(item)"
                >
                  <component 
                    :is="item.is_dir ? Folder : typeIcon(item.doc_type, item.name)" 
                    :size="12" 
                    class="shrink-0"
                    :class="item.in_gitignore
                      ? 'text-[var(--dim-color)]'
                      : (item.is_dir 
                        ? (item.category === 'scratchpad' ? 'text-[var(--status-warn-color)]' : 'text-[var(--status-info-color)]') 
                        : (isItemSelected(item.rel_path) ? 'text-[var(--accent-color)]' : 'text-[var(--fg-color)]/80'))"
                  />
                  
                  <!-- Inline Rename Input OR Name -->
                  <template v-if="inlineRenaming && inlineRenaming.relPath === item.rel_path">
                    <AppInput
                      v-focus
                      v-model="inlineRenamingName"
                      type="text"
                      class="!bg-black/80 !border-[var(--accent-color)] !h-auto !px-1 !py-0.2 text-[10px] font-bold"
                      @click.stop
                      @keydown.enter="submitInlineRename"
                      @keydown.esc="cancelInline"
                    />
                  </template>
                  <template v-else>
                    <span class="truncate" :class="item.is_dir ? 'font-bold' : ''">
                      {{ item.name }}{{ item.is_dir ? '/' : '' }}
                    </span>
                  </template>

                  <!-- Open Comments Indicator -->
                  <AppTooltip
                    v-if="!item.is_dir && item.open_comment_count > 0"
                    :content="`${item.open_comment_count} open comment${item.open_comment_count === 1 ? '' : 's'}`"
                    class="inline-flex items-center shrink-0"
                  >
                    <MessageSquare :size="10" class="text-[var(--status-warn-color)] shrink-0" />
                  </AppTooltip>

                  <!-- Semantic Indexing Status Indicator (Excluded vs Indexed) -->
                  <AppTooltip
                    v-if="item.in_semantic_ignore"
                    content="Excluded from semantic indexing"
                    class="inline-flex items-center shrink-0"
                  >
                    <span class="relative inline-flex items-center justify-center shrink-0">
                      <Brain :size="11" class="text-[var(--dim-color)]" />
                      <span class="absolute w-3.5 h-[1.5px] bg-[var(--dim-color)] -rotate-45 pointer-events-none" />
                    </span>
                  </AppTooltip>
                  <AppTooltip
                    v-else-if="!item.is_dir && item.is_semantically_indexed"
                    content="Semantically indexed"
                    class="inline-flex items-center shrink-0"
                  >
                    <Brain :size="11" class="text-[var(--accent-color)]/70 shrink-0" />
                  </AppTooltip>

                  <!-- Modified Badge if in conversation -->
                  <span
                    v-if="isItemTouchedInConversation(item)"
                    class="px-1 py-0.2 border border-[var(--status-warn-border)] text-[var(--status-warn-color)] bg-[var(--status-warn-bg)] text-[7px] uppercase tracking-wider shrink-0"
                  >
                    MODIFIED
                  </span>

                  <!-- Unsaved Draft Badge -->
                  <span
                    v-if="!item.is_dir && explorerStore.unsavedDrafts[item.rel_path]"
                    class="px-1 py-0.2 border border-[var(--status-warn-border)] text-[var(--status-warn-color)] bg-[var(--status-warn-bg)] text-[7px] uppercase tracking-wider font-bold shrink-0"
                  >
                    • DRAFT
                  </span>
                </div>
              </div>

              <!-- Right side: Git Status + Count for Dir or Size for File -->
              <div class="flex items-center gap-1.5 shrink-0 text-[8px]">
                <AppTooltip
                  v-if="!item.in_gitignore && gitStatusBadge(item)"
                  :content="gitStatusBadge(item)!.label"
                  class="inline-flex items-center shrink-0"
                >
                  <span
                    v-if="!item.is_dir"
                    class="font-bold text-[9px] leading-none"
                    :class="gitStatusBadge(item)!.colorClass"
                  >
                    {{ gitStatusBadge(item)!.code }}
                  </span>
                  <span
                    v-else
                    class="w-1.5 h-1.5 rounded-full bg-current shrink-0"
                    :class="gitStatusBadge(item)!.colorClass"
                  />
                </AppTooltip>
                <span class="opacity-60" v-if="item.is_dir">{{ countFolderFiles(item) }} files</span>
                <span class="opacity-60" v-else>{{ formatBytes(item.size_bytes) }}</span>
                <ChevronRight :size="10" class="opacity-0 group-hover:opacity-100 transition-opacity text-[var(--accent-color)]" />
              </div>
            </div>

            <div v-if="filteredExplorerItems.length === 0 && !inlineCreating" class="p-4 text-center text-[10px] text-[var(--dim-color)] italic opacity-60">
              No files matching criteria. Right-click to create folder/file.
            </div>
          </div>
        </div>

        <!-- Document Reader / Editor Main Stage (Right) -->
        <div class="flex-1 flex flex-col min-w-0 min-h-0 h-full bg-[var(--bg-color)] overflow-hidden relative">
          
          <!-- Reader Topbar: Multi-File Tabs & Explorer Expand Toggle -->
          <div class="h-9 flex items-center justify-between px-2 border-b border-[var(--border-color)] ui-surface-title text-[10px] shrink-0 gap-1.5">
            <div class="flex items-center gap-1 min-w-0 flex-1 overflow-hidden">
              <!-- Expand File Explorer Button if collapsed -->
              <AppTooltip v-if="isExplorerCollapsed" text="Expand File Explorer">
                <button
                  @click="toggleExplorerCollapsed"
                  class="h-6 w-6 border border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] ui-surface-base transition-colors cursor-pointer shrink-0 flex items-center justify-center"
                >
                  <PanelLeft :size="11" />
                </button>
              </AppTooltip>

              <!-- Open File Tabs (Horizontally Scrollable) -->
              <div
                v-if="explorerStore.openDocuments.length > 0"
                ref="docTabsScrollContainer"
                class="flex items-center gap-1 min-w-0 flex-1 overflow-x-auto scrollbar-none select-none"
                @wheel.prevent="handleDocTabsWheel"
              >
                <div
                  v-for="doc in explorerStore.openDocuments"
                  :key="doc.rel_path"
                  class="h-6 px-2 border flex items-center gap-1.5 shrink-0 cursor-pointer transition-colors group"
                  :class="displayContent?.relPath === doc.rel_path
                    ? 'border-[var(--accent-color)] ui-surface-base text-[var(--fg-color)] font-bold'
                    : 'border-[var(--border-color)] ui-surface-list text-[var(--dim-color)] hover:text-[var(--fg-color)] hover:border-[var(--dim-color)]'"
                  @click="onSelectOpenTab(doc.rel_path)"
                  @mousedown.middle.prevent="requestCloseTab(doc.rel_path)"
                >
                  <AppTooltip :text="doc.rel_path" position="bottom">
                    <span class="max-w-[220px] truncate text-[10px]">{{ doc.rel_path }}</span>
                  </AppTooltip>
                  <span
                    v-if="hasUnsavedDraftFor(doc.rel_path)"
                    class="w-1.5 h-1.5 bg-[var(--status-warn-color)] shrink-0"
                  />
                  <button
                    type="button"
                    class="p-0.5 text-[var(--dim-color)] hover:text-[var(--status-error-color)] cursor-pointer shrink-0 opacity-70 group-hover:opacity-100"
                    @click.stop="requestCloseTab(doc.rel_path)"
                  >
                    <X :size="10" />
                  </button>
                </div>
              </div>

              <!-- Symbolic '.' Breadcrumb Fallback when no file tabs are open -->
              <div v-else class="flex items-center gap-1 min-w-0 flex-1 overflow-x-auto scrollbar-none px-1">
                <template v-for="(seg, idx) in breadcrumbSegments" :key="seg.path || '__root__'">
                  <span v-if="idx > 0" class="text-[var(--dim-color)] opacity-40">/</span>
                  <button
                    @click="onBreadcrumbClick(seg)"
                    class="transition-colors cursor-pointer truncate max-w-[140px]"
                    :class="!displayContent && idx === breadcrumbSegments.length - 1
                      ? 'font-bold text-[var(--fg-color)]'
                      : 'text-[var(--dim-color)] hover:text-[var(--accent-color)]'"
                  >
                    {{ seg.name }}
                  </button>
                </template>
                <template v-if="displayContent">
                  <span class="text-[var(--dim-color)] opacity-40">/</span>
                  <span class="font-bold text-[var(--fg-color)] truncate">
                    {{ displayContent.title }}
                  </span>
                </template>
              </div>
            </div>
          </div>

          <!-- ── EXTERNAL DISK CONFLICT WARNING BANNER ───────────────── -->
          <div v-if="diskConflict" class="px-3 py-1.5 bg-[var(--status-warn-bg)] border-b border-[var(--status-warn-border)] text-[var(--status-warn-color)] font-mono text-[9px] flex items-center justify-between gap-2 shrink-0">
            <div class="flex items-center gap-1.5 truncate">
              <AlertTriangle :size="12" class="text-[var(--status-warn-color)] shrink-0" />
              <span class="truncate">File changed on disk by an external process or agent.</span>
            </div>
            <div class="flex items-center gap-1.5 shrink-0">
              <button
                class="px-2 py-0.5 bg-[var(--status-warn-bg)] hover:bg-[var(--status-warn-color)]/30 border border-[var(--status-warn-border)] text-[var(--status-warn-color)] font-bold uppercase text-[8px] cursor-pointer"
                @click="openDiffView"
              >
                Compare (Diff)
              </button>
              <button
                class="px-2 py-0.5 bg-black/40 hover:bg-black/60 border border-[var(--border-color)] text-[var(--fg-color)] uppercase text-[8px] cursor-pointer"
                @click="acceptDiskVersion"
              >
                Reload Disk
              </button>
              <button
                class="px-2 py-0.5 bg-black/40 hover:bg-black/60 border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] uppercase text-[8px] cursor-pointer"
                @click="dismissConflictWarning"
              >
                Keep Draft
              </button>
            </div>
          </div>

          <!-- ── SYMBOL-ONLY CONTEXTUAL TOOLBAR ───────────────────── -->
          <div v-if="displayContent" class="px-3 py-1 border-b border-[var(--border-color)]/60 bg-black/20 flex items-center justify-between gap-2 shrink-0 font-mono">
            <!-- Left: Action Symbols -->
            <div class="flex items-center gap-1.5">
              
              <!-- READ MODE SYMBOL TOOLBAR -->
              <template v-if="!isEditMode">
                <!-- Group 1: File Actions -->
                <AppTooltip :text="isCurrentDocAttached ? 'Detach file from chat review' : 'Attach file & comments for chat review'">
                  <button
                    class="p-1 border text-[9px] font-mono cursor-pointer transition-colors flex items-center justify-center relative"
                    :class="isCurrentDocAttached
                      ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/20 text-[var(--accent-color)] font-bold'
                      : 'border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] ui-surface-base'"
                    @click="toggleAttachCurrentDoc"
                  >
                    <Paperclip :size="11" />
                  </button>
                </AppTooltip>

                <AppTooltip text="Copy file content to clipboard">
                  <button
                    class="p-1 border border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] ui-surface-base transition-colors cursor-pointer flex items-center justify-center"
                    @click="copyContent(displayContent.content)"
                  >
                    <Check v-if="copied" :size="11" class="text-[var(--status-ok-color)]" />
                    <Copy v-else :size="11" />
                  </button>
                </AppTooltip>

                <AppTooltip v-if="displayContent.relPath" text="Open in system editor">
                  <button
                    class="p-1 border border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] ui-surface-base transition-colors cursor-pointer flex items-center justify-center"
                    @click="openFileInExternalEditor(displayContent.relPath)"
                  >
                    <ExternalLink :size="11" />
                  </button>
                </AppTooltip>

                <span class="w-px h-4 bg-[var(--border-color)]/60 mx-0.5 shrink-0" />

                <!-- Group 2: Mutually Exclusive Drawer Toggles -->
                <AppTooltip text="Toggle comments drawer">
                  <button
                    class="p-1 border text-[9px] font-mono cursor-pointer transition-colors flex items-center justify-center relative"
                    :class="showCommentsDrawer
                      ? 'border-[var(--accent-color)] text-[var(--accent-color)] bg-[var(--accent-color)]/20 font-bold'
                      : 'border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] ui-surface-base'"
                    @click="showCommentsDrawer = !showCommentsDrawer; if (showCommentsDrawer) { showOutlineDrawer = false; showInsightsDrawer = false; }"
                  >
                    <MessageSquare :size="11" />
                    <span
                      v-if="explorerStore.activeDocumentComments.length > 0"
                      class="absolute -top-1 -right-1 px-1 py-0.1 bg-[var(--accent-color)] text-[var(--bg-color)] font-bold text-[7px] leading-none"
                    >
                      {{ explorerStore.activeDocumentComments.length }}
                    </span>
                  </button>
                </AppTooltip>

                <AppTooltip v-if="displayContent.relPath" text="Toggle symbol outline (Ctrl+Shift+O)">
                  <button
                    class="p-1 border text-[9px] font-mono cursor-pointer transition-colors flex items-center justify-center"
                    :class="showOutlineDrawer
                      ? 'border-[var(--accent-color)] text-[var(--accent-color)] bg-[var(--accent-color)]/20 font-bold'
                      : 'border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] ui-surface-base'"
                    @click="toggleOutlineDrawer"
                  >
                    <LayoutList :size="11" />
                  </button>
                </AppTooltip>

                <AppTooltip v-if="displayContent.relPath" text="Toggle Knowledge Graph insights for cursor line">
                  <button
                    class="p-1 border text-[9px] font-mono cursor-pointer transition-colors flex items-center justify-center"
                    :class="showInsightsDrawer
                      ? 'border-[var(--accent-color)] text-[var(--accent-color)] bg-[var(--accent-color)]/20 font-bold'
                      : 'border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] ui-surface-base'"
                    @click="toggleInsightsDrawer"
                  >
                    <Network :size="11" />
                  </button>
                </AppTooltip>

                <template v-if="displayContent.relPath">
                  <span class="w-px h-4 bg-[var(--border-color)]/60 mx-0.5 shrink-0" />

                  <!-- Group 3: View Options -->
                  <AppTooltip :text="editorLineWrap ? 'Line Wrap: ON (click for horizontal scroll)' : 'Line Wrap: OFF (horizontal scroll)'">
                    <button
                      class="p-1 border text-[9px] font-mono cursor-pointer transition-colors flex items-center justify-center"
                      :class="editorLineWrap
                        ? 'border-[var(--accent-color)] text-[var(--accent-color)] bg-[var(--accent-color)]/20 font-bold'
                        : 'border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] ui-surface-base'"
                      @click="editorLineWrap = !editorLineWrap"
                    >
                      <WrapText :size="11" />
                    </button>
                  </AppTooltip>
                </template>
              </template>

              <!-- EDIT MODE SYMBOL TOOLBAR -->
              <template v-else>
                <!-- Group 1: File Actions -->
                <AppTooltip text="Save changes to disk (Ctrl+S)">
                  <button
                    :disabled="isSavingDocument || !isDirty"
                    class="p-1 border text-[9px] font-mono cursor-pointer transition-colors flex items-center justify-center relative disabled:opacity-40 disabled:cursor-not-allowed"
                    :class="isDirty
                      ? 'border-[var(--status-ok-color)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)] font-bold animate-pulse'
                      : 'border-[var(--border-color)] text-[var(--dim-color)] ui-surface-base'"
                    @click="handleSaveDocument"
                  >
                    <Check v-if="saveSuccess" :size="11" class="text-[var(--status-ok-color)]" />
                    <Save v-else :size="11" />
                    <span v-if="isDirty" class="absolute -top-0.5 -right-0.5 w-1.5 h-1.5 rounded-full bg-[var(--status-warn-color)]" />
                  </button>
                </AppTooltip>

                <AppTooltip v-if="isDirty" text="Discard unsaved edits">
                  <button
                    class="p-1 border border-[var(--status-error-border)] text-[var(--status-error-color)] hover:bg-[var(--status-error-bg)] ui-surface-base text-[9px] cursor-pointer transition-colors flex items-center justify-center"
                    @click="discardChanges"
                  >
                    <RotateCcw :size="11" />
                  </button>
                </AppTooltip>

                <AppTooltip text="Copy edit buffer">
                  <button
                    class="p-1 border border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] ui-surface-base transition-colors cursor-pointer flex items-center justify-center"
                    @click="copyContent(editBuffer)"
                  >
                    <Check v-if="copied" :size="11" class="text-[var(--status-ok-color)]" />
                    <Copy v-else :size="11" />
                  </button>
                </AppTooltip>

                <span class="w-px h-4 bg-[var(--border-color)]/60 mx-0.5 shrink-0" />

                <!-- Group 2: Mutually Exclusive Drawer Toggles -->
                <AppTooltip text="Toggle comments drawer">
                  <button
                    class="p-1 border text-[9px] font-mono cursor-pointer transition-colors flex items-center justify-center relative"
                    :class="showCommentsDrawer
                      ? 'border-[var(--accent-color)] text-[var(--accent-color)] bg-[var(--accent-color)]/20 font-bold'
                      : 'border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] ui-surface-base'"
                    @click="showCommentsDrawer = !showCommentsDrawer; if (showCommentsDrawer) { showOutlineDrawer = false; showInsightsDrawer = false; }"
                  >
                    <MessageSquare :size="11" />
                    <span
                      v-if="explorerStore.activeDocumentComments.length > 0"
                      class="absolute -top-1 -right-1 px-1 py-0.1 bg-[var(--accent-color)] text-[var(--bg-color)] font-bold text-[7px] leading-none"
                    >
                      {{ explorerStore.activeDocumentComments.length }}
                    </span>
                  </button>
                </AppTooltip>

                <AppTooltip text="Toggle symbol outline (Ctrl+Shift+O)">
                  <button
                    class="p-1 border text-[9px] font-mono cursor-pointer transition-colors flex items-center justify-center"
                    :class="showOutlineDrawer
                      ? 'border-[var(--accent-color)] text-[var(--accent-color)] bg-[var(--accent-color)]/20 font-bold'
                      : 'border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] ui-surface-base'"
                    @click="toggleOutlineDrawer"
                  >
                    <LayoutList :size="11" />
                  </button>
                </AppTooltip>

                <template v-if="displayContent.relPath">
                  <span class="w-px h-4 bg-[var(--border-color)]/60 mx-0.5 shrink-0" />

                  <!-- Group 3: View Options -->
                  <AppTooltip :text="editorLineWrap ? 'Line Wrap: ON (click for horizontal scroll)' : 'Line Wrap: OFF (horizontal scroll)'">
                    <button
                      class="p-1 border text-[9px] font-mono cursor-pointer transition-colors flex items-center justify-center"
                      :class="editorLineWrap
                        ? 'border-[var(--accent-color)] text-[var(--accent-color)] bg-[var(--accent-color)]/20 font-bold'
                        : 'border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] ui-surface-base'"
                      @click="editorLineWrap = !editorLineWrap"
                    >
                      <WrapText :size="11" />
                    </button>
                  </AppTooltip>
                </template>
              </template>

            </div>

            <!-- Right: Read / Edit Mode Switcher -->
            <div v-if="displayContent.relPath" class="flex items-center border border-[var(--border-color)] p-0.5 ui-surface-base shrink-0">
              <AppTooltip text="View mode (Ctrl+E)">
                <button
                  class="px-2 py-0.5 text-[8px] uppercase tracking-wider font-mono font-bold cursor-pointer transition-colors flex items-center gap-1"
                  :class="!isEditMode
                    ? 'bg-[var(--accent-color)] text-[var(--bg-color)]'
                    : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
                  @click="requestSwitchMode('read')"
                >
                  <Eye :size="9" />
                  <span>Read</span>
                </button>
              </AppTooltip>

              <AppTooltip text="Edit directly in CodeMirror (Ctrl+E)">
                <button
                  class="px-2 py-0.5 text-[8px] uppercase tracking-wider font-mono font-bold cursor-pointer transition-colors flex items-center gap-1"
                  :class="isEditMode
                    ? 'bg-[var(--accent-color)] text-[var(--bg-color)]'
                    : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
                  @click="requestSwitchMode('edit')"
                >
                  <Edit3 :size="9" />
                  <span>Edit</span>
                </button>
              </AppTooltip>
            </div>
          </div>

          <!-- Document Body Viewport (Flex Row: Editor + Docked Side Drawers) -->
          <div class="flex-1 min-h-0 overflow-hidden relative flex flex-row">
            <div class="flex-1 min-w-0 min-h-0 overflow-hidden relative flex flex-col">
              <template v-if="!displayContent">
                <div class="h-full flex flex-col items-center justify-center gap-2 text-[var(--dim-color)] opacity-60">
                  <FileText :size="32" />
                  <p class="text-xs uppercase tracking-widest">Select a document or file to read</p>
                </div>
              </template>

              <!-- Edit Mode Editor (CodeMirror 6) -->
              <div v-else-if="isEditMode" class="flex-1 min-h-0 overflow-hidden relative ui-surface-base">
                <AppCodeEditor
                  ref="codeEditorRef"
                  v-model="editBuffer"
                  :language="displayContent.lang"
                  :file-path="displayContent.relPath"
                  :line-wrapping="editorLineWrap"
                  @save="handleSaveDocument"
                  @cursor-change="onEditorCursorChange"
                  @diagnostics-change="onEditorDiagnosticsChange"
                  @editor-contextmenu="onCodeEditorContextMenu"
                  @goto-definition="handleGotoDefinition"
                  @toggle-outline="toggleOutlineDrawer"
                />
              </div>

              <!-- Markdown Preview -->
              <div
                v-else-if="['markdown', 'plan', 'proposal'].includes(displayContent.type) || displayContent.relPath?.endsWith('.md') || displayContent.relPath?.endsWith('.markdown')"
                ref="contentContainerRef"
                class="flex-1 overflow-y-auto p-4 select-text relative"
                @mouseup="handleTextSelection"
              >
                <div 
                  class="markdown-body font-mono text-[11px] leading-relaxed text-[var(--fg-color)]"
                  v-html="renderMarkdown(displayContent.content)"
                />
              </div>

              <!-- Code / Raw Text View (CodeMirror Readonly) -->
              <div
                v-else
                ref="contentContainerRef"
                class="flex-1 min-h-0 h-full overflow-hidden relative ui-surface-base select-text"
              >
                <AppCodeEditor
                  ref="codeEditorRef"
                  :model-value="displayContent.content"
                  :language="displayContent.lang"
                  :file-path="displayContent.relPath"
                  :readonly="true"
                  :line-wrapping="editorLineWrap"
                  @cursor-change="onEditorCursorChange"
                  @diagnostics-change="onEditorDiagnosticsChange"
                  @editor-contextmenu="onCodeEditorContextMenu"
                  @goto-definition="handleGotoDefinition"
                  @toggle-outline="toggleOutlineDrawer"
                />
              </div>

              <!-- ── DEFINITION CANDIDATES PICKER ──────────────────── -->
              <div
                v-if="showDefinitionPopover && definitionCandidates.length > 0"
                class="absolute left-4 bottom-4 z-40 w-80 max-h-60 border border-[var(--accent-color)] ui-surface-title shadow-2xl flex flex-col font-mono text-[10px]"
              >
                <div class="px-2.5 py-1.5 border-b border-[var(--border-color)] flex items-center justify-between">
                  <span class="font-bold text-[var(--accent-color)] uppercase text-[9px]">
                    Definitions ({{ definitionCandidates.length }})
                  </span>
                  <button
                    class="text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer"
                    @click="showDefinitionPopover = false"
                  >
                    <X :size="12" />
                  </button>
                </div>
                <div class="overflow-y-auto p-1.5 flex flex-col gap-1 ui-surface-list">
                  <button
                    v-for="(loc, idx) in definitionCandidates"
                    :key="`${loc.filePath}:${loc.lineStart}:${idx}`"
                    type="button"
                    class="w-full text-left px-2 py-1.5 border border-[var(--border-color)] ui-surface-card hover:border-[var(--accent-color)] flex flex-col gap-0.5 cursor-pointer"
                    @click="jumpToDefinitionCandidate(loc)"
                  >
                    <div class="flex items-center justify-between gap-2">
                      <span class="font-bold text-[var(--fg-color)] break-words">{{ loc.symbolName }}</span>
                      <span class="text-[9px] text-[var(--accent-color)] shrink-0">L{{ loc.lineStart }}</span>
                    </div>
                    <span class="text-[8px] text-[var(--dim-color)] truncate">{{ loc.filePath }}</span>
                  </button>
                </div>
              </div>
            </div>

            <!-- ── DOCKED INLINE SYMBOL OUTLINE DRAWER ───────────── -->
            <div
              v-if="showOutlineDrawer"
              class="w-80 shrink-0 border-l border-[var(--border-color)] ui-surface-base flex flex-col h-full overflow-hidden text-[10px] font-mono"
            >
              <div class="h-9 px-3 border-b border-[var(--border-color)] flex items-center justify-between ui-surface-title shrink-0">
                <div class="flex items-center gap-1.5 min-w-0">
                  <LayoutList :size="11" class="text-[var(--accent-color)] shrink-0" />
                  <span class="font-bold text-[var(--fg-color)] uppercase text-[9px] truncate">
                    Outline ({{ fileSymbols.length }})
                  </span>
                </div>
                <div class="flex items-center gap-1">
                  <AppTooltip text="Refresh symbols">
                    <button
                      class="text-[var(--dim-color)] hover:text-[var(--accent-color)] cursor-pointer p-0.5"
                      @click="loadFileSymbols"
                    >
                      <RefreshCw :size="11" :class="isLoadingSymbols ? 'animate-spin' : ''" />
                    </button>
                  </AppTooltip>
                  <button
                    class="text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer p-0.5"
                    @click="showOutlineDrawer = false"
                  >
                    <X :size="12" />
                  </button>
                </div>
              </div>

              <div class="p-2 border-b border-[var(--border-color)] ui-surface-title shrink-0">
                <AppInput
                  v-model="outlineFilter"
                  placeholder="Filter symbols..."
                  compact
                />
              </div>

              <div class="flex-1 overflow-y-auto py-1 ui-surface-list select-none">
                <div v-if="isLoadingSymbols && fileSymbols.length === 0" class="p-4 text-center text-[var(--dim-color)] text-[9px]">
                  Parsing AST symbols...
                </div>
                <div v-else-if="visibleOutlineRows.length === 0" class="p-4 text-center text-[var(--dim-color)] text-[9px] italic">
                  No structural symbols found in this file.
                </div>
                <div
                  v-for="row in visibleOutlineRows"
                  :key="row.node.id"
                  class="flex items-center gap-1 pr-2.5 py-1 cursor-pointer transition-colors hover:bg-[var(--accent-color)]/10"
                  :class="row.node.id === activeOutlineNodeId
                    ? 'text-[var(--accent-color)] bg-[var(--accent-color)]/10 font-bold'
                    : 'text-[var(--fg-color)]'"
                  :style="{ paddingLeft: `${6 + row.depth * 12}px` }"
                  @click="jumpToEditorLine(row.node.lineStart)"
                >
                  <button
                    v-if="row.hasChildren"
                    type="button"
                    class="w-3.5 h-3.5 flex items-center justify-center text-[var(--dim-color)] hover:text-[var(--fg-color)] shrink-0 cursor-pointer"
                    @click.stop="toggleOutlineNode(row.node.id)"
                  >
                    <ChevronRight v-if="row.isCollapsed" :size="10" />
                    <ChevronDown v-else :size="10" />
                  </button>
                  <span v-else class="w-3.5 shrink-0" />

                  <span class="truncate flex-1 text-[10px]">{{ row.node.label }}</span>
                  <span class="text-[9px] text-[var(--dim-color)] opacity-60 shrink-0 tabular-nums">L{{ row.node.lineStart }}</span>
                </div>
              </div>
            </div>

            <!-- ── DOCKED INLINE KNOWLEDGE GRAPH INSIGHTS DRAWER ── -->
            <div
              v-if="showInsightsDrawer"
              class="w-80 shrink-0 border-l border-[var(--border-color)] ui-surface-base flex flex-col h-full overflow-hidden text-[10px] font-mono"
            >
              <!-- Detail Preview Header (Doc slice or Minute Git Commit Diff) -->
              <template v-if="selectedInsightDoc || selectedInsightMinute">
                <div class="h-9 px-2.5 border-b border-[var(--border-color)] flex items-center justify-between ui-surface-title shrink-0 gap-1.5">
                  <button
                    type="button"
                    class="h-6 px-1.5 border border-[var(--border-color)] ui-surface-base text-[var(--fg-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] flex items-center gap-1 text-[9px] cursor-pointer shrink-0"
                    @click="closeInsightPreview"
                  >
                    <ArrowLeft :size="10" />
                    <span>Back</span>
                  </button>
                  <span class="text-[9px] font-bold text-[var(--fg-color)] truncate flex-1">
                    {{ selectedInsightDoc ? selectedInsightDoc.title : selectedInsightMinute?.title }}
                  </span>
                  <AppTooltip v-if="selectedInsightDoc?.filePath" text="Open document in new editor tab">
                    <button
                      type="button"
                      class="h-6 px-1.5 border border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-[var(--bg-color)] flex items-center gap-1 text-[9px] font-bold cursor-pointer shrink-0 transition-colors"
                      @click="openFileAtLine(selectedInsightDoc.filePath, selectedInsightDoc.lineStart)"
                    >
                      <span>Open</span>
                      <ArrowUpRight :size="10" />
                    </button>
                  </AppTooltip>
                </div>

                <div class="px-2.5 py-1.5 border-b border-[var(--border-color)] ui-surface-card flex items-center justify-between gap-2 text-[8px] text-[var(--dim-color)] shrink-0">
                  <span class="truncate">
                    <template v-if="selectedInsightDoc">{{ selectedInsightDoc.filePath }}</template>
                    <template v-else-if="selectedInsightMinute">
                      {{ selectedInsightMinute.agentId }} · {{ selectedInsightMinute.commitHash ? `#${selectedInsightMinute.commitHash.slice(0, 7)}` : selectedInsightMinute.sourceKind }}
                    </template>
                  </span>
                  <span v-if="insightRangeContent" class="text-[var(--accent-color)] shrink-0">
                    L{{ insightRangeContent.lineStart }}-L{{ insightRangeContent.lineEnd }}
                  </span>
                  <span v-else-if="selectedInsightDoc?.lineStart" class="text-[var(--accent-color)] shrink-0">
                    L{{ selectedInsightDoc.lineStart }}
                  </span>
                  <span v-else-if="selectedInsightMinute && selectedInsightMinute.lineStart > 0" class="text-[var(--accent-color)] shrink-0">
                    L{{ selectedInsightMinute.lineStart }}-L{{ selectedInsightMinute.lineEnd }}
                  </span>
                </div>

                <div class="flex-1 overflow-y-auto p-2.5 ui-surface-list">
                  <div v-if="isLoadingInsightDetail" class="p-4 text-center text-[var(--dim-color)] text-[9px]">
                    Loading source content...
                  </div>
                  <template v-else-if="selectedInsightDoc">
                    <div
                      v-if="insightRangeContent && (insightRangeContent.docType === 'markdown' || insightRangeContent.relPath.endsWith('.md'))"
                      class="markdown-body font-mono text-[10px] leading-relaxed text-[var(--fg-color)] select-text"
                      v-html="renderMarkdown(insightRangeContent.content)"
                    />
                    <pre
                      v-else-if="insightRangeContent"
                      class="text-[10px] leading-relaxed text-[var(--fg-color)] whitespace-pre-wrap break-words select-text font-mono"
                    >{{ insightRangeContent.content }}</pre>
                    <pre
                      v-else
                      class="text-[10px] leading-relaxed text-[var(--fg-color)] whitespace-pre-wrap break-words select-text font-mono"
                    >{{ selectedInsightDoc.contentPreview || 'No preview text indexed for this document.' }}</pre>
                  </template>
                  <template v-else-if="selectedInsightMinute">
                    <div class="flex flex-col gap-2">
                      <div class="p-2 border border-[var(--border-color)] ui-surface-card space-y-1">
                        <div class="text-[8px] uppercase text-[var(--dim-color)]">Summary</div>
                        <p class="text-[10px] text-[var(--fg-color)] whitespace-pre-wrap break-words leading-relaxed select-text">
                          {{ selectedInsightMinute.summary }}
                        </p>
                      </div>
                      <div v-if="selectedInsightMinute.decisions" class="p-2 border border-[var(--border-color)] ui-surface-card space-y-1">
                        <div class="text-[8px] uppercase text-[var(--dim-color)]">Decisions</div>
                        <p class="text-[10px] text-[var(--fg-color)] whitespace-pre-wrap break-words leading-relaxed select-text">
                          {{ selectedInsightMinute.decisions }}
                        </p>
                      </div>
                      <div v-if="insightCommitDiff" class="border border-[var(--border-color)] ui-surface-card overflow-x-auto p-2">
                        <div class="text-[8px] uppercase text-[var(--accent-color)] mb-1">Commit Diff</div>
                        <pre class="text-[9px] leading-relaxed text-[var(--fg-color)] whitespace-pre font-mono select-text">{{ insightCommitDiff }}</pre>
                      </div>
                    </div>
                  </template>
                </div>
              </template>

              <!-- List Header (when browsing causal line context) -->
              <template v-else>
                <div class="h-9 px-3 border-b border-[var(--border-color)] flex items-center justify-between ui-surface-title shrink-0">
                  <div class="flex items-center gap-1.5 min-w-0">
                    <Network :size="11" class="text-[var(--accent-color)] shrink-0" />
                    <span class="font-bold text-[var(--fg-color)] uppercase text-[9px] truncate">
                      Causal Insights (L{{ cursorInfo.line }})
                    </span>
                  </div>
                  <div class="flex items-center gap-1">
                    <AppTooltip text="Refresh insights for current line">
                      <button
                        class="text-[var(--dim-color)] hover:text-[var(--accent-color)] cursor-pointer p-0.5"
                        @click="loadLineInsights()"
                      >
                        <RefreshCw :size="11" :class="isLoadingInsights ? 'animate-spin' : ''" />
                      </button>
                    </AppTooltip>
                    <button
                      class="text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer p-0.5"
                      @click="showInsightsDrawer = false"
                    >
                      <X :size="12" />
                    </button>
                  </div>
                </div>

                <div class="flex-1 overflow-y-auto p-2 flex flex-col gap-2.5 ui-surface-list">
                  <div v-if="isLoadingInsights && !lineInsights" class="p-4 text-center text-[var(--dim-color)] text-[9px]">
                    Resolving causal history & specs...
                  </div>

                  <template v-else-if="lineInsights">
                    <div v-if="!lineInsights.indexed" class="p-3 border border-[var(--border-color)] ui-surface-card flex flex-col gap-2">
                      <p class="text-[9px] text-[var(--dim-color)] leading-relaxed">
                        This project is not indexed yet. Index once to build causal links between minutes, debt, specs, and files.
                      </p>
                      <button
                        type="button"
                        :disabled="isIndexingInsights"
                        class="h-7 px-2.5 bg-[var(--accent-color)] text-[var(--bg-color)] font-bold uppercase text-[9px] cursor-pointer disabled:opacity-40"
                        @click="triggerProjectIndexForInsights"
                      >
                        {{ isIndexingInsights ? 'Indexing...' : 'Index Project Knowledge' }}
                      </button>
                    </div>

                    <template v-else>
                      <div class="p-2 border border-[var(--border-color)] ui-surface-card flex flex-col gap-1">
                        <div class="flex items-center justify-between text-[8px] uppercase text-[var(--dim-color)]">
                          <span>Active Scope</span>
                          <span v-if="lineInsights.lineStart && lineInsights.lineEnd" class="text-[var(--accent-color)]">
                            L{{ lineInsights.lineStart }}-L{{ lineInsights.lineEnd }}
                          </span>
                        </div>
                        <div class="text-[10px] font-bold text-[var(--fg-color)] break-words">
                          {{ lineInsights.symbolName ? splitSymbolScope(lineInsights.symbolName).name : 'File scope' }}
                        </div>
                        <div v-if="lineInsights.symbolName && splitSymbolScope(lineInsights.symbolName).scope" class="text-[8px] text-[var(--dim-color)] truncate">
                          {{ splitSymbolScope(lineInsights.symbolName).scope }}
                        </div>
                      </div>

                      <div
                        v-if="lineInsights.minutes.length === 0 && lineInsights.debtAndTodos.length === 0 && lineInsights.relatedDocs.length === 0"
                        class="p-4 text-center text-[var(--dim-color)] text-[9px] italic"
                      >
                        No minutes, debt, or specs linked to this file/line yet.
                      </div>

                      <!-- 1. Causal Session Minutes -->
                      <div v-if="lineInsights.minutes.length > 0" class="flex flex-col gap-1.5">
                        <div class="text-[8px] font-bold uppercase tracking-wider text-[var(--dim-color)] px-0.5">
                          Causal Minutes ({{ lineInsights.minutes.length }})
                        </div>
                        <button
                          v-for="m in lineInsights.minutes"
                          :key="`min-${m.journalId}-${m.lineStart}`"
                          type="button"
                          class="w-full text-left p-2 border border-[var(--border-color)] ui-surface-card hover:border-[var(--accent-color)] flex flex-col gap-1 cursor-pointer transition-colors"
                          @click="openMinuteDiffPreview(m)"
                        >
                          <div class="flex items-center justify-between gap-1 text-[8px] uppercase">
                            <span
                              class="px-1 py-0.5 border font-bold"
                              :class="m.exactLineMatch ? 'border-[var(--status-ok-border)] text-[var(--status-ok-color)]' : 'border-[var(--border-color)] text-[var(--dim-color)]'"
                            >
                              {{ m.exactLineMatch && m.lineStart > 0 ? `HUNK L${m.lineStart}-${m.lineEnd}` : (m.lineStart > 0 ? `L${m.lineStart}-${m.lineEnd}` : 'FILE') }}
                            </span>
                            <span v-if="m.commitHash" class="text-[var(--accent-color)] font-bold">
                              #{{ m.commitHash.slice(0, 7) }}
                            </span>
                            <span v-else class="text-[var(--dim-color)]">
                              {{ m.agentId }}
                            </span>
                          </div>
                          <div class="text-[10px] text-[var(--fg-color)] font-bold break-words leading-snug">
                            {{ m.title }}
                          </div>
                          <p class="text-[9px] text-[var(--dim-color)] line-clamp-2 leading-relaxed break-words border-l-2 border-[var(--border-color)] pl-1.5">
                            {{ m.decisions || m.summary }}
                          </p>
                        </button>
                      </div>

                      <!-- 2. Anchored Tech Debt & TODOs -->
                      <div v-if="lineInsights.debtAndTodos.length > 0" class="flex flex-col gap-1.5">
                        <div class="text-[8px] font-bold uppercase tracking-wider text-[var(--dim-color)] px-0.5">
                          Tech Debt & TODOs ({{ lineInsights.debtAndTodos.length }})
                        </div>
                        <div
                          v-for="item in lineInsights.debtAndTodos"
                          :key="`anchor-${item.entityType}-${item.entityId}`"
                          class="p-2 border border-[var(--border-color)] ui-surface-card flex flex-col gap-1"
                        >
                          <div class="flex items-center justify-between gap-1 text-[8px] uppercase">
                            <span class="px-1 py-0.5 border border-[var(--status-warn-border)] text-[var(--status-warn-color)] font-bold">
                              {{ item.entityType === 'tech_debt' ? 'DEBT' : 'TODO' }} · {{ item.severityOrPriority }}
                            </span>
                            <span class="text-[var(--dim-color)]">
                              {{ formatAnchoredReason(item) }} · {{ item.status }}
                            </span>
                          </div>
                          <div class="text-[10px] text-[var(--fg-color)] font-bold break-words leading-snug">
                            {{ item.title }}
                          </div>
                          <p v-if="item.details" class="text-[9px] text-[var(--dim-color)] line-clamp-2 leading-relaxed break-words">
                            {{ item.details }}
                          </p>
                        </div>
                      </div>

                      <!-- 3. Related Specs & Docs -->
                      <div v-if="lineInsights.relatedDocs.length > 0" class="flex flex-col gap-1.5">
                        <div class="text-[8px] font-bold uppercase tracking-wider text-[var(--dim-color)] px-0.5">
                          Related Specs & Docs ({{ lineInsights.relatedDocs.length }})
                        </div>
                        <button
                          v-for="doc in lineInsights.relatedDocs"
                          :key="doc.id"
                          type="button"
                          class="w-full text-left p-2 border border-[var(--border-color)] ui-surface-card hover:border-[var(--accent-color)] flex flex-col gap-1 cursor-pointer transition-colors"
                          @click="openInsightDocPreview(doc)"
                        >
                          <div class="flex items-center justify-between gap-1 text-[8px] uppercase">
                            <span class="px-1 py-0.5 border border-[var(--status-ok-border)] text-[var(--status-ok-color)] font-bold">
                              SPEC
                            </span>
                            <span class="text-[var(--dim-color)] truncate">
                              {{ formatDocReason(doc) }}
                            </span>
                          </div>
                          <div class="text-[10px] text-[var(--fg-color)] font-bold break-words leading-snug">
                            {{ doc.title }}
                          </div>
                          <p v-if="doc.contentPreview" class="text-[9px] text-[var(--dim-color)] line-clamp-2 leading-relaxed break-words border-l-2 border-[var(--border-color)] pl-1.5">
                            {{ doc.contentPreview }}
                          </p>
                          <div class="text-[8px] text-[var(--dim-color)] truncate flex items-center justify-between gap-1">
                            <span class="truncate">{{ doc.filePath }}</span>
                            <span v-if="doc.lineStart" class="text-[var(--accent-color)] shrink-0">L{{ doc.lineStart }}</span>
                          </div>
                        </button>
                      </div>
                    </template>
                  </template>
                </div>
              </template>
            </div>

            <!-- ── DOCKED INLINE COMMENTS DRAWER ─────────────────── -->
            <div
              v-if="showCommentsDrawer"
              class="w-80 shrink-0 border-l border-[var(--border-color)] ui-surface-base flex flex-col h-full overflow-hidden text-[10px] font-mono"
            >
              <!-- Drawer Header -->
              <div class="h-9 px-3 border-b border-[var(--border-color)] flex items-center justify-between ui-surface-title shrink-0">
                <div class="flex items-center gap-1.5 min-w-0">
                  <MessageSquare :size="11" class="text-[var(--accent-color)] shrink-0" />
                  <span class="font-bold text-[var(--fg-color)] uppercase text-[9px] truncate">
                    Comments ({{ explorerStore.activeDocumentComments.length }})
                  </span>
                </div>
                <button
                  class="text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer p-0.5"
                  @click="showCommentsDrawer = false"
                >
                  <X :size="12" />
                </button>
              </div>

              <!-- Comments List with Line Navigation -->
              <div class="flex-1 overflow-y-auto p-2 flex flex-col gap-2">
                <div v-if="explorerStore.activeDocumentComments.length === 0" class="p-4 text-center text-[var(--dim-color)] opacity-50 text-[9px] italic">
                  No comments on this document yet. Select text in reader to add review feedback.
                </div>

                <div
                  v-for="cmt in explorerStore.activeDocumentComments"
                  :key="cmt.id"
                  class="p-2 border flex flex-col gap-1.5 transition-colors cursor-pointer group"
                  :class="cmt.status === 'resolved'
                    ? 'border-[var(--border-color)]/40 bg-white/2 opacity-60'
                    : 'border-[var(--border-color)] hover:border-[var(--accent-color)] bg-black/40'"
                  @click="scrollToCommentLine(cmt)"
                >
                  <div class="flex items-center justify-between text-[8px] text-[var(--dim-color)]">
                    <span class="font-bold text-[var(--accent-color)] group-hover:underline">L{{ cmt.start_line }}-L{{ cmt.end_line }}</span>
                    <span
                      class="px-1 py-0.2 border uppercase text-[7px]"
                      :class="cmt.status === 'resolved'
                        ? 'border-[var(--status-ok-border)] text-[var(--status-ok-color)] bg-[var(--status-ok-bg)]'
                        : 'border-[var(--status-warn-border)] text-[var(--status-warn-color)] bg-[var(--status-warn-bg)]'"
                    >
                      {{ cmt.status }}
                    </span>
                  </div>

                  <div class="text-[8px] italic text-[var(--dim-color)] border-l-2 border-[var(--dim-color)]/50 pl-1.5 truncate">
                    "{{ cmt.selected_text }}"
                  </div>

                  <div class="text-[10px] text-[var(--fg-color)] whitespace-pre-wrap leading-relaxed">
                    {{ cmt.comment_text }}
                  </div>

                  <div class="flex items-center justify-between pt-1 border-t border-[var(--border-color)]/40 text-[8px]">
                    <span class="text-[var(--dim-color)] opacity-60">{{ formatDate(cmt.created_at) }}</span>
                    <div class="flex items-center gap-1" @click.stop>
                      <button
                        v-if="cmt.status === 'open'"
                        class="px-1.5 py-0.5 border border-[var(--status-ok-border)] hover:border-[var(--status-ok-color)] text-[var(--status-ok-color)] hover:bg-[var(--status-ok-bg)] text-[8px] uppercase tracking-wider cursor-pointer flex items-center gap-1"
                        @click="explorerStore.resolveDocumentComment(cmt.id)"
                      >
                        <CheckCheck :size="9" />
                        <span>Resolve</span>
                      </button>
                      <button
                        class="p-1 text-[var(--dim-color)] hover:text-[var(--status-error-color)] cursor-pointer transition-colors"
                        @click="explorerStore.deleteDocumentComment(cmt.id)"
                      >
                        <Trash2 :size="10" />
                      </button>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>

          <!-- ── FILE METADATA FOOTER BAR ────────────────────────── -->
          <div v-if="displayContent" class="px-3 py-1 border-t border-[var(--border-color)] ui-surface-title flex items-center justify-between text-[8px] text-[var(--dim-color)] shrink-0 font-mono select-none gap-2">
            <div class="flex items-center gap-3 min-w-0">
              <span>Size: <strong class="text-[var(--fg-color)]">{{ formatBytes(displayContent.size) }}</strong></span>
              <span v-if="displayContent.mtime">Modified: <strong class="text-[var(--fg-color)]">{{ formatDate(displayContent.mtime) }}</strong></span>
              <span>Format: <strong class="text-[var(--accent-color)] uppercase">{{ displayContent.lang || displayContent.type }}</strong></span>
              <span v-if="displayContent.relPath" class="text-[var(--fg-color)]">
                Ln {{ cursorInfo.line }}, Col {{ cursorInfo.col }}
              </span>
              <span
                v-if="displayContent.relPath && editorDiagnostics.errorCount > 0"
                class="px-1 py-0.5 border border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] font-bold"
              >
                {{ editorDiagnostics.errorCount }} SYNTAX ERR
              </span>
              <span v-if="editorStatusHint" class="text-[var(--status-warn-color)] truncate">
                {{ editorStatusHint }}
              </span>
            </div>

            <div class="flex items-center gap-2 shrink-0">
              <span v-if="explorerStore.activeDocumentOpenCommentCount > 0" class="text-[var(--status-warn-color)] font-bold">
                ● {{ explorerStore.activeDocumentOpenCommentCount }} Open Review Comments
              </span>
            </div>
          </div>

          <!-- Editor Right-Click Context Menu -->
          <AppContextMenu
            :show="editorContextMenu.show"
            :x="editorContextMenu.x"
            :y="editorContextMenu.y"
            :items="editorContextMenuItems"
            @close="editorContextMenu.show = false"
          />

          <!-- ── FLOATING SELECTION COMMENT POPOVER ───────────────────── -->
          <div
            v-if="showCommentPopover"
            class="fixed z-50 bg-[#121214] border border-[var(--accent-color)]/70 shadow-2xl p-3 flex flex-col gap-2 w-80 font-mono text-[10px]"
            :style="{ left: `${popoverPosition.x}px`, top: `${popoverPosition.y}px` }"
          >
            <div class="flex items-center justify-between border-b border-[var(--border-color)] pb-1 text-[9px] text-[var(--dim-color)]">
              <span class="font-bold text-[var(--accent-color)] uppercase">
                Comment: L{{ selectedStartLine }}-L{{ selectedEndLine }}
              </span>
              <button class="hover:text-[var(--fg-color)] cursor-pointer" @click="cancelComment">
                <X :size="12" />
              </button>
            </div>

            <div class="text-[8px] italic text-[var(--dim-color)] max-h-12 overflow-hidden text-ellipsis border-l-2 border-[var(--accent-color)] pl-2 line-clamp-2">
              "{{ selectedSnippet }}"
            </div>

            <AppTextarea
              v-model="newCommentText"
              :rows="2"
              placeholder="Write review note or dictate..."
              class="text-[10px] w-full"
            />

            <div class="flex items-center justify-between gap-2 pt-1">
              <AppTooltip v-if="coreStore.isOpendictateAvailable" text="Dictate review comment via OpenDictate">
                <button
                  class="px-2 py-1 border transition-colors cursor-pointer flex items-center gap-1 text-[9px]"
                  :class="isRecordingVoice
                    ? 'border-[var(--status-error-color)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] animate-pulse font-bold'
                    : 'border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)]'"
                  @click="toggleVoiceRecording"
                >
                  <Mic :size="11" />
                  <span>{{ isRecordingVoice ? 'Listening…' : 'Dictate' }}</span>
                </button>
              </AppTooltip>
              <span v-else />

              <div class="flex items-center gap-1.5">
                <button
                  class="px-2 py-1 border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] uppercase text-[9px] cursor-pointer"
                  @click="cancelComment"
                >
                  Cancel
                </button>
                <button
                  :disabled="!newCommentText.trim() || isSavingComment"
                  class="px-2.5 py-1 bg-[var(--accent-color)] text-[var(--bg-color)] font-bold uppercase text-[9px] cursor-pointer disabled:opacity-40"
                  @click="saveComment"
                >
                  {{ isSavingComment ? 'Saving…' : 'Save' }}
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- TAB 2: DEVLOG & TASKS (TODOs, Devlog, Technical Debt) -->
      <div v-else-if="workspaceTab === 'devlog'" class="flex-1 min-h-0 h-full overflow-hidden">
        <DevlogJournalView :project="activeProject" />
      </div>

      <!-- TAB 2B: PROJECT MANAGEMENT (Tasks/Forge Modes: Board, Schedule, Gantt, Spec 34) -->
      <ProjectManagementTab
        v-else-if="workspaceTab === 'project' && activeProject"
        :project="activeProject"
        @open-github="emit('openGithub', $event)"
      />

      <!-- TAB 2C: KNOWLEDGE GRAPH (Semantic Search Spotlight, AST Call Graph & Governance Edges, Spec 22 §5) -->
      <ProjectKnowledgeGraphTab
        v-else-if="workspaceTab === 'knowledge' && activeProject"
        :project="activeProject"
      />

      <!-- TAB 3: GIT & SNAPSHOTS (Cockpit: Branch, Agents Dispatch, Commits, Safety Snapshots & Live Diff) -->
      <ProjectGitSnapshotsTab
        v-else-if="workspaceTab === 'git' && activeProject?.has_git"
        :project="activeProject"
        :active-listening-project-id="activeListeningProjectId"
        :voice-mode="voiceMode"
      />

      <!-- TAB 4: TERMINALS (Multi-Terminal Split Grid) -->
      <div v-else-if="workspaceTab === 'terminal'" class="flex-1 min-h-0 h-full overflow-hidden">
        <ProjectTerminalGrid :project="activeProject" />
      </div>

      <!-- TAB 5: PROFILING (In-Vivo DuckDB Telemetry, Hotspots, Runs, Visual Flamegraph & Diffs) -->
      <ProjectProfilerTab
        v-else-if="workspaceTab === 'profiler'"
        :project="activeProject"
      />

      <!-- TAB 6: SKILLS (Project Skills & Marketplace Orchestrator) -->
      <ProjectSkillsTab
        v-else-if="workspaceTab === 'skills'"
        :project="activeProject"
        @skills-changed="activeProject?.path && explorerStore.fetchProjectDocuments(activeProject.path)"
      />

      <!-- TAB 7: RE LAB (Visual Reverse Engineering Explorer & Multi-Project FIFO Queue) -->
      <ProjectReaTab
        v-else-if="workspaceTab === 'rea'"
        :project="activeProject"
        @refresh-project="daemonCall('get_projects', {}).then((projs) => { coreStore.projects = projs; }).catch(reportAsyncFailure)"
      />

      <!-- TAB 8: ELECTRONICS & EMBEDDED LAB (Firmware, MCUs, Instrumentation, EDA) -->
      <ProjectElectronicsTab
        v-else-if="workspaceTab === 'electronics'"
        :project="activeProject"
      />

      <!-- TAB 9: DESIGN (Desigma Live Preview & Editor Launcher) -->
      <ProjectDesignTab
        v-else-if="workspaceTab === 'design' && activeProject"
        :project="activeProject"
      />

      <!-- TAB 9B: CAD (SolidCraft Snapshot Preview, Metrics & Editor Launcher) -->
      <ProjectCadTab
        v-else-if="workspaceTab === 'cad' && activeProject"
        :project="activeProject"
      />

      <!-- TAB 9C: PCB (PCBCraft Schematic/Board SVG Preview, BOM/Nets/DRC & Launcher) -->
      <ProjectPcbTab
        v-else-if="workspaceTab === 'pcb' && activeProject"
        :project="activeProject"
      />

      <!-- TAB 10: PREVIEW (Framework-Agnostic Web Preview) -->
      <ProjectPreviewTab
        v-else-if="workspaceTab === 'preview' && activeProject"
        :project="activeProject"
        @fullscreen-change="isPreviewFullscreen = $event"
      />

      <!-- Global bottom agent & task bar (aligned with the tab bar) -->
      <AgentTaskbar v-show="!isPreviewFullscreen" />
    </div>

    <!-- ── RIGHT-CLICK CONTEXT MENU ────────────────────────────── -->
    <div
      v-if="showContextMenu"
      class="fixed z-50 w-44 border border-[var(--border-color)] bg-[#0c0c0e] text-[var(--fg-color)] p-1 shadow-2xl font-mono text-[10px] select-none"
      :style="{ left: `${contextMenuPos.x}px`, top: `${contextMenuPos.y}px` }"
      @click.stop
    >
      <!-- Background Context Options -->
      <template v-if="contextMenuTarget.type === 'background'">
        <button class="w-full text-left px-2 py-1 hover:bg-[var(--accent-color)]/20 hover:text-[var(--accent-color)] flex items-center gap-1.5 cursor-pointer" @click="handleCtxNewFolder">
          <FolderPlus :size="10" />
          <span>New Folder</span>
        </button>
        <button class="w-full text-left px-2 py-1 hover:bg-[var(--accent-color)]/20 hover:text-[var(--accent-color)] flex items-center gap-1.5 cursor-pointer" @click="handleCtxNewFile">
          <FileText :size="10" />
          <span>New File</span>
        </button>
        <button v-if="explorerStore.explorerClipboard" class="w-full text-left px-2 py-1 hover:bg-[var(--accent-color)]/20 hover:text-[var(--accent-color)] flex items-center gap-1.5 cursor-pointer border-t border-[var(--border-color)]/40 mt-1 pt-1" @click="handleCtxPaste">
          <ClipboardCopy :size="10" />
          <span>Paste</span>
        </button>
      </template>

      <!-- Directory Context Options -->
      <template v-else-if="contextMenuTarget.type === 'dir'">
        <button class="w-full text-left px-2 py-1 hover:bg-[var(--accent-color)]/20 hover:text-[var(--accent-color)] flex items-center gap-1.5 cursor-pointer" @click="handleCtxNewFolder">
          <FolderPlus :size="10" />
          <span>New Folder</span>
        </button>
        <button class="w-full text-left px-2 py-1 hover:bg-[var(--accent-color)]/20 hover:text-[var(--accent-color)] flex items-center gap-1.5 cursor-pointer" @click="handleCtxNewFile">
          <FileText :size="10" />
          <span>New File</span>
        </button>
        <div class="my-1 border-t border-[var(--border-color)]/40" />
        <button class="w-full text-left px-2 py-1 hover:bg-[var(--accent-color)]/20 hover:text-[var(--accent-color)] flex items-center gap-1.5 cursor-pointer" @click="handleCtxCopy">
          <Copy :size="10" />
          <span>Copy</span>
        </button>
        <button class="w-full text-left px-2 py-1 hover:bg-[var(--accent-color)]/20 hover:text-[var(--accent-color)] flex items-center gap-1.5 cursor-pointer" @click="handleCtxCut">
          <Scissors :size="10" />
          <span>Cut</span>
        </button>
        <button v-if="explorerStore.explorerClipboard" class="w-full text-left px-2 py-1 hover:bg-[var(--accent-color)]/20 hover:text-[var(--accent-color)] flex items-center gap-1.5 cursor-pointer" @click="handleCtxPaste">
          <ClipboardCopy :size="10" />
          <span>Paste</span>
        </button>
        <div class="my-1 border-t border-[var(--border-color)]/40" />
        <button class="w-full text-left px-2 py-1 hover:bg-[var(--accent-color)]/20 hover:text-[var(--accent-color)] flex items-center gap-1.5 cursor-pointer" @click="handleCtxRename">
          <Edit3 :size="10" />
          <span>Rename</span>
        </button>
        <div class="my-1 border-t border-[var(--border-color)]/40" />
        <button
          v-if="contextExclusionStatus?.hasGit"
          class="w-full text-left px-2 py-1 hover:bg-[var(--accent-color)]/20 hover:text-[var(--accent-color)] flex items-center gap-1.5 cursor-pointer"
          @click="handleCtxToggleExclusion('gitignore')"
        >
          <GitBranch :size="10" />
          <span>{{ contextExclusionStatus?.inGitignore ? 'Remove from .gitignore' : 'Exclude in .gitignore' }}</span>
        </button>
        <button
          class="w-full text-left px-2 py-1 hover:bg-[var(--accent-color)]/20 hover:text-[var(--accent-color)] flex items-center gap-1.5 cursor-pointer"
          @click="handleCtxToggleExclusion('semantic')"
        >
          <Network :size="10" />
          <span>{{ contextExclusionStatus?.inSemanticIgnore ? 'Include in Semantic Index' : 'Exclude from Semantic Index' }}</span>
        </button>
        <div class="my-1 border-t border-[var(--border-color)]/40" />
        <button class="w-full text-left px-2 py-1 hover:bg-[var(--status-error-bg)] hover:text-[var(--status-error-color)] flex items-center gap-1.5 cursor-pointer text-[var(--status-error-color)]" @click="handleCtxDelete">
          <Trash2 :size="10" />
          <span>Delete</span>
        </button>
      </template>

      <!-- File Context Options -->
      <template v-else-if="contextMenuTarget.type === 'file'">
        <button class="w-full text-left px-2 py-1 hover:bg-[var(--accent-color)]/20 hover:text-[var(--accent-color)] flex items-center gap-1.5 cursor-pointer" @click="handleCtxOpenExternal">
          <ExternalLink :size="10" />
          <span>Open in External Editor</span>
        </button>
        <button class="w-full text-left px-2 py-1 hover:bg-[var(--accent-color)]/20 hover:text-[var(--accent-color)] flex items-center gap-1.5 cursor-pointer" @click="handleCtxToggleReview">
          <Paperclip :size="10" />
          <span>{{ explorerStore.attachedReviewFiles.some(f => f.rel_path === contextMenuTarget.item?.rel_path) ? 'Detach from Chat' : 'Attach for Review' }}</span>
        </button>
        <div class="my-1 border-t border-[var(--border-color)]/40" />
        <button class="w-full text-left px-2 py-1 hover:bg-[var(--accent-color)]/20 hover:text-[var(--accent-color)] flex items-center gap-1.5 cursor-pointer" @click="handleCtxCopy">
          <Copy :size="10" />
          <span>Copy</span>
        </button>
        <button class="w-full text-left px-2 py-1 hover:bg-[var(--accent-color)]/20 hover:text-[var(--accent-color)] flex items-center gap-1.5 cursor-pointer" @click="handleCtxCut">
          <Scissors :size="10" />
          <span>Cut</span>
        </button>
        <div class="my-1 border-t border-[var(--border-color)]/40" />
        <button class="w-full text-left px-2 py-1 hover:bg-[var(--accent-color)]/20 hover:text-[var(--accent-color)] flex items-center gap-1.5 cursor-pointer" @click="handleCtxRename">
          <Edit3 :size="10" />
          <span>Rename</span>
        </button>
        <div class="my-1 border-t border-[var(--border-color)]/40" />
        <button
          v-if="contextExclusionStatus?.hasGit"
          class="w-full text-left px-2 py-1 hover:bg-[var(--accent-color)]/20 hover:text-[var(--accent-color)] flex items-center gap-1.5 cursor-pointer"
          @click="handleCtxToggleExclusion('gitignore')"
        >
          <GitBranch :size="10" />
          <span>{{ contextExclusionStatus?.inGitignore ? 'Remove from .gitignore' : 'Exclude in .gitignore' }}</span>
        </button>
        <button
          class="w-full text-left px-2 py-1 hover:bg-[var(--accent-color)]/20 hover:text-[var(--accent-color)] flex items-center gap-1.5 cursor-pointer"
          @click="handleCtxToggleExclusion('semantic')"
        >
          <Network :size="10" />
          <span>{{ contextExclusionStatus?.inSemanticIgnore ? 'Include in Semantic Index' : 'Exclude from Semantic Index' }}</span>
        </button>
        <div class="my-1 border-t border-[var(--border-color)]/40" />
        <button class="w-full text-left px-2 py-1 hover:bg-[var(--status-error-bg)] hover:text-[var(--status-error-color)] flex items-center gap-1.5 cursor-pointer text-[var(--status-error-color)]" @click="handleCtxDelete">
          <Trash2 :size="10" />
          <span>Delete</span>
        </button>
      </template>
    </div>

    <!-- ── BATCH DELETE CONFIRMATION MODAL ─────────────────────── -->
    <div
      v-if="showDeleteConfirmModal"
      class="fixed inset-0 z-50 ui-overlay-backdrop flex items-center justify-center p-4 font-mono text-[10px]"
    >
      <div class="bg-[#121214] border border-[var(--status-error-border)] p-4 flex flex-col gap-3 w-80 shadow-2xl">
        <div class="flex items-center justify-between border-b border-[var(--status-error-border)] pb-1.5">
          <div class="flex items-center gap-1.5 font-bold text-[var(--status-error-color)] uppercase">
            <Trash :size="12" />
            <span>Delete {{ selectedItemPaths.length }} Item(s)?</span>
          </div>
          <button class="text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer" @click="showDeleteConfirmModal = false">
            <X :size="12" />
          </button>
        </div>

        <div class="text-[9px] text-[var(--fg-color)] max-h-24 overflow-y-auto flex flex-col gap-1 border border-[var(--border-color)]/50 p-2 bg-black/40">
          <span v-for="p in selectedItemPaths" :key="p" class="truncate font-mono text-[8px] text-[var(--dim-color)]">
            • {{ p }}
          </span>
        </div>

        <p class="text-[8px] text-[var(--status-error-color)]/80 italic">
          Warning: This permanently removes the selected items from disk.
        </p>

        <div class="flex items-center justify-end gap-2 pt-1">
          <button
            class="px-2.5 py-1 border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] uppercase cursor-pointer"
            @click="showDeleteConfirmModal = false"
          >
            Cancel
          </button>
          <button
            :disabled="isOperatingFiles"
            class="px-2.5 py-1 bg-[var(--status-error-color)] hover:opacity-90 text-black font-bold uppercase cursor-pointer disabled:opacity-40"
            @click="confirmBatchDelete"
          >
            {{ isOperatingFiles ? 'Deleting…' : 'Delete' }}
          </button>
        </div>
      </div>
    </div>

    <!-- ── UNSAVED CHANGES WARNING MODAL ───────────────────────── -->
    <div v-if="showUnsavedModal" class="fixed inset-0 z-50 ui-overlay-backdrop flex items-center justify-center p-4">
      <div class="bg-[var(--bg-color)] border border-[var(--accent-color)] p-4 max-w-sm w-full font-mono flex flex-col gap-3 shadow-2xl">
        <div class="flex items-center justify-between border-b border-[var(--border-color)] pb-2 text-[var(--accent-color)] font-bold text-[11px] uppercase tracking-wider">
          <div class="flex items-center gap-1.5">
            <AlertTriangle :size="13" />
            <span>Unsaved Changes</span>
          </div>
          <button class="text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer" @click="cancelPendingNavigation">
            <X :size="12" />
          </button>
        </div>
        <p class="text-[10px] text-[var(--fg-color)] leading-relaxed">
          You have unsaved changes in <span class="font-bold text-[var(--accent-color)]">'{{ pendingNavigation?.targetRelPath || displayContent?.title }}'</span>. What would you like to do?
        </p>
        <div class="flex flex-col gap-1.5 pt-1">
          <button
            class="w-full py-1.5 px-3 bg-[var(--accent-color)] hover:opacity-90 text-[var(--bg-color)] font-bold uppercase text-[9px] text-center cursor-pointer"
            @click="handleUnsavedModalSave"
          >
            Save & Continue
          </button>
          <button
            v-if="pendingNavigation?.type !== 'closeTab'"
            class="w-full py-1.5 px-3 bg-white/10 hover:bg-white/20 border border-[var(--border-color)] text-[var(--fg-color)] uppercase text-[9px] text-center cursor-pointer"
            @click="handleUnsavedModalKeepDraft"
          >
            Keep Draft (In Memory)
          </button>
          <button
            class="w-full py-1.5 px-3 bg-[var(--status-error-bg)] hover:bg-[var(--status-error-color)]/30 border border-[var(--status-error-border)] text-[var(--status-error-color)] uppercase text-[9px] text-center cursor-pointer"
            @click="handleUnsavedModalDiscard"
          >
            Discard Draft
          </button>
          <button
            class="w-full py-1.5 px-3 border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] uppercase text-[8px] text-center cursor-pointer"
            @click="cancelPendingNavigation"
          >
            Cancel
          </button>
        </div>
      </div>
    </div>

    <!-- ── SIDE-BY-SIDE DIFF COMPARISON OVERLAY ───────────────── -->
    <div v-if="showDiffView" class="absolute inset-0 z-50 bg-[var(--bg-color)] flex flex-col font-mono">
      <!-- Header Bar -->
      <div class="px-3 py-2 border-b border-[var(--border-color)] bg-black/60 flex items-center justify-between gap-2 shrink-0">
        <div class="flex items-center gap-2 text-[10px] font-bold text-[var(--accent-color)] uppercase tracking-wider">
          <GitBranch :size="13" />
          <span>Compare Disk vs Unsaved Local Draft</span>
        </div>
        <div class="flex items-center gap-1.5">
          <button
            class="px-2.5 py-1 bg-black/40 hover:bg-black/60 border border-[var(--border-color)] text-[var(--fg-color)] uppercase text-[8px] cursor-pointer"
            @click="acceptDiskVersion"
          >
            Accept Disk Version
          </button>
          <button
            class="px-2.5 py-1 bg-[var(--accent-color)] hover:opacity-90 text-[var(--bg-color)] font-bold uppercase text-[8px] cursor-pointer"
            @click="overwriteDiskWithDraft"
          >
            Save Draft to Disk
          </button>
          <button
            class="px-2 py-1 border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] uppercase text-[8px] cursor-pointer"
            @click="showDiffView = false"
          >
            Close Diff
          </button>
        </div>
      </div>

      <!-- Diff Columns Header -->
      <div class="grid grid-cols-2 border-b border-[var(--border-color)] text-[9px] font-bold uppercase bg-black/30 shrink-0">
        <div class="px-3 py-1 text-[var(--status-info-color)] border-r border-[var(--border-color)] flex items-center justify-between">
          <span>Disk Version (External / Agent)</span>
          <span class="text-[8px] text-[var(--dim-color)] font-normal">Read-only</span>
        </div>
        <div class="px-3 py-1 text-[var(--status-warn-color)] flex items-center justify-between">
          <span>Your Local Draft (Unsaved)</span>
          <span class="text-[8px] text-[var(--dim-color)] font-normal">Active Editor</span>
        </div>
      </div>

      <!-- Diff Columns Content -->
      <div class="flex-1 overflow-y-auto font-mono text-[10px] leading-relaxed select-text">
        <div
          v-for="diff in diffLines"
          :key="`${diff.lineNoA ?? 'x'}-${diff.lineNoB ?? 'x'}`"
          class="grid grid-cols-2 border-b border-white/5 font-mono min-h-[20px]"
          :class="{
            'bg-[var(--status-error-bg)] text-[var(--status-error-color)]': diff.type === 'removed',
            'bg-[var(--status-ok-bg)] text-[var(--status-ok-color)]': diff.type === 'added',
            'bg-[var(--status-warn-bg)] text-[var(--status-warn-color)]': diff.type === 'modified'
          }"
        >
          <!-- Disk Side -->
          <div class="px-2 py-0.5 border-r border-[var(--border-color)]/40 flex items-start gap-2 whitespace-pre-wrap break-all font-mono">
            <span class="w-7 shrink-0 text-right text-[8px] text-[var(--dim-color)] select-none">{{ diff.lineNoA ?? '' }}</span>
            <span class="flex-1">{{ diff.textA }}</span>
          </div>
          <!-- Draft Side -->
          <div class="px-2 py-0.5 flex items-start gap-2 whitespace-pre-wrap break-all font-mono">
            <span class="w-7 shrink-0 text-right text-[8px] text-[var(--dim-color)] select-none">{{ diff.lineNoB ?? '' }}</span>
            <span class="flex-1">{{ diff.textB }}</span>
          </div>
        </div>
      </div>
    </div>

  </div>
</template>

<style scoped>
/* Brutalist Markdown Styling */
:deep(.markdown-body h1) {
  font-size: 13px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--accent-color);
  border-bottom: 1px solid var(--border-color);
  padding-bottom: 4px;
  margin-top: 14px;
  margin-bottom: 8px;
}

:deep(.markdown-body h2) {
  font-size: 12px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--fg-color);
  border-bottom: 1px solid rgba(255, 255, 255, 0.1);
  padding-bottom: 3px;
  margin-top: 12px;
  margin-bottom: 6px;
}

:deep(.markdown-body h3) {
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  color: var(--fg-color);
  margin-top: 10px;
  margin-bottom: 4px;
}

:deep(.markdown-body p) {
  margin-bottom: 10px;
  line-height: 1.6;
  color: rgba(255, 255, 255, 0.88);
}

:deep(.markdown-body ul) {
  list-style-type: disc;
  padding-left: 18px;
  margin-bottom: 10px;
}

:deep(.markdown-body ol) {
  list-style-type: decimal;
  padding-left: 18px;
  margin-bottom: 10px;
}

:deep(.markdown-body li) {
  margin-bottom: 3px;
  line-height: 1.5;
}

:deep(.markdown-body code) {
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid rgba(255, 255, 255, 0.12);
  padding: 1px 4px;
  font-size: 10px;
  color: var(--accent-color);
}

:deep(.markdown-body pre) {
  background: rgba(0, 0, 0, 0.45);
  border: 1px solid var(--border-color);
  padding: 10px;
  margin-bottom: 12px;
  overflow-x: auto;
  line-height: 1.5;
}
</style>
