use std::collections::HashMap;
use std::path::{Path, PathBuf};
use serde::Deserialize;
use tokio::task::JoinHandle;
use ts_rs::TS;

use crate::db::{DatabaseError, TaskExecutionRecord};
use crate::hydration::{ScriptError, ScriptKind};
use crate::registry::{ProjectEntry, RegistryError};

/// Failure to resolve, record or run a declared finite task.
#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error("Task execution {0} is missing after it was recorded")]
    ExecutionMissing(i64),
    #[error("Background task failed: {0}")]
    Task(String),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Script(#[from] ScriptError),
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

impl crate::api::ApiError for RunnerError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::ExecutionMissing(_) | Self::Task(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Database(e) => e.status(),
            Self::Registry(e) => e.status(),
            Self::Script(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::ExecutionMissing(_) => "RunnerError::ExecutionMissing",
            Self::Task(_) => "RunnerError::Task",
            Self::Database(e) => e.code(),
            Self::Registry(e) => e.code(),
            Self::Script(e) => e.code(),
        }
    }
}

/// Arguments of the daemon command `run_project_task`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct RunProjectTaskArgs {
    pub project_id: usize,
    pub task_id: String,
    /// Launch options by name; a missing option takes its declared default.
    pub options: HashMap<String, serde_json::Value>,
}

fn task_err(e: tokio::task::JoinError) -> RunnerError {
    RunnerError::Task(e.to_string())
}

/// A declared task resolved from `rula.toml` for one execution.
struct PreparedTask {
    project_id: usize,
    project_path: String,
    task_id: String,
    command: String,
    args: Vec<String>,
    work_dir: PathBuf,
    /// Display form of the command line, stored in the execution record.
    command_line: String,
}

/// Resolves `[scripts.task.<task_id>]` of `project` and expands its launch `options` (missing
/// options take their declared default). Command, arguments and working directory come only
/// from `rula.toml`.
async fn prepare_task(
    project: &ProjectEntry,
    task_id: &str,
    options: HashMap<String, serde_json::Value>,
) -> Result<PreparedTask, RunnerError> {
    let project_path = project.path.clone();
    let id = task_id.to_string();
    let (script, option_args) = tokio::task::spawn_blocking(move || {
        let script = crate::hydration::find_declared_script(Path::new(&project_path), ScriptKind::Task, &id)?;
        let option_args = crate::hydration::expand_script_options(&script, &options)?;
        Ok::<_, ScriptError>((script, option_args))
    })
    .await
    .map_err(task_err)??;

    let mut args = script.args.clone();
    args.extend(option_args);
    let work_dir = match script.cwd.as_deref() {
        Some(rel) if !rel.is_empty() && rel != "." => Path::new(&project.path).join(rel),
        _ => PathBuf::from(&project.path),
    };
    let command_line = std::iter::once(script.command.as_str())
        .chain(args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ");
    Ok(PreparedTask {
        project_id: project.id,
        project_path: project.path.clone(),
        task_id: task_id.to_string(),
        command: script.command,
        args,
        work_dir,
        command_line,
    })
}

/// Records a `running` execution of `prepared` and returns its ID.
async fn record_start(prepared: &PreparedTask) -> Result<i64, RunnerError> {
    let project_id = prepared.project_id as i64;
    let task_id = prepared.task_id.clone();
    let command_line = prepared.command_line.clone();
    let exec_id = tokio::task::spawn_blocking(move || {
        crate::db::db_insert_task_execution(project_id, &task_id, "task_cli", &command_line)
    })
    .await
    .map_err(task_err)??;
    crate::logger::log("INFO", "runner", &format!(
        "Running task '{}' for project {}: {} (exec_id {})",
        prepared.task_id, prepared.project_id, prepared.command_line, exec_id
    ));
    Ok(exec_id)
}

/// Runs a recorded execution under the supervisor and closes its record. A process that fails
/// to spawn closes the record as `failed` without exit code.
async fn execute(prepared: PreparedTask, exec_id: i64) -> Result<TaskExecutionRecord, RunnerError> {
    let spawned = crate::supervisor::get_supervisor()
        .spawn_task(
            prepared.project_id as i64,
            &prepared.project_path,
            &prepared.task_id,
            exec_id,
            &prepared.command,
            &prepared.args,
            Some(&prepared.work_dir),
        )
        .await;
    let (duration_ms, exit_code, status, logs) = match spawned {
        Ok(r) => (r.duration_ms, Some(r.exit_code), r.status, r.combined_logs),
        Err(e) => {
            crate::logger::log("ERROR", "runner", &format!(
                "Task '{}' (exec_id {}) failed to spawn: {}",
                prepared.task_id, exec_id, e
            ));
            (0, None, "failed".to_string(), format!("Failed to spawn process: {}", e))
        }
    };

    let exit_label = exit_code.map_or_else(|| "none".to_string(), |c| c.to_string());
    let summary = format!("Task '{}' {} with exit code {} in {}ms", prepared.task_id, status, exit_label, duration_ms);
    let details_markdown = format!(
        "### Task Execution: `{}`\n\n- **Status:** `{}`\n- **Exit Code:** `{}`\n- **Duration:** `{}ms`\n- **Command:** `{}`\n",
        prepared.task_id,
        status.to_uppercase(),
        exit_label,
        duration_ms,
        prepared.command_line
    );

    let level = if exit_code == Some(0) { "INFO" } else { "WARN" };
    crate::logger::log(level, "runner", &format!("Task '{}' finished: {}", prepared.task_id, summary));

    let report = tokio::task::spawn_blocking(move || {
        crate::db::db_finish_task_execution(
            exec_id,
            duration_ms,
            exit_code,
            &status,
            Some(&summary),
            Some(&details_markdown),
            Some(&logs),
        )?;
        crate::db::db_get_task_report(exec_id)?.ok_or(RunnerError::ExecutionMissing(exec_id))
    })
    .await
    .map_err(task_err)??;
    Ok(report)
}

/// Runs the declared task `[scripts.task.<task_id>]` of a registered project to completion and
/// returns its execution record. Daemon command.
pub async fn run_project_task(
    project_id: usize,
    task_id: String,
    options: HashMap<String, serde_json::Value>,
) -> Result<TaskExecutionRecord, RunnerError> {
    crate::logger::log("DEBUG", "runner", &format!(
        "run_project_task: project_id={}, task_id='{}', options={}",
        project_id, task_id, options.len()
    ));
    let project = tokio::task::spawn_blocking(move || crate::registry::find_project_by_id(project_id))
        .await
        .map_err(task_err)??;
    let prepared = prepare_task(&project, &task_id, options).await?;
    let exec_id = record_start(&prepared).await?;
    execute(prepared, exec_id).await
}

/// Starts the declared task `[scripts.task.<task_id>]` of `project` in the background.
///
/// Returns the execution ID and a handle resolving to the final record. Dropping the handle
/// does not stop the task; a failure to run or record it is also logged at `ERROR`.
pub async fn start_project_task(
    project: &ProjectEntry,
    task_id: &str,
    options: HashMap<String, serde_json::Value>,
) -> Result<(i64, JoinHandle<Result<TaskExecutionRecord, RunnerError>>), RunnerError> {
    let prepared = prepare_task(project, task_id, options).await?;
    let exec_id = record_start(&prepared).await?;
    let handle = tokio::spawn(async move {
        let task_id = prepared.task_id.clone();
        let result = execute(prepared, exec_id).await;
        if let Err(e) = &result {
            crate::logger::log("ERROR", "runner", &format!("Background task '{}' (exec_id {}): {}", task_id, exec_id, e));
        }
        result
    });
    Ok((exec_id, handle))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(path: &Path) -> ProjectEntry {
        ProjectEntry {
            id: 1,
            path: path.to_string_lossy().into_owned(),
            name: "demo".into(),
            description: None,
            is_favorite: false,
            is_archived: false,
            hidden_tabs: Vec::new(),
        }
    }

    const RULA: &str = r#"
[scripts.task.test]
command = "cargo"
args = ["test"]
cwd = "core"
options.release = { flag = "--release", type = "boolean", default = true }

[scripts.persistent.web]
command = "npm"
"#;

    #[tokio::test]
    async fn task_is_resolved_from_rula_with_option_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("rula.toml"), RULA).unwrap();
        let prepared = prepare_task(&project(tmp.path()), "test", HashMap::new()).await.unwrap();
        assert_eq!(prepared.command, "cargo");
        assert_eq!(prepared.args, ["test", "--release"]);
        assert_eq!(prepared.work_dir, tmp.path().join("core"));
        assert_eq!(prepared.command_line, "cargo test --release");
    }

    #[tokio::test]
    async fn services_and_undeclared_ids_are_not_tasks() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("rula.toml"), RULA).unwrap();
        for id in ["web", "missing"] {
            let err = prepare_task(&project(tmp.path()), id, HashMap::new()).await.err().unwrap();
            assert!(matches!(err, RunnerError::Script(ScriptError::NotDeclared { .. })), "{}: {}", id, err);
        }
    }
}
