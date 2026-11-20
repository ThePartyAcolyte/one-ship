//! Voice Orchestrator Harness (`Spec 40`).
//!
//! Owns the persistent working session, operator scratchpad, deterministic Context Brief
//! hydration, turn ledger folding, and 65K-TPM response compaction independent of volatile
//! `BidiGenerateContent` WebSocket connections.

use serde_json::{json, Value};

use crate::config::VoiceControllerConfig;
use crate::db::{
    db_append_voice_turn_and_fold, db_archive_and_create_voice_session,
    db_clear_voice_scratchpad, db_delete_voice_scratchpad_item,
    db_get_or_create_active_voice_session, db_insert_voice_scratchpad_item,
    db_list_voice_scratchpad, db_list_voice_turn_ledger, db_update_voice_scratchpad_item,
    db_update_voice_session_focus, db_update_voice_session_resumption, DbVoiceScratchpadItem,
    DbVoiceSession, DbVoiceTurnLedgerEntry,
};
use crate::registry::get_projects;

const MAX_ROLLING_SUMMARY_CHARS: usize = 1600;
const MAX_BRIEF_CHARS: usize = 2400;
const RESUMPTION_HANDLE_TTL_SECONDS: i64 = 600;

/// Typed errors for the Voice Orchestrator Harness.
#[derive(Debug, thiserror::Error)]
pub enum VoiceHarnessError {
    #[error("Database error in voice harness: {0}")]
    Database(#[from] crate::db::DatabaseError),

    #[error("Invalid harness tool arguments: {0}")]
    InvalidArgs(String),

    #[error("Internal harness task error: {0}")]
    Task(String),
}

fn now_epoch_seconds() -> i64 {
    chrono::Utc::now().timestamp()
}

fn clip_chars(s: &str, max_chars: usize) -> String {
    let trimmed = s.trim();
    let count = trimmed.chars().count();
    if count <= max_chars {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Ensures an active working session exists in SQLite and returns it.
pub async fn load_or_init_session() -> Result<DbVoiceSession, VoiceHarnessError> {
    let now = now_epoch_seconds();
    tokio::task::spawn_blocking(move || Ok(db_get_or_create_active_voice_session(now)?))
        .await
        .map_err(|e| VoiceHarnessError::Task(e.to_string()))?
}

/// Returns a usable resumption handle only if it has not passed its TTL.
pub fn active_resumption_handle(session: &DbVoiceSession) -> Option<&str> {
    let handle = session.resumption_handle.as_deref()?.trim();
    if handle.is_empty() {
        return None;
    }
    if let Some(valid_until) = session.resumption_valid_until_epoch_seconds {
        if now_epoch_seconds() >= valid_until {
            return None;
        }
    }
    Some(handle)
}

/// Persists a `sessionResumptionUpdate` handle received from Gemini Live.
pub async fn save_resumption_handle(
    session_id: String,
    handle: Option<String>,
) -> Result<(), VoiceHarnessError> {
    let now = now_epoch_seconds();
    let valid_until = handle
        .as_ref()
        .filter(|h| !h.trim().is_empty())
        .map(|_| now + RESUMPTION_HANDLE_TTL_SECONDS);
    tokio::task::spawn_blocking(move || {
        db_update_voice_session_resumption(
            &session_id,
            handle.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            valid_until,
            now,
        )?;
        Ok(())
    })
    .await
    .map_err(|e| VoiceHarnessError::Task(e.to_string()))?
}

/// Records the active project focus in the working session.
pub async fn record_active_project(session_id: String, project_id: i64) {
    let now = now_epoch_seconds();
    if let Err(e) = tokio::task::spawn_blocking(move || {
        db_update_voice_session_focus(&session_id, Some(project_id), None, None, now)
    })
    .await
    {
        crate::logger::log(
            "WARN",
            "voice_harness",
            &format!("Failed to join active project update: {e}"),
        );
    }
}

/// Records the latest delegated `(project_id, agent_id)` target in the working session.
pub async fn record_delegation(session_id: String, project_id: i64, agent_id: String) {
    let now = now_epoch_seconds();
    match tokio::task::spawn_blocking(move || {
        db_update_voice_session_focus(&session_id, Some(project_id), None, Some((project_id, &agent_id)), now)
    })
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => crate::logger::log(
            "WARN",
            "voice_harness",
            &format!("Failed to persist voice delegation anchor: {e}"),
        ),
        Err(e) => crate::logger::log(
            "WARN",
            "voice_harness",
            &format!("Voice delegation persistence task panicked: {e}"),
        ),
    }
}

/// Appends one completed voice turn to the SQLite ledger and folds overflow into `rolling_summary`.
pub async fn record_turn(
    session_id: String,
    project_id: Option<i64>,
    user_transcript: Option<String>,
    actions_summary: Option<String>,
    assistant_transcript: Option<String>,
    cfg: &VoiceControllerConfig,
) {
    let has_content = user_transcript.as_deref().is_some_and(|s| !s.trim().is_empty())
        || actions_summary.as_deref().is_some_and(|s| !s.trim().is_empty())
        || assistant_transcript
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty());
    if !has_content {
        return;
    }
    let max_entries = cfg.turn_ledger_max_entries_count;
    let now = now_epoch_seconds();
    match tokio::task::spawn_blocking(move || {
        db_append_voice_turn_and_fold(
            &session_id,
            project_id,
            user_transcript.as_deref(),
            actions_summary.as_deref(),
            assistant_transcript.as_deref(),
            max_entries,
            MAX_ROLLING_SUMMARY_CHARS,
            now,
        )
    })
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => crate::logger::log(
            "WARN",
            "voice_harness",
            &format!("Failed to append voice turn ledger: {e}"),
        ),
        Err(e) => crate::logger::log(
            "WARN",
            "voice_harness",
            &format!("Voice turn ledger task panicked: {e}"),
        ),
    }
}

/// Loads working session state from SQLite and compiles a bounded Context Brief for `setup`.
pub async fn compile_session_brief(
    session_id: String,
    cfg: &VoiceControllerConfig,
) -> Result<String, VoiceHarnessError> {
    let max_turns = cfg.turn_ledger_max_entries_count.min(8);
    tokio::task::spawn_blocking(move || {
        let session = db_get_or_create_active_voice_session(now_epoch_seconds())?;
        if session.id != session_id {
            crate::logger::log(
                "DEBUG",
                "voice_harness",
                &format!("Active voice session switched from '{session_id}' to '{}'", session.id),
            );
        }
        let scratchpad = db_list_voice_scratchpad(&session.id)?;
        let turns = db_list_voice_turn_ledger(&session.id, max_turns)?;
        let projects = match get_projects() {
            Ok(p) => p,
            Err(e) => {
                crate::logger::log(
                    "WARN",
                    "voice_harness",
                    &format!("Could not hydrate project names for context brief: {e}"),
                );
                Vec::new()
            }
        };
        let active_project_label = session.active_project_id.and_then(|pid| {
            projects
                .iter()
                .find(|p| p.id as i64 == pid)
                .map(|p| format!("{} (id={})", p.name, p.id))
        });
        Ok(build_context_brief_from_parts(
            &session,
            active_project_label.as_deref(),
            &scratchpad,
            &turns,
        ))
    })
    .await
    .map_err(|e| VoiceHarnessError::Task(e.to_string()))?
}

/// Deterministic formatter producing a compact Context Brief capped at `MAX_BRIEF_CHARS`.
pub(crate) fn build_context_brief_from_parts(
    session: &DbVoiceSession,
    active_project_label: Option<&str>,
    scratchpad: &[DbVoiceScratchpadItem],
    recent_turns: &[DbVoiceTurnLedgerEntry],
) -> String {
    let mut lines: Vec<String> = Vec::with_capacity(24);
    lines.push(format!("[SESSION BRIEF id={}]", session.id));
    if let Some(proj) = active_project_label {
        lines.push(format!("Active Project: {proj}"));
    } else if let Some(pid) = session.active_project_id {
        lines.push(format!("Active Project ID: {pid}"));
    }
    if let Some(ref obj) = session.current_objective {
        let trimmed = obj.trim();
        if !trimmed.is_empty() {
            lines.push(format!("Current Objective: {}", clip_chars(trimmed, 180)));
        }
    }
    if let (Some(dpid), Some(ref dag)) = (
        session.last_delegated_project_id,
        &session.last_delegated_agent_id,
    ) {
        lines.push(format!("Last Delegated Agent: {dag} (project_id={dpid})"));
    }

    let open_items: Vec<&DbVoiceScratchpadItem> = scratchpad
        .iter()
        .filter(|i| i.status != "done")
        .take(8)
        .collect();
    if !open_items.is_empty() {
        lines.push("Scratchpad:".to_string());
        for item in open_items {
            let scope = item
                .project_id
                .map(|p| format!("p{p}:"))
                .unwrap_or_default();
            lines.push(format!(
                "- #{} [{}] {}{}",
                item.id,
                item.status,
                scope,
                clip_chars(&item.title, 90)
            ));
        }
    }

    if !session.rolling_summary.trim().is_empty() {
        lines.push("Prior Summary:".to_string());
        lines.push(clip_chars(&session.rolling_summary, 700));
    }

    if !recent_turns.is_empty() {
        lines.push("Recent Turns:".to_string());
        for t in recent_turns.iter().rev().take(5).rev() {
            let mut segs = Vec::with_capacity(3);
            if let Some(ref u) = t.user_transcript {
                if !u.trim().is_empty() {
                    segs.push(format!("U:{}", clip_chars(u, 85)));
                }
            }
            if let Some(ref act) = t.actions_summary {
                if !act.trim().is_empty() {
                    segs.push(format!("Act:{}", clip_chars(act, 85)));
                }
            }
            if let Some(ref a) = t.assistant_transcript {
                if !a.trim().is_empty() {
                    segs.push(format!("A:{}", clip_chars(a, 75)));
                }
            }
            if !segs.is_empty() {
                lines.push(format!("- {}", segs.join(" | ")));
            }
        }
    }

    clip_chars(&lines.join("\n"), MAX_BRIEF_CHARS)
}

/// Executes Tier-1 `harness_scratchpad` and `harness_session_reset` tools.
/// Returns `(tool_result_json, optional_new_session_id)`.
pub async fn execute_harness_tool(
    session_id: String,
    name: &str,
    args: Value,
    cfg: &VoiceControllerConfig,
) -> (Value, Option<String>) {
    match name {
        "harness_scratchpad" => {
            let Some(action) = args.get("action").and_then(|v| v.as_str()) else {
                return (json!({ "error": "Missing string action" }), None);
            };
            let max_items = cfg.scratchpad_max_items_count;
            let now = now_epoch_seconds();
            let action_owned = action.to_string();
            let res = tokio::task::spawn_blocking(move || -> Result<Value, VoiceHarnessError> {
                match action_owned.as_str() {
                    "list" => {
                        let items = db_list_voice_scratchpad(&session_id)?;
                        Ok(json!({ "session_id": session_id, "items": items }))
                    }
                    "set_objective" => {
                        let objective = args
                            .get("objective")
                            .and_then(|v| v.as_str())
                            .map(str::trim)
                            .filter(|s| !s.is_empty());
                        db_update_voice_session_focus(
                            &session_id,
                            None,
                            Some(objective),
                            None,
                            now,
                        )?;
                        Ok(json!({ "ok": true, "current_objective": objective }))
                    }
                    "add" => {
                        let Some(title) = args.get("title").and_then(|v| v.as_str()) else {
                            return Err(VoiceHarnessError::InvalidArgs(
                                "Missing string title for action 'add'".to_string(),
                            ));
                        };
                        let project_id = args.get("project_id").and_then(|v| v.as_i64()).filter(|p| *p > 0);
                        let note = args.get("note").and_then(|v| v.as_str());
                        let item = db_insert_voice_scratchpad_item(
                            &session_id,
                            project_id,
                            title,
                            note,
                            max_items,
                            now,
                        )?;
                        Ok(json!({ "ok": true, "item": item }))
                    }
                    "update" => {
                        let Some(item_id) = args.get("item_id").and_then(|v| v.as_i64()).filter(|i| *i > 0) else {
                            return Err(VoiceHarnessError::InvalidArgs(
                                "Missing positive integer item_id for action 'update'".to_string(),
                            ));
                        };
                        let status = args.get("status").and_then(|v| v.as_str());
                        let title = args.get("title").and_then(|v| v.as_str());
                        let note = match args.get("note") {
                            None => None,
                            Some(Value::Null) => Some(None),
                            Some(v) => Some(v.as_str()),
                        };
                        let item = db_update_voice_scratchpad_item(
                            &session_id,
                            item_id,
                            status,
                            title,
                            note,
                            now,
                        )?;
                        Ok(json!({ "ok": true, "item": item }))
                    }
                    "remove" => {
                        let Some(item_id) = args.get("item_id").and_then(|v| v.as_i64()).filter(|i| *i > 0) else {
                            return Err(VoiceHarnessError::InvalidArgs(
                                "Missing positive integer item_id for action 'remove'".to_string(),
                            ));
                        };
                        db_delete_voice_scratchpad_item(&session_id, item_id)?;
                        Ok(json!({ "ok": true, "removed_item_id": item_id }))
                    }
                    "clear" => {
                        let done_only = args
                            .get("done_only")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(true);
                        let cleared_count = db_clear_voice_scratchpad(&session_id, done_only)?;
                        Ok(json!({ "ok": true, "cleared_count": cleared_count, "done_only": done_only }))
                    }
                    other => Err(VoiceHarnessError::InvalidArgs(format!(
                        "Unknown scratchpad action '{other}'"
                    ))),
                }
            })
            .await;
            match res {
                Ok(Ok(v)) => (v, None),
                Ok(Err(e)) => (json!({ "error": e.to_string() }), None),
                Err(e) => (json!({ "error": e.to_string() }), None),
            }
        }
        "harness_session_reset" => {
            let now = now_epoch_seconds();
            let res = tokio::task::spawn_blocking(move || {
                db_archive_and_create_voice_session(now).map_err(VoiceHarnessError::Database)
            })
            .await;
            match res {
                Ok(Ok(new_sess)) => {
                    let new_id = new_sess.id.clone();
                    (
                        json!({ "ok": true, "new_session_id": new_sess.id }),
                        Some(new_id),
                    )
                }
                Ok(Err(e)) => (json!({ "error": e.to_string() }), None),
                Err(e) => (json!({ "error": e.to_string() }), None),
            }
        }
        other => (json!({ "error": format!("Unknown harness tool '{other}'") }), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_brief_stays_bounded_within_budget() {
        let session = DbVoiceSession {
            id: "sess-123".to_string(),
            status: "active".to_string(),
            active_project_id: Some(3),
            current_objective: Some("Audit project tasks against journal minutes".to_string()),
            last_delegated_project_id: Some(3),
            last_delegated_agent_id: Some("backend".to_string()),
            resumption_handle: None,
            resumption_valid_until_epoch_seconds: None,
            rolling_summary: "summary ".repeat(200),
            created_epoch_seconds: 1000,
            updated_epoch_seconds: 1010,
        };
        let scratchpad: Vec<DbVoiceScratchpadItem> = (1..=12)
            .map(|i| DbVoiceScratchpadItem {
                id: i,
                session_id: "sess-123".to_string(),
                project_id: Some(3),
                title: format!("Check pending item #{i}"),
                status: if i == 1 { "in_progress" } else { "pending" }.to_string(),
                note: None,
                created_epoch_seconds: 1000,
                updated_epoch_seconds: 1000,
            })
            .collect();
        let turns = vec![DbVoiceTurnLedgerEntry {
            id: 1,
            session_id: "sess-123".to_string(),
            project_id: Some(3),
            user_transcript: Some("Check completed journal tasks".to_string()),
            actions_summary: Some("journal.query_session_journal -> ok".to_string()),
            assistant_transcript: Some("Found 3 completed milestones.".to_string()),
            created_epoch_seconds: 1005,
        }];
        let brief = build_context_brief_from_parts(&session, Some("one-ship (id=3)"), &scratchpad, &turns);
        assert!(brief.chars().count() <= MAX_BRIEF_CHARS);
        assert!(brief.contains("one-ship (id=3)"));
        assert!(brief.contains("Check pending item #1"));
        assert!(brief.contains("Last Delegated Agent: backend (project_id=3)"));
    }
}
