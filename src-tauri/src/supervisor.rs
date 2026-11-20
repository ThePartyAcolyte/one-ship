use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::db::DatabaseError;
use crate::events::DaemonEvent;
use crate::kopeng::dna::generate_random_dna;
use crate::kopeng::prompt::build_agent_initial_prompt;

/// Failures of the native process supervisor (spawn, stdin delivery, lifecycle, registration).
///
/// Supervisor functions and daemon commands return this type (`api::ApiError`, AGENTS.md §9.3).
#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    /// No live process is registered under the canonical key.
    #[error("Process '{key}' is not running")]
    ProcessNotRunning { key: String },

    /// The path is not a registered project.
    #[error("'{path}' is not a registered project")]
    ProjectNotRegistered { path: String },

    /// The OS refused to start the child process.
    #[error("Failed to spawn {target}: {source}")]
    Spawn {
        target: String,
        #[source]
        source: std::io::Error,
    },

    /// The child started but reported no PID (it exited immediately).
    #[error("Spawned {target} reported no PID")]
    MissingPid { target: String },

    /// A service's port is held by a process ONE-SHIP did not start.
    #[error("Port {port} is already occupied by PID {pid} ('{cmdline}'). Release the port or terminate the conflicting process before starting service '{service_id}'.")]
    PortOccupied { port: u16, pid: u32, cmdline: String, service_id: String },

    /// The account requested for an agent is missing or its credentials cannot be decrypted.
    #[error("Account '{account_id}' is unusable for agent '{agent_id}': {details}")]
    AccountUnavailable { account_id: String, agent_id: String, details: String },

    /// The child's stdin pipe is closed.
    #[error("Failed to send input to '{key}': {details}")]
    InputDelivery { key: String, details: String },

    /// The agent is busy and its bounded input queue is full.
    #[error("Agent '{key}' is busy and already has {limit} queued messages; wait for it to finish")]
    InputQueueFull { key: String, limit: usize },

    /// The caller's arguments are invalid (empty key, missing directory, unknown runner).
    #[error("{0}")]
    InvalidRequest(String),

    /// Writing the project's runner MCP configs before an agent spawn failed.
    #[error("Project provisioning failed: {0}")]
    Provisioning(#[from] crate::kopeng::skill::SkillError),

    /// A `/proc` inspection (port occupant, run-id scan) failed.
    #[error(transparent)]
    ProcInspection(#[from] crate::port_inspect::PortInspectError),

    #[error(transparent)]
    Database(#[from] DatabaseError),

    /// The agent's runner/model/effort is not a pair its runner manifest declares.
    #[error(transparent)]
    Config(#[from] crate::config::ConfigError),

    /// The agent is not a persona or platform agent of the project.
    #[error("Agent '{agent_id}' is not declared in project '{project_path}'")]
    AgentNotDeclared { agent_id: String, project_path: String },

    #[error(transparent)]
    Registry(#[from] crate::registry::RegistryError),

    #[error(transparent)]
    Conversation(#[from] crate::conversation::ConversationError),

    /// The Brief that seeds a Native Session without history could not be built.
    #[error(transparent)]
    Brief(#[from] crate::brief::BriefError),

    #[error("Background task failed: {0}")]
    Task(String),

    /// The service declares a profiler engine that no valid manifest defines.
    #[error("Service '{service_id}' declares profiler '{profiler_id}', which has no valid manifest")]
    ProfilerNotFound { service_id: String, profiler_id: String },

    /// The service declares a profiler engine whose `check_command` fails.
    #[error("Service '{service_id}' declares profiler '{profiler_id}', which is not installed ('{check_command}' failed)")]
    ProfilerNotInstalled { service_id: String, profiler_id: String, check_command: String },

    /// The task is already running in this project.
    #[error("Task '{task_id}' is already running in project '{project_path}'")]
    TaskAlreadyRunning { task_id: String, project_path: String },

    #[error(transparent)]
    Profiler(#[from] crate::kopeng::profiler::ProfilerError),
}

/// Single-quotes `arg` for `/bin/sh` so it reaches the program as one argument.
fn shell_quote(arg: &str) -> String {
    let is_plain = !arg.is_empty() && arg.chars().all(|c| c.is_ascii_alphanumeric() || "-_./=:,@%+".contains(c));
    if is_plain {
        return arg.to_string();
    }
    format!("'{}'", arg.replace('\'', r"'\''"))
}

/// Command line for `/bin/sh -c`: `command` is shell syntax as declared (`npm run dev`), and each
/// argument is quoted so spaces or metacharacters in it are never re-split or expanded.
fn shell_command_line(command: &str, args: &[String]) -> String {
    let mut line = command.to_string();
    for arg in args {
        line.push(' ');
        line.push_str(&shell_quote(arg));
    }
    line
}

impl crate::api::ApiError for SupervisorError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::ProcessNotRunning { .. } | Self::ProjectNotRegistered { .. } | Self::AgentNotDeclared { .. } => StatusCode::NOT_FOUND,
            Self::PortOccupied { .. } | Self::InputQueueFull { .. } | Self::TaskAlreadyRunning { .. } => StatusCode::CONFLICT,
            Self::InvalidRequest(_)
            | Self::AccountUnavailable { .. }
            | Self::ProfilerNotFound { .. }
            | Self::ProfilerNotInstalled { .. } => StatusCode::BAD_REQUEST,
            Self::Profiler(e) => e.status(),
            Self::Spawn { .. }
            | Self::MissingPid { .. }
            | Self::InputDelivery { .. }
            | Self::Provisioning(_)
            | Self::ProcInspection(_)
            | Self::Brief(_)
            | Self::Task(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Database(e) => e.status(),
            Self::Config(e) => e.status(),
            Self::Registry(e) => e.status(),
            Self::Conversation(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::ProcessNotRunning { .. } => "SupervisorError::ProcessNotRunning",
            Self::ProjectNotRegistered { .. } => "SupervisorError::ProjectNotRegistered",
            Self::Spawn { .. } => "SupervisorError::Spawn",
            Self::MissingPid { .. } => "SupervisorError::MissingPid",
            Self::PortOccupied { .. } => "SupervisorError::PortOccupied",
            Self::AccountUnavailable { .. } => "SupervisorError::AccountUnavailable",
            Self::InputDelivery { .. } => "SupervisorError::InputDelivery",
            Self::InputQueueFull { .. } => "SupervisorError::InputQueueFull",
            Self::InvalidRequest(_) => "SupervisorError::InvalidRequest",
            Self::Provisioning(_) => "SupervisorError::Provisioning",
            Self::ProcInspection(_) => "SupervisorError::ProcInspection",
            Self::Database(e) => e.code(),
            Self::AgentNotDeclared { .. } => "SupervisorError::AgentNotDeclared",
            Self::Brief(_) => "SupervisorError::Brief",
            Self::Task(_) => "SupervisorError::Task",
            Self::ProfilerNotFound { .. } => "SupervisorError::ProfilerNotFound",
            Self::ProfilerNotInstalled { .. } => "SupervisorError::ProfilerNotInstalled",
            Self::TaskAlreadyRunning { .. } => "SupervisorError::TaskAlreadyRunning",
            Self::Profiler(e) => e.code(),
            Self::Config(e) => e.code(),
            Self::Registry(e) => e.code(),
            Self::Conversation(e) => e.code(),
        }
    }
}

/// Default capacity of the per-agent queued prompt buffer (`supervisor.max_pending_inputs_count`).
pub const DEFAULT_MAX_PENDING_INPUTS_COUNT: usize = 16;

fn get_supervisor_max_buffer_lines_count() -> usize {
    crate::config::resolve_effective_usize(None, None, "supervisor", "max_buffer_lines_count")
}

/// Upper bound of prompts queued while an agent is Working (AGENTS.md §8.3).
fn get_supervisor_max_pending_inputs_count() -> usize {
    crate::config::resolve_effective_usize(None, None, "supervisor", "max_pending_inputs_count")
}

fn get_supervisor_shutdown_grace_period_ms() -> u64 {
    crate::config::resolve_effective_u64(None, None, "supervisor", "shutdown_grace_period_ms")
}

/// Default for `profiler.engine_shutdown_grace_period_ms` (registered in `config.rs`).
pub const DEFAULT_ENGINE_SHUTDOWN_GRACE_PERIOD_MS: u64 = 15000;

/// Time a stopped run has between SIGINT and SIGKILL. A run wrapped by a profiler engine gets
/// `profiler.engine_shutdown_grace_period_ms`, since the engine writes its output file after the
/// run exits.
fn stop_grace_period_ms(has_profiler_engine: bool) -> u64 {
    if has_profiler_engine {
        return crate::config::resolve_effective_u64(None, None, "profiler", "engine_shutdown_grace_period_ms");
    }
    get_supervisor_shutdown_grace_period_ms()
}

/// Exit code of a finished child; a signal death is reported as `128 + signal`, as shells do.
fn exit_code_of(status: std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    status.code().or_else(|| status.signal().map(|s| 128 + s)).unwrap_or(-1)
}

/// Orderly stop of a service or task child (Spec 08): SIGINT to its process group so the run, and
/// a profiler engine wrapping it, can flush and exit; SIGKILL to the group after `grace_period_ms`
/// or once the child exits (descendants that outlived it). Returns the child's exit code.
async fn stop_child_in_order(child: &mut tokio::process::Child, pid: u32, grace_period_ms: u64, label: &str) -> i32 {
    #[cfg(target_os = "linux")]
    unsafe {
        kill_process_and_group(pid, libc::SIGINT);
    }
    let waited = tokio::time::timeout(tokio::time::Duration::from_millis(grace_period_ms), child.wait()).await;
    #[cfg(target_os = "linux")]
    unsafe {
        kill_process_and_group(pid, libc::SIGKILL);
    }
    let status = match waited {
        Ok(status) => status,
        Err(_) => {
            crate::logger::log("WARN", "supervisor", &format!("'{}' (PID {}) ignored SIGINT for {}ms; killed", label, pid, grace_period_ms));
            child.wait().await
        }
    };
    match status {
        Ok(st) => exit_code_of(st),
        Err(e) => {
            crate::logger::log("ERROR", "supervisor", &format!("Cannot reap '{}' (PID {}): {}", label, pid, e));
            -1
        }
    }
}

/// An adopted service (a live run of a previous daemon, or found on its port) to watch.
struct AdoptedWatch {
    key: String,
    pid: u32,
    pgid: u32,
    run_id: String,
    stop_grace_period_ms: u64,
}

/// Watches an adopted service, which is not our child: polls its liveness to record its exit, and
/// stops it in order when its cancel token fires.
fn spawn_adopted_service_watcher(watch: AdoptedWatch, status: Arc<RwLock<ProcessStatus>>, cancel_token: CancellationToken) {
    tokio::spawn(async move {
        let poll_step_ms = get_supervisor_process_poll_step_ms().max(1);
        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    stop_adopted_in_order(watch.pid, watch.pgid, watch.stop_grace_period_ms, &watch.key).await;
                    *status.write_safe("supervisor:adopted_service:status_stop") = ProcessStatus::Completed { exit_code: 0 };
                    crate::logger::log_failure("supervisor", "db_record_process_termination (adopted stop)", crate::db::db_record_process_termination(&watch.run_id, "stopped", None, epoch_now_ms(), None));
                    break;
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(poll_step_ms)) => {
                    #[cfg(target_os = "linux")]
                    let alive = unsafe {
                        libc::kill(watch.pid as i32, 0) == 0 || (watch.pgid > 0 && libc::kill(-(watch.pgid as i32), 0) == 0)
                    };
                    #[cfg(not(target_os = "linux"))]
                    let alive = true;
                    if !alive {
                        *status.write_safe("supervisor:adopted_service:status_exit") = ProcessStatus::Completed { exit_code: 0 };
                        crate::logger::log("INFO", "supervisor", &format!("Adopted service '{}' exited, state transitioned to Completed", watch.key));
                        // Not our child: its exit code is unknown.
                        crate::logger::log_failure("supervisor", "db_record_process_termination (adopted exit)", crate::db::db_record_process_termination(&watch.run_id, "completed", None, epoch_now_ms(), None));
                        break;
                    }
                }
            }
        }
    });
}

/// Orderly stop of an adopted service, which is not our child: SIGINT to its group, liveness polled
/// every `poll_step_ms` for `grace_period_ms`, then SIGKILL.
async fn stop_adopted_in_order(pid: u32, pgid: u32, grace_period_ms: u64, label: &str) {
    #[cfg(target_os = "linux")]
    unsafe {
        if pgid > 0 { let _ = libc::kill(-(pgid as i32), libc::SIGINT); }
        let _ = libc::kill(pid as i32, libc::SIGINT);
    }
    let poll_step_ms = get_supervisor_process_poll_step_ms().max(1);
    for _ in 0..(grace_period_ms / poll_step_ms).max(1) {
        #[cfg(target_os = "linux")]
        let alive = unsafe { libc::kill(pid as i32, 0) == 0 || (pgid > 0 && libc::kill(-(pgid as i32), 0) == 0) };
        #[cfg(not(target_os = "linux"))]
        let alive = false;
        if !alive {
            return;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(poll_step_ms)).await;
    }
    crate::logger::log("WARN", "supervisor", &format!("Adopted '{}' (PID {}) ignored SIGINT for {}ms; killed", label, pid, grace_period_ms));
    #[cfg(target_os = "linux")]
    unsafe {
        if pgid > 0 { let _ = libc::kill(-(pgid as i32), libc::SIGKILL); }
        let _ = libc::kill(pid as i32, libc::SIGKILL);
    }
}

fn get_supervisor_state_watcher_interval_ms() -> u64 {
    crate::config::resolve_effective_u64(None, None, "supervisor", "state_watcher_interval_ms")
}

fn get_supervisor_shutdown_step_delay_ms() -> u64 {
    crate::config::resolve_effective_u64(None, None, "supervisor", "shutdown_step_delay_ms")
}

fn get_supervisor_process_poll_step_ms() -> u64 {
    crate::config::resolve_effective_u64(None, None, "supervisor", "process_poll_step_ms")
}

pub(crate) trait RwLockSafe<T> {
    fn read_safe(&self, context: &str) -> std::sync::RwLockReadGuard<'_, T>;
    fn write_safe(&self, context: &str) -> std::sync::RwLockWriteGuard<'_, T>;
}

impl<T> RwLockSafe<T> for RwLock<T> {
    fn read_safe(&self, context: &str) -> std::sync::RwLockReadGuard<'_, T> {
        match self.read() {
            Ok(guard) => guard,
            Err(poisoned) => {
                crate::logger::log("WARN", "supervisor", &format!("Lock contention / poisoned RwLock on read ({}), recovering inner guard", context));
                poisoned.into_inner()
            }
        }
    }

    fn write_safe(&self, context: &str) -> std::sync::RwLockWriteGuard<'_, T> {
        match self.write() {
            Ok(guard) => guard,
            Err(poisoned) => {
                crate::logger::log("WARN", "supervisor", &format!("Lock contention / poisoned RwLock on write ({}), recovering inner guard", context));
                poisoned.into_inner()
            }
        }
    }
}

#[cfg(target_os = "linux")]
unsafe fn setup_child_process_group_and_deathsig(cmd: &mut tokio::process::Command) {
    cmd.pre_exec(|| {
        libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL as libc::c_ulong, 0, 0, 0);
        libc::setpgid(0, 0);
        Ok(())
    });
}

#[cfg(not(target_os = "linux"))]
unsafe fn setup_child_process_group_and_deathsig(_cmd: &mut tokio::process::Command) {}

#[cfg(target_os = "linux")]
unsafe fn kill_process_and_group(pid: u32, signal: i32) {
    if pid > 0 {
        // Signal the process group first (negative PID)
        let _ = libc::kill(-(pid as i32), signal);
        // Signal the direct PID as fallback
        let _ = libc::kill(pid as i32, signal);
    }
}

#[cfg(not(target_os = "linux"))]
unsafe fn kill_process_and_group(_pid: u32, _signal: i32) {}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    if !path.is_file() {
        return false;
    }
    match path.metadata() {
        Ok(meta) => meta.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

/// Target program resolved for process supervision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedExecutable {
    /// Absolute or relative path to the resolved executable binary.
    pub program: PathBuf,
    /// Indicates whether setting `cmd.arg0` is safe (i.e. the program is not a shim dispatcher).
    pub supports_custom_arg0: bool,
}

/// Checks whether an executable path is a known shim dispatcher (e.g. `mise`, `asdf`)
/// whose dispatch behavior relies strictly on `argv[0]`.
pub fn is_known_shim_dispatcher(path: &Path) -> bool {
    let lossy = path.to_string_lossy();
    if lossy.contains("/shims/") || lossy.ends_with("/shims") {
        return true;
    }
    if let Ok(target) = std::fs::canonicalize(path) {
        if let Some(file_name) = target.file_name().and_then(|n| n.to_str()) {
            if file_name == "mise" || file_name == "asdf" {
                return true;
            }
        }
    }
    false
}

fn resolve_via_mise_which(tool_name: &str) -> Option<PathBuf> {
    let output = std::process::Command::new("mise")
        .arg("which")
        .arg(tool_name)
        .output()
        .ok()?;
    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !text.is_empty() {
            let p = PathBuf::from(text);
            if is_executable_file(&p) && !is_known_shim_dispatcher(&p) {
                return Some(p);
            }
        }
    }
    None
}

/// Resolves a binary or command to its underlying executable file.
/// If the binary is managed by `mise` or exists behind shims, attempts to resolve
/// the real binary path so that process execution and process naming (`arg0`) operate safely.
pub fn resolve_executable(command: &str) -> ResolvedExecutable {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return ResolvedExecutable {
            program: PathBuf::from(command),
            supports_custom_arg0: true,
        };
    }

    let raw_path = Path::new(trimmed);
    // If an explicit path was provided:
    if raw_path.is_absolute() || trimmed.contains(std::path::MAIN_SEPARATOR) {
        let is_shim = is_known_shim_dispatcher(raw_path);
        // If it's a shim, check if mise can resolve the tool name
        if is_shim {
            if let Some(tool_name) = raw_path.file_name().and_then(|n| n.to_str()) {
                if let Some(resolved) = resolve_via_mise_which(tool_name) {
                    return ResolvedExecutable {
                        program: resolved,
                        supports_custom_arg0: true,
                    };
                }
            }
        }
        return ResolvedExecutable {
            program: raw_path.to_path_buf(),
            supports_custom_arg0: !is_shim,
        };
    }

    // Bare command name (e.g. "agy", "claude", "opencode", "node")
    // 1. Try mise which first if available
    if let Some(resolved) = resolve_via_mise_which(trimmed) {
        return ResolvedExecutable {
            program: resolved,
            supports_custom_arg0: true,
        };
    }

    // 2. Scan PATH for candidates, preferring non-shims over shims
    if let Some(paths) = std::env::var_os("PATH") {
        let mut first_shim: Option<PathBuf> = None;
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join(trimmed);
            if is_executable_file(&candidate) {
                if is_known_shim_dispatcher(&candidate) {
                    if first_shim.is_none() {
                        first_shim = Some(candidate);
                    }
                } else {
                    return ResolvedExecutable {
                        program: candidate,
                        supports_custom_arg0: true,
                    };
                }
            }
        }

        if let Some(shim) = first_shim {
            return ResolvedExecutable {
                program: shim,
                supports_custom_arg0: false,
            };
        }
    }

    ResolvedExecutable {
        program: PathBuf::from(trimmed),
        supports_custom_arg0: true,
    }
}

#[cfg(target_os = "linux")]
pub fn read_proc_environ_var(pid: u32, var_name: &str) -> Option<String> {
    let path = format!("/proc/{}/environ", pid);
    let bytes = std::fs::read(path).ok()?;
    let prefix = format!("{}=", var_name);
    for entry in bytes.split(|b| *b == 0) {
        let text = String::from_utf8_lossy(entry);
        if let Some(val) = text.strip_prefix(&prefix) {
            return Some(val.to_string());
        }
    }
    None
}

#[cfg(not(target_os = "linux"))]
pub fn read_proc_environ_var(_pid: u32, _var_name: &str) -> Option<String> {
    None
}

/// Summary report of boot-time orphan process reconciliation and service adoption.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ReconciliationSummary {
    pub orphans_killed_count: usize,
    pub stale_closed_count: usize,
    pub adopted_services_count: usize,
    pub reconciled_targets: Vec<String>,
}

/// Sweeps, adopts, and reconciles unclosed managed processes (Spec 01 / Spec 08).
/// Inspects /proc/<pid>/environ to attest process identity:
/// - Active persistent services from the previous session are ADOPTED without killing to preserve workflow continuity.
/// - Unclosed ephemeral tasks are terminated via SIGKILL to free resources.
/// - Dead process records are transitioned to 'crashed'.
pub fn reconcile_orphan_processes() -> Result<ReconciliationSummary, SupervisorError> {
    crate::logger::log("INFO", "supervisor", "Starting startup process reconciliation (Spec 01 / Spec 08)...");
    let unclosed = match crate::db::db_list_unclosed_supervised_processes() {
        Ok(list) => list,
        Err(e) => {
            crate::logger::log("WARN", "supervisor", &format!("Cannot reconcile orphan processes (DB uninitialized or locked): {}", e));
            return Ok(ReconciliationSummary::default());
        }
    };
    
    if unclosed.is_empty() {
        crate::logger::log("INFO", "supervisor", "Process reconciliation clean: zero unclosed processes in SQLite");
        return Ok(ReconciliationSummary::default());
    }

    crate::logger::log("WARN", "supervisor", &format!("Process reconciliation: auditing {} unclosed processes from previous session", unclosed.len()));
    let mut summary = ReconciliationSummary::default();
    let now_ms = epoch_now_ms();
    let sup = get_supervisor();
    let proc_run_map = crate::port_inspect::scan_proc_for_oneship_run_ids()?;

    for proc in unclosed {
        let direct_match = read_proc_environ_var(proc.pid, "ONESHIP_RUN_ID").as_deref() == Some(&proc.run_id);
        let living_children = proc_run_map.get(&proc.run_id).cloned().unwrap_or_default();
        #[cfg(target_os = "linux")]
        let group_alive = proc.pgid > 0 && unsafe { libc::kill(-(proc.pgid as i32), 0) == 0 };
        #[cfg(not(target_os = "linux"))]
        let group_alive = false;

        let is_alive = direct_match || !living_children.is_empty() || group_alive;

        if is_alive {
            let project_path = match crate::db::db_get_projects() {
                Ok(projects) => projects.into_iter().find(|p| p.id as i64 == proc.project_id).map(|p| p.path),
                Err(_) => None,
            };

            if let Some(p_path) = project_path.as_ref().filter(|_| proc.target_type == "service") {
                let effective_pid = if direct_match {
                    proc.pid
                } else if let Some(&c) = living_children.first() {
                    c
                } else {
                    proc.pgid
                };

                crate::logger::log("INFO", "supervisor", &format!(
                    "[RECOVERY] Adopting active persistent service '{}' (PID {}, PGID {}, {} active children) from previous session",
                    proc.target_id, effective_pid, proc.pgid, living_children.len()
                ));

                let key = ProcessSupervisor::format_service_key(&p_path, &proc.target_id);
                let buffer_lines_count = get_supervisor_max_buffer_lines_count();
                let status_arc = Arc::new(RwLock::new(ProcessStatus::Working));
                let mut ring = VecDeque::with_capacity(buffer_lines_count);
                ring.push_back(format!(
                    "[SYSTEM] Service re-attached from previous session (PID {}, PGID {}). Active monitoring resumed.",
                    effective_pid, proc.pgid
                ));
                let ring_arc = Arc::new(RwLock::new(ring));
                let cancel_token = CancellationToken::new();
                let (stdin_tx, _stdin_rx) = mpsc::channel::<String>(32);

                let managed = Arc::new(ManagedProcess {
                    key: key.clone(),
                    id: proc.target_id.clone(),
                    project_path: p_path.clone(),
                    pid: effective_pid,
                    status: status_arc.clone(),
                    stdin_tx,
                    cancel_token: cancel_token.clone(),
                    started_at_ms: proc.spawned_at_ms,
                    output_ring: ring_arc.clone(),
                    run_id: proc.run_id.clone(),
                    runner: None,
                    model: None,
                    effort: None,
                    account_id: None,
                    account_email: None,
                    is_interrupted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                    pending_correction: Arc::new(RwLock::new(None)),
                    pending_inputs: Arc::new(RwLock::new(VecDeque::new())),
                    pending_preamble: Arc::new(RwLock::new(None)),
                    stop_grace_period_ms: stop_grace_period_ms(proc.profiler_engine_id.is_some()),
                });
                let watch = AdoptedWatch {
                    key: key.clone(),
                    pid: effective_pid,
                    pgid: proc.pgid,
                    run_id: proc.run_id.clone(),
                    stop_grace_period_ms: managed.stop_grace_period_ms,
                };

                {
                    let mut procs = sup.processes.write_safe("supervisor:reconcile:adopt_service");
                    procs.insert(key.clone(), managed);
                }
                sup.store_retained_buffer(&key, ring_arc.read_safe("supervisor:reconcile:ring").clone());
                spawn_adopted_service_watcher(watch, status_arc.clone(), cancel_token);

                summary.adopted_services_count += 1;
                summary.reconciled_targets.push(format!("service:{} [ADOPTED] (PID {})", proc.target_id, effective_pid));
            } else {
                // Ephemeral task or unresolvable project -> terminate cleanly
                crate::logger::log("WARN", "supervisor", &format!(
                    "[RECOVERY] Detected active orphan {} '{}' (PID {}, PGID {}) from unexpected shutdown. Terminating process tree...",
                    proc.target_type, proc.target_id, proc.pid, proc.pgid
                ));

                #[cfg(target_os = "linux")]
                unsafe {
                    if proc.pgid > 0 { let _ = libc::kill(-(proc.pgid as i32), libc::SIGKILL); }
                    if proc.pid > 0 { let _ = libc::kill(proc.pid as i32, libc::SIGKILL); }
                    for &child_pid in &living_children {
                        let _ = libc::kill(child_pid as i32, libc::SIGKILL);
                    }
                }

                crate::logger::log_failure("supervisor", "db_record_process_termination (orphaned_killed)", crate::db::db_record_process_termination(
                    &proc.run_id,
                    "orphaned_killed",
                    Some(-1),
                    now_ms,
                    Some("Terminated orphan task process during startup recovery"),
                ));

                if let Some(ref p_path) = project_path {
                    let notice = format!("[SYSTEM] Process was terminated during startup recovery (previous session closed unexpectedly). Resources released. (PID {})", proc.pid);
                    let mut deque = VecDeque::new();
                    deque.push_back(notice);
                    let key = if proc.target_type == "task" {
                        ProcessSupervisor::format_task_key(p_path, &proc.target_id)
                    } else {
                        ProcessSupervisor::format_service_key(p_path, &proc.target_id)
                    };
                    sup.store_retained_buffer(&key, deque);
                }

                summary.orphans_killed_count += 1;
                summary.reconciled_targets.push(format!("{}:{} [KILLED] (PID {})", proc.target_type, proc.target_id, proc.pid));
            }
        } else {
            // Process is dead
            crate::logger::log("INFO", "supervisor", &format!(
                "[RECOVERY] Process {} '{}' (PID {}) is no longer running (or PID recycled). Marking row as crashed.",
                proc.target_type, proc.target_id, proc.pid
            ));
            crate::logger::log_failure("supervisor", "db_record_process_termination (crashed recovery)", crate::db::db_record_process_termination(
                &proc.run_id,
                "crashed",
                None,
                now_ms,
                Some("Process exited before startup recovery"),
            ));
            summary.stale_closed_count += 1;
        }
    }

    crate::logger::log("INFO", "supervisor", &format!(
        "Process reconciliation complete: {} services adopted, {} orphans terminated, {} stale records closed",
        summary.adopted_services_count, summary.orphans_killed_count, summary.stale_closed_count
    ));

    Ok(summary)
}

/// Lifecycle state machine for processes managed by the ProcessSupervisor.
/// Tracks execution from initial spawn through interactive idle/working loops to completion or failure.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum ProcessStatus {
    /// Process definition exists but execution has not been initiated.
    Unlaunched,
    /// Process command has been dispatched to the OS kernel but initial streams are not yet established.
    Starting,
    /// Headless interactive REPL is listening on stdin and ready to process commands.
    Idle,
    /// Process is actively executing a task or generating tool calls.
    Working,
    /// Process is awaiting human feedback, external approval, or blocked on IPC locks.
    Blocked,
    /// Process finished execution cleanly with the associated operating system exit code.
    Completed { exit_code: i32 },
    /// Process terminated abnormally or encountered an unrecoverable supervision failure.
    Failed { error: String },
}

impl Default for ProcessStatus {
    fn default() -> Self {
        ProcessStatus::Unlaunched
    }
}

impl ProcessStatus {
    pub fn as_str(&self) -> &str {
        match self {
            ProcessStatus::Unlaunched => "unlaunched",
            ProcessStatus::Starting => "starting",
            ProcessStatus::Idle => "idle",
            ProcessStatus::Working => "working",
            ProcessStatus::Blocked => "blocked",
            ProcessStatus::Completed { .. } => "completed",
            ProcessStatus::Failed { .. } => "failed",
        }
    }
}

/// In-memory projection of an agent specialist's runtime state, persona metadata, and capabilities.
#[derive(Serialize, Deserialize, Debug, Clone, Default, TS)]
#[ts(export)]
pub struct AgentState {
    pub id: String,
    pub role: String,
    pub runner: String,
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub accept_edits: Option<bool>,
    pub status: String,
    pub pid: Option<u32>,
    #[serde(default)]
    pub is_internal: bool,
    #[serde(default)]
    pub auto_approve: bool,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub context_docs: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub mcp_dialogue_mode: bool,
    pub dna: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub account_email: Option<String>,
}

/// Agents of one project with their live status (`get_project_agent_state`, `agent-states-updated`).
#[derive(Serialize, Deserialize, Debug, Clone, Default, TS)]
#[ts(export)]
pub struct ProjectAgentState {
    pub project_path: String,
    pub project_name: String,
    pub agents: Vec<AgentState>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TaskRunResult {
    pub exit_code: i32,
    pub duration_ms: i64,
    pub stdout: String,
    pub stderr: String,
    pub combined_logs: String,
    pub status: String,
}

pub struct ManagedProcess {
    pub key: String,
    pub id: String,
    pub project_path: String,
    pub pid: u32,
    pub status: Arc<RwLock<ProcessStatus>>,
    pub stdin_tx: mpsc::Sender<String>,
    pub cancel_token: CancellationToken,
    pub started_at_ms: u64,
    pub output_ring: Arc<RwLock<VecDeque<String>>>,
    pub run_id: String,
    pub runner: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub account_id: Option<String>,
    pub account_email: Option<String>,
    pub is_interrupted: Arc<std::sync::atomic::AtomicBool>,
    pub pending_correction: Arc<RwLock<Option<String>>>,
    pub pending_inputs: Arc<RwLock<VecDeque<String>>>,
    /// Persona Preamble (Spec 30) still to be delivered. Set at launch of a Native Session that
    /// has no history; consumed by the first `send_input`, which prepends it to that input.
    pub pending_preamble: Arc<RwLock<Option<String>>>,
    /// Time the process has to exit after `stop_process` signals it (`stop_grace_period_ms`).
    pub stop_grace_period_ms: u64,
}

/// What a process registry key holds (`ProcessSupervisor::key_kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessKeyKind {
    Service,
    Shell,
    Task,
}

/// Kind of a run the continuous profiler samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunTargetType {
    Service,
    Task,
}

impl RunTargetType {
    /// The `target_type` stored in `supervised_processes` and `process_samples`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Service => "service",
            Self::Task => "task",
        }
    }
}

/// A running service or task run (`ProcessSupervisor::running_runs`).
#[derive(Debug, Clone)]
pub struct RunningRun {
    pub project_path: String,
    pub target_type: RunTargetType,
    pub target_id: String,
    pub run_id: String,
    pub pid: u32,
}

/// Launch-time runner, model and account of a supervised process (`process_launch_identity`).
#[derive(Debug, Clone)]
pub struct ProcessLaunchIdentity {
    pub runner: Option<String>,
    pub model: Option<String>,
    pub account_id: Option<String>,
}

/// Message request for re-arming an interrupted agent into the idle state.
#[derive(Debug)]
pub struct AgentRearmRequest {
    pub project_path: String,
    pub project_name: String,
    pub agent_id: String,
    pub runner: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub account_id: Option<String>,
    pub correction: Option<String>,
}

/// Central process supervision and stream management hub.
/// Supervises long-running background services, deterministic tasks, and interactive multi-turn agent processes,
/// providing thread-safe I/O buffering via ring buffers, stdin piping, and event broadcasts.
pub struct ProcessSupervisor {
    processes: Arc<RwLock<HashMap<String, Arc<ManagedProcess>>>>,
    retained_buffers: Arc<RwLock<HashMap<String, VecDeque<String>>>>,
    event_tx: Arc<RwLock<Option<tokio::sync::broadcast::Sender<DaemonEvent>>>>,
    rearm_tx: tokio::sync::mpsc::Sender<AgentRearmRequest>,
    rearm_rx: Arc<tokio::sync::Mutex<Option<tokio::sync::mpsc::Receiver<AgentRearmRequest>>>>,
}

static SUPERVISOR: OnceLock<ProcessSupervisor> = OnceLock::new();

/// Returns the process-wide singleton instance of the `ProcessSupervisor`.
pub fn get_supervisor() -> &'static ProcessSupervisor {
    SUPERVISOR.get_or_init(|| {
        let (rearm_tx, rearm_rx) = tokio::sync::mpsc::channel::<AgentRearmRequest>(32);
        ProcessSupervisor {
            processes: Arc::new(RwLock::new(HashMap::new())),
            retained_buffers: Arc::new(RwLock::new(HashMap::new())),
            event_tx: Arc::new(RwLock::new(None)),
            rearm_tx,
            rearm_rx: Arc::new(tokio::sync::Mutex::new(Some(rearm_rx))),
        }
    })
}

/// Spawns the background worker task that processes agent re-arm requests.
/// Must be invoked during application runtime bootstrap (e.g. Tauri setup or daemon startup).
pub fn spawn_rearm_worker() {
    tauri::async_runtime::spawn(async move {
        let sup = get_supervisor();
        let mut rx_guard = sup.rearm_rx.lock().await;
        if let Some(mut rx) = rx_guard.take() {
            drop(rx_guard);
            crate::logger::log("INFO", "supervisor", "Started background agent re-arm worker loop");
            while let Some(req) = rx.recv().await {
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                let aid = req.agent_id.clone();
                let ppath = req.project_path.clone();
                let rearm_res = launch_agent(
                    req.project_path,
                    req.project_name,
                    req.agent_id.clone(),
                    req.runner,
                    req.model,
                    req.effort,
                    req.account_id,
                    Some(false),
                ).await;

                let sup = get_supervisor();
                match rearm_res {
                    Ok(_) => {
                        crate::logger::log("INFO", "supervisor", &format!("Successfully re-armed agent '{}' in idle state", aid));
                        if let Some(prompt) = req.correction {
                            crate::logger::log("INFO", "supervisor", &format!("Injecting pending corrective prompt to re-armed agent '{}'", aid));
                            tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
                            let key = ProcessSupervisor::format_agent_key(&ppath, &aid);
                            crate::logger::log_failure("supervisor", "send_input of rearm prompt", sup.send_input(&key, &prompt).await);
                        } else {
                            sup.emit_status_report(&aid, &ppath, "idle");
                            sup.emit_step_update(&aid, &ppath, None, "DONE", "result", None, "Execution stopped", None);
                        }
                    }
                    Err(e) => {
                        crate::logger::log("ERROR", "supervisor", &format!("Failed to re-arm agent '{}' after interrupt: {}", aid, e));
                    }
                }
            }
        }
    });
}

/// Reserved id of the orchestrator `system` agent, the only agent allowed to run outside a registered project.
pub const SYSTEM_AGENT_ID: &str = "system";

/// Determines the identity scope for an agent about to be spawned.
///
/// A registered project path yields a project-scoped identity with a stable persona uid.
/// An unregistered path is accepted only for [`SYSTEM_AGENT_ID`]; anything else is an error.
fn resolve_spawn_identity(
    project_path: &str,
    agent_id: &str,
) -> Result<(crate::db::AgentInstanceScope, Option<i64>, Option<String>), SupervisorError> {
    let project = crate::db::db_get_projects()?
        .into_iter()
        .find(|p| p.path == project_path);
    match project {
        Some(p) => {
            let project_id = p.id as i64;
            let agent_uid = crate::db::db_get_or_create_agent_persona(project_id, agent_id)?;
            Ok((crate::db::AgentInstanceScope::Project, Some(project_id), Some(agent_uid)))
        }
        None if agent_id == SYSTEM_AGENT_ID => Ok((crate::db::AgentInstanceScope::System, None, None)),
        None => Err(SupervisorError::ProjectNotRegistered { path: project_path.to_string() }),
    }
}

pub(crate) fn epoch_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Helper to spawn a background stream reader draining lines into an output ring buffer.
pub fn spawn_stream_reader<R>(
    reader: R,
    ring: Arc<RwLock<VecDeque<String>>>,
    cancel: CancellationToken,
    max_lines_count: usize,
    prefix: Option<&'static str>,
    on_line: Option<Arc<dyn Fn(&str) + Send + Sync + 'static>>,
    debug_label: String,
) -> tokio::task::JoinHandle<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                line_res = lines.next_line() => {
                    match line_res {
                        Ok(Some(line)) => {
                            crate::logger::log("DEBUG", "supervisor", &format!("Stream reader chunk arrival for '{}' ({} bytes)", debug_label, line.len()));
                            let formatted = match prefix {
                                Some(p) => format!("{} {}", p, line),
                                None => line.clone(),
                            };
                            {
                                let mut r = ring.write_safe(&format!("output_ring:{}", debug_label));
                                if r.len() >= max_lines_count {
                                    crate::logger::log("DEBUG", "supervisor", &format!("Output ring buffer full ({} lines) for '{}', dropping oldest line", max_lines_count, debug_label));
                                    r.pop_front();
                                }
                                r.push_back(formatted);
                            }
                            if let Some(ref cb) = on_line {
                                cb(&line);
                            }
                        }
                        Ok(None) => {
                            crate::logger::log("WARN", "supervisor", &format!("Child EOF received for stream '{}'", debug_label));
                            break;
                        }
                        Err(e) => {
                            crate::logger::log("WARN", "supervisor", &format!("Error reading stream for '{}' (broken pipe / read error): {:?}", debug_label, e));
                            break;
                        }
                    }
                }
            }
        }
        crate::logger::log("DEBUG", "supervisor", &format!("Stream EOF finished for '{}'", debug_label));
    })
}

/// Helper to spawn stdout and stderr readers for a spawned child process.
pub fn spawn_io_monitors(
    debug_label: &str,
    stdout: Option<tokio::process::ChildStdout>,
    stderr: Option<tokio::process::ChildStderr>,
    ring: Arc<RwLock<VecDeque<String>>>,
    cancel: CancellationToken,
    max_lines_count: usize,
) -> (Option<tokio::task::JoinHandle<()>>, Option<tokio::task::JoinHandle<()>>) {
    let stdout_handle = stdout.map(|out| {
        spawn_stream_reader(
            out,
            ring.clone(),
            cancel.clone(),
            max_lines_count,
            None,
            None,
            format!("{}:stdout", debug_label),
        )
    });
    let stderr_handle = stderr.map(|err| {
        spawn_stream_reader(
            err,
            ring,
            cancel,
            max_lines_count,
            Some("[STDERR]"),
            None,
            format!("{}:stderr", debug_label),
        )
    });
    (stdout_handle, stderr_handle)
}

impl ProcessSupervisor {
    pub fn format_agent_key(project_path: &str, agent_id: &str) -> String {
        format!("{}:{}", project_path, agent_id)
    }

    pub fn format_service_key(project_path: &str, service_id: &str) -> String {
        format!("{}:svc:{}", project_path, service_id)
    }

    pub fn format_task_key(project_path: &str, task_id: &str) -> String {
        format!("{}:tsk:{}", project_path, task_id)
    }

    pub fn set_event_tx(&self, tx: tokio::sync::broadcast::Sender<DaemonEvent>) {
        let mut lock = self.event_tx.write_safe("supervisor:set_event_tx");
        *lock = Some(tx);
    }

    pub fn emit_event(&self, event: DaemonEvent) {
        let lock = self.event_tx.read_safe("supervisor:emit_event");
        if let Some(ref tx) = *lock {
            let _ = tx.send(event);
        }
    }

    fn emit_status_report(&self, agent_id: &str, project_path: &str, status: &str) {
        self.emit_event(DaemonEvent::agent_status_report(&crate::events::AgentStatusReportPayload {
            agent_id: agent_id.to_string(),
            project_path: Some(project_path.to_string()),
            status: status.to_string(),
            summary: None,
            details: None,
            task_id: None,
            needs_user_input: None,
            timestamp: (epoch_now_ms() / 1000) as i64,
        }));
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_step_update(
        &self,
        agent_id: &str,
        project_path: &str,
        conversation_id: Option<&str>,
        state: &str,
        step_type: &str,
        tool_name: Option<&str>,
        action_summary: &str,
        thought_text: Option<&str>,
    ) {
        self.emit_event(DaemonEvent::agent_step_update(&crate::events::AgentStepUpdatePayload {
            agent_id: agent_id.to_string(),
            project_path: project_path.to_string(),
            conversation_id: conversation_id.map(|s| s.to_string()),
            state: state.to_string(),
            step_type: step_type.to_string(),
            tool_name: tool_name.map(|s| s.to_string()),
            action_summary: action_summary.to_string(),
            thought_text: thought_text.map(|s| s.to_string()),
            timestamp_ms: epoch_now_ms(),
        }));
    }

    fn emit_agent_error(&self, agent_id: &str, project_path: &str, error_type: &str, message: &str, exit_code: i32) {
        self.emit_event(DaemonEvent::agent_error(&crate::events::AgentErrorPayload {
            agent_id: agent_id.to_string(),
            project_path: project_path.to_string(),
            error_type: error_type.to_string(),
            message: message.to_string(),
            exit_code,
            timestamp_ms: epoch_now_ms(),
        }));
    }

    /// What a registry key of `project_path` holds: a service, an interactive shell or a task run;
    /// `None` for an agent.
    fn key_kind(project_path: &str, key: &str) -> Option<ProcessKeyKind> {
        let rest = key.strip_prefix(project_path)?.strip_prefix(':')?;
        if let Some(service_id) = rest.strip_prefix("svc:") {
            let is_shell = service_id == "shell" || service_id.starts_with(SHELL_ID_PREFIX);
            return Some(if is_shell { ProcessKeyKind::Shell } else { ProcessKeyKind::Service });
        }
        rest.starts_with("tsk:").then_some(ProcessKeyKind::Task)
    }

    /// The service and task runs that are running now, for the continuous profiler and the service
    /// telemetry. Shells and agents are not runs.
    pub fn running_runs(&self) -> Vec<RunningRun> {
        let procs = self.processes.read_safe("supervisor:running_runs");
        procs.values()
            .filter(|p| *p.status.read_safe("supervisor:running_runs:status") == ProcessStatus::Working)
            .filter_map(|p| {
                let target_type = match Self::key_kind(&p.project_path, &p.key)? {
                    ProcessKeyKind::Service => RunTargetType::Service,
                    ProcessKeyKind::Task => RunTargetType::Task,
                    ProcessKeyKind::Shell => return None,
                };
                Some(RunningRun {
                    project_path: p.project_path.clone(),
                    target_type,
                    target_id: p.id.clone(),
                    run_id: p.run_id.clone(),
                    pid: p.pid,
                })
            })
            .collect()
    }

    /// Runner, model and account recorded when the process at `key` was spawned; `None` when no process has this key.
    pub fn process_launch_identity(&self, key: &str) -> Option<ProcessLaunchIdentity> {
        let procs = self.processes.read_safe("supervisor:process_launch_identity");
        procs.get(key).map(|p| ProcessLaunchIdentity {
            runner: p.runner.clone(),
            model: p.model.clone(),
            account_id: p.account_id.clone(),
        })
    }

    pub fn get_status(&self, key: &str) -> ProcessStatus {
        let procs = self.processes.read_safe("supervisor:get_status");
        if let Some(proc) = procs.get(key) {
            proc.status.read_safe("supervisor:proc.status").clone()
        } else {
            ProcessStatus::Unlaunched
        }
    }

    pub fn read_output(&self, key: &str, max_lines_count: usize) -> Vec<String> {
        let procs = self.processes.read_safe("supervisor:read_output");
        if let Some(proc) = procs.get(key) {
            let ring = proc.output_ring.read_safe("supervisor:read_output:ring");
            let count = max_lines_count.min(ring.len());
            ring.iter().rev().take(count).cloned().collect::<Vec<_>>().into_iter().rev().collect()
        } else {
            let retained = self.retained_buffers.read_safe("supervisor:read_output:retained");
            if let Some(ring) = retained.get(key) {
                let count = max_lines_count.min(ring.len());
                ring.iter().rev().take(count).cloned().collect::<Vec<_>>().into_iter().rev().collect()
            } else {
                Vec::new()
            }
        }
    }

    /// Stores output lines into the retained buffer map so that terminal output survives process termination or re-arming.
    pub fn store_retained_buffer(&self, key: &str, lines: VecDeque<String>) {
        if lines.is_empty() {
            return;
        }
        let mut retained = self.retained_buffers.write_safe("supervisor:store_retained_buffer");
        // Limit map size to 100 process keys to prevent unbounded RAM growth
        if retained.len() >= 100 && !retained.contains_key(key) {
            if let Some(oldest_key) = retained.keys().next().cloned() {
                retained.remove(&oldest_key);
            }
        }
        retained.insert(key.to_string(), lines);
    }

    /// Retrieves a cloned copy of the retained buffer for a key, capped at `max_lines_count`.
    pub fn get_retained_buffer(&self, key: &str, max_lines_count: usize) -> Option<VecDeque<String>> {
        let retained = self.retained_buffers.read_safe("supervisor:get_retained_buffer");
        retained.get(key).map(|lines| {
            let mut copy = lines.clone();
            while copy.len() > max_lines_count {
                copy.pop_front();
            }
            copy
        })
    }

    /// Clears the retained buffer for a key (used on explicit clean context requests).
    pub fn clear_retained_buffer(&self, key: &str) {
        let mut retained = self.retained_buffers.write_safe("supervisor:clear_retained_buffer");
        retained.remove(key);
    }

    pub fn get_agent_key_by_pid(&self, pid: u32) -> Option<String> {
        let procs = self.processes.read_safe("supervisor:get_agent_key_by_pid");
        procs.values().find(|p| p.pid == pid).map(|p| p.key.clone())
    }

    pub fn list_active_processes(&self) -> Vec<serde_json::Value> {
        let procs = self.processes.read_safe("supervisor:list_active_processes");
        let mut list = Vec::with_capacity(procs.len());
        for p in procs.values() {
            list.push(serde_json::json!({
                "key": p.key,
                "id": p.id,
                "project_path": p.project_path,
                "pid": p.pid,
                "status": p.status.read_safe("supervisor:list_active_processes:status").as_str(),
                "started_at_ms": p.started_at_ms,
            }));
        }
        list
    }

    /// IDs of the project's `svc` processes in `Working` state, read from the live process
    /// registry: interactive shells (`SHELL_ID_PREFIX`) when `shells` is true, services otherwise.
    fn working_svc_ids(&self, project_path: &str, shells: bool) -> Vec<String> {
        let prefix = Self::format_service_key(project_path, "");
        let procs = self.processes.read_safe("supervisor:working_svc_ids");
        let mut ids: Vec<String> = procs
            .iter()
            .filter_map(|(key, p)| Some((key.strip_prefix(&prefix)?, p)))
            .filter(|(id, _)| id.starts_with(SHELL_ID_PREFIX) == shells)
            .filter(|(_, p)| *p.status.read_safe("supervisor:working_svc_ids:status") == ProcessStatus::Working)
            .map(|(id, _)| id.to_string())
            .collect();
        ids.sort();
        ids
    }

    /// IDs of the project's running services (interactive shells excluded), sorted.
    pub fn running_service_ids(&self, project_path: &str) -> Vec<String> {
        self.working_svc_ids(project_path, false)
    }

    /// IDs of the project's running interactive shells, sorted (`shell_<epoch_ms>`, so by launch time).
    pub fn running_shell_ids(&self, project_path: &str) -> Vec<String> {
        self.working_svc_ids(project_path, true)
    }

    /// PID of a running service; `None` once it has exited, even while its entry is kept.
    pub fn get_service_pid(&self, project_path: &str, service_id: &str) -> Option<u32> {
        let key = Self::format_service_key(project_path, service_id);
        let procs = self.processes.read_safe("supervisor:get_service_pid");
        procs.get(&key)
            .filter(|p| *p.status.read_safe("supervisor:get_service_pid:status") == ProcessStatus::Working)
            .map(|p| p.pid)
    }

    /// Registers the Persona Preamble to be prepended to the next input sent to `key`.
    pub fn set_pending_preamble(&self, key: &str, preamble: String) -> Result<(), SupervisorError> {
        let procs = self.processes.read_safe("supervisor:set_pending_preamble");
        let process = procs.get(key).ok_or_else(|| SupervisorError::ProcessNotRunning { key: key.to_string() })?;
        crate::logger::log("DEBUG", "supervisor", &format!("set_pending_preamble: {} bytes for '{}'", preamble.len(), key));
        *process.pending_preamble.write_safe("supervisor:set_pending_preamble:write") = Some(preamble);
        Ok(())
    }

    /// Runner a running process was launched with.
    pub fn process_runner(&self, key: &str) -> Option<String> {
        let procs = self.processes.read_safe("supervisor:process_runner");
        procs.get(key).and_then(|p| p.runner.clone())
    }

    /// Appends `context` (a Brief) to what the next input to `key` will carry, after any Persona
    /// Preamble still pending. Returns the runner of the running process.
    pub fn queue_context_for_next_input(&self, key: &str, context: &str) -> Result<Option<String>, SupervisorError> {
        let procs = self.processes.read_safe("supervisor:queue_context_for_next_input");
        let process = procs.get(key).ok_or_else(|| SupervisorError::ProcessNotRunning { key: key.to_string() })?;
        let mut pending = process.pending_preamble.write_safe("supervisor:queue_context_for_next_input:write");
        *pending = Some(match pending.take() {
            Some(existing) => format!("{}\n\n---\n\n{}", existing, context),
            None => context.to_string(),
        });
        crate::logger::log("DEBUG", "supervisor", &format!("queue_context_for_next_input: {} bytes queued for '{}'", context.len(), key));
        Ok(process.runner.clone())
    }

    pub async fn send_input(&self, key: &str, input: &str) -> Result<(), SupervisorError> {
        crate::logger::log("DEBUG", "supervisor", &format!("send_input: dispatching {} bytes to process '{}'", input.len(), key));
        let (tx, status_arc, pending_inputs_arc, preamble_arc) = {
            let procs = self.processes.read_safe("supervisor:send_input");
            procs.get(key)
                .map(|p| (Some(p.stdin_tx.clone()), Some(p.status.clone()), Some(p.pending_inputs.clone()), Some(p.pending_preamble.clone())))
                .unwrap_or((None, None, None, None))
        };

        if let Some(tx) = tx {
            // The first input after a launch without history carries the Persona Preamble.
            let preamble = preamble_arc.as_ref().and_then(|a| a.write_safe("supervisor:send_input:take_preamble").take());
            let delivered_input = match preamble.as_deref() {
                Some(p) => format!("{}\n\n---\n\nUSER MESSAGE:\n{}", p, input),
                None => input.to_string(),
            };
            let input = delivered_input.as_str();
            let restore_preamble = |failed: bool| {
                if failed {
                    if let (Some(p), Some(a)) = (preamble.clone(), preamble_arc.as_ref()) {
                        *a.write_safe("supervisor:send_input:restore_preamble") = Some(p);
                    }
                }
            };

            let is_working = {
                if let Some(ref st) = status_arc {
                    *st.read_safe("supervisor:send_input:status") == ProcessStatus::Working
                } else {
                    false
                }
            };

            // Non-disruptive queueing: If process is currently Working, enqueue input to avoid turn collisions
            if is_working {
                if let Some(ref q) = pending_inputs_arc {
                    let max_pending_inputs_count = get_supervisor_max_pending_inputs_count();
                    let mut queue = q.write_safe("supervisor:send_input:queue");
                    if queue.len() >= max_pending_inputs_count {
                        drop(queue);
                        restore_preamble(true);
                        crate::logger::log("WARN", "supervisor", &format!(
                            "Process '{}' is Working and its input queue is full ({} items); input rejected", key, max_pending_inputs_count
                        ));
                        return Err(SupervisorError::InputQueueFull { key: key.to_string(), limit: max_pending_inputs_count });
                    }
                    queue.push_back(input.to_string());
                    let depth = queue.len();
                    crate::logger::log("INFO", "supervisor", &format!("Process '{}' is Working; enqueued input (queue depth: {})", key, depth));
                    return Ok(());
                }
            }

            if let Err(e) = tx.send(input.to_string()).await {
                restore_preamble(true);
                crate::logger::log("WARN", "supervisor", &format!("Broken pipe sending input to '{}': {}", key, e));
                return Err(SupervisorError::InputDelivery { key: key.to_string(), details: e.to_string() });
            }
            if let Some(st) = status_arc {
                *st.write_safe("supervisor:send_input:set_working") = ProcessStatus::Working;
            }
            Ok(())
        } else {
            crate::logger::log("WARN", "supervisor", &format!("send_input failed: process '{}' is not running", key));
            Err(SupervisorError::ProcessNotRunning { key: key.to_string() })
        }
    }

    /// Dispatches the next queued prompt for the process if any are pending, transitioning it back to Working.
    pub async fn dispatch_next_pending_input(&self, key: &str) -> Result<bool, SupervisorError> {
        let (tx, status_arc, pending_inputs_arc) = {
            let procs = self.processes.read_safe("supervisor:dispatch_next_pending_input");
            procs.get(key)
                .map(|p| (Some(p.stdin_tx.clone()), Some(p.status.clone()), Some(p.pending_inputs.clone())))
                .unwrap_or((None, None, None))
        };

        if let (Some(tx), Some(st), Some(q)) = (tx, status_arc, pending_inputs_arc) {
            let next_item = q.write_safe("supervisor:dispatch_next:pop").pop_front();
            if let Some(input) = next_item {
                crate::logger::log("INFO", "supervisor", &format!("Dispatching queued input ({} bytes) to process '{}'", input.len(), key));
                *st.write_safe("supervisor:dispatch_next:set_working") = ProcessStatus::Working;
                tx.send(input)
                    .await
                    .map_err(|e| SupervisorError::InputDelivery { key: key.to_string(), details: e.to_string() })?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub async fn stop_all_processes(&self) {
        let keys: Vec<String> = {
            let procs = self.processes.read_safe("supervisor:stop_all_processes");
            procs.keys().cloned().collect()
        };
        crate::logger::log("INFO", "supervisor", &format!("Stopping all {} active supervised processes...", keys.len()));
        // Concurrently: each process gets its own grace period, and the longest one (a run wrapped
        // by a profiler engine) bounds the whole drain, within the daemon unit's stop timeout.
        futures_util::future::join_all(keys.iter().map(|key| self.stop_process(key))).await;
    }

    pub async fn stop_process(&self, key: &str) -> Result<(), SupervisorError> {
        crate::logger::log("DEBUG", "supervisor", &format!("Initiating termination for process '{}'", key));
        let proc = {
            let mut procs = self.processes.write_safe("supervisor:stop_process");
            procs.remove(key)
        };

        if let Some(proc) = proc {
            // Retain the output ring so it remains queryable post-termination
            let mut existing_lines = proc.output_ring.read_safe("supervisor:stop_process:ring").clone();
            let divider = format!("[SYSTEM] Process stopped (PID {})", proc.pid);
            let max_lines = get_supervisor_max_buffer_lines_count();
            if existing_lines.len() >= max_lines {
                existing_lines.pop_front();
            }
            existing_lines.push_back(divider);
            self.store_retained_buffer(key, existing_lines);

            debug_assert!(proc.pid > 0, "ManagedProcess PID must be non-zero");
            proc.cancel_token.cancel();

            // The process's own watcher signals it, waits its grace period and reaps it; this wait
            // covers that plus two poll steps for the reap, and only then kills it here.
            let grace_period_ms = proc.stop_grace_period_ms;
            let poll_step_ms = get_supervisor_process_poll_step_ms().max(1);
            let max_attempts_count = (grace_period_ms / poll_step_ms).max(1) + 2;

            let mut terminated = false;
            for _ in 0..max_attempts_count {
                let st = proc.status.read_safe("supervisor:stop_process:status").clone();
                if matches!(st, ProcessStatus::Completed { .. } | ProcessStatus::Failed { .. }) {
                    terminated = true;
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(poll_step_ms)).await;
            }

            if !terminated {
                crate::logger::log("WARN", "supervisor", &format!("Orphan process '{}' (PID {}) did not exit within {}ms grace period; cleaned up via SIGKILL", key, proc.pid, grace_period_ms));
                #[cfg(target_os = "linux")]
                unsafe {
                    kill_process_and_group(proc.pid, libc::SIGKILL);
                }
                *proc.status.write_safe("supervisor:stop_process:status_kill") = ProcessStatus::Completed { exit_code: 0 };
                crate::logger::log("INFO", "supervisor", &format!("Process '{}' state transition: forced to Completed", key));
            } else {
                #[cfg(target_os = "linux")]
                unsafe {
                    kill_process_and_group(proc.pid, libc::SIGKILL);
                }
            }

            crate::logger::log("INFO", "supervisor", &format!("Terminated process '{}' (PID {})", key, proc.pid));
            // A no-op when the watcher already recorded the exit code (the first record wins).
            crate::logger::log_failure("supervisor", "db_record_process_termination (stop)", crate::db::db_record_process_termination(&proc.run_id, "stopped", None, epoch_now_ms(), None));

            if Self::key_kind(&proc.project_path, key).is_none() {
                self.emit_status_report(&proc.id, &proc.project_path, "unlaunched");
                self.emit_step_update(&proc.id, &proc.project_path, None, "DONE", "result", None, "Process terminated", None);
            }

            Ok(())
        } else {
            crate::logger::log("DEBUG", "supervisor", &format!("Process '{}' not found in active processes registry", key));
            Ok(())
        }
    }

    /// Helper to extract an expected TCP port from rula configuration or command line arguments.
    fn detect_script_port(rula_port: Option<u16>, command: &str, args: &[String]) -> Option<u16> {
        if let Some(p) = rula_port {
            return Some(p);
        }
        for i in 0..args.len() {
            if (args[i] == "--port" || args[i] == "-p") && i + 1 < args.len() {
                if let Ok(p) = args[i + 1].parse::<u16>() {
                    return Some(p);
                }
            }
            if let Some(stripped) = args[i].strip_prefix("--port=") {
                if let Ok(p) = stripped.parse::<u16>() {
                    return Some(p);
                }
            }
        }
        let parts: Vec<&str> = command.split_whitespace().collect();
        for i in 0..parts.len() {
            if (parts[i] == "--port" || parts[i] == "-p") && i + 1 < parts.len() {
                if let Ok(p) = parts[i + 1].parse::<u16>() {
                    return Some(p);
                }
            }
            if let Some(stripped) = parts[i].strip_prefix("--port=") {
                if let Ok(p) = stripped.parse::<u16>() {
                    return Some(p);
                }
            }
        }
        None
    }

    /// The manifest of `engine_id` as `project_path` sees it, checked to be installed.
    async fn resolve_profiler_engine(&self, project_path: &Path, service_id: &str, engine_id: &str) -> Result<crate::kopeng::profiler::ProfilerManifest, SupervisorError> {
        let discovery_path = project_path.to_path_buf();
        let discovery = tokio::task::spawn_blocking(move || crate::kopeng::profiler::discover_profilers(&discovery_path))
            .await
            .map_err(|e| SupervisorError::Task(e.to_string()))??;
        let manifest = discovery.profilers.into_iter()
            .map(|p| p.manifest)
            .find(|m| m.plugin.id == engine_id)
            .ok_or_else(|| SupervisorError::ProfilerNotFound { service_id: service_id.to_string(), profiler_id: engine_id.to_string() })?;
        let check_command = manifest.requirements.check_command.clone();
        let probe = check_command.clone();
        let installed = tokio::task::spawn_blocking(move || crate::kopeng::profiler::check_tool_installed(&probe))
            .await
            .map_err(|e| SupervisorError::Task(e.to_string()))?;
        if !installed {
            return Err(SupervisorError::ProfilerNotInstalled { service_id: service_id.to_string(), profiler_id: engine_id.to_string(), check_command });
        }
        Ok(manifest)
    }

    /// Starts (or restarts) the service `service_id` of `project_path`.
    ///
    /// `port` is the declared port (`[scripts.persistent.<id>] port`, else a `--port`/`-p`
    /// argument): a live process of the same service on it is adopted, any other occupant is
    /// `PortOccupied`. `profiler_engine` is the declared profiler engine (`rula.toml` `profiler`),
    /// which must have a valid manifest and be installed: the run is launched wrapped by it, and
    /// its output file is linked to the run. A `command` with spaces is shell syntax run by
    /// `/bin/sh -c` with each argument quoted; otherwise the program is executed directly.
    #[allow(clippy::too_many_arguments)]
    pub async fn spawn_service(
        &self,
        project_path: &str,
        service_id: &str,
        command: &str,
        args: &[String],
        cwd: Option<&str>,
        port: Option<u16>,
        profiler_engine: Option<&str>,
    ) -> Result<u32, SupervisorError> {
        let key = Self::format_service_key(project_path, service_id);
        
        // Stop existing process if already running
        self.stop_process(&key).await?;

        let eff_cwd = if let Some(sub) = cwd {
            if sub != "." && !sub.is_empty() {
                Path::new(project_path).join(sub)
            } else {
                PathBuf::from(project_path)
            }
        } else {
            PathBuf::from(project_path)
        };

        let started_at_ms = epoch_now_ms();
        let run_id = format!("svc_{}_{}", service_id, started_at_ms);

        let p_path = Path::new(project_path);
        let p_name = p_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project");

        // Pre-flight port detection and auto-adoption / conflict guard
        let target_port = Self::detect_script_port(port, command, args);

        if let Some(port) = target_port {
            let occupant = tokio::task::spawn_blocking(move || crate::port_inspect::find_port_listener(port))
                .await
                .map_err(|e| SupervisorError::Task(e.to_string()))??;
            if let Some(occupant) = occupant {
                let same_service_run_id = occupant.oneship_run_id
                    .clone()
                    .filter(|r| r.starts_with(&format!("svc_{}_", service_id)));

                if let Some(occ_run_id) = same_service_run_id {
                    crate::logger::log("INFO", "supervisor", &format!(
                        "Service '{}' is already running on port {} (PID {}, PGID {}). Auto-adopting active process.",
                        service_id, port, occupant.pid, occupant.pgid
                    ));

                    let buffer_lines_count = get_supervisor_max_buffer_lines_count();
                    let status_arc = Arc::new(RwLock::new(ProcessStatus::Working));
                    let mut ring = VecDeque::with_capacity(buffer_lines_count);
                    ring.push_back(format!(
                        "[SYSTEM] Service re-attached to existing process on port {} (PID {}, PGID {}).",
                        port, occupant.pid, occupant.pgid
                    ));
                    let ring_arc = Arc::new(RwLock::new(ring));
                    let cancel_token = CancellationToken::new();
                    let (stdin_tx, _stdin_rx) = mpsc::channel::<String>(32);
                    let has_profiler_engine = crate::db::db_get_target_run(&occ_run_id)?
                        .is_some_and(|run| run.profiler_engine_id.is_some());

                    let managed = Arc::new(ManagedProcess {
                        key: key.clone(),
                        id: service_id.to_string(),
                        project_path: project_path.to_string(),
                        pid: occupant.pid,
                        status: status_arc.clone(),
                        stdin_tx,
                        cancel_token: cancel_token.clone(),
                        started_at_ms,
                        output_ring: ring_arc.clone(),
                        run_id: occ_run_id.clone(),
                        runner: None,
                        model: None,
                        effort: None,
                        account_id: None,
                        account_email: None,
                        is_interrupted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                        pending_correction: Arc::new(RwLock::new(None)),
                        pending_inputs: Arc::new(RwLock::new(VecDeque::new())),
                        pending_preamble: Arc::new(RwLock::new(None)),
                        stop_grace_period_ms: stop_grace_period_ms(has_profiler_engine),
                    });
                    let watch = AdoptedWatch {
                        key: key.clone(),
                        pid: occupant.pid,
                        pgid: occupant.pgid,
                        run_id: occ_run_id,
                        stop_grace_period_ms: managed.stop_grace_period_ms,
                    };

                    {
                        let mut procs = self.processes.write_safe("supervisor:spawn_service:adopt_insert");
                        procs.insert(key.clone(), managed);
                    }
                    self.store_retained_buffer(&key, ring_arc.read_safe("supervisor:adopt:ring").clone());
                    spawn_adopted_service_watcher(watch, status_arc.clone(), cancel_token);

                    return Ok(occupant.pid);
                } else {
                    return Err(SupervisorError::PortOccupied {
                        port,
                        pid: occupant.pid,
                        cmdline: occupant.cmdline.clone(),
                        service_id: service_id.to_string(),
                    });
                }
            }
        }

        let engine = match profiler_engine {
            Some(engine_id) => Some(self.resolve_profiler_engine(p_path, service_id, engine_id).await?),
            None => None,
        };
        let profile_artifact_path = engine.as_ref().map(|m| m.output_rel_path(&run_id));
        let (final_command, final_args) = match (&engine, &profile_artifact_path) {
            (Some(manifest), Some(rel_path)) => {
                let output_file = p_path.join(rel_path);
                if let Some(dir) = output_file.parent() {
                    tokio::fs::create_dir_all(dir).await.map_err(|source| SupervisorError::Spawn {
                        target: format!("profiler output dir {}", dir.display()),
                        source,
                    })?;
                }
                let wrapped = manifest.wrap_command(
                    &shell_command_line(command, args),
                    &shell_quote(&output_file.to_string_lossy()),
                );
                crate::logger::log("INFO", "supervisor", &format!("Service '{}' wrapped by profiler '{}': {}", service_id, manifest.plugin.id, wrapped));
                (wrapped, Vec::new())
            }
            _ => (command.to_string(), args.to_vec()),
        };

        let (mut cmd, supports_custom_arg0) = if final_command.contains(' ') {
            let mut c = tokio::process::Command::new("/bin/sh");
            c.arg("-c").arg(shell_command_line(&final_command, &final_args));
            (c, true)
        } else {
            let cmd_str = final_command.clone();
            let resolved = tokio::task::spawn_blocking(move || resolve_executable(&cmd_str))
                .await
                .unwrap_or_else(|_| ResolvedExecutable { program: std::path::PathBuf::from(&final_command), supports_custom_arg0: false });
            let mut c = tokio::process::Command::new(&resolved.program);
            c.args(&final_args);
            (c, resolved.supports_custom_arg0)
        };
        cmd.current_dir(&eff_cwd);
        cmd.env("ONESHIP_RUN_ID", &run_id);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        #[cfg(unix)]
        {
            if supports_custom_arg0 {
                cmd.arg0(format!("oneship-svc[{}:{}]", p_name, service_id));
            }
        }

        unsafe {
            setup_child_process_group_and_deathsig(&mut cmd);
        }

        crate::logger::log("DEBUG", "supervisor", &format!("Spawning service '{}' in '{}' with command: '{}', args: {:?}", service_id, eff_cwd.display(), command, args));
        let mut child = cmd.spawn().map_err(|e| {
            crate::logger::log("ERROR", "supervisor", &format!("Failed to spawn service '{}': {}", service_id, e));
            SupervisorError::Spawn { target: format!("service '{}'", service_id), source: e }
        })?;
        let pid = child.id().ok_or_else(|| SupervisorError::MissingPid { target: format!("service '{}'", service_id) })?;
        debug_assert!(pid > 0, "Spawned service PID must be non-zero");

        let target_type = if service_id == "shell" || service_id.starts_with("shell_") {
            "shell"
        } else {
            "service"
        };

        if let Ok(Some(proj)) = crate::db::db_get_project_by_path(project_path) {
            let proc_rec = crate::db::SupervisedProcessRecord {
                run_id: run_id.clone(),
                project_id: proj.id as i64,
                target_id: service_id.to_string(),
                target_type: target_type.to_string(),
                command: command.to_string(),
                args_json: serde_json::to_string(args).unwrap_or_else(|_| "[]".to_string()),
                cwd: Some(eff_cwd.to_string_lossy().to_string()),
                pid,
                pgid: pid,
                env_identifier: format!("ONESHIP_RUN_ID={}", run_id),
                status: "working".to_string(),
                exit_code: None,
                spawned_at_ms: started_at_ms,
                terminated_at_ms: None,
                failure_reason: None,
                profiler_engine_id: engine.as_ref().map(|m| m.plugin.id.clone()),
                profile_artifact_path: profile_artifact_path.clone(),
            };
            if let Err(e) = crate::db::db_record_process_spawn(&proc_rec) {
                crate::logger::log("WARN", "supervisor", &format!("Failed to record process spawn in SQLite for '{}': {}", run_id, e));
            }
        }

        let buffer_lines_count = get_supervisor_max_buffer_lines_count();
        let status_arc = Arc::new(RwLock::new(ProcessStatus::Working));
        let ring_arc = Arc::new(RwLock::new(VecDeque::with_capacity(buffer_lines_count)));
        let cancel_token = CancellationToken::new();
        let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(32);

        // Stdin task
        if let Some(mut child_stdin) = child.stdin.take() {
            let cancel = cancel_token.clone();
            let s_id = service_id.to_string();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        msg = stdin_rx.recv() => {
                            match msg {
                                Some(line) => {
                                    if child_stdin.write_all(line.as_bytes()).await.is_err() {
                                        crate::logger::log("WARN", "supervisor", &format!("Broken pipe writing stdin to service '{}'", s_id));
                                        break;
                                    }
                                    let _ = child_stdin.write_all(b"\n").await;
                                    let _ = child_stdin.flush().await;
                                }
                                None => break,
                            }
                        }
                    }
                }
            });
        }

        // Stdout and Stderr I/O monitors
        // The readers outlive the cancel token: a stopping run keeps its pipes during its grace
        // period (a closed pipe would kill it with SIGPIPE before it, or its engine, flushes).
        let readers_cancel = CancellationToken::new();
        spawn_io_monitors(
            &format!("service:{}", service_id),
            child.stdout.take(),
            child.stderr.take(),
            ring_arc.clone(),
            readers_cancel.clone(),
            buffer_lines_count,
        );

        // Child process lifecycle watcher task
        let managed = Arc::new(ManagedProcess {
            key: key.clone(),
            id: service_id.to_string(),
            project_path: project_path.to_string(),
            pid,
            status: status_arc.clone(),
            stdin_tx,
            cancel_token: cancel_token.clone(),
            started_at_ms,
            output_ring: ring_arc,
            run_id: run_id.clone(),
            runner: None,
            model: None,
            effort: None,
            account_id: None,
            account_email: None,
            is_interrupted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            pending_correction: Arc::new(RwLock::new(None)),
            pending_inputs: Arc::new(RwLock::new(VecDeque::new())),
            pending_preamble: Arc::new(RwLock::new(None)),
            stop_grace_period_ms: stop_grace_period_ms(engine.is_some()),
        });
        let grace_period_ms = managed.stop_grace_period_ms;

        {
            let mut procs = self.processes.write_safe("supervisor:spawn_service:insert");
            procs.insert(key.clone(), managed);
        }

        let key_clone = key.clone();
        let run_id_clone = run_id.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    let code = stop_child_in_order(&mut child, pid, grace_period_ms, &key_clone).await;
                    readers_cancel.cancel();
                    *status_arc.write_safe("supervisor:spawn_service:status_stopped") = ProcessStatus::Completed { exit_code: code };
                    crate::logger::log("INFO", "supervisor", &format!("Service '{}' stopped with code {}, state transitioned to Completed", key_clone, code));
                    crate::logger::log_failure("supervisor", "db_record_process_termination (service stopped)", crate::db::db_record_process_termination(&run_id_clone, "stopped", Some(code), epoch_now_ms(), None));
                }
                status_res = child.wait() => {
                    #[cfg(target_os = "linux")]
                    unsafe {
                        kill_process_and_group(pid, libc::SIGKILL);
                    }
                    match status_res {
                        Ok(st) => {
                            let code = exit_code_of(st);
                            *status_arc.write_safe("supervisor:spawn_service:status_arc_ok") = ProcessStatus::Completed { exit_code: code };
                            crate::logger::log("INFO", "supervisor", &format!("Service '{}' exited with code {}, state transitioned to Completed", key_clone, code));
                            crate::logger::log_failure("supervisor", "db_record_process_termination (service exit)", crate::db::db_record_process_termination(
                                &run_id_clone,
                                if code == 0 { "completed" } else { "failed" },
                                Some(code),
                                epoch_now_ms(),
                                None,
                            ));
                        }
                        Err(e) => {
                            *status_arc.write_safe("supervisor:spawn_service:status_arc_err") = ProcessStatus::Failed { error: e.to_string() };
                            crate::logger::log("ERROR", "supervisor", &format!("Service '{}' wait error: {}, state transitioned to Failed", key_clone, e));
                            crate::logger::log_failure("supervisor", "db_record_process_termination (service wait err)", crate::db::db_record_process_termination(
                                &run_id_clone,
                                "failed",
                                Some(-1),
                                epoch_now_ms(),
                                Some(&e.to_string()),
                            ));
                        }
                    }
                }
            }
        });

        crate::logger::log("INFO", "supervisor", &format!("Spawned service '{}' (PID {}), state transitioned to Working", service_id, pid));
        Ok(pid)
    }

    pub async fn spawn_task(
        &self,
        project_id: i64,
        project_path: &str,
        task_id: &str,
        exec_id: i64,
        command: &str,
        args: &[String],
        cwd: Option<&Path>,
    ) -> Result<TaskRunResult, SupervisorError> {
        let key = Self::format_task_key(project_path, task_id);
        if self.get_status(&key) == ProcessStatus::Working {
            return Err(SupervisorError::TaskAlreadyRunning {
                task_id: task_id.to_string(),
                project_path: project_path.to_string(),
            });
        }
        self.clear_retained_buffer(&key);
        let start_instant = std::time::Instant::now();
        let started_at_ms = epoch_now_ms();
        let run_id = format!("tsk_{}_{}_{}", task_id, exec_id, started_at_ms);

        crate::logger::log("DEBUG", "supervisor", &format!("Spawning task '{}' (exec {}) in '{}' with command: '{}', args: {:?}", task_id, exec_id, project_path, command, args));

        let (mut cmd, supports_custom_arg0) = if command.contains(' ') {
            let mut c = tokio::process::Command::new("/bin/sh");
            c.arg("-c").arg(shell_command_line(command, args));
            (c, true)
        } else {
            let cmd_str = command.to_string();
            let resolved = tokio::task::spawn_blocking(move || resolve_executable(&cmd_str))
                .await
                .unwrap_or_else(|_| ResolvedExecutable { program: std::path::PathBuf::from(command), supports_custom_arg0: false });
            let mut c = tokio::process::Command::new(&resolved.program);
            c.args(args);
            (c, resolved.supports_custom_arg0)
        };
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        cmd.env("ONESHIP_RUN_ID", &run_id);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        let p_name = Path::new(project_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project");
        #[cfg(unix)]
        {
            if supports_custom_arg0 {
                cmd.arg0(format!("oneship-task[{}:{}]", p_name, task_id));
            }
        }

        unsafe {
            setup_child_process_group_and_deathsig(&mut cmd);
        }

        let mut child = cmd.spawn().map_err(|e| {
            crate::logger::log("ERROR", "supervisor", &format!("Failed to spawn task '{}' (exec {}): {}", task_id, exec_id, e));
            SupervisorError::Spawn { target: format!("task '{}'", task_id), source: e }
        })?;
        let pid = child.id().ok_or_else(|| SupervisorError::MissingPid { target: format!("task '{}'", task_id) })?;
        debug_assert!(pid > 0, "Spawned task PID must be non-zero");

        crate::logger::log("INFO", "supervisor", &format!("Task '{}' (exec {}) spawned with PID {}, state transitioned to Working", task_id, exec_id, pid));

        let proc_rec = crate::db::SupervisedProcessRecord {
            run_id: run_id.clone(),
            project_id,
            target_id: task_id.to_string(),
            target_type: "task".to_string(),
            command: command.to_string(),
            args_json: serde_json::to_string(args).unwrap_or_else(|_| "[]".to_string()),
            cwd: cwd.map(|c| c.to_string_lossy().to_string()),
            pid,
            pgid: pid,
            env_identifier: format!("ONESHIP_RUN_ID={}", run_id),
            status: "working".to_string(),
            exit_code: None,
            spawned_at_ms: started_at_ms,
            terminated_at_ms: None,
            failure_reason: None,
            profiler_engine_id: None,
            profile_artifact_path: None,
        };
        if let Err(e) = crate::db::db_record_process_spawn(&proc_rec) {
            crate::logger::log("WARN", "supervisor", &format!("Failed to record task spawn in SQLite for '{}': {}", run_id, e));
        }

        let buffer_lines_count = get_supervisor_max_buffer_lines_count();
        let status_arc = Arc::new(RwLock::new(ProcessStatus::Working));
        let ring_arc = Arc::new(RwLock::new(VecDeque::with_capacity(buffer_lines_count)));
        let cancel_token = CancellationToken::new();
        // The readers outlive the cancel token, as for services (`spawn_service`).
        let readers_cancel = CancellationToken::new();
        let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(32);

        if let Some(mut child_stdin) = child.stdin.take() {
            let cancel = cancel_token.clone();
            let t_id = task_id.to_string();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        msg = stdin_rx.recv() => {
                            match msg {
                                Some(line) => {
                                    if child_stdin.write_all(line.as_bytes()).await.is_err() {
                                        crate::logger::log("WARN", "supervisor", &format!("Broken pipe writing stdin to task '{}'", t_id));
                                        break;
                                    }
                                    let _ = child_stdin.write_all(b"\n").await;
                                    let _ = child_stdin.flush().await;
                                }
                                None => break,
                            }
                        }
                    }
                }
            });
        }

        let max_task_capture_lines_count = crate::config::resolve_effective_usize(
            None,
            None,
            "supervisor",
            "max_task_capture_lines_count",
        )
        .max(100);
        let collected_stdout = Arc::new(RwLock::new(Vec::<String>::with_capacity(128)));
        let collected_stderr = Arc::new(RwLock::new(Vec::<String>::with_capacity(128)));

        // Stdout reader task
        let stdout_handle = child.stdout.take().map(|child_stdout| {
            let collected = collected_stdout.clone();
            let t_id = task_id.to_string();
            let p_id = project_id;
            let e_id = exec_id;
            let on_line = Arc::new(move |line: &str| {
                crate::logger::log("DEBUG", "supervisor", &format!("Task '{}:{}' stdout chunk ({} bytes)", t_id, e_id, line.len()));
                {
                    let mut buf = collected.write_safe("supervisor:task_collected_stdout");
                    if buf.len() < max_task_capture_lines_count {
                        buf.push(line.to_string());
                    }
                }
                get_supervisor().emit_event(DaemonEvent::task_output_chunk(&crate::events::TaskOutputChunkPayload {
                    project_id: p_id,
                    task_id: t_id.clone(),
                    exec_id: e_id,
                    line: line.to_string(),
                    is_stderr: false,
                }));
            });
            spawn_stream_reader(
                child_stdout,
                ring_arc.clone(),
                readers_cancel.clone(),
                buffer_lines_count,
                None,
                Some(on_line),
                format!("task-stdout:{}:{}", task_id, exec_id),
            )
        });

        // Stderr reader task
        let stderr_handle = child.stderr.take().map(|child_stderr| {
            let collected = collected_stderr.clone();
            let t_id = task_id.to_string();
            let p_id = project_id;
            let e_id = exec_id;
            let on_line = Arc::new(move |line: &str| {
                crate::logger::log("DEBUG", "supervisor", &format!("Task '{}:{}' stderr chunk ({} bytes)", t_id, e_id, line.len()));
                {
                    let mut buf = collected.write_safe("supervisor:task_collected_stderr");
                    if buf.len() < max_task_capture_lines_count {
                        buf.push(format!("[STDERR] {}", line));
                    }
                }
                get_supervisor().emit_event(DaemonEvent::task_output_chunk(&crate::events::TaskOutputChunkPayload {
                    project_id: p_id,
                    task_id: t_id.clone(),
                    exec_id: e_id,
                    line: line.to_string(),
                    is_stderr: true,
                }));
            });
            spawn_stream_reader(
                child_stderr,
                ring_arc.clone(),
                readers_cancel.clone(),
                buffer_lines_count,
                Some("[STDERR]"),
                Some(on_line),
                format!("task-stderr:{}:{}", task_id, exec_id),
            )
        });

        let managed = Arc::new(ManagedProcess {
            key: key.clone(),
            id: task_id.to_string(),
            project_path: project_path.to_string(),
            pid,
            status: status_arc.clone(),
            stdin_tx,
            cancel_token: cancel_token.clone(),
            started_at_ms,
            output_ring: ring_arc,
            run_id: run_id.clone(),
            runner: None,
            model: None,
            effort: None,
            account_id: None,
            account_email: None,
            is_interrupted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            pending_correction: Arc::new(RwLock::new(None)),
            pending_inputs: Arc::new(RwLock::new(VecDeque::new())),
            pending_preamble: Arc::new(RwLock::new(None)),
            stop_grace_period_ms: stop_grace_period_ms(false),
        });
        let grace_period_ms = managed.stop_grace_period_ms;

        {
            let mut procs = self.processes.write_safe("supervisor:spawn_task:insert");
            procs.insert(key.clone(), managed.clone());
        }

        let (exit_code, was_stopped) = tokio::select! {
            _ = cancel_token.cancelled() => {
                let code = stop_child_in_order(&mut child, pid, grace_period_ms, &key).await;
                readers_cancel.cancel();
                (code, true)
            }
            res = child.wait() => {
                #[cfg(target_os = "linux")]
                unsafe {
                    kill_process_and_group(pid, libc::SIGKILL);
                }
                match res {
                    Ok(st) => (exit_code_of(st), false),
                    Err(_) => (-1, false),
                }
            }
        };

        if let Some(h) = stdout_handle {
            let _ = h.await;
        }
        if let Some(h) = stderr_handle {
            let _ = h.await;
        }

        let duration_ms = start_instant.elapsed().as_millis() as i64;
        let status_str = match (was_stopped, exit_code) {
            (true, _) => "stopped",
            (false, 0) => "completed",
            (false, _) => "failed",
        };

        *status_arc.write_safe("supervisor:spawn_task:status") = if exit_code == 0 || was_stopped {
            ProcessStatus::Completed { exit_code }
        } else {
            ProcessStatus::Failed { error: format!("Exit code {}", exit_code) }
        };

        self.emit_event(DaemonEvent::task_completed(&crate::events::TaskCompletedPayload {
            project_id,
            task_id: task_id.to_string(),
            exec_id,
            exit_code,
            duration_ms,
            status: status_str.to_string(),
        }));

        crate::logger::log_failure("supervisor", "db_record_process_termination (task exit)", crate::db::db_record_process_termination(
            &run_id,
            status_str,
            Some(exit_code),
            epoch_now_ms(),
            if status_str == "failed" { Some("Task exited with non-zero exit code") } else { None },
        ));

        let mut final_ring = managed.output_ring.read_safe("supervisor:spawn_task:retain").clone();
        if was_stopped {
            let divider = format!("[SYSTEM] Process stopped (PID {})", pid);
            if final_ring.len() >= buffer_lines_count {
                final_ring.pop_front();
            }
            final_ring.push_back(divider);
        }
        self.store_retained_buffer(&key, final_ring);

        {
            let mut procs = self.processes.write_safe("supervisor:spawn_task:remove");
            procs.remove(&key);
        }

        let stdout_str = collected_stdout.read_safe("supervisor:spawn_task:collected_stdout").join("\n");
        let stderr_str = collected_stderr.read_safe("supervisor:spawn_task:collected_stderr").join("\n");
        let combined_logs = if stderr_str.trim().is_empty() {
            stdout_str.clone()
        } else if stdout_str.trim().is_empty() {
            stderr_str.clone()
        } else {
            format!("{}\n--- STDERR ---\n{}", stdout_str, stderr_str)
        };

        crate::logger::log("INFO", "supervisor", &format!("Task '{}' (exec {}) completed: state transitioned to {} (exit code {}, duration {}ms)", task_id, exec_id, status_str, exit_code, duration_ms));

        Ok(TaskRunResult {
            exit_code,
            duration_ms,
            stdout: stdout_str,
            stderr: stderr_str,
            combined_logs,
            status: status_str.to_string(),
        })
    }

    pub async fn spawn_agent(
        &self,
        project_path: &str,
        agent_id: &str,
        runner: &str,
        model: &str,
        effort: Option<&str>,
        auto_approve: bool,
        accept_edits: bool,
        extra_args: &[String],
        env_vars: &[(String, String)],
        session_override: Option<&str>,
        explicit_account_id: Option<&str>,
        clean_context: bool,
    ) -> Result<u32, SupervisorError> {
        let key = Self::format_agent_key(project_path, agent_id);

        // The pair is validated against the runner manifest before anything is stopped or spawned;
        // a missing effort takes the model's declared default.
        let pair = crate::kopeng::resolution::finalize_agent_config(&format!("agent '{}'", agent_id), runner.trim(), Some(model), effort)?;
        let model: Option<&str> = Some(pair.model.as_str());
        let effort: Option<&str> = pair.effort.as_deref();

        // Terminate any previous instance
        self.stop_process(&key).await?;

        if clean_context {
            self.clear_retained_buffer(&key);
        }

        let runner_bin = pair.runner.as_str();
        let is_agy = runner_bin == "agy";
        let is_opencode = runner_bin == "opencode";
        let is_claude = runner_bin == "claude";

        // Resolve account credentials for OpenCode, Claude Code, or Antigravity
        // An explicitly requested account that is missing or cannot be decrypted is an error:
        // silently spawning under a different account would misattribute quota and history.
        let missing_account = |id: &str| SupervisorError::AccountUnavailable {
            account_id: id.to_string(),
            agent_id: agent_id.to_string(),
            details: "account does not exist".to_string(),
        };
        let undecryptable = |id: &str, e: crate::kopeng::accounts::AccountsError| SupervisorError::AccountUnavailable {
            account_id: id.to_string(),
            agent_id: agent_id.to_string(),
            details: format!("cannot decrypt credentials: {}", e),
        };
        let (opencode_acc_id, opencode_acc_email, opencode_key) = if is_opencode {
            let account = match explicit_account_id {
                Some(id) => Some(crate::db::db_get_account_by_id(id)?.ok_or_else(|| missing_account(id))?),
                None => crate::db::db_get_active_account_by_provider("deepseek")?,
            };
            match account {
                Some(acc) => {
                    let key = crate::kopeng::accounts::decrypt_refresh_token(&acc.enc_token)
                        .map_err(|e| undecryptable(&acc.id, e))?;
                    (Some(acc.id), Some(acc.email), Some(key))
                }
                None => (None, None, None),
            }
        } else {
            (None, None, None)
        };

        let (claude_acc_id, claude_acc_email) = if is_claude {
            let account = match explicit_account_id {
                Some(id) => Some(crate::db::db_get_account_by_id(id)?.ok_or_else(|| missing_account(id))?),
                None => crate::db::db_get_active_account_by_provider("claude")?,
            };
            match account {
                Some(acc) => (Some(acc.id), Some(acc.email)),
                None => (None, None),
            }
        } else {
            (None, None)
        };

        let (agy_acc_id, agy_acc_email, agy_target_token) = if is_agy {
            match explicit_account_id {
                Some(id) => {
                    let acc = crate::db::db_get_account_by_id(id)?.ok_or_else(|| missing_account(id))?;
                    let token = crate::kopeng::accounts::decrypt_refresh_token(&acc.enc_token)
                        .map_err(|e| undecryptable(&acc.id, e))?;
                    (Some(acc.id), Some(acc.email), Some(token))
                }
                None => (None, None, None),
            }
        } else {
            (None, None, None)
        };

        // Guarantee that project .mcp.json exists so Claude Code discovers one-ship MCP tools natively
        if is_claude {
            let claude_mcp = std::path::Path::new(project_path).join(".mcp.json");
            if !claude_mcp.exists() {
                crate::kopeng::skill::inject_project_mcp_plugin(std::path::Path::new(project_path))?;
            }
        }

        let mut cli_args = Vec::new();
        if is_opencode {
            cli_args.push("acp".to_string());
        } else if is_claude {
            cli_args.push("--print".to_string());
            cli_args.push("--input-format".to_string());
            cli_args.push("stream-json".to_string());
            cli_args.push("--output-format".to_string());
            cli_args.push("stream-json".to_string());
            cli_args.push("--verbose".to_string());
            cli_args.push("--include-partial-messages".to_string());

            if auto_approve {
                cli_args.push("--dangerously-skip-permissions".to_string());
            }
            if let Some(m) = model {
                let trimmed = m.trim();
                if !trimmed.is_empty() {
                    cli_args.push("--model".to_string());
                    cli_args.push(trimmed.to_string());
                }
            }
            // The manifest declares the effort flag whenever a model has efforts (`manifest_problems`).
            let schema = crate::config::load_runner_schema(runner_bin)?;
            if let (Some(flag), Some(e)) = (schema.flags.effort.as_deref(), effort) {
                cli_args.push(flag.to_string());
                cli_args.push(e.to_string());
            }
            for arg in extra_args {
                cli_args.push(arg.clone());
            }
            if let Some(sess) = session_override {
                cli_args.push("--resume".to_string());
                cli_args.push(sess.to_string());
            }
        } else {
            if auto_approve {
                match runner_bin {
                    "agy" => cli_args.push("--dangerously-skip-permissions".to_string()),
                    "cursor" => cli_args.push("--auto-approve".to_string()),
                    _ => cli_args.push("--dangerously-skip-permissions".to_string()),
                }
            }
            if accept_edits && is_agy {
                cli_args.push("--mode".to_string());
                cli_args.push("accept-edits".to_string());
            }
            if let Some(m) = model {
                let trimmed = m.trim();
                if !trimmed.is_empty() && is_agy {
                    cli_args.push("--model".to_string());
                    cli_args.push(trimmed.to_string());
                }
            }
            if let Some(e) = effort {
                if is_agy {
                    cli_args.push("--effort".to_string());
                    cli_args.push(e.to_string());
                }
            }
            if is_agy {
                cli_args.push("--input-format".to_string());
                cli_args.push("stream-json".to_string());
                cli_args.push("--output-format".to_string());
                cli_args.push("stream-json".to_string());
            }
            for arg in extra_args {
                cli_args.push(arg.clone());
            }

            // Resume target session if provided
            if let Some(sess) = session_override {
                if is_agy {
                    cli_args.push("--conversation".to_string());
                    cli_args.push(sess.to_string());
                }
            }
        }

        let bin_str = runner_bin.to_string();
        let resolved = tokio::task::spawn_blocking(move || resolve_executable(&bin_str))
            .await
            .unwrap_or_else(|_| ResolvedExecutable { program: std::path::PathBuf::from(runner_bin), supports_custom_arg0: false });
        let mut cmd = tokio::process::Command::new(&resolved.program);
        cmd.args(&cli_args);
        cmd.current_dir(project_path);

        let mut final_envs = env_vars.to_vec();
        // Resolve identity before spawning so an unattributable agent never starts (spec 12 §6.2).
        let (instance_scope, instance_project_id, instance_agent_uid) = resolve_spawn_identity(project_path, agent_id)?;
        let instance_id = uuid::Uuid::new_v4().to_string();
        final_envs.push((crate::caller_identity::AGENT_INSTANCE_ENV_KEY.to_string(), instance_id.clone()));
        if is_opencode && !final_envs.iter().any(|(k, _)| k == "DEEPSEEK_API_KEY") {
            if let Some(ref key) = opencode_key {
                final_envs.push(("DEEPSEEK_API_KEY".to_string(), key.clone()));
            } else if let Ok(Some(key)) = crate::kopeng::accounts::get_active_deepseek_key() {
                final_envs.push(("DEEPSEEK_API_KEY".to_string(), key));
            } else if let Ok(key) = std::env::var("DEEPSEEK_API_KEY") {
                final_envs.push(("DEEPSEEK_API_KEY".to_string(), key));
            }
        }
        cmd.envs(final_envs.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        let project_name = Path::new(project_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project");
        #[cfg(unix)]
        {
            if resolved.supports_custom_arg0 {
                cmd.arg0(format!("oneship[{}:{}]", project_name, agent_id));
            } else {
                crate::logger::log("DEBUG", "supervisor", &format!(
                    "Skipping custom arg0 process name for agent '{}' because target '{}' is a shim dispatcher",
                    agent_id, resolved.program.display()
                ));
            }
        }

        unsafe {
            setup_child_process_group_and_deathsig(&mut cmd);
        }

        crate::logger::log("DEBUG", "supervisor", &format!("Spawning agent '{}' (runner: {}, resolved: {}, model: {:?}, effort: {:?}) in '{}' with args: {:?}", agent_id, runner_bin, resolved.program.display(), model, effort, project_path, cli_args));

        let init_notify = Arc::new(tokio::sync::Notify::new());
        let init_notify_for_reader = init_notify.clone();

        // Acquire keyring lock for AGY to await any active warmup/probe and ensure active Google credentials
        let child_res = {
            if is_agy {
                let guard = crate::kopeng::accounts::keyring_lock().lock().await;
                if let (Some(ref refresh_token), Some(ref email)) = (&agy_target_token, &agy_acc_email) {
                    let payload = serde_json::json!({
                        "token": {
                            "access_token": "",
                            "token_type": "Bearer",
                            "refresh_token": refresh_token,
                            "expiry": "2000-01-01T00:00:00Z"
                        },
                        "auth_method": "consumer",
                        "id_token": "",
                        "_account_email": email
                    });
                    crate::logger::log_failure("supervisor", "write_keyring_token", crate::kopeng::accounts::write_keyring_token(&payload));
                } else {
                    crate::logger::log_failure("supervisor", "ensure_active_account_in_keyring", crate::kopeng::accounts::ensure_active_account_in_keyring());
                }
                let res = cmd.spawn();
                if agy_target_token.is_some() {
                    let agt_name = agent_id.to_string();
                    if let Some(ref aid) = agy_acc_id {
                        crate::logger::log_failure("supervisor", "db_set_active_account", crate::db::db_set_active_account(aid));
                        crate::logger::log("INFO", "supervisor", &format!("Agent '{}' spawned with target account '{}'", agt_name, aid));
                    }
                    drop(guard);
                } else {
                    drop(guard);
                }
                res
            } else {
                cmd.spawn()
            }
        };

        let mut child = child_res.map_err(|e| {
            crate::logger::log("ERROR", "supervisor", &format!("Failed to spawn agent runner '{}' for agent '{}': {}", runner_bin, agent_id, e));
            SupervisorError::Spawn { target: format!("agent runner '{}' for agent '{}'", runner_bin, agent_id), source: e }
        })?;
        let pid = child.id().ok_or_else(|| SupervisorError::MissingPid { target: format!("agent '{}'", agent_id) })?;
        debug_assert!(pid > 0, "Spawned agent PID must be non-zero");

        let instance_record = crate::db::AgentInstanceRecord {
            instance_id: instance_id.clone(),
            scope: instance_scope,
            project_id: instance_project_id,
            agent_uid: instance_agent_uid,
            runner: runner_bin.to_string(),
            pid,
            spawned_at_ms: epoch_now_ms(),
            terminated_at_ms: None,
        };
        if let Err(e) = crate::db::db_insert_agent_instance(&instance_record) {
            crate::logger::log("ERROR", "supervisor", &format!("Failed to register instance for agent '{}' (PID {}): {}. Killing process.", agent_id, pid, e));
            let _ = child.start_kill();
            return Err(SupervisorError::Database(e));
        }

        crate::logger::log("INFO", "supervisor", &format!("Agent '{}' spawned with PID {}, state transitioned to Idle", agent_id, pid));

        let buffer_lines_count = get_supervisor_max_buffer_lines_count();
        let status_arc = Arc::new(RwLock::new(ProcessStatus::Idle));
        let mut initial_ring = if !clean_context {
            self.get_retained_buffer(&key, buffer_lines_count).unwrap_or_else(|| VecDeque::with_capacity(buffer_lines_count))
        } else {
            VecDeque::with_capacity(buffer_lines_count)
        };
        if !clean_context && !initial_ring.is_empty() {
            let resume_divider = if let Some(sid) = session_override {
                format!("[SYSTEM] Session resumed (id: {}) — agent ready", sid)
            } else {
                "[SYSTEM] Session resumed — agent ready".to_string()
            };
            if initial_ring.len() >= buffer_lines_count {
                initial_ring.pop_front();
            }
            initial_ring.push_back(resume_divider);
        }
        let ring_arc = Arc::new(RwLock::new(initial_ring));
        let cancel_token = CancellationToken::new();
        let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(32);
        let (raw_stdin_tx, mut raw_stdin_rx) = mpsc::channel::<String>(64);
        let raw_stdin_tx_clone = raw_stdin_tx.clone();

        let active_session_id = Arc::new(RwLock::new(session_override.map(|s| s.to_string())));
        let active_session_id_clone = active_session_id.clone();
        let next_rpc_id = Arc::new(AtomicU64::new(1000));
        let target_model = model.map(|s| s.to_string());
        let target_effort = effort.map(|s| s.to_string());
        let session_override_str = session_override.map(|s| s.to_string());

        // Stdin sink task (writes raw lines to child stdin)
        if let Some(mut child_stdin) = child.stdin.take() {
            let cancel = cancel_token.clone();
            let agt_stdin_id = agent_id.to_string();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        msg = raw_stdin_rx.recv() => {
                            match msg {
                                Some(line) => {
                                    if child_stdin.write_all(line.as_bytes()).await.is_err() {
                                        crate::logger::log("WARN", "supervisor", &format!("Broken pipe writing stdin to agent '{}'", agt_stdin_id));
                                        break;
                                    }
                                    let _ = child_stdin.write_all(b"\n").await;
                                    let _ = child_stdin.flush().await;
                                }
                                None => break,
                            }
                        }
                    }
                }
            });
        }

        // Stdin forwarder task (formats user input according to runner protocol)
        {
            let cancel = cancel_token.clone();
            let raw_tx = raw_stdin_tx.clone();
            let sid_arc = active_session_id.clone();
            let rpc_counter = next_rpc_id.clone();
            let agt_fwd_id = agent_id.to_string();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        msg = stdin_rx.recv() => {
                            match msg {
                                Some(raw_input) => {
                                    crate::logger::log("DEBUG", "supervisor", &format!("Forwarding input to agent '{}' ({} bytes)", agt_fwd_id, raw_input.len()));
                                    if is_opencode {
                                        let mut attempts_count = 0;
                                        let sid = loop {
                                            if let Some(s) = sid_arc.read_safe("supervisor:opencode_sid").clone() {
                                                break Some(s);
                                            }
                                            if attempts_count >= 50 {
                                                break None;
                                            }
                                            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                                            attempts_count += 1;
                                        };
                                        if let Some(s) = sid {
                                            let prompt_req_id = rpc_counter.fetch_add(1, Ordering::SeqCst);
                                            let payload = serde_json::json!({
                                                "jsonrpc": "2.0",
                                                "id": prompt_req_id,
                                                "method": "session/prompt",
                                                "params": {
                                                    "sessionId": s,
                                                    "prompt": [
                                                        {
                                                            "type": "text",
                                                            "text": raw_input
                                                        }
                                                    ]
                                                }
                                            }).to_string();
                                            let _ = raw_tx.send(payload).await;
                                        }
                                    } else if is_claude {
                                        let payload = if !raw_input.trim().starts_with('{') {
                                            let mut obj = serde_json::json!({
                                                "type": "user",
                                                "message": {
                                                    "role": "user",
                                                    "content": [
                                                        {
                                                            "type": "text",
                                                            "text": raw_input
                                                        }
                                                    ]
                                                }
                                            });
                                            if let Some(s) = sid_arc.read_safe("supervisor:opencode_sid").clone() {
                                                obj["session_id"] = serde_json::Value::String(s);
                                            }
                                            obj.to_string()
                                        } else {
                                            raw_input
                                        };
                                        let _ = raw_tx.send(payload).await;
                                    } else {
                                        let payload = if is_agy && !raw_input.trim().starts_with('{') {
                                            serde_json::json!({
                                                "event": "user",
                                                "message": {
                                                    "role": "user",
                                                    "content": raw_input
                                                }
                                            }).to_string()
                                        } else {
                                            raw_input
                                        };
                                        let _ = raw_tx.send(payload).await;
                                    }
                                }
                                None => break,
                            }
                        }
                    }
                }
            });
        }

        // OpenCode ACP pre-handshake
        if is_opencode {
            let init_tx = raw_stdin_tx.clone();
            tokio::spawn(async move {
                let init_payload = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "initialize",
                    "params": {
                        "protocolVersion": 1,
                        "capabilities": {},
                        "clientInfo": {
                            "name": "one-ship",
                            "version": crate::kopeng::skill::CURRENT_PLATFORM_VERSION
                        }
                    }
                }).to_string();
                let _ = init_tx.send(init_payload).await;
            });
        }

        // Stdout reader task (with protocol-specific parsing: ACP for OpenCode, stream-json for AGY)
        if let Some(child_stdout) = child.stdout.take() {
            let ring = ring_arc.clone();
            let cancel = cancel_token.clone();
            let max_lines_count = buffer_lines_count;
            let agt_id = agent_id.to_string();
            let proj_path = project_path.to_string();
            let raw_tx = raw_stdin_tx_clone;
            let sid_arc = active_session_id_clone;
            let target_m = target_model;
            let target_eff = target_effort;
            let sess_override = session_override_str;
            let is_auto_approve = auto_approve;
            let init_notify_rx = init_notify_for_reader.clone();
            let proc_status_arc = status_arc.clone();

            let mcp_port = crate::config::mcp_daemon_port();

            tokio::spawn(async move {
                let max_agent_response_bytes = crate::config::resolve_effective_usize(
                    None,
                    None,
                    "supervisor",
                    "max_agent_response_bytes",
                )
                .max(65_536);
                /// Appends `chunk` to `buf` up to `max_bytes` on a UTF-8 char boundary.
                fn push_str_bounded(buf: &mut String, chunk: &str, max_bytes: usize) {
                    let remaining = max_bytes.saturating_sub(buf.len());
                    if remaining == 0 {
                        return;
                    }
                    if chunk.len() <= remaining {
                        buf.push_str(chunk);
                        return;
                    }
                    let mut cut = remaining;
                    while cut > 0 && !chunk.is_char_boundary(cut) {
                        cut -= 1;
                    }
                    buf.push_str(&chunk[..cut]);
                }

                let mut reader = BufReader::new(child_stdout).lines();
                let mut accumulated_response = String::new();
                let mut accumulated_thought = String::new();
                let mut thought_line_buffer = String::new();
                let mut in_thought_phase = false;
                loop {
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        line_res = reader.next_line() => {
                            match line_res {
                                Ok(Some(line)) => {
                                    let mut display_lines: Vec<String> = Vec::new();
                                    if is_opencode {
                                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                                            let rpc_id = val.get("id").and_then(|v| v.as_u64());
                                            let method = val.get("method").and_then(|m| m.as_str());

                                            // 1. Handshake response: initialize (id: 1)
                                            if rpc_id == Some(1) {
                                                let session_req = if let Some(ref sess) = sess_override {
                                                    serde_json::json!({
                                                        "jsonrpc": "2.0",
                                                        "id": 2,
                                                        "method": "session/load",
                                                        "params": {
                                                            "sessionId": sess
                                                        }
                                                    }).to_string()
                                                } else {
                                                    serde_json::json!({
                                                        "jsonrpc": "2.0",
                                                        "id": 2,
                                                        "method": "session/new",
                                                        "params": {
                                                            "cwd": proj_path,
                                                            "mcpServers": [
                                                                {
                                                                    "name": "one-ship",
                                                                    "type": "http",
                                                                    "url": format!("http://localhost:{}/mcp", mcp_port),
                                                                    "headers": []
                                                                }
                                                            ]
                                                        }
                                                    }).to_string()
                                                };
                                                let _ = raw_tx.send(session_req).await;
                                            }
                                            // 2. Handshake response: session/new or session/load (id: 2)
                                            else if rpc_id == Some(2) {
                                                let sid = val.get("result")
                                                    .and_then(|r| r.get("sessionId"))
                                                    .and_then(|s| s.as_str())
                                                    .unwrap_or("");
                                                 if !sid.is_empty() {
                                                    *sid_arc.write_safe("supervisor:opencode_set_sid") = Some(sid.to_string());
                                                    crate::logger::log("INFO", "supervisor", &format!("Agent '{}' (OpenCode) session initialized: {}", agt_id, sid));
                                                    display_lines.push(format!("[INIT] session initialized (conversation: {})", sid));
                                                    record_native_session(&proj_path, &agt_id, "opencode", sid);
                                                    if agt_id == crate::kopeng::initializer::INITIALIZER_AGENT_ID {
                                                        crate::kopeng::initializer::record_initializer_session(&proj_path, sid);
                                                    }
                                                    let sup = get_supervisor();
                                                    sup.emit_step_update(&agt_id, &proj_path, Some(sid), "DONE", "init", None, "Session initialized", None);

                                                    if let Some(ref m) = target_m {
                                                        let set_model_req = serde_json::json!({
                                                            "jsonrpc": "2.0",
                                                            "id": 100,
                                                            "method": "session/set_config_option",
                                                            "params": {
                                                                "sessionId": sid,
                                                                "configId": "model",
                                                                "value": m
                                                            }
                                                        }).to_string();
                                                        let _ = raw_tx.send(set_model_req).await;
                                                    }
                                                    if let Some(ref e) = target_eff {
                                                        let set_effort_req = serde_json::json!({
                                                            "jsonrpc": "2.0",
                                                            "id": 101,
                                                            "method": "session/set_config_option",
                                                            "params": {
                                                                "sessionId": sid,
                                                                "configId": "effort",
                                                                "value": e
                                                            }
                                                        }).to_string();
                                                        let _ = raw_tx.send(set_effort_req).await;
                                                    }
                                                }
                                            }
                                            // 3. Streaming notifications
                                            else if method == Some("session/update") {
                                                if let Some(update) = val.get("params").and_then(|p| p.get("update")) {
                                                    let session_update = update.get("sessionUpdate").and_then(|s| s.as_str()).unwrap_or("");
                                                    match session_update {
                                                        "agent_message_chunk" => {
                                                            if in_thought_phase {
                                                                if !thought_line_buffer.trim().is_empty() {
                                                                    display_lines.push(format!("[THINKING] {}", thought_line_buffer.trim()));
                                                                    thought_line_buffer.clear();
                                                                }
                                                                in_thought_phase = false;
                                                            }
                                                            let text = update.get("content").and_then(|c| c.get("text")).and_then(|t| t.as_str()).unwrap_or("");
                                                            if !text.is_empty() {
                                                                crate::logger::log("DEBUG", "supervisor", &format!("Agent '{}' (OpenCode) message chunk arrival ({} bytes)", agt_id, text.len()));
                                                                push_str_bounded(&mut accumulated_response, text, max_agent_response_bytes);
                                                                push_str_bounded(&mut accumulated_thought, text, max_agent_response_bytes);
                                                                let trimmed = text.trim();
                                                                if !trimmed.is_empty() {
                                                                    display_lines.push(trimmed.to_string());
                                                                }
                                                                let sup = get_supervisor();
                                                                sup.emit_step_update(&agt_id, &proj_path, None, "ACTIVE", "agent_response", None, "Drafting response...", Some(&accumulated_thought));
                                                            }
                                                        }
                                                        "agent_thought_chunk" => {
                                                            let text = update.get("content").and_then(|c| c.get("text")).and_then(|t| t.as_str()).unwrap_or("");
                                                            if !text.is_empty() {
                                                                if !in_thought_phase {
                                                                    accumulated_thought.clear();
                                                                    thought_line_buffer.clear();
                                                                    in_thought_phase = true;
                                                                }
                                                                push_str_bounded(&mut accumulated_thought, text, max_agent_response_bytes);
                                                                push_str_bounded(&mut thought_line_buffer, text, max_agent_response_bytes);

                                                                while let Some(newline_pos) = thought_line_buffer.find('\n') {
                                                                    let line = thought_line_buffer[..newline_pos].trim().to_string();
                                                                    thought_line_buffer = thought_line_buffer[(newline_pos + 1)..].to_string();
                                                                    if !line.is_empty() {
                                                                        display_lines.push(format!("[THINKING] {}", line));
                                                                    }
                                                                }
                                                                if thought_line_buffer.len() >= 100 {
                                                                    if let Some(space_pos) = thought_line_buffer.rfind(' ') {
                                                                        if space_pos > 40 {
                                                                            let line = thought_line_buffer[..space_pos].trim().to_string();
                                                                            thought_line_buffer = thought_line_buffer[(space_pos + 1)..].to_string();
                                                                            if !line.is_empty() {
                                                                                display_lines.push(format!("[THINKING] {}", line));
                                                                            }
                                                                        }
                                                                    }
                                                                }

                                                                let sup = get_supervisor();
                                                                sup.emit_step_update(&agt_id, &proj_path, None, "ACTIVE", "thought", None, "Thinking...", Some(&accumulated_thought));
                                                            }
                                                        }
                                                        "tool_call" | "tool_call_update" => {
                                                            if in_thought_phase {
                                                                if !thought_line_buffer.trim().is_empty() {
                                                                    display_lines.push(format!("[THINKING] {}", thought_line_buffer.trim()));
                                                                    thought_line_buffer.clear();
                                                                }
                                                                in_thought_phase = false;
                                                            }
                                                            let title = update.get("title").and_then(|t| t.as_str()).unwrap_or("tool");
                                                            let status = update.get("status").and_then(|s| s.as_str()).unwrap_or("in_progress");
                                                            let raw_input = update.get("rawInput");

                                                            crate::logger::log("DEBUG", "supervisor", &format!("Agent '{}' (OpenCode) tool call: {} (status: {})", agt_id, title, status));

                                                            let action_summary = if title.contains("post_agent_response") {
                                                                crate::conversation::mark_agent_responded(&proj_path, &agt_id);
                                                                accumulated_response.clear();
                                                                "Delivering response to dialogue channel...".to_string()
                                                            } else if title.contains("raise_initialization_flag") {
                                                                let flag_type = raw_input.and_then(|p| p.get("flag_type")).and_then(|f| f.as_str()).unwrap_or("");
                                                                format!("Emitting initialization flag: {}", flag_type)
                                                            } else {
                                                                format!("Executing tool: {}", title)
                                                            };

                                                            if status == "pending" || status == "in_progress" {
                                                                *proc_status_arc.write_safe("supervisor:opencode:working") = ProcessStatus::Working;
                                                                let params_str = raw_input.map(|p| serde_json::to_string(p).unwrap_or_default()).unwrap_or_default();
                                                                display_lines.push(format!("[TOOL] {} {}", title, params_str));
                                                                let sup = get_supervisor();
                                                                let live_thought = if !accumulated_thought.is_empty() {
                                                                    &accumulated_thought
                                                                } else {
                                                                    &accumulated_response
                                                                };
                                                                sup.emit_step_update(&agt_id, &proj_path, None, "ACTIVE", "tool", Some(title), &action_summary, Some(live_thought));
                                                            } else if status == "completed" {
                                                                display_lines.push(format!("[TOOL:DONE] {}", title));
                                                                accumulated_thought.clear();
                                                            }
                                                        }
                                                        "usage_update" => {
                                                            let cost = update.get("cost").and_then(|c| c.get("amount")).and_then(|a| a.as_f64()).unwrap_or(0.0);
                                                            let used_tokens = update.get("used").and_then(|u| u.as_u64()).unwrap_or(0);
                                                            display_lines.push(format!("[USAGE] Tokens: {}, Cost: ${:.6}", used_tokens, cost));

                                                            let max_ctx = crate::kopeng::stats::context_window_tokens_count(Some("opencode"), target_m.as_deref()).ok().flatten();
                                                            let sup = get_supervisor();
                                                            sup.emit_event(DaemonEvent::agent_usage_update(&crate::events::AgentUsageUpdatePayload {
                                                                agent_id: agt_id.clone(),
                                                                project_path: proj_path.clone(),
                                                                runner: "opencode".to_string(),
                                                                model: target_m.clone(),
                                                                tokens_used_count: used_tokens,
                                                                max_context_tokens_count: max_ctx,
                                                                cost_dollars: Some(cost),
                                                                timestamp_ms: epoch_now_ms(),
                                                            }));
                                                            let p_clone = proj_path.clone();
                                                            let a_clone = agt_id.clone();
                                                            let m_clone = target_m.clone();
                                                            tokio::task::spawn_blocking(move || {
                                                                crate::logger::log_failure(
                                                                    "supervisor",
                                                                    "db_upsert_live_session_usage",
                                                                    crate::db::db_upsert_live_session_usage(&p_clone, &a_clone, "opencode", m_clone.as_deref(), used_tokens, max_ctx, Some(cost)),
                                                                );
                                                            });
                                                        }
                                                        _ => {}
                                                    }
                                                }
                                            }
                                            // 4. Request for permission from OpenCode
                                            else if method == Some("session/request_permission") {
                                                if let Some(req_id) = val.get("id") {
                                                    let outcome = if is_auto_approve { "accepted" } else { "rejected" };
                                                    let approve_res = serde_json::json!({
                                                        "jsonrpc": "2.0",
                                                        "id": req_id,
                                                        "result": {
                                                            "outcome": outcome
                                                        }
                                                    }).to_string();
                                                    let _ = raw_tx.send(approve_res).await;
                                                }
                                            }
                                            // 5. Prompt finished response (id >= 1000)
                                            else if rpc_id.map(|id| id >= 1000).unwrap_or(false) && val.get("result").is_some() {
                                                *proc_status_arc.write_safe("supervisor:opencode:idle") = ProcessStatus::Idle;
                                                if in_thought_phase {
                                                    if !thought_line_buffer.trim().is_empty() {
                                                        display_lines.push(format!("[THINKING] {}", thought_line_buffer.trim()));
                                                        thought_line_buffer.clear();
                                                    }
                                                    in_thought_phase = false;
                                                }
                                                accumulated_thought.clear();
                                                let res = val.get("result");
                                                let stop_reason = res.and_then(|r| r.get("stopReason")).and_then(|s| s.as_str()).unwrap_or("end_turn");
                                                let usage = res.and_then(|r| r.get("usage"));
                                                let total_tokens = usage.and_then(|u| u.get("totalTokens")).and_then(|t| t.as_u64()).unwrap_or(0);
                                                crate::logger::log("INFO", "supervisor", &format!("Agent '{}' (OpenCode) prompt finished: status '{}', {} tokens used", agt_id, stop_reason, total_tokens));
                                                display_lines.push(format!("[RESULT] Status: {} (tokens: {})", stop_reason, total_tokens));

                                                let max_ctx = crate::kopeng::stats::context_window_tokens_count(Some("opencode"), target_m.as_deref()).ok().flatten();
                                                let cost_opt = res.and_then(|r| r.get("cost")).and_then(|c| c.get("amount")).and_then(|a| a.as_f64());
                                                let sup = get_supervisor();
                                                sup.emit_event(DaemonEvent::agent_usage_update(&crate::events::AgentUsageUpdatePayload {
                                                    agent_id: agt_id.clone(),
                                                    project_path: proj_path.clone(),
                                                    runner: "opencode".to_string(),
                                                    model: target_m.clone(),
                                                    tokens_used_count: total_tokens,
                                                    max_context_tokens_count: max_ctx,
                                                    cost_dollars: cost_opt,
                                                    timestamp_ms: epoch_now_ms(),
                                                }));
                                                let p_clone = proj_path.clone();
                                                let a_clone = agt_id.clone();
                                                let m_clone = target_m.clone();
                                                tokio::task::spawn_blocking(move || {
                                                    crate::logger::log_failure(
                                                        "supervisor",
                                                        "db_upsert_live_session_usage",
                                                        crate::db::db_upsert_live_session_usage(&p_clone, &a_clone, "opencode", m_clone.as_deref(), total_tokens, max_ctx, cost_opt),
                                                    );
                                                });
                                                sup.emit_step_update(&agt_id, &proj_path, None, "DONE", "result", None, "", None);

                                                if crate::conversation::should_inject_compliance_reminder(&proj_path, &agt_id) {
                                                    let trimmed_fallback = accumulated_response.trim().to_string();
                                                    if !trimmed_fallback.is_empty() {
                                                        crate::logger::log(
                                                            "WARN",
                                                            "supervisor",
                                                            &format!("Agent '{}' (OpenCode) completed turn via fallback without calling post_agent_response. Ingesting {} chars.", agt_id, trimmed_fallback.len())
                                                        );
                                                        persist_fallback_agent_response(&proj_path, &agt_id, &trimmed_fallback);
                                                        crate::conversation::mark_agent_responded(&proj_path, &agt_id);
                                                    } else {
                                                        let key = ProcessSupervisor::format_agent_key(&proj_path, &agt_id);
                                                        let prompt_clone = "## MANDATORY PROTOCOL CORRECTION:\n\
                                                             You completed your turn without calling the MCP tool `post_agent_response`.\n\
                                                             Output printed to terminal stdout is NOT delivered to the user or rendered in the Chat Overlay.\n\n\
                                                             You MUST IMMEDIATELY invoke `post_agent_response` using your environment's native MCP tool caller with:\n\
                                                             - `dialogue`: Your natural conversational message / explanation for the user.\n\
                                                             - `artifact`: Detailed technical changes, markdown tables, plans, or diffs.\n\
                                                             - `approval`: (Optional) `{ \"title\": \"...\", \"description\": \"...\", \"options\": [...] }` for user confirmation.\n\n\
                                                             Invoke `post_agent_response` now to deliver your output to the Chat Overlay.".to_string();
                                                        tokio::spawn(async move {
                                                            tokio::time::sleep(tokio::time::Duration::from_millis(get_supervisor_shutdown_step_delay_ms())).await;
                                                            let sup = get_supervisor();
                                                            crate::logger::log_failure("supervisor", "send_input of queued prompt", sup.send_input(&key, &prompt_clone).await);
                                                        });
                                                    }
                                                }
                                                accumulated_response.clear();
                                                let proc_key = ProcessSupervisor::format_agent_key(&proj_path, &agt_id);
                                                crate::logger::log_failure("supervisor", "dispatch_next_pending_input", sup.dispatch_next_pending_input(&proc_key).await);
                                            } else {
                                                display_lines.push(line.clone());
                                            }
                                        } else {
                                            display_lines.push(line.clone());
                                        }
                                    } else if is_agy {
                                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                                            if let Some(event) = val.get("event").and_then(|v| v.as_str()) {
                                                match event {
                                                    "init" => {
                                                        let cid = val.get("conversation_id").and_then(|v| v.as_str()).unwrap_or("");
                                                        crate::logger::log("INFO", "supervisor", &format!("Agent '{}' (AGY) session initialized (conversation: {})", agt_id, cid));
                                                        display_lines.push(format!("[INIT] session initialized (conversation: {})", cid));
                                                        init_notify_rx.notify_waiters();
                                                        if !cid.is_empty() {
                                                            *sid_arc.write_safe("supervisor:agy_set_sid") = Some(cid.to_string());
                                                            if agt_id == crate::kopeng::initializer::INITIALIZER_AGENT_ID {
                                                                crate::kopeng::initializer::record_initializer_session(&proj_path, cid);
                                                            }
                                                            record_native_session(&proj_path, &agt_id, "agy", cid);
                                                        }
                                                        let sup = get_supervisor();
                                                        sup.emit_step_update(&agt_id, &proj_path, Some(cid), "DONE", "init", None, "Session initialized", None);
                                                    }
                                                    "step_update" => {
                                                        let update = val.get("step_update");
                                                        let step_type = update.and_then(|u| u.get("step_type")).and_then(|s| s.as_str()).unwrap_or("unknown");
                                                        let state = update.and_then(|u| u.get("state")).and_then(|s| s.as_str()).unwrap_or("ACTIVE");

                                                        if state == "ACTIVE" {
                                                            *proc_status_arc.write_safe("supervisor:agy:working") = ProcessStatus::Working;
                                                        }
                                                        
                                                        match step_type {
                                                             "tool" => {
                                                                let tool_name = update.and_then(|u| u.get("tool_name")).and_then(|t| t.as_str()).unwrap_or("tool");
                                                                let tool_info = update.and_then(|u| u.get("tool_info"));
                                                                let params = tool_info.and_then(|ti| ti.get("parameters"));
                                                                
                                                                crate::logger::log("DEBUG", "supervisor", &format!("Agent '{}' (AGY) executing tool: {}", agt_id, tool_name));

                                                                let action_summary = if tool_name == "call_mcp_tool" {
                                                                    let mcp_tool = params.and_then(|p| p.get("ToolName")).and_then(|t| t.as_str()).unwrap_or("mcp_tool");
                                                                    if mcp_tool == "post_agent_response" {
                                                                        crate::conversation::mark_agent_responded(&proj_path, &agt_id);
                                                                        accumulated_response.clear();
                                                                        "Delivering response to dialogue channel...".to_string()
                                                                    } else if mcp_tool == "raise_initialization_flag" {
                                                                        let flag_type = params.and_then(|p| p.get("Arguments")).and_then(|a| a.get("flag_type")).and_then(|f| f.as_str()).unwrap_or("");
                                                                        format!("Emitting initialization flag: {}", flag_type)
                                                                    } else {
                                                                        format!("Executing MCP tool: {}", mcp_tool)
                                                                    }
                                                                } else if tool_name == "post_agent_response" || tool_name.ends_with("post_agent_response") {
                                                                    crate::conversation::mark_agent_responded(&proj_path, &agt_id);
                                                                    accumulated_response.clear();
                                                                    "Delivering response to dialogue channel...".to_string()
                                                                } else if tool_name == "list_dir" {
                                                                    let dir_path = params.and_then(|p| p.get("DirectoryPath")).and_then(|d| d.as_str()).unwrap_or("");
                                                                    format!("Inspecting directory: {}", dir_path)
                                                                } else if tool_name == "view_file" {
                                                                    let f_path = params.and_then(|p| p.get("AbsolutePath")).and_then(|f| f.as_str()).unwrap_or("");
                                                                    let basename = f_path.split('/').last().unwrap_or(f_path);
                                                                    format!("Reading file: {}", basename)
                                                                } else if tool_name == "write_to_file" {
                                                                    let f_path = params.and_then(|p| p.get("TargetFile")).and_then(|f| f.as_str()).unwrap_or("");
                                                                    let basename = f_path.split('/').last().unwrap_or(f_path);
                                                                    format!("Writing file: {}", basename)
                                                                } else if tool_name == "replace_file_content" {
                                                                    let f_path = params.and_then(|p| p.get("TargetFile")).and_then(|f| f.as_str()).unwrap_or("");
                                                                    let basename = f_path.split('/').last().unwrap_or(f_path);
                                                                    format!("Editing file: {}", basename)
                                                                } else if tool_name == "grep_search" || tool_name == "find_by_name" {
                                                                    let q = params.and_then(|p| p.get("Query").or_else(|| p.get("Pattern"))).and_then(|v| v.as_str()).unwrap_or("");
                                                                    format!("Searching: {}", q)
                                                                } else if tool_name == "run_command" {
                                                                    let cmd = params.and_then(|p| p.get("CommandLine")).and_then(|c| c.as_str()).unwrap_or("");
                                                                    format!("Running: {}", cmd)
                                                                } else {
                                                                    format!("Calling tool: {}", tool_name)
                                                                };

                                                                if state == "ACTIVE" {
                                                                    if in_thought_phase {
                                                                        if !thought_line_buffer.trim().is_empty() {
                                                                            display_lines.push(format!("[THINKING] {}", thought_line_buffer.trim()));
                                                                            thought_line_buffer.clear();
                                                                        }
                                                                        in_thought_phase = false;
                                                                    }
                                                                    let params_str = params.map(|p| serde_json::to_string(p).unwrap_or_default()).unwrap_or_default();
                                                                    display_lines.push(format!("[TOOL] {} {}", tool_name, params_str));
                                                                    let sup = get_supervisor();
                                                                    let live_thought = if !accumulated_thought.is_empty() {
                                                                        &accumulated_thought
                                                                    } else {
                                                                        &accumulated_response
                                                                    };
                                                                    sup.emit_step_update(&agt_id, &proj_path, None, "ACTIVE", "tool", Some(tool_name), &action_summary, Some(live_thought));
                                                                } else {
                                                                    let dur = update.and_then(|u| u.get("duration_seconds")).and_then(|d| d.as_f64()).unwrap_or(0.0);
                                                                    display_lines.push(format!("[TOOL:DONE] {} ({:.2}s)", tool_name, dur));
                                                                    accumulated_thought.clear();
                                                                }
                                                            }
                                                            "agent_response" => {
                                                                if in_thought_phase {
                                                                    if !thought_line_buffer.trim().is_empty() {
                                                                        display_lines.push(format!("[THINKING] {}", thought_line_buffer.trim()));
                                                                        thought_line_buffer.clear();
                                                                    }
                                                                    in_thought_phase = false;
                                                                }
                                                                let delta = update.and_then(|u| {
                                                                    u.get("text_delta")
                                                                        .or_else(|| u.get("thinking_delta"))
                                                                        .or_else(|| u.get("thinking"))
                                                                        .or_else(|| u.get("text"))
                                                                }).and_then(|t| t.as_str());

                                                                if let Some(delta) = delta {
                                                                    crate::logger::log("DEBUG", "supervisor", &format!("Agent '{}' (AGY) response delta ({} bytes)", agt_id, delta.len()));
                                                                    push_str_bounded(&mut accumulated_response, delta, max_agent_response_bytes);
                                                                    push_str_bounded(&mut accumulated_thought, delta, max_agent_response_bytes);
                                                                    let trimmed = delta.trim();
                                                                    if !trimmed.is_empty() {
                                                                        display_lines.push(trimmed.to_string());
                                                                    }
                                                                }
                                                                if state == "ACTIVE" {
                                                                    let sup = get_supervisor();
                                                                    sup.emit_step_update(&agt_id, &proj_path, None, "ACTIVE", "agent_response", None, "Drafting response...", Some(&accumulated_thought));
                                                                }
                                                            }
                                                            "thought" | "thinking" => {
                                                                let thought_text = update.and_then(|u| {
                                                                    u.get("thinking_delta")
                                                                        .or_else(|| u.get("thinking"))
                                                                        .or_else(|| u.get("thought"))
                                                                        .or_else(|| u.get("text_delta"))
                                                                        .or_else(|| u.get("text"))
                                                                }).and_then(|v| v.as_str()).unwrap_or("");

                                                                if !in_thought_phase {
                                                                    accumulated_thought.clear();
                                                                    thought_line_buffer.clear();
                                                                    in_thought_phase = true;
                                                                }

                                                                if !thought_text.is_empty() {
                                                                    push_str_bounded(&mut accumulated_thought, thought_text, max_agent_response_bytes);
                                                                    push_str_bounded(&mut thought_line_buffer, thought_text, max_agent_response_bytes);

                                                                    while let Some(newline_pos) = thought_line_buffer.find('\n') {
                                                                        let line = thought_line_buffer[..newline_pos].trim().to_string();
                                                                        thought_line_buffer = thought_line_buffer[(newline_pos + 1)..].to_string();
                                                                        if !line.is_empty() {
                                                                            display_lines.push(format!("[THINKING] {}", line));
                                                                        }
                                                                    }
                                                                    if thought_line_buffer.len() >= 100 {
                                                                        if let Some(space_pos) = thought_line_buffer.rfind(' ') {
                                                                            if space_pos > 40 {
                                                                                let line = thought_line_buffer[..space_pos].trim().to_string();
                                                                                thought_line_buffer = thought_line_buffer[(space_pos + 1)..].to_string();
                                                                                if !line.is_empty() {
                                                                                    display_lines.push(format!("[THINKING] {}", line));
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                } else {
                                                                    display_lines.push("[THINKING]".to_string());
                                                                }

                                                                let sup = get_supervisor();
                                                                sup.emit_step_update(&agt_id, &proj_path, None, "ACTIVE", "thought", None, "Thinking...", Some(&accumulated_thought));
                                                            }
                                                            _ => {
                                                                display_lines.push(format!("[STEP] {} ({})", step_type, state));
                                                            }
                                                        }
                                                    }
                                                    "result" => {
                                                        *proc_status_arc.write_safe("supervisor:agy:idle") = ProcessStatus::Idle;
                                                        if in_thought_phase {
                                                            if !thought_line_buffer.trim().is_empty() {
                                                                display_lines.push(format!("[THINKING] {}", thought_line_buffer.trim()));
                                                                thought_line_buffer.clear();
                                                            }
                                                            in_thought_phase = false;
                                                        }
                                                        accumulated_thought.clear();
                                                        let res = val.get("result");
                                                        let status = res.and_then(|r| r.get("status")).and_then(|s| s.as_str()).unwrap_or("DONE");
                                                        let turns = res.and_then(|r| r.get("num_turns")).and_then(|n| n.as_u64()).unwrap_or(1);
                                                        crate::logger::log("INFO", "supervisor", &format!("Agent '{}' (AGY) turn completed (status: {}, turns: {})", agt_id, status, turns));
                                                        display_lines.push(format!("[RESULT] Status: {} (turns: {})", status, turns));
                                                        let is_interrupted_result = res.and_then(|r| r.get("error")).and_then(|e| e.as_str()) == Some("interrupted")
                                                            || status == "CANCELLED";
                                                        let sup = get_supervisor();
                                                        sup.emit_step_update(&agt_id, &proj_path, None, "DONE", "result", None, if is_interrupted_result { "Execution stopped" } else { "" }, None);

                                                        // Protocol Compliance Check: If agent completed turn without calling post_agent_response,
                                                        // fallback ingest accumulated response or inject a mandatory corrective prompt.
                                                        if !is_interrupted_result && crate::conversation::should_inject_compliance_reminder(&proj_path, &agt_id) {
                                                            let trimmed_fallback = accumulated_response.trim().to_string();
                                                            if !trimmed_fallback.is_empty() {
                                                                crate::logger::log(
                                                                    "WARN",
                                                                    "supervisor",
                                                                    &format!("Agent '{}' completed turn via stdout/agent_response fallback without calling MCP post_agent_response. Ingesting {} chars.", agt_id, trimmed_fallback.len())
                                                                );
                                                                persist_fallback_agent_response(&proj_path, &agt_id, &trimmed_fallback);
                                                                crate::conversation::mark_agent_responded(&proj_path, &agt_id);
                                                            } else if status == "ERROR" {
                                                                let err_msg = res.and_then(|r| r.get("error")).and_then(|e| e.as_str()).unwrap_or("Agent execution terminated with error without output.");
                                                                crate::logger::log("WARN", "supervisor", &format!("Agent '{}' terminated with ERROR and no text: {}", agt_id, err_msg));
                                                                persist_fallback_agent_response(&proj_path, &agt_id, &format!("⚠️ **Execution Error**: {}", err_msg));
                                                                report_agent_failure(&proj_path, &agt_id, err_msg, false);
                                                                crate::conversation::mark_agent_responded(&proj_path, &agt_id);
                                                            } else {
                                                                crate::logger::log(
                                                                    "WARN",
                                                                    "supervisor",
                                                                    &format!("Agent '{}' finished turn without calling post_agent_response and without text output. Injecting mandatory compliance prompt.", agt_id)
                                                                );
                                                                let key = ProcessSupervisor::format_agent_key(&proj_path, &agt_id);
                                                                let prompt_clone = "## MANDATORY PROTOCOL CORRECTION:\n\
                                                                     You completed your turn without calling the MCP tool `post_agent_response`.\n\
                                                                     Output printed to terminal stdout is NOT delivered to the user or rendered in the Chat Overlay.\n\n\
                                                                     You MUST IMMEDIATELY invoke `post_agent_response` using your environment's native MCP tool caller (`call_mcp_tool` with ServerName `\"one-ship\"`) with:\n\
                                                                     - `dialogue`: Your natural conversational message / explanation for the user (TTS-ready, concise, no markdown tables/backticks).\n\
                                                                     - `artifact`: Detailed technical changes, markdown tables, plans, or diffs.\n\
                                                                     - `approval`: (Optional) `{ \"title\": \"...\", \"description\": \"...\", \"options\": [...] }` for user confirmation.\n\n\
                                                                     Invoke `post_agent_response` now to deliver your output to the Chat Overlay.".to_string();
                                                                tokio::spawn(async move {
                                                                    tokio::time::sleep(tokio::time::Duration::from_millis(get_supervisor_shutdown_step_delay_ms())).await;
                                                                    let sup = get_supervisor();
                                                                    crate::logger::log_failure("supervisor", "send_input of queued prompt", sup.send_input(&key, &prompt_clone).await);
                                                                });
                                                            }
                                                        }
                                                        accumulated_response.clear();
                                                        let proc_key = ProcessSupervisor::format_agent_key(&proj_path, &agt_id);
                                                        crate::logger::log_failure("supervisor", "dispatch_next_pending_input", sup.dispatch_next_pending_input(&proc_key).await);
                                                    }
                                                    _ => {
                                                        display_lines.push(line.clone());
                                                    }
                                                }
                                            } else {
                                                display_lines.push(line.clone());
                                            }
                                        } else {
                                            display_lines.push(line.clone());
                                        }
                                    } else if is_claude {
                                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                                            let msg_type = val.get("type").and_then(|v| v.as_str()).unwrap_or("");
                                            match msg_type {
                                                "system" => {
                                                    let sid = val.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
                                                    if !sid.is_empty() {
                                                        *sid_arc.write_safe("supervisor:claude_set_sid") = Some(sid.to_string());
                                                        crate::logger::log("INFO", "supervisor", &format!("Agent '{}' (Claude) session initialized (session_id: {})", agt_id, sid));
                                                        display_lines.push(format!("[INIT] session initialized (conversation: {})", sid));
                                                        record_native_session(&proj_path, &agt_id, "claude", sid);
                                                        if agt_id == crate::kopeng::initializer::INITIALIZER_AGENT_ID {
                                                            crate::kopeng::initializer::record_initializer_session(&proj_path, sid);
                                                        }
                                                    }
                                                    init_notify_rx.notify_waiters();
                                                    let sup = get_supervisor();
                                                    sup.emit_step_update(&agt_id, &proj_path, Some(sid), "DONE", "init", None, "Session initialized", None);
                                                }
                                                "stream_event" => {
                                                    *proc_status_arc.write_safe("supervisor:claude:working") = ProcessStatus::Working;
                                                    let event_payload = val.get("event").unwrap_or(&val);

                                                    // Text delta
                                                    if let Some(delta) = event_payload.get("text_delta").and_then(|v| v.as_str())
                                                        .or_else(|| event_payload.get("delta").and_then(|d| d.get("text")).and_then(|v| v.as_str())) {
                                                        if in_thought_phase {
                                                            if !thought_line_buffer.trim().is_empty() {
                                                                display_lines.push(format!("[THINKING] {}", thought_line_buffer.trim()));
                                                                thought_line_buffer.clear();
                                                            }
                                                            in_thought_phase = false;
                                                        }
                                                        push_str_bounded(&mut accumulated_response, delta, max_agent_response_bytes);
                                                        push_str_bounded(&mut accumulated_thought, delta, max_agent_response_bytes);
                                                        let trimmed = delta.trim();
                                                        if !trimmed.is_empty() {
                                                            display_lines.push(trimmed.to_string());
                                                        }
                                                        let sup = get_supervisor();
                                                        sup.emit_step_update(&agt_id, &proj_path, None, "ACTIVE", "agent_response", None, "Drafting response...", Some(&accumulated_thought));
                                                    }
                                                    // Thinking delta
                                                    else if let Some(thought_delta) = event_payload.get("thinking_delta").and_then(|v| v.as_str())
                                                        .or_else(|| event_payload.get("delta").and_then(|d| d.get("thinking")).and_then(|v| v.as_str())) {
                                                        if !in_thought_phase {
                                                            accumulated_thought.clear();
                                                            thought_line_buffer.clear();
                                                            in_thought_phase = true;
                                                        }
                                                        if !thought_delta.is_empty() {
                                                            push_str_bounded(&mut accumulated_thought, thought_delta, max_agent_response_bytes);
                                                            push_str_bounded(&mut thought_line_buffer, thought_delta, max_agent_response_bytes);
                                                            while let Some(newline_pos) = thought_line_buffer.find('\n') {
                                                                let line = thought_line_buffer[..newline_pos].trim().to_string();
                                                                thought_line_buffer = thought_line_buffer[(newline_pos + 1)..].to_string();
                                                                if !line.is_empty() {
                                                                    display_lines.push(format!("[THINKING] {}", line));
                                                                }
                                                            }
                                                        }
                                                        let sup = get_supervisor();
                                                        sup.emit_step_update(&agt_id, &proj_path, None, "ACTIVE", "thought", None, "Thinking...", Some(&accumulated_thought));
                                                    }
                                                }
                                                "assistant" => {
                                                    *proc_status_arc.write_safe("supervisor:claude:working") = ProcessStatus::Working;
                                                    let message = val.get("message").unwrap_or(&val);
                                                    if let Some(content_blocks) = message.get("content").and_then(|c| c.as_array()) {
                                                        for block in content_blocks {
                                                            let b_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                                                            if b_type == "tool_use" {
                                                                let tool_name = block.get("name").and_then(|n| n.as_str()).unwrap_or("tool");
                                                                let tool_input = block.get("input");
                                                                let action_summary = if tool_name.ends_with("post_agent_response") {
                                                                    crate::conversation::mark_agent_responded(&proj_path, &agt_id);
                                                                    accumulated_response.clear();
                                                                    "Delivering response to dialogue channel...".to_string()
                                                                } else if tool_name.ends_with("raise_initialization_flag") {
                                                                    let flag_type = tool_input.and_then(|p| p.get("flag_type")).and_then(|f| f.as_str()).unwrap_or("");
                                                                    format!("Emitting initialization flag: {}", flag_type)
                                                                } else {
                                                                    format!("Executing tool: {}", tool_name)
                                                                };
                                                                let params_str = tool_input.map(|p| serde_json::to_string(p).unwrap_or_default()).unwrap_or_default();
                                                                display_lines.push(format!("[TOOL] {} {}", tool_name, params_str));
                                                                let sup = get_supervisor();
                                                                let live_thought = if !accumulated_thought.is_empty() {
                                                                    &accumulated_thought
                                                                } else {
                                                                    &accumulated_response
                                                                };
                                                                sup.emit_step_update(&agt_id, &proj_path, None, "ACTIVE", "tool", Some(tool_name), &action_summary, Some(live_thought));
                                                            } else if b_type == "text" {
                                                                if let Some(text_content) = block.get("text").and_then(|t| t.as_str()) {
                                                                    if !accumulated_response.contains(text_content) {
                                                                        push_str_bounded(&mut accumulated_response, text_content, max_agent_response_bytes);
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                                "rate_limit_event" => {
                                                    let rate_info = val.get("rate_limit_info");
                                                    let windows = rate_info.and_then(|r| r.get("unifiedWindows"));
                                                    if let Some(w) = windows {
                                                        let five_hour = w.get("five_hour");
                                                        let seven_day = w.get("seven_day");

                                                        let five_hour_remaining_pct = five_hour
                                                            .and_then(|f| f.get("utilization"))
                                                            .and_then(|u| u.as_f64())
                                                            .map(|u| ((1.0 - u) * 100.0).clamp(0.0, 100.0));
                                                        let five_hour_reset_epoch = five_hour
                                                            .and_then(|f| f.get("resetsAt"))
                                                            .and_then(|r| r.as_i64());

                                                        let weekly_remaining_pct = seven_day
                                                            .and_then(|s| s.get("utilization"))
                                                            .and_then(|u| u.as_f64())
                                                            .map(|u| ((1.0 - u) * 100.0).clamp(0.0, 100.0));
                                                        let weekly_reset_epoch = seven_day
                                                            .and_then(|s| s.get("resetsAt"))
                                                            .and_then(|r| r.as_i64());

                                                        let active_account_id = crate::db::db_get_active_account_by_provider("claude")
                                                            .ok()
                                                            .flatten()
                                                            .map(|a| a.id);

                                                        if let Some(aid) = active_account_id {
                                                            let now_secs = (epoch_now_ms() / 1000) as i64;
                                                            let quota_rec = crate::db::AccountQuotaRecord {
                                                                id: None,
                                                                account_id: aid,
                                                                provider: "claude".to_string(),
                                                                model: "claude".to_string(),
                                                                five_hour_remaining_pct,
                                                                five_hour_resets_at: None,
                                                                five_hour_reset_epoch_seconds: five_hour_reset_epoch,
                                                                five_hour_resets_at_local: None,
                                                                weekly_remaining_pct,
                                                                weekly_resets_at: None,
                                                                weekly_reset_epoch_seconds: weekly_reset_epoch,
                                                                weekly_resets_at_local: None,
                                                                last_pinged_at: Some(now_secs),
                                                                warmup_enabled: false,
                                                                status: "active".to_string(),
                                                                updated_at: now_secs,
                                                            };
                                                            crate::logger::log_failure("supervisor", "db_upsert_account_quota", crate::db::db_upsert_account_quota(&quota_rec));
                                                            let sup = get_supervisor();
                                                            match crate::db::db_get_account_quotas() {
                                                                Ok(quotas) => sup.emit_event(DaemonEvent::account_quotas_updated(&quotas)),
                                                                Err(e) => crate::logger::log("WARN", "supervisor", &format!("db_get_account_quotas failed: {}", e)),
                                                            }
                                                        }
                                                    }
                                                }
                                                "result" => {
                                                    *proc_status_arc.write_safe("supervisor:claude:idle") = ProcessStatus::Idle;
                                                    if in_thought_phase {
                                                        if !thought_line_buffer.trim().is_empty() {
                                                            display_lines.push(format!("[THINKING] {}", thought_line_buffer.trim()));
                                                            thought_line_buffer.clear();
                                                        }
                                                        in_thought_phase = false;
                                                    }
                                                    accumulated_thought.clear();
                                                    let is_error = val.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false);
                                                    let usage = val.get("usage");
                                                    let total_tokens = usage
                                                        .and_then(|u| u.get("total_tokens").or_else(|| u.get("totalTokens")))
                                                        .and_then(|t| t.as_u64())
                                                        .unwrap_or_else(|| {
                                                            let inp = usage.and_then(|u| u.get("input_tokens")).and_then(|v| v.as_u64()).unwrap_or(0);
                                                            let cache_create = usage.and_then(|u| u.get("cache_creation_input_tokens")).and_then(|v| v.as_u64()).unwrap_or(0);
                                                            let cache_read = usage.and_then(|u| u.get("cache_read_input_tokens")).and_then(|v| v.as_u64()).unwrap_or(0);
                                                            let out = usage.and_then(|u| u.get("output_tokens")).and_then(|v| v.as_u64()).unwrap_or(0);
                                                            inp + cache_create + cache_read + out
                                                        });
                                                    let cost_opt = val.get("total_cost_usd").and_then(|v| v.as_f64())
                                                        .or_else(|| usage.and_then(|u| u.get("cost_usd")).and_then(|v| v.as_f64()));
                                                    let runner_str = if is_agy { "agy" } else { "claude" };
                                                    let max_ctx = crate::kopeng::stats::context_window_tokens_count(Some(runner_str), target_m.as_deref()).ok().flatten();
                                                    crate::logger::log("INFO", "supervisor", &format!("Agent '{}' ({}) turn completed: is_error={}, {} tokens used", agt_id, runner_str, is_error, total_tokens));
                                                    display_lines.push(format!("[RESULT] Status: {} (tokens: {})", if is_error { "ERROR" } else { "DONE" }, total_tokens));

                                                    let sup = get_supervisor();
                                                    sup.emit_event(DaemonEvent::agent_usage_update(&crate::events::AgentUsageUpdatePayload {
                                                        agent_id: agt_id.clone(),
                                                        project_path: proj_path.clone(),
                                                        runner: runner_str.to_string(),
                                                        model: target_m.clone(),
                                                        tokens_used_count: total_tokens,
                                                        max_context_tokens_count: max_ctx,
                                                        cost_dollars: cost_opt,
                                                        timestamp_ms: epoch_now_ms(),
                                                    }));
                                                    let p_clone = proj_path.clone();
                                                    let a_clone = agt_id.clone();
                                                    let m_clone = target_m.clone();
                                                    let r_clone = runner_str.to_string();
                                                    tokio::task::spawn_blocking(move || {
                                                        crate::logger::log_failure(
                                                            "supervisor",
                                                            "db_upsert_live_session_usage",
                                                            crate::db::db_upsert_live_session_usage(&p_clone, &a_clone, &r_clone, m_clone.as_deref(), total_tokens, max_ctx, cost_opt),
                                                        );
                                                    });
                                                    sup.emit_step_update(&agt_id, &proj_path, None, "DONE", "result", None, if is_error { "Execution error" } else { "" }, None);

                                                    if crate::conversation::should_inject_compliance_reminder(&proj_path, &agt_id) {
                                                        let trimmed_fallback = accumulated_response.trim().to_string();
                                                        if !trimmed_fallback.is_empty() {
                                                            crate::logger::log(
                                                                "WARN",
                                                                "supervisor",
                                                                &format!("Agent '{}' (Claude) completed turn via fallback without calling post_agent_response. Ingesting {} chars.", agt_id, trimmed_fallback.len())
                                                            );
                                                            persist_fallback_agent_response(&proj_path, &agt_id, &trimmed_fallback);
                                                            crate::conversation::mark_agent_responded(&proj_path, &agt_id);
                                                        } else if is_error {
                                                            let err_msg = val.get("error").or_else(|| val.get("message")).and_then(|v| v.as_str()).unwrap_or("Claude execution error.");
                                                            crate::logger::log("WARN", "supervisor", &format!("Agent '{}' (Claude) terminated with ERROR: {}", agt_id, err_msg));
                                                            persist_fallback_agent_response(&proj_path, &agt_id, &format!("⚠️ **Execution Error**: {}", err_msg));
                                                            report_agent_failure(&proj_path, &agt_id, err_msg, false);
                                                            crate::conversation::mark_agent_responded(&proj_path, &agt_id);
                                                        } else {
                                                            let key = ProcessSupervisor::format_agent_key(&proj_path, &agt_id);
                                                            let prompt_clone = "## MANDATORY PROTOCOL CORRECTION:\n\
                                                                 You completed your turn without calling the MCP tool `post_agent_response`.\n\
                                                                 Output printed to terminal stdout is NOT delivered to the user or rendered in the Chat Overlay.\n\n\
                                                                 You MUST IMMEDIATELY invoke `post_agent_response` using your environment's native MCP tool caller with:\n\
                                                                 - `dialogue`: Your natural conversational message / explanation for the user.\n\
                                                                 - `artifact`: Detailed technical changes, markdown tables, plans, or diffs.\n\
                                                                 - `approval`: (Optional) `{ \"title\": \"...\", \"description\": \"...\", \"options\": [...] }` for user confirmation.\n\n\
                                                                 Invoke `post_agent_response` now to deliver your output to the Chat Overlay.".to_string();
                                                            tokio::spawn(async move {
                                                                tokio::time::sleep(tokio::time::Duration::from_millis(get_supervisor_shutdown_step_delay_ms())).await;
                                                                let sup = get_supervisor();
                                                                crate::logger::log_failure("supervisor", "send_input of queued prompt", sup.send_input(&key, &prompt_clone).await);
                                                            });
                                                        }
                                                    }
                                                    accumulated_response.clear();
                                                    let proc_key = ProcessSupervisor::format_agent_key(&proj_path, &agt_id);
                                                    crate::logger::log_failure("supervisor", "dispatch_next_pending_input", sup.dispatch_next_pending_input(&proc_key).await);
                                                }
                                                _ => {
                                                    display_lines.push(line.clone());
                                                }
                                            }
                                        } else {
                                            display_lines.push(line.clone());
                                        }
                                    } else {
                                        display_lines.push(line.clone());
                                    }

                                    for dl in display_lines {
                                        if !dl.is_empty() {
                                            let mut r = ring.write_safe("supervisor:agent_output_ring");
                                            if r.len() >= max_lines_count {
                                                crate::logger::log("DEBUG", "supervisor", &format!("Agent '{}' output ring buffer full ({} lines), dropping oldest line", agt_id, max_lines_count));
                                                r.pop_front();
                                            }
                                            r.push_back(dl);
                                        }
                                    }
                                }
                                Ok(None) => break,
                                Err(_) => break,
                            }
                        }
                    }
                }
            });
        }

        // Stderr reader task
        if let Some(child_stderr) = child.stderr.take() {
            let agt_err_id = agent_id.to_string();
            let agt_err_project_path = project_path.to_string();
            let on_stderr_line = Arc::new(move |line: &str| {
                let lower = line.to_lowercase();
                if lower.contains("resource_exhausted")
                    || lower.contains("429")
                    || lower.contains("insufficient_quota")
                    || lower.contains("rate limit")
                    || lower.contains("token limit") {
                    let sup = get_supervisor();
                    sup.emit_agent_error(
                        &agt_err_id,
                        &agt_err_project_path,
                        "quota_exhausted",
                        "Token limit or quota exhausted (429/RESOURCE_EXHAUSTED). Switch to an account with active quota or lighter model.",
                        0,
                    );
                }
            });
            spawn_stream_reader(
                child_stderr,
                ring_arc.clone(),
                cancel_token.clone(),
                buffer_lines_count,
                Some("[STDERR]"),
                Some(on_stderr_line),
                format!("agent-stderr:{}", agent_id),
            );
        }

        let started_at_ms = epoch_now_ms();
        let run_id = format!("agt_{}_{}", agent_id, started_at_ms);

        let (acc_id, acc_email) = if is_opencode {
            (
                opencode_acc_id.or_else(|| Some("deepseek".to_string())),
                opencode_acc_email.or_else(|| Some("DeepSeek (Local API Key)".to_string())),
            )
        } else if is_agy {
            if agy_acc_id.is_some() {
                (agy_acc_id, agy_acc_email)
            } else if let Ok(accounts) = crate::db::db_get_accounts() {
                if let Some(active) = accounts.into_iter().find(|a| a.is_active && (a.provider == "google" || a.provider.is_empty())) {
                    (Some(active.id), Some(active.email))
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            }
        } else if is_claude {
            if claude_acc_id.is_some() {
                (claude_acc_id, claude_acc_email)
            } else if let Ok(accounts) = crate::db::db_get_accounts() {
                if let Some(active) = accounts.into_iter().find(|a| a.is_active && (a.provider == "claude" || a.provider == "anthropic")) {
                    (Some(active.id), Some(active.email))
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        let is_interrupted = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let pending_correction: Arc<RwLock<Option<String>>> = Arc::new(RwLock::new(None));

        let managed = Arc::new(ManagedProcess {
            key: key.clone(),
            id: agent_id.to_string(),
            project_path: project_path.to_string(),
            pid,
            status: status_arc.clone(),
            stdin_tx,
            cancel_token: cancel_token.clone(),
            started_at_ms,
            output_ring: ring_arc.clone(),
            run_id,
            runner: Some(runner_bin.to_string()),
            model: model.map(|s| s.to_string()),
            effort: effort.map(|s| s.to_string()),
            account_id: acc_id.clone(),
            account_email: acc_email,
            is_interrupted: is_interrupted.clone(),
            pending_correction: pending_correction.clone(),
            pending_inputs: Arc::new(RwLock::new(VecDeque::new())),
            pending_preamble: Arc::new(RwLock::new(None)),
            stop_grace_period_ms: get_supervisor_shutdown_grace_period_ms(),
        });

        {
            let mut procs = self.processes.write_safe("supervisor:spawn_agent:insert");
            procs.insert(key.clone(), managed);
        }

        let key_clone = key.clone();
        let agent_id_clone = agent_id.to_string();
        let ring_clone = ring_arc.clone();
        let is_interrupted_clone = is_interrupted.clone();
        let pending_correction_clone = pending_correction.clone();
        let project_path_clone = project_path.to_string();
        let runner_clone = Some(runner_bin.to_string());
        let model_clone = model.map(|s| s.to_string());
        let effort_clone = effort.map(|s| s.to_string());
        let account_id_clone = acc_id;
        let instance_id_clone = instance_id.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    let _ = child.start_kill();
                    #[cfg(target_os = "linux")]
                    unsafe {
                        kill_process_and_group(pid, libc::SIGKILL);
                    }
                }
                status_res = child.wait() => {
                    let sup = crate::supervisor::get_supervisor();
                    match status_res {
                        Ok(st) => {
                            let code = st.code().unwrap_or(0);
                            if code == 0 {
                                #[cfg(target_os = "linux")]
                                unsafe {
                                    kill_process_and_group(pid, libc::SIGKILL);
                                }
                                *status_arc.write_safe("supervisor:spawn_agent:status_ok") = ProcessStatus::Completed { exit_code: code };
                                crate::logger::log("INFO", "supervisor", &format!("Agent '{}' exited with code 0, state transitioned to Completed", key_clone));
                                sup.emit_status_report(&agent_id_clone, &project_path_clone, "completed");
                                sup.emit_step_update(&agent_id_clone, &project_path_clone, None, "DONE", "result", None, "Process completed cleanly", None);
                            } else {
                                let ring_lines = {
                                    let r = ring_clone.read_safe("supervisor:spawn_agent:inspect_err");
                                    r.iter().rev().take(20).cloned().collect::<Vec<_>>()
                                };
                                let combined_output = ring_lines.join("\n").to_lowercase();
                                let was_interrupted = is_interrupted_clone.load(std::sync::atomic::Ordering::SeqCst)
                                    || combined_output.contains("interrupted");

                                if was_interrupted {
                                    *status_arc.write_safe("supervisor:spawn_agent:status_interrupted") = ProcessStatus::Idle;
                                    crate::logger::log(
                                        "INFO",
                                        "supervisor",
                                        &format!("Agent '{}' interrupted by user. Re-arming agent in idle state...", key_clone),
                                    );

                                    // Retain the output ring so it survives interruption and re-arming
                                    let mut retained_lines = ring_clone.read_safe("supervisor:interrupted:ring").clone();
                                    let divider = "[SYSTEM] Execution interrupted by user — re-arming agent in idle state".to_string();
                                    let max_lines = get_supervisor_max_buffer_lines_count();
                                    if retained_lines.len() >= max_lines {
                                        retained_lines.pop_front();
                                    }
                                    retained_lines.push_back(divider);
                                    sup.store_retained_buffer(&key_clone, retained_lines);

                                    {
                                        let mut procs = sup.processes.write_safe("supervisor:interrupted:cleanup");
                                        procs.remove(&key_clone);
                                    }

                                    let project_name = crate::db::db_get_projects()
                                        .ok()
                                        .and_then(|ps| ps.into_iter().find(|p| p.path == project_path_clone).map(|p| p.name))
                                        .unwrap_or_else(|| {
                                            std::path::Path::new(&project_path_clone)
                                                .file_name()
                                                .and_then(|n| n.to_str())
                                                .unwrap_or("project")
                                                .to_string()
                                        });

                                    let correction = pending_correction_clone.write_safe("supervisor:get_correction").take();
                                    let req = AgentRearmRequest {
                                        project_path: project_path_clone,
                                        project_name,
                                        agent_id: agent_id_clone,
                                        runner: runner_clone,
                                        model: model_clone,
                                        effort: effort_clone,
                                        account_id: account_id_clone,
                                        correction,
                                    };
                                    crate::logger::log_failure("supervisor", "enqueue agent rearm request", sup.rearm_tx.try_send(req));
                                } else {
                                    #[cfg(target_os = "linux")]
                                    unsafe {
                                        kill_process_and_group(pid, libc::SIGKILL);
                                    }
                                    let (err_type, user_msg) = if combined_output.contains("resource_exhausted")
                                        || combined_output.contains("429")
                                        || combined_output.contains("quota")
                                        || combined_output.contains("token limit")
                                        || combined_output.contains("rate limit")
                                        || combined_output.contains("insufficient_quota") {
                                        ("quota_exhausted", "Token limit or quota exhausted (429/RESOURCE_EXHAUSTED). Switch to an account with active quota or lighter model.")
                                    } else if combined_output.contains("unauthenticated")
                                        || combined_output.contains("invalid api key")
                                        || combined_output.contains("unauthorized")
                                        || combined_output.contains("permission denied") {
                                        ("auth_failed", "Authentication failed. Verify API key or account credentials.")
                                    } else {
                                        ("process_exit", "Agent process terminated abnormally.")
                                    };

                                    let err_desc = format!("Exit code {}: {}", code, user_msg);
                                    *status_arc.write_safe("supervisor:spawn_agent:status_err") = ProcessStatus::Failed { error: err_desc.clone() };
                                    crate::logger::log("WARN", "supervisor", &format!("Agent '{}' failed with code {}: {}", key_clone, code, user_msg));
                                    // `ring_lines` is newest-first; show the last lines in order so the cause is visible.
                                    let output_tail: String = ring_lines.iter().take(5).rev().cloned().collect::<Vec<_>>().join("\n");
                                    let output_tail: String = output_tail.chars().take(600).collect();
                                    let failure_reason = if output_tail.trim().is_empty() { err_desc.clone() } else { format!("{}\n```\n{}\n```", err_desc, output_tail) };
                                    report_agent_failure(&project_path_clone, &agent_id_clone, &failure_reason, true);

                                    sup.emit_status_report(&agent_id_clone, &project_path_clone, "failed");
                                    sup.emit_agent_error(&agent_id_clone, &project_path_clone, err_type, user_msg, code);
                                    sup.emit_step_update(&agent_id_clone, &project_path_clone, None, "DONE", "result", None, &err_desc, None);
                                }
                            }
                        }
                        Err(e) => {
                            let err_msg = e.to_string();
                            *status_arc.write_safe("supervisor:spawn_agent:status_err") = ProcessStatus::Failed { error: err_msg.clone() };
                            crate::logger::log("ERROR", "supervisor", &format!("Agent '{}' wait error: {}, state transitioned to Failed", key_clone, err_msg));
                            sup.emit_status_report(&agent_id_clone, &project_path_clone, "failed");
                            sup.emit_agent_error(&agent_id_clone, &project_path_clone, "process_exit", &format!("Process execution error: {}", err_msg), -1);
                            sup.emit_step_update(&agent_id_clone, &project_path_clone, None, "DONE", "result", None, &format!("Process failed: {}", err_msg), None);
                        }
                    }
                }
            }
            if let Err(e) = crate::db::db_terminate_agent_instance(&instance_id_clone, epoch_now_ms()) {
                crate::logger::log("ERROR", "supervisor", &format!("Failed to terminate instance '{}' of agent '{}': {}", instance_id_clone, key_clone, e));
            }
        });

        crate::logger::log("INFO", "supervisor", &format!("Spawned agent '{}' (PID {}, instance {}) in {}", agent_id, pid, instance_id, project_path));
        Ok(pid)
    }

    /// Determines whether an agent ID belongs to an ephemeral initialization flag resolution agent.
    pub fn is_flag_resolution_agent(agent_id: &str) -> bool {
        let lower = agent_id.to_ascii_lowercase();
        lower == "docs-resolver"
            || lower == "todos-resolver"
            || lower == "techdebt-resolver"
            || lower == "profiler-crafter"
            || lower == "web-builder-resolver"
            || lower == "devserver-resolver"
            || lower.ends_with("-resolver")
            || lower.ends_with("_resolver")
            || lower.ends_with("-crafter")
            || lower.ends_with("_crafter")
    }

    pub fn compute_project_state(&self, project_path: &str, project_name: &str) -> ProjectAgentState {
        let mut state = ProjectAgentState {
            project_path: project_path.to_string(),
            project_name: project_name.to_string(),
            agents: Vec::new(),
        };

        // Discover agents from rula.toml
        let system_mcp_dialogue_mode = crate::config::load_project_rula(Path::new(project_path))
            .and_then(|rula| rula.get("system").and_then(|s| s.get("mcp_dialogue_mode")).and_then(|v| v.as_bool()))
            .unwrap_or(false);

        let system_auto_approve = crate::config::load_project_rula(Path::new(project_path))
            .and_then(|rula| rula.get("system").and_then(|s| s.get("auto_approve")).and_then(|v| v.as_bool()))
            .unwrap_or(false);

        if let Some(rula) = crate::config::load_project_rula(Path::new(project_path)) {
            if let Some(agents_table) = rula.get("agents").and_then(|v| v.as_table()) {
                for (agent_id, agent_val) in agents_table {
                    // Platform agents and ephemeral flag resolution agents must never be injected as personas.
                    if Self::is_flag_resolution_agent(agent_id)
                        || crate::kopeng::persona::RESERVED_AGENT_IDS.iter().any(|id| id.eq_ignore_ascii_case(agent_id))
                    {
                        continue;
                    }
                    let role = agent_val.get("role").and_then(|v| v.as_str()).unwrap_or("Project Specialist").to_string();
                    // `load_project_rula` returns only validated files, where every persona has a
                    // runner/model pair of its manifest (`config::agent_persona_problems`).
                    let (Some(runner), Some(model)) = (
                        agent_val.get("runner").and_then(|v| v.as_str()).map(String::from),
                        agent_val.get("model").and_then(|v| v.as_str()).map(String::from),
                    ) else {
                        debug_assert!(false, "validated rula.toml has a persona without runner/model");
                        crate::logger::log("ERROR", "supervisor", &format!("Persona '{}' in {} has no runner/model pair; skipped", agent_id, project_path));
                        continue;
                    };
                    let model = Some(model);
                    let effort = agent_val.get("effort").and_then(|v| v.as_str()).map(|s| s.to_string());
                    let accept_edits = agent_val.get("accept_edits").and_then(|v| v.as_bool());
                    let auto_approve = agent_val.get("auto_approve").and_then(|v| v.as_bool()).unwrap_or(system_auto_approve);
                    let args = agent_val.get("args").and_then(|v| v.as_array()).map(|arr| arr.iter().filter_map(|s| s.as_str().map(|str| str.to_string())).collect()).unwrap_or_default();
                    let context_docs = agent_val.get("context_docs").and_then(|v| v.as_array()).map(|arr| arr.iter().filter_map(|s| s.as_str().map(|str| str.to_string())).collect()).unwrap_or_default();
                    let skills = agent_val.get("skills").and_then(|v| v.as_array()).map(|arr| arr.iter().filter_map(|s| s.as_str().map(|str| str.to_string())).collect()).unwrap_or_default();
                    let system_prompt = agent_val.get("system_prompt").and_then(|v| v.as_str()).map(|s| s.to_string());
                    let mcp_dialogue_mode = agent_val.get("mcp_dialogue_mode").and_then(|v| v.as_bool()).unwrap_or(system_mcp_dialogue_mode);
                    let dna = agent_val.get("dna").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| generate_random_dna(agent_id));

                    let key = Self::format_agent_key(project_path, agent_id);
                    let st = self.get_status(&key);
                    let (pid, acc_id, acc_email) = {
                        let procs = self.processes.read_safe("supervisor:compute_project_state:rula");
                        if let Some(p) = procs.get(&key) {
                            (Some(p.pid), p.account_id.clone(), p.account_email.clone())
                        } else {
                            (None, None, None)
                        }
                    };

                    state.agents.push(AgentState {
                        id: agent_id.clone(),
                        role,
                        runner,
                        model,
                        effort,
                        accept_edits,
                        status: st.as_str().to_string(),
                        pid,
                        is_internal: false,
                        auto_approve,
                        args,
                        context_docs,
                        skills,
                        system_prompt,
                        mcp_dialogue_mode,
                        dna: Some(dna),
                        account_id: acc_id,
                        account_email: acc_email,
                    });
                }
            }
        }

        // 3. Transient Initializer Persona Injection
        let rula_exists = Path::new(project_path).join("rula.toml").exists();
        let init_key = Self::format_agent_key(project_path, "initializer");
        let init_status = self.get_status(&init_key);
        // compute_project_state cannot fail (watcher tick): an unreadable init state is logged and
        // the process status alone decides below.
        let init_running_in_db = match crate::db::db_get_init_state(project_path) {
            Ok(state) => state.is_some_and(|st| st.status == "running"),
            Err(e) => {
                crate::logger::log("ERROR", "supervisor", &format!("Cannot read init state of {}: {}", project_path, e));
                false
            }
        };

        let is_init_actively_running = init_running_in_db
            || matches!(init_status, ProcessStatus::Working | ProcessStatus::Starting | ProcessStatus::Idle | ProcessStatus::Blocked);
        let should_inject_initializer = !rula_exists || is_init_actively_running;

        if should_inject_initializer {
            let (pid, running_runner, running_model, running_effort, acc_id, acc_email) = {
                let procs = self.processes.read_safe("supervisor:compute_project_state:init");
                if let Some(p) = procs.get(&init_key) {
                    (Some(p.pid), p.runner.clone(), p.model.clone(), p.effort.clone(), p.account_id.clone(), p.account_email.clone())
                } else {
                    (None, None, None, None, None, None)
                }
            };
            let status_str = init_status.as_str().to_string();

            // A running initializer shows the pair it was spawned with; otherwise the configured one.
            let (init_runner, init_model, init_effort) = match running_runner {
                Some(runner) => (runner, running_model, running_effort),
                None => platform_agent_pair(project_path, None, &crate::kopeng::resolution::INIT_AGENT_KEYS),
            };

            state.agents.push(AgentState {
                id: "initializer".to_string(),
                role: "Project Initializer".to_string(),
                runner: init_runner,
                model: init_model,
                effort: init_effort,
                accept_edits: Some(true),
                status: status_str,
                pid,
                is_internal: true,
                auto_approve: true,
                args: Vec::new(),
                context_docs: Vec::new(),
                skills: Vec::new(),
                system_prompt: Some("You are the ONE-SHIP Project Initializer. Non-invasive reconnaissance and configuration generation.".to_string()),
                mcp_dialogue_mode: true,
                dna: Some(generate_random_dna("initializer")),
                account_id: acc_id,
                account_email: acc_email,
            });
        }

        // 4. Discover active initialization flags & resolution agents from SQLite
        let project_record = match crate::registry::find_project_by_path(project_path) {
            Ok(p) => Some(p),
            Err(crate::registry::RegistryError::PathNotRegistered(_)) => None,
            Err(e) => {
                crate::logger::log("ERROR", "supervisor", &format!("compute_project_state: cannot resolve project '{}': {}", project_path, e));
                None
            }
        };
        if let Some(project_record) = project_record {
            let project_id = project_record.id as i64;
            if let Ok(flags) = crate::db::db_get_init_flags(project_id, None) {
                for flag in flags {
                    if flag.status == "pending" || flag.status == "in_progress" {
                        let (agent_id, role) = match flag.flag_type.as_str() {
                            "docs" => ("docs-resolver", "Documentation Architect & Resolver"),
                            "todos" => ("todos-resolver", "Task Migration Specialist"),
                            "tech_debt" => ("techdebt-resolver", "Technical Debt Remediator"),
                            "profiler" => ("profiler-crafter", "Performance Profiling Crafter"),
                            crate::kopeng::flag_resolver::REMOVAL_FLAG_TYPE => (crate::kopeng::flag_resolver::REMOVAL_AGENT_ID, "Project Removal Assistant"),
                            crate::kopeng::flag_resolver::INVALID_CONFIG_FLAG_TYPE => (crate::kopeng::flag_resolver::CONFIG_AGENT_ID, "Project Configuration Repair"),
                            _ => continue,
                        };

                        if !state.agents.iter().any(|a| a.id.eq_ignore_ascii_case(agent_id)) {
                            let key = Self::format_agent_key(project_path, agent_id);
                            let st = self.get_status(&key);
                            let (pid, acc_id, acc_email) = {
                                let procs = self.processes.read_safe("supervisor:compute_project_state:flags");
                                if let Some(p) = procs.get(&key) {
                                    (Some(p.pid), p.account_id.clone(), p.account_email.clone())
                                } else {
                                    (None, None, None)
                                }
                            };

                            let pair_keys = if agent_id == "git" {
                                &crate::kopeng::resolution::GIT_AGENT_KEYS
                            } else {
                                &crate::kopeng::resolution::DEFAULT_AGENT_KEYS
                            };
                            let (runner, model, effort) = platform_agent_pair(project_path, Some(agent_id), pair_keys);

                            state.agents.push(AgentState {
                                id: agent_id.to_string(),
                                role: role.to_string(),
                                runner,
                                model,
                                effort,
                                accept_edits: Some(true),
                                status: st.as_str().to_string(),
                                pid,
                                is_internal: false,
                                auto_approve: true,
                                args: Vec::new(),
                                context_docs: Vec::new(),
                                skills: Vec::new(),
                                system_prompt: None,
                                mcp_dialogue_mode: true,
                                dna: Some(generate_random_dna(agent_id)),
                                account_id: acc_id,
                                account_email: acc_email,
                            });
                        }
                    }
                }
            }
        }

        // 5. Dynamic Discovery for any other active processes.
        // Strictly exclude service keys (svc:) and task keys (tsk:) — those are
        // managed by the Terminal Hub and must never appear in the agent list.
        {
            let procs = self.processes.read_safe("supervisor:compute_project_state:dynamic");
            let prefix = format!("{}:", project_path);
            for (key, proc) in procs.iter() {
                if let Some(agent_id) = key.strip_prefix(&prefix) {
                    // Guard: skip services and task executions — Terminal Hub processes only
                    if agent_id.starts_with("svc:") || agent_id.starts_with("tsk:") {
                        continue;
                    }
                    if !state.agents.iter().any(|a| a.id.eq_ignore_ascii_case(agent_id)) {
                        let st = self.get_status(key);
                        // Only discover processes that are in an actively running or pending lifecycle state
                        if !matches!(st, ProcessStatus::Working | ProcessStatus::Starting | ProcessStatus::Idle | ProcessStatus::Blocked) {
                            continue;
                        }
                        let role = match agent_id {
                            "docs-resolver" => "Documentation Architect & Resolver",
                            "todos-resolver" => "Task Migration Specialist",
                            "techdebt-resolver" => "Technical Debt Remediator",
                            "profiler-crafter" => "Performance Profiling Crafter",
                            _ => "Project Resolution Specialist",
                        };
                        state.agents.push(AgentState {
                            id: agent_id.to_string(),
                            role: role.to_string(),
                            // spawn_agent records the validated runner of every agent process.
                            runner: proc.runner.clone().unwrap_or_default(),
                            model: proc.model.clone(),
                            effort: proc.effort.clone(),
                            accept_edits: Some(true),
                            status: st.as_str().to_string(),
                            pid: Some(proc.pid),
                            is_internal: false,
                            auto_approve: true,
                            args: Vec::new(),
                            context_docs: Vec::new(),
                            skills: Vec::new(),
                            system_prompt: None,
                            mcp_dialogue_mode: true,
                            dna: Some(generate_random_dna(agent_id)),
                            account_id: proc.account_id.clone(),
                            account_email: proc.account_email.clone(),
                        });
                    }
                }
            }
        }

        state
    }
}

// ---------------------------------------------------------------------------
// TAURI COMMANDS (Direct Native Process Supervision)
// ---------------------------------------------------------------------------

static LAUNCH_IN_PROGRESS: OnceLock<tokio::sync::Mutex<std::collections::HashSet<String>>> = OnceLock::new();

fn get_launch_in_progress() -> &'static tokio::sync::Mutex<std::collections::HashSet<String>> {
    LAUNCH_IN_PROGRESS.get_or_init(|| tokio::sync::Mutex::new(std::collections::HashSet::new()))
}

struct LaunchGuard(String);

impl Drop for LaunchGuard {
    fn drop(&mut self) {
        let k = self.0.clone();
        tokio::spawn(async move {
            let mut set = get_launch_in_progress().lock().await;
            set.remove(&k);
        });
    }
}

/// Arguments of the daemon commands `launch_agent` and `restart_agent`. `runner`, `model` and
/// `effort` are an explicit pair (all of runner and model, or none); `account_id` overrides the
/// Conversation's account; `clean_context` opens a new Conversation.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct LaunchAgentArgs {
    pub project_path: String,
    pub project_name: String,
    pub agent_id: String,
    pub runner: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub account_id: Option<String>,
    pub clean_context: Option<bool>,
}

/// Launches a declared agent of a project (or reports that it already runs). Daemon command.
#[allow(clippy::too_many_arguments)]
pub async fn launch_agent(
    project_path: String,
    project_name: String,
    agent_id: String,
    runner: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    account_id: Option<String>,
    clean_context: Option<bool>,
) -> Result<String, SupervisorError> {
    let key = ProcessSupervisor::format_agent_key(&project_path, &agent_id);
    {
        let mut in_progress = get_launch_in_progress().lock().await;
        if in_progress.contains(&key) {
            crate::logger::log("DEBUG", "supervisor", &format!("Agent '{}' launch already in progress in '{}', skipping concurrent request", agent_id, project_path));
            return Ok(format!("Agent '{}' launch is already in progress", agent_id));
        }
        in_progress.insert(key.clone());
    }
    let _launch_guard = LaunchGuard(key.clone());

    let sup = get_supervisor();
    let state = sup.compute_project_state(&project_path, &project_name);
    let agent_info = state.agents.iter().find(|a| a.id.eq_ignore_ascii_case(&agent_id))
        .ok_or_else(|| SupervisorError::AgentNotDeclared { agent_id: agent_id.clone(), project_path: project_path.clone() })?;

    if agent_info.status != "unlaunched" && agent_info.status != "completed" && agent_info.status != "failed" {
        return Ok(format!("Agent '{}' is already running (PID {:?})", agent_id, agent_info.pid));
    }

    // 1. Resolve the project and the agent's active Conversation. Its runner/model/account/effort
    //    are overrides (NULL inherits the persona); explicit launch arguments outrank them
    //    (Spec 30 §7). Explicit arguments are first applied to the Conversation, which closes it
    //    and opens a clean one when the runner changes.
    let project_id = crate::db::db_get_projects()?
        .into_iter()
        .find(|p| p.path == project_path)
        .map(|p| p.id as i64)
        .ok_or_else(|| SupervisorError::ProjectNotRegistered { path: project_path.clone() })?;

    let mut conversation = if runner.is_some() || model.is_some() || account_id.is_some() || effort.is_some() {
        crate::conversation::apply_conversation_config(
            &project_path, &agent_info.id, runner.as_deref(), model.as_deref(), account_id.as_deref(), effort.as_deref(),
        )?
    } else {
        crate::db::db_get_or_create_active_conversation(project_id, &agent_info.id, None, None, None, None, None)?
    };

    // The runner/model pair comes whole from one source — the explicit arguments, else the
    // Conversation's overrides, else the agent itself — because a model belongs to its runner:
    // the agent's own model is never paired with an overriding runner. spawn_agent validates the
    // pair against the manifest and fills a missing effort with the model's declared default.
    let explicit = crate::kopeng::resolution::ExplicitAgentConfig::from_parts(
        &format!("launch of agent '{}'", agent_info.id), runner.clone(), model.clone(), effort.clone(),
    )?;
    let (target_runner, target_model, target_effort) = match explicit {
        Some(e) => (e.runner, Some(e.model), e.effort),
        None => {
            let runner_overridden = conversation.runner.as_deref().is_some_and(|r| r != agent_info.runner);
            let model_overridden = conversation.model.is_some();
            let target_runner = conversation.runner.clone().unwrap_or_else(|| agent_info.runner.clone());
            let target_model = conversation.model.clone().or_else(|| if runner_overridden { None } else { agent_info.model.clone() });
            let target_effort = effort.clone()
                .or_else(|| conversation.effort.clone())
                .or_else(|| if runner_overridden || model_overridden { None } else { agent_info.effort.clone() });
            (target_runner, target_model, target_effort)
        }
    };
    let target_model: String = target_model.ok_or_else(|| {
        crate::config::ConfigError::MissingModel { context: format!("launch of agent '{}'", agent_info.id), runner: target_runner.clone() }
    })?;

    let target_account_id: Option<String> = account_id.as_deref()
        .or(conversation.account_id.as_deref())
        .map(String::from);

    // 2. Resume the Conversation's Native Session when one exists for this runner; otherwise this
    //    launch starts a Native Session without history. A clean context opens a new Conversation
    //    carrying only the explicit overrides, so the rest keeps inheriting the persona.
    let mut session_resume_id: Option<String> = None;

    if clean_context.unwrap_or(false) {
        // The new Conversation keeps the overrides in force (explicit ones first), so a clean
        // context does not change how the agent runs.
        conversation = crate::db::db_create_agent_conversation(
            project_id,
            &agent_info.id,
            "New Clean Session",
            runner.as_deref().or(conversation.runner.as_deref()),
            model.as_deref().or(conversation.model.as_deref()),
            account_id.as_deref().or(conversation.account_id.as_deref()),
            effort.as_deref().or(conversation.effort.as_deref()),
        )?;
    } else {
        if agent_info.id.eq_ignore_ascii_case(crate::kopeng::initializer::INITIALIZER_AGENT_ID) {
            match crate::db::db_get_init_state(&project_path)? {
                Some(init_state) => {
                    session_resume_id = init_state.agy_conversation_id.clone();
                    if init_state.status != "running" && init_state.status != "completed" {
                        let mut updated = init_state;
                        updated.status = "running".to_string();
                        crate::db::db_upsert_init_state(&updated)?;
                    }
                }
                None => {
                    let has_rula = Path::new(&project_path).join("rula.toml").exists();
                    let new_state = crate::db::ProjectInitState {
                        project_path: project_path.clone(),
                        status: "running".to_string(),
                        mode: if has_rula { "supervised".to_string() } else { "greenfield".to_string() },
                        current_phase: Some("Session active".to_string()),
                        last_message: None,
                        git_checkpoint: None,
                        agy_conversation_id: None,
                    };
                    crate::db::db_upsert_init_state(&new_state)?;
                }
            }
        }

        if session_resume_id.is_none() {
            session_resume_id = crate::db::db_get_active_agent_session(project_id, &agent_info.id, &target_runner)?
                .map(|(native_session_id, _turn_count)| native_session_id);
        }
    }

    let native_session_row = crate::db::db_open_native_session(&conversation.id, &target_runner, Some(&target_model), target_account_id.as_deref())?;

    // Hybrid resume (Spec 30 §4): nothing to resume but the Conversation already has dialogue
    // (runner without native resume, or an invalidated session) -> seed the new Native Session
    // with a Brief of the same Conversation. Built before spawning so a failure aborts cleanly.
    let seed_brief = if session_resume_id.is_none() && !agent_info.id.eq_ignore_ascii_case("initializer") {
        let has_history = crate::db::db_count_dialogue_turns(&conversation.id).map_err(SupervisorError::from);
        // A Conversation with its own dialogue is briefed from itself; a reopened one that has
        // not spoken yet is briefed from the Conversation it reopened (Spec 30 §8).
        let brief = match has_history {
            Ok(n) if n > 0 => crate::brief::build_brief(&conversation.id).map(Some).map_err(SupervisorError::from),
            Ok(_) => match conversation.reopened_from.as_deref() {
                Some(source_id) => crate::brief::build_brief(source_id).map(Some).map_err(SupervisorError::from),
                None => Ok(None),
            },
            Err(e) => Err(e),
        };
        match brief {
            Ok(b) => b,
            Err(e) => {
                crate::logger::log_failure("supervisor", "db_end_native_session", crate::db::db_end_native_session(native_session_row, "exit"));
                return Err(e);
            }
        }
    } else {
        None
    };
    let env_vars = Vec::new(); // Accounts can inject isolated env vars here
    let spawn_result = sup.spawn_agent(
        &project_path,
        &agent_info.id,
        &target_runner,
        &target_model,
        target_effort.as_deref(),
        agent_info.auto_approve,
        agent_info.accept_edits.unwrap_or(false),
        &agent_info.args,
        &env_vars,
        session_resume_id.as_deref(),
        target_account_id.as_deref(),
        clean_context.unwrap_or(false),
    ).await;
    let pid = match spawn_result {
        Ok(pid) => pid,
        Err(e) => {
            crate::logger::log_failure("supervisor", "db_end_native_session", crate::db::db_end_native_session(native_session_row, "exit"));
            return Err(e);
        }
    };

    // Spawn is silent (Spec 30 §4): nothing is sent to the model here. A Native Session without
    // history (nothing to resume) receives the Persona Preamble together with the first user
    // message, and the chat gets a synthetic Readiness Stub. Resumed sessions never receive
    // persona injection again, and a runner without native resume always re-receives the persona.
    let should_inject_persona = session_resume_id.is_none();
    if should_inject_persona {
        let key = ProcessSupervisor::format_agent_key(&project_path, &agent_info.id);
        let persona = build_agent_initial_prompt(
            &agent_info.id,
            agent_info.system_prompt.as_deref(),
            agent_info.mcp_dialogue_mode,
            &agent_info.context_docs,
            &agent_info.skills,
        );
        let preamble = match (persona, seed_brief) {
            (Some(p), Some(b)) => {
                crate::logger::log_failure("supervisor", "db_mark_native_session_seeded", crate::db::db_mark_native_session_seeded(native_session_row));
                crate::logger::log("INFO", "supervisor", &format!("Native Session {} seeded with a Brief ({} bytes, dropped: {:?})", native_session_row, b.text.len(), b.dropped_sections));
                Some(format!("{}\n\n---\n\n{}", p, b.text))
            }
            (None, Some(b)) => {
                crate::logger::log_failure("supervisor", "db_mark_native_session_seeded", crate::db::db_mark_native_session_seeded(native_session_row));
                Some(b.text)
            }
            (p, None) => p,
        };
        if let Some(preamble) = preamble {
            crate::logger::log_failure("supervisor", "set_pending_preamble", sup.set_pending_preamble(&key, preamble));
        }
        insert_readiness_stub(&project_path, &project_name, &agent_info.id);
    }

    Ok(format!("Agent '{}' launched with PID {}", agent_id, pid))
}

/// Persists the session id a runner reported on the agent's open Native Session (Spec 30 §3).
/// Failures are logged: the agent keeps running, but it will not be resumable.
fn record_native_session(project_path: &str, agent_id: &str, runner: &str, native_session_id: &str) {
    // The project-less system agent has no Conversation to attach a session to.
    if agent_id.eq_ignore_ascii_case(SYSTEM_AGENT_ID) {
        return;
    }
    let project = match crate::db::db_get_projects() {
        Ok(projects) => projects.into_iter().find(|p| p.path == project_path),
        Err(e) => {
            crate::logger::log("ERROR", "supervisor", &format!("record_native_session: cannot list projects: {}", e));
            return;
        }
    };
    let Some(project) = project else {
        crate::logger::log("WARN", "supervisor", &format!("record_native_session: '{}' is not a registered project", project_path));
        return;
    };
    match crate::db::db_get_or_create_active_conversation(project.id as i64, agent_id, None, None, None, None, None) {
        Ok(conv) => {
            // Agents spawned outside `launch_agent` have no open Native Session yet.
            crate::logger::log_failure("supervisor", "db_ensure_open_native_session", crate::db::db_ensure_open_native_session(&conv.id, runner));
            crate::logger::log_failure("supervisor", "db_record_native_session_id", crate::db::db_record_native_session_id(&conv.id, native_session_id));
        }
        Err(e) => crate::logger::log("ERROR", "supervisor", &format!("record_native_session: no active conversation for '{}': {}", agent_id, e)),
    }
}

/// Text of the synthetic turn that marks the start of a Conversation (Spec 30 Readiness Stub).
const READINESS_STUB_TEXT: &str = "agent ready";

/// Persists the Readiness Stub as a `system` turn of the agent's active Conversation and notifies
/// the UI. Generated by ONE-SHIP, never by a model; failures are logged and do not abort the launch.
fn insert_readiness_stub(project_path: &str, _project_name: &str, agent_id: &str) {
    let project = match crate::db::db_get_projects() {
        Ok(projects) => projects.into_iter().find(|p| p.path == project_path),
        Err(e) => {
            crate::logger::log("ERROR", "supervisor", &format!("insert_readiness_stub: cannot list projects: {}", e));
            return;
        }
    };
    let Some(project) = project else {
        crate::logger::log("WARN", "supervisor", &format!("insert_readiness_stub: '{}' is not a registered project", project_path));
        return;
    };
    insert_system_turn(project.id as i64, &project.path, agent_id, READINESS_STUB_TEXT);
}

/// Persists `text` as a `system` turn of the agent's active Conversation and notifies the UI.
/// Generated by ONE-SHIP, never by a model; failures are logged at `ERROR` and returned to nobody,
/// because every caller is itself a best-effort notification path.
fn insert_system_turn(project_id: i64, project_path: &str, agent_id: &str, text: &str) {
    let conversation = match crate::db::db_get_or_create_active_conversation(project_id, agent_id, None, None, None, None, None) {
        Ok(conversation) => conversation,
        Err(e) => {
            crate::logger::log("ERROR", "supervisor", &format!("insert_system_turn: no active conversation for '{}': {}", agent_id, e));
            return;
        }
    };
    let turn = crate::db::ChatTurnRecord {
        id: None,
        conversation_id: conversation.id.clone(),
        project_id,
        agent_id: agent_id.to_string(),
        agy_conversation_id: None,
        role: "system".to_string(),
        dialogue: text.to_string(),
        artifact_type: None,
        artifact_lang: None,
        artifact_label: None,
        artifact_file_path: None,
        artifact_content: None,
        is_ephemeral: false,
        terminal_snapshot: None,
        timestamp: (epoch_now_ms() / 1000) as i64,
        topic: None,
        approval_payload: None,
    };
    match crate::db::db_insert_chat_turn(&turn) {
        Ok(turn_id) => get_supervisor().emit_event(DaemonEvent::agent_conversation_turn_added(&crate::events::AgentConversationTurnAddedPayload {
            turn_id: Some(turn_id),
            project_id,
            project_path: project_path.to_string(),
            agent_id: agent_id.to_string(),
            conversation_id: conversation.id,
            role: "system".to_string(),
            dialogue: text.to_string(),
            timestamp: Some(turn.timestamp),
        })),
        Err(e) => crate::logger::log("ERROR", "supervisor", &format!("insert_system_turn: db_insert_chat_turn failed: {}", e)),
    }
}

/// Reports that `agent_id` could not execute work: shows a `system` turn in its chat and, when a
/// delegation to it is still pending in this exact project, marks that delegation `failed` and
/// notifies the originator with the same notice format as `report_task_status`.
///
/// Idempotent: a delegation already closed is not found again, so the two failure sites of one
/// incident (turn error and process exit) cannot notify the originator twice.
fn report_agent_failure(project_path: &str, agent_id: &str, reason: &str, show_in_chat: bool) {
    if show_in_chat {
        match crate::registry::find_project_by_path(project_path) {
            Ok(project) => insert_system_turn(project.id as i64, &project.path, agent_id, &format!("⚠️ **Agent failed**: {}", reason)),
            Err(e) => crate::logger::log("ERROR", "supervisor", &format!("report_agent_failure: '{}' not shown in chat: {}", agent_id, e)),
        }
    }

    let delegation = match crate::db::db_get_pending_delegation_by_target(agent_id, project_path) {
        Ok(Some(delegation)) => delegation,
        Ok(None) => return,
        Err(e) => {
            crate::logger::log("ERROR", "supervisor", &format!("report_agent_failure: pending delegation lookup for '{}' failed: {}", agent_id, e));
            return;
        }
    };

    // Only a delegation handed to THIS process can have been lost with it. Older pending
    // delegations (e.g. never reported by a previous process) are unrelated to this failure.
    let key = ProcessSupervisor::format_agent_key(project_path, agent_id);
    let process_started_at_seconds = {
        let procs = get_supervisor().processes.read_safe("supervisor:report_agent_failure:started_at");
        procs.get(&key).map(|p| p.started_at_ms / 1000)
    };
    let Some(process_started_at_seconds) = process_started_at_seconds else {
        crate::logger::log("WARN", "supervisor", &format!("report_agent_failure: process '{}' is gone; pending delegation '{}' left untouched", key, delegation.task_id));
        return;
    };
    if delegation.dispatched_at < process_started_at_seconds {
        crate::logger::log("DEBUG", "supervisor", &format!(
            "report_agent_failure: pending delegation '{}' predates process '{}'; left untouched", delegation.task_id, key
        ));
        return;
    }

    let summary = format!("Target agent '{}' could not execute the task: {}", agent_id, reason);
    crate::logger::log_failure("supervisor", "db_update_delegation_status (failed)", crate::db::db_update_delegation_status(
        &delegation.task_id, "failed", Some(crate::mcp::chrono_or_epoch()), Some(summary.clone()),
    ));

    let originator = delegation.originator_agent;
    if originator.is_empty() || originator.eq_ignore_ascii_case("ui") || originator.eq_ignore_ascii_case(agent_id) {
        return;
    }
    let notice = format!(
        "[ONE-SHIP NOTIFICATION from '{}']\nTask ID: {}\nStatus: [FAILED]\nSummary: {}",
        agent_id, delegation.task_id, summary
    );
    let originator_path = delegation.originator_project.or_else(|| Some(project_path.to_string()));

    // A failure notice must never launch an agent: only a running originator can act on it.
    let originator_key = ProcessSupervisor::format_agent_key(originator_path.as_deref().unwrap_or(project_path), &originator);
    let originator_status = get_supervisor().get_status(&originator_key);
    if matches!(originator_status, ProcessStatus::Unlaunched | ProcessStatus::Completed { .. } | ProcessStatus::Failed { .. }) {
        crate::logger::log("WARN", "supervisor", &format!(
            "report_agent_failure: originator '{}' is not running; delegation '{}' is recorded as failed without a callback",
            originator, delegation.task_id
        ));
        return;
    }
    tokio::spawn(async move {
        crate::logger::log_failure("supervisor", "failure callback prompt to originator", prompt_agent(originator_path, originator, notice).await);
    });
}

/// Arguments of the daemon command `stop_agent`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct StopAgentArgs {
    pub project_path: String,
    pub agent_id: String,
}

/// Stops an agent of a project. Daemon command.
pub async fn stop_agent(
    project_path: String,
    agent_id: String,
) -> Result<String, SupervisorError> {
    let sup = get_supervisor();
    let key = ProcessSupervisor::format_agent_key(&project_path, &agent_id);
    sup.stop_process(&key).await?;
    Ok(format!("Agent '{}' stopped", agent_id))
}

/// Arguments of the daemon command `interrupt_agent`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct InterruptAgentArgs {
    pub project_path: String,
    pub agent_id: String,
    /// Prompt delivered once the interrupted turn ends (or right away to an idle agent).
    pub corrective_prompt: Option<String>,
}

/// Interrupts the running turn of an agent (SIGINT; `session/cancel` for opencode). Daemon command.
pub async fn interrupt_agent(
    project_path: String,
    agent_id: String,
    corrective_prompt: Option<String>,
) -> Result<String, SupervisorError> {
    crate::logger::log(
        "INFO",
        "supervisor",
        &format!("Interrupt request for agent '{}' in '{}'", agent_id, project_path),
    );

    let sup = get_supervisor();
    let key = ProcessSupervisor::format_agent_key(&project_path, &agent_id);

    let proc = {
        let procs = sup.processes.read_safe("supervisor:interrupt_agent");
        procs.get(&key).cloned()
    };

    if let Some(proc) = proc {
        let current_st = proc.status.read_safe("supervisor:interrupt_agent:status").clone();

        if current_st == ProcessStatus::Idle || current_st == ProcessStatus::Unlaunched {
            if let Some(prompt) = corrective_prompt {
                prompt_agent(Some(project_path.clone()), agent_id.clone(), prompt).await?;
                return Ok(format!("Agent '{}' was idle; prompt dispatched directly", agent_id));
            }
            return Ok(format!("Agent '{}' is already idle", agent_id));
        }

        if let Some(ref prompt) = corrective_prompt {
            *proc.pending_correction.write_safe("supervisor:interrupt_agent:set_correction") = Some(prompt.clone());
        }

        proc.is_interrupted.store(true, std::sync::atomic::Ordering::SeqCst);

        if proc.runner.as_deref() == Some("opencode") {
            let cancel_req = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "session/cancel",
                "params": {}
            }).to_string();
            crate::logger::log_failure("supervisor", "send cancel request to agent stdin", proc.stdin_tx.send(cancel_req).await);
        }

        #[cfg(target_os = "linux")]
        unsafe {
            kill_process_and_group(proc.pid, libc::SIGINT);
        }

        crate::logger::log(
            "INFO",
            "supervisor",
            &format!("Signaled SIGINT to agent '{}' (PID {})", key, proc.pid),
        );

        Ok(format!("Agent '{}' interrupted", agent_id))
    } else {
        if let Some(prompt) = corrective_prompt {
            prompt_agent(Some(project_path.clone()), agent_id.clone(), prompt).await?;
            return Ok(format!("Agent '{}' not running; prompt launched directly", agent_id));
        }
        Ok(format!("Agent '{}' is not running", agent_id))
    }
}

/// Stops and relaunches an agent with the given launch arguments. Daemon command.
#[allow(clippy::too_many_arguments)]
pub async fn restart_agent(
    project_path: String,
    project_name: String,
    agent_id: String,
    runner: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    account_id: Option<String>,
    clean_context: Option<bool>,
) -> Result<String, SupervisorError> {
    stop_agent(project_path.clone(), agent_id.clone()).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(get_supervisor_process_poll_step_ms())).await;
    launch_agent(project_path, project_name, agent_id, runner, model, effort, account_id, clean_context).await
}

/// Arguments of the daemon command `prompt_agent`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct PromptAgentArgs {
    pub project_path: Option<String>,
    pub target: String,
    pub prompt: String,
}

/// Sends a prompt to an agent, launching it first when it is not running. Daemon command.
pub async fn prompt_agent(
    project_path: Option<String>,
    target: String,
    prompt: String,
) -> Result<String, SupervisorError> {
    crate::logger::log(
        "INFO",
        "supervisor",
        &format!("Dispatching prompt to agent '{}' (project_path: {:?})", target, project_path),
    );
    let sup = get_supervisor();

    // Resolve project record
    let projects = crate::registry::list_projects()?;

    // Rule 11: the project must be named by the caller; no process-name matching.
    let p_path = project_path.ok_or_else(|| {
        SupervisorError::InvalidRequest(format!("Cannot dispatch prompt to agent '{}': project_path is required", target))
    })?;
    let p = projects
        .into_iter()
        .find(|p| p.path == p_path)
        .ok_or_else(|| SupervisorError::ProjectNotRegistered { path: p_path.clone() })?;

    let key = ProcessSupervisor::format_agent_key(&p.path, &target);
    let st = sup.get_status(&key);
    if st == ProcessStatus::Unlaunched || matches!(st, ProcessStatus::Completed { .. }) || matches!(st, ProcessStatus::Failed { .. }) {
        launch_agent(p.path.clone(), p.name.clone(), target.clone(), None, None, None, None, None).await?;
    }

    sup.send_input(&key, &prompt).await?;
    crate::conversation::mark_user_message_sent(&p.path, &target);

    // Persist incoming prompt turn in SQLite active conversation
    match crate::db::db_get_or_create_active_conversation(p.id as i64, &target, None, None, None, None, None) {
        Ok(conv) => {
            crate::logger::log_failure("supervisor", "db_insert_chat_turn (prompted user turn)", crate::db::db_insert_chat_turn(&crate::db::ChatTurnRecord {
                id: None,
                conversation_id: conv.id.clone(),
                project_id: p.id as i64,
                agent_id: target.clone(),
                agy_conversation_id: None,
                role: "user".to_string(),
                dialogue: prompt.clone(),
                artifact_type: None,
                artifact_lang: None,
                artifact_label: None,
                artifact_file_path: None,
                artifact_content: None,
                is_ephemeral: false,
                terminal_snapshot: None,
                timestamp: (epoch_now_ms() / 1000) as i64,
                topic: None,
                approval_payload: None,
            }));
            sup.emit_event(DaemonEvent::agent_conversation_turn_added(&crate::events::AgentConversationTurnAddedPayload {
                turn_id: None,
                project_id: p.id as i64,
                project_path: p.path.clone(),
                agent_id: target.clone(),
                conversation_id: conv.id,
                role: "user".to_string(),
                dialogue: prompt.clone(),
                timestamp: None,
            }));
        }
        Err(e) => {
            crate::logger::log("ERROR", "supervisor", &format!("prompt_agent: failed to resolve active conversation for '{}': {}", target, e));
        }
    }

    Ok(format!("Prompt sent to {} ({})", target, p.name))
}

/// Arguments of the daemon command `get_project_agent_state`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct GetProjectAgentStateArgs {
    pub project_path: String,
    pub project_name: String,
}

/// Agents of a project with their live status. Daemon command (reads `rula.toml` and SQLite, so
/// it runs on the blocking pool).
pub async fn get_project_agent_state(project_path: String, project_name: String) -> Result<ProjectAgentState, SupervisorError> {
    tokio::task::spawn_blocking(move || get_supervisor().compute_project_state(&project_path, &project_name))
        .await
        .map_err(|e| SupervisorError::Task(e.to_string()))
}

/// Arguments of the daemon command `dispatch_git_instruction`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct DispatchGitInstructionArgs {
    pub project_id: usize,
    pub instruction: String,
}

/// Sends an instruction to the project's configured default Git agent (`plugins.vcs.default_agent_id`),
/// launching it first if needed. Daemon command.
pub async fn dispatch_git_instruction(project_id: usize, instruction: String) -> Result<String, SupervisorError> {
    let project = tokio::task::spawn_blocking(move || crate::registry::find_project_by_id(project_id))
        .await
        .map_err(|e| SupervisorError::Task(e.to_string()))??;
    let (project_path, project_name) = (project.path, project.name);
    let target_agent = crate::config::resolve_effective_string(Some(&project_path), None, "vcs", "default_agent_id")
        .trim()
        .to_string();
    if target_agent.is_empty() {
        return Err(SupervisorError::InvalidRequest(
            "No default Git agent configured. Select one in Project Settings -> Git & VCS.".to_string(),
        ));
    }
    crate::logger::log(
        "INFO",
        "git",
        &format!(
            "Dispatching git instruction for {} to '{}' ({} bytes)",
            project_name,
            target_agent,
            instruction.len()
        ),
    );
    let sup = get_supervisor();
    let key = ProcessSupervisor::format_agent_key(&project_path, &target_agent);
    if sup.get_status(&key) == ProcessStatus::Unlaunched {
        launch_agent(project_path.clone(), project_name, target_agent.clone(), None, None, None, None, None).await?;
    }
    sup.send_input(&key, &instruction).await?;
    crate::conversation::mark_user_message_sent(&project_path, &target_agent);
    Ok(target_agent)
}

/// Runs a single-turn headless prompt against a runner in `stream-json` (AGY/Claude) or `acp`
/// (OpenCode) mode without registering a persistent supervisor pane or conversation.
pub async fn run_ephemeral_runner_prompt(
    project_path: &Path,
    pair: &crate::kopeng::resolution::EffectiveAgentConfig,
    prompt: &str,
    timeout_ms: u64,
) -> Result<String, SupervisorError> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let runner_bin = pair.runner.trim().to_string();
    let runner_name = std::path::Path::new(&runner_bin)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    let is_claude = runner_name == "claude";
    let is_agy = runner_name == "agy";
    let is_opencode = runner_name == "opencode";

    let resolved = {
        let bin = runner_bin.clone();
        tokio::task::spawn_blocking(move || resolve_executable(&bin))
            .await
            .map_err(|e| SupervisorError::Task(e.to_string()))?
    };

    let mut cli_args = Vec::new();
    if is_opencode {
        cli_args.push("acp".to_string());
    } else if is_claude {
        cli_args.extend([
            "--print".to_string(),
            "--input-format".to_string(),
            "stream-json".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
            "--include-partial-messages".to_string(),
            "--dangerously-skip-permissions".to_string(),
            "--model".to_string(),
            pair.model.clone(),
        ]);
        let schema = crate::config::load_runner_schema(&runner_bin)?;
        if let (Some(flag), Some(effort)) = (schema.flags.effort.as_deref(), pair.effort.as_deref()) {
            cli_args.push(flag.to_string());
            cli_args.push(effort.to_string());
        }
    } else if is_agy {
        cli_args.extend([
            "--dangerously-skip-permissions".to_string(),
            "--model".to_string(),
            pair.model.clone(),
            "--input-format".to_string(),
            "stream-json".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
        ]);
        if let Some(effort) = pair.effort.as_deref() {
            cli_args.push("--effort".to_string());
            cli_args.push(effort.to_string());
        }
    } else {
        return Err(SupervisorError::InvalidRequest(format!(
            "Unsupported runner '{}' for ephemeral prompt",
            runner_bin
        )));
    }

    let mut cmd = tokio::process::Command::new(&resolved.program);
    cmd.args(&cli_args)
        .current_dir(project_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    cmd.env("TERM", "dumb");

    let mut child = cmd
        .spawn()
        .map_err(|source| SupervisorError::Spawn { target: runner_bin.clone(), source })?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| SupervisorError::InputDelivery { key: runner_bin.clone(), details: "Runner stdin unavailable".into() })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| SupervisorError::InputDelivery { key: runner_bin.clone(), details: "Runner stdout unavailable".into() })?;

    let mut reader = AsyncBufReadExt::lines(BufReader::new(stdout));
    let prompt_owned = prompt.to_string();
    let project_str = project_path.to_string_lossy().to_string();

    let run_future = async {
        fn push_bounded(buf: &mut String, chunk: &str, max_bytes: usize) {
            if buf.len() >= max_bytes {
                return;
            }
            let remaining = max_bytes - buf.len();
            if chunk.len() <= remaining {
                buf.push_str(chunk);
            } else {
                let end = chunk.floor_char_boundary(remaining);
                buf.push_str(&chunk[..end]);
            }
        }

        let mut accumulated = String::new();
        const MAX_RESPONSE_BYTES: usize = 32 * 1024;

        if is_opencode {
            let init_req = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": 1,
                    "clientCapabilities": {},
                    "clientInfo": { "name": "one-ship-commit", "version": env!("CARGO_PKG_VERSION") }
                }
            })
            .to_string();
            stdin.write_all(init_req.as_bytes()).await.map_err(|e| SupervisorError::InputDelivery { key: runner_bin.clone(), details: e.to_string() })?;
            stdin.write_all(b"\n").await.map_err(|e| SupervisorError::InputDelivery { key: runner_bin.clone(), details: e.to_string() })?;

            let new_sess_req = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "session/new",
                "params": { "cwd": project_str, "mcpServers": [] }
            })
            .to_string();
            stdin.write_all(new_sess_req.as_bytes()).await.map_err(|e| SupervisorError::InputDelivery { key: runner_bin.clone(), details: e.to_string() })?;
            stdin.write_all(b"\n").await.map_err(|e| SupervisorError::InputDelivery { key: runner_bin.clone(), details: e.to_string() })?;
            stdin.flush().await.map_err(|e| SupervisorError::InputDelivery { key: runner_bin.clone(), details: e.to_string() })?;

            let mut prompt_sent = false;
            while let Ok(Some(line)) = reader.next_line().await {
                let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) else { continue; };
                if !prompt_sent && val.get("id").and_then(|v| v.as_u64()) == Some(2) {
                    if let Some(sid) = val.get("result").and_then(|r| r.get("sessionId")).and_then(|s| s.as_str()) {
                        let req = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": 1000,
                            "method": "session/prompt",
                            "params": {
                                "sessionId": sid,
                                "prompt": [{ "type": "text", "text": prompt_owned }]
                            }
                        })
                        .to_string();
                        stdin
                            .write_all(req.as_bytes())
                            .await
                            .map_err(|e| SupervisorError::InputDelivery {
                                key: runner_bin.clone(),
                                details: e.to_string(),
                            })?;
                        stdin
                            .write_all(b"\n")
                            .await
                            .map_err(|e| SupervisorError::InputDelivery {
                                key: runner_bin.clone(),
                                details: e.to_string(),
                            })?;
                        stdin
                            .flush()
                            .await
                            .map_err(|e| SupervisorError::InputDelivery {
                                key: runner_bin.clone(),
                                details: e.to_string(),
                            })?;
                        prompt_sent = true;
                    }
                    continue;
                }
                if val.get("method").and_then(|m| m.as_str()) == Some("session/update") {
                    if let Some(text) = val
                        .get("params")
                        .and_then(|p| p.get("update"))
                        .and_then(|u| u.get("content"))
                        .and_then(|c| c.get("text"))
                        .and_then(|t| t.as_str())
                    {
                        push_bounded(&mut accumulated, text, MAX_RESPONSE_BYTES);
                    }
                }
                if val.get("id").and_then(|v| v.as_u64()) == Some(1000) && val.get("result").is_some() {
                    break;
                }
            }
        } else {
            let payload = if is_claude {
                serde_json::json!({
                    "type": "user",
                    "message": {
                        "role": "user",
                        "content": [{ "type": "text", "text": prompt_owned }]
                    }
                })
                .to_string()
            } else {
                serde_json::json!({
                    "event": "user",
                    "message": {
                        "role": "user",
                        "content": prompt_owned
                    }
                })
                .to_string()
            };
            stdin.write_all(payload.as_bytes()).await.map_err(|e| SupervisorError::InputDelivery { key: runner_bin.clone(), details: e.to_string() })?;
            stdin.write_all(b"\n").await.map_err(|e| SupervisorError::InputDelivery { key: runner_bin.clone(), details: e.to_string() })?;
            stdin.flush().await.map_err(|e| SupervisorError::InputDelivery { key: runner_bin.clone(), details: e.to_string() })?;

            while let Ok(Some(line)) = reader.next_line().await {
                let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) else { continue; };
                if is_claude {
                    let ev_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    match ev_type {
                        "stream_event" => {
                            if let Some(delta) = val
                                .get("event")
                                .and_then(|e| {
                                    e.get("text_delta")
                                        .and_then(|v| v.as_str())
                                        .or_else(|| e.get("delta").and_then(|d| d.get("text")).and_then(|v| v.as_str()))
                                })
                            {
                                push_bounded(&mut accumulated, delta, MAX_RESPONSE_BYTES);
                            }
                        }
                        "assistant" => {
                            if accumulated.is_empty() {
                                if let Some(blocks) = val.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_array()) {
                                    for b in blocks {
                                        if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                                            if let Some(txt) = b.get("text").and_then(|t| t.as_str()) {
                                                push_bounded(&mut accumulated, txt, MAX_RESPONSE_BYTES);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        "result" => break,
                        _ => {}
                    }
                } else {
                    let ev_type = val.get("event").and_then(|t| t.as_str()).unwrap_or("");
                    match ev_type {
                        "step_update" => {
                            if let Some(update) = val.get("step_update") {
                                if update.get("step_type").and_then(|t| t.as_str()) == Some("agent_response") {
                                    if let Some(delta) = update
                                        .get("text_delta")
                                        .or_else(|| update.get("text"))
                                        .and_then(|t| t.as_str())
                                    {
                                        push_bounded(&mut accumulated, delta, MAX_RESPONSE_BYTES);
                                    }
                                }
                            }
                        }
                        "step" => {
                            if let Some(step) = val.get("step") {
                                if step.get("type").and_then(|t| t.as_str()) == Some("agent_response") {
                                    if let Some(txt) = step.get("text").and_then(|t| t.as_str()) {
                                        push_bounded(&mut accumulated, txt, MAX_RESPONSE_BYTES);
                                    }
                                }
                            }
                        }
                        "assistant" => {
                            if let Some(txt) = val.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                                push_bounded(&mut accumulated, txt, MAX_RESPONSE_BYTES);
                            }
                        }
                        "result" => {
                            if accumulated.is_empty() {
                                if let Some(txt) = val.get("result").and_then(|r| r.get("response").or_else(|| r.get("text"))).and_then(|t| t.as_str()) {
                                    push_bounded(&mut accumulated, txt, MAX_RESPONSE_BYTES);
                                }
                            }
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok::<String, SupervisorError>(accumulated)
    };

    let outcome = tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), run_future).await;
    let _ = child.kill().await;
    let _ = child.wait().await;

    match outcome {
        Ok(Ok(text)) => {
            let trimmed = text.trim().to_string();
            if trimmed.is_empty() {
                Err(SupervisorError::InvalidRequest("Runner returned an empty commit message".to_string()))
            } else {
                Ok(trimmed)
            }
        }
        Ok(Err(e)) => Err(e),
        Err(_) => Err(SupervisorError::Task(format!("Commit message generation timed out after {} ms", timeout_ms))),
    }
}

/// Runner/model/effort shown for a platform agent in the project state (git, initializer, flag
/// resolution). Runs on every watcher tick, so a pair the manifests reject is logged at DEBUG and
/// shown with an empty runner and no model; launching the agent reports the typed error.
fn platform_agent_pair(
    project_path: &str,
    agent_id: Option<&str>,
    keys: &crate::kopeng::resolution::AgentPairKeys,
) -> (String, Option<String>, Option<String>) {
    match crate::kopeng::resolution::resolve_effective_agent_config(Some(project_path), agent_id, None, keys) {
        Ok(pair) => (pair.runner, Some(pair.model), pair.effort),
        Err(e) => {
            crate::logger::log("DEBUG", "supervisor", &format!("No valid runner/model pair for {:?} in {}: {}", agent_id, project_path, e));
            (String::new(), None, None)
        }
    }
}


/// Which lines `read_terminal_output` returns.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TerminalOutputFilter {
    All,
    /// Only the lines `kopeng::errors::extract_semantic_errors` recognizes as errors.
    ErrorsOnly,
}

/// Arguments of the daemon command `read_terminal_output`. `paneId` is a canonical process key,
/// or an agent name of `projectPath`; without a project only full keys and `system` are valid.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ReadTerminalOutputArgs {
    pub project_path: Option<String>,
    pub pane_id: String,
    pub lines_count: usize,
    pub filter: TerminalOutputFilter,
}

/// Output of a terminal pane as one text block. Daemon command.
pub async fn read_terminal_output(
    project_path: Option<String>,
    pane_id: String,
    lines_count: usize,
    filter: TerminalOutputFilter,
) -> Result<String, SupervisorError> {
    // A bare agent name is ambiguous across projects: without a project it matches nothing, and
    // an empty pane looks like an agent that said nothing. Only full process keys and the
    // platform pane are valid without a project.
    if project_path.is_none() && !pane_id.contains(':') && pane_id != SYSTEM_AGENT_ID {
        return Err(SupervisorError::InvalidRequest(format!(
            "read_terminal_output: '{}' is not a process key and no project_path was given", pane_id
        )));
    }
    let key = match project_path {
        Some(ref p_path) if !pane_id.contains(':') && !pane_id.is_empty() => ProcessSupervisor::format_agent_key(p_path, &pane_id),
        _ => pane_id,
    };
    let text = get_supervisor().read_output(&key, lines_count).join("\n");
    Ok(match filter {
        TerminalOutputFilter::ErrorsOnly => crate::kopeng::errors::extract_semantic_errors(&text),
        TerminalOutputFilter::All => text,
    })
}

/// Arguments of the daemon command `send_terminal_input`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SendTerminalInputArgs {
    pub project_path: String,
    pub process_key: String,
    pub input: String,
}

/// Forwards a line typed in a terminal pane to the process with canonical key `process_key`
/// (`<project_path>:<agent>`, `:svc:` or `:tsk:`). The key must belong to `project_path`; no
/// bare names are expanded or guessed (AGENTS.md §1.11). Daemon command.
pub async fn send_terminal_input(
    project_path: String,
    process_key: String,
    input: String,
) -> Result<(), SupervisorError> {
    let owned_by_project = process_key
        .strip_prefix(project_path.as_str())
        .is_some_and(|rest| rest.starts_with(':') && rest.len() > 1);
    if !owned_by_project {
        return Err(SupervisorError::InvalidRequest(format!(
            "Process key '{}' does not belong to project '{}'", process_key, project_path
        )));
    }
    get_supervisor().send_input(&process_key, &input).await
}

/// Arguments of the daemon command `launch_interactive_shell`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct LaunchInteractiveShellArgs {
    pub project_id: usize,
    /// `shell_<epoch_ms>`; the prefix keeps shells from replacing a declared service.
    pub shell_id: String,
}

/// Starts `$SHELL` (interactive) in the project root as a supervised service. Daemon command.
pub async fn launch_interactive_shell(project_id: usize, shell_id: String) -> Result<u32, SupervisorError> {
    crate::logger::log("DEBUG", "supervisor", &format!("launch_interactive_shell: project_id={}, shell_id='{}'", project_id, shell_id));
    if shell_id.len() <= SHELL_ID_PREFIX.len() || !shell_id.starts_with(SHELL_ID_PREFIX) {
        return Err(SupervisorError::InvalidRequest(format!("shell_id '{}' must start with '{}'", shell_id, SHELL_ID_PREFIX)));
    }
    let project = tokio::task::spawn_blocking(move || crate::registry::find_project_by_id(project_id))
        .await
        .map_err(|e| SupervisorError::Task(e.to_string()))??;
    let shell = std::env::var("SHELL")
        .map_err(|_| SupervisorError::InvalidRequest("SHELL is not set in the daemon environment".into()))?;
    let args = if shell.ends_with("bash") || shell.ends_with("zsh") {
        vec!["-i".to_string()]
    } else {
        Vec::new()
    };
    get_supervisor().spawn_service(&project.path, &shell_id, &shell, &args, None, None, None).await
}

/// Required prefix of interactive shell IDs (`shell_<epoch_ms>`).
pub const SHELL_ID_PREFIX: &str = "shell_";

/// Arguments of the daemon command `list_interactive_shells`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ListInteractiveShellsArgs {
    pub project_id: usize,
}

/// Interactive shells running for a registered project. The supervisor is the source of truth,
/// so the terminal grid lists shells that outlive a view change. Daemon command.
pub async fn list_interactive_shells(project_id: usize) -> Result<Vec<String>, SupervisorError> {
    crate::logger::log("DEBUG", "supervisor", &format!("list_interactive_shells: project_id={}", project_id));
    let project = tokio::task::spawn_blocking(move || crate::registry::find_project_by_id(project_id))
        .await
        .map_err(|e| SupervisorError::Task(e.to_string()))??;
    Ok(get_supervisor().running_shell_ids(&project.path))
}

/// Concise active agent summary exposed to the lightweight runtime status file (`oneship-status.json`).
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct RuntimeAgentSummary {
    pub project_name: String,
    pub agent_id: String,
    pub role: String,
    pub status: String,
    pub runner: String,
}

/// Lightweight read-only daemon status snapshot written atomically to `$XDG_RUNTIME_DIR/oneship-status.json`
/// for desktop widgets (e.g. Omarchy bar widget) without opening the operator API.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct DaemonRuntimeStatus {
    pub daemon_pid: u32,
    pub mcp_port: u16,
    pub updated_at_ms: u64,
    pub projects_count: usize,
    pub working_agents_count: usize,
    pub idle_agents_count: usize,
    pub blocked_agents_count: usize,
    pub error_agents_count: usize,
    pub active_agents_count: usize,
    pub running_services_count: usize,
    pub pending_approvals_count: usize,
    pub active_agents: Vec<RuntimeAgentSummary>,
}

const MAX_RUNTIME_STATUS_AGENTS_COUNT: usize = 16;

/// Path of the user-scoped runtime status snapshot (`$XDG_RUNTIME_DIR/oneship-status.json`).
pub fn runtime_status_file_path() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir).join("oneship-status.json");
        }
    }
    #[cfg(unix)]
    {
        let uid = unsafe { libc::geteuid() };
        let run_dir = PathBuf::from(format!("/run/user/{}", uid));
        if run_dir.is_dir() {
            return run_dir.join("oneship-status.json");
        }
    }
    std::env::temp_dir().join("oneship-status.json")
}

/// Builds a [`DaemonRuntimeStatus`] snapshot from the current per-project agent state map.
pub fn build_runtime_status_snapshot(
    states_map: &HashMap<usize, ProjectAgentState>,
    running_services_count: usize,
    pending_approvals_count: usize,
) -> DaemonRuntimeStatus {
    let mut projects: Vec<&ProjectAgentState> = states_map.values().collect();
    projects.sort_by(|a, b| a.project_name.cmp(&b.project_name).then(a.project_path.cmp(&b.project_path)));

    let mut working_agents_count = 0usize;
    let mut idle_agents_count = 0usize;
    let mut blocked_agents_count = 0usize;
    let mut error_agents_count = 0usize;
    let mut active_agents = Vec::new();

    for project in projects {
        let mut agents: Vec<&AgentState> = project.agents.iter().collect();
        agents.sort_by(|a, b| a.id.cmp(&b.id));
        for agent in agents {
            let is_active = match agent.status.as_str() {
                "working" | "starting" => {
                    working_agents_count += 1;
                    true
                }
                "idle" => {
                    idle_agents_count += 1;
                    true
                }
                "blocked" => {
                    blocked_agents_count += 1;
                    true
                }
                "failed" => {
                    error_agents_count += 1;
                    true
                }
                _ => false,
            };
            if is_active && active_agents.len() < MAX_RUNTIME_STATUS_AGENTS_COUNT {
                active_agents.push(RuntimeAgentSummary {
                    project_name: project.project_name.clone(),
                    agent_id: agent.id.clone(),
                    role: agent.role.clone(),
                    status: agent.status.clone(),
                    runner: agent.runner.clone(),
                });
            }
        }
    }

    let active_agents_count =
        working_agents_count + idle_agents_count + blocked_agents_count + error_agents_count;

    DaemonRuntimeStatus {
        daemon_pid: std::process::id(),
        mcp_port: crate::config::mcp_daemon_port(),
        updated_at_ms: epoch_now_ms(),
        projects_count: states_map.len(),
        working_agents_count,
        idle_agents_count,
        blocked_agents_count,
        error_agents_count,
        active_agents_count,
        running_services_count,
        pending_approvals_count,
        active_agents,
    }
}

/// Atomically writes the daemon's runtime status JSON snapshot for desktop bar widgets.
pub fn write_runtime_status_snapshot(
    states_map: &HashMap<usize, ProjectAgentState>,
    running_services_count: usize,
    pending_approvals_count: usize,
) -> std::io::Result<()> {
    let snapshot = build_runtime_status_snapshot(states_map, running_services_count, pending_approvals_count);
    let path = runtime_status_file_path();
    let tmp_path = path.with_extension("json.tmp");
    let json = serde_json::to_string(&snapshot)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&tmp_path, format!("{}\n", json))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp_path, &path)
}

/// Removes the runtime status snapshot when the daemon shuts down cleanly.
pub fn remove_runtime_status_snapshot() {
    let path = runtime_status_file_path();
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => crate::logger::log(
            "WARN",
            "supervisor",
            &format!("Failed to remove {}: {}", path.display(), e),
        ),
    }
}

/// Spawns a background task continuously polling active projects and agent processes.
/// Periodically computes comprehensive agent states and emits `agent-states-updated` events
/// across the system broadcast channel to keep UI indicators synchronized in real time.
pub fn spawn_agent_state_watcher(
    tx: tokio::sync::broadcast::Sender<DaemonEvent>,
    cancel: CancellationToken,
) {
    tauri::async_runtime::spawn(async move {
        crate::logger::log("INFO", "supervisor", "Started background agent state watcher loop");
        loop {
            let sleep_interval_ms = get_supervisor_state_watcher_interval_ms();
            tokio::select! {
                _ = cancel.cancelled() => {
                    crate::logger::log("INFO", "supervisor", "Agent state watcher cancelled, shutting down");
                    break;
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(sleep_interval_ms)) => {}
            }

            // The tick reads SQLite and every project's rula.toml: it runs on the blocking pool.
            let tick = tokio::task::spawn_blocking(|| {
                let projects = crate::registry::list_projects()?;
                let sup = get_supervisor();
                crate::logger::log("DEBUG", "supervisor", &format!("Agent state watcher tick: inspecting {} registered projects", projects.len()));
                let mut states_map = HashMap::new();
                let mut running_services_list = Vec::new();
                for p in &projects {
                    states_map.insert(p.id, sup.compute_project_state(&p.path, &p.name));
                    for service_id in sup.running_service_ids(&p.path) {
                        running_services_list.push(format!("{}:{}", p.id, service_id));
                    }
                }
                let pending_approvals_count = match crate::db::db_get_pending_approvals() {
                    Ok(list) => list.len(),
                    Err(e) => {
                        crate::logger::log("WARN", "supervisor", &format!("Failed to read pending approvals for status snapshot: {}", e));
                        0
                    }
                };
                if let Err(e) = write_runtime_status_snapshot(&states_map, running_services_list.len(), pending_approvals_count) {
                    crate::logger::log("WARN", "supervisor", &format!("Failed to write runtime status snapshot: {}", e));
                }
                Ok::<_, crate::registry::RegistryError>((states_map, running_services_list))
            })
            .await;
            let (states_map, running_services_list) = match tick {
                Ok(Ok(state)) => state,
                Ok(Err(e)) => {
                    crate::logger::log("ERROR", "supervisor", &format!("Agent state watcher tick skipped: {}", e));
                    continue;
                }
                Err(e) => {
                    crate::logger::log("ERROR", "supervisor", &format!("Agent state watcher tick task failed: {}", e));
                    continue;
                }
            };

            if !states_map.is_empty() {
                crate::logger::log("DEBUG", "supervisor", &format!("Emitting agent-states-updated for {} projects", states_map.len()));
                let _ = tx.send(DaemonEvent::agent_states_updated(&states_map));
            }

            let _ = tx.send(DaemonEvent::running_services_updated(&running_services_list));
        }
    });
}

fn persist_fallback_agent_response(
    project_path: &str,
    agent_id: &str,
    response_text: &str,
) {
    // Exact registered-path lookup. The project-less system agent legitimately has no entry:
    // its response is still emitted to the UI below, but no chat turn is persisted.
    let project_entry = match crate::registry::find_project_by_path(project_path) {
        Ok(p) => Some(p),
        Err(e) => {
            crate::logger::log("WARN", "supervisor", &format!("persist_fallback_agent_response: agent '{}' not persisted: {}", agent_id, e));
            None
        }
    };
    let mut db_conversation_id = None;
    let mut agy_conversation_id = None;
    let mut inserted_turn_id = None;
    if let Some(ref proj) = project_entry {
        let conversation_result = crate::db::db_get_or_create_active_conversation(proj.id as i64, agent_id, None, None, None, None, None);
        if let Err(ref e) = conversation_result {
            crate::logger::log("ERROR", "supervisor", &format!("persist_fallback_agent_response: agent '{}' response not stored, no active conversation: {}", agent_id, e));
        }
        if let Ok(conv) = conversation_result {
            db_conversation_id = Some(conv.id.clone());
            agy_conversation_id = conv.native_session_id.clone();
            let turn = crate::db::ChatTurnRecord {
                id: None,
                conversation_id: conv.id,
                project_id: proj.id as i64,
                agent_id: agent_id.to_string(),
                agy_conversation_id: agy_conversation_id.clone(),
                role: "agent".to_string(),
                dialogue: response_text.to_string(),
                artifact_type: None,
                artifact_lang: None,
                artifact_label: None,
                artifact_file_path: None,
                artifact_content: None,
                is_ephemeral: false,
                terminal_snapshot: None,
                timestamp: crate::mcp::chrono_or_epoch() as i64,
                topic: None,
                approval_payload: None,
            };
            match crate::db::db_insert_chat_turn(&turn) {
                Ok(tid) => inserted_turn_id = Some(tid),
                Err(e) => crate::logger::log("ERROR", "supervisor", &format!("Failed to persist fallback agent chat turn: {}", e)),
            }
        }
    }

    let sup = get_supervisor();
    sup.emit_event(DaemonEvent::agent_response(&crate::events::AgentResponsePayload {
        agent_id: agent_id.to_string(),
        project_id: project_entry.as_ref().map(|p| p.id as i64),
        project_path: project_path.to_string(),
        conversation_id: db_conversation_id,
        turn_id: inserted_turn_id,
        dialogue: response_text.to_string(),
        artifact: None,
        artifact_type: None,
        artifact_lang: None,
        file_path: None,
        is_ephemeral: false,
        approval: None,
        agy_conversation_id,
        timestamp: crate::mcp::chrono_or_epoch() as i64,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_supervisor_spawn_and_read_output() {
        let sup = get_supervisor();
        let res = sup.spawn_service("/tmp", "test_svc", "echo", &["hello_from_supervisor".to_string()], None, None, None).await;
        assert!(res.is_ok());
        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
        let key = ProcessSupervisor::format_service_key("/tmp", "test_svc");
        let output = sup.read_output(&key, 50);
        assert!(output.iter().any(|l| l.contains("hello_from_supervisor")));
    }

    #[tokio::test]
    async fn test_supervisor_spawn_task_streaming() {
        let sup = get_supervisor();
        let (tx, mut rx) = tokio::sync::broadcast::channel::<DaemonEvent>(32);
        sup.set_event_tx(tx);

        let res = sup.spawn_task(
            1,
            "/tmp",
            "test_task",
            999,
            "echo",
            &["streamed_task_output_line".to_string()],
            None,
        ).await;

        assert!(res.is_ok());
        let run_res = res.unwrap();
        assert_eq!(run_res.exit_code, 0);
        assert_eq!(run_res.status, "completed");
        assert!(run_res.stdout.contains("streamed_task_output_line"));

        // Verify broadcast events were emitted
        let mut found_chunk = false;
        let mut found_completed = false;
        while let Ok(evt) = rx.try_recv() {
            if evt.event_type == "task-output-chunk" {
                if let Some(line) = evt.payload.get("line").and_then(|v| v.as_str()) {
                    if line.contains("streamed_task_output_line") {
                        found_chunk = true;
                    }
                }
            } else if evt.event_type == "task-completed" {
                found_completed = true;
            }
        }
        assert!(found_chunk);
        assert!(found_completed);
    }

    #[test]
    fn test_compute_project_state_uninitialized_has_initializer() {
        let sup = get_supervisor();
        // /tmp does not have rula.toml
        let state = sup.compute_project_state("/tmp", "temporary_project");
        let init_agent = state.agents.iter().find(|a| a.id == "initializer");
        assert!(init_agent.is_some(), "Uninitialized project must have transient initializer agent");
        let a = init_agent.unwrap();
        assert_eq!(a.role, "Project Initializer");
        assert!(a.is_internal);
        assert!(a.mcp_dialogue_mode);
    }

    #[tokio::test]
    async fn test_supervisor_retained_buffer_continuity() {
        let sup = get_supervisor();
        let key = "proj:test_retained";
        let mut initial_lines = VecDeque::new();
        initial_lines.push_back("Line 1: agent planning".to_string());
        initial_lines.push_back("Line 2: agent executing command".to_string());
        sup.store_retained_buffer(key, initial_lines);

        let output = sup.read_output(key, 10);
        assert_eq!(output.len(), 2);
        assert_eq!(output[0], "Line 1: agent planning");

        let fetched = sup.get_retained_buffer(key, 50).expect("should find retained buffer");
        assert_eq!(fetched.len(), 2);

        sup.clear_retained_buffer(key);
        assert!(sup.get_retained_buffer(key, 50).is_none());
        assert!(sup.read_output(key, 10).is_empty());
    }

    #[test]
    fn test_detect_script_port_precedence_and_parsing() {
        assert_eq!(ProcessSupervisor::detect_script_port(Some(9000), "bash run.sh", &[]), Some(9000));
        assert_eq!(
            ProcessSupervisor::detect_script_port(None, "uvicorn app:app", &["--port".to_string(), "8000".to_string()]),
            Some(8000)
        );
        assert_eq!(
            ProcessSupervisor::detect_script_port(None, "vite --port 5173", &[]),
            Some(5173)
        );
        assert_eq!(
            ProcessSupervisor::detect_script_port(None, "server", &["--port=3000".to_string()]),
            Some(3000)
        );
        assert_eq!(ProcessSupervisor::detect_script_port(None, "echo hello", &[]), None);
    }

    #[test]
    fn test_read_proc_environ_var_current_process() {
        let me = std::process::id();
        assert!(read_proc_environ_var(me, "PATH").is_some());
        assert!(read_proc_environ_var(me, "NON_EXISTENT_VAR_12345").is_none());
    }

    #[test]
    fn test_reconcile_orphan_processes_clean_sweep() {
        let res = reconcile_orphan_processes();
        assert!(res.is_ok(), "reconcile_orphan_processes must succeed even on fresh or empty db");
    }

    #[test]
    fn test_is_known_shim_dispatcher() {
        assert!(is_known_shim_dispatcher(Path::new("/home/user/.local/share/mise/shims/agy")));
        assert!(is_known_shim_dispatcher(Path::new("/custom/path/shims/tool")));
        assert!(!is_known_shim_dispatcher(Path::new("/bin/sh")));
        assert!(!is_known_shim_dispatcher(Path::new("/usr/bin/python3")));
    }

    #[test]
    fn test_resolve_executable_standard_commands() {
        let res = resolve_executable("sh");
        assert!(res.supports_custom_arg0);
        assert!(!res.program.as_os_str().is_empty());

        let res_empty = resolve_executable("");
        assert!(res_empty.supports_custom_arg0);

        let res_explicit = resolve_executable("/bin/sh");
        assert_eq!(res_explicit.program, PathBuf::from("/bin/sh"));
        assert!(res_explicit.supports_custom_arg0);
    }

    #[test]
    fn runtime_status_snapshot_aggregates_active_agents_and_counts() {
        let mut states_map: HashMap<usize, ProjectAgentState> = HashMap::new();
        states_map.insert(
            2,
            ProjectAgentState {
                project_path: "/tmp/beta".to_string(),
                project_name: "beta".to_string(),
                agents: vec![
                    AgentState {
                        id: "b-idle".to_string(),
                        role: "reviewer".to_string(),
                        runner: "codex".to_string(),
                        status: "idle".to_string(),
                        ..Default::default()
                    },
                    AgentState {
                        id: "a-unlaunched".to_string(),
                        role: "architect".to_string(),
                        runner: "claude".to_string(),
                        status: "unlaunched".to_string(),
                        ..Default::default()
                    },
                ],
            },
        );
        states_map.insert(
            1,
            ProjectAgentState {
                project_path: "/tmp/alpha".to_string(),
                project_name: "alpha".to_string(),
                agents: vec![
                    AgentState {
                        id: "c-failed".to_string(),
                        role: "tester".to_string(),
                        runner: "claude".to_string(),
                        status: "failed".to_string(),
                        ..Default::default()
                    },
                    AgentState {
                        id: "a-working".to_string(),
                        role: "builder".to_string(),
                        runner: "gemini".to_string(),
                        status: "working".to_string(),
                        ..Default::default()
                    },
                    AgentState {
                        id: "b-blocked".to_string(),
                        role: "ops".to_string(),
                        runner: "claude".to_string(),
                        status: "blocked".to_string(),
                        ..Default::default()
                    },
                ],
            },
        );

        let snap = build_runtime_status_snapshot(&states_map, 3, 2);
        assert_eq!(snap.projects_count, 2);
        assert_eq!(snap.working_agents_count, 1);
        assert_eq!(snap.idle_agents_count, 1);
        assert_eq!(snap.blocked_agents_count, 1);
        assert_eq!(snap.error_agents_count, 1);
        assert_eq!(snap.active_agents_count, 4);
        assert_eq!(snap.running_services_count, 3);
        assert_eq!(snap.pending_approvals_count, 2);
        assert_eq!(snap.active_agents.len(), 4);
        assert_eq!(snap.active_agents[0].project_name, "alpha");
        assert_eq!(snap.active_agents[0].agent_id, "a-working");
        assert_eq!(snap.active_agents[3].project_name, "beta");
        assert_eq!(snap.active_agents[3].agent_id, "b-idle");
    }
}

