use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::registry::RegistryError;

/// Build and dependency directories never listed. Hidden entries are skipped too, except `.agents`.
const EXCLUDED_DIR_NAMES: [&str; 3] = ["node_modules", "target", "dist"];
const VISIBLE_HIDDEN_NAME: &str = ".agents";

/// Errors of the project file tree scan.
#[derive(Debug, thiserror::Error)]
pub enum FsTreeError {
    #[error("{context}: {source}")]
    Io { context: String, source: std::io::Error },
    #[error("Background task failed: {0}")]
    Task(String),
    #[error(transparent)]
    Registry(#[from] RegistryError),
}

impl crate::api::ApiError for FsTreeError {
    fn status(&self) -> axum::http::StatusCode {
        match self {
            Self::Io { .. } | Self::Task(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Self::Registry(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::Io { .. } => "FsTreeError::Io",
            Self::Task(_) => "FsTreeError::Task",
            Self::Registry(e) => e.code(),
        }
    }
}

/// One entry of the tree. Symlinks are never followed (no loops): they are listed as files.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct FileTreeNode {
    pub name: String,
    pub rel_path: String,
    pub is_dir: bool,
    pub children: Option<Vec<FileTreeNode>>,
    /// A directory whose children were cut by the depth or node limit.
    pub truncated: bool,
}

/// The project tree; `truncated` when any directory was cut by a limit.
#[derive(Serialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct FileTree {
    pub nodes: Vec<FileTreeNode>,
    pub truncated: bool,
}

struct TreeWalker<'a> {
    base: &'a Path,
    max_depth_count: usize,
    max_nodes_count: usize,
    nodes_count: usize,
    truncated: bool,
}

fn is_listed(name: &str) -> bool {
    (!name.starts_with('.') || name == VISIBLE_HIDDEN_NAME) && !EXCLUDED_DIR_NAMES.contains(&name)
}

impl TreeWalker<'_> {
    /// Children of `rel` (directories first, then by name) and whether they were cut by a limit.
    fn walk(&mut self, rel: &str, depth_count: usize) -> Result<(Vec<FileTreeNode>, bool), FsTreeError> {
        if depth_count > self.max_depth_count {
            self.truncated = true;
            return Ok((Vec::new(), true));
        }
        let dir = if rel.is_empty() { self.base.to_path_buf() } else { self.base.join(rel) };
        let io = |e: std::io::Error| FsTreeError::Io { context: format!("Cannot read {}", dir.display()), source: e };

        let mut entries = Vec::new();
        for entry in std::fs::read_dir(&dir).map_err(io)? {
            let entry = entry.map_err(io)?;
            let name = entry.file_name().to_string_lossy().to_string();
            if !is_listed(&name) {
                continue;
            }
            let is_dir = entry.file_type().map_err(io)?.is_dir();
            entries.push((is_dir, name));
        }
        entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

        let mut nodes = Vec::with_capacity(entries.len());
        for (is_dir, name) in entries {
            if self.nodes_count >= self.max_nodes_count {
                self.truncated = true;
                return Ok((nodes, true));
            }
            self.nodes_count += 1;
            let rel_path = if rel.is_empty() { name.clone() } else { format!("{}/{}", rel, name) };
            let (children, truncated) = if is_dir {
                let (children, truncated) = self.walk(&rel_path, depth_count + 1)?;
                (Some(children), truncated)
            } else {
                (None, false)
            };
            nodes.push(FileTreeNode { name, rel_path, is_dir, children, truncated });
        }
        Ok((nodes, false))
    }
}

fn build_tree(base: &Path, max_depth_count: usize, max_nodes_count: usize) -> Result<FileTree, FsTreeError> {
    let mut walker = TreeWalker { base, max_depth_count, max_nodes_count, nodes_count: 0, truncated: false };
    let (nodes, _) = walker.walk("", 0)?;
    Ok(FileTree { nodes, truncated: walker.truncated })
}

/// File tree of a registered project, bounded by `system.fs_tree_max_depth_count` and
/// `system.fs_tree_max_nodes_count`. An unreadable directory or entry is an error.
/// Daemon command `get_project_file_tree`.
pub async fn get_project_file_tree(project_id: usize) -> Result<FileTree, FsTreeError> {
    crate::logger::log("DEBUG", "fs_tree", &format!("get_project_file_tree: project_id={}", project_id));
    let max_depth_count = crate::config::resolve_effective_usize(None, None, "system", "fs_tree_max_depth_count");
    let max_nodes_count = crate::config::resolve_effective_usize(None, None, "system", "fs_tree_max_nodes_count");
    let tree = tokio::task::spawn_blocking(move || {
        let project = crate::registry::find_project_by_id(project_id)?;
        build_tree(Path::new(&project.path), max_depth_count, max_nodes_count)
    })
    .await
    .map_err(|e| FsTreeError::Task(e.to_string()))??;
    crate::logger::log("DEBUG", "fs_tree", &format!("get_project_file_tree: {} top-level nodes, truncated={}", tree.nodes.len(), tree.truncated));
    Ok(tree)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for d in ["src/deep/deeper", "node_modules/x", ".git", ".agents/personas"] {
            std::fs::create_dir_all(dir.path().join(d)).unwrap();
        }
        for f in ["README.md", "src/main.rs", "src/deep/deeper/leaf.rs", ".env"] {
            std::fs::write(dir.path().join(f), "").unwrap();
        }
        dir
    }

    #[test]
    fn lists_directories_first_and_skips_excluded_and_hidden_entries() {
        let dir = fixture();
        let tree = build_tree(dir.path(), 12, 100).unwrap();
        let names: Vec<_> = tree.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, vec![".agents", "src", "README.md"]);
        assert!(!tree.truncated);
    }

    #[test]
    fn depth_and_node_limits_mark_the_tree_truncated() {
        let dir = fixture();
        let by_depth = build_tree(dir.path(), 1, 100).unwrap();
        assert!(by_depth.truncated);
        let src = by_depth.nodes.iter().find(|n| n.name == "src").unwrap();
        let deep = src.children.as_ref().unwrap().iter().find(|n| n.name == "deep").unwrap();
        assert!(deep.truncated && deep.children.as_ref().unwrap().is_empty());

        let by_nodes = build_tree(dir.path(), 12, 2).unwrap();
        assert!(by_nodes.truncated);
    }
}
