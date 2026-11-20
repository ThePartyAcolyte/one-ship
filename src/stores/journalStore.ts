import { defineStore } from 'pinia';
import { daemonCall } from '../utils/daemonClient';
import { useCoreStore } from './coreStore';
import { useDialogueStore } from './dialogueStore';
import type { ProjectTechnicalDebtRecord } from '../types/generated/ProjectTechnicalDebtRecord';
import type { ProjectTodoRecord } from '../types/generated/ProjectTodoRecord';
import type { SessionJournalRecord } from '../types/generated/SessionJournalRecord';

/** Session Minutes (`db::SessionJournalRecord`). */
export type SessionJournalItem = SessionJournalRecord;

/** A project TODO (`db::ProjectTodoRecord`). */
export type ProjectTodoItem = ProjectTodoRecord;

/** A technical debt item (`db::ProjectTechnicalDebtRecord`). */
export type ProjectTechnicalDebtItem = ProjectTechnicalDebtRecord;

function resolveDefaultProjectPath(): string | null {
  const dialogueStore = useDialogueStore();
  const coreStore = useCoreStore();
  return (
    (dialogueStore.activeAgentId ? dialogueStore.resolveAgentProjectPath(dialogueStore.activeAgentId) : null) ||
    coreStore.activeProject?.path ||
    null
  );
}

export const useJournalStore = defineStore('journal', {
  state: () => ({
    sessionJournals: [] as SessionJournalItem[],
    projectTodos: [] as ProjectTodoItem[],
    projectTechnicalDebt: [] as ProjectTechnicalDebtItem[],
    isLoadingJournals: false,
    journalSubTab: 'devlog' as 'devlog' | 'todos' | 'debt',
  }),

  actions: {
    setJournalSubTab(tab: 'devlog' | 'todos' | 'debt') {
      this.journalSubTab = tab;
    },

    clearJournalState() {
      this.sessionJournals = [];
      this.projectTodos = [];
      this.projectTechnicalDebt = [];
    },

    async fetchSessionJournals(projectPath?: string, agentId?: string) {
      const pPath = projectPath || resolveDefaultProjectPath();
      if (!pPath) return;
      this.isLoadingJournals = true;
      try {
        const journals: SessionJournalItem[] = await daemonCall('get_project_session_journals', {
          projectPath: pPath,
          agentId: agentId || null,
          limitCount: 50,
        });
        this.sessionJournals = journals;
      } catch (err) {
        console.error('[journalStore] Failed to fetch session journals:', err);
      } finally {
        this.isLoadingJournals = false;
      }
    },

    async fetchProjectTodos(projectPath?: string, statusFilter?: string) {
      const pPath = projectPath || resolveDefaultProjectPath();
      if (!pPath) return;
      try {
        const todos: ProjectTodoItem[] = await daemonCall('get_project_todos', {
          projectPath: pPath,
          statusFilter: statusFilter || null,
        });
        this.projectTodos = todos;
      } catch (err) {
        console.error('[journalStore] Failed to fetch project todos:', err);
      }
    },

    async fetchProjectTechnicalDebt(projectPath?: string, statusFilter?: string) {
      const pPath = projectPath || resolveDefaultProjectPath();
      if (!pPath) return;
      try {
        const debts: ProjectTechnicalDebtItem[] = await daemonCall('get_project_technical_debt', {
          projectPath: pPath,
          statusFilter: statusFilter || null,
        });
        this.projectTechnicalDebt = debts;
      } catch (err) {
        console.error('[journalStore] Failed to fetch project technical debt:', err);
      }
    },

    async toggleTodoStatus(todoId: number, currentStatus: string) {
      const newStatus = currentStatus === 'completed' ? 'pending' : 'completed';
      try {
        await daemonCall('update_todo_status', {
          todoId,
          status: newStatus,
        });
        const todo = this.projectTodos.find(t => t.id === todoId);
        if (todo) {
          todo.status = newStatus;
          todo.completed_at = newStatus === 'completed' ? Math.floor(Date.now() / 1000) : null;
        }
      } catch (err) {
        console.error('[journalStore] Failed to update todo status:', err);
      }
    },

    async createProjectTodo(
      title: string,
      category?: string,
      priority?: number,
      description?: string,
      projectPath?: string
    ) {
      const dialogueStore = useDialogueStore();
      const pPath = projectPath || resolveDefaultProjectPath();
      if (!pPath || !title.trim()) return;
      try {
        const created: ProjectTodoItem = await daemonCall('create_project_todo', {
          projectPath: pPath,
          agentId: dialogueStore.activeAgentId || null,
          title: title.trim(),
          description: description?.trim() || null,
          category: category || 'feature',
          priority: priority ?? 2,
        });
        this.projectTodos.unshift(created);
      } catch (err) {
        console.error('[journalStore] Failed to create project todo:', err);
      }
    },

    async resolveTechnicalDebt(debtId: number) {
      try {
        await daemonCall('resolve_technical_debt', { debtId });
        const item = this.projectTechnicalDebt.find(d => d.id === debtId);
        if (item) {
          item.status = 'resolved';
          item.resolved_at = Math.floor(Date.now() / 1000);
        }
      } catch (err) {
        console.error('[journalStore] Failed to resolve technical debt:', err);
      }
    },

    async recordTechnicalDebt(
      title: string,
      impactSummary: string,
      remediationPlan?: string,
      severity?: string,
      projectPath?: string
    ) {
      const pPath = projectPath || resolveDefaultProjectPath();
      if (!pPath || !title.trim() || !impactSummary.trim()) return;
      try {
        const created: ProjectTechnicalDebtItem = await daemonCall('record_technical_debt', {
          projectPath: pPath,
          title: title.trim(),
          category: null,
          impactSummary: impactSummary.trim(),
          remediationPlan: remediationPlan?.trim() || null,
          severity: severity || 'medium',
        });
        this.projectTechnicalDebt.unshift(created);
      } catch (err) {
        console.error('[journalStore] Failed to record technical debt:', err);
      }
    },
  },
});
