import { reportAsyncFailure } from "../utils/reportAsyncFailure";
import { daemonBaseUrl, daemonCall } from "../utils/daemonClient";
import { bindDaemonEventSource, onDaemonEvent, type AnyDaemonEvent } from "../utils/daemonEvents";
import { runNavigationGuard } from "../utils/navigationGuard";
import { useDialogueStore } from "./dialogueStore";
import { useWorkspaceLayoutStore } from "./workspaceLayoutStore";
import { useProjectExplorerStore } from "./projectExplorerStore";
import { useJournalStore } from "./journalStore";
import { useAgentMetricsStore } from "./agentMetricsStore";
import { useAccountStore } from "./useAccountStore";
import { defineStore } from 'pinia';
import type { Raw } from 'vue';
import { invoke } from "@tauri-apps/api/core";
import type {
  ProjectRecord, 
  ProjectAgentState, 
  ProjectInitFlagRecord, 
  PendingApproval,
  SystemMetrics,
  ServiceMetrics,
  HostCapabilities
} from "../types";

export type SystemTelemetryData = SystemMetrics;
export type { ServiceMetrics };

let _unlisteners: (() => void)[] = [];
let _sseReconnectTimer: ReturnType<typeof setTimeout> | null = null;
let _daemonEventSource: EventSource | null = null;

/** Daemon events handled by `handleDaemonEvent`. */
const DAEMON_EVENT_TYPES = [
  "system-telemetry",
  "services-telemetry",
  "running-services-updated",
  "agent-states-updated",
  "agent-status-report",
  "agent-step-update",
  "ApprovalRequested",
  "AgentResponse",
  "AccountQuotaUpdated",
  "opendictate_session_cancelled",
  "opendictate_session_finished",
  "accounts-updated",
  "new-account-detected",
  "account-switched",
  "projects-updated",
  "init-flag-resolved",
  "init-flag-raised",
] as const;

/**
 * Health of the MCP daemon, a required subsystem: the app does not work without it, so anything
 * but `connected` is an error state shown in the header (no in-app substitute exists).
 */
export interface DaemonStatus {
  state: 'connecting' | 'connected' | 'unreachable' | 'build_mismatch';
  detail: string | null;
}

export const useCoreStore = defineStore('core', {
  state: () => ({
    listenersInitialized: false,
    daemonStatus: { state: 'connecting', detail: null } as DaemonStatus,
    daemonBaseUrl: null as string | null,
    globalMetrics: {
      cpu: "--%",
      ram: "-- / --",
      swap: "-- / --",
      disk: "-- / --",
      net: "↓ -- ↑ --",
      cpu_cores: [],
      cpu_usage_percent: 0,
      ram_percent: 0,
      ram_used_gbytes: 0,
      ram_total_gbytes: 0,
      swap_percent: 0,
      swap_used_gbytes: 0,
      swap_total_gbytes: 0,
      disk_percent: 0,
      disk_used_gbytes: 0,
      disk_total_gbytes: 0,
      net_rx_kbytes_per_sec: 0,
      net_tx_kbytes_per_sec: 0,
    } as SystemTelemetryData,
    servicesTelemetry: {} as Record<string, ServiceMetrics>,
    // `Raw` keeps Pinia from unwrapping the recursive `JsonValue` of script options at the type level.
    projects: [] as Raw<ProjectRecord>[],
    agentStates: {} as Record<number, ProjectAgentState>,
    projectInitFlags: {} as Record<number, ProjectInitFlagRecord[]>,
    agentOutcomes: {} as Record<string, { status: string; timestamp: number; summary?: string }>,
    statusLogs: [] as string[],
    runningServices: [] as string[],
    pendingApproval: null as PendingApproval | null,
    initializingProjects: {} as Record<number, { 
      isInitializing: boolean; 
      status?: 'running' | 'completed' | 'failed'; 
      message: string;
      report?: string;
    }>,
    activeProjectId: (typeof localStorage !== 'undefined' ? (Number(localStorage.getItem('oneship_active_project_id')) || null) : null) as number | null,
    selectedTerminalKeys: {} as Record<number, string[]>,
    terminalFilterModes: {} as Record<string, 'all' | 'errors_only'>,
    hostCapabilities: null as HostCapabilities | null,
  }),
  getters: {
    activeProject(state): ProjectRecord | null {
      if (state.activeProjectId === null) return null;
      return state.projects.find(p => p.id === state.activeProjectId) ?? null;
    },
    isIntegrationAvailable(state): (pluginId: string) => boolean {
      return (pluginId: string): boolean => {
        if (!state.hostCapabilities) return true;
        const entry = state.hostCapabilities.integrations.find(i => i.plugin_id === pluginId);
        return entry ? entry.available : true;
      };
    },
    isOpendictateAvailable(state): boolean {
      return state.hostCapabilities ? state.hostCapabilities.opendictate_available : false;
    },
    installedRunners(state): string[] | null {
      return state.hostCapabilities ? state.hostCapabilities.installed_runners : null;
    },
  },
  actions: {
    /** Outcomes are stored per (project, agent); only the project-less system agent uses its bare name. */
    agentOutcomeKey(agentId: string, projectPath?: string | null): string {
      const lower = agentId.toLowerCase();
      return projectPath ? `${projectPath}::${lower}` : lower;
    },

    getAgentOutcome(agentId: string, projectPath?: string | null): { status: string; timestamp: number; summary?: string } | null {
      return this.agentOutcomes[this.agentOutcomeKey(agentId, projectPath)] || null;
    },

    setActiveProject(projectId: number | null, options?: { skipDaemonSync?: boolean; bypassNavigationGuard?: boolean }) {
      if (this.activeProjectId === projectId) return;
      if (!options?.bypassNavigationGuard && !runNavigationGuard({ projectId })) return;
      const previousProjectId = this.activeProjectId;
      this.activeProjectId = projectId;
      if (typeof localStorage !== 'undefined') {
        if (projectId !== null) {
          localStorage.setItem('oneship_active_project_id', String(projectId));
        } else {
          localStorage.removeItem('oneship_active_project_id');
        }
      }
      const dialogueStore = useDialogueStore();
      const layoutStore = useWorkspaceLayoutStore();
      const explorerStore = useProjectExplorerStore();
      const journalStore = useJournalStore();
      const p = projectId !== null ? (this.projects.find(proj => proj.id === projectId) || null) : null;

      // Coordinate per-project UI workspace state snapshot & restoration
      layoutStore.switchProjectContext(previousProjectId, projectId, p);
      if (!options?.skipDaemonSync && projectId !== null) {
        layoutStore.syncSelectProject(projectId);
      }

      if (p) {
        const reportFailure = (operation: string) => (err: unknown) =>
          console.error(`[coreStore] ${operation} failed for project ${p.id}:`, err);
        explorerStore.fetchProjectDocuments(p.path).catch(reportFailure('fetchProjectDocuments'));
        journalStore.fetchSessionJournals(p.path).catch(reportFailure('fetchSessionJournals'));
        journalStore.fetchProjectTodos(p.path).catch(reportFailure('fetchProjectTodos'));
        journalStore.fetchProjectTechnicalDebt(p.path).catch(reportFailure('fetchProjectTechnicalDebt'));
        this.fetchInitFlags(p.path ? p.id : projectId!).catch(reportFailure('fetchInitFlags'));
      } else {
        dialogueStore.clearProjectState();
      }
    },
    /** The registered project at `path`, if any. */
    projectByPath(path: string): ProjectRecord | undefined {
      return this.projects.find(p => p.path === path);
    },
    setProjects(projs: ProjectRecord[]) {
      this.projects = projs;
      // A selection that no longer exists is cleared; the user picks explicitly.
      // No project is ever auto-selected by position in the list.
      if (this.activeProjectId !== null && !this.projects.some(p => p.id === this.activeProjectId)) {
        this.setActiveProject(null, { bypassNavigationGuard: true });
      }
    },
    toggleTerminalKey(projectId: number, key: string) {
      if (!this.selectedTerminalKeys[projectId]) {
        this.selectedTerminalKeys[projectId] = [key];
        return;
      }
      const list = this.selectedTerminalKeys[projectId];
      const idx = list.indexOf(key);
      if (idx >= 0) {
        list.splice(idx, 1);
      } else {
        list.push(key);
      }
    },
    setTerminalFilterMode(key: string, mode: 'all' | 'errors_only') {
      this.terminalFilterModes[key] = mode;
    },
    addStatusLog(msg: string) {
      this.statusLogs.push(msg);
      if (this.statusLogs.length > 5) {
        this.statusLogs.shift();
      }
      // Persist to system logs so it shows up in the Log Viewer
      daemonCall("write_client_log", { level: "INFO", module: "client", message: msg }).catch(reportAsyncFailure);
    },
    async initTauriListeners() {
      if (this.listenersInitialized) return;
      this.listenersInitialized = true;

      // Events of the daemon, which owns every process, loop and agent (spec 12 §7).
      for (const eventType of DAEMON_EVENT_TYPES) {
        _unlisteners.push(onDaemonEvent(eventType, (payload) => {
          this.handleDaemonEvent({ event_type: eventType, payload } as AnyDaemonEvent);
        }));
      }
      _unlisteners.push(onDaemonEvent("resync", (payload) => {
        // The daemon skipped events this client was too slow to read: reload state from it.
        this.addStatusLog(`Daemon event stream resync (${payload.missed_count} events missed)`);
        this.resyncFromDaemon().catch(reportAsyncFailure);
      }));

      this.addStatusLog("Event listeners initialized");
      this.initDaemonSse().catch(reportAsyncFailure);
      useAgentMetricsStore().syncGlobalConfig().catch(reportAsyncFailure);
      this.checkPendingApprovals().catch(reportAsyncFailure);
      useAccountStore().fetchAccounts().catch(reportAsyncFailure);
    },
    teardownListeners() {
      _unlisteners.forEach((fn) => fn());
      _unlisteners = [];
      this.listenersInitialized = false;
      if (_sseReconnectTimer) {
        clearTimeout(_sseReconnectTimer);
        _sseReconnectTimer = null;
      }
      if (_daemonEventSource) {
        _daemonEventSource.close();
        _daemonEventSource = null;
        bindDaemonEventSource(null);
      }
      this.addStatusLog("Event listeners torn down");
    },
    /** Base URL of the MCP daemon, from `mcp.mcp_daemon_port`. Throws until `initDaemonSse` resolved it. */
    requireDaemonBaseUrl(): string {
      if (!this.daemonBaseUrl) {
        throw new Error("MCP daemon URL not resolved yet");
      }
      return this.daemonBaseUrl;
    },
    /** Reloads the list of tracked and hydrated projects from the daemon. */
    async reloadProjects() {
      this.projects = await daemonCall("get_projects", {});
    },
    /** Pinned / starred project favorite toggle */
    async toggleProjectFavorite(projectId: number, isFavorite: boolean) {
      await daemonCall("toggle_project_favorite", { projectId, isFavorite });
      const proj = this.projects.find(p => p.id === projectId);
      if (proj) {
        proj.is_favorite = isFavorite;
      }
      await this.reloadProjects();
    },
    /** Archive / hide project from active project lists */
    async toggleProjectArchived(projectId: number, isArchived: boolean) {
      await daemonCall("toggle_project_archived", { projectId, isArchived });
      const proj = this.projects.find(p => p.id === projectId);
      if (proj) {
        proj.is_archived = isArchived;
      }
      await this.reloadProjects();
      if (isArchived && this.activeProjectId === projectId) {
        if (!runNavigationGuard({ projectId: null })) return;
        this.activeProjectId = null;
      }
    },
    /** Workspace tab visibility toggle per project */
    async toggleProjectTabVisibility(projectId: number, tab: string, visible: boolean) {
      const updatedTabs = await daemonCall("toggle_project_tab_visibility", { projectId, tab, visible });
      const proj = this.projects.find(p => p.id === projectId);
      if (proj) {
        proj.hidden_tabs = updatedTabs;
      }
    },
    /** Reloads host binary, runner, audio, and desktop capabilities from the daemon. */
    async fetchHostCapabilities(): Promise<HostCapabilities> {
      const caps = await daemonCall("get_host_capabilities", {});
      this.hostCapabilities = caps;
      return caps;
    },
    /** Reloads the state carried by daemon events after a `resync` (missed events cannot be replayed). */
    async resyncFromDaemon() {
      await this.reloadProjects();
      await this.checkPendingApprovals();
      this.fetchHostCapabilities().catch(reportAsyncFailure);
    },
    setDaemonStatus(state: DaemonStatus['state'], detail: string | null) {
      if (this.daemonStatus.state === state && this.daemonStatus.detail === detail) return;
      this.daemonStatus = { state, detail };
      if (detail) this.addStatusLog(`ERROR: ${detail}`);
    },
    /** Fails loudly when the daemon was built from different sources than this app. */
    async checkDaemonBuild() {
      const appBuildId = await invoke<string>("get_build_id_cmd");
      const response = await fetch(`${this.requireDaemonBaseUrl()}/api/build`);
      if (!response.ok) {
        throw new Error(`Daemon predates build checks (HTTP ${response.status}); restart it with scripts/run_dev.sh`);
      }
      const daemon: { build_id: string } = await response.json();
      if (daemon.build_id !== appBuildId) {
        throw new Error(`Daemon build ${daemon.build_id} differs from app build ${appBuildId}; restart it with scripts/run_dev.sh`);
      }
    },
    /** Restores the newest pending approval from SQLite, the single source of approvals. */
    async checkPendingApprovals() {
      const approvals = await daemonCall("get_pending_approvals", {});
      if (approvals.length === 0) return;
      const latest = approvals[0];
      this.pendingApproval = {
        task_id: latest.task_id,
        phase: latest.phase,
        payload_markdown: latest.payload_markdown,
        project_path: latest.project_path,
      };
      this.addStatusLog(`Hydrated pending approval: ${latest.phase}`);
    },
    async initDaemonSse() {
      if (_daemonEventSource) {
        _daemonEventSource.close();
        _daemonEventSource = null;
      }
      this.daemonBaseUrl = await daemonBaseUrl();
      const unreachableDetail = `ONE-SHIP daemon is not running at ${this.daemonBaseUrl}; start it with scripts/run_dev.sh`;
      const connectSse = () => {
        try {
          const es = new EventSource(`${this.requireDaemonBaseUrl()}/api/events`);
          _daemonEventSource = es;
          bindDaemonEventSource(es);

          es.onopen = () => {
            this.addStatusLog("MCP Daemon SSE Event Stream Connected");
            this.setDaemonStatus('connected', null);
            this.checkPendingApprovals().catch(reportAsyncFailure);
            this.fetchHostCapabilities().catch(reportAsyncFailure);
            this.checkDaemonBuild().catch((err: unknown) => {
              this.setDaemonStatus('build_mismatch', err instanceof Error ? err.message : String(err));
              reportAsyncFailure(err);
            });
          };

          es.onerror = () => {
            es.close();
            if (_daemonEventSource === es) {
              _daemonEventSource = null;
              bindDaemonEventSource(null);
            }
            // The app is in an error state until the daemon is back; keep probing for it.
            this.setDaemonStatus('unreachable', unreachableDetail);
            if (_sseReconnectTimer) clearTimeout(_sseReconnectTimer);
            _sseReconnectTimer = setTimeout(connectSse, 3000);
          };
        } catch (err) {
          this.setDaemonStatus('unreachable', `${unreachableDetail} (${err instanceof Error ? err.message : String(err)})`);
          if (_sseReconnectTimer) clearTimeout(_sseReconnectTimer);
          _sseReconnectTimer = setTimeout(connectSse, 3000);
        }
      };

      connectSse();
    },
    async submitApprovalResponse(taskId: string, response: string): Promise<boolean> {
      try {
        const res = await fetch(`${this.requireDaemonBaseUrl()}/api/approval-response`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ task_id: taskId, response })
        });
        const data = await res.json();
        if (data.status === "ok") {
          this.addStatusLog(`Approval response sent for task: ${taskId}`);
          this.pendingApproval = null;
          return true;
        } else {
          this.addStatusLog(`Approval response error: ${data.message}`);
          return false;
        }
      } catch (err: unknown) {
        const m = err instanceof Error ? err.message : String(err);
        this.addStatusLog(`Failed to send approval response: ${m}`);
        return false;
      }
    },
    async rollbackProjectInit(projectPath: string): Promise<boolean> {
      try {
        await daemonCall("rollback_project_init", { projectPath });
        this.addStatusLog(`Rolled back initialization artifacts for: ${projectPath}`);
        this.projects = await daemonCall("get_projects", {});
        return true;
      } catch (err: unknown) {
        const m = err instanceof Error ? err.message : String(err);
        this.addStatusLog(`Rollback failed: ${m}`);
        return false;
      }
    },
    async fetchInitFlags(projectId: number) {
      try {
        this.projectInitFlags[projectId] = await daemonCall("get_project_init_flags", { projectId, status: null });
      } catch (err) {
        console.error("Failed to fetch init flags:", err);
      }
    },
    setProjectInitializing(
      projectId: number, 
      isInitializing: boolean, 
      message = "Initializing orchestration...", 
      status: 'running' | 'completed' | 'failed' = 'running'
    ) {
      this.initializingProjects[projectId] = { isInitializing, status, message };
    },
    clearProjectInitializing(projectId: number) {
      delete this.initializingProjects[projectId];
    },
    handleDaemonEvent(event: AnyDaemonEvent) {
      const accountStore = useAccountStore();
      switch (event.event_type) {
        case "system-telemetry":
          this.globalMetrics = event.payload;
          break;
        case "services-telemetry":
          this.servicesTelemetry = event.payload || {};
          break;
        case "projects-updated":
          this.projects = event.payload;
          break;
        case "running-services-updated":
          this.runningServices = event.payload;
          break;
        case "agent-states-updated":
          this.agentStates = event.payload;
          break;
        case "accounts-updated":
          accountStore.setAccounts(event.payload || []);
          accountStore.fetchDeepSeekBalance().catch(reportAsyncFailure);
          break;
        case "new-account-detected":
          this.addStatusLog(`New account detected: ${event.payload?.email || 'unknown'}`);
          accountStore.isAddingAccount = false;
          accountStore.fetchAccounts().catch(reportAsyncFailure);
          break;
        case "account-switched":
          this.addStatusLog(`Account active identity switched to: ${event.payload?.email || 'unknown'}`);
          accountStore.fetchAccounts().catch(reportAsyncFailure);
          break;
        case "agent-status-report":
          // An outcome belongs to one (project, agent); a report that cannot say which is not attributed.
          if (!event.payload.project_path && event.payload.agent_id !== 'system') {
            this.addStatusLog(`ERROR: agent-status-report from '${event.payload.agent_id}' carries no project_path and was not attributed`);
            break;
          }
          this.agentOutcomes[this.agentOutcomeKey(event.payload.agent_id, event.payload.project_path)] = {
            status: event.payload.status,
            timestamp: event.payload.timestamp,
            summary: event.payload.summary ?? undefined,
          };
          this.addStatusLog(`[Agent ${event.payload.agent_id}] ${event.payload.status?.toUpperCase?.() || 'REPORT'}: ${event.payload.summary || 'Status update'}`);
          // Observe initializer / init agent status
          if (event.payload.agent_id === 'initializer' || event.payload.agent_id === 'init') {
            for (const pidStr in this.initializingProjects) {
              const pid = Number(pidStr);
              if (this.projects.find(p => p.id === pid)?.path !== event.payload.project_path) continue;
              if (this.initializingProjects[pid]?.isInitializing) {
                if (event.payload.status === 'completed' || event.payload.status === 'done') {
                  this.initializingProjects[pid].isInitializing = false;
                  this.initializingProjects[pid].status = 'completed';
                  this.initializingProjects[pid].message = event.payload.summary || 'Orchestration initialized successfully.';
                  this.initializingProjects[pid].report = event.payload.summary ?? undefined;
                  this.fetchInitFlags(pid).catch(reportAsyncFailure);
                  daemonCall("get_projects", {}).then((projs) => { this.projects = projs; }).catch(reportAsyncFailure);
                } else if (event.payload.status === 'error' || event.payload.status === 'failed') {
                  this.initializingProjects[pid].isInitializing = false;
                  this.initializingProjects[pid].status = 'failed';
                  this.initializingProjects[pid].message = event.payload.summary || 'Initialization failed.';
                  daemonCall("get_projects", {}).then((projs) => { this.projects = projs; }).catch(reportAsyncFailure);
                } else if (event.payload.status === 'working' && event.payload.summary) {
                  this.initializingProjects[pid].message = event.payload.summary;
                }
              }
            }
          }
          break;
        case "ApprovalRequested":
          this.pendingApproval = {
            task_id: event.payload.task_id,
            phase: event.payload.phase,
            payload_markdown: event.payload.payload_markdown,
            project_path: event.payload.project_path,
            agent_id: event.payload.agent_id,
            options: event.payload.options ?? undefined,
          };
          this.addStatusLog(`Approval requested for Phase: ${event.payload?.phase || 'Unknown'}`);
          break;
        case "init-flag-raised":
        case "init-flag-resolved":
          for (const p of this.projects) {
            this.fetchInitFlags(p.id).catch(reportAsyncFailure);
          }
          break;
        case "AgentResponse":
          useDialogueStore().handleAgentResponse(event.payload);
          break;
      }
    }
  }
});
