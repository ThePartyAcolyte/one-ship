//! Provider-neutral Forge inbox read/unread tracking and triage flags (Spec 38 / Spec 39 §9).

use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::db::{
    db_list_forge_inbox_items, db_upsert_forge_inbox_flag, db_upsert_forge_inbox_read,
    ForgeInboxDbRow,
};
use crate::events::{DaemonEvent, ForgeInboxUpdatedPayload};
use crate::kopeng::git::ForgeProviderKind;

use super::{
    provider_for_project_blocking, ForgeError, ForgeIssueQuery, ForgeProjectRef,
    ForgePullRequestQuery,
};

/// Read/activity state of a Forge issue or pull/merge request (Spec 38 §2).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ForgeInboxStatus {
    New,
    Updated,
    Read,
}

/// Optional user triage flag on a Forge item.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ForgeTriageFlag {
    Flagged,
    Muted,
}

impl ForgeTriageFlag {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::Flagged => "flagged",
            Self::Muted => "muted",
        }
    }

    pub fn from_db_str(raw: &str) -> Option<Self> {
        match raw {
            "flagged" => Some(Self::Flagged),
            "muted" => Some(Self::Muted),
            _ => None,
        }
    }
}

/// External Forge item category.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ForgeItemKind {
    Issue,
    Pr,
}

impl ForgeItemKind {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::Issue => "issue",
            Self::Pr => "pr",
        }
    }
}

fn provider_db_str(kind: ForgeProviderKind) -> &'static str {
    match kind {
        ForgeProviderKind::Github => "github",
        ForgeProviderKind::Gitlab => "gitlab",
        ForgeProviderKind::Unknown => "unknown",
    }
}

/// Combined Forge item with persisted inbox read/triage state.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ForgeInboxItem {
    pub provider: ForgeProviderKind,
    pub repo: String,
    pub item_kind: ForgeItemKind,
    #[ts(type = "number")]
    pub item_number: i64,
    pub title: String,
    pub state: String,
    pub author: String,
    pub url: String,
    pub updated_at: String,
    pub inbox_status: ForgeInboxStatus,
    pub is_read: bool,
    pub triage_flag: Option<ForgeTriageFlag>,
}

/// Unread and triage counts for a project's active Forge provider.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ForgeUnreadCounts {
    pub provider: ForgeProviderKind,
    pub issues_unread_count: usize,
    pub prs_unread_count: usize,
    pub total_unread_count: usize,
    pub updated_count: usize,
    pub flagged_count: usize,
}

/// Arguments for `list_forge_inbox`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ListForgeInboxArgs {
    #[ts(type = "number")]
    pub project_id: usize,
    pub provider: Option<ForgeProviderKind>,
    pub unread_only: Option<bool>,
}

/// Arguments for `mark_forge_item_read`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct MarkForgeItemReadArgs {
    #[ts(type = "number")]
    pub project_id: usize,
    pub provider: Option<ForgeProviderKind>,
    pub item_kind: ForgeItemKind,
    #[ts(type = "number")]
    pub number: i64,
    pub read: bool,
    pub updated_at: Option<String>,
}

/// Arguments for `set_forge_item_flag`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SetForgeItemFlagArgs {
    #[ts(type = "number")]
    pub project_id: usize,
    pub provider: Option<ForgeProviderKind>,
    pub item_kind: ForgeItemKind,
    #[ts(type = "number")]
    pub number: i64,
    pub flag: Option<ForgeTriageFlag>,
}

/// Arguments for `get_forge_unread_counts`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct GetForgeUnreadCountsArgs {
    #[ts(type = "number")]
    pub project_id: usize,
}

fn now_epoch_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Computes Spec 38 §2 inbox status from a persisted row and upstream `updated_at` timestamp.
pub fn compute_inbox_status(
    row: Option<&ForgeInboxDbRow>,
    upstream_updated_at: &str,
) -> ForgeInboxStatus {
    let Some(row) = row else {
        return ForgeInboxStatus::New;
    };
    if !row.is_read {
        return ForgeInboxStatus::New;
    }
    match row.last_seen_updated_at.as_deref() {
        Some(last_seen) if upstream_updated_at > last_seen => ForgeInboxStatus::Updated,
        Some(_) => ForgeInboxStatus::Read,
        None => ForgeInboxStatus::Updated,
    }
}

fn build_inbox_items_blocking(
    project_id: usize,
    provider_filter: Option<ForgeProviderKind>,
) -> Result<(ForgeProviderKind, Vec<ForgeInboxItem>), ForgeError> {
    let (path_str, provider) = provider_for_project_blocking(project_id)?;
    let active_kind = provider.kind();
    if let Some(requested) = provider_filter {
        if requested != active_kind {
            return Err(ForgeError::UnsupportedCapability {
                provider: requested,
                operation: "list_forge_inbox".to_string(),
            });
        }
    }

    let proj_ref = ForgeProjectRef {
        project_id,
        path: Path::new(&path_str),
    };
    let ctx = provider.collab_context(proj_ref, false)?;
    let repo = ctx.target_repo.ok_or(ForgeError::MissingTargetRepo)?;
    let issues = provider.list_issues(
        proj_ref,
        &ForgeIssueQuery {
            state: Some("open".to_string()),
            ..Default::default()
        },
    )?;
    let prs = provider.list_pull_requests(
        proj_ref,
        &ForgePullRequestQuery {
            state: Some("open".to_string()),
            ..Default::default()
        },
    )?;

    let db_rows = db_list_forge_inbox_items(project_id as i64, Some(provider_db_str(active_kind)))?;
    let row_map: HashMap<(&str, i64), &ForgeInboxDbRow> = db_rows
        .iter()
        .filter(|r| repo.is_empty() || r.repo == repo)
        .map(|r| ((r.item_kind.as_str(), r.item_number), r))
        .collect();

    let mut items = Vec::with_capacity(issues.len() + prs.len());
    for issue in issues {
        let row = row_map.get(&("issue", issue.number)).copied();
        let status = compute_inbox_status(row, &issue.updated_at);
        let flag = row
            .and_then(|r| r.triage_flag.as_deref())
            .and_then(ForgeTriageFlag::from_db_str);
        items.push(ForgeInboxItem {
            provider: active_kind,
            repo: repo.clone(),
            item_kind: ForgeItemKind::Issue,
            item_number: issue.number,
            title: issue.title,
            state: issue.state,
            author: issue.author,
            url: issue.url,
            updated_at: issue.updated_at,
            inbox_status: status,
            is_read: status == ForgeInboxStatus::Read,
            triage_flag: flag,
        });
    }

    for pr in prs {
        let row = row_map.get(&("pr", pr.number)).copied();
        let status = compute_inbox_status(row, &pr.updated_at);
        let flag = row
            .and_then(|r| r.triage_flag.as_deref())
            .and_then(ForgeTriageFlag::from_db_str);
        items.push(ForgeInboxItem {
            provider: active_kind,
            repo: repo.clone(),
            item_kind: ForgeItemKind::Pr,
            item_number: pr.number,
            title: pr.title,
            state: pr.state,
            author: pr.author,
            url: pr.url,
            updated_at: pr.updated_at,
            inbox_status: status,
            is_read: status == ForgeInboxStatus::Read,
            triage_flag: flag,
        });
    }

    items.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok((active_kind, items))
}

fn summarize_counts(provider: ForgeProviderKind, items: &[ForgeInboxItem]) -> ForgeUnreadCounts {
    let mut issues_unread_count = 0usize;
    let mut prs_unread_count = 0usize;
    let mut updated_count = 0usize;
    let mut flagged_count = 0usize;

    for item in items {
        if item.triage_flag == Some(ForgeTriageFlag::Flagged) {
            flagged_count += 1;
        }
        if item.triage_flag == Some(ForgeTriageFlag::Muted) {
            continue;
        }
        if item.inbox_status != ForgeInboxStatus::Read {
            match item.item_kind {
                ForgeItemKind::Issue => issues_unread_count += 1,
                ForgeItemKind::Pr => prs_unread_count += 1,
            }
            if item.inbox_status == ForgeInboxStatus::Updated {
                updated_count += 1;
            }
        }
    }

    ForgeUnreadCounts {
        provider,
        issues_unread_count,
        prs_unread_count,
        total_unread_count: issues_unread_count + prs_unread_count,
        updated_count,
        flagged_count,
    }
}

/// Lists Forge inbox items with read/updated/new and triage flag metadata.
pub async fn list_forge_inbox(args: ListForgeInboxArgs) -> Result<Vec<ForgeInboxItem>, ForgeError> {
    tokio::task::spawn_blocking(move || {
        let (_provider, mut items) = build_inbox_items_blocking(args.project_id, args.provider)?;
        if args.unread_only.unwrap_or(false) {
            items.retain(|item| item.inbox_status != ForgeInboxStatus::Read);
        }
        Ok(items)
    })
    .await
    .map_err(|e| ForgeError::Task(e.to_string()))?
}

/// Marks a Forge issue or PR as read or unread and emits `forge-inbox-updated`.
pub async fn mark_forge_item_read(
    args: MarkForgeItemReadArgs,
) -> Result<ForgeUnreadCounts, ForgeError> {
    tokio::task::spawn_blocking(move || {
        let (path_str, provider) = provider_for_project_blocking(args.project_id)?;
        let active_kind = provider.kind();
        if let Some(requested) = args.provider {
            if requested != active_kind {
                return Err(ForgeError::UnsupportedCapability {
                    provider: requested,
                    operation: "mark_forge_item_read".to_string(),
                });
            }
        }
        let proj_ref = ForgeProjectRef {
            project_id: args.project_id,
            path: Path::new(&path_str),
        };
        let ctx = provider.collab_context(proj_ref, false)?;
        let repo = ctx.target_repo.ok_or(ForgeError::MissingTargetRepo)?;

        db_upsert_forge_inbox_read(
            args.project_id as i64,
            provider_db_str(active_kind),
            &repo,
            args.item_kind.as_db_str(),
            args.number,
            args.read,
            args.updated_at.as_deref(),
            now_epoch_seconds(),
        )?;

        let (_, items) = build_inbox_items_blocking(args.project_id, None)?;
        let counts = summarize_counts(active_kind, &items);
        crate::supervisor::get_supervisor().emit_event(DaemonEvent::forge_inbox_updated(
            &ForgeInboxUpdatedPayload {
                project_id: args.project_id,
                provider: active_kind,
                unread_count: counts.total_unread_count,
                updated_count: counts.updated_count,
            },
        ));
        Ok(counts)
    })
    .await
    .map_err(|e| ForgeError::Task(e.to_string()))?
}

/// Sets or clears the triage flag (`flagged` / `muted` / `null`) on a Forge item and emits `forge-inbox-updated`.
pub async fn set_forge_item_flag(
    args: SetForgeItemFlagArgs,
) -> Result<ForgeUnreadCounts, ForgeError> {
    tokio::task::spawn_blocking(move || {
        let (path_str, provider) = provider_for_project_blocking(args.project_id)?;
        let active_kind = provider.kind();
        if let Some(requested) = args.provider {
            if requested != active_kind {
                return Err(ForgeError::UnsupportedCapability {
                    provider: requested,
                    operation: "set_forge_item_flag".to_string(),
                });
            }
        }
        let proj_ref = ForgeProjectRef {
            project_id: args.project_id,
            path: Path::new(&path_str),
        };
        let ctx = provider.collab_context(proj_ref, false)?;
        let repo = ctx.target_repo.ok_or(ForgeError::MissingTargetRepo)?;

        db_upsert_forge_inbox_flag(
            args.project_id as i64,
            provider_db_str(active_kind),
            &repo,
            args.item_kind.as_db_str(),
            args.number,
            args.flag.map(|f| f.as_db_str()),
            now_epoch_seconds(),
        )?;

        let (_, items) = build_inbox_items_blocking(args.project_id, None)?;
        let counts = summarize_counts(active_kind, &items);
        crate::supervisor::get_supervisor().emit_event(DaemonEvent::forge_inbox_updated(
            &ForgeInboxUpdatedPayload {
                project_id: args.project_id,
                provider: active_kind,
                unread_count: counts.total_unread_count,
                updated_count: counts.updated_count,
            },
        ));
        Ok(counts)
    })
    .await
    .map_err(|e| ForgeError::Task(e.to_string()))?
}

/// Returns unread and triage counts for `project_id`.
pub async fn get_forge_unread_counts(
    args: GetForgeUnreadCountsArgs,
) -> Result<ForgeUnreadCounts, ForgeError> {
    tokio::task::spawn_blocking(move || {
        let (provider, items) = build_inbox_items_blocking(args.project_id, None)?;
        Ok(summarize_counts(provider, &items))
    })
    .await
    .map_err(|e| ForgeError::Task(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbox_status_follows_spec_38_semantics() {
        assert_eq!(
            compute_inbox_status(None, "2026-11-17T10:00:00Z"),
            ForgeInboxStatus::New
        );

        let unread_row = ForgeInboxDbRow {
            project_id: 1,
            provider: "github".into(),
            repo: "a/b".into(),
            item_kind: "issue".into(),
            item_number: 1,
            provider_node_id: None,
            last_seen_updated_at: Some("2026-11-17T10:00:00Z".into()),
            is_read: false,
            triage_flag: None,
            updated_epoch_seconds: 100,
        };
        assert_eq!(
            compute_inbox_status(Some(&unread_row), "2026-11-17T10:00:00Z"),
            ForgeInboxStatus::New
        );

        let read_row = ForgeInboxDbRow {
            is_read: true,
            ..unread_row.clone()
        };
        assert_eq!(
            compute_inbox_status(Some(&read_row), "2026-11-17T10:00:00Z"),
            ForgeInboxStatus::Read
        );
        assert_eq!(
            compute_inbox_status(Some(&read_row), "2026-11-17T12:00:00Z"),
            ForgeInboxStatus::Updated
        );
    }
}
