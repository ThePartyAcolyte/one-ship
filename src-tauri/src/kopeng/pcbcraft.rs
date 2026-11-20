//! PCBCraft EDA/PCB integration (native SVG previews, BOM/netlist/DRC inspection,
//! external editor launch, and per-project `/mcp/pcbcraft` stdio gateway).
//!
//! Provides:
//! - Project-scoped `[plugins.pcbcraft]` configuration resolution (`rula.toml` + `config.rs`).
//! - Bounded recursive `notify` watcher emitting `pcbcraft-file-changed` over `/api/events`.
//! - Headless BOM, netlist, and `pcb.check_design` inspection via `pcbcraft-cli mcp --project`.
//! - Pure-Rust native SVG renderer for schematics and PCB board layouts (Spec 33 §3.2).
//! - Per-project `pcbcraft-cli mcp` stdio process pool behind `/mcp/pcbcraft` with Rule 11 scoping.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
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

const MAX_INSPECT_CACHE_ITEMS_COUNT: usize = 24;
const MAX_PREVIEW_CACHE_ITEMS_COUNT: usize = 32;
const MAX_SVG_BYTES: usize = 2 * 1024 * 1024;
const MAX_SVG_ELEMENTS_COUNT: usize = 8_000;
const PCBCRAFT_WATCH_DEBOUNCE_MS: u64 = 300;
const MAX_PCBCRAFT_WATCH_BURST_PATHS_COUNT: usize = 128;

/// Strongly-typed error hierarchy for PCBCraft EDA integration and MCP gateway operations.
#[derive(Debug, Error)]
pub enum PcbcraftError {
    #[error("Project not registered: {path}")]
    ProjectNotFound { path: String },

    #[error("Missing or invalid project context: {details}")]
    ProjectContextMissing { details: String },

    #[error("PCBCraft plugin is disabled for project '{project_path}'")]
    PluginDisabled { project_path: String },

    #[error("Invalid PCBCraft setting '{key}': {details}")]
    InvalidSetting { key: &'static str, details: String },

    #[error("PCBCraft binary not found at '{path}'")]
    BinaryNotFound { path: String },

    #[error("PCBCraft process lifecycle error: {details}")]
    ProcessLifecycle { details: String },

    #[error("PCBCraft MCP transport error: {details}")]
    McpTransport { details: String },

    #[error("I/O error ({context}): {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Background task failed: {0}")]
    Task(String),

    #[error(transparent)]
    Registry(#[from] crate::registry::RegistryError),

    #[error(transparent)]
    Config(#[from] crate::config::ConfigError),

    #[error(transparent)]
    Skill(#[from] crate::kopeng::skill::SkillError),
}

impl crate::api::ApiError for PcbcraftError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::ProjectNotFound { .. } => StatusCode::NOT_FOUND,
            Self::ProjectContextMissing { .. } => StatusCode::FORBIDDEN,
            Self::PluginDisabled { .. } | Self::InvalidSetting { .. } => StatusCode::BAD_REQUEST,
            Self::BinaryNotFound { .. } => StatusCode::FAILED_DEPENDENCY,
            Self::McpTransport { .. } => StatusCode::BAD_GATEWAY,
            Self::Io { .. } | Self::ProcessLifecycle { .. } | Self::Task(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            Self::Registry(e) => e.status(),
            Self::Config(e) => e.status(),
            Self::Skill(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::ProjectNotFound { .. } => "PcbcraftError::ProjectNotFound",
            Self::ProjectContextMissing { .. } => "PcbcraftError::ProjectContextMissing",
            Self::PluginDisabled { .. } => "PcbcraftError::PluginDisabled",
            Self::InvalidSetting { .. } => "PcbcraftError::InvalidSetting",
            Self::BinaryNotFound { .. } => "PcbcraftError::BinaryNotFound",
            Self::ProcessLifecycle { .. } => "PcbcraftError::ProcessLifecycle",
            Self::McpTransport { .. } => "PcbcraftError::McpTransport",
            Self::Io { .. } => "PcbcraftError::Io",
            Self::Task(_) => "PcbcraftError::Task",
            Self::Registry(_) => "PcbcraftError::Registry",
            Self::Config(_) => "PcbcraftError::Config",
            Self::Skill(_) => "PcbcraftError::Skill",
        }
    }
}

fn io_err(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> PcbcraftError {
    let ctx = context.into();
    move |source| PcbcraftError::Io { context: ctx, source }
}

/// Live operational status of the PCBCraft EDA plugin for a registered project.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct PcbcraftStatus {
    pub project_path: String,
    pub is_enabled: bool,
    pub pcb_dir_path: String,
    pub pcb_targets_count: usize,
    pub pcb_targets: Vec<String>,
    pub app_available: bool,
    pub cli_available: bool,
    pub is_editor_running: bool,
}

/// Arguments of `get_project_pcbcraft_status`.
#[derive(Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct GetProjectPcbcraftStatusArgs {
    pub project_path: String,
}

/// Arguments of `toggle_project_pcbcraft_plugin`.
#[derive(Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ToggleProjectPcbcraftPluginArgs {
    pub project_path: String,
    pub enabled: bool,
}

/// Arguments of `create_pcbcraft_project`.
#[derive(Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CreatePcbcraftProjectArgs {
    pub project_path: String,
    pub target_name: String,
}

/// Arguments of `launch_project_pcbcraft`.
#[derive(Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct LaunchProjectPcbcraftArgs {
    pub project_path: String,
    pub target_name: Option<String>,
}

/// Arguments of `inspect_pcbcraft_project`.
#[derive(Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectPcbcraftProjectArgs {
    pub project_path: String,
    pub target_name: Option<String>,
}

/// Arguments of `render_pcbcraft_svg_preview`.
#[derive(Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RenderPcbcraftSvgPreviewArgs {
    pub project_path: String,
    pub target_name: Option<String>,
    pub sheet_id: Option<String>,
    pub mode: Option<String>,
}

/// Summary metadata for a schematic sheet inside a PCBCraft project.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct PcbcraftSheetSummary {
    pub id: String,
    pub name: String,
    pub file: String,
    pub symbols_count: usize,
    pub wires_count: usize,
    pub net_labels_count: usize,
}

/// Summary metadata for a physical PCB board inside a PCBCraft project.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct PcbcraftBoardSummary {
    pub id: String,
    pub name: String,
    pub file: String,
    pub tracks_count: usize,
    pub vias_count: usize,
    pub footprints_count: usize,
}

/// Normalized component / BOM entry returned by `inspect_pcbcraft_project`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct PcbcraftComponentSummary {
    pub id: String,
    pub reference: String,
    pub value: String,
    pub definition: String,
    pub footprint: Option<String>,
    pub exclude_from_bom: bool,
    pub exclude_from_board: bool,
    pub exclude_from_simulation: bool,
}

/// Normalized electrical net entry extracted from PCBCraft netlist output.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct PcbcraftNetSummary {
    pub name: String,
    pub terminals_count: usize,
    pub junctions_count: usize,
}

/// Individual Design Rule Check violation entry returned by `pcb.check_design`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct PcbcraftDrcViolation {
    pub code: String,
    pub message: String,
    pub severity: String,
}

/// Complete BOM, netlist, sheet/board inventory, and DRC inspection for a PCBCraft target.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct PcbcraftProjectInspection {
    pub target_name: String,
    pub project_name: String,
    #[ts(type = "number")]
    pub revision: u64,
    pub sheets: Vec<PcbcraftSheetSummary>,
    pub boards: Vec<PcbcraftBoardSummary>,
    pub components: Vec<PcbcraftComponentSummary>,
    pub nets: Vec<PcbcraftNetSummary>,
    pub drc_status: String,
    pub drc_total_violations_count: usize,
    pub drc_violations: Vec<PcbcraftDrcViolation>,
    #[ts(type = "number")]
    pub inspected_at_ms: u64,
}

/// Native ONE-SHIP SVG vector preview for either a schematic sheet or PCB board layout.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct PcbcraftSvgPreviewResult {
    pub target_name: String,
    pub mode: String,
    pub sheet_id: Option<String>,
    pub title: String,
    pub elements_count: usize,
    pub svg: String,
    #[ts(type = "number")]
    pub rendered_at_ms: u64,
}

/// Optional query parameters on `/mcp/pcbcraft`.
#[derive(Deserialize, Debug, Default)]
pub struct PcbcraftGatewayQuery {
    pub project_hex: Option<String>,
    pub target: Option<String>,
}

struct PcbcraftMcpProcess {
    target_path: PathBuf,
    live_port: Option<u16>,
    #[allow(dead_code)]
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

#[derive(Deserialize, Debug, Clone)]
struct LiveEditorEndpoint {
    pid: u32,
    port: u16,
    token_file: PathBuf,
    project_path: PathBuf,
}

#[derive(Clone)]
struct CachedSnapshotEntry {
    modified_ms: u128,
    inspect_raw: Value,
    drc_raw: Value,
}

#[derive(Clone)]
struct CachedPreviewEntry {
    modified_ms: u128,
    result: PcbcraftSvgPreviewResult,
}

struct PcbcraftRuntimePool {
    file_watchers: Mutex<HashMap<String, tokio_util::sync::CancellationToken>>,
    mcp_processes: Mutex<HashMap<String, Arc<Mutex<PcbcraftMcpProcess>>>>,
    snapshot_cache: Mutex<HashMap<PathBuf, CachedSnapshotEntry>>,
    preview_cache: Mutex<HashMap<(PathBuf, String, Option<String>), CachedPreviewEntry>>,
    recent_agent_saves: Mutex<HashMap<(String, String), std::time::Instant>>,
}

static PCBCRAFT_POOL: OnceLock<PcbcraftRuntimePool> = OnceLock::new();

fn runtime_pool() -> &'static PcbcraftRuntimePool {
    PCBCRAFT_POOL.get_or_init(|| PcbcraftRuntimePool {
        file_watchers: Mutex::new(HashMap::new()),
        mcp_processes: Mutex::new(HashMap::new()),
        snapshot_cache: Mutex::new(HashMap::new()),
        preview_cache: Mutex::new(HashMap::new()),
        recent_agent_saves: Mutex::new(HashMap::new()),
    })
}

/// Checks whether `[plugins.pcbcraft] enabled = true` is configured in `<project_path>/rula.toml`.
pub fn is_project_pcbcraft_enabled(project_path: &Path) -> bool {
    let Some(rula) = crate::config::load_project_rula(project_path) else {
        return false;
    };
    rula.get("plugins")
        .and_then(|p| p.get("pcbcraft"))
        .and_then(|s| s.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Builds the project-scoped URL injected into `.mcp.json` and `opencode.json`.
pub fn build_project_pcbcraft_gateway_url(mcp_port: u16, project_path: &str) -> String {
    let mut encoded = String::with_capacity(project_path.len() * 2);
    for b in project_path.as_bytes() {
        let _ = write!(&mut encoded, "{:02x}", b);
    }
    format!("http://localhost:{}/mcp/pcbcraft?project_hex={}", mcp_port, encoded)
}

fn decode_hex_string(hex: &str) -> Option<String> {
    if hex.len() % 2 != 0 || hex.is_empty() {
        return None;
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for i in (0..hex.len()).step_by(2) {
        let byte = u8::from_str_radix(&hex[i..i + 2], 16).ok()?;
        bytes.push(byte);
    }
    String::from_utf8(bytes).ok()
}

fn require_registered_project(project_path: &str) -> Result<String, PcbcraftError> {
    let project = crate::registry::find_project_by_path(project_path)?;
    Ok(project.path)
}

fn resolve_pcb_dir(project_path: &str) -> Result<PathBuf, PcbcraftError> {
    let rel = crate::config::resolve_effective_string(Some(project_path), None, "pcbcraft", "pcb_dir");
    let trimmed = rel.trim();
    if trimmed.is_empty() || trimmed.contains("..") || Path::new(trimmed).is_absolute() {
        return Err(PcbcraftError::InvalidSetting {
            key: "pcb_dir",
            details: format!("'{}' must be a safe relative directory name", rel),
        });
    }
    Ok(Path::new(project_path).join(trimmed))
}

fn resolve_app_binary(project_path: &str) -> Result<PathBuf, PcbcraftError> {
    let path_str = crate::config::resolve_effective_string(Some(project_path), None, "pcbcraft", "app_binary_path");
    crate::desktop::resolve_executable(path_str.trim()).ok_or_else(|| PcbcraftError::BinaryNotFound {
        path: path_str.trim().to_string(),
    })
}

fn resolve_cli_binary(project_path: &str) -> Result<PathBuf, PcbcraftError> {
    let path_str = crate::config::resolve_effective_string(Some(project_path), None, "pcbcraft", "cli_binary_path");
    crate::desktop::resolve_executable(path_str.trim()).ok_or_else(|| PcbcraftError::BinaryNotFound {
        path: path_str.trim().to_string(),
    })
}

fn resolve_mcp_timeout_ms(project_path: &str) -> u64 {
    crate::config::resolve_effective_u64(Some(project_path), None, "pcbcraft", "mcp_request_timeout_ms")
        .clamp(1_000, 120_000)
}

fn validate_target_name(raw: Option<String>, available: &[String], pcb_dir: &Path) -> Result<(String, PathBuf), PcbcraftError> {
    let target_name = match raw {
        Some(name) if !name.trim().is_empty() => {
            let trimmed = name.trim();
            if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") || trimmed.starts_with('-') {
                return Err(PcbcraftError::InvalidSetting {
                    key: "target_name",
                    details: "target_name must be a plain project folder or .pc filename inside pcb_dir".to_string(),
                });
            }
            trimmed.to_string()
        }
        _ => available.first().cloned().ok_or_else(|| PcbcraftError::InvalidSetting {
            key: "target_name",
            details: format!("No PCBCraft projects found in {}", pcb_dir.display()),
        })?,
    };
    let target_path = pcb_dir.join(&target_name);
    Ok((target_name, target_path))
}

async fn list_pcb_targets(pcb_dir: &Path) -> Vec<String> {
    let dir = pcb_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut names = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = entry.file_name().into_string().ok() else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() && path.join("project.json").is_file() {
                names.push(name);
            } else if path.is_file()
                && path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("pc"))
            {
                names.push(name);
            }
        }
        names.sort();
        names
    })
    .await
    .unwrap_or_default()
}

async fn read_target_modified_ms(target_path: &Path) -> u128 {
    let path = target_path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let meta_ms = |p: &Path| -> u128 {
            std::fs::metadata(p)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis())
                .unwrap_or(0)
        };
        if path.is_file() {
            return meta_ms(&path);
        }
        let mut newest = meta_ms(&path.join("project.json"));
        for sub in ["sheets", "boards", "library"] {
            let dir = path.join(sub);
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let ep = entry.path();
                    if ep.is_file() {
                        newest = newest.max(meta_ms(&ep));
                    } else if ep.is_dir() {
                        if let Ok(nested) = std::fs::read_dir(&ep) {
                            for ne in nested.flatten() {
                                newest = newest.max(meta_ms(&ne.path()));
                            }
                        }
                    }
                }
            }
        }
        newest
    })
    .await
    .unwrap_or(0)
}

fn is_pcb_watch_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("json") || e.eq_ignore_ascii_case("pc"))
}

fn extract_target_from_event(pcb_dir: &Path, event_path: &Path) -> Option<String> {
    let rel = event_path.strip_prefix(pcb_dir).ok()?;
    let first = rel.components().next()?;
    let name = first.as_os_str().to_str()?;
    if name.starts_with('.') {
        return None;
    }
    Some(name.to_string())
}

async fn stop_pcb_dir_watcher(project_path: &str) {
    if let Some(token) = runtime_pool().file_watchers.lock().await.remove(project_path) {
        token.cancel();
    }
}

async fn ensure_pcb_dir_watcher(project_path: &str, pcb_dir: &Path) {
    if !tokio::fs::try_exists(pcb_dir).await.unwrap_or(false) {
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
    let dir = pcb_dir.to_path_buf();

    tokio::spawn(async move {
        use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
        let (tx, mut rx) = tokio::sync::mpsc::channel::<PathBuf>(MAX_PCBCRAFT_WATCH_BURST_PATHS_COUNT);

        let watcher_res = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                let Ok(ev) = res else { return; };
                if matches!(ev.kind, EventKind::Access(_)) {
                    return;
                }
                for p in ev.paths {
                    if is_pcb_watch_file(&p) || p.is_dir() {
                        let _ = tx.try_send(p);
                    }
                }
            },
            Config::default(),
        );

        let mut watcher = match watcher_res {
            Ok(w) => w,
            Err(e) => {
                crate::logger::log("WARN", "pcbcraft", &format!("Cannot create PCB watcher for {}: {}", proj, e));
                runtime_pool().file_watchers.lock().await.remove(&proj);
                return;
            }
        };

        if let Err(e) = watcher.watch(&dir, RecursiveMode::Recursive) {
            crate::logger::log("WARN", "pcbcraft", &format!("Cannot watch {}: {}", dir.display(), e));
            runtime_pool().file_watchers.lock().await.remove(&proj);
            return;
        }

        loop {
            tokio::select! {
                _ = token.cancelled() => break,
                maybe_path = rx.recv() => {
                    let Some(first_path) = maybe_path else { break; };
                    let mut targets = HashSet::new();
                    if let Some(t) = extract_target_from_event(&dir, &first_path) {
                        targets.insert(t);
                    }

                    tokio::time::sleep(std::time::Duration::from_millis(PCBCRAFT_WATCH_DEBOUNCE_MS)).await;
                    let mut drained = 0usize;
                    while drained < MAX_PCBCRAFT_WATCH_BURST_PATHS_COUNT {
                        match rx.try_recv() {
                            Ok(extra) => {
                                drained += 1;
                                if let Some(t) = extract_target_from_event(&dir, &extra) {
                                    targets.insert(t);
                                }
                            }
                            Err(_) => break,
                        }
                    }

                    for target_name in targets {
                        let target_path = dir.join(&target_name);
                        let modified_ms = read_target_modified_ms(&target_path).await;
                        let skip_snapshot = {
                            let saves = runtime_pool().recent_agent_saves.lock().await;
                            saves
                                .get(&(proj.clone(), target_name.clone()))
                                .is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(2500))
                        };
                        if !skip_snapshot && modified_ms > 0 {
                            if let Ok(rel) = target_path.strip_prefix(Path::new(&proj)) {
                                let rel_str = rel.to_string_lossy().to_string();
                                if let Err(e) = crate::kopeng::snapshots::try_create_auto_domain_snapshot(
                                    PathBuf::from(&proj),
                                    "pcbcraft",
                                    target_name.clone(),
                                    "user".to_string(),
                                    format!("pcb-edit: {}", target_name),
                                    vec![rel_str],
                                )
                                .await
                                {
                                    crate::logger::log("WARN", "pcbcraft", &format!("Auto-snapshot on watcher edit failed: {}", e));
                                }
                            }
                        }
                        let payload = crate::events::PcbcraftFileChangedPayload {
                            project_path: proj.clone(),
                            target_name,
                            modified_ms: modified_ms as u64,
                        };
                        crate::supervisor::get_supervisor().emit_event(
                            crate::events::DaemonEvent::pcbcraft_file_changed(&payload),
                        );
                    }
                }
            }
        }

        runtime_pool().file_watchers.lock().await.remove(&proj);
    });
}

/// Returns live status for PCBCraft on a registered project.
pub async fn get_project_pcbcraft_status(project_path: String) -> Result<PcbcraftStatus, PcbcraftError> {
    let project_path = require_registered_project(&project_path)?;
    let p_path = Path::new(&project_path);
    let is_enabled = is_project_pcbcraft_enabled(p_path);
    let pcb_dir = resolve_pcb_dir(&project_path)?;
    if is_enabled {
        ensure_pcb_dir_watcher(&project_path, &pcb_dir).await;
    }
    let pcb_targets = list_pcb_targets(&pcb_dir).await;
    let pcb_targets_count = pcb_targets.len();
    let app_available = resolve_app_binary(&project_path).is_ok();
    let cli_available = resolve_cli_binary(&project_path).is_ok();
    let is_editor_running = crate::desktop::find_hyprland_window_for_project(
        &["pcbcraft", "PCBCraft"],
        &project_path,
    )
    .await
    .is_some();

    Ok(PcbcraftStatus {
        project_path,
        is_enabled,
        pcb_dir_path: pcb_dir.display().to_string(),
        pcb_targets_count,
        pcb_targets,
        app_available,
        cli_available,
        is_editor_running,
    })
}

/// Enables or disables `[plugins.pcbcraft]` in `<project_path>/rula.toml`, manages the
/// `oneship-pcbcraft` platform skill symlink, and synchronizes `.mcp.json` / `opencode.json`.
pub async fn toggle_project_pcbcraft_plugin(
    project_path: String,
    enabled: bool,
) -> Result<PcbcraftStatus, PcbcraftError> {
    let project_path = require_registered_project(&project_path)?;
    let p_path = Path::new(&project_path);

    let rula_path = p_path.join("rula.toml");
    let mut rula_val = crate::config::read_project_rula(p_path)?.ok_or_else(|| PcbcraftError::InvalidSetting {
        key: "enabled",
        details: format!("{} does not exist; initialize the project first", rula_path.display()),
    })?;

    if let Some(root_table) = rula_val.as_table_mut() {
        let plugins_entry = root_table
            .entry("plugins".to_string())
            .or_insert_with(|| toml::Value::Table(Default::default()));
        if let Some(plugins_table) = plugins_entry.as_table_mut() {
            let entry = plugins_table
                .entry("pcbcraft".to_string())
                .or_insert_with(|| toml::Value::Table(Default::default()));
            if let Some(tab) = entry.as_table_mut() {
                tab.insert("enabled".to_string(), toml::Value::Boolean(enabled));
            }
        }
    }

    let serialized_toml = toml::to_string_pretty(&rula_val).map_err(|e| PcbcraftError::InvalidSetting {
        key: "enabled",
        details: format!("cannot serialize rula.toml: {}", e),
    })?;
    tokio::fs::write(&rula_path, serialized_toml)
        .await
        .map_err(io_err(format!("Cannot write {}", rula_path.display())))?;

    if enabled {
        let pcb_dir = resolve_pcb_dir(&project_path)?;
        tokio::fs::create_dir_all(&pcb_dir)
            .await
            .map_err(io_err(format!("Cannot create {}", pcb_dir.display())))?;
        let p_clone = project_path.clone();
        tokio::task::spawn_blocking(move || -> Result<(), crate::kopeng::skill::SkillError> {
            crate::kopeng::skill::inject_specialized_skill(p_clone.clone(), "oneship-pcbcraft".to_string())?;
            crate::kopeng::skill::inject_project_mcp_plugin(Path::new(&p_clone))
        })
        .await
        .map_err(|e| PcbcraftError::Task(e.to_string()))??;
    } else {
        stop_pcb_dir_watcher(&project_path).await;
        stop_pcbcraft_mcp_process(&project_path).await;
        let p_clone = project_path.clone();
        tokio::task::spawn_blocking(move || -> Result<(), crate::kopeng::skill::SkillError> {
            let _ = crate::kopeng::skill::cleanup_ephemeral_skill(p_clone.clone(), "oneship-pcbcraft".to_string());
            crate::kopeng::skill::inject_project_mcp_plugin(Path::new(&p_clone))
        })
        .await
        .map_err(|e| PcbcraftError::Task(e.to_string()))??;
    }

    get_project_pcbcraft_status(project_path).await
}

/// Scaffolds a new empty `pcbcraft-project/16` folder inside `<project_path>/<pcb_dir>/<target_name>`
/// and validates it through `pcbcraft-cli mcp --project`.
pub async fn create_pcbcraft_project(
    project_path: String,
    target_name: String,
) -> Result<PcbcraftStatus, PcbcraftError> {
    let project_path = require_registered_project(&project_path)?;
    if !is_project_pcbcraft_enabled(Path::new(&project_path)) {
        return Err(PcbcraftError::PluginDisabled { project_path });
    }

    let trimmed = target_name.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('-')
        || trimmed.starts_with('.')
        || trimmed.contains(['/', '\\', '\0', '\n', '\r'])
        || trimmed.contains("..")
    {
        return Err(PcbcraftError::InvalidSetting {
            key: "target_name",
            details: "target_name must be a safe folder name inside pcb_dir".to_string(),
        });
    }
    let clean_name = trimmed.to_string();
    let cli_bin = resolve_cli_binary(&project_path)?;
    let pcb_dir = resolve_pcb_dir(&project_path)?;
    let target_dir = pcb_dir.join(&clean_name);

    if tokio::fs::try_exists(&target_dir).await.unwrap_or(false) {
        return Err(PcbcraftError::InvalidSetting {
            key: "target_name",
            details: format!("PCBCraft target '{}' already exists", clean_name),
        });
    }

    let target_dir_clone = target_dir.clone();
    let display_name = clean_name.clone();
    tokio::task::spawn_blocking(move || -> Result<(), PcbcraftError> {
        let proj_id = uuid::Uuid::new_v4().to_string();
        let sheet_id = uuid::Uuid::new_v4().to_string();
        let cu_id = uuid::Uuid::new_v4().to_string();
        let occ_id = uuid::Uuid::new_v4().to_string();

        std::fs::create_dir_all(target_dir_clone.join("sheets"))
            .map_err(io_err("Cannot create sheets/ directory"))?;

        let sheet_doc = json!({
            "format": "pcbcraft-sheet/6",
            "id": sheet_id,
            "net_labels": [],
            "labels": [],
            "subsheets": [],
            "symbols": [],
            "junctions": [],
            "wires": []
        });
        let sheet_path = target_dir_clone.join("sheets").join(format!("{}.json", sheet_id));
        std::fs::write(
            &sheet_path,
            serde_json::to_string_pretty(&sheet_doc).unwrap_or_default(),
        )
        .map_err(io_err("Cannot write initial sheet json"))?;

        let project_doc = json!({
            "format": "pcbcraft-project/16",
            "id": proj_id,
            "name": display_name,
            "numeric_id_high_water": 1,
            "layout_root": ".",
            "boards": [],
            "sheets": [{
                "id": sheet_id,
                "name": "Main",
                "file": format!("sheets/{}.json", sheet_id)
            }],
            "schematic_instances": {
                "circuit_uses": [{ "id": cu_id }],
                "sheet_occurrences": [{
                    "id": occ_id,
                    "definition": sheet_id,
                    "circuit_use": cu_id,
                    "parent": Value::Null,
                    "parent_slot": Value::Null
                }]
            },
            "components": []
        });
        std::fs::write(
            target_dir_clone.join("project.json"),
            serde_json::to_string_pretty(&project_doc).unwrap_or_default(),
        )
        .map_err(io_err("Cannot write project.json"))?;
        Ok(())
    })
    .await
    .map_err(|e| PcbcraftError::Task(e.to_string()))??;

    // Verify loadability through `pcbcraft-cli mcp --project <target_dir>`.
    let timeout_ms = resolve_mcp_timeout_ms(&project_path);
    let check_res = run_oneshot_pcbcraft_mcp(&cli_bin, &target_dir, timeout_ms, false).await;
    if let Err(e) = check_res {
        let _ = tokio::fs::remove_dir_all(&target_dir).await;
        return Err(e);
    }

    let modified_ms = read_target_modified_ms(&target_dir).await;
    let payload = crate::events::PcbcraftFileChangedPayload {
        project_path: project_path.clone(),
        target_name: clean_name,
        modified_ms: modified_ms as u64,
    };
    crate::supervisor::get_supervisor().emit_event(
        crate::events::DaemonEvent::pcbcraft_file_changed(&payload),
    );

    get_project_pcbcraft_status(project_path).await
}

/// Launches the external PCBCraft desktop editor scoped to `<project_path>/<pcb_dir>`,
/// focusing an existing window or placing a new window on an empty Hyprland workspace.
pub async fn launch_project_pcbcraft(
    project_path: String,
    target_name: Option<String>,
) -> Result<PcbcraftStatus, PcbcraftError> {
    let project_path = require_registered_project(&project_path)?;
    if !is_project_pcbcraft_enabled(Path::new(&project_path)) {
        return Err(PcbcraftError::PluginDisabled { project_path });
    }

    if let Some(existing_win) = crate::desktop::find_hyprland_window_for_project(
        &["pcbcraft", "PCBCraft"],
        &project_path,
    )
    .await
    {
        crate::desktop::focus_hyprland_window(&existing_win).await;
        return get_project_pcbcraft_status(project_path).await;
    }

    let app_bin = resolve_app_binary(&project_path)?;
    let pcb_dir = resolve_pcb_dir(&project_path)?;
    tokio::fs::create_dir_all(&pcb_dir)
        .await
        .map_err(io_err(format!("Cannot create {}", pcb_dir.display())))?;

    let targets = list_pcb_targets(&pcb_dir).await;
    let launch_target = match target_name {
        Some(name) if !name.trim().is_empty() => {
            let (_, t_path) = validate_target_name(Some(name), &targets, &pcb_dir)?;
            if !tokio::fs::try_exists(&t_path).await.unwrap_or(false) {
                return Err(PcbcraftError::InvalidSetting {
                    key: "target_name",
                    details: format!("PCBCraft project '{}' does not exist", t_path.display()),
                });
            }
            Some(t_path)
        }
        _ => targets.first().map(|first| pcb_dir.join(first)),
    };
    let launch_cwd = launch_target
        .as_ref()
        .filter(|p| p.is_dir())
        .cloned()
        .unwrap_or_else(|| pcb_dir.clone());

    let empty_ws = crate::desktop::find_empty_hyprland_workspace().await;
    let app_str = app_bin.to_string_lossy().to_string();
    let mut std_cmd = crate::desktop::desktop_command(&app_str);
    std_cmd
        .current_dir(&launch_cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(target_path) = launch_target {
        std_cmd.arg(target_path);
    }

    let mut cmd = Command::from(std_cmd);
    let mut child = cmd.spawn().map_err(|e| PcbcraftError::ProcessLifecycle {
        details: format!("Failed to spawn '{}': {}", app_bin.display(), e),
    })?;
    let spawned_pid = child.id();
    tokio::spawn(async move {
        if let Ok(status) = child.wait().await {
            if !status.success() {
                crate::logger::log("WARN", "pcbcraft", &format!("PCBCraft exited with {}", status));
            }
        }
    });

    if let (Some(ws), Some(pid)) = (empty_ws, spawned_pid) {
        crate::desktop::wait_and_move_hyprland_window(
            pid,
            &["pcbcraft", "PCBCraft"],
            &project_path,
            ws,
        )
        .await;
    }

    get_project_pcbcraft_status(project_path).await
}

async fn run_oneshot_pcbcraft_mcp(
    cli_bin: &Path,
    target_path: &Path,
    timeout_ms: u64,
    include_drc: bool,
) -> Result<(Value, Value), PcbcraftError> {
    let mut requests = vec![
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "one-ship", "version": env!("CARGO_PKG_VERSION") }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": "inspect_project", "arguments": {} }
        }),
    ];
    if include_drc {
        requests.push(json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": { "name": "execute", "arguments": { "command": "pcb.check_design", "params": {} } }
        }));
    }

    let mut payload = String::with_capacity(512);
    for req in requests {
        payload.push_str(&req.to_string());
        payload.push('\n');
    }

    let mut cmd = Command::new(cli_bin);
    cmd.arg("mcp")
        .arg("--project")
        .arg(target_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd.spawn().map_err(|e| PcbcraftError::ProcessLifecycle {
        details: format!("Failed spawning pcbcraft-cli mcp: {}", e),
    })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(payload.as_bytes())
            .await
            .map_err(io_err("Failed writing MCP request to pcbcraft-cli"))?;
        let _ = stdin.shutdown().await;
    }

    let output = tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| PcbcraftError::McpTransport {
        details: format!("Timed out after {} ms querying pcbcraft-cli mcp", timeout_ms),
    })?
    .map_err(|e| PcbcraftError::ProcessLifecycle {
        details: format!("Failed waiting for pcbcraft-cli mcp: {}", e),
    })?;

    if !output.status.success() {
        return Err(PcbcraftError::ProcessLifecycle {
            details: format!(
                "pcbcraft-cli mcp exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let mut inspect_val: Option<Value> = None;
    let mut drc_val: Value = Value::Null;

    for line in stdout_str.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(resp) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        let id = resp.get("id").and_then(|v| v.as_i64());
        if id == Some(2) {
            if let Some(err) = resp.get("error") {
                return Err(PcbcraftError::McpTransport {
                    details: format!("inspect_project failed: {}", err),
                });
            }
            if let Some(text) = resp
                .get("result")
                .and_then(|r| r.get("content"))
                .and_then(|c| c.as_array())
                .and_then(|a| a.first())
                .and_then(|item| item.get("text"))
                .and_then(|t| t.as_str())
            {
                inspect_val = serde_json::from_str::<Value>(text).ok();
            }
        } else if id == Some(3) {
            let is_err = resp
                .get("result")
                .and_then(|r| r.get("isError"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if let Some(text) = resp
                .get("result")
                .and_then(|r| r.get("content"))
                .and_then(|c| c.as_array())
                .and_then(|a| a.first())
                .and_then(|item| item.get("text"))
                .and_then(|t| t.as_str())
            {
                if is_err {
                    drc_val = json!({ "error_text": text });
                } else {
                    drc_val = serde_json::from_str::<Value>(text).unwrap_or_else(|_| json!({ "raw": text }));
                }
            }
        }
    }

    let inspect = inspect_val.ok_or_else(|| PcbcraftError::McpTransport {
        details: "pcbcraft-cli mcp did not return inspect_project JSON".to_string(),
    })?;
    Ok((inspect, drc_val))
}

async fn load_cached_snapshot(
    cli_bin: &Path,
    target_path: &Path,
    timeout_ms: u64,
) -> Result<(Value, Value), PcbcraftError> {
    let modified_ms = read_target_modified_ms(target_path).await;
    let disk_key = format!("snapshot|{}", target_path.display());
    if modified_ms > 0 {
        let cache = runtime_pool().snapshot_cache.lock().await;
        if let Some(entry) = cache.get(target_path) {
            if entry.modified_ms == modified_ms {
                return Ok((entry.inspect_raw.clone(), entry.drc_raw.clone()));
            }
        }
        drop(cache);
        if let Some((inspect_raw, drc_raw)) = crate::kopeng::preview_cache::read_cached::<(Value, Value)>(
            "pcbcraft",
            &disk_key,
            modified_ms,
        )
        .await
        {
            let mut cache = runtime_pool().snapshot_cache.lock().await;
            cache.insert(
                target_path.to_path_buf(),
                CachedSnapshotEntry {
                    modified_ms,
                    inspect_raw: inspect_raw.clone(),
                    drc_raw: drc_raw.clone(),
                },
            );
            return Ok((inspect_raw, drc_raw));
        }
    }

    let (inspect_raw, drc_raw) = run_oneshot_pcbcraft_mcp(cli_bin, target_path, timeout_ms, true).await?;

    if modified_ms > 0 {
        let mut cache = runtime_pool().snapshot_cache.lock().await;
        if cache.len() >= MAX_INSPECT_CACHE_ITEMS_COUNT && !cache.contains_key(target_path) {
            if let Some(evict) = cache.keys().next().cloned() {
                cache.remove(&evict);
            }
        }
        cache.insert(
            target_path.to_path_buf(),
            CachedSnapshotEntry {
                modified_ms,
                inspect_raw: inspect_raw.clone(),
                drc_raw: drc_raw.clone(),
            },
        );
        drop(cache);
        crate::kopeng::preview_cache::write_cached(
            "pcbcraft",
            &disk_key,
            modified_ms,
            &(inspect_raw.clone(), drc_raw.clone()),
        )
        .await;
    }

    Ok((inspect_raw, drc_raw))
}

fn parse_pcbcraft_inspection(
    target_name: String,
    inspect_raw: &Value,
    drc_raw: &Value,
) -> PcbcraftProjectInspection {
    let project = inspect_raw.get("project").unwrap_or(inspect_raw);
    let project_name = project
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(&target_name)
        .to_string();
    let revision = inspect_raw
        .get("revision")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let mut footprint_by_comp: HashMap<&str, String> = HashMap::new();
    if let Some(fps) = project
        .get("board")
        .and_then(|b| b.get("footprints"))
        .and_then(|f| f.as_array())
    {
        for fp in fps {
            if let (Some(comp), Some(def)) = (
                fp.get("component").and_then(|v| v.as_str()),
                fp.get("definition").and_then(|v| v.as_str()),
            ) {
                footprint_by_comp.insert(comp, def.to_string());
            }
        }
    }

    let boards_arr = project
        .get("boards")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let board_footprints_count = project
        .get("board")
        .and_then(|b| b.get("footprints"))
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    let sheets = project
        .get("sheets")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|s| {
                    let sch = s.get("schematic");
                    PcbcraftSheetSummary {
                        id: s.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        name: s.get("name").and_then(|v| v.as_str()).unwrap_or("Sheet").to_string(),
                        file: s.get("file").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        symbols_count: sch
                            .and_then(|x| x.get("symbols"))
                            .and_then(|v| v.as_array())
                            .map(|a| a.len())
                            .unwrap_or(0),
                        wires_count: sch
                            .and_then(|x| x.get("wires"))
                            .and_then(|v| v.as_array())
                            .map(|a| a.len())
                            .unwrap_or(0),
                        net_labels_count: sch
                            .and_then(|x| x.get("net_labels"))
                            .and_then(|v| v.as_array())
                            .map(|a| a.len())
                            .unwrap_or(0),
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let boards = boards_arr
        .iter()
        .enumerate()
        .map(|(idx, b)| PcbcraftBoardSummary {
            id: b.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            name: b.get("name").and_then(|v| v.as_str()).unwrap_or("Board").to_string(),
            file: b.get("file").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            tracks_count: b.get("tracks").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0),
            vias_count: b.get("vias").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0),
            footprints_count: if idx == 0 { board_footprints_count } else { 0 },
        })
        .collect::<Vec<_>>();

    let components = project
        .get("components")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|c| {
                    let id = c.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let attrs = c.get("metadata").and_then(|m| m.get("attributes"));
                    PcbcraftComponentSummary {
                        footprint: footprint_by_comp.get(id.as_str()).cloned(),
                        id,
                        reference: c.get("reference").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        value: c.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        definition: c.get("definition").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        exclude_from_bom: attrs
                            .and_then(|a| a.get("exclude_from_bom"))
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                        exclude_from_board: attrs
                            .and_then(|a| a.get("exclude_from_board"))
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                        exclude_from_simulation: attrs
                            .and_then(|a| a.get("exclude_from_simulation"))
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut nets = project
        .get("netlist")
        .and_then(|nl| nl.get("nets"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|n| PcbcraftNetSummary {
                    name: n.get("name").and_then(|v| v.as_str()).unwrap_or("UNNAMED").to_string(),
                    terminals_count: n.get("terminals").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0),
                    junctions_count: n.get("junctions").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    nets.sort_by(|a, b| a.name.cmp(&b.name));

    let (drc_status, drc_total_violations_count, drc_violations) = if drc_raw.get("error_text").is_some() {
        ("no_board".to_string(), 0, Vec::new())
    } else if let Some(dc) = drc_raw.get("design_check") {
        let total = dc.get("total").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let violations = dc
            .get("violations")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|v| PcbcraftDrcViolation {
                        code: v
                            .get("code")
                            .or_else(|| v.get("kind"))
                            .and_then(|x| x.as_str())
                            .unwrap_or("DRC")
                            .to_string(),
                        message: v
                            .get("message")
                            .or_else(|| v.get("summary"))
                            .and_then(|x| x.as_str())
                            .unwrap_or("Design rule violation")
                            .to_string(),
                        severity: v
                            .get("severity")
                            .and_then(|x| x.as_str())
                            .unwrap_or("error")
                            .to_string(),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let status = if total == 0 { "pass" } else { "fail" }.to_string();
        (status, total, violations)
    } else {
        ("unavailable".to_string(), 0, Vec::new())
    };

    let inspected_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    PcbcraftProjectInspection {
        target_name,
        project_name,
        revision,
        sheets,
        boards,
        components,
        nets,
        drc_status,
        drc_total_violations_count,
        drc_violations,
        inspected_at_ms,
    }
}

/// Inspects BOM, netlist, sheets, boards, and DRC rules of a PCBCraft target via `pcbcraft-cli mcp`.
pub async fn inspect_pcbcraft_project(
    project_path: String,
    target_name: Option<String>,
) -> Result<PcbcraftProjectInspection, PcbcraftError> {
    let project_path = require_registered_project(&project_path)?;
    if !is_project_pcbcraft_enabled(Path::new(&project_path)) {
        return Err(PcbcraftError::PluginDisabled { project_path });
    }

    let cli_bin = resolve_cli_binary(&project_path)?;
    let pcb_dir = resolve_pcb_dir(&project_path)?;
    let targets = list_pcb_targets(&pcb_dir).await;
    let (target, target_path) = validate_target_name(target_name, &targets, &pcb_dir)?;
    if !tokio::fs::try_exists(&target_path).await.unwrap_or(false) {
        return Err(PcbcraftError::InvalidSetting {
            key: "target_name",
            details: format!("PCBCraft target '{}' does not exist", target_path.display()),
        });
    }

    let timeout_ms = resolve_mcp_timeout_ms(&project_path);
    let (inspect_raw, drc_raw) = load_cached_snapshot(&cli_bin, &target_path, timeout_ms).await?;
    Ok(parse_pcbcraft_inspection(target, &inspect_raw, &drc_raw))
}

fn xml_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

#[derive(Default)]
struct BoundsNm {
    has_points: bool,
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl BoundsNm {
    fn include(&mut self, x: f64, y: f64) {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        if !self.has_points {
            self.has_points = true;
            self.min_x = x;
            self.max_x = x;
            self.min_y = y;
            self.max_y = y;
        } else {
            self.min_x = self.min_x.min(x);
            self.max_x = self.max_x.max(x);
            self.min_y = self.min_y.min(y);
            self.max_y = self.max_y.max(y);
        }
    }

    fn padded_mm_viewbox(&self, pad_mm: f64) -> (f64, f64, f64, f64) {
        if !self.has_points {
            return (-25.0, -15.0, 50.0, 30.0);
        }
        let min_x = (self.min_x / 1_000_000.0) - pad_mm;
        let min_y = (self.min_y / 1_000_000.0) - pad_mm;
        let max_x = (self.max_x / 1_000_000.0) + pad_mm;
        let max_y = (self.max_y / 1_000_000.0) + pad_mm;
        let w = (max_x - min_x).max(10.0);
        let h = (max_y - min_y).max(10.0);
        (min_x, min_y, w, h)
    }
}

fn nm_to_mm(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(|x| x.as_f64()).map(|n| n / 1_000_000.0)
}

async fn render_native_schematic_svg_via_cli(
    cli_bin: &Path,
    target_path: &Path,
    sheet_id: Option<&str>,
    timeout_ms: u64,
) -> Result<(String, String, usize), PcbcraftError> {
    let mut cmd = Command::new(cli_bin);
    cmd.arg("render-svg").arg(target_path);
    if let Some(sid) = sheet_id.filter(|s| !s.trim().is_empty()) {
        cmd.arg("--sheet").arg(sid);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let output = tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms),
        cmd.output(),
    )
    .await
    .map_err(|_| PcbcraftError::McpTransport {
        details: format!("Timed out after {} ms running pcbcraft-cli render-svg", timeout_ms),
    })?
    .map_err(|e| PcbcraftError::McpTransport {
        details: format!("Failed spawning pcbcraft-cli render-svg: {}", e),
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(PcbcraftError::McpTransport {
            details: format!("pcbcraft-cli render-svg exited with {}: {}", output.status, stderr),
        });
    }

    let parsed: Value = serde_json::from_slice(&output.stdout).map_err(|e| PcbcraftError::McpTransport {
        details: format!("Invalid JSON from pcbcraft-cli render-svg: {}", e),
    })?;
    let title = parsed
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Schematic")
        .to_string();
    let svg = parsed
        .get("svg")
        .and_then(|v| v.as_str())
        .ok_or_else(|| PcbcraftError::McpTransport {
            details: "Missing 'svg' field from pcbcraft-cli render-svg".to_string(),
        })?
        .to_string();
    let elem_count = parsed
        .get("element_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    Ok((title, svg, elem_count))
}

fn render_schematic_svg(inspect_raw: &Value, sheet_id: Option<&str>) -> Result<(String, String, usize), PcbcraftError> {
    let project = inspect_raw.get("project").unwrap_or(inspect_raw);
    let sheets = project
        .get("sheets")
        .and_then(|v| v.as_array())
        .ok_or_else(|| PcbcraftError::McpTransport {
            details: "Missing sheets array in inspect_project output".to_string(),
        })?;

    let sheet = match sheet_id {
        Some(sid) => sheets
            .iter()
            .find(|s| s.get("id").and_then(|v| v.as_str()) == Some(sid))
            .ok_or_else(|| PcbcraftError::InvalidSetting {
                key: "sheet_id",
                details: format!("Schematic sheet '{}' not found", sid),
            })?,
        None => sheets.first().ok_or_else(|| PcbcraftError::InvalidSetting {
            key: "sheet_id",
            details: "Project contains no schematic sheets".to_string(),
        })?,
    };

    let title = sheet.get("name").and_then(|v| v.as_str()).unwrap_or("Main").to_string();
    let sch = sheet.get("schematic").unwrap_or(&Value::Null);

    let mut bounds = BoundsNm::default();
    let symbols = sch.get("symbols").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let wires = sch.get("wires").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let junctions = sch.get("junctions").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let net_labels = sch.get("net_labels").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    for sym in &symbols {
        if let (Some(x), Some(y)) = (sym.get("x_nm").and_then(|v| v.as_f64()), sym.get("y_nm").and_then(|v| v.as_f64())) {
            bounds.include(x - 3_000_000.0, y - 3_000_000.0);
            bounds.include(x + 3_000_000.0, y + 3_000_000.0);
        }
        if let Some(pins) = sym.get("pins").and_then(|v| v.as_array()) {
            for p in pins {
                if let (Some(px), Some(py)) = (p.get("x_nm").and_then(|v| v.as_f64()), p.get("y_nm").and_then(|v| v.as_f64())) {
                    bounds.include(px, py);
                }
            }
        }
    }
    for w in &wires {
        if let Some(path) = w.get("path").and_then(|v| v.as_array()) {
            for pt in path {
                if let (Some(x), Some(y)) = (pt.get("x_nm").and_then(|v| v.as_f64()), pt.get("y_nm").and_then(|v| v.as_f64())) {
                    bounds.include(x, y);
                }
            }
        }
    }
    for nl in &net_labels {
        if let Some(at) = nl.get("at") {
            if let (Some(x), Some(y)) = (at.get("x_nm").and_then(|v| v.as_f64()), at.get("y_nm").and_then(|v| v.as_f64())) {
                bounds.include(x, y);
            }
        }
    }

    let (vx, vy, vw, vh) = bounds.padded_mm_viewbox(12.0);
    let stroke_thin = (vw.max(vh) * 0.0022).clamp(0.18, 0.55);
    let font_sm = (vw.max(vh) * 0.013).clamp(1.4, 3.2);

    let mut svg = String::with_capacity(8192);
    let mut elem_count = 0usize;
    let _ = write!(
        &mut svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{:.3} {:.3} {:.3} {:.3}" class="oneship-pcb-svg" style="background:var(--bg-color,#0b0c0e)">"#,
        vx, vy, vw, vh
    );

    // 1. Wires
    svg.push_str(r##"<g stroke="#22c55e" fill="none" stroke-linecap="round" stroke-linejoin="round">"##);
    for w in &wires {
        if elem_count >= MAX_SVG_ELEMENTS_COUNT || svg.len() >= MAX_SVG_BYTES {
            break;
        }
        let Some(path) = w.get("path").and_then(|v| v.as_array()) else { continue };
        if path.len() < 2 {
            continue;
        }
        let mut pts = String::new();
        for pt in path {
            if let (Some(x), Some(y)) = (nm_to_mm(pt, "x_nm"), nm_to_mm(pt, "y_nm")) {
                let _ = write!(&mut pts, "{:.3},{:.3} ", x, y);
            }
        }
        if !pts.is_empty() {
            let _ = write!(&mut svg, r#"<polyline points="{}" stroke-width="{:.3}"/>"#, pts.trim_end(), stroke_thin * 1.15);
            elem_count += 1;
        }
    }
    svg.push_str("</g>");

    // 2. Junctions
    svg.push_str(r##"<g fill="#22c55e">"##);
    let j_rad = stroke_thin * 2.1;
    for j in &junctions {
        if elem_count >= MAX_SVG_ELEMENTS_COUNT || svg.len() >= MAX_SVG_BYTES {
            break;
        }
        if let (Some(x), Some(y)) = (nm_to_mm(j, "x_nm"), nm_to_mm(j, "y_nm")) {
            let _ = write!(&mut svg, r#"<circle cx="{:.3}" cy="{:.3}" r="{:.3}"/>"#, x, y, j_rad);
            elem_count += 1;
        }
    }
    svg.push_str("</g>");

    // 3. Symbols (body bounds derived from pins, plus pin stubs & labels)
    svg.push_str(r#"<g font-family="ui-monospace, monospace">"#);
    for sym in &symbols {
        if elem_count >= MAX_SVG_ELEMENTS_COUNT || svg.len() >= MAX_SVG_BYTES {
            break;
        }
        let cx = nm_to_mm(sym, "x_nm").unwrap_or(0.0);
        let cy = nm_to_mm(sym, "y_nm").unwrap_or(0.0);
        let mut min_px = cx - 2.5;
        let mut max_px = cx + 2.5;
        let mut min_py = cy - 2.0;
        let mut max_py = cy + 2.0;

        let pins = sym.get("pins").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        if !pins.is_empty() {
            min_px = f64::INFINITY;
            max_px = f64::NEG_INFINITY;
            min_py = f64::INFINITY;
            max_py = f64::NEG_INFINITY;
            for p in &pins {
                if let (Some(px), Some(py)) = (nm_to_mm(p, "x_nm"), nm_to_mm(p, "y_nm")) {
                    min_px = min_px.min(px);
                    max_px = max_px.max(px);
                    min_py = min_py.min(py);
                    max_py = max_py.max(py);
                }
            }
            if (max_px - min_px) < 3.0 {
                min_px -= 1.8;
                max_px += 1.8;
            }
            if (max_py - min_py) < 3.0 {
                min_py -= 1.5;
                max_py += 1.5;
            }
        }

        let inset_x = ((max_px - min_px) * 0.18).min(2.2);
        let inset_y = ((max_py - min_py) * 0.14).min(1.8);
        let bx = (min_px + inset_x).min(cx - 1.2);
        let bw = ((max_px - inset_x) - bx).max(2.4);
        let by = (min_py + inset_y).min(cy - 1.0);
        let bh = ((max_py - inset_y) - by).max(2.0);
        let b_cx = bx + bw / 2.0;
        let b_cy = by + bh / 2.0;

        let _ = write!(
            &mut svg,
            r#"<rect x="{:.3}" y="{:.3}" width="{:.3}" height="{:.3}" fill="var(--surface-color,#18191e)" fill-opacity="0.72" stroke="var(--accent-color,#f59e0b)" stroke-width="{:.3}"/>"#,
            bx, by, bw, bh, stroke_thin
        );
        elem_count += 1;

        for p in &pins {
            if elem_count >= MAX_SVG_ELEMENTS_COUNT || svg.len() >= MAX_SVG_BYTES {
                break;
            }
            let (Some(px), Some(py)) = (nm_to_mm(p, "x_nm"), nm_to_mm(p, "y_nm")) else {
                continue;
            };
            let anchor_x = px.clamp(bx, bx + bw);
            let anchor_y = py.clamp(by, by + bh);
            let _ = write!(
                &mut svg,
                r#"<line x1="{:.3}" y1="{:.3}" x2="{:.3}" y2="{:.3}" stroke="var(--fg-color,#e4e4e7)" stroke-width="{:.3}"/>"#,
                anchor_x, anchor_y, px, py, stroke_thin * 0.85
            );
            let _ = write!(
                &mut svg,
                r#"<circle cx="{:.3}" cy="{:.3}" r="{:.3}" fill="var(--accent-color,#f59e0b)"/>"#,
                px, py, stroke_thin * 1.35
            );
            elem_count += 2;
        }

        let ref_txt = xml_escape(
            sym.get("display_reference")
                .or_else(|| sym.get("reference"))
                .and_then(|v| v.as_str())
                .unwrap_or(""),
        );
        let val_txt = xml_escape(sym.get("value").and_then(|v| v.as_str()).unwrap_or(""));
        if !ref_txt.is_empty() {
            let _ = write!(
                &mut svg,
                r#"<text x="{:.3}" y="{:.3}" fill="var(--accent-color,#f59e0b)" font-size="{:.3}" font-weight="700" text-anchor="middle">{}</text>"#,
                b_cx,
                by - font_sm * 0.35,
                font_sm,
                ref_txt
            );
            elem_count += 1;
        }
        if !val_txt.is_empty() {
            let _ = write!(
                &mut svg,
                r#"<text x="{:.3}" y="{:.3}" fill="var(--dim-color,#a1a1aa)" font-size="{:.3}" text-anchor="middle">{}</text>"#,
                b_cx,
                b_cy + font_sm * 0.35,
                font_sm * 0.82,
                val_txt
            );
            elem_count += 1;
        }
    }
    svg.push_str("</g>");

    // 4. Net Labels
    svg.push_str(r#"<g font-family="ui-monospace, monospace">"#);
    for nl in &net_labels {
        if elem_count >= MAX_SVG_ELEMENTS_COUNT || svg.len() >= MAX_SVG_BYTES {
            break;
        }
        let Some(at) = nl.get("at") else { continue };
        let (Some(x), Some(y)) = (nm_to_mm(at, "x_nm"), nm_to_mm(at, "y_nm")) else {
            continue;
        };
        let label = xml_escape(nl.get("name").and_then(|v| v.as_str()).unwrap_or(""));
        if label.is_empty() {
            continue;
        }
        let _ = write!(
            &mut svg,
            r##"<circle cx="{:.3}" cy="{:.3}" r="{:.3}" fill="#38bdf8"/>"##,
            x, y, stroke_thin * 1.4
        );
        let _ = write!(
            &mut svg,
            r##"<text x="{:.3}" y="{:.3}" fill="#38bdf8" font-size="{:.3}" font-weight="600">{}</text>"##,
            x + font_sm * 0.35,
            y - font_sm * 0.25,
            font_sm * 0.85,
            label
        );
        elem_count += 2;
    }
    svg.push_str("</g></svg>");

    Ok((title, svg, elem_count))
}

fn render_board_svg(inspect_raw: &Value) -> Result<(String, String, usize), PcbcraftError> {
    let project = inspect_raw.get("project").unwrap_or(inspect_raw);
    let boards = project
        .get("boards")
        .and_then(|v| v.as_array())
        .ok_or_else(|| PcbcraftError::McpTransport {
            details: "Missing boards array in inspect_project output".to_string(),
        })?;
    let first_board = boards.first().ok_or_else(|| PcbcraftError::InvalidSetting {
        key: "mode",
        details: "This PCBCraft project has no physical board yet".to_string(),
    })?;

    let title = first_board.get("name").and_then(|v| v.as_str()).unwrap_or("Board").to_string();
    let board_obj = project.get("board").unwrap_or(&Value::Null);

    // Map copper layers (front vs back) and outline layer ids.
    let mut layer_purpose: HashMap<&str, (&str, &str)> = HashMap::new();
    if let Some(layers) = board_obj
        .get("setup")
        .and_then(|s| s.get("layers"))
        .and_then(|l| l.as_array())
    {
        for layer in layers {
            if let (Some(id), Some(purpose)) = (
                layer.get("id").and_then(|v| v.as_str()),
                layer.get("purpose").and_then(|v| v.as_str()),
            ) {
                let side = layer.get("side").and_then(|v| v.as_str()).unwrap_or("");
                layer_purpose.insert(id, (purpose, side));
            }
        }
    }

    let drawings = first_board.get("drawings").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let tracks = first_board.get("tracks").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let vias = first_board.get("vias").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let footprints = board_obj
        .get("footprints")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut bounds = BoundsNm::default();
    for d in &drawings {
        if let Some(pts) = d.get("points").and_then(|v| v.as_array()) {
            for pt in pts {
                if let (Some(x), Some(y)) = (pt.get("x_nm").and_then(|v| v.as_f64()), pt.get("y_nm").and_then(|v| v.as_f64())) {
                    bounds.include(x, y);
                }
            }
        }
    }
    for t in &tracks {
        for end_key in ["start", "end"] {
            if let Some(p) = t.get(end_key) {
                if let (Some(x), Some(y)) = (p.get("x_nm").and_then(|v| v.as_f64()), p.get("y_nm").and_then(|v| v.as_f64())) {
                    bounds.include(x, y);
                }
            }
        }
    }
    for fp in &footprints {
        if let (Some(x), Some(y)) = (fp.get("x_nm").and_then(|v| v.as_f64()), fp.get("y_nm").and_then(|v| v.as_f64())) {
            bounds.include(x, y);
        }
        if let Some(pins) = fp.get("pins").and_then(|v| v.as_array()) {
            for pin in pins {
                if let (Some(x), Some(y)) = (pin.get("x_nm").and_then(|v| v.as_f64()), pin.get("y_nm").and_then(|v| v.as_f64())) {
                    bounds.include(x, y);
                }
            }
        }
    }

    let (vx, vy, vw, vh) = bounds.padded_mm_viewbox(3.5);
    let thin = (vw.max(vh) * 0.003).clamp(0.08, 0.25);
    let font_ref = (vw.max(vh) * 0.024).clamp(0.8, 1.8);

    let mut svg = String::with_capacity(12288);
    let mut elem_count = 0usize;
    let _ = write!(
        &mut svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{:.3} {:.3} {:.3} {:.3}" class="oneship-pcb-svg" style="background:var(--bg-color,#0b0c0e)">"#,
        vx, vy, vw, vh
    );

    // 1. Board Drawings / Edge Cuts
    for d in &drawings {
        if elem_count >= MAX_SVG_ELEMENTS_COUNT || svg.len() >= MAX_SVG_BYTES {
            break;
        }
        let Some(pts) = d.get("points").and_then(|v| v.as_array()) else { continue };
        if pts.len() < 2 {
            continue;
        }
        let closed = d.get("closed").and_then(|v| v.as_bool()).unwrap_or(false);
        let layer_id = d.get("layer").and_then(|v| v.as_str()).unwrap_or("");
        let is_outline = layer_purpose.get(layer_id).is_some_and(|(p, _)| *p == "outline");
        let mut pts_str = String::new();
        for pt in pts {
            if let (Some(x), Some(y)) = (nm_to_mm(pt, "x_nm"), nm_to_mm(pt, "y_nm")) {
                let _ = write!(&mut pts_str, "{:.3},{:.3} ", x, y);
            }
        }
        if is_outline && closed {
            let _ = write!(
                &mut svg,
                r##"<polygon points="{}" fill="#141a17" fill-opacity="0.7" stroke="#eab308" stroke-width="{:.3}"/>"##,
                pts_str.trim_end(),
                thin * 1.4
            );
        } else {
            let _ = write!(
                &mut svg,
                r#"<polyline points="{}" fill="none" stroke="var(--dim-color,#71717a)" stroke-width="{:.3}"/>"#,
                pts_str.trim_end(),
                thin
            );
        }
        elem_count += 1;
    }

    // 2. Copper Tracks (back then front so top copper stays crisp)
    for target_side in ["back", "front", ""] {
        let color = match target_side {
            "back" => "#3b82f6",
            "front" => "#ef4444",
            _ => "#f97316",
        };
        let _ = write!(&mut svg, r#"<g stroke="{}" stroke-linecap="round">"#, color);
        for tr in &tracks {
            if elem_count >= MAX_SVG_ELEMENTS_COUNT || svg.len() >= MAX_SVG_BYTES {
                break;
            }
            let layer_id = tr.get("layer").and_then(|v| v.as_str()).unwrap_or("");
            let side = layer_purpose.get(layer_id).map(|(_, s)| *s).unwrap_or("");
            if side != target_side {
                continue;
            }
            let (Some(start), Some(end)) = (tr.get("start"), tr.get("end")) else { continue };
            let (Some(x1), Some(y1)) = (nm_to_mm(start, "x_nm"), nm_to_mm(start, "y_nm")) else { continue };
            let (Some(x2), Some(y2)) = (nm_to_mm(end, "x_nm"), nm_to_mm(end, "y_nm")) else { continue };
            let w_mm = tr.get("width_nm").and_then(|v| v.as_f64()).map(|n| n / 1_000_000.0).unwrap_or(0.2).max(0.08);
            let _ = write!(
                &mut svg,
                r#"<line x1="{:.3}" y1="{:.3}" x2="{:.3}" y2="{:.3}" stroke-width="{:.3}"/>"#,
                x1, y1, x2, y2, w_mm
            );
            elem_count += 1;
        }
        svg.push_str("</g>");
    }

    // 3. Footprints & Pads
    svg.push_str(r#"<g font-family="ui-monospace, monospace">"#);
    for fp in &footprints {
        if elem_count >= MAX_SVG_ELEMENTS_COUNT || svg.len() >= MAX_SVG_BYTES {
            break;
        }
        let fx = nm_to_mm(fp, "x_nm").unwrap_or(0.0);
        let fy = nm_to_mm(fp, "y_nm").unwrap_or(0.0);
        let side = fp.get("side").and_then(|v| v.as_str()).unwrap_or("front");
        let pad_color = if side == "back" { "#60a5fa" } else { "#f59e0b" };

        let pins = fp.get("pins").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let mut min_px = fx - 0.8;
        let mut max_px = fx + 0.8;
        let mut min_py = fy - 0.6;
        let mut max_py = fy + 0.6;
        for pin in &pins {
            if let (Some(px), Some(py)) = (nm_to_mm(pin, "x_nm"), nm_to_mm(pin, "y_nm")) {
                min_px = min_px.min(px - 0.45);
                max_px = max_px.max(px + 0.45);
                min_py = min_py.min(py - 0.45);
                max_py = max_py.max(py + 0.45);
            }
        }

        let _ = write!(
            &mut svg,
            r#"<rect x="{:.3}" y="{:.3}" width="{:.3}" height="{:.3}" fill="none" stroke="var(--fg-color,#e4e4e7)" stroke-opacity="0.45" stroke-width="{:.3}"/>"#,
            min_px,
            min_py,
            (max_px - min_px).max(0.8),
            (max_py - min_py).max(0.8),
            thin * 0.8
        );
        elem_count += 1;

        for pin in &pins {
            if elem_count >= MAX_SVG_ELEMENTS_COUNT || svg.len() >= MAX_SVG_BYTES {
                break;
            }
            let (Some(px), Some(py)) = (nm_to_mm(pin, "x_nm"), nm_to_mm(pin, "y_nm")) else {
                continue;
            };
            let _ = write!(
                &mut svg,
                r#"<rect x="{:.3}" y="{:.3}" width="0.75" height="0.75" rx="0.12" fill="{}" fill-opacity="0.9"/>"#,
                px - 0.375,
                py - 0.375,
                pad_color
            );
            elem_count += 1;
        }

        let ref_txt = xml_escape(
            fp.get("reference_appearance")
                .and_then(|r| r.get("text"))
                .and_then(|v| v.as_str())
                .unwrap_or(""),
        );
        if !ref_txt.is_empty() {
            let _ = write!(
                &mut svg,
                r#"<text x="{:.3}" y="{:.3}" fill="var(--fg-color,#f4f4f5)" font-size="{:.3}" font-weight="700" text-anchor="middle">{}</text>"#,
                fx,
                min_py - thin * 1.4,
                font_ref * 0.72,
                ref_txt
            );
            elem_count += 1;
        }
    }
    svg.push_str("</g>");

    // 4. Vias
    svg.push_str("<g>");
    for v in &vias {
        if elem_count >= MAX_SVG_ELEMENTS_COUNT || svg.len() >= MAX_SVG_BYTES {
            break;
        }
        let Some(at) = v.get("at") else { continue };
        let (Some(vx_mm), Some(vy_mm)) = (nm_to_mm(at, "x_nm"), nm_to_mm(at, "y_nm")) else { continue };
        let outer_r = (v.get("diameter_nm").and_then(|x| x.as_f64()).unwrap_or(600_000.0) / 2_000_000.0).max(0.15);
        let drill_r = (v.get("drill_nm").and_then(|x| x.as_f64()).unwrap_or(300_000.0) / 2_000_000.0).max(0.08);
        let _ = write!(&mut svg, r##"<circle cx="{:.3}" cy="{:.3}" r="{:.3}" fill="#a1a1aa"/>"##, vx_mm, vy_mm, outer_r);
        let _ = write!(&mut svg, r#"<circle cx="{:.3}" cy="{:.3}" r="{:.3}" fill="var(--bg-color,#0b0c0e)"/>"#, vx_mm, vy_mm, drill_r);
        elem_count += 2;
    }
    svg.push_str("</g></svg>");

    Ok((title, svg, elem_count))
}

/// Renders a native ONE-SHIP SVG vector preview for either `"schematic"` or `"board"`.
pub async fn render_pcbcraft_svg_preview(
    project_path: String,
    target_name: Option<String>,
    sheet_id: Option<String>,
    mode: Option<String>,
) -> Result<PcbcraftSvgPreviewResult, PcbcraftError> {
    let project_path = require_registered_project(&project_path)?;
    if !is_project_pcbcraft_enabled(Path::new(&project_path)) {
        return Err(PcbcraftError::PluginDisabled { project_path });
    }

    let resolved_mode = match mode.as_deref().unwrap_or("schematic").trim() {
        "" | "schematic" => "schematic".to_string(),
        "board" => "board".to_string(),
        other => {
            return Err(PcbcraftError::InvalidSetting {
                key: "mode",
                details: format!("Unsupported preview mode '{}' (expected 'schematic' or 'board')", other),
            });
        }
    };

    let cli_bin = resolve_cli_binary(&project_path)?;
    let pcb_dir = resolve_pcb_dir(&project_path)?;
    let targets = list_pcb_targets(&pcb_dir).await;
    let (target, target_path) = validate_target_name(target_name, &targets, &pcb_dir)?;
    if !tokio::fs::try_exists(&target_path).await.unwrap_or(false) {
        return Err(PcbcraftError::InvalidSetting {
            key: "target_name",
            details: format!("PCBCraft target '{}' does not exist", target_path.display()),
        });
    }

    let modified_ms = read_target_modified_ms(&target_path).await;
    let cache_key = (target_path.clone(), resolved_mode.clone(), sheet_id.clone());
    let disk_preview_key = format!(
        "svg-v2|{}|{}|{}",
        target_path.display(),
        resolved_mode,
        sheet_id.as_deref().unwrap_or("")
    );
    if modified_ms > 0 {
        let cache = runtime_pool().preview_cache.lock().await;
        if let Some(cached) = cache.get(&cache_key) {
            if cached.modified_ms == modified_ms {
                return Ok(cached.result.clone());
            }
        }
        drop(cache);
        if let Some(disk_result) = crate::kopeng::preview_cache::read_cached::<PcbcraftSvgPreviewResult>(
            "pcbcraft",
            &disk_preview_key,
            modified_ms,
        )
        .await
        {
            let mut cache = runtime_pool().preview_cache.lock().await;
            cache.insert(
                cache_key,
                CachedPreviewEntry {
                    modified_ms,
                    result: disk_result.clone(),
                },
            );
            return Ok(disk_result);
        }
    }

    let timeout_ms = resolve_mcp_timeout_ms(&project_path);

    let (title, svg, elements_count) = if resolved_mode == "board" {
        let (inspect_raw, _) = load_cached_snapshot(&cli_bin, &target_path, timeout_ms).await?;
        render_board_svg(&inspect_raw)?
    } else {
        match render_native_schematic_svg_via_cli(&cli_bin, &target_path, sheet_id.as_deref(), timeout_ms).await {
            Ok(rendered) => rendered,
            Err(e) => {
                crate::logger::log("WARN", "pcbcraft", &format!("Native CLI schematic SVG render failed ({}), using fallback renderer", e));
                let (inspect_raw, _) = load_cached_snapshot(&cli_bin, &target_path, timeout_ms).await?;
                render_schematic_svg(&inspect_raw, sheet_id.as_deref())?
            }
        }
    };

    let rendered_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let result = PcbcraftSvgPreviewResult {
        target_name: target,
        mode: resolved_mode,
        sheet_id,
        title,
        elements_count,
        svg,
        rendered_at_ms,
    };

    if modified_ms > 0 {
        let mut cache = runtime_pool().preview_cache.lock().await;
        if cache.len() >= MAX_PREVIEW_CACHE_ITEMS_COUNT && !cache.contains_key(&cache_key) {
            if let Some(evict) = cache.keys().next().cloned() {
                cache.remove(&evict);
            }
        }
        cache.insert(
            cache_key,
            CachedPreviewEntry {
                modified_ms,
                result: result.clone(),
            },
        );
        drop(cache);
        crate::kopeng::preview_cache::write_cached(
            "pcbcraft",
            &disk_preview_key,
            modified_ms,
            &result,
        )
        .await;
    }

    Ok(result)
}

async fn stop_pcbcraft_mcp_process(project_path: &str) {
    let entry = runtime_pool().mcp_processes.lock().await.remove(project_path);
    if let Some(proc_arc) = entry {
        let mut proc = proc_arc.lock().await;
        let _ = proc.child.kill().await;
    }
}

fn find_live_editor_endpoint(target_path: &Path) -> Option<LiveEditorEndpoint> {
    let dir = std::env::var_os("ONE_SHIP_PCBCRAFT_LIVE_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".cache/one-ship/pcbcraft-live")))?;
    let entries = std::fs::read_dir(&dir).ok()?;
    let canonical_target = std::fs::canonicalize(target_path).unwrap_or_else(|_| target_path.to_path_buf());

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(meta) = serde_json::from_str::<LiveEditorEndpoint>(&raw) else {
            continue;
        };
        let proc_exists = Path::new("/proc").join(meta.pid.to_string()).exists();
        if !proc_exists || !meta.token_file.exists() {
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_file(&meta.token_file);
            continue;
        }
        let canonical_meta = std::fs::canonicalize(&meta.project_path).unwrap_or_else(|_| meta.project_path.clone());
        let matches_target = canonical_meta == canonical_target
            || (canonical_meta.file_name().and_then(|n| n.to_str()) == Some("project.json")
                && canonical_meta.parent() == Some(canonical_target.as_path()));
        if matches_target {
            return Some(meta);
        }
    }
    None
}

async fn spawn_mcp_process(
    project_path: &str,
    target_path: &Path,
    live: Option<&LiveEditorEndpoint>,
) -> Result<PcbcraftMcpProcess, PcbcraftError> {
    let cli_bin = resolve_cli_binary(project_path)?;
    let mut cmd = Command::new(&cli_bin);
    cmd.arg("mcp");
    let live_port = if let Some(endpoint) = live {
        cmd.arg("--connect")
            .arg(endpoint.port.to_string())
            .arg("--token-file")
            .arg(&endpoint.token_file);
        Some(endpoint.port)
    } else {
        cmd.arg("--project").arg(target_path);
        None
    };

    let mut child = cmd
        .current_dir(project_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| PcbcraftError::McpTransport {
            details: format!("Failed to spawn '{}': {}", cli_bin.display(), e),
        })?;

    let stdin = child.stdin.take().ok_or_else(|| PcbcraftError::McpTransport {
        details: "pcbcraft-cli child stdin unavailable".to_string(),
    })?;
    let stdout = child.stdout.take().ok_or_else(|| PcbcraftError::McpTransport {
        details: "pcbcraft-cli child stdout unavailable".to_string(),
    })?;

    Ok(PcbcraftMcpProcess {
        target_path: target_path.to_path_buf(),
        live_port,
        child,
        stdin,
        stdout: BufReader::new(stdout),
    })
}

async fn send_stdio_jsonrpc(
    proc: &mut PcbcraftMcpProcess,
    payload: &Value,
    timeout_ms: u64,
    expect_response: bool,
) -> Result<Option<Value>, PcbcraftError> {
    let mut line = serde_json::to_string(payload).map_err(|e| PcbcraftError::McpTransport {
        details: format!("Cannot serialize JSON-RPC payload: {}", e),
    })?;
    line.push('\n');

    proc.stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|e| PcbcraftError::McpTransport {
            details: format!("Failed writing to pcbcraft-cli stdin: {}", e),
        })?;
    proc.stdin.flush().await.map_err(|e| PcbcraftError::McpTransport {
        details: format!("Failed flushing pcbcraft-cli stdin: {}", e),
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
    .map_err(|_| PcbcraftError::McpTransport {
        details: format!("Timed out after {} ms waiting for pcbcraft-cli response", timeout_ms),
    })?;

    let bytes_read = read_res.map_err(|e| PcbcraftError::McpTransport {
        details: format!("Failed reading from pcbcraft-cli stdout: {}", e),
    })?;
    if bytes_read == 0 {
        return Err(PcbcraftError::McpTransport {
            details: "pcbcraft-cli closed stdout unexpectedly".to_string(),
        });
    }

    let parsed: Value = serde_json::from_str(response_line.trim()).map_err(|e| PcbcraftError::McpTransport {
        details: format!("Invalid JSON-RPC response from pcbcraft-cli: {}", e),
    })?;
    Ok(Some(parsed))
}

async fn get_or_spawn_mcp_process(
    project_path: &str,
    target_path: &Path,
) -> Result<(Arc<Mutex<PcbcraftMcpProcess>>, bool), PcbcraftError> {
    let target_owned = target_path.to_path_buf();
    let live_endpoint = tokio::task::spawn_blocking(move || find_live_editor_endpoint(&target_owned))
        .await
        .map_err(|e| PcbcraftError::Task(e.to_string()))?;
    let desired_live_port = live_endpoint.as_ref().map(|e| e.port);

    let mut map = runtime_pool().mcp_processes.lock().await;
    if let Some(existing) = map.get(project_path).cloned() {
        let mut guard = existing.lock().await;
        if guard.target_path == target_path
            && guard.live_port == desired_live_port
            && matches!(guard.child.try_wait(), Ok(None))
        {
            drop(guard);
            return Ok((existing, false));
        }
        let _ = guard.child.kill().await;
        drop(guard);
        map.remove(project_path);
    }

    let proc = spawn_mcp_process(project_path, target_path, live_endpoint.as_ref()).await?;
    let arc = Arc::new(Mutex::new(proc));
    map.insert(project_path.to_string(), arc.clone());
    Ok((arc, true))
}

async fn resolve_pcbcraft_gateway_origin(
    connect_info: Option<ConnectInfo<SocketAddr>>,
    server_port: u16,
    query: &PcbcraftGatewayQuery,
) -> Result<String, PcbcraftError> {
    if let Some(ConnectInfo(peer)) = connect_info {
        let resolved = tokio::task::spawn_blocking(move || {
            crate::caller_identity::resolve_caller(peer, server_port)
        })
        .await
        .map_err(|e| PcbcraftError::Task(e.to_string()))?;

        match resolved {
            Ok(caller) => {
                if let Some(p_path) = caller.project_path {
                    return Ok(p_path);
                }
            }
            Err(crate::caller_identity::CallerError::UnmanagedCaller { .. }) => {}
            Err(e) => return Err(PcbcraftError::ProjectContextMissing { details: e.to_string() }),
        }
    }

    let explicit_path = match query.project_hex.as_deref() {
        Some(hex) => Some(decode_hex_string(hex).ok_or_else(|| PcbcraftError::ProjectContextMissing {
            details: format!("project_hex '{}' is not hex-encoded UTF-8", hex),
        })?),
        None => None,
    };

    if let Some(candidate_path) = explicit_path {
        let projects = crate::db::db_get_projects().map_err(|e| PcbcraftError::ProjectContextMissing {
            details: e.to_string(),
        })?;
        if let Some(verified) = projects.into_iter().find(|p| p.path == candidate_path) {
            return Ok(verified.path);
        }
        return Err(PcbcraftError::ProjectNotFound { path: candidate_path });
    }

    Err(PcbcraftError::ProjectContextMissing {
        details: "Caller is neither an attested ONE-SHIP agent process nor scoped via ?project_hex.".to_string(),
    })
}

fn is_save_tool_call(req: &JsonRpcRequest) -> bool {
    if req.method != "tools/call" {
        return false;
    }
    let Some(params) = req.params.as_ref() else {
        return false;
    };
    let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or_default();
    if tool_name == "save_project" {
        return true;
    }
    tool_name == "execute"
        && params
            .get("arguments")
            .and_then(|a| a.get("command"))
            .and_then(|c| c.as_str())
            == Some("file.save")
}

async fn forward_to_pcbcraft_mcp(
    project_path: &str,
    target_path: &Path,
    req: &JsonRpcRequest,
    req_id: Value,
    is_notification: bool,
    timeout_ms: u64,
) -> Result<JsonRpcResponse, PcbcraftError> {
    let saving = is_save_tool_call(req);
    let target_name = target_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("pcb")
        .to_string();
    if saving {
        if let Ok(rel) = target_path.strip_prefix(Path::new(project_path)) {
            let rel_str = rel.to_string_lossy().to_string();
            if let Err(e) = crate::kopeng::snapshots::try_create_auto_domain_snapshot(
                PathBuf::from(project_path),
                "pcbcraft",
                target_name.clone(),
                "pcbcraft-mcp".to_string(),
                format!("pre-save: {}", target_name),
                vec![rel_str],
            )
            .await
            {
                crate::logger::log("WARN", "pcbcraft", &format!("Pre-save auto-snapshot failed: {}", e));
            }
        }
        runtime_pool()
            .recent_agent_saves
            .lock()
            .await
            .insert((project_path.to_string(), target_name.clone()), std::time::Instant::now());
    }

    let (proc_arc, just_spawned) = get_or_spawn_mcp_process(project_path, target_path).await?;
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
            stop_pcbcraft_mcp_process(project_path).await;
            return Err(e);
        }
        let notif = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        if let Err(e) = send_stdio_jsonrpc(&mut proc, &notif, timeout_ms, false).await {
            drop(proc);
            stop_pcbcraft_mcp_process(project_path).await;
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
        Ok(Some(val)) => {
            if saving {
                runtime_pool().snapshot_cache.lock().await.remove(target_path);
            }
            Ok(JsonRpcResponse {
                jsonrpc: "2.0",
                id: val.get("id").cloned().unwrap_or(req_id),
                result: val.get("result").cloned(),
                error: val.get("error").cloned(),
            })
        }
        Ok(None) => Ok(JsonRpcResponse {
            jsonrpc: "2.0",
            id: Value::Null,
            result: Some(json!({ "status": "ok" })),
            error: None,
        }),
        Err(e) => {
            drop(proc);
            stop_pcbcraft_mcp_process(project_path).await;
            Err(e)
        }
    }
}

/// Axum HTTP POST handler mounted at `/mcp/pcbcraft` on ONE-SHIP's MCP daemon.
/// Routes MCP requests to a per-project `pcbcraft-cli mcp --project <target>` stdio child process.
pub async fn pcbcraft_mcp_gateway_handler(
    connect_info: Option<ConnectInfo<SocketAddr>>,
    Query(query): Query<PcbcraftGatewayQuery>,
    Json(req): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let is_notification = req.id.is_none();
    let req_id = req.id.clone().unwrap_or(Value::Null);
    let mcp_port = crate::config::mcp_daemon_port();

    let project_path = match resolve_pcbcraft_gateway_origin(connect_info, mcp_port, &query).await {
        Ok(p) => p,
        Err(e) => {
            crate::logger::log("WARN", "pcbcraft", &format!("[PCBCRAFT GATEWAY REJECTED] {}", e));
            return Json(JsonRpcResponse {
                jsonrpc: "2.0",
                id: req_id,
                result: None,
                error: Some(json!({ "code": -32001, "message": e.to_string() })),
            });
        }
    };

    if !is_project_pcbcraft_enabled(Path::new(&project_path)) {
        return Json(JsonRpcResponse {
            jsonrpc: "2.0",
            id: req_id,
            result: None,
            error: Some(json!({
                "code": -32002,
                "message": format!("PCBCraft plugin is disabled for project '{}'", project_path),
            })),
        });
    }

    let pcb_dir = match resolve_pcb_dir(&project_path) {
        Ok(d) => d,
        Err(e) => {
            return Json(JsonRpcResponse {
                jsonrpc: "2.0",
                id: req_id,
                result: None,
                error: Some(json!({ "code": -32602, "message": e.to_string() })),
            });
        }
    };

    let targets = list_pcb_targets(&pcb_dir).await;
    let target_path = match validate_target_name(query.target.clone(), &targets, &pcb_dir) {
        Ok((_, path)) => path,
        Err(e) => {
            return Json(JsonRpcResponse {
                jsonrpc: "2.0",
                id: req_id,
                result: None,
                error: Some(json!({ "code": -32602, "message": e.to_string() })),
            });
        }
    };

    let timeout_ms = resolve_mcp_timeout_ms(&project_path);
    match forward_to_pcbcraft_mcp(&project_path, &target_path, &req, req_id.clone(), is_notification, timeout_ms).await {
        Ok(resp) => Json(resp),
        Err(e) => Json(JsonRpcResponse {
            jsonrpc: "2.0",
            id: req_id,
            result: None,
            error: Some(json!({ "code": -32603, "message": e.to_string() })),
        }),
    }
}

/// Extracts a normalized pure-Rust netlist summary keyed by net name for future SPICE bridge reuse.
pub fn extract_net_pin_map(inspect_raw: &Value) -> BTreeMap<String, Vec<(String, String)>> {
    let mut map: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    let project = inspect_raw.get("project").unwrap_or(inspect_raw);
    let mut comp_by_sym: HashMap<String, String> = HashMap::new();
    if let Some(sheets) = project.get("sheets").and_then(|v| v.as_array()) {
        for sheet in sheets {
            if let Some(syms) = sheet
                .get("schematic")
                .and_then(|s| s.get("symbols"))
                .and_then(|v| v.as_array())
            {
                for sym in syms {
                    if let (Some(sid), Some(ref_name)) = (
                        sym.get("id").and_then(|v| v.as_str()),
                        sym.get("display_reference").or_else(|| sym.get("reference")).and_then(|v| v.as_str()),
                    ) {
                        comp_by_sym.insert(sid.to_string(), ref_name.to_string());
                    }
                }
            }
        }
    }

    if let Some(nets) = project
        .get("netlist")
        .and_then(|nl| nl.get("nets"))
        .and_then(|v| v.as_array())
    {
        for net in nets {
            let Some(net_name) = net.get("name").and_then(|v| v.as_str()) else { continue };
            let entry = map.entry(net_name.to_string()).or_default();
            if let Some(terms) = net.get("terminals").and_then(|v| v.as_array()) {
                for t in terms {
                    let sym_id = t.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
                    let pin = t.get("terminal").and_then(|v| v.as_str()).unwrap_or("");
                    let comp_ref = comp_by_sym
                        .get(sym_id)
                        .cloned()
                        .unwrap_or_else(|| sym_id.to_string());
                    entry.push((comp_ref, pin.to_string()));
                }
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_inspect_json() -> Value {
        json!({
            "revision": 42,
            "project": {
                "name": "Sensor Test",
                "components": [{
                    "id": "component-2",
                    "reference": "U1",
                    "value": "ATtiny402",
                    "definition": "attiny402",
                    "metadata": {
                        "attributes": {
                            "exclude_from_bom": false,
                            "exclude_from_board": false,
                            "exclude_from_simulation": false
                        }
                    }
                }],
                "sheets": [{
                    "id": "sheet-1",
                    "name": "Main",
                    "file": "sheets/main.json",
                    "schematic": {
                        "symbols": [{
                            "id": "symbol-2",
                            "display_reference": "U1",
                            "value": "ATtiny402",
                            "x_nm": 0,
                            "y_nm": 0,
                            "pins": [
                                { "name": "VDD", "number": "1", "x_nm": -5_000_000, "y_nm": -2_000_000 },
                                { "name": "GND", "number": "8", "x_nm": 5_000_000, "y_nm": 2_000_000 }
                            ]
                        }],
                        "wires": [{
                            "id": "wire-1",
                            "path": [
                                { "x_nm": -5_000_000, "y_nm": -2_000_000 },
                                { "x_nm": -10_000_000, "y_nm": -2_000_000 }
                            ]
                        }],
                        "junctions": [{ "id": "j-1", "x_nm": -10_000_000, "y_nm": -2_000_000 }],
                        "net_labels": [{ "name": "+3V3", "at": { "x_nm": -10_000_000, "y_nm": -2_000_000 } }]
                    }
                }],
                "boards": [{
                    "id": "board-1",
                    "name": "Board",
                    "file": "boards/b1/board.json",
                    "drawings": [{
                        "layer": "l-outline",
                        "closed": true,
                        "points": [
                            { "x_nm": -10_000_000, "y_nm": -10_000_000 },
                            { "x_nm": 10_000_000, "y_nm": -10_000_000 },
                            { "x_nm": 10_000_000, "y_nm": 10_000_000 },
                            { "x_nm": -10_000_000, "y_nm": 10_000_000 }
                        ]
                    }],
                    "tracks": [{
                        "layer": "l-top",
                        "width_nm": 250_000,
                        "start": { "x_nm": -2_000_000, "y_nm": 0 },
                        "end": { "x_nm": 2_000_000, "y_nm": 0 }
                    }],
                    "vias": [{
                        "at": { "x_nm": 2_000_000, "y_nm": 0 },
                        "diameter_nm": 600_000,
                        "drill_nm": 300_000
                    }]
                }],
                "board": {
                    "setup": {
                        "layers": [
                            { "id": "l-outline", "purpose": "outline" },
                            { "id": "l-top", "purpose": "copper", "side": "front" }
                        ]
                    },
                    "footprints": [{
                        "component": "component-2",
                        "definition": "soic8",
                        "side": "front",
                        "x_nm": 0,
                        "y_nm": 0,
                        "reference_appearance": { "text": "U1" },
                        "pins": [{ "x_nm": -2_000_000, "y_nm": 0 }]
                    }]
                },
                "netlist": {
                    "nets": [{
                        "name": "+3V3",
                        "terminals": [{ "symbol": "symbol-2", "terminal": "1" }],
                        "junctions": ["j-1"]
                    }]
                }
            }
        })
    }

    #[test]
    fn xml_escape_sanitizes_reserved_entities() {
        assert_eq!(xml_escape("<R1 & \"3.3V\">"), "&lt;R1 &amp; &quot;3.3V&quot;&gt;");
    }

    #[test]
    fn validate_target_name_rejects_traversal() {
        let dir = Path::new("/tmp/pcb");
        let avail = vec!["sensor-node".to_string()];
        assert_eq!(
            validate_target_name(None, &avail, dir).unwrap().0,
            "sensor-node"
        );
        assert!(validate_target_name(Some("../secret".to_string()), &avail, dir).is_err());
        assert!(validate_target_name(Some("-flag".to_string()), &avail, dir).is_err());
    }

    #[test]
    fn parse_inspection_and_render_both_svg_modes() {
        let raw = sample_inspect_json();
        let drc = json!({ "design_check": { "total": 0, "violations": [] } });
        let inspection = parse_pcbcraft_inspection("sensor-node".to_string(), &raw, &drc);
        assert_eq!(inspection.project_name, "Sensor Test");
        assert_eq!(inspection.components.len(), 1);
        assert_eq!(inspection.components[0].footprint.as_deref(), Some("soic8"));
        assert_eq!(inspection.nets.len(), 1);
        assert_eq!(inspection.drc_status, "pass");

        let (sch_title, sch_svg, sch_elems) = render_schematic_svg(&raw, None).unwrap();
        assert_eq!(sch_title, "Main");
        assert!(sch_svg.contains("<svg "));
        assert!(sch_svg.contains("ATtiny402"));
        assert!(sch_svg.contains("+3V3"));
        assert!(sch_elems > 0);

        let (brd_title, brd_svg, brd_elems) = render_board_svg(&raw).unwrap();
        assert_eq!(brd_title, "Board");
        assert!(brd_svg.contains("<polygon "));
        assert!(brd_svg.contains("<line "));
        assert!(brd_svg.contains("<circle "));
        assert!(brd_elems > 0);

        let net_pins = extract_net_pin_map(&raw);
        assert_eq!(net_pins.get("+3V3").unwrap(), &vec![("U1".to_string(), "1".to_string())]);
    }
}
