//! Operator RPC API of the daemon (spec 12 §7): `POST /api/v1/call/<command>` with a JSON
//! object of arguments. The daemon is the only owner of processes and state; the desktop app
//! is an interface that calls these commands through `src/utils/daemonClient.ts`.
//!
//! The route sits behind the operator guard of `mcp.rs` (webview `Origin`, not a managed agent,
//! descends from the ONE-SHIP app binary). A success is the command's JSON result with status
//! 200; a failure is [`ApiErrorResponse`] with the status of its [`ApiError`].
//!
//! Every command is registered in [`API_COMMANDS`] and dispatched in [`dispatch`]; the frontend
//! mirrors it in `src/types/daemonApi.ts` (checked by `api_commands_match_the_frontend_map`).

use axum::body::Bytes;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::kopeng::{
    accounts, desigma, diagnostic, documents, electronics, flag_resolver, forge, fs_tree, git, github, gitlab, initializer, manifest, migration, pcbcraft, persona,
    profiler, project_mgmt, rea, resolution, runner, skill, snapshots, solidcraft, stats,
};
use crate::{config, conversation, db, dictation, logger, orchestrator, registry, semantic, supervisor, theme, ui_state, voice_controller};

/// A domain error the operator API can return: an HTTP status (4xx for a request the caller can
/// correct, 5xx for a failure of the daemon or of an external tool) and a stable
/// `<Enum>::<Variant>` code the frontend can match on.
pub trait ApiError: std::error::Error {
    fn status(&self) -> StatusCode;
    fn code(&self) -> &'static str;
}

/// Body of a failed call.
/// Structured error payload returned by the daemon HTTP RPC layer.
#[derive(Serialize, Debug, TS)]
#[ts(export)]
pub struct ApiErrorResponse {
    pub error: ApiErrorBody,
}

/// Structured error payload returned by the daemon HTTP RPC layer.
#[derive(Serialize, Debug, TS)]
#[ts(export)]
pub struct ApiErrorBody {
    /// Stable `<Enum>::<Variant>` identifier of the error.
    pub code: String,
    /// Human-readable message (the error's `Display`).
    pub message: String,
}

/// Failures of the RPC layer itself.
#[derive(Debug, thiserror::Error)]
pub enum ApiCallError {
    #[error("Unknown daemon command '{0}'")]
    UnknownCommand(String),
    #[error("Invalid arguments for '{command}': {details}")]
    InvalidArguments { command: String, details: String },
    #[error("Cannot serialize the result of '{command}': {details}")]
    Serialize { command: String, details: String },
    /// The blocking-pool task running a synchronous command panicked or was cancelled.
    #[error("Command '{command}' failed on the blocking pool: {details}")]
    Task { command: String, details: String },
}

impl ApiError for ApiCallError {
    fn status(&self) -> StatusCode {
        match self {
            Self::UnknownCommand(_) => StatusCode::NOT_FOUND,
            Self::InvalidArguments { .. } => StatusCode::BAD_REQUEST,
            Self::Serialize { .. } | Self::Task { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::UnknownCommand(_) => "ApiCallError::UnknownCommand",
            Self::InvalidArguments { .. } => "ApiCallError::InvalidArguments",
            Self::Serialize { .. } => "ApiCallError::Serialize",
            Self::Task { .. } => "ApiCallError::Task",
        }
    }
}

/// Arguments of a command that takes none. Unknown keys are rejected like for every command.
#[derive(Deserialize, Debug, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct NoArgs {}

/// Arguments of the commands that take only a registered project's path.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ProjectPathArgs {
    pub project_path: String,
}

/// Arguments of the commands that take only a registered project's id.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ProjectIdArgs {
    pub project_id: usize,
}

/// Every command served by [`dispatch`].
pub const API_COMMANDS: &[&str] = &[
    "run_diagnostics",
    "get_host_capabilities",
    "install_missing_tools",
    "run_setup_terminal_action",
    "set_git_global_identity",
    "ensure_projects_root_dir",
    "search_github_repos",
    "check_git_remote_access",
    "clone_and_add_project",
    "link_project_remote",
    "create_github_repo",
    "search_gitlab_repos",
    "create_gitlab_repo",
    "get_configured_agent_pair",
    // Services and tasks
    "start_service",
    "stop_service",
    "release_port",
    "run_project_task",
    // Agents
    "get_project_agent_state",
    "dispatch_git_instruction",
    "launch_agent",
    "restart_agent",
    "stop_agent",
    "interrupt_agent",
    "prompt_agent",
    // Terminals
    "read_terminal_output",
    "send_terminal_input",
    "launch_interactive_shell",
    "list_interactive_shells",
    // Personas
    "create_agent_persona",
    "update_agent_persona",
    "delete_agent_persona",
    // Initialization and flag resolution
    "initialize_project_agent",
    "create_greenfield_project",
    "launch_flag_resolution_agent",
    "launch_project_removal_agent",
    // REA
    "check_rea_toolchain",
    "get_rea_engine_status",
    "get_rea_queue_snapshot",
    "start_rea_engine",
    "stop_rea_engine",
    "toggle_project_rea_plugin",
    "list_rea_targets",
    "get_rea_visual_views",
    // Electronics & Embedded Lab
    "list_connected_mcus",
    "scan_connected_instruments",
    "list_supported_oscilloscopes",
    "get_project_electronics_status",
    "toggle_project_electronics_plugin",
    "build_project_firmware",
    "flash_project_firmware",
    "capture_oscilloscope",
    "capture_logic_analyzer",
    "calibrate_oscilloscope",
    "list_platformio_boards",
    "scaffold_firmware_project",
    // Desigma Visual Design
    "get_project_desigma_status",
    "toggle_project_desigma_plugin",
    "launch_project_desigma",
    "render_desigma_preview",
    // SolidCraft 3D CAD
    "get_project_solidcraft_status",
    "toggle_project_solidcraft_plugin",
    "create_solidcraft_file",
    "launch_project_solidcraft",
    "render_solidcraft_preview",
    "inspect_solidcraft_metrics",
    // PCBCraft EDA / PCB
    "get_project_pcbcraft_status",
    "toggle_project_pcbcraft_plugin",
    "create_pcbcraft_project",
    "launch_project_pcbcraft",
    "inspect_pcbcraft_project",
    "render_pcbcraft_svg_preview",
    // Conversations, Minutes, TODOs and technical debt
    "get_active_conversation",
    "create_agent_conversation",
    "update_active_conversation_config",
    "get_agent_turns_continuous",
    "save_user_chat_turn",
    "update_turn_approval",
    "get_project_session_journals",
    "build_conversation_brief",
    "get_project_todos",
    "create_project_todo",
    "update_todo_status",
    "get_project_technical_debt",
    "record_technical_debt",
    "resolve_technical_debt",
    "reopen_conversation",
    "search_conversations",
    "search_knowledge",
    "index_project_knowledge",
    "get_indexing_status",
    "get_project_knowledge_graph",
    "rebuild_project_knowledge_graph",
    "get_file_line_insights",
    "get_file_line_range_content",
    "get_file_symbols",
    "resolve_symbol_definition",
    "get_pending_approvals",
    // Accounts and quota
    "list_accounts",
    "switch_account",
    "remove_account",
    "rename_account",
    "initiate_add_account",
    "check_active_agent_sessions",
    "get_deepseek_balance",
    "save_deepseek_api_key",
    "get_agent_metrics",
    "get_session_metrics",
    "get_account_quotas",
    "toggle_account_warmup",
    "kickstart_account_quota",
    "get_context_threshold",
    // Projects and initialization flags
    "get_projects",
    "add_project",
    "remove_project",
    "toggle_project_favorite",
    "toggle_project_archived",
    "toggle_project_tab_visibility",
    "update_project_description",
    "update_project_name",
    "get_project_overview_summary",
    "save_project_overview_layout",
    "get_project_init_flags",
    "resolve_project_init_flag",
    "migrate_project_deterministic",
    "rollback_project_init",
    // Task executions and logs
    "get_task_executions",
    "get_task_report",
    "get_system_logs",
    "get_system_log_sources",
    "clear_system_logs",
    "write_client_log",
    // Configuration and runners
    "get_plugin_schemas",
    "get_global_config",
    "get_scoped_project_config",
    "save_global_config",
    "set_show_welcome_on_startup",
    "save_project_rula_config",
    "list_agent_runners",
    // Desktop session
    "get_active_theme",
    "start_dictation",
    "stop_dictation",
    "cancel_dictation",
    "get_dictation_status",
    "set_dictation_auto_send",
    "get_gemini_api_key_status",
    "set_gemini_api_key",
    "clear_gemini_api_key",
    // UI State Machine & Session Persistence (Spec 35)
    "get_ui_registry",
    "get_ui_state",
    "dispatch_ui_action",
    // Global Voice Controller (Spec 36)
    "get_voice_controller_status",
    "set_voice_controller_enabled",
    "test_voice_controller_voice",
    // Git
    "init_git_repo",
    "get_vcs_actions",
    "save_vcs_action",
    "get_project_git_log",
    "get_git_commit_diff",
    "get_commit_files_changed",
    "get_working_tree_diff",
    "get_working_tree_files",
    "list_git_branches",
    "switch_git_branch",
    "create_git_branch",
    "stage_git_files",
    "unstage_git_files",
    "discard_git_files",
    "commit_git_changes",
    "sync_git_branch",
    "generate_commit_message",
    // Project Management (Spec 34)
    "toggle_project_management_plugin",
    "list_project_milestones",
    "get_project_milestone_detail",
    "create_project_milestone",
    "update_project_milestone",
    "delete_project_milestone",
    "list_project_stages",
    "create_project_stage",
    "update_project_stage",
    "reorder_project_stages",
    "delete_project_stage",
    "list_project_tasks",
    "create_project_task",
    "update_project_task",
    "delete_project_task",
    "create_task_subtask",
    "toggle_task_subtask",
    "delete_task_subtask",
    "link_task_relation",
    "unlink_task_relation",
    "take_forge_issue",
    // Forge Open Source Collaboration & Inbox (Spec 34 / 38 / 39)
    "get_forge_context",
    "configure_upstream_remote",
    "sync_upstream_fetch",
    "list_forge_issues",
    "get_forge_issue",
    "list_forge_pull_requests",
    "get_forge_pull_request",
    "list_forge_releases",
    "create_forge_draft_pull_request",
    "list_forge_inbox",
    "mark_forge_item_read",
    "set_forge_item_flag",
    "get_forge_unread_counts",
    // Skills
    "inject_platform_skill",
    "get_project_skills",
    "toggle_project_skill",
    "remove_project_skill",
    "get_project_skill_content",
    "search_marketplace_skills",
    "install_project_skill",
    // Documents and project files
    "get_project_file_tree",
    "get_project_documents",
    "read_project_document",
    "save_project_document",
    "create_document_comment",
    "list_document_comments",
    "resolve_document_comment",
    "delete_document_comment",
    "create_project_directory",
    "create_project_file",
    "rename_project_item",
    "delete_project_items",
    "copy_project_items",
    "move_project_items",
    "save_clipboard_image",
    "attach_file_to_project",
    "paste_clipboard_image",
    "open_file_in_external_editor",
    "get_project_exclusion_status",
    "toggle_project_exclusion",
    // Snapshots
    "create_project_snapshot",
    "list_project_snapshots",
    "restore_project_snapshot",
    "delete_project_snapshot",
    "get_snapshot_diff",
    // Profiler and run history
    "list_available_profilers",
    "toggle_project_profiler_plugin",
    "get_target_runs",
    "get_run_profile",
    "compare_runs",
];

fn error_response<E: ApiError>(command: &str, error: &E) -> Response {
    let status = error.status();
    let level = if status.is_server_error() { "ERROR" } else { "WARN" };
    crate::logger::log(level, "api", &format!("{} failed ({} {}): {}", command, status.as_u16(), error.code(), error));
    let body = ApiErrorResponse { error: ApiErrorBody { code: error.code().to_string(), message: error.to_string() } };
    (status, Json(body)).into_response()
}

fn parse_args<T: DeserializeOwned>(command: &str, args: Value) -> Result<T, ApiCallError> {
    serde_json::from_value(args)
        .map_err(|e| ApiCallError::InvalidArguments { command: command.to_string(), details: e.to_string() })
}

fn reply<T: Serialize, E: ApiError>(command: &str, result: Result<T, E>) -> Response {
    match result {
        Ok(value) => match serde_json::to_value(&value) {
            Ok(json) => (StatusCode::OK, Json(json)).into_response(),
            Err(e) => error_response(command, &ApiCallError::Serialize { command: command.to_string(), details: e.to_string() }),
        },
        Err(e) => error_response(command, &e),
    }
}

/// Runs `command` with `args`. Each arm parses the command's typed arguments (owned by its
/// domain module and exported to TypeScript) and replies with its typed result or error.
///
/// `call!` awaits an async command; `blocking!` runs a synchronous one (SQLite, `/proc` or file
/// reads) on the blocking pool so it never stalls a Tokio worker (AGENTS.md §9.2).
async fn dispatch(command: &str, args: Value) -> Response {
    macro_rules! call {
        ($ty:ty, |$a:ident| $body:expr) => {
            match parse_args::<$ty>(command, args) {
                Ok($a) => reply(command, $body.await),
                Err(e) => error_response(command, &e),
            }
        };
    }
    macro_rules! blocking {
        ($ty:ty, |$a:ident| $body:expr) => {
            match parse_args::<$ty>(command, args) {
                Ok($a) => match tokio::task::spawn_blocking(move || $body).await {
                    Ok(result) => reply(command, result),
                    Err(e) => error_response(command, &ApiCallError::Task { command: command.to_string(), details: e.to_string() }),
                },
                Err(e) => error_response(command, &e),
            }
        };
    }
    match command {
        "run_diagnostics" => call!(NoArgs, |_a| diagnostic::run_diagnostics()),
        "get_host_capabilities" => call!(NoArgs, |_a| diagnostic::get_host_capabilities()),
        "install_missing_tools" => call!(diagnostic::InstallMissingToolsArgs, |a| diagnostic::install_missing_tools(a.tool_ids)),
        "run_setup_terminal_action" => call!(diagnostic::RunSetupTerminalActionArgs, |a| diagnostic::run_setup_terminal_action(a.action)),
        "set_git_global_identity" => call!(diagnostic::SetGitGlobalIdentityArgs, |a| diagnostic::set_git_global_identity(a.name, a.email)),
        "ensure_projects_root_dir" => call!(diagnostic::EnsureProjectsRootDirArgs, |a| diagnostic::ensure_projects_root_dir(a.path)),
        "search_github_repos" => call!(NoArgs, |_a| github::search_github_repos()),
        "check_git_remote_access" => call!(github::CheckGitRemoteAccessArgs, |a| github::check_git_remote_access(a.repo_url)),
        "clone_and_add_project" => call!(github::CloneAndAddProjectArgs, |a| github::clone_and_add_project(a.repo_url, a.parent_path, a.name)),
        "link_project_remote" => call!(github::LinkProjectRemoteArgs, |a| github::link_project_remote(a.project_id, a.remote_url)),
        "create_github_repo" => call!(github::CreateGithubRepoArgs, |a| github::create_github_repo(a.project_id, a.name, a.is_private)),
        "search_gitlab_repos" => call!(NoArgs, |_a| gitlab::search_gitlab_repos()),
        "create_gitlab_repo" => call!(gitlab::CreateGitlabRepoArgs, |a| gitlab::create_gitlab_repo(a.project_id, a.name, a.is_private)),
        "get_configured_agent_pair" => call!(resolution::GetConfiguredAgentPairArgs, |a| resolution::get_configured_agent_pair(a.pair)),

        "start_service" => call!(orchestrator::StartServiceArgs, |a| orchestrator::start_service(a.project_id, a.service_id, a.options)),
        "stop_service" => call!(orchestrator::StopServiceArgs, |a| orchestrator::stop_service(a.project_id, a.service_id)),
        "release_port" => call!(orchestrator::ReleasePortArgs, |a| orchestrator::release_port(a.port, a.target_pid)),
        "run_project_task" => call!(runner::RunProjectTaskArgs, |a| runner::run_project_task(a.project_id, a.task_id, a.options)),

        "get_project_agent_state" => call!(supervisor::GetProjectAgentStateArgs, |a| supervisor::get_project_agent_state(a.project_path, a.project_name)),
        "dispatch_git_instruction" => call!(supervisor::DispatchGitInstructionArgs, |a| supervisor::dispatch_git_instruction(a.project_id, a.instruction)),
        "launch_agent" => call!(supervisor::LaunchAgentArgs, |a| supervisor::launch_agent(
            a.project_path, a.project_name, a.agent_id, a.runner, a.model, a.effort, a.account_id, a.clean_context,
        )),
        "restart_agent" => call!(supervisor::LaunchAgentArgs, |a| supervisor::restart_agent(
            a.project_path, a.project_name, a.agent_id, a.runner, a.model, a.effort, a.account_id, a.clean_context,
        )),
        "stop_agent" => call!(supervisor::StopAgentArgs, |a| supervisor::stop_agent(a.project_path, a.agent_id)),
        "interrupt_agent" => call!(supervisor::InterruptAgentArgs, |a| supervisor::interrupt_agent(a.project_path, a.agent_id, a.corrective_prompt)),
        "prompt_agent" => call!(supervisor::PromptAgentArgs, |a| supervisor::prompt_agent(a.project_path, a.target, a.prompt)),

        "read_terminal_output" => call!(supervisor::ReadTerminalOutputArgs, |a| supervisor::read_terminal_output(a.project_path, a.pane_id, a.lines_count, a.filter)),
        "send_terminal_input" => call!(supervisor::SendTerminalInputArgs, |a| supervisor::send_terminal_input(a.project_path, a.process_key, a.input)),
        "launch_interactive_shell" => call!(supervisor::LaunchInteractiveShellArgs, |a| supervisor::launch_interactive_shell(a.project_id, a.shell_id)),
        "list_interactive_shells" => call!(supervisor::ListInteractiveShellsArgs, |a| supervisor::list_interactive_shells(a.project_id)),

        "create_agent_persona" => call!(persona::SaveAgentPersonaArgs, |a| persona::create_agent_persona(a.project_id, a.agent)),
        "update_agent_persona" => call!(persona::SaveAgentPersonaArgs, |a| persona::update_agent_persona(a.project_id, a.agent)),
        "delete_agent_persona" => call!(persona::DeleteAgentPersonaArgs, |a| persona::delete_agent_persona(a.project_id, a.agent_id)),

        "initialize_project_agent" => call!(initializer::InitializeProjectAgentArgs, |a| initializer::initialize_project_agent(
            a.project_id, a.mode, a.is_retry, a.runner, a.model, a.effort,
        )),
        "create_greenfield_project" => call!(initializer::CreateGreenfieldProjectArgs, |a| initializer::create_greenfield_project(
            a.name, a.parent_path, a.init_git, a.enable_rea, a.tracking_mode, a.runner, a.model, a.effort,
        )),
        "launch_flag_resolution_agent" => call!(flag_resolver::LaunchFlagResolutionAgentArgs, |a| flag_resolver::launch_flag_resolution_agent(
            a.project_id, a.flag_id, a.chosen_option, a.cleanup_strategy, a.runner, a.model, a.effort,
        )),
        "launch_project_removal_agent" => call!(flag_resolver::LaunchProjectRemovalAgentArgs, |a| flag_resolver::launch_project_removal_agent(
            a.project_id, a.purge_traces, a.failure_report,
        )),

        "check_rea_toolchain" => call!(rea::CheckReaToolchainArgs, |a| rea::check_rea_toolchain(a.project_path)),
        "get_rea_engine_status" => call!(ProjectPathArgs, |a| rea::get_rea_engine_status(a.project_path)),
        "get_rea_queue_snapshot" => call!(NoArgs, |_a| rea::get_rea_queue_snapshot()),
        "start_rea_engine" => call!(ProjectPathArgs, |a| rea::start_rea_engine(a.project_path)),
        "stop_rea_engine" => call!(ProjectPathArgs, |a| rea::stop_rea_engine(a.project_path)),
        "toggle_project_rea_plugin" => call!(rea::ToggleProjectReaPluginArgs, |a| rea::toggle_project_rea_plugin(a.project_path, a.enabled)),
        "list_rea_targets" => call!(ProjectPathArgs, |a| rea::list_rea_targets(a.project_path)),
        "get_rea_visual_views" => call!(ProjectPathArgs, |a| rea::get_rea_visual_views(a.project_path)),

        "list_connected_mcus" => blocking!(NoArgs, |_a| electronics::discovery::scan_connected_microcontrollers()),
        "scan_connected_instruments" => call!(NoArgs, |_a| electronics::instruments::scan_connected_instruments()),
        "list_supported_oscilloscopes" => call!(NoArgs, |_a| electronics::instruments::list_supported_oscilloscopes()),
        "get_project_electronics_status" => call!(electronics::ProjectElectronicsPathArgs, |a| electronics::get_project_electronics_status(a.project_path)),
        "toggle_project_electronics_plugin" => call!(electronics::ToggleProjectElectronicsPluginArgs, |a| electronics::toggle_project_electronics_plugin(a.project_path, a.enabled)),
        "build_project_firmware" => call!(electronics::BuildProjectFirmwareArgs, |a| electronics::firmware::build_project_firmware(&a.project_path, a.environment)),
        "flash_project_firmware" => call!(electronics::FlashProjectFirmwareArgs, |a| electronics::firmware::flash_project_firmware(&a.project_path, a.environment, a.upload_port)),
        "capture_oscilloscope" => call!(electronics::instruments::CaptureOscilloscopeArgs, |a| electronics::instruments::capture_oscilloscope(a)),
        "capture_logic_analyzer" => call!(electronics::instruments::CaptureLogicAnalyzerArgs, |a| electronics::instruments::capture_logic_analyzer(a)),
        "calibrate_oscilloscope" => call!(electronics::instruments::CalibrateOscilloscopeArgs, |a| electronics::instruments::calibrate_oscilloscope(a)),
        "list_platformio_boards" => call!(electronics::firmware::ListPlatformioBoardsArgs, |a| electronics::firmware::list_platformio_boards(a.query)),
        "scaffold_firmware_project" => call!(electronics::firmware::ScaffoldFirmwareProjectArgs, |a| electronics::firmware::scaffold_firmware_project(a)),

        "get_project_desigma_status" => call!(desigma::GetProjectDesigmaStatusArgs, |a| desigma::get_project_desigma_status(a.project_path)),
        "toggle_project_desigma_plugin" => call!(desigma::ToggleProjectDesigmaPluginArgs, |a| desigma::toggle_project_desigma_plugin(a.project_path, a.enabled)),
        "launch_project_desigma" => call!(desigma::LaunchProjectDesigmaArgs, |a| desigma::launch_project_desigma(a.project_path)),
        "render_desigma_preview" => call!(desigma::RenderDesigmaPreviewArgs, |a| desigma::render_desigma_preview(a.project_path, a.file_name, a.node_id)),

        "get_project_solidcraft_status" => call!(solidcraft::GetProjectSolidcraftStatusArgs, |a| solidcraft::get_project_solidcraft_status(a.project_path)),
        "toggle_project_solidcraft_plugin" => call!(solidcraft::ToggleProjectSolidcraftPluginArgs, |a| solidcraft::toggle_project_solidcraft_plugin(a.project_path, a.enabled)),
        "create_solidcraft_file" => call!(solidcraft::CreateSolidcraftFileArgs, |a| solidcraft::create_solidcraft_file(a.project_path, a.file_name, a.template_id)),
        "launch_project_solidcraft" => call!(solidcraft::LaunchProjectSolidcraftArgs, |a| solidcraft::launch_project_solidcraft(a.project_path, a.file_name)),
        "render_solidcraft_preview" => call!(solidcraft::RenderSolidcraftPreviewArgs, |a| solidcraft::render_solidcraft_preview(a.project_path, a.file_name, a.view, a.section)),
        "inspect_solidcraft_metrics" => call!(solidcraft::InspectSolidcraftMetricsArgs, |a| solidcraft::inspect_solidcraft_metrics(a.project_path, a.file_name)),

        "get_project_pcbcraft_status" => call!(pcbcraft::GetProjectPcbcraftStatusArgs, |a| pcbcraft::get_project_pcbcraft_status(a.project_path)),
        "toggle_project_pcbcraft_plugin" => call!(pcbcraft::ToggleProjectPcbcraftPluginArgs, |a| pcbcraft::toggle_project_pcbcraft_plugin(a.project_path, a.enabled)),
        "create_pcbcraft_project" => call!(pcbcraft::CreatePcbcraftProjectArgs, |a| pcbcraft::create_pcbcraft_project(a.project_path, a.target_name)),
        "launch_project_pcbcraft" => call!(pcbcraft::LaunchProjectPcbcraftArgs, |a| pcbcraft::launch_project_pcbcraft(a.project_path, a.target_name)),
        "inspect_pcbcraft_project" => call!(pcbcraft::InspectPcbcraftProjectArgs, |a| pcbcraft::inspect_pcbcraft_project(a.project_path, a.target_name)),
        "render_pcbcraft_svg_preview" => call!(pcbcraft::RenderPcbcraftSvgPreviewArgs, |a| pcbcraft::render_pcbcraft_svg_preview(a.project_path, a.target_name, a.sheet_id, a.mode)),

        "get_active_conversation" => blocking!(conversation::AgentConversationConfigArgs, |a| conversation::get_active_conversation(a)),
        "create_agent_conversation" => blocking!(conversation::CreateAgentConversationArgs, |a| conversation::create_agent_conversation(a)),
        "update_active_conversation_config" => blocking!(conversation::AgentConversationConfigArgs, |a| conversation::update_active_conversation_config(a)),
        "get_agent_turns_continuous" => blocking!(conversation::GetAgentTurnsContinuousArgs, |a| conversation::get_agent_turns_continuous(a)),
        "save_user_chat_turn" => blocking!(conversation::SaveUserChatTurnArgs, |a| conversation::save_user_chat_turn(a)),
        "update_turn_approval" => blocking!(conversation::UpdateTurnApprovalArgs, |a| conversation::update_turn_approval(a)),
        "get_project_session_journals" => blocking!(conversation::GetProjectSessionJournalsArgs, |a| conversation::get_project_session_journals(a)),
        "build_conversation_brief" => blocking!(conversation::ProjectAgentArgs, |a| conversation::build_conversation_brief(a)),
        "get_project_todos" => blocking!(conversation::ProjectStatusFilterArgs, |a| conversation::get_project_todos(a)),
        "create_project_todo" => blocking!(conversation::CreateProjectTodoArgs, |a| conversation::create_project_todo(a)),
        "update_todo_status" => blocking!(conversation::UpdateTodoStatusArgs, |a| conversation::update_todo_status(a)),
        "get_project_technical_debt" => blocking!(conversation::ProjectStatusFilterArgs, |a| conversation::get_project_technical_debt(a)),
        "record_technical_debt" => blocking!(conversation::RecordTechnicalDebtArgs, |a| conversation::record_technical_debt(a)),
        "resolve_technical_debt" => blocking!(conversation::ResolveTechnicalDebtArgs, |a| conversation::resolve_technical_debt(a)),
        "reopen_conversation" => blocking!(conversation::ReopenConversationArgs, |a| conversation::reopen_agent_conversation(a)),
        "search_conversations" => blocking!(conversation::SearchConversationsArgs, |a| conversation::search_conversations(a)),
        "search_knowledge" => call!(semantic::SearchKnowledgeArgs, |a| semantic::search_knowledge(a)),
        "index_project_knowledge" => call!(semantic::IndexProjectArgs, |a| semantic::index_project_knowledge(a.project_id, a.force.unwrap_or(true))),
        "get_indexing_status" => call!(semantic::IndexProjectArgs, |a| semantic::get_indexing_status(a.project_id)),
        "get_project_knowledge_graph" => call!(semantic::IndexProjectArgs, |a| semantic::get_project_knowledge_graph(a.project_id)),
        "rebuild_project_knowledge_graph" => call!(semantic::IndexProjectArgs, |a| semantic::rebuild_project_knowledge_graph(a.project_id)),
        "get_file_line_insights" => call!(semantic::GetFileLineInsightsArgs, |a| semantic::get_file_line_insights(a)),
        "get_file_line_range_content" => call!(semantic::GetFileLineRangeContentArgs, |a| semantic::get_file_line_range_content(a)),
        "get_file_symbols" => call!(semantic::GetFileSymbolsArgs, |a| semantic::get_file_symbols(a)),
        "resolve_symbol_definition" => call!(semantic::ResolveSymbolDefinitionArgs, |a| semantic::resolve_symbol_definition(a)),
        "get_pending_approvals" => blocking!(NoArgs, |_a| crate::mcp::get_pending_approvals()),

        "list_accounts" => blocking!(NoArgs, |_a| accounts::list_accounts()),
        "switch_account" => call!(accounts::SwitchAccountArgs, |a| accounts::switch_account_command(a.account_id, a.relaunch_active)),
        "remove_account" => blocking!(accounts::AccountIdArgs, |a| accounts::remove_account(&a.account_id)),
        "rename_account" => blocking!(accounts::RenameAccountArgs, |a| accounts::rename_account(&a.account_id, &a.label)),
        "initiate_add_account" => call!(NoArgs, |_a| accounts::initiate_add_account()),
        "check_active_agent_sessions" => blocking!(NoArgs, |_a| accounts::check_active_agent_sessions()),
        "get_deepseek_balance" => call!(NoArgs, |_a| accounts::query_deepseek_balance()),
        "save_deepseek_api_key" => blocking!(accounts::SaveDeepseekApiKeyArgs, |a| accounts::save_deepseek_api_key(a)),
        "get_agent_metrics" => blocking!(stats::GetAgentMetricsArgs, |a| stats::get_agent_metrics_command(a)),
        "get_session_metrics" => blocking!(stats::GetSessionMetricsArgs, |a| stats::get_session_metrics(a)),
        "get_account_quotas" => blocking!(NoArgs, |_a| stats::get_account_quotas()),
        "toggle_account_warmup" => blocking!(stats::ToggleAccountWarmupArgs, |a| stats::toggle_account_warmup(a)),
        "kickstart_account_quota" => call!(stats::KickstartAccountQuotaArgs, |a| stats::probe_account_quota(&a.account_id, None)),
        "get_context_threshold" => blocking!(stats::GetContextThresholdArgs, |a| stats::get_context_threshold(a)),

        "get_projects" => blocking!(NoArgs, |_a| registry::get_projects()),
        "add_project" => blocking!(registry::AddProjectArgs, |a| registry::add_project(a.path)),
        "remove_project" => call!(registry::RemoveProjectArgs, |a| registry::remove_project(a.id, a.purge_traces)),
        "toggle_project_favorite" => blocking!(registry::ToggleProjectFavoriteArgs, |a| registry::toggle_project_favorite(a.project_id, a.is_favorite)),
        "toggle_project_archived" => blocking!(registry::ToggleProjectArchivedArgs, |a| registry::toggle_project_archived(a.project_id, a.is_archived)),
        "toggle_project_tab_visibility" => blocking!(registry::ToggleProjectTabVisibilityArgs, |a| registry::toggle_project_tab_visibility(a.project_id, &a.tab, a.visible)),
        "update_project_description" => blocking!(registry::UpdateProjectDescriptionArgs, |a| registry::update_project_description(a.project_id, a.description)),
        "update_project_name" => blocking!(registry::UpdateProjectNameArgs, |a| registry::update_project_name(a.project_id, a.name)),
        "get_project_overview_summary" => call!(ProjectIdArgs, |a| crate::overview::get_project_overview_summary(a.project_id)),
        "save_project_overview_layout" => blocking!(crate::overview::SaveProjectOverviewLayoutArgs, |a| crate::overview::save_project_overview_layout(a)),
        "get_project_init_flags" => blocking!(db::GetProjectInitFlagsArgs, |a| db::db_get_init_flags(a.project_id, a.status.as_deref())),
        "resolve_project_init_flag" => blocking!(db::ResolveProjectInitFlagArgs, |a| db::db_resolve_init_flag(a.id, &a.status, None).map(|_| ())),
        "migrate_project_deterministic" => call!(ProjectPathArgs, |a| migration::migrate_project_deterministic(a.project_path)),
        "rollback_project_init" => call!(ProjectPathArgs, |a| manifest::rollback_project_init(a.project_path)),

        "get_task_executions" => blocking!(db::GetTaskExecutionsArgs, |a| db::db_get_task_executions(a.project_id, a.limit_count)),
        "get_task_report" => blocking!(db::GetTaskReportArgs, |a| db::db_get_task_report(a.execution_id)),
        "get_system_logs" => blocking!(db::GetSystemLogsArgs, |a| db::get_system_logs(a)),
        "get_system_log_sources" => blocking!(NoArgs, |_a| db::get_system_log_sources()),
        "clear_system_logs" => blocking!(NoArgs, |_a| db::clear_system_logs()),
        "write_client_log" => blocking!(logger::WriteClientLogArgs, |a| logger::write_client_log(a.level, a.module, a.message)),

        "get_plugin_schemas" => blocking!(NoArgs, |_a| Ok::<_, ApiCallError>(config::get_plugin_schemas())),
        "get_global_config" => blocking!(NoArgs, |_a| Ok::<_, ApiCallError>(config::get_global_config())),
        "get_scoped_project_config" => blocking!(ProjectPathArgs, |a| Ok::<_, ApiCallError>(config::get_scoped_project_config(a.project_path))),
        "save_global_config" => blocking!(config::SaveGlobalConfigArgs, |a| config::save_global_config(a.config_toml)),
        "set_show_welcome_on_startup" => blocking!(config::SetShowWelcomeOnStartupArgs, |a| config::set_show_welcome_on_startup(a)),
        "save_project_rula_config" => blocking!(config::SaveProjectRulaConfigArgs, |a| config::save_project_rula_config(a.project_path, a.config_toml)),
        "list_agent_runners" => blocking!(NoArgs, |_a| config::list_agent_runners()),

        "get_active_theme" => blocking!(NoArgs, |_a| theme::get_active_theme()),
        "start_dictation" => call!(NoArgs, |_a| dictation::start_dictation()),
        "stop_dictation" => call!(dictation::DictationSessionArgs, |a| dictation::stop_dictation(a)),
        "cancel_dictation" => call!(dictation::DictationSessionArgs, |a| dictation::cancel_dictation(a)),
        "get_dictation_status" => blocking!(NoArgs, |_a| Ok::<_, ApiCallError>(dictation::get_dictation_status())),
        "set_dictation_auto_send" => blocking!(config::SetDictationAutoSendArgs, |a| config::set_dictation_auto_send(a)),
        "get_gemini_api_key_status" => blocking!(NoArgs, |_a| Ok::<_, ApiCallError>(config::get_gemini_api_key_status())),
        "set_gemini_api_key" => call!(config::SetGeminiApiKeyArgs, |a| config::set_gemini_api_key(a)),
        "clear_gemini_api_key" => call!(NoArgs, |_a| config::clear_gemini_api_key()),

        "init_git_repo" => call!(ProjectIdArgs, |a| git::init_git_repo(a.project_id)),
        "get_vcs_actions" => call!(ProjectIdArgs, |a| git::get_vcs_actions(a.project_id)),
        "save_vcs_action" => call!(git::SaveVcsActionArgs, |a| git::save_vcs_action(a.project_id, a.name, a.prompt)),
        "get_project_git_log" => call!(git::GetProjectGitLogArgs, |a| git::get_project_git_log(a.project_id, a.limit_count)),
        "get_git_commit_diff" => call!(git::GetGitCommitDiffArgs, |a| git::get_git_commit_diff(a.project_id, a.commit_hash, a.file_path)),
        "get_commit_files_changed" => call!(git::GetCommitFilesChangedArgs, |a| git::get_commit_files_changed(a.project_id, a.commit_hash)),
        "get_working_tree_diff" => call!(git::GetWorkingTreeDiffArgs, |a| git::get_working_tree_diff(a.project_id, a.file_path)),
        "get_working_tree_files" => call!(ProjectIdArgs, |a| git::get_working_tree_files(a.project_id)),
        "list_git_branches" => call!(ProjectIdArgs, |a| git::list_git_branches(a.project_id)),
        "switch_git_branch" => call!(git::SwitchGitBranchArgs, |a| git::switch_git_branch(a.project_id, a.branch_name, a.is_remote)),
        "create_git_branch" => call!(git::CreateGitBranchArgs, |a| git::create_git_branch(a.project_id, a.branch_name, a.start_point, a.switch)),
        "stage_git_files" => call!(git::StageGitFilesArgs, |a| git::stage_git_files(a.project_id, a.paths)),
        "unstage_git_files" => call!(git::StageGitFilesArgs, |a| git::unstage_git_files(a.project_id, a.paths)),
        "discard_git_files" => call!(git::DiscardGitFilesArgs, |a| git::discard_git_files(a.project_id, a.paths)),
        "commit_git_changes" => call!(git::CommitGitChangesArgs, |a| git::commit_git_changes(a.project_id, a.message)),
        "sync_git_branch" => call!(git::SyncGitBranchArgs, |a| git::sync_git_branch(a.project_id, a.mode)),
        "generate_commit_message" => call!(ProjectIdArgs, |a| git::generate_commit_message(a.project_id)),

        "toggle_project_management_plugin" => call!(project_mgmt::ToggleProjectManagementPluginArgs, |a| project_mgmt::toggle_project_management_plugin(a.project_path, a.enabled)),
        "list_project_milestones" => call!(ProjectPathArgs, |a| project_mgmt::list_project_milestones(a.project_path)),
        "get_project_milestone_detail" => call!(project_mgmt::GetProjectMilestoneDetailArgs, |a| project_mgmt::get_project_milestone_detail(a)),
        "create_project_milestone" => call!(project_mgmt::CreateProjectMilestoneArgs, |a| project_mgmt::create_project_milestone(a)),
        "update_project_milestone" => call!(project_mgmt::UpdateProjectMilestoneArgs, |a| project_mgmt::update_project_milestone(a)),
        "delete_project_milestone" => call!(project_mgmt::DeleteProjectMilestoneArgs, |a| project_mgmt::delete_project_milestone(a)),
        "list_project_stages" => call!(project_mgmt::ListProjectStagesArgs, |a| project_mgmt::list_project_stages(a.project_path)),
        "create_project_stage" => call!(project_mgmt::CreateProjectStageArgs, |a| project_mgmt::create_project_stage(a)),
        "update_project_stage" => call!(project_mgmt::UpdateProjectStageArgs, |a| project_mgmt::update_project_stage(a)),
        "reorder_project_stages" => call!(project_mgmt::ReorderProjectStagesArgs, |a| project_mgmt::reorder_project_stages(a)),
        "delete_project_stage" => call!(project_mgmt::DeleteProjectStageArgs, |a| project_mgmt::delete_project_stage(a)),
        "list_project_tasks" => call!(project_mgmt::ListProjectTasksArgs, |a| project_mgmt::list_project_tasks(a.project_path, a.status_filter)),
        "create_project_task" => call!(project_mgmt::CreateProjectTaskArgs, |a| project_mgmt::create_project_task(a)),
        "update_project_task" => call!(project_mgmt::UpdateProjectTaskArgs, |a| project_mgmt::update_project_task(a)),
        "delete_project_task" => call!(project_mgmt::DeleteProjectTaskArgs, |a| project_mgmt::delete_project_task(a.task_id)),
        "create_task_subtask" => call!(project_mgmt::CreateTaskSubtaskArgs, |a| project_mgmt::create_task_subtask(a)),
        "toggle_task_subtask" => call!(project_mgmt::ToggleTaskSubtaskArgs, |a| project_mgmt::toggle_task_subtask(a.subtask_id, a.is_completed)),
        "delete_task_subtask" => call!(project_mgmt::DeleteTaskSubtaskArgs, |a| project_mgmt::delete_task_subtask(a.subtask_id)),
        "link_task_relation" => call!(project_mgmt::TaskRelationArgs, |a| project_mgmt::link_task_relation(a)),
        "unlink_task_relation" => call!(project_mgmt::TaskRelationArgs, |a| project_mgmt::unlink_task_relation(a)),
        "take_forge_issue" => call!(project_mgmt::TakeForgeIssueArgs, |a| project_mgmt::take_forge_issue(a)),

        "get_forge_context" => call!(forge::GetForgeContextArgs, |a| forge::get_forge_context(a)),
        "configure_upstream_remote" => call!(github::ConfigureUpstreamRemoteArgs, |a| github::configure_upstream_remote(a.project_id, a.remote_url)),
        "sync_upstream_fetch" => call!(ProjectIdArgs, |a| github::sync_upstream_fetch(a.project_id)),
        "list_forge_issues" => call!(forge::ListForgeIssuesArgs, |a| forge::list_forge_issues(a)),
        "get_forge_issue" => call!(forge::GetForgeIssueArgs, |a| forge::get_forge_issue(a)),
        "list_forge_pull_requests" => call!(forge::ListForgePullRequestsArgs, |a| forge::list_forge_pull_requests(a)),
        "get_forge_pull_request" => call!(forge::GetForgePullRequestArgs, |a| forge::get_forge_pull_request(a)),
        "list_forge_releases" => call!(forge::ListForgeReleasesArgs, |a| forge::list_forge_releases(a)),
        "create_forge_draft_pull_request" => call!(forge::CreateForgeDraftPullRequestArgs, |a| forge::create_forge_draft_pull_request(a)),
        "list_forge_inbox" => call!(forge::inbox::ListForgeInboxArgs, |a| forge::inbox::list_forge_inbox(a)),
        "mark_forge_item_read" => call!(forge::inbox::MarkForgeItemReadArgs, |a| forge::inbox::mark_forge_item_read(a)),
        "set_forge_item_flag" => call!(forge::inbox::SetForgeItemFlagArgs, |a| forge::inbox::set_forge_item_flag(a)),
        "get_forge_unread_counts" => call!(forge::inbox::GetForgeUnreadCountsArgs, |a| forge::inbox::get_forge_unread_counts(a)),

        // UI State Machine & Session Persistence (Spec 35)
        "get_ui_registry" => blocking!(NoArgs, |_a| Ok::<_, ApiCallError>(ui_state::get_ui_registry())),
        "get_ui_state" => blocking!(NoArgs, |_a| ui_state::get_ui_state()),
        "dispatch_ui_action" => blocking!(ui_state::UiAction, |a| ui_state::dispatch_ui_action(a)),

        // Global Voice Controller (Spec 36)
        "get_voice_controller_status" => blocking!(NoArgs, |_a| Ok::<_, ApiCallError>(voice_controller::get_voice_controller_status())),
        "set_voice_controller_enabled" => blocking!(config::SetVoiceControllerEnabledArgs, |a| config::set_voice_controller_enabled(a.enabled)),
        "test_voice_controller_voice" => call!(voice_controller::TestVoiceArgs, |a| voice_controller::test_voice_controller_voice(a)),

        "inject_platform_skill" => blocking!(ProjectPathArgs, |a| skill::inject_platform_skill(a.project_path)),
        "get_project_skills" => blocking!(ProjectPathArgs, |a| skill::get_project_skills(a.project_path)),
        "toggle_project_skill" => blocking!(skill::ToggleProjectSkillArgs, |a| skill::toggle_project_skill_command(a)),
        "remove_project_skill" => blocking!(skill::ProjectSkillArgs, |a| skill::remove_project_skill_command(a)),
        "get_project_skill_content" => blocking!(skill::ProjectSkillArgs, |a| skill::get_project_skill_content(a)),
        "search_marketplace_skills" => call!(skill::SearchMarketplaceSkillsArgs, |a| skill::search_marketplace_skills(a)),
        "install_project_skill" => call!(skill::InstallProjectSkillArgs, |a| skill::install_project_skill_command(a)),

        "get_project_file_tree" => call!(ProjectIdArgs, |a| fs_tree::get_project_file_tree(a.project_id)),
        "get_project_documents" => call!(documents::GetProjectDocumentsArgs, |a| documents::get_project_documents(a.project_path, a.scope)),
        "read_project_document" => blocking!(documents::ProjectRelPathArgs, |a| documents::read_project_document(a.project_path, a.rel_path)),
        "save_project_document" => blocking!(documents::SaveProjectDocumentArgs, |a| documents::save_project_document(a.project_path, a.rel_path, a.content)),
        "create_document_comment" => blocking!(documents::CreateDocumentCommentArgs, |a| documents::create_document_comment(a)),
        "list_document_comments" => blocking!(documents::ListDocumentCommentsArgs, |a| documents::list_document_comments(a)),
        "resolve_document_comment" => blocking!(documents::ResolveDocumentCommentArgs, |a| documents::resolve_document_comment(a)),
        "delete_document_comment" => blocking!(documents::DeleteDocumentCommentArgs, |a| documents::delete_document_comment(a.comment_id)),
        "create_project_directory" => blocking!(documents::ProjectRelPathArgs, |a| documents::create_project_directory(a.project_path, a.rel_path)),
        "create_project_file" => blocking!(documents::ProjectRelPathArgs, |a| documents::create_project_file(a.project_path, a.rel_path)),
        "rename_project_item" => blocking!(documents::RenameProjectItemArgs, |a| documents::rename_project_item(a.project_path, a.old_rel_path, a.new_rel_path)),
        "delete_project_items" => blocking!(documents::DeleteProjectItemsArgs, |a| documents::delete_project_items(a.project_path, a.rel_paths)),
        "copy_project_items" => blocking!(documents::TransferProjectItemsArgs, |a| documents::copy_project_items(a.project_path, a.src_rel_paths, a.dest_folder_rel)),
        "move_project_items" => blocking!(documents::TransferProjectItemsArgs, |a| documents::move_project_items(a.project_path, a.src_rel_paths, a.dest_folder_rel)),
        "save_clipboard_image" => blocking!(documents::SaveClipboardImageArgs, |a| documents::save_clipboard_image(a.project_path, a.image_base64, a.extension)),
        "attach_file_to_project" => blocking!(documents::AttachFileToProjectArgs, |a| documents::attach_file_to_project(a.project_path, a.source_absolute_path)),
        "paste_clipboard_image" => blocking!(ProjectPathArgs, |a| documents::paste_clipboard_image(a.project_path)),
        "open_file_in_external_editor" => call!(documents::ProjectRelPathArgs, |a| documents::open_file_in_external_editor(a.project_path, a.rel_path)),
        "get_project_exclusion_status" => call!(documents::GetProjectExclusionStatusArgs, |a| documents::get_project_exclusion_status(a)),
        "toggle_project_exclusion" => call!(documents::ToggleProjectExclusionArgs, |a| documents::toggle_project_exclusion(a)),

        "create_project_snapshot" => call!(snapshots::CreateProjectSnapshotArgs, |a| snapshots::create_project_snapshot(a)),
        "list_project_snapshots" => blocking!(snapshots::ListProjectSnapshotsArgs, |a| snapshots::list_project_snapshots(a)),
        "restore_project_snapshot" => call!(snapshots::RestoreProjectSnapshotArgs, |a| snapshots::restore_project_snapshot(a)),
        "delete_project_snapshot" => call!(snapshots::DeleteProjectSnapshotArgs, |a| snapshots::delete_project_snapshot(a)),
        "get_snapshot_diff" => call!(snapshots::GetSnapshotDiffArgs, |a| snapshots::get_snapshot_diff_command(a)),

        "list_available_profilers" => blocking!(ProjectPathArgs, |a| profiler::list_available_profilers(a.project_path)),
        "toggle_project_profiler_plugin" => call!(profiler::ToggleProjectProfilerPluginArgs, |a| profiler::toggle_project_profiler_plugin(
            a.project_path, a.enabled,
        )),
        "get_target_runs" => blocking!(profiler::GetTargetRunsArgs, |a| profiler::get_target_runs(a)),
        "get_run_profile" => blocking!(profiler::RunIdArgs, |a| profiler::get_run_profile(a)),
        "compare_runs" => blocking!(profiler::CompareRunsArgs, |a| profiler::compare_runs(a)),

        _ => error_response(command, &ApiCallError::UnknownCommand(command.to_string())),
    }
}

/// The body is parsed here rather than by axum's `Json` extractor so a malformed body is also
/// an [`ApiErrorResponse`] (`ApiCallError::InvalidArguments`) instead of an untyped rejection.
async fn call_handler(Path(command): Path<String>, body: Bytes) -> Response {
    crate::logger::log("DEBUG", "api", &format!("call: {} ({} bytes)", command, body.len()));
    match serde_json::from_slice::<Value>(&body) {
        Ok(args) => dispatch(&command, args).await,
        Err(e) => error_response(&command, &ApiCallError::InvalidArguments { command: command.clone(), details: e.to_string() }),
    }
}

/// Routes of the RPC API; merged into the daemon router behind the operator guard.
pub fn api_router() -> Router {
    Router::new().route("/api/v1/call/:command", post(call_handler))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Command keys of `DaemonCommands` in `src/types/daemonApi.ts` (lines `  name: {`).
    fn frontend_commands() -> Vec<String> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/types/daemonApi.ts");
        let source = std::fs::read_to_string(&path).unwrap();
        let start = source.find("export interface DaemonCommands {").expect("DaemonCommands map");
        source[start..]
            .lines()
            .skip(1)
            .take_while(|l| !l.starts_with('}'))
            .filter_map(|l| l.strip_prefix("  ")?.split_once(": {").map(|(name, _)| name.to_string()))
            .filter(|name| name.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
            .collect()
    }

    #[test]
    fn api_commands_match_the_frontend_map() {
        let mut backend: Vec<String> = API_COMMANDS.iter().map(|s| s.to_string()).collect();
        let mut frontend = frontend_commands();
        backend.sort();
        frontend.sort();
        assert_eq!(backend, frontend);
    }

    #[tokio::test]
    async fn unknown_commands_and_bad_arguments_are_typed_failures() {
        let unknown = dispatch("nope", serde_json::json!({})).await;
        assert_eq!(unknown.status(), StatusCode::NOT_FOUND);
        let bad = dispatch("install_missing_tools", serde_json::json!({"toolIds": "git"})).await;
        assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
        let extra = dispatch("run_diagnostics", serde_json::json!({"x": 1})).await;
        assert_eq!(extra.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn domain_validation_errors_map_to_400() {
        let response = dispatch("install_missing_tools", serde_json::json!({"toolIds": []})).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let response = dispatch("create_github_repo", serde_json::json!({"projectId": 1, "name": "-x", "isPrivate": true})).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
