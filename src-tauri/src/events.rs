//! Typed server-sent event payloads broadcast over `/api/events` (Spec 12 §7).
//!
//! Every event emitted by `oneship-daemon` is constructed through a typed [`DaemonEvent`]
//! constructor in this module and exported to TypeScript via `ts-rs`, mirroring the
//! `DaemonEvents` map in `src/utils/daemonEvents.ts`.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// Canonical list of all SSE event names emitted by `oneship-daemon`.
/// Kept in sync with `DaemonEvents` in `src/utils/daemonEvents.ts` (enforced by test).
pub const DAEMON_EVENT_TYPES: &[&str] = &[
    "AccountQuotaUpdated",
    "AgentResponse",
    "ApprovalRequested",
    "DocumentCommentResolved",
    "SessionMinutesRecorded",
    "account-quotas-updated",
    "account-switched",
    "accounts-updated",
    "agent-conversation-turn-added",
    "agent-error",
    "agent-states-updated",
    "agent-status-report",
    "agent-step-update",
    "agent-usage-update",
    "conversation-topic-switched",
    "desigma-file-changed",
    "dictation-cancelled",
    "dictation-finished",
    "dictation-interim-text",
    "dictation-state",
    "forge-inbox-updated",
    "init-flag-raised",
    "init-flag-resolved",
    "keyring-warmup-status",
    "new-account-detected",
    "pcbcraft-file-changed",
    "project-files-changed",
    "project-milestones-updated",
    "project-tasks-updated",
    "projects-updated",
    "rea-queue-updated",
    "resync",
    "running-services-updated",
    "services-telemetry",
    "solidcraft-file-changed",
    "system-telemetry",
    "task-completed",
    "task-output-chunk",
    "tech-debt-updated",
    "todo-updated",
    "ui-state-updated",
    "ui-surface-command",
    "voice-controller-state",
];

/// Envelope broadcast over the internal Tokio channel and serialized as an SSE frame.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DaemonEvent {
    pub event_type: String,
    pub payload: Value,
}

impl DaemonEvent {
    fn from_serializable<T: Serialize>(event_type: &'static str, payload: &T) -> Self {
        debug_assert!(
            DAEMON_EVENT_TYPES.contains(&event_type),
            "unregistered daemon event type: {event_type}"
        );
        Self {
            event_type: event_type.to_string(),
            payload: serde_json::to_value(payload).unwrap_or(Value::Null),
        }
    }

    pub fn system_telemetry(payload: &crate::telemetry::SystemMetrics) -> Self {
        Self::from_serializable("system-telemetry", payload)
    }

    pub fn services_telemetry(payload: &HashMap<String, crate::telemetry::ServiceMetrics>) -> Self {
        Self::from_serializable("services-telemetry", payload)
    }

    pub fn running_services_updated(payload: &[String]) -> Self {
        Self::from_serializable("running-services-updated", &payload)
    }

    pub fn agent_states_updated(payload: &HashMap<usize, crate::supervisor::ProjectAgentState>) -> Self {
        Self::from_serializable("agent-states-updated", payload)
    }

    pub fn projects_updated(payload: &[crate::hydration::HydratedProject]) -> Self {
        Self::from_serializable("projects-updated", &payload)
    }

    pub fn accounts_updated(payload: &[crate::db::AccountSummary]) -> Self {
        Self::from_serializable("accounts-updated", &payload)
    }

    pub fn new_account_detected(payload: &crate::db::AccountSummary) -> Self {
        Self::from_serializable("new-account-detected", payload)
    }

    pub fn account_switched(payload: &AccountSwitchedPayload) -> Self {
        Self::from_serializable("account-switched", payload)
    }

    pub fn account_quota_updated(payload: &AccountQuotaUpdatedPayload) -> Self {
        Self::from_serializable("AccountQuotaUpdated", payload)
    }

    pub fn account_quotas_updated(payload: &[crate::db::AccountQuotaRecord]) -> Self {
        Self::from_serializable("account-quotas-updated", &payload)
    }

    pub fn keyring_warmup_status(payload: &KeyringWarmupStatusPayload) -> Self {
        Self::from_serializable("keyring-warmup-status", payload)
    }

    pub fn agent_status_report(payload: &AgentStatusReportPayload) -> Self {
        Self::from_serializable("agent-status-report", payload)
    }

    pub fn agent_step_update(payload: &AgentStepUpdatePayload) -> Self {
        Self::from_serializable("agent-step-update", payload)
    }

    pub fn agent_usage_update(payload: &AgentUsageUpdatePayload) -> Self {
        Self::from_serializable("agent-usage-update", payload)
    }

    pub fn agent_error(payload: &AgentErrorPayload) -> Self {
        Self::from_serializable("agent-error", payload)
    }

    pub fn agent_conversation_turn_added(payload: &AgentConversationTurnAddedPayload) -> Self {
        Self::from_serializable("agent-conversation-turn-added", payload)
    }

    pub fn conversation_topic_switched(payload: &ConversationTopicSwitchedPayload) -> Self {
        Self::from_serializable("conversation-topic-switched", payload)
    }

    pub fn agent_response(payload: &AgentResponsePayload) -> Self {
        Self::from_serializable("AgentResponse", payload)
    }

    pub fn approval_requested(payload: &ApprovalRequestedPayload) -> Self {
        Self::from_serializable("ApprovalRequested", payload)
    }

    pub fn session_minutes_recorded(payload: &SessionMinutesRecordedPayload) -> Self {
        Self::from_serializable("SessionMinutesRecorded", payload)
    }

    pub fn document_comment_resolved(payload: &DocumentCommentResolvedPayload) -> Self {
        Self::from_serializable("DocumentCommentResolved", payload)
    }

    pub fn todo_updated(payload: &TodoUpdatedPayload) -> Self {
        Self::from_serializable("todo-updated", payload)
    }

    pub fn project_tasks_updated(payload: &ProjectTasksUpdatedPayload) -> Self {
        Self::from_serializable("project-tasks-updated", payload)
    }

    pub fn project_milestones_updated(payload: &ProjectMilestonesUpdatedPayload) -> Self {
        Self::from_serializable("project-milestones-updated", payload)
    }

    pub fn tech_debt_updated(payload: &TechDebtUpdatedPayload) -> Self {
        Self::from_serializable("tech-debt-updated", payload)
    }

    pub fn init_flag_raised(payload: &InitFlagRaisedPayload) -> Self {
        Self::from_serializable("init-flag-raised", payload)
    }

    pub fn init_flag_resolved(payload: &InitFlagResolvedPayload) -> Self {
        Self::from_serializable("init-flag-resolved", payload)
    }

    pub fn task_output_chunk(payload: &TaskOutputChunkPayload) -> Self {
        Self::from_serializable("task-output-chunk", payload)
    }

    pub fn task_completed(payload: &TaskCompletedPayload) -> Self {
        Self::from_serializable("task-completed", payload)
    }

    pub fn rea_queue_updated(payload: &ReaQueueEventPayload) -> Self {
        Self::from_serializable("rea-queue-updated", payload)
    }

    pub fn desigma_file_changed(payload: &DesigmaFileChangedPayload) -> Self {
        Self::from_serializable("desigma-file-changed", payload)
    }

    pub fn solidcraft_file_changed(payload: &SolidcraftFileChangedPayload) -> Self {
        Self::from_serializable("solidcraft-file-changed", payload)
    }

    pub fn dictation_state(payload: &crate::dictation::DictationStatePayload) -> Self {
        Self::from_serializable("dictation-state", payload)
    }

    pub fn dictation_interim_text(payload: &crate::dictation::DictationInterimPayload) -> Self {
        Self::from_serializable("dictation-interim-text", payload)
    }

    pub fn dictation_finished(payload: &crate::dictation::DictationFinishedPayload) -> Self {
        Self::from_serializable("dictation-finished", payload)
    }

    pub fn dictation_cancelled(payload: &crate::dictation::DictationCancelledPayload) -> Self {
        Self::from_serializable("dictation-cancelled", payload)
    }

    pub fn project_files_changed(payload: &ProjectFilesChangedPayload) -> Self {
        Self::from_serializable("project-files-changed", payload)
    }

    pub fn pcbcraft_file_changed(payload: &PcbcraftFileChangedPayload) -> Self {
        Self::from_serializable("pcbcraft-file-changed", payload)
    }

    pub fn ui_state_updated(payload: &crate::ui_state::UiStateSnapshot) -> Self {
        Self::from_serializable("ui-state-updated", payload)
    }

    pub fn ui_surface_command(payload: &UiSurfaceCommandPayload) -> Self {
        Self::from_serializable("ui-surface-command", payload)
    }

    pub fn voice_controller_state(payload: &crate::voice_controller::VoiceControllerStatus) -> Self {
        Self::from_serializable("voice-controller-state", payload)
    }

    pub fn forge_inbox_updated(payload: &ForgeInboxUpdatedPayload) -> Self {
        Self::from_serializable("forge-inbox-updated", payload)
    }
}

/// Payload of `forge-inbox-updated` emitted when Forge inbox read/flag state changes (Spec 38/39).
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ForgeInboxUpdatedPayload {
    #[ts(type = "number")]
    pub project_id: usize,
    pub provider: crate::kopeng::git::ForgeProviderKind,
    pub unread_count: usize,
    pub updated_count: usize,
}

/// Payload of `ui-surface-command` for dispatching panel-scoped frontend actions (Spec 36).
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UiSurfaceCommandPayload {
    #[ts(type = "number")]
    pub project_id: i64,
    pub node_path: String,
    pub action_id: String,
    pub params: Value,
}

/// Payload of `resync` when a slow SSE client misses broadcast messages.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct DaemonResyncPayload {
    #[ts(type = "number")]
    pub missed_count: u64,
}

/// Payload of `account-switched`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct AccountSwitchedPayload {
    pub account_id: String,
    pub email: String,
}

/// Payload of `AccountQuotaUpdated`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, TS)]
#[ts(export)]
pub struct AccountQuotaUpdatedPayload {
    pub account_id: Option<String>,
    pub model: Option<String>,
    #[ts(type = "number | null")]
    pub probed_at: Option<i64>,
    pub source: Option<String>,
    pub agent_id: Option<String>,
}

/// Payload of `keyring-warmup-status`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct KeyringWarmupStatusPayload {
    pub in_progress: bool,
    pub account_id: String,
    pub email: Option<String>,
    pub pool: String,
}

/// Payload of `agent-status-report`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, TS)]
#[ts(export)]
pub struct AgentStatusReportPayload {
    pub agent_id: String,
    pub project_path: Option<String>,
    pub status: String,
    pub summary: Option<String>,
    pub details: Option<String>,
    pub task_id: Option<String>,
    pub needs_user_input: Option<bool>,
    #[ts(type = "number")]
    pub timestamp: i64,
}

/// Payload of `agent-step-update`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct AgentStepUpdatePayload {
    pub agent_id: String,
    pub project_path: String,
    pub conversation_id: Option<String>,
    pub state: String,
    pub step_type: String,
    pub tool_name: Option<String>,
    pub action_summary: String,
    pub thought_text: Option<String>,
    #[ts(type = "number")]
    pub timestamp_ms: u64,
}

/// Payload of `agent-usage-update`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct AgentUsageUpdatePayload {
    pub agent_id: String,
    pub project_path: String,
    pub runner: String,
    pub model: Option<String>,
    #[ts(type = "number")]
    pub tokens_used_count: u64,
    #[ts(type = "number | null")]
    pub max_context_tokens_count: Option<u64>,
    pub cost_dollars: Option<f64>,
    #[ts(type = "number")]
    pub timestamp_ms: u64,
}

/// Payload of `agent-error`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct AgentErrorPayload {
    pub agent_id: String,
    pub project_path: String,
    pub error_type: String,
    pub message: String,
    pub exit_code: i32,
    #[ts(type = "number")]
    pub timestamp_ms: u64,
}

/// Payload of `agent-conversation-turn-added`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct AgentConversationTurnAddedPayload {
    #[ts(type = "number | null")]
    pub turn_id: Option<i64>,
    #[ts(type = "number")]
    pub project_id: i64,
    pub project_path: String,
    pub agent_id: String,
    pub conversation_id: String,
    pub role: String,
    pub dialogue: String,
    #[ts(type = "number | null")]
    pub timestamp: Option<i64>,
}

/// Payload of `conversation-topic-switched`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct ConversationTopicSwitchedPayload {
    #[ts(type = "number")]
    pub project_id: i64,
    pub project_path: String,
    pub agent_id: String,
    pub conversation_id: String,
    pub topic: String,
    pub status: String,
}

/// Interactive approval card embedded inside an [`AgentResponsePayload`].
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct AgentResponseApprovalPayload {
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub options: Option<Vec<String>>,
    pub status: Option<String>,
}

/// Payload of `AgentResponse`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct AgentResponsePayload {
    pub agent_id: String,
    #[ts(type = "number | null")]
    pub project_id: Option<i64>,
    pub project_path: String,
    pub conversation_id: Option<String>,
    #[ts(type = "number | null")]
    pub turn_id: Option<i64>,
    pub dialogue: String,
    pub artifact: Option<String>,
    pub artifact_type: Option<String>,
    pub artifact_lang: Option<String>,
    pub file_path: Option<String>,
    pub is_ephemeral: bool,
    pub approval: Option<AgentResponseApprovalPayload>,
    pub agy_conversation_id: Option<String>,
    #[ts(type = "number")]
    pub timestamp: i64,
}

/// Payload of `ApprovalRequested`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct ApprovalRequestedPayload {
    pub task_id: String,
    pub agent_id: String,
    #[ts(type = "number")]
    pub project_id: i64,
    pub project_path: String,
    pub approval_type: String,
    pub title: String,
    pub description: String,
    pub options: Option<Vec<String>>,
    pub phase: String,
    pub payload_markdown: String,
}

/// Payload of `SessionMinutesRecorded`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct SessionMinutesRecordedPayload {
    #[ts(type = "number")]
    pub project_id: i64,
    pub project_path: String,
    pub agent_id: String,
    #[ts(type = "number")]
    pub journal_id: i64,
    pub agy_conversation_id: String,
    pub title: String,
    pub context_pct: Option<f64>,
    #[ts(type = "number | null")]
    pub total_tokens: Option<i64>,
    pub runner: Option<String>,
    pub model: Option<String>,
    pub cost_dollars: Option<f64>,
}

/// Payload of `DocumentCommentResolved`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct DocumentCommentResolvedPayload {
    pub comment_id: String,
    pub resolved_by: Option<String>,
    #[ts(type = "number | null")]
    pub resolved_at: Option<i64>,
}

/// Payload of `todo-updated`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct TodoUpdatedPayload {
    #[ts(type = "number")]
    pub project_id: i64,
    pub project_path: String,
    #[ts(type = "number")]
    pub todo_id: i64,
    pub action: String,
    pub agent_id: String,
}

/// Payload of `tech-debt-updated`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct TechDebtUpdatedPayload {
    #[ts(type = "number")]
    pub project_id: i64,
    pub project_path: String,
    #[ts(type = "number")]
    pub debt_id: i64,
    pub action: String,
    pub agent_id: String,
}

/// Payload of `init-flag-raised`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct InitFlagRaisedPayload {
    #[ts(type = "number")]
    pub flag_id: i64,
    #[ts(type = "number")]
    pub project_id: i64,
    pub flag_type: String,
}

/// Payload of `init-flag-resolved`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct InitFlagResolvedPayload {
    #[ts(type = "number")]
    pub flag_id: i64,
    pub status: String,
    #[ts(type = "number")]
    pub project_id: i64,
    pub project_path: String,
    pub flag_type: String,
    pub agent_id: Option<String>,
}

/// Payload of `task-output-chunk`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct TaskOutputChunkPayload {
    #[ts(type = "number")]
    pub project_id: i64,
    pub task_id: String,
    #[ts(type = "number")]
    pub exec_id: i64,
    pub line: String,
    pub is_stderr: bool,
}

/// Payload of `task-completed`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct TaskCompletedPayload {
    #[ts(type = "number")]
    pub project_id: i64,
    pub task_id: String,
    #[ts(type = "number")]
    pub exec_id: i64,
    pub exit_code: i32,
    #[ts(type = "number")]
    pub duration_ms: i64,
    pub status: String,
}

/// Payload of `rea-queue-updated`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct ReaQueueEventPayload {
    pub jobs: Vec<crate::kopeng::rea::ReaQueuedJob>,
    #[ts(type = "number")]
    pub timestamp_ms: u64,
}

/// Payload of `desigma-file-changed`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct DesigmaFileChangedPayload {
    pub project_path: String,
    pub file_name: String,
    #[ts(type = "number")]
    pub modified_ms: u64,
}

/// Payload of `solidcraft-file-changed`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct SolidcraftFileChangedPayload {
    pub project_path: String,
    pub file_name: String,
    #[ts(type = "number")]
    pub modified_ms: u64,
}

/// Payload of `pcbcraft-file-changed`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct PcbcraftFileChangedPayload {
    pub project_path: String,
    pub target_name: String,
    #[ts(type = "number")]
    pub modified_ms: u64,
}

/// Payload of `project-files-changed`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct ProjectFilesChangedPayload {
    pub project_path: String,
    pub changed_count: usize,
}

/// Payload of `project-tasks-updated`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct ProjectTasksUpdatedPayload {
    pub project_path: String,
    #[ts(type = "number")]
    pub project_id: i64,
}

/// Payload of `project-milestones-updated`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct ProjectMilestonesUpdatedPayload {
    pub project_path: String,
    #[ts(type = "number")]
    pub project_id: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frontend_event_names() -> Vec<String> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/utils/daemonEvents.ts");
        let source = std::fs::read_to_string(&path).expect("read src/utils/daemonEvents.ts");
        let start = source.find("export interface DaemonEvents {").expect("DaemonEvents interface");
        source[start..]
            .lines()
            .skip(1)
            .take_while(|l| !l.starts_with('}'))
            .filter_map(|l| {
                let trimmed = l.trim();
                if trimmed.is_empty() || trimmed.starts_with("//") {
                    return None;
                }
                let (key, _) = trimmed.split_once(':')?;
                Some(key.trim().trim_matches('"').trim_matches('\'').to_string())
            })
            .collect()
    }

    #[test]
    fn daemon_events_match_the_frontend_map() {
        let mut backend: Vec<String> = DAEMON_EVENT_TYPES.iter().map(|s| s.to_string()).collect();
        let mut frontend = frontend_event_names();
        backend.sort();
        frontend.sort();
        assert_eq!(backend, frontend);
    }
}
