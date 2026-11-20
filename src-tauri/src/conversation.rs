//! Conversation tracker for mcp_dialogue_mode compliance monitoring.
//!
//! Tracks whether agents that received a user message have subsequently
//! responded via `post_agent_response`. Drives the idle-without-MCP
//! compliance monitor in the orchestrator.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::db::DatabaseError;

/// Failures of Conversation management (scoping, runner configuration, reopening).
/// Tauri commands convert it to `String` at the IPC boundary.
#[derive(Debug, thiserror::Error)]
pub enum ConversationError {
    /// The path is not a registered project (names are never accepted).
    #[error("Project path '{0}' is not registered")]
    ProjectNotRegistered(String),

    /// The agent is not a persona of the project.
    #[error("Agent '{agent_id}' is not declared in project '{project_name}'")]
    AgentNotDeclared { agent_id: String, project_name: String },

    #[error(transparent)]
    Brief(#[from] crate::brief::BriefError),

    #[error(transparent)]
    Database(#[from] DatabaseError),

    /// The requested runner/model/effort is not a pair the runner manifest declares.
    #[error(transparent)]
    Config(#[from] crate::config::ConfigError),

    /// The Conversation id does not exist.
    #[error("Conversation '{0}' not found")]
    ConversationNotFound(String),

    /// The Conversation exists but belongs to another project or agent.
    #[error("Conversation '{conversation_id}' does not belong to agent '{agent_id}' of project '{project_path}'")]
    ConversationMismatch { conversation_id: String, agent_id: String, project_path: String },

    /// The live process could not take the reopened Conversation's Brief. Boxed because
    /// `SupervisorError` itself wraps `ConversationError`.
    #[error(transparent)]
    Supervisor(Box<crate::supervisor::SupervisorError>),
}

impl crate::api::ApiError for ConversationError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::ProjectNotRegistered(_) | Self::AgentNotDeclared { .. } | Self::ConversationNotFound(_) => StatusCode::NOT_FOUND,
            Self::Brief(crate::brief::BriefError::ConversationNotFound { .. }) => StatusCode::NOT_FOUND,
            Self::ConversationMismatch { .. } => StatusCode::BAD_REQUEST,
            Self::Brief(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Database(e) => e.status(),
            Self::Config(e) => e.status(),
            Self::Supervisor(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::ProjectNotRegistered(_) => "ConversationError::ProjectNotRegistered",
            Self::AgentNotDeclared { .. } => "ConversationError::AgentNotDeclared",
            Self::ConversationNotFound(_) => "ConversationError::ConversationNotFound",
            Self::ConversationMismatch { .. } => "ConversationError::ConversationMismatch",
            Self::Brief(_) => "ConversationError::Brief",
            Self::Database(e) => e.code(),
            Self::Config(e) => e.code(),
            Self::Supervisor(e) => e.code(),
        }
    }
}

/// Per-agent session state for dialogue compliance tracking.
#[derive(Debug, Clone)]
pub struct AgentConversationState {
    /// Epoch seconds of the last user message sent to this agent.
    pub last_user_message_at_seconds: u64,
    /// Epoch seconds of the last successful `post_agent_response` call.
    pub last_mcp_response_at_seconds: Option<u64>,
    /// Guard flag: true if a compliance reminder has already been injected
    /// for the current exchange. Resets when a new user message arrives.
    pub compliance_prompted: bool,
}

impl AgentConversationState {
    fn new_with_message(now: u64) -> Self {
        Self {
            last_user_message_at_seconds: now,
            last_mcp_response_at_seconds: None,
            compliance_prompted: false,
        }
    }
}

/// Window subtracted from the last user message time when looking for a persisted agent turn,
/// absorbing clock granularity between the in-memory mark and the `agent_chat_turns` timestamp.
const DB_RESPONSE_LOOKBACK_SECONDS: u64 = 5;

/// Global in-memory conversation state store.
/// Keys are supervisor agent keys (`<project_path>:<agent_id>`), never bare agent ids.
static STATE: std::sync::OnceLock<Mutex<HashMap<String, AgentConversationState>>> =
    std::sync::OnceLock::new();

/// Locks the store, recovering from poison: the map holds plain timestamps, so a panic in
/// another holder cannot leave it logically inconsistent (AGENTS.md §9.1).
fn lock_state() -> MutexGuard<'static, HashMap<String, AgentConversationState>> {
    STATE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| {
            crate::logger::log("WARN", "conversation", "conversation state mutex was poisoned; recovering");
            PoisonError::into_inner(poisoned)
        })
}

/// Project-scoped key of an agent. `project_path` is mandatory: a bare agent id would merge
/// the state of same-named agents across projects (AGENTS.md §1.11).
fn format_conv_key(project_path: &str, agent_id: &str) -> String {
    debug_assert!(!project_path.is_empty(), "conversation key requires a project path");
    debug_assert!(!agent_id.contains(':'), "agent id '{}' must not be a pre-formatted key", agent_id);
    crate::supervisor::ProcessSupervisor::format_agent_key(project_path, agent_id)
}

/// Called when a user message is dispatched to an agent pane.
/// Resets the compliance guard for that agent within the specified project context.
pub fn mark_user_message_sent(project_path: &str, agent_id: &str) {
    let key = format_conv_key(project_path, agent_id);
    let now = epoch_now_seconds();
    lock_state()
        .entry(key)
        .and_modify(|s| {
            s.last_user_message_at_seconds = now;
            s.compliance_prompted = false;
        })
        .or_insert_with(|| AgentConversationState::new_with_message(now));
}

/// Called by `execute_post_agent_response` when an agent successfully
/// routes a reply through the MCP dialogue channel.
pub fn mark_agent_responded(project_path: &str, agent_id: &str) {
    let key = format_conv_key(project_path, agent_id);
    let now = epoch_now_seconds();
    if let Some(state) = lock_state().get_mut(&key) {
        state.last_mcp_response_at_seconds = Some(now);
    }
}

fn epoch_now_seconds() -> u64 {
    crate::db::now_epoch_secs().max(0) as u64
}

/// True when `agent_chat_turns` holds an agent turn for this project and agent since `since_epoch_seconds`.
fn has_recent_db_response(project_path: &str, agent_id: &str, since_epoch_seconds: u64) -> Result<bool, ConversationError> {
    use crate::db::DbResultExt;
    let project_id = resolve_project_id(project_path)?;
    let conn = crate::db::get_connection()?;
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM agent_chat_turns WHERE project_id = ?1 AND LOWER(agent_id) = LOWER(?2) AND role = 'agent' AND timestamp >= ?3",
            rusqlite::params![project_id, agent_id, since_epoch_seconds as i64],
            |row| row.get(0),
        )
        .db_context("count recent agent turns")?;
    Ok(count > 0)
}

/// Returns true if the agent went idle without posting a `post_agent_response`
/// since the last user message, AND a compliance reminder has not yet been sent.
///
/// Also marks the compliance_prompted flag to prevent duplicate injections. The SQLite check
/// runs without holding the state mutex. When it fails, the reminder is injected (the in-memory
/// state says the agent did not answer) and the failure is logged.
pub fn should_inject_compliance_reminder(project_path: &str, agent_id: &str) -> bool {
    let key = format_conv_key(project_path, agent_id);
    let last_user_message_at_seconds = {
        let store = lock_state();
        let Some(state) = store.get(&key) else {
            return false;
        };
        let responded_in_memory = state.last_mcp_response_at_seconds.is_some_and(|at| at >= state.last_user_message_at_seconds);
        if state.compliance_prompted || responded_in_memory {
            return false;
        }
        state.last_user_message_at_seconds
    };

    let since_epoch_seconds = last_user_message_at_seconds.saturating_sub(DB_RESPONSE_LOOKBACK_SECONDS);
    let responded_in_db = has_recent_db_response(project_path, agent_id, since_epoch_seconds).unwrap_or_else(|e| {
        crate::logger::log("ERROR", "conversation", &format!("should_inject_compliance_reminder: {} (agent '{}')", e, agent_id));
        false
    });

    let mut store = lock_state();
    let Some(state) = store.get_mut(&key) else {
        return false;
    };
    // A newer user message or an MCP response may have arrived while the mutex was released.
    if state.compliance_prompted || state.last_user_message_at_seconds != last_user_message_at_seconds {
        return false;
    }
    if responded_in_db {
        state.last_mcp_response_at_seconds = Some(epoch_now_seconds());
        return false;
    }
    if state.last_mcp_response_at_seconds.is_some_and(|at| at >= state.last_user_message_at_seconds) {
        return false;
    }
    state.compliance_prompted = true;
    true
}

// ─────────────────────────────────────────────────────────────────────────────
// Daemon commands for topical Conversations & history (synchronous SQLite work: `api::dispatch`
// runs them on the blocking pool)
// ─────────────────────────────────────────────────────────────────────────────

/// Resolves a registered project by its exact path. Names are never accepted (AGENTS.md §1.11).
fn resolve_project_id(project_path: &str) -> Result<i64, ConversationError> {
    crate::db::db_get_projects()?
        .into_iter()
        .find(|p| p.path == project_path)
        .map(|p| p.id as i64)
        .ok_or_else(|| ConversationError::ProjectNotRegistered(project_path.to_string()))
}

/// Arguments of the daemon commands `get_active_conversation` and
/// `update_active_conversation_config`: the agent and the overrides to apply or create with.
#[derive(serde::Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct AgentConversationConfigArgs {
    pub project_path: String,
    pub agent_id: String,
    pub runner: Option<String>,
    pub model: Option<String>,
    pub account_id: Option<String>,
    pub effort: Option<String>,
}

/// The agent's active Conversation, created with the given overrides when there is none.
pub fn get_active_conversation(args: AgentConversationConfigArgs) -> Result<crate::db::AgentConversationRecord, ConversationError> {
    let project_id = resolve_project_id(&args.project_path)?;
    Ok(crate::db::db_get_or_create_active_conversation(
        project_id,
        &args.agent_id,
        None,
        args.runner.as_deref(),
        args.model.as_deref(),
        args.account_id.as_deref(),
        args.effort.as_deref(),
    )?)
}

/// Arguments of the daemon command `create_agent_conversation`.
#[derive(serde::Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CreateAgentConversationArgs {
    pub project_path: String,
    pub agent_id: String,
    pub topic: String,
    pub runner: Option<String>,
    pub model: Option<String>,
    pub account_id: Option<String>,
    pub effort: Option<String>,
}

/// Opens a new Conversation of the agent (closing the active one) with a system start turn.
pub fn create_agent_conversation(args: CreateAgentConversationArgs) -> Result<crate::db::AgentConversationRecord, ConversationError> {
    let CreateAgentConversationArgs { project_path, agent_id, topic, runner, model, account_id, effort } = args;
    let project_id = resolve_project_id(&project_path)?;
    let conv = crate::db::db_create_agent_conversation(
        project_id,
        &agent_id,
        &topic,
        runner.as_deref(),
        model.as_deref(),
        account_id.as_deref(),
        effort.as_deref(),
    )?;

    // Persist a system turn marking the start of a fresh conversation context
    let now = crate::db::now_epoch_secs();
    let sys_turn = crate::db::ChatTurnRecord {
        id: None,
        conversation_id: conv.id.clone(),
        project_id,
        agent_id: agent_id.clone(),
        agy_conversation_id: None,
        role: "system".to_string(),
        dialogue: "Fresh conversation context initialized".to_string(),
        artifact_type: None,
        artifact_lang: None,
        artifact_label: None,
        artifact_file_path: None,
        artifact_content: None,
        is_ephemeral: false,
        terminal_snapshot: None,
        timestamp: now,
        topic: Some(topic),
        approval_payload: None,
    };
    crate::db::db_insert_chat_turn(&sys_turn)?;

    Ok(conv)
}

/// Applies runner/model/account/effort overrides to the agent's active Conversation.
pub fn update_active_conversation_config(args: AgentConversationConfigArgs) -> Result<crate::db::AgentConversationRecord, ConversationError> {
    apply_conversation_config(
        &args.project_path,
        &args.agent_id,
        args.runner.as_deref(),
        args.model.as_deref(),
        args.account_id.as_deref(),
        args.effort.as_deref(),
    )
}

/// Applies a user-selected configuration to the agent's active Conversation (Spec 30 §7, D5).
///
/// Values are stored as overrides on the Conversation, except those equal to what the persona
/// already provides, which keep inheriting so later persona edits still apply. A runner other
/// than the one in effect (the Conversation's override, else the persona's) closes the
/// Conversation as `superseded` and opens a clean one carrying the new overrides: the context
/// of one runner is never injected into another implicitly.
///
/// Returns the Conversation that is active afterwards. `project_path` must be an exact
/// registered project path.
pub fn apply_conversation_config(
    project_path: &str,
    agent_id: &str,
    runner: Option<&str>,
    model: Option<&str>,
    account_id: Option<&str>,
    effort: Option<&str>,
) -> Result<crate::db::AgentConversationRecord, ConversationError> {
    let project = crate::db::db_get_projects()?
        .into_iter()
        .find(|p| p.path == project_path)
        .ok_or_else(|| ConversationError::ProjectNotRegistered(project_path.to_string()))?;
    let project_id = project.id as i64;

    let persona = crate::supervisor::get_supervisor()
        .compute_project_state(&project.path, &project.name)
        .agents
        .into_iter()
        .find(|a| a.id.eq_ignore_ascii_case(agent_id))
        .ok_or_else(|| ConversationError::AgentNotDeclared { agent_id: agent_id.to_string(), project_name: project.name.clone() })?;

    let active = crate::db::db_get_or_create_active_conversation(project_id, agent_id, None, None, None, None, None)?;

    let requested_runner = runner.map(str::trim).filter(|r| !r.is_empty());
    let current_runner = active.runner.as_deref().unwrap_or(&persona.runner);

    // The pair in effect afterwards must be one the runner manifest declares; it is checked
    // before anything is stored. A model belongs to its runner, so on a runner change only the
    // requested model counts, and the persona's model only pairs with the persona's runner.
    if requested_runner.is_some() || model.is_some() || effort.is_some() {
        let target_runner = requested_runner.unwrap_or(current_runner);
        let runner_changes = !target_runner.eq_ignore_ascii_case(current_runner);
        let inherited_model = if runner_changes {
            None
        } else {
            active.model.as_deref().or_else(|| persona.model.as_deref().filter(|_| target_runner.eq_ignore_ascii_case(&persona.runner)))
        };
        crate::kopeng::resolution::finalize_agent_config(
            &format!("conversation of agent '{}'", agent_id),
            target_runner,
            model.or(inherited_model),
            effort,
        )?;
    }

    if let Some(requested) = requested_runner {
        if !requested.eq_ignore_ascii_case(current_runner) {
            crate::logger::log(
                "INFO",
                "conversation",
                &format!("apply_conversation_config: runner '{}' -> '{}' for agent '{}'; closing conversation {} as superseded", current_runner, requested, agent_id, active.id),
            );
            crate::db::db_close_active_conversation(project_id, agent_id, "superseded")?;
            return Ok(crate::db::db_create_agent_conversation(
                project_id, agent_id, "General Workspace", Some(requested), model, account_id, effort,
            )?);
        }
    }

    // A value that equals the persona's while no override exists stays unset (inherits).
    let keep_inheriting = |current: &Option<String>, requested: Option<&'_ str>, from_persona: Option<&str>| -> bool {
        current.is_none()
            && matches!((requested, from_persona), (Some(r), Some(p)) if r.eq_ignore_ascii_case(p))
    };
    let runner_to_store = if keep_inheriting(&active.runner, requested_runner, Some(&persona.runner)) { None } else { requested_runner };
    let model_to_store = if keep_inheriting(&active.model, model, persona.model.as_deref()) { None } else { model };
    let effort_to_store = if keep_inheriting(&active.effort, effort, persona.effort.as_deref()) { None } else { effort };

    crate::db::db_update_conversation_config(&active.id, runner_to_store, model_to_store, account_id, effort_to_store)?;
    Ok(crate::db::db_get_or_create_active_conversation(project_id, agent_id, None, None, None, None, None)?)
}

/// Arguments of the daemon command `get_agent_turns_continuous`.
#[derive(serde::Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct GetAgentTurnsContinuousArgs {
    pub project_path: String,
    pub agent_id: String,
    pub limit_count: usize,
    /// Cursor for backward paging: only turns with a smaller id are returned.
    #[ts(type = "number | null")]
    pub before_id: Option<i64>,
}

/// Returns a continuous cross-session turn feed for an agent in a project, ordered chronologically.
/// Each turn includes the `topic` of its parent conversation record for rendering thematic separators.
pub fn get_agent_turns_continuous(args: GetAgentTurnsContinuousArgs) -> Result<Vec<crate::db::ChatTurnRecord>, ConversationError> {
    let project_id = resolve_project_id(&args.project_path)?;
    Ok(crate::db::db_get_agent_turns_continuous(project_id, &args.agent_id, args.limit_count, args.before_id)?)
}

/// Arguments of the daemon command `save_user_chat_turn`. An empty `conversationId` stores the
/// turn in the agent's active Conversation.
#[derive(serde::Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SaveUserChatTurnArgs {
    pub conversation_id: String,
    pub project_path: String,
    pub agent_id: String,
    pub dialogue: String,
}

/// Stores a user turn typed in the chat. A named Conversation must exist and belong to this
/// agent of this project; it is never redirected to another one (AGENTS.md §1.11).
pub fn save_user_chat_turn(args: SaveUserChatTurnArgs) -> Result<crate::db::ChatTurnRecord, ConversationError> {
    let SaveUserChatTurnArgs { conversation_id, project_path, agent_id, dialogue } = args;
    let project_id = resolve_project_id(&project_path)?;
    let normalized_agent = agent_id.trim().to_lowercase();

    let requested_id = conversation_id.trim();
    let (verified_conversation_id, agy_conversation_id) = if requested_id.is_empty() {
        let conv = crate::db::db_get_or_create_active_conversation(project_id, &normalized_agent, None, None, None, None, None)?;
        (conv.id, conv.native_session_id)
    } else {
        let conversation = crate::db::db_get_conversation_by_id(requested_id)?
            .ok_or_else(|| ConversationError::ConversationNotFound(requested_id.to_string()))?;
        if conversation.project_id != project_id || !conversation.agent_id.eq_ignore_ascii_case(&normalized_agent) {
            return Err(ConversationError::ConversationMismatch {
                conversation_id: requested_id.to_string(),
                agent_id: normalized_agent,
                project_path,
            });
        }
        (conversation.id, conversation.native_session_id)
    };

    let now = crate::db::now_epoch_secs();

    let mut turn = crate::db::ChatTurnRecord {
        id: None,
        conversation_id: verified_conversation_id,
        project_id,
        agent_id: normalized_agent,
        agy_conversation_id,
        role: "user".to_string(),
        dialogue,
        artifact_type: None,
        artifact_lang: None,
        artifact_label: None,
        artifact_file_path: None,
        artifact_content: None,
        is_ephemeral: false,
        terminal_snapshot: None,
        timestamp: now,
        topic: None,
        approval_payload: None,
    };

    let inserted_id = crate::db::db_insert_chat_turn(&turn)?;
    turn.id = Some(inserted_id);
    Ok(turn)
}

/// Arguments of the daemon commands that address one agent of a project (`build_conversation_brief`).
#[derive(serde::Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ProjectAgentArgs {
    pub project_path: String,
    pub agent_id: String,
}

/// Builds the Brief (Spec 30 §5) of the agent's active Conversation for review in the context
/// transfer dialog. Read-only: nothing is sent to any agent.
pub fn build_conversation_brief(args: ProjectAgentArgs) -> Result<crate::brief::Brief, ConversationError> {
    let project_id = resolve_project_id(&args.project_path)?;
    crate::logger::log("DEBUG", "conversation", &format!("build_conversation_brief: project_id={}, agent_id={}", project_id, args.agent_id));
    let conversation = crate::db::db_get_or_create_active_conversation(project_id, &args.agent_id, None, None, None, None, None)?;
    Ok(crate::brief::build_brief(&conversation.id)?)
}

/// Arguments of the daemon command `get_project_session_journals`.
#[derive(serde::Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct GetProjectSessionJournalsArgs {
    pub project_path: String,
    /// Only this agent's Minutes; all agents when absent.
    pub agent_id: Option<String>,
    pub limit_count: usize,
}

/// Session Minutes of a project, newest first.
pub fn get_project_session_journals(args: GetProjectSessionJournalsArgs) -> Result<Vec<crate::db::SessionJournalRecord>, ConversationError> {
    let project_id = resolve_project_id(&args.project_path)?;
    Ok(crate::db::db_get_project_session_journals(project_id, args.agent_id.as_deref(), args.limit_count)?)
}

/// Arguments of the daemon commands `get_project_todos` and `get_project_technical_debt`.
#[derive(serde::Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ProjectStatusFilterArgs {
    pub project_path: String,
    /// Only records with this status; all when absent.
    pub status_filter: Option<String>,
}

/// TODOs of a project.
pub fn get_project_todos(args: ProjectStatusFilterArgs) -> Result<Vec<crate::db::ProjectTodoRecord>, ConversationError> {
    let project_id = resolve_project_id(&args.project_path)?;
    Ok(crate::db::db_get_project_todos(project_id, args.status_filter.as_deref())?)
}

/// Arguments of the daemon command `create_project_todo`.
#[derive(serde::Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CreateProjectTodoArgs {
    pub project_path: String,
    pub agent_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    /// Defaults to `feature`.
    pub category: Option<String>,
    /// Defaults to 2.
    pub priority: Option<i32>,
}

/// Records a pending TODO of a project.
pub fn create_project_todo(args: CreateProjectTodoArgs) -> Result<crate::db::ProjectTodoRecord, ConversationError> {
    let CreateProjectTodoArgs { project_path, agent_id, title, description, category, priority } = args;
    let project_id = resolve_project_id(&project_path)?;
    let now = crate::db::now_epoch_secs();
    let conversation_id = agent_id.as_deref().and_then(|aid| {
        crate::db::db_get_or_create_active_conversation(project_id, aid, None, None, None, None, None).ok()
    }).map(|c| c.id);

    let mut rec = crate::db::ProjectTodoRecord {
        id: None,
        project_id,
        agent_id,
        agy_conversation_id: None,
        title,
        description,
        category: category.unwrap_or_else(|| "feature".to_string()),
        status: "pending".to_string(),
        priority: priority.unwrap_or(2),
        created_at: now,
        completed_at: None,
        conversation_id,
        file_path: None,
        line_start: None,
        line_end: None,
        journal_id: None,
    };

    let id = crate::db::db_insert_todo(&rec)?;
    rec.id = Some(id);
    Ok(rec)
}

/// Arguments of the daemon command `update_todo_status`.
#[derive(serde::Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct UpdateTodoStatusArgs {
    #[ts(type = "number")]
    pub todo_id: i64,
    pub status: String,
}

/// Sets the status of a TODO.
pub fn update_todo_status(args: UpdateTodoStatusArgs) -> Result<(), ConversationError> {
    Ok(crate::db::db_update_todo_status(args.todo_id, &args.status)?)
}

/// Technical debt records of a project.
pub fn get_project_technical_debt(args: ProjectStatusFilterArgs) -> Result<Vec<crate::db::ProjectTechnicalDebtRecord>, ConversationError> {
    let project_id = resolve_project_id(&args.project_path)?;
    Ok(crate::db::db_get_project_technical_debt(project_id, args.status_filter.as_deref())?)
}

/// Arguments of the daemon command `record_technical_debt`.
#[derive(serde::Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct RecordTechnicalDebtArgs {
    pub project_path: String,
    pub title: String,
    /// Defaults to `architectural`.
    pub category: Option<String>,
    pub impact_summary: String,
    pub remediation_plan: Option<String>,
    /// Defaults to `medium`.
    pub severity: Option<String>,
}

/// Records an open technical debt item of a project.
pub fn record_technical_debt(args: RecordTechnicalDebtArgs) -> Result<crate::db::ProjectTechnicalDebtRecord, ConversationError> {
    let RecordTechnicalDebtArgs { project_path, title, category, impact_summary, remediation_plan, severity } = args;
    let project_id = resolve_project_id(&project_path)?;
    let now = crate::db::now_epoch_secs();

    let mut rec = crate::db::ProjectTechnicalDebtRecord {
        id: None,
        project_id,
        agy_conversation_id: None,
        title,
        category: category.unwrap_or_else(|| "architectural".to_string()),
        impact_summary,
        remediation_plan,
        status: "open".to_string(),
        severity: severity.unwrap_or_else(|| "medium".to_string()),
        identified_at: now,
        resolved_at: None,
        conversation_id: None,
        milestone_id: None,
        file_path: None,
        line_start: None,
        line_end: None,
        journal_id: None,
    };

    let id = crate::db::db_insert_technical_debt(&rec)?;
    rec.id = Some(id);
    Ok(rec)
}

/// Arguments of the daemon command `resolve_technical_debt`.
#[derive(serde::Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ResolveTechnicalDebtArgs {
    #[ts(type = "number")]
    pub debt_id: i64,
}

/// Marks a technical debt item resolved.
pub fn resolve_technical_debt(args: ResolveTechnicalDebtArgs) -> Result<(), ConversationError> {
    Ok(crate::db::db_resolve_technical_debt(args.debt_id)?)
}

/// A reopened Conversation together with the Brief of the one it reopened (Spec 30 §8).
#[derive(serde::Serialize)]
pub struct ReopenOutcome {
    pub conversation: crate::db::AgentConversationRecord,
    pub brief: crate::brief::Brief,
}

/// Reopens `source_conversation_id` as the agent's new active Conversation and builds its Brief.
///
/// `runner` is the runner of the live process, when there is one: the new Conversation then gets
/// an open Native Session marked as seeded, carrying the live session's native id so a later
/// resume keeps working. Without a live process nothing is opened here; the next launch seeds the
/// Brief itself (`launch_agent`, via `reopened_from`).
pub fn reopen_conversation(
    project_id: i64,
    agent_id: &str,
    source_conversation_id: &str,
    runner: Option<&str>,
) -> Result<ReopenOutcome, ConversationError> {
    let previous = crate::db::db_get_or_create_active_conversation(project_id, agent_id, None, None, None, None, None)?;
    let brief = crate::brief::build_brief(source_conversation_id)?;
    let conversation = crate::db::db_reopen_conversation(project_id, agent_id, source_conversation_id)?;
    if let Some(runner) = runner {
        let native_session_row = crate::db::db_ensure_open_native_session(&conversation.id, runner)?;
        crate::db::db_mark_native_session_seeded(native_session_row)?;
        if let Some(native_session_id) = previous.native_session_id.as_deref() {
            crate::db::db_record_native_session_id(&conversation.id, native_session_id)?;
        }
    }
    Ok(ReopenOutcome { conversation, brief })
}

/// Arguments of the daemon command `reopen_conversation`.
#[derive(serde::Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ReopenConversationArgs {
    pub project_path: String,
    pub agent_id: String,
    pub source_conversation_id: String,
}

/// Reopens a past Conversation of `agent_id` from the UI. A running agent receives the Brief with
/// its next input; a stopped agent is seeded when it launches.
pub fn reopen_agent_conversation(args: ReopenConversationArgs) -> Result<crate::db::AgentConversationRecord, ConversationError> {
    let ReopenConversationArgs { project_path, agent_id, source_conversation_id } = args;
    let project_id = resolve_project_id(&project_path)?;
    crate::logger::log("INFO", "conversation", &format!("reopen_conversation: project_id={}, agent_id={}, source={}", project_id, agent_id, source_conversation_id));
    let key = crate::supervisor::ProcessSupervisor::format_agent_key(&project_path, &agent_id);
    let sup = crate::supervisor::get_supervisor();
    use crate::supervisor::ProcessStatus;
    let running = !matches!(sup.get_status(&key), ProcessStatus::Unlaunched | ProcessStatus::Completed { .. } | ProcessStatus::Failed { .. });

    let live_runner = if running { sup.process_runner(&key) } else { None };
    let outcome = reopen_conversation(project_id, &agent_id, &source_conversation_id, live_runner.as_deref())?;
    if running {
        sup.queue_context_for_next_input(&key, &outcome.brief.text)
            .map_err(|e| ConversationError::Supervisor(Box::new(e)))?;
    }
    Ok(outcome.conversation)
}

/// Arguments of the daemon command `search_conversations`.
#[derive(serde::Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SearchConversationsArgs {
    pub project_path: String,
    pub agent_id: String,
    pub query: String,
    pub status: Option<String>,
}

/// Searches the agent's own Conversations (Spec 30 §8) for the Thread search field, up to
/// `supervisor.search_conversations_max_results_count` results. An empty query returns the
/// most recent conversations of that agent.
pub fn search_conversations(args: SearchConversationsArgs) -> Result<Vec<crate::db::AgentConversationRecord>, ConversationError> {
    let project_id = resolve_project_id(&args.project_path)?;
    let limit = crate::config::resolve_effective_usize(None, None, "supervisor", "search_conversations_max_results_count");
    if args.query.trim().is_empty() {
        return Ok(crate::db::db_list_recent_conversations(project_id, &args.agent_id, args.status.as_deref(), limit)?);
    }
    Ok(crate::db::db_search_conversations(project_id, &args.agent_id, &args.query, args.status.as_deref(), limit)?)
}

/// Arguments of the daemon command `update_turn_approval`.
#[derive(serde::Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct UpdateTurnApprovalArgs {
    #[ts(type = "number")]
    pub turn_id: i64,
    pub status: String,
    pub feedback: Option<String>,
}

/// Records the user's decision on an approval card of a chat turn.
pub fn update_turn_approval(args: UpdateTurnApprovalArgs) -> Result<(), ConversationError> {
    Ok(crate::db::db_update_turn_approval_status(args.turn_id, &args.status, args.feedback.as_deref())?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compliance_state_is_scoped_per_project_and_reminds_once() {
        let (project_a, project_b, agent) = ("/tmp/conv-test-a", "/tmp/conv-test-b", "frontend");
        mark_user_message_sent(project_a, agent);

        // Same agent name in another project has no pending exchange.
        assert!(!should_inject_compliance_reminder(project_b, agent));
        // Unregistered project: the SQLite check errors, so the in-memory state decides.
        assert!(should_inject_compliance_reminder(project_a, agent));
        assert!(!should_inject_compliance_reminder(project_a, agent), "reminder is injected once per exchange");

        mark_user_message_sent(project_a, agent);
        mark_agent_responded(project_a, agent);
        assert!(!should_inject_compliance_reminder(project_a, agent), "an MCP response suppresses the reminder");
    }
}


