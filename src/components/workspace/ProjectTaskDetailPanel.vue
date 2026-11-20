<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import {
  X,
  Plus,
  Trash2,
  CheckSquare,
  Square,
  Link2,
  Unlink,
  BookOpen,
  ListTodo,
  GitBranch,
  Save,
} from '@lucide/vue';
import { AppTooltip, AppInput, AppDatePicker, AppTextarea, AppSelect, type SelectOption } from '../ui';
import {
  useProjectMgmtStore,
  type ProjectTaskDomain,
  type ProjectTaskStatus,
} from '../../stores/projectMgmtStore';
import { useJournalStore } from '../../stores/journalStore';
import { useMilestoneStore } from '../../stores/milestoneStore';
import { useWorkspaceLayoutStore } from '../../stores/workspaceLayoutStore';
import type { ProjectTaskBundle } from '../../types/generated/ProjectTaskBundle';

const props = defineProps<{
  bundle: ProjectTaskBundle;
  projectPath: string;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
}>();

const store = useProjectMgmtStore();
const journalStore = useJournalStore();
const milestoneStore = useMilestoneStore();
const layoutStore = useWorkspaceLayoutStore();

const editTitle = ref('');
const editDescription = ref('');
const editDomain = ref<ProjectTaskDomain>('code');
const editStatus = ref<ProjectTaskStatus>('backlog');
const editPriority = ref('2');
const editMilestoneId = ref('');
const editWeightPoints = ref('1');
const editStartDate = ref('');
const editDueDate = ref('');
const editBranchName = ref('');
const newSubtaskTitle = ref('');
const selectedTodoToLink = ref('');
const selectedJournalToLink = ref('');
const confirmDelete = ref(false);
const isSaving = ref(false);

const domainOptions: SelectOption[] = [
  { label: 'Code', value: 'code' },
  { label: 'Hardware', value: 'hardware' },
  { label: 'Procurement', value: 'procurement' },
  { label: 'Design', value: 'design' },
  { label: 'Docs', value: 'docs' },
  { label: 'Client', value: 'client' },
  { label: 'Milestone', value: 'milestone' },
  { label: 'Ops', value: 'ops' },
];

const statusOptions = computed<SelectOption[]>(() => {
  return store.stages.map(s => ({ label: s.label, value: s.slug }));
});

const milestoneOptions = computed<SelectOption[]>(() => [
  { label: 'No Milestone', value: '' },
  ...milestoneStore.milestones.map(m => ({ label: m.milestone.title, value: String(m.milestone.id) })),
]);

const priorityOptions: SelectOption[] = [
  { label: 'P1 — High', value: '1' },
  { label: 'P2 — Medium', value: '2' },
  { label: 'P3 — Low', value: '3' },
];

function epochToDateInput(epochSeconds: number | null): string {
  if (!epochSeconds) return '';
  const d = new Date(epochSeconds * 1000);
  const year = d.getUTCFullYear();
  const month = String(d.getUTCMonth() + 1).padStart(2, '0');
  const day = String(d.getUTCDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
}

function dateInputToEpoch(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  const parts = trimmed.split('-').map(Number);
  if (parts.length !== 3 || parts.some(Number.isNaN)) return null;
  return Math.floor(Date.UTC(parts[0], parts[1] - 1, parts[2], 12, 0, 0) / 1000);
}

function syncFromBundle(b: ProjectTaskBundle) {
  editTitle.value = b.task.title;
  editDescription.value = b.task.description ?? '';
  editDomain.value = b.task.domain as ProjectTaskDomain;
  editStatus.value = b.task.status as ProjectTaskStatus;
  editPriority.value = String(b.task.priority);
  editMilestoneId.value = b.task.milestone_id ? String(b.task.milestone_id) : '';
  editWeightPoints.value = String(b.task.weight_points ?? 1);
  editStartDate.value = epochToDateInput(b.task.start_epoch_seconds);
  editDueDate.value = epochToDateInput(b.task.due_epoch_seconds);
  editBranchName.value = b.task.branch_name ?? '';
  confirmDelete.value = false;
}

watch(() => props.bundle, syncFromBundle, { immediate: true });

const linkedTodos = computed(() => {
  const ids = new Set(
    props.bundle.links.filter(l => l.link_type === 'todo').map(l => l.target_id)
  );
  return props.bundle.links
    .filter(l => l.link_type === 'todo')
    .map(l => ({
      linkId: l.id,
      targetId: l.target_id,
      record: journalStore.projectTodos.find(t => t.id === l.target_id) || null,
      ids,
    }));
});

const linkedJournals = computed(() => {
  return props.bundle.links
    .filter(l => l.link_type === 'journal')
    .map(l => ({
      linkId: l.id,
      targetId: l.target_id,
      record: journalStore.sessionJournals.find(j => j.id === l.target_id) || null,
    }));
});

const availableTodoOptions = computed<SelectOption[]>(() => {
  const existing = new Set(
    props.bundle.links.filter(l => l.link_type === 'todo').map(l => l.target_id)
  );
  return journalStore.projectTodos
    .filter(t => t.id !== null && !existing.has(t.id))
    .map(t => ({
      label: `#${t.id} [${t.status}] ${t.title}`,
      value: String(t.id),
    }));
});

const availableJournalOptions = computed<SelectOption[]>(() => {
  const existing = new Set(
    props.bundle.links.filter(l => l.link_type === 'journal').map(l => l.target_id)
  );
  return journalStore.sessionJournals
    .filter(j => j.id !== null && !existing.has(j.id))
    .map(j => ({
      label: `#${j.id} ${j.title}`,
      value: String(j.id),
    }));
});

const dateRangeError = computed(() => {
  const start = dateInputToEpoch(editStartDate.value);
  const due = dateInputToEpoch(editDueDate.value);
  if (start !== null && due !== null && due < start) {
    return 'Due date cannot be earlier than start date.';
  }
  return null;
});

async function handleSaveTask() {
  if (!editTitle.value.trim() || dateRangeError.value || isSaving.value) return;
  isSaving.value = true;
  try {
    await store.updateTask({
      taskId: props.bundle.task.id,
      projectPath: props.projectPath,
      title: editTitle.value.trim(),
      description: editDescription.value.trim() || null,
      domain: editDomain.value,
      status: editStatus.value,
      priority: Number(editPriority.value) || 2,
      milestoneId: editMilestoneId.value ? Number(editMilestoneId.value) : null,
      weightPoints: Math.max(1, Number(editWeightPoints.value) || 1),
      startEpochSeconds: dateInputToEpoch(editStartDate.value),
      dueEpochSeconds: dateInputToEpoch(editDueDate.value),
      branchName: editBranchName.value.trim() || null,
    });
  } finally {
    isSaving.value = false;
  }
}

async function handleAddSubtask() {
  const title = newSubtaskTitle.value.trim();
  if (!title) return;
  const ok = await store.addSubtask(props.bundle.task.id, title, props.projectPath);
  if (ok) {
    newSubtaskTitle.value = '';
  }
}

async function handleLinkTodo() {
  const id = Number(selectedTodoToLink.value);
  if (!id) return;
  const ok = await store.linkRelation(props.bundle.task.id, 'todo', id, props.projectPath);
  if (ok) {
    selectedTodoToLink.value = '';
  }
}

async function handleLinkJournal() {
  const id = Number(selectedJournalToLink.value);
  if (!id) return;
  const ok = await store.linkRelation(props.bundle.task.id, 'journal', id, props.projectPath);
  if (ok) {
    selectedJournalToLink.value = '';
  }
}

async function handleDeleteTask() {
  if (!confirmDelete.value) {
    confirmDelete.value = true;
    return;
  }
  const ok = await store.deleteTask(props.bundle.task.id, props.projectPath);
  if (ok) {
    emit('close');
  }
}
</script>

<template>
  <aside class="w-full lg:w-[380px] shrink-0 border-l border-[var(--border-color)] bg-[var(--ui-bg-card)]/60 flex flex-col min-h-0 h-full overflow-hidden">
    <div class="px-3 py-2 border-b border-[var(--border-color)] flex items-center justify-between bg-black/20 shrink-0">
      <div class="flex items-center gap-2 min-w-0">
        <span class="text-[10px] font-mono text-[var(--accent-color)] font-bold uppercase">
          TASK #{{ bundle.task.id }}
        </span>
        <span
          v-if="bundle.task.github_issue_number"
          class="text-[9px] font-mono px-1.5 py-0.2 border border-[var(--status-info-border)] bg-[var(--status-info-bg)] text-[var(--status-info-color)]"
        >
          ISSUE #{{ bundle.task.github_issue_number }}
        </span>
      </div>
      <AppTooltip text="Close inspector">
        <button
          type="button"
          @click="emit('close')"
          class="p-1 text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer"
        >
          <X :size="14" />
        </button>
      </AppTooltip>
    </div>

    <div class="flex-1 overflow-y-auto p-3 flex flex-col gap-4">
      <!-- Core Fields -->
      <div class="flex flex-col gap-2">
        <label class="text-[9px] uppercase font-bold text-[var(--dim-color)]">Title</label>
        <AppInput v-model="editTitle" placeholder="Task title..." />

        <div class="grid grid-cols-3 gap-2">
          <div class="flex flex-col gap-1">
            <label class="text-[9px] uppercase font-bold text-[var(--dim-color)]">Domain</label>
            <AppSelect
              :model-value="editDomain"
              :options="domainOptions"
              @update:model-value="editDomain = $event as ProjectTaskDomain"
            />
          </div>
          <div class="flex flex-col gap-1">
            <label class="text-[9px] uppercase font-bold text-[var(--dim-color)]">Status</label>
            <AppSelect
              :model-value="editStatus"
              :options="statusOptions"
              @update:model-value="editStatus = $event as ProjectTaskStatus"
            />
          </div>
          <div class="flex flex-col gap-1">
            <label class="text-[9px] uppercase font-bold text-[var(--dim-color)]">Priority</label>
            <AppSelect
              :model-value="editPriority"
              :options="priorityOptions"
              @update:model-value="editPriority = $event"
            />
          </div>
        </div>

        <div class="grid grid-cols-3 gap-2">
          <div class="flex flex-col gap-1 col-span-2">
            <label class="text-[9px] uppercase font-bold text-[var(--dim-color)]">Milestone</label>
            <AppSelect
              :model-value="editMilestoneId"
              :options="milestoneOptions"
              @update:model-value="editMilestoneId = $event"
            />
          </div>
          <div class="flex flex-col gap-1">
            <label class="text-[9px] uppercase font-bold text-[var(--dim-color)]">Points</label>
            <AppInput v-model="editWeightPoints" type="number" />
          </div>
        </div>

        <div class="grid grid-cols-2 gap-2">
          <div class="flex flex-col gap-1">
            <label class="text-[9px] uppercase font-bold text-[var(--dim-color)]">Start Date</label>
            <AppDatePicker v-model="editStartDate" />
          </div>
          <div class="flex flex-col gap-1">
            <label class="text-[9px] uppercase font-bold text-[var(--dim-color)]">Due Date</label>
            <AppDatePicker v-model="editDueDate" />
          </div>
        </div>

        <div v-if="dateRangeError" class="text-[10px] text-[var(--status-error-color)] font-bold">
          {{ dateRangeError }}
        </div>

        <div class="flex flex-col gap-1">
          <label class="text-[9px] uppercase font-bold text-[var(--dim-color)] flex items-center gap-1">
            <GitBranch :size="10" />
            <span>Branch</span>
          </label>
          <AppInput v-model="editBranchName" placeholder="optional branch name" />
        </div>

        <div class="flex flex-col gap-1">
          <label class="text-[9px] uppercase font-bold text-[var(--dim-color)]">Description</label>
          <AppTextarea v-model="editDescription" :rows="3" placeholder="Scope, acceptance criteria, notes..." />
        </div>

        <div class="flex items-center justify-between pt-1">
          <button
            type="button"
            @click="handleDeleteTask"
            class="px-2 py-1 text-[10px] font-bold uppercase border transition-colors cursor-pointer flex items-center gap-1"
            :class="confirmDelete
              ? 'border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)]'
              : 'border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--status-error-color)] hover:border-[var(--status-error-border)]'"
          >
            <Trash2 :size="11" />
            <span>{{ confirmDelete ? 'Confirm Delete' : 'Delete' }}</span>
          </button>

          <button
            type="button"
            :disabled="!editTitle.trim() || Boolean(dateRangeError) || isSaving"
            @click="handleSaveTask"
            class="px-3 py-1 text-[10px] font-bold uppercase border border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] hover:bg-[var(--accent-color)] hover:text-black transition-colors cursor-pointer disabled:opacity-40 flex items-center gap-1"
          >
            <Save :size="11" />
            <span>{{ isSaving ? 'Saving...' : 'Save' }}</span>
          </button>
        </div>
      </div>

      <!-- Subtasks -->
      <div class="border-t border-[var(--border-color)] pt-3 flex flex-col gap-2">
        <div class="flex items-center justify-between">
          <span class="text-[10px] font-bold uppercase text-[var(--fg-color)]">
            Subtasks ({{ bundle.subtasks.filter(s => s.is_completed).length }}/{{ bundle.subtasks.length }})
          </span>
        </div>

        <div class="flex items-center gap-1">
          <div class="flex-1 min-w-0">
            <AppInput
              v-model="newSubtaskTitle"
              placeholder="Add subtask step..."
              @keydown.enter.prevent="handleAddSubtask"
            />
          </div>
          <AppTooltip text="Add subtask">
            <button
              type="button"
              :disabled="!newSubtaskTitle.trim()"
              @click="handleAddSubtask"
              class="p-1.5 border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--accent-color)] hover:border-[var(--accent-color)] cursor-pointer disabled:opacity-40"
            >
              <Plus :size="12" />
            </button>
          </AppTooltip>
        </div>

        <div v-if="bundle.subtasks.length > 0" class="flex flex-col gap-1">
          <div
            v-for="sub in bundle.subtasks"
            :key="sub.id"
            class="flex items-center justify-between gap-2 px-2 py-1 border border-[var(--border-color)] bg-[var(--bg-color)]/40 text-xs"
          >
            <button
              type="button"
              @click="store.toggleSubtask(sub.id, !sub.is_completed, projectPath)"
              class="flex items-center gap-1.5 min-w-0 flex-1 text-left cursor-pointer"
            >
              <CheckSquare v-if="sub.is_completed" :size="12" class="text-[var(--status-ok-color)] shrink-0" />
              <Square v-else :size="12" class="text-[var(--dim-color)] shrink-0" />
              <span
                class="truncate text-[11px]"
                :class="sub.is_completed ? 'line-through text-[var(--dim-color)]' : 'text-[var(--fg-color)]'"
              >
                {{ sub.title }}
              </span>
            </button>
            <AppTooltip text="Remove subtask">
              <button
                type="button"
                @click="store.removeSubtask(sub.id, projectPath)"
                class="text-[var(--dim-color)] hover:text-[var(--status-error-color)] cursor-pointer shrink-0"
              >
                <X :size="11" />
              </button>
            </AppTooltip>
          </div>
        </div>
      </div>

      <!-- Journal Links (Decoupled Bridge) -->
      <div class="border-t border-[var(--border-color)] pt-3 flex flex-col gap-3">
        <div class="flex items-center justify-between">
          <span class="text-[10px] font-bold uppercase text-[var(--fg-color)] flex items-center gap-1.5">
            <Link2 :size="11" class="text-[var(--accent-color)]" />
            <span>Journal Execution Links</span>
          </span>
        </div>

        <!-- Linked Todos -->
        <div class="flex flex-col gap-1.5">
          <div class="flex items-center justify-between text-[9px] uppercase font-bold text-[var(--dim-color)]">
            <span class="flex items-center gap-1">
              <ListTodo :size="10" />
              <span>Linked Dev Todos ({{ linkedTodos.length }})</span>
            </span>
            <button
              type="button"
              @click="layoutStore.navigateTo({ tab: 'devlog', devlogSubTab: 'todos' })"
              class="text-[var(--accent-color)] hover:underline cursor-pointer"
            >
              Open Journal
            </button>
          </div>

          <div class="flex items-center gap-1">
            <div class="flex-1 min-w-0">
              <AppSelect
                :model-value="selectedTodoToLink"
                :options="availableTodoOptions"
                placeholder="Link Journal todo..."
                @update:model-value="selectedTodoToLink = $event"
              />
            </div>
            <AppTooltip text="Link todo">
              <button
                type="button"
                :disabled="!selectedTodoToLink"
                @click="handleLinkTodo"
                class="p-1.5 border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--accent-color)] hover:border-[var(--accent-color)] cursor-pointer disabled:opacity-40"
              >
                <Plus :size="12" />
              </button>
            </AppTooltip>
          </div>

          <div
            v-for="item in linkedTodos"
            :key="item.linkId"
            class="flex items-center justify-between gap-2 px-2 py-1 border border-[var(--border-color)] bg-[var(--bg-color)]/40 text-[11px]"
          >
            <div class="flex items-center gap-1.5 min-w-0">
              <span class="text-[9px] font-mono px-1 border border-[var(--border-color)] text-[var(--dim-color)] shrink-0">
                {{ item.record ? item.record.status : 'todo' }}
              </span>
              <span class="truncate text-[var(--fg-color)]">
                {{ item.record ? item.record.title : `Todo #${item.targetId}` }}
              </span>
            </div>
            <AppTooltip text="Unlink todo">
              <button
                type="button"
                @click="store.unlinkRelation(bundle.task.id, 'todo', item.targetId, projectPath)"
                class="text-[var(--dim-color)] hover:text-[var(--status-error-color)] cursor-pointer shrink-0"
              >
                <Unlink :size="11" />
              </button>
            </AppTooltip>
          </div>
        </div>

        <!-- Linked Journal Entries (Evidence) -->
        <div class="flex flex-col gap-1.5">
          <div class="flex items-center justify-between text-[9px] uppercase font-bold text-[var(--dim-color)]">
            <span class="flex items-center gap-1">
              <BookOpen :size="10" />
              <span>Linked Devlog Minutes ({{ linkedJournals.length }})</span>
            </span>
          </div>

          <div class="flex items-center gap-1">
            <div class="flex-1 min-w-0">
              <AppSelect
                :model-value="selectedJournalToLink"
                :options="availableJournalOptions"
                placeholder="Link Devlog entry..."
                @update:model-value="selectedJournalToLink = $event"
              />
            </div>
            <AppTooltip text="Link devlog entry">
              <button
                type="button"
                :disabled="!selectedJournalToLink"
                @click="handleLinkJournal"
                class="p-1.5 border border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--accent-color)] hover:border-[var(--accent-color)] cursor-pointer disabled:opacity-40"
              >
                <Plus :size="12" />
              </button>
            </AppTooltip>
          </div>

          <div
            v-for="item in linkedJournals"
            :key="item.linkId"
            class="flex items-center justify-between gap-2 px-2 py-1 border border-[var(--border-color)] bg-[var(--bg-color)]/40 text-[11px]"
          >
            <div class="flex items-center gap-1.5 min-w-0">
              <span class="text-[9px] font-mono px-1 border border-[var(--status-ok-border)] text-[var(--status-ok-color)] shrink-0">
                #{{ item.targetId }}
              </span>
              <span class="truncate text-[var(--fg-color)]">
                {{ item.record ? item.record.title : `Journal #${item.targetId}` }}
              </span>
            </div>
            <AppTooltip text="Unlink devlog">
              <button
                type="button"
                @click="store.unlinkRelation(bundle.task.id, 'journal', item.targetId, projectPath)"
                class="text-[var(--dim-color)] hover:text-[var(--status-error-color)] cursor-pointer shrink-0"
              >
                <Unlink :size="11" />
              </button>
            </AppTooltip>
          </div>
        </div>
      </div>
    </div>
  </aside>
</template>
