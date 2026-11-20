//! SolidCraft 3D parametric CAD integration (headless previews, read-only metrics & external launcher).
//!
//! Provides:
//! - Project-scoped `[plugins.solidcraft]` configuration resolution (`rula.toml` + `config.rs`).
//! - Headless PNG snapshot rendering via `solidcraft-cli snapshot` (zero LLM CAD modeling).
//! - Read-only physical metrics & parameter evaluation via `solidcraft-cli eval`.
//! - Native `solidcraft` desktop editor launch/focus on Hyprland via `desktop.rs`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::Mutex;
use ts_rs::TS;

const MAX_PREVIEW_CACHE_ITEMS_COUNT: usize = 32;
const MAX_METRICS_CACHE_ITEMS_COUNT: usize = 32;
const MAX_PREVIEW_PNG_BYTES: usize = 16 * 1024 * 1024;
const ALLOWED_CAD_EXTENSIONS: &[&str] = &["sc", "step", "stp", "3mf"];
const ALLOWED_EVAL_EXTENSIONS: &[&str] = &["sc", "step", "stp"];
const ALLOWED_VIEW_PRESETS: &[&str] = &["iso", "front", "back", "top", "bottom", "left", "right"];

/// Strongly-typed error hierarchy for SolidCraft CAD integration operations.
#[derive(Debug, Error)]
pub enum SolidcraftError {
    #[error("SolidCraft plugin is disabled for project '{project_path}'")]
    PluginDisabled { project_path: String },

    #[error("Invalid SolidCraft setting '{key}': {details}")]
    InvalidSetting { key: &'static str, details: String },

    #[error("SolidCraft binary not found at '{path}'")]
    BinaryNotFound { path: String },

    #[error("SolidCraft process lifecycle error: {details}")]
    ProcessLifecycle { details: String },

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
}

impl crate::api::ApiError for SolidcraftError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::PluginDisabled { .. } | Self::InvalidSetting { .. } => StatusCode::BAD_REQUEST,
            Self::BinaryNotFound { .. } => StatusCode::FAILED_DEPENDENCY,
            Self::Io { .. } | Self::ProcessLifecycle { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Registry(e) => e.status(),
            Self::Config(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::PluginDisabled { .. } => "SolidcraftError::PluginDisabled",
            Self::InvalidSetting { .. } => "SolidcraftError::InvalidSetting",
            Self::BinaryNotFound { .. } => "SolidcraftError::BinaryNotFound",
            Self::ProcessLifecycle { .. } => "SolidcraftError::ProcessLifecycle",
            Self::Io { .. } => "SolidcraftError::Io",
            Self::Registry(_) => "SolidcraftError::Registry",
            Self::Config(_) => "SolidcraftError::Config",
        }
    }
}

fn io_err(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> SolidcraftError {
    let ctx = context.into();
    move |source| SolidcraftError::Io { context: ctx, source }
}

/// Live operational status of the SolidCraft CAD plugin for a project.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct SolidcraftStatus {
    /// Absolute path of the owning project.
    pub project_path: String,
    /// Whether `[plugins.solidcraft] enabled = true` is set in `rula.toml`.
    pub is_enabled: bool,
    /// Resolved absolute path to the project's CAD workspace directory.
    pub cad_dir_path: String,
    /// Number of supported CAD files currently in `cad_dir`.
    pub cad_files_count: usize,
    /// Sorted list of CAD filenames inside `cad_dir`.
    pub cad_files: Vec<String>,
    /// Whether the configured desktop editor binary exists on disk.
    pub app_available: bool,
    /// Whether the configured `solidcraft-cli` binary exists on disk.
    pub cli_available: bool,
    /// Whether a SolidCraft desktop editor window/process is currently running.
    pub is_editor_running: bool,
}

/// Arguments of `get_project_solidcraft_status`.
#[derive(Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct GetProjectSolidcraftStatusArgs {
    pub project_path: String,
}

/// Arguments of `toggle_project_solidcraft_plugin`.
#[derive(Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ToggleProjectSolidcraftPluginArgs {
    pub project_path: String,
    pub enabled: bool,
}

/// Arguments of `launch_project_solidcraft`.
#[derive(Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct LaunchProjectSolidcraftArgs {
    pub project_path: String,
    pub file_name: Option<String>,
}

/// Arguments of `create_solidcraft_file`.
#[derive(Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CreateSolidcraftFileArgs {
    pub project_path: String,
    pub file_name: String,
    pub template_id: Option<String>,
}

/// Arguments of `render_solidcraft_preview`.
#[derive(Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RenderSolidcraftPreviewArgs {
    pub project_path: String,
    pub file_name: Option<String>,
    pub view: Option<String>,
    pub section: Option<String>,
}

/// Rendered PNG preview of a `.sc` or `.step` file.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct SolidcraftPreviewResult {
    pub file_name: String,
    pub view: String,
    pub section: Option<String>,
    pub width_px: u32,
    pub height_px: u32,
    pub renderer: String,
    pub png_data_url: String,
    #[ts(type = "number")]
    pub rendered_at_ms: u64,
}

/// Arguments of `inspect_solidcraft_metrics`.
#[derive(Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectSolidcraftMetricsArgs {
    pub project_path: String,
    pub file_name: Option<String>,
}

/// Physical metrics for a single solid body evaluated by `solidcraft-cli eval`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct SolidcraftBodyMetrics {
    pub name: String,
    pub material: String,
    pub closed: bool,
    pub volume_mm3: f64,
    pub area_mm2: f64,
    pub mass_g: f64,
    pub density_g_cm3: f64,
    pub faces_count: u64,
    pub edges_count: u64,
    pub vertices_count: u64,
    pub bbox_min_mm: [f64; 3],
    pub bbox_max_mm: [f64; 3],
    pub center_of_mass_mm: [f64; 3],
}

/// Read-only parametric variable reported by `solidcraft-cli eval`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct SolidcraftParameterInfo {
    pub name: String,
    pub expression: String,
    pub value: Option<f64>,
    pub unit: Option<String>,
}

/// Aggregated physical metrics and parameter inventory for a CAD model.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct SolidcraftMetricsResult {
    pub file_name: String,
    pub bodies_count: usize,
    pub total_volume_mm3: f64,
    pub total_area_mm2: f64,
    pub total_mass_g: f64,
    pub bodies: Vec<SolidcraftBodyMetrics>,
    pub params: Vec<SolidcraftParameterInfo>,
    #[ts(type = "number")]
    pub evaluated_at_ms: u64,
}

#[derive(Clone)]
struct CachedPreviewEntry {
    modified_ms: u128,
    result: SolidcraftPreviewResult,
}

#[derive(Clone)]
struct CachedMetricsEntry {
    modified_ms: u128,
    result: SolidcraftMetricsResult,
}

struct SolidcraftRuntimePool {
    file_watchers: Mutex<HashMap<String, tokio_util::sync::CancellationToken>>,
    preview_cache: Mutex<HashMap<(PathBuf, String, Option<String>, u32, u32), CachedPreviewEntry>>,
    metrics_cache: Mutex<HashMap<PathBuf, CachedMetricsEntry>>,
}

static SOLIDCRAFT_POOL: OnceLock<SolidcraftRuntimePool> = OnceLock::new();

fn runtime_pool() -> &'static SolidcraftRuntimePool {
    SOLIDCRAFT_POOL.get_or_init(|| SolidcraftRuntimePool {
        file_watchers: Mutex::new(HashMap::new()),
        preview_cache: Mutex::new(HashMap::new()),
        metrics_cache: Mutex::new(HashMap::new()),
    })
}

/// Checks whether `[plugins.solidcraft] enabled = true` is configured in `<project_path>/rula.toml`.
pub fn is_project_solidcraft_enabled(project_path: &Path) -> bool {
    let Some(rula) = crate::config::load_project_rula(project_path) else {
        return false;
    };
    rula.get("plugins")
        .and_then(|p| p.get("solidcraft"))
        .and_then(|s| s.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn require_registered_project(project_path: &str) -> Result<String, SolidcraftError> {
    let project = crate::registry::find_project_by_path(project_path)?;
    Ok(project.path)
}

fn resolve_cad_dir(project_path: &str) -> Result<PathBuf, SolidcraftError> {
    let rel = crate::config::resolve_effective_string(Some(project_path), None, "solidcraft", "cad_dir");
    let trimmed = rel.trim();
    if trimmed.is_empty() || trimmed.contains("..") || Path::new(trimmed).is_absolute() {
        return Err(SolidcraftError::InvalidSetting {
            key: "cad_dir",
            details: format!("'{}' must be a safe relative directory name", rel),
        });
    }
    Ok(Path::new(project_path).join(trimmed))
}

fn resolve_app_binary(project_path: &str) -> Result<PathBuf, SolidcraftError> {
    let path_str = crate::config::resolve_effective_string(Some(project_path), None, "solidcraft", "app_binary_path");
    crate::desktop::resolve_executable(path_str.trim()).ok_or_else(|| SolidcraftError::BinaryNotFound {
        path: path_str.trim().to_string(),
    })
}

fn resolve_cli_binary(project_path: &str) -> Result<PathBuf, SolidcraftError> {
    let path_str = crate::config::resolve_effective_string(Some(project_path), None, "solidcraft", "cli_binary_path");
    crate::desktop::resolve_executable(path_str.trim()).ok_or_else(|| SolidcraftError::BinaryNotFound {
        path: path_str.trim().to_string(),
    })
}

fn resolve_snapshot_timeout_ms(project_path: &str) -> u64 {
    crate::config::resolve_effective_u64(Some(project_path), None, "solidcraft", "snapshot_timeout_ms")
        .clamp(1_000, 120_000)
}

fn resolve_snapshot_dimensions_px(project_path: &str) -> (u32, u32) {
    let width = crate::config::resolve_effective_u64(Some(project_path), None, "solidcraft", "snapshot_width_px")
        .clamp(320, 3840) as u32;
    let height = crate::config::resolve_effective_u64(Some(project_path), None, "solidcraft", "snapshot_height_px")
        .clamp(240, 2160) as u32;
    (width, height)
}

fn has_allowed_extension(path: &Path, allowed: &[&str]) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| allowed.iter().any(|candidate| ext.eq_ignore_ascii_case(candidate)))
}

async fn list_cad_files(cad_dir: &Path) -> Vec<String> {
    let dir = cad_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter(|e| e.path().is_file() && has_allowed_extension(&e.path(), ALLOWED_CAD_EXTENSIONS))
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        names.sort();
        names
    })
    .await
    .unwrap_or_default()
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

async fn stop_cad_dir_watcher(project_path: &str) {
    if let Some(token) = runtime_pool().file_watchers.lock().await.remove(project_path) {
        token.cancel();
    }
}

async fn ensure_cad_dir_watcher(project_path: &str, cad_dir: &Path) {
    if !tokio::fs::try_exists(cad_dir).await.unwrap_or(false) {
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
    let dir = cad_dir.to_path_buf();

    tokio::spawn(async move {
        use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
        let (tx, mut rx) = tokio::sync::mpsc::channel::<PathBuf>(64);

        let watcher_res = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                if let Ok(ev) = res {
                    for p in ev.paths {
                        if has_allowed_extension(&p, ALLOWED_CAD_EXTENSIONS) {
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
                crate::logger::log("WARN", "solidcraft", &format!("Cannot start CAD watcher for {}: {}", proj, e));
                runtime_pool().file_watchers.lock().await.remove(&proj);
                return;
            }
        };

        if let Err(e) = watcher.watch(&dir, RecursiveMode::NonRecursive) {
            crate::logger::log("WARN", "solidcraft", &format!("Cannot watch {}: {}", dir.display(), e));
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
                                "solidcraft",
                                file_name.clone(),
                                "solidcraft".to_string(),
                                format!("cad-edit: {}", file_name),
                                vec![rel_str],
                            )
                            .await
                            {
                                crate::logger::log("WARN", "solidcraft", &format!("Auto-snapshot failed: {}", e));
                            }
                        }

                        let payload = crate::events::SolidcraftFileChangedPayload {
                            project_path: proj.clone(),
                            file_name,
                            modified_ms: modified_ms as u64,
                        };
                        crate::supervisor::get_supervisor().emit_event(
                            crate::events::DaemonEvent::solidcraft_file_changed(&payload),
                        );
                    }
                }
            }
        }
    });
}

fn validate_cad_filename(
    file_name: Option<String>,
    available_files: &[String],
    cad_dir: &Path,
) -> Result<(String, PathBuf), SolidcraftError> {
    let target_name = match file_name {
        Some(name) if !name.trim().is_empty() => {
            if name.contains('/') || name.contains('\\') || name.contains("..") {
                return Err(SolidcraftError::InvalidSetting {
                    key: "file_name",
                    details: "file_name must be a plain filename inside cad_dir".to_string(),
                });
            }
            name
        }
        _ => available_files.first().cloned().ok_or_else(|| SolidcraftError::InvalidSetting {
            key: "file_name",
            details: format!("No CAD files found in {}", cad_dir.display()),
        })?,
    };

    let target_path = cad_dir.join(&target_name);
    Ok((target_name, target_path))
}

fn validate_view_name(view: Option<String>) -> Result<String, SolidcraftError> {
    let resolved = view.unwrap_or_else(|| "iso".to_string());
    let trimmed = resolved.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('-')
        || !ALLOWED_VIEW_PRESETS.iter().any(|v| v.eq_ignore_ascii_case(trimmed))
    {
        return Err(SolidcraftError::InvalidSetting {
            key: "view",
            details: format!(
                "view must be one of: {}",
                ALLOWED_VIEW_PRESETS.join(", ")
            ),
        });
    }
    Ok(trimmed.to_ascii_lowercase())
}

fn validate_section_spec(section: Option<String>) -> Result<Option<String>, SolidcraftError> {
    let Some(raw) = section else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.starts_with('-') || trimmed.len() > 64 || trimmed.contains(['\n', '\r', '\0']) {
        return Err(SolidcraftError::InvalidSetting {
            key: "section",
            details: "section must be a valid plane spec (e.g. XY, XZ, YZ, or z=10)".to_string(),
        });
    }
    Ok(Some(trimmed.to_string()))
}

/// Returns the current SolidCraft CAD plugin status for a registered project.
pub async fn get_project_solidcraft_status(project_path: String) -> Result<SolidcraftStatus, SolidcraftError> {
    let project_path = require_registered_project(&project_path)?;
    let p_path = Path::new(&project_path);
    let is_enabled = is_project_solidcraft_enabled(p_path);
    let cad_dir = resolve_cad_dir(&project_path)?;
    if is_enabled {
        ensure_cad_dir_watcher(&project_path, &cad_dir).await;
    }
    let cad_files = list_cad_files(&cad_dir).await;
    let cad_files_count = cad_files.len();
    let app_available = resolve_app_binary(&project_path).is_ok();
    let cli_available = resolve_cli_binary(&project_path).is_ok();
    let is_editor_running = crate::desktop::find_hyprland_window_for_project(
        &["solidcraft", "io.tangencylabs.solidcraft"],
        &project_path,
    )
    .await
    .is_some();

    Ok(SolidcraftStatus {
        project_path,
        is_enabled,
        cad_dir_path: cad_dir.display().to_string(),
        cad_files_count,
        cad_files,
        app_available,
        cli_available,
        is_editor_running,
    })
}

/// Enables or disables `[plugins.solidcraft]` in `<project_path>/rula.toml`.
pub async fn toggle_project_solidcraft_plugin(
    project_path: String,
    enabled: bool,
) -> Result<SolidcraftStatus, SolidcraftError> {
    let project_path = require_registered_project(&project_path)?;
    let p_path = Path::new(&project_path);

    let rula_path = p_path.join("rula.toml");
    let mut rula_val = crate::config::read_project_rula(p_path)?.ok_or_else(|| SolidcraftError::InvalidSetting {
        key: "enabled",
        details: format!("{} does not exist; initialize the project first", rula_path.display()),
    })?;

    if let Some(root_table) = rula_val.as_table_mut() {
        let plugins_entry = root_table
            .entry("plugins".to_string())
            .or_insert_with(|| toml::Value::Table(Default::default()));
        if let Some(plugins_table) = plugins_entry.as_table_mut() {
            let sc_entry = plugins_table
                .entry("solidcraft".to_string())
                .or_insert_with(|| toml::Value::Table(Default::default()));
            if let Some(sc_table) = sc_entry.as_table_mut() {
                sc_table.insert("enabled".to_string(), toml::Value::Boolean(enabled));
            }
        }
    }

    let serialized_toml = toml::to_string_pretty(&rula_val).map_err(|e| SolidcraftError::InvalidSetting {
        key: "enabled",
        details: format!("cannot serialize rula.toml: {}", e),
    })?;
    tokio::fs::write(&rula_path, serialized_toml)
        .await
        .map_err(io_err(format!("Cannot write {}", rula_path.display())))?;

    if enabled {
        let cad_dir = resolve_cad_dir(&project_path)?;
        tokio::fs::create_dir_all(&cad_dir)
            .await
            .map_err(io_err(format!("Cannot create {}", cad_dir.display())))?;
    } else {
        stop_cad_dir_watcher(&project_path).await;
    }

    get_project_solidcraft_status(project_path).await
}

/// Launches the external SolidCraft desktop editor scoped to `<project_path>/<cad_dir>`,
/// focusing an existing window if open or placing a new instance on an empty Hyprland workspace.
pub async fn launch_project_solidcraft(
    project_path: String,
    file_name: Option<String>,
) -> Result<SolidcraftStatus, SolidcraftError> {
    let project_path = require_registered_project(&project_path)?;
    if !is_project_solidcraft_enabled(Path::new(&project_path)) {
        return Err(SolidcraftError::PluginDisabled { project_path });
    }

    if let Some(existing_win) = crate::desktop::find_hyprland_window_for_project(
        &["solidcraft", "io.tangencylabs.solidcraft"],
        &project_path,
    )
    .await
    {
        crate::desktop::focus_hyprland_window(&existing_win).await;
        return get_project_solidcraft_status(project_path).await;
    }

    let app_bin = resolve_app_binary(&project_path)?;
    let cad_dir = resolve_cad_dir(&project_path)?;
    tokio::fs::create_dir_all(&cad_dir)
        .await
        .map_err(io_err(format!("Cannot create {}", cad_dir.display())))?;

    let files = list_cad_files(&cad_dir).await;
    let target_file = match file_name {
        Some(name) if !name.trim().is_empty() => {
            let (_, path) = validate_cad_filename(Some(name), &files, &cad_dir)?;
            if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
                return Err(SolidcraftError::InvalidSetting {
                    key: "file_name",
                    details: format!("CAD file '{}' does not exist", path.display()),
                });
            }
            Some(path)
        }
        _ => files.into_iter().next().map(|n| cad_dir.join(n)),
    };

    let empty_ws = crate::desktop::find_empty_hyprland_workspace().await;

    let app_str = app_bin.to_string_lossy().to_string();
    let mut std_cmd = crate::desktop::desktop_command(&app_str);
    std_cmd
        .current_dir(&cad_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(path) = target_file {
        std_cmd.arg(path);
    }

    let mut cmd = Command::from(std_cmd);
    let mut child = cmd.spawn().map_err(|e| SolidcraftError::ProcessLifecycle {
        details: format!("Failed to spawn '{}': {}", app_bin.display(), e),
    })?;
    let spawned_pid = child.id();
    tokio::spawn(async move {
        if let Ok(status) = child.wait().await {
            if !status.success() {
                crate::logger::log("WARN", "solidcraft", &format!("SolidCraft exited with {}", status));
            }
        }
    });

    if let (Some(ws), Some(pid)) = (empty_ws, spawned_pid) {
        crate::desktop::wait_and_move_hyprland_window(
            pid,
            &["solidcraft", "io.tangencylabs.solidcraft"],
            &project_path,
            ws,
        )
        .await;
    }

    get_project_solidcraft_status(project_path).await
}

struct SolidcraftTemplateSpec {
    units: &'static str,
    create_xy_sketch: bool,
}

fn resolve_create_template(template_id: Option<&str>) -> Result<SolidcraftTemplateSpec, SolidcraftError> {
    match template_id.unwrap_or("blank_mm").trim() {
        "" | "blank_mm" => Ok(SolidcraftTemplateSpec { units: "mm", create_xy_sketch: false }),
        "blank_cm" => Ok(SolidcraftTemplateSpec { units: "cm", create_xy_sketch: false }),
        "blank_m" => Ok(SolidcraftTemplateSpec { units: "m", create_xy_sketch: false }),
        "blank_in" => Ok(SolidcraftTemplateSpec { units: "in", create_xy_sketch: false }),
        "blank_ft" => Ok(SolidcraftTemplateSpec { units: "ft", create_xy_sketch: false }),
        "xy_sketch_mm" => Ok(SolidcraftTemplateSpec { units: "mm", create_xy_sketch: true }),
        "xy_sketch_in" => Ok(SolidcraftTemplateSpec { units: "in", create_xy_sketch: true }),
        other => Err(SolidcraftError::InvalidSetting {
            key: "template_id",
            details: format!("Unsupported SolidCraft template '{}'", other),
        }),
    }
}

fn normalize_new_sc_filename(raw_name: &str) -> Result<(String, String), SolidcraftError> {
    let trimmed = raw_name.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('-')
        || trimmed.contains(['/', '\\', '\0', '\n', '\r'])
        || trimmed.contains("..")
    {
        return Err(SolidcraftError::InvalidSetting {
            key: "file_name",
            details: "file_name must be a valid filename without path separators".to_string(),
        });
    }

    let path_obj = Path::new(trimmed);
    let normalized = match path_obj.extension().and_then(|e| e.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("sc") => trimmed.to_string(),
        Some(other) => {
            return Err(SolidcraftError::InvalidSetting {
                key: "file_name",
                details: format!("New SolidCraft designs must use the .sc extension (got '.{}')", other),
            });
        }
        None => format!("{}.sc", trimmed),
    };

    let stem = Path::new(&normalized)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| SolidcraftError::InvalidSetting {
            key: "file_name",
            details: "file_name must contain a valid design name".to_string(),
        })?
        .to_string();

    Ok((normalized, stem))
}

fn build_mcp_create_payload(
    design_name: &str,
    target_path: &Path,
    spec: &SolidcraftTemplateSpec,
) -> Vec<u8> {
    let mut requests = vec![
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "one-ship", "version": "0.2" }
            }
        }),
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "new_design",
                "arguments": { "name": design_name }
            }
        }),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "execute",
                "arguments": {
                    "command": "document.units",
                    "params": { "units": spec.units }
                }
            }
        }),
    ];

    let mut next_id = 4;
    if spec.create_xy_sketch {
        requests.push(serde_json::json!({
            "jsonrpc": "2.0",
            "id": next_id,
            "method": "tools/call",
            "params": {
                "name": "execute",
                "arguments": {
                    "command": "sketch.create",
                    "params": { "plane": "XY" }
                }
            }
        }));
        next_id += 1;
        requests.push(serde_json::json!({
            "jsonrpc": "2.0",
            "id": next_id,
            "method": "tools/call",
            "params": {
                "name": "execute",
                "arguments": {
                    "command": "sketch.finish",
                    "params": {}
                }
            }
        }));
        next_id += 1;
    }

    requests.push(serde_json::json!({
        "jsonrpc": "2.0",
        "id": next_id,
        "method": "tools/call",
        "params": {
            "name": "save",
            "arguments": { "path": target_path.to_string_lossy() }
        }
    }));

    let mut buf = String::with_capacity(1024);
    for req in requests {
        buf.push_str(&req.to_string());
        buf.push('\n');
    }
    buf.into_bytes()
}

fn verify_mcp_create_responses(stdout: &[u8]) -> Result<(), SolidcraftError> {
    let text = String::from_utf8_lossy(stdout);
    let mut seen_responses = 0usize;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(val) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        if val.get("id").is_none() {
            continue;
        }
        seen_responses += 1;
        if let Some(err_obj) = val.get("error") {
            return Err(SolidcraftError::ProcessLifecycle {
                details: format!("SolidCraft MCP error: {}", err_obj),
            });
        }
        if let Some(result_obj) = val.get("result") {
            if result_obj.get("isError").and_then(|v| v.as_bool()) == Some(true) {
                let err_msg = result_obj
                    .get("content")
                    .and_then(|c| c.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|item| item.get("text"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("Unknown SolidCraft MCP tool failure");
                return Err(SolidcraftError::ProcessLifecycle {
                    details: format!("SolidCraft MCP tool failed: {}", err_msg),
                });
            }
        }
    }
    if seen_responses == 0 {
        return Err(SolidcraftError::ProcessLifecycle {
            details: "SolidCraft MCP returned no JSON-RPC responses".to_string(),
        });
    }
    Ok(())
}

/// Programmatically creates a new `.sc` CAD design inside `<project_path>/<cad_dir>`
/// using `solidcraft-cli mcp --headless` with the chosen unit/sketch template.
pub async fn create_solidcraft_file(
    project_path: String,
    file_name: String,
    template_id: Option<String>,
) -> Result<SolidcraftStatus, SolidcraftError> {
    let project_path = require_registered_project(&project_path)?;
    if !is_project_solidcraft_enabled(Path::new(&project_path)) {
        return Err(SolidcraftError::PluginDisabled { project_path });
    }

    let cli_bin = resolve_cli_binary(&project_path)?;
    let cad_dir = resolve_cad_dir(&project_path)?;
    let template = resolve_create_template(template_id.as_deref())?;
    let (normalized_name, design_name) = normalize_new_sc_filename(&file_name)?;

    tokio::fs::create_dir_all(&cad_dir)
        .await
        .map_err(io_err(format!("Cannot create {}", cad_dir.display())))?;

    let target_path = cad_dir.join(&normalized_name);
    if tokio::fs::try_exists(&target_path).await.unwrap_or(false) {
        return Err(SolidcraftError::InvalidSetting {
            key: "file_name",
            details: format!("CAD file '{}' already exists", normalized_name),
        });
    }

    let timeout_ms = resolve_snapshot_timeout_ms(&project_path);
    let payload_bytes = build_mcp_create_payload(&design_name, &target_path, &template);

    let mut cmd = Command::new(&cli_bin);
    cmd.arg("mcp")
        .arg("--headless")
        .arg("--allow-path")
        .arg(&cad_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd.spawn().map_err(|e| SolidcraftError::ProcessLifecycle {
        details: format!("Failed spawning solidcraft-cli mcp: {}", e),
    })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(&payload_bytes)
            .await
            .map_err(io_err("Failed writing MCP create sequence to solidcraft-cli"))?;
        let _ = stdin.shutdown().await;
    }

    let output = tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| SolidcraftError::ProcessLifecycle {
        details: format!("Timed out after {} ms creating SolidCraft design", timeout_ms),
    })?
    .map_err(|e| SolidcraftError::ProcessLifecycle {
        details: format!("Failed waiting on solidcraft-cli mcp: {}", e),
    })?;

    if !output.status.success() {
        return Err(SolidcraftError::ProcessLifecycle {
            details: format!(
                "solidcraft-cli mcp exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }

    verify_mcp_create_responses(&output.stdout)?;

    if !tokio::fs::try_exists(&target_path).await.unwrap_or(false) {
        return Err(SolidcraftError::ProcessLifecycle {
            details: format!("Expected new design at '{}' was not created", target_path.display()),
        });
    }

    let modified_ms = read_file_modified_ms(&target_path).await;
    let payload = crate::events::SolidcraftFileChangedPayload {
        project_path: project_path.clone(),
        file_name: normalized_name,
        modified_ms: modified_ms as u64,
    };
    crate::supervisor::get_supervisor().emit_event(
        crate::events::DaemonEvent::solidcraft_file_changed(&payload),
    );

    get_project_solidcraft_status(project_path).await
}

/// Renders a `.sc` or `.step` CAD file headlessly to PNG using `solidcraft-cli snapshot`.
/// Results are cached in a bounded map keyed by `(path, view, section, width, height, modified_ms)`.
pub async fn render_solidcraft_preview(
    project_path: String,
    file_name: Option<String>,
    view: Option<String>,
    section: Option<String>,
) -> Result<SolidcraftPreviewResult, SolidcraftError> {
    let project_path = require_registered_project(&project_path)?;
    if !is_project_solidcraft_enabled(Path::new(&project_path)) {
        return Err(SolidcraftError::PluginDisabled { project_path });
    }

    let cli_bin = resolve_cli_binary(&project_path)?;
    let cad_dir = resolve_cad_dir(&project_path)?;
    let files = list_cad_files(&cad_dir).await;
    let (target_name, target_path) = validate_cad_filename(file_name, &files, &cad_dir)?;
    if !tokio::fs::try_exists(&target_path).await.unwrap_or(false) {
        return Err(SolidcraftError::InvalidSetting {
            key: "file_name",
            details: format!("CAD file '{}' does not exist", target_path.display()),
        });
    }

    let view_name = validate_view_name(view)?;
    let section_spec = validate_section_spec(section)?;
    let (width_px, height_px) = resolve_snapshot_dimensions_px(&project_path);
    let timeout_ms = resolve_snapshot_timeout_ms(&project_path);

    let modified_ms = read_file_modified_ms(&target_path).await;
    let cache_key = (
        target_path.clone(),
        view_name.clone(),
        section_spec.clone(),
        width_px,
        height_px,
    );
    let disk_cache_key = format!(
        "preview|{}|{}|{}|{}|{}",
        target_path.display(),
        view_name,
        section_spec.as_deref().unwrap_or(""),
        width_px,
        height_px
    );
    if modified_ms > 0 {
        let cache = runtime_pool().preview_cache.lock().await;
        if let Some(cached) = cache.get(&cache_key) {
            if cached.modified_ms == modified_ms {
                return Ok(cached.result.clone());
            }
        }
        drop(cache);
        if let Some(disk_result) = crate::kopeng::preview_cache::read_cached::<SolidcraftPreviewResult>(
            "solidcraft",
            &disk_cache_key,
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

    let unique_nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp_png_path = std::env::temp_dir().join(format!(
        "oneship_sc_preview_{}_{}.png",
        std::process::id(),
        unique_nanos
    ));

    let mut cmd = Command::new(&cli_bin);
    cmd.arg("snapshot")
        .arg("--in")
        .arg(&target_path)
        .arg("--out")
        .arg(&tmp_png_path)
        .arg("--view")
        .arg(&view_name)
        .arg("--width")
        .arg(width_px.to_string())
        .arg("--height")
        .arg(height_px.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(ref sec) = section_spec {
        cmd.arg("--section").arg(sec);
    }

    let output_res = tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms),
        cmd.output(),
    )
    .await;

    let output = match output_res {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            let _ = tokio::fs::remove_file(&tmp_png_path).await;
            return Err(SolidcraftError::ProcessLifecycle {
                details: format!("Failed running solidcraft-cli snapshot: {}", e),
            });
        }
        Err(_) => {
            let _ = tokio::fs::remove_file(&tmp_png_path).await;
            return Err(SolidcraftError::ProcessLifecycle {
                details: format!("Timed out after {} ms rendering SolidCraft snapshot", timeout_ms),
            });
        }
    };

    if !output.status.success() {
        let _ = tokio::fs::remove_file(&tmp_png_path).await;
        return Err(SolidcraftError::ProcessLifecycle {
            details: format!(
                "solidcraft-cli snapshot exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }

    let png_bytes = tokio::fs::read(&tmp_png_path)
        .await
        .map_err(io_err(format!("Cannot read snapshot {}", tmp_png_path.display())))?;
    let _ = tokio::fs::remove_file(&tmp_png_path).await;

    if png_bytes.len() > MAX_PREVIEW_PNG_BYTES {
        return Err(SolidcraftError::ProcessLifecycle {
            details: format!(
                "Snapshot PNG exceeded max allowed size ({} > {} bytes)",
                png_bytes.len(),
                MAX_PREVIEW_PNG_BYTES
            ),
        });
    }

    let meta: Value = serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);
    let renderer = meta
        .get("renderer")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let encoded_png = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
    let rendered_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let result = SolidcraftPreviewResult {
        file_name: target_name,
        view: view_name,
        section: section_spec,
        width_px,
        height_px,
        renderer,
        png_data_url: format!("data:image/png;base64,{}", encoded_png),
        rendered_at_ms,
    };

    if modified_ms > 0 {
        let mut cache = runtime_pool().preview_cache.lock().await;
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
        drop(cache);
        crate::kopeng::preview_cache::write_cached(
            "solidcraft",
            &disk_cache_key,
            modified_ms,
            &result,
        )
        .await;
    }

    Ok(result)
}

fn json_f64_triple(val: Option<&Value>) -> [f64; 3] {
    let Some(arr) = val.and_then(|v| v.as_array()) else {
        return [0.0, 0.0, 0.0];
    };
    [
        arr.first().and_then(|v| v.as_f64()).unwrap_or(0.0),
        arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0),
        arr.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0),
    ]
}

fn parse_eval_metrics(file_name: String, raw: &Value) -> SolidcraftMetricsResult {
    let body_list = raw
        .get("bodies")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|b| SolidcraftBodyMetrics {
                    name: b.get("name").and_then(|v| v.as_str()).unwrap_or("Body").to_string(),
                    material: b.get("material").and_then(|v| v.as_str()).unwrap_or("Default").to_string(),
                    closed: b.get("closed").and_then(|v| v.as_bool()).unwrap_or(false),
                    volume_mm3: b.get("volume_mm3").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    area_mm2: b.get("area_mm2").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    mass_g: b.get("mass_g").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    density_g_cm3: b.get("density_g_cm3").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    faces_count: b.get("faces").and_then(|v| v.as_u64()).unwrap_or(0),
                    edges_count: b.get("edges").and_then(|v| v.as_u64()).unwrap_or(0),
                    vertices_count: b.get("vertices").and_then(|v| v.as_u64()).unwrap_or(0),
                    bbox_min_mm: json_f64_triple(b.get("bbox").and_then(|bb| bb.get("min"))),
                    bbox_max_mm: json_f64_triple(b.get("bbox").and_then(|bb| bb.get("max"))),
                    center_of_mass_mm: json_f64_triple(b.get("center_of_mass")),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let params_list = raw
        .get("params")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    let name = p.get("name").and_then(|v| v.as_str())?.to_string();
                    let expression = p
                        .get("expression")
                        .or_else(|| p.get("expr"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let value = p.get("value").and_then(|v| v.as_f64());
                    let unit = p.get("unit").and_then(|v| v.as_str()).map(|s| s.to_string());
                    Some(SolidcraftParameterInfo {
                        name,
                        expression,
                        value,
                        unit,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let total = raw.get("total");
    let bodies_count = raw
        .get("body_count")
        .and_then(|v| v.as_u64())
        .map(|c| c as usize)
        .unwrap_or(body_list.len());

    let evaluated_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    SolidcraftMetricsResult {
        file_name,
        bodies_count,
        total_volume_mm3: total.and_then(|t| t.get("volume_mm3")).and_then(|v| v.as_f64()).unwrap_or(0.0),
        total_area_mm2: total.and_then(|t| t.get("area_mm2")).and_then(|v| v.as_f64()).unwrap_or(0.0),
        total_mass_g: total.and_then(|t| t.get("mass_g")).and_then(|v| v.as_f64()).unwrap_or(0.0),
        bodies: body_list,
        params: params_list,
        evaluated_at_ms,
    }
}

/// Evaluates read-only physical metrics and parametric variables for a `.sc` or `.step` file
/// via `solidcraft-cli eval`.
pub async fn inspect_solidcraft_metrics(
    project_path: String,
    file_name: Option<String>,
) -> Result<SolidcraftMetricsResult, SolidcraftError> {
    let project_path = require_registered_project(&project_path)?;
    if !is_project_solidcraft_enabled(Path::new(&project_path)) {
        return Err(SolidcraftError::PluginDisabled { project_path });
    }

    let cli_bin = resolve_cli_binary(&project_path)?;
    let cad_dir = resolve_cad_dir(&project_path)?;
    let files = list_cad_files(&cad_dir).await;
    let (target_name, target_path) = validate_cad_filename(file_name, &files, &cad_dir)?;
    if !tokio::fs::try_exists(&target_path).await.unwrap_or(false) {
        return Err(SolidcraftError::InvalidSetting {
            key: "file_name",
            details: format!("CAD file '{}' does not exist", target_path.display()),
        });
    }
    if !has_allowed_extension(&target_path, ALLOWED_EVAL_EXTENSIONS) {
        return Err(SolidcraftError::InvalidSetting {
            key: "file_name",
            details: "solidcraft-cli eval only supports .sc and .step/.stp files".to_string(),
        });
    }

    let modified_ms = read_file_modified_ms(&target_path).await;
    let disk_metrics_key = format!("metrics|{}", target_path.display());
    if modified_ms > 0 {
        let cache = runtime_pool().metrics_cache.lock().await;
        if let Some(cached) = cache.get(&target_path) {
            if cached.modified_ms == modified_ms {
                return Ok(cached.result.clone());
            }
        }
        drop(cache);
        if let Some(disk_result) = crate::kopeng::preview_cache::read_cached::<SolidcraftMetricsResult>(
            "solidcraft",
            &disk_metrics_key,
            modified_ms,
        )
        .await
        {
            let mut cache = runtime_pool().metrics_cache.lock().await;
            cache.insert(
                target_path,
                CachedMetricsEntry {
                    modified_ms,
                    result: disk_result.clone(),
                },
            );
            return Ok(disk_result);
        }
    }

    let timeout_ms = resolve_snapshot_timeout_ms(&project_path);
    let mut cmd = Command::new(&cli_bin);
    cmd.arg("eval")
        .arg("--in")
        .arg(&target_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let output = tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms),
        cmd.output(),
    )
    .await
    .map_err(|_| SolidcraftError::ProcessLifecycle {
        details: format!("Timed out after {} ms evaluating SolidCraft metrics", timeout_ms),
    })?
    .map_err(|e| SolidcraftError::ProcessLifecycle {
        details: format!("Failed running solidcraft-cli eval: {}", e),
    })?;

    if !output.status.success() {
        return Err(SolidcraftError::ProcessLifecycle {
            details: format!(
                "solidcraft-cli eval exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }

    let raw: Value = serde_json::from_slice(&output.stdout).map_err(|e| {
        SolidcraftError::ProcessLifecycle {
            details: format!("Invalid JSON from solidcraft-cli eval: {}", e),
        }
    })?;

    let result = parse_eval_metrics(target_name, &raw);

    if modified_ms > 0 {
        let mut cache = runtime_pool().metrics_cache.lock().await;
        if cache.len() >= MAX_METRICS_CACHE_ITEMS_COUNT && !cache.contains_key(&target_path) {
            if let Some(evict_key) = cache.keys().next().cloned() {
                cache.remove(&evict_key);
            }
        }
        cache.insert(
            target_path,
            CachedMetricsEntry {
                modified_ms,
                result: result.clone(),
            },
        );
        drop(cache);
        crate::kopeng::preview_cache::write_cached(
            "solidcraft",
            &disk_metrics_key,
            modified_ms,
            &result,
        )
        .await;
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_view_and_section_rejects_unsafe_flags() {
        assert_eq!(validate_view_name(None).unwrap(), "iso");
        assert_eq!(validate_view_name(Some("TOP".to_string())).unwrap(), "top");
        assert!(validate_view_name(Some("--help".to_string())).is_err());
        assert!(validate_view_name(Some("unknown".to_string())).is_err());

        assert_eq!(validate_section_spec(None).unwrap(), None);
        assert_eq!(validate_section_spec(Some("  ".to_string())).unwrap(), None);
        assert_eq!(validate_section_spec(Some("z=12.5".to_string())).unwrap(), Some("z=12.5".to_string()));
        assert!(validate_section_spec(Some("--flag".to_string())).is_err());
    }

    #[test]
    fn validate_cad_filename_blocks_path_traversal() {
        let dir = Path::new("/tmp/cad");
        let files = vec!["part.sc".to_string()];
        assert_eq!(
            validate_cad_filename(None, &files, dir).unwrap().0,
            "part.sc"
        );
        assert!(validate_cad_filename(Some("../secret.sc".to_string()), &files, dir).is_err());
        assert!(validate_cad_filename(Some("sub/part.sc".to_string()), &files, dir).is_err());
    }

    #[test]
    fn parse_eval_metrics_extracts_solidcraft_json_shape() {
        let raw = serde_json::json!({
            "body_count": 1,
            "bodies": [{
                "name": "Body1",
                "material": "Default",
                "closed": true,
                "volume_mm3": 1000.0,
                "area_mm2": 700.0,
                "mass_g": 1.29,
                "density_g_cm3": 1.29,
                "faces": 6,
                "edges": 12,
                "vertices": 8,
                "bbox": { "min": [0.0, 0.0, 0.0], "max": [10.0, 20.0, 5.0] },
                "center_of_mass": [5.0, 10.0, 2.5]
            }],
            "params": [{ "name": "width", "expression": "20", "value": 20.0, "unit": "mm" }],
            "total": { "volume_mm3": 1000.0, "area_mm2": 700.0, "mass_g": 1.29 }
        });
        let parsed = parse_eval_metrics("box.sc".to_string(), &raw);
        assert_eq!(parsed.bodies_count, 1);
        assert_eq!(parsed.total_volume_mm3, 1000.0);
        assert_eq!(parsed.bodies[0].faces_count, 6);
        assert_eq!(parsed.bodies[0].bbox_max_mm, [10.0, 20.0, 5.0]);
        assert_eq!(parsed.params.len(), 1);
        assert_eq!(parsed.params[0].name, "width");
    }

    #[test]
    fn normalize_new_sc_filename_appends_extension_and_rejects_bad_paths() {
        assert_eq!(
            normalize_new_sc_filename("Enclosure").unwrap(),
            ("Enclosure.sc".to_string(), "Enclosure".to_string())
        );
        assert_eq!(
            normalize_new_sc_filename("SensorMount.sc").unwrap(),
            ("SensorMount.sc".to_string(), "SensorMount".to_string())
        );
        assert!(normalize_new_sc_filename("../escape").is_err());
        assert!(normalize_new_sc_filename("part.step").is_err());
        assert!(normalize_new_sc_filename("   ").is_err());
        assert!(resolve_create_template(Some("blank_mm")).is_ok());
        assert!(resolve_create_template(Some("xy_sketch_in")).is_ok());
        assert!(resolve_create_template(Some("invalid_template")).is_err());
    }
}
