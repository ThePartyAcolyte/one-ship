use std::path::Path;
use serde::{Deserialize, Serialize};
use crate::registry::ProjectEntry;
use crate::kopeng::git::{self, GitStatus};
use crate::kopeng::skill::{self, SkillStatus};

/// Declarative option exposed by a project service/task script in `rula.toml`.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct ScriptOption {
    pub key: String,
    pub flag: String,
    pub label: String,
    #[serde(default = "default_script_option_type")]
    pub r#type: String, // "boolean" | "select"
    #[serde(default)]
    pub choices: Vec<String>,
    pub default: serde_json::Value,
}

fn default_script_option_type() -> String {
    "boolean".to_string()
}

/// Service or task definition parsed from a project's `rula.toml`.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct ScriptEntry {
    pub id: String,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub is_persistent: bool,
    pub options: Vec<ScriptOption>,
    /// Profiler engine id the service runs wrapped by (`profiler = "<id>"`, Spec 25 §4.4).
    pub profiler: Option<String>,
    /// Declared `port` of a persistent script (the supervisor's adoption and conflict guard).
    #[serde(default)]
    pub port: Option<u16>,
}

/// A registered project with its git, configuration, skill and init state (project cards).
#[derive(Serialize, ts_rs::TS)]
#[ts(export)]
pub struct HydratedProject {
    pub id: usize,
    pub path: String,
    pub name: String,
    pub description: Option<String>,
    
    // Git State
    pub has_git: bool,
    pub git_status: Option<GitStatus>,
    /// Why `git_status` is absent although `has_git` is true.
    pub git_error: Option<String>,
    
    // Config State
    pub has_config: bool,
    pub scripts: Vec<ScriptEntry>,

    // Platform Skill State
    pub skill_status: SkillStatus,
    
    // Init State
    pub init_state: Option<crate::db::ProjectInitState>,

    // Reverse Engineering Lab (REA) State
    pub has_rea_enabled: bool,

    // Continuous Profiler State
    pub has_profiler_enabled: bool,

    // Electronics & Embedded Lab State
    pub has_electronics_enabled: bool,

    // Desigma Visual Design State
    pub has_desigma_enabled: bool,

    // SolidCraft 3D CAD State
    pub has_solidcraft_enabled: bool,

    // PCBCraft EDA / PCB State
    pub has_pcbcraft_enabled: bool,

    // Project Management State (Spec 34)
    pub has_project_management_enabled: bool,

    // Knowledge Graph Tab Visibility (`[plugins.semantic].graph_tab_enabled`)
    pub has_knowledge_graph_enabled: bool,

    // Agnostic Web Preview URL declared in `rula.toml` (`[preview] url = "..."`)
    pub preview_url: Option<String>,

    // Workspace & Project Preferences
    pub is_favorite: bool,
    pub is_archived: bool,
    pub hidden_tabs: Vec<String>,
}


fn parse_script_option(opt_key: &str, opt_val: &toml::Value) -> ScriptOption {
    let flag = opt_val.get("flag").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let label = opt_val.get("label").and_then(|v| v.as_str()).unwrap_or(opt_key).to_string();
    let r#type = opt_val.get("type").and_then(|v| v.as_str()).unwrap_or("boolean").to_string();
    let choices: Vec<String> = opt_val.get("choices").and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let default_json: serde_json::Value = if let Some(def_toml) = opt_val.get("default") {
        serde_json::to_value(def_toml).unwrap_or_else(|_| {
            if r#type == "select" {
                serde_json::json!(choices.first().cloned().unwrap_or_default())
            } else {
                serde_json::json!(false)
            }
        })
    } else if r#type == "select" {
        serde_json::json!(choices.first().cloned().unwrap_or_default())
    } else {
        serde_json::json!(false)
    };

    ScriptOption {
        key: opt_key.to_string(),
        flag,
        label,
        r#type,
        choices,
        default: default_json,
    }
}

fn parse_script_entry(id: &str, s: &toml::Value, is_persistent: bool) -> ScriptEntry {
    let name = s.get("name").and_then(|v| v.as_str()).unwrap_or(id).to_string();
    let command = s.get("command").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let args: Vec<String> = s.get("args").and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let cwd = s.get("cwd").and_then(|v| v.as_str()).map(|s| s.to_string());

    let mut options = Vec::new();
    if let Some(opts_table) = s.get("options").and_then(|v| v.as_table()) {
        for (opt_key, opt_val) in opts_table {
            options.push(parse_script_option(opt_key, opt_val));
        }
    }

    // A non-string value is rejected at launch (`find_declared_script`).
    let profiler = s.get("profiler").and_then(|v| v.as_str()).map(|s| s.to_string());
    let port = s.get("port").and_then(|v| v.as_integer()).and_then(|p| u16::try_from(p).ok());

    ScriptEntry {
        id: id.to_string(),
        name,
        command,
        args,
        cwd,
        is_persistent,
        options,
        profiler,
        port,
    }
}

/// Section of `rula.toml` a script is declared in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptKind {
    /// `[scripts.persistent.<id>]`: a long-running service.
    Persistent,
    /// `[scripts.task.<id>]`: a finite task.
    Task,
}

impl ScriptKind {
    fn table(self) -> &'static str {
        match self {
            ScriptKind::Persistent => "persistent",
            ScriptKind::Task => "task",
        }
    }
}

/// Failure to resolve a declared script or its launch options.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("Project has no rula.toml declaring [scripts.{kind}.{script_id}]")]
    NoConfig { kind: &'static str, script_id: String },
    #[error("Script '{script_id}' is not declared under [scripts.{kind}]")]
    NotDeclared { kind: &'static str, script_id: String },
    #[error("Script '{script_id}' declares no command")]
    MissingCommand { script_id: String },
    #[error("Script '{script_id}' has no option '{key}'")]
    UnknownOption { script_id: String, key: String },
    #[error("Option '{key}' of script '{script_id}': {details}")]
    InvalidOption { script_id: String, key: String, details: String },
    #[error("Script '{script_id}' declares 'profiler' as something other than an engine id (profiler = \"<id>\")")]
    InvalidProfiler { script_id: String },
    #[error(transparent)]
    Config(#[from] crate::config::ConfigError),
}

impl crate::api::ApiError for ScriptError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::NoConfig { .. } | Self::NotDeclared { .. } => StatusCode::NOT_FOUND,
            Self::MissingCommand { .. }
            | Self::UnknownOption { .. }
            | Self::InvalidOption { .. }
            | Self::InvalidProfiler { .. } => StatusCode::BAD_REQUEST,
            Self::Config(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::NoConfig { .. } => "ScriptError::NoConfig",
            Self::NotDeclared { .. } => "ScriptError::NotDeclared",
            Self::MissingCommand { .. } => "ScriptError::MissingCommand",
            Self::UnknownOption { .. } => "ScriptError::UnknownOption",
            Self::InvalidOption { .. } => "ScriptError::InvalidOption",
            Self::InvalidProfiler { .. } => "ScriptError::InvalidProfiler",
            Self::Config(e) => e.code(),
        }
    }
}

/// Reads the script `script_id` declared under `[scripts.<kind>]` of the project's `rula.toml`.
///
/// An unreadable or invalid `rula.toml` is an error: a script is never launched from a guessed
/// definition.
pub fn find_declared_script(project_path: &Path, kind: ScriptKind, script_id: &str) -> Result<ScriptEntry, ScriptError> {
    crate::logger::log("DEBUG", "hydration", &format!("find_declared_script: {} [{}] '{}'", project_path.display(), kind.table(), script_id));
    let rula = crate::config::read_project_rula(project_path)?
        .ok_or_else(|| ScriptError::NoConfig { kind: kind.table(), script_id: script_id.to_string() })?;
    let declared = rula
        .get("scripts")
        .and_then(|s| s.get(kind.table()))
        .and_then(|t| t.get(script_id))
        .ok_or_else(|| ScriptError::NotDeclared { kind: kind.table(), script_id: script_id.to_string() })?;
    let script = parse_script_entry(script_id, declared, kind == ScriptKind::Persistent);
    if script.command.trim().is_empty() {
        return Err(ScriptError::MissingCommand { script_id: script_id.to_string() });
    }
    if declared.get("profiler").is_some_and(|p| !p.is_str()) {
        return Err(ScriptError::InvalidProfiler { script_id: script_id.to_string() });
    }
    Ok(script)
}

/// Expands the launch option `values` of `script` into CLI arguments, in declaration order.
///
/// A declared option missing from `values` takes its `rula.toml` default. A `boolean` option
/// adds its flag when true; a `select` option adds its flag and the chosen value, which must be
/// one of `choices`. Keys that the script does not declare are rejected.
pub fn expand_script_options(
    script: &ScriptEntry,
    values: &std::collections::HashMap<String, serde_json::Value>,
) -> Result<Vec<String>, ScriptError> {
    let invalid = |key: &str, details: String| ScriptError::InvalidOption {
        script_id: script.id.clone(),
        key: key.to_string(),
        details,
    };
    if let Some(unknown) = values.keys().find(|k| !script.options.iter().any(|o| &o.key == *k)) {
        return Err(ScriptError::UnknownOption { script_id: script.id.clone(), key: unknown.clone() });
    }

    let mut args = Vec::with_capacity(script.options.len() * 2);
    for opt in &script.options {
        if opt.flag.is_empty() {
            return Err(invalid(&opt.key, "declares no flag".into()));
        }
        let value = values.get(&opt.key).unwrap_or(&opt.default);
        match opt.r#type.as_str() {
            "boolean" => {
                let enabled = value.as_bool().ok_or_else(|| invalid(&opt.key, format!("expected a boolean, got {}", value)))?;
                if enabled {
                    args.push(opt.flag.clone());
                }
            }
            "select" => {
                let choice = value.as_str().ok_or_else(|| invalid(&opt.key, format!("expected a string, got {}", value)))?;
                if !opt.choices.iter().any(|c| c == choice) {
                    return Err(invalid(&opt.key, format!("'{}' is not one of {:?}", choice, opt.choices)));
                }
                args.push(opt.flag.clone());
                args.push(choice.to_string());
            }
            other => return Err(invalid(&opt.key, format!("unknown option type '{}'", other))),
        }
    }
    Ok(args)
}

/// Checks if the project has the continuous profiler subsystem enabled in `rula.toml`.
/// Defaults to true if unspecified.
pub fn is_project_profiler_enabled(project_path: &Path) -> bool {
    let rula = match crate::config::read_project_rula(project_path) {
        Ok(Some(r)) => r,
        _ => return true,
    };
    rula.get("plugins")
        .and_then(|p| p.get("profiler"))
        .and_then(|r| r.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

/// Hydrates a registered `ProjectEntry` with Git status, `rula.toml` scripts, skills, and plugin flags.
pub fn hydrate_project(entry: &ProjectEntry) -> HydratedProject {
    let p = Path::new(&entry.path);
    
    // 1. Validate Git
    let git_path = p.join(".git");
    let has_git = git_path.exists() && git_path.is_dir();
    
    // A repository whose status cannot be read reports `git_error` instead of a guessed status.
    let (git_status, git_error) = match has_git.then(|| git::get_git_status(p)) {
        Some(Ok(status)) => (Some(status), None),
        Some(Err(e)) => {
            crate::logger::log("ERROR", "hydration", &format!("Cannot read git status of {}: {}", entry.path, e));
            (None, Some(e.to_string()))
        }
        None => (None, None),
    };
    
    // 2. Validate Config (rula.toml)
    let config_path = p.join("rula.toml");
    let has_config = config_path.exists();

    let mut scripts = Vec::new();

    if let Some(rula) = crate::config::load_project_rula(p) {
        if let Some(scripts_table) = rula.get("scripts").and_then(|v| v.as_table()) {
            if let Some(persistent_table) = scripts_table.get("persistent").and_then(|v| v.as_table()) {
                for (id, s) in persistent_table {
                    scripts.push(parse_script_entry(id, s, true));
                }
            }
            if let Some(tasks_table) = scripts_table.get("task").and_then(|v| v.as_table()) {
                for (id, s) in tasks_table {
                    scripts.push(parse_script_entry(id, s, false));
                }
            }
        }
    }
    
    // 4. Validate Skill State
    let skill_status = skill::get_project_skill_status(p);

    HydratedProject {
        id: entry.id,
        path: entry.path.clone(),
        name: entry.name.clone(),
        description: entry.description.clone(),
        has_git,
        git_status,
        git_error,
        has_config,
        scripts,
        skill_status,
        init_state: match crate::db::db_get_init_state(&entry.path) {
            Ok(st) => st,
            Err(e) => {
                crate::logger::log("WARN", "hydration", &format!("Failed to load init state for '{}': {}", entry.path, e));
                None
            }
        },
        has_rea_enabled: crate::kopeng::rea::is_project_rea_enabled(p),
        has_profiler_enabled: is_project_profiler_enabled(p),
        has_electronics_enabled: crate::kopeng::electronics::is_project_electronics_enabled(p),
        has_desigma_enabled: crate::kopeng::desigma::is_project_desigma_enabled(p),
        has_solidcraft_enabled: crate::kopeng::solidcraft::is_project_solidcraft_enabled(p),
        has_pcbcraft_enabled: crate::kopeng::pcbcraft::is_project_pcbcraft_enabled(p),
        has_project_management_enabled: crate::kopeng::project_mgmt::is_project_management_enabled(p),
        has_knowledge_graph_enabled: crate::config::resolve_effective_bool(Some(entry.path.as_str()), None, "semantic", "graph_tab_enabled"),
        preview_url: crate::config::load_project_rula(p)
            .and_then(|r| {
                r.get("preview")
                    .and_then(|pv| pv.get("url"))
                    .and_then(|u| u.as_str())
                    .map(|s| s.trim().to_string())
            })
            .filter(|s| !s.is_empty()),
        is_favorite: entry.is_favorite,
        is_archived: entry.is_archived,
        hidden_tabs: entry.hidden_tabs.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn script() -> ScriptEntry {
        let declared: toml::Value = toml::from_str(r#"
            command = "npm"
            args = ["run", "dev"]
            [options.host]
            flag = "--host"
            default = true
            [options.mode]
            flag = "--mode"
            type = "select"
            choices = ["development", "production"]
            default = "development"
        "#).unwrap();
        parse_script_entry("web", &declared, true)
    }

    #[test]
    fn missing_options_take_declared_defaults() {
        let args = expand_script_options(&script(), &HashMap::new()).unwrap();
        assert_eq!(args, ["--host", "--mode", "development"]);
    }

    #[test]
    fn given_options_override_defaults() {
        let values = HashMap::from([
            ("host".to_string(), serde_json::json!(false)),
            ("mode".to_string(), serde_json::json!("production")),
        ]);
        assert_eq!(expand_script_options(&script(), &values).unwrap(), ["--mode", "production"]);
    }

    #[test]
    fn undeclared_or_invalid_options_are_rejected() {
        let unknown = HashMap::from([("port".to_string(), serde_json::json!(true))]);
        assert!(matches!(expand_script_options(&script(), &unknown), Err(ScriptError::UnknownOption { .. })));
        let bad_choice = HashMap::from([("mode".to_string(), serde_json::json!("staging"))]);
        assert!(matches!(expand_script_options(&script(), &bad_choice), Err(ScriptError::InvalidOption { .. })));
        let bad_type = HashMap::from([("host".to_string(), serde_json::json!("yes"))]);
        assert!(matches!(expand_script_options(&script(), &bad_type), Err(ScriptError::InvalidOption { .. })));
    }
}
