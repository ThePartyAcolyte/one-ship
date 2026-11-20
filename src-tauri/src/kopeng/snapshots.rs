use std::path::{Path, PathBuf};
use std::process::Command;
use serde::{Deserialize, Serialize};
use crate::db::{
    db_count_snapshots, db_delete_snapshot, db_get_oldest_snapshot_conversations,
    db_get_project_by_path, db_find_snapshots_by_id_or_ref, db_insert_snapshot,
    db_list_domain_asset_snapshots_oldest_first,
    db_list_snapshots, DatabaseError, SnapshotRecord,
};
use crate::config::{resolve_effective_bool, resolve_effective_string, resolve_effective_u64, resolve_snapshot_max_per_project};

/// File extensions treated as visual/hardware domain assets (`desigma`, `pcbcraft`, `solidcraft`).
///
/// These files remain tracked in Git commits, but their diffs are suppressed in the UI and they
/// are excluded from code/milestone snapshots because each domain maintains its own per-asset
/// automatic snapshots.
const DOMAIN_ASSET_EXTENSIONS: &[&str] = &[
    "desigma",
    "fig",
    "kicad_pcb",
    "kicad_sch",
    "kicad_pro",
    "kicad_sym",
    "kicad_mod",
    "scad",
    "step",
    "stp",
    "stl",
    "obj",
    "3mf",
    "iges",
    "igs",
    "brep",
];

fn configured_asset_dirs(project_path: &Path) -> Vec<String> {
    let proj_str = project_path.to_string_lossy();
    let mut dirs = Vec::with_capacity(3);
    for (plugin, key) in [
        ("desigma", "design_dir"),
        ("pcbcraft", "pcb_dir"),
        ("solidcraft", "cad_dir"),
    ] {
        let raw = resolve_effective_string(Some(&proj_str), None, plugin, key);
        let trimmed = raw.trim().trim_matches('/');
        if !trimmed.is_empty() && trimmed != "." {
            dirs.push(trimmed.to_string());
        }
    }
    dirs
}

/// Returns `true` when `rel_path` belongs to a Design, PCB, or CAD domain directory or extension.
pub(crate) fn is_domain_asset_path(project_path: &Path, rel_path: &str) -> bool {
    let norm = rel_path.trim().trim_start_matches("./");
    if configured_asset_dirs(project_path).iter().any(|dir| {
        norm == dir || norm.starts_with(&format!("{}/", dir))
    }) {
        return true;
    }
    Path::new(norm)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| {
            let lower = ext.to_ascii_lowercase();
            DOMAIN_ASSET_EXTENSIONS.iter().any(|known| *known == lower)
        })
}

/// Git pathspec exclusions (`:(exclude)...`) for Design, PCB, and CAD domain assets.
pub(crate) fn domain_asset_exclude_pathspecs(project_path: &Path) -> Vec<String> {
    let dirs = configured_asset_dirs(project_path);
    let mut specs = Vec::with_capacity(dirs.len() + DOMAIN_ASSET_EXTENSIONS.len());
    for dir in dirs {
        specs.push(format!(":(exclude){}", dir));
    }
    for ext in DOMAIN_ASSET_EXTENSIONS {
        specs.push(format!(":(exclude)*.{}", ext));
    }
    specs
}

/// Namespace of the hidden snapshot refs (Spec 16): `<prefix>/<project_id>/<ts>-<conversation>-<label>`.
pub const SNAPSHOT_REF_PREFIX: &str = "refs/one-ship/snapshots";
/// `conversation_id` recorded for a snapshot taken outside an agent conversation.
pub const MANUAL_CONVERSATION_ID: &str = "manual";

/// Failure of a snapshot operation.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("Project '{0}' has no git repository")]
    NoRepository(String),
    #[error("git {operation} failed: {details}")]
    Git { operation: String, details: String },
    #[error("Snapshot '{id_or_ref}' does not exist in project {project_id}")]
    NotFound { project_id: i64, id_or_ref: String },
    #[error("Snapshot {ref_path} has an invalid files_changed list: {source}")]
    FilesChangedJson { ref_path: String, source: serde_json::Error },
    #[error("Invalid snapshot request: {0}")]
    InvalidRequest(String),
    #[error("Background task failed: {0}")]
    Task(String),
    #[error(transparent)]
    Registry(#[from] crate::registry::RegistryError),
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

impl crate::api::ApiError for SnapshotError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::NoRepository(_) | Self::FilesChangedJson { .. } => StatusCode::CONFLICT,
            Self::NotFound { .. } => StatusCode::NOT_FOUND,
            Self::InvalidRequest(_) => StatusCode::BAD_REQUEST,
            Self::Git { .. } => StatusCode::BAD_GATEWAY,
            Self::Task(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Registry(e) => e.status(),
            Self::Database(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::NoRepository(_) => "SnapshotError::NoRepository",
            Self::Git { .. } => "SnapshotError::Git",
            Self::NotFound { .. } => "SnapshotError::NotFound",
            Self::FilesChangedJson { .. } => "SnapshotError::FilesChangedJson",
            Self::InvalidRequest(_) => "SnapshotError::InvalidRequest",
            Self::Task(_) => "SnapshotError::Task",
            Self::Registry(e) => e.code(),
            Self::Database(e) => e.code(),
        }
    }
}

/// Point-in-time Git snapshot record stored in SQLite and referenced under `refs/one-ship/snapshots/`.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct Snapshot {
    pub id: String,
    pub ref_path: String,
    #[ts(type = "number")]
    pub project_id: i64,
    pub conversation_id: String,
    pub agent_id: String,
    pub label: String,
    pub files_changed: Vec<String>,
    pub created_at: String,
    pub domain: String,
    pub asset_key: Option<String>,
}

impl TryFrom<SnapshotRecord> for Snapshot {
    type Error = SnapshotError;

    fn try_from(rec: SnapshotRecord) -> Result<Self, Self::Error> {
        let files_changed = match rec.files_changed.as_deref() {
            Some(json) => serde_json::from_str(json)
                .map_err(|source| SnapshotError::FilesChangedJson { ref_path: rec.ref_path.clone(), source })?,
            None => Vec::new(),
        };
        Ok(Snapshot {
            id: rec.id,
            ref_path: rec.ref_path,
            project_id: rec.project_id,
            conversation_id: rec.conversation_id,
            agent_id: rec.agent_id,
            label: rec.label,
            files_changed,
            created_at: rec.created_at,
            domain: rec.domain,
            asset_key: rec.asset_key,
        })
    }
}

/// Runs `git <args>` in `repo` (optionally against a separate index) and returns its trimmed stdout.
fn git(repo: &Path, args: &[&str], index_file: Option<&Path>) -> Result<String, SnapshotError> {
    let operation = args.first().copied().unwrap_or_default().to_string();
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(repo);
    if let Some(index) = index_file {
        cmd.env("GIT_INDEX_FILE", index);
    }
    let out = cmd.output().map_err(|e| SnapshotError::Git { operation: operation.clone(), details: format!("cannot run git: {}", e) })?;
    if !out.status.success() {
        return Err(SnapshotError::Git { operation, details: String::from_utf8_lossy(&out.stderr).trim().to_string() });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

fn output_lines(stdout: &str) -> Vec<String> {
    stdout.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()
}

/// Commit at HEAD, or `None` for a repository without commits (unborn branch).
pub(crate) fn head_commit(repo: &Path) -> Result<Option<String>, SnapshotError> {
    let out = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", "HEAD^{commit}"])
        .current_dir(repo)
        .output()
        .map_err(|e| SnapshotError::Git { operation: "rev-parse".to_string(), details: format!("cannot run git: {}", e) })?;
    if out.status.success() {
        return Ok(Some(String::from_utf8_lossy(&out.stdout).trim().to_string()));
    }
    // `--verify --quiet` exits 1 without output when HEAD does not resolve (no commits yet).
    if out.status.code() == Some(1) && out.stderr.is_empty() {
        return Ok(None);
    }
    Err(SnapshotError::Git { operation: "rev-parse".to_string(), details: String::from_utf8_lossy(&out.stderr).trim().to_string() })
}

/// A private index file inside the git directory, removed on drop.
struct TempIndex {
    path: PathBuf,
}

impl TempIndex {
    fn new(repo: &Path) -> Result<Self, SnapshotError> {
        let name = format!("oneship-snapshot-{}.index", uuid::Uuid::new_v4());
        let git_path = git(repo, &["rev-parse", "--git-path", &name], None)?;
        Ok(Self { path: repo.join(git_path) })
    }
}

impl Drop for TempIndex {
    fn drop(&mut self) {
        match std::fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => crate::logger::log("ERROR", "snapshots", &format!("Cannot remove temporary index {}: {}", self.path.display(), e)),
        }
    }
}

/// Records the working-tree state as a commit without touching HEAD, the user's index or hooks.
///
/// A private index is seeded from HEAD (empty for a repository without commits); `scoped_files`
/// (or everything, when empty) is added to it, written as a tree and committed with HEAD as
/// parent. Returns the commit and the files it changes relative to HEAD.
fn capture_snapshot_commit(
    repo: &Path,
    scoped_files: &[String],
    exclude_specs: &[String],
    message: &str,
) -> Result<(String, Vec<String>), SnapshotError> {
    let head = head_commit(repo)?;
    let index = TempIndex::new(repo)?;
    let idx = Some(index.path.as_path());
    match head.as_deref() {
        Some(h) => git(repo, &["read-tree", h], idx)?,
        None => git(repo, &["read-tree", "--empty"], idx)?,
    };
    if scoped_files.is_empty() {
        let mut args = vec!["add", "-A", "--", "."];
        args.extend(exclude_specs.iter().map(String::as_str));
        git(repo, &args, idx)?;
    } else {
        let mut args = vec!["add", "-A", "--"];
        args.extend(scoped_files.iter().map(String::as_str));
        git(repo, &args, idx)?;
    }
    let tree = git(repo, &["write-tree"], idx)?;

    let mut commit_args = vec!["commit-tree", tree.as_str(), "-m", message];
    if let Some(h) = head.as_deref() {
        commit_args.extend(["-p", h]);
    }
    let commit = git(repo, &commit_args, None)?;

    let files_changed = match head.as_deref() {
        Some(h) => output_lines(&git(repo, &["diff", "--name-only", h, &commit], None)?),
        None => output_lines(&git(repo, &["ls-tree", "-r", "--name-only", &commit], None)?),
    };
    Ok((commit, files_changed))
}

/// Patch of the working tree (untracked files included, `.gitignore` honoured) against HEAD, or
/// against the empty tree when the repository has no commits; `file_path` scopes it to one path.
///
/// Uses a private index seeded from HEAD, like `capture_snapshot_commit`: the user's index and
/// working tree are untouched.
pub(crate) fn diff_working_tree(repo: &Path, file_path: Option<&str>) -> Result<String, SnapshotError> {
    let head = head_commit(repo)?;
    let index = TempIndex::new(repo)?;
    let idx = Some(index.path.as_path());
    match head.as_deref() {
        Some(h) => git(repo, &["read-tree", h], idx)?,
        None => git(repo, &["read-tree", "--empty"], idx)?,
    };
    let mut add_args = vec!["add", "-A", "--"];
    let mut diff_args = vec!["diff", "--cached", "--"];
    if let Some(f) = file_path {
        add_args.push(f);
        diff_args.push(f);
    }
    git(repo, &add_args, idx)?;
    git(repo, &diff_args, idx)
}

/// Restores `files` (or the whole tree) of the working directory from `source_ref`; the index is untouched.
fn restore_from_ref(repo: &Path, source_ref: &str, files: Option<&[String]>) -> Result<Vec<String>, SnapshotError> {
    let mut args = vec!["restore", "--source", source_ref, "--worktree", "--"];
    let restored = match files {
        Some(list) => {
            args.extend(list.iter().map(String::as_str));
            list.to_vec()
        }
        None => {
            args.push(".");
            vec![".".to_string()]
        }
    };
    git(repo, &args, None)?;
    Ok(restored)
}

/// One ref-name component: ASCII alphanumerics, `_` and `-`; anything else becomes `-`.
fn ref_component(raw: &str) -> String {
    let mapped: String = raw.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '-' }).collect();
    let trimmed = mapped.trim_matches('-');
    if trimmed.is_empty() { "snapshot".to_string() } else { trimmed.to_string() }
}

fn require_repository(project_path: &Path) -> Result<(), SnapshotError> {
    if project_path.join(".git").exists() {
        Ok(())
    } else {
        Err(SnapshotError::NoRepository(project_path.display().to_string()))
    }
}

/// Snapshot record of `project_id` with this exact id or ref path; several matches are an error.
fn find_snapshot(project_id: i64, id_or_ref: &str) -> Result<SnapshotRecord, SnapshotError> {
    let mut matches = db_find_snapshots_by_id_or_ref(project_id, id_or_ref)?;
    match matches.len() {
        0 => Err(SnapshotError::NotFound { project_id, id_or_ref: id_or_ref.to_string() }),
        1 => Ok(matches.remove(0)),
        _ => Err(SnapshotError::InvalidRequest(format!("snapshot id '{}' is ambiguous; use its ref path", id_or_ref))),
    }
}

/// Deletes a snapshot ref and then its record.
fn delete_snapshot_record(project_path: &Path, ref_path: &str) -> Result<(), SnapshotError> {
    debug_assert!(ref_path.starts_with(SNAPSHOT_REF_PREFIX), "only snapshot refs are deleted");
    git(project_path, &["update-ref", "-d", ref_path], None)?;
    db_delete_snapshot(ref_path)?;
    Ok(())
}

/// Deletes every `refs/one-ship/snapshots/<project_id>/*` ref of the repository (project
/// removal; the records go with the project's database rows). Returns the number deleted.
pub fn delete_project_snapshot_refs(project_path: &Path, project_id: i64) -> Result<usize, SnapshotError> {
    let pattern = format!("{}/{}/", SNAPSHOT_REF_PREFIX, project_id);
    let refs = output_lines(&git(project_path, &["for-each-ref", "--format=%(refname)", &pattern], None)?);
    for ref_name in &refs {
        git(project_path, &["update-ref", "-d", ref_name], None)?;
    }
    Ok(refs.len())
}

/// Prunes whole conversations, oldest first, until a new snapshot fits within `max_allowed` (Spec 16 §5.1).
pub fn prune_excess_snapshots(project_path: &Path, project_id: i64, max_allowed: usize) -> Result<(), SnapshotError> {
    let max_rounds_count = db_count_snapshots(project_id)?;
    for _ in 0..=max_rounds_count {
        let count = db_count_snapshots(project_id)?;
        if count < max_allowed {
            return Ok(());
        }
        let Some(conversation_id) = db_get_oldest_snapshot_conversations(project_id, 1)?.into_iter().next() else {
            return Ok(());
        };
        for s in db_list_snapshots(project_id, Some(&conversation_id), None, count)? {
            delete_snapshot_record(project_path, &s.ref_path)?;
        }
        crate::logger::log("INFO", "snapshots", &format!("Pruned snapshots of conversation '{}' in project {}", conversation_id, project_id));
    }
    Ok(())
}

/// Deterministically detects modified, added, renamed, deleted, or untracked non-asset (code/docs)
/// files in `project_path` via `git status --porcelain=v1 -z -uall`, independent of the agent runner.
pub fn detect_changed_code_files(project_path: &Path) -> Result<Vec<String>, SnapshotError> {
    require_repository(project_path)?;
    let out = Command::new("git")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(["status", "--porcelain=v1", "-z", "-uall"])
        .current_dir(project_path)
        .output()
        .map_err(|e| SnapshotError::Git {
            operation: "status".to_string(),
            details: format!("cannot run git: {}", e),
        })?;
    if !out.status.success() {
        return Err(SnapshotError::Git {
            operation: "status".to_string(),
            details: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }

    let mut records = out
        .stdout
        .split(|b| *b == 0)
        .filter(|r| !r.is_empty())
        .map(|r| String::from_utf8_lossy(r).into_owned());

    let mut files: Vec<String> = Vec::new();
    while let Some(record) = records.next() {
        let mut chars = record.chars();
        let (Some(staged_code), Some(unstaged_code), Some(' ')) = (chars.next(), chars.next(), chars.next()) else {
            return Err(SnapshotError::Git {
                operation: "status".to_string(),
                details: format!("unexpected status record '{}'", record),
            });
        };
        let path: String = chars.collect();
        if path.is_empty() {
            continue;
        }
        let old_path = if matches!(staged_code, 'R' | 'C') || matches!(unstaged_code, 'R' | 'C') {
            records.next()
        } else {
            None
        };

        for candidate in [Some(path), old_path].into_iter().flatten() {
            if !is_domain_asset_path(project_path, &candidate) && !files.contains(&candidate) {
                files.push(candidate);
            }
        }
    }
    Ok(files)
}

fn is_tracked_in_head(repo: &Path, rel_path: &str) -> bool {
    Command::new("git")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(["cat-file", "-e", &format!("HEAD:{}", rel_path)])
        .current_dir(repo)
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Prunes oldest snapshots for a `(project_id, domain, asset_key)` group until a new snapshot fits within `max_allowed`.
pub fn prune_domain_asset_snapshots(
    project_path: &Path,
    project_id: i64,
    domain: &str,
    asset_key: Option<&str>,
    max_allowed: usize,
) -> Result<(), SnapshotError> {
    if max_allowed == 0 {
        return Ok(());
    }
    let existing = db_list_domain_asset_snapshots_oldest_first(project_id, domain, asset_key)?;
    if existing.len() < max_allowed {
        return Ok(());
    }
    let prune_count = existing.len() + 1 - max_allowed;
    for snap in existing.into_iter().take(prune_count) {
        let _ = git(project_path, &["update-ref", "-d", &snap.ref_path], None);
        db_delete_snapshot(&snap.ref_path)?;
    }
    crate::logger::log(
        "INFO",
        "snapshots",
        &format!("Pruned {} oldest '{}' snapshots (asset={:?}) in project {}", prune_count, domain, asset_key, project_id),
    );
    Ok(())
}

/// Pins the current project state to a hidden snapshot ref (Spec 16).
///
/// Scope: `explicit_files`, else the files the agent wrote in `conversation_id`'s transcript,
/// else the whole working tree. HEAD, the branch, the user's index and hooks are untouched.
/// `conversation_id: None` records a manual snapshot.
pub fn create_snapshot(
    project_path: &Path,
    project_id: i64,
    conversation_id: Option<&str>,
    agent_id: &str,
    label: &str,
    explicit_files: Option<Vec<String>>,
) -> Result<Snapshot, SnapshotError> {
    create_domain_snapshot(
        project_path,
        project_id,
        conversation_id,
        agent_id,
        label,
        explicit_files,
        "code",
        None,
        resolve_snapshot_max_per_project(Some(project_path)),
    )
}

/// Creates an automatic milestone snapshot only when the working tree contains at least one
/// changed non-asset (code/docs) file, detected deterministically via Git.
///
/// Design, PCB, and CAD assets are excluded so milestone snapshots never duplicate or interfere
/// with per-asset automatic snapshots (`desigma`, `pcbcraft`, `solidcraft`).
pub fn create_milestone_snapshot_if_needed(
    project_path: &Path,
    project_id: i64,
    conversation_id: &str,
    agent_id: &str,
    label: &str,
) -> Result<Option<Snapshot>, SnapshotError> {
    require_repository(project_path)?;
    let code_files = detect_changed_code_files(project_path)?;
    if code_files.is_empty() {
        crate::logger::log(
            "DEBUG",
            "snapshots",
            &format!("Skipping milestone snapshot '{}' for project {}: no non-asset files changed", label, project_id),
        );
        return Ok(None);
    }
    create_snapshot(
        project_path,
        project_id,
        Some(conversation_id),
        agent_id,
        label,
        Some(code_files),
    )
    .map(Some)
}

/// Pins a domain-scoped snapshot (`code`, `pcbcraft`, `solidcraft`, `desigma`) with per-asset retention.
#[allow(clippy::too_many_arguments)]
pub fn create_domain_snapshot(
    project_path: &Path,
    project_id: i64,
    conversation_id: Option<&str>,
    agent_id: &str,
    label: &str,
    explicit_files: Option<Vec<String>>,
    domain: &str,
    asset_key: Option<&str>,
    max_allowed: usize,
) -> Result<Snapshot, SnapshotError> {
    crate::logger::log("DEBUG", "snapshots", &format!("create_domain_snapshot: project_id={}, domain={}, asset={:?}, conversation={:?}, agent={}, label={}", project_id, domain, asset_key, conversation_id, agent_id, label));
    require_repository(project_path)?;
    let label = label.trim();
    if label.is_empty() {
        return Err(SnapshotError::InvalidRequest("label is required".to_string()));
    }
    let conversation = match conversation_id.map(str::trim) {
        Some("") => return Err(SnapshotError::InvalidRequest("conversation id is empty".to_string())),
        Some(c) => Some(c),
        None => None,
    };
    let conv_id = conversation.unwrap_or(MANUAL_CONVERSATION_ID);

    if domain == "code" {
        prune_excess_snapshots(project_path, project_id, max_allowed)?;
    } else {
        prune_domain_asset_snapshots(project_path, project_id, domain, asset_key, max_allowed)?;
    }

    // 1. Scope: explicit files, else deterministic Git diff for code domain, else whole working tree.
    let is_code_domain = domain == "code";
    let requested = match (explicit_files.filter(|f| !f.is_empty()), conversation) {
        (Some(files), _) => files,
        (None, Some(_)) if is_code_domain => detect_changed_code_files(project_path)?,
        (None, _) => Vec::new(),
    };
    let scoped: Vec<String> = requested
        .into_iter()
        .filter(|f| project_path.join(f).symlink_metadata().is_ok() || is_tracked_in_head(project_path, f))
        .filter(|f| !is_code_domain || !is_domain_asset_path(project_path, f))
        .collect();
    let exclude_specs = if is_code_domain {
        domain_asset_exclude_pathspecs(project_path)
    } else {
        Vec::new()
    };

    // 2. Commit the state through a private index, then pin it.
    let message = format!("oneship-snapshot: {} | domain:{} | conv:{} | agent:{}", label, domain, conv_id, agent_id);
    let (commit, mut files_changed) = capture_snapshot_commit(project_path, &scoped, &exclude_specs, &message)?;
    if !scoped.is_empty() {
        files_changed = scoped;
    }

    let created_at = chrono::Utc::now();
    let ref_path = format!(
        "{}/{}/{}-{}-{}",
        SNAPSHOT_REF_PREFIX, project_id, created_at.timestamp_millis(), ref_component(conv_id), ref_component(label)
    );
    git(project_path, &["check-ref-format", &ref_path], None)?;
    // Empty old value: the ref must not exist yet.
    git(project_path, &["update-ref", &ref_path, &commit, ""], None)?;

    let short_id: String = commit.chars().take(12).collect();
    let created_at = iso_utc(created_at);
    let files_changed_json = if files_changed.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&files_changed).map_err(|source| SnapshotError::FilesChangedJson { ref_path: ref_path.clone(), source })?)
    };
    let domain_str = domain.to_string();
    let asset_opt = asset_key.map(|s| s.to_string());
    db_insert_snapshot(&SnapshotRecord {
        id: short_id.clone(),
        ref_path: ref_path.clone(),
        project_id,
        conversation_id: conv_id.to_string(),
        agent_id: agent_id.to_string(),
        label: label.to_string(),
        files_changed: files_changed_json,
        created_at: created_at.clone(),
        domain: domain_str.clone(),
        asset_key: asset_opt.clone(),
    })?;

    crate::logger::log(
        "INFO",
        "snapshots",
        &format!("Snapshot created: id='{}', domain='{}', asset={:?}, label='{}', project_id={}, files_changed={}", short_id, domain_str, asset_opt, label, project_id, files_changed.len()),
    );
    Ok(Snapshot {
        id: short_id,
        ref_path,
        project_id,
        conversation_id: conv_id.to_string(),
        agent_id: agent_id.to_string(),
        label: label.to_string(),
        files_changed,
        created_at,
        domain: domain_str,
        asset_key: asset_opt,
    })
}

/// Captures an automatic domain-scoped snapshot if the project has Git and `<domain>.auto_snapshot_on_edit` is enabled.
pub async fn try_create_auto_domain_snapshot(
    project_path: PathBuf,
    domain: &'static str,
    asset_key: String,
    agent_id: String,
    label: String,
    scoped_files: Vec<String>,
) -> Result<Option<Snapshot>, SnapshotError> {
    if !project_path.join(".git").exists() {
        return Ok(None);
    }
    let path_str = project_path.to_string_lossy().to_string();
    let enabled = resolve_effective_bool(Some(&path_str), None, domain, "auto_snapshot_on_edit");
    if !enabled {
        return Ok(None);
    }
    let max_allowed = resolve_effective_u64(Some(&path_str), None, domain, "max_snapshots_per_asset_count").max(1) as usize;

    let project = match db_get_project_by_path(&path_str)? {
        Some(p) => p,
        None => return Ok(None),
    };
    let project_id = project.id as i64;

    tokio::task::spawn_blocking(move || {
        let existing: Vec<String> = scoped_files
            .into_iter()
            .filter(|rel| project_path.join(rel).exists())
            .collect();
        if existing.is_empty() {
            return Ok(None);
        }
        let snap = create_domain_snapshot(
            &project_path,
            project_id,
            None,
            &agent_id,
            &label,
            Some(existing),
            domain,
            Some(&asset_key),
            max_allowed,
        )?;
        Ok(Some(snap))
    })
    .await
    .map_err(|e| SnapshotError::Task(e.to_string()))?
}

/// Unified diff from a snapshot of the project to `compare_with` (another of its snapshots),
/// or to the working tree when `None`; optionally limited to one file.
pub fn get_snapshot_diff(
    project_path: &Path,
    project_id: i64,
    id_or_ref: &str,
    compare_with: Option<&str>,
    file_path: Option<&str>,
) -> Result<String, SnapshotError> {
    crate::logger::log("DEBUG", "snapshots", &format!("get_snapshot_diff: project_id={}, snapshot={}, compare_with={:?}", project_id, id_or_ref, compare_with));
    require_repository(project_path)?;
    let source = find_snapshot(project_id, id_or_ref)?.ref_path;
    let target = compare_with.map(|c| find_snapshot(project_id, c)).transpose()?.map(|r| r.ref_path);

    let mut args = vec!["diff", source.as_str()];
    if let Some(t) = target.as_deref() {
        args.push(t);
    }
    args.push("--");
    if let Some(f) = file_path {
        args.push(f);
    }
    git(project_path, &args, None)
}

/// Extracts line-level diff hunks captured by a snapshot ref, diffing against `prev_ref` when it shares the same parent as `HEAD`, or against `HEAD` / empty commit otherwise.
pub fn extract_snapshot_diff_hunks(
    project_path: &Path,
    new_ref: &str,
    prev_ref: Option<&str>,
) -> Result<Vec<crate::kopeng::git::DiffFileHunk>, SnapshotError> {
    require_repository(project_path)?;
    let head_opt = head_commit(project_path)?;
    let use_prev = match (prev_ref, head_opt.as_deref()) {
        (Some(prev), Some(head)) => {
            let prev_parent = git(project_path, &["rev-parse", "--verify", "--quiet", &format!("{}^", prev)], None);
            matches!(prev_parent, Ok(ref p) if p.trim() == head)
        }
        (Some(_), None) => true,
        _ => false,
    };

    let diff_text = if let (true, Some(prev)) = (use_prev, prev_ref) {
        git(
            project_path,
            &[
                "-c",
                "core.quotepath=false",
                "diff",
                "--unified=0",
                "--no-color",
                "--no-renames",
                prev,
                new_ref,
            ],
            None,
        )?
    } else if let Some(head) = head_opt.as_deref() {
        git(
            project_path,
            &[
                "-c",
                "core.quotepath=false",
                "diff",
                "--unified=0",
                "--no-color",
                "--no-renames",
                head,
                new_ref,
            ],
            None,
        )?
    } else {
        git(
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
                new_ref,
            ],
            None,
        )?
    };

    Ok(crate::kopeng::git::parse_unified_zero_diff_hunks(&diff_text))
}

/// Lists snapshots for a project
pub fn list_snapshots(
    project_id: i64,
    conversation_id: Option<&str>,
    agent_id: Option<&str>,
    limit: usize,
) -> Result<Vec<Snapshot>, SnapshotError> {
    db_list_snapshots(project_id, conversation_id, agent_id, limit)?
        .into_iter()
        .map(Snapshot::try_from)
        .collect()
}

/// Restores the listed files (or the whole tree when `files` is `None`) from a snapshot of the project.
///
/// Only the working tree changes; an empty list is rejected.
pub fn restore_snapshot(
    project_path: &Path,
    project_id: i64,
    id_or_ref: &str,
    files: Option<Vec<String>>,
) -> Result<Vec<String>, SnapshotError> {
    crate::logger::log("DEBUG", "snapshots", &format!("restore_snapshot: project_id={}, snapshot={}, files={:?}", project_id, id_or_ref, files));
    require_repository(project_path)?;
    if files.as_ref().is_some_and(|f| f.is_empty()) {
        return Err(SnapshotError::InvalidRequest("empty file list; omit `files` to restore the whole snapshot".to_string()));
    }
    let rec = find_snapshot(project_id, id_or_ref)?;
    let restored = restore_from_ref(project_path, &rec.ref_path, files.as_deref()).inspect_err(|e| {
        crate::logger::log("ERROR", "snapshots", &format!("Snapshot restoration failure for project {} from '{}': {}", project_id, rec.ref_path, e));
    })?;
    crate::logger::log("INFO", "snapshots", &format!("Snapshot restored: '{}', project_id={}, files={:?}", rec.ref_path, project_id, restored));
    Ok(restored)
}

/// Deletes a snapshot of the project (its ref and its record).
pub fn delete_snapshot(project_path: &Path, project_id: i64, id_or_ref: &str) -> Result<(), SnapshotError> {
    crate::logger::log("DEBUG", "snapshots", &format!("delete_snapshot: project_id={}, snapshot={}", project_id, id_or_ref));
    require_repository(project_path)?;
    let rec = find_snapshot(project_id, id_or_ref)?;
    delete_snapshot_record(project_path, &rec.ref_path)?;
    crate::logger::log("INFO", "snapshots", &format!("Snapshot deleted: '{}' in {}", rec.ref_path, project_path.display()));
    Ok(())
}

/// Runs snapshot work for a registered project on the blocking pool (git and SQLite).
async fn on_project<T: Send + 'static>(
    project_id: i64,
    work: impl FnOnce(&Path) -> Result<T, SnapshotError> + Send + 'static,
) -> Result<T, SnapshotError> {
    tokio::task::spawn_blocking(move || {
        let project = crate::registry::find_project_by_id(project_id as usize)?;
        work(Path::new(&project.path))
    })
    .await
    .map_err(|e| SnapshotError::Task(e.to_string()))?
}

/// Arguments of `create_project_snapshot`; without `explicit_files` every changed file is kept.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CreateProjectSnapshotArgs {
    #[ts(type = "number")]
    pub project_id: i64,
    pub agent_id: String,
    pub label: String,
    pub explicit_files: Option<Vec<String>>,
}

/// Arguments of `get_snapshot_diff`: against the working tree, or against `compare_with`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct GetSnapshotDiffArgs {
    #[ts(type = "number")]
    pub project_id: i64,
    pub snapshot_id: String,
    pub compare_with: Option<String>,
    pub file_path: Option<String>,
}

/// Arguments of `list_project_snapshots`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ListProjectSnapshotsArgs {
    #[ts(type = "number")]
    pub project_id: i64,
    pub conversation_id: Option<String>,
    pub agent_id: Option<String>,
    pub limit_count: usize,
}

/// Arguments of `restore_project_snapshot`; without `files` the whole snapshot is restored.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct RestoreProjectSnapshotArgs {
    #[ts(type = "number")]
    pub project_id: i64,
    pub snapshot_id: String,
    pub files: Option<Vec<String>>,
}

/// Arguments of `delete_project_snapshot`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct DeleteProjectSnapshotArgs {
    #[ts(type = "number")]
    pub project_id: i64,
    pub snapshot_id: String,
}

/// Daemon command `create_project_snapshot`: a manual snapshot from the UI
/// (`conversation_id` = `manual`).
pub async fn create_project_snapshot(args: CreateProjectSnapshotArgs) -> Result<Snapshot, SnapshotError> {
    let CreateProjectSnapshotArgs { project_id, agent_id, label, explicit_files } = args;
    on_project(project_id, move |path| create_snapshot(path, project_id, None, &agent_id, &label, explicit_files)).await
}

/// Daemon command `get_snapshot_diff`.
pub async fn get_snapshot_diff_command(args: GetSnapshotDiffArgs) -> Result<String, SnapshotError> {
    let GetSnapshotDiffArgs { project_id, snapshot_id, compare_with, file_path } = args;
    on_project(project_id, move |path| {
        get_snapshot_diff(path, project_id, &snapshot_id, compare_with.as_deref(), file_path.as_deref())
    })
    .await
}

/// Daemon command `list_project_snapshots`.
pub fn list_project_snapshots(args: ListProjectSnapshotsArgs) -> Result<Vec<Snapshot>, SnapshotError> {
    list_snapshots(args.project_id, args.conversation_id.as_deref(), args.agent_id.as_deref(), args.limit_count)
}

/// Daemon command `restore_project_snapshot`; returns the restored files.
pub async fn restore_project_snapshot(args: RestoreProjectSnapshotArgs) -> Result<Vec<String>, SnapshotError> {
    let RestoreProjectSnapshotArgs { project_id, snapshot_id, files } = args;
    on_project(project_id, move |path| restore_snapshot(path, project_id, &snapshot_id, files)).await
}

/// Daemon command `delete_project_snapshot`.
pub async fn delete_project_snapshot(args: DeleteProjectSnapshotArgs) -> Result<(), SnapshotError> {
    let DeleteProjectSnapshotArgs { project_id, snapshot_id } = args;
    on_project(project_id, move |path| delete_snapshot(path, project_id, &snapshot_id)).await
}

fn iso_utc(at: chrono::DateTime<chrono::Utc>) -> String {
    at.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn run(repo: &Path, args: &[&str]) {
        let out = Command::new("git").args(args).current_dir(repo).output().unwrap();
        assert!(out.status.success(), "git {:?}: {}", args, String::from_utf8_lossy(&out.stderr));
    }

    fn init_repo(name: &str, with_commit: bool) -> (tempfile::TempDir, PathBuf) {
        let temp_dir = tempfile::tempdir().unwrap();
        let repo = temp_dir.path().join(name);
        fs::create_dir_all(&repo).unwrap();
        run(&repo, &["init", "-q"]);
        run(&repo, &["config", "user.name", "Test User"]);
        run(&repo, &["config", "user.email", "test@example.com"]);
        fs::write(repo.join("file_a.txt"), "initial file a").unwrap();
        fs::write(repo.join("file_b.txt"), "initial file b").unwrap();
        if with_commit {
            run(&repo, &["add", "."]);
            run(&repo, &["commit", "-q", "-m", "Initial base commit"]);
        }
        (temp_dir, repo)
    }

    fn staged_files(repo: &Path) -> Vec<String> {
        output_lines(&git(repo, &["diff", "--cached", "--name-only"], None).unwrap())
    }

    #[test]
    fn working_tree_diff_includes_untracked_files_and_unborn_repos() {
        let (_tmp, repo) = init_repo("wt_diff", true);
        fs::write(repo.join("file_a.txt"), "edited a").unwrap();
        fs::write(repo.join("new file.txt"), "brand new").unwrap();
        fs::write(repo.join(".gitignore"), "ignored.log\n").unwrap();
        fs::write(repo.join("ignored.log"), "noise").unwrap();
        run(&repo, &["add", "file_a.txt"]);

        let diff = diff_working_tree(&repo, None).unwrap();
        assert!(diff.contains("+edited a") && diff.contains("+brand new"), "{}", diff);
        assert!(!diff.contains("noise"), "{}", diff);
        assert_eq!(staged_files(&repo), vec!["file_a.txt"]);
        let scoped = diff_working_tree(&repo, Some("new file.txt")).unwrap();
        assert!(scoped.contains("+brand new") && !scoped.contains("edited a"), "{}", scoped);

        let (_tmp2, unborn) = init_repo("wt_unborn", false);
        assert!(diff_working_tree(&unborn, None).unwrap().contains("+initial file a"));
    }

    #[test]
    fn snapshot_leaves_head_and_user_index_untouched() {
        let (_tmp, repo) = init_repo("snap_head", true);
        let head_before = head_commit(&repo).unwrap();
        fs::write(repo.join("file_a.txt"), "agent edit a").unwrap();
        fs::write(repo.join("file_b.txt"), "user staged b").unwrap();
        run(&repo, &["add", "file_b.txt"]);

        let (commit, files) = capture_snapshot_commit(&repo, &[], &[], "oneship-snapshot: test").unwrap();

        assert_eq!(head_commit(&repo).unwrap(), head_before);
        assert_eq!(staged_files(&repo), vec!["file_b.txt"]);
        assert_eq!(files, vec!["file_a.txt", "file_b.txt"]);
        assert_eq!(git(&repo, &["show", &format!("{}:file_a.txt", commit)], None).unwrap(), "agent edit a");
    }

    #[test]
    fn snapshot_of_scoped_files_keeps_other_files_at_head() {
        let (_tmp, repo) = init_repo("snap_scoped", true);
        fs::write(repo.join("file_a.txt"), "agent edit a").unwrap();
        fs::write(repo.join("file_b.txt"), "unrelated edit b").unwrap();

        let (commit, files) = capture_snapshot_commit(&repo, &["file_a.txt".to_string()], &[], "oneship-snapshot: scoped").unwrap();

        assert_eq!(files, vec!["file_a.txt"]);
        assert_eq!(git(&repo, &["show", &format!("{}:file_b.txt", commit)], None).unwrap(), "initial file b");
    }

    #[test]
    fn snapshot_in_repository_without_commits_creates_no_branch() {
        let (_tmp, repo) = init_repo("snap_unborn", false);

        let (commit, files) = capture_snapshot_commit(&repo, &[], &[], "oneship-snapshot: unborn").unwrap();

        assert_eq!(head_commit(&repo).unwrap(), None);
        assert_eq!(git(&repo, &["for-each-ref", "refs/heads"], None).unwrap(), "");
        assert_eq!(files, vec!["file_a.txt", "file_b.txt"]);
        assert!(!commit.is_empty());
    }

    #[test]
    fn partial_restore_changes_only_listed_files() {
        let (_tmp, repo) = init_repo("restore_partial", true);
        let (commit, _) = capture_snapshot_commit(&repo, &[], &[], "oneship-snapshot: pre").unwrap();
        run(&repo, &["update-ref", "refs/one-ship/snapshots/1/test", &commit]);
        fs::write(repo.join("file_a.txt"), "agent edit a").unwrap();
        fs::write(repo.join("file_b.txt"), "agent edit b").unwrap();

        let restored = restore_from_ref(&repo, "refs/one-ship/snapshots/1/test", Some(&["file_a.txt".to_string()])).unwrap();

        assert_eq!(restored, vec!["file_a.txt"]);
        assert_eq!(fs::read_to_string(repo.join("file_a.txt")).unwrap(), "initial file a");
        assert_eq!(fs::read_to_string(repo.join("file_b.txt")).unwrap(), "agent edit b");
        assert!(staged_files(&repo).is_empty());
    }

    #[test]
    fn ref_components_are_valid_ref_names() {
        assert_eq!(ref_component("Pre-Resolution: docs (estudiorc)"), "Pre-Resolution--docs--estudiorc");
        assert_eq!(ref_component("milestone:3"), "milestone-3");
        assert_eq!(ref_component("../.."), "snapshot");
    }

    #[test]
    fn detect_changed_code_files_is_runner_agnostic_and_excludes_domain_assets() {
        let (_tmp, repo) = init_repo("detect_code", true);
        fs::write(repo.join("file_a.txt"), "edited a").unwrap();
        fs::remove_file(repo.join("file_b.txt")).unwrap();
        fs::write(repo.join("new_code.rs"), "fn main() {}").unwrap();
        fs::write(repo.join("board.kicad_pcb"), "(kicad_pcb)").unwrap();
        fs::write(repo.join("ui.desigma"), "{}").unwrap();

        let detected = detect_changed_code_files(&repo).unwrap();
        assert!(detected.contains(&"file_a.txt".to_string()), "{:?}", detected);
        assert!(detected.contains(&"file_b.txt".to_string()), "{:?}", detected);
        assert!(detected.contains(&"new_code.rs".to_string()), "{:?}", detected);
        assert!(!detected.iter().any(|p| p.ends_with(".kicad_pcb") || p.ends_with(".desigma")), "{:?}", detected);
    }
}
