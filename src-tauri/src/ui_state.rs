//! Declarative UI State Machine, Surface Tree Registry & Session Persistence (Spec 35).
//!
//! `oneship-daemon` is the single source of truth for workspace navigation, subsystem state slots,
//! and per-project agent chat panel lifecycles. Every transition is validated against the
//! declarative [`UiSurfaceRegistry`], persisted atomically to SQLite (`oneship.db`), and broadcast
//! over `/api/events` as `ui-state-updated`.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::api::ApiError;
use crate::db::{
    db_get_all_ui_project_states, db_get_ui_global_state, db_save_ui_state_atomic,
    DbUiGlobalState, DbUiProjectState,
};
use crate::events::DaemonEvent;
use crate::hydration::HydratedProject;
use crate::registry::get_projects;

const DEFAULT_NODE_PATH: &str = "overview";
const MIN_DOCKED_CHAT_WIDTH_VW: f64 = 16.0;
const MAX_DOCKED_CHAT_WIDTH_VW: f64 = 60.0;
const DEFAULT_DOCKED_CHAT_WIDTH_VW: f64 = 28.0;

/// Strongly-typed domain errors returned by the UI state machine.
#[derive(Debug, thiserror::Error)]
pub enum UiStateError {
    #[error("Project {project_id} is not registered")]
    ProjectNotFound { project_id: i64 },

    #[error("Unknown UI surface node path '{node_path}'")]
    UnknownNodePath { node_path: String },

    #[error("UI surface '{node_path}' requires capability '{capability}', which is disabled for project {project_id}")]
    CapabilityNotEnabled {
        project_id: i64,
        node_path: String,
        capability: String,
    },

    #[error("UI surface tab '{tab_id}' is hidden in project {project_id} configuration")]
    TabHidden { project_id: i64, tab_id: String },

    #[error("Unknown UI state slot '{key}'")]
    UnknownSlot { key: String },

    #[error("Invalid value '{value}' for enum slot '{key}'")]
    InvalidSlotValue { key: String, value: String },

    #[error("Agent identifier cannot be empty")]
    EmptyAgentId,

    #[error("Invalid chat panel transition '{action}' from state '{current_state:?}'")]
    InvalidChatTransition {
        action: &'static str,
        current_state: ChatPanelState,
    },

    #[error("Database error in UI state manager: {0}")]
    Database(#[from] crate::db::DatabaseError),

    #[error("Project hydration error: {0}")]
    Hydration(String),

    #[error("Failed to serialize UI state payload: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl ApiError for UiStateError {
    fn status(&self) -> StatusCode {
        match self {
            Self::ProjectNotFound { .. } => StatusCode::NOT_FOUND,
            Self::UnknownNodePath { .. }
            | Self::UnknownSlot { .. }
            | Self::InvalidSlotValue { .. }
            | Self::EmptyAgentId => StatusCode::BAD_REQUEST,
            Self::CapabilityNotEnabled { .. }
            | Self::TabHidden { .. }
            | Self::InvalidChatTransition { .. } => StatusCode::CONFLICT,
            Self::Database(_) | Self::Hydration(_) | Self::Serialization(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::ProjectNotFound { .. } => "UiStateError::ProjectNotFound",
            Self::UnknownNodePath { .. } => "UiStateError::UnknownNodePath",
            Self::CapabilityNotEnabled { .. } => "UiStateError::CapabilityNotEnabled",
            Self::TabHidden { .. } => "UiStateError::TabHidden",
            Self::UnknownSlot { .. } => "UiStateError::UnknownSlot",
            Self::InvalidSlotValue { .. } => "UiStateError::InvalidSlotValue",
            Self::EmptyAgentId => "UiStateError::EmptyAgentId",
            Self::InvalidChatTransition { .. } => "UiStateError::InvalidChatTransition",
            Self::Database(_) => "UiStateError::Database",
            Self::Hydration(_) => "UiStateError::Hydration",
            Self::Serialization(_) => "UiStateError::Serialization",
        }
    }
}

/// Classification of a node within the declarative UI surface tree.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SurfaceNodeKind {
    Tab,
    Subtab,
    View,
}

/// Allowed value domain for a subsystem state slot.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, TS)]
#[serde(tag = "kind", content = "options", rename_all = "snake_case")]
#[ts(export)]
pub enum UiSlotValueKind {
    String,
    Enum(Vec<String>),
}

/// Declarative definition of a subsystem-owned state slot.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UiSlotDef {
    pub key: String,
    pub label: String,
    pub value_kind: UiSlotValueKind,
    pub default_value: Option<String>,
}

/// Execution target for a surface-scoped action (Spec 36).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SurfaceActionTarget {
    Backend,
    Frontend,
}

/// Declarative action exposed by a workspace surface node for dynamic discovery.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UiSurfaceActionDef {
    pub id: String,
    pub description: String,
    pub execution_target: SurfaceActionTarget,
    pub parameters_schema: serde_json::Value,
}

/// Declarative node in the workspace navigation tree.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UiSurfaceNode {
    pub id: String,
    pub label: String,
    pub kind: SurfaceNodeKind,
    pub required_capability: Option<String>,
    pub default_child_id: Option<String>,
    pub children: Vec<UiSurfaceNode>,
    pub state_slots: Vec<UiSlotDef>,
    #[serde(default)]
    pub actions: Vec<UiSurfaceActionDef>,
}

/// Complete declarative registry of workspace tabs, subtabs, and subsystem state slots.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UiSurfaceRegistry {
    pub nodes: Vec<UiSurfaceNode>,
}

/// Layout mode for the agent dialogue panel.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ChatLayoutMode {
    Docked,
    Floating,
}

impl ChatLayoutMode {
    fn as_db_str(self) -> &'static str {
        match self {
            Self::Docked => "docked",
            Self::Floating => "floating",
        }
    }

    fn from_db_str(raw: &str) -> Option<Self> {
        match raw {
            "docked" => Some(Self::Docked),
            "floating" => Some(Self::Floating),
            _ => None,
        }
    }
}

/// Finite state machine states for a project's agent chat panel.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ChatPanelState {
    Closed,
    Open,
    Minimized,
}

impl ChatPanelState {
    fn as_db_str(self) -> &'static str {
        match self {
            Self::Closed => "closed",
            Self::Open => "open",
            Self::Minimized => "minimized",
        }
    }

    fn from_db_str(raw: &str) -> Option<Self> {
        match raw {
            "closed" => Some(Self::Closed),
            "open" => Some(Self::Open),
            "minimized" => Some(Self::Minimized),
            _ => None,
        }
    }
}

/// Global UI layout state shared across the application.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UiGlobalState {
    #[ts(type = "number | null")]
    pub active_project_id: Option<i64>,
    pub chat_layout_mode: ChatLayoutMode,
    pub docked_chat_width_vw: f64,
    #[ts(type = "number")]
    pub updated_at_seconds: i64,
}

/// Per-project UI state (navigation path, subsystem slots, and agent chat panel state).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UiProjectState {
    #[ts(type = "number")]
    pub project_id: i64,
    pub active_node_path: String,
    pub slot_values: HashMap<String, Option<String>>,
    pub chat_panel_state: ChatPanelState,
    pub active_agent_id: Option<String>,
    pub active_conversation_id: Option<String>,
    #[ts(type = "number")]
    pub updated_at_seconds: i64,
}

/// Full UI state snapshot returned by `get_ui_state`, `dispatch_ui_action`, and `ui-state-updated`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UiStateSnapshot {
    pub global: UiGlobalState,
    pub projects: HashMap<String, UiProjectState>,
    #[ts(type = "number")]
    pub revision: u64,
}

/// Explicit UI transition actions accepted by `dispatch_ui_action`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub enum UiAction {
    #[serde(rename_all = "camelCase")]
    SelectProject {
        #[ts(type = "number")]
        project_id: i64,
    },
    #[serde(rename_all = "camelCase")]
    NavigateToNode {
        #[ts(type = "number")]
        project_id: i64,
        node_path: String,
    },
    #[serde(rename_all = "camelCase")]
    SetSlotValue {
        #[ts(type = "number")]
        project_id: i64,
        key: String,
        value: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    OpenAgentChat {
        #[ts(type = "number")]
        project_id: i64,
        agent_id: String,
        conversation_id: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    MinimizeAgentChat {
        #[ts(type = "number")]
        project_id: i64,
    },
    #[serde(rename_all = "camelCase")]
    RestoreAgentChat {
        #[ts(type = "number")]
        project_id: i64,
    },
    #[serde(rename_all = "camelCase")]
    CloseAgentChat {
        #[ts(type = "number")]
        project_id: i64,
    },
    #[serde(rename_all = "camelCase")]
    SetChatLayout {
        mode: ChatLayoutMode,
        docked_chat_width_vw: Option<f64>,
    },
}

fn leaf_node(id: &str, label: &str, kind: SurfaceNodeKind) -> UiSurfaceNode {
    UiSurfaceNode {
        id: id.to_string(),
        label: label.to_string(),
        kind,
        required_capability: None,
        default_child_id: None,
        children: Vec::new(),
        state_slots: Vec::new(),
        actions: Vec::new(),
    }
}

/// Builds the canonical declarative surface tree registry for all workspace subsystems.
pub fn build_surface_registry() -> UiSurfaceRegistry {
    UiSurfaceRegistry {
        nodes: vec![
            UiSurfaceNode {
                id: "settings".to_string(),
                label: "Settings".to_string(),
                kind: SurfaceNodeKind::Tab,
                required_capability: None,
                default_child_id: None,
                children: Vec::new(),
                state_slots: Vec::new(),
                actions: vec![
                    UiSurfaceActionDef {
                        id: "get_project_config_schema".to_string(),
                        description: "Returns the project configuration tree with every configurable plugin field, options, description, current value, and provenance.".to_string(),
                        execution_target: SurfaceActionTarget::Backend,
                        parameters_schema: serde_json::json!({ "type": "OBJECT" }),
                    },
                    UiSurfaceActionDef {
                        id: "set_project_config_value".to_string(),
                        description: "Updates a project configuration field in rula.toml (e.g. plugin_id='vcs', field_key='max_per_project_count', value_json='20').".to_string(),
                        execution_target: SurfaceActionTarget::Backend,
                        parameters_schema: serde_json::json!({
                            "type": "OBJECT",
                            "properties": {
                                "plugin_id": { "type": "STRING" },
                                "field_key": { "type": "STRING" },
                                "value_json": { "type": "STRING" }
                            },
                            "required": ["plugin_id", "field_key", "value_json"]
                        }),
                    },
                ],
            },
            UiSurfaceNode {
                id: "overview".to_string(),
                label: "Overview".to_string(),
                kind: SurfaceNodeKind::Tab,
                required_capability: None,
                default_child_id: None,
                children: Vec::new(),
                state_slots: Vec::new(),
                actions: Vec::new(),
            },
            UiSurfaceNode {
                id: "project".to_string(),
                label: "Project".to_string(),
                kind: SurfaceNodeKind::Tab,
                required_capability: Some("has_project_management_enabled".to_string()),
                default_child_id: Some("board".to_string()),
                children: vec![
                    leaf_node("milestones", "Milestones", SurfaceNodeKind::Subtab),
                    leaf_node("board", "Board", SurfaceNodeKind::Subtab),
                    leaf_node("schedule", "Schedule", SurfaceNodeKind::Subtab),
                    leaf_node("gantt", "Gantt", SurfaceNodeKind::Subtab),
                    UiSurfaceNode {
                        id: "forge".to_string(),
                        label: "Forge".to_string(),
                        kind: SurfaceNodeKind::Subtab,
                        required_capability: Some("has_git".to_string()),
                        default_child_id: Some("issues".to_string()),
                        children: vec![
                            leaf_node("issues", "Issues", SurfaceNodeKind::View),
                            leaf_node("prs", "Pull Requests", SurfaceNodeKind::View),
                            leaf_node("releases", "Releases", SurfaceNodeKind::View),
                        ],
                        state_slots: Vec::new(),
                        actions: vec![
                            UiSurfaceActionDef {
                                id: "list_issues".to_string(),
                                description: "Lists Forge issues for the active provider repository (state: 'open' | 'closed' | 'all').".to_string(),
                                execution_target: SurfaceActionTarget::Backend,
                                parameters_schema: serde_json::json!({
                                    "type": "OBJECT",
                                    "properties": {
                                        "state": { "type": "STRING" },
                                        "limit": { "type": "INTEGER" }
                                    }
                                }),
                            },
                            UiSurfaceActionDef {
                                id: "get_issue".to_string(),
                                description: "Gets full details and comments for a Forge issue number.".to_string(),
                                execution_target: SurfaceActionTarget::Backend,
                                parameters_schema: serde_json::json!({
                                    "type": "OBJECT",
                                    "properties": { "number": { "type": "INTEGER" } },
                                    "required": ["number"]
                                }),
                            },
                            UiSurfaceActionDef {
                                id: "list_prs".to_string(),
                                description: "Lists Forge pull/merge requests for the active provider repository (state: 'open' | 'closed' | 'merged' | 'all').".to_string(),
                                execution_target: SurfaceActionTarget::Backend,
                                parameters_schema: serde_json::json!({
                                    "type": "OBJECT",
                                    "properties": {
                                        "state": { "type": "STRING" },
                                        "limit": { "type": "INTEGER" }
                                    }
                                }),
                            },
                            UiSurfaceActionDef {
                                id: "get_pr".to_string(),
                                description: "Gets full details for a Forge pull/merge request number.".to_string(),
                                execution_target: SurfaceActionTarget::Backend,
                                parameters_schema: serde_json::json!({
                                    "type": "OBJECT",
                                    "properties": { "number": { "type": "INTEGER" } },
                                    "required": ["number"]
                                }),
                            },
                            UiSurfaceActionDef {
                                id: "list_releases".to_string(),
                                description: "Lists Forge releases for the active provider repository.".to_string(),
                                execution_target: SurfaceActionTarget::Backend,
                                parameters_schema: serde_json::json!({
                                    "type": "OBJECT",
                                    "properties": { "limit": { "type": "INTEGER" } }
                                }),
                            },
                            UiSurfaceActionDef {
                                id: "mark_item_read".to_string(),
                                description: "Marks a Forge issue or pull/merge request as read or unread.".to_string(),
                                execution_target: SurfaceActionTarget::Backend,
                                parameters_schema: serde_json::json!({
                                    "type": "OBJECT",
                                    "properties": {
                                        "item_kind": { "type": "STRING" },
                                        "number": { "type": "INTEGER" },
                                        "read": { "type": "BOOLEAN" }
                                    },
                                    "required": ["item_kind", "number", "read"]
                                }),
                            },
                            UiSurfaceActionDef {
                                id: "set_item_flag".to_string(),
                                description: "Sets or clears a triage flag ('flagged' | 'muted' | null) on a Forge issue or pull/merge request.".to_string(),
                                execution_target: SurfaceActionTarget::Backend,
                                parameters_schema: serde_json::json!({
                                    "type": "OBJECT",
                                    "properties": {
                                        "item_kind": { "type": "STRING" },
                                        "number": { "type": "INTEGER" },
                                        "flag": { "type": "STRING" }
                                    },
                                    "required": ["item_kind", "number"]
                                }),
                            },
                        ],
                    },
                ],
                state_slots: vec![UiSlotDef {
                    key: "project.last_tasks_node".to_string(),
                    label: "Last Tasks View".to_string(),
                    value_kind: UiSlotValueKind::Enum(vec![
                        "milestones".to_string(),
                        "board".to_string(),
                        "schedule".to_string(),
                        "gantt".to_string(),
                    ]),
                    default_value: Some("board".to_string()),
                }],
                actions: vec![
                    UiSurfaceActionDef {
                        id: "get_project_summary".to_string(),
                        description: "Returns active/overdue/pending task counts and per-stage breakdown for the project.".to_string(),
                        execution_target: SurfaceActionTarget::Backend,
                        parameters_schema: serde_json::json!({ "type": "OBJECT" }),
                    },
                    UiSurfaceActionDef {
                        id: "list_stages".to_string(),
                        description: "Lists all Kanban stages (columns) for the project.".to_string(),
                        execution_target: SurfaceActionTarget::Backend,
                        parameters_schema: serde_json::json!({ "type": "OBJECT" }),
                    },
                    UiSurfaceActionDef {
                        id: "create_stage".to_string(),
                        description: "Creates a new Kanban stage.".to_string(),
                        execution_target: SurfaceActionTarget::Backend,
                        parameters_schema: serde_json::json!({
                            "type": "OBJECT",
                            "properties": {
                                "label": { "type": "STRING" },
                                "is_terminal": { "type": "BOOLEAN" }
                            },
                            "required": ["label"]
                        }),
                    },
                    UiSurfaceActionDef {
                        id: "update_stage".to_string(),
                        description: "Updates a Kanban stage label, collapsed state, or terminal flag.".to_string(),
                        execution_target: SurfaceActionTarget::Backend,
                        parameters_schema: serde_json::json!({
                            "type": "OBJECT",
                            "properties": {
                                "stage_id": { "type": "INTEGER" },
                                "label": { "type": "STRING" },
                                "is_collapsed": { "type": "BOOLEAN" },
                                "is_terminal": { "type": "BOOLEAN" }
                            },
                            "required": ["stage_id"]
                        }),
                    },
                    UiSurfaceActionDef {
                        id: "delete_stage".to_string(),
                        description: "Deletes a Kanban stage. Fails if tasks remain unless reassign_to_stage_id is provided.".to_string(),
                        execution_target: SurfaceActionTarget::Backend,
                        parameters_schema: serde_json::json!({
                            "type": "OBJECT",
                            "properties": {
                                "stage_id": { "type": "INTEGER" },
                                "reassign_to_stage_id": { "type": "INTEGER" }
                            },
                            "required": ["stage_id"]
                        }),
                    },
                    UiSurfaceActionDef {
                        id: "list_tasks".to_string(),
                        description: "Lists tasks and subtasks for the project, optionally filtered by status, domain, or timeline due date range.".to_string(),
                        execution_target: SurfaceActionTarget::Backend,
                        parameters_schema: serde_json::json!({
                            "type": "OBJECT",
                            "properties": {
                                "status_filter": { "type": "STRING" },
                                "domain_filter": { "type": "STRING" },
                                "due_after_epoch_seconds": { "type": "INTEGER" },
                                "due_before_epoch_seconds": { "type": "INTEGER" }
                            }
                        }),
                    },
                    UiSurfaceActionDef {
                        id: "create_task".to_string(),
                        description: "Creates a project task with optional description, domain, status (stage slug), priority (1-4), start/due epoch seconds, and branch_name.".to_string(),
                        execution_target: SurfaceActionTarget::Backend,
                        parameters_schema: serde_json::json!({
                            "type": "OBJECT",
                            "properties": {
                                "title": { "type": "STRING" },
                                "description": { "type": "STRING" },
                                "domain": { "type": "STRING" },
                                "status": { "type": "STRING" },
                                "priority": { "type": "INTEGER" },
                                "start_epoch_seconds": { "type": "INTEGER" },
                                "due_epoch_seconds": { "type": "INTEGER" },
                                "branch_name": { "type": "STRING" }
                            },
                            "required": ["title"]
                        }),
                    },
                    UiSurfaceActionDef {
                        id: "update_task".to_string(),
                        description: "Partially updates a task (title, description, domain, status/stage, priority, start/due dates, branch_name). Unspecified fields keep their existing values.".to_string(),
                        execution_target: SurfaceActionTarget::Backend,
                        parameters_schema: serde_json::json!({
                            "type": "OBJECT",
                            "properties": {
                                "task_id": { "type": "INTEGER" },
                                "title": { "type": "STRING" },
                                "description": { "type": "STRING" },
                                "domain": { "type": "STRING" },
                                "status": { "type": "STRING" },
                                "priority": { "type": "INTEGER" },
                                "start_epoch_seconds": { "type": "INTEGER" },
                                "due_epoch_seconds": { "type": "INTEGER" },
                                "branch_name": { "type": "STRING" }
                            },
                            "required": ["task_id"]
                        }),
                    },
                    UiSurfaceActionDef {
                        id: "delete_task".to_string(),
                        description: "Deletes a task by task_id.".to_string(),
                        execution_target: SurfaceActionTarget::Backend,
                        parameters_schema: serde_json::json!({
                            "type": "OBJECT",
                            "properties": { "task_id": { "type": "INTEGER" } },
                            "required": ["task_id"]
                        }),
                    },
                    UiSurfaceActionDef {
                        id: "create_subtask".to_string(),
                        description: "Creates a checklist subtask under task_id.".to_string(),
                        execution_target: SurfaceActionTarget::Backend,
                        parameters_schema: serde_json::json!({
                            "type": "OBJECT",
                            "properties": {
                                "task_id": { "type": "INTEGER" },
                                "title": { "type": "STRING" }
                            },
                            "required": ["task_id", "title"]
                        }),
                    },
                    UiSurfaceActionDef {
                        id: "toggle_subtask".to_string(),
                        description: "Marks a subtask completed or incomplete.".to_string(),
                        execution_target: SurfaceActionTarget::Backend,
                        parameters_schema: serde_json::json!({
                            "type": "OBJECT",
                            "properties": {
                                "subtask_id": { "type": "INTEGER" },
                                "is_completed": { "type": "BOOLEAN" }
                            },
                            "required": ["subtask_id", "is_completed"]
                        }),
                    },
                    UiSurfaceActionDef {
                        id: "delete_subtask".to_string(),
                        description: "Deletes a subtask by subtask_id.".to_string(),
                        execution_target: SurfaceActionTarget::Backend,
                        parameters_schema: serde_json::json!({
                            "type": "OBJECT",
                            "properties": { "subtask_id": { "type": "INTEGER" } },
                            "required": ["subtask_id"]
                        }),
                    },
                ],
            },
            UiSurfaceNode {
                id: "files".to_string(),
                label: "Explorer".to_string(),
                kind: SurfaceNodeKind::Tab,
                required_capability: None,
                default_child_id: None,
                children: Vec::new(),
                state_slots: vec![
                    UiSlotDef {
                        key: "files.explorer_scope".to_string(),
                        label: "Explorer Scope".to_string(),
                        value_kind: UiSlotValueKind::Enum(vec![
                            "docs".to_string(),
                            "codebase".to_string(),
                        ]),
                        default_value: Some("docs".to_string()),
                    },
                    UiSlotDef {
                        key: "files.explorer_path".to_string(),
                        label: "Explorer Directory Path".to_string(),
                        value_kind: UiSlotValueKind::String,
                        default_value: Some(String::new()),
                    },
                    UiSlotDef {
                        key: "files.selected_document_path".to_string(),
                        label: "Selected Document Path".to_string(),
                        value_kind: UiSlotValueKind::String,
                        default_value: None,
                    },
                    UiSlotDef {
                        key: "dialogue.active_artifact_id".to_string(),
                        label: "Active Artifact ID".to_string(),
                        value_kind: UiSlotValueKind::String,
                        default_value: None,
                    },
                ],
                actions: vec![
                    UiSurfaceActionDef {
                        id: "search_files".to_string(),
                        description: "Searches project files using voice-friendly normalized token matching on relative path.".to_string(),
                        execution_target: SurfaceActionTarget::Backend,
                        parameters_schema: serde_json::json!({
                            "type": "OBJECT",
                            "properties": { "query": { "type": "STRING" } },
                            "required": ["query"]
                        }),
                    },
                    UiSurfaceActionDef {
                        id: "semantic_search".to_string(),
                        description: "Runs semantic code/docs search (BGE-M3 + LanceDB) across the project to locate where a feature or action is implemented.".to_string(),
                        execution_target: SurfaceActionTarget::Backend,
                        parameters_schema: serde_json::json!({
                            "type": "OBJECT",
                            "properties": {
                                "query": { "type": "STRING" },
                                "limit": { "type": "INTEGER" }
                            },
                            "required": ["query"]
                        }),
                    },
                    UiSurfaceActionDef {
                        id: "list_directory".to_string(),
                        description: "Lists files and subdirectories at a project-relative directory path.".to_string(),
                        execution_target: SurfaceActionTarget::Backend,
                        parameters_schema: serde_json::json!({
                            "type": "OBJECT",
                            "properties": { "relative_path": { "type": "STRING" } }
                        }),
                    },
                    UiSurfaceActionDef {
                        id: "open_file".to_string(),
                        description: "Opens a project-relative file path in the Explorer document viewer.".to_string(),
                        execution_target: SurfaceActionTarget::Frontend,
                        parameters_schema: serde_json::json!({
                            "type": "OBJECT",
                            "properties": { "path": { "type": "STRING" } },
                            "required": ["path"]
                        }),
                    },
                    UiSurfaceActionDef {
                        id: "set_scope".to_string(),
                        description: "Switches Explorer scope between 'docs' and 'codebase'.".to_string(),
                        execution_target: SurfaceActionTarget::Backend,
                        parameters_schema: serde_json::json!({
                            "type": "OBJECT",
                            "properties": { "scope": { "type": "STRING", "enum": ["docs", "codebase"] } },
                            "required": ["scope"]
                        }),
                    },
                ],
            },
            UiSurfaceNode {
                id: "knowledge".to_string(),
                label: "Graph".to_string(),
                kind: SurfaceNodeKind::Tab,
                required_capability: Some("has_knowledge_graph_enabled".to_string()),
                default_child_id: None,
                children: Vec::new(),
                actions: Vec::new(),
                state_slots: Vec::new(),
            },
            UiSurfaceNode {
                id: "electronics".to_string(),
                label: "Electronics".to_string(),
                kind: SurfaceNodeKind::Tab,
                required_capability: Some("has_electronics_enabled".to_string()),
                default_child_id: Some("firmware".to_string()),
                children: vec![
                    leaf_node("firmware", "Firmware", SurfaceNodeKind::Subtab),
                    UiSurfaceNode {
                        id: "oscilloscope".to_string(),
                        label: "Oscilloscope".to_string(),
                        kind: SurfaceNodeKind::Subtab,
                        required_capability: None,
                        default_child_id: None,
                        children: Vec::new(),
                        state_slots: vec![
                            UiSlotDef {
                                key: "electronics.oscilloscope_ch1_enabled".to_string(),
                                label: "Oscilloscope Channel 1 Enabled".to_string(),
                                value_kind: UiSlotValueKind::Enum(vec![
                                    "true".to_string(),
                                    "false".to_string(),
                                ]),
                                default_value: Some("true".to_string()),
                            },
                            UiSlotDef {
                                key: "electronics.oscilloscope_ch2_enabled".to_string(),
                                label: "Oscilloscope Channel 2 Enabled".to_string(),
                                value_kind: UiSlotValueKind::Enum(vec![
                                    "true".to_string(),
                                    "false".to_string(),
                                ]),
                                default_value: Some("true".to_string()),
                            },
                            UiSlotDef {
                                key: "electronics.oscilloscope_timebase_us".to_string(),
                                label: "Oscilloscope Timebase (µs/div)".to_string(),
                                value_kind: UiSlotValueKind::String,
                                default_value: Some("200".to_string()),
                            },
                            UiSlotDef {
                                key: "electronics.oscilloscope_trigger_source".to_string(),
                                label: "Oscilloscope Trigger Source".to_string(),
                                value_kind: UiSlotValueKind::Enum(vec![
                                    "CH1".to_string(),
                                    "CH2".to_string(),
                                    "EXT".to_string(),
                                ]),
                                default_value: Some("CH1".to_string()),
                            },
                            UiSlotDef {
                                key: "electronics.oscilloscope_trigger_slope".to_string(),
                                label: "Oscilloscope Trigger Slope".to_string(),
                                value_kind: UiSlotValueKind::Enum(vec![
                                    "Rising".to_string(),
                                    "Falling".to_string(),
                                ]),
                                default_value: Some("Rising".to_string()),
                            },
                            UiSlotDef {
                                key: "electronics.oscilloscope_trigger_sweep".to_string(),
                                label: "Oscilloscope Trigger Sweep".to_string(),
                                value_kind: UiSlotValueKind::Enum(vec![
                                    "Auto".to_string(),
                                    "Normal".to_string(),
                                    "Single".to_string(),
                                ]),
                                default_value: Some("Auto".to_string()),
                            },
                        ],
                        actions: Vec::new(),
                    },
                    leaf_node("logic", "Logic Analyzer", SurfaceNodeKind::Subtab),
                ],
                state_slots: Vec::new(),
                actions: Vec::new(),
            },
            UiSurfaceNode {
                id: "design".to_string(),
                label: "Design".to_string(),
                kind: SurfaceNodeKind::Tab,
                required_capability: Some("has_desigma_enabled".to_string()),
                default_child_id: None,
                children: Vec::new(),
                state_slots: Vec::new(),
                actions: Vec::new(),
            },
            UiSurfaceNode {
                id: "cad".to_string(),
                label: "CAD".to_string(),
                kind: SurfaceNodeKind::Tab,
                required_capability: Some("has_solidcraft_enabled".to_string()),
                default_child_id: None,
                children: Vec::new(),
                state_slots: Vec::new(),
                actions: Vec::new(),
            },
            UiSurfaceNode {
                id: "pcb".to_string(),
                label: "PCB".to_string(),
                kind: SurfaceNodeKind::Tab,
                required_capability: Some("has_pcbcraft_enabled".to_string()),
                default_child_id: None,
                children: Vec::new(),
                state_slots: Vec::new(),
                actions: Vec::new(),
            },
            UiSurfaceNode {
                id: "preview".to_string(),
                label: "Preview".to_string(),
                kind: SurfaceNodeKind::Tab,
                required_capability: Some("has_preview_url".to_string()),
                default_child_id: None,
                children: Vec::new(),
                state_slots: Vec::new(),
                actions: Vec::new(),
            },
            UiSurfaceNode {
                id: "profiler".to_string(),
                label: "Profiling".to_string(),
                kind: SurfaceNodeKind::Tab,
                required_capability: Some("has_profiler_enabled".to_string()),
                default_child_id: None,
                children: Vec::new(),
                state_slots: Vec::new(),
                actions: Vec::new(),
            },
            UiSurfaceNode {
                id: "rea".to_string(),
                label: "RE Lab".to_string(),
                kind: SurfaceNodeKind::Tab,
                required_capability: Some("has_rea_enabled".to_string()),
                default_child_id: None,
                children: Vec::new(),
                state_slots: Vec::new(),
                actions: Vec::new(),
            },
            UiSurfaceNode {
                id: "terminal".to_string(),
                label: "Scripts".to_string(),
                kind: SurfaceNodeKind::Tab,
                required_capability: None,
                default_child_id: None,
                children: Vec::new(),
                state_slots: Vec::new(),
                actions: Vec::new(),
            },
            UiSurfaceNode {
                id: "git".to_string(),
                label: "Git & Snapshots".to_string(),
                kind: SurfaceNodeKind::Tab,
                required_capability: Some("has_git".to_string()),
                default_child_id: None,
                children: Vec::new(),
                state_slots: Vec::new(),
                actions: Vec::new(),
            },
            UiSurfaceNode {
                id: "devlog".to_string(),
                label: "Journal".to_string(),
                kind: SurfaceNodeKind::Tab,
                required_capability: None,
                default_child_id: Some("devlog".to_string()),
                children: vec![
                    leaf_node("devlog", "Devlog", SurfaceNodeKind::Subtab),
                    leaf_node("todos", "TODOs", SurfaceNodeKind::Subtab),
                    leaf_node("debt", "Tech Debt", SurfaceNodeKind::Subtab),
                ],
                state_slots: Vec::new(),
                actions: Vec::new(),
            },
            UiSurfaceNode {
                id: "skills".to_string(),
                label: "Skills".to_string(),
                kind: SurfaceNodeKind::Tab,
                required_capability: None,
                default_child_id: Some("installed".to_string()),
                children: vec![
                    leaf_node("installed", "Installed", SurfaceNodeKind::Subtab),
                    leaf_node("explore", "Explore", SurfaceNodeKind::Subtab),
                ],
                state_slots: Vec::new(),
                actions: Vec::new(),
            },
        ],
    }
}

/// Finds a [`UiSurfaceNode`] in `registry` by slash-delimited `node_path`.
pub fn find_surface_node<'a>(
    registry: &'a UiSurfaceRegistry,
    node_path: &str,
) -> Option<&'a UiSurfaceNode> {
    let trimmed = node_path.trim().trim_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let mut current_nodes: &[UiSurfaceNode] = &registry.nodes;
    let mut found: Option<&UiSurfaceNode> = None;
    for seg in trimmed.split('/') {
        let node = current_nodes.iter().find(|n| n.id == seg)?;
        found = Some(node);
        current_nodes = &node.children;
    }
    found
}

fn now_epoch_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn clamp_docked_width_vw(vw: f64) -> f64 {
    if vw.is_nan() || vw.is_infinite() {
        return DEFAULT_DOCKED_CHAT_WIDTH_VW;
    }
    (vw.clamp(MIN_DOCKED_CHAT_WIDTH_VW, MAX_DOCKED_CHAT_WIDTH_VW) * 10.0).round() / 10.0
}

fn collect_slot_defs<'a>(nodes: &'a [UiSurfaceNode], out: &mut HashMap<String, &'a UiSlotDef>) {
    for node in nodes {
        for slot in &node.state_slots {
            out.insert(slot.key.clone(), slot);
        }
        collect_slot_defs(&node.children, out);
    }
}

fn default_slot_values(registry: &UiSurfaceRegistry) -> HashMap<String, Option<String>> {
    let mut defs = HashMap::new();
    collect_slot_defs(&registry.nodes, &mut defs);
    defs.into_iter()
        .map(|(k, def)| (k, def.default_value.clone()))
        .collect()
}

pub(crate) fn project_has_capability(project: &HydratedProject, capability: &str) -> bool {
    match capability {
        "has_git" => project.has_git,
        "has_rea_enabled" => project.has_rea_enabled,
        "has_profiler_enabled" => project.has_profiler_enabled,
        "has_electronics_enabled" => project.has_electronics_enabled,
        "has_desigma_enabled" => project.has_desigma_enabled,
        "has_solidcraft_enabled" => project.has_solidcraft_enabled,
        "has_pcbcraft_enabled" => project.has_pcbcraft_enabled,
        "has_project_management_enabled" => project.has_project_management_enabled,
        "has_knowledge_graph_enabled" => project.has_knowledge_graph_enabled,
        "has_preview_url" => project.preview_url.as_ref().is_some_and(|u| !u.trim().is_empty()),
        _ => false,
    }
}

/// Validates `node_path` against `registry` and `project` capabilities, resolving any
/// `default_child_id` suffixes down to the deepest default leaf.
fn resolve_and_validate_node_path(
    registry: &UiSurfaceRegistry,
    project: &HydratedProject,
    node_path: &str,
) -> Result<String, UiStateError> {
    let trimmed = node_path.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Err(UiStateError::UnknownNodePath {
            node_path: node_path.to_string(),
        });
    }

    let segments: Vec<&str> = trimmed.split('/').collect();
    let project_id = project.id as i64;
    let mut resolved = Vec::with_capacity(segments.len() + 2);
    let mut current_nodes: &[UiSurfaceNode] = &registry.nodes;
    let mut last_node: Option<&UiSurfaceNode> = None;

    for (idx, seg) in segments.iter().enumerate() {
        let Some(node) = current_nodes.iter().find(|n| n.id == *seg) else {
            return Err(UiStateError::UnknownNodePath {
                node_path: node_path.to_string(),
            });
        };

        if idx == 0 && project.hidden_tabs.iter().any(|t| t == &node.id) {
            return Err(UiStateError::TabHidden {
                project_id,
                tab_id: node.id.clone(),
            });
        }

        if let Some(cap) = &node.required_capability {
            if !project_has_capability(project, cap) {
                return Err(UiStateError::CapabilityNotEnabled {
                    project_id,
                    node_path: node_path.to_string(),
                    capability: cap.clone(),
                });
            }
        }

        resolved.push(node.id.clone());
        current_nodes = &node.children;
        last_node = Some(node);
    }

    // Descend through default children (bounded to prevent cycles).
    let mut depth_guard = 0usize;
    while let Some(node) = last_node {
        depth_guard += 1;
        debug_assert!(depth_guard <= 16, "surface tree depth exceeded bound");
        if depth_guard > 16 {
            break;
        }
        let Some(default_id) = &node.default_child_id else {
            break;
        };
        let Some(child) = node.children.iter().find(|c| c.id == *default_id) else {
            break;
        };
        if let Some(cap) = &child.required_capability {
            if !project_has_capability(project, cap) {
                break;
            }
        }
        resolved.push(child.id.clone());
        last_node = Some(child);
    }

    Ok(resolved.join("/"))
}

struct UiStateCache {
    registry: UiSurfaceRegistry,
    global: UiGlobalState,
    projects: HashMap<i64, UiProjectState>,
    revision: u64,
    hydrated_from_db: bool,
}

/// Thread-safe, poison-resistant coordinator for canonical UI state (Spec 35).
pub struct UiStateManager {
    cache: RwLock<UiStateCache>,
}

static UI_STATE_MANAGER: OnceLock<UiStateManager> = OnceLock::new();

/// Returns the global [`UiStateManager`] singleton.
pub fn get_ui_state_manager() -> &'static UiStateManager {
    UI_STATE_MANAGER.get_or_init(UiStateManager::new)
}

/// Validates a slot assignment against the declarative registry definition.
fn validate_slot_value(
    registry: &UiSurfaceRegistry,
    key: &str,
    value: Option<&str>,
) -> Result<(), UiStateError> {
    let mut slot_defs = HashMap::new();
    collect_slot_defs(&registry.nodes, &mut slot_defs);
    let Some(def) = slot_defs.get(key) else {
        return Err(UiStateError::UnknownSlot {
            key: key.to_string(),
        });
    };
    if let (UiSlotValueKind::Enum(allowed), Some(val)) = (&def.value_kind, value) {
        if !allowed.iter().any(|a| a == val) {
            return Err(UiStateError::InvalidSlotValue {
                key: key.to_string(),
                value: val.to_string(),
            });
        }
    }
    Ok(())
}

/// Validates and normalizes a `DbUiProjectState` row's `active_node_path` during DB hydration.
fn hydrate_sanitized_node_path(
    registry: &UiSurfaceRegistry,
    project: &HydratedProject,
    raw_path: &str,
) -> String {
    match resolve_and_validate_node_path(registry, project, raw_path) {
        Ok(valid) => valid,
        Err(err) => {
            crate::logger::log(
                "WARN",
                "ui_state",
                &format!(
                    "Sanitizing stale active_node_path '{}' for project {}: {}",
                    raw_path, project.id, err
                ),
            );
            DEFAULT_NODE_PATH.to_string()
        }
    }
}

impl UiStateManager {
    fn new() -> Self {
        let registry = build_surface_registry();
        Self {
            cache: RwLock::new(UiStateCache {
                registry,
                global: UiGlobalState {
                    active_project_id: None,
                    chat_layout_mode: ChatLayoutMode::Docked,
                    docked_chat_width_vw: DEFAULT_DOCKED_CHAT_WIDTH_VW,
                    updated_at_seconds: now_epoch_seconds(),
                },
                projects: HashMap::new(),
                revision: 1,
                hydrated_from_db: false,
            }),
        }
    }

    fn read_cache(&self) -> std::sync::RwLockReadGuard<'_, UiStateCache> {
        match self.cache.read() {
            Ok(guard) => guard,
            Err(poisoned) => {
                crate::logger::log("WARN", "ui_state", "Recovering from poisoned UiStateCache read lock");
                poisoned.into_inner()
            }
        }
    }

    fn write_cache(&self) -> std::sync::RwLockWriteGuard<'_, UiStateCache> {
        match self.cache.write() {
            Ok(guard) => guard,
            Err(poisoned) => {
                crate::logger::log("WARN", "ui_state", "Recovering from poisoned UiStateCache write lock");
                poisoned.into_inner()
            }
        }
    }

    /// Returns the declarative surface registry.
    pub fn get_registry(&self) -> UiSurfaceRegistry {
        self.read_cache().registry.clone()
    }

    /// Ensures cache is hydrated from SQLite and sanitized against currently registered projects.
    pub fn get_state(&self) -> Result<UiStateSnapshot, UiStateError> {
        let hydrated_projects = get_projects().map_err(|e| UiStateError::Hydration(e.to_string()))?;
        let project_map: HashMap<i64, &HydratedProject> =
            hydrated_projects.iter().map(|p| (p.id as i64, p)).collect();

        let mut cache = self.write_cache();
        if !cache.hydrated_from_db {
            self.hydrate_from_db_locked(&mut cache, &hydrated_projects, &project_map)?;
        }

        // Ensure active_project_id still points to an existing project (or first project if unset).
        if let Some(active_id) = cache.global.active_project_id {
            if !project_map.contains_key(&active_id) {
                crate::logger::log(
                    "WARN",
                    "ui_state",
                    &format!("Sanitizing stale active_project_id {} (project no longer registered)", active_id),
                );
                cache.global.active_project_id = hydrated_projects.first().map(|p| p.id as i64);
            }
        } else if let Some(first) = hydrated_projects.first() {
            cache.global.active_project_id = Some(first.id as i64);
        }

        Ok(snapshot_from_cache(&cache))
    }

    fn hydrate_from_db_locked(
        &self,
        cache: &mut UiStateCache,
        hydrated_projects: &[HydratedProject],
        project_map: &HashMap<i64, &HydratedProject>,
    ) -> Result<(), UiStateError> {
        if let Some(db_global) = db_get_ui_global_state()? {
            let mode = ChatLayoutMode::from_db_str(&db_global.chat_layout_mode).unwrap_or_else(|| {
                crate::logger::log(
                    "WARN",
                    "ui_state",
                    &format!("Sanitizing invalid chat_layout_mode '{}' to docked", db_global.chat_layout_mode),
                );
                ChatLayoutMode::Docked
            });
            cache.global = UiGlobalState {
                active_project_id: db_global.active_project_id,
                chat_layout_mode: mode,
                docked_chat_width_vw: clamp_docked_width_vw(db_global.docked_chat_width_vw),
                updated_at_seconds: db_global.updated_at_seconds,
            };
            cache.revision = db_global.revision.max(1) as u64;
        } else {
            cache.global.active_project_id = hydrated_projects.first().map(|p| p.id as i64);
        }

        let db_projects = db_get_all_ui_project_states()?;
        for row in db_projects {
            let Some(hydrated) = project_map.get(&row.project_id) else {
                continue;
            };

            let sanitized_path = hydrate_sanitized_node_path(
                &cache.registry,
                hydrated,
                &row.active_node_path,
            );

            let mut slot_values = default_slot_values(&cache.registry);
            if let Ok(parsed) = serde_json::from_str::<HashMap<String, Option<String>>>(&row.slot_values_json) {
                for (k, v) in parsed {
                    if slot_values.contains_key(&k) {
                        slot_values.insert(k, v);
                    }
                }
            }

            let mut panel_state = ChatPanelState::from_db_str(&row.chat_panel_state).unwrap_or(ChatPanelState::Closed);
            let mut active_agent_id = row.active_agent_id.filter(|s| !s.trim().is_empty());
            let mut active_conversation_id = row.active_conversation_id.filter(|s| !s.trim().is_empty());
            if panel_state != ChatPanelState::Closed && active_agent_id.is_none() {
                crate::logger::log(
                    "WARN",
                    "ui_state",
                    &format!("Sanitizing chat_panel_state {:?} without active_agent_id for project {}", panel_state, row.project_id),
                );
                panel_state = ChatPanelState::Closed;
                active_conversation_id = None;
            }
            if panel_state == ChatPanelState::Closed {
                active_agent_id = None;
                active_conversation_id = None;
            }

            cache.projects.insert(
                row.project_id,
                UiProjectState {
                    project_id: row.project_id,
                    active_node_path: sanitized_path,
                    slot_values,
                    chat_panel_state: panel_state,
                    active_agent_id,
                    active_conversation_id,
                    updated_at_seconds: row.updated_at_seconds,
                },
            );
        }

        cache.hydrated_from_db = true;
        Ok(())
    }

    /// Validates, applies, persists, and broadcasts a [`UiAction`].
    pub fn dispatch_action(&self, action: UiAction) -> Result<UiStateSnapshot, UiStateError> {
        let hydrated_projects = get_projects().map_err(|e| UiStateError::Hydration(e.to_string()))?;
        let project_map: HashMap<i64, &HydratedProject> =
            hydrated_projects.iter().map(|p| (p.id as i64, p)).collect();

        let mut cache = self.write_cache();
        if !cache.hydrated_from_db {
            self.hydrate_from_db_locked(&mut cache, &hydrated_projects, &project_map)?;
        }

        let now = now_epoch_seconds();
        let registry_clone = cache.registry.clone();
        let mut modified_project_id: Option<i64> = None;

        match action {
            UiAction::SelectProject { project_id } => {
                if !project_map.contains_key(&project_id) {
                    return Err(UiStateError::ProjectNotFound { project_id });
                }
                cache.global.active_project_id = Some(project_id);
                cache.global.updated_at_seconds = now;
                if !cache.projects.contains_key(&project_id) {
                    cache.projects.insert(
                        project_id,
                        new_default_project_state(project_id, &registry_clone, now),
                    );
                    modified_project_id = Some(project_id);
                }
            }
            UiAction::NavigateToNode { project_id, node_path } => {
                let hydrated = project_map
                    .get(&project_id)
                    .ok_or(UiStateError::ProjectNotFound { project_id })?;
                let resolved_path =
                    resolve_and_validate_node_path(&registry_clone, hydrated, &node_path)?;
                let entry = cache
                    .projects
                    .entry(project_id)
                    .or_insert_with(|| new_default_project_state(project_id, &registry_clone, now));
                entry.active_node_path = resolved_path;
                entry.updated_at_seconds = now;
                modified_project_id = Some(project_id);
            }
            UiAction::SetSlotValue { project_id, key, value } => {
                if !project_map.contains_key(&project_id) {
                    return Err(UiStateError::ProjectNotFound { project_id });
                }
                validate_slot_value(&registry_clone, &key, value.as_deref())?;
                let entry = cache
                    .projects
                    .entry(project_id)
                    .or_insert_with(|| new_default_project_state(project_id, &registry_clone, now));
                entry.slot_values.insert(key, value);
                entry.updated_at_seconds = now;
                modified_project_id = Some(project_id);
            }
            UiAction::OpenAgentChat {
                project_id,
                agent_id,
                conversation_id,
            } => {
                if !project_map.contains_key(&project_id) {
                    return Err(UiStateError::ProjectNotFound { project_id });
                }
                let trimmed_agent = agent_id.trim().to_string();
                if trimmed_agent.is_empty() {
                    return Err(UiStateError::EmptyAgentId);
                }
                let entry = cache
                    .projects
                    .entry(project_id)
                    .or_insert_with(|| new_default_project_state(project_id, &registry_clone, now));
                entry.chat_panel_state = ChatPanelState::Open;
                entry.active_agent_id = Some(trimmed_agent);
                entry.active_conversation_id = conversation_id.filter(|s| !s.trim().is_empty());
                entry.updated_at_seconds = now;
                modified_project_id = Some(project_id);
            }
            UiAction::MinimizeAgentChat { project_id } => {
                if !project_map.contains_key(&project_id) {
                    return Err(UiStateError::ProjectNotFound { project_id });
                }
                let entry = cache
                    .projects
                    .entry(project_id)
                    .or_insert_with(|| new_default_project_state(project_id, &registry_clone, now));
                if entry.chat_panel_state != ChatPanelState::Open {
                    return Err(UiStateError::InvalidChatTransition {
                        action: "MinimizeAgentChat",
                        current_state: entry.chat_panel_state,
                    });
                }
                entry.chat_panel_state = ChatPanelState::Minimized;
                entry.updated_at_seconds = now;
                modified_project_id = Some(project_id);
            }
            UiAction::RestoreAgentChat { project_id } => {
                if !project_map.contains_key(&project_id) {
                    return Err(UiStateError::ProjectNotFound { project_id });
                }
                let entry = cache
                    .projects
                    .entry(project_id)
                    .or_insert_with(|| new_default_project_state(project_id, &registry_clone, now));
                if entry.chat_panel_state != ChatPanelState::Minimized {
                    return Err(UiStateError::InvalidChatTransition {
                        action: "RestoreAgentChat",
                        current_state: entry.chat_panel_state,
                    });
                }
                entry.chat_panel_state = ChatPanelState::Open;
                entry.updated_at_seconds = now;
                modified_project_id = Some(project_id);
            }
            UiAction::CloseAgentChat { project_id } => {
                if !project_map.contains_key(&project_id) {
                    return Err(UiStateError::ProjectNotFound { project_id });
                }
                let entry = cache
                    .projects
                    .entry(project_id)
                    .or_insert_with(|| new_default_project_state(project_id, &registry_clone, now));
                entry.chat_panel_state = ChatPanelState::Closed;
                entry.active_agent_id = None;
                entry.active_conversation_id = None;
                entry.updated_at_seconds = now;
                modified_project_id = Some(project_id);
            }
            UiAction::SetChatLayout {
                mode,
                docked_chat_width_vw,
            } => {
                cache.global.chat_layout_mode = mode;
                if let Some(vw) = docked_chat_width_vw {
                    cache.global.docked_chat_width_vw = clamp_docked_width_vw(vw);
                }
                cache.global.updated_at_seconds = now;
            }
        }

        cache.revision += 1;

        let db_global = DbUiGlobalState {
            active_project_id: cache.global.active_project_id,
            chat_layout_mode: cache.global.chat_layout_mode.as_db_str().to_string(),
            docked_chat_width_vw: cache.global.docked_chat_width_vw,
            revision: cache.revision as i64,
            updated_at_seconds: cache.global.updated_at_seconds,
        };

        let db_project = modified_project_id
            .and_then(|pid| cache.projects.get(&pid))
            .map(|proj| -> Result<DbUiProjectState, UiStateError> {
                Ok(DbUiProjectState {
                    project_id: proj.project_id,
                    active_node_path: proj.active_node_path.clone(),
                    slot_values_json: serde_json::to_string(&proj.slot_values)?,
                    chat_panel_state: proj.chat_panel_state.as_db_str().to_string(),
                    active_agent_id: proj.active_agent_id.clone(),
                    active_conversation_id: proj.active_conversation_id.clone(),
                    updated_at_seconds: proj.updated_at_seconds,
                })
            })
            .transpose()?;

        db_save_ui_state_atomic(&db_global, db_project.as_ref())?;

        let snapshot = snapshot_from_cache(&cache);
        drop(cache);

        crate::supervisor::get_supervisor().emit_event(DaemonEvent::ui_state_updated(&snapshot));
        Ok(snapshot)
    }
}

fn new_default_project_state(
    project_id: i64,
    registry: &UiSurfaceRegistry,
    now_seconds: i64,
) -> UiProjectState {
    UiProjectState {
        project_id,
        active_node_path: DEFAULT_NODE_PATH.to_string(),
        slot_values: default_slot_values(registry),
        chat_panel_state: ChatPanelState::Closed,
        active_agent_id: None,
        active_conversation_id: None,
        updated_at_seconds: now_seconds,
    }
}

fn snapshot_from_cache(cache: &UiStateCache) -> UiStateSnapshot {
    let projects = cache
        .projects
        .iter()
        .map(|(id, st)| (id.to_string(), st.clone()))
        .collect();
    UiStateSnapshot {
        global: cache.global.clone(),
        projects,
        revision: cache.revision,
    }
}

/// Daemon command handler for `get_ui_registry`.
pub fn get_ui_registry() -> UiSurfaceRegistry {
    get_ui_state_manager().get_registry()
}

/// Daemon command handler for `get_ui_state`.
pub fn get_ui_state() -> Result<UiStateSnapshot, UiStateError> {
    get_ui_state_manager().get_state()
}

/// Daemon command handler for `dispatch_ui_action`.
pub fn dispatch_ui_action(action: UiAction) -> Result<UiStateSnapshot, UiStateError> {
    get_ui_state_manager().dispatch_action(action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_hydrated_project() -> HydratedProject {
        HydratedProject {
            id: 1,
            path: "/tmp/test-proj".to_string(),
            name: "test-proj".to_string(),
            description: None,
            has_git: true,
            git_status: None,
            git_error: None,
            has_config: true,
            scripts: Vec::new(),
            skill_status: crate::kopeng::skill::SkillStatus {
                is_installed: true,
                installed_version: None,
                current_version: "0.2.0".to_string(),
                is_outdated: false,
                is_symlinked: false,
                schema_version: None,
                needs_migration: false,
            },
            init_state: None,
            has_rea_enabled: false,
            has_profiler_enabled: false,
            has_electronics_enabled: false,
            has_desigma_enabled: false,
            has_solidcraft_enabled: false,
            has_pcbcraft_enabled: false,
            has_project_management_enabled: true,
            has_knowledge_graph_enabled: true,
            preview_url: None,
            is_favorite: false,
            is_archived: false,
            hidden_tabs: vec!["skills".to_string()],
        }
    }

    #[test]
    fn resolves_default_children_recursively() {
        let reg = build_surface_registry();
        let proj = test_hydrated_project();

        let board = resolve_and_validate_node_path(&reg, &proj, "project").expect("project -> board");
        assert_eq!(board, "project/board");

        let milestones = resolve_and_validate_node_path(&reg, &proj, "project/milestones").expect("milestones");
        assert_eq!(milestones, "project/milestones");

        let schedule = resolve_and_validate_node_path(&reg, &proj, "project/schedule").expect("schedule");
        assert_eq!(schedule, "project/schedule");

        let gantt = resolve_and_validate_node_path(&reg, &proj, "project/gantt").expect("gantt");
        assert_eq!(gantt, "project/gantt");

        let issues = resolve_and_validate_node_path(&reg, &proj, "project/forge").expect("forge -> issues");
        assert_eq!(issues, "project/forge/issues");

        let prs = resolve_and_validate_node_path(&reg, &proj, "project/forge/prs").expect("exact leaf");
        assert_eq!(prs, "project/forge/prs");

        // Legacy paths must be rejected on direct dispatch/resolution (zero backward compatibility).
        assert!(resolve_and_validate_node_path(&reg, &proj, "project/timeline").is_err());
        assert!(resolve_and_validate_node_path(&reg, &proj, "project/github").is_err());
        assert_eq!(
            hydrate_sanitized_node_path(&reg, &proj, "project/timeline"),
            DEFAULT_NODE_PATH
        );
        assert_eq!(
            hydrate_sanitized_node_path(&reg, &proj, "project/github"),
            DEFAULT_NODE_PATH
        );
    }

    #[test]
    fn validates_project_last_tasks_node_slot() {
        let reg = build_surface_registry();
        let defaults = default_slot_values(&reg);
        assert_eq!(
            defaults.get("project.last_tasks_node").and_then(|v| v.as_deref()),
            Some("board")
        );

        assert!(validate_slot_value(&reg, "project.last_tasks_node", Some("milestones")).is_ok());
        assert!(validate_slot_value(&reg, "project.last_tasks_node", Some("board")).is_ok());
        assert!(validate_slot_value(&reg, "project.last_tasks_node", Some("schedule")).is_ok());
        assert!(validate_slot_value(&reg, "project.last_tasks_node", Some("gantt")).is_ok());

        let err = validate_slot_value(&reg, "project.last_tasks_node", Some("inbox")).unwrap_err();
        assert_eq!(err.code(), "UiStateError::InvalidSlotValue");
    }

    #[test]
    fn rejects_disabled_capability_and_hidden_tab() {
        let reg = build_surface_registry();
        let proj = test_hydrated_project();

        let err = resolve_and_validate_node_path(&reg, &proj, "electronics/oscilloscope").unwrap_err();
        assert_eq!(err.code(), "UiStateError::CapabilityNotEnabled");

        let hidden_err = resolve_and_validate_node_path(&reg, &proj, "skills").unwrap_err();
        assert_eq!(hidden_err.code(), "UiStateError::TabHidden");
    }
}
