//! Desigma visual design plugin integration and per-project MCP stdio gateway.
//!
//! Provides:
//! - Project-scoped `[plugins.desigma]` configuration resolution (`rula.toml` + `config.rs`).
//! - Per-project `desigma-mcp --workspace <project>/<design_dir>` stdio process pool,
//!   proxied behind ONE-SHIP's `/mcp/desigma` HTTP gateway with Rule 11 caller attestation
//!   so `agy`, Claude Code, and OpenCode share identical `desigma_*` tool names.
//! - Native `desigma --bridge` editor launch for live human/agent co-editing.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};

use axum::extract::{ConnectInfo, Query};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use ts_rs::TS;

use crate::mcp::{JsonRpcRequest, JsonRpcResponse};

/// Strongly-typed error hierarchy for Desigma integration and MCP gateway operations.
#[derive(Debug, Error)]
pub enum DesigmaError {
    #[error("Project not registered: {path}")]
    ProjectNotFound { path: String },

    #[error("Missing or invalid project context: {details}")]
    ProjectContextMissing { details: String },

    #[error("Desigma plugin is disabled for project '{project_path}'")]
    PluginDisabled { project_path: String },

    #[error("Invalid Desigma setting '{key}': {details}")]
    InvalidSetting { key: &'static str, details: String },

    #[error("Desigma binary not found at '{path}'")]
    BinaryNotFound { path: String },

    #[error("Desigma process lifecycle error: {details}")]
    ProcessLifecycle { details: String },

    #[error("Desigma MCP transport error: {details}")]
    McpTransport { details: String },

    #[error("I/O error ({context}): {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    Registry(#[from] crate::registry::RegistryError),

    #[error(transparent)]
    Config(#[from] crate::config::ConfigError),

    #[error(transparent)]
    Skill(#[from] crate::kopeng::skill::SkillError),

    #[error("Background task error: {0}")]
    Task(String),
}

impl crate::api::ApiError for DesigmaError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::ProjectNotFound { .. } => StatusCode::NOT_FOUND,
            Self::ProjectContextMissing { .. }
            | Self::PluginDisabled { .. }
            | Self::InvalidSetting { .. } => StatusCode::BAD_REQUEST,
            Self::BinaryNotFound { .. } => StatusCode::FAILED_DEPENDENCY,
            Self::McpTransport { .. } => StatusCode::BAD_GATEWAY,
            Self::Io { .. }
            | Self::ProcessLifecycle { .. }
            | Self::Task(_)
            | Self::Skill(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Registry(e) => e.status(),
            Self::Config(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::ProjectNotFound { .. } => "DesigmaError::ProjectNotFound",
            Self::ProjectContextMissing { .. } => "DesigmaError::ProjectContextMissing",
            Self::PluginDisabled { .. } => "DesigmaError::PluginDisabled",
            Self::InvalidSetting { .. } => "DesigmaError::InvalidSetting",
            Self::BinaryNotFound { .. } => "DesigmaError::BinaryNotFound",
            Self::ProcessLifecycle { .. } => "DesigmaError::ProcessLifecycle",
            Self::McpTransport { .. } => "DesigmaError::McpTransport",
            Self::Io { .. } => "DesigmaError::Io",
            Self::Registry(_) => "DesigmaError::Registry",
            Self::Config(_) => "DesigmaError::Config",
            Self::Skill(_) => "DesigmaError::Skill",
            Self::Task(_) => "DesigmaError::Task",
        }
    }
}

fn io_err(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> DesigmaError {
    let ctx = context.into();
    move |source| DesigmaError::Io { context: ctx, source }
}

/// Live operational status of the Desigma plugin for a project.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct DesigmaStatus {
    /// Absolute path of the owning project.
    pub project_path: String,
    /// Whether `[plugins.desigma] enabled = true` is set in `rula.toml`.
    pub is_enabled: bool,
    /// Resolved absolute path to the project's design workspace directory.
    pub design_dir_path: String,
    /// ONE-SHIP HTTP MCP gateway URL injected into `.mcp.json` and `opencode.json`.
    pub gateway_url: String,
    /// Number of `.desigma` or `.fig` files currently in the design directory.
    pub design_files_count: usize,
    /// Sorted list of `.desigma` / `.fig` filenames inside the design directory.
    pub design_files: Vec<String>,
    /// Whether a Desigma desktop editor instance is currently running.
    pub is_editor_running: bool,
}

/// Arguments of the daemon command `render_desigma_preview`.
#[derive(Deserialize, TS)]
#[ts(export)]
pub struct RenderDesigmaPreviewArgs {
    /// Absolute path of the project.
    pub project_path: String,
    /// Optional design filename inside `design_dir` (defaults to the first `.desigma`/`.fig` file).
    pub file_name: Option<String>,
    /// Optional target page or layer node ID to render (defaults to first page).
    #[ts(type = "number | null")]
    pub node_id: Option<u64>,
}

/// Page summary extracted from a `.desigma` / `.fig` document.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct DesigmaPageInfo {
    #[ts(type = "number")]
    pub id: u64,
    pub name: String,
}

/// Flattened layer tree entry extracted from the active page.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct DesigmaLayerInfo {
    #[ts(type = "number")]
    pub id: u64,
    pub name: String,
    pub kind: String,
    pub depth: usize,
    pub has_children: bool,
}

/// Interactive vector SVG preview and document structure of a `.desigma` / `.fig` file.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct DesigmaPreviewResult {
    /// Name of the rendered design file.
    pub file_name: String,
    /// Document title from the design file.
    pub document_name: String,
    /// Active page node ID.
    #[ts(type = "number")]
    pub active_page_id: u64,
    /// Rendered target node ID (page or selected layer/frame).
    #[ts(type = "number")]
    pub selected_node_id: u64,
    /// Available pages in the design file.
    pub pages: Vec<DesigmaPageInfo>,
    /// Flattened layer hierarchy of the active page.
    pub layers: Vec<DesigmaLayerInfo>,
    /// Self-contained vector SVG markup for interactive pan/zoom rendering.
    pub svg: String,
    /// Unix epoch timestamp in milliseconds when the render completed.
    #[ts(type = "number")]
    pub rendered_at_ms: u64,
}

/// Arguments of the daemon command `get_project_desigma_status`.
#[derive(Deserialize, TS)]
#[ts(export)]
pub struct GetProjectDesigmaStatusArgs {
    /// Absolute path of the project.
    pub project_path: String,
}

/// Arguments of the daemon command `toggle_project_desigma_plugin`.
#[derive(Deserialize, TS)]
#[ts(export)]
pub struct ToggleProjectDesigmaPluginArgs {
    /// Absolute path of the project.
    pub project_path: String,
    /// Desired enabled state.
    pub enabled: bool,
}

/// Arguments of the daemon command `launch_project_desigma`.
#[derive(Deserialize, TS)]
#[ts(export)]
pub struct LaunchProjectDesigmaArgs {
    /// Absolute path of the project.
    pub project_path: String,
}

/// Query parameters accepted by `/mcp/desigma`.
#[derive(Deserialize)]
pub struct DesigmaGatewayQuery {
    /// Hex-encoded UTF-8 project path for unmanaged HTTP MCP clients (e.g. Claude Code / OpenCode).
    pub project_hex: Option<String>,
}

struct DesigmaMcpProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

struct DesigmaRuntimePool {
    mcp_processes: Mutex<HashMap<String, Arc<Mutex<DesigmaMcpProcess>>>>,
    editor_processes: Mutex<HashMap<String, Child>>,
    file_watchers: Mutex<HashMap<String, tokio_util::sync::CancellationToken>>,
}

static DESIGMA_POOL: OnceLock<DesigmaRuntimePool> = OnceLock::new();

fn runtime_pool() -> &'static DesigmaRuntimePool {
    DESIGMA_POOL.get_or_init(|| DesigmaRuntimePool {
        mcp_processes: Mutex::new(HashMap::new()),
        editor_processes: Mutex::new(HashMap::new()),
        file_watchers: Mutex::new(HashMap::new()),
    })
}

async fn reap_finished_editors() {
    let mut editors = runtime_pool().editor_processes.lock().await;
    let mut finished_keys = Vec::new();
    for (key, child) in editors.iter_mut() {
        if !matches!(child.try_wait(), Ok(None)) {
            finished_keys.push(key.clone());
        }
    }
    for key in finished_keys {
        editors.remove(&key);
    }
}

async fn stop_design_dir_watcher(project_path: &str) {
    if let Some(token) = runtime_pool().file_watchers.lock().await.remove(project_path) {
        token.cancel();
    }
}

async fn ensure_design_dir_watcher(project_path: &str, design_dir: &Path) {
    if !design_dir.exists() {
        return;
    }
    let mut watchers = runtime_pool().file_watchers.lock().await;
    if watchers.contains_key(project_path) {
        return;
    }
    let token = tokio_util::sync::CancellationToken::new();
    watchers.insert(project_path.to_string(), token.clone());
    drop(watchers);

    let proj = project_path.to_string();
    let dir = design_dir.to_path_buf();

    tokio::spawn(async move {
        use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
        let (tx, mut rx) = tokio::sync::mpsc::channel::<PathBuf>(64);

        let watcher_res = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                if let Ok(ev) = res {
                    for p in ev.paths {
                        let is_design = p
                            .extension()
                            .and_then(|e| e.to_str())
                            .is_some_and(|e| e.eq_ignore_ascii_case("desigma") || e.eq_ignore_ascii_case("fig"));
                        if is_design {
                            let _ = tx.try_send(p);
                        }
                    }
                }
            },
            Config::default(),
        );

        let mut watcher = match watcher_res {
            Ok(w) => w,
            Err(e) => {
                crate::logger::log("WARN", "desigma", &format!("Cannot start design watcher for {}: {}", proj, e));
                runtime_pool().file_watchers.lock().await.remove(&proj);
                return;
            }
        };

        if let Err(e) = watcher.watch(&dir, RecursiveMode::NonRecursive) {
            crate::logger::log("WARN", "desigma", &format!("Cannot watch {}: {}", dir.display(), e));
            runtime_pool().file_watchers.lock().await.remove(&proj);
            return;
        }

        const WATCH_DEBOUNCE_MS: u64 = 250;
        let mut last_modified_by_file: HashMap<String, u128> = HashMap::new();

        loop {
            tokio::select! {
                _ = token.cancelled() => break,
                maybe_path = rx.recv() => {
                    let Some(first_path) = maybe_path else { break; };
                    let mut changed_paths = HashSet::new();
                    changed_paths.insert(first_path);

                    // Coalesce burst writes while desigma atomically saves.
                    tokio::time::sleep(std::time::Duration::from_millis(WATCH_DEBOUNCE_MS)).await;
                    while let Ok(extra) = rx.try_recv() {
                        changed_paths.insert(extra);
                    }

                    for p in changed_paths {
                        let Some(file_name) = p.file_name().and_then(|n| n.to_str()).map(|s| s.to_string()) else {
                            continue;
                        };
                        let modified_ms = read_file_modified_ms(&p).await;
                        if modified_ms == 0 {
                            continue;
                        }
                        if last_modified_by_file.get(&file_name).copied() == Some(modified_ms) {
                            continue;
                        }
                        last_modified_by_file.insert(file_name.clone(), modified_ms);

                        if let Ok(rel) = p.strip_prefix(Path::new(&proj)) {
                            let rel_str = rel.to_string_lossy().to_string();
                            if let Err(e) = crate::kopeng::snapshots::try_create_auto_domain_snapshot(
                                PathBuf::from(&proj),
                                "desigma",
                                file_name.clone(),
                                "desigma".to_string(),
                                format!("design-edit: {}", file_name),
                                vec![rel_str],
                            )
                            .await
                            {
                                crate::logger::log("WARN", "desigma", &format!("Auto-snapshot failed: {}", e));
                            }
                        }

                        let payload = crate::events::DesigmaFileChangedPayload {
                            project_path: proj.clone(),
                            file_name,
                            modified_ms: modified_ms as u64,
                        };
                        crate::supervisor::get_supervisor().emit_event(
                            crate::events::DaemonEvent::desigma_file_changed(&payload),
                        );
                    }
                }
            }
        }
    });
}

fn require_registered_project(project_path: &str) -> Result<String, DesigmaError> {
    let project = crate::registry::find_project_by_path(project_path)?;
    Ok(project.path)
}

/// Checks whether `[plugins.desigma] enabled = true` is configured in `<project_path>/rula.toml`.
pub fn is_project_desigma_enabled(project_path: &Path) -> bool {
    let Some(rula) = crate::config::load_project_rula(project_path) else {
        return false;
    };
    rula.get("plugins")
        .and_then(|p| p.get("desigma"))
        .and_then(|d| d.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn encode_hex_string(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for b in input.as_bytes() {
        use std::fmt::Write;
        let _ = write!(&mut out, "{:02x}", b);
    }
    out
}

fn decode_hex_string(hex: &str) -> Option<String> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for i in (0..hex.len()).step_by(2) {
        let byte = u8::from_str_radix(&hex[i..i + 2], 16).ok()?;
        bytes.push(byte);
    }
    String::from_utf8(bytes).ok()
}

/// Builds the ONE-SHIP `/mcp/desigma` gateway URL scoped to `project_path`.
pub fn build_project_desigma_gateway_url(mcp_port: u16, project_path: &str) -> String {
    format!(
        "http://localhost:{}/mcp/desigma?project_hex={}",
        mcp_port,
        encode_hex_string(project_path)
    )
}

fn resolve_design_dir(project_path: &str) -> Result<PathBuf, DesigmaError> {
    let rel = crate::config::resolve_effective_string(Some(project_path), None, "desigma", "design_dir");
    let trimmed = rel.trim();
    if trimmed.is_empty() || trimmed.contains("..") || Path::new(trimmed).is_absolute() {
        return Err(DesigmaError::InvalidSetting {
            key: "design_dir",
            details: format!("'{}' must be a safe relative directory name", rel),
        });
    }
    Ok(Path::new(project_path).join(trimmed))
}

fn resolve_mcp_binary(project_path: &str) -> Result<PathBuf, DesigmaError> {
    let path_str = crate::config::resolve_effective_string(Some(project_path), None, "desigma", "mcp_binary_path");
    crate::desktop::resolve_executable(path_str.trim()).ok_or_else(|| DesigmaError::BinaryNotFound {
        path: path_str.trim().to_string(),
    })
}

fn resolve_app_binary(project_path: &str) -> Result<PathBuf, DesigmaError> {
    let path_str = crate::config::resolve_effective_string(Some(project_path), None, "desigma", "app_binary_path");
    crate::desktop::resolve_executable(path_str.trim()).ok_or_else(|| DesigmaError::BinaryNotFound {
        path: path_str.trim().to_string(),
    })
}

fn resolve_enable_bridge(project_path: &str) -> bool {
    crate::config::resolve_effective_bool(Some(project_path), None, "desigma", "enable_bridge")
}

fn resolve_mcp_timeout_ms(project_path: &str) -> u64 {
    crate::config::resolve_effective_u64(Some(project_path), None, "desigma", "mcp_request_timeout_ms")
        .clamp(1000, 120_000)
}

async fn list_design_files(design_dir: &Path) -> Vec<String> {
    let dir = design_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "desigma" | "fig"))
            })
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        names.sort();
        names
    })
    .await
    .unwrap_or_default()
}

async fn is_editor_process_alive(project_path: &str) -> bool {
    let mut editors = runtime_pool().editor_processes.lock().await;
    if let Some(child) = editors.get_mut(project_path) {
        match child.try_wait() {
            Ok(None) => return true,
            Ok(Some(_)) | Err(_) => {
                editors.remove(project_path);
            }
        }
    }
    drop(editors);

    crate::desktop::find_hyprland_window_for_project(
        &["desigma", "oneship-designer"],
        project_path,
    )
    .await
    .is_some()
}

/// Returns the current Desigma plugin operational status for a registered project.
pub async fn get_project_desigma_status(project_path: String) -> Result<DesigmaStatus, DesigmaError> {
    let project_path = require_registered_project(&project_path)?;
    let p_path = Path::new(&project_path);
    let is_enabled = is_project_desigma_enabled(p_path);
    let design_dir = resolve_design_dir(&project_path)?;
    if is_enabled {
        ensure_design_dir_watcher(&project_path, &design_dir).await;
    }
    let design_files = list_design_files(&design_dir).await;
    let design_files_count = design_files.len();
    let is_editor_running = is_editor_process_alive(&project_path).await;
    let mcp_port = crate::config::mcp_daemon_port();

    Ok(DesigmaStatus {
        gateway_url: build_project_desigma_gateway_url(mcp_port, &project_path),
        project_path,
        is_enabled,
        design_dir_path: design_dir.display().to_string(),
        design_files_count,
        design_files,
        is_editor_running,
    })
}

/// Enables or disables `[plugins.desigma]` in `<project_path>/rula.toml`,
/// manages the `oneship-desigma` skill, and synchronizes `.mcp.json` / `opencode.json`.
pub async fn toggle_project_desigma_plugin(
    project_path: String,
    enabled: bool,
) -> Result<DesigmaStatus, DesigmaError> {
    let project_path = require_registered_project(&project_path)?;
    let p_path = Path::new(&project_path);

    let rula_path = p_path.join("rula.toml");
    let mut rula_val = crate::config::read_project_rula(p_path)?.ok_or_else(|| DesigmaError::InvalidSetting {
        key: "enabled",
        details: format!("{} does not exist; initialize the project first", rula_path.display()),
    })?;

    if let Some(root_table) = rula_val.as_table_mut() {
        let plugins_entry = root_table
            .entry("plugins".to_string())
            .or_insert_with(|| toml::Value::Table(Default::default()));
        if let Some(plugins_table) = plugins_entry.as_table_mut() {
            let desigma_entry = plugins_table
                .entry("desigma".to_string())
                .or_insert_with(|| toml::Value::Table(Default::default()));
            if let Some(desigma_table) = desigma_entry.as_table_mut() {
                desigma_table.insert("enabled".to_string(), toml::Value::Boolean(enabled));
            }
        }
    }

    let serialized_toml = toml::to_string_pretty(&rula_val).map_err(|e| DesigmaError::InvalidSetting {
        key: "enabled",
        details: format!("cannot serialize rula.toml: {}", e),
    })?;
    tokio::fs::write(&rula_path, serialized_toml)
        .await
        .map_err(io_err(format!("Cannot write {}", rula_path.display())))?;

    if enabled {
        let design_dir = resolve_design_dir(&project_path)?;
        tokio::fs::create_dir_all(&design_dir)
            .await
            .map_err(io_err(format!("Cannot create {}", design_dir.display())))?;
        crate::kopeng::skill::inject_specialized_skill(project_path.clone(), "oneship-desigma".to_string())?;
    } else {
        stop_design_dir_watcher(&project_path).await;
        stop_desigma_mcp_process(&project_path).await;
        crate::kopeng::skill::cleanup_ephemeral_skill(project_path.clone(), "oneship-desigma".to_string())?;
    }

    crate::kopeng::skill::inject_project_mcp_plugin(p_path)?;

    get_project_desigma_status(project_path).await
}

/// Launches the native `desigma` desktop editor scoped to the project's design directory,
/// focusing an existing window if already open or placing a new instance on an empty Hyprland workspace.
pub async fn launch_project_desigma(project_path: String) -> Result<DesigmaStatus, DesigmaError> {
    let project_path = require_registered_project(&project_path)?;
    if !is_project_desigma_enabled(Path::new(&project_path)) {
        return Err(DesigmaError::PluginDisabled { project_path });
    }

    if let Some(existing_win) = crate::desktop::find_hyprland_window_for_project(
        &["desigma", "oneship-designer"],
        &project_path,
    )
    .await
    {
        crate::desktop::focus_hyprland_window(&existing_win).await;
        return get_project_desigma_status(project_path).await;
    }

    let app_bin = resolve_app_binary(&project_path)?;
    let design_dir = resolve_design_dir(&project_path)?;
    tokio::fs::create_dir_all(&design_dir)
        .await
        .map_err(io_err(format!("Cannot create {}", design_dir.display())))?;

    let use_bridge = resolve_enable_bridge(&project_path);

    // If a .desigma file already exists in design_dir, open the first one directly.
    let first_file = {
        let dir = design_dir.clone();
        tokio::task::spawn_blocking(move || {
            std::fs::read_dir(&dir).ok().and_then(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|e| e.path())
                    .find(|p| p.extension().and_then(|ext| ext.to_str()) == Some("desigma"))
            })
        })
        .await
        .ok()
        .flatten()
    };

    let empty_ws = crate::desktop::find_empty_hyprland_workspace().await;

    let app_str = app_bin.to_string_lossy().to_string();
    let mut std_cmd = crate::desktop::desktop_command(&app_str);
    std_cmd
        .current_dir(&design_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if use_bridge {
        std_cmd.arg("--bridge");
    } else {
        std_cmd.arg("--no-bridge");
    }
    if let Some(file_path) = first_file {
        std_cmd.arg(file_path);
    }
    let mut cmd = Command::from(std_cmd);
    let child = cmd.spawn().map_err(|e| DesigmaError::ProcessLifecycle {
        details: format!("Failed to spawn '{}': {}", app_bin.display(), e),
    })?;
    let spawned_pid = child.id();
    runtime_pool()
        .editor_processes
        .lock()
        .await
        .insert(project_path.clone(), child);

    if let (Some(ws), Some(pid)) = (empty_ws, spawned_pid) {
        crate::desktop::wait_and_move_hyprland_window(
            pid,
            &["desigma", "oneship-designer"],
            &project_path,
            ws,
        )
        .await;
    }

    get_project_desigma_status(project_path).await
}

const MAX_PREVIEW_CACHE_ITEMS_COUNT: usize = 32;

#[derive(Clone)]
struct CachedPreviewEntry {
    modified_ms: u128,
    result: DesigmaPreviewResult,
}

#[derive(Deserialize)]
struct RawPreviewJson {
    name: String,
    active_page_id: u64,
    selected_node_id: u64,
    pages: Vec<DesigmaPageInfo>,
    layers: Vec<DesigmaLayerInfo>,
    svg: String,
}

fn preview_cache() -> &'static Mutex<HashMap<(PathBuf, Option<u64>), CachedPreviewEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<(PathBuf, Option<u64>), CachedPreviewEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn read_file_modified_ms(path: &Path) -> u128 {
    tokio::fs::metadata(path)
        .await
        .ok()
        .and_then(|meta| meta.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Renders a `.desigma` / `.fig` file from `<project_path>/<design_dir>` headlessly to vector SVG
/// and extracts page/layer hierarchy using `desigma --preview-json` (zero GUI windows).
/// Results are cached by `(target_path, node_id, modified_ms)`.
pub async fn render_desigma_preview(
    project_path: String,
    file_name: Option<String>,
    node_id: Option<u64>,
) -> Result<DesigmaPreviewResult, DesigmaError> {
    let project_path = require_registered_project(&project_path)?;
    if !is_project_desigma_enabled(Path::new(&project_path)) {
        return Err(DesigmaError::PluginDisabled { project_path });
    }

    let app_bin = resolve_app_binary(&project_path)?;
    let design_dir = resolve_design_dir(&project_path)?;
    let files = list_design_files(&design_dir).await;
    let target_name = match file_name {
        Some(name) if !name.trim().is_empty() => {
            if name.contains('/') || name.contains('\\') || name.contains("..") {
                return Err(DesigmaError::InvalidSetting {
                    key: "file_name",
                    details: "file_name must be a plain filename inside design_dir".to_string(),
                });
            }
            name
        }
        _ => files.into_iter().next().ok_or_else(|| DesigmaError::InvalidSetting {
            key: "file_name",
            details: format!("No .desigma or .fig files found in {}", design_dir.display()),
        })?,
    };

    let target_path = design_dir.join(&target_name);
    if !target_path.exists() {
        return Err(DesigmaError::InvalidSetting {
            key: "file_name",
            details: format!("Design file '{}' does not exist", target_path.display()),
        });
    }

    let cache_key = (target_path.clone(), node_id);
    let modified_ms = read_file_modified_ms(&target_path).await;
    if modified_ms > 0 {
        let cache = preview_cache().lock().await;
        if let Some(cached) = cache.get(&cache_key) {
            if cached.modified_ms == modified_ms {
                return Ok(cached.result.clone());
            }
        }
    }

    let mut cmd = Command::new(&app_bin);
    cmd.arg(&target_path)
        .arg("--preview-json")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(id) = node_id {
        cmd.arg("--node").arg(id.to_string());
    }

    let output = tokio::time::timeout(std::time::Duration::from_secs(15), cmd.output())
        .await
        .map_err(|_| DesigmaError::ProcessLifecycle {
            details: "Timed out exporting Desigma vector preview".to_string(),
        })?
        .map_err(|e| DesigmaError::ProcessLifecycle {
            details: format!("Failed running desigma --preview-json: {}", e),
        })?;

    if !output.status.success() {
        return Err(DesigmaError::ProcessLifecycle {
            details: format!(
                "desigma --preview-json exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }

    let raw: RawPreviewJson = serde_json::from_slice(&output.stdout).map_err(|e| {
        DesigmaError::ProcessLifecycle {
            details: format!("Invalid JSON from desigma --preview-json: {}", e),
        }
    })?;

    let rendered_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let result = DesigmaPreviewResult {
        file_name: target_name,
        document_name: raw.name,
        active_page_id: raw.active_page_id,
        selected_node_id: raw.selected_node_id,
        pages: raw.pages,
        layers: raw.layers,
        svg: raw.svg,
        rendered_at_ms,
    };

    if modified_ms > 0 {
        let mut cache = preview_cache().lock().await;
        if cache.len() >= MAX_PREVIEW_CACHE_ITEMS_COUNT && !cache.contains_key(&cache_key) {
            if let Some(evict_key) = cache.keys().next().cloned() {
                cache.remove(&evict_key);
            }
        }
        cache.insert(
            cache_key,
            CachedPreviewEntry {
                modified_ms,
                result: result.clone(),
            },
        );
    }

    Ok(result)
}

async fn stop_desigma_mcp_process(project_path: &str) {
    let entry = runtime_pool().mcp_processes.lock().await.remove(project_path);
    if let Some(proc_arc) = entry {
        let mut proc = proc_arc.lock().await;
        let _ = proc.child.kill().await;
    }
}

async fn spawn_and_initialize_mcp(project_path: &str) -> Result<DesigmaMcpProcess, DesigmaError> {
    let mcp_bin = resolve_mcp_binary(project_path)?;
    let design_dir = resolve_design_dir(project_path)?;
    tokio::fs::create_dir_all(&design_dir)
        .await
        .map_err(io_err(format!("Cannot create {}", design_dir.display())))?;

    let mut child = Command::new(&mcp_bin)
        .arg("--workspace")
        .arg(&design_dir)
        .current_dir(project_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| DesigmaError::McpTransport {
            details: format!("Failed to spawn '{}': {}", mcp_bin.display(), e),
        })?;

    let stdin = child.stdin.take().ok_or_else(|| DesigmaError::McpTransport {
        details: "desigma-mcp child stdin unavailable".to_string(),
    })?;
    let stdout = child.stdout.take().ok_or_else(|| DesigmaError::McpTransport {
        details: "desigma-mcp child stdout unavailable".to_string(),
    })?;

    Ok(DesigmaMcpProcess {
        child,
        stdin,
        stdout: BufReader::new(stdout),
    })
}

async fn send_stdio_jsonrpc(
    proc: &mut DesigmaMcpProcess,
    payload: &Value,
    timeout_ms: u64,
    expect_response: bool,
) -> Result<Option<Value>, DesigmaError> {
    let mut line = serde_json::to_string(payload).map_err(|e| DesigmaError::McpTransport {
        details: format!("Cannot serialize JSON-RPC payload: {}", e),
    })?;
    line.push('\n');

    proc.stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|e| DesigmaError::McpTransport {
            details: format!("Failed writing to desigma-mcp stdin: {}", e),
        })?;
    proc.stdin.flush().await.map_err(|e| DesigmaError::McpTransport {
        details: format!("Failed flushing desigma-mcp stdin: {}", e),
    })?;

    if !expect_response {
        return Ok(None);
    }

    let mut response_line = String::new();
    let read_res = tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms),
        proc.stdout.read_line(&mut response_line),
    )
    .await
    .map_err(|_| DesigmaError::McpTransport {
        details: format!("Timed out after {} ms waiting for desigma-mcp response", timeout_ms),
    })?;

    let bytes_read = read_res.map_err(|e| DesigmaError::McpTransport {
        details: format!("Failed reading from desigma-mcp stdout: {}", e),
    })?;
    if bytes_read == 0 {
        return Err(DesigmaError::McpTransport {
            details: "desigma-mcp closed stdout unexpectedly".to_string(),
        });
    }

    let parsed: Value = serde_json::from_str(response_line.trim()).map_err(|e| DesigmaError::McpTransport {
        details: format!("Invalid JSON-RPC response from desigma-mcp: {}", e),
    })?;
    Ok(Some(parsed))
}

async fn get_or_spawn_mcp_process(project_path: &str) -> Result<(Arc<Mutex<DesigmaMcpProcess>>, bool), DesigmaError> {
    let mut map = runtime_pool().mcp_processes.lock().await;
    if let Some(existing) = map.get(project_path).cloned() {
        let mut guard = existing.lock().await;
        if matches!(guard.child.try_wait(), Ok(None)) {
            drop(guard);
            return Ok((existing, false));
        }
        map.remove(project_path);
    }

    let proc = spawn_and_initialize_mcp(project_path).await?;
    let arc = Arc::new(Mutex::new(proc));
    map.insert(project_path.to_string(), arc.clone());
    Ok((arc, true))
}

/// Resolves target project path for `/mcp/desigma` using Rule 11 process attestation
/// or `?project_hex=` for unmanaged local clients.
async fn resolve_desigma_gateway_origin(
    connect_info: Option<ConnectInfo<SocketAddr>>,
    server_port: u16,
    query: &DesigmaGatewayQuery,
) -> Result<String, DesigmaError> {
    if let Some(ConnectInfo(peer)) = connect_info {
        let resolved = tokio::task::spawn_blocking(move || {
            crate::caller_identity::resolve_caller(peer, server_port)
        })
        .await
        .map_err(|e| DesigmaError::Task(e.to_string()))?;

        match resolved {
            Ok(caller) => {
                if let Some(p_path) = caller.project_path {
                    return Ok(p_path);
                }
            }
            Err(crate::caller_identity::CallerError::UnmanagedCaller { .. }) => {}
            Err(e) => return Err(DesigmaError::ProjectContextMissing { details: e.to_string() }),
        }
    }

    let explicit_path = match query.project_hex.as_deref() {
        Some(hex) => Some(decode_hex_string(hex).ok_or_else(|| DesigmaError::ProjectContextMissing {
            details: format!("project_hex '{}' is not hex-encoded UTF-8", hex),
        })?),
        None => None,
    };

    if let Some(candidate_path) = explicit_path {
        let projects = crate::db::db_get_projects().map_err(|e| DesigmaError::ProjectContextMissing {
            details: e.to_string(),
        })?;
        if let Some(verified) = projects.into_iter().find(|p| p.path == candidate_path) {
            return Ok(verified.path);
        }
        return Err(DesigmaError::ProjectNotFound { path: candidate_path });
    }

    Err(DesigmaError::ProjectContextMissing {
        details: "Caller is neither an attested ONE-SHIP agent process nor scoped via ?project_hex.".to_string(),
    })
}

fn should_restart_stale_mcp(resp: &JsonRpcResponse) -> bool {
    let serialized = serde_json::to_string(resp).unwrap_or_default();
    serialized.contains("Editor bridge is off") || serialized.contains("An editor appeared")
}

async fn forward_to_desigma_mcp(
    project_path: &str,
    req: &JsonRpcRequest,
    req_id: Value,
    is_notification: bool,
    timeout_ms: u64,
) -> Result<JsonRpcResponse, DesigmaError> {
    let (proc_arc, just_spawned) = get_or_spawn_mcp_process(project_path).await?;
    let mut proc = proc_arc.lock().await;

    if just_spawned && req.method != "initialize" {
        let init_req = json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "one-ship-gateway", "version": env!("CARGO_PKG_VERSION") }
            }
        });
        if let Err(e) = send_stdio_jsonrpc(&mut proc, &init_req, timeout_ms, true).await {
            drop(proc);
            stop_desigma_mcp_process(project_path).await;
            return Err(e);
        }
        let notif = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        if let Err(e) = send_stdio_jsonrpc(&mut proc, &notif, timeout_ms, false).await {
            drop(proc);
            stop_desigma_mcp_process(project_path).await;
            return Err(e);
        }
    }

    let mut raw_obj = serde_json::Map::new();
    raw_obj.insert("jsonrpc".to_string(), json!(req.jsonrpc));
    if let Some(id) = req.id.clone() {
        raw_obj.insert("id".to_string(), id);
    }
    raw_obj.insert("method".to_string(), json!(req.method));
    if let Some(params) = req.params.clone() {
        raw_obj.insert("params".to_string(), params);
    }
    let raw_req = Value::Object(raw_obj);

    match send_stdio_jsonrpc(&mut proc, &raw_req, timeout_ms, !is_notification).await {
        Ok(Some(val)) => Ok(JsonRpcResponse {
            jsonrpc: "2.0",
            id: val.get("id").cloned().unwrap_or(req_id),
            result: val.get("result").cloned(),
            error: val.get("error").cloned(),
        }),
        Ok(None) => Ok(JsonRpcResponse {
            jsonrpc: "2.0",
            id: Value::Null,
            result: Some(json!({ "status": "ok" })),
            error: None,
        }),
        Err(e) => {
            drop(proc);
            stop_desigma_mcp_process(project_path).await;
            Err(e)
        }
    }
}

/// Axum HTTP POST handler mounted at `/mcp/desigma` on ONE-SHIP's MCP daemon.
/// Routes MCP requests to a per-project `desigma-mcp --workspace <project>/<design_dir>` child process.
pub async fn desigma_mcp_gateway_handler(
    connect_info: Option<ConnectInfo<SocketAddr>>,
    Query(query): Query<DesigmaGatewayQuery>,
    Json(req): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    reap_finished_editors().await;
    let is_notification = req.id.is_none();
    let req_id = req.id.clone().unwrap_or(Value::Null);
    let mcp_port = crate::config::mcp_daemon_port();

    let project_path = match resolve_desigma_gateway_origin(connect_info, mcp_port, &query).await {
        Ok(p) => p,
        Err(e) => {
            crate::logger::log("WARN", "desigma", &format!("[DESIGMA GATEWAY REJECTED] {}", e));
            return Json(JsonRpcResponse {
                jsonrpc: "2.0",
                id: req_id,
                result: None,
                error: Some(json!({ "code": -32001, "message": e.to_string() })),
            });
        }
    };

    if !is_project_desigma_enabled(Path::new(&project_path)) {
        return Json(JsonRpcResponse {
            jsonrpc: "2.0",
            id: req_id,
            result: None,
            error: Some(json!({
                "code": -32002,
                "message": format!("Desigma plugin is disabled for project '{}'", project_path),
            })),
        });
    }

    let timeout_ms = resolve_mcp_timeout_ms(&project_path);
    match forward_to_desigma_mcp(&project_path, &req, req_id.clone(), is_notification, timeout_ms).await {
        Ok(resp) if should_restart_stale_mcp(&resp) => {
            crate::logger::log("WARN", "desigma", "Restarting stale desigma-mcp session after editor state change");
            stop_desigma_mcp_process(&project_path).await;
            match forward_to_desigma_mcp(&project_path, &req, req_id.clone(), is_notification, timeout_ms).await {
                Ok(retry_resp) => Json(retry_resp),
                Err(e) => Json(JsonRpcResponse {
                    jsonrpc: "2.0",
                    id: req_id,
                    result: None,
                    error: Some(json!({ "code": -32603, "message": e.to_string() })),
                }),
            }
        }
        Ok(resp) => Json(resp),
        Err(e) => Json(JsonRpcResponse {
            jsonrpc: "2.0",
            id: req_id,
            result: None,
            error: Some(json!({ "code": -32603, "message": e.to_string() })),
        }),
    }
}
