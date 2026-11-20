use std::path::Path;

use crate::db::{DatabaseError, ProjectInitFlagRecord};
use crate::kopeng::skill::SkillError;
use crate::kopeng::snapshots::SnapshotError;
use crate::registry::{ProjectEntry, RegistryError};
use crate::supervisor::SupervisorError;

/// Failure to launch a flag resolution agent.
#[derive(Debug, thiserror::Error)]
pub enum FlagResolverError {
    #[error("Flag {flag_id} belongs to project id {flag_project_id}, not to project id {project_id}")]
    FlagNotInProject { flag_id: i64, flag_project_id: i64, project_id: usize },
    #[error("Flag {flag_id} is '{status}'; only pending flags are resolved")]
    FlagNotPending { flag_id: i64, status: String },
    #[error("Unknown initialization flag type '{0}'")]
    UnknownFlagType(String),
    #[error("A docs flag requires {field} (one of: {expected})")]
    MissingDocsOption { field: &'static str, expected: String },
    #[error("Invalid {field} '{value}' for a docs flag (one of: {expected})")]
    InvalidDocsOption { field: &'static str, value: String, expected: String },
    #[error("Flag type '{0}' takes no archetype or cleanup strategy")]
    OptionNotApplicable(String),
    #[error("Cannot serialize removal metadata: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Background task failed: {0}")]
    Task(String),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    Skill(#[from] SkillError),
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),
    #[error(transparent)]
    Supervisor(#[from] SupervisorError),
    #[error(transparent)]
    Config(#[from] crate::config::ConfigError),
}

impl crate::api::ApiError for FlagResolverError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::FlagNotInProject { .. }
            | Self::UnknownFlagType(_)
            | Self::MissingDocsOption { .. }
            | Self::InvalidDocsOption { .. }
            | Self::OptionNotApplicable(_) => StatusCode::BAD_REQUEST,
            Self::FlagNotPending { .. } => StatusCode::CONFLICT,
            Self::Serialization(_) | Self::Task(_) | Self::Skill(_) | Self::Snapshot(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Database(e) => e.status(),
            Self::Registry(e) => e.status(),
            Self::Supervisor(e) => e.status(),
            Self::Config(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::FlagNotInProject { .. } => "FlagResolverError::FlagNotInProject",
            Self::FlagNotPending { .. } => "FlagResolverError::FlagNotPending",
            Self::UnknownFlagType(_) => "FlagResolverError::UnknownFlagType",
            Self::MissingDocsOption { .. } => "FlagResolverError::MissingDocsOption",
            Self::InvalidDocsOption { .. } => "FlagResolverError::InvalidDocsOption",
            Self::OptionNotApplicable(_) => "FlagResolverError::OptionNotApplicable",
            Self::Serialization(_) => "FlagResolverError::Serialization",
            Self::Task(_) => "FlagResolverError::Task",
            Self::Database(e) => e.code(),
            Self::Skill(_) => "FlagResolverError::Skill",
            Self::Snapshot(_) => "FlagResolverError::Snapshot",
            Self::Registry(e) => e.code(),
            Self::Supervisor(e) => e.code(),
            Self::Config(e) => e.code(),
        }
    }
}

/// Arguments of the daemon command `launch_project_removal_agent`.
#[derive(serde::Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct LaunchProjectRemovalAgentArgs {
    pub project_id: usize,
    pub purge_traces: bool,
    pub failure_report: String,
}

/// Arguments of the daemon command `launch_flag_resolution_agent`. `runner`, `model` and
/// `effort` are an explicit pair (runner and model together, or none).
#[derive(serde::Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct LaunchFlagResolutionAgentArgs {
    pub project_id: usize,
    #[ts(type = "number")]
    pub flag_id: i64,
    pub chosen_option: Option<String>,
    pub cleanup_strategy: Option<String>,
    pub runner: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
}

/// Docs archetypes accepted by a `docs` flag and the specialized skill each one injects.
const DOCS_ARCHETYPE_SKILLS: [(&str, &str); 4] = [
    ("sdd", "oneship-docstyle-sdd"),
    ("lean", "oneship-docstyle-lean"),
    ("contract", "oneship-docstyle-contract"),
    ("custom", "oneship-docstyles-crafter"),
];
/// Cleanup strategies accepted by a `docs` flag.
const DOCS_CLEANUP_STRATEGIES: [&str; 3] = ["purge", "annotate", "preserve"];
/// Skill injected with every docs archetype.
const DOCS_BASE_SKILL: &str = "oneship-docstyles";

/// Flag type that carries an aborted project removal to the removal agent.
pub const REMOVAL_FLAG_TYPE: &str = "removal";
/// Agent that fixes the failures of an aborted project removal and closes it.
pub const REMOVAL_AGENT_ID: &str = "removal-resolver";
/// Flag type raised by the project watcher while a project's `rula.toml` is invalid
/// (`config::sync_project_config_flag`); it closes when the file validates again.
pub const INVALID_CONFIG_FLAG_TYPE: &str = "invalid_config";
/// Agent that repairs an invalid `rula.toml`.
pub const CONFIG_AGENT_ID: &str = "config-resolver";

/// `metadata_json` of a pending `removal` flag: the removal to retry and why it aborted.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct RemovalFlagMetadata {
    pub purge_traces: bool,
    pub failure_report: String,
}

/// The docs archetype and cleanup strategy chosen for a `docs` flag.
struct DocsChoice {
    archetype: String,
    cleanup_strategy: String,
}

/// Agent, prompt template and specialized skill that resolve one flag.
struct FlagAgentPlan {
    agent_id: &'static str,
    template_name: &'static str,
    specialized_skill: &'static str,
    docs: Option<DocsChoice>,
}

/// Index of `value` in `allowed`; an absent or unlisted value is an error naming `field`.
fn require_docs_option(field: &'static str, value: Option<String>, allowed: &[&str]) -> Result<(usize, String), FlagResolverError> {
    let expected = allowed.join(", ");
    let value = value.ok_or_else(|| FlagResolverError::MissingDocsOption { field, expected: expected.clone() })?;
    match allowed.iter().position(|candidate| *candidate == value) {
        Some(idx) => Ok((idx, value)),
        None => Err(FlagResolverError::InvalidDocsOption { field, value, expected }),
    }
}

/// Maps a flag type to its resolution agent. Only `docs` takes an archetype and a cleanup
/// strategy, and requires both; any other type rejects them.
fn plan_for_flag(
    flag_type: &str,
    chosen_option: Option<String>,
    cleanup_strategy: Option<String>,
) -> Result<FlagAgentPlan, FlagResolverError> {
    if flag_type == "docs" {
        let archetypes = DOCS_ARCHETYPE_SKILLS.map(|(archetype, _)| archetype);
        let (archetype_idx, archetype) = require_docs_option("archetype", chosen_option, &archetypes)?;
        let (_, cleanup_strategy) = require_docs_option("cleanup strategy", cleanup_strategy, &DOCS_CLEANUP_STRATEGIES)?;
        let specialized_skill = DOCS_ARCHETYPE_SKILLS[archetype_idx].1;
        return Ok(FlagAgentPlan {
            agent_id: "docs-resolver",
            template_name: "flag_resolve_docs",
            specialized_skill,
            docs: Some(DocsChoice { archetype, cleanup_strategy }),
        });
    }
    if chosen_option.is_some() || cleanup_strategy.is_some() {
        return Err(FlagResolverError::OptionNotApplicable(flag_type.to_string()));
    }
    let (agent_id, template_name, specialized_skill) = match flag_type {
        "profiler" => ("profiler-crafter", "flag_resolve_profiler", "oneship-profiler-crafter"),
        "todos" => ("todos-resolver", "flag_resolve_todos", "oneship-devlog"),
        "tech_debt" => ("techdebt-resolver", "flag_resolve_techdebt", "oneship-techdebt"),
        REMOVAL_FLAG_TYPE => (REMOVAL_AGENT_ID, "flag_resolve_removal", "oneship-core"),
        INVALID_CONFIG_FLAG_TYPE => (CONFIG_AGENT_ID, "flag_resolve_config", "oneship-core"),
        unknown => return Err(FlagResolverError::UnknownFlagType(unknown.to_string())),
    };
    Ok(FlagAgentPlan { agent_id, template_name, specialized_skill, docs: None })
}

/// Hands an aborted `remove_project` to the removal agent.
///
/// Records the failure as a pending `removal` flag (replacing an earlier pending one of the
/// project) and launches `removal-resolver` through the flag resolution path. The agent closes
/// the removal with the `complete_project_removal` MCP tool. Returns the agent id.
pub async fn launch_project_removal_agent(
    project_id: usize,
    purge_traces: bool,
    failure_report: String,
) -> Result<String, FlagResolverError> {
    crate::logger::log("DEBUG", "flag_resolver", &format!("launch_project_removal_agent: project_id={}, purge_traces={}", project_id, purge_traces));
    let (project_name, flag_id) = tokio::task::spawn_blocking(move || -> Result<_, FlagResolverError> {
        let project = crate::registry::find_project_by_id(project_id)?;
        for flag in crate::db::db_get_init_flags(project_id as i64, Some("pending"))? {
            if flag.flag_type == REMOVAL_FLAG_TYPE {
                crate::db::db_resolve_init_flag(flag.id, "dismissed", None)?;
            }
        }
        let metadata_json = serde_json::to_string(&RemovalFlagMetadata { purge_traces, failure_report: failure_report.clone() })?;
        let flag_id = crate::db::db_insert_init_flag(&crate::db::NewInitFlag {
            project_id: project_id as i64,
            flag_type: REMOVAL_FLAG_TYPE.to_string(),
            title: "Project removal failed".to_string(),
            description: Some(failure_report),
            recommended_option: None,
            options_json: None,
            metadata_json: Some(metadata_json),
            raised_by_agent_uid: None,
        })?;
        Ok((project.name, flag_id))
    })
    .await
    .map_err(|e| FlagResolverError::Task(e.to_string()))??;

    crate::logger::log("INFO", "flag_resolver", &format!("Removal of project '{}' handed to {} (flag_id: {})", project_name, REMOVAL_AGENT_ID, flag_id));
    launch_flag_resolution_agent(project_id, flag_id, None, None, None, None, None).await
}

/// Launches the resolution agent of a pending initialization flag and returns its agent id.
///
/// The flag is read from SQLite and must belong to `project_id` and be pending; its type selects
/// the agent, prompt template and skill, and its `metadata_json` reaches the prompt. `docs` flags
/// require `chosen_option` (archetype) and `cleanup_strategy`; other flag types reject them.
/// Injects the platform and specialized skills, takes a pre-resolution snapshot when the project
/// is under git, records the opening user turn and spawns the agent (replacing an earlier
/// instance); the prompt is piped to its stdin afterwards.
#[allow(clippy::too_many_arguments)]
pub async fn launch_flag_resolution_agent(
    project_id: usize,
    flag_id: i64,
    chosen_option: Option<String>,
    cleanup_strategy: Option<String>,
    runner: Option<String>,
    model: Option<String>,
    effort: Option<String>,
) -> Result<String, FlagResolverError> {
    crate::logger::log(
        "DEBUG",
        "flag_resolver",
        &format!("launch_flag_resolution_agent: project_id={}, flag_id={}, chosen_option={:?}, cleanup_strategy={:?}", project_id, flag_id, chosen_option, cleanup_strategy),
    );

    let (project, flag, plan, conv_id) = tokio::task::spawn_blocking(
        move || -> Result<(ProjectEntry, ProjectInitFlagRecord, FlagAgentPlan, String), FlagResolverError> {
            let project = crate::registry::find_project_by_id(project_id)?;
            let flag = crate::db::db_get_init_flag(flag_id)?;
            if flag.project_id != project.id as i64 {
                return Err(FlagResolverError::FlagNotInProject { flag_id, flag_project_id: flag.project_id, project_id });
            }
            if flag.status != "pending" {
                return Err(FlagResolverError::FlagNotPending { flag_id, status: flag.status });
            }
            let plan = plan_for_flag(&flag.flag_type, chosen_option, cleanup_strategy)?;
            crate::logger::log(
                "DEBUG",
                "flag_resolver",
                &format!("Flag {} ({}) resolves with agent='{}', template='{}', skill='{}'", flag_id, flag.flag_type, plan.agent_id, plan.template_name, plan.specialized_skill),
            );

            // inject_platform_skill also refreshes AGENTS.md and the runner MCP configs.
            crate::kopeng::skill::inject_platform_skill(project.path.clone())?;
            if !crate::kopeng::skill::PLATFORM_BASE_SKILLS.contains(&plan.specialized_skill) {
                crate::kopeng::skill::inject_specialized_skill(project.path.clone(), plan.specialized_skill.to_string())?;
            }
            if plan.docs.is_some() && plan.specialized_skill != DOCS_BASE_SKILL {
                crate::kopeng::skill::inject_specialized_skill(project.path.clone(), DOCS_BASE_SKILL.to_string())?;
            }

            let conv = crate::db::db_get_or_create_active_conversation(
                project.id as i64,
                plan.agent_id,
                Some(&format!("Flag Resolution: {}", flag.flag_type)),
                None,
                None,
                None,
                None,
            )?;

            // Pre-resolution snapshot (projects under git only; a failed snapshot stops the resolution).
            let root = Path::new(&project.path);
            if root.join(".git").exists() {
                crate::logger::log("DEBUG", "flag_resolver", &format!("Capturing pre-resolution snapshot for flag {} ({})", flag.flag_type, project.name));
                crate::kopeng::snapshots::create_snapshot(
                    root,
                    project.id as i64,
                    Some(&conv.id),
                    plan.agent_id,
                    &format!("Pre-Resolution: {} ({})", flag.flag_type, project.name),
                    None,
                )?;
            }
            Ok((project, flag, plan, conv.id))
        },
    )
    .await
    .map_err(|e| FlagResolverError::Task(e.to_string()))??;

    let flag_id_str = flag_id.to_string();
    // Absent metadata reaches the prompt as JSON `null`.
    let metadata_json = flag.metadata_json.clone().unwrap_or_else(|| "null".to_string());
    // Agents that may edit `[agents.<id>]` (config repair) take runner/model pairs only from it.
    let runner_catalog = crate::kopeng::resolution::runner_catalog_markdown()?;
    let mut vars = vec![
        ("PROJECT_NAME", project.name.as_str()),
        ("PROJECT_PATH", project.path.as_str()),
        ("FLAG_ID", flag_id_str.as_str()),
        ("METADATA_JSON", metadata_json.as_str()),
        ("RUNNER_CATALOG", runner_catalog.as_str()),
    ];
    if let Some(docs) = &plan.docs {
        vars.push(("CHOSEN_OPTION", docs.archetype.as_str()));
        vars.push(("CLEANUP_STRATEGY", docs.cleanup_strategy.as_str()));
    }
    let base_prompt = crate::kopeng::skill::load_prompt_template(plan.template_name, &vars).await?;

    // A removal is closed by `complete_project_removal` (user-approved deletion), never by flag resolution.
    let closure_rule = if flag.flag_type == REMOVAL_FLAG_TYPE {
        "3. **REMOVAL CLOSURE INVARIANT:**\n\
            - Never call `resolve_initialization_flag` for this flag.\n\
            - When every reported failure is fixed, call `complete_project_removal`; it asks the user to approve the deletion and then deletes the project.".to_string()
    } else if flag.flag_type == INVALID_CONFIG_FLAG_TYPE {
        "3. **CONFIG CLOSURE INVARIANT:**\n\
            - Never call `resolve_initialization_flag` for this flag: ONE-SHIP closes it automatically as soon as `rula.toml` validates.\n\
            - After your fix, report what you changed with `post_agent_response`; the flag badge disappears on the next watcher tick.".to_string()
    } else {
        format!(
            "3. **MULTI-TURN CLOSURE INVARIANT:**\n\
            - DO NOT call `resolve_initialization_flag` in Turn 1.\n\
            - Only call `resolve_initialization_flag` with `flag_id: {}` and `status: \"resolved\"` after receiving the user's selected option and executing the changes in subsequent turns.",
            flag_id
        )
    };

    let full_prompt = format!(
        "{}\n\n\
         ## MANDATORY AGENT PROTOCOL & NATIVE MCP TOOL USAGE:\n\
         1. **NATIVE TOOL CALLING ONLY (NO PYTHON / CURL / RAW SCRIPTS):**\n\
            - Always invoke MCP tools (`post_agent_response`, `manage_todo_item`, `record_technical_debt`, `resolve_initialization_flag`, `complete_project_removal`) using your environment's native MCP tool calling facility (`call_mcp_tool` with ServerName `\"one-ship\"`, or direct tool wrappers).\n\
            - Writing Python scripts, bash curl commands, or shell scripts to make raw HTTP requests to `http://localhost:8765/mcp` is STRICTLY PROHIBITED.\n\
         2. **TERMINAL STDOUT SILENCE & SINGLE TERMINAL RESPONSE:**\n\
            - Do NOT output final conversational text, markdown summaries, or dialogue directly to the terminal stdout. Terminal output is internal log only.\n\
            - Deliver your response by calling `post_agent_response` EXACTLY ONCE at the conclusion of your turn when all scanning and staging are complete.\n\
            - Pass `dialogue`: Concise spoken conversational summary ready for Text-to-Speech.\n\
            - Pass `artifact`: Detailed technical changes, markdown tables, or execution plans.\n\
            - Pass `approval`: Object `{{ \"title\": \"...\", \"description\": \"...\", \"options\": [...] }}` containing the actionable strategy options so the UI renders interactive buttons.\n\
         {}",
        base_prompt, closure_rule
    );

    // A resolution agent may have a pair template in `[agents.<agent_id>]`; otherwise it runs on
    // the global default pair (`agent_runner` / `default_model` / `default_effort`).
    let explicit = crate::kopeng::resolution::ExplicitAgentConfig::from_parts("flag resolution launch", runner, model, effort)?;
    let effective_config = crate::kopeng::resolution::resolve_effective_agent_config(
        Some(&project.path),
        Some(plan.agent_id),
        explicit,
        &crate::kopeng::resolution::DEFAULT_AGENT_KEYS,
    )?;
    crate::logger::log(
        "DEBUG",
        "flag_resolver",
        &format!(
            "Resolved agent configuration for '{}': runner='{}', model='{}', effort='{:?}'",
            plan.agent_id, effective_config.runner, effective_config.model, effective_config.effort
        ),
    );

    // The opening user turn is recorded before the spawn; a failed insert stops the launch.
    let dialogue = match &plan.docs {
        Some(docs) => format!("Resolve flag {}: {} ({})", flag.flag_type, docs.archetype, docs.cleanup_strategy),
        None => format!("Resolve flag {}", flag.flag_type),
    };
    let user_turn = crate::db::ChatTurnRecord {
        id: None,
        conversation_id: conv_id,
        project_id: project.id as i64,
        agent_id: plan.agent_id.to_string(),
        agy_conversation_id: None,
        role: "user".to_string(),
        dialogue,
        artifact_type: None,
        artifact_lang: None,
        artifact_label: None,
        artifact_file_path: None,
        artifact_content: None,
        is_ephemeral: false,
        terminal_snapshot: None,
        timestamp: crate::db::now_epoch_secs(),
        topic: None,
        approval_payload: None,
    };
    tokio::task::spawn_blocking(move || crate::db::db_insert_chat_turn(&user_turn))
        .await
        .map_err(|e| FlagResolverError::Task(e.to_string()))??;

    // spawn_agent stops an earlier instance of this agent first.
    let sup = crate::supervisor::get_supervisor();
    let pid = sup
        .spawn_agent(
            &project.path,
            plan.agent_id,
            &effective_config.runner,
            &effective_config.model,
            effective_config.effort.as_deref(),
            true,
            true,
            &[],
            &[],
            None,
            None,
            true,
        )
        .await
        .inspect_err(|e| {
            crate::logger::log("ERROR", "flag_resolver", &format!("Flag {}: could not spawn agent '{}': {}", flag_id, plan.agent_id, e));
        })?;
    crate::logger::log(
        "INFO",
        "flag_resolver",
        &format!("Launched flag resolution agent '{}' for {} (flag_id: {}, PID: {}) in {}", plan.agent_id, flag.flag_type, flag_id, pid, project.path),
    );

    let key = crate::supervisor::ProcessSupervisor::format_agent_key(&project.path, plan.agent_id);
    let agent_id = plan.agent_id.to_string();
    let project_path = project.path;
    let agent_id_for_input = agent_id.clone();
    tauri::async_runtime::spawn(async move {
        let delay_ms = crate::config::resolve_effective_u64(None, None, "orchestrator", "agent_spawn_poll_step_ms");
        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
        if let Err(e) = crate::supervisor::get_supervisor().send_input(&key, &full_prompt).await {
            crate::logger::log("ERROR", "flag_resolver", &format!("Flag {}: could not deliver the resolution prompt to '{}': {}", flag_id, key, e));
            return;
        }
        crate::conversation::mark_user_message_sent(&project_path, &agent_id_for_input);
    });

    Ok(agent_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docs_flags_require_a_valid_archetype_and_cleanup_strategy() {
        let plan = plan_for_flag("docs", Some("lean".into()), Some("annotate".into())).unwrap();
        assert_eq!(plan.specialized_skill, "oneship-docstyle-lean");
        assert!(matches!(plan_for_flag("docs", None, Some("purge".into())), Err(FlagResolverError::MissingDocsOption { .. })));
        assert!(matches!(plan_for_flag("docs", Some("sdd".into()), None), Err(FlagResolverError::MissingDocsOption { .. })));
        assert!(matches!(plan_for_flag("docs", Some("wiki".into()), Some("purge".into())), Err(FlagResolverError::InvalidDocsOption { .. })));
        assert!(matches!(plan_for_flag("docs", Some("sdd".into()), Some("keep".into())), Err(FlagResolverError::InvalidDocsOption { .. })));
    }

    #[test]
    fn other_flags_reject_docs_options_and_unknown_types() {
        assert_eq!(plan_for_flag(REMOVAL_FLAG_TYPE, None, None).unwrap().agent_id, REMOVAL_AGENT_ID);
        assert!(matches!(plan_for_flag("todos", Some("todos".into()), None), Err(FlagResolverError::OptionNotApplicable(_))));
        assert!(matches!(plan_for_flag("bogus", None, None), Err(FlagResolverError::UnknownFlagType(_))));
    }
}
