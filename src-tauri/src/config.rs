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
    /// When true, hidden behind the "Show Advanced" toggle by default.
    pub advanced: bool,
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
                    key: "show_welcome_on_startup".into(),
                    label: "Show Welcome Screen on Startup".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Shows the Welcome & Project Overview screen when launching ONE-SHIP. Always shown when no projects are registered.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "preferred_terminal".into(),
                    label: "Default Terminal App".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("ghostty"),
                    options: Some(vec![
                        "ghostty".into(),
                        "kitty".into(),
                        "alacritty".into(),
                        "foot".into(),
                        "wezterm".into(),
                        "ptyxis".into(),
                        "kgx".into(),
                        "gnome-terminal".into(),
                        "konsole".into(),
                        "xfce4-terminal".into(),
                        "x-terminal-emulator".into(),
                        "xterm".into(),
                    ]),
                    description: "Terminal application launched for account sign-in flows and interactive CLI sessions. Must be installed on your system.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "default_projects_root_path".into(),
                    label: "Default Projects Folder".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!(
                        dirs::home_dir()
                            .map(|h| h.join("Projects").to_string_lossy().into_owned())
                            .unwrap_or_else(|| ".".to_string())
                    ),
                    options: None,
                    description: "Base directory suggested when adding or creating new projects. Changing this only affects future project dialogs.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "log_level".into(),
                    label: "Diagnostic Log Detail".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("INFO"),
                    options: Some(vec![
                        "DEBUG".into(),
                        "INFO".into(),
                        "WARN".into(),
                        "ERROR".into(),
                    ]),
                    description: "Controls how much detail is recorded in oneship.log and the log viewer. DEBUG captures verbose diagnostics; ERROR records failures only.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "log_max_size_bytes".into(),
                    label: "Max Log File Size (Bytes)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(512_000),
                    options: None,
                    description: "Size limit in bytes before oneship.log rotates to a backup file. Lower values save disk space; higher values keep longer single-file history.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "log_max_rotations_count".into(),
                    label: "Rotated Log Backups".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(2),
                    options: None,
                    description: "Number of archived log files kept on disk after rotation. Older archives are deleted automatically.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "log_db_max_rows_count".into(),
                    label: "Database Log Retention (Rows)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(crate::logger::DEFAULT_LOG_DB_MAX_ROWS_COUNT),
                    options: None,
                    description: "Maximum number of recent log entries kept in SQLite. Higher values extend searchable history at the cost of database size.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "log_db_prune_interval_seconds".into(),
                    label: "Log Cleanup Interval (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(300),
                    options: None,
                    description: "How often (in seconds) the background logger prunes excess SQLite log rows.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "max_dir_walk_depth_count".into(),
                    label: "Max Indexing Folder Depth".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(32),
                    options: None,
                    description: "Maximum nested folder depth traversed during project discovery and semantic indexing. Increase only for deeply nested monorepos.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "max_dir_walk_nodes_count".into(),
                    label: "Max Indexing File Entries".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(50000),
                    options: None,
                    description: "Hard cap on filesystem entries visited per indexing pass to prevent hangs on symlink loops or massive generated trees.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "fs_tree_max_depth_count".into(),
                    label: "File Explorer Max Depth".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(12),
                    options: None,
                    description: "Maximum directory depth shown when scanning project files for UI pickers. Deeper files are omitted from listings.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "fs_tree_max_nodes_count".into(),
                    label: "File Explorer Max Items".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(5000),
                    options: None,
                    description: "Maximum entries listed when browsing project files; larger trees are truncated to keep the UI responsive.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "dbus_call_timeout_seconds".into(),
                    label: "System D-Bus Timeout (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(2),
                    options: None,
                    description: "Timeout in seconds for D-Bus diagnostic checks. Increase if slow desktop services cause false timeout warnings.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "keyring_timeout_seconds".into(),
                    label: "Keyring Prompt Timeout (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(30),
                    options: None,
                    description: "Maximum wait time in seconds for the OS Secret Service (keyring) to respond or unlock before reporting an error.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "webkit_renderer_mode".into(),
                    label: "Graphics Renderer Mode".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("auto"),
                    options: Some(vec![
                        "auto".into(),
                        "dmabuf".into(),
                        "shared_memory".into(),
                        "software".into(),
                    ]),
                    description: "WebKitGTK rendering backend. Switch to 'shared_memory' or 'software' if you see blank windows or GPU driver glitches. Requires restarting the UI.".into(),
                    scope: "global_only".into(),
                    advanced: true,
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
                    description: "Visual color palette for the entire interface. 'Auto (Omarchy)' follows your active Omarchy desktop theme in real time.".into(),
                    scope: "global_only".into(),
                    advanced: false,
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
                    description: "Primary monospace font family applied across panels, logs, and controls.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "font_scale".into(),
                    label: "Text & UI Scale".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("100% (Normal)"),
                    options: Some(vec![
                        "90% (Compact)".into(),
                        "100% (Normal)".into(),
                        "110% (Medium)".into(),
                        "120% (Large)".into(),
                        "130% (Extra Large)".into(),
                    ]),
                    description: "Scales typography and spacing globally. Use higher percentages on HiDPI monitors or lower for denser layouts.".into(),
                    scope: "global_only".into(),
                    advanced: false,
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
                    description: "Visual style for account quota and rolling window indicators in the taskbar (concentric rings, dual bars, or classic pie).".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
            ],
        },
        PluginConfigSchema {
            plugin_id: "voice_controller".into(),
            plugin_name: "Voice Controller (Gemini Live)".into(),
            agent_pairs: Vec::new(),
            fields: vec![
                ConfigFieldSchema {
                    key: "enabled".into(),
                    label: "Enable Voice Controller".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Enables the Push-to-Talk Gemini Live voice assistant in the top bar for hands-free orchestration.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "gemini_api_key".into(),
                    label: "Google AI Studio API Key".into(),
                    field_type: "secret".into(),
                    default_value: serde_json::json!(""),
                    options: None,
                    description: "API key required for both Voice Controller and Native Dictation. Stored securely in your OS keyring, never in config.toml.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "model".into(),
                    label: "Voice Assistant Model".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("gemini-3.8-live"),
                    options: Some(vec![
                        "gemini-3.8-live".into(),
                        "gemini-3.8-live-extended-thinking".into(),
                    ]),
                    description: "'gemini-3.8-live' prioritizes instant low-latency replies; 'extended-thinking' adds background reasoning for complex commands.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "voice_name".into(),
                    label: "Assistant Voice".into(),
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
                    description: "Studio voice persona used for spoken replies. Click Test to preview a sample.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "thinking_level".into(),
                    label: "Voice Reasoning Depth".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("low"),
                    options: Some(vec!["low".into(), "medium".into(), "high".into()]),
                    description: "Controls reasoning effort when using the 'extended-thinking' model. Higher levels improve complex planning but add response latency.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "persona_style".into(),
                    label: "Speaking Style".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("belta"),
                    options: Some(vec!["belta".into(), "neutral".into()]),
                    description: "'belta' gives the Ship Brain a Belter Creole tone; 'neutral' uses concise, standard technical phrasing.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "idle_disconnect_seconds".into(),
                    label: "Idle Disconnect Timeout (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(180),
                    options: None,
                    description: "Seconds of inactivity before closing the live WebSocket to save resources. Your working session is preserved in SQLite.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "turn_ledger_max_entries_count".into(),
                    label: "Verbatim Voice History (Turns)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(12),
                    options: None,
                    description: "Number of recent voice turns kept verbatim before older turns are folded into a rolling summary.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "scratchpad_max_items_count".into(),
                    label: "Voice Scratchpad Capacity".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(20),
                    options: None,
                    description: "Maximum active working notes retained in the voice orchestrator scratchpad.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "context_compression_trigger_tokens".into(),
                    label: "Context Compression Trigger (Tokens)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(32000),
                    options: None,
                    description: "Token count at which Gemini Live automatically compresses older conversation context. Lower values reduce token cost; higher values retain longer exact recall.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "context_compression_target_tokens".into(),
                    label: "Context Compression Target (Tokens)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(16000),
                    options: None,
                    description: "Target token size after context compression runs. Must stay below the compression trigger threshold.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "voice_tool_response_max_bytes".into(),
                    label: "Large Tool Response Warning (Bytes)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(6144),
                    options: None,
                    description: "Logs a warning (without truncating) when a tool response sent to Gemini Live exceeds this byte size.".into(),
                    scope: "global_only".into(),
                    advanced: true,
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
                    label: "Enable Voice Dictation".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Enables real-time speech-to-text dictation into ONE-SHIP input fields and agent prompts.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "mode".into(),
                    label: "Transcription Cleanup Mode".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("smart"),
                    options: Some(vec!["smart".into(), "verbatim".into()]),
                    description: "'smart' adds punctuation and removes filler words; 'verbatim' keeps the literal spoken words as transcribed.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "auto_send".into(),
                    label: "Auto-Send Dictated Prompts".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Automatically submits the message when dictation finishes inside an agent chat box. Regular text inputs are never auto-submitted.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "speech_energy_threshold".into(),
                    label: "Microphone Speech Threshold".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(0.03),
                    options: None,
                    description: "Normalized RMS audio level (0.0-1.0) used to tell speech apart from background noise. Raise if background noise prevents rollover; lower for quiet mics.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "rollover_min_session_seconds".into(),
                    label: "Session Rollover Min Age (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(90),
                    options: None,
                    description: "Minimum session age in seconds before a pause in speech can rotate to a fresh Live connection. Does not limit dictation length.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "rollover_silence_ms".into(),
                    label: "Rollover Silence Window (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(800),
                    options: None,
                    description: "Continuous silence duration in milliseconds needed to trigger a seamless session rollover once past the minimum age.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "rollover_finalize_timeout_seconds".into(),
                    label: "Rollover Finalize Wait (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(5),
                    options: None,
                    description: "Seconds to wait for the current session to flush its final words during rollover. Unconfirmed audio is replayed automatically.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "watchdog_silence_seconds".into(),
                    label: "Stalled Stream Watchdog (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(10),
                    options: None,
                    description: "Automatically reconnects if speech is detected for this many seconds without receiving any transcription from the server.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "finalize_timeout_seconds".into(),
                    label: "Stop Finalization Timeout (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(120),
                    options: None,
                    description: "Maximum wait in seconds for the final transcript after you stop dictating. Recorded audio is preserved on timeout.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "reconnect_attempts_count".into(),
                    label: "Max Reconnect Attempts".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(5),
                    options: None,
                    description: "Maximum automatic reconnection retries per dictation incident before reporting an error and saving recovery audio.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "recovery_min_interval_seconds".into(),
                    label: "Reconnect Cooldown (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(5),
                    options: None,
                    description: "Minimum delay in seconds between consecutive recovery attempts to avoid hammering the upstream endpoint.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "stale_audio_retention_hours".into(),
                    label: "Recovery Audio Retention (h)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(24),
                    options: None,
                    description: "Hours to keep unrecovered dictation audio files on disk before cleanup removes them at startup.".into(),
                    scope: "global_only".into(),
                    advanced: true,
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
                    label: "Project Setup Runner".into(),
                    field_type: "runner".into(),
                    default_value: serde_json::json!("agy"),
                    options: None,
                    description: "CLI runner used when onboarding or initializing a new project.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "init_model".into(),
                    label: "Project Setup Model".into(),
                    field_type: "runner_model".into(),
                    default_value: serde_json::json!("gemini-3.8-flash"),
                    options: None,
                    description: "AI model used by the Project Setup Runner during initialization.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "init_effort".into(),
                    label: "Project Setup Reasoning Effort".into(),
                    field_type: "runner_effort".into(),
                    default_value: serde_json::json!("high"),
                    options: None,
                    description: "Reasoning depth for project initialization. Higher effort improves architecture discovery but takes longer.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "agent_runner".into(),
                    label: "Default Agent Runner".into(),
                    field_type: "runner".into(),
                    default_value: serde_json::json!("agy"),
                    options: None,
                    description: "Fallback CLI runner for platform agents without a specific persona or project override.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "default_model".into(),
                    label: "Default AI Model".into(),
                    field_type: "runner_model".into(),
                    default_value: serde_json::json!("gemini-3.8-flash"),
                    options: None,
                    description: "Default AI model paired with the Default Agent Runner.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "default_effort".into(),
                    label: "Default Reasoning Effort".into(),
                    field_type: "runner_effort".into(),
                    default_value: serde_json::json!("high"),
                    options: None,
                    description: "Default reasoning depth for platform agents (empty uses the model's built-in default).".into(),
                    scope: "global_only".into(),
                    advanced: false,
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
                    description: "Living documentation structure scaffolded when initializing new projects (Spec-Driven, Lean, Contract, Test-Anchored, or Custom).".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "auto_approve".into(),
                    label: "Auto-Approve Agent Tool Calls".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(false),
                    options: None,
                    description: "Allows spawned CLI agents to run tools without pausing for interactive confirmation. Enable only in trusted workspaces.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "agent_spawn_poll_step_ms".into(),
                    label: "Agent Startup Poll Interval (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(250),
                    options: None,
                    description: "Polling delay in milliseconds while verifying an agent subprocess is ready to accept stdin.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "agent_log_tail_lines_count".into(),
                    label: "Failure Diagnostic Tail (Lines)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(50),
                    options: None,
                    description: "Number of recent output lines attached to agent crash or startup failure reports.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "init_timeout_minutes".into(),
                    label: "Project Setup Timeout (min)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(15),
                    options: None,
                    description: "Maximum minutes allowed for unattended project initialization before marking it failed. Greenfield sessions have no timeout.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "port_release_timeout_ms".into(),
                    label: "Port Cleanup Timeout (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(3000),
                    options: None,
                    description: "Maximum milliseconds to wait for a conflicting port to become free after terminating its occupant.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "port_release_poll_step_ms".into(),
                    label: "Port Cleanup Poll Step (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(50),
                    options: None,
                    description: "Interval in milliseconds between port availability checks during port release.".into(),
                    scope: "global_only".into(),
                    advanced: true,
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
                    label: "Auto-Start Rolling Quota Window".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Automatically sends a tiny 1-turn probe when an account's quota resets so its rolling cooldown timer starts immediately.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "context_warning_threshold_percent".into(),
                    label: "Context Usage Warning (%)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(30.0),
                    options: None,
                    description: "Shows a compaction reminder once an agent's context window exceeds this percentage. Lower values encourage earlier compaction.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "quota_alert_threshold_percent".into(),
                    label: "Low Quota Warning (%)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(15.0),
                    options: None,
                    description: "Emits a warning indicator when remaining account quota drops below this percentage.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "quota_danger_threshold_percent".into(),
                    label: "Critical Quota Alert (%)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(5.0),
                    options: None,
                    description: "Triggers a critical alert state when remaining account quota drops below this percentage.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "stats_refresh_interval_seconds".into(),
                    label: "Quota Refresh Interval (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(60),
                    options: None,
                    description: "How often (in seconds) background quota telemetry refreshes. Lower values update gauges faster.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "quota_depleted_threshold_percent".into(),
                    label: "Warmup Depleted Threshold (%)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(5.0),
                    options: None,
                    description: "Remaining quota at or below which a pool is treated as depleted and eligible for automatic warmup probing.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "quota_probe_cooldown_seconds".into(),
                    label: "Warmup Probe Cooldown (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(900),
                    options: None,
                    description: "Minimum seconds between consecutive warmup probes on the same account to avoid redundant API calls.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "quota_probe_timeout_ms".into(),
                    label: "Warmup Probe Timeout (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(7000),
                    options: None,
                    description: "Maximum milliseconds a warmup probe waits for agy statusline output before failing.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "quota_probe_poll_step_ms".into(),
                    label: "Warmup Probe Poll Step (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(150),
                    options: None,
                    description: "Polling interval in milliseconds when reading the statusline observation file during a warmup probe.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "quota_probe_model".into(),
                    label: "Warmup Probe Model".into(),
                    field_type: "runner_model".into(),
                    default_value: serde_json::json!("gemini-3.8-flash"),
                    options: None,
                    description: "Lightweight agy model used to execute the 1-turn quota warmup probe.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "quota_probe_effort".into(),
                    label: "Warmup Probe Reasoning Effort".into(),
                    field_type: "runner_effort".into(),
                    default_value: serde_json::json!("low"),
                    options: None,
                    description: "Reasoning effort for the warmup probe. Keep at 'low' to minimize token usage and latency.".into(),
                    scope: "global_only".into(),
                    advanced: true,
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
                    description: "Project agent persona assigned to Git operations and 'Open Agent' sessions in this repository.".into(),
                    scope: "project_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "git_runner".into(),
                    label: "Git Agent Runner".into(),
                    field_type: "runner".into(),
                    default_value: serde_json::json!("agy"),
                    options: None,
                    description: "CLI runner used for Git assistant sessions and AI-generated commit messages.".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "git_model".into(),
                    label: "Git Agent Model".into(),
                    field_type: "runner_model".into(),
                    default_value: serde_json::json!("gemini-3.8-flash"),
                    options: None,
                    description: "AI model used for Git operations and commit message generation.".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "git_effort".into(),
                    label: "Git Agent Reasoning Effort".into(),
                    field_type: "runner_effort".into(),
                    default_value: serde_json::json!("high"),
                    options: None,
                    description: "Reasoning depth for Git agent tasks (empty uses the model's built-in default).".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "default_branch".into(),
                    label: "Initial Repository Branch".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("master"),
                    options: None,
                    description: "Branch name created when initializing a brand-new Git repository.".into(),
                    scope: "global_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "snapshots_enabled".into(),
                    label: "Enable Safety Snapshots".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Master switch for automatic project safety snapshots and rollback checkpoints.".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "minutes_checkpoint_mode".into(),
                    label: "Session Minutes Checkpoint".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("snapshot"),
                    options: Some(vec!["snapshot".into(), "commit".into(), "off".into()]),
                    description: "Action taken when an agent records session minutes: save a hidden snapshot ref, create a real Git commit, or do nothing.".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "auto_commit_detail_level".into(),
                    label: "Auto-Commit Message Detail".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("title_and_decisions"),
                    options: Some(vec!["title_only".into(), "title_and_decisions".into(), "full".into()]),
                    description: "Controls how much session minute context is written into automatic Git commits when Checkpoint Mode is set to 'commit'.".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "auto_snapshot_on_file_change".into(),
                    label: "Auto-Snapshot on File Save".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Automatically captures a safety snapshot when project files are modified.".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "max_per_project_count".into(),
                    label: "Max Snapshots Per Project".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(50),
                    options: None,
                    description: "Maximum safety snapshots kept per project before the oldest ones are pruned automatically.".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "auto_snapshot_on_init".into(),
                    label: "Genesis Snapshot on Setup".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Creates an initial baseline snapshot right after project initialization finishes.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "scratchpad_retention_days".into(),
                    label: "Scratchpad Auto-Prune (Days)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(0),
                    options: None,
                    description: "Deletes scratchpad/ files older than N days automatically. Set to 0 to never auto-delete.".into(),
                    scope: "cascade".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "github_repo_list_limit_count".into(),
                    label: "GitHub Repo Fetch Limit".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(1000),
                    options: None,
                    description: "Maximum repositories fetched by `gh repo list` in the Remote VCS picker.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "gitlab_repo_list_limit_count".into(),
                    label: "GitLab Repo Fetch Limit".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(100),
                    options: None,
                    description: "Maximum repositories fetched by `glab api projects` in the Remote VCS picker.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "forge_cli_timeout_seconds".into(),
                    label: "Remote Git / Forge Timeout (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(60),
                    options: None,
                    description: "Maximum seconds to wait for `gh`, `glab`, or remote Git commands before aborting.".into(),
                    scope: "global_only".into(),
                    advanced: true,
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
                    description: "Enables the Kanban board, milestone timeline, and GitHub/GitLab collaboration tab for this project.".into(),
                    scope: "project_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "tracking_mode".into(),
                    label: "Milestone Progress Mode".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("linear"),
                    options: Some(vec!["linear".into(), "shape_up".into(), "weighted".into()]),
                    description: "'linear' rolls up task completion; 'shape_up' uses Hill Charts and appetite; 'weighted' tracks commercial baseline weights and drift.".into(),
                    scope: "project_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "github_cache_ttl_seconds".into(),
                    label: "Forge Cache Duration (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(120),
                    options: None,
                    description: "Seconds before cached GitHub/GitLab issues, PRs, and releases refresh from the remote CLI. Lower values show fresher data with more API calls.".into(),
                    scope: "cascade".into(),
                    advanced: true,
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
                    label: "Enable Semantic Search".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Enables local vector search (LanceDB), background AST indexing, and cross-project knowledge lookup.".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "graph_tab_enabled".into(),
                    label: "Show Knowledge Graph Tab".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Displays the interactive Knowledge Graph tab inside the project workspace.".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "ignored_patterns".into(),
                    label: "Excluded Folders & Patterns".into(),
                    field_type: "string_list".into(),
                    default_value: serde_json::json!([
                        "target", "node_modules", "dist", "build", ".git", ".env*", "*.min.js", "*.lock", "__pycache__", ".venv", "venv", "*.pyc", "scratchpad"
                    ]),
                    options: None,
                    description: "Comma-separated glob patterns skipped by semantic indexing. Separate from .gitignore so local docs and specs can still be indexed.".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "included_extensions".into(),
                    label: "Indexed File Extensions".into(),
                    field_type: "string_list".into(),
                    default_value: serde_json::json!([
                        "rs", "ts", "tsx", "js", "jsx", "py", "vue", "svelte", "astro",
                        "go", "java", "cs", "c", "cpp", "h", "hpp", "rb", "php",
                        "css", "sql", "sh", "md", "txt", "pdf"
                    ]),
                    options: None,
                    description: "File extensions parsed for code symbols and documentation chunks. Add custom extensions if your stack uses others.".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "extra_paths".into(),
                    label: "Extra Documentation Folders".into(),
                    field_type: "string_list".into(),
                    default_value: serde_json::json!([
                        "docs", "specs", "devlog", "research", "manuals"
                    ]),
                    options: None,
                    description: "Additional project folders indexed recursively for specs, research notes, devlogs, and manuals.".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "semantic_edge_min_score_percent".into(),
                    label: "Graph Link Similarity (%)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(70),
                    options: None,
                    description: "Minimum similarity (1-100%) needed to link specs, tasks, and journals to code in the Knowledge Graph. Higher values yield fewer, stricter links.".into(),
                    scope: "cascade".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "max_file_size_bytes".into(),
                    label: "Max Indexed File Size (Bytes)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(262144),
                    options: None,
                    description: "Skips files larger than this byte limit (default 256 KiB; 0 = unlimited) to avoid indexing huge generated bundles.".into(),
                    scope: "cascade".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "file_watch_debounce_ms".into(),
                    label: "File Change Re-Index Delay (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(10000),
                    options: None,
                    description: "Wait time in milliseconds after saving files before background re-indexing starts. Higher values reduce CPU churn during rapid edits.".into(),
                    scope: "cascade".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "entity_watch_debounce_ms".into(),
                    label: "DB Entity Re-Index Delay (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(2000),
                    options: None,
                    description: "Wait time in milliseconds after updating tasks, debt, or minutes before refreshing their search vectors.".into(),
                    scope: "cascade".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "embedding_batch_size_count".into(),
                    label: "Embedding Batch Size".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(10),
                    options: None,
                    description: "Text chunks vectorized per ONNX pass before writing to LanceDB. Larger batches index faster but use more RAM.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "embedding_batch_sleep_ms".into(),
                    label: "Embedding Batch Pause (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(20),
                    options: None,
                    description: "Brief pause in milliseconds between embedding batches to keep the system responsive during indexing.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "throttler_min_free_mem_mbytes".into(),
                    label: "Background Index Min Free RAM (MiB)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(2048),
                    options: None,
                    description: "Background indexing pauses automatically if available system RAM falls below this threshold.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "throttler_critical_free_mem_mbytes".into(),
                    label: "Manual Index Min Free RAM (MiB)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(512),
                    options: None,
                    description: "Minimum free RAM (MiB) required even when you manually trigger semantic indexing.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "throttler_max_cpu_percent".into(),
                    label: "Background Index Max CPU (%)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(45),
                    options: None,
                    description: "Background indexing yields whenever total host CPU usage exceeds this percentage.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "onnx_intra_threads_count".into(),
                    label: "Embedding Engine CPU Threads".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(2),
                    options: None,
                    description: "CPU threads allocated to the local BGE-M3 ONNX model. Higher counts speed up indexing at the expense of CPU load.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "max_relations_per_result_count".into(),
                    label: "Graph Relations Per Search Hit".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(6),
                    options: None,
                    description: "Maximum 1-hop knowledge graph links attached to each semantic search result.".into(),
                    scope: "global_only".into(),
                    advanced: true,
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
                    description: "Enables run-bound CPU/memory telemetry, flamegraphs, and the Profiling tab for this project.".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "sample_interval_ms".into(),
                    label: "Sampling Interval (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(500),
                    options: None,
                    description: "Milliseconds between process-tree samples for running services and tasks. Lower values give finer resolution with larger trace storage.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "engine_shutdown_grace_period_ms".into(),
                    label: "Profiler Flush Grace Period (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(crate::supervisor::DEFAULT_ENGINE_SHUTDOWN_GRACE_PERIOD_MS),
                    options: None,
                    description: "Wait time between SIGINT and SIGKILL when stopping a profiled service so the profiler engine can flush its output file.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "process_breakdown_max_items_count".into(),
                    label: "Process Breakdown Limit".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(crate::duck::DEFAULT_PROCESS_BREAKDOWN_MAX_ITEMS_COUNT),
                    options: None,
                    description: "Maximum top processes (sorted by average CPU) shown in a run profile summary.".into(),
                    scope: "global_only".into(),
                    advanced: true,
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
                    description: "Enables .desigma visual prototyping and Designer MCP tools for this project.".into(),
                    scope: "project_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "design_dir".into(),
                    label: "Design Files Folder".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("design"),
                    options: None,
                    description: "Project-relative folder where .desigma files are stored and exposed to desigma-mcp.".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "enable_bridge".into(),
                    label: "Live Agent Canvas Sync (--bridge)".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(false),
                    options: None,
                    description: "Starts One-Ship Designer with --bridge so agents can modify the open canvas live.".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "auto_snapshot_on_edit".into(),
                    label: "Auto-Snapshot Design Edits".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Automatically saves a scoped safety snapshot when an agent or editor modifies design files.".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "mcp_binary_path".into(),
                    label: "Designer MCP Binary Path".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("desigma-mcp"),
                    options: None,
                    description: "Executable name on PATH or absolute path to the desigma-mcp server binary. Also editable in Settings → Integrations.".into(),
                    scope: "cascade".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "app_binary_path".into(),
                    label: "Designer App Binary Path".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("oneship-designer"),
                    options: None,
                    description: "Executable name on PATH or absolute path to the One-Ship Designer desktop binary. Also editable in Settings → Integrations.".into(),
                    scope: "cascade".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "mcp_request_timeout_ms".into(),
                    label: "Designer MCP Timeout (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(30000),
                    options: None,
                    description: "Maximum milliseconds to wait for a single desigma-mcp tool response.".into(),
                    scope: "cascade".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "max_snapshots_per_asset_count".into(),
                    label: "Max Undo Snapshots Per File".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(20),
                    options: None,
                    description: "Maximum undo snapshots retained per .desigma file before oldest ones are pruned.".into(),
                    scope: "cascade".into(),
                    advanced: true,
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
                    label: "Enable SolidCraft 3D CAD".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(false),
                    options: None,
                    description: "Enables 3D CAD preview, physical metrics inspection, and external SolidCraft launching for this project.".into(),
                    scope: "project_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "cad_dir".into(),
                    label: "CAD Files Folder".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("cad"),
                    options: None,
                    description: "Project-relative folder watched for .sc, .step, .stp, and .3mf CAD models.".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "auto_snapshot_on_edit".into(),
                    label: "Auto-Snapshot CAD Edits".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Automatically saves a scoped safety snapshot whenever CAD files are modified.".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "app_binary_path".into(),
                    label: "SolidCraft Editor Binary Path".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("solidcraft"),
                    options: None,
                    description: "Executable name on PATH or absolute path to the SolidCraft desktop editor. Also editable in Settings → Integrations.".into(),
                    scope: "cascade".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "cli_binary_path".into(),
                    label: "SolidCraft CLI Binary Path".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("solidcraft-cli"),
                    options: None,
                    description: "Executable name on PATH or absolute path to solidcraft-cli for headless renders and physical metrics. Also editable in Settings → Integrations.".into(),
                    scope: "cascade".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "snapshot_timeout_ms".into(),
                    label: "CAD Render & Eval Timeout (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(15000),
                    options: None,
                    description: "Maximum milliseconds for headless solidcraft-cli snapshot or geometry evaluation calls. Increase for complex assemblies.".into(),
                    scope: "cascade".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "snapshot_width_px".into(),
                    label: "CAD Preview Width (px)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(1280),
                    options: None,
                    description: "Width in pixels of rendered CAD preview images. Higher resolution takes slightly longer to render.".into(),
                    scope: "cascade".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "snapshot_height_px".into(),
                    label: "CAD Preview Height (px)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(800),
                    options: None,
                    description: "Height in pixels of rendered CAD preview images.".into(),
                    scope: "cascade".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "max_snapshots_per_asset_count".into(),
                    label: "Max Undo Snapshots Per File".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(20),
                    options: None,
                    description: "Maximum CAD undo snapshots retained per file before older ones are pruned.".into(),
                    scope: "cascade".into(),
                    advanced: true,
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
                    description: "Enables schematic/PCB SVG preview, BOM & DRC inspection, and the PCBCraft MCP gateway for this project.".into(),
                    scope: "project_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "pcb_dir".into(),
                    label: "PCB Files Folder".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("pcb"),
                    options: None,
                    description: "Project-relative folder watched for PCBCraft project directories and .pc board archives.".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "auto_snapshot_on_edit".into(),
                    label: "Auto-Snapshot PCB Edits".into(),
                    field_type: "boolean".into(),
                    default_value: serde_json::json!(true),
                    options: None,
                    description: "Automatically saves a scoped safety snapshot when an agent or external editor modifies PCB files.".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "app_binary_path".into(),
                    label: "PCBCraft Editor Binary Path".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("pcbcraft"),
                    options: None,
                    description: "Executable name on PATH or absolute path to the PCBCraft desktop editor. Also editable in Settings → Integrations.".into(),
                    scope: "cascade".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "cli_binary_path".into(),
                    label: "PCBCraft CLI Binary Path".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("pcbcraft-cli"),
                    options: None,
                    description: "Executable name on PATH or absolute path to pcbcraft-cli for DRC/BOM inspection. Also editable in Settings → Integrations.".into(),
                    scope: "cascade".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "mcp_request_timeout_ms".into(),
                    label: "PCBCraft MCP Timeout (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(30000),
                    options: None,
                    description: "Maximum milliseconds to wait for a single pcbcraft-cli MCP response.".into(),
                    scope: "cascade".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "max_snapshots_per_asset_count".into(),
                    label: "Max Undo Snapshots Per Board".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(20),
                    options: None,
                    description: "Maximum PCB undo snapshots retained per board target before older ones are pruned.".into(),
                    scope: "cascade".into(),
                    advanced: true,
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
                    description: "Enables benchtop instrument telemetry, serial monitor, and microcontroller firmware build/flash tools for this project.".into(),
                    scope: "project_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "firmware_dir".into(),
                    label: "Firmware Folder".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("."),
                    options: None,
                    description: "Project-relative folder containing the firmware project (platformio.ini, src/, include/).".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "schematics_dir".into(),
                    label: "Schematics Folder".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("."),
                    options: None,
                    description: "Project-relative folder containing EDA schematics (.kicad_sch) and SPICE netlists (.cir).".into(),
                    scope: "cascade".into(),
                    advanced: false,
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
                    description: "Initial serial baud rate used when opening the microcontroller serial monitor. Must match your firmware UART configuration.".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "flasher_tool".into(),
                    label: "Firmware Flashing Tool".into(),
                    field_type: "select_enum".into(),
                    default_value: serde_json::json!("pio"),
                    options: Some(vec![
                        "pio".into(),
                        "probe-rs".into(),
                        "esptool".into(),
                    ]),
                    description: "CLI tool used to upload compiled firmware to target boards (PlatformIO, probe-rs, or esptool).".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "usb_flush_max_attempts_count".into(),
                    label: "USB Endpoint Flush Attempts".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(256),
                    options: None,
                    description: "Maximum bulk read attempts when draining stale USB packets from benchtop instruments before a new command.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "firmware_upload_timeout_seconds".into(),
                    label: "Firmware Upload Timeout (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(180),
                    options: None,
                    description: "Maximum seconds allowed for `mpremote` or `pio` firmware uploads before aborting.".into(),
                    scope: "global_only".into(),
                    advanced: true,
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
                    description: "Enables REA MCP tools, isolated .one-ship/rea evidence storage, and the RE Lab tab for this project.".into(),
                    scope: "project_only".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "targets_dir".into(),
                    label: "Target Binaries Folder".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("targets"),
                    options: None,
                    description: "Project-relative folder scanned for target binaries, firmware images, and packages to analyze.".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "ghidra_install_dir".into(),
                    label: "Ghidra Installation Folder".into(),
                    field_type: "string".into(),
                    default_value: serde_json::json!("/opt/ghidra"),
                    options: None,
                    description: "Local path to your Ghidra installation (must contain support/analyzeHeadless). Also editable in Settings → Integrations.".into(),
                    scope: "cascade".into(),
                    advanced: false,
                },
                ConfigFieldSchema {
                    key: "max_concurrent_heavy_jobs_count".into(),
                    label: "Max Concurrent Decompilations".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(1),
                    options: None,
                    description: "Maximum heavy Ghidra headless jobs allowed to run simultaneously across all projects; extra jobs queue in FIFO order.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "idle_timeout_seconds".into(),
                    label: "Engine Idle Auto-Stop (s)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(600),
                    options: None,
                    description: "Seconds of inactivity before stopping a project's REA HTTP engine to free RAM. Restarts automatically on next use.".into(),
                    scope: "cascade".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "max_jvm_heap_mbytes".into(),
                    label: "Ghidra Max JVM Heap (MB)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(2048),
                    options: None,
                    description: "Maximum JVM memory (-Xmx) allocated to headless Ghidra workers. Raise for large binaries; lower on memory-constrained hosts.".into(),
                    scope: "cascade".into(),
                    advanced: true,
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
                    label: "Handoff Brief Recent Turns".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(20),
                    options: None,
                    description: "Maximum recent chat turns included in the handoff Brief when seeding a fresh native session.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "search_conversations_max_results_count".into(),
                    label: "Conversation Search Limit".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(10),
                    options: None,
                    description: "Maximum conversations returned by a single conversation search query.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "brief_max_bytes".into(),
                    label: "Handoff Brief Max Size (Bytes)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(32768),
                    options: None,
                    description: "Hard byte limit for a session handoff Brief; lower-priority sections are trimmed first when exceeded.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "identity_ancestor_walk_max_depth_count".into(),
                    label: "Caller Process Ancestor Depth".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(16),
                    options: None,
                    description: "Maximum parent-process hops walked when matching an incoming MCP caller to its registered agent process.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "max_buffer_lines_count".into(),
                    label: "Process Output Buffer (Lines)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(500),
                    options: None,
                    description: "Recent stdout/stderr lines kept in memory per supervised process for live inspection.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "max_task_capture_lines_count".into(),
                    label: "Task Report Output Limit (Lines)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(20000),
                    options: None,
                    description: "Maximum output lines captured per task execution report to prevent unbounded memory growth.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "max_agent_response_bytes".into(),
                    label: "Max Agent Turn Buffer (Bytes)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(4_194_304),
                    options: None,
                    description: "Maximum bytes accumulated for a single agent turn response/thought stream before truncation.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "max_pending_inputs_count".into(),
                    label: "Max Queued Agent Prompts".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(crate::supervisor::DEFAULT_MAX_PENDING_INPUTS_COUNT),
                    options: None,
                    description: "Prompts allowed to queue while an agent is busy; additional inputs are rejected until the queue drains.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "shutdown_grace_period_ms".into(),
                    label: "Process Stop Grace Period (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(3000),
                    options: None,
                    description: "Milliseconds waited after SIGTERM before force-killing a supervised process.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "state_watcher_interval_ms".into(),
                    label: "Supervisor Sync Interval (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(2500),
                    options: None,
                    description: "Milliseconds between background supervisor state reconciliation sweeps.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "shutdown_step_delay_ms".into(),
                    label: "Shutdown Step Delay (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(150),
                    options: None,
                    description: "Short delay in milliseconds between graceful shutdown steps.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "process_poll_step_ms".into(),
                    label: "Process Poll Step (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(100),
                    options: None,
                    description: "Polling delay in milliseconds while waiting for process restarts or exits.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "watcher_debounce_ms".into(),
                    label: "File Watcher Debounce (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(1500),
                    options: None,
                    description: "Debounce window in milliseconds for workspace filesystem change events.".into(),
                    scope: "global_only".into(),
                    advanced: true,
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
                    description: "Local TCP port bound by the ONE-SHIP daemon HTTP/MCP server. Requires restarting the daemon to take effect.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "task_wait_timeout_ms".into(),
                    label: "Default Task Wait Timeout (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(30000),
                    options: None,
                    description: "Wait time in milliseconds for start_project_task(wait=true) when no timeout is specified; afterwards the task continues in the background.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "approval_ttl_ms".into(),
                    label: "Approval Request Expiration (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(300_000),
                    options: None,
                    description: "Milliseconds before an unanswered interactive user approval request expires automatically.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "event_channel_capacity_count".into(),
                    label: "Event Bus Buffer Capacity".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(DEFAULT_EVENT_CHANNEL_CAPACITY_COUNT),
                    options: None,
                    description: "Events buffered per /api/events subscriber. Slow clients that overflow receive a resync event.".into(),
                    scope: "global_only".into(),
                    advanced: true,
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
                    label: "Host Telemetry Poll Interval (ms)".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(2000),
                    options: None,
                    description: "Milliseconds between OS CPU, RAM, and network usage samples shown in the header telemetry charts.".into(),
                    scope: "global_only".into(),
                    advanced: true,
                },
                ConfigFieldSchema {
                    key: "db_connection_pool_max_count".into(),
                    label: "SQLite Connection Pool Size".into(),
                    field_type: "number".into(),
                    default_value: serde_json::json!(4),
                    options: None,
                    description: "Maximum concurrent SQLite connections in the daemon pool.".into(),
                    scope: "global_only".into(),
                    advanced: true,
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

/// Arguments of `set_show_welcome_on_startup`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SetShowWelcomeOnStartupArgs {
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
    set_system_config_value(
        "default_projects_root_path",
        toml::Value::String(root_path.to_string()),
    )?;
    crate::logger::log(
        "INFO",
        "config",
        &format!("Updated [system].default_projects_root_path = '{}'", root_path),
    );
    Ok(())
}

/// Daemon command `set_show_welcome_on_startup`: persists `[system].show_welcome_on_startup` in `config.toml`.
pub fn set_show_welcome_on_startup(args: SetShowWelcomeOnStartupArgs) -> Result<(), ConfigError> {
    set_system_config_value("show_welcome_on_startup", toml::Value::Boolean(args.enabled))?;
    crate::logger::log(
        "INFO",
        "config",
        &format!("Updated [system].show_welcome_on_startup = {}", args.enabled),
    );
    Ok(())
}

fn set_system_config_value(key: &str, value: toml::Value) -> Result<(), ConfigError> {
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
    sys_tab.insert(key.to_string(), value);

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



