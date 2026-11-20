use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, LazyLock, Mutex};
use serde::{Deserialize, Serialize};

use crate::config::ConfigError;
use crate::kopeng::snapshots::SnapshotError;
use crate::registry::RegistryError;

/// Upper bound of `get_project_git_log` (`limit_count`).
pub const MAX_GIT_LOG_ITEMS_COUNT: usize = 200;

/// Failure of a git inspection or VCS configuration operation.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("Project '{0}' has no git repository")]
    NoRepository(String),
    #[error("'{0}' is not a commit hash (expected 7 to 64 hexadecimal characters)")]
    InvalidCommitHash(String),
    #[error("Git log limit must be between 1 and {max}, got {requested}")]
    InvalidLogLimit { requested: usize, max: usize },
    #[error("git {operation} failed: {details}")]
    Command { operation: String, details: String },
    #[error("Unexpected output of git {operation}: {details}")]
    UnexpectedOutput { operation: String, details: String },
    #[error("Invalid VCS action: {0}")]
    InvalidVcsAction(String),
    #[error("Background task failed: {0}")]
    Task(String),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),
    #[error(transparent)]
    Supervisor(#[from] crate::supervisor::SupervisorError),
}

impl crate::api::ApiError for GitError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::NoRepository(_) => StatusCode::CONFLICT,
            Self::InvalidCommitHash(_) | Self::InvalidLogLimit { .. } | Self::InvalidVcsAction(_) => StatusCode::BAD_REQUEST,
            Self::Command { .. } | Self::UnexpectedOutput { .. } => StatusCode::BAD_GATEWAY,
            Self::Task(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Registry(e) => e.status(),
            Self::Config(e) => e.status(),
            Self::Snapshot(e) => e.status(),
            Self::Supervisor(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::NoRepository(_) => "GitError::NoRepository",
            Self::InvalidCommitHash(_) => "GitError::InvalidCommitHash",
            Self::InvalidLogLimit { .. } => "GitError::InvalidLogLimit",
            Self::Command { .. } => "GitError::Command",
            Self::UnexpectedOutput { .. } => "GitError::UnexpectedOutput",
            Self::InvalidVcsAction(_) => "GitError::InvalidVcsAction",
            Self::Task(_) => "GitError::Task",
            Self::Registry(e) => e.code(),
            Self::Config(e) => e.code(),
            Self::Snapshot(e) => e.code(),
            Self::Supervisor(e) => e.code(),
        }
    }
}

/// Arguments of `save_vcs_action`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SaveVcsActionArgs {
    pub project_id: usize,
    pub name: String,
    pub prompt: String,
}

/// Arguments of `get_project_git_log`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct GetProjectGitLogArgs {
    pub project_id: usize,
    pub limit_count: usize,
}

/// Arguments of `get_git_commit_diff`; `file_path` limits the diff to one file.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct GetGitCommitDiffArgs {
    pub project_id: usize,
    pub commit_hash: String,
    pub file_path: Option<String>,
}

/// Arguments of `get_commit_files_changed`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct GetCommitFilesChangedArgs {
    pub project_id: usize,
    pub commit_hash: String,
}

/// Arguments of `get_working_tree_diff`; `file_path` limits the diff to one file.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct GetWorkingTreeDiffArgs {
    pub project_id: usize,
    pub file_path: Option<String>,
}

fn unexpected(operation: &str, details: impl Into<String>) -> GitError {
    GitError::UnexpectedOutput { operation: operation.to_string(), details: details.into() }
}

/// Runs `git <args>` in `repo` with `GIT_TERMINAL_PROMPT=0` and a deterministic timeout;
/// a non-zero exit is `GitError::Command`.
pub(crate) fn git(repo: &Path, args: &[&str]) -> Result<Vec<u8>, GitError> {
    let operation = args.first().copied().unwrap_or_default().to_string();
    let mut cmd = Command::new("git");
    cmd.env("GIT_TERMINAL_PROMPT", "0")
        .args(args)
        .current_dir(repo);
    let timeout_seconds =
        crate::config::resolve_effective_u64(None, None, "vcs", "forge_cli_timeout_seconds").max(1);
    let out = crate::kopeng::forge::run_sync_command_with_timeout(
        cmd,
        timeout_seconds,
        &format!("git {operation}"),
    )
    .map_err(|e| GitError::Command {
        operation: operation.clone(),
        details: format!("cannot run git: {}", e),
    })?;
    if !out.status.success() {
        return Err(GitError::Command {
            operation,
            details: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }
    Ok(out.stdout)
}

/// NUL-separated records of a `-z` git output (the trailing empty record is dropped).
fn z_records(stdout: &[u8]) -> Vec<String> {
    stdout
        .split(|b| *b == 0)
        .filter(|r| !r.is_empty())
        .map(|r| String::from_utf8_lossy(r).into_owned())
        .collect()
}

/// Ensures the given directory contains a Git repository.
pub(crate) fn require_repository(project_path: &Path) -> Result<(), GitError> {
    if project_path.join(".git").exists() {
        return Ok(());
    }
    Err(GitError::NoRepository(project_path.display().to_string()))
}

fn require_commit_hash(hash: &str) -> Result<(), GitError> {
    if (7..=64).contains(&hash.len()) && hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(());
    }
    Err(GitError::InvalidCommitHash(hash.to_string()))
}

/// Runs `work` on the registered project's directory in a blocking task.
async fn on_project<T: Send + 'static>(
    project_id: usize,
    work: impl FnOnce(&Path) -> Result<T, GitError> + Send + 'static,
) -> Result<T, GitError> {
    tokio::task::spawn_blocking(move || {
        let project = crate::registry::find_project_by_id(project_id)?;
        work(Path::new(&project.path))
    })
    .await
    .map_err(|e| GitError::Task(e.to_string()))?
}

/// Forge provider inferred from a Git remote URL.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ForgeProviderKind {
    Github,
    Gitlab,
    Unknown,
}

/// Configured Git remote entry with its classified Forge provider.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GitRemote {
    pub name: String,
    pub url: String,
    pub provider: ForgeProviderKind,
}

/// Classifies a Git remote URL into a [`ForgeProviderKind`] without guessing unknown self-hosted hosts.
pub fn classify_remote_provider(url: &str) -> ForgeProviderKind {
    let host = extract_remote_host(url);
    let Some(host) = host else {
        return ForgeProviderKind::Unknown;
    };
    let lower = host.to_ascii_lowercase();
    if lower == "github.com" || lower == "www.github.com" {
        return ForgeProviderKind::Github;
    }
    if lower == "gitlab.com" || lower.contains("gitlab") {
        return ForgeProviderKind::Gitlab;
    }
    ForgeProviderKind::Unknown
}

fn extract_remote_host(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(after_scheme) = trimmed.split_once("://").map(|(_, rest)| rest) {
        let authority = after_scheme.split('/').next()?;
        let host_port = authority.rsplit_once('@').map(|(_, h)| h).unwrap_or(authority);
        let host = host_port.split(':').next().unwrap_or(host_port);
        return (!host.is_empty()).then_some(host);
    }
    // SCP-style SSH URL: `git@github.com:owner/repo.git`
    if let Some((user_host, _path)) = trimmed.split_once(':') {
        let host = user_host.rsplit_once('@').map(|(_, h)| h).unwrap_or(user_host);
        return (!host.is_empty() && !host.contains('/')).then_some(host);
    }
    None
}

fn parse_git_remotes(stdout: &[u8]) -> Vec<GitRemote> {
    let text = String::from_utf8_lossy(stdout);
    let mut remotes: Vec<GitRemote> = Vec::new();
    let mut seen_names = std::collections::HashSet::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((key, url)) = trimmed.split_once(char::is_whitespace) else {
            continue;
        };
        let Some(name) = key.strip_prefix("remote.").and_then(|s| s.strip_suffix(".url")) else {
            continue;
        };
        let url = url.trim();
        if name.is_empty() || url.is_empty() || !seen_names.insert(name.to_string()) {
            continue;
        }
        remotes.push(GitRemote {
            name: name.to_string(),
            url: url.to_string(),
            provider: classify_remote_provider(url),
        });
    }
    remotes
}

/// Repository summary shown on project cards (`HydratedProject::git_status`).
#[derive(Serialize, Clone, Debug, Default, PartialEq, ts_rs::TS)]
#[ts(export)]
pub struct GitStatus {
    /// Checked-out branch (also for a branch without commits); `None` when HEAD is detached.
    pub branch: Option<String>,
    pub has_remote: bool,
    pub remote_url: Option<String>,
    pub remotes: Vec<GitRemote>,
    pub is_clean: bool,
    pub has_commits: bool,
    pub staged_count: usize,
    pub modified_count: usize,
    pub untracked_count: usize,
    pub ahead_count: usize,
    pub behind_count: usize,
}

/// Branch, upstream, divergence and change counts of the repository at `path`.
///
/// Reads `git status --porcelain=v2 --branch -z` and all configured remotes; any git failure or
/// unparsable record is an error, never a default value.
pub fn get_git_status(path: &Path) -> Result<GitStatus, GitError> {
    let mut status = parse_status_v2(&git(path, &["status", "--porcelain=v2", "--branch", "-z"])?)?;

    // `git config --get-regexp` exits 1 when no remote.*.url keys exist.
    let remote = Command::new("git")
        .args(["config", "--get-regexp", r"^remote\..*\.url$"])
        .current_dir(path)
        .output()
        .map_err(|e| GitError::Command { operation: "config".to_string(), details: format!("cannot run git: {}", e) })?;
    match remote.status.code() {
        Some(0) => {
            let remotes = parse_git_remotes(&remote.stdout);
            status.remote_url = remotes
                .iter()
                .find(|r| r.name == "origin")
                .map(|r| r.url.clone());
            status.has_remote = status.has_remote || !remotes.is_empty();
            status.remotes = remotes;
        }
        Some(1) => {}
        _ => return Err(GitError::Command { operation: "config".to_string(), details: String::from_utf8_lossy(&remote.stderr).trim().to_string() }),
    }

    crate::logger::log(
        "DEBUG",
        "git",
        &format!(
            "Git status for {}: branch={:?}, is_clean={}, staged={}, modified={}, untracked={}",
            path.display(), status.branch, status.is_clean, status.staged_count, status.modified_count, status.untracked_count
        ),
    );
    Ok(status)
}

fn parse_status_v2(stdout: &[u8]) -> Result<GitStatus, GitError> {
    const OP: &str = "status";
    let mut status = GitStatus::default();
    let mut records = z_records(stdout).into_iter();
    while let Some(record) = records.next() {
        let (kind, rest) = record.split_once(' ').ok_or_else(|| unexpected(OP, format!("record '{}'", record)))?;
        match kind {
            "#" => parse_branch_header(rest, &mut status)?,
            "?" => status.untracked_count += 1,
            "!" => {}
            "1" | "2" | "u" => {
                let xy = rest.as_bytes();
                if xy.len() < 2 {
                    return Err(unexpected(OP, format!("record '{}'", record)));
                }
                if kind == "u" {
                    status.modified_count += 1;
                } else {
                    status.staged_count += usize::from(xy[0] != b'.');
                    status.modified_count += usize::from(xy[1] != b'.');
                }
                // A rename/copy entry is followed by its original path.
                if kind == "2" && records.next().is_none() {
                    return Err(unexpected(OP, format!("rename record '{}' has no original path", record)));
                }
            }
            other => return Err(unexpected(OP, format!("record type '{}'", other))),
        }
    }
    status.is_clean = status.staged_count == 0 && status.modified_count == 0 && status.untracked_count == 0;
    Ok(status)
}

fn parse_branch_header(header: &str, status: &mut GitStatus) -> Result<(), GitError> {
    const OP: &str = "status";
    let (key, value) = header.split_once(' ').ok_or_else(|| unexpected(OP, format!("header '{}'", header)))?;
    match key {
        "branch.oid" => status.has_commits = value != "(initial)",
        "branch.head" => status.branch = (value != "(detached)").then(|| value.to_string()),
        "branch.upstream" => status.has_remote = true,
        "branch.ab" => {
            let (ahead, behind) = value.split_once(' ').ok_or_else(|| unexpected(OP, format!("header '{}'", header)))?;
            let count = |v: &str, sign: char| v.strip_prefix(sign).and_then(|n| n.parse::<usize>().ok());
            status.ahead_count = count(ahead, '+').ok_or_else(|| unexpected(OP, format!("header '{}'", header)))?;
            status.behind_count = count(behind, '-').ok_or_else(|| unexpected(OP, format!("header '{}'", header)))?;
        }
        // Other headers (e.g. `stash`) carry nothing GitStatus reports.
        _ => {}
    }
    Ok(())
}

/// Runs `git init` in `path` (an existing directory). Adapting `.gitignore` / `README.md` is the
/// initializer agent's task, never a template written here.
pub(crate) fn init_repository(path: &Path) -> Result<(), GitError> {
    crate::logger::log("INFO", "git", &format!("Initializing git repository at {}", path.display()));
    git(path, &["init"]).inspect_err(|e| {
        crate::logger::log("ERROR", "git", &format!("Failed to initialize git repository at {}: {}", path.display(), e));
    })?;
    Ok(())
}

/// Runs `git init` when the project has no repository yet.
pub(crate) fn ensure_repository(path: &Path) -> Result<(), GitError> {
    if !path.join(".git").exists() {
        init_repository(path)?;
    }
    Ok(())
}

/// Whether `origin` is configured (`git config --get` exits 1 when the key is absent).
pub(crate) fn has_origin(path: &Path) -> Result<bool, GitError> {
    let out = Command::new("git")
        .args(["config", "--get", "remote.origin.url"])
        .current_dir(path)
        .output()
        .map_err(|e| GitError::Command { operation: "config".to_string(), details: format!("cannot run git: {}", e) })?;
    match out.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(GitError::Command { operation: "config".to_string(), details: String::from_utf8_lossy(&out.stderr).trim().to_string() }),
    }
}

/// Initializes git in a registered project.
pub async fn init_git_repo(project_id: usize) -> Result<(), GitError> {
    crate::logger::log("DEBUG", "git", &format!("init_git_repo: project_id={}", project_id));
    on_project(project_id, init_repository).await
}

/// A custom VCS action declared in `rula.toml` (`[[plugins.vcs.actions]]`).
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct VcsAction {
    pub name: String,
    pub prompt: String,
}

/// `plugins.vcs.actions` of a parsed `rula.toml`; absent tables are an empty list.
fn vcs_actions_of(rula: &toml::Value) -> Result<Vec<VcsAction>, GitError> {
    let Some(actions) = rula.get("plugins").and_then(|p| p.get("vcs")).and_then(|v| v.get("actions")) else {
        return Ok(Vec::new());
    };
    actions
        .clone()
        .try_into()
        .map_err(|e: toml::de::Error| GitError::InvalidVcsAction(format!("plugins.vcs.actions: {}", e)))
}

/// Custom VCS actions of a registered project; a project without `rula.toml` has none, an
/// invalid one is an error.
pub async fn get_vcs_actions(project_id: usize) -> Result<Vec<VcsAction>, GitError> {
    crate::logger::log("DEBUG", "git", &format!("get_vcs_actions: project_id={}", project_id));
    on_project(project_id, |path| match crate::config::read_project_rula(path)? {
        Some(rula) => vcs_actions_of(&rula),
        None => Ok(Vec::new()),
    })
    .await
}

/// Appends `action` to `plugins.vcs.actions`; the name must be unique and both fields non-empty.
fn add_vcs_action(rula: &mut toml::Value, action: VcsAction) -> Result<(), GitError> {
    if action.name.is_empty() || action.prompt.is_empty() {
        return Err(GitError::InvalidVcsAction("name and prompt must not be empty".to_string()));
    }
    if vcs_actions_of(rula)?.iter().any(|a| a.name == action.name) {
        return Err(GitError::InvalidVcsAction(format!("an action named '{}' already exists", action.name)));
    }
    let not_a = |key: &str, kind: &str| GitError::InvalidVcsAction(format!("'{}' is not {}", key, kind));
    let root = rula.as_table_mut().ok_or_else(|| not_a("rula.toml", "a table"))?;
    let plugins = root
        .entry("plugins")
        .or_insert_with(|| toml::Value::Table(Default::default()))
        .as_table_mut()
        .ok_or_else(|| not_a("plugins", "a table"))?;
    let vcs = plugins
        .entry("vcs")
        .or_insert_with(|| toml::Value::Table(Default::default()))
        .as_table_mut()
        .ok_or_else(|| not_a("plugins.vcs", "a table"))?;
    let actions = vcs
        .entry("actions")
        .or_insert_with(|| toml::Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| not_a("plugins.vcs.actions", "an array"))?;
    let mut entry = toml::map::Map::new();
    entry.insert("name".to_string(), toml::Value::String(action.name));
    entry.insert("prompt".to_string(), toml::Value::String(action.prompt));
    actions.push(toml::Value::Table(entry));
    Ok(())
}

/// Adds a custom VCS action to the project's valid `rula.toml` (serialized and validated before
/// it is written).
pub async fn save_vcs_action(project_id: usize, name: String, prompt: String) -> Result<(), GitError> {
    crate::logger::log("DEBUG", "git", &format!("save_vcs_action: project_id={}, name='{}', prompt_len={}", project_id, name, prompt.len()));
    on_project(project_id, move |path| {
        let mut rula = crate::config::read_project_rula(path)?.ok_or_else(|| {
            GitError::InvalidVcsAction(format!("{} has no rula.toml; initialize the project first", path.display()))
        })?;
        let name = name.trim().to_string();
        add_vcs_action(&mut rula, VcsAction { name: name.clone(), prompt: prompt.trim().to_string() })?;
        crate::config::write_project_rula(path, &rula)?;
        crate::logger::log("INFO", "vcs", &format!("Added custom action '{}' to {}/rula.toml", name, path.display()));
        Ok(())
    })
    .await
}

/// Metadata for a single Git commit returned by `git log`.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct GitCommitInfo {
    pub hash: String,
    pub short_hash: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: String,
    pub message: String,
}

/// The last `limit_count` commits (1..=`MAX_GIT_LOG_ITEMS_COUNT`); a repository without commits
/// has an empty log.
pub async fn get_project_git_log(project_id: usize, limit_count: usize) -> Result<Vec<GitCommitInfo>, GitError> {
    crate::logger::log("DEBUG", "git", &format!("get_project_git_log: project_id={}, limit_count={}", project_id, limit_count));
    if !(1..=MAX_GIT_LOG_ITEMS_COUNT).contains(&limit_count) {
        return Err(GitError::InvalidLogLimit { requested: limit_count, max: MAX_GIT_LOG_ITEMS_COUNT });
    }
    on_project(project_id, move |path| {
        require_repository(path)?;
        if crate::kopeng::snapshots::head_commit(path)?.is_none() {
            return Ok(Vec::new());
        }
        let stdout = git(path, &["log", &format!("-n{}", limit_count), "-z", "--format=%H%x1f%h%x1f%an%x1f%ae%x1f%aI%x1f%s"])?;
        parse_git_log(&stdout, limit_count)
    })
    .await
}

fn parse_git_log(stdout: &[u8], limit_count: usize) -> Result<Vec<GitCommitInfo>, GitError> {
    let mut commits = Vec::with_capacity(limit_count);
    for record in z_records(stdout) {
        let fields: Vec<&str> = record.trim_start_matches('\n').split('\x1f').collect();
        let [hash, short_hash, author_name, author_email, timestamp, message] = fields[..] else {
            return Err(unexpected("log", format!("record with {} fields", fields.len())));
        };
        commits.push(GitCommitInfo {
            hash: hash.to_string(),
            short_hash: short_hash.to_string(),
            author_name: author_name.to_string(),
            author_email: author_email.to_string(),
            timestamp: timestamp.to_string(),
            message: message.to_string(),
        });
    }
    Ok(commits)
}

/// Patch introduced by `commit_hash`, optionally scoped to `file_path`.
pub async fn get_git_commit_diff(project_id: usize, commit_hash: String, file_path: Option<String>) -> Result<String, GitError> {
    crate::logger::log("DEBUG", "git", &format!("get_git_commit_diff: project_id={}, commit='{}', file={:?}", project_id, commit_hash, file_path));
    require_commit_hash(&commit_hash)?;
    on_project(project_id, move |path| {
        require_repository(path)?;
        let mut args = vec!["show", "--format=", "--patch", "--end-of-options", commit_hash.as_str()];
        if let Some(f) = file_path.as_deref() {
            args.extend(["--", f]);
        }
        Ok(String::from_utf8_lossy(&git(path, &args)?).into_owned())
    })
    .await
}

/// File path and change status inside a historical Git commit.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct CommitFileChange {
    pub path: String,
    /// `added` | `deleted` | `modified` | `renamed` | `copied` | `type_changed`
    pub status: String,
    /// `A` | `D` | `M` | `R` | `C` | `T`
    pub status_code: char,
    pub is_domain_asset: bool,
}

/// Uncommitted file entry in the repository working tree or staging index.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct WorkingTreeFile {
    pub path: String,
    /// `untracked` | `deleted` | `added` | `renamed` | `modified`
    pub status: String,
    /// Porcelain v1 index column (`M`, `A`, `D`, `R`, `C`, `T`, `U`, `?`, ` `).
    pub staged_code: char,
    /// Porcelain v1 working-tree column (`M`, `D`, `T`, `U`, `?`, ` `).
    pub unstaged_code: char,
    pub is_staged: bool,
    pub is_deleted: bool,
    pub is_untracked: bool,
    pub exists_on_disk: bool,
    pub is_domain_asset: bool,
    pub old_path: Option<String>,
}

/// Files changed by `commit_hash` (a root commit lists all of its files).
pub async fn get_commit_files_changed(project_id: usize, commit_hash: String) -> Result<Vec<CommitFileChange>, GitError> {
    crate::logger::log("DEBUG", "git", &format!("get_commit_files_changed: project_id={}, commit='{}'", project_id, commit_hash));
    require_commit_hash(&commit_hash)?;
    on_project(project_id, move |path| {
        require_repository(path)?;
        let stdout = git(path, &["diff-tree", "--root", "--no-commit-id", "--name-status", "-r", "-z", "--end-of-options", commit_hash.as_str()])?;
        let mut files = parse_name_status(&stdout)?;
        for file in &mut files {
            file.is_domain_asset = crate::kopeng::snapshots::is_domain_asset_path(path, &file.path);
        }
        Ok(files)
    })
    .await
}

fn parse_name_status(stdout: &[u8]) -> Result<Vec<CommitFileChange>, GitError> {
    const OP: &str = "diff-tree";
    let mut files = Vec::new();
    let mut records = z_records(stdout).into_iter();
    while let Some(code) = records.next() {
        let status_code = code.chars().next().ok_or_else(|| unexpected(OP, "empty status"))?;
        let status = match status_code {
            'A' => "added",
            'D' => "deleted",
            'M' => "modified",
            'R' => "renamed",
            'C' => "copied",
            'T' => "type_changed",
            other => return Err(unexpected(OP, format!("status '{}'", other))),
        };
        let mut path = records.next().ok_or_else(|| unexpected(OP, format!("status '{}' has no path", code)))?;
        // Renames and copies list the source path first, then the destination.
        if matches!(status_code, 'R' | 'C') {
            path = records.next().ok_or_else(|| unexpected(OP, format!("status '{}' has no destination path", code)))?;
        }
        files.push(CommitFileChange { path, status: status.to_string(), status_code, is_domain_asset: false });
    }
    Ok(files)
}

/// Working-tree patch against HEAD (untracked files included), optionally scoped to `file_path`.
pub async fn get_working_tree_diff(project_id: usize, file_path: Option<String>) -> Result<String, GitError> {
    crate::logger::log("DEBUG", "git", &format!("get_working_tree_diff: project_id={}, file={:?}", project_id, file_path));
    on_project(project_id, move |path| {
        require_repository(path)?;
        Ok(crate::kopeng::snapshots::diff_working_tree(path, file_path.as_deref())?)
    })
    .await
}

/// Changed, staged and untracked files of the working tree.
pub async fn get_working_tree_files(project_id: usize) -> Result<Vec<WorkingTreeFile>, GitError> {
    crate::logger::log("DEBUG", "git", &format!("get_working_tree_files: project_id={}", project_id));
    on_project(project_id, |path| {
        require_repository(path)?;
        let stdout = git(path, &["status", "--porcelain=v1", "-z", "-uall"])?;
        let mut files = parse_status_v1(&stdout, |p| path.join(p).symlink_metadata().is_ok())?;
        for file in &mut files {
            file.is_domain_asset = crate::kopeng::snapshots::is_domain_asset_path(path, &file.path);
        }
        Ok(files)
    })
    .await
}

fn parse_status_v1(stdout: &[u8], exists: impl Fn(&str) -> bool) -> Result<Vec<WorkingTreeFile>, GitError> {
    const OP: &str = "status";
    let mut files = Vec::new();
    let mut records = z_records(stdout).into_iter();
    while let Some(record) = records.next() {
        let mut chars = record.chars();
        let (Some(staged_code), Some(unstaged_code), Some(' ')) = (chars.next(), chars.next(), chars.next()) else {
            return Err(unexpected(OP, format!("record '{}'", record)));
        };
        let path: String = chars.collect();
        if path.is_empty() {
            return Err(unexpected(OP, format!("record '{}' has no path", record)));
        }
        // With -z a rename/copy entry is followed by its original path.
        let old_path = if matches!(staged_code, 'R' | 'C') || matches!(unstaged_code, 'R' | 'C') {
            Some(records.next().ok_or_else(|| unexpected(OP, format!("rename '{}' has no original path", path)))?)
        } else {
            None
        };

        let exists_on_disk = exists(&path);
        let is_untracked = staged_code == '?' && unstaged_code == '?';
        let is_staged = !matches!(staged_code, ' ' | '?' | '!');
        let is_deleted = staged_code == 'D' || unstaged_code == 'D' || !exists_on_disk;
        let status = if is_untracked {
            "untracked"
        } else if is_deleted {
            "deleted"
        } else if staged_code == 'A' {
            "added"
        } else if old_path.is_some() {
            "renamed"
        } else {
            "modified"
        };
        files.push(WorkingTreeFile {
            path,
            status: status.to_string(),
            staged_code,
            unstaged_code,
            is_staged,
            is_deleted,
            is_untracked,
            exists_on_disk,
            is_domain_asset: false,
            old_path,
        });
    }
    Ok(files)
}

/// Returns a map of `rel_path -> status` (`"modified"`, `"untracked"`, `"added"`, `"renamed"`, `"deleted"`)
/// for the working tree at `path`, or an empty map if `path` is not a Git repository.
pub(crate) fn get_working_tree_status_map(path: &Path) -> Result<std::collections::HashMap<String, String>, GitError> {
    if !path.join(".git").exists() {
        return Ok(std::collections::HashMap::new());
    }
    let stdout = git(path, &["status", "--porcelain=v1", "-z", "-uall"])?;
    let files = parse_status_v1(&stdout, |p| path.join(p).symlink_metadata().is_ok())?;
    let mut map = std::collections::HashMap::with_capacity(files.len());
    for file in files {
        map.insert(file.path, file.status);
    }
    Ok(map)
}

/// Branch entry returned by `list_git_branches`.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GitBranchInfo {
    pub name: String,
    pub is_current: bool,
    pub is_remote: bool,
    pub remote_name: Option<String>,
    pub upstream: Option<String>,
}

/// Arguments of `switch_git_branch`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SwitchGitBranchArgs {
    pub project_id: usize,
    pub branch_name: String,
    pub is_remote: Option<bool>,
}

/// Arguments of `create_git_branch`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CreateGitBranchArgs {
    pub project_id: usize,
    pub branch_name: String,
    pub start_point: Option<String>,
    pub switch: Option<bool>,
}

fn validate_branch_name(path: &Path, name: &str) -> Result<String, GitError> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.starts_with('-') || trimmed.chars().any(char::is_whitespace) {
        return Err(GitError::InvalidVcsAction(format!("Invalid branch name '{}'", name)));
    }
    let out = Command::new("git")
        .args(["check-ref-format", "--branch", trimmed])
        .current_dir(path)
        .output()
        .map_err(|e| GitError::Command {
            operation: "check-ref-format".to_string(),
            details: format!("cannot run git: {}", e),
        })?;
    if !out.status.success() {
        return Err(GitError::InvalidVcsAction(format!(
            "Invalid branch name '{}': {}",
            trimmed,
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(trimmed.to_string())
}

/// Lists local and remote branches of a repository (excluding symbolic `*/HEAD` pointers).
pub async fn list_git_branches(project_id: usize) -> Result<Vec<GitBranchInfo>, GitError> {
    on_project(project_id, |path| {
        require_repository(path)?;
        let raw = git(
            path,
            &[
                "for-each-ref",
                "--format=%(HEAD)\t%(refname:short)\t%(refname)\t%(upstream:short)",
                "refs/heads",
                "refs/remotes",
            ],
        )?;
        let text = String::from_utf8_lossy(&raw);
        let mut branches = Vec::new();
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 4 {
                continue;
            }
            let head_flag = parts[0].trim();
            let short_name = parts[1].trim();
            let full_ref = parts[2].trim();
            let upstream = parts[3].trim();

            if full_ref.ends_with("/HEAD") {
                continue;
            }
            let is_remote = full_ref.starts_with("refs/remotes/");
            let remote_name = if is_remote {
                full_ref
                    .strip_prefix("refs/remotes/")
                    .and_then(|rest| rest.split('/').next())
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            } else {
                None
            };
            branches.push(GitBranchInfo {
                name: short_name.to_string(),
                is_current: head_flag == "*",
                is_remote,
                remote_name,
                upstream: (!upstream.is_empty()).then(|| upstream.to_string()),
            });
        }
        Ok(branches)
    })
    .await
}

/// Switches to an existing local or remote-tracking branch without force-discarding uncommitted changes.
pub async fn switch_git_branch(
    project_id: usize,
    branch_name: String,
    is_remote: Option<bool>,
) -> Result<(), GitError> {
    on_project(project_id, move |path| {
        require_repository(path)?;
        let trimmed = branch_name.trim();
        if trimmed.is_empty() || trimmed.starts_with('-') || trimmed.chars().any(char::is_whitespace) {
            return Err(GitError::InvalidVcsAction(format!("Invalid branch name '{}'", branch_name)));
        }
        if is_remote.unwrap_or(false) {
            git(path, &["switch", "--track", trimmed])?;
        } else {
            git(path, &["switch", trimmed])?;
        }
        Ok(())
    })
    .await?;
    crate::db::notify_projects_updated();
    Ok(())
}

/// Creates a new local branch, optionally switching `HEAD` to it (defaults to `true`).
pub async fn create_git_branch(
    project_id: usize,
    branch_name: String,
    start_point: Option<String>,
    switch: Option<bool>,
) -> Result<String, GitError> {
    let should_switch = switch.unwrap_or(true);
    let created = on_project(project_id, move |path| {
        require_repository(path)?;
        let valid_name = validate_branch_name(path, &branch_name)?;
        if let Some(ref base) = start_point {
            let trimmed_base = base.trim();
            if trimmed_base.is_empty() || trimmed_base.starts_with('-') || trimmed_base.chars().any(char::is_whitespace) {
                return Err(GitError::InvalidVcsAction(format!("Invalid start point '{}'", base)));
            }
            if should_switch {
                git(path, &["switch", "-c", &valid_name, trimmed_base])?;
            } else {
                git(path, &["branch", &valid_name, trimmed_base])?;
            }
        } else if should_switch {
            git(path, &["switch", "-c", &valid_name])?;
        } else {
            git(path, &["branch", &valid_name])?;
        }
        Ok(valid_name)
    })
    .await?;
    crate::db::notify_projects_updated();
    Ok(created)
}

/// Arguments of `stage_git_files` and `unstage_git_files`; when `paths` is `None`, applies to all changes.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct StageGitFilesArgs {
    pub project_id: usize,
    pub paths: Option<Vec<String>>,
}

/// Arguments of `discard_git_files`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct DiscardGitFilesArgs {
    pub project_id: usize,
    pub paths: Vec<String>,
}

/// Arguments of `commit_git_changes`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CommitGitChangesArgs {
    pub project_id: usize,
    pub message: String,
}

/// Sync direction for `sync_git_branch`.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum GitSyncMode {
    Pull,
    Push,
    Sync,
}

/// Arguments of `sync_git_branch`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SyncGitBranchArgs {
    pub project_id: usize,
    pub mode: GitSyncMode,
}

fn validate_rel_paths(paths: &[String]) -> Result<Vec<&str>, GitError> {
    let mut out = Vec::with_capacity(paths.len());
    for raw in paths {
        let p = raw.trim();
        if p.is_empty() || p.starts_with('-') || p.contains('\0') {
            return Err(GitError::InvalidVcsAction(format!("Invalid file path '{}'", raw)));
        }
        if Path::new(p).is_absolute()
            || Path::new(p)
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(GitError::InvalidVcsAction(format!("Path escapes repository root: '{}'", raw)));
        }
        out.push(p);
    }
    Ok(out)
}

fn has_head_commit(repo: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(repo)
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Stages the specified relative paths, or all working tree changes when `paths` is `None`.
pub async fn stage_git_files(project_id: usize, paths: Option<Vec<String>>) -> Result<(), GitError> {
    on_project(project_id, move |path| {
        require_repository(path)?;
        match paths.as_deref() {
            Some(list) if !list.is_empty() => {
                let valid = validate_rel_paths(list)?;
                let mut args = Vec::with_capacity(2 + valid.len());
                args.extend(["add", "--"]);
                args.extend(valid);
                git(path, &args)?;
            }
            Some(_) => {
                return Err(GitError::InvalidVcsAction("No file paths provided to stage".to_string()));
            }
            None => {
                git(path, &["add", "-A"])?;
            }
        }
        Ok(())
    })
    .await?;
    crate::db::notify_projects_updated();
    Ok(())
}

/// Unstages the specified relative paths, or all staged changes when `paths` is `None`.
pub async fn unstage_git_files(project_id: usize, paths: Option<Vec<String>>) -> Result<(), GitError> {
    on_project(project_id, move |path| {
        require_repository(path)?;
        let head_exists = has_head_commit(path);
        match paths.as_deref() {
            Some(list) if !list.is_empty() => {
                let valid = validate_rel_paths(list)?;
                let mut args = Vec::with_capacity(4 + valid.len());
                if head_exists {
                    args.extend(["restore", "--staged", "--"]);
                } else {
                    args.extend(["rm", "--cached", "-r", "--ignore-unmatch", "--"]);
                }
                args.extend(valid);
                git(path, &args)?;
            }
            Some(_) => {
                return Err(GitError::InvalidVcsAction("No file paths provided to unstage".to_string()));
            }
            None => {
                if head_exists {
                    git(path, &["reset", "HEAD", "--"])?;
                } else {
                    git(path, &["rm", "--cached", "-r", "--ignore-unmatch", "."])?;
                }
            }
        }
        Ok(())
    })
    .await?;
    crate::db::notify_projects_updated();
    Ok(())
}

/// Discards working-tree modifications for tracked files and removes untracked files.
pub async fn discard_git_files(project_id: usize, paths: Vec<String>) -> Result<(), GitError> {
    if paths.is_empty() {
        return Err(GitError::InvalidVcsAction("No file paths provided to discard".to_string()));
    }
    on_project(project_id, move |path| {
        require_repository(path)?;
        let valid = validate_rel_paths(&paths)?;
        let stdout = git(path, &["status", "--porcelain=v1", "-z", "-uall"])?;
        let status_entries = parse_status_v1(&stdout, |p| path.join(p).symlink_metadata().is_ok())?;
        let untracked: std::collections::HashSet<&str> = status_entries
            .iter()
            .filter(|e| e.is_untracked)
            .map(|e| e.path.as_str())
            .collect();

        let mut tracked_to_restore = Vec::new();
        for rel in valid {
            if untracked.contains(rel) {
                let full = path.join(rel);
                if full.is_dir() {
                    std::fs::remove_dir_all(&full)
                        .map_err(|e| GitError::Command { operation: "discard".into(), details: e.to_string() })?;
                } else if full.symlink_metadata().is_ok() {
                    std::fs::remove_file(&full)
                        .map_err(|e| GitError::Command { operation: "discard".into(), details: e.to_string() })?;
                }
            } else {
                tracked_to_restore.push(rel);
            }
        }

        if !tracked_to_restore.is_empty() {
            let mut args = Vec::with_capacity(3 + tracked_to_restore.len());
            args.extend(["restore", "--worktree", "--"]);
            args.extend(tracked_to_restore);
            git(path, &args)?;
        }
        Ok(())
    })
    .await?;
    crate::db::notify_projects_updated();
    Ok(())
}

static PROJECT_COMMIT_LOCKS: LazyLock<Mutex<HashMap<i64, Arc<Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn project_commit_lock(project_id: i64) -> Arc<Mutex<()>> {
    let mut map = PROJECT_COMMIT_LOCKS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    map.entry(project_id)
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

/// Validates that `project_path` is eligible for automatic session-minutes commits
/// (`.git` present, branch checked out / non-detached `HEAD`, and at least one initial commit).
/// Returns `Ok(true)` if non-asset code/docs changes exist to commit, `Ok(false)` if clean.
pub fn validate_auto_commit_ready(project_path: &Path) -> Result<bool, GitError> {
    require_repository(project_path)?;
    if git(project_path, &["symbolic-ref", "-q", "HEAD"]).is_err() {
        return Err(GitError::InvalidVcsAction(
            "Cannot auto-commit session minutes on a detached HEAD. Checkout a branch first.".to_string(),
        ));
    }
    if crate::kopeng::snapshots::head_commit(project_path)?.is_none() {
        return Err(GitError::InvalidVcsAction(
            "Cannot auto-commit session minutes before an initial commit exists on this branch. Create an initial commit first.".to_string(),
        ));
    }
    let changed = crate::kopeng::snapshots::detect_changed_code_files(project_path)?;
    Ok(!changed.is_empty())
}

/// Stages all changed code/docs files (resetting any residual index state first) and executes a
/// real `git commit` with `message`. Runs Git hooks; on hook or commit failure the staged index is
/// reset and the exact hook stdout/stderr is returned so the caller can surface and fix it.
pub fn auto_commit_session_minutes(
    project_id: i64,
    project_path: &Path,
    message: &str,
) -> Result<Option<String>, GitError> {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return Err(GitError::InvalidVcsAction("Commit message cannot be empty".to_string()));
    }
    let lock = project_commit_lock(project_id);
    let _guard = lock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    if !validate_auto_commit_ready(project_path)? {
        return Ok(None);
    }

    let code_files = crate::kopeng::snapshots::detect_changed_code_files(project_path)?;
    if code_files.is_empty() {
        return Ok(None);
    }

    // Clear any residual user staging so only the current work cycle's code/docs files are committed.
    git(project_path, &["reset", "-q"])?;

    let mut add_args = Vec::with_capacity(3 + code_files.len());
    add_args.extend(["add", "-A", "--"]);
    add_args.extend(code_files.iter().map(String::as_str));
    git(project_path, &add_args)?;

    let staged = git(project_path, &["diff", "--cached", "--name-only"])?;
    if String::from_utf8_lossy(&staged).trim().is_empty() {
        return Ok(None);
    }

    let timeout_seconds =
        crate::config::resolve_effective_u64(None, None, "vcs", "forge_cli_timeout_seconds").max(1);
    let mut cmd = Command::new("git");
    cmd.env("GIT_TERMINAL_PROMPT", "0")
        .args(["commit", "-m", trimmed])
        .current_dir(project_path);
    let out = crate::kopeng::forge::run_sync_command_with_timeout(
        cmd,
        timeout_seconds,
        "git commit",
    )
    .map_err(|e| {
        if let Err(reset_err) = git(project_path, &["reset", "-q"]) {
            crate::logger::log("WARN", "git", &format!("Failed to reset index after commit spawn error: {}", reset_err));
        }
        GitError::Command {
            operation: "commit".to_string(),
            details: format!("cannot run git commit: {}", e),
        }
    })?;

    if !out.status.success() {
        if let Err(reset_err) = git(project_path, &["reset", "-q"]) {
            crate::logger::log("WARN", "git", &format!("Failed to reset index after commit hook failure: {}", reset_err));
        }
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let details = match (stdout.is_empty(), stderr.is_empty()) {
            (false, false) => format!("{}\n{}", stdout, stderr),
            (false, true) => stdout,
            (true, false) => stderr,
            (true, true) => format!("git commit exited with status {}", out.status),
        };
        return Err(GitError::Command {
            operation: "commit".to_string(),
            details,
        });
    }

    let head = git(project_path, &["rev-parse", "HEAD"])?;
    Ok(Some(String::from_utf8_lossy(&head).trim().to_string()))
}

/// Line-level diff hunk extracted from a Git commit or snapshot patch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffFileHunk {
    pub file_path: String,
    pub line_start: u32,
    pub line_end: u32,
    pub change_kind: String,
}

/// Parses a `git diff --unified=0 --no-color --no-renames` patch into per-file line hunks.
pub fn parse_unified_zero_diff_hunks(diff_text: &str) -> Vec<DiffFileHunk> {
    let mut hunks = Vec::new();
    let mut current_file: Option<String> = None;
    let mut change_kind = "modified".to_string();
    let mut saw_hunk = false;

    let flush_file = |file_opt: &mut Option<String>,
                      kind: &mut String,
                      had_hunk: &mut bool,
                      out: &mut Vec<DiffFileHunk>| {
        if let Some(path) = file_opt.take() {
            if !*had_hunk {
                out.push(DiffFileHunk {
                    file_path: path,
                    line_start: 0,
                    line_end: 0,
                    change_kind: kind.clone(),
                });
            }
        }
        *kind = "modified".to_string();
        *had_hunk = false;
    };

    for line in diff_text.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            flush_file(&mut current_file, &mut change_kind, &mut saw_hunk, &mut hunks);
            if let Some((_, b_part)) = rest.split_once(" b/") {
                current_file = Some(b_part.trim().to_string());
            } else if let Some(a_part) = rest.strip_prefix("a/") {
                if let Some((a_path, _)) = a_part.split_once(' ') {
                    current_file = Some(a_path.trim().to_string());
                }
            }
            continue;
        }

        if line.starts_with("new file mode ") {
            change_kind = "added".to_string();
            continue;
        }
        if line.starts_with("deleted file mode ") {
            change_kind = "deleted".to_string();
            continue;
        }
        if let Some(plus_path) = line.strip_prefix("+++ b/") {
            let trimmed = plus_path.trim();
            if !trimmed.is_empty() {
                current_file = Some(trimmed.to_string());
            }
            continue;
        }
        if line == "+++ /dev/null" {
            change_kind = "deleted".to_string();
            continue;
        }

        if let Some(after_at) = line.strip_prefix("@@ ") {
            let Some(ref path) = current_file else {
                continue;
            };
            let Some((header_range, _)) = after_at.split_once(" @@") else {
                continue;
            };
            let Some(plus_token) = header_range.split_whitespace().find(|t| t.starts_with('+')) else {
                continue;
            };
            let range_str = &plus_token[1..];
            let (start_str, count_str_opt) = match range_str.split_once(',') {
                Some((s, c)) => (s, Some(c)),
                None => (range_str, None),
            };
            let Ok(new_start) = start_str.parse::<u32>() else {
                continue;
            };
            let new_count = match count_str_opt {
                Some(c) => match c.parse::<u32>() {
                    Ok(v) => v,
                    Err(_) => continue,
                },
                None => 1,
            };

            let (line_start, line_end) = if change_kind == "deleted" && new_start == 0 {
                (0, 0)
            } else if new_count == 0 {
                let anchor = new_start.max(1);
                (anchor, anchor)
            } else {
                let start = new_start.max(1);
                (start, start.saturating_add(new_count.saturating_sub(1)))
            };

            hunks.push(DiffFileHunk {
                file_path: path.clone(),
                line_start,
                line_end,
                change_kind: change_kind.clone(),
            });
            saw_hunk = true;
        }
    }

    flush_file(&mut current_file, &mut change_kind, &mut saw_hunk, &mut hunks);
    hunks
}

/// Extracts line-level diff hunks introduced by `commit_hash` (`git show --unified=0`).
pub fn extract_commit_diff_hunks(
    project_path: &Path,
    commit_hash: &str,
) -> Result<Vec<DiffFileHunk>, GitError> {
    require_repository(project_path)?;
    require_commit_hash(commit_hash)?;
    let stdout = git(
        project_path,
        &[
            "-c",
            "core.quotepath=false",
            "show",
            "--format=",
            "--patch",
            "--unified=0",
            "--no-color",
            "--no-renames",
            "--end-of-options",
            commit_hash,
        ],
    )?;
    let text = String::from_utf8_lossy(&stdout);
    Ok(parse_unified_zero_diff_hunks(&text))
}

/// Commits currently staged changes with `message` and returns the new commit hash.
pub async fn commit_git_changes(project_id: usize, message: String) -> Result<String, GitError> {
    let trimmed = message.trim().to_string();
    if trimmed.is_empty() {
        return Err(GitError::InvalidVcsAction("Commit message cannot be empty".to_string()));
    }
    let hash = on_project(project_id, move |path| {
        require_repository(path)?;
        let staged = git(path, &["diff", "--cached", "--name-only"])?;
        if String::from_utf8_lossy(&staged).trim().is_empty() {
            return Err(GitError::InvalidVcsAction("No staged changes to commit".to_string()));
        }
        git(path, &["commit", "-m", &trimmed])?;
        let head = git(path, &["rev-parse", "--short", "HEAD"])?;
        Ok(String::from_utf8_lossy(&head).trim().to_string())
    })
    .await?;
    crate::db::notify_projects_updated();
    Ok(hash)
}

/// Synchronizes the current branch with its configured remote (`pull`, `push`, or `sync`).
pub async fn sync_git_branch(project_id: usize, mode: GitSyncMode) -> Result<String, GitError> {
    let output = on_project(project_id, move |path| {
        require_repository(path)?;
        let mut logs = Vec::with_capacity(2);
        if matches!(mode, GitSyncMode::Pull | GitSyncMode::Sync) {
            let out = git(path, &["pull", "--rebase"])?;
            let text = String::from_utf8_lossy(&out).trim().to_string();
            if !text.is_empty() {
                logs.push(text);
            }
        }
        if matches!(mode, GitSyncMode::Push | GitSyncMode::Sync) {
            let out = git(path, &["push"])?;
            let text = String::from_utf8_lossy(&out).trim().to_string();
            if !text.is_empty() {
                logs.push(text);
            }
        }
        Ok(if logs.is_empty() { "Synced".to_string() } else { logs.join("\n") })
    })
    .await?;
    crate::db::notify_projects_updated();
    Ok(output)
}

/// Generates a concise commit message from the currently staged diff using the project's default runner.
///
/// Design, PCB, and CAD asset files are listed by path only (their diffs are omitted).
pub async fn generate_commit_message(project_id: usize) -> Result<String, GitError> {
    const MAX_COMMIT_DIFF_BYTES: usize = 24 * 1024;
    let timeout_ms = crate::config::resolve_effective_u64(
        None,
        None,
        "vcs",
        "forge_cli_timeout_seconds",
    )
    .max(90)
    .saturating_mul(1000);

    let project = tokio::task::spawn_blocking(move || crate::registry::find_project_by_id(project_id))
        .await
        .map_err(|e| GitError::Task(e.to_string()))??;
    let project_path_str = project.path.clone();

    let (staged_summary, code_diff) = tokio::task::spawn_blocking(move || {
        let path = Path::new(&project_path_str);
        require_repository(path)?;
        let staged_raw = git(path, &["diff", "--cached", "--name-status", "-z"])?;
        let staged_entries = parse_name_status(&staged_raw)?;
        if staged_entries.is_empty() {
            return Err(GitError::InvalidVcsAction("Stage at least one file before generating a commit message".to_string()));
        }

        let mut summary_lines = Vec::with_capacity(staged_entries.len());
        let mut code_paths = Vec::new();
        for entry in &staged_entries {
            let is_asset = crate::kopeng::snapshots::is_domain_asset_path(path, &entry.path);
            let tag = if is_asset { "asset" } else { "code" };
            summary_lines.push(format!("- [{}|{}] {}", entry.status, tag, entry.path));
            if !is_asset {
                code_paths.push(entry.path.clone());
            }
        }

        let diff_text = if code_paths.is_empty() {
            String::new()
        } else {
            let mut args = Vec::with_capacity(4 + code_paths.len());
            args.extend(["diff", "--cached", "--"]);
            args.extend(code_paths.iter().map(String::as_str));
            let raw_diff = git(path, &args)?;
            let lossy = String::from_utf8_lossy(&raw_diff);
            if lossy.len() > MAX_COMMIT_DIFF_BYTES {
                let end = lossy.floor_char_boundary(MAX_COMMIT_DIFF_BYTES);
                format!("{}\n\n[diff truncated at {} bytes]", &lossy[..end], MAX_COMMIT_DIFF_BYTES)
            } else {
                lossy.into_owned()
            }
        };

        Ok::<(String, String), GitError>((summary_lines.join("\n"), diff_text))
    })
    .await
    .map_err(|e| GitError::Task(e.to_string()))??;

    let pair = crate::kopeng::resolution::resolve_effective_agent_config(
        Some(&project.path),
        None,
        None,
        &crate::kopeng::resolution::GIT_AGENT_KEYS,
    )?;

    let prompt = format!(
        "You are a commit message generator. Output ONLY the raw Git commit message in English (no markdown fences, no quotes, no commentary, no tool calls).\n\
Use Conventional Commits format (`type(scope): summary`, e.g. `feat(git): add staging controls`) with a concise subject line (<= 72 chars) and optional short bullet body only if multiple distinct changes are staged.\n\n\
Staged files:\n{}\n\n\
Code diff:\n{}",
        staged_summary,
        if code_diff.is_empty() { "(only visual/hardware asset files staged)" } else { &code_diff }
    );

    let raw = crate::supervisor::run_ephemeral_runner_prompt(
        Path::new(&project.path),
        &pair,
        &prompt,
        timeout_ms,
    )
    .await?;

    let mut stripped = raw.trim();
    if let Some(after_open) = stripped.strip_prefix("```") {
        stripped = after_open
            .split_once('\n')
            .map(|(_, body)| body)
            .unwrap_or(after_open);
        stripped = stripped.strip_suffix("```").unwrap_or(stripped);
    }
    let cleaned = stripped.trim().to_string();
    Ok(cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_hashes_must_be_hexadecimal() {
        assert!(require_commit_hash("3c5d997").is_ok());
        assert!(require_commit_hash(&"a".repeat(40)).is_ok());
        assert!(matches!(require_commit_hash("--output=/tmp/x"), Err(GitError::InvalidCommitHash(_))));
        assert!(matches!(require_commit_hash("abc"), Err(GitError::InvalidCommitHash(_))));
        assert!(matches!(require_commit_hash("HEAD~1"), Err(GitError::InvalidCommitHash(_))));
    }

    #[test]
    fn status_v2_reports_branch_divergence_and_counts() {
        let out = b"# branch.oid 3c5d997\0# branch.head master\0# branch.upstream origin/master\0# branch.ab +2 -1\0\
1 .M N... 100644 100644 100644 a b src/a.rs\0\
2 R. N... 100644 100644 100644 a b R100 new name.rs\0old name.rs\0\
? notes\xc3\xa9.md\0";
        let status = parse_status_v2(out).unwrap();
        assert_eq!(status.branch.as_deref(), Some("master"));
        assert!(status.has_commits && status.has_remote);
        assert_eq!((status.ahead_count, status.behind_count), (2, 1));
        assert_eq!((status.staged_count, status.modified_count, status.untracked_count), (1, 1, 1));
        assert!(!status.is_clean);

        let unborn = parse_status_v2(b"# branch.oid (initial)\0# branch.head main\0").unwrap();
        assert!(!unborn.has_commits && unborn.is_clean);
        assert_eq!(unborn.branch.as_deref(), Some("main"));
        let detached = parse_status_v2(b"# branch.oid 3c5d997\0# branch.head (detached)\0").unwrap();
        assert_eq!(detached.branch, None);
        assert!(parse_status_v2(b"# branch.ab +x -1\0").is_err());
        assert!(parse_status_v2(b"z what\0").is_err());
    }

    #[test]
    fn status_v1_keeps_exact_paths_and_renames() {
        let out = b"R  new dir/b.rs\0old dir/b.rs\0?? caf\xc3\xa9 \"q\".txt\0 D gone.rs\0";
        let files = parse_status_v1(out, |p| p != "gone.rs").unwrap();
        assert_eq!(files.len(), 3);
        assert_eq!((files[0].path.as_str(), files[0].old_path.as_deref(), files[0].status.as_str()), ("new dir/b.rs", Some("old dir/b.rs"), "renamed"));
        assert_eq!((files[1].path.as_str(), files[1].status.as_str()), ("café \"q\".txt", "untracked"));
        assert_eq!(files[2].status, "deleted");
        assert!(parse_status_v1(b"R  only-new\0", |_| true).is_err());
    }

    #[test]
    fn classifies_remote_providers_and_parses_remotes() {
        assert_eq!(
            classify_remote_provider("https://github.com/org/repo.git"),
            ForgeProviderKind::Github
        );
        assert_eq!(
            classify_remote_provider("git@github.com:org/repo.git"),
            ForgeProviderKind::Github
        );
        assert_eq!(
            classify_remote_provider("https://gitlab.com/group/proj.git"),
            ForgeProviderKind::Gitlab
        );
        assert_eq!(
            classify_remote_provider("ssh://git@gitlab.corp.internal:2222/group/proj.git"),
            ForgeProviderKind::Gitlab
        );
        assert_eq!(
            classify_remote_provider("https://git.sr.ht/~user/repo"),
            ForgeProviderKind::Unknown
        );

        let raw = b"remote.origin.url git@github.com:a/b.git\n\
                    remote.origin.url https://github.com/a/b-dup.git\n\
                    remote.upstream.url https://gitlab.com/c/d.git\n";
        let parsed = parse_git_remotes(raw);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "origin");
        assert_eq!(parsed[0].provider, ForgeProviderKind::Github);
        assert_eq!(parsed[1].name, "upstream");
        assert_eq!(parsed[1].provider, ForgeProviderKind::Gitlab);
    }

    #[test]
    fn get_git_status_reports_origin_upstream_multiple_and_no_remotes() {
        let dir = std::env::temp_dir().join(format!("oneship_git_remotes_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let run_git = |args: &[&str]| {
            let out = Command::new("git").args(args).current_dir(&dir).output().unwrap();
            assert!(out.status.success(), "git {:?} failed: {}", args, String::from_utf8_lossy(&out.stderr));
        };

        run_git(&["init"]);
        let s0 = get_git_status(&dir).unwrap();
        assert!(!s0.has_remote);
        assert!(s0.remote_url.is_none());
        assert!(s0.remotes.is_empty());

        run_git(&["remote", "add", "upstream", "https://gitlab.com/org/repo.git"]);
        let s1 = get_git_status(&dir).unwrap();
        assert!(s1.has_remote);
        assert!(s1.remote_url.is_none());
        assert_eq!(s1.remotes.len(), 1);
        assert_eq!(s1.remotes[0].name, "upstream");
        assert_eq!(s1.remotes[0].provider, ForgeProviderKind::Gitlab);

        run_git(&["remote", "add", "origin", "git@github.com:org/fork.git"]);
        let s2 = get_git_status(&dir).unwrap();
        assert!(s2.has_remote);
        assert_eq!(s2.remote_url.as_deref(), Some("git@github.com:org/fork.git"));
        assert_eq!(s2.remotes.len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn name_status_maps_every_code_and_rejects_unknown_ones() {
        let out = b"A\0a.rs\0R100\0old.rs\0new.rs\0T\0link\0";
        let files = parse_name_status(out).unwrap();
        assert_eq!(files.iter().map(|f| (f.path.as_str(), f.status.as_str())).collect::<Vec<_>>(), [("a.rs", "added"), ("new.rs", "renamed"), ("link", "type_changed")]);
        assert!(parse_name_status(b"X\0a.rs\0").is_err());
        assert!(parse_name_status(b"M\0").is_err());
    }

    #[test]
    fn git_log_records_need_every_field() {
        let out = b"h1\x1fh\x1fAna\x1fa@x\x1f2026-11-03T00:00:00Z\x1fsubject one\0\nh2\x1fh\x1fBo\x1fb@x\x1f2026-11-02T00:00:00Z\x1fsubject two\0";
        let commits = parse_git_log(out, 2).unwrap();
        assert_eq!(commits.iter().map(|c| c.hash.as_str()).collect::<Vec<_>>(), ["h1", "h2"]);
        assert!(parse_git_log(b"h1\x1fh\0", 1).is_err());
    }

    #[test]
    fn vcs_actions_are_appended_unique_and_non_empty() {
        let mut rula: toml::Value = toml::from_str("[project]\nname = \"x\"\n").unwrap();
        let action = |n: &str, p: &str| VcsAction { name: n.to_string(), prompt: p.to_string() };
        add_vcs_action(&mut rula, action("release", "Tag \"v1\"\nand push \\ now")).unwrap();
        let round_trip: toml::Value = toml::from_str(&toml::to_string_pretty(&rula).unwrap()).unwrap();
        let actions = vcs_actions_of(&round_trip).unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].prompt, "Tag \"v1\"\nand push \\ now");
        assert!(matches!(add_vcs_action(&mut rula, action("release", "again")), Err(GitError::InvalidVcsAction(_))));
        assert!(matches!(add_vcs_action(&mut rula, action("", "x")), Err(GitError::InvalidVcsAction(_))));
        let mut bad: toml::Value = toml::from_str("[plugins]\nvcs = 3\n").unwrap();
        assert!(matches!(add_vcs_action(&mut bad, action("a", "b")), Err(GitError::InvalidVcsAction(_))));
    }

    #[test]
    fn auto_commit_session_minutes_respects_detached_unborn_hooks_and_assets() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let run_git = |args: &[&str]| {
            let out = Command::new("git").args(args).current_dir(&repo).output().unwrap();
            assert!(out.status.success(), "git {:?}: {}", args, String::from_utf8_lossy(&out.stderr));
        };
        run_git(&["init", "-q"]);
        run_git(&["config", "user.name", "Test"]);
        run_git(&["config", "user.email", "test@example.com"]);
        std::fs::write(repo.join("main.rs"), "fn main() {}\n").unwrap();

        // 1. Unborn repo (no initial commit) must be rejected.
        let unborn_err = auto_commit_session_minutes(1, &repo, "feat: init").unwrap_err().to_string();
        assert!(unborn_err.contains("initial commit"), "{}", unborn_err);

        // Create initial commit.
        run_git(&["add", "main.rs"]);
        run_git(&["commit", "-q", "-m", "chore: initial"]);
        let base_hash = String::from_utf8_lossy(&git(&repo, &["rev-parse", "HEAD"]).unwrap()).trim().to_string();

        // 2. Detached HEAD must be rejected.
        run_git(&["checkout", "-q", "--detach", "HEAD"]);
        let detached_err = auto_commit_session_minutes(1, &repo, "feat: detached").unwrap_err().to_string();
        assert!(detached_err.contains("detached HEAD"), "{}", detached_err);
        run_git(&["checkout", "-q", "-"]);

        // 3. Residual staged file + code edit + domain asset: resets old stage, commits code, leaves asset unstaged.
        std::fs::write(repo.join("stale.txt"), "stale").unwrap();
        run_git(&["add", "stale.txt"]);
        std::fs::write(repo.join("main.rs"), "fn main() { println!(\"hi\"); }\n").unwrap();
        std::fs::write(repo.join("board.kicad_pcb"), "(kicad_pcb)").unwrap();

        let hash = auto_commit_session_minutes(1, &repo, "feat: greet\n\nDecisions:\n- Print greeting")
            .unwrap()
            .expect("commit hash");
        assert_ne!(hash, base_hash);
        let committed_files = String::from_utf8_lossy(
            &git(&repo, &["diff-tree", "--no-commit-id", "--name-only", "-r", &hash]).unwrap(),
        )
        .to_string();
        assert!(committed_files.contains("main.rs"));
        assert!(!committed_files.contains("board.kicad_pcb"));

        // 4. Failing pre-commit hook must fail the commit, surface hook output, and reset index.
        let hooks_dir = repo.join(".git/hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        let hook_path = hooks_dir.join("pre-commit");
        std::fs::write(&hook_path, "#!/bin/sh\necho 'LINT FAILED: trailing space' >&2\nexit 1\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::write(repo.join("main.rs"), "fn main() { let x = 1; }\n").unwrap();
        let hook_err = auto_commit_session_minutes(1, &repo, "fix: x").unwrap_err().to_string();
        assert!(hook_err.contains("LINT FAILED"), "{}", hook_err);
        let staged_after = String::from_utf8_lossy(&git(&repo, &["diff", "--cached", "--name-only"]).unwrap()).to_string();
        assert!(staged_after.trim().is_empty(), "staged_after={}", staged_after);
    }

    #[test]
    fn parse_unified_zero_diff_hunks_extracts_exact_line_ranges() {
        let patch = r#"diff --git a/models/engine.py b/models/engine.py
index 111..222 100644
--- a/models/engine.py
+++ b/models/engine.py
@@ -10,0 +11,3 @@ class Engine:
+    a = 1
+    b = 2
+    c = 3
@@ -45 +48 @@ class Engine:
-    old()
+    new()
diff --git a/models/new_helper.py b/models/new_helper.py
new file mode 100644
index 000..333
--- /dev/null
+++ b/models/new_helper.py
@@ -0,0 +1,5 @@
+def helper():
+    pass
diff --git a/models/old.py b/models/old.py
deleted file mode 100644
index 444..000
--- a/models/old.py
+++ /dev/null
@@ -1,4 +0,0 @@
-gone
"#;
        let hunks = parse_unified_zero_diff_hunks(patch);
        assert_eq!(
            hunks,
            vec![
                DiffFileHunk {
                    file_path: "models/engine.py".to_string(),
                    line_start: 11,
                    line_end: 13,
                    change_kind: "modified".to_string(),
                },
                DiffFileHunk {
                    file_path: "models/engine.py".to_string(),
                    line_start: 48,
                    line_end: 48,
                    change_kind: "modified".to_string(),
                },
                DiffFileHunk {
                    file_path: "models/new_helper.py".to_string(),
                    line_start: 1,
                    line_end: 5,
                    change_kind: "added".to_string(),
                },
                DiffFileHunk {
                    file_path: "models/old.py".to_string(),
                    line_start: 0,
                    line_end: 0,
                    change_kind: "deleted".to_string(),
                },
            ]
        );
    }
}
