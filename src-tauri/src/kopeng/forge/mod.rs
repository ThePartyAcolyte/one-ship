//! Provider-neutral Forge collaboration contract (Spec 39 §6–9).
//!
//! Abstracts issue, pull/merge request, release, and inbox operations across
//! GitHub (`gh`), GitLab (`glab`), and future VCS providers without silent fallbacks.

pub mod github;
pub mod gitlab;
pub mod inbox;

use std::path::Path;

use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::api::ApiError;
use crate::db::DatabaseError;
use crate::kopeng::git::{get_git_status, ForgeProviderKind, GitError, GitRemote};
use crate::kopeng::github::GithubError;
use crate::registry::{find_project_by_id, RegistryError};

/// Typed errors for provider-neutral Forge operations.
#[derive(Debug, thiserror::Error)]
pub enum ForgeError {
    #[error("Forge provider {provider:?} is unavailable: {reason}")]
    ProviderUnavailable {
        provider: ForgeProviderKind,
        reason: String,
    },
    #[error("Forge provider {provider:?} does not support '{operation}'")]
    UnsupportedCapability {
        provider: ForgeProviderKind,
        operation: String,
    },
    #[error("No Git remote is configured for this project")]
    NoRemoteConfigured,
    #[error("Unable to resolve target repository identifier (owner/repo) from the project's configured Git remote")]
    MissingTargetRepo,
    #[error(transparent)]
    Gh(#[from] GithubError),
    #[error("glab {operation} failed: {details}")]
    Glab { operation: String, details: String },
    #[error("Unexpected output from {operation}: {details}")]
    UnexpectedOutput { operation: String, details: String },
    #[error("Invalid provider response: {0}")]
    InvalidProviderResponse(String),
    #[error("Background task failed: {0}")]
    Task(String),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

impl ApiError for ForgeError {
    fn status(&self) -> StatusCode {
        match self {
            Self::ProviderUnavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Self::UnsupportedCapability { .. } => StatusCode::NOT_IMPLEMENTED,
            Self::NoRemoteConfigured | Self::MissingTargetRepo => StatusCode::BAD_REQUEST,
            Self::Gh(e) => e.status(),
            Self::Glab { .. }
            | Self::UnexpectedOutput { .. }
            | Self::InvalidProviderResponse(_) => StatusCode::BAD_GATEWAY,
            Self::Task(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Git(e) => e.status(),
            Self::Registry(e) => e.status(),
            Self::Database(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::ProviderUnavailable { .. } => "ForgeError::ProviderUnavailable",
            Self::UnsupportedCapability { .. } => "ForgeError::UnsupportedCapability",
            Self::NoRemoteConfigured => "ForgeError::NoRemoteConfigured",
            Self::MissingTargetRepo => "ForgeError::MissingTargetRepo",
            Self::Gh(e) => e.code(),
            Self::Glab { .. } => "ForgeError::Glab",
            Self::UnexpectedOutput { .. } => "ForgeError::UnexpectedOutput",
            Self::InvalidProviderResponse(_) => "ForgeError::InvalidProviderResponse",
            Self::Task(_) => "ForgeError::Task",
            Self::Git(e) => e.code(),
            Self::Registry(e) => e.code(),
            Self::Database(e) => e.code(),
        }
    }
}

/// Scoped project identity passed from the daemon command layer to a [`ForgeProvider`].
/// Providers never resolve projects by themselves.
#[derive(Clone, Copy, Debug)]
pub struct ForgeProjectRef<'a> {
    pub project_id: usize,
    pub path: &'a Path,
}

/// Executes a blocking [`std::process::Command`] with a deterministic timeout and concurrent
/// stdout/stderr pipe draining so large outputs never deadlock and hung processes are terminated.
pub(crate) fn run_sync_command_with_timeout(
    mut cmd: std::process::Command,
    timeout_seconds: u64,
    label: &str,
) -> Result<std::process::Output, std::io::Error> {
    use std::io::Read;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn()?;
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    let stdout_handle = std::thread::spawn(move || -> std::io::Result<Vec<u8>> {
        let mut buf = Vec::new();
        if let Some(mut pipe) = stdout_pipe {
            pipe.read_to_end(&mut buf)?;
        }
        Ok(buf)
    });
    let stderr_handle = std::thread::spawn(move || -> std::io::Result<Vec<u8>> {
        let mut buf = Vec::new();
        if let Some(mut pipe) = stderr_pipe {
            pipe.read_to_end(&mut buf)?;
        }
        Ok(buf)
    });

    let deadline = Instant::now() + Duration::from_secs(timeout_seconds.max(1));
    let status = loop {
        match child.try_wait()? {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_handle.join();
                let _ = stderr_handle.join();
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("{label} timed out after {timeout_seconds}s"),
                ));
            }
            None => std::thread::sleep(Duration::from_millis(25)),
        }
    };

    let stdout = stdout_handle
        .join()
        .map_err(|_| std::io::Error::other(format!("{label} stdout reader thread panicked")))??;
    let stderr = stderr_handle
        .join()
        .map_err(|_| std::io::Error::other(format!("{label} stderr reader thread panicked")))??;

    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

/// Provider-neutral topology and capability summary for a project's Forge remotes.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ForgeCollabContext {
    pub provider: ForgeProviderKind,
    pub provider_label: String,
    pub pull_request_label: String,
    pub has_origin: bool,
    pub origin_url: Option<String>,
    pub origin_repo: Option<String>,
    pub has_upstream: bool,
    pub upstream_url: Option<String>,
    pub upstream_repo: Option<String>,
    pub is_fork: bool,
    pub parent_repo: Option<String>,
    pub parent_url: Option<String>,
    pub target_repo: Option<String>,
    pub default_branch: Option<String>,
    pub ahead_upstream_count: usize,
    pub behind_upstream_count: usize,
    pub viewer_permission: Option<String>,
    pub supports_draft_pull_request: bool,
    pub supports_inbox: bool,
    pub error: Option<String>,
}

/// Provider-neutral summary of an issue.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ForgeIssueSummary {
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

/// Provider-neutral comment inside an issue or pull/merge request.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ForgeCommentItem {
    pub author: String,
    pub body: String,
    pub created_at: String,
}

/// Provider-neutral full issue detail.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ForgeIssueDetail {
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
    pub comments: Vec<ForgeCommentItem>,
}

/// Provider-neutral summary of a pull request or merge request.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ForgePullRequestSummary {
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

/// Provider-neutral review/approval entry on a pull/merge request.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ForgeReviewItem {
    pub author: String,
    pub state: String,
    pub body: String,
    pub submitted_at: String,
}

/// Provider-neutral CI/status check entry on a pull/merge request.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ForgeCheckItem {
    pub name: String,
    pub status: String,
    pub conclusion: String,
}

/// Provider-neutral full pull/merge request detail.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ForgePullRequestDetail {
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
    pub comments: Vec<ForgeCommentItem>,
    pub reviews: Vec<ForgeReviewItem>,
    pub checks: Vec<ForgeCheckItem>,
}

/// Provider-neutral summary of a release.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ForgeReleaseSummary {
    pub tag_name: String,
    pub name: String,
    pub is_draft: bool,
    pub is_prerelease: bool,
    pub published_at: String,
    pub url: String,
}

/// Query options passed to [`ForgeProvider::list_issues`].
#[derive(Clone, Debug, Default)]
pub struct ForgeIssueQuery {
    pub state: Option<String>,
    pub label: Option<String>,
    pub search: Option<String>,
    pub force_refresh: bool,
}

/// Query options passed to [`ForgeProvider::list_pull_requests`].
#[derive(Clone, Debug, Default)]
pub struct ForgePullRequestQuery {
    pub state: Option<String>,
    pub author_me: bool,
    pub force_refresh: bool,
}

/// Request payload passed to [`ForgeProvider::create_draft_pull_request`].
#[derive(Clone, Debug)]
pub struct ForgeDraftPullRequest {
    pub target_repo: String,
    pub base_branch: String,
    pub head_branch: String,
    pub title: String,
    pub body: String,
}

/// Synchronous Forge provider contract; daemon commands invoke it inside `spawn_blocking`.
pub trait ForgeProvider: Send + Sync {
    fn kind(&self) -> ForgeProviderKind;
    fn collab_context(
        &self,
        project: ForgeProjectRef<'_>,
        force_refresh: bool,
    ) -> Result<ForgeCollabContext, ForgeError>;
    fn list_issues(
        &self,
        project: ForgeProjectRef<'_>,
        query: &ForgeIssueQuery,
    ) -> Result<Vec<ForgeIssueSummary>, ForgeError>;
    fn get_issue(
        &self,
        project: ForgeProjectRef<'_>,
        number: i64,
        force_refresh: bool,
    ) -> Result<ForgeIssueDetail, ForgeError>;
    fn list_pull_requests(
        &self,
        project: ForgeProjectRef<'_>,
        query: &ForgePullRequestQuery,
    ) -> Result<Vec<ForgePullRequestSummary>, ForgeError>;
    fn get_pull_request(
        &self,
        project: ForgeProjectRef<'_>,
        number: i64,
        force_refresh: bool,
    ) -> Result<ForgePullRequestDetail, ForgeError>;
    fn list_releases(
        &self,
        project: ForgeProjectRef<'_>,
        force_refresh: bool,
    ) -> Result<Vec<ForgeReleaseSummary>, ForgeError>;
    fn create_draft_pull_request(
        &self,
        project: ForgeProjectRef<'_>,
        request: &ForgeDraftPullRequest,
    ) -> Result<String, ForgeError>;
}

/// Arguments for `get_forge_context`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct GetForgeContextArgs {
    #[ts(type = "number")]
    pub project_id: usize,
    pub force_refresh: Option<bool>,
}

/// Arguments for `list_forge_issues`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ListForgeIssuesArgs {
    #[ts(type = "number")]
    pub project_id: usize,
    pub state: Option<String>,
    pub label: Option<String>,
    pub search: Option<String>,
    pub force_refresh: Option<bool>,
}

/// Arguments for `get_forge_issue`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct GetForgeIssueArgs {
    #[ts(type = "number")]
    pub project_id: usize,
    #[ts(type = "number")]
    pub issue_number: i64,
    pub force_refresh: Option<bool>,
}

/// Arguments for `list_forge_pull_requests`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ListForgePullRequestsArgs {
    #[ts(type = "number")]
    pub project_id: usize,
    pub state: Option<String>,
    pub author_me: Option<bool>,
    pub force_refresh: Option<bool>,
}

/// Arguments for `get_forge_pull_request`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct GetForgePullRequestArgs {
    #[ts(type = "number")]
    pub project_id: usize,
    #[ts(type = "number")]
    pub pull_request_number: i64,
    pub force_refresh: Option<bool>,
}

/// Arguments for `list_forge_releases`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ListForgeReleasesArgs {
    #[ts(type = "number")]
    pub project_id: usize,
    pub force_refresh: Option<bool>,
}

/// Arguments for `create_forge_draft_pull_request`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CreateForgeDraftPullRequestArgs {
    #[ts(type = "number")]
    pub project_id: usize,
    pub target_repo: String,
    pub base_branch: String,
    pub head_branch: String,
    pub title: String,
    pub body: String,
}

/// Picks the effective remote (`upstream` > `origin` > first remote) and returns its provider.
pub fn select_primary_remote(remotes: &[GitRemote]) -> Result<&GitRemote, ForgeError> {
    if remotes.is_empty() {
        return Err(ForgeError::NoRemoteConfigured);
    }
    if let Some(upstream) = remotes.iter().find(|r| r.name == "upstream") {
        return Ok(upstream);
    }
    if let Some(origin) = remotes.iter().find(|r| r.name == "origin") {
        return Ok(origin);
    }
    Ok(&remotes[0])
}

/// Resolves the [`ForgeProvider`] for a repository path without silent fallbacks.
pub fn resolve_provider_for_path(path: &Path) -> Result<Box<dyn ForgeProvider>, ForgeError> {
    let status = get_git_status(path)?;
    let primary = select_primary_remote(&status.remotes)?;
    match primary.provider {
        ForgeProviderKind::Github => Ok(Box::new(github::GithubForgeProvider)),
        ForgeProviderKind::Gitlab => Ok(Box::new(gitlab::GitlabForgeProvider)),
        ForgeProviderKind::Unknown => Err(ForgeError::UnsupportedCapability {
            provider: ForgeProviderKind::Unknown,
            operation: "forge_collaboration".to_string(),
        }),
    }
}

fn provider_for_project_blocking(
    project_id: usize,
) -> Result<(String, Box<dyn ForgeProvider>), ForgeError> {
    let project = find_project_by_id(project_id)?;
    let provider = resolve_provider_for_path(Path::new(&project.path))?;
    Ok((project.path, provider))
}

/// Returns the Forge collaboration context for `project_id`.
pub async fn get_forge_context(args: GetForgeContextArgs) -> Result<ForgeCollabContext, ForgeError> {
    tokio::task::spawn_blocking(move || {
        let (path, provider) = provider_for_project_blocking(args.project_id)?;
        provider.collab_context(
            ForgeProjectRef {
                project_id: args.project_id,
                path: Path::new(&path),
            },
            args.force_refresh.unwrap_or(false),
        )
    })
    .await
    .map_err(|e| ForgeError::Task(e.to_string()))?
}

/// Lists Forge issues for `project_id`.
pub async fn list_forge_issues(args: ListForgeIssuesArgs) -> Result<Vec<ForgeIssueSummary>, ForgeError> {
    tokio::task::spawn_blocking(move || {
        let (path, provider) = provider_for_project_blocking(args.project_id)?;
        let query = ForgeIssueQuery {
            state: args.state,
            label: args.label,
            search: args.search,
            force_refresh: args.force_refresh.unwrap_or(false),
        };
        provider.list_issues(
            ForgeProjectRef {
                project_id: args.project_id,
                path: Path::new(&path),
            },
            &query,
        )
    })
    .await
    .map_err(|e| ForgeError::Task(e.to_string()))?
}

/// Gets full details of a Forge issue by number.
pub async fn get_forge_issue(args: GetForgeIssueArgs) -> Result<ForgeIssueDetail, ForgeError> {
    tokio::task::spawn_blocking(move || {
        let (path, provider) = provider_for_project_blocking(args.project_id)?;
        provider.get_issue(
            ForgeProjectRef {
                project_id: args.project_id,
                path: Path::new(&path),
            },
            args.issue_number,
            args.force_refresh.unwrap_or(false),
        )
    })
    .await
    .map_err(|e| ForgeError::Task(e.to_string()))?
}

/// Lists Forge pull/merge requests for `project_id`.
pub async fn list_forge_pull_requests(
    args: ListForgePullRequestsArgs,
) -> Result<Vec<ForgePullRequestSummary>, ForgeError> {
    tokio::task::spawn_blocking(move || {
        let (path, provider) = provider_for_project_blocking(args.project_id)?;
        let query = ForgePullRequestQuery {
            state: args.state,
            author_me: args.author_me.unwrap_or(false),
            force_refresh: args.force_refresh.unwrap_or(false),
        };
        provider.list_pull_requests(
            ForgeProjectRef {
                project_id: args.project_id,
                path: Path::new(&path),
            },
            &query,
        )
    })
    .await
    .map_err(|e| ForgeError::Task(e.to_string()))?
}

/// Gets full details of a Forge pull/merge request by number.
pub async fn get_forge_pull_request(
    args: GetForgePullRequestArgs,
) -> Result<ForgePullRequestDetail, ForgeError> {
    tokio::task::spawn_blocking(move || {
        let (path, provider) = provider_for_project_blocking(args.project_id)?;
        provider.get_pull_request(
            ForgeProjectRef {
                project_id: args.project_id,
                path: Path::new(&path),
            },
            args.pull_request_number,
            args.force_refresh.unwrap_or(false),
        )
    })
    .await
    .map_err(|e| ForgeError::Task(e.to_string()))?
}

/// Lists Forge releases for `project_id`.
pub async fn list_forge_releases(
    args: ListForgeReleasesArgs,
) -> Result<Vec<ForgeReleaseSummary>, ForgeError> {
    tokio::task::spawn_blocking(move || {
        let (path, provider) = provider_for_project_blocking(args.project_id)?;
        provider.list_releases(
            ForgeProjectRef {
                project_id: args.project_id,
                path: Path::new(&path),
            },
            args.force_refresh.unwrap_or(false),
        )
    })
    .await
    .map_err(|e| ForgeError::Task(e.to_string()))?
}

/// Creates a draft pull/merge request via the active provider.
pub async fn create_forge_draft_pull_request(
    args: CreateForgeDraftPullRequestArgs,
) -> Result<String, ForgeError> {
    tokio::task::spawn_blocking(move || {
        let (path, provider) = provider_for_project_blocking(args.project_id)?;
        let req = ForgeDraftPullRequest {
            target_repo: args.target_repo,
            base_branch: args.base_branch,
            head_branch: args.head_branch,
            title: args.title,
            body: args.body,
        };
        provider.create_draft_pull_request(
            ForgeProjectRef {
                project_id: args.project_id,
                path: Path::new(&path),
            },
            &req,
        )
    })
    .await
    .map_err(|e| ForgeError::Task(e.to_string()))?
}
