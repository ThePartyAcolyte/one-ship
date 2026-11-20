use axum::{
    extract::{ConnectInfo, Query},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::sync::Semaphore;
use ts_rs::TS;
use tokio_util::sync::CancellationToken;

use crate::events::DaemonEvent;
use crate::mcp::{JsonRpcRequest, JsonRpcResponse};
use crate::supervisor::{get_supervisor, ProcessStatus, ProcessSupervisor, RwLockSafe};

/// Canonical service identifier used by `ProcessSupervisor` for a project's REA HTTP engine.
pub const REA_SERVICE_ID: &str = "rea_mcp";

/// Base TCP port from which deterministic per-project REA engine ports are derived.
pub const DEFAULT_REA_BASE_PORT: u16 = 8790;

/// Maximum number of recent and active queue records retained in the bounded ring buffer.
pub const MAX_REA_QUEUE_HISTORY_COUNT: usize = 200;

/// Largest target file hashed by `list_rea_targets`; larger files report `large-file-deferred`.
const MAX_TARGET_HASH_BYTES: u64 = 256 * 1024 * 1024;

/// Maximum polling attempts when waiting for the REA HTTP engine to bind its local TCP port.
const REA_ENGINE_BOOT_MAX_ATTEMPTS_COUNT: usize = 60;

/// Step delay in milliseconds between TCP readiness probes during REA engine startup.
const REA_ENGINE_BOOT_POLL_STEP_MS: u64 = 250;

/// Interval in milliseconds between background idle reaper sweeps.
const REA_IDLE_REAPER_INTERVAL_MS: u64 = 15_000;

/// Domain-specific error hierarchy for the REA Reverse Engineering Kopeng module.
#[derive(Debug, Error)]
pub enum ReaError {
    /// Target project directory does not exist on disk.
    #[error("Project path does not exist: {path}")]
    ProjectNotFound { path: String },

    /// Caller project context could not be resolved unambiguously (Rule 11).
    #[error("Unambiguous project context required for REA operation: {details}")]
    ProjectContextMissing { details: String },

    /// Filesystem I/O failure during REA workspace initialization or artifact scanning.
    #[error("{context}: {source}")]
    Io {
        context: String,
        source: std::io::Error,
    },

    /// Process supervision or engine readiness failure.
    #[error("REA engine lifecycle error: {details}")]
    EngineLifecycle { details: String },

    /// Upstream HTTP forwarding failure when communicating with the local REA engine.
    #[error("REA MCP upstream transport error: {details}")]
    UpstreamTransport { details: String },

    /// The `rea` binary is not installed (there is no `npx` substitute).
    #[error("The `rea` binary is not installed; install it with `npm install -g rea-agents@latest`")]
    EngineNotInstalled,

    /// A REA setting has an unusable value.
    #[error("Invalid REA setting {key}: {details}")]
    InvalidSetting { key: &'static str, details: String },

    /// A REA view file produced by the engine cannot be read as JSON.
    #[error("Invalid REA view {path}: {details}")]
    InvalidView { path: String, details: String },

    /// A blocking worker task failed.
    #[error("Background task failed: {0}")]
    Task(String),

    #[error(transparent)]
    Registry(#[from] crate::registry::RegistryError),

    #[error(transparent)]
    Config(#[from] crate::config::ConfigError),

    #[error(transparent)]
    Skill(#[from] crate::kopeng::skill::SkillError),
}

impl crate::api::ApiError for ReaError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::ProjectNotFound { .. } => StatusCode::NOT_FOUND,
            Self::ProjectContextMissing { .. } | Self::InvalidSetting { .. } => StatusCode::BAD_REQUEST,
            Self::EngineNotInstalled => StatusCode::FAILED_DEPENDENCY,
            Self::UpstreamTransport { .. } => StatusCode::BAD_GATEWAY,
            Self::Io { .. } | Self::EngineLifecycle { .. } | Self::InvalidView { .. } | Self::Task(_) | Self::Skill(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Registry(e) => e.status(),
            Self::Config(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::ProjectNotFound { .. } => "ReaError::ProjectNotFound",
            Self::ProjectContextMissing { .. } => "ReaError::ProjectContextMissing",
            Self::Io { .. } => "ReaError::Io",
            Self::EngineLifecycle { .. } => "ReaError::EngineLifecycle",
            Self::UpstreamTransport { .. } => "ReaError::UpstreamTransport",
            Self::EngineNotInstalled => "ReaError::EngineNotInstalled",
            Self::InvalidSetting { .. } => "ReaError::InvalidSetting",
            Self::InvalidView { .. } => "ReaError::InvalidView",
            Self::Task(_) => "ReaError::Task",
            Self::Skill(_) => "ReaError::Skill",
            Self::Registry(e) => e.code(),
            Self::Config(e) => e.code(),
        }
    }
}

/// Arguments of the daemon command `check_rea_toolchain`; the project sets the Ghidra directory.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CheckReaToolchainArgs {
    pub project_path: Option<String>,
}

/// Arguments of the daemon command `toggle_project_rea_plugin`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ToggleProjectReaPluginArgs {
    pub project_path: String,
    pub enabled: bool,
}

fn io_err(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> ReaError {
    let context = context.into();
    move |source| ReaError::Io { context, source }
}

/// A registered project's path; the REA commands act only on registered projects (Rule 11).
fn require_registered_project(project_path: &str) -> Result<String, ReaError> {
    Ok(crate::registry::find_project_by_path(project_path)?.path)
}

/// Diagnostic status of an individual reverse-engineering tool on the host system.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct ReaToolStatus {
    /// Canonical tool identifier (e.g. `"rea-agents"`, `"ghidra"`, `"radare2"`).
    pub id: String,
    /// Human-readable display name.
    pub name: String,
    /// Binary or command probed on the host system.
    pub binary: String,
    /// Category of targets handled by this tool (`"core"`, `"native"`, `"mobile"`, `"firmware"`).
    pub category: String,
    /// Whether the tool is currently installed and reachable.
    pub is_installed: bool,
    /// Detected version string if available.
    pub version: Option<String>,
    /// Recommended installation command for the detected Linux distribution.
    pub install_hint: String,
    /// Whether `install_missing_tools` can install this tool via the host package manager.
    pub auto_installable: bool,
}

/// Live operational and storage status of a project's REA Reverse Engineering engine.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct ReaEngineStatus {
    /// Absolute path of the owning project.
    pub project_path: String,
    /// Whether `[plugins.rea] enabled = true` is set in `rula.toml`.
    pub is_enabled: bool,
    /// Whether the supervised `rea_mcp` HTTP process is currently running.
    pub is_running: bool,
    /// Operating system PID if the engine is currently active.
    pub pid: Option<u32>,
    /// Deterministic local TCP port assigned to this project's REA HTTP server.
    pub http_port: u16,
    /// ONE-SHIP gateway URL injected into `.mcp.json` and `opencode.json`.
    pub gateway_url: String,
    /// Supervisor key (`${project_path}:svc:rea_mcp`).
    pub service_key: String,
    /// Number of MCP requests currently executing in flight for this project.
    pub in_flight_requests_count: usize,
    /// Number of requests for this project currently waiting in the FIFO queue.
    pub queued_requests_count: usize,
    /// Epoch timestamp in milliseconds of the last REA activity in this project.
    #[ts(type = "number")]
    pub last_activity_epoch_ms: u64,
    /// Configured inactivity timeout in seconds before auto-shutdown.
    #[ts(type = "number")]
    pub idle_timeout_seconds: u64,
    /// Total disk usage in bytes of `<project_path>/.one-ship/rea/`.
    #[ts(type = "number")]
    pub data_dir_size_bytes: u64,
    /// Number of target binaries/artifacts currently present in `<project_path>/targets/`.
    pub targets_count: usize,
}

/// Metadata for a target binary, firmware image, or package inside `<project_path>/targets/`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct ReaTargetArtifact {
    /// File name inside `targets/` (e.g. `"xochitl"`, `"firmware.bin"`).
    pub name: String,
    /// Path relative to the project root (e.g. `"targets/xochitl"`).
    pub rel_path: String,
    /// File size in bytes.
    #[ts(type = "number")]
    pub size_bytes: u64,
    /// SHA-256 cryptographic digest (hex-encoded) for REA evidence provenance.
    pub sha256_hex: String,
    /// Detected binary or container format based on magic header bytes and extension.
    pub detected_format: String,
    /// Last modification timestamp in epoch seconds.
    #[ts(type = "number")]
    pub modified_epoch_seconds: u64,
}

/// Record representing an individual reverse-engineering MCP job in the global FIFO queue.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct ReaQueuedJob {
    /// Unique monotonic job identifier (`rea-job-<counter>`).
    pub job_id: String,
    /// Project path that originated the request.
    pub project_path: String,
    /// Project display name.
    pub project_name: String,
    /// Originating agent persona identifier (e.g. `"reverser"`, `"architect"`).
    pub agent_id: String,
    /// MCP method or REA tool name being invoked.
    pub tool_name: String,
    /// Whether this job requires a heavy concurrency slot (e.g. binary decompilation).
    pub is_heavy: bool,
    /// Lifecycle status: `"queued"`, `"running"`, `"completed"`, or `"failed"`.
    pub status: String,
    /// Timestamp in epoch milliseconds when the request entered the queue.
    #[ts(type = "number")]
    pub enqueued_at_ms: u64,
    /// Timestamp in epoch milliseconds when execution started.
    #[ts(type = "number | null")]
    pub started_at_ms: Option<u64>,
    /// Timestamp in epoch milliseconds when execution completed.
    #[ts(type = "number | null")]
    pub completed_at_ms: Option<u64>,
}

/// Logical subsystem node discovered inside a reverse-engineered binary or firmware target.
#[derive(Serialize, Deserialize, Clone, Debug, Default, TS)]
#[ts(export)]
pub struct ReaSubsystemNode {
    /// Unique identifier for `v-for` keying.
    pub id: String,
    /// Subsystem name (e.g. `"Cloud Sync Engine"`, `"Framebuffer Renderer"`).
    pub name: String,
    /// Summary of the subsystem's role in the binary.
    pub description: String,
    /// Key symbols or modules belonging to this subsystem.
    #[serde(default)]
    pub symbols: Vec<String>,
}

/// Directed call-graph or cross-reference (`xref`) edge between two binary symbols.
#[derive(Serialize, Deserialize, Clone, Debug, Default, TS)]
#[ts(export)]
pub struct ReaCallFlowEdge {
    /// Unique identifier for `v-for` keying.
    pub id: String,
    /// Caller function or module symbol.
    pub from_symbol: String,
    /// Callee function or target symbol.
    pub to_symbol: String,
    /// Hexadecimal address offset in the binary (e.g. `"0x0041a8c0"`); `None` when not recorded.
    pub offset_hex: Option<String>,
    /// Plain-language explanation of this interaction.
    pub summary: String,
}

/// Curated decompiled function highlight with readable pseudocode and architectural explanation.
#[derive(Serialize, Deserialize, Clone, Debug, Default, TS)]
#[ts(export)]
pub struct ReaDecompiledHighlight {
    /// Unique identifier for `v-for` keying.
    pub id: String,
    /// Function symbol or mangled/demangled name.
    pub symbol: String,
    /// Binary address offset (e.g. `"0x0041a8c0"`); `None` when not recorded.
    pub offset_hex: Option<String>,
    /// Owning subsystem label; `None` when not assigned.
    pub subsystem: Option<String>,
    /// Reconstructed C/C++/TS function signature; `None` when not reconstructed.
    pub signature: Option<String>,
    /// Decompiled C/C++/TS pseudocode body.
    pub pseudocode: String,
    /// Agent's plain-language explanation of what this function does and why it matters.
    pub explanation: String,
}

/// Extracted network endpoint, IPC route, SQLite schema, or binary protocol structure.
#[derive(Serialize, Deserialize, Clone, Debug, Default, TS)]
#[ts(export)]
pub struct ReaProtocolItem {
    /// Unique identifier for `v-for` keying.
    pub id: String,
    /// Category: `"rest"`, `"websocket"`, `"ipc"`, `"sqlite"`, `"binary_format"`, or `"crypto"`; `None` when not classified.
    pub category: Option<String>,
    /// URL, IPC channel name, table name, or magic header.
    pub endpoint_or_key: String,
    /// Transport or format specification.
    pub protocol: String,
    /// Binary offset or source location where this artifact was proven; `None` when not recorded.
    pub offset_hex: Option<String>,
    /// Architectural explanation and payload layout.
    pub description: String,
}

/// Aggregated visual exploration bundle loaded from `<project_path>/.one-ship/rea/views/`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, TS)]
#[ts(export)]
pub struct ReaVisualViewsBundle {
    /// Discovered high-level subsystems (`architecture_map.json`).
    pub subsystems: Vec<ReaSubsystemNode>,
    /// Directed call-flow edges (`architecture_map.json`).
    pub call_edges: Vec<ReaCallFlowEdge>,
    /// Decompiled C/C++/TS pseudocode highlights (`decompiled_highlights.json`).
    pub decompiled_functions: Vec<ReaDecompiledHighlight>,
    /// Extracted protocols, endpoints, and schemas (`protocols_and_endpoints.json`).
    pub protocol_items: Vec<ReaProtocolItem>,
}

/// Per-project runtime counters tracked by the REA orchestrator.
#[derive(Debug, Clone, Default)]
struct ProjectReaRuntimeMetrics {
    in_flight_requests_count: usize,
    queued_requests_count: usize,
    last_activity_epoch_ms: u64,
}

/// Global poison-resistant state coordinating the multi-project REA FIFO queue and idle reaper.
pub struct ReaOrchestratorState {
    /// Global semaphore bounding concurrent heavy decompilation jobs across all projects.
    heavy_job_semaphore: Arc<Semaphore>,
    /// Bounded ring buffer of active and recent REA queue jobs.
    queue_ring: RwLock<VecDeque<ReaQueuedJob>>,
    /// Per-project activity and in-flight counters.
    project_metrics: RwLock<HashMap<String, ProjectReaRuntimeMetrics>>,
    /// Monotonic counter for generating unique job IDs.
    job_seq_counter: std::sync::atomic::AtomicU64,
    /// Per-project async startup lock to prevent duplicate concurrent spawns of `rea_mcp`.
    boot_locks: RwLock<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

static REA_ORCHESTRATOR: OnceLock<ReaOrchestratorState> = OnceLock::new();

/// Returns a reference to the singleton `ReaOrchestratorState`.
pub fn get_rea_orchestrator() -> &'static ReaOrchestratorState {
    REA_ORCHESTRATOR.get_or_init(|| {
        let initial_cap = get_max_concurrent_heavy_jobs_count();
        ReaOrchestratorState {
            heavy_job_semaphore: Arc::new(Semaphore::new(initial_cap)),
            queue_ring: RwLock::new(VecDeque::with_capacity(MAX_REA_QUEUE_HISTORY_COUNT)),
            project_metrics: RwLock::new(HashMap::new()),
            job_seq_counter: std::sync::atomic::AtomicU64::new(1),
            boot_locks: RwLock::new(HashMap::new()),
        }
    })
}

fn epoch_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn get_max_concurrent_heavy_jobs_count() -> usize {
    crate::config::resolve_effective_usize(
        None,
        None,
        "rea",
        "max_concurrent_heavy_jobs_count",
    )
    .max(1)
}

fn get_project_idle_timeout_seconds(project_path: &str) -> u64 {
    crate::config::resolve_effective_u64(
        Some(project_path),
        None,
        "rea",
        "idle_timeout_seconds",
    )
}

/// `rea.ghidra_install_dir` for the project (or the global value); an empty setting is an error.
fn get_project_ghidra_install_dir(project_path: Option<&str>) -> Result<String, ReaError> {
    let val = crate::config::resolve_effective_string(project_path, None, "rea", "ghidra_install_dir");
    if val.trim().is_empty() {
        return Err(ReaError::InvalidSetting { key: "ghidra_install_dir", details: "empty path".to_string() });
    }
    Ok(val)
}

fn get_project_max_jvm_heap_mbytes(project_path: &str) -> u64 {
    crate::config::resolve_effective_u64(
        Some(project_path),
        None,
        "rea",
        "max_jvm_heap_mbytes",
    )
    .max(512)
}

/// Checks whether `[plugins.rea] enabled = true` is configured in the project's `rula.toml`.
pub fn is_project_rea_enabled(project_path: &Path) -> bool {
    let Some(rula) = crate::config::load_project_rula(project_path) else {
        return false;
    };
    rula.get("plugins")
        .and_then(|p| p.get("rea"))
        .and_then(|r| r.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Local TCP port of a registered project's internal `rea mcp --http` server:
/// `DEFAULT_REA_BASE_PORT + project id`, unique per project. An unregistered project is an error.
pub fn resolve_project_rea_port(project_path: &str) -> Result<u16, ReaError> {
    let project = crate::registry::find_project_by_path(project_path)?;
    u16::try_from(project.id)
        .ok()
        .and_then(|id| DEFAULT_REA_BASE_PORT.checked_add(id))
        .ok_or_else(|| ReaError::InvalidSetting {
            key: "rea port",
            details: format!("project id {} does not fit in the port range above {}", project.id, DEFAULT_REA_BASE_PORT),
        })
}

/// Builds the ONE-SHIP MCP gateway URL for a specific project path.
pub fn build_project_rea_gateway_url(mcp_port: u16, project_path: &str) -> String {
    let encoded_hex = project_path
        .as_bytes()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    format!("http://localhost:{}/mcp/rea?project_hex={}", mcp_port, encoded_hex)
}

fn decode_hex_string(hex_str: &str) -> Option<String> {
    if !hex_str.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(hex_str.len() / 2);
    // Byte pairs, not `&str` slices: a non-ASCII parameter must be rejected, not panic.
    for pair in hex_str.as_bytes().chunks(2) {
        let digits = std::str::from_utf8(pair).ok()?;
        bytes.push(u8::from_str_radix(digits, 16).ok()?);
    }
    String::from_utf8(bytes).ok()
}

impl ReaOrchestratorState {
    fn get_project_boot_lock(&self, project_path: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.boot_locks.write_safe("rea:get_project_boot_lock");
        locks
            .entry(project_path.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    /// Touches the activity timestamp for a project so the idle reaper knows it was recently used.
    pub fn touch_project_activity(&self, project_path: &str) {
        let now_ms = epoch_now_ms();
        let mut map = self.project_metrics.write_safe("rea:touch_project_activity");
        let entry = map.entry(project_path.to_string()).or_default();
        entry.last_activity_epoch_ms = now_ms;
    }

    fn enqueue_job(
        &self,
        project_path: &str,
        agent_id: &str,
        tool_name: &str,
        is_heavy: bool,
    ) -> String {
        let seq = self
            .job_seq_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let job_id = format!("rea-job-{}", seq);
        let now_ms = epoch_now_ms();

        let project_name = Path::new(project_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_string();

        let job = ReaQueuedJob {
            job_id: job_id.clone(),
            project_path: project_path.to_string(),
            project_name,
            agent_id: agent_id.to_string(),
            tool_name: tool_name.to_string(),
            is_heavy,
            status: "queued".to_string(),
            enqueued_at_ms: now_ms,
            started_at_ms: None,
            completed_at_ms: None,
        };

        {
            let mut ring = self.queue_ring.write_safe("rea:enqueue_job:ring");
            if ring.len() >= MAX_REA_QUEUE_HISTORY_COUNT {
                ring.pop_front();
            }
            ring.push_back(job);
        }

        {
            let mut metrics = self.project_metrics.write_safe("rea:enqueue_job:metrics");
            let entry = metrics.entry(project_path.to_string()).or_default();
            entry.queued_requests_count += 1;
            entry.last_activity_epoch_ms = now_ms;
        }

        self.emit_queue_event();
        job_id
    }

    fn mark_job_running(&self, job_id: &str, project_path: &str) {
        let now_ms = epoch_now_ms();
        {
            let mut ring = self.queue_ring.write_safe("rea:mark_job_running:ring");
            if let Some(job) = ring.iter_mut().find(|j| j.job_id == job_id) {
                job.status = "running".to_string();
                job.started_at_ms = Some(now_ms);
            }
        }
        {
            let mut metrics = self.project_metrics.write_safe("rea:mark_job_running:metrics");
            let entry = metrics.entry(project_path.to_string()).or_default();
            entry.queued_requests_count = entry.queued_requests_count.saturating_sub(1);
            entry.in_flight_requests_count += 1;
            entry.last_activity_epoch_ms = now_ms;
        }
        self.emit_queue_event();
    }

    fn mark_job_finished(&self, job_id: &str, project_path: &str, succeeded: bool) {
        let now_ms = epoch_now_ms();
        {
            let mut ring = self.queue_ring.write_safe("rea:mark_job_finished:ring");
            if let Some(job) = ring.iter_mut().find(|j| j.job_id == job_id) {
                job.status = if succeeded {
                    "completed".to_string()
                } else {
                    "failed".to_string()
                };
                job.completed_at_ms = Some(now_ms);
            }
        }
        {
            let mut metrics = self.project_metrics.write_safe("rea:mark_job_finished:metrics");
            let entry = metrics.entry(project_path.to_string()).or_default();
            entry.in_flight_requests_count = entry.in_flight_requests_count.saturating_sub(1);
            entry.last_activity_epoch_ms = now_ms;
        }
        self.emit_queue_event();
    }

    fn emit_queue_event(&self) {
        let snapshot: Vec<ReaQueuedJob> = {
            let ring = self.queue_ring.read_safe("rea:emit_queue_event");
            ring.iter().cloned().collect()
        };
        get_supervisor().emit_event(DaemonEvent::rea_queue_updated(&crate::events::ReaQueueEventPayload {
            jobs: snapshot,
            timestamp_ms: epoch_now_ms(),
        }));
    }

    /// Returns a snapshot of the bounded REA job queue (newest first).
    pub fn get_queue_snapshot(&self) -> Vec<ReaQueuedJob> {
        let ring = self.queue_ring.read_safe("rea:get_queue_snapshot");
        ring.iter().rev().cloned().collect()
    }
}

/// Resolves the configured targets directory (`[plugins.rea].targets_dir`) safely within `project_path`.
fn resolve_rea_targets_dir(project_path: &Path) -> Result<(PathBuf, String), ReaError> {
    crate::config::resolve_project_subdir(project_path, "rea", "targets_dir")
        .map_err(|e| ReaError::InvalidSetting {
            key: "rea.targets_dir",
            details: e.to_string(),
        })
}

/// Ensures the project's isolated `.one-ship/rea/` and configured targets directory hierarchy exists.
pub fn ensure_project_rea_directories(project_path: &Path) -> Result<PathBuf, ReaError> {
    if !project_path.exists() {
        return Err(ReaError::ProjectNotFound {
            path: project_path.display().to_string(),
        });
    }

    let rea_root = project_path.join(".one-ship").join("rea");
    let subdirs = ["data", "cache", "config", "tmp", "views"];
    for sub in &subdirs {
        let dir_path = rea_root.join(sub);
        fs::create_dir_all(&dir_path).map_err(io_err(format!("Cannot create {}", dir_path.display())))?;
    }

    let (targets_dir, _) = resolve_rea_targets_dir(project_path)?;
    fs::create_dir_all(&targets_dir).map_err(io_err(format!("Cannot create {}", targets_dir.display())))?;

    Ok(rea_root)
}

/// Checks if the local REA HTTP port is accepting TCP connections.
async fn is_rea_port_listening(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    matches!(
        tokio::time::timeout(
            std::time::Duration::from_millis(200),
            tokio::net::TcpStream::connect(addr),
        )
        .await,
        Ok(Ok(_))
    )
}

/// Starts or adopts the project's supervised `rea_mcp` HTTP server on-demand.
/// Safe against concurrent invocations for the same project via per-project async mutex.
pub async fn ensure_rea_engine_running(project_path: &str) -> Result<u16, ReaError> {
    let orchestrator = get_rea_orchestrator();
    let boot_lock = orchestrator.get_project_boot_lock(project_path);
    let _guard = boot_lock.lock().await;

    let port = resolve_project_rea_port(project_path)?;
    let sup = get_supervisor();
    let key = ProcessSupervisor::format_service_key(project_path, REA_SERVICE_ID);
    let current_status = sup.get_status(&key);

    if matches!(
        current_status,
        ProcessStatus::Working | ProcessStatus::Starting | ProcessStatus::Idle
    ) && is_rea_port_listening(port).await
    {
        orchestrator.touch_project_activity(project_path);
        return Ok(port);
    }

    let p_path = Path::new(project_path);
    let rea_root = ensure_project_rea_directories(p_path)?;

    let data_dir = rea_root.join("data");
    let cache_dir = rea_root.join("cache");
    let config_dir = rea_root.join("config");
    let tmp_dir = rea_root.join("tmp");
    let ghidra_dir = get_project_ghidra_install_dir(Some(project_path))?;
    let max_heap_mbytes = get_project_max_jvm_heap_mbytes(project_path);

    // The installed `rea` binary is the only engine (Spec 26 §2); no runtime `npx` download.
    let (has_rea, _) = probe_command_version("rea", &["--version"]).await;
    if !has_rea {
        return Err(ReaError::EngineNotInstalled);
    }
    let rea_invocation = format!("rea mcp --http --host 127.0.0.1 --port {}", port);

    let full_cmd = format!(
        "env XDG_DATA_HOME='{}' XDG_CACHE_HOME='{}' XDG_CONFIG_HOME='{}' TMPDIR='{}' GHIDRA_INSTALL_DIR='{}' MAXMEM='{}M' {}",
        data_dir.display(),
        cache_dir.display(),
        config_dir.display(),
        tmp_dir.display(),
        ghidra_dir,
        max_heap_mbytes,
        rea_invocation
    );

    crate::logger::log(
        "INFO",
        "rea",
        &format!(
            "Auto-waking supervised REA HTTP engine for '{}' on port {}",
            project_path, port
        ),
    );

    sup.spawn_service(project_path, REA_SERVICE_ID, &full_cmd, &[], None, Some(port), None)
        .await
        .map_err(|e| ReaError::EngineLifecycle { details: e.to_string() })?;

    orchestrator.touch_project_activity(project_path);

    // Bounded readiness polling loop (Tiger Style Rule 8.4)
    for _attempt in 0..REA_ENGINE_BOOT_MAX_ATTEMPTS_COUNT {
        if is_rea_port_listening(port).await {
            crate::logger::log(
                "INFO",
                "rea",
                &format!(
                    "REA HTTP engine ready for '{}' on 127.0.0.1:{}",
                    project_path, port
                ),
            );
            return Ok(port);
        }
        tokio::time::sleep(std::time::Duration::from_millis(
            REA_ENGINE_BOOT_POLL_STEP_MS,
        ))
        .await;
    }

    Err(ReaError::EngineLifecycle {
        details: format!(
            "REA HTTP engine on port {} did not become ready within {} ms",
            port,
            REA_ENGINE_BOOT_MAX_ATTEMPTS_COUNT as u64 * REA_ENGINE_BOOT_POLL_STEP_MS
        ),
    })
}

/// Stops a project's supervised `rea_mcp` process and its entire child process group (`-pgid`),
/// releasing 100% of Node.js and headless Ghidra JVM memory while preserving disk evidence.
pub async fn stop_rea_engine_internal(project_path: &str) -> Result<(), ReaError> {
    let key = ProcessSupervisor::format_service_key(project_path, REA_SERVICE_ID);
    crate::logger::log(
        "INFO",
        "rea",
        &format!(
            "Stopping supervised REA HTTP engine for '{}' (key: '{}')",
            project_path, key
        ),
    );
    get_supervisor()
        .stop_process(&key)
        .await
        .map_err(|e| ReaError::EngineLifecycle { details: e.to_string() })
}

/// Spawns the background idle reaper loop that monitors active REA engines and stops them
/// ONLY when a project has 0 in-flight requests, 0 queued jobs, and has exceeded `idle_timeout_seconds`.
/// Switching projects in the UI never triggers termination.
pub fn spawn_rea_idle_reaper_loop(cancel_token: CancellationToken) {
    tauri::async_runtime::spawn(async move {
        crate::logger::log(
            "INFO",
            "rea",
            "Started REA activity-based idle reaper loop",
        );
        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    crate::logger::log("INFO", "rea", "REA idle reaper loop shutting down");
                    break;
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(REA_IDLE_REAPER_INTERVAL_MS)) => {
                    let now_ms = epoch_now_ms();
                    let candidates: Vec<(String, u64)> = {
                        let orchestrator = get_rea_orchestrator();
                        let map = orchestrator.project_metrics.read_safe("rea:idle_reaper:read");
                        map.iter()
                            .filter(|(_, m)| m.in_flight_requests_count == 0 && m.queued_requests_count == 0 && m.last_activity_epoch_ms > 0)
                            .map(|(path, m)| (path.clone(), m.last_activity_epoch_ms))
                            .collect()
                    };

                    let sup = get_supervisor();
                    for (project_path, last_active_ms) in candidates {
                        let key = ProcessSupervisor::format_service_key(&project_path, REA_SERVICE_ID);
                        let st = sup.get_status(&key);
                        if !matches!(st, ProcessStatus::Working | ProcessStatus::Starting | ProcessStatus::Idle) {
                            continue;
                        }
                        let timeout_ms = get_project_idle_timeout_seconds(&project_path).saturating_mul(1000);
                        if timeout_ms > 0 && now_ms.saturating_sub(last_active_ms) >= timeout_ms {
                            crate::logger::log(
                                "INFO",
                                "rea",
                                &format!(
                                    "[IDLE REAPER] Project '{}' REA engine idle for {}s (0 active/queued requests). Stopping engine to reclaim RAM.",
                                    project_path,
                                    now_ms.saturating_sub(last_active_ms) / 1000
                                ),
                            );
                            crate::logger::log_failure("rea", "idle reaper stop", stop_rea_engine_internal(&project_path).await);
                        }
                    }
                }
            }
        }
    });
}

/// Query parameters accepted by `/mcp/rea`.
#[derive(Debug, Deserialize, Default)]
pub struct ReaGatewayQuery {
    /// Hex-encoded project path injected by `inject_project_mcp_plugin`.
    pub project_hex: Option<String>,
}

/// Resolves the target project path and originating agent identity for an incoming `/mcp/rea` call.
/// Adheres strictly to Rule 11 (Zero Permissive Fallbacks): requires either an attested `CallerContext`
/// or an explicit, registered project parameter.
async fn resolve_rea_gateway_origin(
    connect_info: Option<ConnectInfo<SocketAddr>>,
    server_port: u16,
    query: &ReaGatewayQuery,
) -> Result<(String, String), ReaError> {
    if let Some(ConnectInfo(peer)) = connect_info {
        let resolved = tokio::task::spawn_blocking(move || {
            crate::caller_identity::resolve_caller(peer, server_port)
        })
        .await
        .map_err(|e| ReaError::Task(e.to_string()))?;

        // Only a caller that is not a managed agent may scope itself by `project_hex`; any other
        // identity failure rejects the request (same rule as the `/mcp` endpoint).
        match resolved {
            Ok(caller) => {
                if let Some(p_path) = caller.project_path {
                    let agent = caller.agent_uid.or(caller.agent_display_name).unwrap_or_else(|| caller.instance_id.clone());
                    return Ok((p_path, agent));
                }
            }
            Err(crate::caller_identity::CallerError::UnmanagedCaller { .. }) => {}
            Err(e) => return Err(ReaError::ProjectContextMissing { details: e.to_string() }),
        }
    }

    let explicit_path = match query.project_hex.as_deref() {
        Some(hex) => Some(decode_hex_string(hex).ok_or_else(|| ReaError::ProjectContextMissing {
            details: format!("project_hex '{}' is not hex-encoded UTF-8", hex),
        })?),
        None => None,
    };

    if let Some(candidate_path) = explicit_path {
        let projects = crate::db::db_get_projects().map_err(|e| ReaError::ProjectContextMissing {
            details: e.to_string(),
        })?;
        if let Some(verified) = projects.into_iter().find(|p| p.path == candidate_path) {
            return Ok((verified.path, "mcp-client".to_string()));
        }
        return Err(ReaError::ProjectNotFound {
            path: candidate_path,
        });
    }

    Err(ReaError::ProjectContextMissing {
        details: "Caller is neither an attested ONE-SHIP agent process nor scoped via a registered ?project_hex parameter.".to_string(),
    })
}

/// Forwards a JSON-RPC request payload to a project's local `rea mcp --http` server.
async fn forward_jsonrpc_to_rea_engine(
    rea_port: u16,
    req_method: &str,
    req_id: &Value,
    req_params: &Option<Value>,
) -> Result<Value, ReaError> {
    let url = format!("http://127.0.0.1:{}/mcp", rea_port);
    let payload = json!({
        "jsonrpc": "2.0",
        "id": req_id,
        "method": req_method,
        "params": req_params.clone().unwrap_or_else(|| json!({})),
    });
    let body_str = serde_json::to_string(&payload).map_err(|e| ReaError::UpstreamTransport {
        details: e.to_string(),
    })?;

    let output = tokio::process::Command::new("curl")
        .args([
            "-s",
            "-S",
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "-H",
            "Accept: application/json, text/event-stream",
            "--max-time",
            "320",
            "--data-raw",
            &body_str,
            &url,
        ])
        .output()
        .await
        .map_err(|e| ReaError::UpstreamTransport {
            details: format!("Failed to invoke curl to local REA engine: {}", e),
        })?;

    if !output.status.success() {
        let stderr_str = String::from_utf8_lossy(&output.stderr);
        return Err(ReaError::UpstreamTransport {
            details: format!(
                "REA engine on port {} returned error (exit {:?}): {}",
                rea_port,
                output.status.code(),
                stderr_str.trim()
            ),
        });
    }

    let raw_stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = raw_stdout.trim();

    // Handle both standard JSON and SSE (`data: {...}`) responses from Streamable HTTP MCP servers
    if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
        return Ok(parsed);
    }

    for line in trimmed.lines().rev() {
        if let Some(data_json) = line.strip_prefix("data:") {
            if let Ok(parsed) = serde_json::from_str::<Value>(data_json.trim()) {
                return Ok(parsed);
            }
        }
    }

    Err(ReaError::UpstreamTransport {
        details: format!(
            "Unparseable response from REA engine on port {}: {}",
            rea_port,
            trimmed.chars().take(240).collect::<String>()
        ),
    })
}

/// Axum HTTP POST handler mounted at `/mcp/rea` on ONE-SHIP's MCP daemon.
/// Implements:
/// 1. Strict project origin attribution (Rule 11).
/// 2. Multi-project asynchronous FIFO queuing and global heavy-job concurrency gating.
/// 3. Lazy on-demand wakeup of the target project's `rea_mcp` process.
pub async fn rea_mcp_gateway_handler(
    connect_info: Option<ConnectInfo<SocketAddr>>,
    Query(query): Query<ReaGatewayQuery>,
    Extension(_tx): Extension<tokio::sync::broadcast::Sender<DaemonEvent>>,
    Json(req): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    if req.id.is_none() {
        return Json(JsonRpcResponse {
            jsonrpc: "2.0",
            id: Value::Null,
            result: Some(json!({"status": "ok"})),
            error: None,
        });
    }
    let req_id = req.id.clone().unwrap_or(Value::Null);

    let mcp_port = crate::config::mcp_daemon_port();
    let (project_path, agent_id) = match resolve_rea_gateway_origin(connect_info, mcp_port, &query).await {
        Ok(origin) => origin,
        Err(e) => {
            crate::logger::log("WARN", "rea", &format!("[REA GATEWAY REJECTED] {}", e));
            return Json(JsonRpcResponse {
                jsonrpc: "2.0",
                id: req_id,
                result: None,
                error: Some(json!({
                    "code": -32001,
                    "message": e.to_string(),
                })),
            });
        }
    };

    let is_tool_call = req.method == "tools/call";
    let tool_label = if is_tool_call {
        req.params
            .as_ref()
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("tools/call")
            .to_string()
    } else {
        req.method.clone()
    };

    let orchestrator = get_rea_orchestrator();
    let job_id = orchestrator.enqueue_job(&project_path, &agent_id, &tool_label, is_tool_call);

    // Acquire global heavy-job semaphore permit in FIFO order for tool executions
    let _heavy_permit = if is_tool_call {
        match orchestrator.heavy_job_semaphore.clone().acquire_owned().await {
            Ok(permit) => Some(permit),
            Err(e) => {
                orchestrator.mark_job_finished(&job_id, &project_path, false);
                return Json(JsonRpcResponse {
                    jsonrpc: "2.0",
                    id: req_id,
                    result: None,
                    error: Some(json!({
                        "code": -32603,
                        "message": format!("REA queue semaphore error: {}", e),
                    })),
                });
            }
        }
    } else {
        None
    };

    orchestrator.mark_job_running(&job_id, &project_path);

    let rea_port = match ensure_rea_engine_running(&project_path).await {
        Ok(p) => p,
        Err(e) => {
            orchestrator.mark_job_finished(&job_id, &project_path, false);
            return Json(JsonRpcResponse {
                jsonrpc: "2.0",
                id: req_id,
                result: Some(json!({
                    "content": [{"type": "text", "text": format!("REA Engine failed to start for project '{}': {}", project_path, e)}],
                    "isError": true
                })),
                error: None,
            });
        }
    };

    match forward_jsonrpc_to_rea_engine(rea_port, &req.method, &req_id, &req.params).await {
        Ok(upstream_val) => {
            orchestrator.mark_job_finished(&job_id, &project_path, true);
            let result = upstream_val.get("result").cloned();
            let error = upstream_val.get("error").cloned();
            Json(JsonRpcResponse {
                jsonrpc: "2.0",
                id: req_id,
                result,
                error,
            })
        }
        Err(e) => {
            orchestrator.mark_job_finished(&job_id, &project_path, false);
            Json(JsonRpcResponse {
                jsonrpc: "2.0",
                id: req_id,
                result: Some(json!({
                    "content": [{"type": "text", "text": format!("REA Gateway transport error: {}", e)}],
                    "isError": true
                })),
                error: None,
            })
        }
    }
}

/// Total size in bytes of the files under `dir` (0 when it does not exist). Symlinks are not
/// followed and the recursion is bounded by `depth_remaining_count`.
fn compute_dir_size_bytes(dir: &Path, depth_remaining_count: usize) -> Result<u64, ReaError> {
    if depth_remaining_count == 0 {
        return Ok(0);
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(io_err(format!("Cannot list {}", dir.display()))(e)),
    };
    let mut total_bytes = 0u64;
    for entry in entries {
        let entry = entry.map_err(io_err(format!("Cannot list {}", dir.display())))?;
        let file_type = entry.file_type().map_err(io_err(format!("Cannot inspect {}", entry.path().display())))?;
        if file_type.is_file() {
            let meta = entry.metadata().map_err(io_err(format!("Cannot inspect {}", entry.path().display())))?;
            total_bytes = total_bytes.saturating_add(meta.len());
        } else if file_type.is_dir() {
            total_bytes = total_bytes.saturating_add(compute_dir_size_bytes(&entry.path(), depth_remaining_count - 1)?);
        }
    }
    Ok(total_bytes)
}

/// Identifies a file's binary or container format using magic header bytes and file extension.
fn classify_binary_magic(file_path: &Path) -> Result<String, ReaError> {
    let mut header = [0u8; 64];
    let bytes_read = fs::File::open(file_path)
        .and_then(|mut f| std::io::Read::read(&mut f, &mut header))
        .map_err(io_err(format!("Cannot read {}", file_path.display())))?;
    Ok(classify_header(&header, bytes_read, file_path))
}

fn classify_header(header: &[u8; 64], bytes_read: usize, file_path: &Path) -> String {

    if bytes_read >= 4 {
        if &header[0..4] == b"\x7fELF" {
            let class_str = match header.get(4) {
                Some(1) => "32-bit",
                Some(2) => "64-bit",
                _ => "ELF",
            };
            let arch_str = match (header.get(18), header.get(19)) {
                (Some(0x28), Some(0x00)) => "ARM32",
                (Some(0xb7), Some(0x00)) => "AArch64",
                (Some(0x3e), Some(0x00)) => "x86_64",
                (Some(0x03), Some(0x00)) => "x86",
                (Some(0xf3), Some(0x00)) => "RISC-V",
                _ => "Native",
            };
            return format!("ELF {} ({})", class_str, arch_str);
        }
        if &header[0..2] == b"MZ" {
            return "PE / Windows Executable".to_string();
        }
        if &header[0..4] == b"\xfe\xed\xfa\xce"
            || &header[0..4] == b"\xfe\xed\xfa\xcf"
            || &header[0..4] == b"\xce\xfa\xed\xfe"
            || &header[0..4] == b"\xcf\xfa\xed\xfe"
        {
            return "Mach-O Binary".to_string();
        }
        if &header[0..4] == b"\x00asm" {
            return "WebAssembly (WASM)".to_string();
        }
        if &header[0..4] == b"dex\n" {
            return "Android Dalvik Bytecode (DEX)".to_string();
        }
        if bytes_read >= 15 && &header[0..15] == b"SQLite format 3" {
            return "SQLite 3 Database".to_string();
        }
        if &header[0..4] == b"hsqs" || &header[0..4] == b"sqsh" {
            return "SquashFS Firmware Image".to_string();
        }
    }

    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "asar" => "Electron Archive (ASAR)".to_string(),
        "apk" => "Android Package (APK)".to_string(),
        "har" => "HTTP Archive Capture (HAR)".to_string(),
        "rm" => "reMarkable Notebook Binary (.rm)".to_string(),
        "bin" | "fw" | "img" => "Firmware / Raw Binary Image".to_string(),
        "so" => "Shared Library (.so)".to_string(),
        "dll" => "Dynamic Link Library (.dll)".to_string(),
        _ => "Binary / Unknown Artifact".to_string(),
    }
}

/// Builds the current `ReaEngineStatus` for a given project path.
pub async fn build_rea_engine_status(project_path: &str) -> Result<ReaEngineStatus, ReaError> {
    let p_path = Path::new(project_path);
    if !p_path.exists() {
        return Err(ReaError::ProjectNotFound {
            path: project_path.to_string(),
        });
    }

    let is_enabled = is_project_rea_enabled(p_path);
    let http_port = resolve_project_rea_port(project_path)?;
    let service_key = ProcessSupervisor::format_service_key(project_path, REA_SERVICE_ID);
    let sup = get_supervisor();
    let st = sup.get_status(&service_key);
    let is_running = matches!(
        st,
        ProcessStatus::Working | ProcessStatus::Starting | ProcessStatus::Idle
    );
    let pid = sup.get_service_pid(project_path, REA_SERVICE_ID);

    let mcp_port =
        crate::config::mcp_daemon_port();
    let gateway_url = build_project_rea_gateway_url(mcp_port, project_path);

    let (in_flight_requests_count, queued_requests_count, last_activity_epoch_ms) = {
        let orchestrator = get_rea_orchestrator();
        let map = orchestrator
            .project_metrics
            .read_safe("rea:build_status:metrics");
        if let Some(m) = map.get(project_path) {
            (
                m.in_flight_requests_count,
                m.queued_requests_count,
                m.last_activity_epoch_ms,
            )
        } else {
            (0, 0, 0)
        }
    };

    let idle_timeout_seconds = get_project_idle_timeout_seconds(project_path);
    let p_owned = p_path.to_path_buf();
    let (data_dir_size_bytes, targets_count) = tokio::task::spawn_blocking(move || -> Result<(u64, usize), ReaError> {
        let size_bytes = compute_dir_size_bytes(&p_owned.join(".one-ship").join("rea"), 6)?;
        let (targets_dir, _) = resolve_rea_targets_dir(&p_owned)?;
        let count = match fs::read_dir(&targets_dir) {
            Ok(entries) => {
                let mut files_count = 0usize;
                for entry in entries {
                    let entry = entry.map_err(io_err(format!("Cannot list {}", targets_dir.display())))?;
                    if entry.file_type().map_err(io_err(format!("Cannot inspect {}", entry.path().display())))?.is_file() {
                        files_count += 1;
                    }
                }
                files_count
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => 0,
            Err(e) => return Err(io_err(format!("Cannot list {}", targets_dir.display()))(e)),
        };
        Ok((size_bytes, count))
    })
    .await
    .map_err(|e| ReaError::Task(e.to_string()))??;

    Ok(ReaEngineStatus {
        project_path: project_path.to_string(),
        is_enabled,
        is_running,
        pid,
        http_port,
        gateway_url,
        service_key,
        in_flight_requests_count,
        queued_requests_count,
        last_activity_epoch_ms,
        idle_timeout_seconds,
        data_dir_size_bytes,
        targets_count,
    })
}

async fn probe_command_version(bin: &str, args: &[&str]) -> (bool, Option<String>) {
    let Ok(output) = tokio::process::Command::new(bin).args(args).output().await else {
        return (false, None);
    };
    if !output.status.success() && output.stdout.is_empty() && output.stderr.is_empty() {
        return (false, None);
    }
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let first_line = combined
        .lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .map(|l| l.chars().take(80).collect::<String>());
    (true, first_line)
}

// ---------------------------------------------------------------------------
// TAURI IPC COMMANDS
// ---------------------------------------------------------------------------

/// Checks the availability and versions of the reverse-engineering toolchain on the Omarchy host.
pub async fn check_rea_toolchain(
    project_path: Option<String>,
) -> Result<Vec<ReaToolStatus>, ReaError> {
    let ghidra_dir = get_project_ghidra_install_dir(project_path.as_deref())?;

    let mut tools = Vec::with_capacity(7);

    // 1. REA Engine: the installed `rea` binary (the engine has no `npx` substitute)
    let (has_rea, rea_ver) = probe_command_version("rea", &["--version"]).await;
    tools.push(ReaToolStatus {
        id: "rea-agents".to_string(),
        name: "REA Engine (rea-agents)".to_string(),
        binary: "rea".to_string(),
        category: "core".to_string(),
        is_installed: has_rea,
        version: rea_ver,
        install_hint: "npm install -g rea-agents@latest".to_string(),
        auto_installable: false,
    });

    // 2. Ghidra Headless
    let headless_path = Path::new(&ghidra_dir).join("support").join("analyzeHeadless");
    let (has_ghidra_path, _) = probe_command_version("which", &["ghidraRun"]).await;
    let ghidra_installed = headless_path.exists() || has_ghidra_path;
    let (ghidra_hint, ghidra_auto) = crate::kopeng::diagnostic::resolve_tool_install_hint(
        "ghidra",
        "Download Ghidra release archive and configure plugins.rea.ghidra_dir",
    );
    tools.push(ReaToolStatus {
        id: "ghidra".to_string(),
        name: "Ghidra Headless Decompiler".to_string(),
        binary: headless_path.to_string_lossy().to_string(),
        category: "native".to_string(),
        is_installed: ghidra_installed,
        version: if ghidra_installed {
            Some(format!("Installed ({})", ghidra_dir))
        } else {
            None
        },
        install_hint: ghidra_hint,
        auto_installable: ghidra_auto,
    });

    // 3. Radare2
    let (has_r2, r2_ver) = probe_command_version("r2", &["-v"]).await;
    let (r2_hint, r2_auto) = crate::kopeng::diagnostic::resolve_tool_install_hint(
        "radare2",
        "Install radare2 via your distribution package manager",
    );
    tools.push(ReaToolStatus {
        id: "radare2".to_string(),
        name: "Radare2 Disassembler".to_string(),
        binary: "r2".to_string(),
        category: "native".to_string(),
        is_installed: has_r2,
        version: r2_ver,
        install_hint: r2_hint,
        auto_installable: r2_auto,
    });

    // 4. Binutils (objdump / strings)
    let (has_objdump, objdump_ver) = probe_command_version("objdump", &["--version"]).await;
    let (binutils_hint, binutils_auto) = crate::kopeng::diagnostic::resolve_tool_install_hint(
        "binutils",
        "Install binutils via your distribution package manager",
    );
    tools.push(ReaToolStatus {
        id: "binutils".to_string(),
        name: "GNU Binutils (objdump / nm / strings)".to_string(),
        binary: "objdump".to_string(),
        category: "native".to_string(),
        is_installed: has_objdump,
        version: objdump_ver,
        install_hint: binutils_hint,
        auto_installable: binutils_auto,
    });

    // 5. Binwalk (Firmware extraction)
    let (has_binwalk, binwalk_ver) = probe_command_version("binwalk", &["--version"]).await;
    let (binwalk_hint, binwalk_auto) = crate::kopeng::diagnostic::resolve_tool_install_hint(
        "binwalk",
        "Install binwalk via your distribution package manager",
    );
    tools.push(ReaToolStatus {
        id: "binwalk".to_string(),
        name: "Binwalk Firmware Extractor".to_string(),
        binary: "binwalk".to_string(),
        category: "firmware".to_string(),
        is_installed: has_binwalk,
        version: binwalk_ver,
        install_hint: binwalk_hint,
        auto_installable: binwalk_auto,
    });

    // 6. JADX (Android / DEX decompiler)
    let (has_jadx, jadx_ver) = probe_command_version("jadx", &["--version"]).await;
    let (jadx_hint, jadx_auto) = crate::kopeng::diagnostic::resolve_tool_install_hint(
        "jadx",
        "Install jadx from official GitHub releases or Homebrew/Linuxbrew",
    );
    tools.push(ReaToolStatus {
        id: "jadx".to_string(),
        name: "JADX Dex-to-Java Decompiler".to_string(),
        binary: "jadx".to_string(),
        category: "mobile".to_string(),
        is_installed: has_jadx,
        version: jadx_ver,
        install_hint: jadx_hint,
        auto_installable: jadx_auto,
    });

    // 7. SquashFS Tools (reMarkable / Embedded Linux rootfs)
    let (has_unsquashfs, sq_ver) = probe_command_version("unsquashfs", &["-version"]).await;
    let (sq_hint, sq_auto) = crate::kopeng::diagnostic::resolve_tool_install_hint(
        "squashfs-tools",
        "Install squashfs-tools via your distribution package manager",
    );
    tools.push(ReaToolStatus {
        id: "squashfs-tools".to_string(),
        name: "SquashFS Rootfs Unpacker".to_string(),
        binary: "unsquashfs".to_string(),
        category: "firmware".to_string(),
        is_installed: has_unsquashfs,
        version: sq_ver,
        install_hint: sq_hint,
        auto_installable: sq_auto,
    });

    Ok(tools)
}

/// Returns the current operational, queue, and storage status of a project's REA engine.
pub async fn get_rea_engine_status(project_path: String) -> Result<ReaEngineStatus, ReaError> {
    build_rea_engine_status(&require_registered_project(&project_path)?).await
}

/// Returns a live snapshot of the global multi-project REA FIFO job queue. Daemon command.
pub async fn get_rea_queue_snapshot() -> Result<Vec<ReaQueuedJob>, ReaError> {
    Ok(get_rea_orchestrator().get_queue_snapshot())
}

/// Explicitly starts a project's supervised REA HTTP engine.
pub async fn start_rea_engine(project_path: String) -> Result<ReaEngineStatus, ReaError> {
    let project_path = require_registered_project(&project_path)?;
    ensure_rea_engine_running(&project_path).await?;
    build_rea_engine_status(&project_path).await
}

/// Explicitly stops a project's supervised REA HTTP engine, freeing all associated RAM.
pub async fn stop_rea_engine(project_path: String) -> Result<ReaEngineStatus, ReaError> {
    let project_path = require_registered_project(&project_path)?;
    stop_rea_engine_internal(&project_path).await?;
    build_rea_engine_status(&project_path).await
}

/// Enables or disables the REA Reverse Engineering plugin in `<project_path>/rula.toml`,
/// synchronizes `.mcp.json` / `opencode.json`, and links or removes the `oneship-rea` skill.
pub async fn toggle_project_rea_plugin(
    project_path: String,
    enabled: bool,
) -> Result<ReaEngineStatus, ReaError> {
    let project_path = require_registered_project(&project_path)?;
    let p_path = Path::new(&project_path);

    // A missing or invalid rula.toml is an error: it is never replaced by the plugin table alone.
    let rula_path = p_path.join("rula.toml");
    let mut rula_val = crate::config::read_project_rula(p_path)?.ok_or_else(|| ReaError::InvalidSetting {
        key: "enabled",
        details: format!("{} does not exist; initialize the project first", rula_path.display()),
    })?;

    if let Some(root_table) = rula_val.as_table_mut() {
        let plugins_entry = root_table
            .entry("plugins".to_string())
            .or_insert_with(|| toml::Value::Table(Default::default()));
        if let Some(plugins_table) = plugins_entry.as_table_mut() {
            let rea_entry = plugins_table
                .entry("rea".to_string())
                .or_insert_with(|| toml::Value::Table(Default::default()));
            if let Some(rea_table) = rea_entry.as_table_mut() {
                rea_table.insert("enabled".to_string(), toml::Value::Boolean(enabled));
            }
        }
    }

    let serialized_toml = toml::to_string_pretty(&rula_val).map_err(|e| ReaError::InvalidSetting {
        key: "enabled",
        details: format!("cannot serialize rula.toml: {}", e),
    })?;
    tokio::fs::write(&rula_path, serialized_toml)
        .await
        .map_err(io_err(format!("Cannot write {}", rula_path.display())))?;

    if enabled {
        ensure_project_rea_directories(p_path)?;
        crate::kopeng::skill::inject_specialized_skill(project_path.clone(), "oneship-rea".to_string())?;
    } else {
        stop_rea_engine_internal(&project_path).await?;
        crate::kopeng::skill::cleanup_ephemeral_skill(project_path.clone(), "oneship-rea".to_string())?;
    }

    crate::kopeng::skill::inject_project_mcp_plugin(p_path)?;

    build_rea_engine_status(&project_path).await
}

/// Scans the configured `[plugins.rea].targets_dir` (default `targets/`) and returns metadata, SHA-256 digests, and magic format classifications.
pub async fn list_rea_targets(project_path: String) -> Result<Vec<ReaTargetArtifact>, ReaError> {
    let p_path = PathBuf::from(require_registered_project(&project_path)?);

    tokio::task::spawn_blocking(move || {
        let (targets_dir, targets_rel) = resolve_rea_targets_dir(&p_path)?;
        let entries = match fs::read_dir(&targets_dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(io_err(format!("Cannot list {}", targets_dir.display()))(e)),
        };

        let mut artifacts = Vec::new();
        for entry in entries {
            let entry = entry.map_err(io_err(format!("Cannot list {}", targets_dir.display())))?;
            let path = entry.path();
            let meta = entry.metadata().map_err(io_err(format!("Cannot inspect {}", path.display())))?;
            if !meta.is_file() {
                continue;
            }

            let name = entry.file_name().to_string_lossy().to_string();
            let rel_path = if targets_rel == "." {
                name.clone()
            } else {
                format!("{}/{}", targets_rel, name)
            };
            let size_bytes = meta.len();
            let modified_epoch_seconds = meta
                .modified()
                .map_err(io_err(format!("Cannot inspect {}", path.display())))?
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or_default();

            let detected_format = classify_binary_magic(&path)?;

            // SHA-256 only up to MAX_TARGET_HASH_BYTES; larger files are marked as not hashed.
            let sha256_hex = if size_bytes <= MAX_TARGET_HASH_BYTES {
                let mut file = fs::File::open(&path).map_err(io_err(format!("Cannot open {}", path.display())))?;
                let mut hasher = Sha256::new();
                std::io::copy(&mut file, &mut hasher).map_err(io_err(format!("Cannot hash {}", path.display())))?;
                format!("{:x}", hasher.finalize())
            } else {
                "large-file-deferred".to_string()
            };

            artifacts.push(ReaTargetArtifact {
                name,
                rel_path,
                size_bytes,
                sha256_hex,
                detected_format,
                modified_epoch_seconds,
            });
        }

        artifacts.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(artifacts)
    })
    .await
    .map_err(|e| ReaError::Task(e.to_string()))?
}

/// A view JSON file written by the REA agents; `Ok(None)` when it does not exist.
fn read_view(path: &Path) -> Result<Option<Value>, ReaError> {
    let raw = match fs::read_to_string(path) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_err(format!("Cannot read {}", path.display()))(e)),
    };
    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|e| ReaError::InvalidView { path: path.display().to_string(), details: e.to_string() })
}

/// One parsed view file, validated against its single shape (Spec 26 §5, `oneship-rea` skill).
struct ViewFile<'a> {
    path: &'a Path,
    value: &'a Value,
}

impl ViewFile<'_> {
    fn invalid(&self, details: String) -> ReaError {
        ReaError::InvalidView { path: self.path.display().to_string(), details }
    }

    /// The array under `key` of the root object; empty when the key is absent.
    fn list(&self, key: &str) -> Result<&[Value], ReaError> {
        let root = self.value.as_object().ok_or_else(|| self.invalid("the root must be a JSON object".to_string()))?;
        match root.get(key) {
            None => Ok(&[]),
            Some(Value::Array(items)) => Ok(items),
            Some(_) => Err(self.invalid(format!("'{}' must be an array", key))),
        }
    }

    /// A string field; `None` when absent, an error when present with another type.
    fn optional(&self, item: &Value, (idx, list): (usize, &str), key: &str) -> Result<Option<String>, ReaError> {
        match item.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(self.invalid(format!("{}[{}].{} must be a string", list, idx, key))),
        }
    }

    /// A non-empty string field that identifies the item; absent or empty is an error.
    fn required(&self, item: &Value, at: (usize, &str), key: &str) -> Result<String, ReaError> {
        self.optional(item, at, key)?
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| self.invalid(format!("{}[{}] has no '{}'", at.1, at.0, key)))
    }

    /// An optional array of strings.
    fn strings(&self, item: &Value, (idx, list): (usize, &str), key: &str) -> Result<Vec<String>, ReaError> {
        match item.get(key) {
            None | Some(Value::Null) => Ok(Vec::new()),
            Some(Value::Array(values)) => values
                .iter()
                .map(|v| v.as_str().map(String::from))
                .collect::<Option<Vec<String>>>()
                .ok_or_else(|| self.invalid(format!("{}[{}].{} must be an array of strings", list, idx, key))),
            Some(_) => Err(self.invalid(format!("{}[{}].{} must be an array of strings", list, idx, key))),
        }
    }
}

/// Loads structured reverse-engineering visual views from `<project_path>/.one-ship/rea/views/`.
pub async fn get_rea_visual_views(project_path: String) -> Result<ReaVisualViewsBundle, ReaError> {
    let p_path = PathBuf::from(require_registered_project(&project_path)?);

    tokio::task::spawn_blocking(move || {
        let views_dir = p_path.join(".one-ship").join("rea").join("views");
        let mut bundle = ReaVisualViewsBundle::default();
        if !views_dir.exists() {
            return Ok(bundle);
        }

        // 1. architecture_map.json: { "subsystems": [...], "call_edges": [...] }
        let arch_path = views_dir.join("architecture_map.json");
        if let Some(val) = read_view(&arch_path)? {
            let view = ViewFile { path: &arch_path, value: &val };
            for (idx, item) in view.list("subsystems")?.iter().enumerate() {
                let at = (idx, "subsystems");
                bundle.subsystems.push(ReaSubsystemNode {
                    id: view.optional(item, at, "id")?.unwrap_or_else(|| format!("sub-{}", idx)),
                    name: view.required(item, at, "name")?,
                    description: view.optional(item, at, "description")?.unwrap_or_default(),
                    symbols: view.strings(item, at, "symbols")?,
                });
            }
            for (idx, item) in view.list("call_edges")?.iter().enumerate() {
                let at = (idx, "call_edges");
                bundle.call_edges.push(ReaCallFlowEdge {
                    id: view.optional(item, at, "id")?.unwrap_or_else(|| format!("edge-{}", idx)),
                    from_symbol: view.required(item, at, "from_symbol")?,
                    to_symbol: view.required(item, at, "to_symbol")?,
                    offset_hex: view.optional(item, at, "offset_hex")?,
                    summary: view.optional(item, at, "summary")?.unwrap_or_default(),
                });
            }
        }

        // 2. decompiled_highlights.json: { "functions": [...] }
        let decomp_path = views_dir.join("decompiled_highlights.json");
        if let Some(val) = read_view(&decomp_path)? {
            let view = ViewFile { path: &decomp_path, value: &val };
            for (idx, item) in view.list("functions")?.iter().enumerate() {
                let at = (idx, "functions");
                bundle.decompiled_functions.push(ReaDecompiledHighlight {
                    id: view.optional(item, at, "id")?.unwrap_or_else(|| format!("fn-{}", idx)),
                    symbol: view.required(item, at, "symbol")?,
                    offset_hex: view.optional(item, at, "offset_hex")?,
                    subsystem: view.optional(item, at, "subsystem")?,
                    signature: view.optional(item, at, "signature")?,
                    pseudocode: view.optional(item, at, "pseudocode")?.unwrap_or_default(),
                    explanation: view.optional(item, at, "explanation")?.unwrap_or_default(),
                });
            }
        }

        // 3. protocols_and_endpoints.json: { "items": [...] }
        let proto_path = views_dir.join("protocols_and_endpoints.json");
        if let Some(val) = read_view(&proto_path)? {
            let view = ViewFile { path: &proto_path, value: &val };
            for (idx, item) in view.list("items")?.iter().enumerate() {
                let at = (idx, "items");
                bundle.protocol_items.push(ReaProtocolItem {
                    id: view.optional(item, at, "id")?.unwrap_or_else(|| format!("proto-{}", idx)),
                    category: view.optional(item, at, "category")?,
                    endpoint_or_key: view.required(item, at, "endpoint_or_key")?,
                    protocol: view.optional(item, at, "protocol")?.unwrap_or_default(),
                    offset_hex: view.optional(item, at, "offset_hex")?,
                    description: view.optional(item, at, "description")?.unwrap_or_default(),
                });
            }
        }

        Ok(bundle)
    })
    .await
    .map_err(|e| ReaError::Task(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_hex_decoding_rejects_malformed_input_without_panicking() {
        assert_eq!(decode_hex_string("2f746d70").as_deref(), Some("/tmp"));
        assert_eq!(decode_hex_string("2f7"), None);
        assert_eq!(decode_hex_string("zz"), None);
        assert_eq!(decode_hex_string("ééé"), None);
    }

    #[test]
    fn views_are_read_strictly() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("architecture_map.json");
        assert!(read_view(&path).unwrap().is_none());
        fs::write(&path, "{\"subsystems\": [").unwrap();
        assert!(matches!(read_view(&path), Err(ReaError::InvalidView { .. })));
        fs::write(&path, "{\"subsystems\": [{\"name\": \"net\"}]}").unwrap();
        let val = read_view(&path).unwrap().unwrap();
        let view = ViewFile { path: &path, value: &val };
        assert_eq!(view.list("subsystems").unwrap().len(), 1);
        assert!(view.list("call_edges").unwrap().is_empty());
    }

    #[test]
    fn view_items_require_their_identifying_fields() {
        let path = Path::new("decompiled_highlights.json");
        let val: Value = serde_json::from_str(
            r#"{"functions": [{"symbol": "parse_header"}, {"offset_hex": "0x10"}], "items": {}}"#,
        )
        .unwrap();
        let view = ViewFile { path, value: &val };
        let items = view.list("functions").unwrap();
        assert_eq!(view.required(&items[0], (0, "functions"), "symbol").unwrap(), "parse_header");
        assert_eq!(view.optional(&items[0], (0, "functions"), "offset_hex").unwrap(), None);
        let err = view.required(&items[1], (1, "functions"), "symbol").unwrap_err().to_string();
        assert!(err.contains("functions[1] has no 'symbol'"), "{}", err);
        assert!(view.list("items").is_err());

        let list_root: Value = serde_json::from_str("[]").unwrap();
        assert!(ViewFile { path, value: &list_root }.list("functions").is_err());
    }

    #[test]
    fn dir_size_does_not_follow_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a.bin"), [0u8; 10]).unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("sub/b.bin"), [0u8; 5]).unwrap();
        std::os::unix::fs::symlink(tmp.path(), tmp.path().join("sub/loop")).unwrap();
        assert_eq!(compute_dir_size_bytes(tmp.path(), 6).unwrap(), 15);
        assert_eq!(compute_dir_size_bytes(&tmp.path().join("missing"), 6).unwrap(), 0);
    }
}

