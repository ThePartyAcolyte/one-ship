use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};

use crate::config::ConfigError;
use crate::db::{DatabaseError, ProjectInitState};
use crate::kopeng::git::GitError;
use crate::kopeng::rea::ReaError;
use crate::kopeng::skill::SkillError;
use crate::kopeng::snapshots::SnapshotError;
use crate::registry::{ProjectEntry, RegistryError};
use crate::supervisor::{ProcessStatus, SupervisorError};

/// Agent id of the project initializer.
pub const INITIALIZER_AGENT_ID: &str = "initializer";
/// Ephemeral skill injected for the initialization and removed when it ends.
const INITIALIZER_SKILL: &str = "oneship-initializer";
/// `ProjectInitState::current_phase` set by the MCP dialogue channel once the initializer has
/// presented its proposal; a supervised watcher stops at this phase.
pub const PROPOSAL_PRESENTED_PHASE: &str = "Proposal presented — review in Chat";

/// Failure to prepare, launch or close a project initialization.
#[derive(Debug, thiserror::Error)]
pub enum InitializerError {
    #[error("Invalid project name '{name}': {reason}")]
    InvalidProjectName { name: String, reason: &'static str },
    #[error("Parent location '{0}' is not an existing directory")]
    ParentNotDirectory(String),
    #[error("'{0}' already exists and is not an empty directory")]
    TargetNotEmpty(String),
    #[error("Initialization of '{project}' cannot be completed yet: {reason}")]
    Incomplete { project: String, reason: String },
    #[error("{context}: {source}")]
    Io { context: String, source: std::io::Error },
    #[error("Background task failed: {0}")]
    Task(String),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    Skill(#[from] SkillError),
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),
    #[error(transparent)]
    Supervisor(#[from] SupervisorError),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Rea(#[from] ReaError),
}

impl crate::api::ApiError for InitializerError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::InvalidProjectName { .. } | Self::ParentNotDirectory(_) => StatusCode::BAD_REQUEST,
            Self::TargetNotEmpty(_) | Self::Incomplete { .. } => StatusCode::CONFLICT,
            Self::Io { .. } | Self::Task(_) | Self::Skill(_) | Self::Snapshot(_) | Self::Git(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Database(e) => e.status(),
            Self::Registry(e) => e.status(),
            Self::Config(e) => e.status(),
            Self::Supervisor(e) => e.status(),
            Self::Rea(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::InvalidProjectName { .. } => "InitializerError::InvalidProjectName",
            Self::ParentNotDirectory(_) => "InitializerError::ParentNotDirectory",
            Self::TargetNotEmpty(_) => "InitializerError::TargetNotEmpty",
            Self::Incomplete { .. } => "InitializerError::Incomplete",
            Self::Io { .. } => "InitializerError::Io",
            Self::Task(_) => "InitializerError::Task",
            Self::Database(e) => e.code(),
            Self::Skill(_) => "InitializerError::Skill",
            Self::Snapshot(_) => "InitializerError::Snapshot",
            Self::Git(_) => "InitializerError::Git",
            Self::Registry(e) => e.code(),
            Self::Config(e) => e.code(),
            Self::Supervisor(e) => e.code(),
            Self::Rea(e) => e.code(),
        }
    }
}

/// Arguments of the daemon command `initialize_project_agent`. `runner`, `model` and `effort`
/// are an explicit pair (runner and model together, or none).
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct InitializeProjectAgentArgs {
    pub project_id: usize,
    pub mode: InitMode,
    pub is_retry: bool,
    pub runner: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
}

/// Arguments of the daemon command `create_greenfield_project`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CreateGreenfieldProjectArgs {
    pub name: String,
    pub parent_path: String,
    pub init_git: bool,
    pub enable_rea: bool,
    pub tracking_mode: Option<String>,
    pub runner: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
}

fn io_err(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> InitializerError {
    let context = context.into();
    move |source| InitializerError::Io { context, source }
}

/// How the initializer runs: a greenfield scoping conversation, a brownfield proposal the user
/// reviews, or a pre-authorized brownfield run that writes the configuration directly.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum InitMode {
    Greenfield,
    Supervised,
    Unattended,
}

impl InitMode {
    fn as_str(self) -> &'static str {
        match self {
            InitMode::Greenfield => "greenfield",
            InitMode::Supervised => "supervised",
            InitMode::Unattended => "unattended",
        }
    }
}

/// Blocking pre-boot results needed to build the initializer prompt.
struct PreBoot {
    project: ProjectEntry,
    project_uid: String,
    existing_init: Option<ProjectInitState>,
    git_checkpoint: Option<String>,
}

/// Injects the platform and initializer skills and reads the project's identity and state.
/// The uid is `[project].id` of a valid `rula.toml`, or a new one when the file or key is absent;
/// an invalid `rula.toml` is an error (repaired through the `invalid_config` flag).
fn pre_boot(project_id: usize, mode: InitMode) -> Result<PreBoot, InitializerError> {
    let project = crate::registry::find_project_by_id(project_id)?;
    crate::logger::log("DEBUG", "initializer", &format!("Running deterministic pre-boot for {} (mode: {})", project.path, mode.as_str()));
    // Base Platform Skills, AGENTS.md governance block, runner MCP configs, CLAUDE.md symlink.
    crate::kopeng::skill::inject_platform_skill(project.path.clone())?;
    let mut specialized = vec![INITIALIZER_SKILL];
    if mode == InitMode::Greenfield {
        specialized.extend(["oneship-docstyles", "oneship-archetypes"]);
    }
    for skill_name in specialized {
        crate::kopeng::skill::inject_specialized_skill(project.path.clone(), skill_name.to_string())?;
    }

    let root = Path::new(&project.path);
    let existing_uid = crate::config::read_project_rula(root)?
        .and_then(|rula| rula.get("project")?.get("id")?.as_str().map(str::to_string));
    let project_uid = existing_uid.unwrap_or_else(|| format!("proj-{}", uuid::Uuid::new_v4()));
    let existing_init = crate::db::db_get_init_state(&project.path)?;
    let git_checkpoint = match root.join(".git").exists() {
        true => crate::kopeng::snapshots::head_commit(root)?,
        false => None,
    };
    Ok(PreBoot { project, project_uid, existing_init, git_checkpoint })
}

/// Launches the initializer agent of a registered project and returns a launch summary.
///
/// Pre-boot (skills, uid, state) runs in a blocking task; the init state is recorded as
/// `running` before the agent is spawned (resuming the previous session unless `is_retry`).
/// Supervised and unattended runs get a bounded background watcher (`watch_initialization`);
/// every mode ends with the `complete_project_initialization` MCP tool.
pub async fn initialize_project_agent(
    project_id: usize,
    mode: InitMode,
    is_retry: bool,
    runner: Option<String>,
    model: Option<String>,
    effort: Option<String>,
) -> Result<String, InitializerError> {
    crate::logger::log("DEBUG", "initializer", &format!("initialize_project_agent: project_id={}, mode={}, is_retry={}", project_id, mode.as_str(), is_retry));
    let PreBoot { project, project_uid, existing_init, git_checkpoint } = tokio::task::spawn_blocking(move || pre_boot(project_id, mode))
        .await
        .map_err(|e| InitializerError::Task(e.to_string()))??;
    let project_path = project.path.clone();
    let project_name = project.name.clone();

    // The pair comes whole from the init modal's choice or from the initializer pair of the
    // configuration, validated against the runner manifest.
    let explicit = crate::kopeng::resolution::ExplicitAgentConfig::from_parts("initializer launch", runner, model, effort)?;
    let effective_config = crate::kopeng::resolution::resolve_effective_agent_config(
        None,
        None,
        explicit,
        &crate::kopeng::resolution::INIT_AGENT_KEYS,
    )?;
    let effective_runner = effective_config.runner;
    let effective_model = effective_config.model;
    let effective_effort = effective_config.effort;

    crate::logger::log(
        "DEBUG",
        "initializer",
        &format!(
            "Resolved agent configuration: runner='{}', model='{}', effort='{:?}' for project '{}'",
            effective_runner, effective_model, effective_effort, project_path
        ),
    );

    // An unfinished session with a captured runner conversation is resumed (unless retrying).
    let session_resume_id = existing_init
        .filter(|st| !is_retry && st.status == "running")
        .and_then(|st| st.agy_conversation_id);
    let is_resuming = session_resume_id.is_some();

    let retry_instruction = if is_retry {
        "\n[RETRY MODE ACTIVE]\nThis is a retry attempt after a previous failure. Check the directory for partial progress, recover state, and continue from where you left off.\n"
    } else {
        ""
    };

    // Personas default to the agents' pair (`agent_runner` / `default_model`), not the
    // initializer's own; every pair the initializer writes comes from the manifest catalog.
    let persona_default = crate::kopeng::resolution::resolve_effective_agent_config(
        Some(&project_path),
        None,
        None,
        &crate::kopeng::resolution::DEFAULT_AGENT_KEYS,
    )?;
    let runner_catalog = crate::kopeng::resolution::runner_catalog_markdown()?;
    let persona_rules = format!(
        "## MANDATORY PERSONA CREATION RULES (`rula.toml`):\n\
         When defining agent personas under `[agents.<id>]`:\n\
         1. Configure domain specialist personas strictly dictated by the project's actual architectural boundaries and decoupled directory responsibilities (e.g., a single lead architect for simple scripts or compact utilities; distinct specialists like core/backend, frontend, git only when the codebase has distinct decoupled directories/technologies). Never invent redundant specialists or adhere to arbitrary numerical limits.\n\
         2. Every persona declares `runner` and `model` as a pair taken ONLY from the runner catalog below (generated from the ONE-SHIP runner manifests); never write a model that is not listed under its runner, and never choose models from memory. Unless the project needs a different pair, use the default pair `runner = \"{}\"`, `model = \"{}\"`. `effort` is optional and, when set, must be one of the efforts the catalog lists for that model. Also set `mcp_dialogue_mode = true` and `auto_approve = true`.\n\
            Runner catalog:\n{}\n\
         3. Every persona's `system_prompt` MUST be strictly bounded (under 100 words) containing ONLY:\n\
            - Role & exact file directory ownership.\n\
            - Negative boundaries & delegation target via `delegate_agent_task`.\n\
            - Key tech stack (descriptive only; NEVER include verification commands or imperative action directives in system_prompt — define verification commands strictly in `[scripts]`).\n\
         4. NEVER duplicate platform communication rules or approvals in persona prompts (governed by `AGENTS.md`).\n\
         5. Set `context_docs` to at most 1-2 targeted documentation paths (paths only, loaded lazily).\n\
         6. In `AGENTS.md`, preserve existing rules and enrich the guidelines sections below the managed header block.",
        persona_default.runner, persona_default.model, runner_catalog
    );

    let vars = [
        ("PROJECT_NAME", project_name.as_str()),
        ("PROJECT_PATH", project_path.as_str()),
        ("PROJECT_UID", project_uid.as_str()),
        ("RETRY_INSTRUCTION", retry_instruction),
        ("PERSONA_RULES", persona_rules.as_str()),
        ("ONESHIP_VERSION", crate::kopeng::skill::CURRENT_PLATFORM_VERSION),
    ];
    let template_name = match mode {
        InitMode::Greenfield => "greenfield_initializer",
        InitMode::Supervised | InitMode::Unattended => "project_initializer",
    };
    let base_prompt = crate::kopeng::skill::load_prompt_template(template_name, &vars).await?;
    crate::logger::log(
        "DEBUG",
        "initializer",
        &format!("Loaded prompt template '{}' for project '{}'", template_name, project_name),
    );

    let prompt = if is_resuming {
        format!(
            "Session resumed for project \"{}\" at \"{}\". Please review previous progress and continue with the initialization.",
            project_name, project_path
        )
    } else if mode == InitMode::Greenfield {
        format!(
            "{}\n\n\
             ## MANDATORY GREENFIELD FIRST-TURN DIRECTIVE:\n\
             1. This is a brand new project. The directory \"{}\" is intentionally empty.\n\
             2. ZERO TOOL CALLS BEFORE RESPONSE: DO NOT scan for existing files, execute shell commands, or raise any initialization flags. YOUR VERY FIRST ACTION IS TO CALL `post_agent_response`.\n\
             3. Welcome the developer and introduce yourself as the Lead Systems Architect directly in the chat.\n\
             4. Initiate the conversational scoping turn by calling `post_agent_response`:\n\
                - `agent_id`: \"initializer\"\n\
                - `dialogue`: Friendly conversational greeting, the scoping roadmap in steps, an ASCII roadmap diagram, and your first questions for the developer. Use rich Markdown (lists, formatting, and ```text diagram). Never leave dialogue as a brief redirection stub.\n\
                - DO NOT emit an artifact on this first turn. Scoping and requirements discussion happens strictly in the chat conversation.\n\
             5. Conclude your turn and await the developer's reply in the Chat Overlay.",
            base_prompt, project_path
        )
    } else if mode == InitMode::Supervised {
        format!(
            "{}\n\n\
             ## MANDATORY SUPERVISED MODE DIRECTIVE:\n\
             1. Non-invasively inspect the codebase strictly within \"{}\" to determine the stack, persistent and task scripts, and domain specialist personas.\n\
             2. DO NOT write `rula.toml` or edit `AGENTS.md` during this turn.\n\
             3. Formulate the Phase 1 Initialization Technical Proposal and deliver it exclusively by calling the MCP tool `post_agent_response`:\n\
                - `agent_id`: \"initializer\"\n\
                - `dialogue`: Thorough conversational explanation of what was discovered, key architectural decisions, proposed services, and next steps for the developer. Use rich Markdown.\n\
                - `artifact`: The structured technical proposal containing the proposed `rula.toml` configuration and `AGENTS.md` guidelines.\n\
                - `artifact_type`: \"proposal\"\n\
                - `artifact_lang`: \"toml\"\n\
             4. DO NOT print markdown blocks directly to the terminal stdout. Call `post_agent_response` and conclude your turn so the user can review the proposal in the Chat Overlay.",
            base_prompt, project_path
        )
    } else {
        format!(
            "{}\n\n\
             ## MANDATORY UNATTENDED EXECUTION MANDATE (EXPLICIT USER PRE-AUTHORIZATION):\n\
             The human developer has explicitly launched this task in UNATTENDED mode and has ALREADY granted full pre-authorization to write the initialization files directly.\n\
             - MANDATORY OVERRIDE: The requirement to present a technical proposal and wait for user approval is PRE-APPROVED by the human developer for this unattended run. DO NOT generate an interactive proposal or ask for user confirmation.\n\
             - DO NOT pause, halt, or wait for user confirmation. Stopping without writing files is a critical failure.\n\
             - You MUST immediately inspect files using `list_dir` or `view_file` and invoke file-writing tools (`write_to_file`, `replace_file_content`) to create `{}/rula.toml` and enrich `{}/AGENTS.md` before concluding your turn.\n\
             - Analyze the codebase inside \"{}\", determine the stack, and write the necessary [project], [scripts], and bounded [agents] domain specialist personas directly.\n\
             - Record the Genesis Milestone with `record_session_minutes`.\n\
             - Deliver your spoken TTS dialogue summary and artifact plan with `post_agent_response`, then call `complete_project_initialization` as your final action.",
            base_prompt, project_path, project_path, project_path
        )
    };

    let init_state_record = ProjectInitState {
        project_path: project_path.clone(),
        status: "running".to_string(),
        mode: mode.as_str().to_string(),
        current_phase: Some(if is_resuming { "Resuming session..." } else { "Starting up..." }.to_string()),
        last_message: None,
        git_checkpoint,
        agy_conversation_id: session_resume_id.clone(),
    };
    tokio::task::spawn_blocking(move || crate::db::db_upsert_init_state(&init_state_record))
        .await
        .map_err(|e| InitializerError::Task(e.to_string()))??;

    // spawn_agent stops an earlier initializer instance first.
    let sup = crate::supervisor::get_supervisor();
    let key = crate::supervisor::ProcessSupervisor::format_agent_key(&project_path, INITIALIZER_AGENT_ID);
    let pid = sup
        .spawn_agent(
            &project_path,
            INITIALIZER_AGENT_ID,
            &effective_runner,
            &effective_model,
            effective_effort.as_deref(),
            true,
            true,
            &[],
            &[],
            session_resume_id.as_deref(),
            None,
            !is_resuming,
        )
        .await?;

    crate::logger::log(
        "INFO",
        "initializer",
        &format!(
            "Starting initialization (mode: {}, runner: {}, model: {}, resuming: {}, PID: {}) for {}",
            mode.as_str(), effective_runner, effective_model, is_resuming, pid, project_path
        ),
    );

    // Pipe the initial prompt to the interactive child process stdin stream.
    let key_input = key.clone();
    let proj_input = project_path.clone();
    tauri::async_runtime::spawn(async move {
        let delay_ms = crate::config::resolve_effective_u64(None, None, "orchestrator", "agent_spawn_poll_step_ms");
        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
        if let Err(e) = crate::supervisor::get_supervisor().send_input(&key_input, &prompt).await {
            crate::logger::log("ERROR", "initializer", &format!("Cannot deliver the initialization prompt to '{}': {}", key_input, e));
            return;
        }
        crate::conversation::mark_user_message_sent(&proj_input, INITIALIZER_AGENT_ID);
    });

    // Greenfield onboarding is an interactive multi-turn conversation in the Chat Overlay
    // (Spec 26): it has no timeout watcher.
    if mode == InitMode::Greenfield {
        crate::logger::log("INFO", "initializer", &format!("Greenfield initialization active in Chat Overlay for {}; no timeout watcher.", project_path));
    } else {
        tauri::async_runtime::spawn(watch_initialization(project_path, key, mode));
    }

    Ok(format!("Background initialization ({}) started with PID {}.", mode.as_str(), pid))
}

/// Records the native session id of the project's initializer in its init state, so a later
/// launch can resume it. Called by the supervisor stream readers; failures are logged (the
/// readers cannot return them), and a missing init state is an error.
pub(crate) fn record_initializer_session(project_path: &str, session_id: &str) {
    match crate::db::db_get_init_state(project_path) {
        Ok(Some(mut state)) => {
            state.agy_conversation_id = Some(session_id.to_string());
            crate::logger::log_failure("initializer", "record initializer session", crate::db::db_upsert_init_state(&state));
        }
        Ok(None) => crate::logger::log("ERROR", "initializer", &format!("Initializer session {} of {} not recorded: the project has no init state", session_id, project_path)),
        Err(e) => crate::logger::log("ERROR", "initializer", &format!("Initializer session {} of {} not recorded: {}", session_id, project_path, e)),
    }
}

fn emit_status_report(project_path: &str, status: &str, summary: &str) {
    crate::supervisor::get_supervisor().emit_event(crate::events::DaemonEvent::agent_status_report(&crate::events::AgentStatusReportPayload {
        agent_id: INITIALIZER_AGENT_ID.to_string(),
        project_path: Some(project_path.to_string()),
        status: status.to_string(),
        summary: Some(summary.to_string()),
        details: None,
        task_id: None,
        needs_user_input: None,
        timestamp: crate::mcp::chrono_or_epoch() as i64,
    }));
}

fn initialization_completed(project_path: &str) -> bool {
    matches!(crate::db::db_get_init_state(project_path), Ok(Some(st)) if st.status == "completed")
}

/// Marks the initialization failed: removes the ephemeral skill, stops the agent, records the
/// state and reports it. Each step that fails is logged; the remaining steps still run.
async fn fail_initialization(project_path: &str, key: &str, phase: &str, message: &str) {
    crate::logger::log("ERROR", "initializer", &format!("Initialization of {} failed: {}", project_path, message));
    crate::logger::log_failure(
        "initializer",
        "cleanup oneship-initializer skill",
        crate::kopeng::skill::cleanup_ephemeral_skill(project_path.to_string(), INITIALIZER_SKILL.to_string()),
    );
    match crate::supervisor::get_supervisor().stop_process(key).await {
        Ok(()) | Err(SupervisorError::ProcessNotRunning { .. }) => {}
        Err(e) => crate::logger::log("ERROR", "initializer", &format!("Cannot stop initializer '{}': {}", key, e)),
    }
    match crate::db::db_get_init_state(project_path) {
        Ok(Some(mut state)) => {
            state.status = "failed".to_string();
            state.current_phase = Some(phase.to_string());
            state.last_message = Some(message.to_string());
            crate::logger::log_failure("initializer", "record failed init state", crate::db::db_upsert_init_state(&state));
        }
        Ok(None) => crate::logger::log("ERROR", "initializer", &format!("No init state recorded for {}; the failure is only reported", project_path)),
        Err(e) => crate::logger::log("ERROR", "initializer", &format!("Cannot read init state of {}: {}", project_path, e)),
    }
    emit_status_report(project_path, "failed", message);
}

/// Bounded watcher of a supervised or unattended initialization (`init_timeout_minutes`).
///
/// Ends when the state is `completed` (closed by `complete_project_initialization`), when a
/// supervised run reaches `PROPOSAL_PRESENTED_PHASE` (the review continues in the chat), or
/// fails the initialization when the agent exits or fails first, or on timeout.
async fn watch_initialization(project_path: String, key: String, mode: InitMode) {
    let sup = crate::supervisor::get_supervisor();
    let poll_step_ms = crate::config::resolve_effective_u64(None, None, "orchestrator", "agent_spawn_poll_step_ms").max(50);
    let timeout_minutes = crate::config::resolve_effective_u64(None, None, "orchestrator", "init_timeout_minutes");
    let max_attempts_count = timeout_minutes.saturating_mul(60_000) / poll_step_ms;

    for _ in 0..max_attempts_count {
        tokio::time::sleep(tokio::time::Duration::from_millis(poll_step_ms)).await;
        match crate::db::db_get_init_state(&project_path) {
            Ok(Some(st)) if st.status == "completed" => {
                crate::logger::log("DEBUG", "initializer", &format!("Initialization of {} completed; watcher ends", project_path));
                return;
            }
            Ok(Some(st)) if mode == InitMode::Supervised && st.current_phase.as_deref() == Some(PROPOSAL_PRESENTED_PHASE) => {
                crate::logger::log("INFO", "initializer", &format!("Initialization proposal presented for {}; watcher ends, review continues in the chat", project_path));
                emit_status_report(&project_path, "waiting_for_user", "Technical proposal presented in Chat Overlay. Awaiting user review.");
                return;
            }
            Ok(_) => {}
            Err(e) => crate::logger::log("ERROR", "initializer", &format!("Cannot read init state of {}: {}", project_path, e)),
        }

        let failure = match sup.get_status(&key) {
            ProcessStatus::Completed { exit_code } => {
                let tail_lines_count = crate::config::resolve_effective_usize(None, None, "orchestrator", "agent_log_tail_lines_count");
                let tail = sup.read_output(&key, tail_lines_count).iter().rev().take(5).cloned().collect::<Vec<_>>().join("\n");
                Some(format!("The initializer exited with code {} before completing the initialization: {}", exit_code, tail))
            }
            ProcessStatus::Failed { error } => Some(format!("Supervisor error: {}", error)),
            ProcessStatus::Unlaunched => Some("The initializer process is no longer registered".to_string()),
            ProcessStatus::Starting | ProcessStatus::Idle | ProcessStatus::Working | ProcessStatus::Blocked => None,
        };
        if let Some(message) = failure {
            // `complete_project_initialization` stops the agent after recording completion.
            if initialization_completed(&project_path) {
                return;
            }
            fail_initialization(&project_path, &key, "Agent exited", &message).await;
            return;
        }
    }
    let message = format!("Initialization exceeded the {}-minute limit", timeout_minutes);
    fail_initialization(&project_path, &key, "Timed out", &message).await;
}

/// Closes the initialization of `project` (called by the `complete_project_initialization` MCP tool).
///
/// Requires an initialization in progress, a valid `rula.toml` declaring `[project].id` and a
/// session journal entry of the initializer (the Genesis milestone). Records `completed`,
/// removes the ephemeral skill and reports the completion; the caller stops the agent.
pub(crate) fn complete_initialization(project: &ProjectEntry) -> Result<(), InitializerError> {
    let incomplete = |reason: &str| InitializerError::Incomplete { project: project.name.clone(), reason: reason.to_string() };
    let rula = crate::config::read_project_rula(Path::new(&project.path))?
        .ok_or_else(|| incomplete("'rula.toml' was not found in the project root. Create it with your project definition and [project].id first"))?;
    let has_uid = rula.get("project").and_then(|p| p.get("id")).and_then(|v| v.as_str()).is_some_and(|id| !id.trim().is_empty());
    if !has_uid {
        return Err(incomplete("'rula.toml' declares no [project].id. Please assign a valid project UID (e.g. proj-<uuid>)"));
    }
    if crate::db::db_get_project_session_journals(project.id as i64, Some(INITIALIZER_AGENT_ID), 1)?.is_empty() {
        return Err(incomplete("the Genesis milestone was not recorded. Call the MCP tool 'record_session_minutes' with a summary of Phase 1 discovery, then invoke complete_project_initialization again"));
    }

    let mut state = crate::db::db_get_init_state(&project.path)?
        .ok_or_else(|| incomplete("no initialization state is recorded for this project"))?;

    state.status = "completed".to_string();
    state.current_phase = Some("Initialization completed successfully".to_string());
    state.last_message = Some("Initialization finished successfully".to_string());
    crate::db::db_upsert_init_state(&state)?;
    crate::kopeng::skill::cleanup_ephemeral_skill(project.path.clone(), INITIALIZER_SKILL.to_string())?;
    crate::logger::log("INFO", "initializer", &format!("Initialization of '{}' completed", project.name));
    emit_status_report(&project.path, "completed", "Initialization finished successfully");
    crate::supervisor::get_supervisor().emit_event(crate::events::DaemonEvent::agent_step_update(&crate::events::AgentStepUpdatePayload {
        agent_id: INITIALIZER_AGENT_ID.to_string(),
        project_path: project.path.clone(),
        conversation_id: None,
        state: "DONE".to_string(),
        step_type: "result".to_string(),
        tool_name: None,
        action_summary: "Initialization completed".to_string(),
        thought_text: None,
        timestamp_ms: crate::mcp::chrono_or_epoch() * 1000,
    }));
    Ok(())
}

/// Creates a greenfield project (directory, optional git repository, optional REA scaffold),
/// registers it and launches its greenfield initializer.
#[allow(clippy::too_many_arguments)]
pub async fn create_greenfield_project(
    name: String,
    parent_path: String,
    init_git: bool,
    enable_rea: bool,
    tracking_mode: Option<String>,
    runner: Option<String>,
    model: Option<String>,
    effort: Option<String>,
) -> Result<crate::hydration::HydratedProject, InitializerError> {
    crate::logger::log("DEBUG", "initializer", &format!("create_greenfield_project: name='{}', parent='{}', git={}, rea={}, tracking_mode={:?}", name, parent_path, init_git, enable_rea, tracking_mode));
    let trimmed_name = validate_project_name(&name)?.to_string();
    let validated_tracking_mode = match tracking_mode.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some("linear") => Some("linear".to_string()),
        Some("shape_up") => Some("shape_up".to_string()),
        Some("weighted") => Some("weighted".to_string()),
        Some(_) | None => None,
    };
    // Validated before anything is created. The REA `reverser` persona takes the chosen pair,
    // else the agents' default pair; both come from the runner manifests.
    let explicit = crate::kopeng::resolution::ExplicitAgentConfig::from_parts("greenfield creation", runner.clone(), model.clone(), effort.clone())?;
    let reverser_pair = crate::kopeng::resolution::resolve_effective_agent_config(None, None, explicit, &crate::kopeng::resolution::DEFAULT_AGENT_KEYS)?;
    let hydrated = tokio::task::spawn_blocking(move || {
        scaffold_greenfield_project(&trimmed_name, &parent_path, init_git, enable_rea, validated_tracking_mode.as_deref(), &reverser_pair)
    })
    .await
    .map_err(|e| InitializerError::Task(e.to_string()))??;

    initialize_project_agent(hydrated.id, InitMode::Greenfield, false, runner, model, effort).await?;
    Ok(hydrated)
}

/// A project name is one non-empty path component.
fn validate_project_name(name: &str) -> Result<&str, InitializerError> {
    let trimmed = name.trim();
    let invalid = |reason| InitializerError::InvalidProjectName { name: name.to_string(), reason };
    if trimmed.is_empty() {
        return Err(invalid("it is empty"));
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err(invalid("it contains a path separator"));
    }
    if trimmed == "." || trimmed == ".." {
        return Err(invalid("it is a relative directory reference"));
    }
    Ok(trimmed)
}

/// Initial `rula.toml` of a greenfield REA lab: identity, the enabled REA plugin and the
/// `reverser` persona with its runner/model pair (and effort, when the model has one).
fn rea_lab_rula(name: &str, pair: &crate::kopeng::resolution::EffectiveAgentConfig) -> toml::Value {
    let mut project = toml::map::Map::new();
    project.insert("id".to_string(), toml::Value::String(format!("proj-{}", uuid::Uuid::new_v4())));
    project.insert("name".to_string(), toml::Value::String(name.to_string()));
    project.insert("oneship_version".to_string(), toml::Value::String(crate::kopeng::skill::CURRENT_PLATFORM_VERSION.to_string()));

    let mut rea = toml::map::Map::new();
    rea.insert("enabled".to_string(), toml::Value::Boolean(true));
    let mut plugins = toml::map::Map::new();
    plugins.insert("rea".to_string(), toml::Value::Table(rea));

    let mut reverser = toml::map::Map::new();
    reverser.insert("role".to_string(), toml::Value::String("Reverse Engineering & Protocol Specialist".to_string()));
    reverser.insert(
        "system_prompt".to_string(),
        toml::Value::String("Inspect binaries in targets/ using REA MCP tools, extract verifiable evidence, and populate .one-ship/rea/views/ JSON maps.".to_string()),
    );
    reverser.insert("runner".to_string(), toml::Value::String(pair.runner.clone()));
    reverser.insert("model".to_string(), toml::Value::String(pair.model.clone()));
    if let Some(effort) = &pair.effort {
        reverser.insert("effort".to_string(), toml::Value::String(effort.clone()));
    }
    let mut agents = toml::map::Map::new();
    agents.insert("reverser".to_string(), toml::Value::Table(reverser));

    let mut root = toml::map::Map::new();
    root.insert("project".to_string(), toml::Value::Table(project));
    root.insert("plugins".to_string(), toml::Value::Table(plugins));
    root.insert("agents".to_string(), toml::Value::Table(agents));
    toml::Value::Table(root)
}

/// Creates the greenfield directory, optional Git repository and REA scaffold, and registers the
/// project. Blocking (filesystem, `git`, SQLite): call through `spawn_blocking`.
fn scaffold_greenfield_project(
    trimmed_name: &str,
    parent_path: &str,
    init_git: bool,
    enable_rea: bool,
    tracking_mode: Option<&str>,
    reverser_pair: &crate::kopeng::resolution::EffectiveAgentConfig,
) -> Result<crate::hydration::HydratedProject, InitializerError> {
    let parent = Path::new(parent_path);
    if !parent.is_dir() {
        return Err(InitializerError::ParentNotDirectory(parent_path.to_string()));
    }

    let project_path = parent.join(trimmed_name);
    crate::logger::log("INFO", "initializer", &format!("Creating greenfield project '{}' at {}", trimmed_name, project_path.display()));
    match fs::symlink_metadata(&project_path) {
        Ok(meta) if meta.is_dir() => {
            let mut entries = fs::read_dir(&project_path).map_err(io_err(format!("Cannot read {}", project_path.display())))?;
            if entries.next().is_some() {
                return Err(InitializerError::TargetNotEmpty(project_path.display().to_string()));
            }
        }
        Ok(_) => return Err(InitializerError::TargetNotEmpty(project_path.display().to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(&project_path).map_err(io_err(format!("Cannot create {}", project_path.display())))?;
        }
        Err(e) => return Err(io_err(format!("Cannot inspect {}", project_path.display()))(e)),
    }

    // A requested repository that cannot be created stops the creation.
    if init_git {
        crate::kopeng::git::init_repository(&project_path)?;
    }

    let mut hydrated = crate::registry::add_project(project_path.to_string_lossy().to_string())?;

    if enable_rea {
        crate::kopeng::rea::ensure_project_rea_directories(&project_path)?;
        let mut rula = rea_lab_rula(trimmed_name, reverser_pair);
        if let Some(mode) = tracking_mode {
            if let Some(plugins) = rula.get_mut("plugins").and_then(|v| v.as_table_mut()) {
                let mut pm = toml::map::Map::new();
                pm.insert("enabled".to_string(), toml::Value::Boolean(true));
                pm.insert("tracking_mode".to_string(), toml::Value::String(mode.to_string()));
                plugins.insert("project_management".to_string(), toml::Value::Table(pm));
            }
        }
        crate::config::write_project_rula(&project_path, &rula)?;
        crate::kopeng::skill::inject_specialized_skill(hydrated.path.clone(), "oneship-rea".to_string())?;
        hydrated.has_rea_enabled = true;
        if tracking_mode.is_some() {
            crate::kopeng::skill::inject_specialized_skill(hydrated.path.clone(), "oneship-project-mgmt".to_string())?;
            hydrated.has_project_management_enabled = true;
        }
    } else if let Some(mode) = tracking_mode {
        let mut project = toml::map::Map::new();
        project.insert("id".to_string(), toml::Value::String(format!("proj-{}", uuid::Uuid::new_v4())));
        project.insert("name".to_string(), toml::Value::String(trimmed_name.to_string()));
        project.insert("oneship_version".to_string(), toml::Value::String(crate::kopeng::skill::CURRENT_PLATFORM_VERSION.to_string()));
        let mut pm = toml::map::Map::new();
        pm.insert("enabled".to_string(), toml::Value::Boolean(true));
        pm.insert("tracking_mode".to_string(), toml::Value::String(mode.to_string()));
        let mut plugins = toml::map::Map::new();
        plugins.insert("project_management".to_string(), toml::Value::Table(pm));
        let mut root = toml::map::Map::new();
        root.insert("project".to_string(), toml::Value::Table(project));
        root.insert("plugins".to_string(), toml::Value::Table(plugins));
        crate::config::write_project_rula(&project_path, &toml::Value::Table(root))?;
        crate::kopeng::skill::inject_specialized_skill(hydrated.path.clone(), "oneship-project-mgmt".to_string())?;
        hydrated.has_project_management_enabled = true;
    }

    crate::logger::log(
        "DEBUG",
        "initializer",
        &format!("Greenfield project registered: id={}, path='{}', rea={}", hydrated.id, hydrated.path, enable_rea),
    );
    Ok(hydrated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_names_are_single_path_components() {
        assert_eq!(validate_project_name("  lab  ").unwrap(), "lab");
        for bad in ["", "   ", "a/b", "a\\b", ".", ".."] {
            assert!(matches!(validate_project_name(bad), Err(InitializerError::InvalidProjectName { .. })), "{:?}", bad);
        }
    }

    #[test]
    fn init_modes_are_closed() {
        assert_eq!(serde_json::from_str::<InitMode>("\"supervised\"").unwrap(), InitMode::Supervised);
        assert!(serde_json::from_str::<InitMode>("\"brownfield\"").is_err());
    }

    #[test]
    fn rea_lab_rula_writes_a_valid_reverser_pair_and_escapes_names() {
        let pair = crate::kopeng::resolution::EffectiveAgentConfig {
            runner: "claude".into(),
            model: "claude-sonnet-5-5".into(),
            effort: Some("medium".into()),
        };
        let value = rea_lab_rula("lab \"x\"", &pair);
        let serialized = toml::to_string_pretty(&value).unwrap();
        let reparsed: toml::Value = toml::from_str(&serialized).unwrap();
        assert_eq!(reparsed["project"]["name"].as_str(), Some("lab \"x\""));
        assert!(reparsed["project"]["id"].as_str().unwrap().starts_with("proj-"));
        assert_eq!(reparsed["plugins"]["rea"]["enabled"].as_bool(), Some(true));
        assert_eq!(reparsed["agents"]["reverser"]["runner"].as_str(), Some("claude"));
        assert_eq!(reparsed["agents"]["reverser"]["model"].as_str(), Some("claude-sonnet-5-5"));
        // The scaffold passes rula validation (write_project_rula refuses an invalid document).
        let dir = tempfile::tempdir().unwrap();
        assert!(crate::config::write_project_rula(dir.path(), &value).is_ok());
    }
}
