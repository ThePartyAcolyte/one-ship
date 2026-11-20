use std::collections::HashMap;
use std::path::Path;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::hydration::{ScriptError, ScriptKind};
use crate::port_inspect::PortInspectError;
use crate::registry::{ProjectEntry, RegistryError};
use crate::supervisor::{get_supervisor, ProcessSupervisor, SupervisorError};

/// Failure of a service lifecycle or port release command.
#[derive(Debug, thiserror::Error)]
pub enum OrchestratorError {
    /// The port changed hands between the conflict report and the release request.
    #[error("Port {port} is now held by PID {pid} ('{cmdline}'), not PID {expected_pid}; review the conflict again")]
    PortOccupantChanged { port: u16, expected_pid: u32, pid: u32, cmdline: String },
    #[error("Background task failed: {0}")]
    Task(String),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Script(#[from] ScriptError),
    #[error(transparent)]
    Supervisor(#[from] SupervisorError),
    #[error(transparent)]
    PortInspect(#[from] PortInspectError),
}

impl crate::api::ApiError for OrchestratorError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::PortOccupantChanged { .. } => StatusCode::CONFLICT,
            Self::Task(_) | Self::PortInspect(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Registry(e) => e.status(),
            Self::Script(e) => e.status(),
            Self::Supervisor(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::PortOccupantChanged { .. } => "OrchestratorError::PortOccupantChanged",
            Self::Task(_) => "OrchestratorError::Task",
            Self::PortInspect(_) => "OrchestratorError::PortInspect",
            Self::Registry(e) => e.code(),
            Self::Script(e) => e.code(),
            Self::Supervisor(e) => e.code(),
        }
    }
}

/// Arguments of the daemon command `start_service`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct StartServiceArgs {
    pub project_id: usize,
    pub service_id: String,
    /// Launch options by name; a missing option takes its declared default.
    pub options: HashMap<String, serde_json::Value>,
}

/// Arguments of the daemon command `stop_service`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct StopServiceArgs {
    pub project_id: usize,
    pub service_id: String,
}

/// Arguments of the daemon command `release_port`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ReleasePortArgs {
    pub port: u16,
    pub target_pid: u32,
}

/// Result of a service start. A port held by another process is an outcome the UI resolves
/// (offering `release_port`), not a failure.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum ServiceStartOutcome {
    /// The service runs (spawned, or adopted from a live process of the same service).
    Started { pid: u32 },
    /// The service's port is held by a process that is not this service.
    PortOccupied { port: u16, pid: u32, cmdline: String },
}

fn task_err(e: tokio::task::JoinError) -> OrchestratorError {
    OrchestratorError::Task(e.to_string())
}

async fn find_project(project_id: usize) -> Result<ProjectEntry, OrchestratorError> {
    let project = tokio::task::spawn_blocking(move || crate::registry::find_project_by_id(project_id))
        .await
        .map_err(task_err)??;
    Ok(project)
}

/// Starts the service `[scripts.persistent.<service_id>]` of `project` with the launch option
/// `options` (missing options take their declared default). Command, arguments and working
/// directory come only from `rula.toml`.
pub async fn start_project_service(
    project: &ProjectEntry,
    service_id: &str,
    options: HashMap<String, serde_json::Value>,
) -> Result<ServiceStartOutcome, OrchestratorError> {
    let project_path = project.path.clone();
    let id = service_id.to_string();
    let (script, option_args) = tokio::task::spawn_blocking(move || {
        let script = crate::hydration::find_declared_script(Path::new(&project_path), ScriptKind::Persistent, &id)?;
        let option_args = crate::hydration::expand_script_options(&script, &options)?;
        Ok::<_, ScriptError>((script, option_args))
    })
    .await
    .map_err(task_err)??;

    let mut args = script.args.clone();
    args.extend(option_args);
    crate::logger::log("INFO", "orchestrator", &format!(
        "Starting service '{}' for {}: {} {:?}",
        service_id, project.name, script.command, args
    ));

    match get_supervisor().spawn_service(&project.path, service_id, &script.command, &args, script.cwd.as_deref(), script.port, script.profiler.as_deref()).await {
        Ok(pid) => {
            crate::logger::log("INFO", "orchestrator", &format!("Service '{}' started with PID {}", service_id, pid));
            Ok(ServiceStartOutcome::Started { pid })
        }
        Err(SupervisorError::PortOccupied { port, pid, cmdline, .. }) => {
            crate::logger::log("WARN", "orchestrator", &format!(
                "Service '{}' not started: port {} is held by PID {} ('{}')",
                service_id, port, pid, cmdline
            ));
            Ok(ServiceStartOutcome::PortOccupied { port, pid, cmdline })
        }
        Err(e) => {
            crate::logger::log("ERROR", "orchestrator", &format!("Failed to start service '{}': {}", service_id, e));
            Err(e.into())
        }
    }
}

/// Stops the service or interactive shell `service_id` of the project at `project_path`.
/// Stopping a service that is not running succeeds.
pub async fn stop_project_service(project_path: &str, service_id: &str) -> Result<(), OrchestratorError> {
    crate::logger::log("INFO", "orchestrator", &format!("Stopping service '{}' of {}", service_id, project_path));
    let key = ProcessSupervisor::format_service_key(project_path, service_id);
    if let Err(e) = get_supervisor().stop_process(&key).await {
        crate::logger::log("ERROR", "orchestrator", &format!("Failed to stop service '{}': {}", service_id, e));
        return Err(e.into());
    }
    Ok(())
}

/// Starts a declared persistent service of a registered project. A running instance is
/// replaced, so this also restarts the service. Daemon command.
pub async fn start_service(
    project_id: usize,
    service_id: String,
    options: HashMap<String, serde_json::Value>,
) -> Result<ServiceStartOutcome, OrchestratorError> {
    crate::logger::log("DEBUG", "orchestrator", &format!(
        "start_service: project_id={}, service_id='{}', options={}",
        project_id, service_id, options.len()
    ));
    let project = find_project(project_id).await?;
    start_project_service(&project, &service_id, options).await
}

/// Stops a service or interactive shell of a registered project. Daemon command.
pub async fn stop_service(project_id: usize, service_id: String) -> Result<(), OrchestratorError> {
    crate::logger::log("DEBUG", "orchestrator", &format!("stop_service: project_id={}, service_id='{}'", project_id, service_id));
    let project = find_project(project_id).await?;
    stop_project_service(&project.path, &service_id).await
}

/// Terminates the process `target_pid` (reported by a `PortOccupied` start outcome) holding
/// `port`, with its process group. A port that is already free succeeds; a port now held by a
/// different process is `PortOccupantChanged`. Daemon command.
pub async fn release_port(port: u16, target_pid: u32) -> Result<(), OrchestratorError> {
    crate::logger::log("INFO", "orchestrator", &format!("Releasing port {} (target PID {})", port, target_pid));

    let occupant = tokio::task::spawn_blocking(move || crate::port_inspect::find_port_listener(port))
        .await
        .map_err(task_err)??;
    let Some(occupant) = occupant else {
        crate::logger::log("INFO", "orchestrator", &format!("Port {} is already free", port));
        return Ok(());
    };
    if occupant.pid != target_pid && occupant.pgid != target_pid {
        return Err(OrchestratorError::PortOccupantChanged {
            port,
            expected_pid: target_pid,
            pid: occupant.pid,
            cmdline: occupant.cmdline,
        });
    }

    let timeout_ms = crate::config::resolve_effective_u64(None, None, "orchestrator", "port_release_timeout_ms");
    let poll_step_ms = crate::config::resolve_effective_u64(None, None, "orchestrator", "port_release_poll_step_ms").max(1);
    crate::port_inspect::terminate_port_occupant(&occupant, timeout_ms, poll_step_ms).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_outcome_serializes_with_kind_tag() {
        let started = serde_json::to_value(ServiceStartOutcome::Started { pid: 7 }).unwrap();
        assert_eq!(started, serde_json::json!({"kind": "started", "pid": 7}));
        let occupied = serde_json::to_value(ServiceStartOutcome::PortOccupied { port: 8000, pid: 9, cmdline: "uvicorn".into() }).unwrap();
        assert_eq!(occupied, serde_json::json!({"kind": "port_occupied", "port": 8000, "pid": 9, "cmdline": "uvicorn"}));
    }
}
