use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::{OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::api::ApiError;
use crate::kopeng::git::GitError;
use crate::registry::RegistryError;

/// GitHub's maximum repository name length.
const MAX_REPO_NAME_CHARS_COUNT: usize = 100;

/// Errors of the GitHub remote commands (Spec 11). `gh` failures carry its own stderr, which
/// already names the fix (e.g. `gh auth login`).
#[derive(Debug, thiserror::Error)]
pub enum GithubError {
    #[error("gh {operation} failed: {details}")]
    Gh { operation: String, details: String },
    #[error("Invalid repository name '{0}': use 1 to 100 letters, digits, '.', '_' or '-', not starting with '-' and not '.' or '..'")]
    InvalidRepoName(String),
    #[error("Invalid remote URL '{0}': it must be non-empty, contain no whitespace and not start with '-'")]
    InvalidRemoteUrl(String),
    #[error("Target directory '{0}' already exists")]
    TargetDirectoryExists(String),
    #[error("Invalid target path '{0}': {1}")]
    InvalidTargetPath(String, String),
    #[error("Unexpected output of gh {operation}: {details}")]
    UnexpectedOutput { operation: String, details: String },
    #[error("Background task failed: {0}")]
    Task(String),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Registry(#[from] RegistryError),
}

impl ApiError for GithubError {
    fn status(&self) -> StatusCode {
        match self {
            Self::InvalidRepoName(_) | Self::InvalidRemoteUrl(_) | Self::InvalidTargetPath(_, _) => StatusCode::BAD_REQUEST,
            Self::TargetDirectoryExists(_) => StatusCode::CONFLICT,
            Self::Gh { .. } | Self::UnexpectedOutput { .. } => StatusCode::BAD_GATEWAY,
            Self::Git(_) | Self::Task(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Registry(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::Gh { .. } => "GithubError::Gh",
            Self::InvalidRepoName(_) => "GithubError::InvalidRepoName",
            Self::InvalidRemoteUrl(_) => "GithubError::InvalidRemoteUrl",
            Self::TargetDirectoryExists(_) => "GithubError::TargetDirectoryExists",
            Self::InvalidTargetPath(_, _) => "GithubError::InvalidTargetPath",
            Self::UnexpectedOutput { .. } => "GithubError::UnexpectedOutput",
            Self::Task(_) => "GithubError::Task",
            Self::Git(_) => "GithubError::Git",
            Self::Registry(e) => e.code(),
        }
    }
}

/// A repository of the authenticated `gh` user, as listed by `gh repo list --json`.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GithubRepo {
    pub name: String,
    pub name_with_owner: String,
    pub url: String,
    pub is_private: bool,
    pub viewer_permission: Option<String>,
}

/// Runs `gh <args>` (in `cwd` when given) and returns its stdout; a non-zero exit is `GithubError::Gh`.
fn gh(cwd: Option<&Path>, args: &[&str]) -> Result<Vec<u8>, GithubError> {
    let operation = args.iter().take(2).copied().collect::<Vec<_>>().join(" ");
    let mut cmd = Command::new("gh");
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let out = cmd
        .output()
        .map_err(|e| GithubError::Gh { operation: operation.clone(), details: format!("cannot run gh: {}", e) })?;
    if !out.status.success() {
        return Err(GithubError::Gh { operation, details: String::from_utf8_lossy(&out.stderr).trim().to_string() });
    }
    Ok(out.stdout)
}

/// Runs `work` on the registered project's directory in a blocking task.
async fn on_project<T: Send + 'static>(
    project_id: usize,
    work: impl FnOnce(&Path) -> Result<T, GithubError> + Send + 'static,
) -> Result<T, GithubError> {
    tokio::task::spawn_blocking(move || {
        let project = crate::registry::find_project_by_id(project_id)?;
        work(Path::new(&project.path))
    })
    .await
    .map_err(|e| GithubError::Task(e.to_string()))?
}

/// A name `gh repo create` takes as a repository, never as an option.
fn validate_repo_name(name: &str) -> Result<(), GithubError> {
    let valid_chars = name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    let valid = valid_chars
        && !name.is_empty()
        && name.chars().count() <= MAX_REPO_NAME_CHARS_COUNT
        && !name.starts_with('-')
        && name != "."
        && name != "..";
    if !valid {
        return Err(GithubError::InvalidRepoName(name.to_string()));
    }
    Ok(())
}

/// A URL `git remote` takes as a URL, never as an option.
fn validate_remote_url(url: &str) -> Result<(), GithubError> {
    if url.is_empty() || url.starts_with('-') || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(GithubError::InvalidRemoteUrl(url.to_string()));
    }
    Ok(())
}

/// Arguments of the daemon command `link_project_remote`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct LinkProjectRemoteArgs {
    #[ts(type = "number")]
    pub project_id: usize,
    pub remote_url: String,
}

/// Arguments of the daemon command `create_github_repo`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CreateGithubRepoArgs {
    #[ts(type = "number")]
    pub project_id: usize,
    pub name: String,
    pub is_private: bool,
}

/// Arguments of the daemon command `check_git_remote_access`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CheckGitRemoteAccessArgs {
    pub repo_url: String,
}

/// Result of checking access to a remote git repository.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GitRemoteAccessCheck {
    pub accessible: bool,
    pub is_private: Option<bool>,
    pub repo_name: Option<String>,
    pub default_branch: Option<String>,
    pub viewer_permission: Option<String>,
    pub error_message: Option<String>,
}

/// Arguments of the daemon command `clone_and_add_project`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CloneAndAddProjectArgs {
    pub repo_url: String,
    pub parent_path: String,
    pub name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhRepoViewPayload {
    name: String,
    is_private: bool,
    viewer_permission: Option<String>,
    default_branch_ref: Option<GhBranchRefPayload>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhBranchRefPayload {
    name: String,
}

/// Extracts a repository directory name from a Git URL.
pub fn extract_repo_name(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches('/');
    let no_git = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    let last = no_git.rsplit(|c| c == '/' || c == ':').next()?;
    if last.is_empty() || last.starts_with('-') || last == "." || last == ".." {
        None
    } else {
        Some(last.to_string())
    }
}

/// Daemon command `search_github_repos`: repositories of the authenticated `gh` user, up to
/// `vcs.github_repo_list_limit_count`. The UI filters them locally.
pub async fn search_github_repos() -> Result<Vec<GithubRepo>, GithubError> {
    let limit_count = crate::config::resolve_effective_usize(None, None, "vcs", "github_repo_list_limit_count");
    crate::logger::log("DEBUG", "github", &format!("search_github_repos: limit_count={}", limit_count));
    let stdout = tokio::task::spawn_blocking(move || {
        gh(None, &["repo", "list", "--json", "name,nameWithOwner,url,isPrivate,viewerPermission", "--limit", &limit_count.to_string()])
    })
    .await
    .map_err(|e| GithubError::Task(e.to_string()))??;

    let repos: Vec<GithubRepo> = serde_json::from_slice(&stdout).map_err(|e| GithubError::UnexpectedOutput {
        operation: "repo list".to_string(),
        details: e.to_string(),
    })?;
    crate::logger::log("DEBUG", "github", &format!("search_github_repos: returning {} repositories", repos.len()));
    Ok(repos)
}

/// Daemon command `link_project_remote`: sets the project's `origin` to `remote_url`,
/// initializing git first when needed.
pub async fn link_project_remote(project_id: usize, remote_url: String) -> Result<(), GithubError> {
    crate::logger::log("DEBUG", "github", &format!("link_project_remote: project_id={}, remote_url={}", project_id, remote_url));
    validate_remote_url(&remote_url)?;
    on_project(project_id, move |path| {
        crate::kopeng::git::ensure_repository(path)?;
        let action = if crate::kopeng::git::has_origin(path)? { "set-url" } else { "add" };
        crate::kopeng::git::git(path, &["remote", action, "origin", &remote_url])?;
        crate::logger::log("INFO", "github", &format!("Configured remote origin to {} in {}", remote_url, path.display()));
        Ok(())
    })
    .await
}

/// Daemon command `create_github_repo`: creates the GitHub repository `name` from the project
/// (`gh repo create --source=. --remote=origin`), initializing git first when needed. Returns
/// the new repository's URL.
pub async fn create_github_repo(project_id: usize, name: String, is_private: bool) -> Result<String, GithubError> {
    crate::logger::log("DEBUG", "github", &format!("create_github_repo: project_id={}, name={}, is_private={}", project_id, name, is_private));
    validate_repo_name(&name)?;
    on_project(project_id, move |path| {
        crate::kopeng::git::ensure_repository(path)?;
        let visibility = if is_private { "--private" } else { "--public" };
        let stdout = gh(Some(path), &["repo", "create", &name, visibility, "--source=.", "--remote=origin"])?;
        let repo_url = String::from_utf8_lossy(&stdout).trim().to_string();
        if repo_url.is_empty() {
            return Err(GithubError::UnexpectedOutput {
                operation: "repo create".to_string(),
                details: "no repository URL printed".to_string(),
            });
        }
        crate::logger::log("INFO", "github", &format!("Created GitHub repository '{}' ({}) for {}", name, repo_url, path.display()));
        Ok(repo_url)
    })
    .await
}

/// Daemon command `check_git_remote_access`: tests connectivity and access permissions to a remote
/// Git or GitHub repository. Non-blocking with `GIT_TERMINAL_PROMPT=0`.
pub async fn check_git_remote_access(repo_url: String) -> Result<GitRemoteAccessCheck, GithubError> {
    crate::logger::log("DEBUG", "github", &format!("check_git_remote_access: repo_url={}", repo_url));
    if let Err(e) = validate_remote_url(&repo_url) {
        return Ok(GitRemoteAccessCheck {
            accessible: false,
            is_private: None,
            repo_name: None,
            default_branch: None,
            viewer_permission: None,
            error_message: Some(e.to_string()),
        });
    }

    let default_name = extract_repo_name(&repo_url);
    let url_clone = repo_url.clone();

    let check = tokio::task::spawn_blocking(move || {
        // 1. If it looks like a GitHub URL, try `gh repo view` first for rich metadata
        if url_clone.contains("github.com") {
            if let Ok(stdout) = gh(None, &["repo", "view", &url_clone, "--json", "name,nameWithOwner,isPrivate,viewerPermission,defaultBranchRef"]) {
                if let Ok(view) = serde_json::from_slice::<GhRepoViewPayload>(&stdout) {
                    return GitRemoteAccessCheck {
                        accessible: true,
                        is_private: Some(view.is_private),
                        repo_name: Some(view.name),
                        default_branch: view.default_branch_ref.map(|b| b.name),
                        viewer_permission: view.viewer_permission,
                        error_message: None,
                    };
                }
            }
        }

        // 2. Standard universal git ls-remote check (works with public, private SSH keys, credential helpers, etc.)
        let mut cmd = Command::new("git");
        cmd.env("GIT_TERMINAL_PROMPT", "0");
        cmd.args(["ls-remote", "--exit-code", &url_clone, "HEAD"]);

        match cmd.output() {
            Ok(output) if output.status.success() => GitRemoteAccessCheck {
                accessible: true,
                is_private: None,
                repo_name: default_name,
                default_branch: None,
                viewer_permission: None,
                error_message: None,
            },
            Ok(output) => {
                let err_str = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let clean_err = if err_str.contains("terminal prompts disabled") {
                    "Authentication required or repository does not exist. Verify the URL or ensure credentials/SSH keys are configured.".to_string()
                } else if err_str.is_empty() {
                    "Repository not accessible or does not exist.".to_string()
                } else {
                    err_str
                };
                GitRemoteAccessCheck {
                    accessible: false,
                    is_private: None,
                    repo_name: default_name,
                    default_branch: None,
                    viewer_permission: None,
                    error_message: Some(clean_err),
                }
            }
            Err(e) => GitRemoteAccessCheck {
                accessible: false,
                is_private: None,
                repo_name: default_name,
                default_branch: None,
                viewer_permission: None,
                error_message: Some(format!("Failed to execute git: {}", e)),
            },
        }
    })
    .await
    .map_err(|e| GithubError::Task(e.to_string()))?;

    Ok(check)
}

/// Daemon command `clone_and_add_project`: clones a remote Git/GitHub repository into
/// `parent_path/name`, registers it as a project in SQLite, and returns its hydrated record.
pub async fn clone_and_add_project(
    repo_url: String,
    parent_path: String,
    name: String,
) -> Result<crate::hydration::HydratedProject, GithubError> {
    crate::logger::log(
        "INFO",
        "github",
        &format!("clone_and_add_project: url='{}', parent='{}', name='{}'", repo_url, parent_path, name),
    );
    validate_remote_url(&repo_url)?;
    validate_repo_name(&name)?;

    let parent = std::path::PathBuf::from(&parent_path);
    if !parent.is_absolute() {
        return Err(GithubError::InvalidTargetPath(parent_path, "parent path must be absolute".to_string()));
    }
    if parent.exists() && !parent.is_dir() {
        return Err(GithubError::InvalidTargetPath(parent_path, "parent path exists and is not a directory".to_string()));
    }
    if !parent.exists() {
        tokio::fs::create_dir_all(&parent)
            .await
            .map_err(|e| GithubError::InvalidTargetPath(parent_path.clone(), format!("cannot create parent directory: {e}")))?;
    }

    let target_dir = parent.join(&name);
    if target_dir.exists() {
        return Err(GithubError::TargetDirectoryExists(target_dir.display().to_string()));
    }

    let target_dir_str = target_dir.to_string_lossy().to_string();
    let url_clone = repo_url.clone();
    let target_dir_for_spawn = target_dir.clone();

    tokio::task::spawn_blocking(move || {
        let mut cmd = Command::new("git");
        cmd.env("GIT_TERMINAL_PROMPT", "0");
        cmd.args(["clone", &url_clone, &target_dir_str]);
        let output = cmd.output().map_err(|e| GithubError::Git(GitError::Command {
            operation: "clone".to_string(),
            details: format!("cannot execute git clone: {}", e),
        }))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if target_dir_for_spawn.exists() {
                let _ = std::fs::remove_dir_all(&target_dir_for_spawn);
            }
            return Err(GithubError::Git(GitError::Command {
                operation: "clone".to_string(),
                details: stderr,
            }));
        }
        Ok(())
    })
    .await
    .map_err(|e| GithubError::Task(e.to_string()))??;

    let hydrated = crate::registry::add_project(target_dir.to_string_lossy().to_string())?;
    crate::logger::log(
        "INFO",
        "github",
        &format!("clone_and_add_project: successfully cloned and registered project '{}' (id={})", hydrated.name, hydrated.id),
    );
    Ok(hydrated)
}

// ============================================================================
// Open Source Collaboration: Fork/Upstream, Issues, PRs, Releases (Spec 34)
// ============================================================================

const MAX_GH_LIST_ITEMS_COUNT: usize = 50;

#[derive(Clone)]
struct CachedJsonEntry {
    fetched_at_seconds: i64,
    payload: serde_json::Value,
}

static GH_COLLAB_CACHE: OnceLock<RwLock<HashMap<String, CachedJsonEntry>>> = OnceLock::new();

fn gh_cache() -> &'static RwLock<HashMap<String, CachedJsonEntry>> {
    GH_COLLAB_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn now_epoch_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn read_cached_json<T: for<'de> Deserialize<'de>>(key: &str, ttl_seconds: i64, force_refresh: bool) -> Option<T> {
    if force_refresh || ttl_seconds <= 0 {
        return None;
    }
    let guard = match gh_cache().read() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    let entry = guard.get(key)?;
    if now_epoch_seconds().saturating_sub(entry.fetched_at_seconds) > ttl_seconds {
        return None;
    }
    serde_json::from_value(entry.payload.clone()).ok()
}

fn write_cached_json<T: Serialize>(key: String, value: &T) {
    const MAX_CACHE_ENTRIES_COUNT: usize = 200;
    let Ok(json) = serde_json::to_value(value) else {
        return;
    };
    let mut guard = match gh_cache().write() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    if guard.len() >= MAX_CACHE_ENTRIES_COUNT {
        guard.clear();
    }
    guard.insert(
        key,
        CachedJsonEntry {
            fetched_at_seconds: now_epoch_seconds(),
            payload: json,
        },
    );
}

fn invalidate_project_cache(project_id: usize) {
    let prefix = format!("proj:{}:", project_id);
    let mut guard = match gh_cache().write() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.retain(|k, _| !k.starts_with(&prefix));
}

fn cache_ttl_seconds(project_path: &str) -> i64 {
    crate::config::resolve_effective_usize(
        Some(project_path),
        None,
        "project_management",
        "github_cache_ttl_seconds",
    ) as i64
}

/// Extracts `owner/repo` from a GitHub HTTPS or SSH remote URL.
pub fn extract_github_nwo(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches('/');
    let without_git = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    let path_part = if let Some(rest) = without_git.strip_prefix("git@github.com:") {
        rest
    } else if let Some(idx) = without_git.find("github.com/") {
        &without_git[idx + "github.com/".len()..]
    } else {
        return None;
    };
    let mut segs = path_part.split('/').filter(|s| !s.is_empty());
    let owner = segs.next()?;
    let repo = segs.next()?;
    if segs.next().is_some() {
        return None;
    }
    if owner.starts_with('-') || repo.starts_with('-') {
        return None;
    }
    Some(format!("{}/{}", owner, repo))
}

fn validate_nwo(nwo: &str) -> Result<String, GithubError> {
    let trimmed = nwo.trim();
    let mut parts = trimmed.split('/');
    let (Some(owner), Some(repo), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(GithubError::InvalidRepoName(nwo.to_string()));
    };
    validate_repo_name(owner)?;
    validate_repo_name(repo)?;
    Ok(format!("{}/{}", owner, repo))
}

fn validate_ref_arg(name: &str) -> Result<String, GithubError> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.starts_with('-') || trimmed.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(GithubError::InvalidRepoName(name.to_string()));
    }
    Ok(trimmed.to_string())
}

fn git_remote_url(path: &Path, remote_name: &str) -> Option<String> {
    let out = Command::new("git")
        .args(["config", "--get", &format!("remote.{}.url", remote_name)])
        .current_dir(path)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let val = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!val.is_empty()).then_some(val)
}

fn git_rev_list_counts(path: &Path, left_ref: &str, right_ref: &str) -> (usize, usize) {
    let range = format!("{}...{}", left_ref, right_ref);
    let Ok(out) = Command::new("git")
        .args(["rev-list", "--left-right", "--count", &range])
        .current_dir(path)
        .output()
    else {
        return (0, 0);
    };
    if !out.status.success() {
        return (0, 0);
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut nums = text.split_whitespace().filter_map(|s| s.parse::<usize>().ok());
    let ahead = nums.next().unwrap_or(0);
    let behind = nums.next().unwrap_or(0);
    (ahead, behind)
}

/// Topology and fork/upstream status of a project's GitHub remotes.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GithubCollabContext {
    pub has_origin: bool,
    pub origin_url: Option<String>,
    pub origin_repo: Option<String>,
    pub has_upstream: bool,
    pub upstream_url: Option<String>,
    pub upstream_repo: Option<String>,
    pub is_fork: bool,
    pub parent_repo: Option<String>,
    pub parent_url: Option<String>,
    /// Effective collaboration repository (`upstream` > `parent` > `origin`).
    pub target_repo: Option<String>,
    pub default_branch: Option<String>,
    pub ahead_upstream_count: usize,
    pub behind_upstream_count: usize,
    pub viewer_permission: Option<String>,
    pub gh_error: Option<String>,
}

/// Arguments for `configure_upstream_remote`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ConfigureUpstreamRemoteArgs {
    #[ts(type = "number")]
    pub project_id: usize,
    pub remote_url: String,
}

/// Summary of a GitHub issue.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GithubIssueSummary {
    #[ts(type = "number")]
    pub number: i64,
    pub title: String,
    pub state: String,
    pub author: String,
    pub labels: Vec<String>,
    pub comments_count: usize,
    pub is_assigned: bool,
    pub url: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Comment inside an issue or PR.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GithubCommentItem {
    pub author: String,
    pub body: String,
    pub created_at: String,
}

/// Full GitHub issue detail including body and comments.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GithubIssueDetail {
    #[ts(type = "number")]
    pub number: i64,
    pub title: String,
    pub state: String,
    pub author: String,
    pub labels: Vec<String>,
    pub url: String,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
    pub comments: Vec<GithubCommentItem>,
}

/// Summary of a GitHub pull request.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GithubPrSummary {
    #[ts(type = "number")]
    pub number: i64,
    pub title: String,
    pub state: String,
    pub is_draft: bool,
    pub author: String,
    pub head_ref_name: String,
    pub base_ref_name: String,
    pub mergeable: String,
    pub checks_summary: String,
    pub url: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Review entry on a pull request.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GithubReviewItem {
    pub author: String,
    pub state: String,
    pub body: String,
    pub submitted_at: String,
}

/// Status check entry on a pull request.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GithubCheckItem {
    pub name: String,
    pub status: String,
    pub conclusion: String,
}

/// Full GitHub pull request detail including diff metrics, reviews, checks, and comments.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GithubPrDetail {
    #[ts(type = "number")]
    pub number: i64,
    pub title: String,
    pub state: String,
    pub is_draft: bool,
    pub author: String,
    pub head_ref_name: String,
    pub base_ref_name: String,
    pub mergeable: String,
    pub url: String,
    pub body: String,
    pub changed_files_count: usize,
    pub additions_count: usize,
    pub deletions_count: usize,
    pub comments: Vec<GithubCommentItem>,
    pub reviews: Vec<GithubReviewItem>,
    pub checks: Vec<GithubCheckItem>,
}

/// Summary of a GitHub release.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GithubReleaseSummary {
    pub tag_name: String,
    pub name: String,
    pub is_draft: bool,
    pub is_prerelease: bool,
    pub published_at: String,
    pub url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhRepoParentPayload {
    name_with_owner: String,
    url: String,
    default_branch_ref: Option<GhBranchRefPayload>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhCollabRepoViewPayload {
    is_fork: bool,
    parent: Option<GhRepoParentPayload>,
    default_branch_ref: Option<GhBranchRefPayload>,
    viewer_permission: Option<String>,
}

#[derive(Deserialize)]
struct GhAuthorField {
    login: Option<String>,
}

#[derive(Deserialize)]
struct GhLabelField {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhCommentField {
    author: Option<GhAuthorField>,
    body: Option<String>,
    created_at: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhIssueListRow {
    number: i64,
    title: String,
    state: String,
    author: Option<GhAuthorField>,
    #[serde(default)]
    labels: Vec<GhLabelField>,
    #[serde(default)]
    comments: Vec<serde_json::Value>,
    #[serde(default)]
    assignees: Vec<serde_json::Value>,
    url: String,
    created_at: String,
    updated_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhIssueDetailRow {
    number: i64,
    title: String,
    state: String,
    author: Option<GhAuthorField>,
    #[serde(default)]
    labels: Vec<GhLabelField>,
    url: String,
    body: Option<String>,
    created_at: String,
    updated_at: String,
    #[serde(default)]
    comments: Vec<GhCommentField>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhCheckRollupRow {
    name: Option<String>,
    context: Option<String>,
    status: Option<String>,
    state: Option<String>,
    conclusion: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhReviewRow {
    author: Option<GhAuthorField>,
    state: Option<String>,
    body: Option<String>,
    submitted_at: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhPrListRow {
    number: i64,
    title: String,
    state: String,
    is_draft: bool,
    author: Option<GhAuthorField>,
    head_ref_name: String,
    base_ref_name: String,
    mergeable: Option<String>,
    #[serde(default)]
    status_check_rollup: Vec<GhCheckRollupRow>,
    url: String,
    created_at: String,
    updated_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhPrDetailRow {
    number: i64,
    title: String,
    state: String,
    is_draft: bool,
    author: Option<GhAuthorField>,
    head_ref_name: String,
    base_ref_name: String,
    mergeable: Option<String>,
    url: String,
    body: Option<String>,
    changed_files: Option<usize>,
    additions: Option<usize>,
    deletions: Option<usize>,
    #[serde(default)]
    comments: Vec<GhCommentField>,
    #[serde(default)]
    reviews: Vec<GhReviewRow>,
    #[serde(default)]
    status_check_rollup: Vec<GhCheckRollupRow>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhReleaseRow {
    tag_name: String,
    name: Option<String>,
    is_draft: bool,
    is_prerelease: bool,
    published_at: Option<String>,
    url: String,
}

fn summarize_checks(rollup: &[GhCheckRollupRow]) -> String {
    if rollup.is_empty() {
        return "none".to_string();
    }
    let mut has_pending = false;
    for item in rollup {
        let conclusion = item.conclusion.as_deref().unwrap_or("").to_uppercase();
        let state = item.state.as_deref().unwrap_or("").to_uppercase();
        if matches!(conclusion.as_str(), "FAILURE" | "TIMED_OUT" | "ACTION_REQUIRED" | "STARTUP_FAILURE")
            || matches!(state.as_str(), "FAILURE" | "ERROR")
        {
            return "failing".to_string();
        }
        let status = item.status.as_deref().unwrap_or("").to_uppercase();
        if matches!(status.as_str(), "IN_PROGRESS" | "QUEUED" | "PENDING" | "WAITING")
            || matches!(state.as_str(), "PENDING" | "EXPECTED")
        {
            has_pending = true;
        }
    }
    if has_pending {
        "pending".to_string()
    } else {
        "passing".to_string()
    }
}

/// Resolves fork/upstream topology synchronously for a project path.
pub fn resolve_collab_context_blocking(project_id: usize, path: &Path, project_path_str: &str, force_refresh: bool) -> GithubCollabContext {
    let ttl = cache_ttl_seconds(project_path_str);
    let cache_key = format!("proj:{}:ctx", project_id);
    if let Some(cached) = read_cached_json::<GithubCollabContext>(&cache_key, ttl, force_refresh) {
        return cached;
    }

    let origin_url = git_remote_url(path, "origin");
    let origin_repo = origin_url.as_deref().and_then(extract_github_nwo);
    let upstream_url = git_remote_url(path, "upstream");
    let upstream_repo = upstream_url.as_deref().and_then(extract_github_nwo);

    let mut is_fork = false;
    let mut parent_repo = None;
    let mut parent_url = None;
    let mut default_branch = None;
    let mut viewer_permission = None;
    let mut gh_error = None;

    if let Some(ref origin_nwo) = origin_repo {
        match gh(
            Some(path),
            &[
                "repo",
                "view",
                origin_nwo,
                "--json",
                "isFork,parent,defaultBranchRef,viewerPermission",
            ],
        ) {
            Ok(stdout) => {
                if let Ok(view) = serde_json::from_slice::<GhCollabRepoViewPayload>(&stdout) {
                    is_fork = view.is_fork;
                    viewer_permission = view.viewer_permission;
                    default_branch = view.default_branch_ref.map(|b| b.name);
                    if let Some(p) = view.parent {
                        parent_repo = Some(p.name_with_owner);
                        parent_url = Some(p.url);
                        if default_branch.is_none() {
                            default_branch = p.default_branch_ref.map(|b| b.name);
                        }
                    }
                }
            }
            Err(e) => {
                gh_error = Some(e.to_string());
            }
        }
    }

    let target_repo = upstream_repo
        .clone()
        .or_else(|| parent_repo.clone())
        .or_else(|| origin_repo.clone());

    let (ahead_upstream_count, behind_upstream_count) = if upstream_url.is_some() {
        let base_branch = default_branch.as_deref().unwrap_or("main");
        let upstream_ref = format!("upstream/{}", base_branch);
        git_rev_list_counts(path, "HEAD", &upstream_ref)
    } else {
        (0, 0)
    };

    let ctx = GithubCollabContext {
        has_origin: origin_url.is_some(),
        origin_url,
        origin_repo,
        has_upstream: upstream_url.is_some(),
        upstream_url,
        upstream_repo,
        is_fork,
        parent_repo,
        parent_url,
        target_repo,
        default_branch,
        ahead_upstream_count,
        behind_upstream_count,
        viewer_permission,
        gh_error,
    };
    write_cached_json(cache_key, &ctx);
    ctx
}

/// Configures or updates the `upstream` git remote and runs `git fetch upstream`.
pub async fn configure_upstream_remote(
    project_id: usize,
    remote_url: String,
) -> Result<GithubCollabContext, GithubError> {
    validate_remote_url(&remote_url)?;
    let ctx = on_project(project_id, move |path| {
        crate::kopeng::git::require_repository(path)?;
        let action = if git_remote_url(path, "upstream").is_some() {
            "set-url"
        } else {
            "add"
        };
        crate::kopeng::git::git(path, &["remote", action, "upstream", &remote_url])?;
        let mut fetch_cmd = Command::new("git");
        fetch_cmd.env("GIT_TERMINAL_PROMPT", "0");
        fetch_cmd.args(["fetch", "upstream"]);
        fetch_cmd.current_dir(path);
        let out = fetch_cmd.output().map_err(|e| GithubError::Git(GitError::Command {
            operation: "fetch upstream".to_string(),
            details: format!("cannot run git: {}", e),
        }))?;
        if !out.status.success() {
            return Err(GithubError::Git(GitError::Command {
                operation: "fetch upstream".to_string(),
                details: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            }));
        }
        invalidate_project_cache(project_id);
        let proj_str = path.to_string_lossy().to_string();
        Ok(resolve_collab_context_blocking(project_id, path, &proj_str, true))
    })
    .await?;
    crate::db::notify_projects_updated();
    Ok(ctx)
}

/// Runs `git fetch upstream` and returns updated collaboration topology.
pub async fn sync_upstream_fetch(project_id: usize) -> Result<GithubCollabContext, GithubError> {
    let ctx = on_project(project_id, move |path| {
        crate::kopeng::git::require_repository(path)?;
        if git_remote_url(path, "upstream").is_none() {
            return Err(GithubError::Gh {
                operation: "fetch upstream".to_string(),
                details: "Remote 'upstream' is not configured for this project.".to_string(),
            });
        }
        let mut fetch_cmd = Command::new("git");
        fetch_cmd.env("GIT_TERMINAL_PROMPT", "0");
        fetch_cmd.args(["fetch", "upstream"]);
        fetch_cmd.current_dir(path);
        let out = fetch_cmd.output().map_err(|e| GithubError::Git(GitError::Command {
            operation: "fetch upstream".to_string(),
            details: format!("cannot run git: {}", e),
        }))?;
        if !out.status.success() {
            return Err(GithubError::Git(GitError::Command {
                operation: "fetch upstream".to_string(),
                details: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            }));
        }
        invalidate_project_cache(project_id);
        let proj_str = path.to_string_lossy().to_string();
        Ok(resolve_collab_context_blocking(project_id, path, &proj_str, true))
    })
    .await?;
    Ok(ctx)
}

/// Lists GitHub issues from the project's effective target repository (`upstream` > `parent` > `origin`).
pub fn list_github_issues_blocking(
    project_id: usize,
    state: Option<&str>,
    label: Option<&str>,
    search: Option<&str>,
    force_refresh: bool,
) -> Result<Vec<GithubIssueSummary>, GithubError> {
    let project = crate::registry::find_project_by_id(project_id)?;
    let path = Path::new(&project.path);
    let ctx = resolve_collab_context_blocking(project_id, path, &project.path, false);
    let target_repo = ctx.target_repo.ok_or_else(|| GithubError::Gh {
        operation: "issue list".to_string(),
        details: "No GitHub remote ('upstream' or 'origin') detected for this project.".to_string(),
    })?;
    let nwo = validate_nwo(&target_repo)?;
    let state_val = match state.unwrap_or("open") {
        "open" => "open",
        "closed" => "closed",
        "all" => "all",
        other => {
            return Err(GithubError::Gh {
                operation: "issue list".to_string(),
                details: format!("Invalid issue state '{}'", other),
            });
        }
    };
    let label_val = label.map(str::trim).filter(|s| !s.is_empty());
    let search_val = search.map(str::trim).filter(|s| !s.is_empty());
    let cache_key = format!(
        "proj:{}:issues:{}:{}:{}:{}",
        project_id,
        nwo,
        state_val,
        label_val.unwrap_or(""),
        search_val.unwrap_or("")
    );
    let ttl = cache_ttl_seconds(&project.path);
    if let Some(cached) = read_cached_json::<Vec<GithubIssueSummary>>(&cache_key, ttl, force_refresh) {
        return Ok(cached);
    }

    let limit_str = MAX_GH_LIST_ITEMS_COUNT.to_string();
    let mut args: Vec<&str> = vec![
        "issue",
        "list",
        "--repo",
        &nwo,
        "--state",
        state_val,
        "--limit",
        &limit_str,
        "--json",
        "number,title,state,author,labels,comments,assignees,url,createdAt,updatedAt",
    ];
    if let Some(lbl) = label_val {
        if lbl.starts_with('-') {
            return Err(GithubError::Gh {
                operation: "issue list".to_string(),
                details: format!("Invalid label '{}'", lbl),
            });
        }
        args.push("--label");
        args.push(lbl);
    }
    if let Some(q) = search_val {
        if q.starts_with('-') {
            return Err(GithubError::Gh {
                operation: "issue list".to_string(),
                details: format!("Invalid search query '{}'", q),
            });
        }
        args.push("--search");
        args.push(q);
    }

    let stdout = gh(Some(path), &args)?;
    let rows: Vec<GhIssueListRow> = serde_json::from_slice(&stdout).map_err(|e| GithubError::UnexpectedOutput {
        operation: "issue list".to_string(),
        details: e.to_string(),
    })?;
    let summaries: Vec<GithubIssueSummary> = rows
        .into_iter()
        .map(|r| GithubIssueSummary {
            number: r.number,
            title: r.title,
            state: r.state,
            author: r.author.and_then(|a| a.login).unwrap_or_else(|| "ghost".to_string()),
            labels: r.labels.into_iter().map(|l| l.name).collect(),
            comments_count: r.comments.len(),
            is_assigned: !r.assignees.is_empty(),
            url: r.url,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect();
    write_cached_json(cache_key, &summaries);
    Ok(summaries)
}

/// Reads full details and discussion comments for a single GitHub issue.
pub fn get_github_issue_blocking(
    project_id: usize,
    issue_number: i64,
    force_refresh: bool,
) -> Result<GithubIssueDetail, GithubError> {
    if issue_number <= 0 {
        return Err(GithubError::Gh {
            operation: "issue view".to_string(),
            details: "Issue number must be positive.".to_string(),
        });
    }
    let project = crate::registry::find_project_by_id(project_id)?;
    let path = Path::new(&project.path);
    let ctx = resolve_collab_context_blocking(project_id, path, &project.path, false);
    let target_repo = ctx.target_repo.ok_or_else(|| GithubError::Gh {
        operation: "issue view".to_string(),
        details: "No GitHub remote detected for this project.".to_string(),
    })?;
    let nwo = validate_nwo(&target_repo)?;
    let cache_key = format!("proj:{}:issue:{}:{}", project_id, nwo, issue_number);
    let ttl = cache_ttl_seconds(&project.path);
    if let Some(cached) = read_cached_json::<GithubIssueDetail>(&cache_key, ttl, force_refresh) {
        return Ok(cached);
    }

    let num_str = issue_number.to_string();
    let stdout = gh(
        Some(path),
        &[
            "issue",
            "view",
            &num_str,
            "--repo",
            &nwo,
            "--json",
            "number,title,state,author,labels,url,body,createdAt,updatedAt,comments",
        ],
    )?;
    let row: GhIssueDetailRow = serde_json::from_slice(&stdout).map_err(|e| GithubError::UnexpectedOutput {
        operation: "issue view".to_string(),
        details: e.to_string(),
    })?;
    let detail = GithubIssueDetail {
        number: row.number,
        title: row.title,
        state: row.state,
        author: row.author.and_then(|a| a.login).unwrap_or_else(|| "ghost".to_string()),
        labels: row.labels.into_iter().map(|l| l.name).collect(),
        url: row.url,
        body: row.body.unwrap_or_default(),
        created_at: row.created_at,
        updated_at: row.updated_at,
        comments: row
            .comments
            .into_iter()
            .map(|c| GithubCommentItem {
                author: c.author.and_then(|a| a.login).unwrap_or_else(|| "ghost".to_string()),
                body: c.body.unwrap_or_default(),
                created_at: c.created_at.unwrap_or_default(),
            })
            .collect(),
    };
    write_cached_json(cache_key, &detail);
    Ok(detail)
}

/// Lists GitHub pull requests from the project's effective target repository.
pub fn list_github_prs_blocking(
    project_id: usize,
    state: Option<&str>,
    author_me: bool,
    force_refresh: bool,
) -> Result<Vec<GithubPrSummary>, GithubError> {
    let project = crate::registry::find_project_by_id(project_id)?;
    let path = Path::new(&project.path);
    let ctx = resolve_collab_context_blocking(project_id, path, &project.path, false);
    let target_repo = ctx.target_repo.ok_or_else(|| GithubError::Gh {
        operation: "pr list".to_string(),
        details: "No GitHub remote detected for this project.".to_string(),
    })?;
    let nwo = validate_nwo(&target_repo)?;
    let state_val = match state.unwrap_or("open") {
        "open" => "open",
        "closed" => "closed",
        "merged" => "merged",
        "all" => "all",
        other => {
            return Err(GithubError::Gh {
                operation: "pr list".to_string(),
                details: format!("Invalid PR state '{}'", other),
            });
        }
    };
    let cache_key = format!("proj:{}:prs:{}:{}:{}", project_id, nwo, state_val, author_me);
    let ttl = cache_ttl_seconds(&project.path);
    if let Some(cached) = read_cached_json::<Vec<GithubPrSummary>>(&cache_key, ttl, force_refresh) {
        return Ok(cached);
    }

    let limit_str = MAX_GH_LIST_ITEMS_COUNT.to_string();
    let mut args: Vec<&str> = vec![
        "pr",
        "list",
        "--repo",
        &nwo,
        "--state",
        state_val,
        "--limit",
        &limit_str,
        "--json",
        "number,title,state,isDraft,author,headRefName,baseRefName,mergeable,statusCheckRollup,url,createdAt,updatedAt",
    ];
    if author_me {
        args.push("--author");
        args.push("@me");
    }

    let stdout = gh(Some(path), &args)?;
    let rows: Vec<GhPrListRow> = serde_json::from_slice(&stdout).map_err(|e| GithubError::UnexpectedOutput {
        operation: "pr list".to_string(),
        details: e.to_string(),
    })?;
    let summaries: Vec<GithubPrSummary> = rows
        .into_iter()
        .map(|r| GithubPrSummary {
            number: r.number,
            title: r.title,
            state: r.state,
            is_draft: r.is_draft,
            author: r.author.and_then(|a| a.login).unwrap_or_else(|| "ghost".to_string()),
            head_ref_name: r.head_ref_name,
            base_ref_name: r.base_ref_name,
            mergeable: r.mergeable.unwrap_or_else(|| "UNKNOWN".to_string()),
            checks_summary: summarize_checks(&r.status_check_rollup),
            url: r.url,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect();
    write_cached_json(cache_key, &summaries);
    Ok(summaries)
}

/// Reads full PR details, reviews, CI checks, and comments.
pub fn get_github_pr_blocking(
    project_id: usize,
    pr_number: i64,
    force_refresh: bool,
) -> Result<GithubPrDetail, GithubError> {
    if pr_number <= 0 {
        return Err(GithubError::Gh {
            operation: "pr view".to_string(),
            details: "PR number must be positive.".to_string(),
        });
    }
    let project = crate::registry::find_project_by_id(project_id)?;
    let path = Path::new(&project.path);
    let ctx = resolve_collab_context_blocking(project_id, path, &project.path, false);
    let target_repo = ctx.target_repo.ok_or_else(|| GithubError::Gh {
        operation: "pr view".to_string(),
        details: "No GitHub remote detected for this project.".to_string(),
    })?;
    let nwo = validate_nwo(&target_repo)?;
    let cache_key = format!("proj:{}:pr:{}:{}", project_id, nwo, pr_number);
    let ttl = cache_ttl_seconds(&project.path);
    if let Some(cached) = read_cached_json::<GithubPrDetail>(&cache_key, ttl, force_refresh) {
        return Ok(cached);
    }

    let num_str = pr_number.to_string();
    let stdout = gh(
        Some(path),
        &[
            "pr",
            "view",
            &num_str,
            "--repo",
            &nwo,
            "--json",
            "number,title,state,isDraft,author,headRefName,baseRefName,mergeable,url,body,changedFiles,additions,deletions,comments,reviews,statusCheckRollup",
        ],
    )?;
    let row: GhPrDetailRow = serde_json::from_slice(&stdout).map_err(|e| GithubError::UnexpectedOutput {
        operation: "pr view".to_string(),
        details: e.to_string(),
    })?;
    let detail = GithubPrDetail {
        number: row.number,
        title: row.title,
        state: row.state,
        is_draft: row.is_draft,
        author: row.author.and_then(|a| a.login).unwrap_or_else(|| "ghost".to_string()),
        head_ref_name: row.head_ref_name,
        base_ref_name: row.base_ref_name,
        mergeable: row.mergeable.unwrap_or_else(|| "UNKNOWN".to_string()),
        url: row.url,
        body: row.body.unwrap_or_default(),
        changed_files_count: row.changed_files.unwrap_or(0),
        additions_count: row.additions.unwrap_or(0),
        deletions_count: row.deletions.unwrap_or(0),
        comments: row
            .comments
            .into_iter()
            .map(|c| GithubCommentItem {
                author: c.author.and_then(|a| a.login).unwrap_or_else(|| "ghost".to_string()),
                body: c.body.unwrap_or_default(),
                created_at: c.created_at.unwrap_or_default(),
            })
            .collect(),
        reviews: row
            .reviews
            .into_iter()
            .map(|rv| GithubReviewItem {
                author: rv.author.and_then(|a| a.login).unwrap_or_else(|| "ghost".to_string()),
                state: rv.state.unwrap_or_default(),
                body: rv.body.unwrap_or_default(),
                submitted_at: rv.submitted_at.unwrap_or_default(),
            })
            .collect(),
        checks: row
            .status_check_rollup
            .into_iter()
            .map(|ck| GithubCheckItem {
                name: ck.name.or(ck.context).unwrap_or_else(|| "check".to_string()),
                status: ck.status.or(ck.state).unwrap_or_default(),
                conclusion: ck.conclusion.unwrap_or_default(),
            })
            .collect(),
    };
    write_cached_json(cache_key, &detail);
    Ok(detail)
}

/// Lists latest GitHub releases from the project's effective target repository.
pub fn list_github_releases_blocking(
    project_id: usize,
    force_refresh: bool,
) -> Result<Vec<GithubReleaseSummary>, GithubError> {
    let project = crate::registry::find_project_by_id(project_id)?;
    let path = Path::new(&project.path);
    let ctx = resolve_collab_context_blocking(project_id, path, &project.path, false);
    let target_repo = ctx.target_repo.ok_or_else(|| GithubError::Gh {
        operation: "release list".to_string(),
        details: "No GitHub remote detected for this project.".to_string(),
    })?;
    let nwo = validate_nwo(&target_repo)?;
    let cache_key = format!("proj:{}:releases:{}", project_id, nwo);
    let ttl = cache_ttl_seconds(&project.path);
    if let Some(cached) = read_cached_json::<Vec<GithubReleaseSummary>>(&cache_key, ttl, force_refresh) {
        return Ok(cached);
    }

    let stdout = gh(
        Some(path),
        &[
            "release",
            "list",
            "--repo",
            &nwo,
            "--limit",
            "20",
            "--json",
            "tagName,name,isDraft,isPrerelease,publishedAt,url",
        ],
    )?;
    let rows: Vec<GhReleaseRow> = serde_json::from_slice(&stdout).map_err(|e| GithubError::UnexpectedOutput {
        operation: "release list".to_string(),
        details: e.to_string(),
    })?;
    let releases: Vec<GithubReleaseSummary> = rows
        .into_iter()
        .map(|r| GithubReleaseSummary {
            tag_name: r.tag_name.clone(),
            name: r.name.filter(|s| !s.is_empty()).unwrap_or(r.tag_name),
            is_draft: r.is_draft,
            is_prerelease: r.is_prerelease,
            published_at: r.published_at.unwrap_or_default(),
            url: r.url,
        })
        .collect();
    write_cached_json(cache_key, &releases);
    Ok(releases)
}

/// Creates a Draft Pull Request via `gh pr create --draft` and returns its GitHub URL.
pub fn create_github_draft_pr_blocking(
    project_id: usize,
    target_repo: &str,
    base_branch: &str,
    head_branch: &str,
    title: &str,
    body: &str,
) -> Result<String, GithubError> {
    let nwo = validate_nwo(target_repo)?;
    let base = validate_ref_arg(base_branch)?;
    let head = validate_ref_arg(head_branch)?;
    let clean_title = title.trim();
    if clean_title.is_empty() {
        return Err(GithubError::Gh {
            operation: "pr create".to_string(),
            details: "Pull request title must not be empty.".to_string(),
        });
    }
    let project = crate::registry::find_project_by_id(project_id)?;
    let path = Path::new(&project.path);
    let stdout = gh(
        Some(path),
        &[
            "pr",
            "create",
            "--draft",
            "--repo",
            &nwo,
            "--base",
            &base,
            "--head",
            &head,
            "--title",
            clean_title,
            "--body",
            body,
        ],
    )?;
    invalidate_project_cache(project_id);
    let url = String::from_utf8_lossy(&stdout).trim().to_string();
    if url.is_empty() {
        return Err(GithubError::UnexpectedOutput {
            operation: "pr create".to_string(),
            details: "gh pr create did not print a PR URL".to_string(),
        });
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_names_that_gh_could_read_as_options_are_rejected() {
        for name in ["one-ship", "a.b_c", "x"] {
            assert!(validate_repo_name(name).is_ok(), "{}", name);
        }
        for name in ["", "-private", "--public", ".", "..", "a b", "a/b", &"a".repeat(101)] {
            assert!(validate_repo_name(name).is_err(), "{}", name);
        }
    }

    #[test]
    fn remote_urls_that_git_could_read_as_options_are_rejected() {
        assert!(validate_remote_url("git@github.com:o/r.git").is_ok());
        assert!(validate_remote_url("https://github.com/o/r").is_ok());
        for url in ["", "--upload-pack=x", "https://x y", "a\nb"] {
            assert!(validate_remote_url(url).is_err(), "{}", url);
        }
    }

    #[test]
    fn extract_repo_name_parses_various_git_urls() {
        assert_eq!(extract_repo_name("https://github.com/octocat/Hello-World.git"), Some("Hello-World".to_string()));
        assert_eq!(extract_repo_name("git@github.com:octocat/Hello-World.git"), Some("Hello-World".to_string()));
        assert_eq!(extract_repo_name("https://github.com/octocat/Hello-World/"), Some("Hello-World".to_string()));
        assert_eq!(extract_repo_name("https://gitlab.com/group/subgroup/project"), Some("project".to_string()));
        assert_eq!(extract_repo_name(""), None);
        assert_eq!(extract_repo_name("-invalid"), None);
    }

    #[test]
    fn extract_github_nwo_parses_https_and_ssh() {
        assert_eq!(extract_github_nwo("https://github.com/octocat/Hello-World.git"), Some("octocat/Hello-World".to_string()));
        assert_eq!(extract_github_nwo("https://github.com/octocat/Hello-World/"), Some("octocat/Hello-World".to_string()));
        assert_eq!(extract_github_nwo("git@github.com:octocat/Hello-World.git"), Some("octocat/Hello-World".to_string()));
        assert_eq!(extract_github_nwo("https://gitlab.com/group/project"), None);
    }

    #[test]
    fn export_bindings_checkgitremoteaccessargs() {
        CheckGitRemoteAccessArgs::export_all().unwrap();
    }

    #[test]
    fn export_bindings_gitremoteaccesscheck() {
        GitRemoteAccessCheck::export_all().unwrap();
    }

    #[test]
    fn export_bindings_cloneandaddprojectargs() {
        CloneAndAddProjectArgs::export_all().unwrap();
    }
}
