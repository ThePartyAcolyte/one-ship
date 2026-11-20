//! Git & Snapshots Capability Provider (`domain = "git"`, Spec 16 & Spec 40).
//!
//! Unifies repository status, commit history, working-tree diffs, branch listing, and
//! safety snapshot management for both MCP agents and the Global Voice Controller without
//! requiring tab navigation.

use std::borrow::Cow;
use std::path::Path;
use serde_json::{json, Value};

use crate::db::db_get_or_create_active_conversation;
use crate::kopeng::git::{
    get_git_status, get_project_git_log, get_working_tree_diff, get_working_tree_files,
    list_git_branches, MAX_GIT_LOG_ITEMS_COUNT,
};
use crate::kopeng::snapshots::{create_snapshot, list_snapshots, restore_snapshot};

use super::{
    paginate_items, parse_pagination, require_non_empty_str, run_blocking, BoxFuture,
    CapabilityCaller, CapabilityContext, CapabilityDef, CapabilityError, CapabilityProvider,
    CapabilityRisk, CapabilityTarget, CapabilityVisibility,
};

pub static PROVIDER: CapabilityProvider = CapabilityProvider {
    domain_id: "git",
    label: "Git & Snapshots",
    required_capability: Some("has_git"),
    linked_surface_path: Some("git"),
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
    voice_max_items: Option<usize>,
) -> CapabilityDef {
    CapabilityDef {
        id: Cow::Borrowed(id),
        mcp_name: mcp_name.map(Cow::Borrowed),
        description: Cow::Borrowed(description),
        parameters_schema,
        visibility: CapabilityVisibility::Shared,
        risk,
        target: CapabilityTarget::Backend,
        voice_max_items,
    }
}

fn static_actions() -> Vec<CapabilityDef> {
    vec![
        cap(
            "get_status",
            None,
            "Returns repository Git status: active branch, dirty state, ahead/behind tracking, and configured remotes.",
            json!({ "type": "OBJECT", "properties": {} }),
            CapabilityRisk::ReadOnly,
            None,
        ),
        cap(
            "list_commits",
            None,
            "Lists Git commits (hash, author, date, message) for the project with optional offset/limit pagination.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "offset": { "type": "INTEGER", "description": "Pagination start index (default 0)" },
                    "limit": { "type": "INTEGER", "description": "Optional page size (omit for full log up to repo cap)" }
                }
            }),
            CapabilityRisk::ReadOnly,
            Some(15),
        ),
        cap(
            "get_working_tree_diff",
            None,
            "Inspects unstaged/staged working-tree changed files and diff output (optionally filtered to one relative file_path).",
            json!({
                "type": "OBJECT",
                "properties": {
                    "file_path": { "type": "STRING", "description": "Optional relative path to narrow diff output" }
                }
            }),
            CapabilityRisk::ReadOnly,
            None,
        ),
        cap(
            "list_branches",
            None,
            "Lists local and remote Git branches configured in the repository.",
            json!({ "type": "OBJECT", "properties": {} }),
            CapabilityRisk::ReadOnly,
            Some(25),
        ),
        cap(
            "list_snapshots",
            Some("list_snapshots"),
            "Lists safety snapshots for the project, optionally filtered by conversation_id, agent_id, offset, and limit.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "conversation_id": { "type": "STRING", "description": "Optional conversation UUID filter" },
                    "agent_id": { "type": "STRING", "description": "Filter snapshots created by a specific agent" },
                    "offset": { "type": "INTEGER", "description": "Pagination start index (default 0)" },
                    "limit": { "type": "INTEGER", "description": "Optional page size (omit for all)" }
                }
            }),
            CapabilityRisk::ReadOnly,
            Some(15),
        ),
        cap(
            "create_snapshot",
            Some("create_snapshot"),
            "Creates an atomic safety snapshot on a hidden Git ref without altering HEAD, index, or hooks.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "label": { "type": "STRING", "description": "Human-readable checkpoint label (e.g. 'before refactor')" },
                    "conversation_id": { "type": "STRING", "description": "Optional conversation UUID to link" },
                    "explicit_files": { "type": "ARRAY", "items": { "type": "STRING" }, "description": "Optional relative paths to capture instead of full working tree" }
                },
                "required": ["label"]
            }),
            CapabilityRisk::Mutating,
            None,
        ),
        cap(
            "restore_snapshot",
            Some("restore_snapshot"),
            "Restores working-tree files (or full tree if files is omitted) from a snapshot id or ref_path.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "snapshot_id": { "type": "STRING", "description": "Snapshot numeric ID or full refs/one-ship/... path from list_snapshots" },
                    "files": { "type": "ARRAY", "items": { "type": "STRING" }, "description": "Optional relative paths to restore (omit to restore entire snapshot)" }
                },
                "required": ["snapshot_id"]
            }),
            CapabilityRisk::ConfirmRequired,
            None,
        ),
    ]
}

fn optional_str(params: &Value, key: &str) -> Result<Option<String>, CapabilityError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => {
            let Some(raw) = v.as_str() else {
                return Err(CapabilityError::InvalidParams(format!(
                    "'{key}' must be a string"
                )));
            };
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
    }
}

fn optional_string_array(
    params: &Value,
    key: &str,
) -> Result<Option<Vec<String>>, CapabilityError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => {
            let Some(arr) = v.as_array() else {
                return Err(CapabilityError::InvalidParams(format!(
                    "'{key}' must be an array of strings"
                )));
            };
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                let Some(s) = item.as_str() else {
                    return Err(CapabilityError::InvalidParams(format!(
                        "Every item in '{key}' must be a string"
                    )));
                };
                out.push(s.to_string());
            }
            Ok(Some(out))
        }
    }
}

fn execute<'a>(
    ctx: CapabilityContext,
    action_id: &'a str,
    params: Value,
) -> BoxFuture<'a, Result<Value, CapabilityError>> {
    Box::pin(async move {
        let project_id = ctx.project_id();
        let project_id_usize = ctx.project.id;
        let project_path = ctx.project.path.clone();

        match action_id {
            "get_status" => {
                let status = run_blocking(move || Ok(get_git_status(Path::new(&project_path))?))
                    .await?;
                Ok(json!({ "status": status }))
            }
            "list_commits" => {
                let _ = parse_pagination(&params)?;
                let commits = get_project_git_log(project_id_usize, MAX_GIT_LOG_ITEMS_COUNT).await?;
                paginate_items(commits, "commits", &params)
            }
            "get_working_tree_diff" => {
                let file_path = optional_str(&params, "file_path")?;
                let files = get_working_tree_files(project_id_usize).await?;
                let diff = get_working_tree_diff(project_id_usize, file_path).await?;
                Ok(json!({ "files": files, "diff": diff }))
            }
            "list_branches" => {
                let branches = list_git_branches(project_id_usize).await?;
                Ok(json!({ "branches": branches }))
            }
            "list_snapshots" => {
                let conv = optional_str(&params, "conversation_id")?;
                let agent = optional_str(&params, "agent_id")?;
                let _ = parse_pagination(&params)?;
                let snapshots = run_blocking(move || {
                    Ok(list_snapshots(
                        project_id,
                        conv.as_deref(),
                        agent.as_deref(),
                        i64::MAX as usize,
                    )?)
                })
                .await?;
                paginate_items(snapshots, "snapshots", &params)
            }
            "create_snapshot" => {
                let label = require_non_empty_str(&params, "label")?;
                let explicit_conv = optional_str(&params, "conversation_id")?;
                let explicit_files = optional_string_array(&params, "explicit_files")?;
                let actor = ctx.caller.actor_label().to_string();
                let is_mcp = matches!(ctx.caller, CapabilityCaller::Mcp { .. });

                let snap = run_blocking(move || {
                    let conv_id = match explicit_conv {
                        Some(c) => Some(c),
                        None if is_mcp => Some(
                            db_get_or_create_active_conversation(
                                project_id, &actor, None, None, None, None, None,
                            )?
                            .id,
                        ),
                        None => None,
                    };
                    Ok(create_snapshot(
                        Path::new(&project_path),
                        project_id,
                        conv_id.as_deref(),
                        &actor,
                        &label,
                        explicit_files,
                    )?)
                })
                .await?;
                Ok(serde_json::to_value(snap).map_err(|e| CapabilityError::Task(e.to_string()))?)
            }
            "restore_snapshot" => {
                let snapshot_id = require_non_empty_str(&params, "snapshot_id")?;
                let files = optional_string_array(&params, "files")?;
                let restored = run_blocking(move || {
                    Ok(restore_snapshot(
                        Path::new(&project_path),
                        project_id,
                        &snapshot_id,
                        files,
                    )?)
                })
                .await?;
                Ok(json!({ "status": "ok", "restored_files": restored }))
            }
            other => Err(CapabilityError::UnknownAction {
                domain: "git".to_string(),
                action_id: other.to_string(),
            }),
        }
    })
}
