<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from "vue";
import { reportAsyncFailure } from "./utils/reportAsyncFailure";
import { useCoreStore } from "./stores/coreStore";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useI18n } from 'vue-i18n';
import type { AgentPersonaInput, DiagnosticState, ProjectRecord } from './types';
import { daemonCall } from './utils/daemonClient';
import type { AgentPersonaState } from './components/modals/AddPersonaModal.vue';

// Modular Components
import AppHeader from './components/AppHeader.vue';
import ConfirmDeleteModal from './components/modals/ConfirmDeleteModal.vue';
import RemovalFailedModal, { type RemovalFailure } from './components/modals/RemovalFailedModal.vue';
import RemoteVcsModal from './components/modals/RemoteVcsModal.vue';
import SettingsModal from './components/modals/SettingsModal.vue';
import AddPersonaModal from './components/modals/AddPersonaModal.vue';
import TaskReportModal from './components/modals/TaskReportModal.vue';
import ProjectInitModal from './components/modals/ProjectInitModal.vue';
import AddProjectModal from './components/modals/AddProjectModal.vue';
import SwitchConfirmationModal from './components/modals/SwitchConfirmationModal.vue';
import ApprovalRequestModal from './components/modals/ApprovalRequestModal.vue';
import OmnisearchModal from './components/modals/OmnisearchModal.vue';
import AboutModal from './components/modals/AboutModal.vue';
import SetupChecklistModal from './components/modals/SetupChecklistModal.vue';
import { ONBOARDING_DISMISSED_KEY } from './utils/integrationGuides';
import RollingStatusLog from './components/floating/RollingStatusLog.vue';
import ProjectWorkspace from './components/workspace/ProjectWorkspace.vue';
import FloatingChatOverlay from './components/dialogue/FloatingChatOverlay.vue';
import { useDialogueStore } from './stores/dialogueStore';
import { useWorkspaceLayoutStore, type WorkspaceTab } from './stores/workspaceLayoutStore';
import { useAccountStore } from './stores/useAccountStore';
import { useAgentMetricsStore } from './stores/agentMetricsStore';
import { useVoiceStore } from './stores/voiceStore';
import { useVoiceControllerStore } from './stores/voiceControllerStore';
import { emitUiEvent, onUiEvent } from './utils/uiEvents';

const { locale } = useI18n();
const coreStore = useCoreStore();

type WindowResizeDirection = Parameters<ReturnType<typeof getCurrentWindow>['startResizeDragging']>[0];

const showWindowControls = computed(() => !(coreStore.hostCapabilities?.hyprland_available ?? false));

const WINDOW_RESIZE_EDGES: ReadonlyArray<{ direction: WindowResizeDirection; cls: string }> = [
  { direction: 'North', cls: 'top-0 left-2 right-2 h-1 cursor-ns-resize' },
  { direction: 'South', cls: 'bottom-0 left-2 right-2 h-1 cursor-ns-resize' },
  { direction: 'West', cls: 'left-0 top-2 bottom-2 w-1 cursor-ew-resize' },
  { direction: 'East', cls: 'right-0 top-2 bottom-2 w-1 cursor-ew-resize' },
  { direction: 'NorthWest', cls: 'top-0 left-0 w-2 h-2 cursor-nwse-resize' },
  { direction: 'NorthEast', cls: 'top-0 right-0 w-2 h-2 cursor-nesw-resize' },
  { direction: 'SouthWest', cls: 'bottom-0 left-0 w-2 h-2 cursor-nesw-resize' },
  { direction: 'SouthEast', cls: 'bottom-0 right-0 w-2 h-2 cursor-nwse-resize' },
];

function startWindowResize(direction: WindowResizeDirection, e: MouseEvent) {
  if (!showWindowControls.value || e.button !== 0) return;
  getCurrentWindow().startResizeDragging(direction).catch(reportAsyncFailure);
}
const dialogueStore = useDialogueStore();
const layoutStore = useWorkspaceLayoutStore();
const accountStore = useAccountStore();
const metricsStore = useAgentMetricsStore();

function openSystemLogs() {
  emitUiEvent('open-status-log');
}

const statusLogs = computed(() => coreStore.statusLogs);
function addStatusLog(msg: string) {
  coreStore.addStatusLog(msg);
}

// Language Toggle
function toggleLanguage() {
  if (locale.value === 'en') {
    locale.value = 'belta_translated';
  } else if (locale.value === 'belta_translated') {
    locale.value = 'belta_pure';
  } else {
    locale.value = 'en';
  }
}

// System Diagnostics & Theme
const diagnostics = ref<DiagnosticState | null>(null);
const diagnosticsError = ref<string | null>(null);
const isInstallingTools = ref(false);
const showSetupModal = ref(false);
let onboardingChecked = false;

const isAllDiagnosticsOk = computed(() => {
  const d = diagnostics.value;
  if (!d) return false;
  const hasIdentity = Boolean(d.git_user_name?.trim() && d.git_user_email?.trim());
  const hasRunner = (coreStore.installedRunners?.length ?? 0) > 0;
  return Boolean(d.git_installed && hasIdentity && d.projects_root_exists && hasRunner);
});

async function runDiagnostics() {
  try {
    diagnostics.value = await daemonCall("run_diagnostics", {});
    diagnosticsError.value = null;
    if (!onboardingChecked && coreStore.hostCapabilities !== null) {
      onboardingChecked = true;
      if (!isAllDiagnosticsOk.value && localStorage.getItem(ONBOARDING_DISMISSED_KEY) !== '1') {
        showSetupModal.value = true;
      }
    }
  } catch (e: unknown) {
    diagnostics.value = null;
    diagnosticsError.value = String(e);
  }
}

/** Installs missing tools via pkexec (Polkit asks for consent), then re-runs the diagnostics. */
async function installTools(toolIds: string[]) {
  isInstallingTools.value = true;
  try {
    await daemonCall("install_missing_tools", { toolIds });
    addStatusLog(`Installed: ${toolIds.join(', ')}`);
  } catch (e: unknown) {
    addStatusLog(`Tool installation failed: ${String(e)}`);
  } finally {
    isInstallingTools.value = false;
    await runDiagnostics();
    coreStore.fetchHostCapabilities().catch(reportAsyncFailure);
  }
}

async function syncTheme() {
  try {
    const colors = await daemonCall("get_active_theme", {});
    const root = document.documentElement;
    root.style.colorScheme = colors.mode === 'light' ? 'light' : 'dark';
    root.dataset.themeMode = colors.mode;
    root.style.setProperty('--bg-color', colors.bg_color);
    root.style.setProperty('--fg-color', colors.fg_color);
    root.style.setProperty('--dim-color', colors.dim_color);
    root.style.setProperty('--border-color', colors.border_color);
    root.style.setProperty('--accent-color', colors.accent_color);
    root.style.setProperty('--status-ok-color', colors.ok_color);
    root.style.setProperty('--status-warn-color', colors.warn_color);
    root.style.setProperty('--status-error-color', colors.error_color);
    root.style.setProperty('--status-info-color', colors.info_color);

    if (colors.auto_sync && !themeInterval) {
      themeInterval = setInterval(syncTheme, 5000);
    } else if (!colors.auto_sync && themeInterval) {
      clearInterval(themeInterval);
      themeInterval = null;
    }
  } catch (e) {
    reportAsyncFailure(e);
  }
}

// Real-time Global Metrics & Projects
const globalMetrics = computed(() => coreStore.globalMetrics);
const projects = computed(() => coreStore.projects);
const agentStates = computed(() => coreStore.agentStates);
const agentOutcomes = computed(() => coreStore.agentOutcomes);
let themeInterval: ReturnType<typeof setInterval> | null = null;

let isUpdatingAgents = false;

async function updateAgentStates() {
  if (isUpdatingAgents) return;
  isUpdatingAgents = true;
  try {
    await updateAllAgentStates();
  } catch (e) {
    console.warn('[App] updateAllAgentStates failed:', e);
  } finally {
    isUpdatingAgents = false;
  }
}

// Service Orchestration
const runningServices = computed(() => coreStore.runningServices);

// Project Operations
const activeProjectMenuId = ref<number | null>(null);

function toggleProjectMenu(projectId: number) {
  if (activeProjectMenuId.value === projectId) {
    activeProjectMenuId.value = null;
  } else {
    activeProjectMenuId.value = projectId;
  }
}

const showAddProjectModal = ref(false);

function addProject() {
  showAddProjectModal.value = true;
}

async function handleProjectAdded(project: ProjectRecord, shouldInit?: boolean) {
  try {
    coreStore.setProjects(await daemonCall("get_projects", {}));
    if (project?.id) {
      coreStore.setActiveProject(project.id);
    }
    await updateAgentStates();
    addStatusLog(`Added project: ${project?.path || project?.name || 'Unknown'}`);
    if (shouldInit && project) {
      triggerInitProject(project);
    }
  } catch (e) {
    addStatusLog("Error adding project: " + e);
  }
}

async function handleGreenfieldCreated(project: ProjectRecord) {
  try {
    coreStore.setProjects(await daemonCall("get_projects", {}));
    if (project?.id) {
      coreStore.setActiveProject(project.id);
    }
    await updateAgentStates();
    addStatusLog(`Created greenfield project: ${project?.name} (${project?.path})`);
    
    // Automatically open the Floating Chat Overlay with the Initializer agent
    await dialogueStore.openOverlay('initializer', project?.path);
  } catch (e) {
    addStatusLog("Error finalizing greenfield project: " + e);
  }
}

async function initGit(project: ProjectRecord) {
  try {
    await daemonCall("init_git_repo", { projectId: project.id });
    coreStore.setProjects(await daemonCall("get_projects", {}));
    addStatusLog(`Initialized git in: ${project.path}`);
  } catch (e) {
    addStatusLog("Error initializing Git: " + e);
  }
}

async function updateAllAgentStates() {
  for (const p of projects.value) {
    try {
      const st = await daemonCall("get_project_agent_state", {
        projectPath: p.path,
        projectName: p.name
      });
      coreStore.agentStates[p.id] = st;
    } catch (e) {
      console.error("Failed to load agent state:", e);
    }
  }
}

async function refreshProjects() {
  coreStore.setProjects(await daemonCall("get_projects", {}));
  await updateAllAgentStates();
}

// Project Initialization Modal
const projectToInit = ref<ProjectRecord | null>(null);

function triggerInitProject(project: ProjectRecord) {
  projectToInit.value = project;
}

async function syncPlatformAssets(project: ProjectRecord) {
  if (!project?.path) return;
  try {
    addStatusLog(`Synchronizing platform assets & skills for ${project.name}...`);
    if (project.skill_status?.needs_migration) {
      const migRes = await daemonCall("migrate_project_deterministic", {
        projectPath: project.path,
      });
      addStatusLog(migRes.message);
    } else {
      const res = await daemonCall("inject_platform_skill", {
        projectPath: project.path,
      });
      addStatusLog(res);
    }
    await refreshProjects();
  } catch (err: unknown) {
    console.error("Failed to sync platform assets:", err);
    const msg = err instanceof Error ? err.message : String(err);
    addStatusLog(`Error syncing platform assets: ${msg}`);
  }
}

// Delete Project Modal
const projectToDelete = ref<ProjectRecord | null>(null);

function requestDeleteProject(project: ProjectRecord) {
  activeProjectMenuId.value = null;
  projectToDelete.value = project;
}

async function confirmDeleteProject(payload: { project: ProjectRecord; purgeTraces: boolean }) {
  const project = payload.project;
  const purgeTraces = payload.purgeTraces ?? false;
  try {
    await daemonCall("remove_project", { id: project.id, purgeTraces });
    layoutStore.deleteProjectState(project.id);
    coreStore.setProjects(await daemonCall("get_projects", {}));
    projectToDelete.value = null;
    if (purgeTraces) {
      addStatusLog(`De-integrated project & purged traces: ${project.name}`);
    } else {
      addStatusLog(`Unregistered project from hub: ${project.name}`);
    }
  } catch (e) {
    // The project stays registered; the user cancels or hands the failure to the removal agent.
    projectToDelete.value = null;
    removalFailure.value = { project, purgeTraces, report: String(e) };
    addStatusLog("Error removing project: " + e);
  }
}

const removalFailure = ref<RemovalFailure | null>(null);

async function resolveRemovalWithAgent() {
  const failure = removalFailure.value;
  if (!failure) return;
  removalFailure.value = null;
  try {
    const agentId = await daemonCall('launch_project_removal_agent', {
      projectId: failure.project.id,
      purgeTraces: failure.purgeTraces,
      failureReport: failure.report,
    });
    await dialogueStore.openOverlay(agentId, failure.project.path);
    addStatusLog(`Removal of ${failure.project.name} handed to ${agentId}`);
  } catch (e) {
    addStatusLog("Error launching removal agent: " + e);
  }
}

// GitHub Remote Modal
const githubModalProject = ref<ProjectRecord | null>(null);

function openGithubModal(project: ProjectRecord) {
  githubModalProject.value = project;
}

// Add / Edit Agent Persona Modal
const addPersonaProject = ref<ProjectRecord | null>(null);
const editingPersonaTarget = ref<{ project: ProjectRecord; persona: AgentPersonaState } | null>(null);

function openAddPersonaModal(project: ProjectRecord) {
  editingPersonaTarget.value = null;
  addPersonaProject.value = project;
}

function openEditPersonaModal(payload: { project: ProjectRecord; agent: AgentPersonaState }) {
  addPersonaProject.value = null;
  editingPersonaTarget.value = {
    project: payload.project,
    persona: payload.agent,
  };
}

function closePersonaModal() {
  addPersonaProject.value = null;
  editingPersonaTarget.value = null;
}

const isSavingPersona = ref(false);

async function handleSavePersona(payload: AgentPersonaInput) {
  const isUpdate = !!editingPersonaTarget.value;
  const targetProject = addPersonaProject.value || editingPersonaTarget.value?.project;
  if (!targetProject) return;

  isSavingPersona.value = true;
  try {
    await daemonCall(isUpdate ? "update_agent_persona" : "create_agent_persona", {
      projectId: targetProject.id,
      agent: payload,
    });
    addStatusLog(`Agent persona '${payload.id}' saved to rula.toml`);
    closePersonaModal();
    daemonCall("get_project_agent_state", {
      projectPath: targetProject.path,
      projectName: targetProject.name,
    }).then((st) => {
      coreStore.agentStates[targetProject.id] = st;
    }).catch(reportAsyncFailure);
  } catch (e: unknown) {
    addStatusLog(`Error saving persona: ${String(e)}`);
  } finally {
    isSavingPersona.value = false;
  }
}

async function handleDeletePersona(payload: { agentId: string }) {
  const targetProject = editingPersonaTarget.value?.project || addPersonaProject.value;
  if (!targetProject) return;

  try {
    // The backend stops the agent first and keeps the persona when the stop fails.
    await daemonCall("delete_agent_persona", {
      projectId: targetProject.id,
      agentId: payload.agentId,
    });
    addStatusLog(`Agent persona '${payload.agentId}' deleted`);
    closePersonaModal();
    const st = await daemonCall("get_project_agent_state", {
      projectPath: targetProject.path,
      projectName: targetProject.name,
    });
    coreStore.agentStates[targetProject.id] = st;
  } catch (e: unknown) {
    addStatusLog(`Error deleting persona: ${String(e)}`);
  }
}

async function triggerVCSAction(project: ProjectRecord) {
  coreStore.setActiveProject(project.id);
  layoutStore.setWorkspaceTab('git');
}

// Scoped Settings Modal
const showSettingsModal = ref(false);
const isOmnisearchOpen = ref(false);
const omnisearchInitialQuery = ref('');
const showAboutModal = ref(false);

function openOmnisearch(initialQuery = '') {
  omnisearchInitialQuery.value = initialQuery;
  isOmnisearchOpen.value = true;
}
const settingsScope = ref<'global' | 'project'>('global');
const settingsProject = ref<ProjectRecord | null>(null);

function openSettingsModal(scope: 'global' | 'project' = 'global', project?: ProjectRecord) {
  activeProjectMenuId.value = null;
  if (scope === 'project' && project) {
    coreStore.setActiveProject(project.id);
    layoutStore.setWorkspaceTab('settings');
    return;
  }
  settingsScope.value = 'global';
  settingsProject.value = null;
  showSettingsModal.value = true;
}

function onSettingsSaved(scope: 'global' | 'project', name?: string) {
  if (scope === 'project') {
    addStatusLog(`Saved project settings for ${name} (rula.toml)`);
  } else {
    addStatusLog("Saved global settings (~/.config/one-ship/config.toml)");
  }
}

// Task Report and Execution History Modal
const taskReportProject = ref<ProjectRecord | null>(null);
const taskReportInitialExecId = ref<number | null>(null);

function openTaskHistory(project: ProjectRecord) {
  taskReportProject.value = project;
  taskReportInitialExecId.value = null;
}

function openTaskReport(project: ProjectRecord, execId: number) {
  taskReportProject.value = project;
  taskReportInitialExecId.value = execId;
}

// Global Click Handler
function onGlobalClick(e: MouseEvent) {
  const target = e.target as HTMLElement;
  if (!target.closest('.project-menu-container')) {
    activeProjectMenuId.value = null;
  }
}

// Global Context Menu Blocker (prevents browser/webview default menu)
function onGlobalContextMenu(e: MouseEvent) {
  e.preventDefault();
}

async function onWindowFocus() {
  await updateAgentStates();
}

function onGlobalKeydown(e: KeyboardEvent) {
  const voiceController = useVoiceControllerStore();
  if (
    voiceController.isPttHeld &&
    ((e.altKey && (e.key === 'Backspace' || e.code === 'Backspace')) || e.key === 'Escape')
  ) {
    e.preventDefault();
    voiceController.cancelPushToTalk();
    return;
  }
  if (e.altKey && (e.key === 'v' || e.key === 'V')) {
    e.preventDefault();
    if (!e.repeat) {
      void voiceController.toggleRecording();
    }
    return;
  }
  if ((e.ctrlKey || e.metaKey) && (e.key === 'k' || e.key === 'K')) {
    e.preventDefault();
    isOmnisearchOpen.value = !isOmnisearchOpen.value;
    return;
  }
  if (e.altKey && e.key === '1') {
    e.preventDefault();
    layoutStore.setWorkspaceTab('overview');
  } else if (e.altKey && e.key === '2') {
    e.preventDefault();
    if (layoutStore.isFloatingChatOpen) {
      layoutStore.toggleFloatingChatMinimize();
    } else {
      layoutStore.openFloatingChat(dialogueStore.activeAgentId || 'system', coreStore.activeProject?.path);
    }
  } else if (e.altKey && e.key === '3') {
    e.preventDefault();
    layoutStore.setWorkspaceTab('files');
  } else if (e.altKey && e.key === '4') {
    e.preventDefault();
    layoutStore.setWorkspaceTab('devlog');
  } else if (e.altKey && e.key === '5') {
    e.preventDefault();
    layoutStore.setWorkspaceTab('git');
  } else if (e.altKey && e.key === '6') {
    e.preventDefault();
    layoutStore.setWorkspaceTab('terminal');
  } else if (e.altKey && e.key === '7') {
    e.preventDefault();
    layoutStore.setWorkspaceTab('profiler');
  } else if (e.altKey && e.key === '8') {
    e.preventDefault();
    layoutStore.setWorkspaceTab('skills');
  } else if (e.altKey && e.key === '9') {
    e.preventDefault();
    layoutStore.setWorkspaceTab('rea');
  } else if ((e.altKey && e.key.toLowerCase() === 'w') || (e.ctrlKey && e.key === 'Tab')) {
    e.preventDefault();
    const tabs: WorkspaceTab[] = ['overview', 'files', 'devlog', 'git', 'terminal', 'profiler', 'skills'];
    if (coreStore.activeProject?.has_rea_enabled) tabs.push('rea');
    if (coreStore.activeProject?.has_electronics_enabled) tabs.push('electronics');
    const currIdx = tabs.indexOf(layoutStore.activeWorkspaceTab);
    const nextIdx = (currIdx + 1) % tabs.length;
    layoutStore.setWorkspaceTab(tabs[nextIdx]);
  }
}

// Resizing logic for docked chat column (native pointer capture in vw)
const isResizingChat = ref(false);
let startChatResizeX = 0;
let startChatResizeVw = 28;
let activePointerCleanup: (() => void) | null = null;

function stopChatResize() {
  if (activePointerCleanup) {
    activePointerCleanup();
    activePointerCleanup = null;
  }
}

function startChatResize(e: PointerEvent) {
  e.preventDefault();
  stopChatResize();
  const target = e.currentTarget as HTMLElement | null;
  const pointerId = e.pointerId;
  if (target && target.setPointerCapture) {
    try {
      target.setPointerCapture(pointerId);
    } catch (err) {
      console.debug('[App] setPointerCapture skipped:', err);
    }
  }
  isResizingChat.value = true;
  startChatResizeX = e.clientX;
  startChatResizeVw = Number(layoutStore.dockedChatWidthVw) || 28;
  document.body.style.cursor = 'col-resize';
  document.body.style.userSelect = 'none';

  const onPointerMove = (ev: PointerEvent) => {
    if (!isResizingChat.value) return;
    // Dragging left increases docked chat width; dragging right decreases it
    const deltaPx = startChatResizeX - ev.clientX;
    const deltaVw = (deltaPx / window.innerWidth) * 100;
    layoutStore.setDockedChatWidthVw(startChatResizeVw + deltaVw);
  };

  const onPointerUp = () => {
    stopChatResize();
  };

  activePointerCleanup = () => {
    isResizingChat.value = false;
    if (target && target.releasePointerCapture) {
      try {
        target.releasePointerCapture(pointerId);
      } catch (err) {
        console.debug('[App] releasePointerCapture skipped:', err);
      }
    }
    window.removeEventListener('pointermove', onPointerMove);
    window.removeEventListener('pointerup', onPointerUp);
    window.removeEventListener('pointercancel', onPointerUp);
    if (target) {
      target.removeEventListener('pointermove', onPointerMove as EventListener);
      target.removeEventListener('pointerup', onPointerUp as EventListener);
      target.removeEventListener('pointercancel', onPointerUp as EventListener);
    }
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
  };

  window.addEventListener('pointermove', onPointerMove);
  window.addEventListener('pointerup', onPointerUp);
  window.addEventListener('pointercancel', onPointerUp);
  if (target) {
    target.addEventListener('pointermove', onPointerMove as EventListener);
    target.addEventListener('pointerup', onPointerUp as EventListener);
    target.addEventListener('pointercancel', onPointerUp as EventListener);
  }
}

onMounted(async () => {
  // Initial State Hydration
  accountStore.fetchAccounts().catch(reportAsyncFailure);
  metricsStore.fetchAccountQuotas().catch(reportAsyncFailure);
  coreStore.setProjects(await daemonCall("get_projects", {}));
  await updateAllAgentStates();
  await layoutStore.initUiState();
  runDiagnostics();
  coreStore.fetchHostCapabilities().catch(reportAsyncFailure);
  await syncTheme();

  // Global Kopeng Plugin Event Bus
  const uiUnlisteners: (() => void)[] = [
    onUiEvent('oneship-add-persona', (proj) => { if (proj) openAddPersonaModal(proj); }),
    onUiEvent('oneship-edit-persona', (payload) => { openEditPersonaModal(payload); }),
    onUiEvent('oneship-open-history', (proj) => { if (proj) openTaskHistory(proj); }),
    onUiEvent('oneship-open-task-report', (detail) => { openTaskReport(detail.project, detail.execId); }),
    onUiEvent('oneship-refresh-project', () => { refreshProjects(); }),
    onUiEvent('switch-to-workspace-terminal', () => { layoutStore.setWorkspaceTab('terminal'); }),
  ];

  // Initialize Native Tauri IPC Event Listeners via Pinia Store
  coreStore.initTauriListeners();
  dialogueStore.initListener();
  useVoiceControllerStore().initListeners().catch(reportAsyncFailure);

  let unlistenResize: (() => void) | null = null;
  try {
    const appWindow = getCurrentWindow();
    appWindow.onResized(() => {
      window.dispatchEvent(new Event('resize'));
    }).then((unlisten) => {
      unlistenResize = unlisten;
    }).catch(reportAsyncFailure);
  } catch (e) {
    console.debug('[App] Tauri window resize listener unavailable:', e);
  }

  function applyAppearancePreferences() {
    const savedFont = localStorage.getItem('oneship-body-font') || 'IBM Plex Mono';
    document.documentElement.style.setProperty('--app-font-family', `'${savedFont}', monospace`);

    const savedScaleStr = localStorage.getItem('oneship-font-scale') || '100% (Normal)';
    const scaleMap: Record<string, number> = {
      '90% (Compact)': 0.9,
      '100% (Normal)': 1.0,
      '110% (Medium)': 1.1,
      '120% (Large)': 1.2,
      '130% (Extra Large)': 1.3,
    };
    const scaleVal = scaleMap[savedScaleStr] ?? 1.0;
    document.documentElement.style.setProperty('--app-font-scale', String(scaleVal));
  }

  applyAppearancePreferences();
  uiUnlisteners.push(onUiEvent('oneship-appearance-updated', () => {
    applyAppearancePreferences();
    syncTheme();
  }));

  window.addEventListener('keydown', onGlobalKeydown);
  window.addEventListener('focus', onWindowFocus);
  window.addEventListener('click', onGlobalClick);
  window.addEventListener('contextmenu', onGlobalContextMenu);

  onUnmounted(() => {
    if (unlistenResize) unlistenResize();
    uiUnlisteners.forEach(u => u());
    window.removeEventListener('keydown', onGlobalKeydown);
    window.removeEventListener('focus', onWindowFocus);
    window.removeEventListener('click', onGlobalClick);
    window.removeEventListener('contextmenu', onGlobalContextMenu);
    coreStore.teardownListeners();
    dialogueStore.teardownListeners();
    layoutStore.teardown();
    useVoiceControllerStore().teardown();
    useVoiceStore().teardown();
    stopChatResize();
    if (themeInterval) clearInterval(themeInterval);
  });
});
</script>

<template>
  <div class="h-full w-full min-h-0 min-w-0 overflow-hidden bg-[var(--bg-color)] text-[var(--fg-color)] font-mono flex flex-col p-2 sm:p-3 selection:bg-[var(--accent-color)] selection:text-[var(--bg-color)] relative">
    <!-- Frameless Window Resize Handles (non-Hyprland desktops) -->
    <template v-if="showWindowControls">
      <div
        v-for="edge in WINDOW_RESIZE_EDGES"
        :key="edge.direction"
        class="fixed z-[9990]"
        :class="edge.cls"
        @mousedown="startWindowResize(edge.direction, $event)"
      />
    </template>
    
    <!-- Global Topbar (Machine Metrics & Unified Toolbar) -->
    <AppHeader 
      :global-metrics="globalMetrics"
      :diagnostics="diagnostics"
      :diagnostics-error="diagnosticsError"
      :is-installing-tools="isInstallingTools"
      :is-diagnostics-ok="isAllDiagnosticsOk"
      @toggle-language="toggleLanguage"
      @install-tools="installTools"
      @open-settings="() => openSettingsModal('global')"
      @open-setup="showSetupModal = true"
      @open-logs="openSystemLogs"
      @open-about="showAboutModal = true"
      @open-omnisearch="openOmnisearch"
    />

    <!-- ── MAIN CONTENT VIEWPORT (Project Workspace + Docked Chat) ────── -->
    <div class="flex-1 flex overflow-hidden min-h-0 relative">
      <!-- Main Workspace Stage Area -->
      <div class="flex-1 flex min-w-0 min-h-0 h-full overflow-hidden relative">
        <ProjectWorkspace 
          :projects="projects"
          :active-project-menu-id="activeProjectMenuId"
          :agent-states="agentStates"
          :agent-outcomes="agentOutcomes"
          :running-services="runningServices"
          @add-project="addProject"
          @init-project="triggerInitProject"
          @sync-skills="syncPlatformAssets"
          @open-github="openGithubModal"
          @trigger-vcs="triggerVCSAction"
          @toggle-menu="toggleProjectMenu"
          @open-settings="(p) => openSettingsModal('project', p)"
          @delete-project="requestDeleteProject"
          @init-git="initGit"
        />
      </div>

      <!-- Docked Chat Resizer Handle with Expanded Grab Zone -->
      <div
        v-if="layoutStore.isFloatingChatOpen && !layoutStore.isFloatingChatMinimized && layoutStore.chatLayoutMode === 'docked'"
        class="w-1 h-full cursor-col-resize hover:bg-[var(--accent-color)] active:bg-[var(--accent-color)] bg-[var(--border-color)]/60 transition-colors z-30 shrink-0 select-none relative before:absolute before:-inset-x-2 before:inset-y-0 before:z-50 before:cursor-col-resize"
        @pointerdown="startChatResize"
        @dblclick="layoutStore.setDockedChatWidthVw(28)"
      />

      <!-- Docked Collapsible Agent Chat Column (Right side) -->
      <div 
        v-if="layoutStore.isFloatingChatOpen && !layoutStore.isFloatingChatMinimized && layoutStore.chatLayoutMode === 'docked'"
        :style="{ width: `${layoutStore.dockedChatWidthVw || 28}vw`, minWidth: '280px', maxWidth: '60vw' }"
        class="shrink-0 border-l border-[var(--border-color)] bg-[var(--bg-color)] h-full z-20 flex flex-col overflow-hidden"
      >
        <FloatingChatOverlay />
      </div>
    </div>

    <!-- Global Drag Shield while Resizing -->
    <div 
      v-if="isResizingChat" 
      class="fixed inset-0 z-[9999] cursor-col-resize select-none pointer-events-none" 
    />

    <!-- ── GLOBAL FLOATING AGENT CONVERSATION OVERLAY (When in Floating Mode) ───────────── -->
    <FloatingChatOverlay v-if="layoutStore.chatLayoutMode === 'floating'" />

    <!-- System Logs Modal (Triggered by AppHeader or Event) -->
    <RollingStatusLog :status-logs="statusLogs" />

    <!-- Modals -->

    <ConfirmDeleteModal 
      :project="projectToDelete"
      @close="projectToDelete = null"
      @confirm="confirmDeleteProject"
    />

    <RemovalFailedModal
      :failure="removalFailure"
      @cancel="removalFailure = null"
      @resolve-with-agent="resolveRemovalWithAgent"
    />

    <RemoteVcsModal 
      :project="githubModalProject"
      :diagnostics="diagnostics"
      :is-installing-tools="isInstallingTools"
      @install-tools="installTools"
      @close="githubModalProject = null"
      @remote-changed="() => daemonCall('get_projects', {}).then(p => coreStore.projects = p)"
    />

    <SettingsModal 
      :show="showSettingsModal"
      :scope="settingsScope"
      :project="settingsProject"
      @close="showSettingsModal = false"
      @saved="onSettingsSaved"
    />

    <AddPersonaModal 
      :project="addPersonaProject || editingPersonaTarget?.project"
      :persona="editingPersonaTarget?.persona"
      :is-saving="isSavingPersona"
      @close="closePersonaModal"
      @save="handleSavePersona"
      @delete="handleDeletePersona"
    />

    <TaskReportModal
      v-if="taskReportProject"
      :project="taskReportProject"
      :initial-exec-id="taskReportInitialExecId"
      @close="taskReportProject = null"
    />

    <ProjectInitModal
      :show="projectToInit !== null"
      :project="projectToInit"
      @close="projectToInit = null"
      @initialized="refreshProjects"
    />

    <AddProjectModal
      :show="showAddProjectModal"
      @close="showAddProjectModal = false"
      @added="handleProjectAdded"
      @created-greenfield="handleGreenfieldCreated"
    />

    <ApprovalRequestModal />

    <SwitchConfirmationModal />

    <OmnisearchModal 
      :is-open="isOmnisearchOpen"
      :active-project-id="coreStore.activeProject?.id"
      :active-project-path="coreStore.activeProject?.path"
      :initial-query="omnisearchInitialQuery"
      @close="isOmnisearchOpen = false"
    />

    <AboutModal
      :show="showAboutModal"
      @close="showAboutModal = false"
    />

    <SetupChecklistModal
      :show="showSetupModal"
      :diagnostics="diagnostics"
      :is-installing-tools="isInstallingTools"
      @close="showSetupModal = false"
      @refresh-diagnostics="runDiagnostics"
      @install-tools="installTools"
      @open-integrations="() => { showSetupModal = false; openSettingsModal('global'); }"
    />

  </div>
</template>

<style>
@import "tailwindcss";

@font-face {
  font-family: 'Protest Guerrilla';
  src: url('/fonts/Protest_Guerrilla/ProtestGuerrilla-Regular.ttf') format('truetype');
  font-weight: 400;
  font-style: normal;
  font-display: swap;
}

@font-face {
  font-family: 'Anonymous Pro';
  src: url('/fonts/Anonymous_Pro/AnonymousPro-Regular.ttf') format('truetype');
  font-weight: 400;
  font-style: normal;
  font-display: swap;
  size-adjust: 112%;
}
@font-face {
  font-family: 'Anonymous Pro';
  src: url('/fonts/Anonymous_Pro/AnonymousPro-Bold.ttf') format('truetype');
  font-weight: 700;
  font-style: normal;
  font-display: swap;
  size-adjust: 112%;
}
@font-face {
  font-family: 'Anonymous Pro';
  src: url('/fonts/Anonymous_Pro/AnonymousPro-Italic.ttf') format('truetype');
  font-weight: 400;
  font-style: italic;
  font-display: swap;
  size-adjust: 112%;
}
@font-face {
  font-family: 'Anonymous Pro';
  src: url('/fonts/Anonymous_Pro/AnonymousPro-BoldItalic.ttf') format('truetype');
  font-weight: 700;
  font-style: italic;
  font-display: swap;
  size-adjust: 112%;
}

@font-face {
  font-family: 'Courier Prime';
  src: url('/fonts/Courier_Prime/CourierPrime-Regular.ttf') format('truetype');
  font-weight: 400;
  font-style: normal;
  font-display: swap;
  size-adjust: 115%;
}
@font-face {
  font-family: 'Courier Prime';
  src: url('/fonts/Courier_Prime/CourierPrime-Bold.ttf') format('truetype');
  font-weight: 700;
  font-style: normal;
  font-display: swap;
  size-adjust: 115%;
}
@font-face {
  font-family: 'Courier Prime';
  src: url('/fonts/Courier_Prime/CourierPrime-Italic.ttf') format('truetype');
  font-weight: 400;
  font-style: italic;
  font-display: swap;
  size-adjust: 115%;
}
@font-face {
  font-family: 'Courier Prime';
  src: url('/fonts/Courier_Prime/CourierPrime-BoldItalic.ttf') format('truetype');
  font-weight: 700;
  font-style: italic;
  font-display: swap;
  size-adjust: 115%;
}

:root {
  --app-font-family: 'IBM Plex Mono', monospace;
  --app-font-scale: 1;
  --title-font-family: 'Protest Guerrilla', sans-serif;
  --status-ok-color: #10b981;
  --status-ok-border: color-mix(in srgb, var(--status-ok-color) 60%, transparent);
  --status-ok-bg: color-mix(in srgb, var(--status-ok-color) 15%, transparent);
  --status-warn-color: #f59e0b;
  --status-warn-border: color-mix(in srgb, var(--status-warn-color) 40%, transparent);
  --status-warn-bg: color-mix(in srgb, var(--status-warn-color) 15%, transparent);
  --status-error-color: #ef4444;
  --status-error-border: color-mix(in srgb, var(--status-error-color) 60%, transparent);
  --status-error-bg: color-mix(in srgb, var(--status-error-color) 20%, transparent);
  --status-info-color: #38bdf8;
  --status-info-border: color-mix(in srgb, var(--status-info-color) 50%, transparent);
  --status-info-bg: color-mix(in srgb, var(--status-info-color) 15%, transparent);
}

.font-mono {
  font-family: var(--app-font-family), monospace !important;
}

.font-title {
  font-family: var(--title-font-family), sans-serif !important;
}

html {
  zoom: var(--app-font-scale, 1);
}

html,
body,
#app {
  height: 100%;
  width: 100%;
  margin: 0;
  padding: 0;
  overflow: hidden;
  box-sizing: border-box;
}

*, *::before, *::after {
  box-sizing: inherit;
}

/* Hide native scrollbar on main content area (replaced by ProjectMinimap) or when requested */
.scrollbar-none::-webkit-scrollbar {
  display: none;
}
.scrollbar-none {
  scrollbar-width: none !important;
  -ms-overflow-style: none !important;
}

/* Peeker rail slide transitions */
.peeker-slide-enter-active,
.peeker-slide-leave-active {
  transition: opacity 0.2s ease, width 0.2s ease, transform 0.2s ease;
}
.peeker-slide-enter-from,
.peeker-slide-leave-to {
  opacity: 0;
  width: 0;
  transform: scaleX(0.8);
}

/* Log transient fade transitions */
.log-fade-enter-active {
  transition: opacity 0.4s ease, max-height 0.4s ease;
}
.log-fade-leave-active {
  transition: opacity 0.6s ease, max-height 0.6s ease;
}
.log-fade-enter-from,
.log-fade-leave-to {
  opacity: 0;
  max-height: 0;
}
.log-fade-enter-to,
.log-fade-leave-from {
  opacity: 1;
  max-height: 2rem;
}

/* View panel slide transitions */
.view-slide-enter-active,
.view-slide-leave-active {
  transition: opacity 0.18s ease, transform 0.18s cubic-bezier(0.16, 1, 0.3, 1);
}
.view-slide-enter-from {
  opacity: 0;
  transform: translateX(18px);
}
.view-slide-leave-to {
  opacity: 0;
  transform: translateX(-18px);
}
</style>
