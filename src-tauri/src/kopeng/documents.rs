use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::db::DatabaseError;

/// Failures of project document, explorer, comment and attachment operations.
///
/// Serializes as its message, so Tauri commands return it directly and the UI receives text
/// (AGENTS.md §9.3).
#[derive(Debug, thiserror::Error)]
pub enum DocumentsError {
    /// The project directory does not exist.
    #[error("Project directory not found: {0}")]
    ProjectNotFound(String),

    /// A relative path contains traversal or resolves outside the project root.
    #[error("Path '{0}' escapes the project root")]
    PathEscapesRoot(String),

    /// The requested file does not exist (or is not a file).
    #[error("File not found: {0}")]
    FileNotFound(String),

    /// A filesystem operation failed.
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    /// The caller's input is invalid (e.g. malformed base64 image data).
    #[error("{0}")]
    InvalidRequest(String),

    /// The background task running the operation panicked or was cancelled.
    #[error("Background task failed: {0}")]
    Task(String),

    #[error(transparent)]
    Database(#[from] DatabaseError),

    #[error(transparent)]
    Config(#[from] crate::config::ConfigError),
}

impl crate::api::ApiError for DocumentsError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::ProjectNotFound(_) | Self::FileNotFound(_) => StatusCode::NOT_FOUND,
            Self::PathEscapesRoot(_) | Self::InvalidRequest(_) => StatusCode::BAD_REQUEST,
            Self::Io { .. } | Self::Task(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Database(e) => e.status(),
            Self::Config(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::ProjectNotFound(_) => "DocumentsError::ProjectNotFound",
            Self::PathEscapesRoot(_) => "DocumentsError::PathEscapesRoot",
            Self::FileNotFound(_) => "DocumentsError::FileNotFound",
            Self::Io { .. } => "DocumentsError::Io",
            Self::InvalidRequest(_) => "DocumentsError::InvalidRequest",
            Self::Task(_) => "DocumentsError::Task",
            Self::Database(e) => e.code(),
            Self::Config(e) => e.code(),
        }
    }
}

/// Arguments of `get_project_documents`; `scope` limits the listing (`docs`, `scratchpad`).
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct GetProjectDocumentsArgs {
    pub project_path: String,
    pub scope: Option<String>,
}

/// Arguments of the commands on one project-relative path.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ProjectRelPathArgs {
    pub project_path: String,
    pub rel_path: String,
}

/// Arguments of `save_project_document`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SaveProjectDocumentArgs {
    pub project_path: String,
    pub rel_path: String,
    pub content: String,
}

/// Arguments of `create_document_comment`; lines are 1-based and inclusive.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CreateDocumentCommentArgs {
    #[ts(type = "number")]
    pub project_id: i64,
    pub file_path: String,
    #[ts(type = "number")]
    pub start_line: i64,
    #[ts(type = "number")]
    pub end_line: i64,
    pub selected_text: String,
    pub comment_text: String,
    pub author: String,
}

/// Arguments of `list_document_comments`; `file_path` and `status` filter.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ListDocumentCommentsArgs {
    #[ts(type = "number")]
    pub project_id: i64,
    pub file_path: Option<String>,
    pub status: Option<String>,
}

/// Arguments of `resolve_document_comment`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ResolveDocumentCommentArgs {
    pub comment_id: String,
    pub resolved_by: Option<String>,
}

/// Arguments of `delete_document_comment`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct DeleteDocumentCommentArgs {
    pub comment_id: String,
}

/// Arguments of `delete_project_items`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct DeleteProjectItemsArgs {
    pub project_path: String,
    pub rel_paths: Vec<String>,
}

/// Arguments of `copy_project_items` and `move_project_items`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct TransferProjectItemsArgs {
    pub project_path: String,
    pub src_rel_paths: Vec<String>,
    pub dest_folder_rel: String,
}

/// Arguments of `rename_project_item`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct RenameProjectItemArgs {
    pub project_path: String,
    pub old_rel_path: String,
    pub new_rel_path: String,
}

/// Arguments of `save_clipboard_image`: base64 image bytes; `extension` defaults to `png`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SaveClipboardImageArgs {
    pub project_path: String,
    pub image_base64: String,
    pub extension: Option<String>,
}

/// Arguments of `attach_file_to_project`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct AttachFileToProjectArgs {
    pub project_path: String,
    pub source_absolute_path: String,
}

/// `map_err` adapter that attaches a context message to an I/O error.
fn io_err(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> DocumentsError {
    let context = context.into();
    move |source| DocumentsError::Io { context, source }
}

/// Returns the project root, failing when the directory does not exist.
fn require_project_root(project_path: &str) -> Result<&Path, DocumentsError> {
    let root = Path::new(project_path);
    if !root.is_dir() {
        return Err(DocumentsError::ProjectNotFound(project_path.to_string()));
    }
    Ok(root)
}

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct DocumentItem {
    pub rel_path: String,
    pub name: String,
    pub category: String, // "docs" | "scratchpad"
    pub is_dir: bool,
    #[ts(type = "number")]
    pub size_bytes: u64,
    #[ts(type = "number | null")]
    pub mtime_seconds: Option<u64>,
    pub doc_type: String, // "markdown" | "code" | "json" | "diff" | "pdf" | "text"
    pub in_gitignore: bool,
    pub in_semantic_ignore: bool,
    pub is_semantically_indexed: bool,
    pub git_status: Option<String>,
    pub open_comment_count: u32,
    pub children: Option<Vec<DocumentItem>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct DocumentContent {
    pub rel_path: String,
    pub name: String,
    pub category: String,
    pub content: String,
    #[ts(type = "number")]
    pub mtime_seconds: u64,
    #[ts(type = "number")]
    pub size_bytes: u64,
    pub doc_type: String,
    pub doc_lang: Option<String>,
}

pub fn ensure_project_scratchpad_dir(project_path: &Path) -> std::io::Result<PathBuf> {
    let scratchpad = project_path.join("scratchpad");
    if !scratchpad.exists() {
        fs::create_dir_all(&scratchpad)?;
    } else {
        let p_str = project_path.to_string_lossy();
        let retention_days = crate::config::resolve_effective_u64(Some(&p_str), None, "vcs", "scratchpad_retention_days");
        if retention_days > 0 {
            let max_age_seconds = retention_days.saturating_mul(86_400);
            if let Ok(entries) = fs::read_dir(&scratchpad) {
                for entry in entries.flatten().take(1000) {
                    let path = entry.path();
                    if path.is_file() {
                        if let Ok(meta) = path.metadata() {
                            if let Ok(modified) = meta.modified() {
                                if let Ok(elapsed) = modified.elapsed() {
                                    if elapsed.as_secs() > max_age_seconds {
                                        let _ = fs::remove_file(&path);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(scratchpad)
}

fn ensure_attachments_dir(project_path: &Path) -> Result<PathBuf, DocumentsError> {
    let attachments_dir = project_path.join(".one-ship").join("attachments");
    fs::create_dir_all(&attachments_dir).map_err(io_err("Failed to create attachments folder"))?;
    Ok(attachments_dir)
}

fn infer_doc_type(path: &Path) -> (String, Option<String>) {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    match ext.as_str() {
        "md" | "markdown" => ("markdown".to_string(), Some("markdown".to_string())),
        "rs" => ("code".to_string(), Some("rust".to_string())),
        "ts" => ("code".to_string(), Some("typescript".to_string())),
        "js" => ("code".to_string(), Some("javascript".to_string())),
        "json" => ("json".to_string(), Some("json".to_string())),
        "diff" | "patch" => ("diff".to_string(), Some("diff".to_string())),
        "toml" => ("code".to_string(), Some("toml".to_string())),
        "yaml" | "yml" => ("code".to_string(), Some("yaml".to_string())),
        "py" => ("code".to_string(), Some("python".to_string())),
        "sh" | "bash" => ("code".to_string(), Some("bash".to_string())),
        "vue" => ("code".to_string(), Some("vue".to_string())),
        "html" => ("code".to_string(), Some("html".to_string())),
        "css" => ("code".to_string(), Some("css".to_string())),
        "pdf" => ("pdf".to_string(), Some("pdf".to_string())),
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "svg" => ("image".to_string(), Some("image".to_string())),
        _ => ("text".to_string(), None),
    }
}

struct DocumentTreeContext<'a> {
    gitignore_patterns: &'a [String],
    semantic_ignored_patterns: &'a [String],
    open_comment_counts: &'a HashMap<String, u32>,
    indexed_files: &'a HashSet<String>,
    git_statuses: &'a HashMap<String, String>,
}

fn git_status_priority(status: &str) -> u8 {
    match status {
        "conflict" => 5,
        "modified" => 4,
        "added" => 3,
        "renamed" => 2,
        "untracked" => 1,
        _ => 0,
    }
}

fn aggregate_children_git_status(children: &[DocumentItem]) -> Option<String> {
    let mut best: Option<&str> = None;
    let mut best_prio = 0u8;
    for child in children {
        if child.in_gitignore {
            continue;
        }
        if let Some(ref st) = child.git_status {
            let prio = git_status_priority(st);
            if prio > best_prio {
                best_prio = prio;
                best = Some(st.as_str());
            }
        }
    }
    best.map(|s| s.to_string())
}

fn load_gitignore_patterns(root: &Path) -> Result<Vec<String>, DocumentsError> {
    let gitignore_path = root.join(".gitignore");
    if !gitignore_path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&gitignore_path).map_err(io_err("Failed to read .gitignore"))?;
    Ok(content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with('!'))
        .map(|l| l.to_string())
        .collect())
}

fn matches_gitignore_patterns(rel_path: &str, patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return false;
    }
    let normalized = rel_path.replace('\\', "/");
    let segments: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();

    for raw_pattern in patterns {
        let anchored = raw_pattern.starts_with('/');
        let pat = raw_pattern.trim_start_matches('/').trim_end_matches('/');
        if pat.is_empty() {
            continue;
        }
        if let Some(suffix) = pat.strip_prefix('*') {
            if normalized.ends_with(suffix) || segments.last().is_some_and(|name| name.ends_with(suffix)) {
                return true;
            }
        } else if let Some(prefix) = pat.strip_suffix('*') {
            if normalized.starts_with(prefix) || (!anchored && segments.iter().any(|s| s.starts_with(prefix))) {
                return true;
            }
        } else if normalized == pat || normalized.starts_with(&format!("{}/", pat)) {
            return true;
        } else if !anchored && !pat.contains('/') && segments.contains(&pat) {
            return true;
        }
    }
    false
}

fn build_doc_tree(
    base_dir: &Path,
    current_rel: &str,
    category: &str,
    depth: usize,
    max_depth_count: usize,
    visited_nodes_count: &mut usize,
    max_nodes_count: usize,
    parent_in_gitignore: bool,
    parent_in_semantic_ignore: bool,
    ctx: &DocumentTreeContext<'_>,
) -> Vec<DocumentItem> {
    if depth > max_depth_count || *visited_nodes_count >= max_nodes_count {
        return Vec::new();
    }

    let target_dir = base_dir.join(current_rel);
    if !target_dir.is_dir() {
        return Vec::new();
    }

    let mut items = Vec::new();
    let entries = match fs::read_dir(&target_dir) {
        Ok(e) => e,
        Err(_) => return items,
    };

    for entry in entries.flatten() {
        if *visited_nodes_count >= max_nodes_count {
            break;
        }
        *visited_nodes_count += 1;
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();

        if file_name.starts_with('.') {
            continue;
        }

        let rel_child = if current_rel.is_empty() {
            file_name.clone()
        } else {
            format!("{}/{}", current_rel, file_name)
        };

        let is_dir = path.is_dir();
        let metadata = fs::metadata(&path).ok();
        let size_bytes = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
        let mtime = metadata
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());

        let (doc_type, _lang) = if is_dir {
            ("directory".to_string(), None)
        } else {
            infer_doc_type(&path)
        };

        let full_rel_path = format!("{}/{}", category, rel_child);
        let in_gitignore = parent_in_gitignore || matches_gitignore_patterns(&full_rel_path, ctx.gitignore_patterns);
        let in_semantic_ignore =
            parent_in_semantic_ignore || crate::semantic::is_path_ignored(&full_rel_path, ctx.semantic_ignored_patterns);
        let open_comment_count = if is_dir {
            0
        } else {
            match ctx.open_comment_counts.get(&full_rel_path) {
                Some(&c) => c,
                None => 0,
            }
        };

        let is_semantically_indexed = !is_dir && !in_semantic_ignore && ctx.indexed_files.contains(&full_rel_path);

        let children = if is_dir {
            Some(build_doc_tree(
                base_dir,
                &rel_child,
                category,
                depth + 1,
                max_depth_count,
                visited_nodes_count,
                max_nodes_count,
                in_gitignore,
                in_semantic_ignore,
                ctx,
            ))
        } else {
            None
        };

        let git_status = if in_gitignore {
            None
        } else if let Some(ref ch) = children {
            aggregate_children_git_status(ch)
        } else {
            ctx.git_statuses.get(&full_rel_path).cloned()
        };

        items.push(DocumentItem {
            rel_path: full_rel_path,
            name: file_name,
            category: category.to_string(),
            is_dir,
            size_bytes,
            mtime_seconds: mtime,
            doc_type,
            in_gitignore,
            in_semantic_ignore,
            is_semantically_indexed,
            git_status,
            open_comment_count,
            children,
        });
    }

    // Sort: directories first, then alphabetically
    items.sort_by(|a, b| {
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    items
}

fn is_ignored_directory_or_file(name: &str) -> bool {
    matches!(
        name,
        ".git" | "target" | "node_modules" | "dist" | ".gemini" | "package-lock.json" | "Cargo.lock" | ".DS_Store" | "tmp"
    )
}

const PROJECT_WATCH_DEBOUNCE_MS: u64 = 350;
const MAX_PROJECT_WATCH_BURST_PATHS_COUNT: usize = 256;

static PROJECT_FILE_WATCHERS: OnceLock<Mutex<HashMap<PathBuf, CancellationToken>>> = OnceLock::new();

fn project_file_watchers() -> &'static Mutex<HashMap<PathBuf, CancellationToken>> {
    PROJECT_FILE_WATCHERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn path_has_visible_explorer_change(root: &Path, event_path: &Path) -> bool {
    let Ok(rel) = event_path.strip_prefix(root) else {
        return false;
    };
    let mut has_component = false;
    for comp in rel.components() {
        if let Component::Normal(os_name) = comp {
            has_component = true;
            if is_ignored_directory_or_file(&os_name.to_string_lossy()) {
                return false;
            }
        }
    }
    has_component
}

/// Starts a debounced recursive `notify` watcher for `root` if a Tokio runtime is active
/// and emits `project-files-changed` whenever non-ignored files or directories change.
fn ensure_project_files_watcher(root: &Path) {
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let project_path = root.display().to_string();

    let token = {
        let mut guard = match project_file_watchers().lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if guard.contains_key(&canonical_root) {
            return;
        }
        let token = CancellationToken::new();
        guard.insert(canonical_root.clone(), token.clone());
        token
    };

    handle.spawn(async move {
        use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

        let (tx, mut rx) = tokio::sync::mpsc::channel::<PathBuf>(MAX_PROJECT_WATCH_BURST_PATHS_COUNT);
        let root_for_cb = canonical_root.clone();

        let watcher_res = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                let Ok(ev) = res else { return; };
                if matches!(ev.kind, EventKind::Access(_)) {
                    return;
                }
                for p in ev.paths {
                    if path_has_visible_explorer_change(&root_for_cb, &p) {
                        let _ = tx.try_send(p);
                    }
                }
            },
            Config::default(),
        );

        let mut watcher = match watcher_res {
            Ok(w) => w,
            Err(e) => {
                crate::logger::log("WARN", "documents", &format!("Cannot create project watcher for {}: {}", project_path, e));
                project_file_watchers().lock().unwrap_or_else(|p| p.into_inner()).remove(&canonical_root);
                return;
            }
        };

        if let Err(e) = watcher.watch(&canonical_root, RecursiveMode::Recursive) {
            crate::logger::log("WARN", "documents", &format!("Cannot watch project {}: {}", project_path, e));
            project_file_watchers().lock().unwrap_or_else(|p| p.into_inner()).remove(&canonical_root);
            return;
        }

        loop {
            tokio::select! {
                _ = token.cancelled() => break,
                maybe_path = rx.recv() => {
                    let Some(first_path) = maybe_path else { break; };
                    let mut changed = HashSet::with_capacity(16);
                    changed.insert(first_path);

                    tokio::time::sleep(std::time::Duration::from_millis(PROJECT_WATCH_DEBOUNCE_MS)).await;
                    while changed.len() < MAX_PROJECT_WATCH_BURST_PATHS_COUNT {
                        match rx.try_recv() {
                            Ok(extra) => { changed.insert(extra); }
                            Err(_) => break,
                        }
                    }

                    if !canonical_root.exists() {
                        break;
                    }

                    let payload = crate::events::ProjectFilesChangedPayload {
                        project_path: project_path.clone(),
                        changed_count: changed.len(),
                    };
                    crate::supervisor::get_supervisor().emit_event(
                        crate::events::DaemonEvent::project_files_changed(&payload),
                    );
                    crate::semantic::schedule_background_indexing_for_path(&project_path);
                }
            }
        }

        project_file_watchers().lock().unwrap_or_else(|p| p.into_inner()).remove(&canonical_root);
    });
}

fn build_codebase_tree(
    base_dir: &Path,
    current_rel: &str,
    depth: usize,
    max_depth_count: usize,
    visited_nodes_count: &mut usize,
    max_nodes_count: usize,
    parent_in_gitignore: bool,
    parent_in_semantic_ignore: bool,
    ctx: &DocumentTreeContext<'_>,
) -> Vec<DocumentItem> {
    if depth > max_depth_count || *visited_nodes_count >= max_nodes_count {
        return Vec::new();
    }

    let target_dir = if current_rel.is_empty() {
        base_dir.to_path_buf()
    } else {
        base_dir.join(current_rel)
    };

    if !target_dir.is_dir() {
        return Vec::new();
    }

    let mut items = Vec::new();
    let entries = match fs::read_dir(&target_dir) {
        Ok(e) => e,
        Err(_) => return items,
    };

    for entry in entries.flatten() {
        if *visited_nodes_count >= max_nodes_count {
            break;
        }
        *visited_nodes_count += 1;
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();

        if is_ignored_directory_or_file(&file_name) {
            continue;
        }

        let rel_child = if current_rel.is_empty() {
            file_name.clone()
        } else {
            format!("{}/{}", current_rel, file_name)
        };

        let is_dir = path.is_dir();
        let metadata = fs::metadata(&path).ok();
        let size_bytes = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
        let mtime = metadata
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());

        let (doc_type, _lang) = if is_dir {
            ("directory".to_string(), None)
        } else {
            infer_doc_type(&path)
        };

        let in_gitignore = parent_in_gitignore || matches_gitignore_patterns(&rel_child, ctx.gitignore_patterns);
        let in_semantic_ignore =
            parent_in_semantic_ignore || crate::semantic::is_path_ignored(&rel_child, ctx.semantic_ignored_patterns);
        let open_comment_count = if is_dir {
            0
        } else {
            match ctx.open_comment_counts.get(&rel_child) {
                Some(&c) => c,
                None => 0,
            }
        };

        let is_semantically_indexed = !is_dir && !in_semantic_ignore && ctx.indexed_files.contains(&rel_child);

        let children = if is_dir {
            Some(build_codebase_tree(
                base_dir,
                &rel_child,
                depth + 1,
                max_depth_count,
                visited_nodes_count,
                max_nodes_count,
                in_gitignore,
                in_semantic_ignore,
                ctx,
            ))
        } else {
            None
        };

        let git_status = if in_gitignore {
            None
        } else if let Some(ref ch) = children {
            aggregate_children_git_status(ch)
        } else {
            ctx.git_statuses.get(&rel_child).cloned()
        };

        items.push(DocumentItem {
            rel_path: rel_child,
            name: file_name,
            category: "codebase".to_string(),
            is_dir,
            size_bytes,
            mtime_seconds: mtime,
            doc_type,
            in_gitignore,
            in_semantic_ignore,
            is_semantically_indexed,
            git_status,
            open_comment_count,
            children,
        });
    }

    items.sort_by(|a, b| {
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    items
}

pub async fn get_project_documents(project_path: String, scope: Option<String>) -> Result<Vec<DocumentItem>, DocumentsError> {
    let root = require_project_root(&project_path)?.to_path_buf();
    ensure_project_files_watcher(&root);

    let proj_lookup_path = project_path.clone();
    let project_id_opt = tokio::task::spawn_blocking(move || crate::db::db_get_project_by_path(&proj_lookup_path))
        .await
        .map_err(|e| DocumentsError::Task(e.to_string()))??
        .map(|p| p.id as i64);

    let mut indexed_files = if let Some(pid) = project_id_opt {
        match crate::semantic::store::list_indexed_file_paths(pid).await {
            Ok(paths) => paths,
            Err(e) => {
                crate::logger::log("WARN", "documents", &format!("Failed to list LanceDB indexed paths for project {}: {}", pid, e));
                HashSet::new()
            }
        }
    } else {
        HashSet::new()
    };

    tokio::task::spawn_blocking(move || -> Result<Vec<DocumentItem>, DocumentsError> {
        // Auto-create scratchpad directory if absent
        ensure_project_scratchpad_dir(&root).map_err(io_err("Failed to create scratchpad directory"))?;

        let max_depth_count = crate::config::resolve_effective_usize(None, None, "system", "max_dir_walk_depth_count").max(1);
        let max_nodes_count = crate::config::resolve_effective_usize(None, None, "system", "max_dir_walk_nodes_count").max(100);
        let mut visited_nodes_count = 0usize;

        let gitignore_patterns = load_gitignore_patterns(&root)?;
        let semantic_ignored_patterns = crate::config::resolve_effective_string_list(
            Some(&project_path),
            None,
            "semantic",
            "ignored_patterns",
        );

        let open_comment_counts = if let Some(pid) = project_id_opt {
            crate::db::db_get_open_comment_counts_by_file(pid)?
        } else {
            HashMap::new()
        };

        if let Some(pid) = project_id_opt {
            match crate::db::db_get_project_code_symbol_hashes(pid) {
                Ok(code_hashes) => {
                    for rel_path in code_hashes.into_keys() {
                        indexed_files.insert(rel_path);
                    }
                }
                Err(e) => {
                    crate::logger::log("WARN", "documents", &format!("Failed to load code symbol hashes for project {}: {}", pid, e));
                }
            }
        }

        let git_statuses = match crate::kopeng::git::get_working_tree_status_map(&root) {
            Ok(map) => map,
            Err(e) => {
                crate::logger::log("WARN", "documents", &format!("Failed to read git status for '{}': {}", root.display(), e));
                HashMap::new()
            }
        };

        let ctx = DocumentTreeContext {
            gitignore_patterns: &gitignore_patterns,
            semantic_ignored_patterns: &semantic_ignored_patterns,
            open_comment_counts: &open_comment_counts,
            indexed_files: &indexed_files,
            git_statuses: &git_statuses,
        };

        let effective_scope = scope.as_deref().unwrap_or("codebase");
        if effective_scope == "codebase" || effective_scope == "all" {
            return Ok(build_codebase_tree(
                &root,
                "",
                0,
                max_depth_count,
                &mut visited_nodes_count,
                max_nodes_count,
                false,
                false,
                &ctx,
            ));
        }

        let mut result = Vec::new();

        // 1. Scan docs/
        let docs_in_gitignore = matches_gitignore_patterns("docs", &gitignore_patterns);
        let docs_in_semantic_ignore = crate::semantic::is_path_ignored("docs", &semantic_ignored_patterns);
        let docs_dir = root.join("docs");
        let docs_children = if docs_dir.is_dir() {
            build_doc_tree(
                &docs_dir,
                "",
                "docs",
                0,
                max_depth_count,
                &mut visited_nodes_count,
                max_nodes_count,
                docs_in_gitignore,
                docs_in_semantic_ignore,
                &ctx,
            )
        } else {
            Vec::new()
        };
        let docs_git_status = if docs_in_gitignore {
            None
        } else {
            aggregate_children_git_status(&docs_children)
        };

        result.push(DocumentItem {
            rel_path: "docs".to_string(),
            name: "docs".to_string(),
            category: "docs".to_string(),
            is_dir: true,
            size_bytes: 0,
            mtime_seconds: None,
            doc_type: "directory".to_string(),
            in_gitignore: docs_in_gitignore,
            in_semantic_ignore: docs_in_semantic_ignore,
            is_semantically_indexed: false,
            git_status: docs_git_status,
            open_comment_count: 0,
            children: Some(docs_children),
        });

        // 2. Scan scratchpad/
        let scratchpad_in_gitignore = matches_gitignore_patterns("scratchpad", &gitignore_patterns);
        let scratchpad_in_semantic_ignore = crate::semantic::is_path_ignored("scratchpad", &semantic_ignored_patterns);
        let scratchpad_dir = root.join("scratchpad");
        let scratchpad_children = if scratchpad_dir.is_dir() {
            build_doc_tree(
                &scratchpad_dir,
                "",
                "scratchpad",
                0,
                max_depth_count,
                &mut visited_nodes_count,
                max_nodes_count,
                scratchpad_in_gitignore,
                scratchpad_in_semantic_ignore,
                &ctx,
            )
        } else {
            Vec::new()
        };
        let scratchpad_git_status = if scratchpad_in_gitignore {
            None
        } else {
            aggregate_children_git_status(&scratchpad_children)
        };

        result.push(DocumentItem {
            rel_path: "scratchpad".to_string(),
            name: "scratchpad".to_string(),
            category: "scratchpad".to_string(),
            is_dir: true,
            size_bytes: 0,
            mtime_seconds: None,
            doc_type: "directory".to_string(),
            in_gitignore: scratchpad_in_gitignore,
            in_semantic_ignore: scratchpad_in_semantic_ignore,
            is_semantically_indexed: false,
            git_status: scratchpad_git_status,
            open_comment_count: 0,
            children: Some(scratchpad_children),
        });

        Ok(result)
    })
    .await
    .map_err(|e| DocumentsError::Task(e.to_string()))?
}

/// Resolves `rel_path` to an existing file inside `root`, rejecting traversal and symlinks that
/// escape the project. Returns the cleaned relative path and the canonical file path.
fn resolve_existing_project_file<'a>(root: &Path, rel_path: &'a str, allow_skill_symlink: bool) -> Result<(&'a str, PathBuf), DocumentsError> {
    let cleaned_rel = rel_path.trim_start_matches('/');
    if cleaned_rel.contains("..") {
        return Err(DocumentsError::PathEscapesRoot(cleaned_rel.to_string()));
    }
    let full_path = root.join(cleaned_rel);
    if !full_path.is_file() {
        return Err(DocumentsError::FileNotFound(cleaned_rel.to_string()));
    }
    let canonical_root = root.canonicalize().map_err(io_err(format!("Failed to resolve {}", root.display())))?;
    let canonical_file = full_path.canonicalize().map_err(io_err(format!("Failed to resolve {}", cleaned_rel)))?;
    if !canonical_file.starts_with(&canonical_root) {
        if allow_skill_symlink && cleaned_rel.starts_with(".agents/skills/") {
            return Ok((cleaned_rel, canonical_file));
        }
        return Err(DocumentsError::PathEscapesRoot(cleaned_rel.to_string()));
    }
    Ok((cleaned_rel, canonical_file))
}

pub fn read_project_document(project_path: String, rel_path: String) -> Result<DocumentContent, DocumentsError> {
    let root = require_project_root(&project_path)?;
    let (cleaned_rel, canonical_file) = resolve_existing_project_file(root, &rel_path, true)?;

    let content = fs::read_to_string(&canonical_file).map_err(io_err(format!("Failed to read {}", cleaned_rel)))?;
    let metadata = fs::metadata(&canonical_file).map_err(io_err(format!("Failed to stat {}", cleaned_rel)))?;
    let size_bytes = metadata.len();
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let (doc_type, doc_lang) = infer_doc_type(&canonical_file);
    let category = if cleaned_rel.starts_with("docs") {
        "docs".to_string()
    } else if cleaned_rel.starts_with("scratchpad") {
        "scratchpad".to_string()
    } else {
        "codebase".to_string()
    };

    let name = canonical_file.file_name().and_then(|s| s.to_str()).unwrap_or("document").to_string();

    Ok(DocumentContent {
        rel_path: cleaned_rel.to_string(),
        name,
        category,
        content,
        mtime_seconds: mtime,
        size_bytes,
        doc_type,
        doc_lang,
    })
}

pub fn save_project_document(
    project_path: String,
    rel_path: String,
    content: String,
) -> Result<DocumentContent, DocumentsError> {
    let root = require_project_root(&project_path)?;

    let cleaned_rel = rel_path.trim_start_matches('/');
    let target = sanitize_project_rel_path(root, cleaned_rel)?;

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(io_err("Failed to create parent directory"))?;
    }

    fs::write(&target, &content).map_err(io_err(format!("Failed to write {}", cleaned_rel)))?;

    let canonical_file = target.canonicalize().map_err(io_err(format!("Failed to resolve {}", cleaned_rel)))?;
    let metadata = fs::metadata(&canonical_file).map_err(io_err(format!("Failed to stat {}", cleaned_rel)))?;
    let size_bytes = metadata.len();
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let (doc_type, doc_lang) = infer_doc_type(&canonical_file);
    let category = if cleaned_rel.starts_with("docs") {
        "docs".to_string()
    } else if cleaned_rel.starts_with("scratchpad") {
        "scratchpad".to_string()
    } else {
        "codebase".to_string()
    };

    let name = canonical_file.file_name().and_then(|s| s.to_str()).unwrap_or("document").to_string();

    // Automatically create a scoped code snapshot on file save when enabled (Spec 16 §3.1)
    let snapshots_enabled = crate::config::resolve_effective_bool(Some(&project_path), None, "vcs", "snapshots_enabled");
    let auto_on_file_change = crate::config::resolve_effective_bool(Some(&project_path), None, "vcs", "auto_snapshot_on_file_change");
    if snapshots_enabled && auto_on_file_change && root.join(".git").exists() {
        if let Ok(Some(proj)) = crate::db::db_get_project_by_path(&project_path) {
            let snap_label = format!("file-save:{}", name);
            if let Err(e) = crate::kopeng::snapshots::create_snapshot(
                root,
                proj.id as i64,
                None,
                "user",
                &snap_label,
                Some(vec![cleaned_rel.to_string()]),
            ) {
                crate::logger::log(
                    "WARN",
                    "documents",
                    &format!("Auto-snapshot on file save failed for '{}': {}", cleaned_rel, e),
                );
            }
        }
    }

    Ok(DocumentContent {
        rel_path: cleaned_rel.to_string(),
        name,
        category,
        content,
        mtime_seconds: mtime,
        size_bytes,
        doc_type,
        doc_lang,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Document Comment Tauri IPC Commands (Spec 20 Phase 3)
// ─────────────────────────────────────────────────────────────────────────────

use crate::db::{
    db_insert_document_comment, db_list_document_comments, db_resolve_document_comment,
    db_delete_document_comment, DocumentCommentRecord,
};

/// Daemon command `create_document_comment`: an open comment on a line range of a file.
pub fn create_document_comment(args: CreateDocumentCommentArgs) -> Result<DocumentCommentRecord, DocumentsError> {
    let now = crate::db::now_epoch_secs();
    let id = format!("cmt_{}_{}", now, uuid_v4_short());
    let record = DocumentCommentRecord {
        id,
        project_id: args.project_id,
        file_path: args.file_path,
        start_line: args.start_line,
        end_line: args.end_line,
        selected_text: args.selected_text,
        comment_text: args.comment_text,
        author: args.author,
        status: "open".to_string(),
        created_at: now,
        resolved_at: None,
        resolved_by: None,
    };
    db_insert_document_comment(&record)?;
    Ok(record)
}

/// Daemon command `list_document_comments`.
pub fn list_document_comments(args: ListDocumentCommentsArgs) -> Result<Vec<DocumentCommentRecord>, DocumentsError> {
    Ok(db_list_document_comments(args.project_id, args.file_path.as_deref(), args.status.as_deref())?)
}

/// Daemon command `resolve_document_comment`.
pub fn resolve_document_comment(args: ResolveDocumentCommentArgs) -> Result<(), DocumentsError> {
    Ok(db_resolve_document_comment(&args.comment_id, args.resolved_by.as_deref(), crate::db::now_epoch_secs())?)
}

/// Daemon command `delete_document_comment`.
pub fn delete_document_comment(comment_id: String) -> Result<(), DocumentsError> {
    Ok(db_delete_document_comment(&comment_id)?)
}

// ─────────────────────────────────────────────────────────────────────────────
// Workspace File & Directory Operations (Spec 20 Phase 4)
// ─────────────────────────────────────────────────────────────────────────────

/// Resolves `rel_path` (existing or not yet created) inside `root`, rejecting traversal and
/// symlinked parents that escape the project.
fn sanitize_project_rel_path(root: &Path, rel_path: &str) -> Result<PathBuf, DocumentsError> {
    let cleaned = rel_path.trim_start_matches('/');
    if cleaned.contains("..") {
        return Err(DocumentsError::PathEscapesRoot(cleaned.to_string()));
    }
    let full = root.join(cleaned);
    let canonical_root = root.canonicalize().map_err(io_err(format!("Failed to resolve {}", root.display())))?;

    // For non-existent paths (like new folders), verify parent
    if full.exists() {
        let canonical_full = full.canonicalize().map_err(io_err(format!("Failed to resolve {}", cleaned)))?;
        if !canonical_full.starts_with(&canonical_root) {
            return Err(DocumentsError::PathEscapesRoot(cleaned.to_string()));
        }
        return Ok(canonical_full);
    }
    if let Some(parent) = full.parent().filter(|p| p.exists()) {
        let canonical_parent = parent.canonicalize().map_err(io_err(format!("Failed to resolve parent of {}", cleaned)))?;
        if !canonical_parent.starts_with(&canonical_root) {
            return Err(DocumentsError::PathEscapesRoot(cleaned.to_string()));
        }
    }
    Ok(full)
}

fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}

pub fn create_project_directory(project_path: String, rel_path: String) -> Result<(), DocumentsError> {
    let root = require_project_root(&project_path)?;
    let target = sanitize_project_rel_path(root, &rel_path)?;
    fs::create_dir_all(&target).map_err(io_err(format!("Failed to create folder {}", rel_path)))
}

pub fn delete_project_items(project_path: String, rel_paths: Vec<String>) -> Result<(), DocumentsError> {
    let root = require_project_root(&project_path)?;

    for rel in rel_paths {
        if rel.trim().is_empty() || rel == "." || rel == "/" {
            continue;
        }
        let target = sanitize_project_rel_path(root, &rel)?;
        if target.is_dir() {
            fs::remove_dir_all(&target).map_err(io_err(format!("Failed to remove folder {}", rel)))?;
        } else if target.is_file() {
            fs::remove_file(&target).map_err(io_err(format!("Failed to remove file {}", rel)))?;
        }
    }
    Ok(())
}

/// Upper bound of " (n)" suffixes tried when a destination name is taken (AGENTS.md §8.4).
const MAX_UNIQUE_NAME_ATTEMPTS_COUNT: usize = 10_000;

/// Returns `dest_dir/file_name`, or `stem (n).ext` with the first free `n` when it exists.
fn get_unique_target_path(dest_dir: &Path, file_name: &std::ffi::OsStr) -> Result<PathBuf, DocumentsError> {
    let initial_target = dest_dir.join(file_name);
    if !initial_target.exists() {
        return Ok(initial_target);
    }

    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let ext = Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e))
        .unwrap_or_default();

    for counter in 1..=MAX_UNIQUE_NAME_ATTEMPTS_COUNT {
        let candidate_path = dest_dir.join(format!("{} ({}){}", stem, counter, ext));
        if !candidate_path.exists() {
            return Ok(candidate_path);
        }
    }
    Err(DocumentsError::InvalidRequest(format!(
        "No free name for '{}{}' in {} after {} attempts", stem, ext, dest_dir.display(), MAX_UNIQUE_NAME_ATTEMPTS_COUNT
    )))
}

pub fn copy_project_items(
    project_path: String,
    src_rel_paths: Vec<String>,
    dest_folder_rel: String,
) -> Result<(), DocumentsError> {
    let root = require_project_root(&project_path)?;

    let dest_dir = sanitize_project_rel_path(root, &dest_folder_rel)?;
    fs::create_dir_all(&dest_dir).map_err(io_err("Failed to create destination folder"))?;

    for src_rel in src_rel_paths {
        let src_path = sanitize_project_rel_path(root, &src_rel)?;
        let file_name = match src_path.file_name() {
            Some(n) => n,
            None => continue,
        };
        let target_dest = get_unique_target_path(&dest_dir, file_name)?;

        if src_path.is_dir() {
            copy_dir_all(&src_path, &target_dest).map_err(io_err(format!("Failed to copy folder {}", src_rel)))?;
        } else if src_path.is_file() {
            fs::copy(&src_path, &target_dest).map_err(io_err(format!("Failed to copy file {}", src_rel)))?;
        }
    }
    Ok(())
}

pub fn move_project_items(
    project_path: String,
    src_rel_paths: Vec<String>,
    dest_folder_rel: String,
) -> Result<(), DocumentsError> {
    let root = require_project_root(&project_path)?;

    let dest_dir = sanitize_project_rel_path(root, &dest_folder_rel)?;
    fs::create_dir_all(&dest_dir).map_err(io_err("Failed to create destination folder"))?;

    for src_rel in src_rel_paths {
        let src_path = sanitize_project_rel_path(root, &src_rel)?;
        let file_name = match src_path.file_name() {
            Some(n) => n,
            None => continue,
        };

        let initial_dest = dest_dir.join(file_name);
        if initial_dest == src_path {
            continue; // Moving to same exact path is a no-op
        }

        let target_dest = if initial_dest.exists() {
            get_unique_target_path(&dest_dir, file_name)?
        } else {
            initial_dest
        };

        // rename(2) cannot cross filesystems (EXDEV); only that case is done as copy + delete.
        match fs::rename(&src_path, &target_dest) {
            Ok(()) => {}
            Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
                if src_path.is_dir() {
                    copy_dir_all(&src_path, &target_dest).map_err(io_err(format!("Failed to move directory {}", src_rel)))?;
                    fs::remove_dir_all(&src_path).map_err(io_err(format!("Failed to remove source directory {}", src_rel)))?;
                } else {
                    fs::copy(&src_path, &target_dest).map_err(io_err(format!("Failed to move file {}", src_rel)))?;
                    fs::remove_file(&src_path).map_err(io_err(format!("Failed to remove source file {}", src_rel)))?;
                }
            }
            Err(e) => return Err(io_err(format!("Failed to move {}", src_rel))(e)),
        }
    }
    Ok(())
}

pub fn create_project_file(project_path: String, rel_path: String) -> Result<(), DocumentsError> {
    let root = require_project_root(&project_path)?;
    let target = sanitize_project_rel_path(root, &rel_path)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(io_err("Failed to create parent folder"))?;
    }
    if !target.exists() {
        fs::write(&target, "").map_err(io_err(format!("Failed to create file {}", rel_path)))?;
    }
    Ok(())
}

pub fn rename_project_item(project_path: String, old_rel_path: String, new_rel_path: String) -> Result<(), DocumentsError> {
    let root = require_project_root(&project_path)?;
    let src = sanitize_project_rel_path(root, &old_rel_path)?;
    let dst = sanitize_project_rel_path(root, &new_rel_path)?;
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(io_err("Failed to create parent folder"))?;
    }
    // Both paths are inside the project, so rename(2) fails only on a real error.
    fs::rename(&src, &dst).map_err(io_err(format!("Failed to rename {} to {}", old_rel_path, new_rel_path)))
}

fn uuid_v4_short() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    format!("{:06x}", nanos % 0xFFFFFF)
}

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct AttachedFileResult {
    pub rel_path: String,
    pub name: String,
    pub is_external: bool,
    #[ts(type = "number")]
    pub size_bytes: u64,
    pub doc_type: String,
}

fn pick_clipboard_image_mime(types_str: &str) -> Option<(&'static str, &'static str)> {
    if types_str.contains("image/png") {
        Some(("image/png", "png"))
    } else if types_str.contains("image/jpeg") || types_str.contains("image/jpg") {
        Some(("image/jpeg", "jpg"))
    } else if types_str.contains("image/webp") {
        Some(("image/webp", "webp"))
    } else {
        None
    }
}

pub fn paste_clipboard_image(
    project_path: String,
) -> Result<Option<AttachedFileResult>, DocumentsError> {
    use std::process::Command;

    let root = require_project_root(&project_path)?;

    let prefer_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty());
    let has_wl_paste = crate::desktop::resolve_executable("wl-paste").is_some();
    let has_xclip = crate::desktop::resolve_executable("xclip").is_some();

    let (mime, ext, bytes) = if (prefer_wayland && has_wl_paste) || (!has_xclip && has_wl_paste) {
        let list_output = Command::new("wl-paste")
            .arg("--list-types")
            .output()
            .map_err(io_err("Failed to run wl-paste"))?;
        let types_str = String::from_utf8_lossy(&list_output.stdout);
        let Some((mime, ext)) = pick_clipboard_image_mime(&types_str) else {
            return Ok(None);
        };
        let paste_output = Command::new("wl-paste")
            .arg("--type")
            .arg(mime)
            .output()
            .map_err(io_err("Failed to read Wayland clipboard"))?;
        if !paste_output.status.success() || paste_output.stdout.is_empty() {
            return Ok(None);
        }
        (mime, ext, paste_output.stdout)
    } else if has_xclip {
        let list_output = Command::new("xclip")
            .args(["-selection", "clipboard", "-t", "TARGETS", "-o"])
            .output()
            .map_err(io_err("Failed to run xclip"))?;
        let types_str = String::from_utf8_lossy(&list_output.stdout);
        let Some((mime, ext)) = pick_clipboard_image_mime(&types_str) else {
            return Ok(None);
        };
        let paste_output = Command::new("xclip")
            .args(["-selection", "clipboard", "-t", mime, "-o"])
            .output()
            .map_err(io_err("Failed to read X11 clipboard"))?;
        if !paste_output.status.success() || paste_output.stdout.is_empty() {
            return Ok(None);
        }
        (mime, ext, paste_output.stdout)
    } else {
        return Err(DocumentsError::Io {
            context: "Clipboard image paste requires wl-paste (Wayland) or xclip (X11)".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "missing clipboard utility"),
        });
    };
    let _ = mime;
    let size_bytes = bytes.len() as u64;

    let attachments_dir = ensure_attachments_dir(root)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let filename = format!("img_{}_{}.{}", now.as_secs(), uuid_v4_short(), ext);
    let target_path = attachments_dir.join(&filename);

    fs::write(&target_path, &bytes).map_err(io_err("Failed to write clipboard image"))?;

    let rel_path = format!(".one-ship/attachments/{}", filename);

    Ok(Some(AttachedFileResult {
        rel_path,
        name: filename,
        is_external: true,
        size_bytes,
        doc_type: "image".to_string(),
    }))
}

pub fn save_clipboard_image(
    project_path: String,
    image_base64: String,
    extension: Option<String>,
) -> Result<AttachedFileResult, DocumentsError> {
    use base64::Engine as _;
    let root = require_project_root(&project_path)?;

    let attachments_dir = ensure_attachments_dir(root)?;

    let clean_base64 = if let Some(idx) = image_base64.find(',') {
        &image_base64[idx + 1..]
    } else {
        &image_base64
    };

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(clean_base64.trim())
        .map_err(|e| DocumentsError::InvalidRequest(format!("Invalid base64 image data: {}", e)))?;

    let ext = extension.unwrap_or_else(|| "png".to_string());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let filename = format!("img_{}_{}.{}", now.as_secs(), uuid_v4_short(), ext);
    let target_path = attachments_dir.join(&filename);

    fs::write(&target_path, &bytes).map_err(io_err("Failed to write image file"))?;

    let rel_path = format!(".one-ship/attachments/{}", filename);
    let size_bytes = bytes.len() as u64;

    Ok(AttachedFileResult {
        rel_path,
        name: filename,
        is_external: true,
        size_bytes,
        doc_type: "image".to_string(),
    })
}

pub fn attach_file_to_project(
    project_path: String,
    source_absolute_path: String,
) -> Result<AttachedFileResult, DocumentsError> {
    let root = require_project_root(&project_path)?;

    let src = PathBuf::from(&source_absolute_path);
    if !src.is_file() {
        return Err(DocumentsError::FileNotFound(source_absolute_path));
    }

    let file_name = src.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("attached_file")
        .to_string();

    let (doc_type, _) = infer_doc_type(&src);
    let metadata = fs::metadata(&src).map_err(io_err("Failed to read file metadata"))?;
    let size_bytes = metadata.len();

    // Check if source file is already inside the project root
    let canonical_root = root.canonicalize().map_err(io_err(format!("Failed to resolve {}", root.display())))?;
    let canonical_src = src.canonicalize().map_err(io_err(format!("Failed to resolve {}", src.display())))?;

    if let Ok(rel) = canonical_src.strip_prefix(&canonical_root) {
        // Internal file within project
        let rel_str = rel.to_string_lossy().to_string();
        return Ok(AttachedFileResult {
            rel_path: rel_str,
            name: file_name,
            is_external: false,
            size_bytes,
            doc_type,
        });
    }

    // External file: Copy into .one-ship/attachments
    let attachments_dir = ensure_attachments_dir(root)?;

    let dest_name = if attachments_dir.join(&file_name).exists() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        format!("{}_{}", now.as_secs(), file_name)
    } else {
        file_name.clone()
    };

    let target_dest = attachments_dir.join(&dest_name);
    fs::copy(&src, &target_dest).map_err(io_err("Failed to copy attachment to project"))?;

    let rel_path = format!(".one-ship/attachments/{}", dest_name);

    Ok(AttachedFileResult {
        rel_path,
        name: dest_name,
        is_external: true,
        size_bytes,
        doc_type,
    })
}

/// Opens a project file in the system default external editor/application.
///
/// Invariants:
/// - Resolves path within `project_path` safely, preventing directory traversal.
/// - Verifies that the target file exists on disk.
/// - Offloads the process spawn to `tokio::task::spawn_blocking` to avoid blocking the Tokio runtime.
/// - Opens it with `xdg-open` in its own desktop scope (`desktop::desktop_command`), so the editor
///   outlives a daemon restart.
///
/// Daemon command `open_file_in_external_editor`.
pub async fn open_file_in_external_editor(
    project_path: String,
    rel_path: String,
) -> Result<(), DocumentsError> {
    crate::logger::log(
        "DEBUG",
        "documents",
        &format!("open_file_in_external_editor entry: project_path='{}', rel_path='{}'", project_path, rel_path),
    );

    let root = require_project_root(&project_path)?;
    let (_, canonical_file) = resolve_existing_project_file(root, &rel_path, false)?;

    crate::logger::log(
        "INFO",
        "documents",
        &format!("open_file_in_external_editor: opening '{:?}' in system editor", canonical_file),
    );

    tokio::task::spawn_blocking(move || {
        let mut cmd = crate::desktop::desktop_command("xdg-open");
        cmd.arg(&canonical_file);
        crate::desktop::spawn_detached(cmd, &format!("xdg-open {}", canonical_file.display()))
            .map_err(io_err(format!("Failed to open {} in the external editor", canonical_file.display())))
    })
    .await
    .map_err(|e| DocumentsError::Task(e.to_string()))?
}

/// Exclusion state of a workspace file or directory across `.gitignore` and `[plugins.semantic]`.
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ProjectExclusionStatus {
    pub rel_path: String,
    pub has_git: bool,
    pub in_gitignore: bool,
    pub in_semantic_ignore: bool,
}

/// Arguments for `get_project_exclusion_status`.
#[derive(Debug, Clone, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct GetProjectExclusionStatusArgs {
    pub project_path: String,
    pub rel_path: String,
    pub is_dir: bool,
}

/// Arguments for `toggle_project_exclusion`.
#[derive(Debug, Clone, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ToggleProjectExclusionArgs {
    pub project_path: String,
    pub rel_path: String,
    pub is_dir: bool,
    /// Target exclusion list: `"gitignore"` or `"semantic"`.
    pub target: String,
}

fn normalize_exclusion_rel_path(rel_path: &str) -> Result<String, DocumentsError> {
    let trimmed = rel_path.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Err(DocumentsError::InvalidRequest("rel_path must not be empty".into()));
    }
    let rel = Path::new(trimmed);
    for comp in rel.components() {
        if matches!(comp, Component::ParentDir | Component::RootDir | Component::Prefix(_)) {
            return Err(DocumentsError::PathEscapesRoot(rel_path.to_string()));
        }
    }
    Ok(trimmed.replace('\\', "/"))
}

fn gitignore_matches(line: &str, norm_rel: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return false;
    }
    let clean = trimmed.trim_start_matches('/').trim_end_matches('/');
    clean == norm_rel
}

fn read_exclusion_status_sync(
    root: &Path,
    project_path: &str,
    norm_rel: &str,
) -> Result<ProjectExclusionStatus, DocumentsError> {
    let has_git = root.join(".git").exists();
    let gitignore_path = root.join(".gitignore");
    let in_gitignore = if gitignore_path.exists() {
        let content = fs::read_to_string(&gitignore_path)
            .map_err(io_err("Failed to read .gitignore"))?;
        content.lines().any(|l| gitignore_matches(l, norm_rel))
    } else {
        false
    };

    let ignored_patterns = crate::config::resolve_effective_string_list(
        Some(project_path),
        None,
        "semantic",
        "ignored_patterns",
    );
    let in_semantic_ignore = ignored_patterns.iter().any(|p| {
        let clean = p.trim().trim_start_matches('/').trim_end_matches('/');
        clean == norm_rel
    });

    Ok(ProjectExclusionStatus {
        rel_path: norm_rel.to_string(),
        has_git,
        in_gitignore,
        in_semantic_ignore,
    })
}

/// Returns whether a relative file/directory path is currently excluded in `.gitignore` or `[plugins.semantic]`.
pub async fn get_project_exclusion_status(
    args: GetProjectExclusionStatusArgs,
) -> Result<ProjectExclusionStatus, DocumentsError> {
    let _ = args.is_dir;
    let norm_rel = normalize_exclusion_rel_path(&args.rel_path)?;
    let root = require_project_root(&args.project_path)?.to_path_buf();
    let project_path = args.project_path;
    tokio::task::spawn_blocking(move || read_exclusion_status_sync(&root, &project_path, &norm_rel))
        .await
        .map_err(|e| DocumentsError::Task(e.to_string()))?
}

/// Adds or removes a relative file/directory path from `.gitignore` or `[plugins.semantic].ignored_patterns`.
pub async fn toggle_project_exclusion(
    args: ToggleProjectExclusionArgs,
) -> Result<ProjectExclusionStatus, DocumentsError> {
    let norm_rel = normalize_exclusion_rel_path(&args.rel_path)?;
    let root = require_project_root(&args.project_path)?.to_path_buf();
    let project_path = args.project_path;
    let is_dir = args.is_dir;
    let target = args.target;

    let status = tokio::task::spawn_blocking(move || -> Result<ProjectExclusionStatus, DocumentsError> {
        match target.as_str() {
            "gitignore" => {
                let gitignore_path = root.join(".gitignore");
                let existing = if gitignore_path.exists() {
                    fs::read_to_string(&gitignore_path).map_err(io_err("Failed to read .gitignore"))?
                } else {
                    String::new()
                };
                let already_present = existing.lines().any(|l| gitignore_matches(l, &norm_rel));
                let next_content = if already_present {
                    let kept: Vec<&str> = existing
                        .lines()
                        .filter(|l| !gitignore_matches(l, &norm_rel))
                        .collect();
                    if kept.is_empty() {
                        String::new()
                    } else {
                        format!("{}\n", kept.join("\n"))
                    }
                } else {
                    let entry = if is_dir {
                        format!("{}/", norm_rel)
                    } else {
                        norm_rel.clone()
                    };
                    if existing.is_empty() {
                        format!("{}\n", entry)
                    } else if existing.ends_with('\n') {
                        format!("{}{}\n", existing, entry)
                    } else {
                        format!("{}\n{}\n", existing, entry)
                    }
                };
                fs::write(&gitignore_path, next_content)
                    .map_err(io_err("Failed to write .gitignore"))?;
            }
            "semantic" => {
                let mut patterns = crate::config::resolve_effective_string_list(
                    Some(&project_path),
                    None,
                    "semantic",
                    "ignored_patterns",
                );
                let before_len = patterns.len();
                patterns.retain(|p| {
                    let clean = p.trim().trim_start_matches('/').trim_end_matches('/');
                    clean != norm_rel
                });
                if patterns.len() == before_len {
                    patterns.push(norm_rel.clone());
                }
                crate::config::set_project_config_field(
                    &project_path,
                    "semantic",
                    "ignored_patterns",
                    serde_json::json!(patterns),
                )?;
                crate::semantic::schedule_background_indexing_for_path(&project_path);
            }
            other => {
                return Err(DocumentsError::InvalidRequest(format!(
                    "Invalid exclusion target '{}': expected 'gitignore' or 'semantic'",
                    other
                )));
            }
        }
        read_exclusion_status_sync(&root, &project_path, &norm_rel)
    })
    .await
    .map_err(|e| DocumentsError::Task(e.to_string()))??;

    Ok(status)
}

