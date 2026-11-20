//! Project Files & Bounded Reading Provider (`domain = "files"`, Spec 40).
//!
//! Exposes canonical project-scoped file search, directory listing, bounded line-ranged file
//! excerpt reading, and frontend explorer slot/open transitions without requiring tab navigation.
//! Hybrid semantic search, AST symbol lookup, and causal line insights live in `capabilities::knowledge`.

use std::borrow::Cow;
use std::path::Path;
use serde_json::{json, Value};

use crate::events::{DaemonEvent, UiSurfaceCommandPayload};
use crate::ui_state::{dispatch_ui_action, UiAction};

use super::{
    paginate_items, parse_pagination, require_non_empty_str, run_blocking, BoxFuture,
    CapabilityContext, CapabilityDef, CapabilityError, CapabilityProvider, CapabilityRisk,
    CapabilityTarget, CapabilityVisibility,
};

const MAX_FILE_WALK_VISITED_COUNT: usize = 5000;
const MAX_EXCERPT_FILE_BYTES: u64 = 2 * 1024 * 1024;
const DEFAULT_EXCERPT_MAX_LINES: usize = 120;
const MAX_EXCERPT_LINES_COUNT: usize = 250;

pub static PROVIDER: CapabilityProvider = CapabilityProvider {
    domain_id: "files",
    label: "Files & Code Search",
    required_capability: None,
    linked_surface_path: Some("files"),
    static_actions,
    dynamic_actions: |_| Vec::new(),
    execute,
};

fn cap(
    id: &'static str,
    mcp_name: Option<&'static str>,
    description: &'static str,
    parameters_schema: Value,
    risk: CapabilityRisk,
    target: CapabilityTarget,
    voice_max_items: Option<usize>,
) -> CapabilityDef {
    CapabilityDef {
        id: Cow::Borrowed(id),
        mcp_name: mcp_name.map(Cow::Borrowed),
        description: Cow::Borrowed(description),
        parameters_schema,
        visibility: CapabilityVisibility::Shared,
        risk,
        target,
        voice_max_items,
    }
}

fn static_actions() -> Vec<CapabilityDef> {
    vec![
        cap(
            "search_files",
            None,
            "Searches project files by filename or relative path substring/tokens within the project root (supports offset/limit pagination).",
            json!({
                "type": "OBJECT",
                "properties": {
                    "query": { "type": "STRING", "description": "Filename or partial path words (e.g. 'mcp router' or 'Cargo.toml')" },
                    "offset": { "type": "INTEGER", "description": "Pagination start index (default 0)" },
                    "limit": { "type": "INTEGER", "description": "Optional page size (omit for all)" }
                },
                "required": ["query"]
            }),
            CapabilityRisk::ReadOnly,
            CapabilityTarget::Backend,
            Some(25),
        ),
        cap(
            "list_directory",
            None,
            "Lists directory entries inside the project root (omit relative_path for project root, supports offset/limit pagination).",
            json!({
                "type": "OBJECT",
                "properties": {
                    "relative_path": { "type": "STRING", "description": "Folder relative to project root (omit for root)" },
                    "offset": { "type": "INTEGER", "description": "Pagination start index (default 0)" },
                    "limit": { "type": "INTEGER", "description": "Optional page size (omit for all)" }
                }
            }),
            CapabilityRisk::ReadOnly,
            CapabilityTarget::Backend,
            Some(40),
        ),
        cap(
            "read_file_excerpt",
            None,
            "Reads a bounded UTF-8 line range from a file inside the project root (defaults: start_line=1, max_lines=120, max 250 lines).",
            json!({
                "type": "OBJECT",
                "properties": {
                    "path": { "type": "STRING", "description": "Relative path from project root" },
                    "start_line": { "type": "INTEGER", "description": "1-based line number to start from (default: 1)" },
                    "max_lines": { "type": "INTEGER", "description": "Lines to return (1-250, default: 120)" }
                },
                "required": ["path"]
            }),
            CapabilityRisk::ReadOnly,
            CapabilityTarget::Backend,
            None,
        ),
        cap(
            "set_scope",
            None,
            "Updates the Files explorer scope slot ('project' | 'global' | 'specs').",
            json!({
                "type": "OBJECT",
                "properties": {
                    "scope": { "type": "STRING", "enum": ["project", "global", "specs"], "description": "Explorer tree view mode in Files tab" }
                },
                "required": ["scope"]
            }),
            CapabilityRisk::Mutating,
            CapabilityTarget::FrontendSlot {
                slot_key: "files.explorer_scope",
            },
            None,
        ),
        cap(
            "open_file",
            None,
            "Selects and opens a relative project file path in the Files viewer and persists files.selected_document_path.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "path": { "type": "STRING", "description": "Relative file path inside project root to display in UI" }
                },
                "required": ["path"]
            }),
            CapabilityRisk::Mutating,
            CapabilityTarget::FrontendCommand {
                node_path: "files",
            },
            None,
        ),
    ]
}

fn resolve_contained_path(project_path: &str, relative_path: &str) -> Result<std::path::PathBuf, CapabilityError> {
    let root = std::fs::canonicalize(project_path)?;
    let trimmed = relative_path.trim().trim_start_matches('/');
    let candidate = if trimmed.is_empty() {
        root.clone()
    } else {
        std::fs::canonicalize(root.join(trimmed))?
    };
    if !candidate.starts_with(&root) {
        return Err(CapabilityError::InvalidParams(
            "Path escapes project root".to_string(),
        ));
    }
    Ok(candidate)
}

fn search_project_files(project_path: &str, query_lower: &str) -> Result<Vec<String>, CapabilityError> {
    let root = std::fs::canonicalize(project_path)?;
    let tokens: Vec<String> = query_lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect();
    let mut stack = vec![root.clone()];
    let mut scored: Vec<(u8, String)> = Vec::with_capacity(64);
    let mut visited_count = 0usize;

    while let Some(dir) = stack.pop() {
        if visited_count >= MAX_FILE_WALK_VISITED_COUNT {
            break;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) => {
                crate::logger::log(
                    "DEBUG",
                    "capabilities::files",
                    &format!("Skipping unreadable directory {}: {}", dir.display(), e),
                );
                continue;
            }
        };
        for entry_res in entries {
            visited_count += 1;
            if visited_count >= MAX_FILE_WALK_VISITED_COUNT {
                break;
            }
            let entry = match entry_res {
                Ok(entry) => entry,
                Err(e) => {
                    crate::logger::log(
                        "DEBUG",
                        "capabilities::files",
                        &format!("Skipping unreadable directory entry in {}: {}", dir.display(), e),
                    );
                    continue;
                }
            };
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if matches!(name.as_str(), ".git" | "target" | "node_modules" | "dist") {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(rel) = path.strip_prefix(&root) {
                let rel_str = rel.to_string_lossy().to_string();
                let rel_lower = rel_str.to_lowercase();
                let name_lower = name.to_lowercase();
                if tokens.is_empty() {
                    scored.push((2, rel_str));
                } else if tokens.iter().all(|t| rel_lower.contains(t.as_str())) {
                    let rank = if name_lower == query_lower {
                        0
                    } else if tokens.iter().all(|t| name_lower.contains(t.as_str())) {
                        1
                    } else {
                        2
                    };
                    scored.push((rank, rel_str));
                }
            }
        }
    }

    scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.len().cmp(&b.1.len())));
    let matches: Vec<String> = scored.into_iter().map(|(_, p)| p).collect();
    Ok(matches)
}

fn list_project_directory(project_path: &str, relative_path: &str) -> Result<Vec<Value>, CapabilityError> {
    let target = resolve_contained_path(project_path, relative_path)?;
    let entries = std::fs::read_dir(&target)?;
    let mut items = Vec::with_capacity(32);
    for entry_res in entries {
        let entry = match entry_res {
            Ok(entry) => entry,
            Err(e) => {
                crate::logger::log(
                    "DEBUG",
                    "capabilities::files",
                    &format!("Skipping unreadable entry in {}: {}", target.display(), e),
                );
                continue;
            }
        };
        let name = entry.file_name().to_string_lossy().to_string();
        let is_dir = entry.path().is_dir();
        items.push(json!({ "name": name, "is_dir": is_dir }));
    }
    items.sort_by(|a, b| {
        let a_dir = a.get("is_dir").and_then(|v| v.as_bool()) == Some(true);
        let b_dir = b.get("is_dir").and_then(|v| v.as_bool()) == Some(true);
        b_dir.cmp(&a_dir).then_with(|| {
            let an = a.get("name").and_then(|v| v.as_str()).map(str::to_string);
            let bn = b.get("name").and_then(|v| v.as_str()).map(str::to_string);
            an.cmp(&bn)
        })
    });
    Ok(items)
}

fn read_project_file_excerpt(
    project_path: &str,
    relative_path: &str,
    start_line: usize,
    max_lines: usize,
) -> Result<Value, CapabilityError> {
    let target = resolve_contained_path(project_path, relative_path)?;
    let meta = std::fs::metadata(&target)?;
    if meta.is_dir() {
        return Err(CapabilityError::InvalidParams(format!(
            "'{}' is a directory, not a file",
            relative_path
        )));
    }
    if meta.len() > MAX_EXCERPT_FILE_BYTES {
        return Err(CapabilityError::InvalidParams(format!(
            "File '{}' exceeds the {} byte read limit",
            relative_path, MAX_EXCERPT_FILE_BYTES
        )));
    }

    let content = std::fs::read_to_string(&target)?;
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();
    if total_lines == 0 {
        return Ok(json!({
            "path": relative_path,
            "start_line": 1,
            "end_line": 0,
            "total_lines": 0,
            "truncated": false,
            "excerpt": ""
        }));
    }

    let clamped_start = start_line.max(1).min(total_lines);
    let clamped_max = max_lines.clamp(1, MAX_EXCERPT_LINES_COUNT);
    let start_idx = clamped_start - 1;
    let end_idx = (start_idx + clamped_max).min(total_lines);
    let excerpt = lines[start_idx..end_idx].join("\n");

    Ok(json!({
        "path": relative_path,
        "start_line": clamped_start,
        "end_line": end_idx,
        "total_lines": total_lines,
        "truncated": end_idx < total_lines,
        "excerpt": excerpt
    }))
}

fn parse_optional_positive_usize(
    params: &Value,
    key: &str,
    default: usize,
    max: usize,
) -> Result<usize, CapabilityError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => match v.as_u64() {
            Some(n) if n > 0 => Ok((n as usize).clamp(1, max)),
            _ => Err(CapabilityError::InvalidParams(format!(
                "'{key}' must be a positive integer"
            ))),
        },
    }
}

fn execute<'a>(
    ctx: CapabilityContext,
    action_id: &'a str,
    params: Value,
) -> BoxFuture<'a, Result<Value, CapabilityError>> {
    Box::pin(async move {
        let project_id = ctx.project_id();
        let project_path = ctx.project.path.clone();

        match action_id {
            "search_files" => {
                let query = require_non_empty_str(&params, "query")?.to_lowercase();
                let _ = parse_pagination(&params)?;
                let matches = run_blocking(move || search_project_files(&project_path, &query)).await?;
                paginate_items(matches, "matches", &params)
            }
            "list_directory" => {
                let rel = match params.get("relative_path") {
                    None | Some(Value::Null) => String::new(),
                    Some(v) => v
                        .as_str()
                        .map(str::to_string)
                        .ok_or_else(|| CapabilityError::InvalidParams("'relative_path' must be a string".to_string()))?,
                };
                let _ = parse_pagination(&params)?;
                let entries = run_blocking(move || list_project_directory(&project_path, &rel)).await?;
                paginate_items(entries, "entries", &params)
            }
            "read_file_excerpt" => {
                let rel = require_non_empty_str(&params, "path")?;
                let start_line = parse_optional_positive_usize(&params, "start_line", 1, usize::MAX)?;
                let max_lines = parse_optional_positive_usize(
                    &params,
                    "max_lines",
                    DEFAULT_EXCERPT_MAX_LINES,
                    MAX_EXCERPT_LINES_COUNT,
                )?;
                run_blocking(move || {
                    read_project_file_excerpt(&project_path, &rel, start_line, max_lines)
                })
                .await
            }
            "set_scope" => {
                let scope = require_non_empty_str(&params, "scope")?;
                let snapshot = run_blocking(move || {
                    Ok(dispatch_ui_action(UiAction::SetSlotValue {
                        project_id,
                        key: "files.explorer_scope".to_string(),
                        value: Some(scope),
                    })?)
                })
                .await?;
                Ok(json!({ "ok": true, "revision": snapshot.revision }))
            }
            "open_file" => {
                let rel_path = require_non_empty_str(&params, "path")?;
                if Path::new(&rel_path).is_absolute()
                    || rel_path.split(['/', '\\']).any(|seg| seg == "..")
                {
                    return Err(CapabilityError::InvalidParams(
                        "'path' must be a relative path inside the project root".to_string(),
                    ));
                }
                let slot_value = rel_path.clone();
                let snapshot = run_blocking(move || {
                    Ok(dispatch_ui_action(UiAction::SetSlotValue {
                        project_id,
                        key: "files.selected_document_path".to_string(),
                        value: Some(slot_value),
                    })?)
                })
                .await?;
                crate::supervisor::get_supervisor().emit_event(DaemonEvent::ui_surface_command(
                    &UiSurfaceCommandPayload {
                        project_id,
                        node_path: "files".to_string(),
                        action_id: "open_file".to_string(),
                        params: json!({ "path": rel_path }),
                    },
                ));
                Ok(json!({
                    "ok": true,
                    "dispatched_to": "frontend",
                    "revision": snapshot.revision
                }))
            }
            other => Err(CapabilityError::UnknownAction {
                domain: "files".to_string(),
                action_id: other.to_string(),
            }),
        }
    })
}
