//! Unified Capability Core (`Spec 40`).
//!
//! Each domain module (`kopeng/*`, `db`, `semantic`, hardware drivers, etc.) declares its
//! capabilities in one place instead of duplicating schemas, parsing, and ownership checks across
//! `mcp.rs` and `voice_controller.rs`.

pub mod electronics;
pub mod files;
pub mod forge;
pub mod git;
pub mod journal;
pub mod knowledge;
pub mod logs;
pub mod profiler;
pub mod project;
pub mod scripts;
pub mod settings;
pub mod skills;

use std::borrow::Cow;
use std::future::Future;
use std::pin::Pin;

use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::api::ApiError;
use crate::hydration::HydratedProject;
use crate::registry::get_projects;
use crate::ui_state::project_has_capability;

/// Who can see and invoke a declared capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityVisibility {
    Shared,
    McpOnly,
    VoiceOnly,
}

/// Risk classification used to enforce verbal confirmation on destructive voice operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityRisk {
    ReadOnly,
    Mutating,
    ConfirmRequired,
}

/// Execution target for a capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CapabilityTarget {
    Backend,
    FrontendSlot { slot_key: &'static str },
    FrontendCommand { node_path: &'static str },
}

/// Declarative definition of one domain capability action.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityDef {
    pub id: Cow<'static, str>,
    pub mcp_name: Option<Cow<'static, str>>,
    pub description: Cow<'static, str>,
    pub parameters_schema: Value,
    pub visibility: CapabilityVisibility,
    pub risk: CapabilityRisk,
    pub target: CapabilityTarget,
    pub voice_max_items: Option<usize>,
}

/// Caller identity projected into capability execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapabilityCaller {
    Voice,
    Mcp { agent_id: String },
}

impl CapabilityCaller {
    /// Returns whether `visibility` permits this caller to inspect and invoke the capability.
    pub fn allows(&self, visibility: CapabilityVisibility) -> bool {
        match (self, visibility) {
            (_, CapabilityVisibility::Shared) => true,
            (Self::Voice, CapabilityVisibility::VoiceOnly) => true,
            (Self::Mcp { .. }, CapabilityVisibility::McpOnly) => true,
            _ => false,
        }
    }

    /// Label used for audit/event attribution (`"voice_controller"` or the calling agent ID).
    pub fn actor_label(&self) -> &str {
        match self {
            Self::Voice => "voice_controller",
            Self::Mcp { agent_id } => agent_id.as_str(),
        }
    }
}

/// Validated project context passed to capability executors.
pub struct CapabilityContext {
    pub project: HydratedProject,
    pub caller: CapabilityCaller,
}

impl CapabilityContext {
    pub fn project_id(&self) -> i64 {
        self.project.id as i64
    }
}

/// Typed domain errors for the Unified Capability Core.
#[derive(Debug, thiserror::Error)]
pub enum CapabilityError {
    #[error("project_id must be a positive integer, got {0}")]
    InvalidProjectId(i64),

    #[error("Project {0} is not registered")]
    ProjectNotFound(i64),

    #[error("Unknown capability domain '{0}'")]
    UnknownDomain(String),

    #[error("Domain '{domain}' requires capability '{capability}', which is disabled for project {project_id}")]
    CapabilityNotEnabled {
        project_id: i64,
        domain: String,
        capability: String,
    },

    #[error("Unknown action '{action_id}' in domain '{domain}'")]
    UnknownAction { domain: String, action_id: String },

    #[error("Action '{domain}.{action_id}' is not permitted for this caller")]
    ForbiddenForCaller { domain: String, action_id: String },

    #[error("Invalid parameters: {0}")]
    InvalidParams(String),

    #[error("{entity} #{entity_id} does not belong to project {project_id}")]
    OwnershipMismatch {
        entity: &'static str,
        entity_id: i64,
        project_id: i64,
    },

    #[error("{entity} #{id} not found")]
    NotFound { entity: &'static str, id: i64 },

    #[error("Database error: {0}")]
    Database(#[from] crate::db::DatabaseError),

    #[error("Project management error: {0}")]
    ProjectMgmt(#[from] crate::kopeng::project_mgmt::ProjectMgmtError),

    #[error("Registry error: {0}")]
    Registry(#[from] crate::registry::RegistryError),

    #[error("Forge error: {0}")]
    Forge(#[from] crate::kopeng::forge::ForgeError),

    #[error("Configuration error: {0}")]
    Config(#[from] crate::config::ConfigError),

    #[error("Semantic search error: {0}")]
    Semantic(#[from] crate::semantic::SemanticError),

    #[error("UI state error: {0}")]
    UiState(#[from] crate::ui_state::UiStateError),

    #[error("Git error: {0}")]
    Git(#[from] crate::kopeng::git::GitError),

    #[error("Snapshot error: {0}")]
    Snapshot(#[from] crate::kopeng::snapshots::SnapshotError),

    #[error("Task runner error: {0}")]
    Runner(#[from] crate::kopeng::runner::RunnerError),

    #[error("Script error: {0}")]
    Script(#[from] crate::hydration::ScriptError),

    #[error("Service orchestrator error: {0}")]
    Orchestrator(#[from] crate::orchestrator::OrchestratorError),

    #[error("Supervisor error: {0}")]
    Supervisor(#[from] crate::supervisor::SupervisorError),

    #[error("Electronics error: {0}")]
    Electronics(#[from] crate::kopeng::electronics::errors::ElectronicsError),

    #[error("Profiler error: {0}")]
    Profiler(#[from] crate::kopeng::profiler::ProfilerError),

    #[error("Skill error: {0}")]
    Skill(#[from] crate::kopeng::skill::SkillError),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Internal task error: {0}")]
    Task(String),
}

impl ApiError for CapabilityError {
    fn status(&self) -> StatusCode {
        match self {
            Self::InvalidProjectId(_) | Self::InvalidParams(_) => StatusCode::BAD_REQUEST,
            Self::ProjectNotFound(_) | Self::UnknownDomain(_) | Self::UnknownAction { .. } | Self::NotFound { .. } => {
                StatusCode::NOT_FOUND
            }
            Self::ForbiddenForCaller { .. } | Self::OwnershipMismatch { .. } => StatusCode::FORBIDDEN,
            Self::CapabilityNotEnabled { .. } | Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Database(_) | Self::ProjectMgmt(_) | Self::Registry(_) | Self::Task(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            Self::Forge(e) => e.status(),
            Self::Config(e) => e.status(),
            Self::Semantic(e) => e.status(),
            Self::UiState(e) => e.status(),
            Self::Git(e) => e.status(),
            Self::Snapshot(e) => e.status(),
            Self::Runner(e) => e.status(),
            Self::Script(e) => e.status(),
            Self::Orchestrator(e) => e.status(),
            Self::Supervisor(e) => e.status(),
            Self::Electronics(e) => e.status(),
            Self::Profiler(e) => e.status(),
            Self::Skill(e) => e.status(),
            Self::Io(e) => match e.kind() {
                std::io::ErrorKind::NotFound => StatusCode::NOT_FOUND,
                std::io::ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            },
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::InvalidProjectId(_) => "CapabilityError::InvalidProjectId",
            Self::ProjectNotFound(_) => "CapabilityError::ProjectNotFound",
            Self::UnknownDomain(_) => "CapabilityError::UnknownDomain",
            Self::CapabilityNotEnabled { .. } => "CapabilityError::CapabilityNotEnabled",
            Self::UnknownAction { .. } => "CapabilityError::UnknownAction",
            Self::ForbiddenForCaller { .. } => "CapabilityError::ForbiddenForCaller",
            Self::InvalidParams(_) => "CapabilityError::InvalidParams",
            Self::OwnershipMismatch { .. } => "CapabilityError::OwnershipMismatch",
            Self::NotFound { .. } => "CapabilityError::NotFound",
            Self::Conflict(_) => "CapabilityError::Conflict",
            Self::Database(_) => "CapabilityError::Database",
            Self::ProjectMgmt(_) => "CapabilityError::ProjectMgmt",
            Self::Registry(_) => "CapabilityError::Registry",
            Self::Forge(e) => e.code(),
            Self::Config(e) => e.code(),
            Self::Semantic(e) => e.code(),
            Self::UiState(e) => e.code(),
            Self::Git(e) => e.code(),
            Self::Snapshot(e) => e.code(),
            Self::Runner(e) => e.code(),
            Self::Script(e) => e.code(),
            Self::Orchestrator(e) => e.code(),
            Self::Supervisor(e) => e.code(),
            Self::Electronics(e) => e.code(),
            Self::Profiler(e) => e.code(),
            Self::Skill(e) => e.code(),
            Self::Io(_) => "CapabilityError::Io",
            Self::Task(_) => "CapabilityError::Task",
        }
    }
}

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Declarative provider registered by each subsystem or plugin (Spec 40 §3.1).
pub struct CapabilityProvider {
    pub domain_id: &'static str,
    pub label: &'static str,
    pub required_capability: Option<&'static str>,
    pub linked_surface_path: Option<&'static str>,
    pub static_actions: fn() -> Vec<CapabilityDef>,
    pub dynamic_actions: fn(&HydratedProject) -> Vec<CapabilityDef>,
    pub execute: for<'a> fn(
        ctx: CapabilityContext,
        action_id: &'a str,
        params: Value,
    ) -> BoxFuture<'a, Result<Value, CapabilityError>>,
}

/// Compact summary of an enabled capability domain for a project.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainIndexEntry {
    pub domain: &'static str,
    pub label: &'static str,
    pub linked_surface_path: Option<&'static str>,
    pub action_count: usize,
}

static PROVIDERS: &[&CapabilityProvider] = &[
    &project::PROVIDER,
    &journal::PROVIDER,
    &forge::PROVIDER,
    &files::PROVIDER,
    &knowledge::PROVIDER,
    &settings::PROVIDER,
    &git::PROVIDER,
    &scripts::PROVIDER,
    &logs::PROVIDER,
    &electronics::PROVIDER,
    &profiler::PROVIDER,
    &skills::PROVIDER,
];

/// All providers migrated onto the Unified Capability Core.
pub fn all_providers() -> &'static [&'static CapabilityProvider] {
    PROVIDERS
}

/// Looks up a provider by `domain_id`.
pub fn find_provider(domain: &str) -> Option<&'static CapabilityProvider> {
    all_providers().iter().copied().find(|p| p.domain_id == domain)
}

/// Validates `project_id > 0` and resolves the registered [`HydratedProject`].
pub async fn resolve_project(project_id: i64) -> Result<HydratedProject, CapabilityError> {
    if project_id <= 0 {
        return Err(CapabilityError::InvalidProjectId(project_id));
    }
    let pid_usize = usize::try_from(project_id)
        .map_err(|_| CapabilityError::InvalidProjectId(project_id))?;
    tokio::task::spawn_blocking(move || {
        get_projects()?
            .into_iter()
            .find(|p| p.id == pid_usize)
            .ok_or(CapabilityError::ProjectNotFound(project_id))
    })
    .await
    .map_err(|e| CapabilityError::Task(e.to_string()))?
}

/// Verifies that `provider` is enabled for `project` via `required_capability`.
/// UI tab visibility (`hidden_tabs`) only governs `ui_navigate`, not backend domain capabilities.
pub fn ensure_provider_enabled(
    provider: &CapabilityProvider,
    project: &HydratedProject,
) -> Result<(), CapabilityError> {
    let project_id = project.id as i64;
    if let Some(cap) = provider.required_capability {
        if !project_has_capability(project, cap) {
            return Err(CapabilityError::CapabilityNotEnabled {
                project_id,
                domain: provider.domain_id.to_string(),
                capability: cap.to_string(),
            });
        }
    }
    Ok(())
}

/// Collects all actions for `provider` on `project` filtered by `caller` visibility.
pub fn collect_provider_actions(
    provider: &CapabilityProvider,
    project: &HydratedProject,
    caller: &CapabilityCaller,
) -> Vec<CapabilityDef> {
    let mut actions = (provider.static_actions)();
    actions.extend((provider.dynamic_actions)(project));
    actions.retain(|a| caller.allows(a.visibility));
    actions
}

/// Lists enabled domains from the Unified Capability Core for `project` and `caller`.
pub fn list_enabled_domains(
    project: &HydratedProject,
    caller: &CapabilityCaller,
) -> Vec<DomainIndexEntry> {
    let mut out = Vec::new();
    for provider in all_providers() {
        if ensure_provider_enabled(provider, project).is_err() {
            continue;
        }
        let actions = collect_provider_actions(provider, project, caller);
        if actions.is_empty() {
            continue;
        }
        out.push(DomainIndexEntry {
            domain: provider.domain_id,
            label: provider.label,
            linked_surface_path: provider.linked_surface_path,
            action_count: actions.len(),
        });
    }
    out
}

/// Builds the discovery payload for `(project_id, domain)` scoped to `caller`.
///
/// For `CapabilityCaller::Voice`, actions are projected cleanly (`id`, `description`, `risk`,
/// `parameters`) without internal MCP/routing metadata, returning every available action without
/// silent truncation.
pub fn build_discovery_payload(
    project: &HydratedProject,
    provider: &CapabilityProvider,
    caller: &CapabilityCaller,
) -> Result<Value, CapabilityError> {
    let actions = collect_provider_actions(provider, project, caller);
    let projected: Vec<Value> = match caller {
        CapabilityCaller::Voice => actions
            .into_iter()
            .map(|a| {
                json!({
                    "id": a.id,
                    "description": a.description,
                    "risk": a.risk,
                    "parameters": a.parameters_schema,
                })
            })
            .collect(),
        CapabilityCaller::Mcp { .. } => actions
            .into_iter()
            .map(|a| serde_json::to_value(a).map_err(|e| CapabilityError::Task(e.to_string())))
            .collect::<Result<Vec<Value>, _>>()?,
    };
    Ok(json!({
        "project_id": project.id,
        "domain": provider.domain_id,
        "label": provider.label,
        "linked_surface_path": provider.linked_surface_path,
        "action_count": projected.len(),
        "actions": projected,
    }))
}

/// Returns complete capability discovery metadata for `(project_id, domain)`.
pub async fn discover_domain(
    project_id: i64,
    domain: &str,
    caller: &CapabilityCaller,
) -> Result<Value, CapabilityError> {
    let Some(provider) = find_provider(domain) else {
        return Err(CapabilityError::UnknownDomain(domain.to_string()));
    };
    let project = resolve_project(project_id).await?;
    ensure_provider_enabled(provider, &project)?;
    build_discovery_payload(&project, provider, caller)
}

/// Executes a capability `(domain, action_id)` on `project_id` after enforcing enablement,
/// visibility, and project scoping.
pub async fn execute_capability(
    project_id: i64,
    domain: &str,
    action_id: &str,
    params: Value,
    caller: CapabilityCaller,
) -> Result<Value, CapabilityError> {
    let Some(provider) = find_provider(domain) else {
        return Err(CapabilityError::UnknownDomain(domain.to_string()));
    };
    let project = resolve_project(project_id).await?;
    ensure_provider_enabled(provider, &project)?;

    let mut all_actions = (provider.static_actions)();
    all_actions.extend((provider.dynamic_actions)(&project));
    let Some(def) = all_actions.into_iter().find(|a| a.id == action_id) else {
        return Err(CapabilityError::UnknownAction {
            domain: domain.to_string(),
            action_id: action_id.to_string(),
        });
    };
    if !caller.allows(def.visibility) {
        return Err(CapabilityError::ForbiddenForCaller {
            domain: domain.to_string(),
            action_id: action_id.to_string(),
        });
    }

    let ctx = CapabilityContext { project, caller };
    (provider.execute)(ctx, action_id, params).await
}

/// Helper to run a blocking capability closure via `tokio::task::spawn_blocking`.
pub async fn run_blocking<F, T>(f: F) -> Result<T, CapabilityError>
where
    F: FnOnce() -> Result<T, CapabilityError> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| CapabilityError::Task(e.to_string()))?
}

/// Extracts a required positive `i64` parameter from `params`.
pub(crate) fn require_positive_i64(params: &Value, key: &str) -> Result<i64, CapabilityError> {
    match params.get(key).and_then(|v| v.as_i64()) {
        Some(n) if n > 0 => Ok(n),
        Some(_) => Err(CapabilityError::InvalidParams(format!(
            "'{key}' must be a positive integer"
        ))),
        None => Err(CapabilityError::InvalidParams(format!(
            "Missing integer parameter '{key}'"
        ))),
    }
}

/// Extracts a required non-empty trimmed string parameter from `params`.
pub(crate) fn require_non_empty_str(params: &Value, key: &str) -> Result<String, CapabilityError> {
    match params.get(key).and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => Ok(s.trim().to_string()),
        Some(_) => Err(CapabilityError::InvalidParams(format!(
            "'{key}' cannot be empty"
        ))),
        None => Err(CapabilityError::InvalidParams(format!(
            "Missing string parameter '{key}'"
        ))),
    }
}

/// Extracts optional `offset` (default `0`) and `limit` (optional positive integer) from `params`.
pub(crate) fn parse_pagination(params: &Value) -> Result<(usize, Option<usize>), CapabilityError> {
    let offset = match params.get("offset") {
        None | Some(Value::Null) => 0usize,
        Some(v) => v
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| {
                CapabilityError::InvalidParams(
                    "'offset' must be a non-negative integer".to_string(),
                )
            })?,
    };
    let limit = match params.get("limit") {
        None | Some(Value::Null) => None,
        Some(v) => match v.as_u64().and_then(|n| usize::try_from(n).ok()) {
            Some(n) if n > 0 => Some(n),
            _ => {
                return Err(CapabilityError::InvalidParams(
                    "'limit' must be a positive integer".to_string(),
                ));
            }
        },
    };
    Ok((offset, limit))
}

/// Paginates `items` using optional `offset` / `limit` parameters and attaches explicit
/// `total_count`, `offset`, `limit`, and `has_more` metadata without silent truncation.
pub(crate) fn paginate_items<T: Serialize>(
    items: Vec<T>,
    list_key: &str,
    params: &Value,
) -> Result<Value, CapabilityError> {
    let (offset, limit) = parse_pagination(params)?;
    let total_count = items.len();
    let iter = items.into_iter().skip(offset);
    let page: Vec<T> = match limit {
        Some(n) => iter.take(n).collect(),
        None => iter.collect(),
    };
    let has_more = offset.saturating_add(page.len()) < total_count;
    let mut obj = serde_json::Map::with_capacity(5);
    obj.insert(
        list_key.to_string(),
        serde_json::to_value(page).map_err(|e| CapabilityError::Task(e.to_string()))?,
    );
    obj.insert("total_count".to_string(), json!(total_count));
    obj.insert("offset".to_string(), json!(offset));
    obj.insert("limit".to_string(), json!(limit));
    obj.insert("has_more".to_string(), json!(has_more));
    Ok(Value::Object(obj))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_project(pm_enabled: bool, hidden_tabs: Vec<&str>) -> HydratedProject {
        HydratedProject {
            id: 7,
            path: "/tmp/cap-test".to_string(),
            name: "cap-test".to_string(),
            description: None,
            has_git: true,
            git_status: None,
            git_error: None,
            has_config: true,
            scripts: Vec::new(),
            skill_status: crate::kopeng::skill::SkillStatus {
                is_installed: true,
                installed_version: None,
                current_version: "0.2.1".to_string(),
                is_outdated: false,
                is_symlinked: false,
                schema_version: None,
                needs_migration: false,
            },
            init_state: None,
            has_rea_enabled: false,
            has_profiler_enabled: false,
            has_electronics_enabled: false,
            has_desigma_enabled: false,
            has_solidcraft_enabled: false,
            has_pcbcraft_enabled: false,
            has_project_management_enabled: pm_enabled,
            has_knowledge_graph_enabled: true,
            preview_url: None,
            is_favorite: false,
            is_archived: false,
            hidden_tabs: hidden_tabs.into_iter().map(str::to_string).collect(),
        }
    }

    #[test]
    fn caller_visibility_gating_is_strict() {
        let voice = CapabilityCaller::Voice;
        let mcp = CapabilityCaller::Mcp {
            agent_id: "backend".to_string(),
        };
        assert!(voice.allows(CapabilityVisibility::Shared));
        assert!(voice.allows(CapabilityVisibility::VoiceOnly));
        assert!(!voice.allows(CapabilityVisibility::McpOnly));

        assert!(mcp.allows(CapabilityVisibility::Shared));
        assert!(mcp.allows(CapabilityVisibility::McpOnly));
        assert!(!mcp.allows(CapabilityVisibility::VoiceOnly));
    }

    #[test]
    fn domain_enablement_respects_capabilities_independent_of_hidden_tabs() {
        let disabled_pm = fixture_project(false, vec![]);
        let domains = list_enabled_domains(&disabled_pm, &CapabilityCaller::Voice);
        let ids: Vec<&str> = domains.iter().map(|d| d.domain).collect();
        assert!(!ids.contains(&"project"));
        assert!(!ids.contains(&"electronics"));
        assert!(!ids.contains(&"profiler"));
        assert!(ids.contains(&"journal"));
        assert!(ids.contains(&"knowledge"));
        assert!(ids.contains(&"skills"));

        let mut full_project = fixture_project(true, vec!["devlog", "files", "settings"]);
        full_project.has_electronics_enabled = true;
        full_project.has_profiler_enabled = true;
        let domains = list_enabled_domains(&full_project, &CapabilityCaller::Voice);
        let ids: Vec<&str> = domains.iter().map(|d| d.domain).collect();
        assert!(ids.contains(&"project"));
        assert!(ids.contains(&"journal"));
        assert!(ids.contains(&"forge"));
        assert!(ids.contains(&"files"));
        assert!(ids.contains(&"knowledge"));
        assert!(ids.contains(&"settings"));
        assert!(ids.contains(&"git"));
        assert!(ids.contains(&"scripts"));
        assert!(ids.contains(&"logs"));
        assert!(ids.contains(&"electronics"));
        assert!(ids.contains(&"profiler"));
        assert!(ids.contains(&"skills"));
    }

    #[tokio::test]
    async fn zero_and_negative_project_ids_fail_early() {
        let err = execute_capability(0, "journal", "list_todos", json!({}), CapabilityCaller::Voice)
            .await
            .unwrap_err();
        assert_eq!(err.code(), "CapabilityError::InvalidProjectId");

        let err_neg = execute_capability(-5, "project", "list_tasks", json!({}), CapabilityCaller::Voice)
            .await
            .unwrap_err();
        assert_eq!(err_neg.code(), "CapabilityError::InvalidProjectId");
    }

    #[test]
    fn every_provider_discovery_payload_is_complete_and_valid() {
        let mut project = fixture_project(true, vec![]);
        project.has_electronics_enabled = true;
        project.has_profiler_enabled = true;
        let caller = CapabilityCaller::Voice;

        for provider in all_providers() {
            let expected = collect_provider_actions(provider, &project, &caller);
            let payload = build_discovery_payload(&project, provider, &caller)
                .expect("build discovery payload");
            let returned = payload
                .get("actions")
                .and_then(|v| v.as_array())
                .expect("discovery actions array");
            assert_eq!(
                returned.len(),
                expected.len(),
                "Discovery for domain '{}' dropped actions",
                provider.domain_id
            );
            let encoded = serde_json::to_string(&payload).expect("serialize discovery payload");
            assert!(!encoded.is_empty());
        }
    }

    #[test]
    fn paginate_items_returns_unclipped_default_and_exact_pages() {
        let data = vec![10, 20, 30, 40, 50];
        let full = paginate_items(data.clone(), "items", &json!({})).unwrap();
        assert_eq!(full["items"], json!([10, 20, 30, 40, 50]));
        assert_eq!(full["total_count"], 5);
        assert_eq!(full["offset"], 0);
        assert_eq!(full["limit"], Value::Null);
        assert_eq!(full["has_more"], false);

        let page1 = paginate_items(data.clone(), "items", &json!({ "offset": 1, "limit": 2 })).unwrap();
        assert_eq!(page1["items"], json!([20, 30]));
        assert_eq!(page1["total_count"], 5);
        assert_eq!(page1["has_more"], true);

        let page_last = paginate_items(data, "items", &json!({ "offset": 3, "limit": 5 })).unwrap();
        assert_eq!(page_last["items"], json!([40, 50]));
        assert_eq!(page_last["has_more"], false);
    }
}
