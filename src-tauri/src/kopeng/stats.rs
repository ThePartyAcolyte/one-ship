use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::db::DatabaseError;
use crate::kopeng::accounts::AccountsError;

/// Failure of a quota or session metrics operation.
#[derive(Debug, thiserror::Error)]
pub enum StatsError {
    #[error("Home directory is unavailable")]
    HomeUnavailable,
    #[error("{context}: {source}")]
    Io { context: String, source: std::io::Error },
    #[error("Statusline observation {path} is not valid JSON: {source}")]
    StatuslineJson { path: String, source: serde_json::Error },
    #[error("Statusline observation carries no account_id or email; quota not attributed")]
    UnattributedStatusline,
    #[error("Statusline observation (account_id={file_account_id:?}, email={file_email:?}) does not belong to a registered account")]
    UnknownStatuslineAccount { file_account_id: Option<String>, file_email: Option<String> },
    #[error("Statusline observation (account_id={file_account_id:?}, email={file_email:?}) does not belong to account {target}")]
    AccountMismatch { target: String, file_account_id: Option<String>, file_email: Option<String> },
    #[error("Account '{0}' is not registered")]
    UnknownAccount(String),
    #[error("Configuration {plugin}.{key} must be {expected}")]
    InvalidConfig { plugin: &'static str, key: &'static str, expected: &'static str },
    #[error("Quota probe for {email} failed: {reason}")]
    ProbeFailed { email: String, reason: String },
    #[error(transparent)]
    RunnerManifest(#[from] crate::config::ConfigError),
    #[error("Background task failed: {0}")]
    Task(String),
    #[error(transparent)]
    Registry(#[from] crate::registry::RegistryError),
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    Accounts(#[from] AccountsError),
}

impl crate::api::ApiError for StatsError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::UnknownAccount(_) => StatusCode::NOT_FOUND,
            Self::InvalidConfig { .. } => StatusCode::BAD_REQUEST,
            Self::ProbeFailed { .. } => StatusCode::BAD_GATEWAY,
            Self::HomeUnavailable
            | Self::Io { .. }
            | Self::StatuslineJson { .. }
            | Self::UnattributedStatusline
            | Self::UnknownStatuslineAccount { .. }
            | Self::AccountMismatch { .. }
            | Self::Task(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Database(e) => e.status(),
            Self::RunnerManifest(e) => e.status(),
            Self::Registry(e) => e.status(),
            Self::Accounts(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::HomeUnavailable => "StatsError::HomeUnavailable",
            Self::Io { .. } => "StatsError::Io",
            Self::StatuslineJson { .. } => "StatsError::StatuslineJson",
            Self::UnattributedStatusline => "StatsError::UnattributedStatusline",
            Self::UnknownStatuslineAccount { .. } => "StatsError::UnknownStatuslineAccount",
            Self::AccountMismatch { .. } => "StatsError::AccountMismatch",
            Self::UnknownAccount(_) => "StatsError::UnknownAccount",
            Self::InvalidConfig { .. } => "StatsError::InvalidConfig",
            Self::ProbeFailed { .. } => "StatsError::ProbeFailed",
            Self::Task(_) => "StatsError::Task",
            Self::Database(e) => e.code(),
            Self::RunnerManifest(e) => e.code(),
            Self::Registry(e) => e.code(),
            Self::Accounts(e) => e.code(),
        }
    }
}

fn io_err(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> StatsError {
    let context = context.into();
    move |source| StatsError::Io { context, source }
}

/// Resolves a numeric `accounts` setting through the configuration cascade (Spec 10).
fn accounts_f64(project_path: Option<&str>, key: &'static str) -> Result<f64, StatsError> {
    crate::config::resolve_effective_value(project_path, None, "accounts", key)
        .as_f64()
        .ok_or(StatsError::InvalidConfig { plugin: "accounts", key, expected: "a number" })
}

fn accounts_u64(key: &'static str) -> Result<u64, StatsError> {
    crate::config::resolve_effective_value(None, None, "accounts", key)
        .as_u64()
        .ok_or(StatsError::InvalidConfig { plugin: "accounts", key, expected: "a non-negative integer" })
}

/// Rate-limit utilization summary for a rolling quota window (`5h` or `7d`).
#[derive(Serialize, Deserialize, Debug, Clone, Default, ts_rs::TS)]
#[ts(export)]
pub struct UsageWindowStats {
    pub label: String,                // "5h", "7d"
    pub used_percent: f64,            // e.g. 15.0
    pub remaining_percent: f64,       // e.g. 85.0
    pub resets_at: Option<String>,     // ISO 8601 or countdown e.g. "3h 20m"
    #[ts(type = "number | null")]
    pub reset_epoch_seconds: Option<i64>,     // Calculated target Unix epoch for window reset
    pub severity: String,             // "normal", "warning", "danger"
}

/// Prompt cache token breakdown and hit ratio for an agent session.
#[derive(Serialize, Deserialize, Debug, Clone, Default, ts_rs::TS)]
#[ts(export)]
pub struct CacheUsageStats {
    #[ts(type = "number")]
    pub fresh_input_tokens_count: u64,
    #[ts(type = "number")]
    pub read_tokens_count: u64,
    #[ts(type = "number")]
    pub creation_tokens_count: u64,
    pub hit_percent: f64,
    #[ts(type = "number | null")]
    pub ttl_seconds: Option<u64>,
}

/// Context window saturation percentage and cache statistics for an active session.
#[derive(Serialize, Deserialize, Debug, Clone, Default, ts_rs::TS)]
#[ts(export)]
pub struct AgentContextStats {
    pub used_percent: f64,
    pub remaining_percent: f64,
    pub cache: Option<CacheUsageStats>,
}

/// Aggregated telemetry and quota metrics for a single agent conversation session.
#[derive(Serialize, Deserialize, Debug, Clone, Default, ts_rs::TS)]
#[ts(export)]
pub struct AgentSessionMetrics {
    pub agent_key: Option<String>,
    pub conversation_id: Option<String>,
    pub provider: String,
    pub model: Option<String>,
    pub status: String,
    pub five_hour_window: Option<UsageWindowStats>,
    pub weekly_window: Option<UsageWindowStats>,
    pub context: Option<AgentContextStats>,
    pub source: String,               // "statusline", "transcript", "hybrid"
    #[ts(type = "number")]
    pub captured_at_epoch_seconds: u64,
    #[ts(type = "number | null")]
    pub tokens_used_count: Option<u64>,
    pub cost_dollars: Option<f64>,
    #[ts(type = "number | null")]
    pub max_context_tokens_count: Option<u64>,
}

/// Helper to get current epoch seconds.
fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Parse human countdown strings (e.g. "3h 20m", "3h17m", "5d 14h", "3d15h", "45m", "30s") or ISO strings ("2026-09-25T09:15:59Z") to a target Unix epoch.
pub fn parse_countdown_to_epoch(countdown_str: &str, now: i64) -> Option<i64> {
    let s = countdown_str.trim().trim_matches(|c| c == '(' || c == ')');
    if s.is_empty() || s.contains("N/A") || s.contains("n/a") {
        return None;
    }

    // 1. Attempt ISO-8601 / RFC 3339 timestamp parsing (e.g. "2026-09-25T09:15:59Z")
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.timestamp());
    }
    if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%SZ") {
        return Some(ndt.and_utc().timestamp());
    }
    if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Some(ndt.and_utc().timestamp());
    }
    if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Some(ndt.and_utc().timestamp());
    }

    // 2. Relative countdown string parsing
    let mut total_secs: i64 = 0;
    let mut matched_any = false;
    let mut num_buf = String::new();

    for c in s.chars() {
        if c.is_ascii_digit() {
            num_buf.push(c);
        } else if c == 'd' || c == 'h' || c == 'm' || c == 's' {
            if let Ok(val) = num_buf.parse::<i64>() {
                match c {
                    'd' => total_secs += val * 86400,
                    'h' => total_secs += val * 3600,
                    'm' => total_secs += val * 60,
                    's' => total_secs += val,
                    _ => {}
                }
                matched_any = true;
            }
            num_buf.clear();
        } else if c.is_whitespace() || c == ':' || c == ',' {
            num_buf.clear();
        }
    }

    if matched_any {
        Some(now + total_secs)
    } else {
        None
    }
}

/// Formats a Unix epoch timestamp into local date-time string ("YYYY-MM-DD HH:MM:SS")
/// according to the host machine's local timezone.
pub fn epoch_to_local_datetime_string(epoch: i64) -> String {
    if let Some(utc_dt) = chrono::DateTime::from_timestamp(epoch, 0) {
        let local_dt: chrono::DateTime<chrono::Local> = utc_dt.with_timezone(&chrono::Local);
        local_dt.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        String::new()
    }
}

/// Canonical quota pool resolver. Maps any Gemini variant to `("Agy", "gemini-pool")`
/// and any external model (Claude, GPT, etc.) to `("Anthropic", "3p-pool")`.
pub fn canonical_pool_identity(provider: &str, model: Option<&str>) -> (&'static str, &'static str) {
    let lower_model = model.unwrap_or("").to_ascii_lowercase();
    let lower_prov = provider.to_ascii_lowercase();

    if lower_model.contains("claude")
        || lower_model.contains("sonnet")
        || lower_model.contains("opus")
        || lower_model.contains("gpt")
        || lower_prov.contains("anthropic")
        || lower_prov.contains("openai")
    {
        ("Anthropic", "3p-pool")
    } else {
        ("Agy", "gemini-pool")
    }
}

/// StatusLine observation file path generated by the Antigravity `statusLine` hook.
///
/// Same resolution as `scripts/agy_statusline.py`: `ONESHIP_STATUSLINE_FILE`, else
/// `$XDG_STATE_HOME` (default `~/.local/state`) `/one-ship/agy-statusline.json`.
pub fn agy_statusline_file_path() -> Result<PathBuf, StatsError> {
    if let Ok(p) = std::env::var("ONESHIP_STATUSLINE_FILE") {
        return Ok(PathBuf::from(p));
    }
    let state_dir = dirs::state_dir().ok_or(StatsError::HomeUnavailable)?;
    Ok(state_dir.join("one-ship/agy-statusline.json"))
}

/// Parse a statusline JSON Value for a specific conversation session and account.
pub fn parse_statusline_json_for_session(
    val: &serde_json::Value,
    agy_conversation_id: Option<&str>,
    account_id: &str,
    now: i64,
) -> Option<crate::db::AccountQuotaRecord> {
    let is_active_account = crate::db::db_get_accounts()
        .ok()
        .and_then(|accs| accs.into_iter().find(|a| a.is_active).map(|a| a.id == account_id))
        .unwrap_or(false);

    // 1. Find model for this session or fallback
    let model_str = if let Some(cid) = agy_conversation_id {
        val.get("session_models")
            .and_then(|sm| sm.get(cid))
            .and_then(|m| m.as_str())
            .or_else(|| if is_active_account { val.get("model").and_then(|m| m.as_str()) } else { None })
    } else if is_active_account {
        val.get("model").and_then(|m| m.as_str())
    } else {
        None
    };

    let provider_str = val.get("provider").and_then(|p| p.as_str()).unwrap_or("Agy");
    let (canon_prov, canon_model) = canonical_pool_identity(provider_str, model_str);

    // 2. Extract quota windows from session_windows or root windows (root windows only allowed for active account)
    let windows_arr = if let Some(cid) = agy_conversation_id {
        val.get("session_windows")
            .and_then(|sw| sw.get(cid))
            .and_then(|w| w.as_array())
            .or_else(|| if is_active_account { val.get("windows").and_then(|w| w.as_array()) } else { None })
    } else if is_active_account {
        val.get("windows").and_then(|w| w.as_array())
    } else {
        None
    };

    if windows_arr.is_none() && is_active_account {
        if let Some(quota) = val.get("quota").and_then(|q| q.as_object()) {
            let (b_5h, b_w) = if canon_prov == "Anthropic" {
                (
                    quota.get("3p-5h").or_else(|| quota.get("claude-5h")),
                    quota.get("3p-weekly").or_else(|| quota.get("claude-weekly")),
                )
            } else {
                (
                    quota.get("gemini-5h"),
                    quota.get("gemini-weekly"),
                )
            };

            let p_5h = b_5h.and_then(|b| parse_window_bucket(b, now));
            let p_w = b_w.and_then(|b| parse_window_bucket(b, now));

            if p_5h.is_some() || p_w.is_some() {
                let ep_5h = p_5h.as_ref().and_then(|(_, _, e)| *e);
                let ep_w = p_w.as_ref().and_then(|(_, _, e)| *e);
                return Some(crate::db::AccountQuotaRecord {
                    id: None,
                    account_id: account_id.to_string(),
                    provider: canon_prov.to_string(),
                    model: canon_model.to_string(),
                    five_hour_remaining_pct: p_5h.as_ref().map(|(r, _, _)| *r),
                    five_hour_resets_at: p_5h.as_ref().and_then(|(_, c, _)| c.clone()),
                    five_hour_reset_epoch_seconds: ep_5h,
                    five_hour_resets_at_local: ep_5h.map(epoch_to_local_datetime_string),
                    weekly_remaining_pct: p_w.as_ref().map(|(r, _, _)| *r),
                    weekly_resets_at: p_w.as_ref().and_then(|(_, c, _)| c.clone()),
                    weekly_reset_epoch_seconds: ep_w,
                    weekly_resets_at_local: ep_w.map(epoch_to_local_datetime_string),
                    last_pinged_at: Some(now),
                    warmup_enabled: true,
                    status: "active".to_string(),
                    updated_at: now,
                });
            }
        }
    }

    let windows = windows_arr?;

    let mut five_h_rem: Option<f64> = None;
    let mut five_h_epoch: Option<i64> = None;
    let mut five_h_resets_at: Option<String> = None;

    let mut week_rem: Option<f64> = None;
    let mut week_epoch: Option<i64> = None;
    let mut weekly_resets_at: Option<String> = None;

    for w in windows {
        let kind = w.get("kind").and_then(|k| k.as_str()).unwrap_or("");
        let rem = w.get("remaining_percent").and_then(|r| r.as_f64());
        let reset_val = w.get("resets_at");

        let epoch = reset_val.and_then(|rv| {
            if let Some(num) = rv.as_i64() {
                Some(num)
            } else if let Some(s) = rv.as_str() {
                parse_countdown_to_epoch(s, now)
            } else {
                None
            }
        });

        let countdown_str = epoch.map(|ep| {
            let diff = (ep - now).max(0);
            if diff >= 86400 {
                format!("{}d {}h", diff / 86400, (diff % 86400) / 3600)
            } else if diff >= 3600 {
                format!("{}h {}m", diff / 3600, (diff % 3600) / 60)
            } else {
                format!("{}m", diff / 60)
            }
        });

        if kind == "five_hour" {
            five_h_rem = rem;
            five_h_epoch = epoch;
            five_h_resets_at = countdown_str;
        } else if kind == "weekly" {
            week_rem = rem;
            week_epoch = epoch;
            weekly_resets_at = countdown_str;
        }
    }

    if five_h_rem.is_none() && week_rem.is_none() {
        return None;
    }

    Some(crate::db::AccountQuotaRecord {
        id: None,
        account_id: account_id.to_string(),
        provider: canon_prov.to_string(),
        model: canon_model.to_string(),
        five_hour_remaining_pct: five_h_rem,
        five_hour_resets_at: five_h_resets_at,
        five_hour_reset_epoch_seconds: five_h_epoch,
        five_hour_resets_at_local: five_h_epoch.map(epoch_to_local_datetime_string),
        weekly_remaining_pct: week_rem,
        weekly_resets_at,
        weekly_reset_epoch_seconds: week_epoch,
        weekly_resets_at_local: week_epoch.map(epoch_to_local_datetime_string),
        last_pinged_at: Some(now),
        warmup_enabled: true,
        status: "active".to_string(),
        updated_at: now,
    })
}

/// Helper to parse a single window bucket from Antigravity's quota map (e.g. "gemini-5h", "3p-weekly").
fn parse_window_bucket(bucket: &serde_json::Value, now: i64) -> Option<(f64, Option<String>, Option<i64>)> {
    let rem = bucket.get("remaining_fraction")
        .or_else(|| bucket.get("remainingFraction"))
        .and_then(|v| v.as_f64())
        .map(|f| (f * 100.0).clamp(0.0, 100.0))
        .or_else(|| {
            bucket.get("remaining_percent")
                .or_else(|| bucket.get("remainingPercentage"))
                .and_then(|v| v.as_f64())
                .map(|p| p.clamp(0.0, 100.0))
        })?;

    let epoch = bucket.get("reset_in_seconds")
        .or_else(|| bucket.get("resetInSeconds"))
        .and_then(|v| v.as_i64())
        .map(|sec| now + sec)
        .or_else(|| {
            bucket.get("resets_at").and_then(|rv| {
                if let Some(num) = rv.as_i64() {
                    Some(num)
                } else if let Some(s) = rv.as_str() {
                    parse_countdown_to_epoch(s, now)
                } else {
                    None
                }
            })
        })
        .or_else(|| {
            bucket.get("reset_time")
                .or_else(|| bucket.get("resetTime"))
                .and_then(|v| v.as_str())
                .and_then(|s| parse_countdown_to_epoch(s, now))
        });

    let countdown_str = epoch.map(|ep| {
        let diff = (ep - now).max(0);
        if diff >= 86400 {
            format!("{}d {}h", diff / 86400, (diff % 86400) / 3600)
        } else if diff >= 3600 {
            format!("{}h {}m", diff / 3600, (diff % 3600) / 60)
        } else {
            format!("{}m", diff / 60)
        }
    });

    Some((rem, countdown_str, epoch))
}

/// Parse all available quota pools (Gemini and 3P/Anthropic) from a statusline JSON value.
pub fn parse_quota_records_from_value(
    val: &serde_json::Value,
    account_id: &str,
    now: i64,
) -> Vec<crate::db::AccountQuotaRecord> {
    let mut records = Vec::new();
    let quota_obj = val.get("quota").and_then(|q| q.as_object());

    if let Some(quota) = quota_obj {
        // 1. Gemini Pool (gemini-5h, gemini-weekly)
        let gemini_5h = quota.get("gemini-5h").and_then(|b| parse_window_bucket(b, now));
        let gemini_weekly = quota.get("gemini-weekly").and_then(|b| parse_window_bucket(b, now));

        if gemini_5h.is_some() || gemini_weekly.is_some() {
            let ep_5h = gemini_5h.as_ref().and_then(|(_, _, e)| *e);
            let ep_w = gemini_weekly.as_ref().and_then(|(_, _, e)| *e);
            records.push(crate::db::AccountQuotaRecord {
                id: None,
                account_id: account_id.to_string(),
                provider: "Agy".to_string(),
                model: "gemini-pool".to_string(),
                five_hour_remaining_pct: gemini_5h.as_ref().map(|(r, _, _)| *r),
                five_hour_resets_at: gemini_5h.as_ref().and_then(|(_, c, _)| c.clone()),
                five_hour_reset_epoch_seconds: ep_5h,
                five_hour_resets_at_local: ep_5h.map(epoch_to_local_datetime_string),
                weekly_remaining_pct: gemini_weekly.as_ref().map(|(r, _, _)| *r),
                weekly_resets_at: gemini_weekly.as_ref().and_then(|(_, c, _)| c.clone()),
                weekly_reset_epoch_seconds: ep_w,
                weekly_resets_at_local: ep_w.map(epoch_to_local_datetime_string),
                last_pinged_at: Some(now),
                warmup_enabled: true,
                status: "active".to_string(),
                updated_at: now,
            });
        }

        // 2. Third-Party Pool (3p-5h, 3p-weekly, claude-5h, claude-weekly)
        let p3_5h = quota.get("3p-5h")
            .or_else(|| quota.get("claude-5h"))
            .and_then(|b| parse_window_bucket(b, now));
        let p3_weekly = quota.get("3p-weekly")
            .or_else(|| quota.get("claude-weekly"))
            .and_then(|b| parse_window_bucket(b, now));

        if p3_5h.is_some() || p3_weekly.is_some() {
            let ep_5h = p3_5h.as_ref().and_then(|(_, _, e)| *e);
            let ep_w = p3_weekly.as_ref().and_then(|(_, _, e)| *e);
            records.push(crate::db::AccountQuotaRecord {
                id: None,
                account_id: account_id.to_string(),
                provider: "Anthropic".to_string(),
                model: "3p-pool".to_string(),
                five_hour_remaining_pct: p3_5h.as_ref().map(|(r, _, _)| *r),
                five_hour_resets_at: p3_5h.as_ref().and_then(|(_, c, _)| c.clone()),
                five_hour_reset_epoch_seconds: ep_5h,
                five_hour_resets_at_local: ep_5h.map(epoch_to_local_datetime_string),
                weekly_remaining_pct: p3_weekly.as_ref().map(|(r, _, _)| *r),
                weekly_resets_at: p3_weekly.as_ref().and_then(|(_, c, _)| c.clone()),
                weekly_reset_epoch_seconds: ep_w,
                weekly_resets_at_local: ep_w.map(epoch_to_local_datetime_string),
                last_pinged_at: Some(now),
                warmup_enabled: true,
                status: "active".to_string(),
                updated_at: now,
            });
        }
    }

    records
}

/// Directly syncs quota and metrics from the native Antigravity `statusLine` observation file
/// into SQLite (`account_quotas` & `agent_session_metrics`).
/// Reads attribution straight from `/proc`, guaranteeing deterministic attribution per conversation and account.
///
/// The observation must name its account (`account_id` or `email`); with `target_account_id` it
/// must name that account. `Ok(false)`: no observation file yet, or no quota record in it.
pub fn sync_quota_from_statusline_file(agy_conversation_id: Option<&str>, target_account_id: Option<&str>) -> Result<bool, StatsError> {
    let path = agy_statusline_file_path()?;
    crate::logger::log("DEBUG", "quota_sync", &format!("sync_quota_from_statusline_file: conversation={:?}, target_account={:?}", agy_conversation_id, target_account_id));
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(io_err(format!("Cannot read {}", path.display()))(e)),
    };
    let val: Value = serde_json::from_str(&content)
        .map_err(|source| StatsError::StatuslineJson { path: path.display().to_string(), source })?;

    let now = now_unix() as i64;
    let account_id = resolve_statusline_account(&val, target_account_id)?;

    // 1. Multi-pool Antigravity dictionary, else 2. single session windows.
    let mut records = parse_quota_records_from_value(&val, &account_id, now);
    if records.is_empty() {
        records.extend(parse_statusline_json_for_session(&val, agy_conversation_id, &account_id, now));
    }
    if records.is_empty() {
        return Ok(false);
    }
    for mut rec in records {
        // Preserve the account's warmup choice; a new pool starts enabled.
        rec.warmup_enabled = crate::db::db_get_account_quota(&account_id, &rec.provider, &rec.model)?
            .map(|r| r.warmup_enabled)
            .unwrap_or(true);
        crate::db::db_upsert_account_quota(&rec)?;
    }
    Ok(true)
}

/// Registered account an observation belongs to, by its `account_id`, else its `email`.
///
/// Cross-account contamination guard: an observation without identity is never attributed to
/// the active account, and with `target_account_id` it must match that account.
fn resolve_statusline_account(val: &Value, target_account_id: Option<&str>) -> Result<String, StatsError> {
    let file_account_id = val.get("account_id").and_then(|a| a.as_str());
    let file_email = val.get("email")
        .or_else(|| val.get("raw_payload").and_then(|r| r.get("email")))
        .and_then(|e| e.as_str());
    if file_account_id.is_none() && file_email.is_none() {
        return Err(StatsError::UnattributedStatusline);
    }
    let owned = |s: Option<&str>| s.map(String::from);

    if let Some(target_id) = target_account_id {
        let target = crate::db::db_get_account_by_id(target_id)?
            .ok_or_else(|| StatsError::UnknownAccount(target_id.to_string()))?;
        let matches_id = file_account_id == Some(target_id);
        let matches_email = file_email.is_some_and(|fe| fe.eq_ignore_ascii_case(&target.email));
        if !matches_id && !matches_email {
            return Err(StatsError::AccountMismatch {
                target: target_id.to_string(),
                file_account_id: owned(file_account_id),
                file_email: owned(file_email),
            });
        }
        return Ok(target_id.to_string());
    }

    let accounts = crate::db::db_get_accounts()?;
    file_account_id
        .and_then(|fid| accounts.iter().find(|a| a.id == fid))
        .or_else(|| file_email.and_then(|fe| accounts.iter().find(|a| a.email.eq_ignore_ascii_case(fe))))
        .map(|a| a.id.clone())
        .ok_or_else(|| StatsError::UnknownStatuslineAccount {
            file_account_id: owned(file_account_id),
            file_email: owned(file_email),
        })
}


/// Accumulate token usage from an agy conversation transcript
/// (`~/.gemini/antigravity-cli/brain/<id>/.system_generated/logs/transcript.jsonl`).
///
/// `Ok(None)`: no transcript yet, or no usage recorded in it. Lines that are not JSON are skipped
/// (agy appends while the file is read, so the last line can be partial); a read error fails.
pub fn parse_transcript_tokens(conversation_id: &str) -> Result<Option<CacheUsageStats>, StatsError> {
    if conversation_id.is_empty() || conversation_id.contains('/') || conversation_id.contains("..") {
        return Ok(None);
    }
    let home = dirs::home_dir().ok_or(StatsError::HomeUnavailable)?;
    let file_path = home.join(format!(".gemini/antigravity-cli/brain/{}/.system_generated/logs/transcript.jsonl", conversation_id));
    let file = match File::open(&file_path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_err(format!("Cannot open {}", file_path.display()))(e)),
    };
    let reader = BufReader::new(file);

    let mut total_fresh = 0u64;
    let mut total_read = 0u64;
    let mut total_creation = 0u64;

    for line in reader.lines() {
        let line = line.map_err(io_err(format!("Cannot read {}", file_path.display())))?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        let usage = entry
            .get("message")
            .and_then(|m| m.get("usage"))
            .or_else(|| entry.get("usage"));

        if let Some(u) = usage {
            let fresh = u.get("input_tokens")
                .or_else(|| u.get("inputTokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let read = u.get("cache_read_input_tokens")
                .or_else(|| u.get("cacheReadInputTokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let creation = u.get("cache_creation_input_tokens")
                .or_else(|| u.get("cacheCreationInputTokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            total_fresh = total_fresh.saturating_add(fresh);
            total_read = total_read.saturating_add(read);
            total_creation = total_creation.saturating_add(creation);
        }
    }

    let total = total_fresh.saturating_add(total_read).saturating_add(total_creation);
    if total == 0 {
        return Ok(None);
    }

    let hit_percent = (total_read as f64 / total as f64) * 100.0;

    Ok(Some(CacheUsageStats {
        fresh_input_tokens_count: total_fresh,
        read_tokens_count: total_read,
        creation_tokens_count: total_creation,
        hit_percent: hit_percent.clamp(0.0, 100.0),
        ttl_seconds: None,
    }))
}

/// Which conversation an agent metrics request names.
#[derive(Debug, Clone, Copy)]
pub enum ConversationRef<'a> {
    /// ONE-SHIP Conversation id; resolved to the newest agy session recorded in its turns.
    OneShip(&'a str),
    /// agy native conversation id.
    Native(&'a str),
}

/// Rolling quota window from a stored remaining percentage.
fn usage_window(
    label: &str,
    remaining: f64,
    resets_at: Option<String>,
    reset_epoch_seconds: Option<i64>,
    warning_threshold: f64,
    danger_threshold: f64,
) -> UsageWindowStats {
    let severity = if remaining <= danger_threshold {
        "danger"
    } else if remaining <= warning_threshold {
        "warning"
    } else {
        "normal"
    };
    UsageWindowStats {
        label: label.to_string(),
        used_percent: (100.0 - remaining).clamp(0.0, 100.0),
        remaining_percent: remaining,
        resets_at,
        reset_epoch_seconds,
        severity: severity.to_string(),
    }
}

/// Context window of `model` declared in the manifest of `runner` (`context_window_tokens_count`).
///
/// `Ok(None)` when the runner or model is unknown or the model declares no window.
pub fn context_window_tokens_count(runner: Option<&str>, model: Option<&str>) -> Result<Option<u64>, StatsError> {
    let (Some(runner), Some(model)) = (runner, model) else {
        return Ok(None);
    };
    let schema = crate::config::load_runner_schema(runner)?;
    Ok(schema.models.into_iter().find(|m| m.id == model).and_then(|m| m.context_window_tokens_count))
}

/// agy native session of an agent: the one named by `conversation`, else its live agy session,
/// else the newest one recorded in its turns.
fn resolve_native_session(project_path: &str, agent_id: &str, conversation: Option<ConversationRef<'_>>) -> Result<Option<String>, StatsError> {
    let named = match conversation {
        Some(ConversationRef::Native(id)) => return Ok(Some(id.to_string())),
        Some(ConversationRef::OneShip(cid)) => crate::db::db_latest_native_session_for_conversation(cid)?,
        None => None,
    };
    if named.is_some() {
        return Ok(named);
    }
    let project = crate::registry::find_project_by_path(project_path)?;
    Ok(crate::db::db_latest_native_session_for_agent(project.id as i64, agent_id)?)
}

/// Context, cache and quota metrics of one agent of one project. Read-only.
///
/// The context window comes from the runner manifest of the agent's process and the quota from
/// the account the process was launched with. When the session records no context percentage
/// and the window is unknown, or the account is unknown, that part stays empty: it is never
/// borrowed from another model or from the active account.
pub fn get_agent_metrics(project_path: &str, agent_id: &str, conversation: Option<ConversationRef<'_>>) -> Result<AgentSessionMetrics, StatsError> {
    crate::logger::log("DEBUG", "stats", &format!("get_agent_metrics: project={}, agent={}, conversation={:?}", project_path, agent_id, conversation));
    let key = crate::supervisor::ProcessSupervisor::format_agent_key(project_path, agent_id);
    let launch = crate::supervisor::get_supervisor().process_launch_identity(&key);
    let launch_runner = launch.as_ref().and_then(|l| l.runner.clone());
    let launch_model = launch.as_ref().and_then(|l| l.model.clone());
    let initial_window = context_window_tokens_count(launch_runner.as_deref(), launch_model.as_deref())?;

    let mut metrics = AgentSessionMetrics {
        agent_key: Some(key),
        conversation_id: None,
        provider: match launch_runner.as_deref() {
            Some("opencode") => "deepseek".to_string(),
            Some("claude") => "claude".to_string(),
            _ => "Agy".to_string(),
        },
        model: launch_model.clone(),
        status: "idle".to_string(),
        five_hour_window: None,
        weekly_window: None,
        context: None,
        source: "transcript".to_string(),
        captured_at_epoch_seconds: now_unix(),
        tokens_used_count: None,
        cost_dollars: None,
        max_context_tokens_count: initial_window,
    };

    let oneship_conv_id = match conversation {
        Some(ConversationRef::OneShip(cid)) => Some(cid.to_string()),
        _ => None,
    };

    let mut resolved_session_rec: Option<crate::db::AgentSessionMetricsRecord> = None;

    if let Some(native_id) = resolve_native_session(project_path, agent_id, conversation)? {
        let session_rec = crate::db::db_get_session_metrics(&native_id)?;
        if let Some(ref rec) = session_rec {
            if rec.model.is_some() {
                metrics.model = rec.model.clone();
            }
            if !rec.provider.is_empty() {
                metrics.provider = rec.provider.clone();
            }
            if let Some(w) = rec.max_context_tokens_count.filter(|&w| w > 0) {
                metrics.max_context_tokens_count = Some(w as u64);
            }
        }
        resolved_session_rec = session_rec.clone();

        if let Some(cache) = parse_transcript_tokens(&native_id)? {
            let eff_runner = session_rec.as_ref().and_then(|r| r.runner.as_deref()).or(launch_runner.as_deref());
            let eff_model = metrics.model.as_deref();
            let window = context_window_tokens_count(eff_runner, eff_model)?.or(metrics.max_context_tokens_count);
            metrics.max_context_tokens_count = window;
            let total_tokens = (cache.fresh_input_tokens_count + cache.read_tokens_count + cache.creation_tokens_count) as f64;
            metrics.tokens_used_count = Some(total_tokens as u64);
            let used_pct = session_rec.as_ref()
                .map(|r| r.context_used_pct)
                .filter(|pct| *pct > 0.0)
                .or_else(|| window.map(|w| (total_tokens / w as f64 * 100.0).clamp(0.0, 100.0)));
            match used_pct {
                Some(used) => {
                    metrics.context = Some(AgentContextStats {
                        used_percent: used,
                        remaining_percent: (100.0 - used).clamp(0.0, 100.0),
                        cache: Some(cache),
                    });
                    metrics.source = "hybrid".to_string();
                }
                None => crate::logger::log("DEBUG", "stats", &format!("get_agent_metrics: no context window for model {:?}; context percentage omitted", launch_model)),
            }
        }
        metrics.conversation_id = Some(native_id);
    }

    // Fallback to persisted live runner usage in `agent_session_metrics` (e.g. OpenCode/Claude/AGY
    // where local transcript files are absent or unparsed).
    if metrics.context.is_none() || metrics.tokens_used_count.is_none() {
        let fallback_rec = match resolved_session_rec {
            Some(r) => Some(r),
            None => match oneship_conv_id.as_deref() {
                Some(cid) => crate::db::db_get_session_metrics_by_conversation(cid)?,
                None => None,
            },
        };
        if let Some(rec) = fallback_rec {
            if metrics.model.is_none() {
                metrics.model = rec.model.clone();
            }
            if !rec.provider.is_empty() {
                metrics.provider = rec.provider.clone();
            }
            if metrics.max_context_tokens_count.is_none() {
                metrics.max_context_tokens_count = rec.max_context_tokens_count.filter(|&w| w > 0).map(|w| w as u64);
            }
            if metrics.tokens_used_count.is_none() && rec.total_tokens_count > 0 {
                metrics.tokens_used_count = Some(rec.total_tokens_count as u64);
            }
            if metrics.cost_dollars.is_none() {
                metrics.cost_dollars = rec.cost_dollars;
            }
            if metrics.context.is_none() && (rec.context_used_pct > 0.0 || rec.total_tokens_count > 0) {
                let used_pct = if rec.context_used_pct > 0.0 {
                    rec.context_used_pct
                } else if let Some(w) = metrics.max_context_tokens_count.filter(|&w| w > 0) {
                    (rec.total_tokens_count as f64 / w as f64 * 100.0).clamp(0.0, 100.0)
                } else {
                    0.0
                };
                metrics.context = Some(AgentContextStats {
                    used_percent: used_pct,
                    remaining_percent: (100.0 - used_pct).clamp(0.0, 100.0),
                    cache: None,
                });
                metrics.source = "agent_session_metrics".to_string();
            }
        }
    }

    if let Some(account_id) = launch.as_ref().and_then(|l| l.account_id.as_deref()) {
        let (canon_prov, canon_model) = canonical_pool_identity(&metrics.provider, metrics.model.as_deref());
        if let Some(quota) = crate::db::db_get_account_quota(account_id, canon_prov, canon_model)? {
            let warning_threshold = crate::config::resolve_effective_f64(None, None, "accounts", "quota_alert_threshold_percent");
            let danger_threshold = crate::config::resolve_effective_f64(None, None, "accounts", "quota_danger_threshold_percent");
            metrics.five_hour_window = quota.five_hour_remaining_pct
                .map(|rem| usage_window("5h", rem, quota.five_hour_resets_at.clone(), quota.five_hour_reset_epoch_seconds, warning_threshold, danger_threshold));
            metrics.weekly_window = quota.weekly_remaining_pct
                .map(|rem| usage_window("7d", rem, quota.weekly_resets_at.clone(), quota.weekly_reset_epoch_seconds, warning_threshold, danger_threshold));
        }
    }

    Ok(metrics)
}

/// Persists the session part of `metrics` (context and cache of its native session) under the
/// agent's ONE-SHIP Conversation. Quota windows are not written back: they are read from
/// `account_quotas`, which only the statusline sync and the warmup probe update.
pub fn record_session_metrics(metrics: &AgentSessionMetrics, project_id: i64, agent_id: &str, conversation_id: &str) -> Result<(), StatsError> {
    let Some(ref agy_id) = metrics.conversation_id else {
        return Ok(());
    };
    let now = now_unix() as i64;

    let cache = metrics.context.as_ref().and_then(|c| c.cache.as_ref());
    let (fresh, read, creation, hit_pct) = match cache {
        Some(cs) => (cs.fresh_input_tokens_count as i64, cs.read_tokens_count as i64, cs.creation_tokens_count as i64, cs.hit_percent),
        None => (0, 0, 0, 0.0),
    };
    let total = fresh + read + creation;
    let ctx_used = metrics.context.as_ref().map(|c| c.used_percent).unwrap_or(0.0);

    let session_rec = crate::db::AgentSessionMetricsRecord {
        agy_conversation_id: agy_id.clone(),
        conversation_id: conversation_id.to_string(),
        project_id,
        agent_id: agent_id.to_string(),
        provider: metrics.provider.clone(),
        model: metrics.model.clone(),
        context_used_pct: ctx_used,
        fresh_tokens_count: fresh,
        cache_read_tokens_count: read,
        cache_creation_tokens_count: creation,
        total_tokens_count: total,
        cache_hit_pct: hit_pct,
        five_hour_used_pct: metrics.five_hour_window.as_ref().map(|w| w.used_percent),
        five_hour_resets_at: metrics.five_hour_window.as_ref().and_then(|w| w.resets_at.clone()),
        five_hour_reset_epoch_seconds: metrics.five_hour_window.as_ref().and_then(|w| w.reset_epoch_seconds),
        weekly_used_pct: metrics.weekly_window.as_ref().map(|w| w.used_percent),
        weekly_resets_at: metrics.weekly_window.as_ref().and_then(|w| w.resets_at.clone()),
        weekly_reset_epoch_seconds: metrics.weekly_window.as_ref().and_then(|w| w.reset_epoch_seconds),
        updated_at: now,
        runner: None,
        cost_dollars: metrics.cost_dollars,
        max_context_tokens_count: metrics.max_context_tokens_count.map(|v| v as i64),
    };
    crate::db::db_upsert_session_metrics(&session_rec)?;
    Ok(())
}

// --- Daemon commands (synchronous ones are dispatched on the blocking pool) ---

/// Arguments of the daemon command `get_agent_metrics`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct GetAgentMetricsArgs {
    pub project_path: String,
    pub agent_id: String,
    /// A ONE-SHIP Conversation id; the agent's live or newest session when absent.
    pub conversation_id: Option<String>,
}

/// Metrics of one agent.
pub fn get_agent_metrics_command(args: GetAgentMetricsArgs) -> Result<AgentSessionMetrics, StatsError> {
    get_agent_metrics(&args.project_path, &args.agent_id, args.conversation_id.as_deref().map(ConversationRef::OneShip))
}

/// Arguments of the daemon command `get_session_metrics`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct GetSessionMetricsArgs {
    pub agy_conversation_id: String,
}

/// Stored metrics of one agy session.
pub fn get_session_metrics(args: GetSessionMetricsArgs) -> Result<Option<crate::db::AgentSessionMetricsRecord>, StatsError> {
    Ok(crate::db::db_get_session_metrics(&args.agy_conversation_id)?)
}

/// Quota windows of every account.
pub fn get_account_quotas() -> Result<Vec<crate::db::AccountQuotaRecord>, StatsError> {
    Ok(crate::db::db_get_account_quotas()?)
}

/// Arguments of the daemon command `toggle_account_warmup`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ToggleAccountWarmupArgs {
    pub account_id: String,
    pub enabled: bool,
}

/// Enables or disables the scheduled quota warmup of an account.
pub fn toggle_account_warmup(args: ToggleAccountWarmupArgs) -> Result<(), StatsError> {
    Ok(crate::db::db_toggle_account_warmup(&args.account_id, args.enabled)?)
}

/// Arguments of the daemon command `get_context_threshold`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct GetContextThresholdArgs {
    pub project_path: Option<String>,
}

/// Effective `accounts.context_warning_threshold_percent` (configuration cascade, Spec 10).
pub fn get_context_threshold(args: GetContextThresholdArgs) -> Result<f64, StatsError> {
    accounts_f64(args.project_path.as_deref(), "context_warning_threshold_percent")
}

/// Arguments of the daemon command `kickstart_account_quota`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct KickstartAccountQuotaArgs {
    pub account_id: String,
}

/// One-turn warmup probe of an account: swaps its credentials into the keyring, runs agy with
/// `accounts.quota_probe_model` in a PTY until the statusline hook reports this account, syncs
/// that observation, and restores the original credentials.
///
/// Fails when the probe cannot run, times out (`quota_probe_timeout_ms`) or the observation holds
/// no quota. `keyring-warmup-status` brackets the probe on every outcome.
pub async fn probe_account_quota(account_id: &str, target_pool: Option<&str>) -> Result<(), StatsError> {
    crate::logger::log("DEBUG", "quota_kickstart", &format!("probe_account_quota: account={}, pool={:?}", account_id, target_pool));
    let acc = crate::db::db_get_account_by_id(account_id)?
        .ok_or_else(|| StatsError::UnknownAccount(account_id.to_string()))?;
    let settings = ProbeSettings::resolve()?;

    let original_token = crate::kopeng::accounts::read_keyring_token()?;
    let refresh_token = crate::kopeng::accounts::decrypt_refresh_token(&acc.enc_token)?;

    let payload = serde_json::json!({
        "token": {
            "access_token": "",
            "token_type": "Bearer",
            "refresh_token": refresh_token,
            "expiry": "2000-01-01T00:00:00Z"
        },
        "auth_method": "consumer",
        "id_token": "",
        "_account_email": acc.email
    });

    let _guard = crate::kopeng::accounts::keyring_lock().lock().await;

    // Record probe ping timestamp immediately to enforce cooldown and prevent race conditions
    crate::db::db_update_account_quota_ping(account_id, now_unix() as i64)?;

    let pool_name = target_pool.unwrap_or("gemini-pool");
    let sup = crate::supervisor::get_supervisor();
    sup.emit_event(crate::events::DaemonEvent::keyring_warmup_status(
        &crate::events::KeyringWarmupStatusPayload {
            in_progress: true,
            account_id: account_id.to_string(),
            email: Some(acc.email.clone()),
            pool: pool_name.to_string(),
        },
    ));

    let result = run_quota_probe(&acc, &payload, &original_token, &settings).await;

    sup.emit_event(crate::events::DaemonEvent::keyring_warmup_status(
        &crate::events::KeyringWarmupStatusPayload {
            in_progress: false,
            account_id: account_id.to_string(),
            email: None,
            pool: pool_name.to_string(),
        },
    ));
    match &result {
        Ok(()) => crate::logger::log("INFO", "quota_kickstart", &format!("Captured live quota for account {}", acc.email)),
        Err(e) => crate::logger::log("ERROR", "quota_kickstart", &e.to_string()),
    }
    result
}

/// Warmup probe settings from the `accounts` configuration (Spec 10).
struct ProbeSettings {
    /// agy model and effort of the probe, a pair declared by the agy manifest.
    pair: crate::kopeng::resolution::EffectiveAgentConfig,
    timeout_ms: u64,
    poll_step_ms: u64,
}

impl ProbeSettings {
    fn resolve() -> Result<Self, StatsError> {
        let value = |key: &str| crate::config::resolve_effective_string(None, None, "accounts", key);
        let pair = crate::kopeng::resolution::finalize_agent_config(
            "plugins.accounts.quota_probe_model",
            crate::config::QUOTA_PROBE_RUNNER,
            Some(&value("quota_probe_model")),
            Some(&value("quota_probe_effort")),
        )?;
        let timeout_ms = accounts_u64("quota_probe_timeout_ms")?;
        let poll_step_ms = accounts_u64("quota_probe_poll_step_ms")?;
        if poll_step_ms == 0 {
            return Err(StatsError::InvalidConfig { plugin: "accounts", key: "quota_probe_poll_step_ms", expected: "greater than 0" });
        }
        Ok(Self { pair, timeout_ms, poll_step_ms })
    }

    /// agy arguments of the one-turn probe; the model and effort flags come from the agy manifest.
    fn agy_args(&self) -> Result<Vec<String>, StatsError> {
        let schema = crate::config::load_runner_schema(crate::config::QUOTA_PROBE_RUNNER)?;
        let mut args = vec!["-i".to_string(), "hi".to_string()];
        let model_flag = schema.flags.model.as_deref().unwrap_or("--model");
        args.extend([model_flag.to_string(), self.pair.model.clone()]);
        if let (Some(flag), Some(effort)) = (schema.flags.effort.as_deref(), self.pair.effort.as_deref()) {
            args.extend([flag.to_string(), effort.to_string()]);
        }
        args.push("--disable-slash-commands".to_string());
        Ok(args)
    }
}

/// Restores the keyring credentials replaced by a probe, on every exit path.
struct KeyringRestoreGuard<'a> {
    original_token: &'a Option<serde_json::Value>,
}

impl Drop for KeyringRestoreGuard<'_> {
    fn drop(&mut self) {
        let restored = match self.original_token {
            Some(orig) => crate::kopeng::accounts::write_keyring_token(orig),
            None => crate::kopeng::accounts::ensure_active_account_in_keyring(),
        };
        crate::logger::log_failure("quota_kickstart", "restore keyring credentials after probe", restored);
    }
}

/// `dup` of a PTY descriptor as an owned `File`.
fn dup_file(fd: libc::c_int) -> std::io::Result<std::fs::File> {
    use std::os::unix::io::FromRawFd;
    // SAFETY: `fd` is an open descriptor owned by the caller; `dup` returns a new one or -1.
    let new_fd = unsafe { libc::dup(fd) };
    if new_fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: `new_fd` is a fresh descriptor owned by nothing else.
    Ok(unsafe { std::fs::File::from_raw_fd(new_fd) })
}

/// Whether a statusline observation was written for `acc` at or after `since_epoch_seconds`.
fn observation_matches(content: &str, acc: &crate::db::AccountRecord, since_epoch_seconds: i64) -> bool {
    let Ok(val) = serde_json::from_str::<Value>(content) else {
        return false; // the hook may be mid-write; the next poll reads it again
    };
    let ts = val.get("timestamp").and_then(|t| t.as_i64());
    let file_email = val.get("email")
        .or_else(|| val.get("raw_payload").and_then(|r| r.get("email")))
        .and_then(|e| e.as_str());
    let file_acc_id = val.get("account_id").and_then(|a| a.as_str());
    let is_account = file_acc_id == Some(acc.id.as_str()) || file_email.is_some_and(|e| e.eq_ignore_ascii_case(&acc.email));
    is_account && ts.is_some_and(|t| t >= since_epoch_seconds)
}

async fn run_quota_probe(
    acc: &crate::db::AccountRecord,
    payload: &serde_json::Value,
    original_token: &Option<serde_json::Value>,
    settings: &ProbeSettings,
) -> Result<(), StatsError> {
    let failed = |reason: String| StatsError::ProbeFailed { email: acc.email.clone(), reason };
    let statusline_path = agy_statusline_file_path()?;

    crate::kopeng::accounts::write_keyring_token(payload)?;
    let restore_guard = KeyringRestoreGuard { original_token };
    let probe_start_epoch = now_unix() as i64 - 1;

    // Antigravity only runs the statusLine hook (scripts/agy_statusline.py) inside an
    // interactive terminal, so the probe gets a PTY.
    let mut master_fd: libc::c_int = -1;
    let mut slave_fd: libc::c_int = -1;
    // SAFETY: out-pointers to two local descriptors; null name/termios/winsize are allowed.
    let pty_res = unsafe {
        libc::openpty(&mut master_fd, &mut slave_fd, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut())
    };
    if pty_res != 0 {
        return Err(failed(format!("cannot allocate a PTY: {}", std::io::Error::last_os_error())));
    }
    use std::os::unix::io::FromRawFd;
    // SAFETY: `openpty` succeeded, so both descriptors are open and owned here.
    let master_file = unsafe { std::fs::File::from_raw_fd(master_fd) };
    let slave_file = unsafe { std::fs::File::from_raw_fd(slave_fd) };
    let slave_io = (dup_file(slave_fd), dup_file(slave_fd), dup_file(slave_fd));
    drop(slave_file);
    let (slave_in, slave_out, slave_err) = match slave_io {
        (Ok(i), Ok(o), Ok(e)) => (i, o, e),
        (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => return Err(failed(format!("cannot duplicate the PTY: {}", e))),
    };

    // Drain the master side so the child never blocks on output; ends at EOF when the child exits.
    tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let mut file = master_file;
        let mut buf = [0u8; 1024];
        while let Ok(n) = file.read(&mut buf) {
            if n == 0 {
                break;
            }
        }
    });

    let probe_bin = crate::supervisor::resolve_executable(crate::config::QUOTA_PROBE_RUNNER);
    let mut child = tokio::process::Command::new(&probe_bin.program)
        .args(settings.agy_args()?)
        .stdin(std::process::Stdio::from(slave_in))
        .stdout(std::process::Stdio::from(slave_out))
        .stderr(std::process::Stdio::from(slave_err))
        .spawn()
        .map_err(|e| failed(format!("cannot spawn agy: {}", e)))?;

    let max_polls_count = settings.timeout_ms.div_ceil(settings.poll_step_ms);
    let mut observed = false;
    for _ in 0..max_polls_count {
        tokio::time::sleep(tokio::time::Duration::from_millis(settings.poll_step_ms)).await;
        match tokio::fs::read_to_string(&statusline_path).await {
            Ok(content) if observation_matches(&content, acc, probe_start_epoch) => {
                observed = true;
                break;
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_err(format!("Cannot read {}", statusline_path.display()))(e)),
        }
    }

    crate::logger::log_failure("quota_kickstart", "kill agy probe", child.kill().await);
    drop(restore_guard);

    if !observed {
        return Err(failed(format!("no statusline observation for the account within {} ms", settings.timeout_ms)));
    }
    let account_id = acc.id.clone();
    let synced = tokio::task::spawn_blocking(move || sync_quota_from_statusline_file(None, Some(&account_id)))
        .await
        .map_err(|e| StatsError::Task(e.to_string()))??;
    if !synced {
        return Err(failed("the statusline observation holds no quota record".to_string()));
    }
    Ok(())
}

/// Spawns a background task ticking every 60 seconds that checks whether any accounts
/// with `warmup_enabled = true` have expired rolling quota reset epochs.
/// If an account's quota reset epoch has passed and has not been pinged since the reset,
/// it executes `probe_account_quota` and emits `AccountQuotaUpdated`.
pub fn spawn_quota_watcher(
    tx: tokio::sync::broadcast::Sender<crate::events::DaemonEvent>,
    cancel_token: tokio_util::sync::CancellationToken,
) {
    tauri::async_runtime::spawn(async move {
        let refresh_s = match accounts_u64("stats_refresh_interval_seconds") {
            Ok(s) if s > 0 => s,
            Ok(_) => {
                crate::logger::log("ERROR", "quota_watcher", "accounts.stats_refresh_interval_seconds must be greater than 0; quota watcher not started");
                return;
            }
            Err(e) => {
                crate::logger::log("ERROR", "quota_watcher", &format!("{}; quota watcher not started", e));
                return;
            }
        };
        let mut refresh_interval = tokio::time::interval(tokio::time::Duration::from_secs(refresh_s));
        refresh_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        crate::logger::log("INFO", "quota_watcher", &format!("Spawned background account quota warmup watcher (tick={}s)", refresh_s));

        // Startup sync from the statusline file only; never probe on boot.
        sync_statusline_on_blocking_pool().await;

        // Consume initial tick immediately so that scheduled checks only start after the first interval
        refresh_interval.tick().await;

        let mut last_check_epoch_seconds = now_unix() as i64;

        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    crate::logger::log("INFO", "quota_watcher", "Quota watcher cancelled");
                    break;
                }
                _ = refresh_interval.tick() => {
                    let now = now_unix() as i64;
                    crate::logger::log_failure("quota_watcher", "quota watcher tick", quota_watcher_tick(&tx, last_check_epoch_seconds, now).await);
                    last_check_epoch_seconds = now;
                }
            }
        }
    });
}

async fn sync_statusline_on_blocking_pool() {
    match tokio::task::spawn_blocking(|| sync_quota_from_statusline_file(None, None)).await {
        Ok(result) => crate::logger::log_failure("quota_watcher", "statusline quota sync", result),
        Err(e) => crate::logger::log("ERROR", "quota_watcher", &format!("Statusline quota sync task failed: {}", e)),
    }
}

/// One watcher tick: statusline sync, then (when `accounts.auto_warmup_enabled`) a warmup probe
/// of every depleted pool whose reset epoch has arrived and whose cooldown has elapsed.
async fn quota_watcher_tick(
    tx: &tokio::sync::broadcast::Sender<crate::events::DaemonEvent>,
    last_check_epoch_seconds: i64,
    now: i64,
) -> Result<(), StatsError> {
    sync_statusline_on_blocking_pool().await;

    let auto_warmup = crate::config::resolve_effective_value(None, None, "accounts", "auto_warmup_enabled")
        .as_bool()
        .ok_or(StatsError::InvalidConfig { plugin: "accounts", key: "auto_warmup_enabled", expected: "a boolean" })?;
    if !auto_warmup {
        return Ok(());
    }
    let depleted_threshold_pct = accounts_f64(None, "quota_depleted_threshold_percent")?;
    let cooldown_seconds = accounts_u64("quota_probe_cooldown_seconds")? as i64;

    let quotas = tokio::task::spawn_blocking(crate::db::db_get_account_quotas)
        .await
        .map_err(|e| StatsError::Task(e.to_string()))??;

    for q in quotas {
        if !q.warmup_enabled {
            continue;
        }
        // Only a depleted pool warrants a warmup; healthy accounts are never probed.
        let five_hour_depleted = q.five_hour_remaining_pct.is_some_and(|p| p <= depleted_threshold_pct);
        let weekly_depleted = q.weekly_remaining_pct.is_some_and(|p| p <= depleted_threshold_pct);
        if !five_hour_depleted && !weekly_depleted {
            continue;
        }
        if q.last_pinged_at.is_some_and(|lp| now - lp < cooldown_seconds) {
            continue;
        }

        // A reset epoch arrived during this run (last check < epoch <= now), or was reached and
        // the account has not been pinged since.
        let reset_arrived = |epoch: Option<i64>| {
            epoch.is_some_and(|e| {
                (e <= now && e > last_check_epoch_seconds) || (now >= e && q.last_pinged_at.is_none_or(|lp| lp < e))
            })
        };
        let five_hour_reset_arrived = reset_arrived(q.five_hour_reset_epoch_seconds);
        let weekly_reset_arrived = reset_arrived(q.weekly_reset_epoch_seconds);
        if !((five_hour_depleted && five_hour_reset_arrived) || (weekly_depleted && weekly_reset_arrived)) {
            continue;
        }

        crate::logger::log(
            "INFO",
            "quota_watcher",
            &format!(
                "Target reset time reached for account {} pool {} (5h_arr={}, w_arr={}). Triggering scheduled warmup...",
                q.account_id, q.model, five_hour_reset_arrived, weekly_reset_arrived
            ),
        );
        // A failed probe is logged by `probe_account_quota`; the other pools are still checked.
        if probe_account_quota(&q.account_id, Some(&q.model)).await.is_ok() {
            let _ = tx.send(crate::events::DaemonEvent::account_quota_updated(
                &crate::events::AccountQuotaUpdatedPayload {
                    account_id: Some(q.account_id.clone()),
                    model: Some(q.model.clone()),
                    probed_at: Some(now),
                    source: None,
                    agent_id: None,
                },
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_countdown_to_epoch() {
        let now = 1000000;
        assert_eq!(parse_countdown_to_epoch("3h 20m", now), Some(now + 12000));
        assert_eq!(parse_countdown_to_epoch("3h17m", now), Some(now + 11820));
        assert_eq!(parse_countdown_to_epoch("5d 14h", now), Some(now + 482400));
        assert_eq!(parse_countdown_to_epoch("3d15h", now), Some(now + 313200));
        assert_eq!(parse_countdown_to_epoch("45m", now), Some(now + 2700));
        assert_eq!(parse_countdown_to_epoch("(30s)", now), Some(now + 30));
        assert_eq!(parse_countdown_to_epoch("5h N/A", now), None);
        assert_eq!(parse_countdown_to_epoch("2026-09-25T09:15:59Z", now), Some(1790327759));
        assert_eq!(parse_countdown_to_epoch("2026-09-25T09:15:59", now), Some(1790327759));
        assert!(epoch_to_local_datetime_string(1790327759).contains("2026-09-25"));
    }

    #[test]
    fn test_canonical_pool_identity() {
        assert_eq!(canonical_pool_identity("Agy", Some("Gemini 3.8 Flash (High)")), ("Agy", "gemini-pool"));
        assert_eq!(canonical_pool_identity("Agy", Some("gemini-3.7-flash-low")), ("Agy", "gemini-pool"));
        assert_eq!(canonical_pool_identity("Agy", Some("gemini-3.1-pro-high")), ("Agy", "gemini-pool"));
        assert_eq!(canonical_pool_identity("Anthropic", Some("Claude Sonnet 4.6 (Thinking)")), ("Anthropic", "3p-pool"));
        assert_eq!(canonical_pool_identity("Agy", Some("claude-opus-4-6-thinking")), ("Anthropic", "3p-pool"));
        assert_eq!(canonical_pool_identity("OpenAI", Some("gpt-oss-120b-medium")), ("Anthropic", "3p-pool"));
    }

    #[test]
    fn test_parse_statusline_json_for_session() {
        let json_val = serde_json::json!({
            "provider": "agy",
            "model": "Gemini 3.7 Flash (High)",
            "windows": [
                {
                    "kind": "five_hour",
                    "remaining_percent": 90.0,
                    "resets_at": 1800000000
                },
                {
                    "kind": "weekly",
                    "remaining_percent": 80.0,
                    "resets_at": 1800500000
                }
            ],
            "session_models": {
                "sess-gemini-1": "Gemini 3.8 Flash (Low)",
                "sess-claude-2": "Claude Sonnet 4.6 (Thinking)"
            },
            "session_windows": {
                "sess-gemini-1": [
                    {
                        "kind": "five_hour",
                        "remaining_percent": 65.0,
                        "resets_at": 1800010000
                    },
                    {
                        "kind": "weekly",
                        "remaining_percent": 45.0,
                        "resets_at": 1800510000
                    }
                ],
                "sess-claude-2": [
                    {
                        "kind": "five_hour",
                        "remaining_percent": 30.0,
                        "resets_at": 1800020000
                    },
                    {
                        "kind": "weekly",
                        "remaining_percent": 15.0,
                        "resets_at": 1800520000
                    }
                ]
            }
        });

        let now = 1799990000;

        // 1. Exact Gemini session attribution
        let gemini_rec = parse_statusline_json_for_session(&json_val, Some("sess-gemini-1"), "acc-1", now).expect("gemini rec");
        assert_eq!(gemini_rec.account_id, "acc-1");
        assert_eq!(gemini_rec.provider, "Agy");
        assert_eq!(gemini_rec.model, "gemini-pool");
        assert_eq!(gemini_rec.five_hour_remaining_pct, Some(65.0));
        assert_eq!(gemini_rec.weekly_remaining_pct, Some(45.0));
        assert_eq!(gemini_rec.five_hour_reset_epoch_seconds, Some(1800010000));
        assert_eq!(gemini_rec.weekly_reset_epoch_seconds, Some(1800510000));

        // 2. Exact Claude session attribution
        let claude_rec = parse_statusline_json_for_session(&json_val, Some("sess-claude-2"), "acc-2", now).expect("claude rec");
        assert_eq!(claude_rec.account_id, "acc-2");
        assert_eq!(claude_rec.provider, "Anthropic");
        assert_eq!(claude_rec.model, "3p-pool");
        assert_eq!(claude_rec.five_hour_remaining_pct, Some(30.0));
        assert_eq!(claude_rec.weekly_remaining_pct, Some(15.0));
        assert_eq!(claude_rec.five_hour_reset_epoch_seconds, Some(1800020000));
        assert_eq!(claude_rec.weekly_reset_epoch_seconds, Some(1800520000));

        // 3. Non-active account without a valid conversation ID must safely return None (no active root pollution)
        let non_active_rec = parse_statusline_json_for_session(&json_val, None, "non-active-acc", now);
        assert!(non_active_rec.is_none());

        let unknown_sess_rec = parse_statusline_json_for_session(&json_val, Some("unknown-cid"), "non-active-acc", now);
        assert!(unknown_sess_rec.is_none());

        // 4. Active account can resolve root window when conversation ID is None
        if let Ok(accounts) = crate::db::db_get_accounts() {
            if let Some(active_acc) = accounts.into_iter().find(|a| a.is_active) {
                let root_rec = parse_statusline_json_for_session(&json_val, None, &active_acc.id, now).expect("active root rec");
                assert_eq!(root_rec.account_id, active_acc.id);
                assert_eq!(root_rec.provider, "Agy");
                assert_eq!(root_rec.model, "gemini-pool");
                assert_eq!(root_rec.five_hour_remaining_pct, Some(90.0));
                assert_eq!(root_rec.weekly_remaining_pct, Some(80.0));
            }
        }
    }

    #[test]
    fn test_parse_quota_records_from_antigravity_dict() {
        let json_val = serde_json::json!({
            "quota": {
                "gemini-5h": {
                    "remaining_fraction": 0.858,
                    "reset_in_seconds": 12345,
                    "reset_time": "2026-09-17T17:00:00Z"
                },
                "gemini-weekly": {
                    "remaining_fraction": 0.967,
                    "reset_in_seconds": 345678,
                    "reset_time": "2026-09-21T12:00:00Z"
                },
                "3p-5h": {
                    "remaining_fraction": 1.0,
                    "reset_in_seconds": 12345,
                    "reset_time": "2026-09-17T17:00:00Z"
                },
                "3p-weekly": {
                    "remaining_fraction": 0.0,
                    "reset_in_seconds": 0,
                    "reset_time": "2026-09-17T13:00:00Z"
                }
            }
        });
        let now = 1789520000;
        let records = parse_quota_records_from_value(&json_val, "test-acc-id", now);
        assert_eq!(records.len(), 2);

        let gemini = records.iter().find(|r| r.model == "gemini-pool").expect("gemini pool");
        assert_eq!(gemini.provider, "Agy");
        assert!((gemini.five_hour_remaining_pct.unwrap() - 85.8).abs() < 0.01);
        assert!((gemini.weekly_remaining_pct.unwrap() - 96.7).abs() < 0.01);
        assert_eq!(gemini.five_hour_reset_epoch_seconds, Some(now + 12345));
        assert_eq!(gemini.weekly_reset_epoch_seconds, Some(now + 345678));

        let third_party = records.iter().find(|r| r.model == "3p-pool").expect("3p pool");
        assert_eq!(third_party.provider, "Anthropic");
        assert_eq!(third_party.five_hour_remaining_pct, Some(100.0));
        assert_eq!(third_party.weekly_remaining_pct, Some(0.0));
        assert_eq!(third_party.five_hour_reset_epoch_seconds, Some(now + 12345));
        assert_eq!(third_party.weekly_reset_epoch_seconds, Some(now));
    }
}

