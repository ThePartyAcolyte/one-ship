//! System Logs, Audit Trail & Agent State Provider (`domain = "logs"`, Spec 40).
//!
//! Provides read-only observability over project/system logs, agent audit history, and live
//! supervised agent statuses for both MCP callers and the Global Voice Controller.

use std::borrow::Cow;
use serde_json::{json, Value};

use crate::db::{db_get_audit_trail, get_system_logs, GetSystemLogsArgs};
use crate::supervisor::get_supervisor;

use super::{
    run_blocking, BoxFuture, CapabilityContext, CapabilityDef, CapabilityError, CapabilityProvider,
    CapabilityRisk, CapabilityTarget, CapabilityVisibility,
};

const DEFAULT_LOGS_LIMIT_COUNT: usize = 30;
const MAX_LOGS_LIMIT_COUNT: usize = 100;

pub static PROVIDER: CapabilityProvider = CapabilityProvider {
    domain_id: "logs",
    label: "Logs, Audit & Agents",
    required_capability: None,
    linked_surface_path: None,
    static_actions,
    dynamic_actions: |_| Vec::new(),
    execute,
};

fn cap(
    id: &'static str,
    mcp_name: Option<&'static str>,
    description: &'static str,
    parameters_schema: Value,
    voice_max_items: Option<usize>,
) -> CapabilityDef {
    CapabilityDef {
        id: Cow::Borrowed(id),
        mcp_name: mcp_name.map(Cow::Borrowed),
        description: Cow::Borrowed(description),
        parameters_schema,
        visibility: CapabilityVisibility::Shared,
        risk: CapabilityRisk::ReadOnly,
        target: CapabilityTarget::Backend,
        voice_max_items,
    }
}

fn static_actions() -> Vec<CapabilityDef> {
    vec![
        cap(
            "query_system_logs",
            Some("query_system_logs"),
            "Retrieves recent project or orchestrator logs filtered optionally by level ('INFO' | 'WARN' | 'ERROR' | 'DEBUG'), search_query, scope ('project' | 'system'), and limit.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "level": { "type": "STRING", "enum": ["INFO", "WARN", "ERROR", "DEBUG"], "description": "Severity level filter" },
                    "search_query": { "type": "STRING", "description": "Keyword substring in log message" },
                    "scope": { "type": "STRING", "enum": ["project", "system"], "description": "Project logs (default) or daemon-wide orchestrator logs" },
                    "limit": { "type": "INTEGER", "description": "Max log lines (1-100, default 30)" }
                }
            }),
            Some(20),
        ),
        cap(
            "query_project_audit",
            Some("query_project_audit"),
            "Reads the project's agent audit trail (dispatched prompts, file edits, approvals), optionally filtered by YYYY-MM-DD date and limit.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "date": { "type": "STRING", "description": "Optional YYYY-MM-DD date filter" },
                    "limit": { "type": "INTEGER", "description": "Max audit events (1-100, default 30)" }
                }
            }),
            Some(20),
        ),
        cap(
            "query_agents",
            Some("query_project_agents"),
            "Lists configured agents for the project along with current running/waiting/offline supervisor status and active task bindings.",
            json!({ "type": "OBJECT", "properties": {} }),
            Some(15),
        ),
    ]
}

fn optional_str(params: &Value, key: &str) -> Result<Option<String>, CapabilityError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => {
            let Some(raw) = v.as_str() else {
                return Err(CapabilityError::InvalidParams(format!(
                    "'{key}' must be a string"
                )));
            };
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
    }
}

fn bounded_limit(params: &Value) -> Result<usize, CapabilityError> {
    match params.get("limit") {
        None | Some(Value::Null) => Ok(DEFAULT_LOGS_LIMIT_COUNT),
        Some(v) => match v.as_u64() {
            Some(n) if n > 0 => Ok((n as usize).clamp(1, MAX_LOGS_LIMIT_COUNT)),
            _ => Err(CapabilityError::InvalidParams(
                "'limit' must be a positive integer".to_string(),
            )),
        },
    }
}

fn execute<'a>(
    ctx: CapabilityContext,
    action_id: &'a str,
    params: Value,
) -> BoxFuture<'a, Result<Value, CapabilityError>> {
    Box::pin(async move {
        let project_id = ctx.project_id();
        let project_path = ctx.project.path.clone();
        let project_name = ctx.project.name.clone();

        match action_id {
            "query_system_logs" => {
                let level = optional_str(&params, "level")?;
                if let Some(ref lvl) = level {
                    if !matches!(lvl.as_str(), "INFO" | "WARN" | "ERROR" | "DEBUG") {
                        return Err(CapabilityError::InvalidParams(format!(
                            "Invalid log level '{lvl}'; expected INFO, WARN, ERROR, or DEBUG"
                        )));
                    }
                }
                let search_query = optional_str(&params, "search_query")?;
                let limit_count = bounded_limit(&params)?;
                let origin = match optional_str(&params, "scope")?.as_deref() {
                    None | Some("project") => Some(format!("project:{project_id}")),
                    Some("system") => Some("system".to_string()),
                    Some(other) => {
                        return Err(CapabilityError::InvalidParams(format!(
                            "Invalid log scope '{other}'; expected 'project' or 'system'"
                        )));
                    }
                };

                let logs = run_blocking(move || {
                    Ok(get_system_logs(GetSystemLogsArgs {
                        origin,
                        source: None,
                        search_query,
                        level,
                        limit_count,
                        before_id: None,
                        after_id: None,
                    })?)
                })
                .await?;
                Ok(json!({ "logs": logs }))
            }
            "query_project_audit" => {
                let date = optional_str(&params, "date")?;
                let limit = bounded_limit(&params)?;
                let records = run_blocking(move || {
                    Ok(db_get_audit_trail(project_id, limit, date.as_deref())?)
                })
                .await?;
                Ok(json!({ "records": records }))
            }
            "query_agents" => {
                let result = run_blocking(move || {
                    let state = get_supervisor().compute_project_state(&project_path, &project_name);
                    Ok(json!({
                        "project_name": project_name,
                        "project_path": project_path,
                        "agents": state.agents
                    }))
                })
                .await?;
                Ok(result)
            }
            other => Err(CapabilityError::UnknownAction {
                domain: "logs".to_string(),
                action_id: other.to_string(),
            }),
        }
    })
}
