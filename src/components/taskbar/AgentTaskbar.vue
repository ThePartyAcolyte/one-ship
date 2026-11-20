<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue';
import { 
  Bot, X, ChevronUp, ChevronDown, Plus, Eye, EyeOff, 
  Play, Terminal, RotateCw, Trash2, SlidersHorizontal 
} from '@lucide/vue';
import { useDialogueStore, type AgentConversation } from '../../stores/dialogueStore';
import { useWorkspaceLayoutStore } from '../../stores/workspaceLayoutStore';
import { useAgentMetricsStore } from '../../stores/agentMetricsStore';
import { useCoreStore } from '../../stores/coreStore';
import { AppTooltip, AppContextMenu, AppAgentPortrait, type ContextMenuItem } from '../ui';
import AccountQuotaControls from './AccountQuotaControls.vue';
import { daemonCall } from '../../utils/daemonClient';
import { emitUiEvent } from '../../utils/uiEvents';
import type { AgentState } from '../../types';

const dialogueStore = useDialogueStore();
const layoutStore = useWorkspaceLayoutStore();
const metricsStore = useAgentMetricsStore();
const coreStore = useCoreStore();

const showAgentLauncher = ref(false);

const activeProject = computed(() => coreStore.activeProject);

function isEphemeralOrInit(id: string): boolean {
  const lower = id.toLowerCase();
  return (
    lower === 'initializer' ||
    lower === 'init' ||
    lower.endsWith('_resolver') ||
    lower.endsWith('-resolver') ||
    lower.endsWith('_crafter') ||
    lower.endsWith('-crafter') ||
    lower.startsWith('flag_resolution_') ||
    lower.startsWith('init_')
  );
}

function isSystemOrGit(id: string): boolean {
  const lower = id.toLowerCase();
  return lower === 'system' || lower === 'git';
}

// 1. Domain Project Specialists from rula.toml
const domainAgents = computed(() => {
  const list: AgentState[] = [];

  if (activeProject.value) {
    const st = coreStore.agentStates[activeProject.value.id];
    if (st?.agents) {
      for (const ag of st.agents) {
        if (ag && ag.id && !isEphemeralOrInit(ag.id) && !isSystemOrGit(ag.id)) {
          list.push(ag);
        }
      }
    }
  }

  return list;
});

// Helper: An approval is only actionable if it belongs to the LAST message of the conversation and was sent by the agent
function checkConversationPendingApproval(conv?: AgentConversation | null): boolean {
  if (!conv?.messages || conv.messages.length === 0) return false;
  const lastMsg = conv.messages[conv.messages.length - 1];
  return lastMsg?.role === 'agent' && lastMsg?.approval?.status === 'pending';
}

// 3. Ephemeral Flag Resolution Tasks (Rendered while process is active, pending approval, or uninitialized)
const activeResolvers = computed(() => {
  const list: AgentState[] = [];

  if (activeProject.value) {
    const st = coreStore.agentStates[activeProject.value.id];
    if (st?.agents) {
      for (const ag of st.agents) {
        if (ag && ag.id && isEphemeralOrInit(ag.id)) {
          const isLive = isAgentProcessActive(ag.id);
          const conv = dialogueStore.getConversation(ag.id, activeProject.value.path);
          const hasPending = checkConversationPendingApproval(conv);
          const isUninitializedInit = (ag.id.toLowerCase() === 'initializer' || ag.id.toLowerCase() === 'init') && !activeProject.value.has_config;
          if (isLive || hasPending || isUninitializedInit) {
            list.push(ag);
          }
        }
      }
    }
  }

  return list;
});

const totalAgentsCount = computed(() => {
  return domainAgents.value.length + activeResolvers.value.length;
});

function isAgentProcessActive(agentId: string): boolean {
  const lower = agentId.toLowerCase();
  if (activeProject.value) {
    const st = coreStore.agentStates[activeProject.value.id];
    const ag = st?.agents?.find((a) => a.id.toLowerCase() === lower);
    if (ag) {
      const s = ag.status?.toLowerCase();
      return s === 'working' || s === 'running' || s === 'idle' || s === 'starting' || s === 'blocked';
    }
  }
  return false;
}

function isAgentRunning(agent: { id?: string } | null | undefined): boolean {
  if (!agent?.id) return false;
  return isAgentProcessActive(agent.id);
}

function isAgentProcessWorking(agentId: string): boolean {
  const lower = agentId.toLowerCase();
  if (activeProject.value) {
    const st = coreStore.agentStates[activeProject.value.id];
    const ag = st?.agents?.find((a) => a.id.toLowerCase() === lower);
    if (ag) {
      const s = ag.status?.toLowerCase();
      return s === 'working';
    }
  }
  return false;
}

function getTabTooltip(conv: {
  agentId: string;
  unread: number;
  hasPendingApproval: boolean;
  isWorking: boolean;
  activeStepSummary?: string;
}): string {
  if (conv.hasPendingApproval) {
    return `${conv.agentId}: Pending approval required`;
  }
  if (conv.isWorking) {
    return conv.activeStepSummary 
      ? `${conv.agentId}: Working (${conv.activeStepSummary})` 
      : `${conv.agentId}: Agent is actively working`;
  }
  if (conv.unread > 0) {
    return `${conv.agentId}: ${conv.unread} unread response(s)`;
  }
  return `${conv.agentId}: Idle`;
}

function getAgentTransient(agentId: string): string | null {
  const outcome = coreStore.getAgentOutcome(agentId, activeProject.value?.path);
  if (!outcome) return null;
  const elapsed = Date.now() - outcome.timestamp;
  if (elapsed < 6000) {
    return outcome.status;
  }
  return null;
}

function getAgentActivityScore(agent: AgentState): number {
  if (agent.status === 'working') return 100;
  const transient = getAgentTransient(agent.id);
  if (transient === 'blocked') return 90;
  if (transient === 'error') return 80;
  if (transient === 'completed') return 70;
  if (isAgentRunning(agent)) return 60;
  return 10;
}

// Visibility toggle for domain agents with localStorage persistence
const showInactiveDomainAgents = ref(true);

function loadVisibilityPreferences() {
  if (!activeProject.value?.id) return;
  const val = localStorage.getItem(`oneship:visibility:taskbar_domain_agents:${activeProject.value.id}`);
  if (val !== null) showInactiveDomainAgents.value = val === 'true';
}

watch(() => activeProject.value?.id, () => {
  loadVisibilityPreferences();
}, { immediate: true });

function toggleShowInactiveDomainAgents() {
  showInactiveDomainAgents.value = !showInactiveDomainAgents.value;
  if (activeProject.value?.id) {
    localStorage.setItem(
      `oneship:visibility:taskbar_domain_agents:${activeProject.value.id}`,
      String(showInactiveDomainAgents.value)
    );
  }
}

const visibleDomainAgents = computed(() => {
  const list = showInactiveDomainAgents.value 
    ? domainAgents.value 
    : domainAgents.value.filter(ag => isAgentRunning(ag));
  return [...list].sort((a, b) => getAgentActivityScore(b) - getAgentActivityScore(a));
});

function isAgentBelongsToActiveProject(agentId: string): boolean {
  const lower = agentId.toLowerCase();
  if (lower === 'system') return true;
  if (!activeProject.value) return false;

  // If this agent is currently open in floating chat, ensure it was opened for the active project
  if (layoutStore.floatingChatAgentId?.toLowerCase() === lower && layoutStore.floatingChatProjectPath) {
    if (layoutStore.floatingChatProjectPath !== activeProject.value.path) {
      return false;
    }
  }

  const st = coreStore.agentStates[activeProject.value.id];
  if (st?.agents?.some((a) => a.id.toLowerCase() === lower)) {
    return true;
  }
  if (isEphemeralOrInit(lower)) {
    if (coreStore.initializingProjects[activeProject.value.id]) return true;
    if (lower === 'initializer' || lower === 'init') {
      return !activeProject.value.has_config;
    }
    if (isAgentProcessActive(lower)) return true;
    const flags = coreStore.projectInitFlags[activeProject.value.id] || [];
    return flags.some((f) => f.status === 'pending' || f.status === 'in_progress');
  }
  return false;
}

// Active conversation keys (strictly filtered to active agents of current project with live processes or open chat)
const activeConversations = computed(() => {
  const list: Array<{
    agentId: string;
    unread: number;
    hasPendingApproval: boolean;
    isWorking: boolean;
    activeStepSummary?: string;
    lastMessage?: string;
  }> = [];

  const candidateIds = new Set<string>();

  // 1. Live agents belonging to active project
  if (activeProject.value) {
    const st = coreStore.agentStates[activeProject.value.id];
    st?.agents?.forEach((ag) => {
      if (ag?.id && isAgentProcessActive(ag.id)) candidateIds.add(ag.id);
    });
  }

  // 2. Global system agent if live
  if (isAgentProcessActive('system')) {
    candidateIds.add('system');
  }

  // 3. Currently open/selected chat agent if it belongs to this project
  if (layoutStore.floatingChatAgentId && 
      (!layoutStore.floatingChatProjectPath || layoutStore.floatingChatProjectPath === activeProject.value?.path) &&
      isAgentBelongsToActiveProject(layoutStore.floatingChatAgentId)) {
    candidateIds.add(layoutStore.floatingChatAgentId);
  }

  // 4. Conversations belonging to this project with unread messages or pending approvals
  if (activeProject.value) {
    for (const ag of coreStore.agentStates[activeProject.value.id]?.agents ?? []) {
      const c = dialogueStore.getConversation(ag.id, activeProject.value.path);
      if (checkConversationPendingApproval(c) || (c?.unread || 0) > 0) {
        candidateIds.add(ag.id);
      }
    }
  }

  for (const agentId of candidateIds) {
    if (!isAgentBelongsToActiveProject(agentId)) {
      continue;
    }

    const isLive = isAgentProcessActive(agentId);
    const isCurrentChatOpen = layoutStore.isFloatingChatOpen && 
      (!layoutStore.floatingChatProjectPath || layoutStore.floatingChatProjectPath === activeProject.value?.path) &&
      layoutStore.floatingChatAgentId?.toLowerCase() === agentId.toLowerCase();
    const isVisibleInChat = isCurrentChatOpen && !layoutStore.isFloatingChatMinimized;
    const c = dialogueStore.getConversation(agentId, activeProject.value?.path);
    if (isVisibleInChat && c && c.unread > 0) {
      c.unread = 0;
    }
    const hasPendingApproval = checkConversationPendingApproval(c);
    const unreadCount = isVisibleInChat ? 0 : (c?.unread || 0);
    const hasUnread = unreadCount > 0;

    // Only render in taskbar if process is live, or chat is open/minimized in this project, or has pending approvals/unreads
    if (!isLive && !isCurrentChatOpen && !hasPendingApproval && !hasUnread) {
      continue;
    }

    const liveStep = dialogueStore.getActiveStep(agentId, activeProject.value?.path);
    const isProcessWorking = isAgentProcessWorking(agentId);
    const isWorking = (liveStep ? liveStep.state === 'ACTIVE' : false) || isProcessWorking;

    list.push({
      agentId,
      unread: unreadCount,
      hasPendingApproval,
      isWorking,
      activeStepSummary: liveStep?.actionSummary || (liveStep?.toolName ? `Tool: ${liveStep.toolName}` : undefined),
      lastMessage: c?.messages ? c.messages[c.messages.length - 1]?.dialogue : undefined,
    });
  }

  return list;
});

// Launch and Open Agent action
async function handleLaunchAndOpenAgent(agentId: string) {
  showAgentLauncher.value = false;
  layoutStore.openFloatingChat(agentId, activeProject.value?.path);

  // Optimistically mark agent as starting in coreStore
  if (activeProject.value?.id !== undefined) {
    const st = coreStore.agentStates[activeProject.value.id];
    const ag = st?.agents?.find((a) => a.id.toLowerCase() === agentId.toLowerCase());
    if (ag && (ag.status === 'unlaunched' || ag.status === 'unknown')) {
      ag.status = 'starting';
    }
  }

  // dialogueStore.openFloatingChat already initiates ensureAgentRunning
  if (activeProject.value && dialogueStore.isAgentLaunching(agentId, activeProject.value.path)) {
    return;
  }

  if (activeProject.value) {
    try {
      coreStore.addStatusLog(`Launching agent: ${agentId}...`);
      const msg = await daemonCall('launch_agent', {
        projectPath: activeProject.value.path,
        projectName: activeProject.value.name,
        agentId,
        runner: null,
        model: null,
        effort: null,
        accountId: null,
        cleanContext: null,
      });
      coreStore.addStatusLog(msg);
    } catch (err: unknown) {
      console.error('Failed to launch agent', err);
      coreStore.addStatusLog(`Launch error: ${err instanceof Error ? err.message : String(err)}`);
    }
  }
}

function handleAgentRowClick(agent: { id: string }) {
  if (isAgentRunning(agent)) {
    showAgentLauncher.value = false;
    layoutStore.openFloatingChat(agent.id, activeProject.value?.path);
  } else {
    handleLaunchAndOpenAgent(agent.id);
  }
}

async function handleStopAgent(agentId: string) {
  if (!activeProject.value) return;
  try {
    coreStore.addStatusLog(`Stopping agent: ${agentId}...`);
    const msg = await daemonCall('stop_agent', {
      projectPath: activeProject.value.path,
      agentId
    });
    coreStore.addStatusLog(msg);
  } catch (err: unknown) {
    console.error('Failed to stop agent', err);
    coreStore.addStatusLog(`Stop error: ${err instanceof Error ? err.message : String(err)}`);
  }
}

async function handleRestartAgent(agentId: string) {
  if (!activeProject.value) return;
  try {
    coreStore.addStatusLog(`Restarting agent: ${agentId}...`);
    const msg = await daemonCall('restart_agent', {
      projectPath: activeProject.value.path,
      projectName: activeProject.value.name,
      agentId,
      runner: null,
      model: null,
      effort: null,
      accountId: null,
      cleanContext: null,
    });
    coreStore.addStatusLog(msg);
  } catch (err: unknown) {
    console.error('Failed to restart agent:', err);
    coreStore.addStatusLog(`Restart error: ${err instanceof Error ? err.message : String(err)}`);
  }
}

function handleAddPersona() {
  showAgentLauncher.value = false;
  emitUiEvent('oneship-add-persona', activeProject.value ?? undefined);
}

// Right-click Context Menu State
const contextMenuVisible = ref(false);
const contextMenuX = ref(0);
const contextMenuY = ref(0);
const contextMenuItems = ref<ContextMenuItem[]>([]);

function openAgentContextMenu(e: MouseEvent, agent: { id: string; role?: string; runner?: string }) {
  e.preventDefault();
  e.stopPropagation();

  contextMenuX.value = e.clientX;
  contextMenuY.value = e.clientY;

  const items: ContextMenuItem[] = [];
  const running = isAgentRunning(agent);

  if (running) {
    items.push({
      label: 'Open Chat',
      icon: Terminal,
      action: () => {
        showAgentLauncher.value = false;
        layoutStore.openFloatingChat(agent.id, activeProject.value?.path);
      }
    });
    items.push({
      label: 'Restart Agent',
      icon: RotateCw,
      action: () => handleRestartAgent(agent.id)
    });
    items.push({
      label: 'Stop Agent',
      icon: Trash2,
      danger: true,
      action: () => handleStopAgent(agent.id)
    });
  } else {
    items.push({
      label: `Launch ${agent.id}`,
      icon: Play,
      action: () => handleLaunchAndOpenAgent(agent.id)
    });
  }

  items.push({
    separator: true,
    label: '',
    action: () => {}
  });

  items.push({
    label: 'Edit Persona',
    icon: SlidersHorizontal,
    action: () => {
      showAgentLauncher.value = false;
      if (activeProject.value) {
        emitUiEvent('oneship-edit-persona', { project: activeProject.value, agent });
      }
    }
  });

  contextMenuItems.value = items;
  contextMenuVisible.value = true;
}

function openTabContextMenu(e: MouseEvent, agentId: string) {
  let ag: AgentState | undefined = domainAgents.value.find(a => a.id.toLowerCase() === agentId.toLowerCase());
  if (!ag) {
    ag = activeResolvers.value.find(a => a.id.toLowerCase() === agentId.toLowerCase());
  }
  if (ag) {
    openAgentContextMenu(e, ag);
  } else {
    openAgentContextMenu(e, { id: agentId, role: 'Agent', runner: '' });
  }
}

function handleTabClick(agentId: string) {
  if (layoutStore.isFloatingChatOpen && 
      layoutStore.floatingChatAgentId === agentId &&
      (!layoutStore.floatingChatProjectPath || layoutStore.floatingChatProjectPath === activeProject.value?.path)) {
    layoutStore.toggleFloatingChatMinimize();
  } else {
    layoutStore.openFloatingChat(agentId, activeProject.value?.path);
  }
}

async function handleCloseTab(e: MouseEvent, agentId: string) {
  e.stopPropagation();
  await dialogueStore.closeAndKillAgent(activeProject.value?.path, agentId);
}

function handleDocumentClick(e: MouseEvent) {
  const target = e.target as HTMLElement;
  if (!target.closest('.agent-launcher-container') && !target.closest('.app-context-menu')) {
    showAgentLauncher.value = false;
  }
}

onMounted(() => {
  document.addEventListener('click', handleDocumentClick);
});

onUnmounted(() => {
  document.removeEventListener('click', handleDocumentClick);
});
</script>

<template>
  <div class="h-9 shrink-0 border-t border-[var(--border-color)] ui-surface-title flex items-center justify-between px-3 font-mono text-[10px] select-none z-30 gap-2">
    
    <!-- ── LEFT: Active Project & Agent Launcher Button ────────── -->
    <div class="flex items-center gap-1.5 shrink-0 relative agent-launcher-container">
      <AppTooltip text="Open Project Agents Launcher">
        <button
          @click.stop="showAgentLauncher = !showAgentLauncher"
          class="flex items-center gap-1.5 h-6 px-2 border transition-colors cursor-pointer"
          :class="showAgentLauncher 
            ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/20 text-[var(--accent-color)] font-bold' 
            : 'border-[var(--border-color)] bg-[var(--bg-color)] text-[var(--fg-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)]'"
        >
          <Bot :size="12" class="text-[var(--accent-color)]" />
          <span class="font-bold uppercase tracking-wider truncate max-w-[12rem]">{{ activeProject?.name || 'PROJECT' }}</span>
          <span class="text-[8px] px-1 py-0.2 ui-surface-title border border-[var(--border-color)] text-[var(--dim-color)]">
            {{ totalAgentsCount }} AGENTS
          </span>
          <ChevronUp v-if="showAgentLauncher" :size="10" />
          <ChevronDown v-else :size="10" />
        </button>
      </AppTooltip>

      <!-- Agent Launcher Popover Menu -->
      <div 
        v-if="showAgentLauncher"
        class="absolute bottom-full left-0 mb-1.5 w-84 max-w-[calc(100vw-4rem)] ui-surface-title border border-[var(--border-color)] shadow-2xl z-50 p-1 flex flex-col gap-1 font-mono text-[10px]"
      >
        <!-- Section 1: Project Domain Specialists -->
        <div class="px-2 py-1.5 text-[9px] text-[var(--dim-color)] uppercase tracking-wider border-b border-[var(--border-color)] flex justify-between items-center font-bold">
          <div class="flex items-center gap-1.5 truncate">
            <span class="text-[var(--fg-color)]">SPECIALISTS</span>
            <span class="text-[8px] opacity-50">({{ visibleDomainAgents.length }}/{{ domainAgents.length }})</span>
            <AppTooltip :text="showInactiveDomainAgents ? 'Hide inactive agents' : 'Show all agents'" position="top">
              <button 
                @click.stop="toggleShowInactiveDomainAgents"
                class="p-0.5 hover:text-[var(--accent-color)] transition-colors cursor-pointer inline-flex items-center ml-0.5"
              >
                <Eye v-if="showInactiveDomainAgents" :size="10" />
                <EyeOff v-else :size="10" class="text-[var(--dim-color)] opacity-50" />
              </button>
            </AppTooltip>
          </div>

          <!-- Add Persona Button (Minimalist pure '+') -->
          <AppTooltip text="Add Agent Persona" position="top">
            <button
              @click.stop="handleAddPersona"
              class="border border-dashed border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] px-1.5 py-0.2 text-[9px] transition-colors cursor-pointer flex items-center justify-center shrink-0"
            >
              <Plus :size="10" />
            </button>
          </AppTooltip>
        </div>

        <div class="flex flex-col max-h-56 overflow-y-auto">
          <div 
            v-for="ag in visibleDomainAgents" 
            :key="ag.id"
            @click="handleAgentRowClick(ag)"
            @contextmenu="openAgentContextMenu($event, ag)"
            class="flex items-center justify-between px-2 py-1.5 hover:bg-[var(--ui-bg-card)]/40 cursor-pointer group transition-colors text-left border-b border-[var(--border-color)]/30 last:border-b-0 gap-2 select-none"
            :class="ag.status === 'working' ? 'bg-[var(--accent-color)]/5' : ''"
          >
            <!-- Left: Portrait & Identity -->
            <div class="flex items-center gap-2 min-w-0 flex-1">
              <AppAgentPortrait :agent-id="ag.id" :dna="ag.dna" :size="20" class="shrink-0" />
              <div class="flex flex-col min-w-0 truncate">
                <div class="flex items-center gap-1.5 truncate">
                  <span 
                    class="w-1.5 h-1.5 rounded-full shrink-0"
                    :class="ag.status === 'working'
                      ? 'bg-[var(--accent-color)] animate-ping'
                      : (getAgentTransient(ag.id) === 'completed'
                          ? 'bg-[var(--accent-color)]'
                          : (getAgentTransient(ag.id) === 'blocked'
                              ? 'bg-[var(--status-warn-color)]'
                              : (getAgentTransient(ag.id) === 'error'
                                  ? 'bg-[var(--status-error-color)]'
                                  : (isAgentRunning(ag) ? 'bg-[var(--status-ok-color)]' : 'bg-transparent border border-[var(--dim-color)]'))))"
                  ></span>
                  <span class="font-bold text-[var(--fg-color)] group-hover:text-[var(--accent-color)] uppercase truncate">
                    {{ ag.id }}
                  </span>
                </div>
                <span class="text-[8px] text-[var(--dim-color)] truncate">{{ ag.role }}</span>
              </div>
            </div>

            <!-- Right: Status Badge & Runner -->
            <div class="flex items-center gap-1.5 shrink-0">
              <span v-if="ag.status === 'working'" class="text-[8px] text-[var(--accent-color)] font-bold shrink-0 animate-pulse">
                [WORKING]
              </span>
              <span v-else-if="isAgentRunning(ag)" class="text-[8px] shrink-0">
                <span v-if="getAgentTransient(ag.id) === 'completed'" class="text-[var(--accent-color)]">[✓ DONE]</span>
                <span v-else-if="getAgentTransient(ag.id) === 'blocked'" class="text-[var(--status-warn-color)]">[! INPUT]</span>
                <span v-else-if="getAgentTransient(ag.id) === 'error'" class="text-[var(--status-error-color)]">[✗ FAIL]</span>
                <span v-else class="text-[var(--status-ok-color)] opacity-80">[IDLE]</span>
              </span>
              <span v-else class="text-[8px] text-[var(--accent-color)] opacity-70 shrink-0">
                +LAUNCH
              </span>

              <span class="text-[8px] text-[var(--dim-color)] opacity-60 font-mono hidden sm:inline">
                {{ ag.runner || '—' }}
              </span>
            </div>
          </div>

          <div v-if="visibleDomainAgents.length === 0 && domainAgents.length > 0" class="px-2 py-2 text-[9px] text-[var(--dim-color)] italic opacity-50 text-center">
            Inactive agents hidden
          </div>
          <div v-else-if="domainAgents.length === 0" class="px-2 py-2 text-[9px] text-[var(--dim-color)] italic opacity-50 text-center">
            No personas configured
          </div>
        </div>

        <!-- Section 2: Active Resolution Tasks (Rendered only while active) -->
        <template v-if="activeResolvers.length > 0">
          <div class="px-2 py-1 text-[9px] text-[var(--status-warn-color)] uppercase tracking-wider border-t border-b border-[var(--status-warn-border)] flex justify-between items-center font-bold mt-1 bg-[var(--status-warn-bg)]">
            <span>Active Resolutions</span>
            <span class="animate-pulse">● RUNNING</span>
          </div>

          <div class="flex flex-col">
            <div 
              v-for="ag in activeResolvers" 
              :key="ag.id"
              @click="handleAgentRowClick(ag)"
              @contextmenu="openAgentContextMenu($event, ag)"
              class="flex items-center justify-between px-2 py-1.5 hover:bg-[var(--status-warn-bg)] cursor-pointer group transition-colors text-left border-b border-[var(--status-warn-border)] last:border-b-0 gap-2 select-none"
            >
              <div class="flex items-center gap-2 min-w-0 flex-1">
                <AppAgentPortrait :agent-id="ag.id" :dna="ag.dna" :size="20" class="shrink-0" />
                <div class="flex flex-col min-w-0 truncate">
                  <div class="flex items-center gap-1.5">
                    <span class="font-bold text-[var(--status-warn-color)] uppercase">
                      {{ ag.id }}
                    </span>
                    <span class="text-[8px] px-1 py-0.1 border border-[var(--status-warn-border)] text-[var(--status-warn-color)] uppercase animate-pulse">
                      {{ ag.status }}
                    </span>
                  </div>
                  <span class="text-[8px] text-[var(--dim-color)] truncate">{{ ag.role }}</span>
                </div>
              </div>
            </div>
          </div>
        </template>
      </div>
    </div>

    <!-- ── CENTER: Open / Active Conversation Tabs ────────────── -->
    <div class="flex-1 flex items-center gap-1.5 min-w-0 overflow-x-auto scrollbar-none px-1">
      <AppTooltip
        v-for="conv in activeConversations"
        :key="conv.agentId"
        :text="getTabTooltip(conv)"
        class="shrink-0"
      >
        <div 
          @click="handleTabClick(conv.agentId)"
          @contextmenu="openTabContextMenu($event, conv.agentId)"
          class="flex items-center gap-1.5 h-6 px-2 border transition-all cursor-pointer group shrink-0 max-w-[190px]"
          :class="[
            layoutStore.isFloatingChatOpen && 
            !layoutStore.isFloatingChatMinimized && 
            layoutStore.floatingChatAgentId === conv.agentId &&
            (!layoutStore.floatingChatProjectPath || layoutStore.floatingChatProjectPath === activeProject?.path)
              ? 'border-[var(--accent-color)] bg-[var(--accent-color)]/15 text-[var(--accent-color)] font-bold'
              : (conv.hasPendingApproval 
                  ? 'border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)] animate-pulse' 
                  : 'border-[var(--border-color)] bg-[var(--bg-color)]/80 text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--fg-color)]')
          ]"
        >
          <!-- Mini Avatar Portrait -->
          <AppAgentPortrait :agent-id="conv.agentId" :size="14" class="shrink-0" />

          <!-- Status dot or Working spinner -->
          <span 
            v-if="conv.isWorking" 
            class="w-1.5 h-1.5 rounded-full bg-[var(--status-ok-color)] animate-ping shrink-0" 
          />
          <span 
            v-else 
            class="w-1.5 h-1.5 rounded-full shrink-0" 
            :class="conv.hasPendingApproval ? 'bg-[var(--status-warn-color)]' : 'bg-[var(--accent-color)]'"
          />

          <span class="uppercase tracking-tight truncate text-[9px]">
            {{ conv.agentId }}
          </span>

          <!-- Urgent Pending Approval Badge -->
          <span 
            v-if="conv.hasPendingApproval" 
            class="px-1 py-0.2 bg-[var(--status-warn-color)] text-[var(--bg-color)] text-[8px] font-bold uppercase shrink-0"
          >
            ! APPROVAL
          </span>

          <!-- Unread Counter -->
          <span 
            v-else-if="conv.unread > 0"
            class="min-w-[12px] h-3 px-1 bg-[var(--accent-color)] text-black text-[8px] font-bold flex items-center justify-center rounded-xs shrink-0"
          >
            {{ conv.unread }}
          </span>

          <!-- Close tab button -->
          <button 
            @click="handleCloseTab($event, conv.agentId)"
            class="opacity-0 group-hover:opacity-100 hover:text-[var(--status-error-color)] p-0.5 transition-opacity cursor-pointer ml-0.5"
          >
            <X :size="9" />
          </button>
        </div>
      </AppTooltip>

      <div v-if="activeConversations.length === 0" class="text-[9px] text-[var(--dim-color)] italic opacity-40 px-2">
        No active agent conversations
      </div>
    </div>

    <!-- ── RIGHT: Metrics & Account Quota Controls ────────── -->
    <div class="flex items-center gap-2 shrink-0">
      <!-- Context threshold metric -->
      <div v-if="metricsStore.activeSessionMetrics?.context" class="hidden md:flex items-center gap-1 text-[8px] text-[var(--dim-color)] font-mono">
        <span>Ctx:</span>
        <span class="font-bold text-[var(--fg-color)]">
          {{ metricsStore.activeSessionMetrics.context.used_percent.toFixed(0) }}%
        </span>
      </div>

      <AccountQuotaControls />
    </div>

    <!-- Right-click Context Menu -->
    <AppContextMenu
      :show="contextMenuVisible"
      :x="contextMenuX"
      :y="contextMenuY"
      :items="contextMenuItems"
      @close="contextMenuVisible = false"
    />
  </div>
</template>
