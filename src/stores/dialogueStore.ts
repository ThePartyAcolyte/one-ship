import { defineStore } from 'pinia';
import { reportAsyncFailure } from '../utils/reportAsyncFailure';
import { daemonCall } from '../utils/daemonClient';
import { formatAttachedReviewMessage } from '../utils/dialogueFormat';
import {
  handleAgentResponseHelper,
  registerDialogueDaemonListeners,
  sendUserMessageHelper,
} from '../utils/dialogueListeners';
import {
  closeAndKillAgentHelper,
  ensureAgentRunningHelper,
  interruptAgentHelper,
  isAgentLaunchingHelper,
  loadActiveConversationHelper,
  loadOlderMessagesHelper,
  setAgentLaunchingHelper,
  startCleanSessionHelper,
  updateAgentExecutionConfigHelper,
} from '../utils/dialogueActions';
import { useCoreStore } from './coreStore';
import { useWorkspaceLayoutStore } from './workspaceLayoutStore';
import { useProjectExplorerStore } from './projectExplorerStore';
import { useJournalStore } from './journalStore';
import { useAgentMetricsStore } from './agentMetricsStore';
import type { AgentConversationRecord } from '../types/generated/AgentConversationRecord';
import type { AgentResponsePayload } from '../types/generated/AgentResponsePayload';
import type { ChatTurnRecord } from '../types/generated/ChatTurnRecord';

let _dialogueUnlisteners: (() => void)[] = [];

export interface DialogueArtifact {
  id: string;
  agentId: string;
  type: 'code' | 'diff' | 'markdown' | 'json' | 'plan' | string;
  lang?: string;
  content: string;
  timestamp: number;
  label: string;
  filePath?: string;
  isEphemeral?: boolean;
}

export interface ApprovalRequestData {
  title: string;
  description: string;
  options?: string[];
  status: 'pending' | 'accepted' | 'rejected' | 'superseded';
  resolvedAt?: number;
  feedback?: string;
}

export type AgentChatTurn = ChatTurnRecord;

export interface DialogueMessage {
  id: string;
  turnId?: number;
  role: 'user' | 'agent' | 'system';
  agentId: string;
  dialogue: string;
  artifact?: DialogueArtifact;
  approval?: ApprovalRequestData;
  timestamp: number;
  terminalSnapshot?: string;
  conversationId?: string;
  topic?: string;
}

export type { AgentConversationRecord };

export interface AgentConversation {
  agentId: string;
  projectPath?: string;
  conversationId?: string | null;
  conversationRecord?: AgentConversationRecord | null;
  topic?: string | null;
  oldestLoadedTurnId?: number | null;
  hasMoreHistory?: boolean;
  artifactsTouched?: string[];
  messages: DialogueMessage[];
  unread: number;
}

export interface AgentLiveStep {
  agentId: string;
  projectPath: string;
  state: 'ACTIVE' | 'DONE' | 'ERROR';
  stepType: string;
  toolName?: string;
  actionSummary: string;
  thoughtText?: string;
  timestampMs: number;
}

export const useDialogueStore = defineStore('dialogue', {
  state: () => ({
    isOpen: false,
    activeAgentId: null as string | null,
    conversations: {} as Record<string, AgentConversation>,
    activeArtifactId: null as string | null,
    listenerInitialized: false,
    activeConversationId: null as string | null,
    activeConversationRecord: null as AgentConversationRecord | null,
    activeTopic: null as string | null,
    hasMoreHistory: false,
    isLoadingHistory: false,
    oldestLoadedTurnId: null as number | null,
    artifactsTouched: [] as string[],
    activeSteps: {} as Record<string, AgentLiveStep | null>,
    agentErrors: {} as Record<string, { type: string; message: string; exitCode?: number; timestamp: number } | null>,
    pendingRetryPrompts: {} as Record<string, string>,
    agentInputDrafts: {} as Record<string, string>,
    activeProjectPath: null as string | null,
  }),

  getters: {
    activeConversation(state): AgentConversation | null {
      if (!state.activeAgentId) return null;
      const lower = state.activeAgentId.toLowerCase();
      if (lower === 'system' && !state.activeProjectPath) {
        return state.conversations['system'] ?? null;
      }
      const pPath = state.activeProjectPath || useDialogueStore().resolveAgentProjectPath(state.activeAgentId);
      if (!pPath) return null;
      return state.conversations[`${pPath}::${lower}`] ?? null;
    },

    activeArtifact(state): DialogueArtifact | null {
      if (!state.activeArtifactId) return null;
      for (const conv of Object.values(state.conversations)) {
        for (const msg of conv.messages) {
          if (msg.artifact?.id === state.activeArtifactId) return msg.artifact;
        }
      }
      return null;
    },

    activeConversationArtifacts(): DialogueArtifact[] {
      const conv = this.activeConversation;
      if (!conv) return [];
      return conv.messages.filter(m => m.artifact).map(m => m.artifact!).reverse();
    },

    conversationScratchpads(): DialogueArtifact[] {
      return this.activeConversationArtifacts.filter(a => a.filePath && a.filePath.startsWith('scratchpad'));
    },

    conversationEphemeralArtifacts(): DialogueArtifact[] {
      return this.activeConversationArtifacts.filter(a => a.isEphemeral || (!a.filePath && !!a.content));
    },

    totalUnread(state): number {
      return Object.values(state.conversations).reduce((sum, c) => sum + c.unread, 0);
    },
  },

  actions: {
    getConversationKey(agentId: string, projectPath?: string | null): string {
      const lower = agentId.toLowerCase();
      if (projectPath) return `${projectPath}::${lower}`;
      if (lower === 'system') return 'system';
      throw new Error(`Cannot build conversation key for agent '${agentId}' without projectPath`);
    },

    isCurrentActiveAgent(agentId?: string | null, projectPath?: string | null): boolean {
      if (!agentId) return false;
      const layoutStore = useWorkspaceLayoutStore();
      const lower = agentId.toLowerCase();
      const currentAid = (this.activeAgentId || layoutStore.floatingChatAgentId)?.toLowerCase();
      if (!currentAid || currentAid !== lower) return false;
      if (lower === 'system') return true;
      const currentPath = this.activeProjectPath || layoutStore.floatingChatProjectPath;
      return !!projectPath && !!currentPath && currentPath === projectPath;
    },

    isChatVisible(agentId?: string | null, projectPath?: string | null): boolean {
      const layoutStore = useWorkspaceLayoutStore();
      if (!layoutStore.isFloatingChatOpen || layoutStore.isFloatingChatMinimized) return false;
      return this.isCurrentActiveAgent(agentId, projectPath);
    },

    ensureConversation(agentId: string, projectPath: string): AgentConversation {
      if (!projectPath && agentId.toLowerCase() !== 'system') {
        throw new Error(`Cannot open a conversation for agent '${agentId}' without a project`);
      }
      const key = this.getConversationKey(agentId, projectPath);
      if (!this.conversations[key]) {
        this.conversations[key] = {
          agentId,
          projectPath,
          conversationId: null,
          conversationRecord: null,
          topic: null,
          oldestLoadedTurnId: null,
          hasMoreHistory: false,
          artifactsTouched: [],
          messages: [],
          unread: 0,
        };
      }
      return this.conversations[key];
    },

    getConversation(agentId?: string | null, projectPath?: string | null): AgentConversation | null {
      if (!agentId || !projectPath) return null;
      return this.conversations[this.getConversationKey(agentId, projectPath)] || null;
    },

    getActiveStep(agentId?: string | null, projectPath?: string | null): AgentLiveStep | null {
      if (!agentId || !projectPath) return null;
      return this.activeSteps[this.getConversationKey(agentId, projectPath)] || null;
    },

    clearActiveStep(agentId: string, projectPath: string) {
      if (!projectPath) throw new Error(`Cannot clear the live step of agent '${agentId}' without a project`);
      this.activeSteps[this.getConversationKey(agentId, projectPath)] = null;
    },

    resolveAgentProjectPath(agentId?: string | null): string | null {
      if (!agentId) return null;
      if (agentId.includes('::')) return agentId.split('::')[0];
      if (agentId.includes(':') && agentId.startsWith('/')) {
        return agentId.substring(0, agentId.lastIndexOf(':'));
      }
      return null;
    },

    async initListener() {
      if (this.listenerInitialized) return;
      this.listenerInitialized = true;
      _dialogueUnlisteners = registerDialogueDaemonListeners();
    },

    teardownListeners() {
      _dialogueUnlisteners.forEach(fn => fn());
      _dialogueUnlisteners = [];
      this.listenerInitialized = false;
    },

    handleAgentResponse(payload: AgentResponsePayload) {
      handleAgentResponseHelper(payload);
    },

    async loadActiveConversation(projectPath: string, agentId: string) {
      await loadActiveConversationHelper(projectPath, agentId);
    },

    async loadOlderMessages() {
      await loadOlderMessagesHelper();
    },

    async sendUserMessage(agentId: string, text: string, explicitProjectPath?: string) {
      await sendUserMessageHelper(agentId, text, explicitProjectPath);
    },

    async interruptAgent(projectPath: string, agentId: string, correctivePrompt?: string) {
      await interruptAgentHelper(projectPath, agentId, correctivePrompt);
    },

    async openOverlay(agentId?: string, projectPath?: string) {
      this.isOpen = true;
      if (agentId) {
        await this.selectAgent(agentId, projectPath);
        useWorkspaceLayoutStore().openFloatingChat(agentId, projectPath);
      }
    },

    closeOverlay() {
      this.isOpen = false;
      const layoutStore = useWorkspaceLayoutStore();
      layoutStore.closeFloatingChat();
    },

    async closeAndKillAgent(projectPath?: string, specificAgentId?: string) {
      await closeAndKillAgentHelper(projectPath, specificAgentId);
    },

    async selectAgent(agentId: string, projectPath?: string) {
      const coreStore = useCoreStore();
      const resolvedPath = projectPath || this.resolveAgentProjectPath(agentId) || coreStore.activeProject?.path;
      this.activeProjectPath = resolvedPath || null;
      this.activeAgentId = agentId;
      const selectedConv = this.getConversation(agentId, resolvedPath);
      if (selectedConv) selectedConv.unread = 0;
      const metricsStore = useAgentMetricsStore();
      if (resolvedPath) {
        const explorerStore = useProjectExplorerStore();
        const journalStore = useJournalStore();
        await this.loadActiveConversation(resolvedPath, agentId);
        await explorerStore.fetchProjectDocuments(resolvedPath);
        await journalStore.fetchSessionJournals(resolvedPath);
        await journalStore.fetchProjectTodos(resolvedPath);
        await journalStore.fetchProjectTechnicalDebt(resolvedPath);
        await metricsStore.fetchContextThreshold(resolvedPath);
        await metricsStore.fetchActiveMetrics(resolvedPath, agentId, this.activeConversationId || undefined);
      }
      await metricsStore.fetchAccountQuotas();
      if (resolvedPath && this.getActiveStep(agentId, resolvedPath)) {
        const normalized = agentId.toLowerCase();
        const selectedProject = coreStore.projects.find(p => p.path === resolvedPath);
        const found = selectedProject ? coreStore.agentStates[selectedProject.id]?.agents?.find(a => a.id.toLowerCase() === normalized) : null;
        if (found?.status !== 'working') this.clearActiveStep(agentId, resolvedPath);
      }
      this.ensureAgentRunning(agentId, resolvedPath || undefined).catch(reportAsyncFailure);
    },

    async ensureAgentRunning(agentId: string, projectPath?: string) {
      await ensureAgentRunningHelper(agentId, projectPath);
    },

    isAgentLaunching(agentId: string, projectPath: string): boolean {
      return isAgentLaunchingHelper(agentId, projectPath);
    },

    setAgentLaunching(agentId: string, projectPath: string, launching: boolean) {
      setAgentLaunchingHelper(agentId, projectPath, launching);
    },

    clearProjectState() {
      useProjectExplorerStore().clearExplorerState();
      useJournalStore().clearJournalState();
      this.artifactsTouched = [];
      this.activeArtifactId = null;
    },

    selectArtifact(artifactId: string, projectPath?: string) {
      this.activeArtifactId = artifactId;
      useProjectExplorerStore().selectedDocument = null;
      useWorkspaceLayoutStore().setSlotValue('dialogue.active_artifact_id', artifactId).catch(reportAsyncFailure);
      const art = this.activeArtifact;
      const pPath = projectPath || this.activeProjectPath || this.resolveAgentProjectPath(this.activeAgentId);
      if (art?.filePath && pPath) this.refreshActiveArtifact(pPath).catch(reportAsyncFailure);
    },

    async refreshActiveArtifact(projectPath: string) {
      const art = this.activeArtifact;
      if (!art?.filePath || !projectPath) return;
      const relPath = art.filePath.startsWith(`${projectPath}/`)
        ? art.filePath.slice(projectPath.length + 1)
        : art.filePath;
      try {
        const doc = await daemonCall('read_project_document', { projectPath, relPath });
        if (doc && doc.content !== art.content) {
          art.content = doc.content;
          art.timestamp = doc.mtime_seconds;
        }
      } catch (err) {
        console.warn('[dialogueStore] Failed to refresh active artifact:', err);
      }
    },

    async reopenConversation(sourceConversationId: string, specificAgentId?: string, specificProjectPath?: string) {
      const targetAgentId = specificAgentId || this.activeAgentId;
      if (!targetAgentId) throw new Error('No agent specified or active to reopen this conversation');
      const projectPath = specificProjectPath || this.resolveAgentProjectPath(targetAgentId) || useCoreStore().activeProject?.path;
      if (!projectPath) throw new Error(`No project resolved for agent '${targetAgentId}'`);
      if (this.activeAgentId !== targetAgentId || this.activeProjectPath !== projectPath) {
        await this.selectAgent(targetAgentId, projectPath);
      }
      this.clearActiveStep(targetAgentId, projectPath);
      await daemonCall('reopen_conversation', { projectPath, agentId: targetAgentId, sourceConversationId });
      await this.loadActiveConversation(projectPath, targetAgentId);
      useWorkspaceLayoutStore().openFloatingChat(targetAgentId, projectPath);
    },

    async searchConversations(query: string, status?: 'active' | 'archived', specificProjectPath?: string, specificAgentId?: string): Promise<AgentConversationRecord[]> {
      const targetAgentId = specificAgentId || this.activeAgentId;
      if (!targetAgentId) throw new Error('No active agent to search conversations for');
      const projectPath = specificProjectPath || this.activeProjectPath || this.resolveAgentProjectPath(targetAgentId);
      if (!projectPath) throw new Error(`No project resolved for agent '${targetAgentId}'`);
      return daemonCall('search_conversations', { projectPath, agentId: targetAgentId, query, status: status ?? null });
    },

    async updateTurnApproval(turnId: number, status: 'accepted' | 'rejected' | 'superseded', feedback?: string) {
      try {
        await daemonCall('update_turn_approval', { turnId, status, feedback: feedback || null });
      } catch (err) {
        console.error('[dialogue] Failed to update turn approval in database:', err);
      }
    },

    getAgentDraft(agentId: string, projectPath?: string | null): string {
      if (!agentId) return '';
      const resolvedPath = projectPath || this.resolveAgentProjectPath(agentId);
      return this.agentInputDrafts[resolvedPath ? this.getConversationKey(agentId, resolvedPath) : agentId.toLowerCase()] ?? '';
    },

    setAgentDraft(agentId: string, text: string, projectPath?: string | null) {
      if (!agentId) return;
      const resolvedPath = projectPath || this.resolveAgentProjectPath(agentId);
      this.agentInputDrafts[resolvedPath ? this.getConversationKey(agentId, resolvedPath) : agentId.toLowerCase()] = text;
    },

    clearAgentDraft(agentId: string, projectPath?: string | null) {
      if (!agentId) return;
      const resolvedPath = projectPath || this.resolveAgentProjectPath(agentId);
      delete this.agentInputDrafts[resolvedPath ? this.getConversationKey(agentId, resolvedPath) : agentId.toLowerCase()];
    },

    async submitAgentPrompt(agentId: string, customText?: string, explicitProjectPath?: string) {
      if (!agentId) return;
      const layoutStore = useWorkspaceLayoutStore();
      const explorerStore = useProjectExplorerStore();
      const targetProjectPath =
        explicitProjectPath ||
        (this.activeAgentId?.toLowerCase() === agentId.toLowerCase() ? this.activeProjectPath : null) ||
        (layoutStore.floatingChatAgentId?.toLowerCase() === agentId.toLowerCase() ? layoutStore.floatingChatProjectPath : null) ||
        this.resolveAgentProjectPath(agentId);
      const rawText = (customText !== undefined ? customText : this.getAgentDraft(agentId, targetProjectPath)).trim();
      if (!rawText && explorerStore.attachedReviewFiles.length === 0) return;
      const finalMessage = formatAttachedReviewMessage(rawText, explorerStore.attachedReviewFiles, explorerStore.documentComments);
      this.sendUserMessage(agentId, finalMessage, targetProjectPath || undefined);
      this.clearAgentDraft(agentId, targetProjectPath);
      explorerStore.clearAttachedReviewFiles();
    },

    async startCleanSession(projectPath: string, agentId: string, runner?: string, model?: string, accountId?: string, effort?: string) {
      await startCleanSessionHelper(projectPath, agentId, runner, model, accountId, effort);
    },

    clearAgentError(agentId: string, projectPath: string) {
      if (agentId && projectPath) this.agentErrors[this.getConversationKey(agentId, projectPath)] = null;
    },

    getAgentError(agentId: string, projectPath: string) {
      if (!agentId || !projectPath) return null;
      return this.agentErrors[this.getConversationKey(agentId, projectPath)] || null;
    },

    getLastUserPrompt(agentId: string, projectPath: string): string | null {
      const conv = this.getConversation(agentId, projectPath);
      if (!conv?.messages.length) return null;
      for (let i = conv.messages.length - 1; i >= 0; i--) {
        if (conv.messages[i].role === 'user') return conv.messages[i].dialogue;
      }
      return null;
    },

    async retryLastAgentPrompt(agentId: string, explicitProjectPath?: string) {
      if (!agentId) return;
      const coreStore = useCoreStore();
      const projectPath = explicitProjectPath || this.resolveAgentProjectPath(agentId) || coreStore.activeProject?.path;
      if (!projectPath) throw new Error(`Cannot retry: no project resolved for agent '${agentId}'`);
      const agtKey = this.getConversationKey(agentId, projectPath);
      const retryPrompt = this.pendingRetryPrompts[agtKey] || this.getLastUserPrompt(agentId, projectPath);
      this.clearAgentError(agentId, projectPath);
      delete this.pendingRetryPrompts[agtKey];
      if (retryPrompt) {
        coreStore.addStatusLog(`[Dialogue] Retrying prompt for agent '${agentId}'...`);
        await daemonCall('prompt_agent', { projectPath, target: agentId, prompt: retryPrompt }).catch((err: unknown) => {
          coreStore.addStatusLog(`[Dialogue] Failed to dispatch retry prompt to agent '${agentId}': ${err instanceof Error ? err.message : String(err)}`);
        });
      }
    },

    async persistConversationConfig(projectPath: string, agentId: string, runner?: string, model?: string, accountId?: string, effort?: string) {
      if (!projectPath || !agentId) return;
      try {
        const updated = await daemonCall('update_active_conversation_config', {
          projectPath, agentId, runner: runner || null, model: model || null, accountId: accountId || null, effort: effort || null,
        });
        if (updated) {
          const convObj = this.ensureConversation(agentId, projectPath);
          convObj.conversationRecord = updated;
          convObj.conversationId = updated.id;
          convObj.topic = updated.topic;
          if (this.isCurrentActiveAgent(agentId, projectPath)) {
            this.activeConversationRecord = updated;
            this.activeConversationId = updated.id;
            this.activeTopic = updated.topic;
          }
        }
      } catch (err) {
        console.error('[dialogue] Failed to persist conversation config:', err);
      }
    },

    async compactContextDirective(agentId: string, explicitProjectPath?: string) {
      if (!agentId) return;
      this.sendUserMessage(
        agentId,
        `[SYSTEM DIRECTIVE: CONTEXT COMPACTION]\nPlease summarize our progress, current technical state, and open items into a concise milestone record. Once recorded, retain only active tasks in memory to compact the context footprint.`,
        explicitProjectPath
      );
    },

    async updateAgentExecutionConfig(projectPath: string, agentId: string, runner?: string, model?: string, accountId?: string, effort?: string) {
      await updateAgentExecutionConfigHelper(projectPath, agentId, runner, model, accountId, effort);
    },
  },
});
