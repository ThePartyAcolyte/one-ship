import { defineStore } from 'pinia';
import { daemonCall } from '../utils/daemonClient';
import { listenDaemonEvent } from '../utils/daemonEvents';
import { runNavigationGuard } from '../utils/navigationGuard';
import { reportAsyncFailure } from '../utils/reportAsyncFailure';
import type { ChatLayoutMode } from '../types/generated/ChatLayoutMode';
import type { UiAction } from '../types/generated/UiAction';
import type { UiStateSnapshot } from '../types/generated/UiStateSnapshot';
import { useCoreStore } from './coreStore';
import { useDialogueStore } from './dialogueStore';
import { useProjectExplorerStore, type DocumentContent } from './projectExplorerStore';
import { useJournalStore } from './journalStore';
import { useAgentMetricsStore } from './agentMetricsStore';
import { useElectronicsStore } from './electronicsStore';

export type WorkspaceTab =
  | 'settings'
  | 'overview'
  | 'project'
  | 'design'
  | 'cad'
  | 'pcb'
  | 'preview'
  | 'files'
  | 'knowledge'
  | 'devlog'
  | 'git'
  | 'terminal'
  | 'profiler'
  | 'rea'
  | 'skills'
  | 'electronics';

export interface NavigationTarget {
  projectId?: number;
  projectPath?: string;
  tab?: WorkspaceTab;
  filePath?: string;
  artifactId?: string;
  devlogId?: number;
  todoId?: number;
  debtId?: number;
  devlogSubTab?: 'devlog' | 'todos' | 'debt';
}

export interface ProjectUiWorkspaceState {
  workspaceTab: WorkspaceTab;
  activeNodePath: string;
  selectedDocument: DocumentContent | null;
  activeArtifactId: string | null;
  explorerScope: 'docs' | 'codebase';
  explorerPath: string;
  journalSubTab: 'devlog' | 'todos' | 'debt';
  isFloatingChatOpen: boolean;
  floatingChatAgentId: string | null;
  floatingChatProjectPath?: string | null;
  isFloatingChatMinimized: boolean;
  activeAgentId: string | null;
}

let _uiStateUnlisteners: Array<() => void> = [];

export const useWorkspaceLayoutStore = defineStore('workspaceLayout', {
  state: () => ({
    activeWorkspaceTab: 'overview' as WorkspaceTab,
    activeNodePath: 'overview' as string,
    uiSnapshot: null as UiStateSnapshot | null,
    localRevision: 0,
    listenersInitialized: false,
    dockedChatWidthVw: (() => {
      const saved = localStorage.getItem('oneship:dialogue:docked_chat_width_vw');
      const parsed = saved ? parseFloat(saved) : 28;
      return !isNaN(parsed) && parsed >= 16 && parsed <= 60 ? parsed : 28;
    })(),
    chatLayoutMode:
      (localStorage.getItem('oneship:chat_layout_mode') as ChatLayoutMode) || 'docked',
    isFloatingChatOpen: false,
    floatingChatAgentId: null as string | null,
    floatingChatProjectPath: null as string | null,
    isFloatingChatMinimized: false,
    isProjectDrawerOpen: false,
    isStatusLogOpen: false,
    projectWorkspaceStates: {} as Record<number, ProjectUiWorkspaceState>,
  }),

  actions: {
    /**
     * Typed accessor for a persisted project surface slot value from the daemon snapshot.
     */
    projectSlotValue(projectId: number, key: string): string | null {
      return this.uiSnapshot?.projects[String(projectId)]?.slotValues[key] ?? null;
    },
    setProjectDrawerOpen(open: boolean) {
      this.isProjectDrawerOpen = open;
    },
    setStatusLogOpen(open: boolean) {
      this.isStatusLogOpen = open;
    },
    /**
     * Subscribes to `ui-state-updated` SSE events and hydrates the declarative registry
     * and persisted UI snapshot from `oneship-daemon` (Spec 35).
     */
    async initUiState() {
      if (!this.listenersInitialized) {
        this.listenersInitialized = true;
        const unlisten = listenDaemonEvent('ui-state-updated', (event) => {
          this.applyDaemonSnapshot(event.payload);
        });
        _uiStateUnlisteners.push(unlisten);
      }

      const snapshot = await daemonCall('get_ui_state', {});
      this.applyDaemonSnapshot(snapshot);
    },

    /**
     * Cleanly unregisters all daemon event listeners on unmount/teardown.
     */
    teardown() {
      for (const unlisten of _uiStateUnlisteners) {
        try {
          unlisten();
        } catch (err) {
          console.warn('[workspaceLayoutStore] Listener teardown failed:', err);
        }
      }
      _uiStateUnlisteners = [];
      this.listenersInitialized = false;
    },

    /**
     * Applies a canonical `UiStateSnapshot` from the daemon if its monotonic revision
     * is newer than `localRevision`.
     */
    applyDaemonSnapshot(snapshot: UiStateSnapshot) {
      if (snapshot.revision <= this.localRevision) {
        return;
      }
      this.localRevision = snapshot.revision;
      this.uiSnapshot = snapshot;
      this.chatLayoutMode = snapshot.global.chatLayoutMode;
      this.dockedChatWidthVw = snapshot.global.dockedChatWidthVw;

      const coreStore = useCoreStore();
      const dialogueStore = useDialogueStore();
      const journalStore = useJournalStore();
      const explorerStore = useProjectExplorerStore();
      const electronicsStore = useElectronicsStore();

      const targetProjectId = snapshot.global.activeProjectId;
      if (
        targetProjectId !== null &&
        targetProjectId !== coreStore.activeProjectId &&
        coreStore.projects.some((p) => p.id === targetProjectId)
      ) {
        if (!runNavigationGuard({ projectId: targetProjectId })) return;
        coreStore.setActiveProject(targetProjectId, { skipDaemonSync: true, bypassNavigationGuard: true });
      }

      const activePid = targetProjectId ?? coreStore.activeProjectId;
      if (activePid === null) return;

      const projState = snapshot.projects[String(activePid)];
      if (!projState) return;

      const targetProj = coreStore.projects.find((p) => p.id === activePid);
      const targetPath = targetProj?.path || null;

      this.activeNodePath = projState.activeNodePath;
      const rootTab = projState.activeNodePath.split('/')[0] as WorkspaceTab;
      if (rootTab && rootTab !== this.activeWorkspaceTab && !runNavigationGuard({ workspaceTab: rootTab })) return;
      if (rootTab) {
        this.activeWorkspaceTab = rootTab;
      }

      if (projState.activeNodePath.startsWith('devlog/')) {
        const sub = projState.activeNodePath.split('/')[1];
        if (sub === 'devlog' || sub === 'todos' || sub === 'debt') {
          journalStore.journalSubTab = sub;
        }
      }

      const scopeSlot = projState.slotValues['files.explorer_scope'];
      if (scopeSlot === 'docs' || scopeSlot === 'codebase') {
        explorerStore.explorerScope = scopeSlot;
      }
      const pathSlot = projState.slotValues['files.explorer_path'];
      if (typeof pathSlot === 'string') {
        explorerStore.explorerPath = pathSlot;
      }
      const artifactSlot = projState.slotValues['dialogue.active_artifact_id'];
      if (artifactSlot !== undefined) {
        dialogueStore.activeArtifactId = artifactSlot;
      }

      const ch1Slot = projState.slotValues['electronics.oscilloscope_ch1_enabled'];
      if (ch1Slot === 'true' || ch1Slot === 'false') {
        electronicsStore.ch1Enabled = ch1Slot === 'true';
      }
      const ch2Slot = projState.slotValues['electronics.oscilloscope_ch2_enabled'];
      if (ch2Slot === 'true' || ch2Slot === 'false') {
        electronicsStore.ch2Enabled = ch2Slot === 'true';
      }
      const tbSlot = projState.slotValues['electronics.oscilloscope_timebase_us'];
      if (typeof tbSlot === 'string' && tbSlot.length > 0) {
        electronicsStore.timebaseDivUs = tbSlot;
      }
      const trigSrcSlot = projState.slotValues['electronics.oscilloscope_trigger_source'];
      if (trigSrcSlot === 'CH1' || trigSrcSlot === 'CH2' || trigSrcSlot === 'EXT') {
        electronicsStore.triggerSource = trigSrcSlot;
      }
      const trigSlopeSlot = projState.slotValues['electronics.oscilloscope_trigger_slope'];
      if (trigSlopeSlot === 'Rising' || trigSlopeSlot === 'Falling') {
        electronicsStore.triggerSlope = trigSlopeSlot;
      }
      const trigSweepSlot = projState.slotValues['electronics.oscilloscope_trigger_sweep'];
      if (trigSweepSlot === 'Auto' || trigSweepSlot === 'Normal' || trigSweepSlot === 'Single') {
        electronicsStore.triggerSweep = trigSweepSlot;
      }

      const isOpenOrMin = projState.chatPanelState !== 'closed' && Boolean(projState.activeAgentId);
      const prevAgent = this.floatingChatAgentId;
      this.isFloatingChatOpen = isOpenOrMin;
      this.isFloatingChatMinimized = projState.chatPanelState === 'minimized';
      this.floatingChatAgentId = isOpenOrMin ? projState.activeAgentId : null;
      this.floatingChatProjectPath = isOpenOrMin ? targetPath : null;
      dialogueStore.isOpen = projState.chatPanelState === 'open';
      dialogueStore.activeAgentId = isOpenOrMin ? projState.activeAgentId : null;

      if (isOpenOrMin && projState.activeAgentId && targetPath && prevAgent !== projState.activeAgentId) {
        dialogueStore.activeProjectPath = targetPath;
        dialogueStore.loadActiveConversation(targetPath, projState.activeAgentId).catch(reportAsyncFailure);
      }
    },

    /**
     * Dispatches a validated `UiAction` to `oneship-daemon` and applies the resulting snapshot.
     */
    async dispatchUiAction(action: UiAction): Promise<UiStateSnapshot> {
      const snapshot = await daemonCall('dispatch_ui_action', action);
      this.applyDaemonSnapshot(snapshot);
      return snapshot;
    },

    syncSelectProject(projectId: number) {
      this.dispatchUiAction({ type: 'selectProject', projectId }).catch(reportAsyncFailure);
    },

    async navigateToNode(nodePath: string, explicitProjectId?: number) {
      const coreStore = useCoreStore();
      const projectId = explicitProjectId ?? coreStore.activeProjectId;
      if (projectId === null) return;
      const rootTab = nodePath.split('/')[0] as WorkspaceTab;
      if (rootTab && rootTab !== this.activeWorkspaceTab && !runNavigationGuard({ workspaceTab: rootTab })) return;
      this.activeNodePath = nodePath;
      if (rootTab) {
        this.activeWorkspaceTab = rootTab;
      }
      await this.dispatchUiAction({
        type: 'navigateToNode',
        projectId,
        nodePath,
      });
    },

    async setSlotValue(key: string, value: string | null, explicitProjectId?: number) {
      const coreStore = useCoreStore();
      const projectId = explicitProjectId ?? coreStore.activeProjectId;
      if (projectId === null) return;
      await this.dispatchUiAction({
        type: 'setSlotValue',
        projectId,
        key,
        value,
      });
    },

    setWorkspaceTab(tab: WorkspaceTab, options?: { bypassNavigationGuard?: boolean }) {
      if (!options?.bypassNavigationGuard && tab !== this.activeWorkspaceTab && !runNavigationGuard({ workspaceTab: tab })) return;
      this.activeWorkspaceTab = tab;
      const coreStore = useCoreStore();
      if (coreStore.activeProjectId !== null) {
        this.navigateToNode(tab, coreStore.activeProjectId).catch(reportAsyncFailure);
      }
    },

    setDockedChatWidthVw(vw: number) {
      const clamped = Math.max(16, Math.min(60, Math.round(vw * 10) / 10));
      this.dockedChatWidthVw = clamped;
      try {
        localStorage.setItem('oneship:dialogue:docked_chat_width_vw', String(clamped));
      } catch (err) {
        console.warn('[workspaceLayoutStore] Failed to save dockedChatWidthVw:', err);
      }
      this.dispatchUiAction({
        type: 'setChatLayout',
        mode: this.chatLayoutMode,
        dockedChatWidthVw: clamped,
      }).catch(reportAsyncFailure);
    },

    setChatLayoutMode(mode: ChatLayoutMode) {
      this.chatLayoutMode = mode;
      try {
        localStorage.setItem('oneship:chat_layout_mode', mode);
      } catch (err) {
        console.warn('[workspaceLayoutStore] Failed to save chatLayoutMode:', err);
      }
      this.dispatchUiAction({
        type: 'setChatLayout',
        mode,
        dockedChatWidthVw: this.dockedChatWidthVw,
      }).catch(reportAsyncFailure);
    },

    toggleFloatingChatMinimize() {
      const coreStore = useCoreStore();
      const wasMinimized = this.isFloatingChatMinimized;
      this.isFloatingChatMinimized = !wasMinimized;
      if (coreStore.activeProjectId !== null && this.isFloatingChatOpen) {
        const actionType = wasMinimized ? 'restoreAgentChat' : 'minimizeAgentChat';
        this.dispatchUiAction({
          type: actionType,
          projectId: coreStore.activeProjectId,
        }).catch(reportAsyncFailure);
      }
    },

    openFloatingChat(agentId: string, projectPath?: string) {
      const coreStore = useCoreStore();
      const dialogueStore = useDialogueStore();
      const resolvedPath =
        projectPath || dialogueStore.resolveAgentProjectPath(agentId) || coreStore.activeProject?.path;
      this.floatingChatAgentId = agentId;
      this.floatingChatProjectPath = resolvedPath || null;
      this.isFloatingChatOpen = true;
      this.isFloatingChatMinimized = false;
      dialogueStore.isOpen = true;
      if (resolvedPath) {
        const conv = dialogueStore.getConversation(agentId, resolvedPath);
        if (conv) conv.unread = 0;
      }
      dialogueStore.selectAgent(agentId, resolvedPath).catch(reportAsyncFailure);

      const targetProj = resolvedPath
        ? coreStore.projects.find((p) => p.path === resolvedPath)
        : coreStore.activeProject;
      if (targetProj) {
        this.dispatchUiAction({
          type: 'openAgentChat',
          projectId: targetProj.id,
          agentId,
          conversationId: dialogueStore.activeConversationId ?? null,
        }).catch(reportAsyncFailure);
      }
    },

    closeFloatingChat() {
      const coreStore = useCoreStore();
      this.isFloatingChatOpen = false;
      this.floatingChatAgentId = null;
      this.floatingChatProjectPath = null;
      this.isFloatingChatMinimized = false;
      const dialogueStore = useDialogueStore();
      dialogueStore.isOpen = false;
      if (coreStore.activeProjectId !== null) {
        this.dispatchUiAction({
          type: 'closeAgentChat',
          projectId: coreStore.activeProjectId,
        }).catch(reportAsyncFailure);
      }
    },

    deleteProjectState(projectId: number) {
      delete this.projectWorkspaceStates[projectId];
    },

    switchProjectContext(
      fromProjectId: number | null,
      toProjectId: number | null,
      toProject?: {
        path?: string;
        has_git?: boolean;
        has_electronics_enabled?: boolean;
        has_desigma_enabled?: boolean;
        has_solidcraft_enabled?: boolean;
        has_pcbcraft_enabled?: boolean;
      } | null
    ) {
      const dialogueStore = useDialogueStore();
      const explorerStore = useProjectExplorerStore();
      const journalStore = useJournalStore();
      const metricsStore = useAgentMetricsStore();

      if (fromProjectId !== null) {
        this.projectWorkspaceStates[fromProjectId] = {
          workspaceTab: this.activeWorkspaceTab,
          activeNodePath: this.activeNodePath,
          selectedDocument: explorerStore.selectedDocument,
          activeArtifactId: dialogueStore.activeArtifactId,
          explorerScope: explorerStore.explorerScope,
          explorerPath: explorerStore.explorerPath,
          journalSubTab: journalStore.journalSubTab,
          isFloatingChatOpen: this.isFloatingChatOpen,
          floatingChatAgentId: this.floatingChatAgentId,
          floatingChatProjectPath: this.floatingChatProjectPath,
          isFloatingChatMinimized: this.isFloatingChatMinimized,
          activeAgentId: dialogueStore.activeAgentId,
        };
      }

      if (toProjectId !== null) {
        const saved = this.projectWorkspaceStates[toProjectId];
        const coreStore = useCoreStore();
        const targetProj = coreStore.projects.find((proj) => proj.id === toProjectId);
        const targetPath = targetProj?.path || toProject?.path || null;

        if (saved) {
          let validTab = saved.workspaceTab;
          if (validTab === 'git' && !toProject?.has_git) validTab = 'overview';
          if (validTab === 'electronics' && !toProject?.has_electronics_enabled) validTab = 'overview';
          if (validTab === 'design' && !toProject?.has_desigma_enabled) validTab = 'overview';
          if (validTab === 'cad' && !toProject?.has_solidcraft_enabled) validTab = 'overview';
          if (validTab === 'pcb' && !toProject?.has_pcbcraft_enabled) validTab = 'overview';

          let restoreChatOpen = saved.isFloatingChatOpen;
          let restoreAgentId = saved.floatingChatAgentId;
          if (restoreAgentId && targetPath) {
            const st = coreStore.agentStates[toProjectId];
            const lowerRestoreId = restoreAgentId.toLowerCase();
            const inAgents = st?.agents?.some((a) => a.id.toLowerCase() === lowerRestoreId);
            const isEphemeralForProject =
              saved.floatingChatProjectPath === targetPath &&
              (lowerRestoreId === 'initializer' ||
                lowerRestoreId === 'init' ||
                lowerRestoreId.endsWith('-resolver') ||
                lowerRestoreId.endsWith('_resolver') ||
                lowerRestoreId.endsWith('-crafter') ||
                lowerRestoreId.endsWith('_crafter'));
            if (!inAgents && !isEphemeralForProject) {
              restoreChatOpen = false;
              restoreAgentId = null;
            }
          }

          this.activeWorkspaceTab = validTab;
          this.activeNodePath = validTab === 'overview' ? 'overview' : saved.activeNodePath;
          explorerStore.selectedDocument = saved.selectedDocument;
          dialogueStore.activeArtifactId = saved.activeArtifactId;
          explorerStore.explorerScope = saved.explorerScope || 'docs';
          explorerStore.explorerPath = saved.explorerPath || '';
          journalStore.journalSubTab = saved.journalSubTab || 'devlog';
          this.isFloatingChatOpen = restoreChatOpen;
          this.floatingChatAgentId = restoreAgentId;
          this.floatingChatProjectPath = restoreChatOpen ? targetPath : null;
          this.isFloatingChatMinimized = saved.isFloatingChatMinimized;
          dialogueStore.activeAgentId = restoreAgentId;

          if (restoreChatOpen && restoreAgentId && targetPath) {
            dialogueStore.activeProjectPath = targetPath;
            dialogueStore.loadActiveConversation(targetPath, restoreAgentId).catch(reportAsyncFailure);
          } else {
            dialogueStore.activeProjectPath = targetPath;
            dialogueStore.activeConversationId = null;
            dialogueStore.activeConversationRecord = null;
            dialogueStore.activeTopic = null;
            metricsStore.activeSessionMetrics = null;
            dialogueStore.oldestLoadedTurnId = null;
            dialogueStore.hasMoreHistory = false;
            dialogueStore.artifactsTouched = [];
          }
        } else {
          this.activeWorkspaceTab = 'overview';
          this.activeNodePath = 'overview';
          explorerStore.selectedDocument = null;
          dialogueStore.activeArtifactId = null;
          explorerStore.explorerScope = 'docs';
          explorerStore.explorerPath = '';
          journalStore.journalSubTab = 'devlog';
          this.isFloatingChatOpen = false;
          this.floatingChatAgentId = null;
          this.floatingChatProjectPath = null;
          this.isFloatingChatMinimized = false;
          dialogueStore.activeAgentId = null;
          dialogueStore.activeProjectPath = targetPath;
          dialogueStore.activeConversationId = null;
          dialogueStore.activeConversationRecord = null;
          dialogueStore.activeTopic = null;
          metricsStore.activeSessionMetrics = null;
          dialogueStore.oldestLoadedTurnId = null;
          dialogueStore.hasMoreHistory = false;
          dialogueStore.artifactsTouched = [];
        }
      } else {
        dialogueStore.clearProjectState();
        this.activeWorkspaceTab = 'overview';
        this.activeNodePath = 'overview';
        this.isFloatingChatOpen = false;
        this.floatingChatAgentId = null;
        this.floatingChatProjectPath = null;
        this.isFloatingChatMinimized = false;
        dialogueStore.activeAgentId = null;
        dialogueStore.activeProjectPath = null;
        dialogueStore.activeConversationId = null;
        dialogueStore.activeConversationRecord = null;
        dialogueStore.activeTopic = null;
        metricsStore.activeSessionMetrics = null;
        dialogueStore.oldestLoadedTurnId = null;
        dialogueStore.hasMoreHistory = false;
      }
    },

    async navigateTo(target: NavigationTarget) {
      const coreStore = useCoreStore();
      const dialogueStore = useDialogueStore();
      const explorerStore = useProjectExplorerStore();
      const journalStore = useJournalStore();

      let targetProjectId: number | undefined;
      if (target.projectId) {
        targetProjectId = target.projectId;
      } else if (target.projectPath) {
        targetProjectId = coreStore.projects.find((p) => p.path === target.projectPath)?.id;
      }

      const devlogTarget = Boolean(
        !target.tab && !target.filePath && !target.artifactId &&
        (target.devlogId || target.todoId || target.debtId || target.devlogSubTab)
      );
      let targetTab: WorkspaceTab | undefined;
      if (target.tab) {
        targetTab = target.tab;
      } else if (target.filePath || target.artifactId) {
        targetTab = 'files';
      } else if (devlogTarget) {
        targetTab = 'devlog';
      }

      const projectChanges = targetProjectId !== undefined && targetProjectId !== coreStore.activeProjectId;
      const tabChanges = targetTab !== undefined && targetTab !== this.activeWorkspaceTab;
      if ((projectChanges || tabChanges) && !runNavigationGuard({
        ...(projectChanges ? { projectId: targetProjectId } : {}),
        ...(tabChanges ? { workspaceTab: targetTab } : {}),
      })) return;

      if (targetProjectId !== undefined) {
        coreStore.setActiveProject(targetProjectId, { bypassNavigationGuard: true });
      }

      if (devlogTarget) {
        const sub = target.devlogSubTab || 'devlog';
        journalStore.journalSubTab = sub;
        await this.navigateToNode(`devlog/${sub}`).catch(reportAsyncFailure);
      } else if (targetTab) {
        this.activeWorkspaceTab = targetTab;
        if (coreStore.activeProjectId !== null) {
          await this.navigateToNode(targetTab, coreStore.activeProjectId).catch(reportAsyncFailure);
        }
      }

      const activeProj = coreStore.activeProject;
      if (activeProj) {
        if (target.artifactId) {
          dialogueStore.selectArtifact(target.artifactId, activeProj.path);
          const art = dialogueStore.activeArtifact;
          if (art?.filePath) {
            const relPath = art.filePath.startsWith(`${activeProj.path}/`)
              ? art.filePath.slice(activeProj.path.length + 1)
              : art.filePath;
            await explorerStore.selectProjectDocument(activeProj.path, relPath, activeProj.id);
          }
        } else if (target.filePath) {
          const relPath = target.filePath.startsWith(`${activeProj.path}/`)
            ? target.filePath.slice(activeProj.path.length + 1)
            : target.filePath;
          await explorerStore.selectProjectDocument(activeProj.path, relPath, activeProj.id);
        }
      }
    },
  },
});
