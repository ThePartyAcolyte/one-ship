//! Project Management Capability Provider (`domain = "project"`, Spec 34 & Spec 40).

use std::borrow::Cow;
use serde_json::{json, Value};

use crate::events::DaemonEvent;
use crate::kopeng::project_mgmt::{
    create_project_milestone, create_project_stage, create_project_task, create_task_subtask,
    delete_project_milestone, delete_project_stage, delete_project_task, delete_task_subtask,
    get_project_milestone_detail, list_project_milestones, list_project_stages, list_project_tasks,
    reorder_project_stages, toggle_task_subtask, update_project_milestone, update_project_stage,
    update_project_task, CreateProjectMilestoneArgs, CreateProjectStageArgs, CreateProjectTaskArgs,
    CreateTaskSubtaskArgs, DeleteProjectMilestoneArgs, DeleteProjectStageArgs,
    GetProjectMilestoneDetailArgs, ReorderProjectStagesArgs, UpdateProjectMilestoneArgs,
    UpdateProjectStageArgs, UpdateProjectTaskArgs,
};

use super::{
    paginate_items, require_non_empty_str, require_positive_i64, run_blocking, BoxFuture,
    CapabilityContext, CapabilityDef, CapabilityError, CapabilityProvider, CapabilityRisk,
    CapabilityTarget, CapabilityVisibility,
};

pub static PROVIDER: CapabilityProvider = CapabilityProvider {
    domain_id: "project",
    label: "Project Management",
    required_capability: Some("has_project_management_enabled"),
    linked_surface_path: Some("project"),
    static_actions,
    dynamic_actions: |_| Vec::new(),
    execute,
};

fn cap(
    id: &'static str,
    mcp_name: Option<&'static str>,
    description: &'static str,
    parameters_schema: Value,
    risk: CapabilityRisk,
) -> CapabilityDef {
    CapabilityDef {
        id: Cow::Borrowed(id),
        mcp_name: mcp_name.map(Cow::Borrowed),
        description: Cow::Borrowed(description),
        parameters_schema,
        visibility: CapabilityVisibility::Shared,
        risk,
        target: CapabilityTarget::Backend,
        voice_max_items: None,
    }
}

fn static_actions() -> Vec<CapabilityDef> {
    vec![
        cap(
            "get_project_summary",
            None,
            "Returns active, overdue, and per-stage task counts for the project.",
            json!({ "type": "OBJECT" }),
            CapabilityRisk::ReadOnly,
        ),
        cap(
            "list_stages",
            Some("manage_project_stage"),
            "Lists all Kanban stages (columns) for the project.",
            json!({ "type": "OBJECT" }),
            CapabilityRisk::ReadOnly,
        ),
        cap(
            "create_stage",
            Some("manage_project_stage"),
            "Creates a new Kanban stage.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "label": { "type": "STRING", "description": "Visible column name (slug is derived from it)" },
                    "is_terminal": { "type": "BOOLEAN", "description": "True if tasks in this stage count as completed" }
                },
                "required": ["label"]
            }),
            CapabilityRisk::Mutating,
        ),
        cap(
            "update_stage",
            Some("manage_project_stage"),
            "Updates a Kanban stage label, collapsed state, or terminal flag.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "stage_id": { "type": "INTEGER", "description": "Numeric stage ID from list_stages" },
                    "label": { "type": "STRING", "description": "New visible column label" },
                    "is_collapsed": { "type": "BOOLEAN", "description": "Collapse column horizontally on Kanban board" },
                    "is_terminal": { "type": "BOOLEAN", "description": "Mark stage as completed/done" }
                },
                "required": ["stage_id"]
            }),
            CapabilityRisk::Mutating,
        ),
        cap(
            "reorder_stages",
            Some("manage_project_stage"),
            "Reorders Kanban stages by an array of stage IDs.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "ordered_stage_ids": { "type": "ARRAY", "items": { "type": "INTEGER" }, "description": "Full left-to-right list of stage IDs" }
                },
                "required": ["ordered_stage_ids"]
            }),
            CapabilityRisk::Mutating,
        ),
        cap(
            "delete_stage",
            Some("manage_project_stage"),
            "Deletes a Kanban stage (requires explicit confirmation). Fails if tasks remain unless reassign_to_stage_id is supplied.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "stage_id": { "type": "INTEGER", "description": "Stage ID to delete" },
                    "reassign_to_stage_id": { "type": "INTEGER", "description": "Destination stage ID for existing tasks before deletion" }
                },
                "required": ["stage_id"]
            }),
            CapabilityRisk::ConfirmRequired,
        ),
        cap(
            "list_tasks",
            Some("query_project_tasks"),
            "Lists project tasks and subtasks, optionally filtered by status_filter, domain_filter, milestone_id, or due epoch bounds.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "status_filter": { "type": "STRING", "description": "Stage slug from list_stages (e.g. backlog, in_progress)" },
                    "domain_filter": { "type": "STRING", "description": "Deliverable domain: code|hardware|procurement|design|docs|client|milestone|ops" },
                    "milestone_id": { "type": "INTEGER", "description": "Filter tasks assigned to this milestone ID" },
                    "due_after_epoch_seconds": { "type": "INTEGER", "description": "Unix epoch seconds lower bound for due date" },
                    "due_before_epoch_seconds": { "type": "INTEGER", "description": "Unix epoch seconds upper bound for due date" },
                    "offset": { "type": "INTEGER", "description": "Pagination start index (default 0)" },
                    "limit": { "type": "INTEGER", "description": "Optional page size (omit for all)" }
                }
            }),
            CapabilityRisk::ReadOnly,
        ),
        cap(
            "create_task",
            Some("manage_project_task"),
            "Creates a project task with optional description, domain, status (stage slug), priority (1-4), milestone_id, weight_points, dates, and branch_name.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "title": { "type": "STRING", "description": "Short deliverable title" },
                    "description": { "type": "STRING", "description": "Detailed task scope or acceptance notes" },
                    "domain": { "type": "STRING", "description": "code|hardware|procurement|design|docs|client|milestone|ops" },
                    "status": { "type": "STRING", "description": "Stage slug from list_stages" },
                    "priority": { "type": "INTEGER", "description": "1=urgent, 2=high, 3=normal, 4=low" },
                    "milestone_id": { "type": "INTEGER", "description": "Parent strategic milestone ID" },
                    "weight_points": { "type": "INTEGER", "description": "Relative effort weight for weighted milestones" },
                    "start_epoch_seconds": { "type": "INTEGER", "description": "Planned start timestamp (Unix epoch seconds)" },
                    "due_epoch_seconds": { "type": "INTEGER", "description": "Target due timestamp (Unix epoch seconds)" },
                    "branch_name": { "type": "STRING", "description": "Associated Git branch name" }
                },
                "required": ["title"]
            }),
            CapabilityRisk::Mutating,
        ),
        cap(
            "update_task",
            Some("manage_project_task"),
            "Partially updates a project task (title, description, domain, status/stage, priority, milestone_id, weight_points, dates, branch_name).",
            json!({
                "type": "OBJECT",
                "properties": {
                    "task_id": { "type": "INTEGER", "description": "Numeric task ID" },
                    "title": { "type": "STRING", "description": "Updated deliverable title" },
                    "description": { "type": "STRING", "description": "Updated task scope" },
                    "domain": { "type": "STRING", "description": "code|hardware|procurement|design|docs|client|milestone|ops" },
                    "status": { "type": "STRING", "description": "Target Kanban stage slug from list_stages" },
                    "priority": { "type": "INTEGER", "description": "1=urgent, 2=high, 3=normal, 4=low" },
                    "milestone_id": { "type": "INTEGER", "description": "New parent milestone ID" },
                    "weight_points": { "type": "INTEGER", "description": "Effort weight points" },
                    "start_epoch_seconds": { "type": "INTEGER", "description": "Planned start (Unix epoch seconds)" },
                    "due_epoch_seconds": { "type": "INTEGER", "description": "Target due date (Unix epoch seconds)" },
                    "branch_name": { "type": "STRING", "description": "Associated Git branch" }
                },
                "required": ["task_id"]
            }),
            CapabilityRisk::Mutating,
        ),
        cap(
            "delete_task",
            Some("manage_project_task"),
            "Deletes a project task by task_id (requires explicit confirmation).",
            json!({
                "type": "OBJECT",
                "properties": { "task_id": { "type": "INTEGER", "description": "Numeric task ID to delete" } },
                "required": ["task_id"]
            }),
            CapabilityRisk::ConfirmRequired,
        ),
        cap(
            "create_subtask",
            Some("manage_task_subtask"),
            "Creates a checklist subtask under task_id.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "task_id": { "type": "INTEGER", "description": "Parent task ID" },
                    "title": { "type": "STRING", "description": "Checklist step text" },
                    "linked_todo_id": { "type": "INTEGER", "description": "Optional journal TODO ID to associate" }
                },
                "required": ["task_id", "title"]
            }),
            CapabilityRisk::Mutating,
        ),
        cap(
            "toggle_subtask",
            Some("manage_task_subtask"),
            "Marks a checklist subtask completed or incomplete.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "subtask_id": { "type": "INTEGER", "description": "Checklist subtask ID" },
                    "is_completed": { "type": "BOOLEAN", "description": "True when step is done" }
                },
                "required": ["subtask_id", "is_completed"]
            }),
            CapabilityRisk::Mutating,
        ),
        cap(
            "delete_subtask",
            Some("manage_task_subtask"),
            "Deletes a checklist subtask by subtask_id (requires explicit confirmation).",
            json!({
                "type": "OBJECT",
                "properties": { "subtask_id": { "type": "INTEGER", "description": "Subtask ID to delete" } },
                "required": ["subtask_id"]
            }),
            CapabilityRisk::ConfirmRequired,
        ),
        cap(
            "list_milestones",
            Some("manage_project_milestone"),
            "Lists project milestones and overall progress summary.",
            json!({ "type": "OBJECT" }),
            CapabilityRisk::ReadOnly,
        ),
        cap(
            "get_milestone_detail",
            Some("manage_project_milestone"),
            "Returns full milestone details, linked tasks, and metrics for milestone_id.",
            json!({
                "type": "OBJECT",
                "properties": { "milestone_id": { "type": "INTEGER", "description": "Numeric milestone ID" } },
                "required": ["milestone_id"]
            }),
            CapabilityRisk::ReadOnly,
        ),
        cap(
            "create_milestone",
            Some("manage_project_milestone"),
            "Creates a project milestone.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "title": { "type": "STRING", "description": "Milestone name" },
                    "description": { "type": "STRING", "description": "Goal or delivery criteria" },
                    "status": { "type": "STRING", "description": "planned|active|completed|archived" },
                    "weight_percent": { "type": "NUMBER", "description": "Share of project progress (0-100) in weighted mode" },
                    "hill_position_percent": { "type": "INTEGER", "description": "Shape Up hill (0-50 uphill figuring out, 50-100 downhill execution)" },
                    "appetite_days": { "type": "INTEGER", "description": "Shape Up time budget in days" },
                    "planned_start_epoch_seconds": { "type": "INTEGER", "description": "Planned start (Unix epoch seconds)" },
                    "planned_due_epoch_seconds": { "type": "INTEGER", "description": "Target deadline (Unix epoch seconds)" }
                },
                "required": ["title"]
            }),
            CapabilityRisk::Mutating,
        ),
        cap(
            "update_milestone",
            Some("manage_project_milestone"),
            "Updates an existing project milestone.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "milestone_id": { "type": "INTEGER", "description": "Numeric milestone ID" },
                    "title": { "type": "STRING", "description": "Updated milestone name" },
                    "description": { "type": "STRING", "description": "Updated goal criteria" },
                    "status": { "type": "STRING", "description": "planned|active|completed|archived" },
                    "weight_percent": { "type": "NUMBER", "description": "Progress share (0-100) in weighted mode" },
                    "hill_position_percent": { "type": "INTEGER", "description": "Shape Up hill (0-50 exploration, 50-100 execution)" },
                    "appetite_days": { "type": "INTEGER", "description": "Shape Up time budget in days" },
                    "planned_start_epoch_seconds": { "type": "INTEGER", "description": "Planned start (Unix epoch seconds)" },
                    "planned_due_epoch_seconds": { "type": "INTEGER", "description": "Target deadline (Unix epoch seconds)" }
                },
                "required": ["milestone_id"]
            }),
            CapabilityRisk::Mutating,
        ),
        cap(
            "delete_milestone",
            Some("manage_project_milestone"),
            "Deletes a project milestone (requires explicit confirmation).",
            json!({
                "type": "OBJECT",
                "properties": { "milestone_id": { "type": "INTEGER", "description": "Numeric milestone ID to delete" } },
                "required": ["milestone_id"]
            }),
            CapabilityRisk::ConfirmRequired,
        ),
        cap(
            "link_task_relation",
            Some("link_journal_to_task"),
            "Links or unlinks a project task with a session journal or todo entry (action: 'link' | 'unlink', link_type: 'journal' | 'todo').",
            json!({
                "type": "OBJECT",
                "properties": {
                    "action": { "type": "STRING", "enum": ["link", "unlink"], "description": "Defaults to 'link'" },
                    "task_id": { "type": "INTEGER", "description": "Project Kanban task ID" },
                    "link_type": { "type": "STRING", "enum": ["journal", "todo"], "description": "Kind of journal item to attach" },
                    "target_id": { "type": "INTEGER", "description": "ID of the session journal or TODO item" }
                },
                "required": ["task_id", "link_type", "target_id"]
            }),
            CapabilityRisk::Mutating,
        ),
    ]
}

async fn verify_stage_owned(
    stage_id: i64,
    project_id: i64,
) -> Result<crate::db::ProjectTaskStageRecord, CapabilityError> {
    let stage = run_blocking(move || Ok(crate::db::db_get_project_stage(stage_id)?))
        .await?
        .ok_or(CapabilityError::NotFound {
            entity: "Stage",
            id: stage_id,
        })?;
    if stage.project_id != project_id {
        return Err(CapabilityError::OwnershipMismatch {
            entity: "Stage",
            entity_id: stage_id,
            project_id,
        });
    }
    Ok(stage)
}

async fn verify_task_owned(
    task_id: i64,
    project_id: i64,
) -> Result<crate::db::ProjectTaskRecord, CapabilityError> {
    let task = run_blocking(move || Ok(crate::db::db_get_project_task(task_id)?))
        .await?
        .ok_or(CapabilityError::NotFound {
            entity: "Task",
            id: task_id,
        })?;
    if task.project_id != project_id {
        return Err(CapabilityError::OwnershipMismatch {
            entity: "Task",
            entity_id: task_id,
            project_id,
        });
    }
    Ok(task)
}

async fn verify_subtask_owned(subtask_id: i64, project_id: i64) -> Result<(), CapabilityError> {
    let owner_pid = run_blocking(move || Ok(crate::db::db_get_task_subtask_project_id(subtask_id)?))
        .await?
        .ok_or(CapabilityError::NotFound {
            entity: "Subtask",
            id: subtask_id,
        })?;
    if owner_pid != project_id {
        return Err(CapabilityError::OwnershipMismatch {
            entity: "Subtask",
            entity_id: subtask_id,
            project_id,
        });
    }
    Ok(())
}

async fn verify_milestone_owned(
    milestone_id: i64,
    project_id: i64,
) -> Result<crate::db::ProjectMilestoneRecord, CapabilityError> {
    let milestone = run_blocking(move || Ok(crate::db::db_get_project_milestone(milestone_id)?))
        .await?
        .ok_or(CapabilityError::NotFound {
            entity: "Milestone",
            id: milestone_id,
        })?;
    if milestone.project_id != project_id {
        return Err(CapabilityError::OwnershipMismatch {
            entity: "Milestone",
            entity_id: milestone_id,
            project_id,
        });
    }
    Ok(milestone)
}

fn execute<'a>(
    ctx: CapabilityContext,
    action_id: &'a str,
    params: Value,
) -> BoxFuture<'a, Result<Value, CapabilityError>> {
    Box::pin(async move {
        let project_id = ctx.project_id();
        let project_path = ctx.project.path.clone();

        match action_id {
            "get_project_summary" => {
                let stages = list_project_stages(project_path.clone()).await?;
                let tasks = list_project_tasks(project_path, None).await?;
                let now_epoch = chrono::Utc::now().timestamp();
                let mut active_count = 0usize;
                let mut overdue_count = 0usize;
                let mut stage_counts = serde_json::Map::new();
                let mut terminal_slugs = std::collections::HashSet::with_capacity(stages.len());
                for st in &stages {
                    stage_counts.insert(st.slug.clone(), json!(0));
                    if st.is_terminal {
                        terminal_slugs.insert(st.slug.as_str());
                    }
                }
                for bundle in &tasks {
                    let st = bundle.task.status.as_str();
                    let count = stage_counts.get(st).and_then(|v| v.as_u64()).unwrap_or(0) + 1;
                    stage_counts.insert(st.to_string(), json!(count));
                    if !terminal_slugs.contains(st) {
                        active_count += 1;
                        if bundle.task.due_epoch_seconds.is_some_and(|d| d < now_epoch) {
                            overdue_count += 1;
                        }
                    }
                }
                Ok(json!({
                    "total_tasks": tasks.len(),
                    "active_tasks": active_count,
                    "overdue_tasks": overdue_count,
                    "stages": stages,
                    "counts_by_stage": stage_counts
                }))
            }
            "list_stages" => {
                let stages = list_project_stages(project_path).await?;
                Ok(json!({ "stages": stages }))
            }
            "create_stage" => {
                let label = require_non_empty_str(&params, "label")?;
                let is_terminal = params.get("is_terminal").and_then(|v| v.as_bool());
                let stage = create_project_stage(CreateProjectStageArgs {
                    project_path,
                    label,
                    is_terminal,
                })
                .await?;
                Ok(json!({ "ok": true, "stage": stage }))
            }
            "update_stage" => {
                let stage_id = require_positive_i64(&params, "stage_id")?;
                let existing = verify_stage_owned(stage_id, project_id).await?;
                let label = params
                    .get("label")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .unwrap_or(existing.label);
                let is_collapsed = params
                    .get("is_collapsed")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(existing.is_collapsed);
                let is_terminal = params
                    .get("is_terminal")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(existing.is_terminal);
                update_project_stage(UpdateProjectStageArgs {
                    stage_id,
                    label: label.clone(),
                    is_collapsed,
                    is_terminal,
                })
                .await?;
                Ok(json!({ "ok": true, "stage_id": stage_id, "label": label }))
            }
            "reorder_stages" => {
                let arr = params
                    .get("ordered_stage_ids")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| {
                        CapabilityError::InvalidParams(
                            "'ordered_stage_ids' array is required".to_string(),
                        )
                    })?;
                let mut ids = Vec::with_capacity(arr.len());
                for v in arr {
                    let Some(id) = v.as_i64().filter(|id| *id > 0) else {
                        return Err(CapabilityError::InvalidParams(
                            "Every entry in 'ordered_stage_ids' must be a positive integer"
                                .to_string(),
                        ));
                    };
                    ids.push(id);
                }
                reorder_project_stages(ReorderProjectStagesArgs {
                    project_path,
                    ordered_stage_ids: ids,
                })
                .await?;
                Ok(json!({ "ok": true }))
            }
            "delete_stage" => {
                let stage_id = require_positive_i64(&params, "stage_id")?;
                verify_stage_owned(stage_id, project_id).await?;
                let reassign_to_stage_id = match params.get("reassign_to_stage_id") {
                    None | Some(Value::Null) => None,
                    Some(v) => {
                        let target_id = v.as_i64().filter(|id| *id > 0).ok_or_else(|| {
                            CapabilityError::InvalidParams(
                                "'reassign_to_stage_id' must be a positive integer".to_string(),
                            )
                        })?;
                        verify_stage_owned(target_id, project_id).await?;
                        Some(target_id)
                    }
                };
                delete_project_stage(DeleteProjectStageArgs {
                    stage_id,
                    reassign_to_stage_id,
                })
                .await?;
                Ok(json!({ "ok": true, "stage_id": stage_id }))
            }
            "list_tasks" => {
                let status_filter = params
                    .get("status_filter")
                    .or_else(|| params.get("status"))
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                if let Some(ref slug) = status_filter {
                    let slug_owned = slug.clone();
                    let exists = run_blocking(move || {
                        Ok(crate::db::db_project_stage_exists(project_id, &slug_owned)?)
                    })
                    .await?;
                    if !exists {
                        return Err(CapabilityError::InvalidParams(format!(
                            "Invalid stage '{slug}'"
                        )));
                    }
                }
                let domain_filter = params
                    .get("domain_filter")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                let milestone_id = params.get("milestone_id").and_then(|v| v.as_i64());
                let due_after = params
                    .get("due_after_epoch_seconds")
                    .and_then(|v| v.as_i64());
                let due_before = params
                    .get("due_before_epoch_seconds")
                    .and_then(|v| v.as_i64());

                let mut tasks = list_project_tasks(project_path, status_filter).await?;
                if let Some(ref dom) = domain_filter {
                    tasks.retain(|b| b.task.domain == *dom);
                }
                if let Some(mid) = milestone_id {
                    tasks.retain(|b| b.task.milestone_id == Some(mid));
                }
                if let Some(after) = due_after {
                    tasks.retain(|b| b.task.due_epoch_seconds.is_some_and(|d| d >= after));
                }
                if let Some(before) = due_before {
                    tasks.retain(|b| b.task.due_epoch_seconds.is_some_and(|d| d <= before));
                }
                paginate_items(tasks, "tasks", &params)
            }
            "create_task" => {
                let title = require_non_empty_str(&params, "title")?;
                let milestone_id = match params.get("milestone_id") {
                    None | Some(Value::Null) => None,
                    Some(v) => {
                        let mid = v.as_i64().filter(|id| *id > 0).ok_or_else(|| {
                            CapabilityError::InvalidParams(
                                "'milestone_id' must be a positive integer".to_string(),
                            )
                        })?;
                        verify_milestone_owned(mid, project_id).await?;
                        Some(mid)
                    }
                };
                let args = CreateProjectTaskArgs {
                    project_path,
                    title,
                    description: params
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    domain: params
                        .get("domain")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    status: params
                        .get("status")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    priority: params
                        .get("priority")
                        .and_then(|v| v.as_i64())
                        .map(|p| p as i32),
                    milestone_id,
                    weight_points: params
                        .get("weight_points")
                        .and_then(|v| v.as_i64())
                        .map(|w| w as i32),
                    start_epoch_seconds: params
                        .get("start_epoch_seconds")
                        .and_then(|v| v.as_i64()),
                    due_epoch_seconds: params.get("due_epoch_seconds").and_then(|v| v.as_i64()),
                    github_repo: params
                        .get("github_repo")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    github_issue_number: params
                        .get("github_issue_number")
                        .and_then(|v| v.as_i64()),
                    github_pr_number: params.get("github_pr_number").and_then(|v| v.as_i64()),
                    branch_name: params
                        .get("branch_name")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                };
                let task_id = create_project_task(args).await?;
                Ok(json!({ "ok": true, "task_id": task_id }))
            }
            "update_task" => {
                let task_id = require_positive_i64(&params, "task_id")?;
                let existing = verify_task_owned(task_id, project_id).await?;

                let title = params
                    .get("title")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .unwrap_or(existing.title);
                let description = match params.get("description") {
                    Some(Value::Null) => None,
                    Some(v) => v.as_str().map(str::to_string).or(existing.description),
                    None => existing.description,
                };
                let domain = params
                    .get("domain")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .unwrap_or(existing.domain);
                let status = params
                    .get("status")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .unwrap_or(existing.status);
                let priority = params
                    .get("priority")
                    .and_then(|v| v.as_i64())
                    .map(|p| p as i32)
                    .unwrap_or(existing.priority);
                let milestone_id = match params.get("milestone_id") {
                    Some(Value::Null) => None,
                    Some(v) => {
                        let mid = v.as_i64().filter(|id| *id > 0).ok_or_else(|| {
                            CapabilityError::InvalidParams(
                                "'milestone_id' must be a positive integer".to_string(),
                            )
                        })?;
                        verify_milestone_owned(mid, project_id).await?;
                        Some(mid)
                    }
                    None => existing.milestone_id,
                };
                let weight_points = params
                    .get("weight_points")
                    .and_then(|v| v.as_i64())
                    .map(|w| w as i32)
                    .unwrap_or(existing.weight_points);
                let start_epoch_seconds = match params.get("start_epoch_seconds") {
                    Some(Value::Null) => None,
                    Some(v) => v.as_i64().or(existing.start_epoch_seconds),
                    None => existing.start_epoch_seconds,
                };
                let due_epoch_seconds = match params.get("due_epoch_seconds") {
                    Some(Value::Null) => None,
                    Some(v) => v.as_i64().or(existing.due_epoch_seconds),
                    None => existing.due_epoch_seconds,
                };
                let branch_name = match params.get("branch_name") {
                    Some(Value::Null) => None,
                    Some(v) => v.as_str().map(str::to_string).or(existing.branch_name),
                    None => existing.branch_name,
                };

                update_project_task(UpdateProjectTaskArgs {
                    task_id,
                    title: title.clone(),
                    description,
                    domain,
                    status: status.clone(),
                    priority,
                    milestone_id,
                    weight_points: Some(weight_points),
                    start_epoch_seconds,
                    due_epoch_seconds,
                    branch_name,
                })
                .await?;
                Ok(json!({ "ok": true, "task_id": task_id, "title": title, "status": status }))
            }
            "delete_task" => {
                let task_id = require_positive_i64(&params, "task_id")?;
                verify_task_owned(task_id, project_id).await?;
                delete_project_task(task_id).await?;
                Ok(json!({ "ok": true, "task_id": task_id }))
            }
            "create_subtask" => {
                let task_id = require_positive_i64(&params, "task_id")?;
                verify_task_owned(task_id, project_id).await?;
                let title = require_non_empty_str(&params, "title")?;
                let linked_todo_id = params.get("linked_todo_id").and_then(|v| v.as_i64());
                let subtask_id = create_task_subtask(CreateTaskSubtaskArgs {
                    task_id,
                    title,
                    linked_todo_id,
                })
                .await?;
                Ok(json!({ "ok": true, "subtask_id": subtask_id, "task_id": task_id }))
            }
            "toggle_subtask" => {
                let subtask_id = require_positive_i64(&params, "subtask_id")?;
                verify_subtask_owned(subtask_id, project_id).await?;
                let is_completed = params
                    .get("is_completed")
                    .and_then(|v| v.as_bool())
                    .ok_or_else(|| {
                        CapabilityError::InvalidParams(
                            "Missing boolean parameter 'is_completed'".to_string(),
                        )
                    })?;
                toggle_task_subtask(subtask_id, is_completed).await?;
                Ok(json!({ "ok": true, "subtask_id": subtask_id, "is_completed": is_completed }))
            }
            "delete_subtask" => {
                let subtask_id = require_positive_i64(&params, "subtask_id")?;
                verify_subtask_owned(subtask_id, project_id).await?;
                delete_task_subtask(subtask_id).await?;
                Ok(json!({ "ok": true, "subtask_id": subtask_id }))
            }
            "list_milestones" => {
                let overview = list_project_milestones(project_path).await?;
                Ok(serde_json::to_value(&overview)
                    .map_err(|e| CapabilityError::Task(e.to_string()))?)
            }
            "get_milestone_detail" => {
                let milestone_id = require_positive_i64(&params, "milestone_id")?;
                verify_milestone_owned(milestone_id, project_id).await?;
                let detail = get_project_milestone_detail(GetProjectMilestoneDetailArgs {
                    project_path,
                    milestone_id,
                })
                .await?;
                Ok(serde_json::to_value(&detail)
                    .map_err(|e| CapabilityError::Task(e.to_string()))?)
            }
            "create_milestone" => {
                let title = require_non_empty_str(&params, "title")?;
                let milestone = create_project_milestone(CreateProjectMilestoneArgs {
                    project_path,
                    title,
                    description: params
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    status: params
                        .get("status")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    weight_percent: params.get("weight_percent").and_then(|v| v.as_f64()),
                    hill_position_percent: params
                        .get("hill_position_percent")
                        .and_then(|v| v.as_i64())
                        .map(|n| n as i32),
                    appetite_days: params
                        .get("appetite_days")
                        .and_then(|v| v.as_i64())
                        .map(|n| n as i32),
                    planned_start_epoch_seconds: params
                        .get("planned_start_epoch_seconds")
                        .and_then(|v| v.as_i64()),
                    planned_due_epoch_seconds: params
                        .get("planned_due_epoch_seconds")
                        .and_then(|v| v.as_i64()),
                })
                .await?;
                Ok(json!({ "ok": true, "milestone": milestone }))
            }
            "update_milestone" => {
                let milestone_id = require_positive_i64(&params, "milestone_id")?;
                let existing = verify_milestone_owned(milestone_id, project_id).await?;
                let title = params
                    .get("title")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .unwrap_or(existing.title);
                let description = match params.get("description") {
                    Some(Value::Null) => None,
                    Some(v) => v.as_str().map(str::to_string).or(existing.description),
                    None => existing.description,
                };
                let status = params
                    .get("status")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .unwrap_or(existing.status);
                let weight_percent = params
                    .get("weight_percent")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(existing.weight_percent);
                let hill_position_percent = params
                    .get("hill_position_percent")
                    .and_then(|v| v.as_i64())
                    .map(|n| n as i32)
                    .unwrap_or(existing.hill_position_percent);
                let appetite_days = match params.get("appetite_days") {
                    Some(Value::Null) => None,
                    Some(v) => v.as_i64().map(|n| n as i32).or(existing.appetite_days),
                    None => existing.appetite_days,
                };
                let planned_start_epoch_seconds = match params.get("planned_start_epoch_seconds") {
                    Some(Value::Null) => None,
                    Some(v) => v.as_i64().or(existing.planned_start_epoch_seconds),
                    None => existing.planned_start_epoch_seconds,
                };
                let planned_due_epoch_seconds = match params.get("planned_due_epoch_seconds") {
                    Some(Value::Null) => None,
                    Some(v) => v.as_i64().or(existing.planned_due_epoch_seconds),
                    None => existing.planned_due_epoch_seconds,
                };
                update_project_milestone(UpdateProjectMilestoneArgs {
                    project_path,
                    milestone_id,
                    title: title.clone(),
                    description,
                    status: status.clone(),
                    weight_percent,
                    hill_position_percent,
                    appetite_days,
                    planned_start_epoch_seconds,
                    planned_due_epoch_seconds,
                    sort_order: existing.sort_order,
                })
                .await?;
                Ok(json!({ "ok": true, "milestone_id": milestone_id, "title": title, "status": status }))
            }
            "delete_milestone" => {
                let milestone_id = require_positive_i64(&params, "milestone_id")?;
                verify_milestone_owned(milestone_id, project_id).await?;
                delete_project_milestone(DeleteProjectMilestoneArgs {
                    project_path,
                    milestone_id,
                })
                .await?;
                Ok(json!({ "ok": true, "milestone_id": milestone_id }))
            }
            "link_task_relation" => {
                let action = params
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("link");
                let task_id = require_positive_i64(&params, "task_id")?;
                let target_id = require_positive_i64(&params, "target_id")?;
                let link_type = params
                    .get("link_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("journal")
                    .to_string();
                verify_task_owned(task_id, project_id).await?;
                let verb = match action {
                    "link" => {
                        let lt = link_type.clone();
                        run_blocking(move || {
                            crate::db::db_link_task_relation(task_id, &lt, target_id)?;
                            Ok("Linked")
                        })
                        .await?
                    }
                    "unlink" => {
                        let lt = link_type.clone();
                        run_blocking(move || {
                            crate::db::db_unlink_task_relation(task_id, &lt, target_id)?;
                            Ok("Unlinked")
                        })
                        .await?
                    }
                    other => {
                        return Err(CapabilityError::InvalidParams(format!(
                            "Unknown relation action '{other}': expected 'link' or 'unlink'"
                        )));
                    }
                };
                crate::supervisor::get_supervisor().emit_event(
                    DaemonEvent::project_tasks_updated(&crate::events::ProjectTasksUpdatedPayload {
                        project_path,
                        project_id,
                    }),
                );
                Ok(json!({
                    "ok": true,
                    "verb": verb,
                    "task_id": task_id,
                    "link_type": link_type,
                    "target_id": target_id
                }))
            }
            other => Err(CapabilityError::UnknownAction {
                domain: "project".to_string(),
                action_id: other.to_string(),
            }),
        }
    })
}
