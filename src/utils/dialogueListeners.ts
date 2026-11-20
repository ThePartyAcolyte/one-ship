import { reportAsyncFailure } from './reportAsyncFailure';
import { listenDaemonEvent } from './daemonEvents';
import { buildResponseArtifact } from './dialogueFormat';
import { emitUiEvent } from './uiEvents';
import { useCoreStore } from '../stores/coreStore';
import { useDialogueStore, type ApprovalRequestData, type DialogueMessage } from '../stores/dialogueStore';
import { daemonCall } from './daemonClient';
import { useWorkspaceLayoutStore } from '../stores/workspaceLayoutStore';
import { useProjectExplorerStore } from '../stores/projectExplorerStore';
import { useJournalStore } from '../stores/journalStore';
import { useAgentMetricsStore } from '../stores/agentMetricsStore';
import type { AgentResponsePayload } from '../types/generated/AgentResponsePayload';

export function registerDialogueDaemonListeners(): (() => void)[] {
  const unlisteners: (() => void)[] = [];
  const dialogueStore = useDialogueStore();
  const metricsStore = useAgentMetricsStore();
  const journalStore = useJournalStore();
  const explorerStore = useProjectExplorerStore();
  const layoutStore = useWorkspaceLayoutStore();

  metricsStore.fetchAccountQuotas().catch(reportAsyncFailure);

  unlisteners.push(
    listenDaemonEvent('conversation-topic-switched', async ({ payload }) => {
      if (payload?.project_path && dialogueStore.isCurrentActiveAgent(payload.agent_id, payload.project_path)) {
        await dialogueStore.loadActiveConversation(payload.project_path, payload.agent_id);
      }
    })
  );

  unlisteners.push(
    listenDaemonEvent('agent-conversation-turn-added', async ({ payload }) => {
      if (payload.role !== 'system' || !dialogueStore.isCurrentActiveAgent(payload.agent_id, payload.project_path)) {
        return;
      }
      await dialogueStore.loadActiveConversation(payload.project_path, payload.agent_id);
    })
  );

  unlisteners.push(
    listenDaemonEvent('SessionMinutesRecorded', async ({ payload }) => {
      const pPath = payload?.project_path || dialogueStore.activeProjectPath || useCoreStore().activeProject?.path;
      if (pPath) await journalStore.fetchSessionJournals(pPath);
      if (payload?.agy_conversation_id) await metricsStore.fetchSessionMetrics(payload.agy_conversation_id);
    })
  );

  unlisteners.push(
    listenDaemonEvent('AccountQuotaUpdated', async () => {
      await metricsStore.fetchAccountQuotas();
      if (dialogueStore.activeAgentId && dialogueStore.activeProjectPath) {
        metricsStore
          .fetchActiveMetrics(
            dialogueStore.activeProjectPath,
            dialogueStore.activeAgentId,
            dialogueStore.activeConversationId || undefined
          )
          .catch(reportAsyncFailure);
      }
    })
  );

  unlisteners.push(
    listenDaemonEvent('todo-updated', async ({ payload }) => {
      const pPath =
        payload?.project_path ||
        (payload?.agent_id ? dialogueStore.resolveAgentProjectPath(payload.agent_id) : null) ||
        useCoreStore().activeProject?.path;
      if (pPath) await journalStore.fetchProjectTodos(pPath);
    })
  );

  unlisteners.push(
    listenDaemonEvent('tech-debt-updated', async ({ payload }) => {
      const pPath =
        payload?.project_path ||
        (payload?.agent_id ? dialogueStore.resolveAgentProjectPath(payload.agent_id) : null) ||
        useCoreStore().activeProject?.path;
      if (pPath) await journalStore.fetchProjectTechnicalDebt(pPath);
    })
  );

  unlisteners.push(
    listenDaemonEvent('DocumentCommentResolved', ({ payload }) => {
      explorerStore.applyDocumentCommentResolved(payload);
    })
  );

  unlisteners.push(
    listenDaemonEvent('agent-step-update', ({ payload }) => {
      if (!payload?.agent_id) return;
      if (!payload.project_path) {
        useCoreStore().addStatusLog(
          `ERROR: agent-step-update for '${payload.agent_id}' carries no project_path and was not attributed`
        );
        return;
      }
      const aid = payload.agent_id.toLowerCase();
      const stepKey = dialogueStore.getConversationKey(aid, payload.project_path);
      const prev = dialogueStore.activeSteps[stepKey];
      if (payload.state === 'DONE') {
        dialogueStore.clearActiveStep(aid, payload.project_path);
        const currentAid = dialogueStore.activeAgentId || layoutStore.floatingChatAgentId;
        const currentPath = currentAid ? dialogueStore.resolveAgentProjectPath(currentAid) : null;
        if (currentAid && currentPath && currentAid.toLowerCase() === aid && currentPath === payload.project_path) {
          metricsStore
            .fetchActiveMetrics(currentPath, currentAid, dialogueStore.activeConversationId || undefined)
            .catch(reportAsyncFailure);
        }
      } else {
        dialogueStore.activeSteps[stepKey] = {
          agentId: payload.agent_id,
          projectPath: payload.project_path,
          state: payload.state as 'ACTIVE' | 'DONE' | 'ERROR',
          stepType: payload.step_type,
          toolName: payload.tool_name ?? undefined,
          actionSummary: payload.action_summary,
          thoughtText: payload.thought_text ?? prev?.thoughtText ?? '',
          timestampMs: payload.timestamp_ms || Date.now(),
        };
      }
    })
  );

  unlisteners.push(
    listenDaemonEvent('init-flag-resolved', async ({ payload }) => {
      if (payload?.agent_id && payload.project_path) {
        dialogueStore.clearActiveStep(payload.agent_id, payload.project_path);
      }
      const coreStore = useCoreStore();
      const targetPid = payload?.project_id || coreStore.activeProject?.id;
      if (targetPid) await coreStore.fetchInitFlags(targetPid);
      const pPath =
        payload?.project_path ||
        (targetPid ? coreStore.projects.find(p => p.id === targetPid)?.path : null) ||
        dialogueStore.resolveAgentProjectPath(payload?.agent_id || dialogueStore.activeAgentId);
      if (pPath) {
        if (payload?.flag_type === 'tech_debt') await journalStore.fetchProjectTechnicalDebt(pPath);
        else if (payload?.flag_type === 'todos') await journalStore.fetchProjectTodos(pPath);
        else if (payload?.flag_type === 'docs') await explorerStore.fetchProjectDocuments(pPath);
      }
      emitUiEvent('oneship-refresh-project');
    })
  );

  unlisteners.push(
    listenDaemonEvent('keyring-warmup-status', async ({ payload }) => {
      await metricsStore.applyKeyringWarmupStatus(payload);
    })
  );

  unlisteners.push(
    listenDaemonEvent('agent-usage-update', ({ payload }) => {
      metricsStore.applyUsageUpdate(payload);
    })
  );

  unlisteners.push(
    listenDaemonEvent('agent-error', ({ payload }) => {
      if (!payload?.agent_id) return;
      if (!payload.project_path) {
        useCoreStore().addStatusLog(
          `ERROR: agent-error for '${payload.agent_id}' carries no project_path and was not attributed: ${payload.message || 'no message'}`
        );
        return;
      }
      const errKey = dialogueStore.getConversationKey(payload.agent_id, payload.project_path);
      dialogueStore.agentErrors[errKey] = {
        type: payload.error_type || 'process_exit',
        message: payload.message || 'Agent error encountered',
        exitCode: payload.exit_code,
        timestamp: payload.timestamp_ms || Date.now(),
      };
      const conv = dialogueStore.conversations[errKey];
      if (conv?.messages.length) {
        for (let i = conv.messages.length - 1; i >= 0; i--) {
          if (conv.messages[i].role === 'user') {
            dialogueStore.pendingRetryPrompts[errKey] = conv.messages[i].dialogue;
            break;
          }
        }
      }
    })
  );

  unlisteners.push(
    listenDaemonEvent('agent-status-report', ({ payload }) => {
      if (!payload?.agent_id || !payload.project_path) return;
      if (payload.status === 'working' || payload.status === 'idle') {
        dialogueStore.agentErrors[dialogueStore.getConversationKey(payload.agent_id, payload.project_path)] = null;
      }
    })
  );

  return unlisteners;
}

export function handleAgentResponseHelper(payload: AgentResponsePayload): void {
  const dialogueStore = useDialogueStore();
  const agentId = payload.agent_id;
  if (!payload.project_path) throw new Error(`AgentResponse from '${agentId}' carries no project_path`);
  const conv = dialogueStore.ensureConversation(agentId, payload.project_path);
  const convKey = dialogueStore.getConversationKey(agentId, payload.project_path);
  const isCurrentActive = dialogueStore.isCurrentActiveAgent(agentId, payload.project_path);
  dialogueStore.clearActiveStep(agentId, payload.project_path);

  if (payload.conversation_id) {
    conv.conversationId = payload.conversation_id;
    if (isCurrentActive) dialogueStore.activeConversationId = payload.conversation_id;
  }

  const artifact = buildResponseArtifact(payload);
  if (artifact?.filePath) {
    if (!conv.artifactsTouched) conv.artifactsTouched = [];
    if (!conv.artifactsTouched.includes(artifact.filePath)) conv.artifactsTouched.push(artifact.filePath);
    if (isCurrentActive && !dialogueStore.artifactsTouched.includes(artifact.filePath)) {
      dialogueStore.artifactsTouched.push(artifact.filePath);
    }
  }

  const approval: ApprovalRequestData | undefined = payload.approval
    ? {
        title: payload.approval.title,
        description: payload.approval.description,
        options: payload.approval.options ?? undefined,
        status: (payload.approval.status as ApprovalRequestData['status']) || 'pending',
      }
    : undefined;

  if (
    conv.messages.some(
      m => m.role === 'agent' && m.timestamp === payload.timestamp && m.dialogue === payload.dialogue
    )
  ) {
    return;
  }
  for (const m of conv.messages) {
    if (m.approval?.status === 'pending') m.approval.status = 'superseded';
  }

  conv.messages.push({
    id: `msg-${payload.timestamp}-${Math.random().toString(36).slice(2, 7)}`,
    turnId: payload.turn_id ?? undefined,
    role: 'agent',
    agentId,
    dialogue: payload.dialogue,
    artifact,
    approval,
    timestamp: payload.timestamp,
    conversationId: payload.conversation_id || conv.conversationId || undefined,
  });
  dialogueStore.conversations[convKey] = conv;

  if (isCurrentActive && artifact && !dialogueStore.activeArtifactId) {
    dialogueStore.activeArtifactId = artifact.id;
  }
  if (dialogueStore.isChatVisible(agentId, payload.project_path)) {
    conv.unread = 0;
  } else {
    conv.unread++;
  }

  const metricsStore = useAgentMetricsStore();
  metricsStore.fetchAccountQuotas().catch(reportAsyncFailure);
  if (isCurrentActive) {
    metricsStore
      .fetchActiveMetrics(
        payload.project_path,
        agentId,
        conv.conversationId || dialogueStore.activeConversationId || undefined
      )
      .catch(reportAsyncFailure);
  }
}

export async function sendUserMessageHelper(agentId: string, text: string, explicitProjectPath?: string): Promise<void> {
  if (!text.trim()) return;
  const dialogueStore = useDialogueStore();
  const layoutStore = useWorkspaceLayoutStore();
  const projectPath =
    explicitProjectPath ||
    (dialogueStore.activeAgentId?.toLowerCase() === agentId.toLowerCase()
      ? dialogueStore.activeProjectPath
      : null) ||
    (layoutStore.floatingChatAgentId?.toLowerCase() === agentId.toLowerCase()
      ? layoutStore.floatingChatProjectPath
      : null) ||
    dialogueStore.resolveAgentProjectPath(agentId);
  if (!projectPath && agentId.toLowerCase() !== 'system') {
    throw new Error(`Cannot send a message to agent '${agentId}': no project resolved`);
  }
  const conv = dialogueStore.ensureConversation(agentId, projectPath || '');
  for (const m of conv.messages) {
    if (m.approval?.status === 'pending') {
      m.approval.status = 'superseded';
      if (m.turnId) dialogueStore.updateTurnApproval(m.turnId, 'superseded').catch(reportAsyncFailure);
    }
  }
  const targetConversationId =
    conv.conversationId ||
    (dialogueStore.isCurrentActiveAgent(agentId, projectPath) ? dialogueStore.activeConversationId : null) ||
    '';
  const msg: DialogueMessage = {
    id: `msg-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
    role: 'user',
    agentId,
    dialogue: text.trim(),
    timestamp: Math.floor(Date.now() / 1000),
    conversationId: targetConversationId || undefined,
  };
  conv.messages.push(msg);
  dialogueStore.conversations[dialogueStore.getConversationKey(agentId, projectPath)] = conv;

  if (projectPath) {
    try {
      const savedTurn = await daemonCall('save_user_chat_turn', {
        conversationId: targetConversationId,
        projectPath,
        agentId,
        dialogue: text.trim(),
      });
      if (savedTurn) {
        if (savedTurn.id) msg.turnId = savedTurn.id;
        if (savedTurn.conversation_id) {
          msg.conversationId = savedTurn.conversation_id;
          conv.conversationId = savedTurn.conversation_id;
          if (dialogueStore.isCurrentActiveAgent(agentId, projectPath)) {
            dialogueStore.activeConversationId = savedTurn.conversation_id;
          }
        }
      }
    } catch (err) {
      console.warn('[dialogue] Failed to save user chat turn to SQLite:', err);
    }
  }
  await daemonCall('prompt_agent', { projectPath: projectPath ?? null, target: agentId, prompt: text.trim() }).catch(
    (err: unknown) => {
      const m = err instanceof Error ? err.message : String(err);
      useCoreStore().addStatusLog(`[Dialogue] Failed to dispatch prompt to agent '${agentId}': ${m}`);
    }
  );
}
