import { defineStore } from 'pinia';
import { daemonCall } from '../utils/daemonClient';
import type { ProjectMilestoneRecord } from '../types/generated/ProjectMilestoneRecord';
import type { ProjectMilestoneSummary } from '../types/generated/ProjectMilestoneSummary';
import type { ProjectMilestoneDetail } from '../types/generated/ProjectMilestoneDetail';

export type MilestoneTrackingMode = 'linear' | 'shape_up' | 'weighted';
export type MilestoneStatus = 'planned' | 'active' | 'completed' | 'archived';

export interface CreateMilestoneInput {
  projectPath: string;
  title: string;
  description?: string | null;
  status?: MilestoneStatus;
  weightPercent?: number;
  hillPositionPercent?: number;
  appetiteDays?: number | null;
  plannedStartEpochSeconds?: number | null;
  plannedDueEpochSeconds?: number | null;
}

export interface UpdateMilestoneInput {
  projectPath: string;
  milestoneId: number;
  title: string;
  description?: string | null;
  status: MilestoneStatus;
  weightPercent: number;
  hillPositionPercent: number;
  appetiteDays?: number | null;
  plannedStartEpochSeconds?: number | null;
  plannedDueEpochSeconds?: number | null;
  sortOrder: number;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export const useMilestoneStore = defineStore('milestones', {
  state: () => ({
    trackingMode: 'linear' as MilestoneTrackingMode,
    overallProgressPercent: 0,
    totalTokensUsedCount: 0,
    milestones: [] as ProjectMilestoneSummary[],
    selectedDetail: null as ProjectMilestoneDetail | null,
    isLoading: false,
    isLoadingDetail: false,
    error: null as string | null,
  }),

  actions: {
    clearMilestones() {
      this.trackingMode = 'linear';
      this.overallProgressPercent = 0;
      this.totalTokensUsedCount = 0;
      this.milestones = [];
      this.selectedDetail = null;
      this.error = null;
    },

    async fetchMilestones(projectPath: string) {
      if (!projectPath) return;
      this.isLoading = true;
      this.error = null;
      try {
        const res = await daemonCall('list_project_milestones', { projectPath });
        this.trackingMode = (res.trackingMode as MilestoneTrackingMode) || 'linear';
        this.overallProgressPercent = res.overallProgressPercent;
        this.totalTokensUsedCount = res.totalTokensUsedCount;
        this.milestones = res.milestones;
      } catch (err) {
        this.error = errorMessage(err);
      } finally {
        this.isLoading = false;
      }
    },

    async fetchMilestoneDetail(projectPath: string, milestoneId: number): Promise<ProjectMilestoneDetail | null> {
      if (!projectPath) return null;
      this.isLoadingDetail = true;
      this.error = null;
      try {
        const detail = await daemonCall('get_project_milestone_detail', {
          projectPath,
          milestoneId,
        });
        this.selectedDetail = detail;
        return detail;
      } catch (err) {
        this.error = errorMessage(err);
        return null;
      } finally {
        this.isLoadingDetail = false;
      }
    },

    async createMilestone(input: CreateMilestoneInput): Promise<ProjectMilestoneRecord | null> {
      this.error = null;
      try {
        const created = await daemonCall('create_project_milestone', {
          projectPath: input.projectPath,
          title: input.title,
          description: input.description ?? null,
          status: input.status ?? 'planned',
          weightPercent: input.weightPercent ?? 0,
          hillPositionPercent: input.hillPositionPercent ?? 0,
          appetiteDays: input.appetiteDays ?? null,
          plannedStartEpochSeconds: input.plannedStartEpochSeconds ?? null,
          plannedDueEpochSeconds: input.plannedDueEpochSeconds ?? null,
        });
        await this.fetchMilestones(input.projectPath);
        return created;
      } catch (err) {
        this.error = errorMessage(err);
        return null;
      }
    },

    async updateMilestone(input: UpdateMilestoneInput): Promise<boolean> {
      this.error = null;
      try {
        await daemonCall('update_project_milestone', {
          projectPath: input.projectPath,
          milestoneId: input.milestoneId,
          title: input.title,
          description: input.description ?? null,
          status: input.status,
          weightPercent: input.weightPercent,
          hillPositionPercent: input.hillPositionPercent,
          appetiteDays: input.appetiteDays ?? null,
          plannedStartEpochSeconds: input.plannedStartEpochSeconds ?? null,
          plannedDueEpochSeconds: input.plannedDueEpochSeconds ?? null,
          sortOrder: input.sortOrder,
        });
        await this.fetchMilestones(input.projectPath);
        if (this.selectedDetail?.summary.milestone.id === input.milestoneId) {
          await this.fetchMilestoneDetail(input.projectPath, input.milestoneId);
        }
        return true;
      } catch (err) {
        this.error = errorMessage(err);
        return false;
      }
    },

    async deleteMilestone(milestoneId: number, projectPath: string): Promise<boolean> {
      this.error = null;
      try {
        await daemonCall('delete_project_milestone', { projectPath, milestoneId });
        if (this.selectedDetail?.summary.milestone.id === milestoneId) {
          this.selectedDetail = null;
        }
        await this.fetchMilestones(projectPath);
        return true;
      } catch (err) {
        this.error = errorMessage(err);
        return false;
      }
    },
  },
});
