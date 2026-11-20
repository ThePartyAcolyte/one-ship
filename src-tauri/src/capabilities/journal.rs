//! Journal, TODOs & Technical Debt Capability Provider (`domain = "journal"`, Spec 40).
//!
//! Exposes session journal inspection, TODO tracking, technical debt inspection/resolution,
//! and project-wide conversation search to both MCP agents and the Global Voice Controller
//! without requiring UI navigation to the `devlog` tab. Formal implementation minutes
//! (`record_session_minutes`) remain `McpOnly` in `mcp.rs`.

use std::borrow::Cow;
use serde_json::{json, Value};

use crate::events::{DaemonEvent, TechDebtUpdatedPayload, TodoUpdatedPayload};

use super::{
    paginate_items, parse_pagination, require_non_empty_str, require_positive_i64, run_blocking,
    BoxFuture, CapabilityCaller, CapabilityContext, CapabilityDef, CapabilityError,
    CapabilityProvider, CapabilityRisk, CapabilityTarget, CapabilityVisibility,
};

pub static PROVIDER: CapabilityProvider = CapabilityProvider {
    domain_id: "journal",
    label: "Journal, TODOs & Tech Debt",
    required_capability: None,
    linked_surface_path: Some("devlog"),
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
    voice_max_items: Option<usize>,
) -> CapabilityDef {
    CapabilityDef {
        id: Cow::Borrowed(id),
        mcp_name: mcp_name.map(Cow::Borrowed),
        description: Cow::Borrowed(description),
        parameters_schema,
        visibility: CapabilityVisibility::Shared,
        risk,
        target: CapabilityTarget::Backend,
        voice_max_items,
    }
}

fn static_actions() -> Vec<CapabilityDef> {
    vec![
        cap(
            "query_session_journal",
            Some("query_session_journal"),
            "Lists recorded session journal minutes (title, summary, decisions, files_touched, tags) for the project, optionally filtered by agent_id, offset, and limit.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "agent_id": { "type": "STRING", "description": "Filter minutes by agent identifier (e.g. backend, frontend)" },
                    "offset": { "type": "INTEGER", "description": "Pagination start index (default 0)" },
                    "limit": { "type": "INTEGER", "description": "Optional page size (omit for all)" }
                }
            }),
            CapabilityRisk::ReadOnly,
            Some(12),
        ),
        cap(
            "list_todos",
            Some("manage_todo_item"),
            "Lists project TODO items, optionally filtered by status ('pending' | 'in_progress' | 'completed' | 'cancelled'), offset, and limit.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "status_filter": { "type": "STRING", "description": "Filter by status: pending|in_progress|completed|cancelled" },
                    "offset": { "type": "INTEGER", "description": "Pagination start index (default 0)" },
                    "limit": { "type": "INTEGER", "description": "Optional page size (omit for all)" }
                }
            }),
            CapabilityRisk::ReadOnly,
            Some(20),
        ),
        cap(
            "create_todo",
            Some("manage_todo_item"),
            "Creates a tracked TODO item in the project journal, optionally anchored to file_path, line_start, line_end, or journal_id.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "title": { "type": "STRING", "description": "Short description of the code/implementation TODO" },
                    "description": { "type": "STRING", "description": "Optional technical context or acceptance details" },
                    "category": { "type": "STRING", "description": "feature|bugfix|refactor|docs|test (default: feature)" },
                    "priority": { "type": "INTEGER", "description": "1=urgent, 2=normal (default), 3=low" },
                    "file_path": { "type": "STRING", "description": "Optional project-relative file path anchor" },
                    "line_start": { "type": "INTEGER", "description": "Optional 1-based start line anchor" },
                    "line_end": { "type": "INTEGER", "description": "Optional 1-based end line anchor" },
                    "journal_id": { "type": "INTEGER", "description": "Optional originating session journal ID" }
                },
                "required": ["title"]
            }),
            CapabilityRisk::Mutating,
            None,
        ),
        cap(
            "update_todo_status",
            Some("manage_todo_item"),
            "Updates a project TODO status ('pending' | 'in_progress' | 'completed' | 'cancelled').",
            json!({
                "type": "OBJECT",
                "properties": {
                    "todo_id": { "type": "INTEGER", "description": "Numeric TODO ID from list_todos" },
                    "status": { "type": "STRING", "enum": ["pending", "in_progress", "completed", "cancelled"], "description": "New workflow state" }
                },
                "required": ["todo_id", "status"]
            }),
            CapabilityRisk::Mutating,
            None,
        ),
        cap(
            "list_technical_debt",
            None,
            "Lists recorded technical debt items for the project, optionally filtered by status ('open' | 'resolved'), offset, and limit.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "status_filter": { "type": "STRING", "description": "Filter by status: open|resolved" },
                    "offset": { "type": "INTEGER", "description": "Pagination start index (default 0)" },
                    "limit": { "type": "INTEGER", "description": "Optional page size (omit for all)" }
                }
            }),
            CapabilityRisk::ReadOnly,
            Some(15),
        ),
        cap(
            "record_technical_debt",
            Some("record_technical_debt"),
            "Records a new project technical debt item (title, impact_summary, optional severity/category/remediation_plan/milestone_id/file_path/line_start/line_end/journal_id).",
            json!({
                "type": "OBJECT",
                "properties": {
                    "title": { "type": "STRING", "description": "Short label of the shortcut or architectural debt" },
                    "impact_summary": { "type": "STRING", "description": "Why this debt matters and what risks it introduces" },
                    "severity": { "type": "STRING", "enum": ["low", "medium", "high", "critical"], "description": "Impact urgency (default: medium)" },
                    "category": { "type": "STRING", "description": "Debt area (e.g. architectural, testing, security, performance)" },
                    "remediation_plan": { "type": "STRING", "description": "Recommended approach to resolve it properly later" },
                    "milestone_id": { "type": "INTEGER", "description": "Optional milestone ID to attribute this debt to" },
                    "file_path": { "type": "STRING", "description": "Optional project-relative file path anchor" },
                    "line_start": { "type": "INTEGER", "description": "Optional 1-based start line anchor" },
                    "line_end": { "type": "INTEGER", "description": "Optional 1-based end line anchor" },
                    "journal_id": { "type": "INTEGER", "description": "Optional originating session journal ID" }
                },
                "required": ["title", "impact_summary"]
            }),
            CapabilityRisk::Mutating,
            None,
        ),
        cap(
            "resolve_technical_debt",
            Some("resolve_technical_debt"),
            "Marks a project technical debt item as resolved.",
            json!({
                "type": "OBJECT",
                "properties": { "debt_id": { "type": "INTEGER", "description": "Technical debt ID from list_technical_debt" } },
                "required": ["debt_id"]
            }),
            CapabilityRisk::Mutating,
            None,
        ),
        cap(
            "search_conversations",
            Some("search_conversations"),
            "Searches project agent conversations and session minutes using full-text keywords.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "query": { "type": "STRING", "description": "Keywords to match against conversation topics and summaries" },
                    "agent_id": { "type": "STRING", "description": "Optional agent ID filter" },
                    "status": { "type": "STRING", "enum": ["active", "archived"], "description": "Filter by conversation lifecycle state" },
                    "limit": { "type": "INTEGER", "description": "Max conversations to return (1-25, default 10)" }
                },
                "required": ["query"]
            }),
            CapabilityRisk::ReadOnly,
            Some(10),
        ),
    ]
}

fn parse_optional_file_anchor(
    params: &Value,
) -> Result<(Option<String>, Option<u32>, Option<u32>, Option<i64>), CapabilityError> {
    let file_path = match params.get("file_path") {
        None | Some(Value::Null) => None,
        Some(v) => match v.as_str().map(str::trim).map(|s| s.trim_start_matches("./")) {
            Some(s) if !s.is_empty() => Some(s.to_string()),
            Some(_) => None,
            None => {
                return Err(CapabilityError::InvalidParams(
                    "'file_path' must be a string".to_string(),
                ));
            }
        },
    };
    let line_start = match params.get("line_start") {
        None | Some(Value::Null) => None,
        Some(v) => match v.as_u64().and_then(|n| u32::try_from(n).ok()) {
            Some(n) if n > 0 => Some(n),
            _ => {
                return Err(CapabilityError::InvalidParams(
                    "'line_start' must be a positive integer".to_string(),
                ));
            }
        },
    };
    let line_end = match params.get("line_end") {
        None | Some(Value::Null) => None,
        Some(v) => match v.as_u64().and_then(|n| u32::try_from(n).ok()) {
            Some(n) if n > 0 => Some(n),
            _ => {
                return Err(CapabilityError::InvalidParams(
                    "'line_end' must be a positive integer".to_string(),
                ));
            }
        },
    };
    let journal_id = match params.get("journal_id") {
        None | Some(Value::Null) => None,
        Some(v) => match v.as_i64() {
            Some(id) if id > 0 => Some(id),
            _ => {
                return Err(CapabilityError::InvalidParams(
                    "'journal_id' must be a positive integer".to_string(),
                ));
            }
        },
    };
    Ok((file_path, line_start, line_end, journal_id))
}

fn execute<'a>(
    ctx: CapabilityContext,
    action_id: &'a str,
    params: Value,
) -> BoxFuture<'a, Result<Value, CapabilityError>> {
    Box::pin(async move {
        let project_id = ctx.project_id();
        let project_path = ctx.project.path.clone();
        let actor = ctx.caller.actor_label().to_string();

        match action_id {
            "query_session_journal" => {
                let agent_id = params
                    .get("agent_id")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                let _ = parse_pagination(&params)?;
                let records = run_blocking(move || {
                    Ok(crate::db::db_get_project_session_journals(
                        project_id,
                        agent_id.as_deref(),
                        i64::MAX as usize,
                    )?)
                })
                .await?;
                paginate_items(records, "journals", &params)
            }
            "list_todos" => {
                let status_filter = params
                    .get("status_filter")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                let _ = parse_pagination(&params)?;
                let todos = run_blocking(move || {
                    Ok(crate::db::db_get_project_todos(
                        project_id,
                        status_filter.as_deref(),
                    )?)
                })
                .await?;
                paginate_items(todos, "todos", &params)
            }
            "create_todo" => {
                let title = require_non_empty_str(&params, "title")?;
                let description = params
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                let category = match params.get("category") {
                    None | Some(Value::Null) => "feature".to_string(),
                    Some(v) => match v.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                        Some(cat) => cat.to_string(),
                        None => {
                            return Err(CapabilityError::InvalidParams(
                                "'category' must be a non-empty string".to_string(),
                            ));
                        }
                    },
                };
                let priority = match params.get("priority") {
                    None | Some(Value::Null) => 2i32,
                    Some(v) => match v.as_i64().map(i32::try_from) {
                        Some(Ok(p)) => p,
                        _ => {
                            return Err(CapabilityError::InvalidParams(
                                "'priority' must be a 32-bit integer".to_string(),
                            ));
                        }
                    },
                };
                let (file_path, line_start, line_end, journal_id) = parse_optional_file_anchor(&params)?;
                let now = chrono::Utc::now().timestamp();
                let actor_for_db = actor.clone();
                let is_mcp = matches!(ctx.caller, CapabilityCaller::Mcp { .. });
                let todo_id = run_blocking(move || {
                    let conv_id = if is_mcp {
                        Some(
                            crate::db::db_get_or_create_active_conversation(
                                project_id,
                                &actor_for_db,
                                None,
                                None,
                                None,
                                None,
                                None,
                            )?
                            .id,
                        )
                    } else {
                        None
                    };
                    let rec = crate::db::ProjectTodoRecord {
                        id: None,
                        project_id,
                        agent_id: Some(actor_for_db),
                        agy_conversation_id: None,
                        title,
                        description,
                        category,
                        status: "pending".to_string(),
                        priority,
                        created_at: now,
                        completed_at: None,
                        conversation_id: conv_id,
                        file_path,
                        line_start,
                        line_end,
                        journal_id,
                    };
                    Ok(crate::db::db_insert_todo(&rec)?)
                })
                .await?;
                crate::supervisor::get_supervisor().emit_event(DaemonEvent::todo_updated(
                    &TodoUpdatedPayload {
                        project_id,
                        project_path,
                        todo_id,
                        action: "create".to_string(),
                        agent_id: actor,
                    },
                ));
                crate::semantic::schedule_background_indexing_for_entity(project_id);
                Ok(json!({ "ok": true, "todo_id": todo_id }))
            }
            "update_todo_status" => {
                let todo_id = require_positive_i64(&params, "todo_id")?;
                let status = require_non_empty_str(&params, "status")?;
                if !matches!(
                    status.as_str(),
                    "pending" | "in_progress" | "completed" | "cancelled"
                ) {
                    return Err(CapabilityError::InvalidParams(format!(
                        "Invalid todo status '{status}'"
                    )));
                }
                let st_for_db = status.clone();
                run_blocking(move || {
                    Ok(crate::db::db_update_todo_status_scoped(
                        project_id, todo_id, &st_for_db,
                    )?)
                })
                .await?;
                crate::supervisor::get_supervisor().emit_event(DaemonEvent::todo_updated(
                    &TodoUpdatedPayload {
                        project_id,
                        project_path,
                        todo_id,
                        action: "update_status".to_string(),
                        agent_id: actor,
                    },
                ));
                crate::semantic::schedule_background_indexing_for_entity(project_id);
                Ok(json!({ "ok": true, "todo_id": todo_id, "status": status }))
            }
            "list_technical_debt" => {
                let status_filter = params
                    .get("status_filter")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                let _ = parse_pagination(&params)?;
                let items = run_blocking(move || {
                    Ok(crate::db::db_get_project_technical_debt(
                        project_id,
                        status_filter.as_deref(),
                    )?)
                })
                .await?;
                paginate_items(items, "technical_debt", &params)
            }
            "record_technical_debt" => {
                let title = require_non_empty_str(&params, "title")?;
                let impact_summary = require_non_empty_str(&params, "impact_summary")?;
                let severity = match params.get("severity") {
                    None | Some(Value::Null) => "medium".to_string(),
                    Some(v) => match v.as_str().map(str::trim) {
                        Some(s) if matches!(s, "low" | "medium" | "high" | "critical") => s.to_string(),
                        Some(other) => {
                            return Err(CapabilityError::InvalidParams(format!(
                                "Invalid severity '{other}'; expected low, medium, high, or critical"
                            )));
                        }
                        None => {
                            return Err(CapabilityError::InvalidParams(
                                "'severity' must be a string".to_string(),
                            ));
                        }
                    },
                };
                let category = match params.get("category") {
                    None | Some(Value::Null) => "architectural".to_string(),
                    Some(v) => match v.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                        Some(c) => c.to_string(),
                        None => {
                            return Err(CapabilityError::InvalidParams(
                                "'category' must be a non-empty string".to_string(),
                            ));
                        }
                    },
                };
                let remediation_plan = match params.get("remediation_plan") {
                    None | Some(Value::Null) => None,
                    Some(v) => Some(
                        v.as_str()
                            .ok_or_else(|| {
                                CapabilityError::InvalidParams(
                                    "'remediation_plan' must be a string".to_string(),
                                )
                            })?
                            .to_string(),
                    ),
                };
                let milestone_id = match params.get("milestone_id") {
                    None | Some(Value::Null) => None,
                    Some(v) => Some(v.as_i64().ok_or_else(|| {
                        CapabilityError::InvalidParams("'milestone_id' must be an integer".to_string())
                    })?),
                };
                let (file_path, line_start, line_end, journal_id) = parse_optional_file_anchor(&params)?;
                let now = chrono::Utc::now().timestamp();
                let actor_for_db = actor.clone();
                let is_mcp = matches!(ctx.caller, CapabilityCaller::Mcp { .. });
                let debt_id = run_blocking(move || {
                    let conv_id = if is_mcp {
                        Some(
                            crate::db::db_get_or_create_active_conversation(
                                project_id,
                                &actor_for_db,
                                None,
                                None,
                                None,
                                None,
                                None,
                            )?
                            .id,
                        )
                    } else {
                        None
                    };
                    let rec = crate::db::ProjectTechnicalDebtRecord {
                        id: None,
                        project_id,
                        agy_conversation_id: None,
                        title,
                        category,
                        impact_summary,
                        remediation_plan,
                        status: "open".to_string(),
                        severity,
                        identified_at: now,
                        resolved_at: None,
                        conversation_id: conv_id,
                        milestone_id,
                        file_path,
                        line_start,
                        line_end,
                        journal_id,
                    };
                    Ok(crate::db::db_insert_technical_debt(&rec)?)
                })
                .await?;
                crate::supervisor::get_supervisor().emit_event(DaemonEvent::tech_debt_updated(
                    &TechDebtUpdatedPayload {
                        project_id,
                        project_path,
                        debt_id,
                        action: "record".to_string(),
                        agent_id: actor,
                    },
                ));
                crate::semantic::schedule_background_indexing_for_entity(project_id);
                Ok(json!({ "ok": true, "debt_id": debt_id }))
            }
            "resolve_technical_debt" => {
                let debt_id = require_positive_i64(&params, "debt_id")?;
                run_blocking(move || {
                    Ok(crate::db::db_resolve_technical_debt_scoped(
                        project_id, debt_id,
                    )?)
                })
                .await?;
                crate::supervisor::get_supervisor().emit_event(DaemonEvent::tech_debt_updated(
                    &TechDebtUpdatedPayload {
                        project_id,
                        project_path,
                        debt_id,
                        action: "resolve".to_string(),
                        agent_id: actor,
                    },
                ));
                crate::semantic::schedule_background_indexing_for_entity(project_id);
                Ok(json!({ "ok": true, "debt_id": debt_id }))
            }
            "search_conversations" => {
                let query = require_non_empty_str(&params, "query")?;
                let agent_id = params
                    .get("agent_id")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                let status = params
                    .get("status")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                let limit = match params.get("limit") {
                    None | Some(Value::Null) => 10usize,
                    Some(v) => match v.as_u64() {
                        Some(n) if n > 0 => (n as usize).clamp(1, 25),
                        _ => {
                            return Err(CapabilityError::InvalidParams(
                                "'limit' must be a positive integer".to_string(),
                            ));
                        }
                    },
                };
                let hits = run_blocking(move || {
                    Ok(crate::db::db_search_project_conversations(
                        project_id,
                        agent_id.as_deref(),
                        &query,
                        status.as_deref(),
                        limit,
                    )?)
                })
                .await?;
                let compact: Vec<Value> = hits
                    .into_iter()
                    .map(|c| {
                        json!({
                            "conversation_id": c.id,
                            "agent_id": c.agent_id,
                            "topic": c.topic,
                            "status": c.status,
                            "summary": c.summary,
                            "updated_at": c.updated_at
                        })
                    })
                    .collect();
                Ok(json!({ "conversations": compact }))
            }
            other => Err(CapabilityError::UnknownAction {
                domain: "journal".to_string(),
                action_id: other.to_string(),
            }),
        }
    })
}
