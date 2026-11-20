//! Brief builder (Spec 30 §5).
//!
//! A Brief is the plain-text context ONE-SHIP hands to a new Native Session when the runner cannot
//! resume the previous one (no native resume, invalidated session, Reopen, explicit transfer).
//! It is runner-neutral, bounded, and built without any model call.
//!
//! Priority, highest first: header, Conversation summary, recent turns, Minutes digest, state report,
//! open items. When the result exceeds `brief_max_bytes`, sections are dropped in reverse priority;
//! the summary and the most recent turns go last. The build is pure over its input ([`compose_brief`]);
//! [`build_brief`] only gathers that input from SQLite.

use thiserror::Error;

/// Reasons a Brief could not be built.
#[derive(Debug, Error)]
pub enum BriefError {
    /// The requested Conversation does not exist.
    #[error("Conversation '{conversation_id}' not found")]
    ConversationNotFound { conversation_id: String },

    /// A database read failed.
    #[error("Brief data could not be read: {0}")]
    Database(#[from] crate::db::DatabaseError),
}

/// One chat turn as it appears in a Brief.
#[derive(Debug, Clone, PartialEq)]
pub struct BriefTurn {
    /// `user` or the agent's display name.
    pub author: String,
    pub text: String,
    /// Reference to an artifact attached to the turn (label, type or path), never its body.
    pub artifact_ref: Option<String>,
}

/// One Minutes entry as it appears in the digest.
#[derive(Debug, Clone, PartialEq)]
pub struct BriefMinutes {
    pub title: String,
    pub summary: String,
    pub decisions: Option<String>,
    pub files_touched: Option<String>,
    pub technical_debt: Option<String>,
    pub tags: Option<String>,
}

/// One TODO item in the state report.
#[derive(Debug, Clone, PartialEq)]
pub struct BriefTodo {
    pub title: String,
    pub status: String,
    pub category: String,
}

/// One technical debt item in the state report.
#[derive(Debug, Clone, PartialEq)]
pub struct BriefTechDebt {
    pub title: String,
    pub severity: String,
    pub category: String,
}

/// State report of a conversation (Spec 30 §5): files touched, tasks and tech debt.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BriefStateReport {
    pub files_touched: Vec<String>,
    pub todos: Vec<BriefTodo>,
    pub technical_debt: Vec<BriefTechDebt>,
}

/// One delegation in the open items section.
#[derive(Debug, Clone, PartialEq)]
pub struct BriefDelegation {
    pub target_agent: String,
    pub instruction: String,
    pub status: String,
}

/// Open items of a conversation (Spec 30 §5): pending approvals and unresolved delegations.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BriefOpenItems {
    pub pending_approvals: Vec<String>,
    pub unresolved_delegations: Vec<BriefDelegation>,
}

/// Everything a Brief is composed from.
#[derive(Debug, Clone, Default)]
pub struct BriefInput {
    pub topic: String,
    pub summary: Option<String>,
    /// Chronological order, oldest first.
    pub turns: Vec<BriefTurn>,
    /// Newest first, as stored.
    pub minutes: Vec<BriefMinutes>,
    pub state_report: BriefStateReport,
    pub open_items: BriefOpenItems,
}

/// A built Brief and what had to be left out to respect the size cap.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct Brief {
    pub text: String,
    /// Sections dropped or shortened to fit `brief_max_bytes`, for example `open_items`.
    pub dropped_sections: Vec<String>,
    /// Number of turns that made it into the text.
    pub turns_included_count: usize,
}

const HEADER_TEMPLATE: &str = "# Context Brief\n\
This is a reconstruction of an earlier conversation, built by ONE-SHIP from its recorded history. \
It is context, not a replay: the tool state and file reads of that session are not restored, so \
re-read any file before relying on it.\n\nTopic: ";

/// Truncates `text` to at most `max_bytes` on a character boundary, marking the cut.
fn truncate_utf8(text: &str, max_bytes: usize) -> String {
    const MARK: &str = "…";
    if text.len() <= max_bytes {
        return text.to_string();
    }
    if max_bytes <= MARK.len() {
        return String::new();
    }
    let mut end = max_bytes - MARK.len();
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{}", &text[..end], MARK)
}

fn render_turn(turn: &BriefTurn, turn_max_bytes: usize) -> String {
    let mut line = format!("- {}: {}", turn.author, truncate_utf8(turn.text.trim(), turn_max_bytes).replace('\n', "\n  "));
    if let Some(reference) = &turn.artifact_ref {
        line.push_str(&format!(" [artifact: {}]", reference));
    }
    line.push('\n');
    line
}

fn render_minutes(minutes: &[BriefMinutes]) -> String {
    let mut out = String::new();
    for m in minutes {
        out.push_str(&format!("- {}: {}\n", m.title, m.summary.trim()));
        for (label, value) in [
            ("Decisions", &m.decisions),
            ("Files", &m.files_touched),
            ("Debt", &m.technical_debt),
            ("Tags", &m.tags),
        ] {
            if let Some(v) = value.as_deref().filter(|v| !v.trim().is_empty()) {
                out.push_str(&format!("  {}: {}\n", label, v.trim().replace('\n', " ")));
            }
        }
    }
    out
}

fn render_state_report(report: &BriefStateReport) -> String {
    let mut out = String::new();
    if !report.files_touched.is_empty() {
        out.push_str("- Files touched:\n");
        for f in &report.files_touched {
            out.push_str(&format!("  * {}\n", f));
        }
    }
    if !report.todos.is_empty() {
        out.push_str("- Tasks (TODOs):\n");
        for t in &report.todos {
            out.push_str(&format!("  * [{}] {} ({})\n", t.status, t.title, t.category));
        }
    }
    if !report.technical_debt.is_empty() {
        out.push_str("- Technical Debt:\n");
        for d in &report.technical_debt {
            out.push_str(&format!("  * [{}] {} ({})\n", d.severity, d.title, d.category));
        }
    }
    out
}

fn render_open_items(items: &BriefOpenItems) -> String {
    let mut out = String::new();
    if !items.pending_approvals.is_empty() {
        out.push_str("- Pending Approvals:\n");
        for a in &items.pending_approvals {
            out.push_str(&format!("  * {}\n", a.trim().replace('\n', " ")));
        }
    }
    if !items.unresolved_delegations.is_empty() {
        out.push_str("- Unresolved Delegations:\n");
        for d in &items.unresolved_delegations {
            out.push_str(&format!("  * To {}: {} [{}]\n", d.target_agent, truncate_utf8(d.instruction.trim(), 120).replace('\n', " "), d.status));
        }
    }
    out
}

fn assemble(
    topic: &str,
    summary: Option<&str>,
    turns: &[String],
    minutes: &str,
    state_report: &str,
    open_items: &str,
) -> String {
    let mut text = format!("{}{}\n", HEADER_TEMPLATE, topic);
    if let Some(s) = summary.filter(|s| !s.trim().is_empty()) {
        text.push_str(&format!("\n## Summary\n{}\n", s.trim()));
    }
    if !turns.is_empty() {
        text.push_str("\n## Recent turns\n");
        turns.iter().for_each(|t| text.push_str(t));
    }
    if !minutes.is_empty() {
        text.push_str(&format!("\n## Minutes\n{}", minutes));
    }
    if !state_report.is_empty() {
        text.push_str(&format!("\n## State report\n{}", state_report));
    }
    if !open_items.is_empty() {
        text.push_str(&format!("\n## Open items\n{}", open_items));
    }
    text
}

/// Composes a Brief from `input` within `max_bytes` (never exceeded), keeping at most
/// `recent_turns_count` of the latest turns.
///
/// Drop order when over budget: open items, then state report, then Minutes digest, then the oldest
/// turns one by one, then the summary is truncated. The header is always present unless the cap is
/// smaller than the header itself, in which case the text is cut to the cap.
pub fn compose_brief(input: &BriefInput, recent_turns_count: usize, max_bytes: usize) -> Brief {
    debug_assert!(max_bytes > 0, "brief_max_bytes must be positive");
    let skip = input.turns.len().saturating_sub(recent_turns_count);
    let kept_turns = &input.turns[skip..];
    // One turn may use at most an even share of the budget, so a single long message cannot starve the rest.
    let turn_max_bytes = (max_bytes / recent_turns_count.max(1)).max(256);
    let mut rendered_turns: Vec<String> = kept_turns.iter().map(|t| render_turn(t, turn_max_bytes)).collect();
    let mut open_items = render_open_items(&input.open_items);
    let mut state_report = render_state_report(&input.state_report);
    let mut minutes = render_minutes(&input.minutes);
    let mut summary = input.summary.clone();
    let mut dropped = Vec::new();

    let mut text = assemble(&input.topic, summary.as_deref(), &rendered_turns, &minutes, &state_report, &open_items);

    // Drop in reverse priority order (Spec 30 §5):
    // Priority (highest first): header, Summary, recent turns, Minutes digest, state report, open items.
    // 1. open_items
    if text.len() > max_bytes && !open_items.is_empty() {
        open_items.clear();
        dropped.push("open_items".to_string());
        text = assemble(&input.topic, summary.as_deref(), &rendered_turns, &minutes, &state_report, &open_items);
    }
    // 2. state_report
    if text.len() > max_bytes && !state_report.is_empty() {
        state_report.clear();
        dropped.push("state_report".to_string());
        text = assemble(&input.topic, summary.as_deref(), &rendered_turns, &minutes, &state_report, &open_items);
    }
    // 3. minutes_digest
    if text.len() > max_bytes && !minutes.is_empty() {
        minutes.clear();
        dropped.push("minutes_digest".to_string());
        text = assemble(&input.topic, summary.as_deref(), &rendered_turns, &minutes, &state_report, &open_items);
    }
    // 4. older_turns
    if text.len() > max_bytes && !rendered_turns.is_empty() {
        let before = rendered_turns.len();
        while text.len() > max_bytes && !rendered_turns.is_empty() {
            rendered_turns.remove(0);
            text = assemble(&input.topic, summary.as_deref(), &rendered_turns, &minutes, &state_report, &open_items);
        }
        dropped.push(format!("older_turns({})", before - rendered_turns.len()));
    }
    // 5. summary
    if text.len() > max_bytes {
        if let Some(s) = summary.take() {
            let overflow = text.len() - max_bytes;
            summary = Some(truncate_utf8(&s, s.len().saturating_sub(overflow)));
            dropped.push("summary_truncated".to_string());
            text = assemble(&input.topic, summary.as_deref(), &rendered_turns, &minutes, &state_report, &open_items);
        }
    }
    // 6. header
    if text.len() > max_bytes {
        text = truncate_utf8(&text, max_bytes);
        dropped.push("header_truncated".to_string());
    }
    debug_assert!(text.len() <= max_bytes, "a Brief must never exceed brief_max_bytes");
    Brief { text, dropped_sections: dropped, turns_included_count: rendered_turns.len() }
}

/// Builds the Brief of one Conversation from SQLite, using `supervisor.brief_recent_turns_count`
/// and `supervisor.brief_max_bytes`.
pub fn build_brief(conversation_id: &str) -> Result<Brief, BriefError> {
    let recent_turns_count = crate::config::resolve_effective_usize(None, None, "supervisor", "brief_recent_turns_count");
    let max_bytes = crate::config::resolve_effective_usize(None, None, "supervisor", "brief_max_bytes");
    crate::logger::log("DEBUG", "brief", &format!("build_brief: conversation_id={}, recent_turns_count={}, max_bytes={}", conversation_id, recent_turns_count, max_bytes));

    let conversation = crate::db::db_get_conversation_by_id(conversation_id)
        .map_err(BriefError::Database)?
        .ok_or_else(|| BriefError::ConversationNotFound { conversation_id: conversation_id.to_string() })?;

    // Newest first from the database; fetch extra because system turns are filtered out.
    let mut turns: Vec<BriefTurn> = crate::db::db_get_conversation_turns(conversation_id, recent_turns_count.saturating_mul(2).max(1), None)
        .map_err(BriefError::Database)?
        .into_iter()
        .filter(|t| t.role == "user" || t.role == "agent")
        .map(|t| BriefTurn {
            author: if t.role == "user" { "user".to_string() } else { t.agent_id.clone() },
            text: t.dialogue,
            artifact_ref: t.artifact_label.or(t.artifact_file_path).or(t.artifact_type),
        })
        .collect();
    turns.truncate(recent_turns_count);
    turns.reverse();

    let minutes = crate::db::db_get_conversation_session_journals(conversation_id, recent_turns_count)
        .map_err(BriefError::Database)?
        .into_iter()
        .map(|m| BriefMinutes {
            title: m.title,
            summary: m.summary,
            decisions: m.decisions,
            files_touched: m.files_touched,
            technical_debt: m.technical_debt,
            tags: m.tags,
        })
        .collect::<Vec<_>>();

    // State report (Spec 30 §5)
    let mut files_set = std::collections::BTreeSet::new();
    let snaps = crate::db::db_get_conversation_snapshots(conversation_id).map_err(BriefError::Database)?;
    for s in snaps {
        if let Some(json_str) = s.files_changed {
            match serde_json::from_str::<Vec<String>>(&json_str) {
                Ok(files) => {
                    for f in files {
                        files_set.insert(f);
                    }
                }
                Err(e) => {
                    crate::logger::log("WARN", "brief", &format!("build_brief: invalid files_changed JSON in snapshot '{}': {}", s.id, e));
                }
            }
        }
    }
    for m in &minutes {
        if let Some(ft) = &m.files_touched {
            for f in ft.split(&[',', '\n'][..]) {
                let trimmed = f.trim();
                if !trimmed.is_empty() {
                    files_set.insert(trimmed.to_string());
                }
            }
        }
    }

    let todos = crate::db::db_get_conversation_todos(conversation_id)
        .map_err(BriefError::Database)?
        .into_iter()
        .map(|t| BriefTodo {
            title: t.title,
            status: t.status,
            category: t.category,
        })
        .collect();

    let technical_debt = crate::db::db_get_conversation_technical_debt(conversation_id)
        .map_err(BriefError::Database)?
        .into_iter()
        .map(|d| BriefTechDebt {
            title: d.title,
            severity: d.severity,
            category: d.category,
        })
        .collect();

    let state_report = BriefStateReport {
        files_touched: files_set.into_iter().collect(),
        todos,
        technical_debt,
    };

    // Open items (Spec 30 §5)
    let raw_turns = crate::db::db_get_conversation_turns(conversation_id, 100, None).map_err(BriefError::Database)?;
    let mut pending_approvals = Vec::new();
    for t in raw_turns {
        if let Some(p) = t.approval_payload {
            match serde_json::from_str::<serde_json::Value>(&p) {
                Ok(v) => {
                    if v.get("status").and_then(|s| s.as_str()) == Some("pending") {
                        let desc = v.get("description").or_else(|| v.get("command")).or_else(|| v.get("title"))
                            .and_then(|d| d.as_str())
                            .unwrap_or("Pending approval");
                        pending_approvals.push(desc.to_string());
                    }
                }
                Err(e) => {
                    crate::logger::log("WARN", "brief", &format!("build_brief: invalid approval_payload JSON in turn {:?}: {}", t.id, e));
                }
            }
        }
    }

    let unresolved_delegations = crate::db::db_get_conversation_delegations(conversation_id)
        .map_err(BriefError::Database)?
        .into_iter()
        .filter(|d| d.status != "completed" && d.status != "resolved")
        .map(|d| BriefDelegation {
            target_agent: d.target_agent,
            instruction: d.instruction,
            status: d.status,
        })
        .collect();

    let open_items = BriefOpenItems {
        pending_approvals,
        unresolved_delegations,
    };

    let input = BriefInput {
        topic: conversation.topic,
        summary: conversation.summary,
        turns,
        minutes,
        state_report,
        open_items,
    };
    Ok(compose_brief(&input, recent_turns_count, max_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(author: &str, text: &str) -> BriefTurn {
        BriefTurn { author: author.to_string(), text: text.to_string(), artifact_ref: None }
    }

    fn minutes(title: &str) -> BriefMinutes {
        BriefMinutes {
            title: title.to_string(),
            summary: "did things".to_string(),
            decisions: Some("chose X".to_string()),
            files_touched: None,
            technical_debt: None,
            tags: Some("db".to_string()),
        }
    }

    #[test]
    fn full_brief_contains_every_section_in_priority_order() {
        let input = BriefInput {
            topic: "Database".to_string(),
            summary: Some("We moved to FTS5.".to_string()),
            turns: vec![turn("user", "hello"), turn("frontend", "hi")],
            minutes: vec![minutes("M1")],
            state_report: BriefStateReport {
                files_touched: vec!["src/db.rs".to_string()],
                todos: vec![BriefTodo { title: "Fix schema".to_string(), status: "pending".to_string(), category: "refactor".to_string() }],
                technical_debt: vec![BriefTechDebt { title: "Legacy shim".to_string(), severity: "medium".to_string(), category: "arch".to_string() }],
            },
            open_items: BriefOpenItems {
                pending_approvals: vec!["Run migration".to_string()],
                unresolved_delegations: vec![BriefDelegation { target_agent: "worker".to_string(), instruction: "sync db".to_string(), status: "pending".to_string() }],
            },
        };
        let brief = compose_brief(&input, 20, 32768);
        let (s, t, m, sr, oi) = (
            brief.text.find("## Summary").expect("summary"),
            brief.text.find("## Recent turns").expect("turns"),
            brief.text.find("## Minutes").expect("minutes"),
            brief.text.find("## State report").expect("state report"),
            brief.text.find("## Open items").expect("open items"),
        );
        assert!(s < t && t < m && m < sr && sr < oi);
        assert!(brief.dropped_sections.is_empty());
        assert_eq!(brief.turns_included_count, 2);
    }

    #[test]
    fn only_the_latest_turns_are_kept() {
        let input = BriefInput {
            topic: "T".to_string(),
            summary: None,
            turns: (0..10).map(|i| turn("user", &format!("message-{}", i))).collect(),
            minutes: vec![],
            state_report: BriefStateReport::default(),
            open_items: BriefOpenItems::default(),
        };
        let brief = compose_brief(&input, 3, 32768);
        assert_eq!(brief.turns_included_count, 3);
        assert!(brief.text.contains("message-9") && brief.text.contains("message-7"));
        assert!(!brief.text.contains("message-6"));
    }

    #[test]
    fn over_budget_drops_open_items_first_then_state_report_then_minutes() {
        let input = BriefInput {
            topic: "T".to_string(),
            summary: Some("keep me".to_string()),
            turns: (0..6).map(|i| turn("user", &format!("turn-{}-{}", i, "x".repeat(200)))).collect(),
            minutes: vec![minutes("M1")],
            state_report: BriefStateReport {
                files_touched: vec!["a.rs".to_string()],
                todos: vec![],
                technical_debt: vec![],
            },
            open_items: BriefOpenItems {
                pending_approvals: vec!["approval".to_string()],
                unresolved_delegations: vec![],
            },
        };
        let full = compose_brief(&input, 20, 32768);
        // Budget just below full: open_items dropped
        let tight1 = compose_brief(&input, 20, full.text.len() - 1);
        assert_eq!(tight1.dropped_sections, vec!["open_items".to_string()]);
        // Budget below state report: open_items and state_report dropped
        let tight2 = compose_brief(&input, 20, tight1.text.len() - 1);
        assert_eq!(tight2.dropped_sections, vec!["open_items".to_string(), "state_report".to_string()]);
        // Budget below minutes: open_items, state_report, minutes_digest dropped
        let tight3 = compose_brief(&input, 20, tight2.text.len() - 1);
        assert_eq!(tight3.dropped_sections, vec!["open_items".to_string(), "state_report".to_string(), "minutes_digest".to_string()]);
    }

    #[test]
    fn truncation_respects_utf8_boundaries() {
        let input = BriefInput {
            topic: "ñ".repeat(10),
            summary: Some("é".repeat(500)),
            turns: vec![],
            minutes: vec![],
            state_report: BriefStateReport::default(),
            open_items: BriefOpenItems::default(),
        };
        let brief = compose_brief(&input, 20, 300);
        assert!(brief.text.len() <= 300);
        assert!(brief.text.is_char_boundary(brief.text.len()));
    }
}
