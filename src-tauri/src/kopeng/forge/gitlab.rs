//! GitLab adapter for [`ForgeProvider`] backed by `glab api` (Spec 39 §8).

use std::path::Path;
use std::process::Command;

use serde_json::Value;

use crate::kopeng::git::ForgeProviderKind;

use super::{
    ForgeCheckItem, ForgeCollabContext, ForgeCommentItem, ForgeDraftPullRequest, ForgeError,
    ForgeIssueDetail, ForgeIssueQuery, ForgeIssueSummary, ForgeProjectRef, ForgeProvider,
    ForgePullRequestDetail, ForgePullRequestQuery, ForgePullRequestSummary, ForgeReleaseSummary,
    ForgeReviewItem,
};

/// GitLab implementation of [`ForgeProvider`] via `glab api`.
pub struct GitlabForgeProvider;

fn git_remote_url(path: &Path, remote_name: &str) -> Option<String> {
    let mut cmd = Command::new("git");
    cmd.env("GIT_TERMINAL_PROMPT", "0")
        .args(["config", "--get", &format!("remote.{}.url", remote_name)])
        .current_dir(path);
    let timeout_seconds =
        crate::config::resolve_effective_u64(None, None, "vcs", "forge_cli_timeout_seconds").max(1);
    let out = match super::run_sync_command_with_timeout(cmd, timeout_seconds, "git config --get") {
        Ok(out) => out,
        Err(e) => {
            crate::logger::log("WARN", "forge", &format!("git remote lookup failed: {e}"));
            return None;
        }
    };
    if !out.status.success() {
        return None;
    }
    let val = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!val.is_empty()).then_some(val)
}

fn git_rev_list_counts(path: &Path, left_ref: &str, right_ref: &str) -> (usize, usize) {
    let range = format!("{}...{}", left_ref, right_ref);
    let mut cmd = Command::new("git");
    cmd.env("GIT_TERMINAL_PROMPT", "0")
        .args(["rev-list", "--left-right", "--count", &range])
        .current_dir(path);
    let timeout_seconds =
        crate::config::resolve_effective_u64(None, None, "vcs", "forge_cli_timeout_seconds").max(1);
    let out = match super::run_sync_command_with_timeout(cmd, timeout_seconds, "git rev-list") {
        Ok(out) => out,
        Err(e) => {
            crate::logger::log("WARN", "forge", &format!("git rev-list failed: {e}"));
            return (0, 0);
        }
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

/// Extracts `(host, namespace/project)` from a GitLab HTTPS, SSH, or SCP-style remote URL.
pub fn extract_gitlab_host_and_path(url: &str) -> Option<(String, String)> {
    let trimmed = url.trim().trim_end_matches('/');
    let without_git = trimmed.strip_suffix(".git").unwrap_or(trimmed);

    let (host, path_part) = if let Some(after_scheme) = without_git.split_once("://").map(|(_, r)| r) {
        let (authority, rest) = after_scheme.split_once('/')?;
        let host_port = authority.rsplit_once('@').map(|(_, h)| h).unwrap_or(authority);
        let host = host_port.split(':').next().unwrap_or(host_port);
        (host, rest)
    } else if let Some((user_host, rest)) = without_git.split_once(':') {
        let host = user_host.rsplit_once('@').map(|(_, h)| h).unwrap_or(user_host);
        if host.contains('/') {
            return None;
        }
        (host, rest)
    } else {
        return None;
    };

    let clean_path = path_part.trim_matches('/');
    if host.is_empty() || clean_path.is_empty() || !clean_path.contains('/') {
        return None;
    }
    if clean_path
        .split('/')
        .any(|seg| seg.is_empty() || seg.starts_with('-'))
    {
        return None;
    }
    Some((host.to_string(), clean_path.to_string()))
}

fn percent_encode_component(raw: &str) -> String {
    let mut encoded = String::with_capacity(raw.len() * 3);
    for b in raw.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            encoded.push(*b as char);
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{:02X}", b));
        }
    }
    encoded
}

fn run_glab_api(host: Option<&str>, endpoint: &str, operation: &str) -> Result<Value, ForgeError> {
    let mut cmd = Command::new("glab");
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    if let Some(h) = host {
        if !h.eq_ignore_ascii_case("gitlab.com") {
            cmd.env("GITLAB_HOST", h);
        }
    }
    cmd.args(["api", endpoint]);

    let timeout_seconds =
        crate::config::resolve_effective_u64(None, None, "vcs", "forge_cli_timeout_seconds").max(1);
    let output = super::run_sync_command_with_timeout(
        cmd,
        timeout_seconds,
        &format!("glab api {endpoint}"),
    )
    .map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ForgeError::ProviderUnavailable {
                provider: ForgeProviderKind::Gitlab,
                reason: "glab CLI is not installed or not in PATH. Install `glab` and run `glab auth login`.".to_string(),
            }
        } else {
            ForgeError::Glab {
                operation: operation.to_string(),
                details: format!("cannot run glab: {}", e),
            }
        }
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let details = if !stderr.is_empty() { stderr } else { stdout };
        let lower = details.to_ascii_lowercase();
        if lower.contains("401")
            || lower.contains("unauthorized")
            || lower.contains("not logged in")
            || lower.contains("glab auth login")
            || lower.contains("no token")
            || lower.contains("invalid token")
        {
            return Err(ForgeError::ProviderUnavailable {
                provider: ForgeProviderKind::Gitlab,
                reason: details,
            });
        }
        return Err(ForgeError::Glab {
            operation: operation.to_string(),
            details,
        });
    }

    serde_json::from_slice(&output.stdout).map_err(|e| {
        ForgeError::InvalidProviderResponse(format!(
            "glab api {} returned invalid JSON: {}",
            endpoint, e
        ))
    })
}

fn resolve_target_host_and_path(path: &Path) -> Result<(String, String), ForgeError> {
    let target_url = git_remote_url(path, "upstream")
        .or_else(|| git_remote_url(path, "origin"))
        .ok_or(ForgeError::NoRemoteConfigured)?;

    extract_gitlab_host_and_path(&target_url).ok_or_else(|| ForgeError::InvalidProviderResponse(
        format!("Cannot parse GitLab project path from remote URL '{}'", target_url),
    ))
}

fn resolve_gitlab_project(
    path: &Path,
) -> Result<(String, String, i64, Value), ForgeError> {
    let (host, repo_path) = resolve_target_host_and_path(path)?;
    let encoded = percent_encode_component(&repo_path);
    let endpoint = format!("projects/{}", encoded);
    let project_json = run_glab_api(Some(&host), &endpoint, "resolve_project")?;
    let project_id = project_json
        .get("id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| {
            ForgeError::InvalidProviderResponse(format!(
                "GitLab project '{}' response missing numeric 'id'",
                repo_path
            ))
        })?;
    Ok((host, repo_path, project_id, project_json))
}

fn map_glab_issue_state(state: Option<&str>) -> &'static str {
    match state.unwrap_or("open") {
        "closed" => "closed",
        "all" => "all",
        _ => "opened",
    }
}

fn map_glab_mr_state(state: Option<&str>) -> &'static str {
    match state.unwrap_or("open") {
        "closed" => "closed",
        "merged" => "merged",
        "all" => "all",
        _ => "opened",
    }
}

fn normalize_gitlab_state(raw: &str) -> String {
    if raw.eq_ignore_ascii_case("opened") {
        "open".to_string()
    } else {
        raw.to_ascii_lowercase()
    }
}

fn map_checks_summary(pipeline_status: Option<&str>) -> String {
    match pipeline_status.unwrap_or("").to_ascii_lowercase().as_str() {
        "success" => "passing".to_string(),
        "failed" => "failing".to_string(),
        "running" | "pending" | "created" | "waiting_for_resource" | "preparing" => {
            "pending".to_string()
        }
        _ => "unknown".to_string(),
    }
}

fn map_glab_notes(notes_json: &Value) -> Vec<ForgeCommentItem> {
    notes_json
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|n| !n.get("system").and_then(|v| v.as_bool()).unwrap_or(false))
                .map(|n| ForgeCommentItem {
                    author: n
                        .pointer("/author/username")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    body: n
                        .get("body")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    created_at: n
                        .get("created_at")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn extract_labels(val: &Value) -> Vec<String> {
    val.get("labels")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    item.as_str()
                        .map(str::to_string)
                        .or_else(|| item.get("name").and_then(|n| n.as_str()).map(str::to_string))
                })
                .collect()
        })
        .unwrap_or_default()
}

impl ForgeProvider for GitlabForgeProvider {
    fn kind(&self) -> ForgeProviderKind {
        ForgeProviderKind::Gitlab
    }

    fn collab_context(
        &self,
        project: ForgeProjectRef<'_>,
        _force_refresh: bool,
    ) -> Result<ForgeCollabContext, ForgeError> {
        let origin_url = git_remote_url(project.path, "origin");
        let upstream_url = git_remote_url(project.path, "upstream");
        let origin_repo = origin_url
            .as_deref()
            .and_then(extract_gitlab_host_and_path)
            .map(|(_, p)| p);
        let upstream_repo = upstream_url
            .as_deref()
            .and_then(extract_gitlab_host_and_path)
            .map(|(_, p)| p);

        let mut ctx = ForgeCollabContext {
            provider: ForgeProviderKind::Gitlab,
            provider_label: "GitLab".to_string(),
            pull_request_label: "Merge Requests".to_string(),
            has_origin: origin_url.is_some(),
            origin_url,
            origin_repo: origin_repo.clone(),
            has_upstream: upstream_url.is_some(),
            upstream_url,
            upstream_repo: upstream_repo.clone(),
            is_fork: false,
            parent_repo: None,
            parent_url: None,
            target_repo: upstream_repo.or(origin_repo),
            default_branch: None,
            ahead_upstream_count: 0,
            behind_upstream_count: 0,
            viewer_permission: None,
            supports_draft_pull_request: false,
            supports_inbox: true,
            error: None,
        };

        match resolve_gitlab_project(project.path) {
            Ok((_host, repo_path, _id, json)) => {
                ctx.target_repo = Some(repo_path);
                ctx.default_branch = json
                    .get("default_branch")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                if let Some(forked) = json.get("forked_from_project") {
                    if !forked.is_null() {
                        ctx.is_fork = true;
                        ctx.parent_repo = forked
                            .get("path_with_namespace")
                            .and_then(|v| v.as_str())
                            .map(str::to_string);
                        ctx.parent_url = forked
                            .get("web_url")
                            .and_then(|v| v.as_str())
                            .map(str::to_string);
                    }
                }
                let access = json
                    .pointer("/permissions/project_access/access_level")
                    .and_then(|v| v.as_i64())
                    .or_else(|| {
                        json.pointer("/permissions/group_access/access_level")
                            .and_then(|v| v.as_i64())
                    });
                ctx.viewer_permission = access.map(|lvl| match lvl {
                    50 => "OWNER".to_string(),
                    40 => "MAINTAINER".to_string(),
                    30 => "DEVELOPER".to_string(),
                    20 => "REPORTER".to_string(),
                    _ => "GUEST".to_string(),
                });
            }
            Err(err) => {
                ctx.error = Some(err.to_string());
            }
        }

        if ctx.has_upstream {
            if let Some(ref default_branch) = ctx.default_branch {
                let upstream_ref = format!("upstream/{}", default_branch);
                let (ahead, behind) = git_rev_list_counts(project.path, "HEAD", &upstream_ref);
                ctx.ahead_upstream_count = ahead;
                ctx.behind_upstream_count = behind;
            }
        }

        Ok(ctx)
    }

    fn list_issues(
        &self,
        project: ForgeProjectRef<'_>,
        query: &ForgeIssueQuery,
    ) -> Result<Vec<ForgeIssueSummary>, ForgeError> {
        let (host, _repo, project_id, _) = resolve_gitlab_project(project.path)?;
        let state = map_glab_issue_state(query.state.as_deref());
        let mut endpoint = format!("projects/{}/issues?state={}&per_page=50", project_id, state);
        if let Some(label) = query.label.as_deref().filter(|s| !s.trim().is_empty()) {
            endpoint.push_str(&format!("&labels={}", percent_encode_component(label.trim())));
        }
        if let Some(search) = query.search.as_deref().filter(|s| !s.trim().is_empty()) {
            endpoint.push_str(&format!("&search={}", percent_encode_component(search.trim())));
        }

        let json = run_glab_api(Some(&host), &endpoint, "list_issues")?;
        let arr = json.as_array().ok_or_else(|| {
            ForgeError::InvalidProviderResponse("GitLab issues response is not an array".to_string())
        })?;

        Ok(arr
            .iter()
            .filter_map(|item| {
                let number = item.get("iid").and_then(|v| v.as_i64())?;
                let assignees_empty = item
                    .get("assignees")
                    .and_then(|v| v.as_array())
                    .map(|a| a.is_empty())
                    .unwrap_or(true);
                Some(ForgeIssueSummary {
                    number,
                    title: item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    state: normalize_gitlab_state(
                        item.get("state").and_then(|v| v.as_str()).unwrap_or("opened"),
                    ),
                    author: item
                        .pointer("/author/username")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    labels: extract_labels(item),
                    comments_count: item
                        .get("user_notes_count")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize,
                    is_assigned: !assignees_empty || item.get("assignee").map(|a| !a.is_null()).unwrap_or(false),
                    url: item.get("web_url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    created_at: item.get("created_at").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    updated_at: item.get("updated_at").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                })
            })
            .collect())
    }

    fn get_issue(
        &self,
        project: ForgeProjectRef<'_>,
        number: i64,
        _force_refresh: bool,
    ) -> Result<ForgeIssueDetail, ForgeError> {
        let (host, _repo, project_id, _) = resolve_gitlab_project(project.path)?;
        let issue_ep = format!("projects/{}/issues/{}", project_id, number);
        let item = run_glab_api(Some(&host), &issue_ep, "get_issue")?;
        let notes_ep = format!("projects/{}/issues/{}/notes?per_page=100", project_id, number);
        let notes = run_glab_api(Some(&host), &notes_ep, "get_issue_notes")?;

        Ok(ForgeIssueDetail {
            number: item.get("iid").and_then(|v| v.as_i64()).unwrap_or(number),
            title: item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            state: normalize_gitlab_state(
                item.get("state").and_then(|v| v.as_str()).unwrap_or("opened"),
            ),
            author: item
                .pointer("/author/username")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            labels: extract_labels(&item),
            url: item.get("web_url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            body: item.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            created_at: item.get("created_at").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            updated_at: item.get("updated_at").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            comments: map_glab_notes(&notes),
        })
    }

    fn list_pull_requests(
        &self,
        project: ForgeProjectRef<'_>,
        query: &ForgePullRequestQuery,
    ) -> Result<Vec<ForgePullRequestSummary>, ForgeError> {
        let (host, _repo, project_id, _) = resolve_gitlab_project(project.path)?;
        let state = map_glab_mr_state(query.state.as_deref());
        let mut endpoint = format!(
            "projects/{}/merge_requests?state={}&per_page=50",
            project_id, state
        );
        if query.author_me {
            let user_json = run_glab_api(Some(&host), "user", "get_current_user")?;
            if let Some(uid) = user_json.get("id").and_then(|v| v.as_i64()) {
                endpoint.push_str(&format!("&author_id={}", uid));
            }
        }

        let json = run_glab_api(Some(&host), &endpoint, "list_merge_requests")?;
        let arr = json.as_array().ok_or_else(|| {
            ForgeError::InvalidProviderResponse(
                "GitLab merge_requests response is not an array".to_string(),
            )
        })?;

        Ok(arr
            .iter()
            .filter_map(|item| {
                let number = item.get("iid").and_then(|v| v.as_i64())?;
                let is_draft = item.get("draft").and_then(|v| v.as_bool()).unwrap_or(false)
                    || item
                        .get("work_in_progress")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                let mergeable = item
                    .get("detailed_merge_status")
                    .and_then(|v| v.as_str())
                    .or_else(|| item.get("merge_status").and_then(|v| v.as_str()))
                    .unwrap_or("unknown")
                    .to_string();
                let pipeline_status = item
                    .pointer("/head_pipeline/status")
                    .and_then(|v| v.as_str());
                Some(ForgePullRequestSummary {
                    number,
                    title: item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    state: normalize_gitlab_state(
                        item.get("state").and_then(|v| v.as_str()).unwrap_or("opened"),
                    ),
                    is_draft,
                    author: item
                        .pointer("/author/username")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    head_ref_name: item
                        .get("source_branch")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    base_ref_name: item
                        .get("target_branch")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    mergeable,
                    checks_summary: map_checks_summary(pipeline_status),
                    url: item.get("web_url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    created_at: item.get("created_at").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    updated_at: item.get("updated_at").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                })
            })
            .collect())
    }

    fn get_pull_request(
        &self,
        project: ForgeProjectRef<'_>,
        number: i64,
        _force_refresh: bool,
    ) -> Result<ForgePullRequestDetail, ForgeError> {
        let (host, _repo, project_id, _) = resolve_gitlab_project(project.path)?;
        let mr_ep = format!("projects/{}/merge_requests/{}", project_id, number);
        let item = run_glab_api(Some(&host), &mr_ep, "get_merge_request")?;
        let notes_ep = format!(
            "projects/{}/merge_requests/{}/notes?per_page=100",
            project_id, number
        );
        let notes = run_glab_api(Some(&host), &notes_ep, "get_merge_request_notes")?;
        let changes_ep = format!("projects/{}/merge_requests/{}/changes", project_id, number);
        let changed_files_count = run_glab_api(Some(&host), &changes_ep, "get_merge_request_changes")
            .ok()
            .and_then(|c| c.get("changes").and_then(|a| a.as_array()).map(|a| a.len()))
            .unwrap_or(0);

        let is_draft = item.get("draft").and_then(|v| v.as_bool()).unwrap_or(false)
            || item
                .get("work_in_progress")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
        let mergeable = item
            .get("detailed_merge_status")
            .and_then(|v| v.as_str())
            .or_else(|| item.get("merge_status").and_then(|v| v.as_str()))
            .unwrap_or("unknown")
            .to_string();

        let reviews: Vec<ForgeReviewItem> = item
            .get("approved_by")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|entry| {
                        let author = entry
                            .pointer("/user/username")
                            .and_then(|v| v.as_str())
                            .or_else(|| entry.get("username").and_then(|v| v.as_str()))?;
                        Some(ForgeReviewItem {
                            author: author.to_string(),
                            state: "APPROVED".to_string(),
                            body: String::new(),
                            submitted_at: String::new(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let checks = item
            .get("head_pipeline")
            .and_then(|hp| hp.as_object())
            .map(|hp| {
                vec![ForgeCheckItem {
                    name: "head_pipeline".to_string(),
                    status: hp
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    conclusion: hp
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                }]
            })
            .unwrap_or_default();

        Ok(ForgePullRequestDetail {
            number: item.get("iid").and_then(|v| v.as_i64()).unwrap_or(number),
            title: item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            state: normalize_gitlab_state(
                item.get("state").and_then(|v| v.as_str()).unwrap_or("opened"),
            ),
            is_draft,
            author: item
                .pointer("/author/username")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            head_ref_name: item
                .get("source_branch")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            base_ref_name: item
                .get("target_branch")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            mergeable,
            url: item.get("web_url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            body: item.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            changed_files_count,
            additions_count: 0,
            deletions_count: 0,
            comments: map_glab_notes(&notes),
            reviews,
            checks,
        })
    }

    fn list_releases(
        &self,
        project: ForgeProjectRef<'_>,
        _force_refresh: bool,
    ) -> Result<Vec<ForgeReleaseSummary>, ForgeError> {
        let (host, _repo, project_id, _) = resolve_gitlab_project(project.path)?;
        let endpoint = format!("projects/{}/releases?per_page=50", project_id);
        let json = run_glab_api(Some(&host), &endpoint, "list_releases")?;
        let arr = json.as_array().ok_or_else(|| {
            ForgeError::InvalidProviderResponse("GitLab releases response is not an array".to_string())
        })?;

        Ok(arr
            .iter()
            .map(|item| ForgeReleaseSummary {
                tag_name: item.get("tag_name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                name: item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                is_draft: false,
                is_prerelease: item
                    .get("upcoming_release")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                published_at: item
                    .get("released_at")
                    .and_then(|v| v.as_str())
                    .or_else(|| item.get("created_at").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_string(),
                url: item
                    .pointer("/_links/self")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
            .collect())
    }

    fn create_draft_pull_request(
        &self,
        _project: ForgeProjectRef<'_>,
        _request: &ForgeDraftPullRequest,
    ) -> Result<String, ForgeError> {
        Err(ForgeError::UnsupportedCapability {
            provider: ForgeProviderKind::Gitlab,
            operation: "create_draft_pull_request".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Mutex;

    static PATH_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn extracts_gitlab_host_and_nested_group_path() {
        assert_eq!(
            extract_gitlab_host_and_path("https://gitlab.com/group/subgroup/repo.git"),
            Some(("gitlab.com".to_string(), "group/subgroup/repo".to_string()))
        );
        assert_eq!(
            extract_gitlab_host_and_path("git@gitlab.corp.local:team/proj.git"),
            Some(("gitlab.corp.local".to_string(), "team/proj".to_string()))
        );
        assert_eq!(
            extract_gitlab_host_and_path("ssh://git@gitlab.example.com:2222/a/b"),
            Some(("gitlab.example.com".to_string(), "a/b".to_string()))
        );
    }

    #[test]
    fn fake_glab_lists_issues_mrs_and_releases_and_maps_auth_errors() {
        let _guard = PATH_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("oneship_fake_glab_test_{}", std::process::id()));
        let bin_dir = dir.join("bin");
        let repo_dir = dir.join("repo");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&bin_dir).unwrap();
        std::fs::create_dir_all(&repo_dir).unwrap();

        let init = Command::new("git").args(["init"]).current_dir(&repo_dir).output().unwrap();
        assert!(init.status.success());
        let remote = Command::new("git")
            .args(["remote", "add", "origin", "https://gitlab.com/group/sub/app.git"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        assert!(remote.status.success());

        let script = r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" != "api" ]]; then
  exit 2
fi
ep="${2:-}"
if [[ "${FAKE_GLAB_UNAUTH:-0}" == "1" ]]; then
  echo "401 Unauthorized: glab auth login required" >&2
  exit 1
fi
case "$ep" in
  "projects/group%2Fsub%2Fapp")
    echo '{"id":42,"default_branch":"main","permissions":{"project_access":{"access_level":40}}}'
    ;;
  projects/42/issues\?*)
    echo '[{"iid":7,"title":"Fix bug","state":"opened","author":{"username":"alice"},"labels":["bug"],"user_notes_count":3,"assignees":[{"username":"alice"}],"web_url":"https://gitlab.com/group/sub/app/-/issues/7","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-02T00:00:00Z"}]'
    ;;
  projects/42/merge_requests\?*)
    echo '[{"iid":12,"title":"Draft MR","state":"opened","draft":true,"author":{"username":"bob"},"source_branch":"feat","target_branch":"main","detailed_merge_status":"mergeable","head_pipeline":{"status":"success"},"web_url":"https://gitlab.com/group/sub/app/-/merge_requests/12","created_at":"2026-01-03T00:00:00Z","updated_at":"2026-01-04T00:00:00Z"}]'
    ;;
  "projects/42/releases?per_page=50")
    echo '[{"tag_name":"v1.0.0","name":"Release 1.0","upcoming_release":false,"released_at":"2026-01-05T00:00:00Z","_links":{"self":"https://gitlab.com/group/sub/app/-/releases/v1.0.0"}}]'
    ;;
  *)
    echo "unexpected endpoint: $ep" >&2
    exit 1
    ;;
esac
"#;
        let glab_path = bin_dir.join("glab");
        std::fs::write(&glab_path, script).unwrap();
        let mut perms = std::fs::metadata(&glab_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&glab_path, perms).unwrap();

        let prev_path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{}", bin_dir.display(), prev_path));
        std::env::remove_var("FAKE_GLAB_UNAUTH");

        let provider = GitlabForgeProvider;
        let proj = ForgeProjectRef {
            project_id: 1,
            path: &repo_dir,
        };

        let ctx = provider.collab_context(proj, false).unwrap();
        assert_eq!(ctx.provider, ForgeProviderKind::Gitlab);
        assert_eq!(ctx.pull_request_label, "Merge Requests");
        assert_eq!(ctx.default_branch.as_deref(), Some("main"));
        assert!(!ctx.supports_draft_pull_request);

        let issues = provider.list_issues(proj, &ForgeIssueQuery::default()).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].number, 7);
        assert_eq!(issues[0].state, "open");
        assert_eq!(issues[0].comments_count, 3);
        assert!(issues[0].is_assigned);

        let mrs = provider
            .list_pull_requests(proj, &ForgePullRequestQuery::default())
            .unwrap();
        assert_eq!(mrs.len(), 1);
        assert_eq!(mrs[0].number, 12);
        assert!(mrs[0].is_draft);
        assert_eq!(mrs[0].checks_summary, "passing");

        let releases = provider.list_releases(proj, false).unwrap();
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].tag_name, "v1.0.0");

        let draft_err = provider
            .create_draft_pull_request(
                proj,
                &ForgeDraftPullRequest {
                    target_repo: "group/sub/app".into(),
                    base_branch: "main".into(),
                    head_branch: "feat".into(),
                    title: "t".into(),
                    body: "b".into(),
                },
            )
            .unwrap_err();
        assert!(matches!(draft_err, ForgeError::UnsupportedCapability { .. }));

        std::env::set_var("FAKE_GLAB_UNAUTH", "1");
        let auth_err = provider.list_issues(proj, &ForgeIssueQuery::default()).unwrap_err();
        assert!(matches!(auth_err, ForgeError::ProviderUnavailable { .. }));

        std::env::remove_var("FAKE_GLAB_UNAUTH");
        std::env::set_var("PATH", prev_path);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
