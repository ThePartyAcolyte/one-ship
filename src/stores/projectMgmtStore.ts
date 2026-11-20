import { defineStore } from 'pinia';
import { daemonCall } from '../utils/daemonClient';
import { useCoreStore } from './coreStore';
import type { GitBranchInfo } from '../types/generated/GitBranchInfo';
import type { ProjectTaskBundle } from '../types/generated/ProjectTaskBundle';
import type { ProjectTaskStageRecord } from '../types/generated/ProjectTaskStageRecord';

export type ProjectTaskDomain =
  | 'code'
  | 'hardware'
  | 'procurement'
  | 'design'
  | 'docs'
  | 'client'
  | 'milestone'
  | 'ops';

export type ProjectTaskStatus = string;

export interface CreateProjectTaskInput {
  projectPath: string;
  title: string;
  description?: string | null;
  domain: ProjectTaskDomain;
  status: ProjectTaskStatus;
  priority: number;
  milestoneId?: number | null;
  weightPoints?: number | null;
  startEpochSeconds?: number | null;
  dueEpochSeconds?: number | null;
  branchName?: string | null;
}

export interface UpdateProjectTaskInput {
  taskId: number;
  projectPath: string;
  title: string;
  description?: string | null;
  domain: ProjectTaskDomain;
  status: ProjectTaskStatus;
  priority: number;
  milestoneId?: number | null;
  weightPoints?: number | null;
  startEpochSeconds?: number | null;
  dueEpochSeconds?: number | null;
  branchName?: string | null;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export const useProjectMgmtStore = defineStore('projectMgmt', {
  state: () => ({
    stages: [] as ProjectTaskStageRecord[],
    tasks: [] as ProjectTaskBundle[],
    overlayTasksByProjectPath: {} as Record<string, ProjectTaskBundle[]>,
    isLoadingOverlayTasks: false,
    isLoadingStages: false,
    isLoadingTasks: false,
    tasksError: null as string | null,
    branchesByProjectId: {} as Record<number, GitBranchInfo[]>,
    isSwitchingBranchByProjectId: {} as Record<number, boolean>,
    branchError: null as string | null,
  }),

  actions: {
    clearTasks() {
      this.stages = [];
      this.tasks = [];
      this.tasksError = null;
    },

    async fetchProjectStages(projectPath: string) {
      if (!projectPath) return;
      this.isLoadingStages = true;
      this.tasksError = null;
      try {
        this.stages = await daemonCall('list_project_stages', { projectPath });
      } catch (err) {
        this.tasksError = errorMessage(err);
      } finally {
        this.isLoadingStages = false;
      }
    },

    async fetchProjectTasks(projectPath: string, statusFilter?: ProjectTaskStatus | null) {
      if (!projectPath) return;
      this.isLoadingTasks = true;
      this.tasksError = null;
      try {
        this.tasks = await daemonCall('list_project_tasks', {
          projectPath,
          statusFilter: statusFilter ?? null,
        });
      } catch (err) {
        this.tasksError = errorMessage(err);
      } finally {
        this.isLoadingTasks = false;
      }
    },

    async fetchProjectBoard(projectPath: string) {
      if (!projectPath) return;
      await Promise.all([
        this.fetchProjectStages(projectPath),
        this.fetchProjectTasks(projectPath),
      ]);
    },

    async fetchOverlayProjectTasks(projectPaths: string[]) {
      const uniquePaths = Array.from(new Set(projectPaths.filter(Boolean)));
      if (uniquePaths.length === 0) return;
      this.isLoadingOverlayTasks = true;
      try {
        const results = await Promise.all(
          uniquePaths.map(async (projectPath) => {
            const tasks = await daemonCall('list_project_tasks', {
              projectPath,
              statusFilter: null,
            });
            return [projectPath, tasks] as const;
          })
        );
        const nextMap = { ...this.overlayTasksByProjectPath };
        for (const [projectPath, tasks] of results) {
          nextMap[projectPath] = tasks;
        }
        this.overlayTasksByProjectPath = nextMap;
      } catch (err) {
        this.tasksError = errorMessage(err);
      } finally {
        this.isLoadingOverlayTasks = false;
      }
    },

    async createStage(
      projectPath: string,
      label: string,
      isTerminal = false
    ): Promise<ProjectTaskStageRecord | null> {
      this.tasksError = null;
      try {
        const stage = await daemonCall('create_project_stage', {
          projectPath,
          label,
          isTerminal,
        });
        await this.fetchProjectStages(projectPath);
        return stage;
      } catch (err) {
        this.tasksError = errorMessage(err);
        return null;
      }
    },

    async updateStage(
      stageId: number,
      label: string,
      isCollapsed: boolean,
      isTerminal: boolean,
      projectPath: string
    ): Promise<boolean> {
      this.tasksError = null;
      try {
        await daemonCall('update_project_stage', {
          stageId,
          label,
          isCollapsed,
          isTerminal,
        });
        await this.fetchProjectBoard(projectPath);
        return true;
      } catch (err) {
        this.tasksError = errorMessage(err);
        return false;
      }
    },

    async reorderStages(projectPath: string, orderedStageIds: number[]): Promise<boolean> {
      this.tasksError = null;
      try {
        await daemonCall('reorder_project_stages', { projectPath, orderedStageIds });
        await this.fetchProjectStages(projectPath);
        return true;
      } catch (err) {
        this.tasksError = errorMessage(err);
        return false;
      }
    },

    async deleteStage(
      stageId: number,
      projectPath: string,
      reassignToStageId?: number | null
    ): Promise<boolean> {
      this.tasksError = null;
      try {
        await daemonCall('delete_project_stage', {
          stageId,
          reassignToStageId: reassignToStageId ?? null,
        });
        await this.fetchProjectBoard(projectPath);
        return true;
      } catch (err) {
        this.tasksError = errorMessage(err);
        return false;
      }
    },

    async createTask(input: CreateProjectTaskInput): Promise<number | null> {
      this.tasksError = null;
      try {
        const id = await daemonCall('create_project_task', {
          projectPath: input.projectPath,
          title: input.title,
          description: input.description ?? null,
          domain: input.domain,
          status: input.status,
          priority: input.priority,
          milestoneId: input.milestoneId ?? null,
          weightPoints: input.weightPoints ?? null,
          startEpochSeconds: input.startEpochSeconds ?? null,
          dueEpochSeconds: input.dueEpochSeconds ?? null,
          githubRepo: null,
          githubIssueNumber: null,
          githubPrNumber: null,
          branchName: input.branchName ?? null,
        });
        await this.fetchProjectTasks(input.projectPath);
        return id;
      } catch (err) {
        this.tasksError = errorMessage(err);
        return null;
      }
    },

    async updateTask(input: UpdateProjectTaskInput): Promise<boolean> {
      this.tasksError = null;
      try {
        await daemonCall('update_project_task', {
          taskId: input.taskId,
          title: input.title,
          description: input.description ?? null,
          domain: input.domain,
          status: input.status,
          priority: input.priority,
          milestoneId: input.milestoneId ?? null,
          weightPoints: input.weightPoints ?? null,
          startEpochSeconds: input.startEpochSeconds ?? null,
          dueEpochSeconds: input.dueEpochSeconds ?? null,
          branchName: input.branchName ?? null,
        });
        await this.fetchProjectTasks(input.projectPath);
        return true;
      } catch (err) {
        this.tasksError = errorMessage(err);
        return false;
      }
    },

    async moveTaskStatus(bundle: ProjectTaskBundle, newStatus: ProjectTaskStatus, projectPath: string): Promise<boolean> {
      return this.updateTask({
        taskId: bundle.task.id,
        projectPath,
        title: bundle.task.title,
        description: bundle.task.description,
        domain: bundle.task.domain as ProjectTaskDomain,
        status: newStatus,
        priority: bundle.task.priority,
        milestoneId: bundle.task.milestone_id,
        weightPoints: bundle.task.weight_points,
        startEpochSeconds: bundle.task.start_epoch_seconds,
        dueEpochSeconds: bundle.task.due_epoch_seconds,
        branchName: bundle.task.branch_name,
      });
    },

    async deleteTask(taskId: number, projectPath: string): Promise<boolean> {
      this.tasksError = null;
      try {
        await daemonCall('delete_project_task', { taskId });
        await this.fetchProjectTasks(projectPath);
        return true;
      } catch (err) {
        this.tasksError = errorMessage(err);
        return false;
      }
    },

    async addSubtask(taskId: number, title: string, projectPath: string, linkedTodoId?: number | null): Promise<boolean> {
      this.tasksError = null;
      try {
        await daemonCall('create_task_subtask', {
          taskId,
          title,
          linkedTodoId: linkedTodoId ?? null,
        });
        await this.fetchProjectTasks(projectPath);
        return true;
      } catch (err) {
        this.tasksError = errorMessage(err);
        return false;
      }
    },

    async toggleSubtask(subtaskId: number, isCompleted: boolean, projectPath: string): Promise<boolean> {
      this.tasksError = null;
      try {
        await daemonCall('toggle_task_subtask', { subtaskId, isCompleted });
        await this.fetchProjectTasks(projectPath);
        return true;
      } catch (err) {
        this.tasksError = errorMessage(err);
        return false;
      }
    },

    async removeSubtask(subtaskId: number, projectPath: string): Promise<boolean> {
      this.tasksError = null;
      try {
        await daemonCall('delete_task_subtask', { subtaskId });
        await this.fetchProjectTasks(projectPath);
        return true;
      } catch (err) {
        this.tasksError = errorMessage(err);
        return false;
      }
    },

    async linkRelation(taskId: number, linkType: 'todo' | 'journal', targetId: number, projectPath: string): Promise<boolean> {
      this.tasksError = null;
      try {
        await daemonCall('link_task_relation', { taskId, linkType, targetId });
        await this.fetchProjectTasks(projectPath);
        return true;
      } catch (err) {
        this.tasksError = errorMessage(err);
        return false;
      }
    },

    async unlinkRelation(taskId: number, linkType: 'todo' | 'journal', targetId: number, projectPath: string): Promise<boolean> {
      this.tasksError = null;
      try {
        await daemonCall('unlink_task_relation', { taskId, linkType, targetId });
        await this.fetchProjectTasks(projectPath);
        return true;
      } catch (err) {
        this.tasksError = errorMessage(err);
        return false;
      }
    },

    async fetchBranches(projectId: number): Promise<GitBranchInfo[]> {
      this.branchError = null;
      try {
        const list = await daemonCall('list_git_branches', { projectId });
        this.branchesByProjectId[projectId] = list;
        return list;
      } catch (err) {
        this.branchError = errorMessage(err);
        return [];
      }
    },

    async switchBranch(projectId: number, branchName: string, isRemote: boolean): Promise<boolean> {
      this.branchError = null;
      this.isSwitchingBranchByProjectId[projectId] = true;
      try {
        await daemonCall('switch_git_branch', { projectId, branchName, isRemote });
        const coreStore = useCoreStore();
        await Promise.all([this.fetchBranches(projectId), coreStore.reloadProjects()]);
        return true;
      } catch (err) {
        this.branchError = errorMessage(err);
        return false;
      } finally {
        this.isSwitchingBranchByProjectId[projectId] = false;
      }
    },

    async createBranch(
      projectId: number,
      branchName: string,
      startPoint?: string | null,
      switchAfterCreate = true
    ): Promise<boolean> {
      this.branchError = null;
      this.isSwitchingBranchByProjectId[projectId] = true;
      try {
        await daemonCall('create_git_branch', {
          projectId,
          branchName,
          startPoint: startPoint ?? null,
          switch: switchAfterCreate,
        });
        const coreStore = useCoreStore();
        await Promise.all([this.fetchBranches(projectId), coreStore.reloadProjects()]);
        return true;
      } catch (err) {
        this.branchError = errorMessage(err);
        return false;
      } finally {
        this.isSwitchingBranchByProjectId[projectId] = false;
      }
    },
  },
});
