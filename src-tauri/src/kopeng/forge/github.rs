//! GitHub adapter for [`ForgeProvider`], delegating to [`crate::kopeng::github`] blocking primitives.

use crate::kopeng::git::ForgeProviderKind;
use crate::kopeng::github::{
    self, GithubCommentItem, GithubError, GithubIssueDetail, GithubIssueSummary, GithubPrDetail,
    GithubPrSummary, GithubReleaseSummary,
};

use super::{
    ForgeCheckItem, ForgeCollabContext, ForgeCommentItem, ForgeDraftPullRequest, ForgeError,
    ForgeIssueDetail, ForgeIssueQuery, ForgeIssueSummary, ForgeProjectRef, ForgeProvider,
    ForgePullRequestDetail, ForgePullRequestQuery, ForgePullRequestSummary, ForgeReleaseSummary,
    ForgeReviewItem,
};

/// GitHub implementation of [`ForgeProvider`].
pub struct GithubForgeProvider;

fn map_github_error(err: GithubError) -> ForgeError {
    if let GithubError::Gh { ref details, .. } = err {
        let lower = details.to_ascii_lowercase();
        if lower.contains("cannot run gh")
            || lower.contains("not logged into")
            || lower.contains("gh auth login")
            || lower.contains("no such file or directory")
        {
            return ForgeError::ProviderUnavailable {
                provider: ForgeProviderKind::Github,
                reason: details.clone(),
            };
        }
    }
    ForgeError::Gh(err)
}

fn map_comment(c: GithubCommentItem) -> ForgeCommentItem {
    ForgeCommentItem {
        author: c.author,
        body: c.body,
        created_at: c.created_at,
    }
}

fn map_issue_summary(i: GithubIssueSummary) -> ForgeIssueSummary {
    ForgeIssueSummary {
        number: i.number,
        title: i.title,
        state: i.state,
        author: i.author,
        labels: i.labels,
        comments_count: i.comments_count,
        is_assigned: i.is_assigned,
        url: i.url,
        created_at: i.created_at,
        updated_at: i.updated_at,
    }
}

fn map_issue_detail(d: GithubIssueDetail) -> ForgeIssueDetail {
    ForgeIssueDetail {
        number: d.number,
        title: d.title,
        state: d.state,
        author: d.author,
        labels: d.labels,
        url: d.url,
        body: d.body,
        created_at: d.created_at,
        updated_at: d.updated_at,
        comments: d.comments.into_iter().map(map_comment).collect(),
    }
}

fn map_pr_summary(p: GithubPrSummary) -> ForgePullRequestSummary {
    ForgePullRequestSummary {
        number: p.number,
        title: p.title,
        state: p.state,
        is_draft: p.is_draft,
        author: p.author,
        head_ref_name: p.head_ref_name,
        base_ref_name: p.base_ref_name,
        mergeable: p.mergeable,
        checks_summary: p.checks_summary,
        url: p.url,
        created_at: p.created_at,
        updated_at: p.updated_at,
    }
}

fn map_pr_detail(p: GithubPrDetail) -> ForgePullRequestDetail {
    ForgePullRequestDetail {
        number: p.number,
        title: p.title,
        state: p.state,
        is_draft: p.is_draft,
        author: p.author,
        head_ref_name: p.head_ref_name,
        base_ref_name: p.base_ref_name,
        mergeable: p.mergeable,
        url: p.url,
        body: p.body,
        changed_files_count: p.changed_files_count,
        additions_count: p.additions_count,
        deletions_count: p.deletions_count,
        comments: p.comments.into_iter().map(map_comment).collect(),
        reviews: p
            .reviews
            .into_iter()
            .map(|r| ForgeReviewItem {
                author: r.author,
                state: r.state,
                body: r.body,
                submitted_at: r.submitted_at,
            })
            .collect(),
        checks: p
            .checks
            .into_iter()
            .map(|c| ForgeCheckItem {
                name: c.name,
                status: c.status,
                conclusion: c.conclusion,
            })
            .collect(),
    }
}

fn map_release(r: GithubReleaseSummary) -> ForgeReleaseSummary {
    ForgeReleaseSummary {
        tag_name: r.tag_name,
        name: r.name,
        is_draft: r.is_draft,
        is_prerelease: r.is_prerelease,
        published_at: r.published_at,
        url: r.url,
    }
}

impl ForgeProvider for GithubForgeProvider {
    fn kind(&self) -> ForgeProviderKind {
        ForgeProviderKind::Github
    }

    fn collab_context(
        &self,
        project: ForgeProjectRef<'_>,
        force_refresh: bool,
    ) -> Result<ForgeCollabContext, ForgeError> {
        let proj_str = project.path.to_string_lossy().to_string();
        let ctx = github::resolve_collab_context_blocking(
            project.project_id,
            project.path,
            &proj_str,
            force_refresh,
        );
        Ok(ForgeCollabContext {
            provider: ForgeProviderKind::Github,
            provider_label: "GitHub".to_string(),
            pull_request_label: "Pull Requests".to_string(),
            has_origin: ctx.has_origin,
            origin_url: ctx.origin_url,
            origin_repo: ctx.origin_repo,
            has_upstream: ctx.has_upstream,
            upstream_url: ctx.upstream_url,
            upstream_repo: ctx.upstream_repo,
            is_fork: ctx.is_fork,
            parent_repo: ctx.parent_repo,
            parent_url: ctx.parent_url,
            target_repo: ctx.target_repo,
            default_branch: ctx.default_branch,
            ahead_upstream_count: ctx.ahead_upstream_count,
            behind_upstream_count: ctx.behind_upstream_count,
            viewer_permission: ctx.viewer_permission,
            supports_draft_pull_request: true,
            supports_inbox: true,
            error: ctx.gh_error,
        })
    }

    fn list_issues(
        &self,
        project: ForgeProjectRef<'_>,
        query: &ForgeIssueQuery,
    ) -> Result<Vec<ForgeIssueSummary>, ForgeError> {
        let issues = github::list_github_issues_blocking(
            project.project_id,
            query.state.as_deref(),
            query.label.as_deref(),
            query.search.as_deref(),
            query.force_refresh,
        )
        .map_err(map_github_error)?;
        Ok(issues.into_iter().map(map_issue_summary).collect())
    }

    fn get_issue(
        &self,
        project: ForgeProjectRef<'_>,
        number: i64,
        force_refresh: bool,
    ) -> Result<ForgeIssueDetail, ForgeError> {
        let detail = github::get_github_issue_blocking(project.project_id, number, force_refresh)
            .map_err(map_github_error)?;
        Ok(map_issue_detail(detail))
    }

    fn list_pull_requests(
        &self,
        project: ForgeProjectRef<'_>,
        query: &ForgePullRequestQuery,
    ) -> Result<Vec<ForgePullRequestSummary>, ForgeError> {
        let prs = github::list_github_prs_blocking(
            project.project_id,
            query.state.as_deref(),
            query.author_me,
            query.force_refresh,
        )
        .map_err(map_github_error)?;
        Ok(prs.into_iter().map(map_pr_summary).collect())
    }

    fn get_pull_request(
        &self,
        project: ForgeProjectRef<'_>,
        number: i64,
        force_refresh: bool,
    ) -> Result<ForgePullRequestDetail, ForgeError> {
        let detail = github::get_github_pr_blocking(project.project_id, number, force_refresh)
            .map_err(map_github_error)?;
        Ok(map_pr_detail(detail))
    }

    fn list_releases(
        &self,
        project: ForgeProjectRef<'_>,
        force_refresh: bool,
    ) -> Result<Vec<ForgeReleaseSummary>, ForgeError> {
        let releases = github::list_github_releases_blocking(project.project_id, force_refresh)
            .map_err(map_github_error)?;
        Ok(releases.into_iter().map(map_release).collect())
    }

    fn create_draft_pull_request(
        &self,
        project: ForgeProjectRef<'_>,
        request: &ForgeDraftPullRequest,
    ) -> Result<String, ForgeError> {
        github::create_github_draft_pr_blocking(
            project.project_id,
            &request.target_repo,
            &request.base_branch,
            &request.head_branch,
            &request.title,
            &request.body,
        )
        .map_err(map_github_error)
    }
}
