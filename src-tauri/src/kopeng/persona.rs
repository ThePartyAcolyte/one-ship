use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::ConfigError;
use crate::kopeng::dna::generate_random_dna;
use crate::registry::RegistryError;
use crate::supervisor::SupervisorError;

/// Platform agent managed by ONE-SHIP when the project has `.git/`; never declared in `rula.toml`.
const GIT_AGENT_ID: &str = "git";

/// Agent IDs owned by the platform; a persona cannot take them.
pub(crate) const RESERVED_AGENT_IDS: [&str; 5] = [
    crate::supervisor::SYSTEM_AGENT_ID,
    GIT_AGENT_ID,
    crate::kopeng::initializer::INITIALIZER_AGENT_ID,
    crate::kopeng::flag_resolver::REMOVAL_AGENT_ID,
    crate::kopeng::flag_resolver::CONFIG_AGENT_ID,
];

/// Errors of the persona commands, which edit `[agents.<id>]` in a project's `rula.toml`.
#[derive(Debug, thiserror::Error)]
pub enum PersonaError {
    #[error("Invalid persona {field}: {reason}")]
    InvalidField { field: &'static str, reason: String },
    #[error("'{0}' is a platform agent and cannot be declared as a persona")]
    ReservedId(String),
    #[error("Persona '{0}' already exists")]
    AlreadyExists(String),
    #[error("Persona '{0}' is not declared in rula.toml")]
    NotDeclared(String),
    #[error("{0} has no rula.toml; initialize the project first")]
    NoConfig(String),
    #[error("[agents] in rula.toml is not a table")]
    MalformedAgents,
    #[error("Background task failed: {0}")]
    Task(String),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Supervisor(#[from] SupervisorError),
}

impl crate::api::ApiError for PersonaError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::InvalidField { .. } | Self::ReservedId(_) | Self::MalformedAgents => StatusCode::BAD_REQUEST,
            Self::AlreadyExists(_) => StatusCode::CONFLICT,
            Self::NotDeclared(_) | Self::NoConfig(_) => StatusCode::NOT_FOUND,
            Self::Task(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Config(e) => e.status(),
            Self::Registry(e) => e.status(),
            Self::Supervisor(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::InvalidField { .. } => "PersonaError::InvalidField",
            Self::ReservedId(_) => "PersonaError::ReservedId",
            Self::AlreadyExists(_) => "PersonaError::AlreadyExists",
            Self::NotDeclared(_) => "PersonaError::NotDeclared",
            Self::NoConfig(_) => "PersonaError::NoConfig",
            Self::MalformedAgents => "PersonaError::MalformedAgents",
            Self::Task(_) => "PersonaError::Task",
            Self::Config(e) => e.code(),
            Self::Registry(e) => e.code(),
            Self::Supervisor(e) => e.code(),
        }
    }
}

/// Arguments of the daemon commands `create_agent_persona` and `update_agent_persona`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SaveAgentPersonaArgs {
    pub project_id: usize,
    pub agent: AgentPersonaInput,
}

/// Arguments of the daemon command `delete_agent_persona`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct DeleteAgentPersonaArgs {
    pub project_id: usize,
    pub agent_id: String,
}

/// Persona fields edited by the persona modal. Keys of `[agents.<id>]` outside this set
/// (e.g. `mcp_dialogue_mode`) are kept on update. `runner` and `model` are a pair of the
/// runner's manifest; `effort`, when set, is one the manifest declares for the model.
#[derive(Serialize, Deserialize, Debug, Clone, ts_rs::TS)]
#[ts(export)]
pub struct AgentPersonaInput {
    pub id: String,
    pub role: String,
    pub runner: String,
    pub model: String,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub accept_edits: Option<bool>,
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
    pub dna: Option<String>,
}

#[derive(Clone, Copy, Debug)]
enum SaveMode {
    Create,
    Update,
}

/// IDs are lowercase ASCII letters, digits, `-` and `_` (the modal normalizes user input).
fn validate_agent_id(agent_id: &str) -> Result<(), PersonaError> {
    let valid = !agent_id.is_empty()
        && agent_id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if !valid {
        return Err(PersonaError::InvalidField {
            field: "id",
            reason: format!("'{}' must be non-empty lowercase letters, digits, '-' or '_'", agent_id),
        });
    }
    if RESERVED_AGENT_IDS.contains(&agent_id) {
        return Err(PersonaError::ReservedId(agent_id.to_string()));
    }
    Ok(())
}

fn trimmed_list(items: Vec<String>, keep: impl Fn(&str) -> bool) -> toml::Value {
    toml::Value::Array(
        items
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && keep(s))
            .map(toml::Value::String)
            .collect(),
    )
}

/// Sets `key` to the trimmed value, or removes it when the value is absent or blank.
fn set_optional_string(table: &mut toml::value::Table, key: &str, value: Option<String>) {
    match value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()) {
        Some(v) => table.insert(key.to_string(), toml::Value::String(v)),
        None => table.remove(key),
    };
}

/// Writes the modal-managed fields of `agent` into `table`, leaving every other key untouched.
/// A blank DNA keeps the existing one, or generates one for a persona without it.
fn apply_persona_fields(table: &mut toml::value::Table, agent_id: &str, agent: AgentPersonaInput) {
    table.insert("role".to_string(), toml::Value::String(agent.role.trim().to_string()));
    table.insert("runner".to_string(), toml::Value::String(agent.runner.trim().to_string()));
    table.insert("model".to_string(), toml::Value::String(agent.model.trim().to_string()));
    set_optional_string(table, "effort", agent.effort);
    match agent.accept_edits {
        Some(accept) => table.insert("accept_edits".to_string(), toml::Value::Boolean(accept)),
        None => table.remove("accept_edits"),
    };
    table.insert("auto_approve".to_string(), toml::Value::Boolean(agent.auto_approve));
    table.insert("args".to_string(), trimmed_list(agent.args, |_| true));
    table.insert("context_docs".to_string(), trimmed_list(agent.context_docs, |_| true));
    // Platform base skills are always active, so they are never listed per persona.
    table.insert(
        "skills".to_string(),
        trimmed_list(agent.skills, |s| !crate::kopeng::skill::PLATFORM_BASE_SKILLS.contains(&s)),
    );
    set_optional_string(table, "system_prompt", agent.system_prompt);

    match agent.dna.map(|d| d.trim().to_string()).filter(|d| !d.is_empty()) {
        Some(dna) => {
            table.insert("dna".to_string(), toml::Value::String(dna));
        }
        None if !table.contains_key("dna") => {
            table.insert("dna".to_string(), toml::Value::String(generate_random_dna(agent_id)));
        }
        None => {}
    }
}

fn agents_table(rula: &mut toml::Value) -> Result<&mut toml::value::Table, PersonaError> {
    rula.as_table_mut()
        .ok_or(PersonaError::MalformedAgents)?
        .entry("agents".to_string())
        .or_insert_with(|| toml::Value::Table(Default::default()))
        .as_table_mut()
        .ok_or(PersonaError::MalformedAgents)
}

/// Creates or updates `[agents.<id>]` in `rula`; returns the persona ID.
fn upsert_persona(rula: &mut toml::Value, agent: AgentPersonaInput, mode: SaveMode) -> Result<String, PersonaError> {
    let agent_id = agent.id.trim().to_string();
    validate_agent_id(&agent_id)?;
    if agent.role.trim().is_empty() {
        return Err(PersonaError::InvalidField { field: "role", reason: "must not be empty".to_string() });
    }
    // The runner manifest is the only source of runner/model/effort pairs: an unknown runner, a
    // missing model, a model of another runner or an unsupported effort is an error.
    crate::kopeng::resolution::finalize_agent_config(
        &format!("persona '{}'", agent_id),
        agent.runner.trim(),
        Some(agent.model.trim()),
        agent.effort.as_deref().map(str::trim),
    )?;

    let agents = agents_table(rula)?;
    let table = match mode {
        SaveMode::Create => {
            if agents.contains_key(&agent_id) {
                return Err(PersonaError::AlreadyExists(agent_id));
            }
            agents
                .entry(agent_id.clone())
                .or_insert_with(|| toml::Value::Table(Default::default()))
                .as_table_mut()
                .ok_or(PersonaError::MalformedAgents)?
        }
        SaveMode::Update => agents
            .get_mut(&agent_id)
            .ok_or_else(|| PersonaError::NotDeclared(agent_id.clone()))?
            .as_table_mut()
            .ok_or(PersonaError::MalformedAgents)?,
    };
    apply_persona_fields(table, &agent_id, agent);
    Ok(agent_id)
}

fn read_rula(project_path: &Path) -> Result<toml::Value, PersonaError> {
    crate::config::read_project_rula(project_path)?
        .ok_or_else(|| PersonaError::NoConfig(project_path.display().to_string()))
}

fn require_declared(rula: &toml::Value, agent_id: &str) -> Result<(), PersonaError> {
    if rula.get("agents").and_then(|a| a.get(agent_id)).is_none() {
        return Err(PersonaError::NotDeclared(agent_id.to_string()));
    }
    Ok(())
}

async fn save_persona(project_id: usize, agent: AgentPersonaInput, mode: SaveMode) -> Result<(), PersonaError> {
    tokio::task::spawn_blocking(move || {
        let project = crate::registry::find_project_by_id(project_id)?;
        let project_path = Path::new(&project.path);
        let mut rula = read_rula(project_path)?;
        let agent_id = upsert_persona(&mut rula, agent, mode)?;
        crate::config::write_project_rula(project_path, &rula)?;
        crate::logger::log("INFO", "persona", &format!("Saved agent persona '{}' ({:?}) in {}", agent_id, mode, project.path));
        Ok(())
    })
    .await
    .map_err(|e| PersonaError::Task(e.to_string()))?
}

/// Declares a new persona `[agents.<id>]` in the project's `rula.toml`; an existing ID is an error.
pub async fn create_agent_persona(project_id: usize, agent: AgentPersonaInput) -> Result<(), PersonaError> {
    crate::logger::log("DEBUG", "persona", &format!("create_agent_persona: project_id={}, agent_id={}", project_id, agent.id));
    save_persona(project_id, agent, SaveMode::Create).await
}

/// Updates the modal-managed fields of a declared persona, keeping its other keys.
pub async fn update_agent_persona(project_id: usize, agent: AgentPersonaInput) -> Result<(), PersonaError> {
    crate::logger::log("DEBUG", "persona", &format!("update_agent_persona: project_id={}, agent_id={}", project_id, agent.id));
    save_persona(project_id, agent, SaveMode::Update).await
}

/// Stops the persona's agent, then removes `[agents.<id>]`; when the stop fails the persona is kept.
pub async fn delete_agent_persona(project_id: usize, agent_id: String) -> Result<(), PersonaError> {
    crate::logger::log("DEBUG", "persona", &format!("delete_agent_persona: project_id={}, agent_id={}", project_id, agent_id));
    let check_id = agent_id.clone();
    let project = tokio::task::spawn_blocking(move || {
        let project = crate::registry::find_project_by_id(project_id)?;
        require_declared(&read_rula(Path::new(&project.path))?, &check_id)?;
        Ok::<_, PersonaError>(project)
    })
    .await
    .map_err(|e| PersonaError::Task(e.to_string()))??;

    let key = crate::supervisor::ProcessSupervisor::format_agent_key(&project.path, &agent_id);
    crate::supervisor::get_supervisor().stop_process(&key).await?;

    tokio::task::spawn_blocking(move || {
        let project_path = Path::new(&project.path);
        let mut rula = read_rula(project_path)?;
        agents_table(&mut rula)?
            .remove(&agent_id)
            .ok_or_else(|| PersonaError::NotDeclared(agent_id.clone()))?;
        crate::config::write_project_rula(project_path, &rula)?;
        crate::logger::log("INFO", "persona", &format!("Deleted agent persona '{}' from {}", agent_id, project.path));
        Ok(())
    })
    .await
    .map_err(|e| PersonaError::Task(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(id: &str, runner: &str) -> AgentPersonaInput {
        AgentPersonaInput {
            id: id.to_string(),
            role: "Backend".to_string(),
            runner: runner.to_string(),
            model: "claude-opus-5-5".to_string(),
            effort: None,
            accept_edits: Some(true),
            auto_approve: false,
            args: vec![],
            context_docs: vec![],
            skills: vec![],
            system_prompt: None,
            dna: None,
        }
    }

    fn rula_with_backend() -> toml::Value {
        toml::from_str("[agents.backend]\nrole = \"Old\"\nrunner = \"claude\"\nmcp_dialogue_mode = true\ndna = \"B01:E0:H01:BD01:A01\"\nmodel = \"claude-sonnet-5-5\"\neffort = \"max\"\n").unwrap()
    }

    #[test]
    fn create_rejects_an_existing_id_and_update_an_undeclared_one() {
        let mut rula = rula_with_backend();
        assert!(matches!(upsert_persona(&mut rula, input("backend", "claude"), SaveMode::Create), Err(PersonaError::AlreadyExists(_))));
        assert!(matches!(upsert_persona(&mut rula, input("frontend", "claude"), SaveMode::Update), Err(PersonaError::NotDeclared(_))));
        assert!(upsert_persona(&mut rula, input("frontend", "claude"), SaveMode::Create).is_ok());
        assert!(rula["agents"]["frontend"].get("dna").is_some());
    }

    #[test]
    fn update_keeps_unmanaged_keys_and_dna_and_clears_blank_fields() {
        let mut rula = rula_with_backend();
        upsert_persona(&mut rula, input("backend", "claude"), SaveMode::Update).unwrap();
        let backend = &rula["agents"]["backend"];
        assert_eq!(backend["role"].as_str(), Some("Backend"));
        assert_eq!(backend["mcp_dialogue_mode"].as_bool(), Some(true));
        assert_eq!(backend["dna"].as_str(), Some("B01:E0:H01:BD01:A01"));
        assert_eq!(backend["model"].as_str(), Some("claude-opus-5-5"));
        assert!(backend.get("effort").is_none());
    }

    #[test]
    fn a_model_of_another_runner_a_blank_model_or_an_unsupported_effort_is_rejected() {
        let mut rula = rula_with_backend();
        let cross = AgentPersonaInput { model: "gemini-3.8-flash".into(), ..input("qa", "claude") };
        assert!(matches!(upsert_persona(&mut rula, cross, SaveMode::Create), Err(PersonaError::Config(crate::config::ConfigError::UnknownModel { .. }))));
        let blank = AgentPersonaInput { model: " ".into(), ..input("qa", "claude") };
        assert!(matches!(upsert_persona(&mut rula, blank, SaveMode::Create), Err(PersonaError::Config(crate::config::ConfigError::MissingModel { .. }))));
        let effort = AgentPersonaInput { effort: Some("ultra".into()), ..input("qa", "claude") };
        assert!(matches!(upsert_persona(&mut rula, effort, SaveMode::Create), Err(PersonaError::Config(crate::config::ConfigError::InvalidEffort { .. }))));
        let ok = AgentPersonaInput { effort: Some("xhigh".into()), ..input("qa", "claude") };
        assert!(upsert_persona(&mut rula, ok, SaveMode::Create).is_ok());
    }

    #[test]
    fn reserved_and_malformed_ids_and_unknown_runners_are_rejected() {
        let mut rula = rula_with_backend();
        for id in ["git", "system", "initializer", "removal-resolver", "config-resolver"] {
            assert!(matches!(upsert_persona(&mut rula, input(id, "claude"), SaveMode::Create), Err(PersonaError::ReservedId(_))), "{}", id);
        }
        for id in ["", "Back End", "a/b"] {
            assert!(matches!(upsert_persona(&mut rula, input(id, "claude"), SaveMode::Create), Err(PersonaError::InvalidField { .. })), "{}", id);
        }
        assert!(matches!(upsert_persona(&mut rula, input("qa", ""), SaveMode::Create), Err(PersonaError::Config(_))));
        assert!(matches!(upsert_persona(&mut rula, input("qa", "nope"), SaveMode::Create), Err(PersonaError::Config(_))));
    }
}
