//! Scripts, Tasks, Services & Terminal Inspection Provider (`domain = "scripts"`, Spec 40).
//!
//! Unifies declared `rula.toml` script discovery, finite task execution/polling/cancellation,
//! persistent service start/stop/restart, and live process terminal inspection across MCP
//! agents and the Global Voice Controller without tab navigation.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use serde_json::{json, Value};

use crate::config::resolve_effective_u64;
use crate::db::{db_get_task_report, TaskExecutionRecord};
use crate::hydration::{find_declared_script, ScriptKind};
use crate::kopeng::errors::extract_semantic_errors;
use crate::kopeng::runner::start_project_task;
use crate::orchestrator::{start_project_service, stop_project_service, ServiceStartOutcome};
use crate::registry::find_project_by_id;
use crate::supervisor::{get_supervisor, ProcessSupervisor};

use super::{
    require_non_empty_str, run_blocking, BoxFuture, CapabilityContext, CapabilityDef,
    CapabilityError, CapabilityProvider, CapabilityRisk, CapabilityTarget, CapabilityVisibility,
};

const DEFAULT_TERMINAL_LINES_COUNT: usize = 50;
const MAX_TERMINAL_LINES_COUNT: usize = 200;

pub static PROVIDER: CapabilityProvider = CapabilityProvider {
    domain_id: "scripts",
    label: "Scripts, Tasks & Terminals",
    required_capability: None,
    linked_surface_path: Some("overview"),
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
            "list_scripts",
            Some("list_project_scripts"),
            "Lists all declared scripts ([scripts.persistent] and [scripts.task]) from rula.toml along with current running/stopped status.",
            json!({ "type": "OBJECT", "properties": {} }),
            CapabilityRisk::ReadOnly,
            Some(20),
        ),
        cap(
            "start_task",
            Some("start_project_task"),
            "Starts a declared finite task ([scripts.task.<task_id>]) asynchronously or synchronously (wait=true up to timeout_ms).",
            json!({
                "type": "OBJECT",
                "properties": {
                    "task_id": { "type": "STRING", "description": "Task identifier declared in rula.toml (from list_scripts)" },
                    "options": { "type": "OBJECT", "description": "Named script parameter overrides declared in rula.toml" },
                    "wait": { "type": "BOOLEAN", "description": "True to block until exit (up to timeout_ms); default false runs async" },
                    "timeout_ms": { "type": "INTEGER", "description": "Sync wait timeout in milliseconds" }
                },
                "required": ["task_id"]
            }),
            CapabilityRisk::Mutating,
            None,
        ),
        cap(
            "query_task_status",
            Some("query_project_task"),
            "Queries status, duration, exit code, and live or recorded stdout logs for a task execution ID (exec_id).",
            json!({
                "type": "OBJECT",
                "properties": {
                    "exec_id": { "type": "INTEGER", "description": "Execution ID returned by start_task" }
                },
                "required": ["exec_id"]
            }),
            CapabilityRisk::ReadOnly,
            None,
        ),
        cap(
            "cancel_task",
            Some("cancel_project_task"),
            "Cancels a currently running finite task execution (exec_id) owned by this project.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "exec_id": { "type": "INTEGER", "description": "Running execution ID returned by start_task" }
                },
                "required": ["exec_id"]
            }),
            CapabilityRisk::ConfirmRequired,
            None,
        ),
        cap(
            "manage_service",
            Some("manage_project_script"),
            "Starts, stops, or restarts a declared persistent service ([scripts.persistent.<script_id>]).",
            json!({
                "type": "OBJECT",
                "properties": {
                    "script_id": { "type": "STRING", "description": "Persistent service ID from list_scripts" },
                    "action": { "type": "STRING", "enum": ["start", "stop", "restart"], "description": "Lifecycle operation on the service" }
                },
                "required": ["script_id", "action"]
            }),
            CapabilityRisk::Mutating,
            None,
        ),
        cap(
            "inspect_terminal",
            Some("inspect_terminal_output"),
            "Reads live buffered terminal output lines for an agent id, 'svc:<service_id>', or 'tsk:<task_id>' (optional filter='errors_only').",
            json!({
                "type": "OBJECT",
                "properties": {
                    "target": { "type": "STRING", "description": "Agent name (e.g. 'backend'), 'svc:<script_id>', or 'tsk:<task_id>'" },
                    "lines": { "type": "INTEGER", "description": "Tail line count (1-200, default 50)" },
                    "filter": { "type": "STRING", "enum": ["all", "errors_only"], "description": "Show all lines or extract compiler/runtime errors only" }
                },
                "required": ["target"]
            }),
            CapabilityRisk::ReadOnly,
            None,
        ),
    ]
}

fn parse_exec_id(params: &Value) -> Result<i64, CapabilityError> {
    let val = params.get("exec_id").ok_or_else(|| {
        CapabilityError::InvalidParams("Missing positive integer parameter 'exec_id'".to_string())
    })?;
    let parsed = if let Some(n) = val.as_i64() {
        Some(n)
    } else if let Some(s) = val.as_str() {
        s.trim().parse::<i64>().map(Some).map_err(|_| {
            CapabilityError::InvalidParams("'exec_id' must be a valid integer".to_string())
        })?
    } else {
        None
    };
    match parsed {
        Some(n) if n > 0 => Ok(n),
        Some(n) => Err(CapabilityError::InvalidParams(format!(
            "'exec_id' must be positive, got {n}"
        ))),
        None => Err(CapabilityError::InvalidParams(
            "'exec_id' must be an integer".to_string(),
        )),
    }
}

fn optional_bool(params: &Value, key: &str, default: bool) -> Result<bool, CapabilityError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v
            .as_bool()
            .ok_or_else(|| CapabilityError::InvalidParams(format!("'{key}' must be a boolean"))),
    }
}

fn parse_options(params: &Value) -> Result<HashMap<String, Value>, CapabilityError> {
    match params.get("options") {
        None | Some(Value::Null) => Ok(HashMap::new()),
        Some(v) => {
            let Some(obj) = v.as_object() else {
                return Err(CapabilityError::InvalidParams(
                    "'options' must be a JSON object".to_string(),
                ));
            };
            Ok(obj.iter().map(|(k, val)| (k.clone(), val.clone())).collect())
        }
    }
}

fn load_owned_task_record(exec_id: i64, project_id: i64) -> Result<TaskExecutionRecord, CapabilityError> {
    let Some(rec) = db_get_task_report(exec_id)? else {
        return Err(CapabilityError::NotFound {
            entity: "Task execution",
            id: exec_id,
        });
    };
    if rec.project_id != project_id {
        return Err(CapabilityError::OwnershipMismatch {
            entity: "Task execution",
            entity_id: exec_id,
            project_id,
        });
    }
    Ok(rec)
}

fn execute<'a>(
    ctx: CapabilityContext,
    action_id: &'a str,
    params: Value,
) -> BoxFuture<'a, Result<Value, CapabilityError>> {
    Box::pin(async move {
        let project_id = ctx.project_id();
        let project_id_usize = ctx.project.id;
        let project_path = ctx.project.path.clone();
        let project_name = ctx.project.name.clone();

        match action_id {
            "list_scripts" => {
                let scripts = ctx.project.scripts.clone();
                let items = run_blocking(move || {
                    let running = get_supervisor().running_service_ids(&project_path);
                    let mut out = Vec::with_capacity(scripts.len());
                    for script in scripts {
                        let is_running = running.contains(&script.id);
                        let mut val = serde_json::to_value(script)
                            .map_err(|e| CapabilityError::Task(e.to_string()))?;
                        if let Some(obj) = val.as_object_mut() {
                            obj.insert(
                                "status".to_string(),
                                json!(if is_running { "running" } else { "stopped" }),
                            );
                        }
                        out.push(val);
                    }
                    Ok(out)
                })
                .await?;
                Ok(json!({ "scripts": items }))
            }
            "start_task" => {
                let task_id = require_non_empty_str(&params, "task_id")?;
                let options = parse_options(&params)?;
                let wait = optional_bool(&params, "wait", false)?;
                let timeout_ms = match params.get("timeout_ms") {
                    None | Some(Value::Null) => resolve_effective_u64(
                        Some(&project_path),
                        None,
                        "mcp",
                        "task_wait_timeout_ms",
                    ),
                    Some(v) => v.as_u64().ok_or_else(|| {
                        CapabilityError::InvalidParams(
                            "'timeout_ms' must be a positive integer".to_string(),
                        )
                    })?,
                };
                let project_entry = run_blocking(move || Ok(find_project_by_id(project_id_usize)?))
                    .await?;
                let (exec_id, handle) = start_project_task(&project_entry, &task_id, options).await?;
                if !wait {
                    return Ok(json!({
                        "status": "started",
                        "exec_id": exec_id,
                        "task_id": task_id,
                        "project": project_name,
                        "message": format!(
                            "Task '{}' started asynchronously in project '{}' with execution ID: {}. Poll query_project_task with exec_id={} to check progress.",
                            task_id, project_name, exec_id, exec_id
                        )
                    }));
                }

                match tokio::time::timeout(tokio::time::Duration::from_millis(timeout_ms), handle)
                    .await
                {
                    Ok(Ok(Ok(rec))) => {
                        let is_failure = rec.status == "failed" || rec.exit_code != Some(0);
                        Ok(json!({
                            "status": rec.status,
                            "exec_id": rec.id,
                            "task_id": rec.task_id,
                            "project": project_name,
                            "exit_code": rec.exit_code,
                            "duration_ms": rec.duration_ms,
                            "summary": rec.summary,
                            "logs": rec.stdout_log,
                            "is_failure": is_failure
                        }))
                    }
                    Ok(Ok(Err(e))) => Err(CapabilityError::Runner(e)),
                    Ok(Err(e)) => Err(CapabilityError::Task(format!(
                        "Task '{}' (exec_id={}) runner aborted: {e}",
                        task_id, exec_id
                    ))),
                    Err(_) => Ok(json!({
                        "status": "running",
                        "timed_out_waiting": true,
                        "exec_id": exec_id,
                        "task_id": task_id,
                        "project": project_name,
                        "message": format!(
                            "Task '{}' (exec_id={}) in project '{}' is still running after {}ms; it continues in the background. Use query_project_task(exec_id={}) to continue monitoring.",
                            task_id, exec_id, project_name, timeout_ms, exec_id
                        )
                    })),
                }
            }
            "query_task_status" => {
                let exec_id = parse_exec_id(&params)?;
                let res = run_blocking(move || {
                    let rec = load_owned_task_record(exec_id, project_id)?;
                    let mut result = json!({
                        "exec_id": rec.id,
                        "project_id": rec.project_id,
                        "task_id": rec.task_id,
                        "task_type": rec.task_type,
                        "command": rec.command,
                        "started_at": rec.started_at,
                        "duration_ms": rec.duration_ms,
                        "exit_code": rec.exit_code,
                        "status": rec.status,
                        "summary": rec.summary,
                    });
                    if rec.status == "running" {
                        let sup = get_supervisor();
                        let key = ProcessSupervisor::format_task_key(&project_path, &rec.task_id);
                        let is_live = sup.list_active_processes().iter().any(|proc| {
                            proc.get("key").and_then(|v| v.as_str()) == Some(key.as_str())
                        });
                        if is_live {
                            let lines = sup.read_output(&key, DEFAULT_TERMINAL_LINES_COUNT);
                            result["live_logs"] = json!(lines.join("\n"));
                        }
                    } else if let Some(ref logs) = rec.stdout_log {
                        result["logs"] = json!(logs);
                    }
                    Ok(result)
                })
                .await?;
                Ok(res)
            }
            "cancel_task" => {
                let exec_id = parse_exec_id(&params)?;
                let key = run_blocking(move || {
                    let rec = load_owned_task_record(exec_id, project_id)?;
                    Ok(ProcessSupervisor::format_task_key(&project_path, &rec.task_id))
                })
                .await?;
                let sup = get_supervisor();
                let is_live = sup
                    .list_active_processes()
                    .iter()
                    .any(|p| p.get("key").and_then(|v| v.as_str()) == Some(key.as_str()));
                if !is_live {
                    return Err(CapabilityError::NotFound {
                        entity: "Running task process",
                        id: exec_id,
                    });
                }
                sup.stop_process(&key).await?;
                Ok(json!({
                    "ok": true,
                    "exec_id": exec_id,
                    "message": format!("Task execution {} cancelled successfully.", exec_id)
                }))
            }
            "manage_service" => {
                let script_id = require_non_empty_str(&params, "script_id")?;
                let action = require_non_empty_str(&params, "action")?;
                let path_clone = project_path.clone();
                let id_clone = script_id.clone();
                let sc = run_blocking(move || {
                    Ok(find_declared_script(
                        Path::new(&path_clone),
                        ScriptKind::Persistent,
                        &id_clone,
                    )?)
                })
                .await?;

                match action.as_str() {
                    "start" | "restart" => {
                        let entry = run_blocking(move || Ok(find_project_by_id(project_id_usize)?))
                            .await?;
                        match start_project_service(&entry, &sc.id, HashMap::new()).await? {
                            ServiceStartOutcome::Started { pid } => Ok(json!({
                                "ok": true,
                                "script_id": sc.id,
                                "action": action,
                                "pid": pid,
                                "message": format!("Service '{}' {}ed (PID {}).", sc.id, action, pid)
                            })),
                            ServiceStartOutcome::PortOccupied { port, pid, cmdline } => {
                                Err(CapabilityError::Conflict(format!(
                                    "Service '{}' was not started: port {} is held by PID {} ('{}'). Ask the user to release the port from the ONE-SHIP terminal grid.",
                                    sc.id, port, pid, cmdline
                                )))
                            }
                        }
                    }
                    "stop" => {
                        stop_project_service(&project_path, &sc.id).await?;
                        Ok(json!({
                            "ok": true,
                            "script_id": sc.id,
                            "action": "stop",
                            "message": format!("Service '{}' stopped successfully.", sc.id)
                        }))
                    }
                    other => Err(CapabilityError::InvalidParams(format!(
                        "Unknown service action '{other}'; expected start, stop, or restart"
                    ))),
                }
            }
            "inspect_terminal" => {
                let target = require_non_empty_str(&params, "target")?;
                let lines = match params.get("lines") {
                    None | Some(Value::Null) => DEFAULT_TERMINAL_LINES_COUNT,
                    Some(v) => match v.as_u64() {
                        Some(n) if n > 0 => (n as usize).clamp(1, MAX_TERMINAL_LINES_COUNT),
                        _ => {
                            return Err(CapabilityError::InvalidParams(
                                "'lines' must be a positive integer".to_string(),
                            ));
                        }
                    },
                };
                let errors_only = match params.get("filter") {
                    None | Some(Value::Null) => false,
                    Some(v) => match v.as_str() {
                        Some("all") => false,
                        Some("errors_only") => true,
                        Some(other) => {
                            return Err(CapabilityError::InvalidParams(format!(
                                "Invalid terminal filter '{other}'; expected 'all' or 'errors_only'"
                            )));
                        }
                        None => {
                            return Err(CapabilityError::InvalidParams(
                                "'filter' must be a string".to_string(),
                            ));
                        }
                    },
                };

                let out = run_blocking(move || {
                    let key = if let Some(service_id) = target.strip_prefix("svc:") {
                        ProcessSupervisor::format_service_key(&project_path, service_id)
                    } else if target.starts_with("tsk:") {
                        format!("{project_path}:{target}")
                    } else {
                        ProcessSupervisor::format_agent_key(&project_path, &target)
                    };
                    let raw = get_supervisor().read_output(&key, lines).join("\n");
                    let output = if errors_only {
                        extract_semantic_errors(&raw)
                    } else {
                        raw
                    };
                    Ok(json!({
                        "target": target,
                        "lines_requested": lines,
                        "errors_only": errors_only,
                        "output": output
                    }))
                })
                .await?;
                Ok(out)
            }
            other => Err(CapabilityError::UnknownAction {
                domain: "scripts".to_string(),
                action_id: other.to_string(),
            }),
        }
    })
}
