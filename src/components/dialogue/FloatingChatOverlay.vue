<script setup lang="ts">
import { ref, computed, watch, nextTick, onUnmounted } from 'vue';
import { reportAsyncFailure } from '../../utils/reportAsyncFailure';
import { 
  Send, Bot, Paperclip, Play, Terminal, X, Minus, PanelRight, 
  Layers, ShieldAlert, Check, MessageSquare, Copy, RefreshCw,
  Mic, FileCode, FileText, Brain, RotateCcw, ArrowRightLeft, Square, Search
} from '@lucide/vue';
import { daemonCall } from '../../utils/daemonClient';
import { openUrl } from '@tauri-apps/plugin-opener';
import { marked } from 'marked';
import { AppTooltip, AppAgentPortrait, AppTextarea, AppExecutionConfigPicker, type ModelEffortOption } from '../ui';
import ContextTransferModal from './ContextTransferModal.vue';
import ConversationSearchModal from './ConversationSearchModal.vue';
import { useDialogueStore, type DialogueMessage } from '../../stores/dialogueStore';
import { useWorkspaceLayoutStore } from '../../stores/workspaceLayoutStore';
import { useProjectExplorerStore, type AttachedReviewFile } from '../../stores/projectExplorerStore';
import { useAgentMetricsStore } from '../../stores/agentMetricsStore';
import { useAccountStore } from '../../stores/useAccountStore';
import { useCoreStore } from '../../stores/coreStore';
import { useDictationStore } from '../../stores/dictationStore';
import { useRunnerCatalog } from '../../utils/runnerCatalog';
import { formatTokenCount } from '../../utils/format';

function renderDialogueMarkdown(content: string): string {
  if (!content) return '';
  try {
    return marked.parse(content, { breaks: true, gfm: true }) as string;
  } catch {
    return content;
  }
}

const dialogueStore = useDialogueStore();
const layoutStore = useWorkspaceLayoutStore();
const explorerStore = useProjectExplorerStore();
const metricsStore = useAgentMetricsStore();
const accountStore = useAccountStore();
const coreStore = useCoreStore();
const dictationStore = useDictationStore();

const messagesEnd = ref<HTMLElement | null>(null);
const messagesContainerRef = ref<HTMLElement | null>(null);
const terminalContainerRef = ref<HTMLElement | null>(null);
const thoughtBoxRef = ref<HTMLElement | null>(null);
const textareaRef = ref<{ focus: () => void; insertAtCursor?: (text: string) => void; $el?: HTMLElement } | null>(null);

const isChatNearBottom = ref(true);
const isTerminalNearBottom = ref(true);

const agentId = computed(() => layoutStore.floatingChatAgentId || dialogueStore.activeAgentId);
const conv = computed(() => {
  if (!agentId.value) return null;
  return dialogueStore.getConversation(agentId.value, activeProjectPath.value);
});

const inputText = computed({
  get: () => agentId.value ? dialogueStore.getAgentDraft(agentId.value, activeProjectPath.value) : '',
  set: (val: string) => {
    if (agentId.value) {
      dialogueStore.setAgentDraft(agentId.value, val, activeProjectPath.value);
    }
  },
});

const activeAgentInfo = computed(() => {
  if (!agentId.value) return null;
  if (activeProject.value?.id) {
    const state = coreStore.agentStates[activeProject.value.id];
    const found = state?.agents?.find(a => a.id.toLowerCase() === agentId.value?.toLowerCase());
    if (found) return found;
  }
  if (agentId.value.toLowerCase() === 'initializer' || agentId.value.toLowerCase() === 'init') {
    const initProj = activeProject.value?.id
      ? coreStore.initializingProjects[activeProject.value.id]
      : null;
    const isRunning = initProj?.isInitializing && initProj?.status === 'running';
    return {
      id: 'initializer',
      role: 'Project Initializer',
      status: isRunning ? 'working' : (coreStore.getAgentOutcome('initializer', activeProjectPath.value)?.status || 'completed'),
      // Unknown until the backend reports the agent state.
      runner: '',
      model: undefined,
      effort: undefined,
      dna: null,
      account_id: null,
      account_email: null,
    };
  }
  return null;
});

const isAgentUnlaunched = computed(() => {
  return !activeAgentInfo.value || activeAgentInfo.value.status === 'unlaunched';
});

const currentLiveStep = computed(() => {
  if (!agentId.value) return null;
  return dialogueStore.getActiveStep(agentId.value, activeProjectPath.value);
});

const voiceMicClasses = computed(() => {
  if (dictationStore.isActiveFor('chat')) {
    return 'text-[var(--status-error-color,#f43f5e)] opacity-100 animate-pulse';
  }
  if (dictationStore.state === 'processing') {
    return 'text-[var(--accent-color)] opacity-80 animate-pulse';
  }
  return 'text-[var(--dim-color)] opacity-25 hover:opacity-75';
});

const voiceMicTooltip = computed(() => {
  const autoSendSuffix = dictationStore.autoSend ? ' [Auto-send]' : '';
  if (dictationStore.isActiveFor('chat')) return `Recording voice...${autoSendSuffix}`;
  if (dictationStore.state === 'processing') return 'Finalizing dictation...';
  if (!dictationStore.configured) return 'Set Google AI Studio API Key in Settings';
  if (!dictationStore.available) return 'PipeWire audio utilities (pw-record) are not available';
  return `Dictate into the prompt [Alt+V]${autoSendSuffix}`;
});

async function onMicClick() {
  if (dictationStore.isActiveFor('chat')) {
    await dictationStore.stop();
    return;
  }
  textareaRef.value?.focus();
  const element = document.activeElement;
  if (element instanceof HTMLTextAreaElement || element instanceof HTMLInputElement) {
    await dictationStore.startDom('chat', element, {
      kind: 'chat',
      agentId: agentId.value,
      projectPath: activeProjectPath.value,
    });
  }
}

const isWorking = computed(() => {
  if (currentLiveStep.value && currentLiveStep.value.state === 'ACTIVE' && currentLiveStep.value.stepType !== 'init') {
    return true;
  }
  if (isAgentUnlaunched.value) return false;
  const status = activeAgentInfo.value?.status;
  if (status === 'completed' || status === 'failed') return false;
  return status === 'working';
});

const activeProjectPath = computed(() => {
  return layoutStore.floatingChatProjectPath || dialogueStore.resolveAgentProjectPath(agentId.value) || '';
});

const activeProject = computed(() => {
  const pPath = activeProjectPath.value;
  if (!pPath) return null;
  return coreStore.projects.find(p => p.path === pPath) ?? null;
});

watch(
  () => [activeProjectPath.value, agentId.value],
  async ([pPath, aId], [oldPath, oldAId]) => {
    if (pPath && aId && (pPath !== oldPath || aId !== oldAId)) {
      await dialogueStore.loadActiveConversation(pPath, aId);
      await metricsStore.fetchActiveMetrics(pPath, aId, dialogueStore.activeConversationId || undefined);
    }
  }
);

const savedMessagesScrollTop = ref<number | null>(null);

function onMessagesScroll(e: Event) {
  const el = e.target as HTMLElement;
  const thresholdPx = 45;
  isChatNearBottom.value = (el.scrollHeight - el.scrollTop - el.clientHeight) <= thresholdPx;
  savedMessagesScrollTop.value = el.scrollTop;
}

function onTerminalScroll(e: Event) {
  const el = e.target as HTMLElement;
  const thresholdPx = 45;
  isTerminalNearBottom.value = (el.scrollHeight - el.scrollTop - el.clientHeight) <= thresholdPx;
}

function scrollToBottom(force = false) {
  if (!force && !isChatNearBottom.value) return;
  nextTick(() => {
    if (messagesContainerRef.value) {
      messagesContainerRef.value.scrollTop = messagesContainerRef.value.scrollHeight;
    } else {
      messagesEnd.value?.scrollIntoView({ behavior: 'smooth' });
    }
  });
}

function scrollTerminalToBottom(force = false) {
  if (!force && !isTerminalNearBottom.value) return;
  nextTick(() => {
    if (terminalContainerRef.value) {
      terminalContainerRef.value.scrollTop = terminalContainerRef.value.scrollHeight;
    }
  });
}

watch(() => conv.value?.messages.length, () => {
  scrollToBottom();
});

watch(() => currentLiveStep.value?.thoughtText, () => {
  nextTick(() => {
    if (thoughtBoxRef.value) {
      thoughtBoxRef.value.scrollTop = thoughtBoxRef.value.scrollHeight;
    }
  });
  scrollToBottom();
});

watch(() => currentLiveStep.value?.actionSummary, () => {
  scrollToBottom();
});

watch(() => [agentId.value, layoutStore.isFloatingChatOpen], ([, open]) => {
  if (open) {
    isChatNearBottom.value = true;
    isTerminalNearBottom.value = true;
    scrollToBottom(true);
    scrollTerminalToBottom(true);
    nextTick(() => {
      textareaRef.value?.focus();
    });
    if (agentId.value && isAgentUnlaunched.value && !dialogueStore.isAgentLaunching(agentId.value, activeProjectPath.value)) {
      launchCurrentAgent().catch(reportAsyncFailure);
    }
  }
});

async function launchCurrentAgent() {
  if (!agentId.value) return;
  const currentAgentId = agentId.value;
  const launchProjectPath = activeProjectPath.value;
  if (dialogueStore.isAgentLaunching(currentAgentId, launchProjectPath)) return;
  dialogueStore.setAgentLaunching(currentAgentId, launchProjectPath, true);

  try {
    const proj = activeProject.value;
    if (proj) {
      try {
        coreStore.addStatusLog(`Launching agent '${currentAgentId}'...`);
        const msg = await daemonCall('launch_agent', {
          projectPath: proj.path,
          projectName: proj.name,
          agentId: currentAgentId,
          // Config is resolved by the backend (conversation override, then persona). Sending the
          // selector state here would pin a value the user never chose, e.g. the 'agy' default
          // shown before the persona has loaded.
          runner: null,
          model: null,
          effort: null,
          accountId: null,
          cleanContext: null,
        });
        coreStore.addStatusLog(msg);
      } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        coreStore.addStatusLog(`Launch error: ${msg}`);
      }
    }
  } finally {
    dialogueStore.setAgentLaunching(currentAgentId, launchProjectPath, false);
  }
}

function getAttachmentPlaceholder(file: AttachedReviewFile, index: number): string {
  const isImg = file.doc_type === 'image' || /\.(png|jpe?g|webp|gif|svg)$/i.test(file.name);
  const isDoc = file.category === 'docs' || (file.comment_count !== undefined && file.comment_count > 0);
  const all = explorerStore.attachedReviewFiles;
  if (isImg) {
    const imgIndex = all.slice(0, index + 1).filter(f => f.doc_type === 'image' || /\.(png|jpe?g|webp|gif|svg)$/i.test(f.name)).length;
    return `[IMG${imgIndex}]`;
  } else if (isDoc) {
    const docIndex = all.slice(0, index + 1).filter(f => f.category === 'docs' || (f.comment_count !== undefined && f.comment_count > 0)).length;
    return `[DOC${docIndex}]`;
  } else {
    const fileIndex = all.slice(0, index + 1).filter(f => !f.doc_type?.startsWith('image') && !/\.(png|jpe?g|webp|gif|svg)$/i.test(f.name) && f.category !== 'docs' && (!f.comment_count || f.comment_count === 0)).length;
    return `[FILE${fileIndex}]`;
  }
}

function onAttachmentDragStart(e: DragEvent, file: AttachedReviewFile, index: number) {
  const placeholder = getAttachmentPlaceholder(file, index);
  if (e.dataTransfer) {
    e.dataTransfer.setData('text/plain', placeholder);
    e.dataTransfer.effectAllowed = 'copy';
  }
}

function insertAttachmentPlaceholder(file: AttachedReviewFile, index: number) {
  const placeholder = getAttachmentPlaceholder(file, index);
  if (textareaRef.value?.insertAtCursor) {
    textareaRef.value.insertAtCursor(` ${placeholder} `);
  } else {
    inputText.value = (inputText.value ? `${inputText.value} ` : '') + `${placeholder} `;
  }
}

let isPasting = false;

async function handlePaste(event: ClipboardEvent) {
  if (isPasting || !activeProjectPath.value) return;

  const items = event.clipboardData?.items;
  const hasClipboardImage = items
    ? Array.from(items).some((item) => item.type.startsWith('image/'))
    : false;
  const hasClipboardFile = event.clipboardData?.files
    ? Array.from(event.clipboardData.files).some((file) => file.type.startsWith('image/'))
    : false;

  if (!hasClipboardImage && !hasClipboardFile) {
    return;
  }

  // Prevent default synchronously before reading clipboard via daemon (Wayland/X11)
  event.preventDefault();
  isPasting = true;
  try {
    await explorerStore.pasteClipboardImage(activeProjectPath.value);
  } finally {
    isPasting = false;
  }
}

async function openFilePicker() {
  if (!activeProjectPath.value) return;
  await explorerStore.openAndAttachFileDialog(activeProjectPath.value);
}

async function send() {
  if (!agentId.value) return;
  const rawText = inputText.value.trim();
  const attachedFiles = explorerStore.attachedReviewFiles;
  if (!rawText && attachedFiles.length === 0) return;

  if (isAgentUnlaunched.value) {
    const hasHistory = !!(conv.value && conv.value.messages.length > 0);
    await handleLaunchOrResume(!hasHistory);
  }

  await dialogueStore.submitAgentPrompt(agentId.value, inputText.value, activeProjectPath.value || undefined);
  inputText.value = '';
}

function handleStopTurn() {
  if (!agentId.value || !activeProjectPath.value) return;
  dialogueStore.interruptAgent(activeProjectPath.value, agentId.value);
}

async function handleMainAction() {
  if (!agentId.value) return;
  if (isWorking.value) {
    const rawText = inputText.value.trim();
    if (!activeProjectPath.value) return;
    if (rawText) {
      await dialogueStore.interruptAgent(activeProjectPath.value, agentId.value, rawText);
      inputText.value = '';
      explorerStore.clearAttachedReviewFiles();
    } else {
      handleStopTurn();
    }
    return;
  }
  await send();
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && isWorking.value) {
    e.preventDefault();
    handleStopTurn();
    return;
  }
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault();
    handleMainAction();
  }
}

function handleAcceptApproval(msg: DialogueMessage) {
  if (!msg.approval || !agentId.value) return;
  msg.approval.status = 'accepted';
  if (msg.turnId) {
    dialogueStore.updateTurnApproval(msg.turnId, 'accepted');
  }
  const confirmationText = `Proposal accepted: "${msg.approval.title}". Please proceed with the execution.`;
  dialogueStore.sendUserMessage(agentId.value, confirmationText, activeProjectPath.value || undefined);
}

function handleSelectOption(msg: DialogueMessage, option: string) {
  if (!msg.approval || !agentId.value) return;
  msg.approval.status = 'accepted';
  if (msg.turnId) {
    dialogueStore.updateTurnApproval(msg.turnId, 'accepted', option);
  }
  const confirmationText = `Selected option: "${option}". Please proceed with this execution.`;
  dialogueStore.sendUserMessage(agentId.value, confirmationText, activeProjectPath.value || undefined);
}

function handleRequestChanges(msg: DialogueMessage) {
  if (!msg.approval) return;
  inputText.value = `Regarding proposal "${msg.approval.title}": `;
  textareaRef.value?.focus();
}

function handleSelectArtifact(artifactId: string) {
  const proj = activeProject.value;
  dialogueStore.selectArtifact(artifactId, proj?.path);
  const art = dialogueStore.activeArtifact;
  layoutStore.navigateTo({
    projectId: proj?.id,
    projectPath: proj?.path,
    tab: 'files',
    artifactId,
    filePath: art?.filePath,
  });
}

async function handleChatMarkdownClick(e: MouseEvent) {
  const target = (e.target as HTMLElement).closest('a');
  if (!target) return;
  const href = target.getAttribute('href');
  if (!href) return;

  e.preventDefault();
  e.stopPropagation();

  if (href.startsWith('http://') || href.startsWith('https://')) {
    try {
      await openUrl(href);
    } catch (err) {
      console.error('[FloatingChatOverlay] Failed to open external URL:', err);
    }
    return;
  }

  const cleanHref = href.replace(/^#/, '');
  const matchedArt = dialogueStore.activeConversationArtifacts.find(a => a.id === cleanHref);
  if (matchedArt) {
    handleSelectArtifact(matchedArt.id);
    return;
  }

  const proj = activeProject.value;
  let relPath = cleanHref.startsWith('file://') ? cleanHref.slice('file://'.length).trim() : cleanHref.trim();
  if (proj && relPath.startsWith(`${proj.path}/`)) {
    relPath = relPath.slice(proj.path.length + 1);
  }
  if (proj && relPath) {
    layoutStore.navigateTo({
      projectId: proj.id,
      projectPath: proj.path,
      tab: 'files',
      filePath: relPath,
    });
  }
}

// In-window View Mode: 'messages' | 'terminal'
const chatViewMode = ref<'messages' | 'terminal'>('messages');
const terminalOutput = ref('');
const terminalFilter = ref<'all' | 'errors_only'>('all');
const terminalCopied = ref(false);
let terminalCopyTimer: ReturnType<typeof setTimeout> | null = null;
let terminalPollInterval: ReturnType<typeof setInterval> | null = null;

async function fetchTerminalOutput() {
  if (!agentId.value) return;
  try {
    const out = await daemonCall('read_terminal_output', {
      projectPath: activeProjectPath.value ?? null,
      paneId: agentId.value,
      linesCount: 250,
      filter: terminalFilter.value === 'all' ? 'all' : 'errors_only',
    });
    terminalOutput.value = out || `// No live output yet for agent '${agentId.value}'`;
    scrollTerminalToBottom();
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    terminalOutput.value = `// Failed to read terminal output for '${agentId.value}':\n// ${msg}`;
    scrollTerminalToBottom();
  }
}

function toggleTerminalFilter() {
  terminalFilter.value = terminalFilter.value === 'all' ? 'errors_only' : 'all';
  fetchTerminalOutput();
}

async function copyTerminalOutput() {
  if (!terminalOutput.value) return;
  try {
    await navigator.clipboard.writeText(terminalOutput.value);
    terminalCopied.value = true;
    if (terminalCopyTimer) clearTimeout(terminalCopyTimer);
    terminalCopyTimer = setTimeout(() => {
      terminalCopied.value = false;
    }, 2000);
  } catch (e) {
    console.error('Failed to copy terminal output:', e);
  }
}

watch(
  () => [chatViewMode.value, agentId.value],
  ([mode]) => {
    if (mode === 'terminal') {
      isTerminalNearBottom.value = true;
      fetchTerminalOutput();
      scrollTerminalToBottom(true);
      if (terminalPollInterval) clearInterval(terminalPollInterval);
      terminalPollInterval = setInterval(fetchTerminalOutput, 2000);
    } else {
      if (terminalPollInterval) {
        clearInterval(terminalPollInterval);
        terminalPollInterval = null;
      }
      nextTick(() => {
        if (!messagesContainerRef.value) return;
        if (isChatNearBottom.value) {
          scrollToBottom(true);
        } else if (savedMessagesScrollTop.value !== null) {
          messagesContainerRef.value.scrollTop = savedMessagesScrollTop.value;
        }
      });
    }
  },
  { immediate: true }
);

// ── IN-CHAT AGENT LIFECYCLE & EXECUTION TOOLBOX ──────────────────────────
// Runners, models and efforts come only from the runner manifests.
const runnerCatalog = useRunnerCatalog();
const runnerOptions = computed(() =>
  runnerCatalog.runners.value.map(r => ({ label: r.id.toUpperCase(), value: r.id })),
);

const selectedRunner = ref<string>('');
const selectedModel = ref<string>('');
const selectedEffort = ref<string>('');
const selectedAccountId = ref<string>('');
const availableModels = ref<ModelEffortOption[]>([]);
const isApplyingConfig = ref(false);
const isCleaningSession = ref(false);
const showContextTransferModal = ref(false);
const showConversationSearch = ref(false);

// Hydrations overlap while the conversation record loads in several steps; a boolean would be
// cleared by the first one to finish while a later one still assigns values, and those
// assignments would then be mistaken for user edits and restart the agent.
const hydrationsInFlightCount = ref(0);
const isHydratingConfig = computed(() => hydrationsInFlightCount.value > 0);
const isSwitchingRunner = ref(false);
const isEditingExecutionConfig = ref(false);
const pendingConfigUpdate = ref(false);

/** Models of `runner` from its manifest; a catalog load failure is reported, never replaced. */
async function loadModelsForRunner(runner: string) {
  try {
    await runnerCatalog.loadRunnerCatalog();
  } catch (e: unknown) {
    coreStore.addStatusLog(`[ERROR] Cannot load the runner catalog: ${String(e)}`);
  }
  availableModels.value = runnerCatalog.modelsOf(runner).map(m => ({
    id: m.id,
    label: m.label,
    default_effort: m.default_effort ?? undefined,
    effort_levels: m.effort_levels,
  }));
}

/** True when the selected runner/model/effort is declared by the runner's manifest. */
const isSelectedPairDeclared = computed(() =>
  runnerCatalog.isDeclaredPair(selectedRunner.value, selectedModel.value, selectedEffort.value || null),
);

const recommendedAccountData = computed(() => {
  return metricsStore.getRecommendedAccountForModel(
    accountStore.accounts,
    selectedRunner.value,
    selectedModel.value
  );
});

const accountOptions = computed(() => {
  const { accountQuotasMap } = recommendedAccountData.value;
  if (selectedRunner.value === 'opencode' || selectedRunner.value === 'claude') {
    const isClaude = selectedRunner.value === 'claude';
    const providerAccounts = accountStore.accounts.filter(a => a.provider === (isClaude ? 'claude' : 'deepseek'));
    if (providerAccounts.length === 0) {
      return [{ label: isClaude ? 'Active Claude Login' : 'Default DeepSeek Key', value: '' }];
    }
    return providerAccounts.map(a => {
      const q = accountQuotasMap[a.id];
      const quotaLabel = q ? ` [${q.pct}%]` : '';
      return {
        label: `${a.email || a.label || (isClaude ? 'Claude Account' : 'DeepSeek Account')}${quotaLabel}`,
        value: a.id,
      };
    });
  } else {
    const googleAccounts = accountStore.accounts.filter(a => !a.provider || a.provider === 'antigravity' || a.provider === 'google');
    if (googleAccounts.length === 0) {
      return [{ label: 'Active Keyring Account', value: '' }];
    }
    return googleAccounts.map(a => {
      const q = accountQuotasMap[a.id];
      let quotaLabel = '';
      if (q) {
        quotaLabel = q.resetsIn ? ` [${q.pct}% • resets ${q.resetsIn}]` : ` [${q.pct}%]`;
      }
      return {
        label: `${a.email || a.label || 'Google Account'}${quotaLabel}`,
        value: a.id,
      };
    });
  }
});

const activeAgentError = computed(() => {
  if (!agentId.value) return null;
  return dialogueStore.getAgentError(agentId.value, activeProjectPath.value);
});

const alternativeAccounts = computed(() => {
  const recId = recommendedAccountData.value.recommendedAccountId;
  return accountOptions.value
    .filter(opt => opt.value !== selectedAccountId.value && opt.value !== '')
    .map(opt => ({
      ...opt,
      isRecommended: opt.value === recId,
    }))
    .sort((a, b) => (b.isRecommended ? 1 : 0) - (a.isRecommended ? 1 : 0));
});

/** Lightest model of the selected runner (its manifest's `quota_fallback_model`), offered as the
 *  quick switch when a heavier model hits its quota. */
const lightModel = computed(() => {
  const runner = runnerCatalog.findRunner(selectedRunner.value);
  return runner ? runnerCatalog.findModel(runner.id, runner.quota_fallback_model) ?? null : null;
});

const canSwitchToFlashModel = computed(() => !!lightModel.value && selectedModel.value !== lightModel.value.id);

const flashModelName = computed(() => lightModel.value?.label ?? '');

async function resolveErrorWithAccount(accId: string) {
  selectedAccountId.value = accId;
  dialogueStore.clearAgentError(agentId.value!, activeProjectPath.value);
  await triggerAutoApply();
}

async function resolveErrorWithFlashModel() {
  if (!lightModel.value) return;
  selectedModel.value = lightModel.value.id;
  selectedEffort.value = lightModel.value.default_effort ?? '';
  dialogueStore.clearAgentError(agentId.value!, activeProjectPath.value);
  await triggerAutoApply();
}

async function retryAfterError() {
  const m = availableModels.value.find(item => item.id === selectedModel.value);
  if (m?.effort_levels && m.effort_levels.length > 0 && !selectedEffort.value) {
    selectedEffort.value = m.default_effort ?? '';
  }
  dialogueStore.clearAgentError(agentId.value!, activeProjectPath.value);
  await triggerAutoApply();
}

async function retryLastPrompt() {
  if (!agentId.value) return;
  await dialogueStore.retryLastAgentPrompt(agentId.value, activeProjectPath.value || undefined);
}

async function handleLaunchOrResume(isClean: boolean) {
  if (!agentId.value || !activeProjectPath.value) return;
  const proj = coreStore.projects.find(p => p.path === activeProjectPath.value) || coreStore.activeProject;
  const projName = proj?.name || '';
  coreStore.addStatusLog(`${isClean ? 'Starting clean session' : 'Resuming session'} for agent '${agentId.value}'...`);

  try {
    const res = await daemonCall('launch_agent', {
      projectPath: activeProjectPath.value,
      projectName: projName,
      agentId: agentId.value,
      // User choices are persisted by the selector watchers (triggerAutoApply); the backend reads
      // them from the Conversation.
      runner: null,
      model: null,
      effort: null,
      accountId: null,
      cleanContext: isClean,
    });
    coreStore.addStatusLog(res);
    if (proj?.id) {
      const st = await daemonCall('get_project_agent_state', {
        projectPath: activeProjectPath.value,
        projectName: projName,
      });
      coreStore.agentStates[proj.id] = st;
    }
    await dialogueStore.loadActiveConversation(activeProjectPath.value, agentId.value);
    await metricsStore.fetchActiveMetrics(activeProjectPath.value, agentId.value, dialogueStore.activeConversationId || undefined);
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    coreStore.addStatusLog(`Failed to launch agent: ${msg}`);
  }
}

async function handleCompactContext() {
  if (!agentId.value) return;
  await dialogueStore.compactContextDirective(agentId.value, activeProjectPath.value || undefined);
}

watch(activeAgentError, (err) => {
  if (err) {
    scrollToBottom(true);
  }
});

async function hydrateExecutionConfig() {
  hydrationsInFlightCount.value += 1;
  const info = activeAgentInfo.value;
  const conv = dialogueStore.activeConversationRecord;
  // Same pairing as the backend launch: a model belongs to its runner, so the agent's own model
  // never pairs with a Conversation runner override. No pair = no preselected model.
  const r = conv?.runner || info?.runner || '';
  const runnerOverridden = !!conv?.runner && conv.runner !== info?.runner;
  selectedRunner.value = r;
  await loadModelsForRunner(r);
  selectedModel.value = conv?.model || (runnerOverridden ? '' : info?.model) || '';
  const effort = conv?.effort || (runnerOverridden || conv?.model ? '' : info?.effort) || '';
  const curModel = availableModels.value.find(m => m.id === selectedModel.value);
  selectedEffort.value = effort && curModel?.effort_levels?.includes(effort) ? effort : (curModel?.default_effort ?? '');
  selectedAccountId.value = conv?.account_id || info?.account_id || '';
  nextTick(() => {
    hydrationsInFlightCount.value -= 1;
  });
}

async function handleExecutionConfigCancel() {
  isEditingExecutionConfig.value = false;
  await hydrateExecutionConfig();
}

watch(
  () => [
    agentId.value,
    activeAgentInfo.value?.runner,
    activeAgentInfo.value?.model,
    activeAgentInfo.value?.effort,
    activeAgentInfo.value?.account_id,
    dialogueStore.activeConversationRecord?.id,
    dialogueStore.activeConversationRecord?.runner,
    dialogueStore.activeConversationRecord?.model,
    dialogueStore.activeConversationRecord?.effort,
    dialogueStore.activeConversationRecord?.account_id,
  ],
  async () => {
    // Pause background hydration while the user is mid-wizard (e.g. switching runner -> picking model).
    if (isEditingExecutionConfig.value) return;
    await hydrateExecutionConfig();
  },
  { immediate: true }
);

watch(selectedRunner, async (newRunner) => {
  if (isHydratingConfig.value) return;
  isSwitchingRunner.value = true;
  await loadModelsForRunner(newRunner);
  const validAcc = accountOptions.value.find(opt => opt.value === selectedAccountId.value);
  if (!validAcc) {
    selectedAccountId.value = accountOptions.value[0]?.value || '';
  }
  // A model of the old runner does not belong to the new one: the user picks one of its models,
  // and nothing is applied until the pair is complete.
  if (!availableModels.value.some(m => m.id === selectedModel.value)) {
    selectedModel.value = '';
    selectedEffort.value = '';
  }
  isSwitchingRunner.value = false;
  await triggerAutoApply();
});

watch(selectedModel, async (newModel, oldModel) => {
  if (isHydratingConfig.value || isSwitchingRunner.value || !newModel || newModel === oldModel) return;
  const m = availableModels.value.find(item => item.id === newModel);
  if (m?.effort_levels && m.effort_levels.length > 0) {
    if (!selectedEffort.value || !m.effort_levels.includes(selectedEffort.value)) {
      selectedEffort.value = m.default_effort ?? '';
    }
  } else {
    selectedEffort.value = '';
  }
  await triggerAutoApply();
});

watch(selectedEffort, async (newEffort, oldEffort) => {
  if (isHydratingConfig.value || isSwitchingRunner.value || newEffort === oldEffort) return;
  await triggerAutoApply();
});

watch(selectedAccountId, async (newAcc, oldAcc) => {
  if (isHydratingConfig.value || isSwitchingRunner.value || newAcc === oldAcc) return;
  await triggerAutoApply();
});

async function triggerAutoApply() {
  if (!agentId.value || isHydratingConfig.value || isSwitchingRunner.value) return;
  // Only a pair the runner manifest declares is stored or applied.
  if (!isSelectedPairDeclared.value) return;

  await dialogueStore.persistConversationConfig(
    activeProjectPath.value,
    agentId.value,
    selectedRunner.value,
    selectedModel.value,
    selectedAccountId.value || undefined,
    selectedEffort.value || undefined
  );

  if (isAgentUnlaunched.value) {
    return;
  }

  if (isWorking.value) {
    pendingConfigUpdate.value = true;
    coreStore.addStatusLog(`Agent '${agentId.value}' is busy. Configuration update queued.`);
    return;
  }

  pendingConfigUpdate.value = false;
  await applyExecutionConfig();
}

watch(isWorking, async (working) => {
  if (!working && pendingConfigUpdate.value) {
    pendingConfigUpdate.value = false;
    await applyExecutionConfig();
  }
});

async function applyExecutionConfig() {
  if (!agentId.value || isApplyingConfig.value) return;
  isApplyingConfig.value = true;
  try {
    await dialogueStore.updateAgentExecutionConfig(
      activeProjectPath.value,
      agentId.value,
      selectedRunner.value,
      selectedModel.value,
      selectedAccountId.value || undefined,
      selectedEffort.value || undefined
    );
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    coreStore.addStatusLog(`Error updating config: ${msg}`);
  } finally {
    isApplyingConfig.value = false;
  }
}

async function handleCleanSession() {
  if (!agentId.value || isCleaningSession.value) return;
  isCleaningSession.value = true;
  try {
    await dialogueStore.startCleanSession(
      activeProjectPath.value,
      agentId.value,
      selectedRunner.value,
      selectedModel.value,
      selectedAccountId.value || undefined,
      selectedEffort.value || undefined
    );
    coreStore.addStatusLog(`Clean session started for agent '${agentId.value}'`);
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    coreStore.addStatusLog(`Error starting clean session: ${msg}`);
  } finally {
    isCleaningSession.value = false;
  }
}

const projectAgentsList = computed(() => {
  const proj = activeProject.value;
  if (!proj) return [];
  const state = coreStore.agentStates[proj.id];
  if (!state?.agents) return [];
  return state.agents.map(a => ({
    id: a.id,
    role: a.role,
  }));
});

async function handleContextTransfer(payload: { targetAgentId: string; handoffMessage: string }) {
  showContextTransferModal.value = false;
  try {
    const { targetAgentId, handoffMessage } = payload;
    coreStore.addStatusLog(`Transferring context from '${agentId.value}' to '${targetAgentId}'...`);

    layoutStore.openFloatingChat(targetAgentId, activeProjectPath.value || undefined);

    const proj = activeProject.value;
    const targetState = proj ? coreStore.agentStates[proj.id]?.agents?.find(a => a.id.toLowerCase() === targetAgentId.toLowerCase()) : null;
    if ((!targetState || targetState.status === 'unlaunched') && !dialogueStore.isAgentLaunching(targetAgentId, activeProjectPath.value)) {
      if (proj) {
        await daemonCall('launch_agent', {
          projectPath: proj.path,
          projectName: proj.name,
          agentId: targetAgentId,
          runner: null,
          model: null,
          effort: null,
          accountId: null,
          cleanContext: null,
        });
      }
    }

    dialogueStore.sendUserMessage(targetAgentId, handoffMessage, activeProjectPath.value || undefined);
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    coreStore.addStatusLog(`Error during context transfer: ${msg}`);
  }
}

// ── TOKEN USAGE & CONTEXT TELEMETRY ─────────────────────────────────────
const sessionMetrics = computed(() => metricsStore.activeSessionMetrics);

const isCurrentRunnerOpenCode = computed(() => {
  const r = activeAgentInfo.value?.runner || dialogueStore.activeConversationRecord?.runner || selectedRunner.value;
  return r === 'opencode';
});

const openCodeTokensUsed = computed(() => {
  return sessionMetrics.value?.tokens_used_count ?? 0;
});

const openCodeCostFormatted = computed(() => {
  const cost = sessionMetrics.value?.cost_dollars;
  if (cost === undefined || cost === null) return null;
  return `$${cost.toFixed(4)}`;
});

const contextUsedPct = computed(() => {
  if (sessionMetrics.value?.context?.used_percent != null && sessionMetrics.value.context.used_percent > 0) {
    return sessionMetrics.value.context.used_percent;
  }
  const used = sessionMetrics.value?.tokens_used_count;
  const max = sessionMetrics.value?.max_context_tokens_count || (isCurrentRunnerOpenCode.value ? 131072 : 0);
  if (used != null && max > 0) {
    return Math.min(100, (used / max) * 100);
  }
  return 0;
});

const totalTokens = computed(() => {
  const cache = sessionMetrics.value?.context?.cache;
  if (cache) {
    return (cache.fresh_input_tokens_count || 0) + (cache.read_tokens_count || 0) + (cache.creation_tokens_count || 0);
  }
  return sessionMetrics.value?.tokens_used_count ?? 0;
});

const contextBadgeClass = computed(() => {
  const pct = contextUsedPct.value;
  if (pct >= 85) return 'border-[var(--status-error-border)] bg-[var(--status-error-bg)] text-[var(--status-error-color)]';
  if (pct >= 60) return 'border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)]';
  return 'border-[var(--border-color)] bg-[var(--ui-bg-card)] text-[var(--fg-color)]';
});

const contextBarClass = computed(() => {
  const pct = contextUsedPct.value;
  if (pct >= 85) return 'bg-[var(--status-error-color)]';
  if (pct >= 60) return 'bg-[var(--status-warn-color)]';
  return 'bg-[var(--accent-color)]';
});

const contextTooltip = computed(() => {
  const m = sessionMetrics.value;
  if (isCurrentRunnerOpenCode.value) {
    const used = m?.tokens_used_count ?? 0;
    const max = m?.max_context_tokens_count || 131072;
    const pct = ((used / max) * 100).toFixed(1);
    return `OpenCode Context: ${formatTokenCount(used)} / ${formatTokenCount(max)} tokens (${pct}% used)`;
  }
  if (!m?.context) return 'Context window usage: No telemetry yet';
  const remaining = m.context.remaining_percent.toFixed(1);
  const used = m.context.used_percent.toFixed(1);
  return `Context Window: ${used}% used, ${remaining}% remaining`;
});

const tokenBreakdownTooltip = computed(() => {
  if (isCurrentRunnerOpenCode.value) {
    const cost = openCodeCostFormatted.value || '$0.0000';
    const used = formatTokenCount(openCodeTokensUsed.value);
    return `DeepSeek Usage: ${used} tokens | Cost: ${cost} USD`;
  }
  const cache = sessionMetrics.value?.context?.cache;
  if (!cache) return 'Token telemetry: awaiting agent turn';
  const fresh = formatTokenCount(cache.fresh_input_tokens_count);
  const read = formatTokenCount(cache.read_tokens_count);
  const created = formatTokenCount(cache.creation_tokens_count);
  const hit = cache.hit_percent.toFixed(1);
  return `Tokens Breakdown: Fresh: ${fresh} | Cache Read: ${read} (${hit}% hit) | Cache Created: ${created}`;
});

let metricsPollInterval: ReturnType<typeof setInterval> | null = null;

function pollMetrics() {
  if (agentId.value && layoutStore.isFloatingChatOpen) {
    const pPath = activeProjectPath.value;
    if (pPath) {
      metricsStore.fetchActiveMetrics(pPath, agentId.value, dialogueStore.activeConversationId || undefined).catch(reportAsyncFailure);
    }
    metricsStore.fetchAccountQuotas().catch(reportAsyncFailure);
  }
}

watch(
  () => [agentId.value, layoutStore.isFloatingChatOpen],
  ([aid, open]) => {
    if (aid && open) {
      pollMetrics();
      if (metricsPollInterval) clearInterval(metricsPollInterval);
      metricsPollInterval = setInterval(pollMetrics, 8000);
    } else {
      if (metricsPollInterval) {
        clearInterval(metricsPollInterval);
        metricsPollInterval = null;
      }
    }
  },
  { immediate: true }
);

onUnmounted(() => {
  if (terminalPollInterval) clearInterval(terminalPollInterval);
  if (terminalCopyTimer) clearTimeout(terminalCopyTimer);
  if (metricsPollInterval) clearInterval(metricsPollInterval);
});
</script>

<template>
  <div 
    v-if="layoutStore.isFloatingChatOpen && !layoutStore.isFloatingChatMinimized"
    class="flex flex-col font-mono text-xs overflow-hidden select-none bg-[var(--bg-color)]"
    :class="[
      layoutStore.chatLayoutMode === 'docked'
        ? 'relative w-full h-full min-h-0'
        : 'fixed z-40 bg-[var(--bg-color)] border-2 border-[var(--border-color)] shadow-2xl transition-all duration-200 bottom-12 right-4 w-[500px] max-w-[calc(100vw-32px)] h-[580px] max-h-[calc(100vh-80px)]'
    ]"
  >
    <!-- ── CHAT WINDOW HEADER ──────────────────────────────────── -->
    <div class="px-3 py-2 border-b border-[var(--border-color)] ui-surface-title flex items-center justify-between shrink-0 select-none">
      <div class="flex items-center gap-2 min-w-0 flex-1">
        <AppAgentPortrait
          v-if="agentId"
          :agent-id="agentId"
          :dna="activeAgentInfo?.dna"
          :size="32"
          class="shrink-0 border border-[var(--accent-color)] ui-surface-base"
        />
        <Bot v-else :size="16" class="text-[var(--accent-color)] shrink-0" />

        <div class="flex flex-col min-w-0 flex-1">
          <div class="flex items-center gap-2">
            <span class="font-bold text-[var(--fg-color)] uppercase tracking-wider truncate">
              {{ agentId || 'AGENT CHAT' }}
            </span>
            <span
              v-if="agentId"
              class="text-[8px] uppercase px-1 py-0.2 border font-mono shrink-0"
              :class="activeAgentInfo?.status === 'working'
                ? 'border-[var(--accent-color)] text-[var(--accent-color)] animate-pulse'
                : (conv && conv.messages.length > 0 && !isAgentUnlaunched
                    ? 'border-[var(--status-ok-border)] text-[var(--status-ok-color)] bg-[var(--status-ok-bg)]'
                    : 'border-[var(--border-color)] text-[var(--dim-color)]')"
            >
              {{ activeAgentInfo?.status === 'working' ? 'working' : (conv && conv.messages.length > 0 && !isAgentUnlaunched ? 'active' : (activeAgentInfo?.status || (isAgentUnlaunched ? 'unlaunched' : 'idle'))) }}
            </span>
          </div>
          <span class="text-[8px] text-[var(--dim-color)] truncate">{{ activeAgentInfo?.role }}</span>
        </div>
      </div>

      <!-- Header Controls -->
      <div class="flex items-center gap-1 shrink-0">
        <!-- Launch Button (if unlaunched) -->
        <AppTooltip v-if="isAgentUnlaunched" text="Launch agent">
          <button
            @click="launchCurrentAgent"
            class="ui-icon-btn"
          >
            <Play :size="11" />
          </button>
        </AppTooltip>

        <!-- In-Window View Mode Toggle: Messages <-> Terminal -->
        <AppTooltip :text="chatViewMode === 'terminal' ? 'Show Dialogue Messages' : 'Show Live Terminal Stream'">
          <button
            @click="chatViewMode = chatViewMode === 'terminal' ? 'messages' : 'terminal'"
            class="ui-icon-btn"
            :class="chatViewMode === 'terminal' 
              ? '!border-[var(--accent-color)] !bg-[var(--accent-color)]/20 !text-[var(--accent-color)]' 
              : ''"
          >
            <MessageSquare v-if="chatViewMode === 'terminal'" :size="11" />
            <Terminal v-else :size="11" />
          </button>
        </AppTooltip>

        <!-- Docked vs Floating Layout Mode Toggle -->
        <AppTooltip :text="layoutStore.chatLayoutMode === 'docked' ? 'Float as Window' : 'Dock as Right Column'">
          <button
            @click="layoutStore.setChatLayoutMode(layoutStore.chatLayoutMode === 'docked' ? 'floating' : 'docked')"
            class="ui-icon-btn"
          >
            <Layers v-if="layoutStore.chatLayoutMode === 'docked'" :size="11" />
            <PanelRight v-else :size="11" />
          </button>
        </AppTooltip>

        <!-- Minimize Button -->
        <AppTooltip text="Minimize to Bottom Bar">
          <button
            @click="layoutStore.toggleFloatingChatMinimize()"
            class="ui-icon-btn"
          >
            <Minus :size="11" />
          </button>
        </AppTooltip>

        <!-- Close Button -->
        <AppTooltip text="Close & Stop Agent">
          <button
            @click="dialogueStore.closeAndKillAgent(activeProjectPath)"
            class="ui-icon-btn ui-icon-btn--danger"
          >
            <X :size="11" />
          </button>
        </AppTooltip>
      </div>
    </div>

    <!-- ── AGENT TOOLBOX: RUNNER, MODEL, ACCOUNT & LIFECYCLE ── -->
    <div class="h-9 px-3 ui-surface-card border-b border-[var(--border-color)] flex items-center justify-between gap-2 text-[8px] font-mono shrink-0 select-none">
      <div class="flex items-center gap-1.5 min-w-0 flex-1">
        <!-- Consolidated Execution Config Dropdown Wizard -->
        <AppExecutionConfigPicker
          v-model:runner="selectedRunner"
          v-model:model="selectedModel"
          v-model:effort="selectedEffort"
          v-model:account-id="selectedAccountId"
          :runner-options="runnerOptions"
          :models="availableModels"
          :account-options="accountOptions"
          :pending-config-update="pendingConfigUpdate"
          :disabled="isApplyingConfig"
          @open="isEditingExecutionConfig = true"
          @cancel="handleExecutionConfigCancel"
          @change="isEditingExecutionConfig = false"
        />

        <!-- Queued Config Indicator -->
        <AppTooltip v-if="pendingConfigUpdate" text="Configuration change queued (will auto-apply when turn completes)" position="bottom">
          <div class="px-1.5 py-0.5 border border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] text-[var(--status-warn-color)] text-[8px] flex items-center gap-1 font-bold shrink-0 animate-pulse">
            <RefreshCw :size="8" class="animate-spin" />
            <span>QUEUED</span>
          </div>
        </AppTooltip>
      </div>

      <!-- Right: Lifecycle Tools (New Session, Compact Context, Handoff) -->
      <div class="flex items-center gap-1 shrink-0">
        <!-- New Session Button -->
        <AppTooltip text="Start New Session (archive current turns & clean context)" position="bottom">
          <button
            @click="handleCleanSession"
            :disabled="isCleaningSession"
            class="ui-icon-btn disabled:opacity-50"
          >
            <RotateCcw :size="10" :class="{ 'animate-spin': isCleaningSession }" />
          </button>
        </AppTooltip>

        <!-- Compact Context Button -->
        <AppTooltip text="Compact Context (instruct agent to summarize memory & record milestone)" position="bottom">
          <button
            @click="handleCompactContext"
            class="ui-icon-btn"
          >
            <Brain :size="10" />
          </button>
        </AppTooltip>

        <!-- Transfer Context Button -->
        <AppTooltip text="Transfer Context (handoff active session to another agent)" position="bottom">
          <button
            @click="showContextTransferModal = true"
            class="ui-icon-btn"
          >
            <ArrowRightLeft :size="10" />
          </button>
        </AppTooltip>

        <!-- Search Conversations Button -->
        <AppTooltip text="Search and reopen past conversations" position="bottom">
          <button
            @click="showConversationSearch = true"
            class="ui-icon-btn"
          >
            <Search :size="10" />
          </button>
        </AppTooltip>
      </div>
    </div>

    <!-- ── CONTEXT & QUOTA TELEMETRY BAR ────────────────────────── -->
    <div 
      v-if="sessionMetrics || activeAgentInfo"
      class="px-3 py-1 ui-surface-card border-b border-[var(--border-color)]/80 text-[8px] flex items-center justify-between gap-3 font-mono shrink-0 select-none"
    >
      <!-- Left: Context Usage & Progress -->
      <div class="flex items-center gap-2 min-w-0">
        <AppTooltip :text="contextTooltip">
          <div class="flex items-center gap-1.5 px-1.5 py-0.5 border cursor-default" :class="contextBadgeClass">
            <span class="font-bold tracking-tight">CTX</span>
            <span>{{ contextUsedPct.toFixed(1) }}%</span>
            <div class="w-10 h-1 bg-[var(--bg-color)] rounded-none overflow-hidden ml-0.5 border border-[var(--border-color)]/60">
              <div 
                class="h-full transition-all duration-300"
                :class="contextBarClass"
                :style="{ width: `${Math.min(100, Math.max(0, contextUsedPct))}%` }"
              />
            </div>
          </div>
        </AppTooltip>

        <!-- Token count & Cache hit / Cost -->
        <AppTooltip :text="tokenBreakdownTooltip">
          <div class="flex items-center gap-1 text-[var(--dim-color)] cursor-default px-1">
            <span class="uppercase">TOKENS:</span>
            <span class="text-[var(--fg-color)] font-bold">
              {{ totalTokens > 0 ? formatTokenCount(totalTokens) : (sessionMetrics?.context || sessionMetrics?.tokens_used_count !== undefined ? '0' : '--') }}
            </span>
            <span v-if="isCurrentRunnerOpenCode && openCodeCostFormatted" class="text-[var(--status-ok-color)] font-bold ml-1">
              ({{ openCodeCostFormatted }})
            </span>
            <span v-else-if="sessionMetrics?.context?.cache?.hit_percent" class="text-[var(--status-ok-color)]/90 text-[8px]">
              ({{ sessionMetrics.context.cache.hit_percent.toFixed(0) }}% hit)
            </span>
          </div>
        </AppTooltip>
      </div>

      <!-- Right: Quota Windows (5h / 7d) & Account Indicator -->
      <div class="flex items-center gap-2 shrink-0">
        <!-- 5-hour quota badge (AGY only) -->
        <AppTooltip 
          v-if="!isCurrentRunnerOpenCode && sessionMetrics?.five_hour_window"
          :text="`5-Hour Quota: ${sessionMetrics.five_hour_window.remaining_percent.toFixed(0)}% remaining (Resets: ${sessionMetrics.five_hour_window.resets_at || 'in rolling window'})`"
        >
          <div class="flex items-center gap-1 px-1 py-0.5 border border-[var(--border-color)] ui-surface-base text-[8px] cursor-default">
            <span class="text-[var(--dim-color)] uppercase">5H:</span>
            <span :class="sessionMetrics.five_hour_window.remaining_percent < 20 ? 'text-[var(--status-error-color)] font-bold' : 'text-[var(--fg-color)]'">
              {{ sessionMetrics.five_hour_window.remaining_percent.toFixed(0) }}%
            </span>
          </div>
        </AppTooltip>

        <!-- Weekly quota badge (AGY only) -->
        <AppTooltip 
          v-if="!isCurrentRunnerOpenCode && sessionMetrics?.weekly_window"
          :text="`Weekly Quota: ${sessionMetrics.weekly_window.remaining_percent.toFixed(0)}% remaining (Resets: ${sessionMetrics.weekly_window.resets_at || 'in rolling window'})`"
        >
          <div class="flex items-center gap-1 px-1 py-0.5 border border-[var(--border-color)] ui-surface-base text-[8px] cursor-default">
            <span class="text-[var(--dim-color)] uppercase">7D:</span>
            <span :class="sessionMetrics.weekly_window.remaining_percent < 20 ? 'text-[var(--status-error-color)] font-bold' : 'text-[var(--fg-color)]'">
              {{ sessionMetrics.weekly_window.remaining_percent.toFixed(0) }}%
            </span>
          </div>
        </AppTooltip>

        <!-- Agent Account Tag -->
        <AppTooltip
          v-if="activeAgentInfo?.account_email || (isCurrentRunnerOpenCode && selectedAccountId)"
          :text="`Running under account: ${activeAgentInfo?.account_email || selectedAccountId}`"
        >
          <span class="text-[8px] text-[var(--dim-color)] truncate max-w-[120px] px-1 py-0.2 border border-[var(--border-color)]/80 ui-surface-base cursor-default">
            {{ (activeAgentInfo?.account_email || selectedAccountId).split('@')[0] }}
          </span>
        </AppTooltip>
      </div>
    </div>

    <!-- ── VIEW CONTAINER: TERMINAL / MESSAGES & MODAL MOUNT ─────── -->
    <div id="chat-messages-modal-mount" class="relative flex-1 flex flex-col min-h-0 overflow-hidden">
      <!-- ── VIEW A: LIVE TERMINAL STREAM (When chatViewMode === 'terminal') ─ -->
      <div 
        v-show="chatViewMode === 'terminal'" 
      class="flex-1 flex flex-col min-h-0 bg-black/90 font-mono text-[10px] overflow-hidden select-text"
    >
      <!-- Terminal Sub-Header Controls -->
      <div class="flex items-center justify-between px-3 py-1 bg-black/70 border-b border-[var(--border-color)]/60 text-[9px] text-[var(--dim-color)] shrink-0">
        <div class="flex items-center gap-2">
          <span 
            class="w-1.5 h-1.5 rounded-full" 
            :class="activeAgentInfo?.status === 'working' ? 'bg-[var(--status-ok-color)] animate-pulse' : 'bg-[var(--dim-color)]'" 
          />
          <span class="uppercase tracking-wider font-bold text-[var(--fg-color)]">
            {{ agentId }} // STDOUT & STDERR
          </span>
        </div>

        <div class="flex items-center gap-2">
          <!-- Filter toggle -->
          <button
            @click="toggleTerminalFilter"
            class="px-1.5 py-0.5 border text-[8px] uppercase tracking-wider transition-colors cursor-pointer"
            :class="terminalFilter === 'errors_only' 
              ? 'border-[var(--status-error-color)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] font-bold' 
              : 'border-[var(--border-color)] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)]'"
          >
            {{ terminalFilter === 'errors_only' ? 'ERRORS ONLY' : 'ALL OUTPUT' }}
          </button>

          <!-- Refresh -->
          <AppTooltip text="Refresh stream">
            <button
              @click="fetchTerminalOutput"
              class="p-0.5 border border-[var(--border-color)] hover:border-[var(--accent-color)] text-[var(--dim-color)] hover:text-[var(--accent-color)] transition-colors cursor-pointer"
            >
              <RefreshCw :size="9" />
            </button>
          </AppTooltip>

          <!-- Copy Button -->
          <button
            @click="copyTerminalOutput"
            class="px-1.5 py-0.5 border border-[var(--border-color)] hover:border-[var(--accent-color)] text-[var(--dim-color)] hover:text-[var(--accent-color)] text-[8px] uppercase tracking-wider transition-colors cursor-pointer flex items-center gap-1"
          >
            <Check v-if="terminalCopied" :size="9" class="text-[var(--status-ok-color)]" />
            <Copy v-else :size="9" />
            <span>{{ terminalCopied ? 'COPIED' : 'COPY' }}</span>
          </button>
        </div>
      </div>

      <!-- Terminal Output Pre -->
      <pre 
        ref="terminalContainerRef" 
        @scroll="onTerminalScroll" 
        class="flex-1 p-3 overflow-y-auto overflow-x-auto text-[10px] leading-relaxed text-[var(--fg-color)] font-mono whitespace-pre-wrap break-words bg-black/60"
      >{{ terminalOutput }}</pre>
    </div>

    <!-- ── VIEW B: MESSAGES STREAM (When chatViewMode === 'messages') ──── -->
    <div 
      v-show="chatViewMode === 'messages'" 
      ref="messagesContainerRef" 
      @scroll="onMessagesScroll" 
      class="flex-1 overflow-y-auto p-3 flex flex-col gap-2.5 ui-surface-list select-text"
    >
      
      <div v-if="!conv || conv.messages.length === 0" class="h-full flex flex-col items-center justify-center gap-2 text-[var(--dim-color)] opacity-60">
        <Bot :size="28" />
        <p class="text-[10px] uppercase tracking-widest">No messages yet with {{ agentId }}</p>
        <p class="text-[9px] opacity-70">Send a prompt to start work</p>
      </div>

      <template v-else>
        <template v-for="(msg, index) in conv.messages" :key="msg.id">
          <!-- Conversation separator: rendered when the Conversation changes between consecutive turns -->
          <div
            v-if="index > 0 && msg.conversationId && conv.messages[index - 1].conversationId && msg.conversationId !== conv.messages[index - 1].conversationId"
            class="flex items-center gap-2 my-2 select-none"
          >
            <div class="flex-1 border-t border-[var(--border-color)]/40" />
            <div class="flex items-center gap-1.5 px-2 py-0.5 border border-[var(--border-color)]/60 ui-surface-card text-[8px] text-[var(--dim-color)] font-mono tracking-widest">
              <span class="opacity-60">◈</span>
              <span>{{ msg.topic || '[ new conversation ]' }}</span>
              <AppTooltip :text="`Reopen '${conv.messages[index - 1].topic || 'previous conversation'}'`">
                <button
                  class="ml-1 px-1 text-[8px] border border-[var(--accent-color)]/40 text-[var(--accent-color)] hover:bg-[var(--accent-color)]/10 transition-colors font-mono uppercase tracking-wider cursor-pointer"
                  @click="dialogueStore.reopenConversation(conv.messages[index - 1].conversationId!, agentId || undefined, activeProjectPath || undefined).catch(reportAsyncFailure)"
                >Reopen</button>
              </AppTooltip>
            </div>
            <div class="flex-1 border-t border-[var(--border-color)]/40" />
          </div>

          <!-- System event separator -->
          <div
            v-if="msg.role === 'system'"
            class="flex items-center gap-2 my-1 text-[8px] text-[var(--dim-color)] font-mono uppercase tracking-widest select-none"
          >
            <div class="flex-1 border-t border-[var(--border-color)]/40" />
            <span>// {{ msg.dialogue }}</span>
            <div class="flex-1 border-t border-[var(--border-color)]/40" />
          </div>

          <!-- Message Bubble -->
          <div
            v-else
            class="flex"
            :class="msg.role === 'user' ? 'justify-end' : 'justify-start'"
          >
            <div
              class="flex flex-col gap-1 max-w-[85%]"
              :class="msg.role === 'user' ? 'items-end' : 'items-start'"
            >
              <div
                class="px-2.5 py-1.5 text-xs leading-relaxed select-text font-mono chat-markdown-body"
                :class="msg.role === 'user'
                  ? 'bg-[var(--accent-color)]/15 border border-[var(--accent-color)]/30 text-[var(--fg-color)]'
                  : 'bg-white/5 border border-[var(--border-color)] text-[var(--fg-color)]'"
                @click="handleChatMarkdownClick"
                v-html="renderDialogueMarkdown(msg.dialogue)"
              />

              <!-- Artifact chip -->
              <button
                v-if="msg.artifact"
                @click="handleSelectArtifact(msg.artifact!.id)"
                class="flex items-center gap-1.5 px-2 py-0.5 border border-[var(--border-color)] ui-surface-card text-[9px] text-[var(--dim-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] transition-colors cursor-pointer"
              >
                <Paperclip :size="9" />
                <span class="truncate max-w-[150px]">{{ msg.artifact.label }}</span>
                <span class="text-[8px] text-[var(--accent-color)] uppercase font-bold">[OPEN IN READER]</span>
              </button>

              <!-- Inline Approval Request Card -->
              <div
                v-if="msg.approval"
                class="w-full mt-1 border p-2.5 flex flex-col gap-2 font-mono select-none"
                :class="msg.approval.status === 'superseded' 
                  ? 'border-[var(--border-color)] ui-surface-card opacity-60' 
                  : (msg.approval.status === 'accepted' 
                      ? 'border-[var(--status-ok-border)] bg-[var(--status-ok-bg)]' 
                      : (msg.approval.status === 'rejected' 
                          ? 'border-[var(--status-error-border)] bg-[var(--status-error-bg)]' 
                          : 'border-[var(--status-warn-border)] bg-[var(--status-warn-bg)]'))"
              >
                <div 
                  class="flex items-center justify-between gap-2 border-b pb-1"
                  :class="msg.approval.status === 'superseded' ? 'border-[var(--border-color)]/50' : 'border-[var(--status-warn-border)]'"
                >
                  <div class="flex items-center gap-1.5 min-w-0">
                    <ShieldAlert :size="12" :class="msg.approval.status === 'superseded' ? 'text-[var(--dim-color)] shrink-0' : 'text-[var(--status-warn-color)] shrink-0'" />
                    <span 
                      class="text-[9px] font-bold uppercase tracking-wider truncate"
                      :class="msg.approval.status === 'superseded' ? 'text-[var(--dim-color)]' : 'text-[var(--status-warn-color)]'"
                    >
                      APPROVAL: {{ msg.approval.title }}
                    </span>
                  </div>
                  <span 
                    class="text-[8px] uppercase font-bold"
                    :class="msg.approval.status === 'superseded' 
                      ? 'text-[var(--dim-color)]' 
                      : (msg.approval.status === 'accepted' 
                          ? 'text-[var(--status-ok-color)]' 
                          : (msg.approval.status === 'rejected' 
                              ? 'text-[var(--status-error-color)]' 
                              : 'text-[var(--status-warn-color)]'))"
                  >
                    {{ msg.approval.status }}
                  </span>
                </div>

                <p v-if="msg.approval.description" class="text-[10px] leading-relaxed text-[var(--fg-color)] font-mono select-text">
                  {{ msg.approval.description }}
                </p>

                <!-- Interactive Action Buttons -->
                <div v-if="msg.approval.status === 'pending'" class="flex flex-col gap-1.5 pt-1">
                  <!-- Dynamic Options -->
                  <div v-if="msg.approval.options && msg.approval.options.length > 0" class="flex flex-wrap gap-1">
                    <button
                      v-for="(option, optIdx) in msg.approval.options"
                      :key="optIdx"
                      @click="handleSelectOption(msg, option)"
                      class="px-2 py-0.5 bg-[var(--status-warn-bg)] hover:bg-[var(--status-warn-color)]/25 border border-[var(--status-warn-border)] text-[var(--status-warn-color)] text-[8px] font-bold tracking-wider cursor-pointer"
                    >
                      [{{ optIdx + 1 }}] {{ option }}
                    </button>
                  </div>

                  <!-- Standard Accept / Changes -->
                  <div class="flex items-center gap-1.5 pt-0.5">
                    <button
                      v-if="!msg.approval.options || msg.approval.options.length === 0"
                      @click="handleAcceptApproval(msg)"
                      class="px-2.5 py-0.5 bg-[var(--status-ok-bg)] hover:bg-[var(--status-ok-color)]/30 border border-[var(--status-ok-border)] text-[var(--status-ok-color)] text-[8px] font-bold uppercase tracking-wider cursor-pointer flex items-center gap-1"
                    >
                      <Check :size="10" />
                      <span>Accept Proposal</span>
                    </button>

                    <button
                      @click="handleRequestChanges(msg)"
                      class="px-2.5 py-0.5 ui-surface-base border border-[var(--border-color)] hover:border-[var(--status-warn-color)] text-[var(--dim-color)] hover:text-[var(--status-warn-color)] text-[8px] uppercase tracking-wider cursor-pointer flex items-center gap-1"
                    >
                      <MessageSquare :size="10" />
                      <span>Request Changes</span>
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </template>
      </template>

      <!-- ── TRANSIENT LIVE WORKING & THINKING BUBBLE ───────────── -->
      <div
        v-if="isWorking"
        class="border border-[var(--border-color)] ui-surface-card p-3 max-w-[85%] font-mono text-[10px] space-y-2 select-text transition-all"
      >
        <!-- Section 1: Active Thinking Stream -->
        <div v-if="currentLiveStep?.thoughtText" class="space-y-1">
          <div class="flex items-center gap-1.5 text-[9px] uppercase tracking-wider text-[var(--accent-color)] font-bold opacity-80 select-none">
            <Brain :size="11" />
            <span>Thinking</span>
          </div>
          <div 
            ref="thoughtBoxRef"
            class="max-h-44 overflow-y-auto text-[var(--dim-color)] italic leading-relaxed whitespace-pre-wrap pl-2 border-l border-[var(--accent-color)]/30 select-text"
          >
            {{ currentLiveStep.thoughtText }}
          </div>
        </div>

        <!-- Section 2: Subtle Active Action / Tool Indicator -->
        <div class="flex items-center gap-2 pt-0.5 text-[var(--fg-color)]/90 truncate">
          <span class="w-1.5 h-1.5 rounded-full bg-[var(--accent-color)] animate-pulse shrink-0" />
          <span class="truncate">
            {{ currentLiveStep?.actionSummary || (isAgentUnlaunched ? `${agentId} is offline` : `${agentId} is working…`) }}
          </span>
        </div>
      </div>

      <!-- ── SUBTLE CONVERSATION CONTINUATION INDICATOR ─────────── -->
      <div
        v-if="isAgentUnlaunched && !isWorking && !activeAgentError && conv && conv.messages.length > 0"
        class="flex items-center gap-2 px-2 py-1 text-[9px] font-mono text-[var(--dim-color)] opacity-75 select-none"
      >
        <span class="w-1.5 h-1.5 rounded-full bg-[var(--status-ok-color)]/70 shrink-0" />
        <span>Conversation ready to continue</span>
      </div>
      <div
        v-else-if="isAgentUnlaunched && !isWorking && !activeAgentError"
        class="flex items-center gap-2 px-2 py-1 text-[9px] font-mono text-[var(--dim-color)] opacity-75 select-none"
      >
        <span class="w-1.5 h-1.5 rounded-full bg-[var(--dim-color)] shrink-0" />
        <span>Ready to start conversation</span>
      </div>

      <!-- ── IN-CHAT ERROR / QUOTA RESOLUTION BANNER ───────────── -->
      <div
        v-if="activeAgentError"
        class="border border-[var(--status-error-border)] bg-[var(--status-error-bg)] p-3 max-w-[95%] font-mono text-xs flex flex-col gap-2 select-text"
      >
        <div class="flex items-center justify-between gap-2 border-b border-[var(--status-error-border)] pb-1.5">
          <div class="flex items-center gap-1.5 min-w-0">
            <ShieldAlert :size="13" class="text-[var(--status-error-color)] shrink-0" />
            <span class="text-[9px] font-bold text-[var(--status-error-color)] uppercase tracking-wider truncate">
              {{ activeAgentError.type === 'quota_exhausted' ? 'TOKEN LIMIT / QUOTA EXHAUSTED' : 'AGENT EXECUTION ERROR' }}
            </span>
          </div>
          <AppTooltip text="Dismiss error">
            <button
              @click="dialogueStore.clearAgentError(agentId!, activeProjectPath)"
              class="text-[var(--dim-color)] hover:text-[var(--fg-color)] text-[10px] p-0.5 cursor-pointer"
            >
              <X :size="11" />
            </button>
          </AppTooltip>
        </div>

        <p class="text-[10px] leading-relaxed text-[var(--fg-color)] select-text">
          {{ activeAgentError.message }}
        </p>

        <!-- Actionable Recovery Paths -->
        <div class="flex flex-wrap items-center gap-1.5 pt-1 border-t border-[var(--status-error-border)]">
          <!-- Alternative Accounts (if quota exhausted) -->
          <template v-if="activeAgentError.type === 'quota_exhausted' && alternativeAccounts.length > 0">
            <span class="text-[8px] uppercase tracking-wider text-[var(--dim-color)] font-bold mr-0.5">Switch Account:</span>
            <button
              v-for="altAcc in alternativeAccounts"
              :key="altAcc.value"
              @click="resolveErrorWithAccount(altAcc.value)"
              class="px-2 py-0.5 border text-[8px] font-bold cursor-pointer transition-colors flex items-center gap-1"
              :class="altAcc.isRecommended
                ? 'border-[var(--status-ok-color)] bg-[var(--status-ok-bg)] text-[var(--status-ok-color)] ring-1 ring-[var(--status-ok-border)]'
                : 'border-[var(--border-color)] ui-surface-base hover:bg-[var(--ui-bg-card)] text-[var(--fg-color)]'"
            >
              <span>{{ altAcc.label }}</span>
              <span v-if="altAcc.isRecommended" class="text-[8px] text-[var(--status-ok-color)] font-bold uppercase tracking-widest">[REC]</span>
            </button>
          </template>

          <!-- Switch to Flash / lighter model -->
          <button
            v-if="canSwitchToFlashModel"
            @click="resolveErrorWithFlashModel"
            class="px-2 py-0.5 border border-[var(--status-info-border)] bg-[var(--status-info-bg)] hover:bg-[var(--status-info-color)]/20 text-[var(--status-info-color)] text-[8px] font-bold cursor-pointer transition-colors"
          >
            Switch to {{ flashModelName }}
          </button>

          <!-- Retry Failed Message -->
          <button
            @click="retryLastPrompt"
            class="px-2 py-0.5 border border-[var(--status-warn-border)] bg-[var(--status-warn-bg)] hover:bg-[var(--status-warn-color)]/20 text-[var(--status-warn-color)] text-[8px] font-bold cursor-pointer transition-colors flex items-center gap-1"
          >
            <RotateCcw :size="9" />
            <span>Retry Message</span>
          </button>

          <!-- Retry / Restart -->
          <button
            @click="retryAfterError"
            class="px-2 py-0.5 border border-[var(--border-color)] bg-[var(--bg-color)] hover:bg-white/5 text-[var(--fg-color)] text-[8px] font-bold cursor-pointer transition-colors flex items-center gap-1"
          >
            <RefreshCw :size="9" />
            <span>Restart Agent</span>
          </button>
        </div>
      </div>

      <div ref="messagesEnd" />
    </div>
  </div>

    <!-- ── FOOTER: INPUT PROMPT BOX & ATTACHMENT TRAY ──────────── -->
    <div class="border-t border-[var(--border-color)] bg-[var(--ui-bg-card)] shrink-0 flex flex-col relative">
      <!-- Base spacer for 1 line to prevent message overlap when idle -->
      <div 
        class="w-full transition-all duration-150"
        :class="explorerStore.attachedReviewFiles.length > 0 ? 'min-h-[82px]' : 'min-h-[50px]'"
      />

      <!-- Floating Expanding Composer Card anchored to bottom -->
      <div class="absolute bottom-0 left-0 right-0 z-20 bg-[var(--ui-bg-card)] border-t border-[var(--border-color)] flex flex-col shadow-[0_-4px_20px_rgba(0,0,0,0.5)]">
        <!-- Attached Review Files Tray -->
        <div 
          v-if="explorerStore.attachedReviewFiles.length > 0" 
          class="px-2.5 py-1.5 flex items-center gap-1.5 overflow-x-auto ui-surface-title border-b border-[var(--border-color)]/60 text-[9px] font-mono select-none"
        >
          <span class="text-[8px] uppercase tracking-wider text-[var(--dim-color)] mr-0.5">Attached:</span>
          <div
            v-for="(file, idx) in explorerStore.attachedReviewFiles"
            :key="file.rel_path"
            draggable="true"
            class="flex items-center gap-1 px-1.5 py-0.5 border border-[var(--accent-color)]/60 bg-[var(--accent-color)]/10 text-[9px] text-[var(--accent-color)] font-mono cursor-grab active:cursor-grabbing hover:bg-[var(--accent-color)]/20 transition-colors"
            @dragstart="(e) => onAttachmentDragStart(e, file, idx)"
            @click="insertAttachmentPlaceholder(file, idx)"
          >
            <!-- Thumbnail preview for images -->
            <img
              v-if="file.preview_url || file.doc_type === 'image' || /\.(png|jpe?g|webp|gif|svg)$/i.test(file.name)"
              :src="file.preview_url || (file.rel_path.startsWith('/') ? `file://${file.rel_path}` : `file://${activeProjectPath}/${file.rel_path}`)"
              class="w-3.5 h-3.5 object-cover border border-[var(--accent-color)]/40 rounded-none shrink-0"
              alt="thumbnail"
            />
            <FileCode v-else-if="file.doc_type === 'code'" :size="9" class="shrink-0" />
            <FileText v-else-if="file.doc_type === 'markdown' || file.category === 'docs'" :size="9" class="shrink-0" />
            <Paperclip v-else :size="9" class="shrink-0" />

            <!-- Placeholder badge -->
            <span class="px-1 py-0.1 bg-[var(--accent-color)] text-[var(--bg-color)] font-bold text-[8px] tracking-tight">
              {{ getAttachmentPlaceholder(file, idx) }}
            </span>

            <AppTooltip :text="file.rel_path">
              <span class="font-bold truncate max-w-[100px] inline-block">{{ file.name }}</span>
            </AppTooltip>

            <span
              v-if="file.comment_count && file.comment_count > 0"
              class="px-1 py-0.1 bg-[var(--status-warn-color)] text-[var(--bg-color)] font-bold text-[8px]"
            >
              {{ file.comment_count }}
            </span>

            <button
              class="hover:text-[var(--fg-color)] cursor-pointer ml-0.5 p-0.5"
              @click.stop="explorerStore.detachFileFromChat(file.rel_path)"
            >
              <X :size="9" />
            </button>
          </div>

          <button
            class="text-[8px] uppercase text-[var(--dim-color)] hover:text-[var(--fg-color)] underline cursor-pointer ml-auto"
            @click="explorerStore.clearAttachedReviewFiles"
          >
            Clear all
          </button>
        </div>

        <!-- Prompt Input Row -->
        <div class="p-2 flex items-end gap-1.5">
          <!-- Attach File Button -->
          <AppTooltip text="Attach file from disk (or paste image with Ctrl+V)" class="shrink-0 self-end">
            <button
              type="button"
              :disabled="!agentId"
              class="w-8 h-8 border border-[var(--border-color)] hover:border-[var(--accent-color)] text-[var(--dim-color)] hover:text-[var(--accent-color)] transition-colors cursor-pointer shrink-0 disabled:opacity-30 disabled:cursor-not-allowed ui-surface-base flex items-center justify-center"
              @click="openFilePicker"
            >
              <Paperclip :size="13" />
            </button>
          </AppTooltip>

          <!-- Textarea with Centered Voice Status Mic -->
          <div class="relative flex-1 flex items-end min-w-0">
            <AppTextarea
              ref="textareaRef"
              v-model="inputText"
              :rows="1"
              :max-rows="10"
              :auto-grow="true"
              :auto-scroll="true"
              :disabled="!agentId"
              :placeholder="conv && conv.messages.length > 0 ? 'Conversation ready to continue...' : (explorerStore.attachedReviewFiles.length > 0 ? 'Add instructions or drag attachment chips...' : 'Send prompt (Ctrl+V to paste images)...')"
              class="flex-1 resize-none text-xs font-mono pr-8 overflow-x-hidden break-words"
              @keydown="onKeydown"
              @paste="handlePaste"
            />

            <!-- Microphone Status Indicator (Symbol-only, Pinned to Bottom-Right) -->
            <AppTooltip
              v-if="dictationStore.enabled && dictationStore.available"
              :text="voiceMicTooltip"
              class="absolute right-2.5 bottom-2.5"
            >
              <div
                class="flex items-center justify-center cursor-pointer select-none transition-colors"
                :class="voiceMicClasses"
                @mousedown.prevent
                @click="onMicClick"
              >
                <Mic :size="13" />
              </div>
            </AppTooltip>
          </div>

          <!-- Send / Stop Dynamic Button -->
          <AppTooltip :text="!isWorking && coreStore.hostCapabilities !== null && !coreStore.hasAnyRunner ? 'Install an AI CLI runner in Setup & Readiness first' : (isWorking ? (inputText.trim() ? 'Stop execution & send correction [Enter]' : 'Stop execution [Esc]') : 'Send message [Enter]')" class="shrink-0 self-end">
            <button
              type="button"
              :disabled="!agentId || (!isWorking && ((coreStore.hostCapabilities !== null && !coreStore.hasAnyRunner) || (!inputText.trim() && explorerStore.attachedReviewFiles.length === 0)))"
              class="w-8 h-8 border transition-colors cursor-pointer shrink-0 disabled:opacity-30 disabled:cursor-not-allowed flex items-center justify-center"
              :class="isWorking
                ? (inputText.trim()
                    ? 'border-[var(--status-error-color)] bg-[var(--status-error-bg)] text-[var(--status-error-color)] hover:bg-[var(--status-error-color)]/30'
                    : 'border-[var(--status-error-color)] text-[var(--status-error-color)] ui-surface-base hover:bg-[var(--status-error-bg)]')
                : ((inputText.trim() || explorerStore.attachedReviewFiles.length > 0) && agentId
                    ? 'border-[var(--accent-color)] bg-[var(--accent-color)] text-[var(--bg-color)] hover:bg-[var(--accent-color)]/80'
                    : 'border-[var(--border-color)] text-[var(--dim-color)] ui-surface-base')"
              @click="handleMainAction"
            >
              <Square v-if="isWorking" :size="12" class="fill-current" />
              <Send v-else :size="13" />
            </button>
          </AppTooltip>
        </div>
      </div>
    </div>

    <!-- ── CONTEXT TRANSFER MODAL ────────────────────────────── -->
    <ContextTransferModal
      :is-open="showContextTransferModal"
      :source-agent-id="agentId || ''"
      :project-path="activeProjectPath"
      :project-name="activeProject?.name || ''"
      :available-agents="projectAgentsList"
      @close="showContextTransferModal = false"
      @transfer="handleContextTransfer"
    />

    <ConversationSearchModal
      :is-open="showConversationSearch"
      :agent-id="agentId || ''"
      :project-path="activeProjectPath"
      @close="showConversationSearch = false"
    />

  </div>
</template>

<style scoped>
:deep(.chat-markdown-body a) {
  color: var(--accent-color);
  text-decoration: underline;
  text-underline-offset: 2px;
  cursor: pointer;
}
:deep(.chat-markdown-body a:hover) {
  filter: brightness(1.2);
}
:deep(.chat-markdown-body p) {
  margin-bottom: 0.5rem;
  line-height: 1.5;
}
:deep(.chat-markdown-body p:last-child) {
  margin-bottom: 0;
}
:deep(.chat-markdown-body ul) {
  list-style-type: disc;
  padding-left: 1.25rem;
  margin-bottom: 0.5rem;
}
:deep(.chat-markdown-body ol) {
  list-style-type: decimal;
  padding-left: 1.25rem;
  margin-bottom: 0.5rem;
}
:deep(.chat-markdown-body li) {
  margin-bottom: 0.25rem;
  line-height: 1.4;
}
:deep(.chat-markdown-body code) {
  background: rgba(255, 255, 255, 0.08);
  border: 1px solid rgba(255, 255, 255, 0.15);
  padding: 1px 4px;
  font-size: 11px;
  color: var(--accent-color);
}
:deep(.chat-markdown-body pre) {
  background: rgba(0, 0, 0, 0.5);
  border: 1px solid var(--border-color);
  padding: 8px 10px;
  margin: 0.5rem 0;
  overflow-x: auto;
  font-size: 11px;
  line-height: 1.35;
}
:deep(.chat-markdown-body pre code) {
  background: transparent;
  border: none;
  padding: 0;
  color: var(--fg-color);
}
:deep(.chat-markdown-body h1),
:deep(.chat-markdown-body h2),
:deep(.chat-markdown-body h3),
:deep(.chat-markdown-body h4) {
  font-weight: bold;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--accent-color);
  margin-top: 0.5rem;
  margin-bottom: 0.25rem;
}
:deep(.chat-markdown-body blockquote) {
  border-left: 2px solid var(--accent-color);
  padding-left: 0.75rem;
  opacity: 0.85;
  margin: 0.5rem 0;
}
</style>
