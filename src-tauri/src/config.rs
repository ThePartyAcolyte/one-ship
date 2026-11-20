use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use std::time::SystemTime;

/// Failure of a configuration read, validation or write (`config.toml`, `rula.toml`, runner
/// manifests, the global agy MCP config).
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Configuration directory is unavailable")]
    ConfigDirUnavailable,
    #[error("Home directory is unavailable")]
    HomeUnavailable,
    #[error("{context}: {source}")]
    Io { context: String, source: std::io::Error },
    #[error("Invalid {path}: {reason}")]
    Invalid { path: String, reason: String },
    #[error("Unknown runner '{runner}' (no manifest at {path})")]
    UnknownRunner { runner: String, path: String },
    #[error("Model '{model}' is not declared by runner '{runner}' (declared: {declared})")]
    UnknownModel { runner: String, model: String, declared: String },
    #[error("Effort '{effort}' is not supported by model '{model}' of runner '{runner}' (supported: {supported})")]
    InvalidEffort { runner: String, model: String, effort: String, supported: String },
    #[error("{context}: no model configured for runner '{runner}'; a model of that runner is required")]
    MissingModel { context: String, runner: String },
    #[error("{context}: runner and model must be given together (a model always belongs to its runner)")]
    IncompletePair { context: String },
    #[error("Templates directory unavailable: {0}")]
    TemplatesNotFound(String),
    #[error(transparent)]
    Registry(#[from] crate::registry::RegistryError),
    #[error(transparent)]
    Database(#[from] crate::db::DatabaseError),
}

impl crate::api::ApiError for ConfigError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            // The configuration (or the request's pair) must be corrected by the user.
            Self::Invalid { .. }
            | Self::UnknownRunner { .. }
            | Self::UnknownModel { .. }
            | Self::InvalidEffort { .. }
            | Self::MissingModel { .. }
            | Self::IncompletePair { .. } => StatusCode::BAD_REQUEST,
            Self::ConfigDirUnavailable
            | Self::HomeUnavailable
            | Self::Io { .. }
            | Self::TemplatesNotFound(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Database(e) => e.status(),
            Self::Registry(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::ConfigDirUnavailable => "ConfigError::ConfigDirUnavailable",
            Self::HomeUnavailable => "ConfigError::HomeUnavailable",
            Self::Io { .. } => "ConfigError::Io",
            Self::Invalid { .. } => "ConfigError::Invalid",
            Self::UnknownRunner { .. } => "ConfigError::UnknownRunner",
            Self::UnknownModel { .. } => "ConfigError::UnknownModel",
            Self::InvalidEffort { .. } => "ConfigError::InvalidEffort",
            Self::MissingModel { .. } => "ConfigError::MissingModel",
            Self::IncompletePair { .. } => "ConfigError::IncompletePair",
            Self::TemplatesNotFound(_) => "ConfigError::TemplatesNotFound",
            Self::Database(e) => e.code(),
            Self::Registry(e) => e.code(),
        }
    }
}


fn io_err(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> ConfigError {
    let context = context.into();
    move |source| ConfigError::Io { context, source }
}

fn invalid(path: &Path, reason: impl Into<String>) -> ConfigError {
    ConfigError::Invalid { path: path.display().to_string(), reason: reason.into() }
}

/// Declarative definition of an individual configuration option.
/// Enables automatic UI form rendering in SettingsModal and deterministic cascade resolution.
#[derive(Serialize, Deserialize, Debug, Clone, ts_rs::TS)]
#[ts(export)]
pub struct ConfigFieldSchema {
    /// Configuration key identifier (e.g. `log_max_size_bytes`).
    pub key: String,
    /// Human-readable label displayed in settings interfaces.
    pub label: String,
    /// Field input type: "boolean", "string", "select_enum", "number", or "string_list".
    pub field_type: String,
    /// Canonical system fallback value if omitted in both project and global files.
    pub default_value: serde_json::Value,
    /// Permitted enumeration values when `field_type` is `"select_enum"`.
    pub options: Option<Vec<String>>,
    /// Technical description explaining the operational impact of the setting.
    pub description: String,
    /// Resolution scope: "global_only", "project_only", or "cascade" (project overrides global).
    pub scope: String,
}

/// Logical grouping of configuration fields belonging to a specific subsystem or plugin domain.
#[derive(Serialize, Deserialize, Debug, Clone, ts_rs::TS)]
#[ts(export)]
pub struct PluginConfigSchema {
    /// Unique identifier of the plugin domain (e.g. `logger`, `supervisor`, `telemetry`).
    pub plugin_id: String,
    /// Human-friendly display title for settings categories.
    pub plugin_name: String,
    /// Field specifications defined under this plugin namespace.
    pub fields: Vec<ConfigFieldSchema>,
    /// Runner/model/effort triples among `fields`, validated as a unit (see [`AgentPairSchema`]).
    pub agent_pairs: Vec<AgentPairSchema>,
}

/// A runner/model/effort triple of a plugin table. The runner manifests are the only source of
/// the runner → models → efforts relation, so the three keys are validated together against
/// them: a table that sets any of them sets the model (and the runner, when configurable) too,
/// and an unknown runner or model or an unsupported effort makes the document invalid.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct AgentPairSchema {
    /// Key of the runner field (`field_type = "runner"`); `None` when the runner is fixed.
    pub runner_key: Option<String>,
    /// Runner of a pair whose runner is not configurable (the agy quota probe, the system agent).
    pub fixed_runner: Option<String>,
    /// Key of the model field (`field_type = "runner_model"`).
    pub model_key: String,
    /// Key of the effort field (`field_type = "runner_effort"`); empty value = the model's default.
    pub effort_key: String,
}

/// Runner of the quota warmup probe (`kopeng::stats`), which reads agy's statusline.
pub const QUOTA_PROBE_RUNNER: &str = "agy";

fn agent_pair(runner_key: &str, model_key: &str, effort_key: &str) -> AgentPairSchema {
    AgentPairSchema { runner_key: Some(runner_key.into()), fixed_runner: None, model_key: model_key.into(), effort_key: effort_key.into() }
}

fn fixed_runner_pair(runner: &str, model_key: &str, effort_key: &str) -> AgentPairSchema {
    AgentPairSchema { runner_key: None, fixed_runner: Some(runner.into()), model_key: model_key.into(), effort_key: effort_key.into() }
}

/// Returns the system schema catalog for all configurable plugins.
pub fn get_all_plugin_schemas() -> Vec<PluginConfigSchema> {
    vec![
        PluginConfigSchema {
            plugin_id: "system".into(),
            plugin_name: "General".into(),
            agent_pairs: Vec::new(),
            fields: vec![
                ConfigFieldSchema {
                    key: "preferred_terminal".into(),
                    label: "Preferred Terminal Emulator".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("ghostty"),
                    options: Some(vec![
                        "ghostty".into(),
                        "kitty".into(),
                        "alacritty".into(),
                        "foot".into(),
                        "wezterm".into(),
                        "gnome-terminal".into(),
                        "konsole".into(),
                        "xfce4-terminal".into(),
                        "x-terminal-emulator".into(),
                        "xterm".into(),
                    ]),
                    description: "Terminal emulator spawned for account login flows and interactive CLI sessions.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "log_level".into(),
                    label: "System Log Level".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("INFO"),
                    options: Some(vec![
                        "DEBUG".into(),
                        "INFO".into(),
                        "WARN".into(),
                        "ERROR".into(),
                    ]),
                    description: "Minimum level written to oneship.log and the system_logs table. DEBUG enables exhaustive diagnostics.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "log_max_size_bytes".into(),
                    label: "Max Log Size (Bytes)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(512_000),
                    options: None,
                    description: "Maximum log file size in bytes before automatic rotation.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "log_max_rotations_count".into(),
                    label: "Max Log Rotations".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(2),
                    options: None,
                    description: "Maximum number of rotated backup log files retained on disk.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "log_db_max_rows_count".into(),
                    label: "Max Log Rows (SQLite)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(crate::logger::DEFAULT_LOG_DB_MAX_ROWS_COUNT),
                    options: None,
                    description: "Newest system_logs rows kept in SQLite; older rows are pruned at startup and periodically.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "log_db_prune_interval_seconds".into(),
                    label: "Log Prune Interval (Seconds)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(300),
                    options: None,
                    description: "Interval between system_logs prunes performed by the logger worker.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "max_dir_walk_depth_count".into(),
                    label: "Max Directory Walk Depth".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(32),
                    options: None,
                    description: "Maximum recursion depth when walking project trees for file explorer or semantic indexing.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "max_dir_walk_nodes_count".into(),
                    label: "Max Directory Walk Nodes".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(50000),
                    options: None,
                    description: "Maximum filesystem entries visited per tree/index traversal to prevent infinite symlink/repo hangs.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "fs_tree_max_depth_count".into(),
                    label: "File Tree Max Depth".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(12),
                    options: None,
                    description: "Maximum directory recursion depth when scanning project files.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "fs_tree_max_nodes_count".into(),
                    label: "File Tree Max Nodes".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(5000),
                    options: None,
                    description: "Maximum number of entries listed when scanning project files; a larger tree is shown truncated.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "default_projects_root_path".into(),
                    label: "Default Projects Root Path".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!(
                        dirs::home_dir()
                            .map(|h| h.join("Projects").to_string_lossy().into_owned())
                            .unwrap_or_else(|| ".".to_string())
                    ),
                    options: None,
                    description: "Default filesystem root directory suggested when adding or creating projects.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "dbus_call_timeout_seconds".into(),
                    label: "D-Bus Call Timeout (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(2),
                    options: None,
                    description: "Timeout of the D-Bus status calls made by the system diagnostics (gdbus --timeout).".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "keyring_timeout_seconds".into(),
                    label: "OS Keyring Timeout (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(30),
                    options: None,
                    description: "Maximum time the daemon waits for a Secret Service lookup, store or clear operation before reporting a keyring error.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "webkit_renderer_mode".into(),
                    label: "WebKitGTK Renderer Mode".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("auto"),
                    options: Some(vec![
                        "auto".into(),
                        "dmabuf".into(),
                        "shared_memory".into(),
                        "software".into(),
                    ]),
                    description: "Graphics backing-store transport for the WebKitGTK interface (auto, dmabuf, shared_memory, software). Requires restarting the interface window to take effect.".into(),
                    scope: "global_only".into(),
                },
            ],
        },
        PluginConfigSchema {
            plugin_id: "appearance".into(),
            plugin_name: "Appearance".into(),
            agent_pairs: Vec::new(),
            fields: vec![
                ConfigFieldSchema {
                    key: "theme".into(),
                    label: "Color Theme".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!(crate::theme::default_theme_name()),
                    options: Some(crate::theme::available_theme_names()),
                    description: "Color palette for the interface. 'Auto (Omarchy)' syncs live with your active Omarchy desktop theme when available.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "body_font".into(),
                    label: "Interface Font".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("IBM Plex Mono"),
                    options: Some(vec![
                        "IBM Plex Mono".into(),
                        "Anonymous Pro".into(),
                        "Courier Prime".into(),
                    ]),
                    description: "Primary typography family used for the application interface and editor chrome.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "font_scale".into(),
                    label: "Font Scale".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("100% (Normal)"),
                    options: Some(vec![
                        "90% (Compact)".into(),
                        "100% (Normal)".into(),
                        "110% (Medium)".into(),
                        "120% (Large)".into(),
                        "130% (Extra Large)".into(),
                    ]),
                    description: "Global zoom and typography scaling factor for readability across displays.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "quota_gauge_style".into(),
                    label: "Quota Gauge Style".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("concentric"),
                    options: Some(vec![
                        "concentric".into(),
                        "bars".into(),
                        "classic".into(),
                    ]),
                    description: "Visual rendering style for account quota and time window telemetry (concentric dual-ring, dual-linear bars, or classic pie).".into(),
                    scope: "global_only".into(),
                },
            ],
        },
        PluginConfigSchema {
            plugin_id: "orchestrator".into(),
            plugin_name: "Orchestrator".into(),
            agent_pairs: vec![
                agent_pair("init_runner", "init_model", "init_effort"),
                agent_pair("agent_runner", "default_model", "default_effort"),
            ],
            fields: vec![
                ConfigFieldSchema {
                    key: "init_runner".into(),
                    label: "Project Initializer Runner".into(),
                    field_type: "runner".into(),
                    default_value: serde_json::json!("agy"),
                    options: None,
                    description: "Runner of the project initialization agent (one with a manifest in templates/runners/).".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "init_model".into(),
                    label: "Project Initializer Model".into(),
                    field_type: "runner_model".into(),
                    default_value: serde_json::json!("gemini-3.8-flash"),
                    options: None,
                    description: "Model of the project initialization agent, declared by the initializer runner's manifest.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "init_effort".into(),
                    label: "Project Initializer Reasoning Effort".into(),
                    field_type: "runner_effort".into(),
                    default_value: serde_json::json!("high"),
                    options: None,
                    description: "Reasoning effort of the initialization agent, one of the efforts the manifest declares for init_model (empty = the model's default).".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "agent_runner".into(),
                    label: "Default Agent Runner".into(),
                    field_type: "runner".into(),
                    default_value: serde_json::json!("agy"),
                    options: None,
                    description: "Runner of platform agents without a persona (git, flag resolution) unless rula.toml declares one.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "default_model".into(),
                    label: "Default AI Model".into(),
                    field_type: "runner_model".into(),
                    default_value: serde_json::json!("gemini-3.8-flash"),
                    options: None,
                    description: "Model paired with Default Agent Runner, declared by that runner's manifest.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "default_effort".into(),
                    label: "Default Reasoning Effort".into(),
                    field_type: "runner_effort".into(),
                    default_value: serde_json::json!("high"),
                    options: None,
                    description: "Reasoning effort paired with Default AI Model (empty = the model's default).".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "default_docstyle".into(),
                    label: "Default Documentation Style".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("sdd"),
                    options: Some(vec![
                        "sdd".into(),
                        "lean".into(),
                        "contract".into(),
                        "test_anchored".into(),
                        "custom".into(),
                    ]),
                    description: "Default living documentation archetype scaffolded during project initialization.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "auto_approve".into(),
                    label: "Auto-Approve Tool Permissions".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(false),
                    options: None,
                    description: "Allow CLI agents to execute tools without interactive confirmation prompts.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "agent_spawn_poll_step_ms".into(),
                    label: "Agent Spawn Poll Delay (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(250),
                    options: None,
                    description: "Delay in milliseconds between agent initialization checks and stdin pipes.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "agent_log_tail_lines_count".into(),
                    label: "Agent Failure Log Tail (Lines)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(50),
                    options: None,
                    description: "Number of process output lines retained for failure diagnostic reports.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "init_timeout_minutes".into(),
                    label: "Initialization Timeout (min)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(15),
                    options: None,
                    description: "Upper bound for a supervised or unattended project initialization before it is marked failed (greenfield sessions have no timeout).".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "port_release_timeout_ms".into(),
                    label: "Port Release Timeout (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(3000),
                    options: None,
                    description: "Upper bound for a port to become free after its conflicting occupant receives SIGKILL.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "port_release_poll_step_ms".into(),
                    label: "Port Release Poll Step (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(50),
                    options: None,
                    description: "Delay between port availability checks while releasing a port.".into(),
                    scope: "global_only".into(),
                },
            ],
        },
        PluginConfigSchema {
            plugin_id: "accounts".into(),
            plugin_name: "Accounts & Quotas".into(),
            agent_pairs: vec![fixed_runner_pair(QUOTA_PROBE_RUNNER, "quota_probe_model", "quota_probe_effort")],
            fields: vec![
                ConfigFieldSchema {
                    key: "auto_warmup_enabled".into(),
                    label: "Auto-Kickstart Quota Runway".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Automatically perform a 1-token probe when account quota resets to kickstart the rolling window countdown.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "context_warning_threshold_percent".into(),
                    label: "Context Warning Threshold (%)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(30.0),
                    options: None,
                    description: "Context utilization percentage threshold beyond which compaction is recommended (default: 30%).".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "quota_alert_threshold_percent".into(),
                    label: "Quota Alert Threshold (%)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(15.0),
                    options: None,
                    description: "Remaining account runway percentage threshold below which low-quota warnings are emitted.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "quota_danger_threshold_percent".into(),
                    label: "Quota Danger Threshold (%)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(5.0),
                    options: None,
                    description: "Remaining account runway percentage threshold below which critical danger warnings are emitted.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "stats_refresh_interval_seconds".into(),
                    label: "Quota Refresh Interval (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(60),
                    options: None,
                    description: "Background interval in seconds between rolling quota runway sync checks.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "quota_depleted_threshold_percent".into(),
                    label: "Quota Depleted Threshold (%)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(5.0),
                    options: None,
                    description: "Remaining quota at or below which a pool counts as depleted and becomes eligible for the scheduled warmup probe.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "quota_probe_cooldown_seconds".into(),
                    label: "Quota Probe Cooldown (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(900),
                    options: None,
                    description: "Minimum time between two warmup probes of the same account.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "quota_probe_timeout_ms".into(),
                    label: "Quota Probe Timeout (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(7000),
                    options: None,
                    description: "Time a warmup probe waits for the statusline observation of the probed account before failing.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "quota_probe_poll_step_ms".into(),
                    label: "Quota Probe Poll Step (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(150),
                    options: None,
                    description: "Interval between reads of the statusline observation file during a warmup probe.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "quota_probe_model".into(),
                    label: "Quota Probe Model".into(),
                    field_type: "runner_model".into(),
                    default_value: serde_json::json!("gemini-3.8-flash"),
                    options: None,
                    description: "agy model used by the one-turn warmup probe, declared by the agy manifest.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "quota_probe_effort".into(),
                    label: "Quota Probe Effort".into(),
                    field_type: "runner_effort".into(),
                    default_value: serde_json::json!("low"),
                    options: None,
                    description: "Reasoning effort of the warmup probe, one of the efforts agy declares for the probe model.".into(),
                    scope: "global_only".into(),
                },
            ],
        },
        PluginConfigSchema {
            plugin_id: "supervisor".into(),
            plugin_name: "Process Supervisor".into(),
            agent_pairs: Vec::new(),
            fields: vec![
                ConfigFieldSchema {
                    key: "brief_recent_turns_count".into(),
                    label: "Brief Recent Turns (Count)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(20),
                    options: None,
                    description: "Maximum number of recent chat turns included in a Brief that seeds a new Native Session (Spec 30 §5).".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "search_conversations_max_results_count".into(),
                    label: "Conversation Search Results (Count)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(10),
                    options: None,
                    description: "Maximum number of Conversations returned by a conversation search (Spec 30 §8).".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "brief_max_bytes".into(),
                    label: "Brief Size Limit (Bytes)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(32768),
                    options: None,
                    description: "Hard size cap of a Brief; lower-priority sections are dropped first when it is exceeded.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "identity_ancestor_walk_max_depth_count".into(),
                    label: "Caller Identity Ancestor Walk Depth".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(16),
                    options: None,
                    description: "Maximum parent-process hops when matching an MCP caller to its registered agent process.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "max_buffer_lines_count".into(),
                    label: "Output Ring Buffer Capacity (Lines)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(500),
                    options: None,
                    description: "Maximum number of stdout/stderr lines retained in-memory per process.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "max_task_capture_lines_count".into(),
                    label: "Max Task Capture Lines".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(20000),
                    options: None,
                    description: "Maximum stdout/stderr lines retained per task execution report to prevent unbounded memory growth.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "max_agent_response_bytes".into(),
                    label: "Max Agent Response Buffer (Bytes)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(4_194_304),
                    options: None,
                    description: "Maximum bytes accumulated per single agent turn response/thought stream before truncation.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "max_pending_inputs_count".into(),
                    label: "Max Queued Agent Inputs".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(crate::supervisor::DEFAULT_MAX_PENDING_INPUTS_COUNT),
                    options: None,
                    description: "Prompts queued while an agent is busy; further inputs are rejected until it drains.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "shutdown_grace_period_ms".into(),
                    label: "Process Teardown Grace Period (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(3000),
                    options: None,
                    description: "Grace period in milliseconds before forcefully terminating processes on stop.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "state_watcher_interval_ms".into(),
                    label: "State Watcher Interval (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(2500),
                    options: None,
                    description: "Interval in milliseconds between background supervisor state sync sweeps.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "shutdown_step_delay_ms".into(),
                    label: "Shutdown Step Delay (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(150),
                    options: None,
                    description: "Delay in milliseconds during graceful process termination sequences.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "process_poll_step_ms".into(),
                    label: "Process Poll Step Delay (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(100),
                    options: None,
                    description: "Delay in milliseconds when polling process restarts and termination.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "watcher_debounce_ms".into(),
                    label: "Watcher Debounce Delay (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(1500),
                    options: None,
                    description: "Debounce interval in milliseconds for project filesystem change events.".into(),
                    scope: "global_only".into(),
                },
            ],
        },
        PluginConfigSchema {
            plugin_id: "telemetry".into(),
            plugin_name: "Telemetry & Storage".into(),
            agent_pairs: Vec::new(),
            fields: vec![
                ConfigFieldSchema {
                    key: "telemetry_interval_ms".into(),
                    label: "System Telemetry Interval (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(2000),
                    options: None,
                    description: "Interval in milliseconds between OS-level CPU, RAM, and Network metric polls.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "db_connection_pool_max_count".into(),
                    label: "DB Connection Pool Max Count".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(4),
                    options: None,
                    description: "Maximum number of concurrent SQLite database connections in the connection pool.".into(),
                    scope: "global_only".into(),
                },
            ],
        },
        PluginConfigSchema {
            plugin_id: "profiler".into(),
            plugin_name: "Profiler".into(),
            agent_pairs: Vec::new(),
            fields: vec![
                ConfigFieldSchema {
                    key: "enabled".into(),
                    label: "Enable Continuous Profiler".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Enables run-bound continuous telemetry, flamegraph profiling, and the Profiling workspace tab for this project.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "sample_interval_ms".into(),
                    label: "Continuous Profiler Sample Interval (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(500),
                    options: None,
                    description: "Interval between samples of the process tree of every running service and task. Samples are kept without pruning.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "engine_shutdown_grace_period_ms".into(),
                    label: "Engine Shutdown Grace Period (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(crate::supervisor::DEFAULT_ENGINE_SHUTDOWN_GRACE_PERIOD_MS),
                    options: None,
                    description: "Time a stopped service wrapped by a profiler engine has between SIGINT and SIGKILL, so the engine can write its output file.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "process_breakdown_max_items_count".into(),
                    label: "Process Breakdown Limit".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(crate::duck::DEFAULT_PROCESS_BREAKDOWN_MAX_ITEMS_COUNT),
                    options: None,
                    description: "Most process names listed in a run profile, ordered by average CPU.".into(),
                    scope: "global_only".into(),
                },
            ],
        },
        PluginConfigSchema {
            plugin_id: "vcs".into(),
            plugin_name: "Git & Snapshots".into(),
            agent_pairs: vec![agent_pair("git_runner", "git_model", "git_effort")],
            fields: vec![
                ConfigFieldSchema {
                    key: "default_agent_id".into(),
                    label: "Default Git Agent".into(),
                    field_type: "project_agent".into(),
                    default_value: serde_json::json!(""),
                    options: None,
                    description: "Declared project agent used for Git actions and OPEN AGENT sessions in this repository.".into(),
                    scope: "project_only".into(),
                },
                ConfigFieldSchema {
                    key: "git_runner".into(),
                    label: "Git Agent Runner".into(),
                    field_type: "runner".into(),
                    default_value: serde_json::json!("agy"),
                    options: None,
                    description: "Default runner used for Git agent sessions and AI commit message generation.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "git_model".into(),
                    label: "Git Agent Model".into(),
                    field_type: "runner_model".into(),
                    default_value: serde_json::json!("gemini-3.8-flash"),
                    options: None,
                    description: "Model paired with Git Agent Runner, declared by that runner's manifest.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "git_effort".into(),
                    label: "Git Agent Reasoning Effort".into(),
                    field_type: "runner_effort".into(),
                    default_value: serde_json::json!("high"),
                    options: None,
                    description: "Reasoning effort paired with Git Agent Model (empty = the model's default).".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "default_branch".into(),
                    label: "Default Branch".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("master"),
                    options: None,
                    description: "Default branch name applied during repository initialization.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "github_repo_list_limit_count".into(),
                    label: "GitHub Repository List Limit".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(1000),
                    options: None,
                    description: "Maximum number of repositories listed by `gh repo list` in the Remote VCS modal.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "gitlab_repo_list_limit_count".into(),
                    label: "GitLab Repository List Limit".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(100),
                    options: None,
                    description: "Maximum number of repositories listed by `glab api projects` in the Remote VCS modal.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "forge_cli_timeout_seconds".into(),
                    label: "Forge CLI & Remote Git Timeout (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(60),
                    options: None,
                    description: "Maximum wait time in seconds for external Forge CLI (`glab`/`gh`) and Git inspection commands.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "snapshots_enabled".into(),
                    label: "Enable Project Snapshots".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Master toggle for project-level snapshot tracking and automatic safety checkpoints.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "minutes_checkpoint_mode".into(),
                    label: "Session Minutes Checkpoint Mode".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("snapshot"),
                    options: Some(vec!["snapshot".into(), "commit".into(), "off".into()]),
                    description: "Checkpoint action executed when an agent records session minutes: hidden snapshot ref, real Git commit on the active branch, or disabled.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "auto_commit_detail_level".into(),
                    label: "Auto-Commit Message Detail".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("title_and_decisions"),
                    options: Some(vec!["title_only".into(), "title_and_decisions".into(), "full".into()]),
                    description: "Content included in automatic Git commits created from session minutes (subject is always the concise minute title).".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "auto_snapshot_on_file_change".into(),
                    label: "Auto-Snapshot on File Save".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Automatically create a scoped code snapshot when a project file is saved.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "max_per_project_count".into(),
                    label: "Max Code Snapshots per Project".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(50),
                    options: None,
                    description: "Maximum number of code/milestone snapshots retained in project history before auto-pruning oldest states.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "auto_snapshot_on_init".into(),
                    label: "Auto-Snapshot on Initialization".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Automatically create a genesis safety snapshot upon project bootstrapping.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "scratchpad_retention_days".into(),
                    label: "Scratchpad Auto-Prune Retention (Days)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(0),
                    options: None,
                    description: "Automatically delete scratchpad/ files older than N days (0 = disabled, never auto-delete).".into(),
                    scope: "cascade".into(),
                },
            ],
        },
        PluginConfigSchema {
            plugin_id: "mcp".into(),
            plugin_name: "MCP Protocol & Approvals".into(),
            agent_pairs: Vec::new(),
            fields: vec![
                ConfigFieldSchema {
                    key: "mcp_daemon_port".into(),
                    label: "MCP Daemon HTTP Port".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(8765),
                    options: None,
                    description: "Local HTTP port bound by the ONE-SHIP native MCP daemon.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "task_wait_timeout_ms".into(),
                    label: "Task Wait Timeout (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(30000),
                    options: None,
                    description: "How long start_project_task(wait=true) waits when the agent gives no timeout_ms; the task then continues in the background.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "approval_ttl_ms".into(),
                    label: "Approval Request TTL (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(300_000),
                    options: None,
                    description: "Time-to-live in milliseconds before interactive user approval requests expire.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "event_channel_capacity_count".into(),
                    label: "Event Channel Capacity".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(DEFAULT_EVENT_CHANNEL_CAPACITY_COUNT),
                    options: None,
                    description: "Events buffered per subscriber of the daemon event bus (/api/events). A slower client skips the overflow and receives a resync event.".into(),
                    scope: "global_only".into(),
                },
            ],
        },
        PluginConfigSchema {
            plugin_id: "rea".into(),
            plugin_name: "Reverse Engineering Lab (REA)".into(),
            agent_pairs: Vec::new(),
            fields: vec![
                ConfigFieldSchema {
                    key: "enabled".into(),
                    label: "Enable Reverse Engineering Lab".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(false),
                    options: None,
                    description: "Enables REA MCP integration, isolated .one-ship/rea evidence storage, and the RE Lab workspace for this project.".into(),
                    scope: "project_only".into(),
                },
                ConfigFieldSchema {
                    key: "max_concurrent_heavy_jobs_count".into(),
                    label: "Max Concurrent Heavy Analysis Jobs".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(1),
                    options: None,
                    description: "Maximum number of heavy binary decompilation jobs (e.g. Ghidra headless) allowed to execute simultaneously across all projects before queuing in FIFO order.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "idle_timeout_seconds".into(),
                    label: "Engine Idle Auto-Stop Timeout (Seconds)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(600),
                    options: None,
                    description: "Inactivity duration in seconds (with 0 active or queued requests) before automatically stopping a project's REA HTTP engine to reclaim RAM.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "ghidra_install_dir".into(),
                    label: "Ghidra Installation Directory".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("/opt/ghidra"),
                    options: None,
                    description: "Filesystem path to the local Ghidra installation containing support/analyzeHeadless.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "max_jvm_heap_mbytes".into(),
                    label: "Max Ghidra JVM Heap (MB)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(2048),
                    options: None,
                    description: "Maximum JVM heap allocation in megabytes (-Xmx) for headless Ghidra decompilation workers.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "targets_dir".into(),
                    label: "Targets Directory".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("targets"),
                    options: None,
                    description: "Project-relative subdirectory scanned for target binaries, firmware images, and packages.".into(),
                    scope: "cascade".into(),
                },
            ],
        },
        PluginConfigSchema {
            plugin_id: "electronics".into(),
            plugin_name: "Electronics & Embedded Lab".into(),
            agent_pairs: Vec::new(),
            fields: vec![
                ConfigFieldSchema {
                    key: "enabled".into(),
                    label: "Enable Electronics & Embedded Lab".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(false),
                    options: None,
                    description: "Enables hardware inspection, microcontroller firmware compilation and flashing tools for this project.".into(),
                    scope: "project_only".into(),
                },
                ConfigFieldSchema {
                    key: "firmware_dir".into(),
                    label: "Firmware Directory".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("."),
                    options: None,
                    description: "Project-relative subdirectory containing the microcontroller firmware project (platformio.ini, src/, include/).".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "schematics_dir".into(),
                    label: "Schematics Directory".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("."),
                    options: None,
                    description: "Project-relative subdirectory containing EDA schematics (.kicad_sch) and SPICE simulation netlists (.cir).".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "default_baud_rate_bps".into(),
                    label: "Default Serial Baud Rate".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("115200"),
                    options: Some(vec![
                        "9600".into(),
                        "19200".into(),
                        "38400".into(),
                        "57600".into(),
                        "115200".into(),
                        "230400".into(),
                        "460800".into(),
                        "921600".into(),
                    ]),
                    description: "Default baud rate used when opening the serial monitor to listen to the microcontroller.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "flasher_tool".into(),
                    label: "Default Firmware Flashing Tool".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("pio"),
                    options: Some(vec![
                        "pio".into(),
                        "probe-rs".into(),
                        "esptool".into(),
                    ]),
                    description: "Toolchain utilized to upload and flash firmware binaries to target development boards.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "usb_flush_max_attempts_count".into(),
                    label: "USB Endpoint Flush Max Attempts".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(256),
                    options: None,
                    description: "Maximum bulk read attempts when draining stale USB endpoint packets on benchtop instruments.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "firmware_upload_timeout_seconds".into(),
                    label: "Firmware Upload Timeout (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(180),
                    options: None,
                    description: "Maximum wait time in seconds for `mpremote` and `pio` microcontroller firmware uploads.".into(),
                    scope: "global_only".into(),
                },
            ],
        },
        PluginConfigSchema {
            plugin_id: "desigma".into(),
            plugin_name: "One-Ship Designer".into(),
            agent_pairs: Vec::new(),
            fields: vec![
                ConfigFieldSchema {
                    key: "enabled".into(),
                    label: "Enable One-Ship Designer".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(false),
                    options: None,
                    description: "Enables One-Ship Designer MCP integration and local .desigma design prototyping for this project.".into(),
                    scope: "project_only".into(),
                },
                ConfigFieldSchema {
                    key: "mcp_binary_path".into(),
                    label: "Designer MCP Binary Path".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("desigma-mcp"),
                    options: None,
                    description: "Absolute path to the compiled desigma-mcp stdio server binary.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "app_binary_path".into(),
                    label: "One-Ship Designer Binary Path".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("oneship-designer"),
                    options: None,
                    description: "Absolute path to the compiled One-Ship Designer native desktop editor binary.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "design_dir".into(),
                    label: "Design Workspace Directory".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("design"),
                    options: None,
                    description: "Project-relative directory where .desigma files are stored and scoped for desigma-mcp.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "enable_bridge".into(),
                    label: "Enable Live Agent Bridge (--bridge)".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(false),
                    options: None,
                    description: "Launches One-Ship Designer with --bridge so MCP tools edit the open canvas in real time.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "mcp_request_timeout_ms".into(),
                    label: "MCP Request Timeout (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(30000),
                    options: None,
                    description: "Maximum wait time in milliseconds for a single desigma-mcp stdio JSON-RPC response.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "auto_snapshot_on_edit".into(),
                    label: "Auto-Snapshot on Edit".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Automatically create a scoped Git snapshot when an agent or external editor modifies Design files.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "max_snapshots_per_asset_count".into(),
                    label: "Max Snapshots Per Asset".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(20),
                    options: None,
                    description: "Maximum number of Design undo snapshots retained per .desigma file.".into(),
                    scope: "cascade".into(),
                },
            ],
        },
        PluginConfigSchema {
            plugin_id: "solidcraft".into(),
            plugin_name: "SolidCraft 3D CAD".into(),
            agent_pairs: Vec::new(),
            fields: vec![
                ConfigFieldSchema {
                    key: "enabled".into(),
                    label: "Enable SolidCraft CAD".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(false),
                    options: None,
                    description: "Enables SolidCraft CAD file preview, physical metrics inspection, and external editor launching for this project.".into(),
                    scope: "project_only".into(),
                },
                ConfigFieldSchema {
                    key: "app_binary_path".into(),
                    label: "SolidCraft Editor Binary Path".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("solidcraft"),
                    options: None,
                    description: "Executable name on PATH or absolute path to the SolidCraft desktop CAD editor binary.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "cli_binary_path".into(),
                    label: "SolidCraft CLI Binary Path".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("solidcraft-cli"),
                    options: None,
                    description: "Executable name on PATH or absolute path to the solidcraft-cli binary used for headless snapshots and metrics evaluation.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "cad_dir".into(),
                    label: "CAD Workspace Directory".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("cad"),
                    options: None,
                    description: "Project-relative directory monitored for .sc, .step, .stp, and .3mf CAD files.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "snapshot_timeout_ms".into(),
                    label: "Snapshot & Eval Timeout (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(15000),
                    options: None,
                    description: "Maximum execution time in milliseconds for headless solidcraft-cli snapshot or eval invocations.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "snapshot_width_px".into(),
                    label: "Preview Width (px)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(1280),
                    options: None,
                    description: "Width in pixels of rendered SolidCraft PNG previews.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "snapshot_height_px".into(),
                    label: "Preview Height (px)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(800),
                    options: None,
                    description: "Height in pixels of rendered SolidCraft PNG previews.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "auto_snapshot_on_edit".into(),
                    label: "Auto-Snapshot on Edit".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Automatically create a scoped Git snapshot when an agent or external editor modifies CAD files.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "max_snapshots_per_asset_count".into(),
                    label: "Max Snapshots Per Asset".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(20),
                    options: None,
                    description: "Maximum number of CAD undo snapshots retained per file.".into(),
                    scope: "cascade".into(),
                },
            ],
        },
        PluginConfigSchema {
            plugin_id: "pcbcraft".into(),
            plugin_name: "PCBCraft EDA / PCB".into(),
            agent_pairs: Vec::new(),
            fields: vec![
                ConfigFieldSchema {
                    key: "enabled".into(),
                    label: "Enable PCBCraft EDA".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(false),
                    options: None,
                    description: "Enables PCBCraft schematic/PCB SVG preview, BOM & DRC inspection, and the /mcp/pcbcraft gateway for this project.".into(),
                    scope: "project_only".into(),
                },
                ConfigFieldSchema {
                    key: "app_binary_path".into(),
                    label: "PCBCraft Editor Binary Path".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("pcbcraft"),
                    options: None,
                    description: "Absolute path to the PCBCraft desktop editor binary.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "cli_binary_path".into(),
                    label: "PCBCraft CLI Binary Path".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("pcbcraft-cli"),
                    options: None,
                    description: "Absolute path to the pcbcraft-cli binary used for MCP inspection and headless project validation.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "pcb_dir".into(),
                    label: "PCB Workspace Directory".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("pcb"),
                    options: None,
                    description: "Project-relative directory monitored for pcbcraft-project/17 folders and .pc archives.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "mcp_request_timeout_ms".into(),
                    label: "MCP Request Timeout (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(30000),
                    options: None,
                    description: "Maximum wait time in milliseconds for a single pcbcraft-cli mcp JSON-RPC response.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "auto_snapshot_on_edit".into(),
                    label: "Auto-Snapshot on Edit".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Automatically create a scoped Git snapshot when an agent saves or external PCBCraft modifies PCB files.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "max_snapshots_per_asset_count".into(),
                    label: "Max Snapshots Per Asset".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(20),
                    options: None,
                    description: "Maximum number of PCB undo snapshots retained per board target.".into(),
                    scope: "cascade".into(),
                },
            ],
        },
        PluginConfigSchema {
            plugin_id: "voice_controller".into(),
            plugin_name: "Global Voice Controller (Gemini Live)".into(),
            agent_pairs: Vec::new(),
            fields: vec![
                ConfigFieldSchema {
                    key: "enabled".into(),
                    label: "Enable Global Voice Controller".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Enables the Push-to-Talk Gemini Live orchestrator in the top bar.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "model".into(),
                    label: "Live Model".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("gemini-3.8-live"),
                    options: Some(vec![
                        "gemini-3.8-live".into(),
                        "gemini-3.8-live-extended-thinking".into(),
                    ]),
                    description: "Gemini Live model: 'gemini-3.8-live' (low-latency default) or 'gemini-3.8-live-extended-thinking' (background reasoning).".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "voice_name".into(),
                    label: "Voice".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("Fenrir"),
                    options: Some(vec![
                        "Zephyr".into(),
                        "Puck".into(),
                        "Charon".into(),
                        "Kore".into(),
                        "Fenrir".into(),
                        "Leda".into(),
                        "Orus".into(),
                        "Aoede".into(),
                        "Callirrhoe".into(),
                        "Autonoe".into(),
                        "Enceladus".into(),
                        "Iapetus".into(),
                        "Umbriel".into(),
                        "Algieba".into(),
                        "Despina".into(),
                        "Erinome".into(),
                        "Algenib".into(),
                        "Rasalgethi".into(),
                        "Laomedeia".into(),
                        "Achernar".into(),
                        "Alnilam".into(),
                        "Schedar".into(),
                        "Gacrux".into(),
                        "Pulcherrima".into(),
                        "Achird".into(),
                        "Zubenelgenubi".into(),
                        "Vindemiatrix".into(),
                        "Sadachbia".into(),
                        "Sadaltager".into(),
                        "Sulafat".into(),
                    ]),
                    description: "Prebuilt Gemini Live studio voice used for spoken responses.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "thinking_level".into(),
                    label: "Thinking Level".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("low"),
                    options: Some(vec!["low".into(), "medium".into(), "high".into()]),
                    description: "Reasoning depth when using 'gemini-3.8-live-extended-thinking' (ignored on 'gemini-3.8-live').".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "persona_style".into(),
                    label: "Voice Persona Style".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("belta"),
                    options: Some(vec!["belta".into(), "neutral".into()]),
                    description: "Conversational tone for the Ship Brain voice controller ('belta' uses Belter Creole flavor with phonetic hints; 'neutral' uses standard technical tone).".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "gemini_api_key".into(),
                    label: "Google AI Studio API Key".into(),
                    field_type: "secret".into(),
                    default_value: serde_json::json!(""),
                    options: None,
                    description: "API Key from Google AI Studio used by oneship-daemon for BidiGenerateContent Live sessions. Stored in the OS keyring; never written to config.toml or sent to the interface.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "idle_disconnect_seconds".into(),
                    label: "Idle Disconnect Timeout (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(180),
                    options: None,
                    description: "Seconds of inactivity before closing the upstream Gemini Live WebSocket while preserving the SQLite working session.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "turn_ledger_max_entries_count".into(),
                    label: "Turn Ledger Max Entries".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(12),
                    options: None,
                    description: "Maximum recent voice turns kept verbatim before folding oldest entries into the rolling summary.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "scratchpad_max_items_count".into(),
                    label: "Scratchpad Max Items".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(20),
                    options: None,
                    description: "Maximum active working items retained in the voice orchestrator scratchpad.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "context_compression_trigger_tokens".into(),
                    label: "Context Compression Trigger (tokens)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(32000),
                    options: None,
                    description: "Gemini Live contextWindowCompression triggerTokens threshold.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "context_compression_target_tokens".into(),
                    label: "Context Compression Target (tokens)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(16000),
                    options: None,
                    description: "Gemini Live contextWindowCompression slidingWindow targetTokens.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "voice_tool_response_max_bytes".into(),
                    label: "Tool Response Warning Threshold (Bytes)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(6144),
                    options: None,
                    description: "Non-truncating WARN log threshold in bytes for serialized toolResponse payloads returned to Gemini Live.".into(),
                    scope: "global_only".into(),
                },
            ],
        },
        PluginConfigSchema {
            plugin_id: "dictation".into(),
            plugin_name: "Dictation (Gemini Live)".into(),
            agent_pairs: Vec::new(),
            fields: vec![
                ConfigFieldSchema {
                    key: "enabled".into(),
                    label: "Enable Native Dictation".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Enables Gemini Live speech-to-text dictation into ONE-SHIP text fields.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "mode".into(),
                    label: "Transcription Mode".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("smart"),
                    options: Some(vec!["smart".into(), "verbatim".into()]),
                    description: "'smart' uses the transcription model's native punctuation, formatting and disfluency removal. 'verbatim' keeps the literal acoustic transcript. Neither mode runs a second LLM cleanup pass.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "auto_send".into(),
                    label: "Auto-send Dictated Prompts".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Submits the final transcript automatically when dictation targets an agent chat prompt. Other text fields are never submitted.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "speech_energy_threshold".into(),
                    label: "Speech Energy Threshold".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(0.03),
                    options: None,
                    description: "Normalized RMS threshold used by the client-side VAD to distinguish speech from silence for watchdog and rollover decisions.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "rollover_min_session_seconds".into(),
                    label: "Rollover Minimum Session Age (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(90),
                    options: None,
                    description: "Minimum Live session age before a silence window may trigger a proactive rollover. This is not a dictation time limit.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "rollover_silence_ms".into(),
                    label: "Rollover Silence Window (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(800),
                    options: None,
                    description: "Continuous silence required, together with the minimum session age, before proactively opening a fresh Live session.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "rollover_finalize_timeout_seconds".into(),
                    label: "Rollover Finalization Wait (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(5),
                    options: None,
                    description: "Network wait for the current Live session to confirm its final segment during a proactive rollover. Unconfirmed audio is replayed, never discarded.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "watchdog_silence_seconds".into(),
                    label: "Speech Stall Watchdog (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(10),
                    options: None,
                    description: "Reconnects the Live session when recent speech produced no transcription for this many seconds.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "finalize_timeout_seconds".into(),
                    label: "Stop Finalization Wait (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(120),
                    options: None,
                    description: "Network wait for the final Live transcription after the user stops dictating. A timeout never deletes recorded audio.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "reconnect_attempts_count".into(),
                    label: "Live Reconnect Attempts".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(5),
                    options: None,
                    description: "Maximum reconnect attempts per Live incident before reporting an error and preserving the session audio file for recovery.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "recovery_min_interval_seconds".into(),
                    label: "Minimum Recovery Interval (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(5),
                    options: None,
                    description: "Cooldown between consecutive Live recovery attempts so a persistent server failure does not produce a reconnect storm.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "stale_audio_retention_hours".into(),
                    label: "Stale Dictation Audio Retention (h)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(24),
                    options: None,
                    description: "How long unrecovered dictation PCM files are kept in the state directory before startup cleanup removes them.".into(),
                    scope: "global_only".into(),
                },
            ],
        },
        PluginConfigSchema {
            plugin_id: "project_management".into(),
            plugin_name: "Project Management".into(),
            agent_pairs: Vec::new(),
            fields: vec![
                ConfigFieldSchema {
                    key: "enabled".into(),
                    label: "Enable Project Management".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(false),
                    options: None,
                    description: "Enables the Project Kanban board, timeline, and open-source collaboration workspace tab for this project.".into(),
                    scope: "project_only".into(),
                },
                ConfigFieldSchema {
                    key: "tracking_mode".into(),
                    label: "Milestone Tracking Mode".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("linear"),
                    options: Some(vec!["linear".into(), "shape_up".into(), "weighted".into()]),
                    description: "Controls how milestone progress is calculated and rendered: 'linear' (emergent task rollup), 'shape_up' (Hill Chart uphill/downhill + appetite), or 'weighted' (commercial baseline weights + drift compass).".into(),
                    scope: "project_only".into(),
                },
                ConfigFieldSchema {
                    key: "github_cache_ttl_seconds".into(),
                    label: "GitHub Cache TTL (seconds)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(120),
                    options: None,
                    description: "Time-to-live in seconds before cached GitHub issues, pull requests, and release queries refresh from gh.".into(),
                    scope: "cascade".into(),
                },
            ],
        },
        PluginConfigSchema {
            plugin_id: "semantic".into(),
            plugin_name: "Knowledge & Semantic Search".into(),
            agent_pairs: Vec::new(),
            fields: vec![
                ConfigFieldSchema {
                    key: "enabled".into(),
                    label: "Enable Semantic Search & Indexing".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Enables transversal semantic search, LanceDB vector storage, and background AST indexing.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "graph_tab_enabled".into(),
                    label: "Show Graph Tab".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Displays the interactive Knowledge Graph tab in the project workspace.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "ignored_patterns".into(),
                    label: "Ignored Patterns".into(),
                    field_type: "string_list".into(),
                    default_value: serde_json::json!([
                        "target", "node_modules", "dist", "build", ".git", ".env*", "*.min.js", "*.lock", "__pycache__", ".venv", "venv", "*.pyc", "scratchpad"
                    ]),
                    options: None,
                    description: "Files and directory patterns excluded from semantic indexing. Distinct from .gitignore to preserve local research and documentation.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "included_extensions".into(),
                    label: "Included Extensions".into(),
                    field_type: "string_list".into(),
                    default_value: serde_json::json!([
                        "rs", "ts", "tsx", "js", "jsx", "py", "vue", "svelte", "astro",
                        "go", "java", "cs", "c", "cpp", "h", "hpp", "rb", "php",
                        "css", "sql", "sh", "md", "txt", "pdf"
                    ]),
                    options: None,
                    description: "File extensions processed by the deterministic AST symbol scanner and prose documentation chunker.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "extra_paths".into(),
                    label: "Extra Documentation Paths".into(),
                    field_type: "string_list".into(),
                    default_value: serde_json::json!([
                        "docs", "specs", "devlog", "research", "manuals"
                    ]),
                    options: None,
                    description: "Additional project directories to recursively index for architectural specifications, research notes, and manuals.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "semantic_edge_min_score_percent".into(),
                    label: "Semantic Graph Edge Min Score (%)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(70),
                    options: None,
                    description: "Minimum cosine similarity percentage (1-100) required to link specs, tasks, debt, and journals to code symbols via semantic edges.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "max_file_size_bytes".into(),
                    label: "Max File Size (Bytes)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(262144),
                    options: None,
                    description: "Maximum file size in bytes (default 256 KiB, 0 disables) indexed for code and text files to skip oversized generated dumps.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "file_watch_debounce_ms".into(),
                    label: "File Watch Auto-Index Debounce (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(10000),
                    options: None,
                    description: "Debounce window in milliseconds after workspace file edits before triggering background incremental semantic indexing.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "entity_watch_debounce_ms".into(),
                    label: "Database Entity Auto-Index Debounce (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(2000),
                    options: None,
                    description: "Debounce window in milliseconds after session minutes, tasks, or technical debt updates before triggering incremental indexing.".into(),
                    scope: "cascade".into(),
                },
                ConfigFieldSchema {
                    key: "embedding_batch_size_count".into(),
                    label: "Embedding Micro-Batch Size".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(10),
                    options: None,
                    description: "Number of text chunks vectorized per ONNX batch before checkpointing to LanceDB.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "embedding_batch_sleep_ms".into(),
                    label: "Embedding Batch Breathing Pause (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(20),
                    options: None,
                    description: "Cooperative pause in milliseconds between embedding micro-batches.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "throttler_min_free_mem_mbytes".into(),
                    label: "Background Indexing Min Free RAM (MiB)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(2048),
                    options: None,
                    description: "Minimum available host RAM (MiB) required to run background semantic indexing.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "throttler_critical_free_mem_mbytes".into(),
                    label: "Manual Indexing Min Free RAM (MiB)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(512),
                    options: None,
                    description: "Minimum available host RAM (MiB) required for manual semantic indexing.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "throttler_max_cpu_percent".into(),
                    label: "Background Indexing Max Host CPU (%)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(45),
                    options: None,
                    description: "Maximum global host CPU usage percentage allowed before background indexing yields.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "onnx_intra_threads_count".into(),
                    label: "ONNX Intra-Op Threads".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(2),
                    options: None,
                    description: "Number of CPU intra-op threads assigned to the local BGE-M3 ONNX embedding engine.".into(),
                    scope: "global_only".into(),
                },
                ConfigFieldSchema {
                    key: "max_relations_per_result_count".into(),
                    label: "Default Graph Relations Per Search Result".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(6),
                    options: None,
                    description: "Default number of 1-hop knowledge graph relations attached to each semantic search result.".into(),
                    scope: "global_only".into(),
                },
            ],
        },
    ]
}

/// `~/.config/one-ship/config.toml`, creating the directory if needed.
#[cfg(not(test))]
pub fn get_global_config_path() -> Result<PathBuf, ConfigError> {
    let app_dir = dirs::config_dir().ok_or(ConfigError::ConfigDirUnavailable)?.join("one-ship");
    fs::create_dir_all(&app_dir).map_err(io_err(format!("Cannot create {}", app_dir.display())))?;
    Ok(app_dir.join("config.toml"))
}

/// Test builds never read or write the user's `config.toml`: each test process gets its own,
/// created with the defaults on first read, so results do not depend on the machine's settings.
#[cfg(test)]
pub fn get_global_config_path() -> Result<PathBuf, ConfigError> {
    Ok(test_process_dir().join("config.toml"))
}

/// Throwaway per-process directory that replaces `~/.config/one-ship` in test builds
/// (configuration, database, log files).
#[cfg(test)]
pub(crate) fn test_process_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("oneship-test-process-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("create per-process test directory");
    dir
}

/// Table of `plugin_id` in a configuration document: `[system]` for the `system` plugin,
/// `[plugins.<id>]` for every other plugin (Spec 10, cascade levels 2–3). No other location
/// and no key alias is read.
fn plugin_table<'a>(root: &'a toml::Value, plugin_id: &str) -> Option<&'a toml::map::Map<String, toml::Value>> {
    let table = if plugin_id == "system" {
        root.get("system")
    } else {
        root.get("plugins").and_then(|p| p.get(plugin_id))
    };
    table.and_then(|t| t.as_table())
}

fn value_matches_type(value: &toml::Value, field: &ConfigFieldSchema) -> bool {
    match field.field_type.as_str() {
        "boolean" => value.is_bool(),
        "number" => value.is_integer() || value.is_float(),
        // Runner/model/effort values are checked as a unit against the manifests (`agent_pair_problems`).
        "string" | "project_agent" | "runner" | "runner_model" | "runner_effort" | "secret" => value.is_str(),
        "select_enum" => value
            .as_str()
            .is_some_and(|s| field.options.as_ref().is_none_or(|opts| opts.iter().any(|o| o == s))),
        "string_list" => value.as_array().is_some_and(|a| a.iter().all(|v| v.is_str())),
        _ => false,
    }
}

/// Registered keys set with a value of the wrong type, e.g. `plugins.vcs.auto_approve: expected boolean`.
/// Unregistered keys are not checked (tables also hold scripts, agents and project metadata).
fn schema_type_problems(root: &toml::Value) -> Vec<String> {
    let mut problems = Vec::new();
    for schema in get_all_plugin_schemas() {
        let Some(table) = plugin_table(root, &schema.plugin_id) else {
            continue;
        };
        let location = if schema.plugin_id == "system" { "system".to_string() } else { format!("plugins.{}", schema.plugin_id) };
        for field in &schema.fields {
            let Some(value) = table.get(&field.key) else {
                continue;
            };
            if value_matches_type(value, field) {
                continue;
            }
            let expected = match (field.field_type.as_str(), &field.options) {
                ("select_enum", Some(options)) => format!("one of {}", options.join(", ")),
                (field_type, _) => field_type.to_string(),
            };
            problems.push(format!("{}.{}: expected {}, found {}", location, field.key, expected, value));
        }
    }
    problems
}

/// Runner/model/effort triples of the plugin tables (`agent_pairs`) that a document sets
/// incompletely or that its runner's manifest does not declare.
fn agent_pair_problems(root: &toml::Value) -> Vec<String> {
    let mut problems = Vec::new();
    for schema in get_all_plugin_schemas() {
        let Some(table) = plugin_table(root, &schema.plugin_id) else {
            continue;
        };
        let location = if schema.plugin_id == "system" { "system".to_string() } else { format!("plugins.{}", schema.plugin_id) };
        for pair in &schema.agent_pairs {
            let get = |key: &str| table.get(key).and_then(|v| v.as_str());
            let runner = pair.runner_key.as_deref().and_then(get);
            let model = get(&pair.model_key);
            let effort = get(&pair.effort_key);
            if runner.is_none() && model.is_none() && effort.is_none() {
                continue;
            }
            let keys = [pair.runner_key.as_deref(), Some(pair.model_key.as_str())].into_iter().flatten().collect::<Vec<_>>().join(" and ");
            let (Some(runner), Some(model)) = (runner.or(pair.fixed_runner.as_deref()), model) else {
                problems.push(format!("{}: set {} together (a model always belongs to its runner)", location, keys));
                continue;
            };
            if let Err(e) = validate_agent_pair(runner, model, effort) {
                problems.push(format!("{}.{}: {}", location, pair.model_key, e));
            }
        }
    }
    problems
}

/// `[agents.<id>]` tables of a `rula.toml`: reserved platform agent IDs (`system`, `git`,
/// `initializer`, `removal-resolver`, `config-resolver`) are rejected, and each persona declares
/// `runner` and `model` (and optionally `effort`) as a pair its runner's manifest declares.
fn agent_persona_problems(root: &toml::Value) -> Vec<String> {
    let Some(agents) = root.get("agents") else {
        return Vec::new();
    };
    let Some(agents) = agents.as_table() else {
        return vec!["agents: expected a table".to_string()];
    };
    let mut problems = Vec::new();
    for (agent_id, agent) in agents {
        if crate::kopeng::persona::RESERVED_AGENT_IDS
            .iter()
            .any(|id| id.eq_ignore_ascii_case(agent_id))
        {
            problems.push(format!(
                "agents.{}: '{}' is a platform agent and cannot be declared in rula.toml",
                agent_id, agent_id
            ));
            continue;
        }
        let field = |key: &str| agent.get(key).and_then(|v| v.as_str());
        match (field("runner"), field("model")) {
            (Some(runner), Some(model)) => {
                if let Err(e) = validate_agent_pair(runner, model, field("effort")) {
                    problems.push(format!("agents.{}: {}", agent_id, e));
                }
            }
            _ => problems.push(format!("agents.{}: runner and model are required (a model of the runner's manifest)", agent_id)),
        }
    }
    problems
}

/// Parses a configuration document and checks the type of every registered key it sets, its
/// runner/model/effort pairs and its agent personas.
fn parse_config_document(path: &Path, content: &str) -> Result<toml::Value, ConfigError> {
    let value: toml::Value = toml::from_str(content).map_err(|e| invalid(path, e.to_string()))?;
    let mut problems = schema_type_problems(&value);
    problems.extend(agent_pair_problems(&value));
    problems.extend(agent_persona_problems(&value));
    if !problems.is_empty() {
        return Err(invalid(path, problems.join("; ")));
    }
    Ok(value)
}

/// Graphics renderer transport mode for WebKitGTK (Spec 12 §7.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebkitRendererMode {
    Auto,
    Dmabuf,
    SharedMemory,
    Software,
}

impl WebkitRendererMode {
    /// Parses a configured `[system] webkit_renderer_mode` value.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "auto" => Some(Self::Auto),
            "dmabuf" => Some(Self::Dmabuf),
            "shared_memory" => Some(Self::SharedMemory),
            "software" => Some(Self::Software),
            _ => None,
        }
    }

    /// Canonical string identifier for logging and config serialization.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Dmabuf => "dmabuf",
            Self::SharedMemory => "shared_memory",
            Self::Software => "software",
        }
    }
}

/// Pre-boot typed read of `[system] webkit_renderer_mode` from `config.toml` (Spec 12 §7.4).
/// Returns `WebkitRendererMode::Auto` when the key is absent, logging an explicit warning to
/// stderr if `config.toml` fails validation or contains an unrecognized mode value.
pub fn read_webkit_renderer_mode() -> WebkitRendererMode {
    let doc = match read_global_config() {
        Ok(doc) => doc,
        Err(e) => {
            eprintln!("[WARN] config: failed to read config.toml for webkit_renderer_mode ({e}); using 'auto'");
            return WebkitRendererMode::Auto;
        }
    };
    let Some(system_tab) = doc.get("system").and_then(|t| t.as_table()) else {
        return WebkitRendererMode::Auto;
    };
    let Some(val) = system_tab.get("webkit_renderer_mode") else {
        return WebkitRendererMode::Auto;
    };
    let Some(raw) = val.as_str() else {
        eprintln!("[WARN] config: [system].webkit_renderer_mode is not a string; using 'auto'");
        return WebkitRendererMode::Auto;
    };
    match WebkitRendererMode::parse(raw) {
        Some(mode) => mode,
        None => {
            eprintln!(
                "[WARN] config: invalid [system].webkit_renderer_mode '{raw}' (expected auto|dmabuf|shared_memory|software); using 'auto'"
            );
            WebkitRendererMode::Auto
        }
    }
}

/// Migrates legacy `[plugins.snapshots]` entries into `[plugins.vcs]`, preserving comments and
/// formatting elsewhere in the TOML document. Returns `Ok(None)` when `[plugins.snapshots]` is absent.
pub fn migrate_toml_snapshots_to_vcs(content: &str) -> Result<Option<String>, String> {
    let parsed: toml::Value = toml::from_str(content).map_err(|e| format!("invalid TOML: {e}"))?;
    let Some(snap_tbl) = parsed
        .get("plugins")
        .and_then(|p| p.get("snapshots"))
        .and_then(|s| s.as_table())
    else {
        return Ok(None);
    };

    let existing_vcs = parsed
        .get("plugins")
        .and_then(|p| p.get("vcs"))
        .and_then(|v| v.as_table());

    let mut migrated_lines: Vec<String> = Vec::new();
    let mut push_if_absent = |target_key: &str, toml_val: &toml::Value| {
        if existing_vcs.is_some_and(|v| v.contains_key(target_key)) {
            return;
        }
        migrated_lines.push(format!("{} = {}", target_key, toml_val));
    };

    if let Some(val) = snap_tbl.get("enabled") {
        push_if_absent("snapshots_enabled", val);
    }
    if let Some(val) = snap_tbl.get("auto_snapshot_on_minutes") {
        if let Some(enabled) = val.as_bool() {
            let mode = toml::Value::String(if enabled { "snapshot" } else { "off" }.to_string());
            push_if_absent("minutes_checkpoint_mode", &mode);
        }
    }
    for key in [
        "auto_snapshot_on_file_change",
        "max_per_project_count",
        "auto_snapshot_on_init",
        "scratchpad_retention_days",
    ] {
        if let Some(val) = snap_tbl.get(key) {
            push_if_absent(key, val);
        }
    }

    let is_header = |line: &str| -> Option<String> {
        let code = line.split('#').next().unwrap_or_default().trim();
        let inner = code.strip_prefix('[')?.strip_suffix(']')?;
        (!inner.starts_with('[')).then(|| inner.trim().to_string())
    };

    let mut out_lines: Vec<String> = Vec::with_capacity(content.lines().count() + migrated_lines.len() + 2);
    let mut in_snapshots = false;
    let mut in_vcs = false;
    let mut inserted_into_vcs = false;

    for line in content.lines() {
        if let Some(hdr) = is_header(line) {
            if in_vcs && !inserted_into_vcs && !migrated_lines.is_empty() {
                while out_lines.last().is_some_and(|l| l.trim().is_empty()) {
                    out_lines.pop();
                }
                out_lines.extend(migrated_lines.iter().cloned());
                out_lines.push(String::new());
                inserted_into_vcs = true;
            }
            in_snapshots = hdr == "plugins.snapshots";
            in_vcs = hdr == "plugins.vcs";
            if in_snapshots {
                continue;
            }
        } else if in_snapshots {
            continue;
        }
        out_lines.push(line.to_string());
    }

    if in_vcs && !inserted_into_vcs && !migrated_lines.is_empty() {
        out_lines.extend(migrated_lines.iter().cloned());
        inserted_into_vcs = true;
    }

    if !inserted_into_vcs && !migrated_lines.is_empty() {
        if out_lines.last().is_some_and(|l| !l.trim().is_empty()) {
            out_lines.push(String::new());
        }
        out_lines.push("[plugins.vcs]".to_string());
        out_lines.extend(migrated_lines);
    }

    let mut updated = out_lines.join("\n");
    if content.ends_with('\n') || !updated.ends_with('\n') {
        updated.push('\n');
    }

    let reparsed: toml::Value =
        toml::from_str(&updated).map_err(|e| format!("migrated TOML failed to parse: {e}"))?;
    if reparsed
        .get("plugins")
        .and_then(|p| p.get("snapshots"))
        .is_some()
    {
        return Err("migration left [plugins.snapshots] behind".to_string());
    }
    Ok(Some(updated))
}

/// Reads and validates `config.toml`, writing the defaults first when it does not exist.
pub fn read_global_config() -> Result<toml::Value, ConfigError> {
    let path = get_global_config_path()?;
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs::write(&path, DEFAULT_GLOBAL_CONFIG_TOML).map_err(io_err(format!("Cannot write {}", path.display())))?;
            crate::logger::log("INFO", "config", &format!("Created default {}", path.display()));
            DEFAULT_GLOBAL_CONFIG_TOML.to_string()
        }
        Err(e) => return Err(io_err(format!("Cannot read {}", path.display()))(e)),
    };
    match migrate_toml_snapshots_to_vcs(&content) {
        Ok(Some(migrated)) => {
            let validated = parse_config_document(&path, &migrated)?;
            fs::write(&path, &migrated).map_err(io_err(format!("Cannot write migrated {}", path.display())))?;
            crate::logger::log("INFO", "config", &format!("Migrated [plugins.snapshots] into [plugins.vcs] in {}", path.display()));
            return Ok(validated);
        }
        Ok(None) => {}
        Err(e) => {
            crate::logger::log("WARN", "config", &format!("Skipping snapshots->vcs migration in {}: {}", path.display(), e));
        }
    }
    parse_config_document(&path, &content)
}

/// Startup check of `config.toml`: the app and the daemon refuse to start when it cannot be
/// read or is invalid. Primes the cache used by `load_global_config`.
pub fn validate_global_config() -> Result<(), ConfigError> {
    let value = read_global_config()?;
    let path = get_global_config_path()?;
    let mtime = fs::metadata(&path)
        .and_then(|m| m.modified())
        .map_err(io_err(format!("Cannot stat {}", path.display())))?;
    *GLOBAL_CONFIG_CACHE.write().unwrap_or_else(std::sync::PoisonError::into_inner) = Some((mtime, value));
    Ok(())
}

/// mtime of the last invalid `config.toml` version already reported, so it is logged once.
static GLOBAL_CONFIG_REPORTED_INVALID: Mutex<Option<SystemTime>> = Mutex::new(None);

/// Validated `config.toml` for the configuration cascade (hot path; cached by file mtime).
///
/// The file is validated at startup (`validate_global_config`) and on save. If it is later
/// edited into an invalid state, the last valid version stays in effect and the problem is
/// logged at ERROR once per file version, until the file is fixed.
pub fn load_global_config() -> toml::Value {
    let cached = || {
        GLOBAL_CONFIG_CACHE
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| toml::Value::Table(Default::default()))
    };
    let mtime = get_global_config_path().ok().and_then(|p| fs::metadata(p).and_then(|m| m.modified()).ok());
    if let Some(mtime) = mtime {
        let cache = GLOBAL_CONFIG_CACHE.read().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((cached_mtime, value)) = cache.as_ref() {
            if *cached_mtime == mtime {
                return value.clone();
            }
        }
    }
    match read_global_config() {
        Ok(value) => {
            if let Some(mtime) = mtime {
                *GLOBAL_CONFIG_CACHE.write().unwrap_or_else(std::sync::PoisonError::into_inner) = Some((mtime, value.clone()));
            }
            value
        }
        Err(e) => {
            // The lock is released before logging: the logger resolves its settings through
            // this function, and a held lock would deadlock that re-entry.
            let first_report = {
                let mut reported = GLOBAL_CONFIG_REPORTED_INVALID.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                let first = *reported != mtime;
                *reported = mtime;
                first
            };
            if first_report {
                crate::logger::log("ERROR", "config", &format!("{}; the last valid config.toml stays in effect until it is fixed", e));
            }
            cached()
        }
    }
}

const DEFAULT_GLOBAL_CONFIG_TOML: &str = r#"# ONE-SHIP Global Configuration

[system]
preferred_terminal = "ghostty"
log_level = "INFO"
dbus_call_timeout_seconds = 2
webkit_renderer_mode = "auto"

[plugins.appearance]
body_font = "IBM Plex Mono"
font_scale = "100% (Normal)"
quota_gauge_style = "concentric"

[plugins.orchestrator]
init_runner = "agy"
init_model = "gemini-3.8-flash"
init_effort = "high"
agent_runner = "agy"
default_model = "gemini-3.8-flash"
default_effort = "high"
default_docstyle = "sdd"
auto_approve = false

[plugins.accounts]
auto_warmup_enabled = true
context_warning_threshold_percent = 30.0
quota_alert_threshold_percent = 15.0
quota_danger_threshold_percent = 5.0

[plugins.supervisor]
max_buffer_lines_count = 500
shutdown_grace_period_ms = 3000
state_watcher_interval_ms = 2500

[plugins.telemetry]
telemetry_interval_ms = 2000

[plugins.profiler]
sample_interval_ms = 500
engine_shutdown_grace_period_ms = 15000
process_breakdown_max_items_count = 20

[plugins.vcs]
git_runner = "agy"
git_model = "gemini-3.8-flash"
git_effort = "high"
default_branch = "master"
github_repo_list_limit_count = 1000
gitlab_repo_list_limit_count = 100
snapshots_enabled = true
minutes_checkpoint_mode = "snapshot"
auto_commit_detail_level = "title_and_decisions"
auto_snapshot_on_file_change = true
max_per_project_count = 50
auto_snapshot_on_init = true
scratchpad_retention_days = 0
"#;

/// Last valid `config.toml` keyed by the file mtime it was read at. `resolve_effective_*` runs on
/// hot paths (logger, watcher ticks, supervisor loops); without this cache every call re-read
/// and re-parsed the file. Any write to the file changes its mtime and invalidates the entry.
static GLOBAL_CONFIG_CACHE: RwLock<Option<(SystemTime, toml::Value)>> = RwLock::new(None);

/// Validation result of each project's `rula.toml` keyed by path and mtime (the parsed document,
/// or the reason it is invalid), so the hot resolution path neither re-parses nor re-logs it.
static RULA_CACHE: std::sync::LazyLock<Mutex<HashMap<PathBuf, (SystemTime, RulaValidation)>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// Cached outcome of validating one version of a `rula.toml`.
#[derive(Clone)]
enum RulaValidation {
    Valid(toml::Value),
    Invalid { reason: String },
}

/// Parsed and validated `rula.toml` of a project; `Ok(None)` when the project has none.
pub fn read_project_rula(project_path: &Path) -> Result<Option<toml::Value>, ConfigError> {
    let path = project_path.join("rula.toml");
    let mtime = match fs::metadata(&path) {
        Ok(meta) => meta.modified().map_err(io_err(format!("Cannot stat {}", path.display())))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_err(format!("Cannot stat {}", path.display()))(e)),
    };
    {
        let cache = RULA_CACHE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((cached_mtime, validation)) = cache.get(&path) {
            if *cached_mtime == mtime {
                return match validation {
                    RulaValidation::Valid(value) => Ok(Some(value.clone())),
                    RulaValidation::Invalid { reason } => Err(invalid(&path, reason.clone())),
                };
            }
        }
    }
    let content = fs::read_to_string(&path).map_err(io_err(format!("Cannot read {}", path.display())))?;
    let result = parse_config_document(&path, &content);
    let cached = match &result {
        Ok(value) => Some(RulaValidation::Valid(value.clone())),
        Err(ConfigError::Invalid { reason, .. }) => Some(RulaValidation::Invalid { reason: reason.clone() }),
        Err(_) => None,
    };
    if let Some(entry) = cached {
        RULA_CACHE.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(path, (mtime, entry));
    }
    result.map(Some)
}

/// `rula.toml` of a project for the configuration cascade and the other readers; `None` when
/// absent or invalid. An invalid file does not block the project: it runs on the global
/// configuration while the watcher raises its `invalid_config` flag (`sync_project_config_flag`).
pub fn load_project_rula(project_path: &Path) -> Option<toml::Value> {
    match read_project_rula(project_path) {
        Ok(value) => value,
        Err(e) => {
            crate::logger::log("DEBUG", "config", &format!("load_project_rula: {}; project settings ignored", e));
            None
        }
    }
}

/// Raises or closes the project's `invalid_config` flag to match the validity of its `rula.toml`.
///
/// Called by the project watcher when it first sees a project and whenever `rula.toml` changes.
pub fn sync_project_config_flag(project_id: i64, project_path: &Path) -> Result<(), ConfigError> {
    use crate::kopeng::flag_resolver::INVALID_CONFIG_FLAG_TYPE;
    let pending: Vec<crate::db::ProjectInitFlagRecord> = crate::db::db_get_init_flags(project_id, Some("pending"))?
        .into_iter()
        .filter(|f| f.flag_type == INVALID_CONFIG_FLAG_TYPE)
        .collect();
    match read_project_rula(project_path) {
        Ok(_) => {
            for flag in pending {
                crate::db::db_resolve_init_flag(flag.id, "resolved", None)?;
                crate::logger::log("INFO", "config", &format!("rula.toml of {} is valid again; invalid_config flag {} resolved", project_path.display(), flag.id));
            }
        }
        Err(e) => {
            crate::logger::log("ERROR", "config", &e.to_string());
            if pending.is_empty() {
                crate::db::db_insert_init_flag(&crate::db::NewInitFlag {
                    project_id,
                    flag_type: INVALID_CONFIG_FLAG_TYPE.to_string(),
                    title: "Invalid rula.toml".to_string(),
                    description: Some(e.to_string()),
                    recommended_option: None,
                    options_json: None,
                    metadata_json: None,
                    raised_by_agent_uid: None,
                })?;
            }
        }
    }
    Ok(())
}

/// Resolves a setting value across the 4-level cascade:
/// Level 4: Project rula.toml [agents.<agent_name>]
/// Level 3: Project rula.toml [plugins.<plugin_id>]
/// Level 2: Global config.toml [plugins.<plugin_id>] (or [system])
/// Level 1: In-memory Schema default
pub fn resolve_effective_field(
    project_path: Option<&str>,
    agent_name: Option<&str>,
    plugin_id: &str,
    key: &str,
) -> (serde_json::Value, String) {
    // Levels 4 and 3: the project's rula.toml ([agents.<name>], then the plugin table).
    if let Some(rula) = project_path.and_then(|p| load_project_rula(Path::new(p))) {
        let agent_value = agent_name.and_then(|agent| rula.get("agents")?.get(agent)?.get(key));
        if let Some(val) = agent_value {
            return (toml_to_json(val), "agent".to_string());
        }
        if let Some(val) = plugin_table(&rula, plugin_id).and_then(|t| t.get(key)) {
            return (toml_to_json(val), "project".to_string());
        }
    }

    // Level 2: config.toml.
    if let Some(val) = plugin_table(&load_global_config(), plugin_id).and_then(|t| t.get(key)) {
        return (toml_to_json(val), "global".to_string());
    }

    // Level 1: the registered schema default.
    let field = get_all_plugin_schemas()
        .into_iter()
        .filter(|s| s.plugin_id == plugin_id)
        .flat_map(|s| s.fields)
        .find(|f| f.key == key);
    match field {
        Some(f) => (f.default_value, "default".to_string()),
        None => {
            crate::logger::log("ERROR", "config", &format!("Configuration key {}.{} is not registered in get_all_plugin_schemas()", plugin_id, key));
            (serde_json::Value::Null, "default".to_string())
        }
    }
}

pub fn resolve_effective_value(
    project_path: Option<&str>,
    agent_name: Option<&str>,
    plugin_id: &str,
    key: &str,
) -> serde_json::Value {
    resolve_effective_field(project_path, agent_name, plugin_id, key).0
}

pub fn resolve_effective_bool(
    project_path: Option<&str>,
    agent_name: Option<&str>,
    plugin_id: &str,
    key: &str,
) -> bool {
    let val = resolve_effective_value(project_path, agent_name, plugin_id, key);
    val.as_bool().unwrap_or(false)
}


pub fn resolve_effective_string(
    project_path: Option<&str>,
    agent_name: Option<&str>,
    plugin_id: &str,
    key: &str,
) -> String {
    let val = resolve_effective_value(project_path, agent_name, plugin_id, key);
    val.as_str().unwrap_or("").to_string()
}

/// Resolves a registered project-relative subdirectory setting safely within `project_path`.
///
/// Rejects empty values, absolute paths, and parent directory (`..`) traversals so subsystem
/// paths never escape the registered project root. Returns `(resolved_abs_path, normalized_rel_path)`.
pub fn resolve_project_subdir(
    project_path: &Path,
    plugin_id: &str,
    key: &str,
) -> Result<(PathBuf, String), ConfigError> {
    let p_str = project_path.to_string_lossy();
    let raw = resolve_effective_string(Some(&p_str), None, plugin_id, key);
    let trimmed = raw.trim();
    let cfg_key = format!("[plugins.{}].{}", plugin_id, key);
    if trimmed.is_empty() {
        return Err(ConfigError::Invalid {
            path: cfg_key,
            reason: "cannot be empty (use \".\" for project root)".to_string(),
        });
    }
    let rel = Path::new(trimmed);
    if rel.is_absolute() {
        return Err(ConfigError::Invalid {
            path: cfg_key,
            reason: format!("must be a project-relative path, got '{}'", trimmed),
        });
    }

    let mut normalized_parts: Vec<String> = Vec::new();
    let mut resolved = project_path.to_path_buf();
    for comp in rel.components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::Normal(seg) => {
                let s = seg.to_string_lossy();
                normalized_parts.push(s.into_owned());
                resolved.push(seg);
            }
            std::path::Component::ParentDir | std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err(ConfigError::Invalid {
                    path: cfg_key,
                    reason: format!("'{}' cannot traverse outside the project root", trimmed),
                });
            }
        }
    }

    if normalized_parts.is_empty() {
        Ok((project_path.to_path_buf(), ".".to_string()))
    } else {
        Ok((resolved, normalized_parts.join("/")))
    }
}

pub fn resolve_effective_string_list(
    project_path: Option<&str>,
    agent_name: Option<&str>,
    plugin_id: &str,
    key: &str,
) -> Vec<String> {
    let val = resolve_effective_value(project_path, agent_name, plugin_id, key);
    if let Some(arr) = val.as_array() {
        arr.iter().filter_map(|item| item.as_str().map(|s| s.to_string())).collect()
    } else {
        Vec::new()
    }
}

/// Resolves a registered integer configuration key as `usize`.
///
/// The schema (`get_all_plugin_schemas`) is the single source of default values, and document
/// validation guarantees every registered numeric key has a numeric value.
pub fn resolve_effective_usize(
    project_path: Option<&str>,
    agent_name: Option<&str>,
    plugin_id: &str,
    key: &str,
) -> usize {
    resolve_effective_u64(project_path, agent_name, plugin_id, key) as usize
}

/// Resolves a registered integer configuration key as `u64`.
///
/// The schema (`get_all_plugin_schemas`) is the single source of default values, and document
/// validation guarantees every registered numeric key has a numeric value.
pub fn resolve_effective_u64(
    project_path: Option<&str>,
    agent_name: Option<&str>,
    plugin_id: &str,
    key: &str,
) -> u64 {
    let val = resolve_effective_value(project_path, agent_name, plugin_id, key);
    debug_assert!(
        val.as_u64().is_some(),
        "Configuration key {}.{} did not resolve to a non-negative integer: {}",
        plugin_id,
        key,
        val
    );
    val.as_u64().unwrap_or(0)
}

/// Resolves a registered float configuration key as `f64`.
///
/// The schema (`get_all_plugin_schemas`) is the single source of default values, and document
/// validation guarantees every registered numeric key has a numeric value.
pub fn resolve_effective_f64(
    project_path: Option<&str>,
    agent_name: Option<&str>,
    plugin_id: &str,
    key: &str,
) -> f64 {
    let val = resolve_effective_value(project_path, agent_name, plugin_id, key);
    debug_assert!(
        val.as_f64().is_some(),
        "Configuration key {}.{} did not resolve to a float: {}",
        plugin_id,
        key,
        val
    );
    val.as_f64().unwrap_or(0.0)
}

fn toml_to_json(toml_val: &toml::Value) -> serde_json::Value {
    match toml_val {
        toml::Value::String(s) => serde_json::Value::String(s.clone()),
        toml::Value::Integer(i) => serde_json::Value::Number((*i).into()),
        toml::Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        toml::Value::Boolean(b) => serde_json::Value::Bool(*b),
        toml::Value::Array(arr) => serde_json::Value::Array(arr.iter().map(toml_to_json).collect()),
        toml::Value::Table(t) => {
            let mut map = serde_json::Map::new();
            for (k, v) in t {
                map.insert(k.clone(), toml_to_json(v));
            }
            serde_json::Value::Object(map)
        }
        toml::Value::Datetime(dt) => serde_json::Value::String(dt.to_string()),
    }
}

// ================= Daemon commands =================

/// Arguments of `save_global_config`: the whole `config.toml` document.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SaveGlobalConfigArgs {
    pub config_toml: String,
}

/// Arguments of `set_voice_controller_enabled`: toggles `[plugins.voice_controller].enabled`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SetVoiceControllerEnabledArgs {
    pub enabled: bool,
}

/// Arguments of `save_project_rula_config`: a TOML document whose `[plugins]` and `[system]`
/// tables replace the project's.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SaveProjectRulaConfigArgs {
    pub project_path: String,
    pub config_toml: String,
}

/// Daemon command `get_plugin_schemas`.
pub fn get_plugin_schemas() -> Vec<PluginConfigSchema> {
    get_all_plugin_schemas()
}

/// Daemon command `get_global_config`: `config.toml` as JSON, with keyring-backed secrets removed.
pub fn get_global_config() -> serde_json::Value {
    let val = load_global_config();
    let mut json = toml_to_json(&val);
    if let Some(voice) = json
        .get_mut("plugins")
        .and_then(|p| p.get_mut("voice_controller"))
        .and_then(|v| v.as_object_mut())
    {
        voice.remove("gemini_api_key");
    }
    json
}

/// Valid prebuilt Gemini Live voice names.
pub const GEMINI_LIVE_VOICES: &[&str] = &[
    "Zephyr",
    "Puck",
    "Charon",
    "Kore",
    "Fenrir",
    "Leda",
    "Orus",
    "Aoede",
    "Callirrhoe",
    "Autonoe",
    "Enceladus",
    "Iapetus",
    "Umbriel",
    "Algieba",
    "Despina",
    "Erinome",
    "Algenib",
    "Rasalgethi",
    "Laomedeia",
    "Achernar",
    "Alnilam",
    "Schedar",
    "Gacrux",
    "Pulcherrima",
    "Achird",
    "Zubenelgenubi",
    "Vindemiatrix",
    "Sadachbia",
    "Sadaltager",
    "Sulafat",
];

/// Global settings for the Gemini Live Voice Controller (Spec 36 & Spec 40).
#[derive(Clone, Debug)]
pub struct VoiceControllerConfig {
    pub enabled: bool,
    pub model: String,
    pub voice_name: String,
    pub thinking_level: String,
    pub persona_style: String,
    pub gemini_api_key: Option<String>,
    pub idle_disconnect_seconds: u64,
    pub turn_ledger_max_entries_count: usize,
    pub scratchpad_max_items_count: usize,
    pub context_compression_trigger_tokens: u64,
    pub context_compression_target_tokens: u64,
    pub voice_tool_response_max_bytes: usize,
}

static VOICE_CONFIG_LAST_WARN: Mutex<Option<String>> = Mutex::new(None);

fn warn_voice_config_once(warnings: &[String]) {
    if warnings.is_empty() {
        return;
    }
    let combined = warnings.join("; ");
    let should_log = {
        let mut last = VOICE_CONFIG_LAST_WARN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if last.as_deref() == Some(combined.as_str()) {
            false
        } else {
            *last = Some(combined.clone());
            true
        }
    };
    if should_log {
        crate::logger::log(
            "WARN",
            "config",
            &format!("Invalid [plugins.voice_controller] config value(s): {combined}"),
        );
    }
}

/// Reads `[plugins.voice_controller]` from the global `config.toml`.
/// Logs explicit warnings when present fields contain invalid types or unrecognized enum values.
pub fn get_voice_controller_config() -> VoiceControllerConfig {
    let val = load_global_config();
    let section = val
        .get("plugins")
        .and_then(|p| p.get("voice_controller"))
        .and_then(|s| s.as_table());

    let mut warnings: Vec<String> = Vec::new();

    let enabled = match section.and_then(|s| s.get("enabled")) {
        None => true,
        Some(v) => match v.as_bool() {
            Some(b) => b,
            None => {
                warnings.push("enabled must be boolean (using true)".to_string());
                true
            }
        },
    };

    let model = match section.and_then(|s| s.get("model")) {
        None => "gemini-3.8-live".to_string(),
        Some(v) => match v.as_str() {
            Some(s @ ("gemini-3.8-live" | "gemini-3.8-live-extended-thinking")) => s.to_string(),
            Some(other) => {
                warnings.push(format!("unsupported model '{other}' (using 'gemini-3.8-live')"));
                "gemini-3.8-live".to_string()
            }
            None => {
                warnings.push("model must be a string (using 'gemini-3.8-live')".to_string());
                "gemini-3.8-live".to_string()
            }
        },
    };

    let voice_name = match section.and_then(|s| s.get("voice_name")) {
        None => "Fenrir".to_string(),
        Some(v) => match v.as_str() {
            Some(s) if GEMINI_LIVE_VOICES.contains(&s) => s.to_string(),
            Some(other) => {
                warnings.push(format!("unsupported voice_name '{other}' (using 'Fenrir')"));
                "Fenrir".to_string()
            }
            None => {
                warnings.push("voice_name must be a string (using 'Fenrir')".to_string());
                "Fenrir".to_string()
            }
        },
    };

    let thinking_level = match section.and_then(|s| s.get("thinking_level")) {
        None => "low".to_string(),
        Some(v) => match v.as_str() {
            Some(s @ ("low" | "medium" | "high")) => s.to_string(),
            Some(other) => {
                warnings.push(format!("unsupported thinking_level '{other}' (using 'low')"));
                "low".to_string()
            }
            None => {
                warnings.push("thinking_level must be a string (using 'low')".to_string());
                "low".to_string()
            }
        },
    };

    let persona_style = match section.and_then(|s| s.get("persona_style")) {
        None => "belta".to_string(),
        Some(v) => match v.as_str() {
            Some(s @ ("belta" | "neutral")) => s.to_string(),
            Some(other) => {
                warnings.push(format!("unsupported persona_style '{other}' (using 'belta')"));
                "belta".to_string()
            }
            None => {
                warnings.push("persona_style must be a string (using 'belta')".to_string());
                "belta".to_string()
            }
        },
    };

    let gemini_api_key = get_gemini_api_key();

    let parse_positive = |key: &str, default_val: u64, min_val: u64, max_val: u64, warnings: &mut Vec<String>| -> u64 {
        match section.and_then(|s| s.get(key)) {
            None => default_val,
            Some(v) => match v.as_integer() {
                Some(n) if n >= min_val as i64 && n <= max_val as i64 => n as u64,
                Some(n) => {
                    warnings.push(format!(
                        "{key}={n} out of range [{min_val}..={max_val}] (using {default_val})"
                    ));
                    default_val
                }
                None => {
                    warnings.push(format!("{key} must be an integer (using {default_val})"));
                    default_val
                }
            },
        }
    };

    let idle_disconnect_seconds =
        parse_positive("idle_disconnect_seconds", 180, 15, 3600, &mut warnings);
    let turn_ledger_max_entries_count =
        parse_positive("turn_ledger_max_entries_count", 12, 2, 100, &mut warnings) as usize;
    let scratchpad_max_items_count =
        parse_positive("scratchpad_max_items_count", 20, 4, 100, &mut warnings) as usize;
    let context_compression_trigger_tokens =
        parse_positive("context_compression_trigger_tokens", 32000, 4000, 120000, &mut warnings);
    let mut context_compression_target_tokens =
        parse_positive("context_compression_target_tokens", 16000, 2000, 64000, &mut warnings);
    if context_compression_target_tokens >= context_compression_trigger_tokens {
        let clamped = (context_compression_trigger_tokens / 2).max(2000);
        warnings.push(format!(
            "context_compression_target_tokens ({context_compression_target_tokens}) must be < context_compression_trigger_tokens ({context_compression_trigger_tokens}); using {clamped}"
        ));
        context_compression_target_tokens = clamped;
    }
    let voice_tool_response_max_bytes =
        parse_positive("voice_tool_response_max_bytes", 6144, 1024, 32768, &mut warnings) as usize;

    warn_voice_config_once(&warnings);

    VoiceControllerConfig {
        enabled,
        model,
        voice_name,
        thinking_level,
        persona_style,
        gemini_api_key,
        idle_disconnect_seconds,
        turn_ledger_max_entries_count,
        scratchpad_max_items_count,
        context_compression_trigger_tokens,
        context_compression_target_tokens,
        voice_tool_response_max_bytes,
    }
}

/// Global settings for native Gemini Live dictation.
#[derive(Clone, Debug)]
pub struct DictationConfig {
    pub enabled: bool,
    pub mode: crate::dictation::DictationMode,
    pub auto_send: bool,
    pub speech_energy_threshold: f32,
    pub rollover_min_session_seconds: u64,
    pub rollover_silence_ms: u64,
    pub rollover_finalize_timeout_seconds: u64,
    pub watchdog_silence_seconds: u64,
    pub finalize_timeout_seconds: u64,
    pub reconnect_attempts_count: u32,
    pub recovery_min_interval_seconds: u64,
    pub stale_audio_retention_hours: u64,
}

static DICTATION_CONFIG_LAST_WARN: Mutex<Option<String>> = Mutex::new(None);

fn warn_dictation_config_once(warnings: &[String]) {
    if warnings.is_empty() {
        return;
    }
    let combined = warnings.join("; ");
    let should_log = {
        let mut last = DICTATION_CONFIG_LAST_WARN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if last.as_deref() == Some(combined.as_str()) {
            false
        } else {
            *last = Some(combined.clone());
            true
        }
    };
    if should_log {
        crate::logger::log(
            "WARN",
            "config",
            &format!("Invalid [plugins.dictation] config value(s): {combined}"),
        );
    }
}

/// Reads `[plugins.dictation]` from the global `config.toml`.
pub fn get_dictation_config() -> DictationConfig {
    let val = load_global_config();
    let section = val
        .get("plugins")
        .and_then(|p| p.get("dictation"))
        .and_then(|s| s.as_table());

    let mut warnings: Vec<String> = Vec::new();

    let enabled = match section.and_then(|s| s.get("enabled")) {
        None => true,
        Some(v) => match v.as_bool() {
            Some(b) => b,
            None => {
                warnings.push("enabled must be boolean (using true)".to_string());
                true
            }
        },
    };

    let mode = match section.and_then(|s| s.get("mode")) {
        None => crate::dictation::DictationMode::Smart,
        Some(v) => match v.as_str() {
            Some("smart") => crate::dictation::DictationMode::Smart,
            Some("verbatim") => crate::dictation::DictationMode::Verbatim,
            Some(other) => {
                warnings.push(format!("unsupported mode '{other}' (using 'smart')"));
                crate::dictation::DictationMode::Smart
            }
            None => {
                warnings.push("mode must be a string (using 'smart')".to_string());
                crate::dictation::DictationMode::Smart
            }
        },
    };

    let auto_send = match section.and_then(|s| s.get("auto_send")) {
        None => true,
        Some(v) => match v.as_bool() {
            Some(b) => b,
            None => {
                warnings.push("auto_send must be boolean (using true)".to_string());
                true
            }
        },
    };

    let parse_number = |key: &str, default_val: u64, min_val: u64, max_val: u64, warnings: &mut Vec<String>| -> u64 {
        match section.and_then(|s| s.get(key)) {
            None => default_val,
            Some(v) => match v.as_integer() {
                Some(n) if n >= min_val as i64 && n <= max_val as i64 => n as u64,
                Some(n) => {
                    warnings.push(format!("{key}={n} out of range [{min_val}..={max_val}] (using {default_val})"));
                    default_val
                }
                None => {
                    warnings.push(format!("{key} must be an integer (using {default_val})"));
                    default_val
                }
            },
        }
    };

    let speech_energy_threshold = match section.and_then(|s| s.get("speech_energy_threshold")) {
        None => 0.03,
        Some(v) => match v.as_float().or_else(|| v.as_integer().map(|n| n as f64)) {
            Some(n) if (0.005..=0.5).contains(&n) => n as f32,
            Some(n) => {
                warnings.push(format!(
                    "speech_energy_threshold={n} out of range [0.005..=0.5] (using 0.03)"
                ));
                0.03
            }
            None => {
                warnings.push("speech_energy_threshold must be a number (using 0.03)".to_string());
                0.03
            }
        },
    };

    let rollover_min_session_seconds =
        parse_number("rollover_min_session_seconds", 90, 30, 3600, &mut warnings);
    let rollover_silence_ms = parse_number("rollover_silence_ms", 800, 200, 5000, &mut warnings);
    let rollover_finalize_timeout_seconds =
        parse_number("rollover_finalize_timeout_seconds", 5, 1, 60, &mut warnings);
    let watchdog_silence_seconds =
        parse_number("watchdog_silence_seconds", 10, 3, 120, &mut warnings);
    let finalize_timeout_seconds =
        parse_number("finalize_timeout_seconds", 120, 10, 600, &mut warnings);
    let reconnect_attempts_count =
        parse_number("reconnect_attempts_count", 5, 1, 20, &mut warnings) as u32;
    let recovery_min_interval_seconds =
        parse_number("recovery_min_interval_seconds", 5, 1, 60, &mut warnings);
    let stale_audio_retention_hours =
        parse_number("stale_audio_retention_hours", 24, 1, 720, &mut warnings);

    warn_dictation_config_once(&warnings);

    DictationConfig {
        enabled,
        mode,
        auto_send,
        speech_energy_threshold,
        rollover_min_session_seconds,
        rollover_silence_ms,
        rollover_finalize_timeout_seconds,
        watchdog_silence_seconds,
        finalize_timeout_seconds,
        reconnect_attempts_count,
        recovery_min_interval_seconds,
        stale_audio_retention_hours,
    }
}

/// `[system].keyring_timeout_seconds`, bounded to a human-scale wait for a locked keyring.
pub fn keyring_timeout_seconds() -> u64 {
    resolve_effective_usize(None, None, "system", "keyring_timeout_seconds").clamp(5, 300) as u64
}

/// Status of the Gemini API key. The key itself is never serialized.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GeminiApiKeyStatus {
    pub configured: bool,
    pub keyring_available: bool,
    pub error: Option<String>,
}

/// Arguments of `set_gemini_api_key`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SetGeminiApiKeyArgs {
    pub api_key: String,
}

/// Arguments of `set_dictation_auto_send`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SetDictationAutoSendArgs {
    pub enabled: bool,
}

static GEMINI_API_KEY_CACHE: RwLock<Option<String>> = RwLock::new(None);
static GEMINI_API_KEY_ERROR: RwLock<Option<String>> = RwLock::new(None);

fn set_gemini_api_key_cache(key: Option<String>, error: Option<String>) {
    *GEMINI_API_KEY_CACHE
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = key;
    *GEMINI_API_KEY_ERROR
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = error;
}

/// Cached Gemini API key. `None` means the key has not been configured (or the keyring is
/// unavailable); callers surface a typed error instead of falling back to a plaintext file.
pub fn get_gemini_api_key() -> Option<String> {
    GEMINI_API_KEY_CACHE
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

/// Last keyring error recorded while loading or storing the Gemini API key.
pub fn gemini_api_key_error() -> Option<String> {
    GEMINI_API_KEY_ERROR
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

/// Reads the legacy plaintext key from `[plugins.voice_controller]`, if it is still present.
fn read_legacy_gemini_api_key() -> Option<String> {
    load_global_config()
        .get("plugins")
        .and_then(|p| p.get("voice_controller"))
        .and_then(|s| s.get("gemini_api_key"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Removes the `gemini_api_key` line from `[plugins.voice_controller]` while preserving the rest
/// of the user's TOML formatting and comments. Returns `true` when a line was removed.
fn remove_legacy_gemini_api_key_line(content: &str) -> (String, bool) {
    let mut out: Vec<&str> = Vec::with_capacity(content.lines().count());
    let mut in_voice_controller = false;
    let mut removed = false;
    for line in content.lines() {
        let code = match line.split('#').next() {
            Some(code) => code.trim(),
            None => line.trim(),
        };
        if code.starts_with('[') && code.ends_with(']') && !code.starts_with("[[") {
            in_voice_controller = code == "[plugins.voice_controller]";
            out.push(line);
            continue;
        }
        if in_voice_controller && code.starts_with("gemini_api_key") {
            if let Some((key, _)) = code.split_once('=') {
                if key.trim() == "gemini_api_key" {
                    removed = true;
                    continue;
                }
            }
        }
        out.push(line);
    }
    let mut updated = out.join("\n");
    if content.ends_with('\n') {
        updated.push('\n');
    }
    (updated, removed)
}

/// Loads the Gemini API key from the OS keyring into the daemon cache.
///
/// A legacy plaintext key found in `config.toml` is migrated exactly once: it is written to the
/// keyring, removed from the TOML, and the new location is logged. If the keyring is unavailable,
/// the legacy key stays available for the current daemon run, the error is surfaced to the UI and
/// the next successful save replaces it.
pub async fn init_gemini_api_key_cache() -> Result<(), ConfigError> {
    if let Some(key) = read_legacy_gemini_api_key() {
        match crate::secret_store::set(crate::secret_store::SecretKind::GeminiApiKey, &key).await {
            Ok(()) => {
                if let Err(e) = remove_legacy_gemini_api_key_from_config() {
                    crate::logger::log(
                        "WARN",
                        "config",
                        &format!("Migrated Gemini API key to the OS keyring but could not remove the legacy config line: {e}"),
                    );
                }
                crate::logger::log(
                    "INFO",
                    "config",
                    "Migrated the Gemini API key from config.toml to the OS keyring",
                );
                set_gemini_api_key_cache(Some(key), None);
                return Ok(());
            }
            Err(e) => {
                let message = format!("OS keyring unavailable while migrating the Gemini API key: {e}");
                crate::logger::log("ERROR", "config", &message);
                set_gemini_api_key_cache(Some(key), Some(message));
                return Ok(());
            }
        }
    }

    match crate::secret_store::get(crate::secret_store::SecretKind::GeminiApiKey).await {
        Ok(Some(key)) => set_gemini_api_key_cache(Some(key), None),
        Ok(None) => set_gemini_api_key_cache(None, None),
        Err(e) => {
            let message = format!("OS keyring unavailable while reading the Gemini API key: {e}");
            crate::logger::log("ERROR", "config", &message);
            set_gemini_api_key_cache(None, Some(message));
        }
    }
    Ok(())
}

fn remove_legacy_gemini_api_key_from_config() -> Result<(), ConfigError> {
    let path = get_global_config_path()?;
    let content = fs::read_to_string(&path).map_err(io_err(format!("Cannot read {}", path.display())))?;
    let (updated, removed) = remove_legacy_gemini_api_key_line(&content);
    if !removed {
        return Ok(());
    }
    parse_config_document(&path, &updated)?;
    fs::write(&path, &updated).map_err(io_err(format!("Cannot write {}", path.display())))?;
    Ok(())
}

/// Daemon command `get_gemini_api_key_status`.
pub fn get_gemini_api_key_status() -> GeminiApiKeyStatus {
    GeminiApiKeyStatus {
        configured: get_gemini_api_key().is_some(),
        keyring_available: crate::secret_store::is_available(),
        error: gemini_api_key_error(),
    }
}

/// Daemon command `set_gemini_api_key`: writes the key only to the OS keyring.
pub async fn set_gemini_api_key(args: SetGeminiApiKeyArgs) -> Result<GeminiApiKeyStatus, crate::secret_store::SecretStoreError> {
    crate::secret_store::set(crate::secret_store::SecretKind::GeminiApiKey, &args.api_key).await?;
    set_gemini_api_key_cache(Some(args.api_key.trim().to_string()), None);
    if let Err(e) = remove_legacy_gemini_api_key_from_config() {
        crate::logger::log(
            "WARN",
            "config",
            &format!("Gemini API key saved to the keyring but the legacy config line could not be removed: {e}"),
        );
    }
    let status = get_voice_controller_status_payload();
    crate::supervisor::get_supervisor()
        .emit_event(crate::events::DaemonEvent::voice_controller_state(&status));
    Ok(get_gemini_api_key_status())
}

/// Daemon command `clear_gemini_api_key`.
pub async fn clear_gemini_api_key() -> Result<GeminiApiKeyStatus, crate::secret_store::SecretStoreError> {
    crate::secret_store::clear(crate::secret_store::SecretKind::GeminiApiKey).await?;
    set_gemini_api_key_cache(None, None);
    let status = get_voice_controller_status_payload();
    crate::supervisor::get_supervisor()
        .emit_event(crate::events::DaemonEvent::voice_controller_state(&status));
    Ok(get_gemini_api_key_status())
}

fn get_voice_controller_status_payload() -> crate::voice_controller::VoiceControllerStatus {
    crate::voice_controller::get_voice_controller_status()
}

/// Daemon command `set_dictation_auto_send`.
pub fn set_dictation_auto_send(
    args: SetDictationAutoSendArgs,
) -> Result<crate::dictation::DictationStatus, ConfigError> {
    let path = get_global_config_path()?;
    let mut doc = read_global_config()?;
    let root = doc
        .as_table_mut()
        .ok_or_else(|| invalid(&path, "config.toml root must be a table"))?;
    let plugins = root
        .entry("plugins".to_string())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .ok_or_else(|| invalid(&path, "[plugins] must be a table"))?;
    let dictation = plugins
        .entry("dictation".to_string())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .ok_or_else(|| invalid(&path, "[plugins.dictation] must be a table"))?;
    dictation.insert(
        "auto_send".to_string(),
        toml::Value::Boolean(args.enabled),
    );

    let serialized = toml::to_string_pretty(&doc)
        .map_err(|e| invalid(&path, format!("Failed to serialize config.toml: {e}")))?;
    let validated = parse_config_document(&path, &serialized)?;
    fs::write(&path, &serialized).map_err(io_err(format!("Cannot write {}", path.display())))?;
    if let Ok(meta) = fs::metadata(&path) {
        if let Ok(mtime) = meta.modified() {
            *GLOBAL_CONFIG_CACHE
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((mtime, validated));
        }
    }
    crate::logger::log(
        "INFO",
        "config",
        &format!("Set [plugins.dictation].auto_send = {}", args.enabled),
    );
    Ok(crate::dictation::get_dictation_status())
}

/// Daemon command `set_voice_controller_enabled`: atomically updates
/// `[plugins.voice_controller].enabled` in `config.toml`, refreshes the global config cache,
/// and broadcasts the updated `voice-controller-state` event.
pub fn set_voice_controller_enabled(
    enabled: bool,
) -> Result<crate::voice_controller::VoiceControllerStatus, ConfigError> {
    let path = get_global_config_path()?;
    let mut doc = read_global_config()?;
    let root_tab = doc
        .as_table_mut()
        .ok_or_else(|| invalid(&path, "config.toml root must be a table"))?;
    let plugins_entry = root_tab
        .entry("plugins".to_string())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let plugins_tab = plugins_entry
        .as_table_mut()
        .ok_or_else(|| invalid(&path, "[plugins] must be a table"))?;
    let vc_entry = plugins_tab
        .entry("voice_controller".to_string())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let vc_tab = vc_entry
        .as_table_mut()
        .ok_or_else(|| invalid(&path, "[plugins.voice_controller] must be a table"))?;
    vc_tab.insert("enabled".to_string(), toml::Value::Boolean(enabled));

    let serialized = toml::to_string_pretty(&doc)
        .map_err(|e| invalid(&path, format!("Failed to serialize config.toml: {e}")))?;
    let validated = parse_config_document(&path, &serialized)?;
    fs::write(&path, &serialized).map_err(io_err(format!("Cannot write {}", path.display())))?;
    if let Ok(meta) = fs::metadata(&path) {
        if let Ok(mtime) = meta.modified() {
            *GLOBAL_CONFIG_CACHE
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((mtime, validated));
        }
    }
    crate::logger::log(
        "INFO",
        "config",
        &format!("Set [plugins.voice_controller].enabled = {enabled}"),
    );
    let status = crate::voice_controller::get_voice_controller_status();
    crate::supervisor::get_supervisor()
        .emit_event(crate::events::DaemonEvent::voice_controller_state(&status));
    Ok(status)
}

/// Persists `[system].default_projects_root_path` in `config.toml` and refreshes the config cache.
pub fn set_default_projects_root_path(root_path: &str) -> Result<(), ConfigError> {
    let path = get_global_config_path()?;
    let mut doc = read_global_config()?;
    let root_tab = doc
        .as_table_mut()
        .ok_or_else(|| invalid(&path, "config.toml root must be a table"))?;
    let sys_entry = root_tab
        .entry("system".to_string())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let sys_tab = sys_entry
        .as_table_mut()
        .ok_or_else(|| invalid(&path, "[system] must be a table"))?;
    sys_tab.insert(
        "default_projects_root_path".to_string(),
        toml::Value::String(root_path.to_string()),
    );

    let serialized = toml::to_string_pretty(&doc)
        .map_err(|e| invalid(&path, format!("Failed to serialize config.toml: {e}")))?;
    let validated = parse_config_document(&path, &serialized)?;
    fs::write(&path, &serialized).map_err(io_err(format!("Cannot write {}", path.display())))?;
    if let Ok(meta) = fs::metadata(&path) {
        if let Ok(mtime) = meta.modified() {
            *GLOBAL_CONFIG_CACHE
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((mtime, validated));
        }
    }
    crate::logger::log(
        "INFO",
        "config",
        &format!("Updated [system].default_projects_root_path = '{}'", root_path),
    );
    Ok(())
}

/// Daemon command `save_global_config`: validates and writes `config.toml`, then propagates the
/// daemon port to the runner MCP configs.
pub fn save_global_config(config_toml: String) -> Result<(), ConfigError> {
    let path = get_global_config_path()?;
    let parsed = parse_config_document(&path, &config_toml)?;
    if parsed
        .get("plugins")
        .and_then(|p| p.get("voice_controller"))
        .and_then(|v| v.get("gemini_api_key"))
        .is_some()
    {
        return Err(invalid(
            &path,
            "gemini_api_key is stored in the OS keyring and cannot be written to config.toml; use set_gemini_api_key",
        ));
    }
    fs::write(&path, &config_toml).map_err(io_err(format!("Cannot write {}", path.display())))?;
    crate::logger::log("INFO", "config", "Updated global config.toml");

    // Propagate the daemon port to the global agy config and every registered project's runner configs.
    let port = mcp_daemon_port();
    crate::logger::log_failure("config", "sync global agy MCP config", sync_antigravity_global_mcp_config(port));
    let projects = crate::registry::list_projects()?;
    for proj in &projects {
        let p_path = Path::new(&proj.path);
        if p_path.exists() {
            crate::logger::log_failure("config", "inject project MCP config", crate::kopeng::skill::inject_project_mcp_plugin(p_path));
        }
    }
    crate::logger::log("INFO", "config", &format!("Propagated MCP port {} to {} registered projects and global config", port, projects.len()));
    crate::db::notify_projects_updated();

    Ok(())
}


/// Port of the MCP daemon (`mcp.mcp_daemon_port`): the single source for the daemon bind address,
/// every daemon URL the app uses and the MCP endpoint written into runner configs.
pub fn mcp_daemon_port() -> u16 {
    resolve_effective_usize(None, None, "mcp", "mcp_daemon_port") as u16
}

/// Default of `mcp.event_channel_capacity_count`.
pub const DEFAULT_EVENT_CHANNEL_CAPACITY_COUNT: usize = 1024;

/// Capacity of the daemon event bus (`mcp.event_channel_capacity_count`, at least 1).
pub fn event_channel_capacity_count() -> usize {
    resolve_effective_usize(None, None, "mcp", "event_channel_capacity_count").max(1)
}

/// Writes the `one-ship` server into `~/.gemini/config/mcp_config.json` (agy's global MCP config).
///
/// A corrupt file, or one whose root or `mcpServers` is not an object, is an error: it is never
/// replaced, since it holds the user's other MCP servers.
pub fn sync_antigravity_global_mcp_config(mcp_port: u16) -> Result<(), ConfigError> {
    let gemini_config_dir = dirs::home_dir().ok_or(ConfigError::HomeUnavailable)?.join(".gemini").join("config");
    fs::create_dir_all(&gemini_config_dir).map_err(io_err(format!("Cannot create {}", gemini_config_dir.display())))?;
    let mcp_config_path = gemini_config_dir.join("mcp_config.json");

    let mut root: serde_json::Value = match fs::read_to_string(&mcp_config_path) {
        Ok(content) => serde_json::from_str(&content).map_err(|e| invalid(&mcp_config_path, e.to_string()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
        Err(e) => return Err(io_err(format!("Cannot read {}", mcp_config_path.display()))(e)),
    };
    let map = root.as_object_mut().ok_or_else(|| invalid(&mcp_config_path, "root is not a JSON object"))?;
    let servers = map
        .entry("mcpServers".to_string())
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| invalid(&mcp_config_path, "mcpServers is not a JSON object"))?;
    servers.insert(
        "one-ship".to_string(),
        serde_json::json!({
            "serverUrl": format!("http://localhost:{}/mcp", mcp_port),
            "timeoutSeconds": 86400
        }),
    );
    servers.insert(
        "desigma".to_string(),
        serde_json::json!({
            "serverUrl": format!("http://localhost:{}/mcp/desigma", mcp_port),
            "timeoutSeconds": 86400
        }),
    );
    servers.insert(
        "pcbcraft".to_string(),
        serde_json::json!({
            "serverUrl": format!("http://localhost:{}/mcp/pcbcraft", mcp_port),
            "timeoutSeconds": 86400
        }),
    );

    let formatted = serde_json::to_string_pretty(&root).map_err(|e| invalid(&mcp_config_path, e.to_string()))?;
    fs::write(&mcp_config_path, formatted).map_err(io_err(format!("Cannot write {}", mcp_config_path.display())))?;
    crate::logger::log("INFO", "config", &format!("Synchronized one-ship MCP server on port {} in ~/.gemini/config/mcp_config.json", mcp_port));
    Ok(())
}

#[derive(Serialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct ScopedConfigField {
    pub value: serde_json::Value,
    pub provenance: String, // "project", "global", "default"
}

/// Daemon command `get_scoped_project_config`: like `get_effective_project_config`, with the
/// level each value comes from.
pub fn get_scoped_project_config(project_path: String) -> std::collections::BTreeMap<String, ScopedConfigField> {
    let mut result = std::collections::BTreeMap::new();
    let schemas = get_all_plugin_schemas();
    for plugin in schemas {
        for field in plugin.fields {
            let (value, provenance) = resolve_effective_field(Some(&project_path), None, &plugin.plugin_id, &field.key);
            let full_key = format!("{}.{}", plugin.plugin_id, field.key);
            result.insert(full_key, ScopedConfigField { value, provenance: provenance.to_string() });
        }
    }
    result
}

/// Daemon command `save_project_rula_config`.
/// Replaces the `[plugins]` and `[system]` tables of a registered project's `rula.toml` with
/// those of `config_toml`, keeping every other table. An invalid current `rula.toml` is an
/// error (it is never replaced by the settings alone), and the result is validated before writing.
pub fn save_project_rula_config(project_path: String, config_toml: String) -> Result<(), ConfigError> {
    let project = crate::registry::find_project_by_path(&project_path)?;
    let rula_path = Path::new(&project.path).join("rula.toml");
    let mut existing_val = read_project_rula(Path::new(&project.path))?
        .ok_or_else(|| invalid(&rula_path, "the project has no rula.toml"))?;
    let parsed = parse_config_document(&rula_path, &config_toml)?;

    if let (Some(existing_tab), Some(new_tab)) = (existing_val.as_table_mut(), parsed.as_table()) {
        for section in ["plugins", "system"] {
            if let Some(new_section) = new_tab.get(section) {
                existing_tab.insert(section.to_string(), new_section.clone());
            }
        }
    }

    write_project_rula(Path::new(&project.path), &existing_val)?;
    crate::logger::log("INFO", "config", &format!("Updated project rula.toml for {}", project_path));
    crate::db::notify_projects_updated();
    Ok(())
}

/// Serializes `value` as the project's `rula.toml`, validating the result before writing it;
/// an invalid document is never written.
pub(crate) fn write_project_rula(project_path: &Path, value: &toml::Value) -> Result<(), ConfigError> {
    let rula_path = project_path.join("rula.toml");
    let updated_toml = toml::to_string_pretty(value).map_err(|e| invalid(&rula_path, e.to_string()))?;
    parse_config_document(&rula_path, &updated_toml)?;
    fs::write(&rula_path, updated_toml).map_err(io_err(format!("Cannot write {}", rula_path.display())))
}

/// Returns the full project-configurable plugin schema tree with current effective values and provenance.
pub fn get_project_config_tree(project_path: &str) -> serde_json::Value {
    let plugins: Vec<serde_json::Value> = get_all_plugin_schemas()
        .into_iter()
        .filter_map(|plugin| {
            let fields: Vec<serde_json::Value> = plugin
                .fields
                .into_iter()
                .filter(|f| f.scope != "global_only")
                .map(|f| {
                    let (current_value, provenance) =
                        resolve_effective_field(Some(project_path), None, &plugin.plugin_id, &f.key);
                    serde_json::json!({
                        "key": f.key,
                        "label": f.label,
                        "field_type": f.field_type,
                        "options": f.options,
                        "default_value": f.default_value,
                        "description": f.description,
                        "scope": f.scope,
                        "current_value": current_value,
                        "provenance": provenance,
                    })
                })
                .collect();
            if fields.is_empty() {
                None
            } else {
                Some(serde_json::json!({
                    "plugin_id": plugin.plugin_id,
                    "plugin_name": plugin.plugin_name,
                    "fields": fields,
                }))
            }
        })
        .collect();
    serde_json::json!({ "plugins": plugins })
}

fn json_value_to_toml(val: &serde_json::Value, path: &Path, key: &str) -> Result<toml::Value, ConfigError> {
    match val {
        serde_json::Value::Bool(b) => Ok(toml::Value::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(toml::Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(toml::Value::Float(f))
            } else {
                Err(invalid(path, format!("{key}: unsupported numeric value")))
            }
        }
        serde_json::Value::String(s) => Ok(toml::Value::String(s.clone())),
        serde_json::Value::Array(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                out.push(json_value_to_toml(item, path, key)?);
            }
            Ok(toml::Value::Array(out))
        }
        _ => Err(invalid(path, format!("{key}: expected scalar or array value"))),
    }
}

/// Validates and sets a single `[plugins.<plugin_id>].<field_key>` entry in a project's `rula.toml`.
pub fn set_project_config_field(
    project_path: &str,
    plugin_id: &str,
    field_key: &str,
    value: serde_json::Value,
) -> Result<serde_json::Value, ConfigError> {
    let rula_path = Path::new(project_path).join("rula.toml");
    let schemas = get_all_plugin_schemas();
    let plugin = schemas
        .iter()
        .find(|p| p.plugin_id == plugin_id)
        .ok_or_else(|| invalid(&rula_path, format!("unknown plugin_id '{plugin_id}'")))?;
    let field = plugin
        .fields
        .iter()
        .find(|f| f.key == field_key)
        .ok_or_else(|| invalid(&rula_path, format!("unknown field '{plugin_id}.{field_key}'")))?;
    if field.scope == "global_only" {
        return Err(invalid(
            &rula_path,
            format!("field '{plugin_id}.{field_key}' is global_only and cannot be set per project"),
        ));
    }

    let toml_val = json_value_to_toml(&value, &rula_path, &format!("{plugin_id}.{field_key}"))?;
    let mut existing = read_project_rula(Path::new(project_path))?
        .unwrap_or_else(|| toml::Value::Table(toml::map::Map::new()));
    let root_tab = existing
        .as_table_mut()
        .ok_or_else(|| invalid(&rula_path, "rula.toml root must be a table"))?;
    let plugins_entry = root_tab
        .entry("plugins".to_string())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let plugins_tab = plugins_entry
        .as_table_mut()
        .ok_or_else(|| invalid(&rula_path, "[plugins] must be a table"))?;
    let plugin_entry = plugins_tab
        .entry(plugin_id.to_string())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let plugin_tab = plugin_entry
        .as_table_mut()
        .ok_or_else(|| invalid(&rula_path, format!("[plugins.{plugin_id}] must be a table")))?;
    plugin_tab.insert(field_key.to_string(), toml_val);

    write_project_rula(Path::new(project_path), &existing)?;
    crate::db::notify_projects_updated();
    let (effective, provenance) = resolve_effective_field(Some(project_path), None, plugin_id, field_key);
    Ok(serde_json::json!({
        "ok": true,
        "plugin_id": plugin_id,
        "field_key": field_key,
        "current_value": effective,
        "provenance": provenance,
    }))
}

#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct RunnerFlags {
    pub model: Option<String>,
    pub effort: Option<String>,
    pub auto_approve: Option<String>,
    pub accept_edits: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct RunnerModel {
    pub id: String,
    pub label: String,
    /// Effort used when none is configured; required when `effort_levels` is not empty.
    pub default_effort: Option<String>,
    /// Supported efforts; empty for a model without an effort setting.
    #[serde(default)]
    pub effort_levels: Vec<String>,
    /// Context window of the model in tokens; when absent, no context utilization percentage is computed.
    #[ts(type = "number | null")]
    pub context_window_tokens_count: Option<u64>,
}

/// A runner manifest (`templates/runners/<id>.toml`): the only source of the runner → models →
/// efforts relation in the whole system. Nothing else declares, picks or substitutes models.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct RunnerSchema {
    pub id: String,
    pub name: String,
    pub binary: String,
    pub version: String,
    /// Lightest model of the runner, offered by the chat as the switch target when a heavier
    /// model runs out of quota. Must be one of `models`.
    pub quota_fallback_model: String,
    pub flags: RunnerFlags,
    pub models: Vec<RunnerModel>,
}

impl RunnerSchema {
    pub fn model(&self, model_id: &str) -> Option<&RunnerModel> {
        self.models.iter().find(|m| m.id == model_id)
    }

    fn declared_models(&self) -> String {
        self.models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>().join(", ")
    }
}

/// Manifest invariants: at least one model, unique model IDs, `default_effort` set exactly when
/// the model has effort levels and one of them, and `quota_fallback_model` declared.
fn manifest_problems(schema: &RunnerSchema) -> Vec<String> {
    let mut problems = Vec::new();
    if schema.models.is_empty() {
        problems.push("declares no models".to_string());
    }
    for (index, model) in schema.models.iter().enumerate() {
        if schema.models[..index].iter().any(|m| m.id == model.id) {
            problems.push(format!("model '{}' is declared twice", model.id));
        }
        match (&model.default_effort, model.effort_levels.is_empty()) {
            (Some(effort), false) if !model.effort_levels.contains(effort) => {
                problems.push(format!("model '{}': default_effort '{}' is not in effort_levels", model.id, effort))
            }
            (None, false) => problems.push(format!("model '{}' has effort_levels but no default_effort", model.id)),
            (Some(_), true) => problems.push(format!("model '{}' has a default_effort but no effort_levels", model.id)),
            _ => {}
        }
    }
    if schema.flags.effort.is_none() && schema.models.iter().any(|m| !m.effort_levels.is_empty()) {
        problems.push("models declare effort_levels but flags.effort is missing".to_string());
    }
    if schema.model(&schema.quota_fallback_model).is_none() {
        problems.push(format!("quota_fallback_model '{}' is not a declared model", schema.quota_fallback_model));
    }
    problems
}

/// Checks a runner/model/effort triple against the runner's manifest and returns the effort to
/// use: the given one when supported, the model's `default_effort` when none is given, `None` for
/// a model without efforts. An unknown runner or model, or an unsupported effort, is an error;
/// nothing is substituted.
pub fn validate_agent_pair(runner: &str, model: &str, effort: Option<&str>) -> Result<Option<String>, ConfigError> {
    let schema = load_runner_schema(runner)?;
    let Some(entry) = schema.model(model) else {
        return Err(ConfigError::UnknownModel { runner: runner.to_string(), model: model.to_string(), declared: schema.declared_models() });
    };
    match effort.filter(|e| !e.is_empty()) {
        Some(e) if entry.effort_levels.iter().any(|l| l == e) => Ok(Some(e.to_string())),
        Some(e) => Err(ConfigError::InvalidEffort {
            runner: runner.to_string(),
            model: model.to_string(),
            effort: e.to_string(),
            supported: if entry.effort_levels.is_empty() { "none".to_string() } else { entry.effort_levels.join(", ") },
        }),
        None => Ok(entry.default_effort.clone()),
    }
}

/// Loads the runner manifest `templates/runners/<runner>.toml` from the compile-time templates tree.
///
/// The manifest is the only source: an unknown runner, an unreadable file or an invalid manifest
/// (including a broken invariant, see `manifest_problems`) is an error.
pub fn load_runner_schema(runner: &str) -> Result<RunnerSchema, ConfigError> {
    let runners_dir = crate::kopeng::skill::canonical_templates_dir("runners")
        .map_err(|e| ConfigError::TemplatesNotFound(e.to_string()))?;
    if runner.is_empty() || !runner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(invalid(&runners_dir, format!("invalid runner id '{}'", runner)));
    }
    let path = runners_dir.join(format!("{}.toml", runner));
    let content = fs::read_to_string(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => ConfigError::UnknownRunner { runner: runner.to_string(), path: path.display().to_string() },
        _ => io_err(format!("Cannot read runner manifest {}", path.display()))(e),
    })?;
    let schema = toml::from_str::<RunnerSchema>(&content).map_err(|e| invalid(&path, e.to_string()))?;
    if schema.id != runner {
        return Err(invalid(&path, format!("id '{}' does not match the file name", schema.id)));
    }
    let problems = manifest_problems(&schema);
    if !problems.is_empty() {
        return Err(invalid(&path, problems.join("; ")));
    }
    Ok(schema)
}

/// Daemon command `list_agent_runners`: every runner with a manifest in `templates/runners/`,
/// sorted by ID: the only runners an agent persona may declare. An unreadable directory or
/// invalid manifest is an error.
pub fn list_agent_runners() -> Result<Vec<RunnerSchema>, ConfigError> {
    crate::logger::log("DEBUG", "config", "list_agent_runners");
    let runners_dir = crate::kopeng::skill::canonical_templates_dir("runners")
        .map_err(|e| ConfigError::TemplatesNotFound(e.to_string()))?;
    let io = |e| io_err(format!("Cannot read {}", runners_dir.display()))(e);
    let mut runners = Vec::new();
    for entry in fs::read_dir(&runners_dir).map_err(io)? {
        let path = entry.map_err(io)?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let Some(runner_id) = path.file_stem().and_then(|s| s.to_str()) else {
            return Err(invalid(&path, "runner manifest name is not UTF-8"));
        };
        runners.push(load_runner_schema(runner_id)?);
    }
    runners.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(runners)
}

pub fn resolve_snapshot_max_per_project(project_path: Option<&Path>) -> usize {
    let p_str = project_path.and_then(|p| p.to_str());
    resolve_effective_usize(p_str, None, "vcs", "max_per_project_count")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(content: &str) -> Result<toml::Value, ConfigError> {
        parse_config_document(Path::new("test.toml"), content)
    }

    #[test]
    fn default_configuration_and_schema_defaults_are_valid_pairs() {
        assert!(parse(DEFAULT_GLOBAL_CONFIG_TOML).is_ok());
        for schema in get_all_plugin_schemas() {
            let default = |key: &str| {
                schema.fields.iter().find(|f| f.key == key).and_then(|f| f.default_value.as_str().map(String::from)).expect("pair key registered")
            };
            for pair in &schema.agent_pairs {
                let runner = pair.runner_key.as_deref().map(default).or_else(|| pair.fixed_runner.clone()).unwrap();
                let effort = default(&pair.effort_key);
                assert!(validate_agent_pair(&runner, &default(&pair.model_key), Some(&effort)).is_ok(), "{}.{}", schema.plugin_id, pair.model_key);
            }
        }
    }

    #[test]
    fn plugin_pairs_are_set_together_and_must_be_declared_by_the_runner() {
        let runner_only = parse("[plugins.orchestrator]\nagent_runner = \"opencode\"\n").unwrap_err().to_string();
        assert!(runner_only.contains("set agent_runner and default_model together"), "{}", runner_only);
        let foreign = parse("[plugins.orchestrator]\nagent_runner = \"opencode\"\ndefault_model = \"gemini-3.8-flash\"\n").unwrap_err().to_string();
        assert!(foreign.contains("not declared by runner 'opencode'"), "{}", foreign);
        assert!(parse("[plugins.orchestrator]\nagent_runner = \"opencode\"\ndefault_model = \"deepseek/deepseek-flash\"\ndefault_effort = \"max\"\n").is_ok());
        let effort = parse("[plugins.orchestrator]\nagent_runner = \"opencode\"\ndefault_model = \"deepseek/deepseek-flash\"\ndefault_effort = \"medium\"\n");
        assert!(effort.is_err());
        // The probe's model id with an effort suffix is not an agy manifest model.
        assert!(parse("[plugins.accounts]\nquota_probe_model = \"gemini-3.8-flash-low\"\n").is_err());
        assert!(parse("[plugins.accounts]\nquota_probe_effort = \"low\"\n").is_err());
    }

    #[test]
    fn personas_need_a_runner_model_pair_of_the_manifest_and_cannot_use_reserved_ids() {
        let obsolete = parse("[agents.bosmang]\nrunner = \"agy\"\nmodel = \"claude-sonnet-4-6\"\n").unwrap_err().to_string();
        assert!(obsolete.contains("agents.bosmang"), "{}", obsolete);
        let reserved = parse("[agents.git]\nrunner = \"agy\"\nmodel = \"gemini-3.8-flash\"\n").unwrap_err().to_string();
        assert!(reserved.contains("agents.git") && reserved.contains("platform agent"), "{}", reserved);
        assert!(parse("[agents.ui]\nrunner = \"agy\"\n").is_err());
        assert!(parse("[agents.ui]\nmodel = \"gemini-3.8-flash\"\n").is_err());
        assert!(parse("[agents.ui]\nrunner = \"agy\"\nmodel = \"gemini-3.8-flash\"\neffort = \"low\"\n").is_ok());
    }

    #[test]
    fn manifest_invariants_are_enforced() {
        let mut schema = load_runner_schema("claude").unwrap();
        assert!(manifest_problems(&schema).is_empty());
        schema.quota_fallback_model = "nope".into();
        schema.models[0].default_effort = Some("ultra".into());
        let problems = manifest_problems(&schema);
        assert_eq!(problems.len(), 2, "{:?}", problems);
    }

    #[test]
    fn list_agent_runners_returns_every_manifest_sorted() {
        let ids: Vec<String> = list_agent_runners().unwrap().into_iter().map(|r| r.id).collect();
        assert_eq!(ids, vec!["agy", "claude", "opencode"]);
    }

    #[test]
    fn test_load_opencode_runner_schema() {
        let schema = load_runner_schema("opencode").expect("OpenCode schema must load");
        assert_eq!(schema.id, "opencode");
        assert_eq!(schema.binary, "opencode");
        assert!(schema.models.iter().any(|m| m.id == "deepseek/deepseek-flash"));
        assert!(schema.models.iter().any(|m| m.id == "deepseek/deepseek-v4-pro"));
    }

    #[test]
    fn test_load_agy_runner_schema() {
        let schema = load_runner_schema("agy").expect("Agy schema must load");
        assert_eq!(schema.id, "agy");
        assert!(schema.models.iter().any(|m| m.id.starts_with("gemini-")));
        assert!(schema.models.iter().any(|m| m.id == "gemini-4-argon"));
        let hydrogen = schema.models.iter().find(|m| m.id == "gemini-4-hydrogen").expect("gemini-4-hydrogen must exist");
        assert!(hydrogen.effort_levels.iter().any(|e| e == "xhigh"));
        assert!(schema.models.iter().any(|m| m.id == "claude-opus-5-5"));
        assert!(schema.models.iter().any(|m| m.id == "claude-sonnet-5-5"));
    }

    #[test]
    fn test_load_claude_runner_schema() {
        let schema = load_runner_schema("claude").expect("Claude schema must load");
        assert_eq!(schema.id, "claude");
        assert_eq!(schema.binary, "claude");
        assert_eq!(schema.flags.effort, Some("--effort".to_string()));
        assert!(schema.models.iter().any(|m| m.id == "claude-opus-5-5"));
        assert!(schema.models.iter().any(|m| m.id == "claude-sonnet-5-5"));
        assert!(schema.models.iter().any(|m| m.id == "claude-fable-5-1"));
        assert!(!schema.models.iter().any(|m| m.id.contains("haiku")));
        for m in &schema.models {
            assert!(!m.effort_levels.is_empty());
            assert!(m.default_effort.is_some());
        }
    }

    #[test]
    fn default_global_config_is_valid() {
        parse_config_document(Path::new("config.toml"), DEFAULT_GLOBAL_CONFIG_TOML).expect("defaults must validate");
    }

    #[test]
    fn registered_keys_with_wrong_types_are_reported() {
        let doc = "[system]\nlog_level = 3\n[plugins.orchestrator]\nauto_approve = \"yes\"\nunregistered = 1\n[plugins.vcs]\nmax_per_project_count = 10\n";
        let err = parse_config_document(Path::new("rula.toml"), doc).unwrap_err().to_string();
        assert!(err.contains("system.log_level"), "{}", err);
        assert!(err.contains("plugins.orchestrator.auto_approve: expected boolean"), "{}", err);
        assert!(!err.contains("unregistered") && !err.contains("max_per_project_count"), "{}", err);
        assert!(parse_config_document(Path::new("rula.toml"), "[project\n").is_err());
    }

    #[test]
    fn plugin_tables_are_read_only_from_their_canonical_location() {
        let doc: toml::Value = toml::from_str("[vcs]\na = 1\n[system]\nb = 2\n[plugins.orchestrator]\nc = 3\n").unwrap();
        assert!(plugin_table(&doc, "vcs").is_none());
        assert!(plugin_table(&doc, "orchestrator").is_some_and(|t| t.contains_key("c")));
        assert!(plugin_table(&doc, "system").is_some_and(|t| t.contains_key("b")));
    }

    #[test]
    fn invalid_rula_is_reported_and_ignored_by_the_cascade() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("rula.toml"), "[plugins.vcs]\nmax_per_project_count = \"many\"\n").unwrap();
        assert!(matches!(read_project_rula(tmp.path()), Err(ConfigError::Invalid { .. })));
        assert!(load_project_rula(tmp.path()).is_none());
        fs::remove_file(tmp.path().join("rula.toml")).unwrap();
        assert!(read_project_rula(tmp.path()).unwrap().is_none());
    }

    #[test]
    fn migrate_toml_snapshots_to_vcs_merges_and_removes_legacy_table() {
        let raw = "\
[project]
name = \"demo\"

[plugins.vcs]
default_agent_id = \"backend\"

[plugins.snapshots]
enabled = false
auto_snapshot_on_minutes = true
max_per_project_count = 25
";
        let migrated = migrate_toml_snapshots_to_vcs(raw).unwrap().expect("must migrate");
        let parsed: toml::Value = toml::from_str(&migrated).unwrap();
        assert!(parsed["plugins"].get("snapshots").is_none());
        let vcs = parsed["plugins"]["vcs"].as_table().unwrap();
        assert_eq!(vcs["default_agent_id"].as_str(), Some("backend"));
        assert_eq!(vcs["snapshots_enabled"].as_bool(), Some(false));
        assert_eq!(vcs["minutes_checkpoint_mode"].as_str(), Some("snapshot"));
        assert_eq!(vcs["max_per_project_count"].as_integer(), Some(25));
        assert!(migrate_toml_snapshots_to_vcs(&migrated).unwrap().is_none());
    }
}



