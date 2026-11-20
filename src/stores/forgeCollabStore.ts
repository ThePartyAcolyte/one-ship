import { defineStore } from 'pinia';
import { daemonCall } from '../utils/daemonClient';
import { listenDaemonEvent } from '../utils/daemonEvents';
import { reportAsyncFailure } from '../utils/reportAsyncFailure';
import { useCoreStore } from './coreStore';
import { useJournalStore } from './journalStore';
import { useProjectMgmtStore } from './projectMgmtStore';
import type { ForgeCollabContext } from '../types/generated/ForgeCollabContext';
import type { ForgeInboxItem } from '../types/generated/ForgeInboxItem';
import type { ForgeIssueDetail } from '../types/generated/ForgeIssueDetail';
import type { ForgeIssueSummary } from '../types/generated/ForgeIssueSummary';
import type { ForgeItemKind } from '../types/generated/ForgeItemKind';
import type { ForgeProviderKind } from '../types/generated/ForgeProviderKind';
import type { ForgePullRequestDetail } from '../types/generated/ForgePullRequestDetail';
import type { ForgePullRequestSummary } from '../types/generated/ForgePullRequestSummary';
import type { ForgeReleaseSummary } from '../types/generated/ForgeReleaseSummary';
import type { ForgeTriageFlag } from '../types/generated/ForgeTriageFlag';
import type { ForgeUnreadCounts } from '../types/generated/ForgeUnreadCounts';

let _unlistenInboxUpdated: (() => void) | null = null;

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export const useForgeCollabStore = defineStore('forgeCollab', {
  state: () => ({
    collabContext: null as ForgeCollabContext | null,
    collabContextByProjectId: {} as Record<number, ForgeCollabContext>,
    issues: [] as ForgeIssueSummary[],
    selectedIssueDetail: null as ForgeIssueDetail | null,
    prs: [] as ForgePullRequestSummary[],
    selectedPrDetail: null as ForgePullRequestDetail | null,
    releases: [] as ForgeReleaseSummary[],
    inboxItems: [] as ForgeInboxItem[],
    unreadCountsByProjectId: {} as Record<number, ForgeUnreadCounts>,
    isLoadingForge: false,
    isLoadingForgeDetail: false,
    isTakingIssue: false,
    isCreatingDraftPr: false,
    forgeError: null as string | null,
  }),

  actions: {
    initInboxListener() {
      if (_unlistenInboxUpdated) return;
      _unlistenInboxUpdated = listenDaemonEvent('forge-inbox-updated', (event) => {
        const { projectId, provider, unreadCount, updatedCount } = event.payload;
        const existing = this.unreadCountsByProjectId[projectId];
        if (existing) {
          this.unreadCountsByProjectId[projectId] = {
            ...existing,
            provider,
            totalUnreadCount: unreadCount,
            updatedCount,
          };
        }
        this.fetchUnreadCounts(projectId).catch(reportAsyncFailure);
      });
    },

    teardownInboxListener() {
      if (_unlistenInboxUpdated) {
        _unlistenInboxUpdated();
        _unlistenInboxUpdated = null;
      }
    },

    clearForgeState() {
      this.collabContext = null;
      this.issues = [];
      this.selectedIssueDetail = null;
      this.prs = [];
      this.selectedPrDetail = null;
      this.releases = [];
      this.inboxItems = [];
      this.forgeError = null;
    },

    async fetchCollabContext(
      projectId: number,
      forceRefresh = false
    ): Promise<ForgeCollabContext | null> {
      this.forgeError = null;
      this.isLoadingForge = true;
      try {
        const ctx = await daemonCall('get_forge_context', {
          projectId,
          forceRefresh,
        });
        this.collabContext = ctx;
        this.collabContextByProjectId[projectId] = ctx;
        return ctx;
      } catch (err) {
        this.forgeError = errorMessage(err);
        return null;
      } finally {
        this.isLoadingForge = false;
      }
    },

    async configureUpstream(
      projectId: number,
      remoteUrl: string
    ): Promise<ForgeCollabContext | null> {
      this.forgeError = null;
      this.isLoadingForge = true;
      try {
        await daemonCall('configure_upstream_remote', { projectId, remoteUrl });
        return await this.fetchCollabContext(projectId, true);
      } catch (err) {
        this.forgeError = errorMessage(err);
        return null;
      } finally {
        this.isLoadingForge = false;
      }
    },

    async syncUpstream(projectId: number): Promise<ForgeCollabContext | null> {
      this.forgeError = null;
      this.isLoadingForge = true;
      try {
        await daemonCall('sync_upstream_fetch', { projectId });
        return await this.fetchCollabContext(projectId, true);
      } catch (err) {
        this.forgeError = errorMessage(err);
        return null;
      } finally {
        this.isLoadingForge = false;
      }
    },

    async fetchIssues(
      projectId: number,
      opts: { state?: string; label?: string; search?: string; forceRefresh?: boolean } = {}
    ) {
      this.forgeError = null;
      this.isLoadingForge = true;
      try {
        this.issues = await daemonCall('list_forge_issues', {
          projectId,
          state: opts.state ?? 'open',
          label: opts.label || null,
          search: opts.search || null,
          forceRefresh: opts.forceRefresh ?? false,
        });
      } catch (err) {
        this.forgeError = errorMessage(err);
      } finally {
        this.isLoadingForge = false;
      }
    },

    async fetchIssueDetail(projectId: number, issueNumber: number, forceRefresh = false) {
      this.forgeError = null;
      this.isLoadingForgeDetail = true;
      try {
        this.selectedIssueDetail = await daemonCall('get_forge_issue', {
          projectId,
          issueNumber,
          forceRefresh,
        });
      } catch (err) {
        this.forgeError = errorMessage(err);
      } finally {
        this.isLoadingForgeDetail = false;
      }
    },

    async fetchPrs(
      projectId: number,
      opts: { state?: string; authorMe?: boolean; forceRefresh?: boolean } = {}
    ) {
      this.forgeError = null;
      this.isLoadingForge = true;
      try {
        this.prs = await daemonCall('list_forge_pull_requests', {
          projectId,
          state: opts.state ?? 'open',
          authorMe: opts.authorMe ?? false,
          forceRefresh: opts.forceRefresh ?? false,
        });
      } catch (err) {
        this.forgeError = errorMessage(err);
      } finally {
        this.isLoadingForge = false;
      }
    },

    async fetchPrDetail(projectId: number, pullRequestNumber: number, forceRefresh = false) {
      this.forgeError = null;
      this.isLoadingForgeDetail = true;
      try {
        this.selectedPrDetail = await daemonCall('get_forge_pull_request', {
          projectId,
          pullRequestNumber,
          forceRefresh,
        });
      } catch (err) {
        this.forgeError = errorMessage(err);
      } finally {
        this.isLoadingForgeDetail = false;
      }
    },

    async fetchReleases(projectId: number, forceRefresh = false) {
      this.forgeError = null;
      this.isLoadingForge = true;
      try {
        this.releases = await daemonCall('list_forge_releases', {
          projectId,
          forceRefresh,
        });
      } catch (err) {
        this.forgeError = errorMessage(err);
      } finally {
        this.isLoadingForge = false;
      }
    },

    async fetchInbox(
      projectId: number,
      opts: { provider?: ForgeProviderKind | null; unreadOnly?: boolean } = {}
    ) {
      try {
        this.inboxItems = await daemonCall('list_forge_inbox', {
          projectId,
          provider: opts.provider ?? null,
          unreadOnly: opts.unreadOnly ?? false,
        });
      } catch (err) {
        this.forgeError = errorMessage(err);
      }
    },

    async fetchUnreadCounts(projectId: number) {
      try {
        const counts = await daemonCall('get_forge_unread_counts', { projectId });
        this.unreadCountsByProjectId[projectId] = counts;
      } catch {
        // Unconfigured/offline projects may not have counts.
      }
    },

    async markItemRead(params: {
      projectId: number;
      provider?: ForgeProviderKind | null;
      itemKind: ForgeItemKind;
      number: number;
      read: boolean;
      updatedAt?: string | null;
    }) {
      try {
        const counts = await daemonCall('mark_forge_item_read', {
          projectId: params.projectId,
          provider: params.provider ?? null,
          itemKind: params.itemKind,
          number: params.number,
          read: params.read,
          updatedAt: params.updatedAt ?? null,
        });
        this.unreadCountsByProjectId[params.projectId] = counts;
        await this.fetchInbox(params.projectId);
      } catch (err) {
        this.forgeError = errorMessage(err);
      }
    },

    async setItemFlag(params: {
      projectId: number;
      provider?: ForgeProviderKind | null;
      itemKind: ForgeItemKind;
      number: number;
      flag: ForgeTriageFlag | null;
    }) {
      try {
        const counts = await daemonCall('set_forge_item_flag', {
          projectId: params.projectId,
          provider: params.provider ?? null,
          itemKind: params.itemKind,
          number: params.number,
          flag: params.flag,
        });
        this.unreadCountsByProjectId[params.projectId] = counts;
        await this.fetchInbox(params.projectId);
      } catch (err) {
        this.forgeError = errorMessage(err);
      }
    },

    async takeIssue(params: {
      projectId: number;
      projectPath: string;
      issueNumber: number;
      createBranch: boolean;
      branchName: string | null;
      createLinkedTodo: boolean;
    }): Promise<number | null> {
      this.forgeError = null;
      this.isTakingIssue = true;
      try {
        const taskId = await daemonCall('take_forge_issue', {
          projectId: params.projectId,
          issueNumber: params.issueNumber,
          createBranch: params.createBranch,
          branchName: params.branchName,
          createLinkedTodo: params.createLinkedTodo,
        });
        const coreStore = useCoreStore();
        const journalStore = useJournalStore();
        const projectMgmtStore = useProjectMgmtStore();
        await Promise.all([
          projectMgmtStore.fetchProjectBoard(params.projectPath),
          params.createBranch
            ? projectMgmtStore.fetchBranches(params.projectId)
            : Promise.resolve([]),
          params.createBranch ? coreStore.reloadProjects() : Promise.resolve(),
          params.createLinkedTodo
            ? journalStore.fetchProjectTodos(params.projectPath)
            : Promise.resolve(),
        ]);
        return taskId;
      } catch (err) {
        this.forgeError = errorMessage(err);
        return null;
      } finally {
        this.isTakingIssue = false;
      }
    },

    async createDraftPr(params: {
      projectId: number;
      targetRepo: string;
      baseBranch: string;
      headBranch: string;
      title: string;
      body: string;
    }): Promise<string | null> {
      this.forgeError = null;
      this.isCreatingDraftPr = true;
      try {
        const prUrl = await daemonCall('create_forge_draft_pull_request', params);
        await this.fetchPrs(params.projectId, { state: 'open', forceRefresh: true });
        return prUrl;
      } catch (err) {
        this.forgeError = errorMessage(err);
        return null;
      } finally {
        this.isCreatingDraftPr = false;
      }
    },
  },
});
