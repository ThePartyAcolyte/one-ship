use std::path::Path;
use std::process::Command;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::api::ApiError;
use crate::kopeng::git::GitError;
use crate::registry::RegistryError;

/// Maximum repository / project name length.
const MAX_REPO_NAME_CHARS_COUNT: usize = 100;

/// Errors of the GitLab remote commands.
#[derive(Debug, thiserror::Error)]
pub enum GitlabError {
    #[error("glab {operation} failed: {details}")]
    Glab { operation: String, details: String },
    #[error("Invalid repository name '{0}': use 1 to 100 letters, digits, '.', '_' or '-', not starting with '-' and not '.' or '..'")]
    InvalidRepoName(String),
    #[error("Unexpected output of glab {operation}: {details}")]
    UnexpectedOutput { operation: String, details: String },
    #[error("Background task failed: {0}")]
    Task(String),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Registry(#[from] RegistryError),
}

impl ApiError for GitlabError {
    fn status(&self) -> StatusCode {
        match self {
            Self::InvalidRepoName(_) => StatusCode::BAD_REQUEST,
            Self::Glab { .. } | Self::UnexpectedOutput { .. } => StatusCode::BAD_GATEWAY,
            Self::Git(_) | Self::Task(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Registry(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::Glab { .. } => "GitlabError::Glab",
            Self::InvalidRepoName(_) => "GitlabError::InvalidRepoName",
            Self::UnexpectedOutput { .. } => "GitlabError::UnexpectedOutput",
            Self::Task(_) => "GitlabError::Task",
            Self::Git(_) => "GitlabError::Git",
            Self::Registry(e) => e.code(),
        }
    }
}

/// A repository of the authenticated `glab` user.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GitlabRepo {
    pub name: String,
    pub name_with_namespace: String,
    pub web_url: String,
    pub is_private: bool,
}

#[derive(Deserialize, Debug)]
struct RawGitlabProject {
    name: String,
    path_with_namespace: String,
    web_url: String,
    visibility: Option<String>,
}

/// Runs `glab <args>` (in `cwd` when given) and returns its stdout; a non-zero exit is `GitlabError::Glab`.
fn glab(cwd: Option<&Path>, args: &[&str]) -> Result<Vec<u8>, GitlabError> {
    let operation = args.iter().take(2).copied().collect::<Vec<_>>().join(" ");
    let mut cmd = Command::new("glab");
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let out = cmd
        .output()
        .map_err(|e| GitlabError::Glab { operation: operation.clone(), details: format!("cannot run glab: {}", e) })?;
    if !out.status.success() {
        return Err(GitlabError::Glab { operation, details: String::from_utf8_lossy(&out.stderr).trim().to_string() });
    }
    Ok(out.stdout)
}

/// Runs `work` on the registered project's directory in a blocking task.
async fn on_project<T: Send + 'static>(
    project_id: usize,
    work: impl FnOnce(&Path) -> Result<T, GitlabError> + Send + 'static,
) -> Result<T, GitlabError> {
    tokio::task::spawn_blocking(move || {
        let project = crate::registry::find_project_by_id(project_id)?;
        work(Path::new(&project.path))
    })
    .await
    .map_err(|e| GitlabError::Task(e.to_string()))?
}

/// A name `glab repo create` takes as a repository name.
pub fn validate_repo_name(name: &str) -> Result<(), GitlabError> {
    let valid_chars = name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    let valid = valid_chars
        && !name.is_empty()
        && name.chars().count() <= MAX_REPO_NAME_CHARS_COUNT
        && !name.starts_with('-')
        && name != "."
        && name != "..";
    if !valid {
        return Err(GitlabError::InvalidRepoName(name.to_string()));
    }
    Ok(())
}

/// Arguments of the daemon command `create_gitlab_repo`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CreateGitlabRepoArgs {
    #[ts(type = "number")]
    pub project_id: usize,
    pub name: String,
    pub is_private: bool,
}

/// Daemon command `search_gitlab_repos`: repositories of the authenticated `glab` user.
pub async fn search_gitlab_repos() -> Result<Vec<GitlabRepo>, GitlabError> {
    let limit_count = crate::config::resolve_effective_usize(None, None, "vcs", "gitlab_repo_list_limit_count");
    crate::logger::log("DEBUG", "gitlab", &format!("search_gitlab_repos: limit_count={}", limit_count));
    let endpoint = format!("projects?membership=true&simple=true&per_page={}", limit_count);
    let stdout = tokio::task::spawn_blocking(move || {
        glab(None, &["api", &endpoint])
    })
    .await
    .map_err(|e| GitlabError::Task(e.to_string()))??;

    let raw: Vec<RawGitlabProject> = serde_json::from_slice(&stdout).map_err(|e| GitlabError::UnexpectedOutput {
        operation: "api projects".to_string(),
        details: e.to_string(),
    })?;

    let repos: Vec<GitlabRepo> = raw
        .into_iter()
        .map(|p| GitlabRepo {
            name: p.name,
            name_with_namespace: p.path_with_namespace,
            web_url: p.web_url,
            is_private: p.visibility.as_deref().unwrap_or("private") != "public",
        })
        .collect();

    crate::logger::log("DEBUG", "gitlab", &format!("search_gitlab_repos: returning {} repositories", repos.len()));
    Ok(repos)
}

/// Daemon command `create_gitlab_repo`: creates the GitLab repository `name` from the project.
pub async fn create_gitlab_repo(project_id: usize, name: String, is_private: bool) -> Result<String, GitlabError> {
    crate::logger::log("DEBUG", "gitlab", &format!("create_gitlab_repo: project_id={}, name={}, is_private={}", project_id, name, is_private));
    validate_repo_name(&name)?;
    on_project(project_id, move |path| {
        crate::kopeng::git::ensure_repository(path)?;
        let visibility = if is_private { "--private" } else { "--public" };
        let stdout = glab(Some(path), &["repo", "create", &name, visibility, "--default-branch=main"])?;
        let output_str = String::from_utf8_lossy(&stdout).trim().to_string();
        let repo_url = output_str
            .lines()
            .find(|l| l.contains("http://") || l.contains("https://"))
            .map(|l| l.trim().to_string())
            .unwrap_or(output_str);

        if repo_url.is_empty() {
            return Err(GitlabError::UnexpectedOutput {
                operation: "repo create".to_string(),
                details: "no repository URL printed".to_string(),
            });
        }
        let action = if crate::kopeng::git::has_origin(path)? { "set-url" } else { "add" };
        crate::kopeng::git::git(path, &["remote", action, "origin", &repo_url])?;
        crate::logger::log("INFO", "gitlab", &format!("Created GitLab repository '{}' ({}) for {}", name, repo_url, path.display()));
        Ok(repo_url)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_names_validation() {
        for name in ["one-ship", "a.b_c", "x", "group-project"] {
            assert!(validate_repo_name(name).is_ok(), "{}", name);
        }
        for name in ["", "-private", "--public", ".", "..", "a b", "a/b", &"a".repeat(101)] {
            assert!(validate_repo_name(name).is_err(), "{}", name);
        }
    }
}
