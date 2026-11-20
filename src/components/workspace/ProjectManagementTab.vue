<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted, nextTick } from 'vue';
import { VueDraggable } from 'vue-draggable-plus';
import {
  Kanban,
  CalendarClock,
  ChartGantt,
  Plus,
  Search,
  X,
  CheckSquare,
  ListTodo,
  BookOpen,
  GitBranch,
  GitFork,
  GitPullRequest,
  CircleDot,
  Tag,
  AlertCircle,
  GripVertical,
  PanelLeftClose,
  PanelLeftOpen,
  SlidersHorizontal,
  Trash2,
  Check,
  ZoomIn,
  ZoomOut,
  Flag,
} from '@lucide/vue';
import {
  AppTooltip,
  AppInput,
  AppSelect,
  AppToggle,
  AppSegmentedToggle,
  SubtabBar,
  type SegmentedItem,
  type SelectOption,
  type SubtabItem,
} from '../ui';
import {
  useProjectMgmtStore,
  type ProjectTaskStatus,
} from '../../stores/projectMgmtStore';
import { useForgeCollabStore } from '../../stores/forgeCollabStore';
import { useCoreStore } from '../../stores/coreStore';
import { useWorkspaceLayoutStore } from '../../stores/workspaceLayoutStore';
import { useJournalStore } from '../../stores/journalStore';
import { useMilestoneStore } from '../../stores/milestoneStore';
import { useUiSurfaceNode } from '../../composables/useUiSurfaceNode';
import { onDaemonEvent } from '../../utils/daemonEvents';
import { reportAsyncFailure } from '../../utils/reportAsyncFailure';
import type { ProjectRecord } from '../../types';
import type { ProjectTaskBundle } from '../../types/generated/ProjectTaskBundle';
import type { ProjectTaskStageRecord } from '../../types/generated/ProjectTaskStageRecord';
import ProjectTaskDetailPanel from './ProjectTaskDetailPanel.vue';
import MilestoneDetailPanel from './MilestoneDetailPanel.vue';
import ProjectMilestonesView from './ProjectMilestonesView.vue';
import ForgeCollabPanel from './ForgeCollabPanel.vue';
import ForgeTopologyTools from './ForgeTopologyTools.vue';
import CreateProjectTaskModal from '../modals/CreateProjectTaskModal.vue';
import CreateProjectMilestoneModal from '../modals/CreateProjectMilestoneModal.vue';

const props = defineProps<{
  project: ProjectRecord;
}>();

const emit = defineEmits<{
  (e: 'openGithub', project: ProjectRecord): void;
}>();

const store = useProjectMgmtStore();
const coreStore = useCoreStore();
const workspaceLayoutStore = useWorkspaceLayoutStore();
const journalStore = useJournalStore();
const milestoneStore = useMilestoneStore();
const forgeStore = useForgeCollabStore();

const milestoneFilterId = ref<number | null>(null);
const selectedMilestoneId = ref<number | null>(null);
const showCreateMilestoneModal = ref(false);

function openMilestoneDetail(id: number) {
  selectedTaskId.value = null;
  selectedMilestoneId.value = selectedMilestoneId.value === id ? null : id;
}

function toggleMilestoneFilter(id: number) {
  milestoneFilterId.value = milestoneFilterId.value === id ? null : id;
}

type TasksView = 'milestones' | 'board' | 'schedule' | 'gantt';
type ProjectMode = 'tasks' | 'forge';
type ForgeSubTab = 'issues' | 'prs' | 'releases';

const projectSurface = useUiSurfaceNode<'milestones' | 'board' | 'schedule' | 'gantt' | 'forge'>('project', 'board');
const activeSubView = computed(() => projectSurface.activeChildId.value);

/**
 * Forge represents the external collaboration flow (issues, pull requests, releases).
 * It is available on any git project. Without a remote it renders a link-repository
 * empty state instead of mounting the Forge panel and failing on unresolved data.
 */
const forgeModeAvailable = computed(() => props.project.has_git);
const hasForgeRemote = computed(() => Boolean(props.project.git_status?.has_remote));
const isForgeActive = computed(() => forgeModeAvailable.value && activeSubView.value === 'forge');

function readLastTasksNodeSlot(): TasksView {
  const slot = workspaceLayoutStore.projectSlotValue(props.project.id, 'project.last_tasks_node');
  if (slot === 'milestones' || slot === 'schedule' || slot === 'gantt' || slot === 'board') {
    return slot;
  }
  return 'board';
}

function selectTasksView(view: TasksView) {
  projectSurface.selectChild(view);
}

const tasksView = computed<TasksView>({
  get: () => {
    if (
      activeSubView.value === 'milestones' ||
      activeSubView.value === 'board' ||
      activeSubView.value === 'schedule' ||
      activeSubView.value === 'gantt'
    ) {
      return activeSubView.value;
    }
    return isForgeActive.value ? readLastTasksNodeSlot() : 'board';
  },
  set: (view) => selectTasksView(view),
});

const activeMode = computed<ProjectMode>({
  get: () => (isForgeActive.value ? 'forge' : 'tasks'),
  set: (mode) => {
    if (mode === 'forge') {
      if (!forgeModeAvailable.value) return;
      const currentTasks = tasksView.value;
      workspaceLayoutStore
        .setSlotValue('project.last_tasks_node', currentTasks, props.project.id)
        .catch(reportAsyncFailure);
      projectSurface.selectChild('forge');
      return;
    }
    selectTasksView(readLastTasksNodeSlot());
  },
});

const modeToggleItems = computed<SegmentedItem[]>(() => [
  { id: 'tasks', label: 'Tasks', icon: ListTodo },
  { id: 'forge', label: 'Forge', icon: GitFork, hidden: !forgeModeAvailable.value },
]);

const tasksViewItems: SubtabItem[] = [
  { id: 'milestones', label: 'Milestones', icon: Flag },
  { id: 'board', label: 'Board', icon: Kanban },
  { id: 'schedule', label: 'Schedule', icon: CalendarClock },
  { id: 'gantt', label: 'Gantt', icon: ChartGantt },
];

/**
 * Forge keeps its own view node (`project/forge`) so the mode bar and the panel
 * read the same selection: the bar renders Issues | Pull Requests | Releases and
 * `ForgeCollabPanel` renders the matching content without owning a bar.
 */
const forgeSurface = useUiSurfaceNode<ForgeSubTab>('project/forge', 'issues');
const forgeTab = computed(() => forgeSurface.activeChildId.value);
const forgeUnreadCounts = computed(() => forgeStore.unreadCountsByProjectId[props.project.id] ?? null);
const forgePrLabel = computed(() => forgeStore.collabContext?.pullRequestLabel ?? 'Pull Requests');

const forgeTabItems = computed<SubtabItem[]>(() => [
  {
    id: 'issues',
    label: forgeUnreadCounts.value && forgeUnreadCounts.value.issuesUnreadCount > 0
      ? `Issues (${forgeUnreadCounts.value.issuesUnreadCount})`
      : 'Issues',
    icon: CircleDot,
  },
  {
    id: 'prs',
    label: forgeUnreadCounts.value && forgeUnreadCounts.value.prsUnreadCount > 0
      ? `${forgePrLabel.value} (${forgeUnreadCounts.value.prsUnreadCount})`
      : forgePrLabel.value,
    icon: GitPullRequest,
  },
  { id: 'releases', label: 'Releases', icon: Tag },
]);

// Forge without a remote renders only the mode toggle above the link-repository
// empty state, so the contextual tabs are omitted instead of navigating nothing.
const surfaceItems = computed<SubtabItem[]>(() => {
  if (!isForgeActive.value) return tasksViewItems;
  return hasForgeRemote.value ? forgeTabItems.value : [];
});

const activeSurfaceItem = computed(() => (isForgeActive.value ? forgeTab.value : tasksView.value));

function selectSurfaceItem(id: string) {
  if (isForgeActive.value) {
    forgeSurface.selectChild(id as ForgeSubTab);
    return;
  }
  selectTasksView(id as TasksView);
}

const searchQuery = ref('');
const domainFilter = ref<string>('all');
const selectedTaskId = ref<number | null>(null);

// Task creation modal scoped to the clicked stage
const createModalStageSlug = ref<string | null>(null);

// Inline stage editor state
const editingStageId = ref<number | null>(null);
const editStageLabel = ref('');
const editStageTerminal = ref(false);
const deleteStageReassignId = ref<string>('');
const isSavingStage = ref(false);

// Inline "+ Add Stage" column builder at end of board
const showNewStageForm = ref(false);
const newStageLabel = ref('');
const newStageTerminal = ref(false);
const isCreatingStage = ref(false);

const domainFilterOptions: SelectOption[] = [
  { label: 'All Domains', value: 'all' },
  { label: 'Code', value: 'code' },
  { label: 'Hardware', value: 'hardware' },
  { label: 'Procurement', value: 'procurement' },
  { label: 'Design', value: 'design' },
  { label: 'Docs', value: 'docs' },
  { label: 'Client', value: 'client' },
  { label: 'Milestone', value: 'milestone' },
  { label: 'Ops', value: 'ops' },
];

const stageBySlug = computed<Record<string, ProjectTaskStageRecord>>(() => {
  const map: Record<string, ProjectTaskStageRecord> = {};
  for (const s of store.stages) {
    map[s.slug] = s;
  }
  return map;
});

const filteredTasks = computed(() => {
  const q = searchQuery.value.trim().toLowerCase();
  return store.tasks.filter(bundle => {
    if (milestoneFilterId.value !== null && bundle.task.milestone_id !== milestoneFilterId.value) {
      return false;
    }
    if (domainFilter.value !== 'all' && bundle.task.domain !== domainFilter.value) {
      return false;
    }
    if (q) {
      const hay = `${bundle.task.title} ${bundle.task.description ?? ''} ${bundle.task.branch_name ?? ''}`.toLowerCase();
      if (!hay.includes(q)) return false;
    }
    return true;
  });
});

function tasksForStage(slug: string): ProjectTaskBundle[] {
  return filteredTasks.value.filter(b => b.task.status === slug);
}

const scheduledTasks = computed(() => {
  return filteredTasks.value
    .filter(b => b.task.start_epoch_seconds !== null || b.task.due_epoch_seconds !== null)
    .slice()
    .sort((a, b) => {
      const aTime = a.task.due_epoch_seconds ?? a.task.start_epoch_seconds ?? Number.MAX_SAFE_INTEGER;
      const bTime = b.task.due_epoch_seconds ?? b.task.start_epoch_seconds ?? Number.MAX_SAFE_INTEGER;
      return aTime - bTime;
    });
});

// ── GANTT ────────────────────────────────────────────────────────────────────
// Frontend-only rendering of the scheduled tasks node (`project/gantt` / `project/schedule`).
// Positions are derived from `start_epoch_seconds` / `due_epoch_seconds` in UTC
// days so bar geometry never depends on the local timezone.

const MS_PER_DAY = 86_400_000;
const GANTT_LABEL_WIDTH_PX = 240;
const GANTT_MAX_SPAN_DAYS = 7_320; // 20 years: bounded layout, never an unbounded day loop
const MONTH_NAMES = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
const WEEKDAY_LABELS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];

function epochDayIndex(epochSeconds: number): number {
  return Math.floor((epochSeconds * 1000) / MS_PER_DAY);
}

/**
 * Maps the operator's local calendar date to the same UTC day scale used by the
 * date-only task fields (stored at 12:00 UTC). Without this, a task due "today"
 * would look overdue in the afternoon of a negative UTC offset.
 */
function currentLocalDayIndex(): number {
  const now = new Date();
  return Math.floor(Date.UTC(now.getFullYear(), now.getMonth(), now.getDate()) / MS_PER_DAY);
}

const ganttDayWidthPx = ref(24);
const ganttScrollRef = ref<HTMLElement | null>(null);
const ganttOverlayScope = ref<'current' | 'all' | string>('current');
const ganttGroupByMilestone = ref(false);

const otherPmProjects = computed(() =>
  coreStore.projects.filter(
    (p) => p.has_project_management_enabled && !p.is_archived && p.path !== props.project.path
  )
);

const ganttOverlayOptions = computed<SelectOption[]>(() => {
  const opts: SelectOption[] = [{ value: 'current', label: 'This Project' }];
  if (otherPmProjects.value.length > 0) {
    opts.push({ value: 'all', label: 'All PM Projects' });
    for (const p of otherPmProjects.value) {
      opts.push({ value: p.path, label: `+ ${p.name}` });
    }
  }
  return opts;
});

const activeOverlayPaths = computed<string[]>(() => {
  if (ganttOverlayScope.value === 'current') return [];
  if (ganttOverlayScope.value === 'all') return otherPmProjects.value.map((p) => p.path);
  return otherPmProjects.value.some((p) => p.path === ganttOverlayScope.value)
    ? [ganttOverlayScope.value]
    : [];
});

watch(
  [() => !isForgeActive.value && tasksView.value === 'gantt', activeOverlayPaths],
  ([isGantt, paths]) => {
    if (isGantt && paths.length > 0) {
      store.fetchOverlayProjectTasks(paths);
    }
  },
  { immediate: true }
);

function matchesTaskFilters(bundle: ProjectTaskBundle): boolean {
  if (milestoneFilterId.value !== null && bundle.task.milestone_id !== milestoneFilterId.value) return false;
  if (domainFilter.value !== 'all' && bundle.task.domain !== domainFilter.value) return false;
  const q = searchQuery.value.trim().toLowerCase();
  if (!q) return true;
  return (
    bundle.task.title.toLowerCase().includes(q) ||
    (bundle.task.description ?? '').toLowerCase().includes(q)
  );
}

interface GanttTaskEntry {
  bundle: ProjectTaskBundle;
  projectName: string;
  isExternal: boolean;
}

const ganttScheduledTasks = computed<GanttTaskEntry[]>(() => {
  const entries: GanttTaskEntry[] = filteredTasks.value
    .filter((b) => b.task.start_epoch_seconds !== null || b.task.due_epoch_seconds !== null)
    .map((bundle) => ({
      bundle,
      projectName: props.project.name,
      isExternal: false,
    }));

  for (const overlayPath of activeOverlayPaths.value) {
    const proj = otherPmProjects.value.find((p) => p.path === overlayPath);
    if (!proj) continue;
    const tasks = store.overlayTasksByProjectPath[overlayPath] ?? [];
    for (const bundle of tasks) {
      if (bundle.task.start_epoch_seconds === null && bundle.task.due_epoch_seconds === null) continue;
      if (!matchesTaskFilters(bundle)) continue;
      entries.push({
        bundle,
        projectName: proj.name,
        isExternal: true,
      });
    }
  }

  return entries;
});

const ganttTodayDay = ref(currentLocalDayIndex());

interface GanttBounds {
  minDay: number;
  maxDay: number;
  totalDays: number;
}

const ganttBounds = computed<GanttBounds | null>(() => {
  const entries = ganttScheduledTasks.value;
  if (entries.length === 0) return null;

  let found = false;
  let minDay = 0;
  let maxDay = 0;
  for (const { bundle } of entries) {
    const startSec = bundle.task.start_epoch_seconds ?? bundle.task.due_epoch_seconds;
    const dueSec = bundle.task.due_epoch_seconds ?? bundle.task.start_epoch_seconds;
    if (startSec === null || dueSec === null) continue;
    const startDay = epochDayIndex(Math.min(startSec, dueSec));
    const dueDay = epochDayIndex(Math.max(startSec, dueSec));
    if (!found) {
      found = true;
      minDay = startDay;
      maxDay = dueDay;
      continue;
    }
    if (startDay < minDay) minDay = startDay;
    if (dueDay > maxDay) maxDay = dueDay;
  }
  if (!found) return null;

  const today = ganttTodayDay.value;
  minDay = Math.min(minDay, today) - 2;
  maxDay = Math.max(maxDay, today) + 2;
  if (maxDay - minDay + 1 > GANTT_MAX_SPAN_DAYS) {
    maxDay = minDay + GANTT_MAX_SPAN_DAYS - 1;
  }
  return { minDay, maxDay, totalDays: maxDay - minDay + 1 };
});

interface GanttRow {
  rowKey: string;
  bundle: ProjectTaskBundle;
  projectName: string;
  isExternal: boolean;
  isMilestone: boolean;
  isOverdue: boolean;
  startDay: number;
  endDay: number;
  leftPx: number;
  widthPx: number;
}

const ganttRows = computed<GanttRow[]>(() => {
  const bounds = ganttBounds.value;
  if (!bounds) return [];
  const dayWidth = ganttDayWidthPx.value;
  const rows: GanttRow[] = [];

  for (const { bundle, projectName, isExternal } of ganttScheduledTasks.value) {
    const startSec = bundle.task.start_epoch_seconds ?? bundle.task.due_epoch_seconds;
    const dueSec = bundle.task.due_epoch_seconds ?? bundle.task.start_epoch_seconds;
    if (startSec === null || dueSec === null) continue;
    const startDay = epochDayIndex(Math.min(startSec, dueSec));
    const endDay = epochDayIndex(Math.max(startSec, dueSec));
    const isMilestone = bundle.task.domain === 'milestone';
    const anchorDay = isMilestone ? endDay : startDay;
    if (anchorDay > bounds.maxDay) continue;

    const leftPx = isMilestone
      ? (endDay - bounds.minDay + 0.5) * dayWidth - 5
      : Math.max(0, (startDay - bounds.minDay) * dayWidth);
    const widthPx = isMilestone ? 10 : Math.max(4, (endDay - startDay + 1) * dayWidth);

    rows.push({
      rowKey: `${isExternal ? projectName : 'self'}-${bundle.task.id}`,
      bundle,
      projectName,
      isExternal,
      isMilestone,
      isOverdue: isOverdue(bundle),
      startDay,
      endDay,
      leftPx,
      widthPx,
    });
  }

  rows.sort((a, b) => {
    if (ganttGroupByMilestone.value) {
      const aMid = a.isExternal ? Number.MAX_SAFE_INTEGER : (a.bundle.task.milestone_id ?? Number.MAX_SAFE_INTEGER - 1);
      const bMid = b.isExternal ? Number.MAX_SAFE_INTEGER : (b.bundle.task.milestone_id ?? Number.MAX_SAFE_INTEGER - 1);
      if (aMid !== bMid) return aMid - bMid;
    }
    return (a.startDay - b.startDay) || (a.bundle.task.id - b.bundle.task.id);
  });
  return rows;
});

interface GanttPhaseHeaderRow {
  rowKey: string;
  isPhaseHeader: true;
  milestoneId: number | null;
  title: string;
  progressPercent: number;
  hasBaselineBar: boolean;
  leftPx: number;
  widthPx: number;
}

type GanttDisplayRow = (GanttRow & { isPhaseHeader?: false }) | GanttPhaseHeaderRow;

const ganttDisplayRows = computed<GanttDisplayRow[]>(() => {
  if (!ganttGroupByMilestone.value) return ganttRows.value;
  const bounds = ganttBounds.value;
  if (!bounds) return ganttRows.value;
  const dayWidth = ganttDayWidthPx.value;

  const grouped = new Map<number | null, GanttRow[]>();
  const externalRows: GanttRow[] = [];
  for (const r of ganttRows.value) {
    if (r.isExternal) {
      externalRows.push(r);
      continue;
    }
    const mid = r.bundle.task.milestone_id ?? null;
    const list = grouped.get(mid) ?? [];
    list.push(r);
    grouped.set(mid, list);
  }

  const out: GanttDisplayRow[] = [];
  for (const summary of milestoneStore.milestones) {
    const m = summary.milestone;
    const childRows = grouped.get(m.id) ?? [];
    if (childRows.length === 0 && !m.planned_start_epoch_seconds && !m.planned_due_epoch_seconds) {
      continue;
    }
    let hasBaselineBar = false;
    let leftPx = 0;
    let widthPx = 0;
    const startSec = m.planned_start_epoch_seconds ?? m.planned_due_epoch_seconds;
    const dueSec = m.planned_due_epoch_seconds ?? m.planned_start_epoch_seconds;
    if (startSec !== null && dueSec !== null) {
      const startDay = epochDayIndex(Math.min(startSec, dueSec));
      const endDay = epochDayIndex(Math.max(startSec, dueSec));
      if (startDay <= bounds.maxDay) {
        hasBaselineBar = true;
        leftPx = Math.max(0, (startDay - bounds.minDay) * dayWidth);
        widthPx = Math.max(4, (endDay - startDay + 1) * dayWidth);
      }
    }
    out.push({
      rowKey: `phase-${m.id}`,
      isPhaseHeader: true,
      milestoneId: m.id,
      title: m.title,
      progressPercent: summary.progressPercent,
      hasBaselineBar,
      leftPx,
      widthPx,
    });
    for (const cr of childRows) out.push(cr);
  }

  const unassigned = grouped.get(null) ?? [];
  if (unassigned.length > 0) {
    out.push({
      rowKey: 'phase-unassigned',
      isPhaseHeader: true,
      milestoneId: null,
      title: 'Unassigned',
      progressPercent: 0,
      hasBaselineBar: false,
      leftPx: 0,
      widthPx: 0,
    });
    for (const cr of unassigned) out.push(cr);
  }
  for (const er of externalRows) out.push(er);
  return out;
});

function selectGanttDisplayRow(row: GanttDisplayRow) {
  if (row.isPhaseHeader) {
    if (row.milestoneId !== null) openMilestoneDetail(row.milestoneId);
    return;
  }
  selectGanttRow(row);
}

const milestoneTitleById = computed<Record<number, string>>(() => {
  const map: Record<number, string> = {};
  for (const s of milestoneStore.milestones) {
    map[s.milestone.id] = s.milestone.title;
  }
  return map;
});

/** Selects a Gantt row; external overlay tasks are read-only and keep the current selection. */
function selectGanttRow(row: GanttRow) {
  if (row.isExternal) return;
  selectedTaskId.value = row.bundle.task.id;
}

interface GanttMonthSpan {
  key: string;
  label: string;
  offsetDays: number;
  days: number;
}

const ganttMonths = computed<GanttMonthSpan[]>(() => {
  const bounds = ganttBounds.value;
  if (!bounds) return [];
  const months: GanttMonthSpan[] = [];
  const firstDate = new Date(bounds.minDay * MS_PER_DAY);
  let year = firstDate.getUTCFullYear();
  let month = firstDate.getUTCMonth();

  for (let guard = 0; guard < 260; guard += 1) {
    const monthStartDay = Math.floor(Date.UTC(year, month, 1) / MS_PER_DAY);
    const nextMonthStartDay = Math.floor(Date.UTC(year, month + 1, 1) / MS_PER_DAY);
    const start = Math.max(monthStartDay, bounds.minDay);
    const end = Math.min(nextMonthStartDay - 1, bounds.maxDay);
    if (end >= start) {
      months.push({
        key: `${year}-${month}`,
        label: `${MONTH_NAMES[month]} ${year}`,
        offsetDays: start - bounds.minDay,
        days: end - start + 1,
      });
    }
    if (nextMonthStartDay > bounds.maxDay) break;
    month += 1;
    if (month > 11) {
      month = 0;
      year += 1;
    }
  }
  return months;
});

interface GanttDayCell {
  key: string;
  dayOfMonth: number;
  weekdayLabel: string;
  isWeekend: boolean;
  isToday: boolean;
}

/** One cell per day for the date header; bounded by `GANTT_MAX_SPAN_DAYS`. */
const ganttDays = computed<GanttDayCell[]>(() => {
  const bounds = ganttBounds.value;
  if (!bounds) return [];
  const days: GanttDayCell[] = [];
  for (let day = bounds.minDay; day <= bounds.maxDay; day += 1) {
    const date = new Date(day * MS_PER_DAY);
    const weekday = date.getUTCDay();
    days.push({
      key: String(day),
      dayOfMonth: date.getUTCDate(),
      weekdayLabel: WEEKDAY_LABELS[weekday],
      isWeekend: weekday === 0 || weekday === 6,
      isToday: day === ganttTodayDay.value,
    });
  }
  return days;
});

const ganttTodayXPx = computed(() => {
  const bounds = ganttBounds.value;
  if (!bounds) return 0;
  return (ganttTodayDay.value - bounds.minDay + 0.5) * ganttDayWidthPx.value;
});

const ganttOmittedTaskCount = computed(() =>
  Math.max(0, ganttScheduledTasks.value.length - ganttRows.value.length)
);

/** Vertical day grid plus the Saturday/Sunday shading, aligned to the range start. */
const ganttGridStyle = computed<Record<string, string>>((): Record<string, string> => {
  const bounds = ganttBounds.value;
  if (!bounds) return {};
  const dayWidth = ganttDayWidthPx.value;
  const startWeekday = (((bounds.minDay + 4) % 7) + 7) % 7; // 0 = Sunday (epoch day 0 was Thursday)
  const mondayOffsetDays = (1 - startWeekday + 7) % 7;
  const dayLine = 'color-mix(in srgb, var(--border-color) 55%, transparent)';
  const weekendFill = 'color-mix(in srgb, var(--border-color) 16%, transparent)';
  return {
    backgroundImage: [
      `repeating-linear-gradient(to right, ${dayLine} 0 1px, transparent 1px ${dayWidth}px)`,
      `repeating-linear-gradient(to right, transparent 0 ${5 * dayWidth}px, ${weekendFill} ${5 * dayWidth}px ${7 * dayWidth}px)`,
    ].join(', '),
    backgroundPositionX: `${mondayOffsetDays * dayWidth}px, 0px`,
  };
});

function ganttBarClass(bundle: ProjectTaskBundle): string {
  if (isStageTerminal(bundle.task.status)) {
    return 'border-[var(--status-ok-border)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)]';
  }
  if (bundle.task.status === 'blocked') {
    return 'border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)]';
  }
  if (bundle.task.status === 'in_progress' || bundle.task.status === 'review') {
    return 'border-[var(--status-info-border)] bg-[var(--status-info-bg)] text-[var(--status-info-color)]';
  }
  return 'border-[var(--border-color)] bg-[var(--ui-bg-card)] text-[var(--dim-color)]';
}

function ganttAnchorEpoch(bundle: ProjectTaskBundle): number | null {
  return bundle.task.due_epoch_seconds ?? bundle.task.start_epoch_seconds;
}

function formatGanttDate(epochSeconds: number | null): string {
  if (!epochSeconds) return '—';
  const d = new Date(epochSeconds * 1000);
  return `${MONTH_NAMES[d.getUTCMonth()]} ${d.getUTCDate()}`;
}

function zoomGantt(deltaPx: number) {
  ganttDayWidthPx.value = Math.min(48, Math.max(10, ganttDayWidthPx.value + deltaPx));
}

function scrollGanttToToday() {
  const el = ganttScrollRef.value;
  const bounds = ganttBounds.value;
  if (!el || !bounds) return;
  const available = Math.max(120, el.clientWidth - GANTT_LABEL_WIDTH_PX);
  el.scrollLeft = Math.max(0, ganttTodayXPx.value - available / 2);
}

watch(
  () => !isForgeActive.value && tasksView.value === 'gantt',
  async (isGantt) => {
    if (!isGantt) return;
    await nextTick();
    scrollGanttToToday();
  },
  { immediate: true }
);

// The board data often arrives after the view is opened; center the chart once
// rows materialize without recentering on every later edit.
watch(
  () => ganttRows.value.length,
  async (count, previousCount) => {
    if (count === 0 || previousCount !== 0 || isForgeActive.value || tasksView.value !== 'gantt') return;
    await nextTick();
    scrollGanttToToday();
  }
);

const selectedBundle = computed<ProjectTaskBundle | null>(() => {
  if (selectedTaskId.value === null) return null;
  return store.tasks.find(b => b.task.id === selectedTaskId.value) ?? null;
});

function stageLabel(slug: string): string {
  return stageBySlug.value[slug]?.label ?? slug.replace('_', ' ');
}

function isStageTerminal(slug: string): boolean {
  return stageBySlug.value[slug]?.is_terminal ?? (slug === 'done');
}

function formatDateShort(epochSeconds: number | null): string {
  if (!epochSeconds) return '';
  const d = new Date(epochSeconds * 1000);
  return d.toISOString().slice(0, 10);
}

function isOverdue(bundle: ProjectTaskBundle): boolean {
  if (!bundle.task.due_epoch_seconds || isStageTerminal(bundle.task.status)) return false;
  return epochDayIndex(bundle.task.due_epoch_seconds) < currentLocalDayIndex();
}

function todoLinkCount(bundle: ProjectTaskBundle): number {
  return bundle.links.filter(l => l.link_type === 'todo').length;
}

function journalLinkCount(bundle: ProjectTaskBundle): number {
  return bundle.links.filter(l => l.link_type === 'journal').length;
}

function priorityClass(priority: number): string {
  if (priority === 1) return 'border-[var(--status-error-border)] text-[var(--status-error-color)] bg-[var(--status-error-bg)]';
  if (priority === 3) return 'border-[var(--border-color)] text-[var(--dim-color)]';
  return 'border-[var(--status-warn-border)] text-[var(--status-warn-color)] bg-[var(--status-warn-bg)]';
}

function statusBadgeClass(status: string): string {
  if (isStageTerminal(status)) {
    return 'border-[var(--status-ok-border)] text-[var(--status-ok-color)] bg-[var(--status-ok-bg)]';
  }
  if (status === 'blocked') {
    return 'border-[var(--status-error-border)] text-[var(--status-error-color)] bg-[var(--status-error-bg)]';
  }
  if (status === 'in_progress' || status === 'review') {
    return 'border-[var(--status-info-border)] text-[var(--status-info-color)] bg-[var(--status-info-bg)]';
  }
  return 'border-[var(--border-color)] text-[var(--dim-color)]';
}

async function refreshAll() {
  if (!props.project?.path) return;
  ganttTodayDay.value = currentLocalDayIndex();
  await Promise.all([
    store.fetchProjectBoard(props.project.path),
    milestoneStore.fetchMilestones(props.project.path),
    journalStore.fetchProjectTodos(props.project.path),
    journalStore.fetchSessionJournals(props.project.path),
  ]);
}

function openStageEditor(stage: ProjectTaskStageRecord) {
  if (editingStageId.value === stage.id) {
    editingStageId.value = null;
    return;
  }
  editingStageId.value = stage.id;
  editStageLabel.value = stage.label;
  editStageTerminal.value = stage.is_terminal;
  const fallbackTarget = store.stages.find(s => s.id !== stage.id);
  deleteStageReassignId.value = fallbackTarget ? String(fallbackTarget.id) : '';
}

function reassignOptionsFor(stageId: number): SelectOption[] {
  return store.stages
    .filter(s => s.id !== stageId)
    .map(s => ({ label: s.label, value: String(s.id) }));
}

async function handleToggleCollapse(stage: ProjectTaskStageRecord) {
  await store.updateStage(
    stage.id,
    stage.label,
    !stage.is_collapsed,
    stage.is_terminal,
    props.project.path
  );
}

async function handleSaveStage(stage: ProjectTaskStageRecord) {
  const cleanLabel = editStageLabel.value.trim();
  if (!cleanLabel || isSavingStage.value) return;
  isSavingStage.value = true;
  try {
    const ok = await store.updateStage(
      stage.id,
      cleanLabel,
      stage.is_collapsed,
      editStageTerminal.value,
      props.project.path
    );
    if (ok) {
      editingStageId.value = null;
    }
  } finally {
    isSavingStage.value = false;
  }
}

async function handleDeleteStage(stage: ProjectTaskStageRecord) {
  if (store.stages.length <= 1 || isSavingStage.value) return;
  const count = store.tasks.filter(b => b.task.status === stage.slug).length;
  const reassignId = count > 0 && deleteStageReassignId.value
    ? Number(deleteStageReassignId.value)
    : null;
  if (count > 0 && !reassignId) return;

  isSavingStage.value = true;
  try {
    const ok = await store.deleteStage(stage.id, props.project.path, reassignId);
    if (ok) {
      editingStageId.value = null;
    }
  } finally {
    isSavingStage.value = false;
  }
}

async function handleCreateStage() {
  const label = newStageLabel.value.trim();
  if (!label || isCreatingStage.value) return;
  isCreatingStage.value = true;
  try {
    const created = await store.createStage(props.project.path, label, newStageTerminal.value);
    if (created) {
      newStageLabel.value = '';
      newStageTerminal.value = false;
      showNewStageForm.value = false;
    }
  } finally {
    isCreatingStage.value = false;
  }
}

const draggableStages = computed<ProjectTaskStageRecord[]>({
  get: () => store.stages,
  set: (nextStages) => {
    const prevIds = store.stages.map((s) => s.id);
    const nextIds = nextStages.map((s) => s.id);
    store.stages = nextStages;
    const changed =
      nextIds.length === prevIds.length &&
      nextIds.some((id, idx) => id !== prevIds[idx]);
    if (changed) {
      void store.reorderStages(props.project.path, nextIds);
    }
  },
});

async function onTaskAddedToStage(targetStageSlug: string, evt: { item?: HTMLElement }) {
  const rawId = evt.item?.dataset?.taskId;
  const taskId = rawId ? Number(rawId) : NaN;
  if (Number.isNaN(taskId)) return;
  const bundle = store.tasks.find((b) => b.task.id === taskId);
  if (!bundle || bundle.task.status === targetStageSlug) return;
  await store.moveTaskStatus(bundle, targetStageSlug as ProjectTaskStatus, props.project.path);
}

let unlistenTasksUpdated: (() => void) | null = null;
let unlistenMilestonesUpdated: (() => void) | null = null;

watch(
  () => props.project.path,
  () => {
    selectedTaskId.value = null;
    selectedMilestoneId.value = null;
    milestoneFilterId.value = null;
    editingStageId.value = null;
    createModalStageSlug.value = null;
    ganttOverlayScope.value = 'current';
    if (!forgeModeAvailable.value && activeSubView.value === 'forge') {
      selectTasksView('board');
    }
    refreshAll();
  }
);

// A persisted `project/forge` node must not survive on a project without git.
watch(
  [activeSubView, forgeModeAvailable],
  ([view, forgeAvailable]) => {
    if (view === 'forge' && !forgeAvailable) selectTasksView('board');
  },
  { immediate: true }
);

onMounted(() => {
  if (!forgeModeAvailable.value && activeSubView.value === 'forge') {
    selectTasksView('board');
  }
  refreshAll();
  unlistenTasksUpdated = onDaemonEvent('project-tasks-updated', payload => {
    if (payload.project_path === props.project.path) {
      store.fetchProjectBoard(props.project.path);
      milestoneStore.fetchMilestones(props.project.path);
    } else if (activeOverlayPaths.value.includes(payload.project_path)) {
      store.fetchOverlayProjectTasks([payload.project_path]);
    }
  });
  unlistenMilestonesUpdated = onDaemonEvent('project-milestones-updated', payload => {
    if (payload.project_path === props.project.path) {
      milestoneStore.fetchMilestones(props.project.path);
      if (selectedMilestoneId.value !== null) {
        milestoneStore.fetchMilestoneDetail(props.project.path, selectedMilestoneId.value);
      }
    }
  });
});

onUnmounted(() => {
  if (unlistenTasksUpdated) {
    unlistenTasksUpdated();
    unlistenTasksUpdated = null;
  }
  if (unlistenMilestonesUpdated) {
    unlistenMilestonesUpdated();
    unlistenMilestonesUpdated = null;
  }
});
</script>

<template>
  <div class="flex flex-col h-full min-h-0 overflow-hidden bg-[var(--bg-color)]">
    <!-- Single panel bar (Spec 34 §6.5): fixed Tasks/Forge toggle on the left,
         the active mode's views next to it, and the matching tools on the right. -->
    <SubtabBar
      :model-value="activeSurfaceItem"
      @update:model-value="selectSurfaceItem"
      :items="surfaceItems"
    >
      <template v-if="forgeModeAvailable" #leading>
        <AppSegmentedToggle
          :model-value="activeMode"
          :items="modeToggleItems"
          @update:model-value="activeMode = $event as 'tasks' | 'forge'"
        />
      </template>

      <template v-if="!isForgeActive || hasForgeRemote" #right>
        <ForgeTopologyTools v-if="isForgeActive" />
        <div v-else class="flex items-center gap-2 min-w-0">
          <template v-if="tasksView === 'gantt'">
            <button
              type="button"
              @click="scrollGanttToToday"
              class="h-6 px-2.5 border border-[var(--border-color)] text-[9px] font-bold uppercase tracking-wider text-[var(--dim-color)] hover:text-[var(--accent-color)] hover:border-[var(--accent-color)] transition-colors cursor-pointer"
            >
              Today
            </button>
            <div class="flex items-center border border-[var(--border-color)]">
              <AppTooltip text="Zoom out">
                <button
                  type="button"
                  @click="zoomGantt(-4)"
                  class="h-6 w-6 flex items-center justify-center text-[var(--dim-color)] hover:text-[var(--accent-color)] transition-colors cursor-pointer"
                >
                  <ZoomOut :size="11" />
                </button>
              </AppTooltip>
              <AppTooltip text="Zoom in">
                <button
                  type="button"
                  @click="zoomGantt(4)"
                  class="h-6 w-6 flex items-center justify-center border-l border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--accent-color)] transition-colors cursor-pointer"
                >
                  <ZoomIn :size="11" />
                </button>
              </AppTooltip>
            </div>
            <div v-if="ganttOverlayOptions.length > 1" class="w-40 shrink-0">
              <AppSelect
                :model-value="ganttOverlayScope"
                :options="ganttOverlayOptions"
                compact
                class="[&>button]:h-6"
                @update:model-value="ganttOverlayScope = $event"
              />
            </div>
            <AppTooltip text="Group tasks by milestone">
              <button
                type="button"
                @click="ganttGroupByMilestone = !ganttGroupByMilestone"
                class="h-6 px-2 border text-[9px] font-bold uppercase tracking-wider transition-colors cursor-pointer flex items-center gap-1 shrink-0"
                :class="ganttGroupByMilestone
                  ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)]'
                  : 'border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
              >
                <Flag :size="10" />
                <span>By Milestone</span>
              </button>
            </AppTooltip>
          </template>

          <button
            v-if="milestoneFilterId !== null"
            type="button"
            @click="milestoneFilterId = null"
            class="h-6 px-2 border border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] text-[9px] font-bold uppercase flex items-center gap-1 shrink-0 cursor-pointer"
          >
            <Flag :size="10" />
            <span class="max-w-[90px] truncate">{{ milestoneTitleById[milestoneFilterId] ?? `#${milestoneFilterId}` }}</span>
            <X :size="10" />
          </button>

          <div class="w-36 shrink-0">
            <AppSelect
              :model-value="domainFilter"
              :options="domainFilterOptions"
              compact
              class="[&>button]:h-6"
              @update:model-value="domainFilter = $event"
            />
          </div>
          <div class="flex-1 min-w-[120px] max-w-[208px] relative flex items-center">
            <Search :size="12" class="text-[var(--dim-color)] absolute left-2 pointer-events-none" />
            <AppInput
              v-model="searchQuery"
              type="text"
              placeholder="Filter tasks..."
              class="!h-6 !text-[10px] pl-7 pr-6"
            />
            <button
              v-if="searchQuery"
              type="button"
              @click="searchQuery = ''"
              class="absolute right-1.5 text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer"
            >
              <X :size="12" />
            </button>
          </div>
        </div>
      </template>
    </SubtabBar>

    <!-- Error Banner -->
    <div
      v-if="!isForgeActive && store.tasksError"
      class="px-3 py-1.5 border-b border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] text-[11px] font-mono flex items-center gap-2 shrink-0"
    >
      <AlertCircle :size="12" class="shrink-0" />
      <span class="truncate">{{ store.tasksError }}</span>
    </div>

    <!-- Gantt range guard: never hide task rows silently -->
    <div
      v-if="!isForgeActive && tasksView === 'gantt' && ganttOmittedTaskCount > 0"
      class="px-3 py-1.5 border-b border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)] text-[10px] font-mono shrink-0"
    >
      {{ ganttOmittedTaskCount }} scheduled task(s) fall outside the 20-year chart range.
    </div>

    <!-- Main Content Area + Optional Detail Inspector -->
    <div class="flex-1 min-h-0 flex overflow-hidden">
      <!-- MILESTONES VIEW -->
      <ProjectMilestonesView
        v-if="!isForgeActive && tasksView === 'milestones'"
        :selected-milestone-id="selectedMilestoneId"
        :milestone-filter-id="milestoneFilterId"
        @select-milestone="openMilestoneDetail"
        @toggle-filter="toggleMilestoneFilter"
        @create-milestone="showCreateMilestoneModal = true"
      />

      <!-- BOARD (CUSTOMIZABLE KANBAN STAGES) -->
      <div
        v-else-if="!isForgeActive && tasksView === 'board'"
        class="flex-1 min-w-0 min-h-0 overflow-x-auto overflow-y-hidden p-3 flex gap-2.5"
      >
        <VueDraggable
          v-model="draggableStages"
          handle=".stage-drag-handle"
          :animation="150"
          ghost-class="opacity-50"
          class="flex items-stretch gap-2.5 h-full min-h-0"
        >
        <div
          v-for="stage in draggableStages"
          :key="stage.id"
          :data-stage-id="stage.id"
          class="shrink-0 flex flex-col min-h-0 border border-[var(--border-color)] ui-surface-card transition-colors"
          :class="stage.is_collapsed ? 'w-11' : 'w-[270px]'"
        >
          <!-- COLLAPSED COLUMN STRIP -->
          <template v-if="stage.is_collapsed">
            <div
              class="stage-drag-handle flex flex-col items-center justify-between h-full py-2 px-1 gap-2 select-none cursor-grab active:cursor-grabbing"
            >
              <div class="flex flex-col items-center gap-1.5">
                <AppTooltip :text="`Expand ${stage.label}`">
                  <button
                    type="button"
                    @click.stop="handleToggleCollapse(stage)"
                    class="p-1 border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--accent-color)] hover:border-[var(--accent-color)] cursor-pointer"
                  >
                    <PanelLeftOpen :size="11" />
                  </button>
                </AppTooltip>
                <span class="text-[9px] font-mono px-1 border border-[var(--border-color)] text-[var(--dim-color)]">
                  {{ tasksForStage(stage.slug).length }}
                </span>
              </div>

              <div class="flex-1 flex items-center justify-center overflow-hidden">
                <span
                  class="text-[10px] font-bold uppercase tracking-wider text-[var(--dim-color)] whitespace-nowrap [writing-mode:vertical-rl] rotate-180"
                >
                  {{ stage.label }}
                </span>
              </div>

              <AppTooltip :text="`New task in ${stage.label}`">
                <button
                  type="button"
                  @click.stop="createModalStageSlug = stage.slug"
                  class="p-1 border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--accent-color)] hover:border-[var(--accent-color)] cursor-pointer"
                >
                  <Plus :size="11" />
                </button>
              </AppTooltip>
            </div>
          </template>

          <!-- EXPANDED STAGE COLUMN -->
          <template v-else>
            <div
              class="px-2 py-2 border-b border-[var(--border-color)] ui-surface-title flex items-center justify-between gap-1.5 shrink-0 select-none"
              :class="editingStageId === stage.id ? '' : 'stage-drag-handle cursor-grab active:cursor-grabbing'"
            >
              <div class="flex items-center gap-1.5 min-w-0">
                <AppTooltip text="Drag to reorder stage">
                  <GripVertical :size="12" class="text-[var(--dim-color)]/70 cursor-grab active:cursor-grabbing shrink-0" />
                </AppTooltip>
                <span class="text-[10px] font-bold uppercase tracking-wider text-[var(--fg-color)] truncate">
                  {{ stage.label }}
                </span>
                <span
                  v-if="stage.is_terminal"
                  class="text-[8px] font-mono px-1 border border-[var(--status-ok-border)] text-[var(--status-ok-color)] uppercase shrink-0"
                >
                  DONE
                </span>
                <span class="text-[9px] font-mono px-1.5 py-0.2 border border-[var(--border-color)] text-[var(--dim-color)] shrink-0">
                  {{ tasksForStage(stage.slug).length }}
                </span>
              </div>

              <div class="flex items-center gap-1 shrink-0">
                <AppTooltip :text="`Add task in ${stage.label}`">
                  <button
                    type="button"
                    @click.stop="createModalStageSlug = stage.slug"
                    class="p-1 border border-[var(--border-color)] text-[var(--accent-color)] hover:border-[var(--accent-color)] hover:bg-[var(--accent-color)]/15 cursor-pointer"
                  >
                    <Plus :size="11" />
                  </button>
                </AppTooltip>
                <AppTooltip text="Configure stage">
                  <button
                    type="button"
                    @click.stop="openStageEditor(stage)"
                    class="p-1 border transition-colors cursor-pointer"
                    :class="editingStageId === stage.id
                      ? 'border-[var(--accent-color)] text-[var(--accent-color)] bg-[var(--accent-color)]/15'
                      : 'border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
                  >
                    <SlidersHorizontal :size="11" />
                  </button>
                </AppTooltip>
                <AppTooltip text="Compact column">
                  <button
                    type="button"
                    @click.stop="handleToggleCollapse(stage)"
                    class="p-1 border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer"
                  >
                    <PanelLeftClose :size="11" />
                  </button>
                </AppTooltip>
              </div>
            </div>

            <!-- Inline Stage Settings Drawer -->
            <div
              v-if="editingStageId === stage.id"
              class="p-2.5 border-b border-[var(--border-color)] bg-[var(--bg-color)]/85 flex flex-col gap-2 shrink-0 text-[10px] font-mono"
            >
              <div class="flex items-center justify-between">
                <span class="font-bold uppercase text-[var(--accent-color)]">Stage Settings</span>
                <button
                  type="button"
                  @click="editingStageId = null"
                  class="text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer"
                >
                  <X :size="12" />
                </button>
              </div>
              <AppInput
                v-model="editStageLabel"
                placeholder="Stage name..."
                @keydown.enter.prevent="handleSaveStage(stage)"
              />
              <div class="flex items-center justify-between gap-2">
                <div class="flex items-center gap-1.5">
                  <AppToggle v-model="editStageTerminal" />
                  <span class="text-[var(--dim-color)]">Marks tasks done</span>
                </div>
                <button
                  type="button"
                  :disabled="!editStageLabel.trim() || isSavingStage"
                  @click="handleSaveStage(stage)"
                  class="px-2 py-1 text-[9px] font-bold uppercase border border-[var(--accent-color)] bg-[var(--accent-color)] text-black hover:opacity-90 cursor-pointer disabled:opacity-40 flex items-center gap-1"
                >
                  <Check :size="10" />
                  <span>Save</span>
                </button>
              </div>

              <!-- Stage Deletion & Reassignment Guard -->
              <div
                v-if="store.stages.length > 1"
                class="pt-1.5 border-t border-[var(--border-color)]/60 flex flex-col gap-1.5"
              >
                <template v-if="store.tasks.filter(b => b.task.status === stage.slug).length > 0">
                  <span class="text-[9px] text-[var(--status-warn-color)]">
                    Move {{ store.tasks.filter(b => b.task.status === stage.slug).length }} task(s) before deleting:
                  </span>
                  <div class="flex items-center gap-1">
                    <div class="flex-1 min-w-0">
                      <AppSelect
                        :model-value="deleteStageReassignId"
                        :options="reassignOptionsFor(stage.id)"
                        @update:model-value="deleteStageReassignId = $event"
                      />
                    </div>
                    <button
                      type="button"
                      :disabled="isSavingStage || !deleteStageReassignId"
                      @click="handleDeleteStage(stage)"
                      class="p-1.5 border border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] hover:opacity-90 cursor-pointer disabled:opacity-40"
                    >
                      <Trash2 :size="11" />
                    </button>
                  </div>
                </template>
                <button
                  v-else
                  type="button"
                  :disabled="isSavingStage"
                  @click="handleDeleteStage(stage)"
                  class="px-2 py-1 text-[9px] font-bold uppercase border border-[var(--status-error-border)] text-[var(--status-error-color)] hover:bg-[var(--status-error-bg)] cursor-pointer flex items-center justify-center gap-1"
                >
                  <Trash2 :size="10" />
                  <span>Delete Stage</span>
                </button>
              </div>
            </div>

            <!-- Tasks List inside Stage -->
            <VueDraggable
              :model-value="tasksForStage(stage.slug)"
              :group="{ name: 'kanban-tasks', pull: true, put: true }"
              :animation="150"
              ghost-class="opacity-50"
              @add="(evt: { item?: HTMLElement }) => onTaskAddedToStage(stage.slug, evt)"
              class="flex-1 overflow-y-auto p-2 flex flex-col gap-2 min-h-[48px]"
            >
              <div
                v-for="bundle in tasksForStage(stage.slug)"
                :key="bundle.task.id"
                :data-task-id="bundle.task.id"
                @click="selectedTaskId = bundle.task.id"
                class="p-2.5 border ui-surface-base hover:border-[var(--accent-color)] transition-colors cursor-pointer flex flex-col gap-1.5"
                :class="selectedTaskId === bundle.task.id ? 'border-[var(--accent-color)]' : 'border-[var(--border-color)]'"
              >
                <div class="flex items-center justify-between gap-1.5">
                  <div class="flex items-center gap-1 min-w-0">
                    <span
                      class="text-[9px] font-mono px-1 border uppercase shrink-0"
                      :class="priorityClass(bundle.task.priority)"
                    >
                      P{{ bundle.task.priority }}
                    </span>
                    <span class="text-[9px] font-mono px-1 border border-[var(--border-color)] text-[var(--dim-color)] uppercase truncate">
                      {{ bundle.task.domain }}
                    </span>
                  </div>
                  <span class="text-[9px] font-mono text-[var(--dim-color)] shrink-0">
                    #{{ bundle.task.id }}
                  </span>
                </div>

                <div class="text-xs font-bold text-[var(--fg-color)] leading-snug break-words">
                  {{ bundle.task.title }}
                </div>

                <div class="flex flex-wrap items-center gap-1.5 pt-0.5 text-[9px] font-mono text-[var(--dim-color)]">
                  <span
                    v-if="bundle.task.milestone_id"
                    class="px-1 border border-[var(--accent-color)]/40 text-[var(--accent-color)] flex items-center gap-0.5 max-w-[120px] truncate"
                  >
                    <Flag :size="8" class="shrink-0" />
                    <span class="truncate">{{ milestoneTitleById[bundle.task.milestone_id] ?? `#${bundle.task.milestone_id}` }}</span>
                  </span>
                  <span
                    v-if="bundle.task.due_epoch_seconds"
                    class="px-1 border"
                    :class="isOverdue(bundle)
                      ? 'border-[var(--status-error-border)] text-[var(--status-error-color)] bg-[var(--status-error-bg)]'
                      : 'border-[var(--border-color)]'"
                  >
                    Due {{ formatDateShort(bundle.task.due_epoch_seconds) }}
                  </span>
                  <span v-if="bundle.subtasks.length > 0" class="flex items-center gap-0.5">
                    <CheckSquare :size="10" />
                    {{ bundle.subtasks.filter(s => s.is_completed).length }}/{{ bundle.subtasks.length }}
                  </span>
                  <span v-if="todoLinkCount(bundle) > 0" class="flex items-center gap-0.5 text-[var(--status-info-color)]">
                    <ListTodo :size="10" />
                    {{ todoLinkCount(bundle) }}
                  </span>
                  <span v-if="journalLinkCount(bundle) > 0" class="flex items-center gap-0.5 text-[var(--status-ok-color)]">
                    <BookOpen :size="10" />
                    {{ journalLinkCount(bundle) }}
                  </span>
                  <span v-if="bundle.task.branch_name" class="flex items-center gap-0.5 truncate max-w-[120px]">
                    <GitBranch :size="10" />
                    <span class="truncate">{{ bundle.task.branch_name }}</span>
                  </span>
                </div>
              </div>

              <div
                v-if="tasksForStage(stage.slug).length === 0"
                class="text-[10px] font-mono text-[var(--dim-color)]/50 text-center py-4 border border-dashed border-[var(--border-color)]/50"
              >
                Empty
              </div>
            </VueDraggable>
          </template>
        </div>
        </VueDraggable>

        <!-- ADD CUSTOM STAGE COLUMN -->
        <div class="shrink-0 flex flex-col">
          <button
            v-if="!showNewStageForm"
            type="button"
            @click="showNewStageForm = true"
            class="w-44 px-3 py-2.5 border border-dashed border-[var(--border-color)] hover:border-[var(--accent-color)] text-[10px] font-mono font-bold uppercase text-[var(--dim-color)] hover:text-[var(--accent-color)] transition-colors cursor-pointer flex items-center justify-center gap-1.5"
          >
            <Plus :size="12" />
            <span>Add Stage</span>
          </button>

          <div
            v-else
            class="w-64 p-3 border border-[var(--accent-color)]/60 bg-[var(--ui-bg-card)]/40 flex flex-col gap-2 font-mono"
          >
            <div class="flex items-center justify-between text-[10px] font-bold uppercase text-[var(--accent-color)]">
              <span>New Stage</span>
              <button
                type="button"
                @click="showNewStageForm = false; newStageLabel = ''"
                class="text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer"
              >
                <X :size="12" />
              </button>
            </div>
            <AppInput
              v-model="newStageLabel"
              placeholder="e.g. QA, Procurement..."
              @keydown.enter.prevent="handleCreateStage"
              @keydown.esc.prevent="showNewStageForm = false"
            />
            <div class="flex items-center gap-2 text-[10px] text-[var(--dim-color)]">
              <AppToggle v-model="newStageTerminal" />
              <span>Marks tasks done</span>
            </div>
            <button
              type="button"
              :disabled="!newStageLabel.trim() || isCreatingStage"
              @click="handleCreateStage"
              class="px-3 py-1.5 text-[10px] font-bold uppercase border border-[var(--accent-color)] bg-[var(--accent-color)] text-black hover:opacity-90 cursor-pointer disabled:opacity-40"
            >
              {{ isCreatingStage ? 'Creating...' : 'Create Stage' }}
            </button>
          </div>
        </div>
      </div>

      <!-- SCHEDULE (chronological list of every dated task and milestone) -->
      <div
        v-else-if="!isForgeActive && tasksView === 'schedule'"
        class="flex-1 min-w-0 min-h-0 overflow-y-auto p-3"
      >
        <div
          v-if="scheduledTasks.length === 0"
          class="flex flex-col items-center justify-center h-full gap-2 p-6 text-center"
        >
          <CalendarClock :size="24" class="text-[var(--dim-color)]/40" />
          <p class="text-[10px] font-mono uppercase tracking-wider text-[var(--dim-color)]">No scheduled tasks</p>
          <p class="text-[10px] font-mono text-[var(--dim-color)]/70">Add a start or due date to a task to place it on the schedule.</p>
        </div>

        <div v-else class="flex flex-col gap-2 border-l-2 border-[var(--border-color)] pl-3 ml-1">
          <div
            v-for="bundle in scheduledTasks"
            :key="bundle.task.id"
            @click="selectedTaskId = bundle.task.id"
            class="p-3 border bg-[var(--ui-bg-card)]/40 hover:border-[var(--accent-color)] transition-colors cursor-pointer flex flex-col gap-1.5 relative"
            :class="selectedTaskId === bundle.task.id ? 'border-[var(--accent-color)]' : 'border-[var(--border-color)]'"
          >
            <div class="flex flex-wrap items-center justify-between gap-2">
              <div class="flex items-center gap-2 min-w-0">
                <span class="text-[9px] font-mono px-1.5 border uppercase" :class="statusBadgeClass(bundle.task.status)">
                  {{ stageLabel(bundle.task.status) }}
                </span>
                <span class="text-[9px] font-mono px-1.5 border border-[var(--border-color)] text-[var(--dim-color)] uppercase">
                  {{ bundle.task.domain }}
                </span>
                <span
                  v-if="bundle.task.milestone_id"
                  class="text-[9px] font-mono px-1.5 border border-[var(--accent-color)]/40 text-[var(--accent-color)] flex items-center gap-1 max-w-[140px] truncate"
                >
                  <Flag :size="9" class="shrink-0" />
                  <span class="truncate">{{ milestoneTitleById[bundle.task.milestone_id] ?? `#${bundle.task.milestone_id}` }}</span>
                </span>
                <span class="text-xs font-bold text-[var(--fg-color)] truncate">
                  {{ bundle.task.title }}
                </span>
              </div>
              <div class="text-[10px] font-mono text-[var(--dim-color)] shrink-0">
                <span v-if="bundle.task.start_epoch_seconds">{{ formatDateShort(bundle.task.start_epoch_seconds) }}</span>
                <span v-if="bundle.task.start_epoch_seconds && bundle.task.due_epoch_seconds"> → </span>
                <span
                  v-if="bundle.task.due_epoch_seconds"
                  :class="isOverdue(bundle) ? 'text-[var(--status-error-color)] font-bold' : ''"
                >
                  {{ formatDateShort(bundle.task.due_epoch_seconds) }}
                </span>
              </div>
            </div>
            <div v-if="bundle.task.description" class="text-[11px] text-[var(--dim-color)] line-clamp-2">
              {{ bundle.task.description }}
            </div>
            <div class="flex items-center gap-3 text-[9px] font-mono text-[var(--dim-color)]">
              <span v-if="bundle.subtasks.length > 0">
                Subtasks: {{ bundle.subtasks.filter(s => s.is_completed).length }}/{{ bundle.subtasks.length }}
              </span>
              <span v-if="todoLinkCount(bundle) > 0" class="text-[var(--status-info-color)]">
                {{ todoLinkCount(bundle) }} linked todos
              </span>
              <span v-if="journalLinkCount(bundle) > 0" class="text-[var(--status-ok-color)]">
                {{ journalLinkCount(bundle) }} devlog minutes
              </span>
            </div>
          </div>
        </div>
      </div>

      <!-- GANTT (frontend rendering of the scheduled tasks node) -->
      <div
        v-else-if="!isForgeActive && tasksView === 'gantt'"
        class="flex-1 min-w-0 min-h-0 flex flex-col overflow-hidden"
      >
        <div
          v-if="ganttRows.length === 0"
          class="flex flex-col items-center justify-center flex-1 gap-2 p-6 text-center"
        >
          <ChartGantt :size="24" class="text-[var(--dim-color)]/40" />
          <p class="text-[10px] font-mono uppercase tracking-wider text-[var(--dim-color)]">No scheduled tasks</p>
          <p class="text-[10px] font-mono text-[var(--dim-color)]/70">Add a start or due date to a task to place it on the Gantt chart.</p>
        </div>

        <div v-else ref="ganttScrollRef" class="flex-1 min-h-0 overflow-auto">
          <div class="flex min-w-max min-h-full">
            <!-- Fixed task labels -->
            <div class="w-60 shrink-0 sticky left-0 z-30 bg-[var(--bg-color)] border-r border-[var(--border-color)] flex flex-col">
              <div class="h-12 shrink-0 border-b border-[var(--border-color)] ui-surface-title flex flex-col">
                <div class="h-6 shrink-0 border-b border-[var(--border-color)]/60"></div>
                <div class="h-6 shrink-0 px-2 flex items-center justify-between gap-2">
                  <span class="text-[9px] font-bold uppercase tracking-wider text-[var(--dim-color)]">Task</span>
                  <span class="text-[9px] font-mono text-[var(--dim-color)]">{{ ganttRows.length }}</span>
                </div>
              </div>
              <div
                v-for="row in ganttDisplayRows"
                :key="row.rowKey"
                @click="selectGanttDisplayRow(row)"
                class="h-7 shrink-0 px-2 flex items-center gap-1.5 border-b border-[var(--border-color)]/40 transition-colors"
                :class="[
                  row.isPhaseHeader
                    ? 'ui-surface-title cursor-pointer'
                    : row.isExternal
                      ? 'opacity-75 cursor-default'
                      : 'cursor-pointer',
                  !row.isPhaseHeader && !row.isExternal && selectedTaskId === row.bundle.task.id ? 'bg-[var(--accent-color)]/10' : 'hover:bg-white/5'
                ]"
              >
                <template v-if="row.isPhaseHeader">
                  <Flag :size="10" class="text-[var(--accent-color)] shrink-0" />
                  <span class="text-[10px] font-bold uppercase tracking-wider text-[var(--fg-color)] truncate min-w-0 flex-1">
                    {{ row.title }}
                  </span>
                  <span v-if="row.milestoneId !== null" class="text-[9px] font-mono text-[var(--accent-color)] shrink-0">
                    {{ Math.round(row.progressPercent) }}%
                  </span>
                </template>
                <template v-else>
                  <span
                    v-if="row.isExternal"
                    class="text-[8px] font-mono px-1 border border-[var(--border-color)] text-[var(--dim-color)] truncate max-w-[68px] shrink-0"
                  >
                    {{ row.projectName }}
                  </span>
                  <span
                    v-else
                    class="text-[8px] font-mono px-1 border uppercase shrink-0"
                    :class="priorityClass(row.bundle.task.priority)"
                  >
                    P{{ row.bundle.task.priority }}
                  </span>
                  <span
                    class="text-[10px] font-mono truncate min-w-0 flex-1"
                    :class="!row.isExternal && selectedTaskId === row.bundle.task.id ? 'text-[var(--accent-color)]' : 'text-[var(--fg-color)]'"
                  >
                    {{ row.bundle.task.title }}
                  </span>
                  <span
                    class="text-[9px] font-mono shrink-0"
                    :class="row.isOverdue ? 'text-[var(--status-error-color)] font-bold' : 'text-[var(--dim-color)]'"
                  >
                    {{ formatGanttDate(ganttAnchorEpoch(row.bundle)) }}
                  </span>
                </template>
              </div>
              <div class="flex-1"></div>
            </div>

            <!-- Time grid -->
            <div
              class="flex flex-col shrink-0"
              :style="{ width: `${(ganttBounds?.totalDays ?? 0) * ganttDayWidthPx}px` }"
            >
              <!-- Month + day header -->
              <div class="h-12 shrink-0 sticky top-0 z-20 ui-surface-title border-b border-[var(--border-color)] flex flex-col">
                <div class="relative h-6 shrink-0 border-b border-[var(--border-color)]/60 overflow-hidden">
                  <div
                    v-for="month in ganttMonths"
                    :key="month.key"
                    class="absolute inset-y-0 border-l border-[var(--border-color)]/60 px-2 flex items-center overflow-hidden"
                    :style="{ left: `${month.offsetDays * ganttDayWidthPx}px`, width: `${month.days * ganttDayWidthPx}px` }"
                  >
                    <span class="text-[9px] font-bold uppercase tracking-wider text-[var(--dim-color)] whitespace-nowrap">
                      {{ month.label }}
                    </span>
                  </div>
                  <div
                    class="absolute inset-y-0 w-px bg-[var(--accent-color)] pointer-events-none"
                    :style="{ left: `${ganttTodayXPx}px` }"
                  ></div>
                </div>

                <div class="flex h-6 shrink-0 overflow-hidden">
                  <div
                    v-for="day in ganttDays"
                    :key="day.key"
                    class="shrink-0 flex flex-col items-center justify-center leading-none border-r border-[var(--border-color)]/20"
                    :class="day.isToday ? 'bg-[var(--accent-color)]/15' : (day.isWeekend ? 'ui-surface-list' : '')"
                    :style="{ width: `${ganttDayWidthPx}px` }"
                  >
                    <span
                      v-if="ganttDayWidthPx >= 16"
                      class="text-[8px] uppercase"
                      :class="day.isWeekend ? 'text-[var(--dim-color)]/60' : 'text-[var(--dim-color)]'"
                    >
                      {{ day.weekdayLabel }}
                    </span>
                    <span
                      class="font-bold"
                      :class="[
                        ganttDayWidthPx < 16 ? 'text-[8px]' : 'text-[9px]',
                        day.isToday ? 'text-[var(--accent-color)]' : 'text-[var(--fg-color)]'
                      ]"
                    >
                      {{ day.dayOfMonth }}
                    </span>
                  </div>
                </div>
              </div>

              <!-- Rows -->
              <div class="relative flex-1 overflow-hidden" :style="ganttGridStyle">
                <div
                  v-for="month in ganttMonths.slice(1)"
                  :key="`sep-${month.key}`"
                  class="absolute inset-y-0 border-l border-[var(--border-color)]/70 pointer-events-none"
                  :style="{ left: `${month.offsetDays * ganttDayWidthPx}px` }"
                ></div>

                <div
                  class="absolute inset-y-0 w-px bg-[var(--accent-color)] pointer-events-none z-10"
                  :style="{ left: `${ganttTodayXPx}px` }"
                ></div>

                <div
                  v-for="row in ganttDisplayRows"
                  :key="row.rowKey"
                  @click="selectGanttDisplayRow(row)"
                  class="relative h-7 border-b border-[var(--border-color)]/40 transition-colors"
                  :class="[
                    row.isPhaseHeader
                      ? 'ui-surface-title/60 cursor-pointer'
                      : row.isExternal
                        ? 'opacity-75 cursor-default'
                        : 'cursor-pointer',
                    !row.isPhaseHeader && !row.isExternal && selectedTaskId === row.bundle.task.id ? 'bg-[var(--accent-color)]/10' : 'hover:bg-white/5'
                  ]"
                >
                  <template v-if="row.isPhaseHeader">
                    <div
                      v-if="row.hasBaselineBar"
                      class="absolute top-1/2 -translate-y-1/2 h-2.5 border border-dashed border-[var(--accent-color)]/70 bg-[var(--accent-color)]/15"
                      :style="{ left: `${row.leftPx}px`, width: `${row.widthPx}px` }"
                    ></div>
                  </template>
                  <template v-else>
                    <!-- Milestones render as diamonds at their due date -->
                    <div
                      v-if="row.isMilestone"
                      class="absolute top-1/2 w-2.5 h-2.5 -translate-y-1/2 rotate-45 border bg-[var(--accent-color)] border-[var(--accent-color)]"
                      :style="{ left: `${row.leftPx}px` }"
                    ></div>
                    <!-- Dated tasks render as day-scaled bars -->
                    <div
                      v-else
                      class="absolute top-1/2 -translate-y-1/2 h-3.5 border flex items-center overflow-hidden"
                      :class="[ganttBarClass(row.bundle), row.isExternal ? 'border-dashed' : '']"
                      :style="{ left: `${row.leftPx}px`, width: `${row.widthPx}px` }"
                    >
                      <span v-if="row.widthPx >= 110" class="px-1.5 text-[8px] font-mono truncate">
                        {{ row.isExternal ? `[${row.projectName}] ${row.bundle.task.title}` : row.bundle.task.title }}
                      </span>
                    </div>
                  </template>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- FORGE (external collaboration flow, provider-agnostic shell) -->
      <ForgeCollabPanel
        v-else-if="isForgeActive && hasForgeRemote"
        :project="project"
        @task-taken="id => { selectedTaskId = id; }"
      />

      <!-- Forge without a remote (or with a git error): link a repository instead of failing -->
      <div
        v-else-if="isForgeActive"
        class="flex-1 min-w-0 min-h-0 flex flex-col items-center justify-center gap-3 p-6 text-center"
      >
        <GitFork :size="26" class="text-[var(--dim-color)]/40" />
        <p class="text-[10px] font-mono uppercase tracking-wider text-[var(--dim-color)]">No external repository</p>
        <p
          v-if="project.git_error"
          class="max-w-sm text-[10px] font-mono leading-relaxed text-[var(--status-error-color)]"
        >
          Git status is unavailable: {{ project.git_error }}
        </p>
        <p v-else class="max-w-sm text-[10px] font-mono leading-relaxed text-[var(--dim-color)]/70">
          Forge reads issues, pull requests and releases from a Git remote. This repository only exists locally.
        </p>
        <button
          type="button"
          @click="emit('openGithub', project)"
          class="h-7 px-3 text-[9px] font-bold uppercase tracking-wider border border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-black transition-colors cursor-pointer"
        >
          Link Repository
        </button>
      </div>

      <!-- DETAIL INSPECTOR PANEL -->
      <ProjectTaskDetailPanel
        v-if="selectedBundle && !isForgeActive"
        :bundle="selectedBundle"
        :project-path="project.path"
        @close="selectedTaskId = null"
      />
      <MilestoneDetailPanel
        v-else-if="selectedMilestoneId !== null && !isForgeActive"
        :milestone-id="selectedMilestoneId"
        :project-path="project.path"
        @close="selectedMilestoneId = null"
        @select-task="id => { selectedMilestoneId = null; selectedTaskId = id; }"
        @filter-by-milestone="id => { toggleMilestoneFilter(id); }"
      />
    </div>

    <!-- Per-Stage Task Creation Modal -->
    <CreateProjectTaskModal
      v-if="createModalStageSlug !== null"
      :project-path="project.path"
      :initial-stage-slug="createModalStageSlug"
      :initial-milestone-id="milestoneFilterId"
      @created="id => { selectedMilestoneId = null; selectedTaskId = id; }"
      @close="createModalStageSlug = null"
    />

    <!-- Milestone Creation Modal -->
    <CreateProjectMilestoneModal
      v-if="showCreateMilestoneModal"
      :project-path="project.path"
      @created="id => { selectedTaskId = null; selectedMilestoneId = id; }"
      @close="showCreateMilestoneModal = false"
    />
  </div>
</template>
