// Subscriptions to the daemon event stream (`GET /api/events`, spec 12 §7). The daemon owns every
// process and all state, so their events reach the app only here; `coreStore.initDaemonSse` owns
// the connection and binds each (re)connected EventSource.

import type { AccountQuotaRecord } from '../types/generated/AccountQuotaRecord';
import type { AccountQuotaUpdatedPayload } from '../types/generated/AccountQuotaUpdatedPayload';
import type { AccountSummary } from '../types/generated/AccountSummary';
import type { AccountSwitchedPayload } from '../types/generated/AccountSwitchedPayload';
import type { AgentConversationTurnAddedPayload } from '../types/generated/AgentConversationTurnAddedPayload';
import type { AgentErrorPayload } from '../types/generated/AgentErrorPayload';
import type { AgentResponsePayload } from '../types/generated/AgentResponsePayload';
import type { AgentStatusReportPayload } from '../types/generated/AgentStatusReportPayload';
import type { AgentStepUpdatePayload } from '../types/generated/AgentStepUpdatePayload';
import type { AgentUsageUpdatePayload } from '../types/generated/AgentUsageUpdatePayload';
import type { ApprovalRequestedPayload } from '../types/generated/ApprovalRequestedPayload';
import type { ConversationTopicSwitchedPayload } from '../types/generated/ConversationTopicSwitchedPayload';
import type { DaemonResyncPayload } from '../types/generated/DaemonResyncPayload';
import type { DesigmaFileChangedPayload } from '../types/generated/DesigmaFileChangedPayload';
import type { DictationCancelledPayload } from '../types/generated/DictationCancelledPayload';
import type { DictationFinishedPayload } from '../types/generated/DictationFinishedPayload';
import type { DictationInterimPayload } from '../types/generated/DictationInterimPayload';
import type { DictationStatePayload } from '../types/generated/DictationStatePayload';
import type { DocumentCommentResolvedPayload } from '../types/generated/DocumentCommentResolvedPayload';
import type { ForgeInboxUpdatedPayload } from '../types/generated/ForgeInboxUpdatedPayload';
import type { HydratedProject } from '../types/generated/HydratedProject';
import type { InitFlagRaisedPayload } from '../types/generated/InitFlagRaisedPayload';
import type { InitFlagResolvedPayload } from '../types/generated/InitFlagResolvedPayload';
import type { KeyringWarmupStatusPayload } from '../types/generated/KeyringWarmupStatusPayload';
import type { PcbcraftFileChangedPayload } from '../types/generated/PcbcraftFileChangedPayload';
import type { ProjectAgentState } from '../types/generated/ProjectAgentState';
import type { ProjectFilesChangedPayload } from '../types/generated/ProjectFilesChangedPayload';
import type { ProjectMilestonesUpdatedPayload } from '../types/generated/ProjectMilestonesUpdatedPayload';
import type { ProjectTasksUpdatedPayload } from '../types/generated/ProjectTasksUpdatedPayload';
import type { ReaQueueEventPayload } from '../types/generated/ReaQueueEventPayload';
import type { ServiceMetrics } from '../types/generated/ServiceMetrics';
import type { SessionMinutesRecordedPayload } from '../types/generated/SessionMinutesRecordedPayload';
import type { SolidcraftFileChangedPayload } from '../types/generated/SolidcraftFileChangedPayload';
import type { SystemMetrics } from '../types/generated/SystemMetrics';
import type { TaskCompletedPayload } from '../types/generated/TaskCompletedPayload';
import type { TaskOutputChunkPayload } from '../types/generated/TaskOutputChunkPayload';
import type { TechDebtUpdatedPayload } from '../types/generated/TechDebtUpdatedPayload';
import type { TodoUpdatedPayload } from '../types/generated/TodoUpdatedPayload';
import type { UiStateSnapshot } from '../types/generated/UiStateSnapshot';
import type { UiSurfaceCommandPayload } from '../types/generated/UiSurfaceCommandPayload';
import type { VoiceControllerStatus } from '../types/generated/VoiceControllerStatus';

/**
 * Event name → payload type table for every SSE event emitted by `oneship-daemon` (`src-tauri/src/events.rs`).
 * Kept in sync with `DAEMON_EVENT_TYPES` by the Rust unit test `daemon_events_match_the_frontend_map`.
 */
export interface DaemonEvents {
  "AccountQuotaUpdated": AccountQuotaUpdatedPayload;
  "AgentResponse": AgentResponsePayload;
  "ApprovalRequested": ApprovalRequestedPayload;
  "DocumentCommentResolved": DocumentCommentResolvedPayload;
  "SessionMinutesRecorded": SessionMinutesRecordedPayload;
  "account-quotas-updated": AccountQuotaRecord[];
  "account-switched": AccountSwitchedPayload;
  "accounts-updated": AccountSummary[];
  "agent-conversation-turn-added": AgentConversationTurnAddedPayload;
  "agent-error": AgentErrorPayload;
  "agent-states-updated": Record<number, ProjectAgentState>;
  "agent-status-report": AgentStatusReportPayload;
  "agent-step-update": AgentStepUpdatePayload;
  "agent-usage-update": AgentUsageUpdatePayload;
  "conversation-topic-switched": ConversationTopicSwitchedPayload;
  "desigma-file-changed": DesigmaFileChangedPayload;
  "forge-inbox-updated": ForgeInboxUpdatedPayload;
  "init-flag-raised": InitFlagRaisedPayload;
  "init-flag-resolved": InitFlagResolvedPayload;
  "keyring-warmup-status": KeyringWarmupStatusPayload;
  "new-account-detected": AccountSummary;
  "dictation-cancelled": DictationCancelledPayload;
  "dictation-finished": DictationFinishedPayload;
  "dictation-interim-text": DictationInterimPayload;
  "dictation-state": DictationStatePayload;
  "pcbcraft-file-changed": PcbcraftFileChangedPayload;
  "project-files-changed": ProjectFilesChangedPayload;
  "project-milestones-updated": ProjectMilestonesUpdatedPayload;
  "project-tasks-updated": ProjectTasksUpdatedPayload;
  "projects-updated": HydratedProject[];
  "rea-queue-updated": ReaQueueEventPayload;
  "resync": DaemonResyncPayload;
  "running-services-updated": string[];
  "services-telemetry": Record<string, ServiceMetrics>;
  "solidcraft-file-changed": SolidcraftFileChangedPayload;
  "system-telemetry": SystemMetrics;
  "task-completed": TaskCompletedPayload;
  "task-output-chunk": TaskOutputChunkPayload;
  "tech-debt-updated": TechDebtUpdatedPayload;
  "todo-updated": TodoUpdatedPayload;
  "ui-state-updated": UiStateSnapshot;
  "ui-surface-command": UiSurfaceCommandPayload;
  "voice-controller-state": VoiceControllerStatus;
}

export type DaemonEventName = keyof DaemonEvents;

export type AnyDaemonEvent = {
  [K in DaemonEventName]: { event_type: K; payload: DaemonEvents[K] };
}[DaemonEventName];

type Handler = (payload: unknown) => void;

const handlers = new Map<string, Set<Handler>>();
let source: EventSource | null = null;
const attachedNames = new Set<string>();

function dispatcherFor(name: string): (event: MessageEvent) => void {
  return (event: MessageEvent) => {
    let payload: unknown;
    try {
      payload = JSON.parse(event.data);
    } catch (err) {
      console.error(`[daemon events] '${name}' carried invalid JSON:`, err);
      return;
    }
    for (const handler of handlers.get(name) ?? []) {
      handler(payload);
    }
  };
}

function attach(name: string): void {
  if (!source || attachedNames.has(name)) return;
  source.addEventListener(name, dispatcherFor(name) as EventListener);
  attachedNames.add(name);
}

/** Binds the current EventSource, or `null` while the daemon is unreachable. */
export function bindDaemonEventSource(es: EventSource | null): void {
  source = es;
  attachedNames.clear();
  for (const name of handlers.keys()) attach(name);
}

/**
 * Like `onDaemonEvent`, with the handler receiving `{ payload }` (the shape of a Tauri event).
 * A rejected async handler is reported.
 */
export function listenDaemonEvent<K extends DaemonEventName>(
  name: K,
  handler: (event: { payload: DaemonEvents[K] }) => unknown,
): () => void {
  return onDaemonEvent(name, (payload) => {
    Promise.resolve(handler({ payload })).catch((err: unknown) => {
      console.error(`[daemon events] handler of '${name}' failed:`, err);
    });
  });
}

/**
 * Subscribes `handler` to the daemon event `name` and returns its unsubscribe function, which the
 * caller must invoke on teardown (AGENTS.md §10.1).
 */
export function onDaemonEvent<K extends DaemonEventName>(
  name: K,
  handler: (payload: DaemonEvents[K]) => void,
): () => void {
  let subscribers = handlers.get(name);
  if (!subscribers) {
    subscribers = new Set();
    handlers.set(name, subscribers);
  }
  const registered = handler as Handler;
  subscribers.add(registered);
  attach(name);
  return () => {
    subscribers.delete(registered);
  };
}
