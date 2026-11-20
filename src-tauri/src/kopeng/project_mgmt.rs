//! Project Management domain service (Spec 34).
//!
//! Manages `project_task_stages`, `project_tasks`, `project_task_subtasks`, and `project_task_links`
//! as a first-class domain decoupled from `Journal` (`project_todos`), plus
//! per-project plugin opt-in via `[plugins.project_management] enabled = true`.

use std::path::Path;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::api::ApiError;
use crate::config::ConfigError;
use crate::db::{DatabaseError, DeleteStageOutcome, ProjectTaskBundle, ProjectTaskStageRecord};
use crate::events::{DaemonEvent, ProjectTasksUpdatedPayload};
use crate::registry::RegistryError;

/// Valid task domains across software, hardware, design, and client delivery.
pub const VALID_TASK_DOMAINS: &[&str] = &[
    "code",
    "hardware",
    "procurement",
    "design",
    "docs",
    "client",
    "milestone",
    "ops",
];

/// Typed errors of the Project Management subsystem.
#[derive(Debug, thiserror::Error)]
pub enum ProjectMgmtError {
    #[error("Invalid task {field} '{value}': expected one of {allowed}")]
    InvalidField {
        field: &'static str,
        value: String,
        allowed: &'static str,
    },
    #[error("Invalid stage '{0}' for this project")]
    InvalidStage(String),
    #[error("Title must not be empty")]
    EmptyTitle,
    #[error("Priority must be between 1 and 3, got {0}")]
    InvalidPriority(i32),
    #[error("Due date cannot be earlier than start date")]
    InvalidDateRange,
    #[error("Task #{0} not found")]
    TaskNotFound(i64),
    #[error("Subtask #{0} not found")]
    SubtaskNotFound(i64),
    #[error("Stage #{0} not found")]
    StageNotFound(i64),
    #[error("Milestone #{0} not found")]
    MilestoneNotFound(i64),
    #[error("Milestone #{milestone_id} does not belong to project '{project_path}'")]
    MilestoneProjectMismatch {
        milestone_id: i64,
        project_path: String,
    },
    #[error("Stage contains {0} task(s); select a destination stage before deleting")]
    StageHasTasks(usize),
    #[error("Cannot delete the last remaining stage of a project")]
    CannotDeleteLastStage,
    #[error("Cannot write project management setting: {0}")]
    SettingWrite(String),
    #[error("Background task failed: {0}")]
    Task(String),
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Skill(#[from] crate::kopeng::skill::SkillError),
    #[error(transparent)]
    Git(#[from] crate::kopeng::git::GitError),
    #[error(transparent)]
    Github(#[from] crate::kopeng::github::GithubError),
    #[error(transparent)]
    Forge(#[from] crate::kopeng::forge::ForgeError),
}

impl ApiError for ProjectMgmtError {
    fn status(&self) -> StatusCode {
        match self {
            Self::InvalidField { .. }
            | Self::InvalidStage(_)
            | Self::EmptyTitle
            | Self::InvalidPriority(_)
            | Self::InvalidDateRange
            | Self::StageHasTasks(_)
            | Self::CannotDeleteLastStage => StatusCode::BAD_REQUEST,
            Self::TaskNotFound(_)
            | Self::SubtaskNotFound(_)
            | Self::StageNotFound(_)
            | Self::MilestoneNotFound(_) => StatusCode::NOT_FOUND,
            Self::MilestoneProjectMismatch { .. } => StatusCode::FORBIDDEN,
            Self::SettingWrite(_) | Self::Task(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Database(e) => e.status(),
            Self::Registry(e) => e.status(),
            Self::Config(e) => e.status(),
            Self::Skill(e) => e.status(),
            Self::Git(e) => e.status(),
            Self::Github(e) => e.status(),
            Self::Forge(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::InvalidField { .. } => "ProjectMgmtError::InvalidField",
            Self::InvalidStage(_) => "ProjectMgmtError::InvalidStage",
            Self::EmptyTitle => "ProjectMgmtError::EmptyTitle",
            Self::InvalidPriority(_) => "ProjectMgmtError::InvalidPriority",
            Self::InvalidDateRange => "ProjectMgmtError::InvalidDateRange",
            Self::TaskNotFound(_) => "ProjectMgmtError::TaskNotFound",
            Self::SubtaskNotFound(_) => "ProjectMgmtError::SubtaskNotFound",
            Self::StageNotFound(_) => "ProjectMgmtError::StageNotFound",
            Self::MilestoneNotFound(_) => "ProjectMgmtError::MilestoneNotFound",
            Self::MilestoneProjectMismatch { .. } => "ProjectMgmtError::MilestoneProjectMismatch",
            Self::StageHasTasks(_) => "ProjectMgmtError::StageHasTasks",
            Self::CannotDeleteLastStage => "ProjectMgmtError::CannotDeleteLastStage",
            Self::SettingWrite(_) => "ProjectMgmtError::SettingWrite",
            Self::Task(_) => "ProjectMgmtError::Task",
            Self::Database(e) => e.code(),
            Self::Registry(e) => e.code(),
            Self::Config(e) => e.code(),
            Self::Skill(e) => e.code(),
            Self::Git(e) => e.code(),
            Self::Github(e) => e.code(),
            Self::Forge(e) => e.code(),
        }
    }
}

/// Checks whether `[plugins.project_management] enabled = true` is set in `<project_path>/rula.toml`.
/// Logs a warning if `rula.toml` is unreadable or `enabled` has a non-boolean type.
pub fn is_project_management_enabled(project_path: &Path) -> bool {
    let doc = match crate::config::read_project_rula(project_path) {
        Ok(Some(doc)) => doc,
        Ok(None) => return false,
        Err(e) => {
            crate::logger::log(
                "WARN",
                "project_mgmt",
                &format!(
                    "Cannot read rula.toml at {} for project_management.enabled: {e}",
                    project_path.display()
                ),
            );
            return false;
        }
    };
    let Some(val) = doc
        .get("plugins")
        .and_then(|p| p.get("project_management"))
        .and_then(|pm| pm.get("enabled"))
    else {
        return false;
    };
    match val.as_bool() {
        Some(enabled) => enabled,
        None => {
            crate::logger::log(
                "WARN",
                "project_mgmt",
                &format!(
                    "Invalid non-boolean [plugins.project_management].enabled in {}",
                    project_path.display()
                ),
            );
            false
        }
    }
}

/// Resolves `[plugins.project_management].tracking_mode` (`linear` | `shape_up` | `weighted`).
/// Returns `"linear"` when absent; fails explicitly on unreadable `rula.toml` or unrecognized values.
pub fn resolve_tracking_mode(project_path: &Path) -> Result<String, ProjectMgmtError> {
    let Some(doc) = crate::config::read_project_rula(project_path)? else {
        return Ok("linear".to_string());
    };
    let Some(val) = doc
        .get("plugins")
        .and_then(|p| p.get("project_management"))
        .and_then(|pm| pm.get("tracking_mode"))
    else {
        return Ok("linear".to_string());
    };
    let Some(mode) = val.as_str() else {
        return Err(ProjectMgmtError::InvalidField {
            field: "tracking_mode",
            value: val.to_string(),
            allowed: "linear | shape_up | weighted",
        });
    };
    match mode {
        "linear" | "shape_up" | "weighted" => Ok(mode.to_string()),
        other => Err(ProjectMgmtError::InvalidField {
            field: "tracking_mode",
            value: other.to_string(),
            allowed: "linear | shape_up | weighted",
        }),
    }
}

/// Arguments for `toggle_project_management_plugin`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ToggleProjectManagementPluginArgs {
    pub project_path: String,
    pub enabled: bool,
}

/// Result returned by `toggle_project_management_plugin`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProjectManagementStatus {
    pub is_enabled: bool,
}

/// Arguments for `list_project_tasks`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ListProjectTasksArgs {
    pub project_path: String,
    pub status_filter: Option<String>,
}

/// Arguments for `create_project_task`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CreateProjectTaskArgs {
    pub project_path: String,
    pub title: String,
    pub description: Option<String>,
    pub domain: Option<String>,
    pub status: Option<String>,
    pub priority: Option<i32>,
    #[ts(type = "number | null")]
    pub milestone_id: Option<i64>,
    pub weight_points: Option<i32>,
    #[ts(type = "number | null")]
    pub start_epoch_seconds: Option<i64>,
    #[ts(type = "number | null")]
    pub due_epoch_seconds: Option<i64>,
    pub github_repo: Option<String>,
    #[ts(type = "number | null")]
    pub github_issue_number: Option<i64>,
    #[ts(type = "number | null")]
    pub github_pr_number: Option<i64>,
    pub branch_name: Option<String>,
}

/// Arguments for `update_project_task`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct UpdateProjectTaskArgs {
    #[ts(type = "number")]
    pub task_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub domain: String,
    pub status: String,
    pub priority: i32,
    #[ts(type = "number | null")]
    pub milestone_id: Option<i64>,
    pub weight_points: Option<i32>,
    #[ts(type = "number | null")]
    pub start_epoch_seconds: Option<i64>,
    #[ts(type = "number | null")]
    pub due_epoch_seconds: Option<i64>,
    pub branch_name: Option<String>,
}

/// Arguments for `delete_project_task`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct DeleteProjectTaskArgs {
    #[ts(type = "number")]
    pub task_id: i64,
}

/// Arguments for `create_task_subtask`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CreateTaskSubtaskArgs {
    #[ts(type = "number")]
    pub task_id: i64,
    pub title: String,
    #[ts(type = "number | null")]
    pub linked_todo_id: Option<i64>,
}

/// Arguments for `toggle_task_subtask`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ToggleTaskSubtaskArgs {
    #[ts(type = "number")]
    pub subtask_id: i64,
    pub is_completed: bool,
}

/// Arguments for `delete_task_subtask`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct DeleteTaskSubtaskArgs {
    #[ts(type = "number")]
    pub subtask_id: i64,
}

/// Arguments for `link_task_relation` and `unlink_task_relation`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct TaskRelationArgs {
    #[ts(type = "number")]
    pub task_id: i64,
    pub link_type: String,
    #[ts(type = "number")]
    pub target_id: i64,
}

/// Arguments for `take_forge_issue`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct TakeForgeIssueArgs {
    #[ts(type = "number")]
    pub project_id: usize,
    #[ts(type = "number")]
    pub issue_number: i64,
    pub create_branch: Option<bool>,
    pub branch_name: Option<String>,
    pub create_linked_todo: Option<bool>,
}

/// Arguments for `list_project_stages`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ListProjectStagesArgs {
    pub project_path: String,
}

/// Arguments for `create_project_stage`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CreateProjectStageArgs {
    pub project_path: String,
    pub label: String,
    pub is_terminal: Option<bool>,
}

/// Arguments for `update_project_stage`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct UpdateProjectStageArgs {
    #[ts(type = "number")]
    pub stage_id: i64,
    pub label: String,
    pub is_collapsed: bool,
    pub is_terminal: bool,
}

/// Arguments for `reorder_project_stages`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ReorderProjectStagesArgs {
    pub project_path: String,
    #[ts(type = "number[]")]
    pub ordered_stage_ids: Vec<i64>,
}

/// Arguments for `delete_project_stage`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct DeleteProjectStageArgs {
    #[ts(type = "number")]
    pub stage_id: i64,
    #[ts(type = "number | null")]
    pub reassign_to_stage_id: Option<i64>,
}

fn validate_domain(domain: &str) -> Result<(), ProjectMgmtError> {
    if !VALID_TASK_DOMAINS.contains(&domain) {
        return Err(ProjectMgmtError::InvalidField {
            field: "domain",
            value: domain.to_string(),
            allowed: "code, hardware, procurement, design, docs, client, milestone, ops",
        });
    }
    Ok(())
}

fn validate_project_stage(project_id: i64, status: &str) -> Result<(), ProjectMgmtError> {
    if !crate::db::db_project_stage_exists(project_id, status)? {
        return Err(ProjectMgmtError::InvalidStage(status.to_string()));
    }
    Ok(())
}

fn validate_priority(priority: i32) -> Result<(), ProjectMgmtError> {
    if !(1..=3).contains(&priority) {
        return Err(ProjectMgmtError::InvalidPriority(priority));
    }
    Ok(())
}

fn validate_date_range(
    start_epoch_seconds: Option<i64>,
    due_epoch_seconds: Option<i64>,
) -> Result<(), ProjectMgmtError> {
    if let (Some(start), Some(due)) = (start_epoch_seconds, due_epoch_seconds) {
        if due < start {
            return Err(ProjectMgmtError::InvalidDateRange);
        }
    }
    Ok(())
}

fn emit_project_tasks_updated_by_id(project_id: i64) {
    if let Ok(uid) = usize::try_from(project_id) {
        if let Ok(Some(proj)) = crate::db::db_get_project_by_id(uid) {
            crate::supervisor::get_supervisor().emit_event(DaemonEvent::project_tasks_updated(
                &ProjectTasksUpdatedPayload {
                    project_path: proj.path,
                    project_id,
                },
            ));
        }
    }
}

/// Enables or disables `[plugins.project_management]` in `<project_path>/rula.toml` and notifies UI clients.
pub async fn toggle_project_management_plugin(
    project_path: String,
    enabled: bool,
) -> Result<ProjectManagementStatus, ProjectMgmtError> {
    crate::logger::log("INFO", "project_mgmt", &format!("toggle_project_management_plugin: project='{}', enabled={}", project_path, enabled));
    let project = crate::registry::find_project_by_path(&project_path)?;
    let p_path = Path::new(&project.path);
    let rula_path = p_path.join("rula.toml");

    let mut rula_val = crate::config::read_project_rula(p_path)?
        .ok_or_else(|| ProjectMgmtError::SettingWrite("project is not initialized (missing rula.toml)".to_string()))?;

    if let Some(root_table) = rula_val.as_table_mut() {
        let plugins_entry = root_table
            .entry("plugins".to_string())
            .or_insert_with(|| toml::Value::Table(Default::default()));
        if let Some(plugins_table) = plugins_entry.as_table_mut() {
            let pm_entry = plugins_table
                .entry("project_management".to_string())
                .or_insert_with(|| toml::Value::Table(Default::default()));
            if let Some(pm_table) = pm_entry.as_table_mut() {
                pm_table.insert("enabled".to_string(), toml::Value::Boolean(enabled));
            }
        }
    }

    let serialized = toml::to_string_pretty(&rula_val)
        .map_err(|e| ProjectMgmtError::SettingWrite(format!("serialize rula.toml: {}", e)))?;
    tokio::fs::write(&rula_path, serialized)
        .await
        .map_err(|e| ProjectMgmtError::SettingWrite(format!("write {}: {}", rula_path.display(), e)))?;

    if enabled {
        crate::kopeng::skill::inject_specialized_skill(
            project.path.clone(),
            "oneship-project-mgmt".to_string(),
        )?;
    } else {
        crate::kopeng::skill::cleanup_ephemeral_skill(
            project.path.clone(),
            "oneship-project-mgmt".to_string(),
        )?;
    }

    crate::db::notify_projects_updated();
    Ok(ProjectManagementStatus { is_enabled: enabled })
}

/// Lists all ordered Kanban stages for a registered project.
pub async fn list_project_stages(project_path: String) -> Result<Vec<ProjectTaskStageRecord>, ProjectMgmtError> {
    crate::logger::log("DEBUG", "project_mgmt", &format!("list_project_stages: project='{}'", project_path));
    tokio::task::spawn_blocking(move || {
        let project = crate::registry::find_project_by_path(&project_path)?;
        Ok(crate::db::db_list_project_stages(project.id as i64)?)
    })
    .await
    .map_err(|e| ProjectMgmtError::Task(e.to_string()))?
}

/// Creates a custom Kanban stage on a registered project.
pub async fn create_project_stage(args: CreateProjectStageArgs) -> Result<ProjectTaskStageRecord, ProjectMgmtError> {
    crate::logger::log("INFO", "project_mgmt", &format!("create_project_stage: project='{}', label='{}'", args.project_path, args.label));
    let label = args.label.trim().to_string();
    if label.is_empty() {
        return Err(ProjectMgmtError::EmptyTitle);
    }
    let is_terminal = args.is_terminal.unwrap_or(false);
    tokio::task::spawn_blocking(move || {
        let project = crate::registry::find_project_by_path(&args.project_path)?;
        let project_id = project.id as i64;
        let stage = crate::db::db_insert_project_stage(project_id, &label, is_terminal)?;
        emit_project_tasks_updated_by_id(project_id);
        Ok(stage)
    })
    .await
    .map_err(|e| ProjectMgmtError::Task(e.to_string()))?
}

/// Updates a Kanban stage's label, collapsed state, and terminal flag.
pub async fn update_project_stage(args: UpdateProjectStageArgs) -> Result<(), ProjectMgmtError> {
    crate::logger::log("INFO", "project_mgmt", &format!("update_project_stage: stage_id={}, label='{}'", args.stage_id, args.label));
    let label = args.label.trim().to_string();
    if label.is_empty() {
        return Err(ProjectMgmtError::EmptyTitle);
    }
    tokio::task::spawn_blocking(move || {
        let project_id = crate::db::db_update_project_stage(
            args.stage_id,
            &label,
            args.is_collapsed,
            args.is_terminal,
        )?
        .ok_or(ProjectMgmtError::StageNotFound(args.stage_id))?;
        emit_project_tasks_updated_by_id(project_id);
        Ok(())
    })
    .await
    .map_err(|e| ProjectMgmtError::Task(e.to_string()))?
}

/// Persists a custom column order for all stages of a project.
pub async fn reorder_project_stages(args: ReorderProjectStagesArgs) -> Result<(), ProjectMgmtError> {
    crate::logger::log("INFO", "project_mgmt", &format!("reorder_project_stages: project='{}', count={}", args.project_path, args.ordered_stage_ids.len()));
    tokio::task::spawn_blocking(move || {
        let project = crate::registry::find_project_by_path(&args.project_path)?;
        let project_id = project.id as i64;
        crate::db::db_reorder_project_stages(project_id, &args.ordered_stage_ids)?;
        emit_project_tasks_updated_by_id(project_id);
        Ok(())
    })
    .await
    .map_err(|e| ProjectMgmtError::Task(e.to_string()))?
}

/// Deletes a Kanban stage, optionally reassigning contained tasks to another stage.
pub async fn delete_project_stage(args: DeleteProjectStageArgs) -> Result<(), ProjectMgmtError> {
    crate::logger::log("INFO", "project_mgmt", &format!("delete_project_stage: stage_id={}, reassign_to={:?}", args.stage_id, args.reassign_to_stage_id));
    tokio::task::spawn_blocking(move || {
        let outcome = crate::db::db_delete_project_stage(args.stage_id, args.reassign_to_stage_id)?
            .ok_or(ProjectMgmtError::StageNotFound(args.stage_id))?;
        match outcome {
            DeleteStageOutcome::Deleted { project_id } => {
                emit_project_tasks_updated_by_id(project_id);
                Ok(())
            }
            DeleteStageOutcome::HasTasks { tasks_count } => Err(ProjectMgmtError::StageHasTasks(tasks_count)),
            DeleteStageOutcome::LastStage => Err(ProjectMgmtError::CannotDeleteLastStage),
        }
    })
    .await
    .map_err(|e| ProjectMgmtError::Task(e.to_string()))?
}

/// Lists all project management tasks (with subtasks and `Journal` links) for a registered project.
pub async fn list_project_tasks(
    project_path: String,
    status_filter: Option<String>,
) -> Result<Vec<ProjectTaskBundle>, ProjectMgmtError> {
    crate::logger::log("DEBUG", "project_mgmt", &format!("list_project_tasks: project='{}', status_filter={:?}", project_path, status_filter));
    tokio::task::spawn_blocking(move || {
        let project = crate::registry::find_project_by_path(&project_path)?;
        let project_id = project.id as i64;
        if let Some(ref s) = status_filter {
            validate_project_stage(project_id, s)?;
        }
        Ok(crate::db::db_list_project_tasks(project_id, status_filter.as_deref())?)
    })
    .await
    .map_err(|e| ProjectMgmtError::Task(e.to_string()))?
}

/// Creates a new project management task and returns its new row ID.
pub async fn create_project_task(args: CreateProjectTaskArgs) -> Result<i64, ProjectMgmtError> {
    crate::logger::log("INFO", "project_mgmt", &format!("create_project_task: project='{}', title='{}'", args.project_path, args.title));
    let title = args.title.trim().to_string();
    if title.is_empty() {
        return Err(ProjectMgmtError::EmptyTitle);
    }
    let domain = args.domain.unwrap_or_else(|| "code".to_string());
    let priority = args.priority.unwrap_or(2);
    validate_domain(&domain)?;
    validate_priority(priority)?;
    validate_date_range(args.start_epoch_seconds, args.due_epoch_seconds)?;

    tokio::task::spawn_blocking(move || {
        let project = crate::registry::find_project_by_path(&args.project_path)?;
        let project_id = project.id as i64;
        let status = match args.status.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(s) => {
                validate_project_stage(project_id, s)?;
                s.to_string()
            }
            None => crate::db::db_resolve_default_stage_slug(project_id, Some("backlog"))?,
        };
        let desc = args.description.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let repo = args.github_repo.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let branch = args.branch_name.as_deref().map(str::trim).filter(|s| !s.is_empty());
        if let Some(mid) = args.milestone_id {
            validate_project_milestone(project_id, mid)?;
        }
        let weight_points = args.weight_points.unwrap_or(1).max(1);

        let id = crate::db::db_insert_project_task(
            project_id,
            &title,
            desc,
            &domain,
            &status,
            priority,
            args.milestone_id,
            weight_points,
            args.start_epoch_seconds,
            args.due_epoch_seconds,
            repo,
            args.github_issue_number,
            args.github_pr_number,
            branch,
        )?;
        emit_project_tasks_updated_by_id(project_id);
        if args.milestone_id.is_some() {
            emit_project_milestones_updated_by_id(project_id);
        }
        Ok(id)
    })
    .await
    .map_err(|e| ProjectMgmtError::Task(e.to_string()))?
}

/// Updates an existing project management task.
pub async fn update_project_task(args: UpdateProjectTaskArgs) -> Result<(), ProjectMgmtError> {
    crate::logger::log("INFO", "project_mgmt", &format!("update_project_task: task_id={}, status='{}'", args.task_id, args.status));
    let title = args.title.trim().to_string();
    if title.is_empty() {
        return Err(ProjectMgmtError::EmptyTitle);
    }
    validate_domain(&args.domain)?;
    validate_priority(args.priority)?;
    validate_date_range(args.start_epoch_seconds, args.due_epoch_seconds)?;

    tokio::task::spawn_blocking(move || {
        let existing = crate::db::db_get_project_task(args.task_id)?
            .ok_or(ProjectMgmtError::TaskNotFound(args.task_id))?;
        validate_project_stage(existing.project_id, &args.status)?;
        if let Some(mid) = args.milestone_id {
            validate_project_milestone(existing.project_id, mid)?;
        }
        let weight_points = args.weight_points.unwrap_or(existing.weight_points).max(1);
        let desc = args.description.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let branch = args.branch_name.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let project_id = crate::db::db_update_project_task(
            args.task_id,
            &title,
            desc,
            &args.domain,
            &args.status,
            args.priority,
            args.milestone_id,
            weight_points,
            args.start_epoch_seconds,
            args.due_epoch_seconds,
            branch,
        )?
        .ok_or(ProjectMgmtError::TaskNotFound(args.task_id))?;
        emit_project_tasks_updated_by_id(project_id);
        if existing.milestone_id.is_some() || args.milestone_id.is_some() {
            emit_project_milestones_updated_by_id(project_id);
        }
        Ok(())
    })
    .await
    .map_err(|e| ProjectMgmtError::Task(e.to_string()))?
}

/// Deletes a project management task and its cascaded subtasks/links.
pub async fn delete_project_task(task_id: i64) -> Result<(), ProjectMgmtError> {
    crate::logger::log("INFO", "project_mgmt", &format!("delete_project_task: task_id={}", task_id));
    tokio::task::spawn_blocking(move || {
        let project_id = crate::db::db_delete_project_task(task_id)?
            .ok_or(ProjectMgmtError::TaskNotFound(task_id))?;
        emit_project_tasks_updated_by_id(project_id);
        Ok(())
    })
    .await
    .map_err(|e| ProjectMgmtError::Task(e.to_string()))?
}

/// Adds a subtask inside a project management task and returns the new subtask ID.
pub async fn create_task_subtask(args: CreateTaskSubtaskArgs) -> Result<i64, ProjectMgmtError> {
    crate::logger::log("INFO", "project_mgmt", &format!("create_task_subtask: task_id={}, title='{}'", args.task_id, args.title));
    let title = args.title.trim().to_string();
    if title.is_empty() {
        return Err(ProjectMgmtError::EmptyTitle);
    }
    tokio::task::spawn_blocking(move || {
        let (sub_id, project_id) = crate::db::db_insert_task_subtask(
            args.task_id,
            &title,
            args.linked_todo_id,
        )?
        .ok_or(ProjectMgmtError::TaskNotFound(args.task_id))?;
        emit_project_tasks_updated_by_id(project_id);
        Ok(sub_id)
    })
    .await
    .map_err(|e| ProjectMgmtError::Task(e.to_string()))?
}

/// Marks a subtask completed or incomplete.
pub async fn toggle_task_subtask(subtask_id: i64, is_completed: bool) -> Result<(), ProjectMgmtError> {
    crate::logger::log("DEBUG", "project_mgmt", &format!("toggle_task_subtask: subtask_id={}, is_completed={}", subtask_id, is_completed));
    tokio::task::spawn_blocking(move || {
        let project_id = crate::db::db_set_task_subtask_completed(subtask_id, is_completed)?
            .ok_or(ProjectMgmtError::SubtaskNotFound(subtask_id))?;
        emit_project_tasks_updated_by_id(project_id);
        Ok(())
    })
    .await
    .map_err(|e| ProjectMgmtError::Task(e.to_string()))?
}

/// Deletes a subtask.
pub async fn delete_task_subtask(subtask_id: i64) -> Result<(), ProjectMgmtError> {
    crate::logger::log("INFO", "project_mgmt", &format!("delete_task_subtask: subtask_id={}", subtask_id));
    tokio::task::spawn_blocking(move || {
        let project_id = crate::db::db_delete_task_subtask(subtask_id)?
            .ok_or(ProjectMgmtError::SubtaskNotFound(subtask_id))?;
        emit_project_tasks_updated_by_id(project_id);
        Ok(())
    })
    .await
    .map_err(|e| ProjectMgmtError::Task(e.to_string()))?
}

/// Links a `ProjectTaskRecord` to a `Journal` entity (`todo` or `journal`).
pub async fn link_task_relation(args: TaskRelationArgs) -> Result<(), ProjectMgmtError> {
    crate::logger::log("INFO", "project_mgmt", &format!("link_task_relation: task_id={}, link_type='{}', target_id={}", args.task_id, args.link_type, args.target_id));
    tokio::task::spawn_blocking(move || {
        let project_id = crate::db::db_link_task_relation(args.task_id, &args.link_type, args.target_id)?;
        emit_project_tasks_updated_by_id(project_id);
        Ok(())
    })
    .await
    .map_err(|e| ProjectMgmtError::Task(e.to_string()))?
}

/// Unlinks a `ProjectTaskRecord` from a `Journal` entity.
pub async fn unlink_task_relation(args: TaskRelationArgs) -> Result<(), ProjectMgmtError> {
    crate::logger::log("INFO", "project_mgmt", &format!("unlink_task_relation: task_id={}, link_type='{}', target_id={}", args.task_id, args.link_type, args.target_id));
    tokio::task::spawn_blocking(move || {
        let project_id = crate::db::db_unlink_task_relation(args.task_id, &args.link_type, args.target_id)?
            .ok_or(ProjectMgmtError::TaskNotFound(args.task_id))?;
        emit_project_tasks_updated_by_id(project_id);
        Ok(())
    })
    .await
    .map_err(|e| ProjectMgmtError::Task(e.to_string()))?
}

fn slugify_branch_suffix(title: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
        if out.len() >= 36 {
            break;
        }
    }
    out.trim_matches('-').to_string()
}

/// Takes a Forge issue: creates a linked `ProjectTaskRecord`, optionally creates a git branch,
/// and optionally creates & links an implementation `ProjectTodoRecord` in `Journal`.
pub async fn take_forge_issue(args: TakeForgeIssueArgs) -> Result<i64, ProjectMgmtError> {
    crate::logger::log("INFO", "project_mgmt", &format!("take_forge_issue: project_id={}, issue_number={}", args.project_id, args.issue_number));
    let (project, issue, ctx) = tokio::task::spawn_blocking(move || {
        let project = crate::registry::find_project_by_id(args.project_id)?;
        let path = Path::new(&project.path);
        let provider = crate::kopeng::forge::resolve_provider_for_path(path)?;
        let proj_ref = crate::kopeng::forge::ForgeProjectRef {
            project_id: args.project_id,
            path,
        };
        let issue = provider.get_issue(proj_ref, args.issue_number, false)?;
        let ctx = provider.collab_context(proj_ref, false)?;
        Ok::<_, ProjectMgmtError>((project, issue, ctx))
    })
    .await
    .map_err(|e| ProjectMgmtError::Task(e.to_string()))??;

    let want_branch = args.create_branch.unwrap_or(false);
    let created_branch = if want_branch {
        let raw_name = args
            .branch_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                let slug = slugify_branch_suffix(&issue.title);
                if slug.is_empty() {
                    format!("fix/issue-{}", issue.number)
                } else {
                    format!("fix/{}-{}", issue.number, slug)
                }
            });
        let start_point = if ctx.has_upstream {
            ctx.default_branch
                .as_deref()
                .map(|b| format!("upstream/{}", b))
        } else {
            None
        };
        Some(
            crate::kopeng::git::create_git_branch(args.project_id, raw_name, start_point, Some(false)).await?,
        )
    } else {
        args.branch_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };

    let want_todo = args.create_linked_todo.unwrap_or(true);
    let project_id_i64 = project.id as i64;
    let project_path = project.path.clone();

    tokio::task::spawn_blocking(move || {
        let initial_stage = crate::db::db_resolve_default_stage_slug(project_id_i64, Some("planned"))?;
        let task_title = format!("#{} {}", issue.number, issue.title);
        let desc = format!("{}\n\nSource: {}", issue.body.trim(), issue.url);
        let task_id = crate::db::db_insert_project_task(
            project_id_i64,
            &task_title,
            Some(desc.trim()),
            "code",
            &initial_stage,
            2,
            None,
            1,
            None,
            None,
            ctx.target_repo.as_deref(),
            Some(issue.number),
            None,
            created_branch.as_deref(),
        )?;

        if want_todo {
            let todo_rec = crate::db::ProjectTodoRecord {
                id: None,
                project_id: project_id_i64,
                agent_id: None,
                agy_conversation_id: None,
                title: format!("Resolve GitHub issue #{}: {}", issue.number, issue.title),
                description: Some(format!("Issue URL: {}", issue.url)),
                category: "fix".to_string(),
                status: "pending".to_string(),
                priority: 2,
                created_at: 0,
                completed_at: None,
                conversation_id: None,
                file_path: None,
                line_start: None,
                line_end: None,
                journal_id: None,
            };
            let todo_id = crate::db::db_insert_todo(&todo_rec)?;
            crate::db::db_link_task_relation(task_id, "todo", todo_id)?;
            crate::supervisor::get_supervisor().emit_event(DaemonEvent::todo_updated(
                &crate::events::TodoUpdatedPayload {
                    project_id: project_id_i64,
                    project_path: project_path.clone(),
                    todo_id,
                    action: "create".to_string(),
                    agent_id: "operator".to_string(),
                },
            ));
        }

        emit_project_tasks_updated_by_id(project_id_i64);
        Ok(task_id)
    })
    .await
    .map_err(|e| ProjectMgmtError::Task(e.to_string()))?
}


// ============================================================================
// Strategic Milestones, Tracking Modes & Aggregated Inspector (Spec 34 §7)
// ============================================================================

const VALID_MILESTONE_STATUSES: &[&str] = &["planned", "active", "completed", "archived"];

fn validate_milestone_status(status: &str) -> Result<(), ProjectMgmtError> {
    if VALID_MILESTONE_STATUSES.contains(&status) {
        return Ok(());
    }
    Err(ProjectMgmtError::InvalidField {
        field: "milestone status",
        value: status.to_string(),
        allowed: "planned, active, completed, archived",
    })
}

fn validate_project_milestone(project_id: i64, milestone_id: i64) -> Result<(), ProjectMgmtError> {
    let rec = crate::db::db_get_project_milestone(milestone_id)?
        .ok_or(ProjectMgmtError::MilestoneNotFound(milestone_id))?;
    if rec.project_id != project_id {
        return Err(ProjectMgmtError::MilestoneNotFound(milestone_id));
    }
    Ok(())
}

fn emit_project_milestones_updated_by_id(project_id: i64) {
    if let Ok(project) = crate::registry::find_project_by_id(project_id as usize) {
        crate::supervisor::get_supervisor().emit_event(DaemonEvent::project_milestones_updated(
            &crate::events::ProjectMilestonesUpdatedPayload {
                project_path: project.path,
                project_id,
            },
        ));
    }
}

/// Aggregated metrics and Drift Compass signals for a single [`ProjectMilestoneRecord`].
#[derive(Debug, Clone, serde::Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProjectMilestoneSummary {
    pub milestone: crate::db::ProjectMilestoneRecord,
    pub effective_weight_percent: f64,
    pub tasks_total_count: usize,
    pub tasks_done_count: usize,
    #[ts(type = "number")]
    pub total_weight_points: i64,
    #[ts(type = "number")]
    pub done_weight_points: i64,
    pub progress_percent: f64,
    #[ts(type = "number | null")]
    pub schedule_drift_days: Option<i64>,
    pub uphill_risk_flag: bool,
    pub open_tech_debt_count: usize,
    pub critical_tech_debt_count: usize,
    pub linked_todos_total_count: usize,
    pub linked_todos_completed_count: usize,
    #[ts(type = "number")]
    pub tokens_used_count: i64,
    pub stage_counts: std::collections::HashMap<String, usize>,
    pub domain_counts: std::collections::HashMap<String, usize>,
}

/// Overview payload returned by `list_project_milestones`.
#[derive(Debug, Clone, serde::Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProjectMilestonesOverview {
    pub tracking_mode: String,
    pub overall_progress_percent: f64,
    #[ts(type = "number")]
    pub total_tokens_used_count: i64,
    pub total_cost_dollars: f64,
    pub usage_by_runner_model: Vec<crate::db::RunnerModelUsageSummary>,
    pub milestones: Vec<ProjectMilestoneSummary>,
}

/// Aggregated milestone detail payload returned by `get_project_milestone_detail`.
#[derive(Debug, Clone, serde::Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProjectMilestoneDetail {
    pub summary: ProjectMilestoneSummary,
    pub tasks: Vec<crate::db::ProjectTaskBundle>,
    pub journal_entries: Vec<crate::db::SessionJournalRecord>,
    pub todos: Vec<crate::db::ProjectTodoRecord>,
    pub tech_debt: Vec<crate::db::ProjectTechnicalDebtRecord>,
}

/// Arguments for `get_project_milestone_detail`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct GetProjectMilestoneDetailArgs {
    pub project_path: String,
    #[ts(type = "number")]
    pub milestone_id: i64,
}

/// Arguments for `create_project_milestone`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CreateProjectMilestoneArgs {
    pub project_path: String,
    pub title: String,
    pub description: Option<String>,
    pub status: Option<String>,
    pub weight_percent: Option<f64>,
    pub hill_position_percent: Option<i32>,
    pub appetite_days: Option<i32>,
    #[ts(type = "number | null")]
    pub planned_start_epoch_seconds: Option<i64>,
    #[ts(type = "number | null")]
    pub planned_due_epoch_seconds: Option<i64>,
}

/// Arguments for `update_project_milestone`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct UpdateProjectMilestoneArgs {
    pub project_path: String,
    #[ts(type = "number")]
    pub milestone_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub weight_percent: f64,
    pub hill_position_percent: i32,
    pub appetite_days: Option<i32>,
    #[ts(type = "number | null")]
    pub planned_start_epoch_seconds: Option<i64>,
    #[ts(type = "number | null")]
    pub planned_due_epoch_seconds: Option<i64>,
    pub sort_order: i32,
}

/// Arguments for `delete_project_milestone`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct DeleteProjectMilestoneArgs {
    pub project_path: String,
    #[ts(type = "number")]
    pub milestone_id: i64,
}

pub(crate) fn compute_milestone_summaries(
    project_id: i64,
    tracking_mode: &str,
    milestones: Vec<crate::db::ProjectMilestoneRecord>,
) -> Result<(f64, i64, Vec<ProjectMilestoneSummary>), ProjectMgmtError> {
    let stages = crate::db::db_list_project_stages(project_id)?;
    let terminal_slugs: std::collections::HashSet<String> = stages
        .into_iter()
        .filter(|s| s.is_terminal)
        .map(|s| s.slug)
        .collect();
    let tasks = crate::db::db_list_project_tasks(project_id, None)?;
    let all_debt = crate::db::db_get_project_technical_debt(project_id, None)?;
    let all_todos = crate::db::db_get_project_todos(project_id, None)?;
    let (total_tokens_used_count, tokens_by_milestone) = crate::db::db_sum_milestone_tokens(project_id)?;
    let todo_completed_by_id: std::collections::HashMap<i64, bool> = all_todos
        .into_iter()
        .filter_map(|t| t.id.map(|id| (id, t.status == "completed")))
        .collect();

    let non_archived: Vec<&crate::db::ProjectMilestoneRecord> =
        milestones.iter().filter(|m| m.status != "archived").collect();
    let weight_sum: f64 = non_archived.iter().map(|m| m.weight_percent.max(0.0)).sum();
    let equal_fallback = if non_archived.is_empty() {
        0.0
    } else {
        100.0 / non_archived.len() as f64
    };

    let now = crate::db::now_epoch_secs();
    let mut summaries = Vec::with_capacity(milestones.len());
    let mut weighted_overall = 0.0_f64;
    let mut linear_total_points = 0_i64;
    let mut linear_done_points = 0_i64;
    let mut shape_up_sum = 0.0_f64;
    let mut active_count_for_shape = 0_usize;

    for m in milestones {
        let effective_weight_percent = if m.status == "archived" {
            0.0
        } else if weight_sum > 0.0 {
            (m.weight_percent.max(0.0) / weight_sum) * 100.0
        } else {
            equal_fallback
        };

        let mut tasks_total_count = 0_usize;
        let mut tasks_done_count = 0_usize;
        let mut total_weight_points = 0_i64;
        let mut done_weight_points = 0_i64;
        let mut max_active_task_due: Option<i64> = None;
        let mut linked_todo_ids = std::collections::HashSet::<i64>::new();
        let mut stage_counts = std::collections::HashMap::<String, usize>::new();
        let mut domain_counts = std::collections::HashMap::<String, usize>::new();

        for bundle in &tasks {
            if bundle.task.milestone_id != Some(m.id) {
                continue;
            }
            tasks_total_count += 1;
            let pts = i64::from(bundle.task.weight_points.max(1));
            total_weight_points += pts;
            let is_done = terminal_slugs.contains(&bundle.task.status);
            if is_done {
                tasks_done_count += 1;
                done_weight_points += pts;
            } else if let Some(due) = bundle.task.due_epoch_seconds {
                max_active_task_due = Some(max_active_task_due.map_or(due, |cur| cur.max(due)));
            }
            *stage_counts.entry(bundle.task.status.clone()).or_insert(0) += 1;
            *domain_counts.entry(bundle.task.domain.clone()).or_insert(0) += 1;
            for link in &bundle.links {
                if link.link_type == "todo" {
                    linked_todo_ids.insert(link.target_id);
                }
            }
            for sub in &bundle.subtasks {
                if let Some(tid) = sub.linked_todo_id {
                    linked_todo_ids.insert(tid);
                }
            }
        }

        let linked_todos_total_count = linked_todo_ids.len();
        let linked_todos_completed_count = linked_todo_ids
            .iter()
            .filter(|id| todo_completed_by_id.get(id).copied().unwrap_or(false))
            .count();

        let progress_percent = if m.status == "completed" {
            100.0
        } else if total_weight_points > 0 {
            (done_weight_points as f64 / total_weight_points as f64) * 100.0
        } else {
            0.0
        };

        let schedule_drift_days = if m.status == "completed" || m.status == "archived" {
            None
        } else {
            m.planned_due_epoch_seconds.map(|planned_due| {
                let projected = max_active_task_due.unwrap_or(now).max(now);
                (projected - planned_due) / 86_400
            })
        };

        let uphill_risk_flag = if m.status != "completed" && m.status != "archived" && m.hill_position_percent < 50 {
            match (m.planned_start_epoch_seconds, m.planned_due_epoch_seconds) {
                (Some(start), Some(due)) if due > start => now - start > (due - start) / 2,
                _ => false,
            }
        } else {
            false
        };

        let mut open_tech_debt_count = 0_usize;
        let mut critical_tech_debt_count = 0_usize;
        for d in &all_debt {
            if d.milestone_id == Some(m.id) && d.status != "resolved" {
                open_tech_debt_count += 1;
                if d.severity == "critical" || d.severity == "high" {
                    critical_tech_debt_count += 1;
                }
            }
        }

        if m.status != "archived" {
            weighted_overall += effective_weight_percent * (progress_percent / 100.0);
            linear_total_points += total_weight_points;
            linear_done_points += done_weight_points;
            shape_up_sum += f64::from(m.hill_position_percent.clamp(0, 100));
            active_count_for_shape += 1;
        }

        let milestone_id = m.id;
        summaries.push(ProjectMilestoneSummary {
            milestone: m,
            effective_weight_percent,
            tasks_total_count,
            tasks_done_count,
            total_weight_points,
            done_weight_points,
            progress_percent,
            schedule_drift_days,
            uphill_risk_flag,
            open_tech_debt_count,
            critical_tech_debt_count,
            linked_todos_total_count,
            linked_todos_completed_count,
            tokens_used_count: tokens_by_milestone.get(&milestone_id).copied().unwrap_or(0),
            stage_counts,
            domain_counts,
        });
    }

    let overall_progress_percent = match tracking_mode {
        "weighted" => weighted_overall.clamp(0.0, 100.0),
        "shape_up" => {
            if active_count_for_shape > 0 {
                (shape_up_sum / active_count_for_shape as f64).clamp(0.0, 100.0)
            } else {
                0.0
            }
        }
        "linear" => {
            if linear_total_points > 0 {
                (linear_done_points as f64 / linear_total_points as f64) * 100.0
            } else {
                0.0
            }
        }
        other => {
            return Err(ProjectMgmtError::InvalidField {
                field: "tracking_mode",
                value: other.to_string(),
                allowed: "linear | shape_up | weighted",
            });
        }
    };

    Ok((overall_progress_percent, total_tokens_used_count, summaries))
}

/// Lists all milestones for a project along with tracking-mode progress and Drift Compass signals.
pub async fn list_project_milestones(
    project_path: String,
) -> Result<ProjectMilestonesOverview, ProjectMgmtError> {
    tokio::task::spawn_blocking(move || {
        let project = crate::registry::find_project_by_path(&project_path)?;
        let tracking_mode = resolve_tracking_mode(Path::new(&project.path))?;
        let milestones = crate::db::db_list_project_milestones(project.id as i64)?;
        let (overall_progress_percent, total_tokens_used_count, summaries) =
            compute_milestone_summaries(project.id as i64, &tracking_mode, milestones)?;
        let usage_by_runner_model = crate::db::db_sum_project_usage_by_runner_model(project.id as i64)?;
        let total_cost_dollars: f64 = usage_by_runner_model.iter().map(|u| u.total_cost_dollars).sum();
        Ok(ProjectMilestonesOverview {
            tracking_mode,
            overall_progress_percent,
            total_tokens_used_count,
            total_cost_dollars,
            usage_by_runner_model,
            milestones: summaries,
        })
    })
    .await
    .map_err(|e| ProjectMgmtError::Task(e.to_string()))?
}

/// Fetches a single milestone with aggregated tasks, transitive journal minutes, linked TODOs, and tech debt.
pub async fn get_project_milestone_detail(
    args: GetProjectMilestoneDetailArgs,
) -> Result<ProjectMilestoneDetail, ProjectMgmtError> {
    tokio::task::spawn_blocking(move || {
        let project = crate::registry::find_project_by_path(&args.project_path)?;
        let project_id = project.id as i64;
        let tracking_mode = resolve_tracking_mode(Path::new(&project.path))?;
        let milestones = crate::db::db_list_project_milestones(project_id)?;
        let (_, _, summaries) = compute_milestone_summaries(project_id, &tracking_mode, milestones)?;
        let summary = summaries
            .into_iter()
            .find(|s| s.milestone.id == args.milestone_id)
            .ok_or(ProjectMgmtError::MilestoneNotFound(args.milestone_id))?;

        let tasks: Vec<crate::db::ProjectTaskBundle> = crate::db::db_list_project_tasks(project_id, None)?
            .into_iter()
            .filter(|b| b.task.milestone_id == Some(args.milestone_id))
            .collect();
        let (journal_entries, todos, tech_debt) =
            crate::db::db_get_milestone_linked_records(project_id, args.milestone_id)?;

        Ok(ProjectMilestoneDetail {
            summary,
            tasks,
            journal_entries,
            todos,
            tech_debt,
        })
    })
    .await
    .map_err(|e| ProjectMgmtError::Task(e.to_string()))?
}

/// Creates a new strategic milestone for a project.
pub async fn create_project_milestone(
    args: CreateProjectMilestoneArgs,
) -> Result<crate::db::ProjectMilestoneRecord, ProjectMgmtError> {
    let title = args.title.trim().to_string();
    if title.is_empty() {
        return Err(ProjectMgmtError::EmptyTitle);
    }
    let status = args.status.unwrap_or_else(|| "planned".to_string());
    validate_milestone_status(&status)?;

    tokio::task::spawn_blocking(move || {
        let project = crate::registry::find_project_by_path(&args.project_path)?;
        let project_id = project.id as i64;
        let desc = args.description.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let id = crate::db::db_insert_project_milestone(
            project_id,
            &title,
            desc,
            &status,
            args.weight_percent.unwrap_or(0.0),
            args.hill_position_percent.unwrap_or(0),
            args.appetite_days,
            args.planned_start_epoch_seconds,
            args.planned_due_epoch_seconds,
            None,
        )?;
        let created = crate::db::db_get_project_milestone(id)?
            .ok_or(ProjectMgmtError::MilestoneNotFound(id))?;
        emit_project_milestones_updated_by_id(project_id);
        Ok(created)
    })
    .await
    .map_err(|e| ProjectMgmtError::Task(e.to_string()))?
}

/// Updates an existing strategic milestone after verifying ownership by `args.project_path`.
pub async fn update_project_milestone(
    args: UpdateProjectMilestoneArgs,
) -> Result<(), ProjectMgmtError> {
    let title = args.title.trim().to_string();
    if title.is_empty() {
        return Err(ProjectMgmtError::EmptyTitle);
    }
    validate_milestone_status(&args.status)?;

    tokio::task::spawn_blocking(move || {
        let project = crate::registry::find_project_by_path(&args.project_path)?;
        let project_id = project.id as i64;
        let desc = args.description.as_deref().map(str::trim).filter(|s| !s.is_empty());
        match crate::db::db_update_project_milestone(
            args.milestone_id,
            project_id,
            &title,
            desc,
            &args.status,
            args.weight_percent,
            args.hill_position_percent,
            args.appetite_days,
            args.planned_start_epoch_seconds,
            args.planned_due_epoch_seconds,
            args.sort_order,
        )? {
            crate::db::MilestoneScopedOutcome::Applied => {
                emit_project_milestones_updated_by_id(project_id);
                Ok(())
            }
            crate::db::MilestoneScopedOutcome::NotFound => {
                Err(ProjectMgmtError::MilestoneNotFound(args.milestone_id))
            }
            crate::db::MilestoneScopedOutcome::ProjectMismatch { .. } => {
                Err(ProjectMgmtError::MilestoneProjectMismatch {
                    milestone_id: args.milestone_id,
                    project_path: args.project_path,
                })
            }
        }
    })
    .await
    .map_err(|e| ProjectMgmtError::Task(e.to_string()))?
}

/// Deletes a strategic milestone after verifying ownership by `args.project_path`
/// (linked tasks/journal/tech debt keep their rows with `milestone_id = NULL`).
pub async fn delete_project_milestone(args: DeleteProjectMilestoneArgs) -> Result<(), ProjectMgmtError> {
    tokio::task::spawn_blocking(move || {
        let project = crate::registry::find_project_by_path(&args.project_path)?;
        let project_id = project.id as i64;
        match crate::db::db_delete_project_milestone(args.milestone_id, project_id)? {
            crate::db::MilestoneScopedOutcome::Applied => {
                emit_project_milestones_updated_by_id(project_id);
                emit_project_tasks_updated_by_id(project_id);
                Ok(())
            }
            crate::db::MilestoneScopedOutcome::NotFound => {
                Err(ProjectMgmtError::MilestoneNotFound(args.milestone_id))
            }
            crate::db::MilestoneScopedOutcome::ProjectMismatch { .. } => {
                Err(ProjectMgmtError::MilestoneProjectMismatch {
                    milestone_id: args.milestone_id,
                    project_path: args.project_path,
                })
            }
        }
    })
    .await
    .map_err(|e| ProjectMgmtError::Task(e.to_string()))?
}
