import { reportAsyncFailure } from './reportAsyncFailure';
import { daemonCall } from './daemonClient';
import { chatTurnToMessage } from './dialogueFormat';
import { useCoreStore } from '../stores/coreStore';
import { useDialogueStore, type AgentChatTurn, type DialogueMessage } from '../stores/dialogueStore';
import { useWorkspaceLayoutStore } from '../stores/workspaceLayoutStore';
import { useAgentMetricsStore } from '../stores/agentMetricsStore';
import type { AgentConversationRecord } from '../types/generated/AgentConversationRecord';

const _launchingAgentIds = new Set<string>();

export function isAgentLaunchingHelper(agentId: string, projectPath: string): boolean {
  if (!agentId) return false;
  if (agentId.toLowerCase() === 'system') return _launchingAgentIds.has('system');
  if (!projectPath) return false;
  return _launchingAgentIds.has(useDialogueStore().getConversationKey(agentId, projectPath));
}

export function setAgentLaunchingHelper(agentId: string, projectPath: string, launching: boolean): void {
  const isSystem = agentId.toLowerCase() === 'system';
  if (!agentId || (!projectPath && !isSystem)) {
    throw new Error(`Cannot track the launch of '${agentId}' without a project`);
  }
  const key = isSystem ? 'system' : useDialogueStore().getConversationKey(agentId, projectPath);
  if (launching) _launchingAgentIds.add(key);
  else _launchingAgentIds.delete(key);
}

export async function loadActiveConversationHelper(projectPath: string, agentId: string): Promise<void> {
  if (!projectPath || !agentId) return;
  const dialogueStore = useDialogueStore();
  try {
    const conv: AgentConversationRecord = await daemonCall('get_active_conversation', {
      projectPath,
      agentId,
      runner: null,
      model: null,
      accountId: null,
      effort: null,
    });
    let parsedArtifacts: string[] = [];
    if (conv.artifacts_touched) {
      try {
        parsedArtifacts = JSON.parse(conv.artifacts_touched);
      } catch {
        parsedArtifacts = [];
      }
    }
    const rawTurns: AgentChatTurn[] = await daemonCall('get_agent_turns_continuous', {
      projectPath,
      agentId,
      limitCount: 10,
      beforeId: null,
    });
    const messages = rawTurns.map(chatTurnToMessage);
    for (let i = 0; i < messages.length - 1; i++) {
      if (messages[i].approval?.status === 'pending') {
        messages[i].approval!.status = 'superseded';
      }
    }
    const convObj = dialogueStore.ensureConversation(agentId, projectPath);
    convObj.conversationRecord = conv;
    convObj.conversationId = conv.id;
    convObj.topic = conv.topic;
    convObj.artifactsTouched = parsedArtifacts;
    convObj.messages = messages;
    convObj.oldestLoadedTurnId = rawTurns.length > 0 ? (rawTurns[0].id ?? null) : null;
    convObj.hasMoreHistory = rawTurns.length >= 5;
    dialogueStore.conversations[dialogueStore.getConversationKey(agentId, projectPath)] = convObj;

    if (dialogueStore.getActiveStep(agentId, projectPath)?.stepType === 'init') {
      dialogueStore.clearActiveStep(agentId, projectPath);
    }

    if (dialogueStore.isCurrentActiveAgent(agentId, projectPath)) {
      dialogueStore.activeConversationRecord = conv;
      dialogueStore.activeConversationId = conv.id;
      dialogueStore.activeTopic = conv.topic;
      dialogueStore.artifactsTouched = parsedArtifacts;
      dialogueStore.oldestLoadedTurnId = convObj.oldestLoadedTurnId ?? null;
      dialogueStore.hasMoreHistory = convObj.hasMoreHistory ?? false;
      const arts = messages.filter(m => m.artifact).map(m => m.artifact!);
      dialogueStore.activeArtifactId = arts.length > 0 ? arts[arts.length - 1].id : null;
      useAgentMetricsStore().fetchActiveMetrics(projectPath, agentId, conv.id).catch(reportAsyncFailure);
    }
  } catch (err) {
    console.error('[dialogue] Failed to load active conversation:', err);
  }
}

export async function loadOlderMessagesHelper(): Promise<void> {
  const dialogueStore = useDialogueStore();
  if (
    !dialogueStore.activeAgentId ||
    !dialogueStore.activeProjectPath ||
    !dialogueStore.oldestLoadedTurnId ||
    dialogueStore.isLoadingHistory
  ) {
    return;
  }
  const projectPath = dialogueStore.activeProjectPath;
  const agentId = dialogueStore.activeAgentId;
  dialogueStore.isLoadingHistory = true;
  try {
    const rawTurns: AgentChatTurn[] = await daemonCall('get_agent_turns_continuous', {
      projectPath,
      agentId,
      limitCount: 10,
      beforeId: dialogueStore.oldestLoadedTurnId,
    });
    const loadedConv = dialogueStore.ensureConversation(agentId, projectPath);
    if (rawTurns.length > 0) {
      const olderMessages = rawTurns.map(chatTurnToMessage);
      for (const m of olderMessages) {
        if (m.approval?.status === 'pending') m.approval.status = 'superseded';
      }
      loadedConv.messages = [...olderMessages, ...loadedConv.messages];
      loadedConv.oldestLoadedTurnId = rawTurns[0].id ?? null;
      loadedConv.hasMoreHistory = rawTurns.length >= 10;
    } else {
      loadedConv.hasMoreHistory = false;
    }
    if (dialogueStore.isCurrentActiveAgent(agentId, projectPath)) {
      dialogueStore.oldestLoadedTurnId = loadedConv.oldestLoadedTurnId ?? null;
      dialogueStore.hasMoreHistory = loadedConv.hasMoreHistory ?? false;
    }
  } catch (err) {
    console.error('[dialogue] Failed to load older messages:', err);
  } finally {
    dialogueStore.isLoadingHistory = false;
  }
}

export async function ensureAgentRunningHelper(agentId: string, projectPath?: string): Promise<void> {
  if (!agentId) return;
  const lower = agentId.toLowerCase();
  const coreStore = useCoreStore();
  const dialogueStore = useDialogueStore();
  if (lower === 'initializer' || lower.startsWith('resolve-') || lower.endsWith('-resolver')) return;
  const pPath = projectPath || dialogueStore.resolveAgentProjectPath(agentId);
  if (!pPath) return;
  const proj = coreStore.projects.find(p => p.path === pPath);
  if (!proj) return;
  const launchKey = dialogueStore.getConversationKey(agentId, proj.path);
  if (_launchingAgentIds.has(launchKey)) return;
  const s = coreStore.agentStates[proj.id]?.agents
    ?.find(a => a.id.toLowerCase() === lower)
    ?.status?.toLowerCase();
  if (s !== 'working' && s !== 'running' && s !== 'idle' && s !== 'starting' && s !== 'blocked') {
    _launchingAgentIds.add(launchKey);
    try {
      coreStore.addStatusLog(`Auto-launching agent '${agentId}'...`);
      coreStore.addStatusLog(
        await daemonCall('launch_agent', {
          projectPath: proj.path,
          projectName: proj.name,
          agentId,
          runner: null,
          model: null,
          effort: null,
          accountId: null,
          cleanContext: false,
        })
      );
      coreStore.agentStates[proj.id] = await daemonCall('get_project_agent_state', {
        projectPath: proj.path,
        projectName: proj.name,
      });
      await dialogueStore.loadActiveConversation(proj.path, agentId);
      await useAgentMetricsStore().fetchActiveMetrics(
        proj.path,
        agentId,
        dialogueStore.activeConversationId || undefined
      );
    } catch (err: unknown) {
      coreStore.addStatusLog(
        `Auto-launch error for agent '${agentId}': ${err instanceof Error ? err.message : String(err)}`
      );
    } finally {
      _launchingAgentIds.delete(launchKey);
    }
  }
}

export async function closeAndKillAgentHelper(projectPath?: string, specificAgentId?: string): Promise<void> {
  const dialogueStore = useDialogueStore();
  const layoutStore = useWorkspaceLayoutStore();
  const targetAgentId = specificAgentId || layoutStore.floatingChatAgentId || dialogueStore.activeAgentId;
  if (!targetAgentId) return;
  const isMatchingFloating =
    !specificAgentId ||
    (layoutStore.floatingChatAgentId?.toLowerCase() === targetAgentId.toLowerCase() &&
      (!projectPath ||
        !layoutStore.floatingChatProjectPath ||
        layoutStore.floatingChatProjectPath === projectPath));
  if (isMatchingFloating) layoutStore.closeFloatingChat();
  if (dialogueStore.isCurrentActiveAgent(targetAgentId, projectPath)) {
    dialogueStore.activeAgentId = null;
    dialogueStore.activeConversationId = null;
    dialogueStore.activeConversationRecord = null;
    dialogueStore.activeTopic = null;
    useAgentMetricsStore().activeSessionMetrics = null;
  }
  const coreStore = useCoreStore();
  if (!projectPath) {
    throw new Error(`Cannot stop agent '${targetAgentId}': no project was given`);
  }
  dialogueStore.clearActiveStep(targetAgentId, projectPath);
  const project = coreStore.projectByPath(projectPath);
  const st = project ? coreStore.agentStates[project.id] : null;
  const lowerId = targetAgentId.toLowerCase();
  const agIndex = st?.agents?.findIndex(a => a.id.toLowerCase() === lowerId) ?? -1;
  if (st?.agents && agIndex !== -1) {
    const isEphemeral =
      lowerId.endsWith('-resolver') ||
      lowerId.endsWith('_resolver') ||
      lowerId.endsWith('-crafter') ||
      lowerId.endsWith('_crafter') ||
      lowerId === 'initializer' ||
      lowerId === 'init';
    if (isEphemeral) st.agents.splice(agIndex, 1);
    else st.agents[agIndex].status = 'unlaunched';
  }
  try {
    await daemonCall('stop_agent', { projectPath: projectPath || '', agentId: targetAgentId });
  } catch (err) {
    console.warn(`[dialogueStore] Failed to stop agent ${targetAgentId}:`, err);
  }
}

export async function interruptAgentHelper(
  projectPath: string,
  agentId: string,
  correctivePrompt?: string
): Promise<void> {
  const dialogueStore = useDialogueStore();
  const trimmedPrompt = correctivePrompt?.trim() || '';
  if (trimmedPrompt) {
    const interruptConv = dialogueStore.ensureConversation(agentId, projectPath);
    const targetConversationId =
      interruptConv.conversationId ||
      (dialogueStore.isCurrentActiveAgent(agentId, projectPath)
        ? dialogueStore.activeConversationId
        : null) ||
      '';
    const msg: DialogueMessage = {
      id: `msg-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
      role: 'user',
      agentId,
      dialogue: trimmedPrompt,
      timestamp: Math.floor(Date.now() / 1000),
      conversationId: targetConversationId || undefined,
    };
    interruptConv.messages.push(msg);
    if (projectPath) {
      daemonCall('save_user_chat_turn', {
        conversationId: targetConversationId,
        projectPath,
        agentId,
        dialogue: trimmedPrompt,
      })
        .then(savedTurn => {
          if (!savedTurn) return;
          if (savedTurn.id) msg.turnId = savedTurn.id;
          if (savedTurn.conversation_id) {
            msg.conversationId = savedTurn.conversation_id;
            interruptConv.conversationId = savedTurn.conversation_id;
            if (dialogueStore.isCurrentActiveAgent(agentId, projectPath)) {
              dialogueStore.activeConversationId = savedTurn.conversation_id;
            }
          }
        })
        .catch(err => console.warn('[dialogue] Failed to save corrective turn to SQLite:', err));
    }
  }
  dialogueStore.clearActiveStep(agentId, projectPath);
  try {
    await daemonCall('interrupt_agent', {
      projectPath,
      agentId,
      correctivePrompt: trimmedPrompt || null,
    });
  } catch (err: unknown) {
    const m = err instanceof Error ? err.message : String(err);
    useCoreStore().addStatusLog(`[Dialogue] Failed to interrupt agent '${agentId}': ${m}`);
  }
}

export async function startCleanSessionHelper(
  projectPath: string,
  agentId: string,
  runner?: string,
  model?: string,
  accountId?: string,
  effort?: string
): Promise<void> {
  if (!projectPath || !agentId) return;
  const dialogueStore = useDialogueStore();
  const newConv: AgentConversationRecord = await daemonCall('create_agent_conversation', {
    projectPath,
    agentId,
    topic: 'New Session',
    runner: runner || null,
    model: model || null,
    accountId: accountId || null,
    effort: effort || null,
  });
  const convObj = dialogueStore.ensureConversation(agentId, projectPath);
  convObj.conversationRecord = newConv;
  convObj.conversationId = newConv.id;
  convObj.topic = newConv.topic;
  convObj.artifactsTouched = [];
  convObj.messages.push({
    id: `sys-${Date.now()}`,
    agentId,
    role: 'system',
    dialogue: 'Fresh conversation context initialized',
    timestamp: Math.floor(Date.now() / 1000),
    conversationId: newConv.id,
  });
  if (dialogueStore.isCurrentActiveAgent(agentId, projectPath)) {
    dialogueStore.activeConversationRecord = newConv;
    dialogueStore.activeConversationId = newConv.id;
    dialogueStore.activeTopic = newConv.topic;
    dialogueStore.artifactsTouched = [];
    dialogueStore.activeArtifactId = null;
  }
  const coreStore = useCoreStore();
  const proj = coreStore.projects.find(p => p.path === projectPath) || coreStore.activeProject;
  const projName = proj?.name || '';
  coreStore.addStatusLog(`Starting clean session for agent '${agentId}'...`);
  coreStore.addStatusLog(
    await daemonCall('restart_agent', {
      projectPath,
      projectName: projName,
      agentId,
      runner: runner || null,
      model: model || null,
      effort: effort || null,
      accountId: accountId || null,
      cleanContext: true,
    })
  );
  if (proj?.id) {
    coreStore.agentStates[proj.id] = await daemonCall('get_project_agent_state', {
      projectPath,
      projectName: projName,
    });
  }
}

export async function updateAgentExecutionConfigHelper(
  projectPath: string,
  agentId: string,
  runner?: string,
  model?: string,
  accountId?: string,
  effort?: string
): Promise<void> {
  if (!projectPath || !agentId) return;
  const dialogueStore = useDialogueStore();
  await dialogueStore.persistConversationConfig(projectPath, agentId, runner, model, accountId, effort);
  dialogueStore.clearAgentError(agentId, projectPath);
  const coreStore = useCoreStore();
  const proj = coreStore.projects.find(p => p.path === projectPath) || coreStore.activeProject;
  const projName = proj?.name || '';
  coreStore.addStatusLog(
    `Updating agent '${agentId}' configuration (${runner || 'default'}, ${model || 'default'}, ${effort || 'default'})...`
  );
  coreStore.addStatusLog(
    await daemonCall('restart_agent', {
      projectPath,
      projectName: projName,
      agentId,
      runner: runner || null,
      model: model || null,
      effort: effort || null,
      accountId: accountId || null,
      cleanContext: false,
    })
  );
  if (proj?.id) {
    coreStore.agentStates[proj.id] = await daemonCall('get_project_agent_state', {
      projectPath,
      projectName: projName,
    });
  }
  await dialogueStore.loadActiveConversation(projectPath, agentId);
  await useAgentMetricsStore().fetchActiveMetrics(
    projectPath,
    agentId,
    dialogueStore.activeConversationId || undefined
  );
  const retryKey = dialogueStore.getConversationKey(agentId, projectPath);
  const retryPrompt = dialogueStore.pendingRetryPrompts[retryKey];
  if (retryPrompt) {
    delete dialogueStore.pendingRetryPrompts[retryKey];
    await daemonCall('prompt_agent', { projectPath, target: agentId, prompt: retryPrompt }).catch(
      reportAsyncFailure
    );
  }
}
