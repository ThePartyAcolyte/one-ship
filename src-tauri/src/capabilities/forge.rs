//! Forge Collaboration & Inbox Capability Provider (`domain = "forge"`, Spec 39 & Spec 40).
//!
//! Unifies provider-neutral GitHub (`gh`) and GitLab (`glab`) collaboration reads and inbox
//! triage operations for both MCP agents and the Global Voice Controller without requiring
//! UI navigation to `project/forge`. Draft PR/MR proposals (`propose_forge_draft_pr`) remain in
//! `mcp.rs` due to the interactive `await_user_approval` modal flow.

use std::borrow::Cow;
use serde_json::{json, Value};

use crate::kopeng::forge::inbox::{
    get_forge_unread_counts, list_forge_inbox, mark_forge_item_read, set_forge_item_flag,
    ForgeItemKind, ForgeTriageFlag, GetForgeUnreadCountsArgs, ListForgeInboxArgs,
    MarkForgeItemReadArgs, SetForgeItemFlagArgs,
};
use crate::kopeng::forge::{
    get_forge_context, get_forge_issue, get_forge_pull_request, list_forge_issues,
    list_forge_pull_requests, list_forge_releases, GetForgeContextArgs, GetForgeIssueArgs,
    GetForgePullRequestArgs, ListForgeIssuesArgs, ListForgePullRequestsArgs,
    ListForgeReleasesArgs,
};

use super::{
    paginate_items, require_positive_i64, BoxFuture, CapabilityContext, CapabilityDef,
    CapabilityError, CapabilityProvider, CapabilityRisk, CapabilityTarget, CapabilityVisibility,
};

pub static PROVIDER: CapabilityProvider = CapabilityProvider {
    domain_id: "forge",
    label: "Forge Collaboration (GitHub & GitLab)",
    required_capability: Some("has_git"),
    linked_surface_path: Some("project/forge"),
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
            "get_context",
            Some("forge_get_context"),
            "Inspects local Git remotes (origin fork vs upstream parent), active Forge provider (GitHub/GitLab), ahead/behind counts, and target collaboration repository.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "force_refresh": { "type": "BOOLEAN", "description": "Bypass local cache and query GitHub/GitLab directly" }
                }
            }),
            CapabilityRisk::ReadOnly,
            None,
        ),
        cap(
            "list_issues",
            Some("forge_list_issues"),
            "Lists Forge issues from the target collaboration repository (state: 'open' | 'closed' | 'all', optional label/search/offset/limit).",
            json!({
                "type": "OBJECT",
                "properties": {
                    "state": { "type": "STRING", "enum": ["open", "closed", "all"], "description": "Issue state filter (default: open)" },
                    "label": { "type": "STRING", "description": "Filter by exact Forge label name" },
                    "search": { "type": "STRING", "description": "Free-text search filter on issue title/body" },
                    "offset": { "type": "INTEGER", "description": "Pagination start index (default 0)" },
                    "limit": { "type": "INTEGER", "description": "Optional page size (omit for all)" },
                    "force_refresh": { "type": "BOOLEAN", "description": "Bypass TTL cache" }
                }
            }),
            CapabilityRisk::ReadOnly,
            Some(15),
        ),
        cap(
            "get_issue",
            Some("forge_get_issue"),
            "Reads full details, body, labels, and comments of a specific Forge issue by number.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "number": { "type": "INTEGER", "description": "Issue number on GitHub or IID on GitLab" },
                    "force_refresh": { "type": "BOOLEAN", "description": "Bypass TTL cache" }
                },
                "required": ["number"]
            }),
            CapabilityRisk::ReadOnly,
            None,
        ),
        cap(
            "list_prs",
            Some("forge_list_prs"),
            "Lists Forge pull/merge requests on the target collaboration repository (state: 'open' | 'closed' | 'merged' | 'all', optional author_me/offset/limit).",
            json!({
                "type": "OBJECT",
                "properties": {
                    "state": { "type": "STRING", "enum": ["open", "closed", "merged", "all"], "description": "PR/MR state filter (default: open)" },
                    "author_me": { "type": "BOOLEAN", "description": "If true, list only PRs/MRs created by the authenticated user" },
                    "offset": { "type": "INTEGER", "description": "Pagination start index (default 0)" },
                    "limit": { "type": "INTEGER", "description": "Optional page size (omit for all)" },
                    "force_refresh": { "type": "BOOLEAN", "description": "Bypass TTL cache" }
                }
            }),
            CapabilityRisk::ReadOnly,
            Some(15),
        ),
        cap(
            "get_pr",
            Some("forge_get_pr"),
            "Reads full details of a Forge pull/merge request by number (body, diff stats, reviews, CI checks, comments).",
            json!({
                "type": "OBJECT",
                "properties": {
                    "number": { "type": "INTEGER", "description": "Pull/merge request number" },
                    "force_refresh": { "type": "BOOLEAN", "description": "Bypass TTL cache" }
                },
                "required": ["number"]
            }),
            CapabilityRisk::ReadOnly,
            None,
        ),
        cap(
            "list_releases",
            Some("forge_list_releases"),
            "Lists Forge releases/tags from the target collaboration repository with optional offset/limit pagination.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "offset": { "type": "INTEGER", "description": "Pagination start index (default 0)" },
                    "limit": { "type": "INTEGER", "description": "Optional page size (omit for all)" },
                    "force_refresh": { "type": "BOOLEAN", "description": "Bypass TTL cache" }
                }
            }),
            CapabilityRisk::ReadOnly,
            Some(10),
        ),
        cap(
            "list_inbox",
            Some("forge_list_inbox"),
            "Lists Forge inbox items with read/updated/new status, triage flags ('flagged' | 'muted'), and optional offset/limit pagination.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "unread_only": { "type": "BOOLEAN", "description": "True to show only new or updated unread items" },
                    "offset": { "type": "INTEGER", "description": "Pagination start index (default 0)" },
                    "limit": { "type": "INTEGER", "description": "Optional page size (omit for all)" }
                }
            }),
            CapabilityRisk::ReadOnly,
            Some(15),
        ),
        cap(
            "mark_item_read",
            Some("forge_mark_item_read"),
            "Marks a Forge issue or PR/MR as read or unread in the project inbox (item_kind: 'issue' | 'pr').",
            json!({
                "type": "OBJECT",
                "properties": {
                    "item_kind": { "type": "STRING", "enum": ["issue", "pr"], "description": "Whether the item is an issue or pull/merge request" },
                    "number": { "type": "INTEGER", "description": "Issue or PR/MR number" },
                    "read": { "type": "BOOLEAN", "description": "True to mark read, false to mark unread (default: true)" },
                    "updated_at": { "type": "STRING", "description": "Optional ISO timestamp snapshot to baseline future updates" }
                },
                "required": ["item_kind", "number"]
            }),
            CapabilityRisk::Mutating,
            None,
        ),
        cap(
            "set_item_flag",
            Some("forge_set_item_flag"),
            "Sets or clears the triage flag ('flagged' | 'muted' | null) on a Forge issue or PR/MR (item_kind: 'issue' | 'pr').",
            json!({
                "type": "OBJECT",
                "properties": {
                    "item_kind": { "type": "STRING", "enum": ["issue", "pr"], "description": "Whether the item is an issue or pull/merge request" },
                    "number": { "type": "INTEGER", "description": "Issue or PR/MR number" },
                    "flag": { "type": "STRING", "description": "'flagged' to bookmark, 'muted' to silence unread count, or '' to clear" }
                },
                "required": ["item_kind", "number"]
            }),
            CapabilityRisk::Mutating,
            None,
        ),
        cap(
            "get_unread_counts",
            Some("forge_get_unread_counts"),
            "Returns unread, updated, and flagged counts for the project's active Forge inbox.",
            json!({ "type": "OBJECT", "properties": {} }),
            CapabilityRisk::ReadOnly,
            None,
        ),
    ]
}

fn optional_bool(params: &Value, key: &str) -> Result<Option<bool>, CapabilityError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_bool()
            .map(Some)
            .ok_or_else(|| CapabilityError::InvalidParams(format!("'{key}' must be a boolean"))),
    }
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

fn parse_item_kind(params: &Value) -> Result<ForgeItemKind, CapabilityError> {
    match params.get("item_kind").and_then(|v| v.as_str()) {
        Some("issue") => Ok(ForgeItemKind::Issue),
        Some("pr") => Ok(ForgeItemKind::Pr),
        Some(other) => Err(CapabilityError::InvalidParams(format!(
            "Invalid 'item_kind' '{other}'; expected 'issue' or 'pr'"
        ))),
        None => Err(CapabilityError::InvalidParams(
            "Missing string parameter 'item_kind' ('issue' | 'pr')".to_string(),
        )),
    }
}

fn execute<'a>(
    ctx: CapabilityContext,
    action_id: &'a str,
    params: Value,
) -> BoxFuture<'a, Result<Value, CapabilityError>> {
    Box::pin(async move {
        let project_id = ctx.project.id;

        match action_id {
            "get_context" => {
                let force_refresh = optional_bool(&params, "force_refresh")?;
                let info = get_forge_context(GetForgeContextArgs {
                    project_id,
                    force_refresh,
                })
                .await?;
                Ok(json!({ "context": info }))
            }
            "list_issues" => {
                let state = optional_str(&params, "state")?;
                let label = optional_str(&params, "label")?;
                let search = optional_str(&params, "search")?;
                let force_refresh = optional_bool(&params, "force_refresh")?;
                let issues = list_forge_issues(ListForgeIssuesArgs {
                    project_id,
                    state,
                    label,
                    search,
                    force_refresh,
                })
                .await?;
                paginate_items(issues, "issues", &params)
            }
            "get_issue" => {
                let number = require_positive_i64(&params, "number")?;
                let force_refresh = optional_bool(&params, "force_refresh")?;
                let issue = get_forge_issue(GetForgeIssueArgs {
                    project_id,
                    issue_number: number,
                    force_refresh,
                })
                .await?;
                Ok(json!({ "issue": issue }))
            }
            "list_prs" => {
                let state = optional_str(&params, "state")?;
                let author_me = optional_bool(&params, "author_me")?;
                let force_refresh = optional_bool(&params, "force_refresh")?;
                let prs = list_forge_pull_requests(ListForgePullRequestsArgs {
                    project_id,
                    state,
                    author_me,
                    force_refresh,
                })
                .await?;
                paginate_items(prs, "prs", &params)
            }
            "get_pr" => {
                let number = require_positive_i64(&params, "number")?;
                let force_refresh = optional_bool(&params, "force_refresh")?;
                let pr = get_forge_pull_request(GetForgePullRequestArgs {
                    project_id,
                    pull_request_number: number,
                    force_refresh,
                })
                .await?;
                Ok(json!({ "pr": pr }))
            }
            "list_releases" => {
                let force_refresh = optional_bool(&params, "force_refresh")?;
                let releases = list_forge_releases(ListForgeReleasesArgs {
                    project_id,
                    force_refresh,
                })
                .await?;
                paginate_items(releases, "releases", &params)
            }
            "list_inbox" => {
                let unread_only = optional_bool(&params, "unread_only")?;
                let items = list_forge_inbox(ListForgeInboxArgs {
                    project_id,
                    provider: None,
                    unread_only,
                })
                .await?;
                paginate_items(items, "items", &params)
            }
            "mark_item_read" => {
                let item_kind = parse_item_kind(&params)?;
                let number = require_positive_i64(&params, "number")?;
                let read = match optional_bool(&params, "read")? {
                    Some(flag) => flag,
                    None => true,
                };
                let updated_at = optional_str(&params, "updated_at")?;
                let counts = mark_forge_item_read(MarkForgeItemReadArgs {
                    project_id,
                    provider: None,
                    item_kind,
                    number,
                    read,
                    updated_at,
                })
                .await?;
                Ok(json!({ "counts": counts }))
            }
            "set_item_flag" => {
                let item_kind = parse_item_kind(&params)?;
                let number = require_positive_i64(&params, "number")?;
                let flag = match params.get("flag") {
                    None | Some(Value::Null) => None,
                    Some(v) => match v.as_str() {
                        Some(raw) if raw.trim().is_empty() => None,
                        Some("flagged") => Some(ForgeTriageFlag::Flagged),
                        Some("muted") => Some(ForgeTriageFlag::Muted),
                        Some(other) => {
                            return Err(CapabilityError::InvalidParams(format!(
                                "Invalid 'flag' '{other}'; expected 'flagged', 'muted', or empty/null"
                            )));
                        }
                        None => {
                            return Err(CapabilityError::InvalidParams(
                                "'flag' must be a string or null".to_string(),
                            ));
                        }
                    },
                };
                let counts = set_forge_item_flag(SetForgeItemFlagArgs {
                    project_id,
                    provider: None,
                    item_kind,
                    number,
                    flag,
                })
                .await?;
                Ok(json!({ "counts": counts }))
            }
            "get_unread_counts" => {
                let counts = get_forge_unread_counts(GetForgeUnreadCountsArgs { project_id }).await?;
                Ok(json!({ "counts": counts }))
            }
            other => Err(CapabilityError::UnknownAction {
                domain: "forge".to_string(),
                action_id: other.to_string(),
            }),
        }
    })
}
