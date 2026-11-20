use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use crate::db::{DatabaseError, ProjectRecord};
use crate::hydration::{hydrate_project, HydratedProject};

/// Failure of a project registry operation.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("Project path '{path}' is invalid: {reason}")]
    InvalidPath { path: String, reason: String },
    #[error("Project name must not be empty")]
    EmptyName,
    #[error("Project id {0} is not registered")]
    IdNotRegistered(usize),
    #[error("Project '{0}' is not registered")]
    PathNotRegistered(String),
    #[error("Removal of project '{path}' aborted, the project stays registered: {reason}")]
    RemovalAborted { path: String, reason: String },
    #[error("Background task failed: {0}")]
    Task(String),
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

impl crate::api::ApiError for RegistryError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::InvalidPath { .. } | Self::EmptyName => StatusCode::BAD_REQUEST,
            Self::IdNotRegistered(_) | Self::PathNotRegistered(_) => StatusCode::NOT_FOUND,
            Self::RemovalAborted { .. } => StatusCode::CONFLICT,
            Self::Task(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Database(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::InvalidPath { .. } => "RegistryError::InvalidPath",
            Self::EmptyName => "RegistryError::EmptyName",
            Self::IdNotRegistered(_) => "RegistryError::IdNotRegistered",
            Self::PathNotRegistered(_) => "RegistryError::PathNotRegistered",
            Self::RemovalAborted { .. } => "RegistryError::RemovalAborted",
            Self::Task(_) => "RegistryError::Task",
            Self::Database(e) => e.code(),
        }
    }
}

/// Arguments of `add_project`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct AddProjectArgs {
    pub path: String,
}

/// Arguments of `remove_project`. `purge_traces` also deletes the files ONE-SHIP wrote into the
/// project (manifest artifacts, snapshot refs) and reverts its governance block.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct RemoveProjectArgs {
    pub id: usize,
    pub purge_traces: bool,
}

/// Arguments of `toggle_project_favorite`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ToggleProjectFavoriteArgs {
    pub project_id: usize,
    pub is_favorite: bool,
}

/// Arguments of `toggle_project_archived`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ToggleProjectArchivedArgs {
    pub project_id: usize,
    pub is_archived: bool,
}

/// Arguments of `toggle_project_tab_visibility`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ToggleProjectTabVisibilityArgs {
    pub project_id: usize,
    pub tab: String,
    pub visible: bool,
}

/// A registered project as stored in SQLite (`projects` table).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProjectEntry {
    pub id: usize,
    pub path: String,
    pub name: String,
    pub description: Option<String>,
    pub is_favorite: bool,
    pub is_archived: bool,
    pub hidden_tabs: Vec<String>,
}

impl From<ProjectRecord> for ProjectEntry {
    fn from(r: ProjectRecord) -> Self {
        Self {
            id: r.id,
            path: r.path,
            name: r.name,
            description: r.description,
            is_favorite: r.is_favorite,
            is_archived: r.is_archived,
            hidden_tabs: r.hidden_tabs,
        }
    }
}

/// All registered projects, ordered by id. A database failure is an error, never an empty list.
pub fn list_projects() -> Result<Vec<ProjectEntry>, RegistryError> {
    crate::logger::log("DEBUG", "registry", "list_projects: loading registered projects");
    let records = crate::db::db_get_projects()?;
    Ok(records.into_iter().map(ProjectEntry::from).collect())
}

/// The registered project with this id; an unknown id is `IdNotRegistered`.
pub fn find_project_by_id(id: usize) -> Result<ProjectEntry, RegistryError> {
    crate::logger::log("DEBUG", "registry", &format!("find_project_by_id: id={}", id));
    crate::db::db_get_project_by_id(id)?
        .map(ProjectEntry::from)
        .ok_or(RegistryError::IdNotRegistered(id))
}

/// The registered project whose stored path equals `path` exactly; otherwise `PathNotRegistered`.
pub fn find_project_by_path(path: &str) -> Result<ProjectEntry, RegistryError> {
    crate::logger::log("DEBUG", "registry", &format!("find_project_by_path: path={}", path));
    crate::db::db_get_project_by_path(path)?
        .map(ProjectEntry::from)
        .ok_or_else(|| RegistryError::PathNotRegistered(path.to_string()))
}

/// Canonical directory path and name of a project to register.
///
/// The path must be absolute and resolve to an existing directory with a final component.
fn validate_new_project_path(path: &str) -> Result<(PathBuf, String), RegistryError> {
    let invalid = |reason: &str| RegistryError::InvalidPath { path: path.to_string(), reason: reason.to_string() };
    if !Path::new(path).is_absolute() {
        return Err(invalid("not an absolute path"));
    }
    let canonical = std::fs::canonicalize(path).map_err(|e| invalid(&e.to_string()))?;
    if !canonical.is_dir() {
        return Err(invalid("not a directory"));
    }
    let name = canonical
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| invalid("has no directory name"))?;
    Ok((canonical, name))
}

/// Removes a project: exports its knowledge, optionally purges its traces, then deletes its records.
///
/// Every step before the database deletion must succeed; any failure returns `RemovalAborted`
/// and leaves the project registered. Blocking (file and git work); shared by `remove_project`
/// and the removal agent's `complete_project_removal` MCP tool.
pub(crate) fn execute_project_removal(id: usize, purge_traces: bool) -> Result<(), RegistryError> {
    let project = find_project_by_id(id)?;
    let p = Path::new(&project.path);
    let aborted = |reason: String| RegistryError::RemovalAborted { path: project.path.clone(), reason };

    // 1. Export SQLite knowledge (Technical Debt, Tasks/Todos, Session Journal) to Markdown
    crate::kopeng::manifest::export_project_knowledge_to_markdown(p, project.id as i64)
        .map_err(|e| aborted(format!("knowledge export failed: {}", e)))?;

    if purge_traces {
        crate::logger::log("INFO", "registry", &format!("remove_project: purging traces for project '{}' at '{}'", project.name, project.path));
        // 2. Purge disk artifacts registered in manifest & revert AGENTS.md
        crate::kopeng::manifest::purge_oneship_artifacts(p)
            .map_err(|e| aborted(format!("artifact purge failed: {}", e)))?;

        // 3. Prune git snapshot refs if a git repo exists
        if p.join(".git").exists() {
            let removed_count = crate::kopeng::snapshots::delete_project_snapshot_refs(p, id as i64)
                .map_err(|e| aborted(format!("snapshot ref removal failed: {}", e)))?;
            crate::logger::log("INFO", "registry", &format!("remove_project: deleted {} snapshot refs of project id {}", removed_count, id));
        }
    }

    // 4. Delete the project and its dependent rows (including initialization state) in one transaction
    crate::db::db_remove_project(id)?;
    crate::logger::log("INFO", "registry", &format!("remove_project: removed project '{}' (id={})", project.name, id));
    Ok(())
}

/// Daemon command `get_projects`: every registered project, hydrated from its `rula.toml`.
pub fn get_projects() -> Result<Vec<HydratedProject>, RegistryError> {
    crate::logger::log("DEBUG", "registry", "get_projects: fetching hydrated projects");
    let projects: Vec<HydratedProject> = list_projects()?.iter().map(hydrate_project).collect();
    crate::logger::log("DEBUG", "registry", &format!("get_projects: returning {} hydrated projects", projects.len()));
    Ok(projects)
}

/// Daemon command `add_project`: registers an existing directory by its canonical path.
pub fn add_project(path: String) -> Result<HydratedProject, RegistryError> {
    crate::logger::log("DEBUG", "registry", &format!("add_project: adding project with path '{}'", path));
    let (canonical, name) = validate_new_project_path(&path)?;
    let canonical_str = canonical.to_string_lossy().to_string();
    let entry = ProjectEntry::from(crate::db::db_add_project(&canonical_str, &name)?);
    let hydrated = hydrate_project(&entry);
    crate::logger::log("INFO", "registry", &format!("add_project: project '{}' (id={}) registered at '{}'", entry.name, entry.id, entry.path));
    Ok(hydrated)
}

/// Daemon command `remove_project`. A failed removal keeps the project registered
/// (`RemovalAborted`), so the UI can hand the failure to the removal agent.
pub async fn remove_project(id: usize, purge_traces: bool) -> Result<(), RegistryError> {
    crate::logger::log("DEBUG", "registry", &format!("remove_project: invoked for id={}, purge_traces={}", id, purge_traces));
    let result = tokio::task::spawn_blocking(move || execute_project_removal(id, purge_traces))
        .await
        .map_err(|e| RegistryError::Task(e.to_string()))?;
    if let Err(ref e) = result {
        crate::logger::log("ERROR", "registry", &format!("remove_project: id={}: {}", id, e));
    }
    result
}

/// Daemon command `toggle_project_favorite`. Toggles whether a project is pinned as favorite.
pub fn toggle_project_favorite(project_id: usize, is_favorite: bool) -> Result<(), RegistryError> {
    crate::logger::log("DEBUG", "registry", &format!("toggle_project_favorite: id={}, is_favorite={}", project_id, is_favorite));
    crate::db::db_toggle_project_favorite(project_id, is_favorite)?;
    Ok(())
}

/// Daemon command `toggle_project_archived`. Toggles whether a project is archived (hidden from active lists).
pub fn toggle_project_archived(project_id: usize, is_archived: bool) -> Result<(), RegistryError> {
    crate::logger::log("DEBUG", "registry", &format!("toggle_project_archived: id={}, is_archived={}", project_id, is_archived));
    crate::db::db_toggle_project_archived(project_id, is_archived)?;
    Ok(())
}

/// Arguments of `update_project_description`.
#[derive(Deserialize, Clone, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct UpdateProjectDescriptionArgs {
    #[ts(type = "number")]
    pub project_id: usize,
    pub description: Option<String>,
}

/// Arguments of `update_project_name`.
#[derive(Deserialize, Clone, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct UpdateProjectNameArgs {
    #[ts(type = "number")]
    pub project_id: usize,
    pub name: String,
}

/// Daemon command `toggle_project_tab_visibility`. Shows or hides a workspace tab for this project.
pub fn toggle_project_tab_visibility(project_id: usize, tab: &str, visible: bool) -> Result<Vec<String>, RegistryError> {
    crate::logger::log("DEBUG", "registry", &format!("toggle_project_tab_visibility: id={}, tab={}, visible={}", project_id, tab, visible));
    let tabs = crate::db::db_toggle_project_tab_visibility(project_id, tab, visible)?;
    Ok(tabs)
}

/// Daemon command `update_project_description`. Updates or clears a project's short domain description.
pub fn update_project_description(project_id: usize, description: Option<String>) -> Result<Option<String>, RegistryError> {
    crate::logger::log("DEBUG", "registry", &format!("update_project_description: id={}", project_id));
    let updated = crate::db::db_update_project_description(project_id, description.as_deref())?;
    Ok(updated)
}

/// Daemon command `update_project_name`. Updates a project's display title.
pub fn update_project_name(project_id: usize, name: String) -> Result<String, RegistryError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(RegistryError::EmptyName);
    }
    crate::logger::log("DEBUG", "registry", &format!("update_project_name: id={}", project_id));
    let updated = crate::db::db_update_project_name(project_id, trimmed)?;
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_project_path_must_be_an_absolute_named_directory() {
        let dir = std::env::temp_dir().join(format!("oneship_registry_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("file.txt");
        std::fs::write(&file, "x").unwrap();

        let (canonical, name) = validate_new_project_path(dir.to_str().unwrap()).unwrap();
        assert_eq!(canonical, std::fs::canonicalize(&dir).unwrap());
        assert_eq!(name, dir.file_name().unwrap().to_string_lossy());

        assert!(validate_new_project_path("relative/path").is_err());
        assert!(validate_new_project_path("/").is_err());
        assert!(validate_new_project_path(file.to_str().unwrap()).is_err());
        assert!(validate_new_project_path(dir.join("missing").to_str().unwrap()).is_err());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
