import { defineStore } from 'pinia';
import type {
  ProfilerCatalog,
  TargetRunRecord,
  RunProfile,
  RunComparison,
} from '../types';
import { daemonCall } from '../utils/daemonClient';

/** Most runs the Profiler tab lists per project. */
const RUN_HISTORY_LIMIT_COUNT = 200;

/**
 * Run history and continuous profiles (Spec 25). Every service and task run is profiled by the
 * daemon; this store only reads the runs, their profiles and the profiler engine catalog.
 */
export interface ProfilerState {
  catalogs: Record<string, ProfilerCatalog>;
  runs: Record<string, TargetRunRecord[]>;
  /** Profile per run id; `null` while the run has no samples. */
  profiles: Record<string, RunProfile | null>;
  comparison: RunComparison | null;
  isLoadingRuns: boolean;
  isLoadingProfile: boolean;
  isComparing: boolean;
  lastError: string | null;
}

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export const useProfilerStore = defineStore('profiler', {
  state: (): ProfilerState => ({
    catalogs: {},
    runs: {},
    profiles: {},
    comparison: null,
    isLoadingRuns: false,
    isLoadingProfile: false,
    isComparing: false,
    lastError: null,
  }),

  actions: {
    async fetchCatalog(projectPath: string): Promise<void> {
      try {
        this.catalogs[projectPath] = await daemonCall('list_available_profilers', { projectPath });
      } catch (err) {
        this.lastError = `Profiler engines: ${errorText(err)}`;
      }
    },

    async fetchRuns(projectPath: string): Promise<void> {
      this.isLoadingRuns = true;
      try {
        this.runs[projectPath] = await daemonCall('get_target_runs', {
          projectPath,
          targetId: null,
          limitCount: RUN_HISTORY_LIMIT_COUNT,
        });
      } catch (err) {
        this.lastError = `Run history: ${errorText(err)}`;
      } finally {
        this.isLoadingRuns = false;
      }
    },

    async fetchProfile(runId: string): Promise<void> {
      this.isLoadingProfile = true;
      try {
        this.profiles[runId] = await daemonCall('get_run_profile', { runId });
      } catch (err) {
        this.lastError = `Run profile: ${errorText(err)}`;
      } finally {
        this.isLoadingProfile = false;
      }
    },

    async compare(runBeforeId: string, runAfterId: string): Promise<void> {
      this.isComparing = true;
      this.comparison = null;
      try {
        this.comparison = await daemonCall('compare_runs', { runBeforeId, runAfterId });
      } catch (err) {
        this.lastError = `Run comparison: ${errorText(err)}`;
      } finally {
        this.isComparing = false;
      }
    },

    clearError() {
      this.lastError = null;
    },
  },
});
