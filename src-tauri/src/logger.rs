//! Universal logger: rotating `oneship.log` file sink plus the batched `system_logs` SQLite sink.
//!
//! Both sinks honour `system.log_level` (DEBUG < INFO < WARN < ERROR); entries below it are
//! dropped before any I/O. Settings are re-resolved only when `config.toml` changes, so a log call
//! costs one `stat` instead of a config read and parse. The file stays open between calls and
//! rotates by tracked size. SQLite inserts go through a bounded channel to one worker thread that
//! commits micro-batches and prunes `system_logs` down to `system.log_db_max_rows_count` every
//! `system.log_db_prune_interval_seconds` (AGENTS.md §12.3).

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

/// Default for `system.log_level` when an unrecognized level string is encountered.
const DEFAULT_LOG_LEVEL: &str = "INFO";
/// Default for `system.log_db_max_rows_count` (referenced in `config.rs`).
pub const DEFAULT_LOG_DB_MAX_ROWS_COUNT: usize = 50_000;

const LOG_CHANNEL_CAPACITY_COUNT: usize = 4096;
const LOG_BATCH_MAX_ITEMS_COUNT: usize = 64;

/// Entries not queued for SQLite because the channel was full; the worker reports and resets it
/// in `oneship.log` (the file sink always has them).
static DROPPED_DB_LOGS_COUNT: AtomicU64 = AtomicU64::new(0);

/// Set by the desktop app, which does not open the database (spec 12 §7.3): its entries go only
/// to `oneship.log`. The daemon keeps both sinks; its webview logs arrive via `write_client_log`.
static FILE_SINK_ONLY: AtomicBool = AtomicBool::new(false);

/// Restricts this process to the file sink. Called once by the desktop app at startup.
pub fn use_file_sink_only() {
    FILE_SINK_ONLY.store(true, Ordering::Relaxed);
}

/// Errors of the frontend logger entry point.
#[derive(Debug, thiserror::Error)]
pub enum LoggerError {
    #[error("Unknown log level '{0}'; expected DEBUG, INFO, WARN or ERROR")]
    UnknownLevel(String),
}

impl crate::api::ApiError for LoggerError {
    fn status(&self) -> axum::http::StatusCode {
        match self {
            Self::UnknownLevel(_) => axum::http::StatusCode::BAD_REQUEST,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::UnknownLevel(_) => "LoggerError::UnknownLevel",
        }
    }
}

/// Arguments of `write_client_log`.
#[derive(serde::Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct WriteClientLogArgs {
    pub level: String,
    pub module: String,
    pub message: String,
}

/// Logger knobs resolved from the global configuration (all `global_only`, plugin `system`).
#[derive(Clone, Copy, Debug)]
struct LoggerSettings {
    min_level_rank: u8,
    max_file_size_bytes: u64,
    max_rotations_count: usize,
    db_max_rows_count: usize,
    db_prune_interval_seconds: u64,
}

impl LoggerSettings {
    fn resolve() -> Self {
        let level = crate::config::resolve_effective_string(None, None, "system", "log_level");
        let min_level_rank = level_rank(&level).unwrap_or_else(|| {
            level_rank(DEFAULT_LOG_LEVEL).expect("default log level is a known level")
        });
        Self {
            min_level_rank,
            max_file_size_bytes: crate::config::resolve_effective_u64(None, None, "system", "log_max_size_bytes"),
            max_rotations_count: crate::config::resolve_effective_usize(None, None, "system", "log_max_rotations_count"),
            db_max_rows_count: crate::config::resolve_effective_usize(None, None, "system", "log_db_max_rows_count"),
            db_prune_interval_seconds: crate::config::resolve_effective_u64(None, None, "system", "log_db_prune_interval_seconds"),
        }
    }
}

/// Settings cache keyed by the `config.toml` mtime observed when they were resolved.
static SETTINGS_CACHE: Mutex<Option<(Option<SystemTime>, LoggerSettings)>> = Mutex::new(None);

/// Returns the current settings, re-resolving them only when `config.toml` changed.
///
/// Config resolution never logs for global-only keys, so this cannot re-enter the logger.
fn current_settings() -> LoggerSettings {
    let config_mtime = crate::config::get_global_config_path()
        .ok()
        .and_then(|path| fs::metadata(path).and_then(|m| m.modified()).ok());
    {
        let cache = SETTINGS_CACHE.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((cached_mtime, settings)) = cache.as_ref() {
            if *cached_mtime == config_mtime {
                return *settings;
            }
        }
    }
    let settings = LoggerSettings::resolve();
    *SETTINGS_CACHE.lock().unwrap_or_else(PoisonError::into_inner) = Some((config_mtime, settings));
    settings
}

/// Severity rank of a level name (case-insensitive); `None` for unknown names.
fn level_rank(level: &str) -> Option<u8> {
    match level.to_ascii_uppercase().as_str() {
        "DEBUG" => Some(0),
        "INFO" => Some(1),
        "WARN" => Some(2),
        "ERROR" => Some(3),
        _ => None,
    }
}

/// One log entry queued for the SQLite batch worker.
#[derive(Clone, Debug)]
pub struct QueuedLog {
    pub level: String,
    pub source: String,
    pub origin: String,
    pub message: String,
}

/// Open handle of `oneship.log` and its size in bytes, tracked to rotate without `stat` calls.
struct LogFileSink {
    file: File,
    size_bytes: u64,
}

/// Guards the file sink; poison is recovered because the state is only a file handle.
static LOG_FILE: Mutex<Option<LogFileSink>> = Mutex::new(None);

static LOG_SENDER: OnceLock<SyncSender<QueuedLog>> = OnceLock::new();

fn get_log_sender() -> &'static SyncSender<QueuedLog> {
    LOG_SENDER.get_or_init(|| {
        let (tx, rx) = sync_channel::<QueuedLog>(LOG_CHANNEL_CAPACITY_COUNT);
        thread::Builder::new()
            .name("oneship-sqlite-logger".into())
            .spawn(move || {
                let mut batch = Vec::with_capacity(LOG_BATCH_MAX_ITEMS_COUNT);
                let mut last_prune_at = Instant::now();
                while let Ok(first) = rx.recv() {
                    batch.push(first);
                    while batch.len() < LOG_BATCH_MAX_ITEMS_COUNT {
                        match rx.try_recv() {
                            Ok(item) => batch.push(item),
                            Err(_) => break,
                        }
                    }
                    if let Err(e) = crate::db::db_insert_logs_batch(&batch) {
                        // Logging this through `log` would queue another insert into the failing sink.
                        write_file_line(&format_line("ERROR", "logger", "system", &format!("system_logs batch insert failed: {}", e)), &current_settings());
                    }
                    batch.clear();

                    let settings = current_settings();
                    let dropped_count = DROPPED_DB_LOGS_COUNT.swap(0, Ordering::Relaxed);
                    if dropped_count > 0 {
                        write_file_line(&format_line("WARN", "logger", "system", &format!("{} entries dropped from system_logs: the SQLite log channel was full", dropped_count)), &settings);
                    }
                    if last_prune_at.elapsed() >= Duration::from_secs(settings.db_prune_interval_seconds) {
                        last_prune_at = Instant::now();
                        if let Err(e) = crate::db::db_prune_system_logs(settings.db_max_rows_count) {
                            write_file_line(&format_line("ERROR", "logger", "system", &format!("system_logs prune failed: {}", e)), &settings);
                        }
                    }
                }
            })
            .expect("failed to spawn sqlite logger thread");
        tx
    })
}

/// Test builds never write the user's `oneship.log`: logs go to the per-process test directory.
#[cfg(test)]
fn get_logs_dir() -> PathBuf {
    let path = crate::config::test_process_dir().join("logs");
    let _ = fs::create_dir_all(&path);
    path
}

#[cfg(not(test))]
fn get_logs_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let mut path = PathBuf::from(home);
    path.push(".config");
    path.push("one-ship");
    path.push("logs");
    let _ = fs::create_dir_all(&path);
    path
}

/// Shifts `oneship.log` -> `oneship.1.log` -> ... and drops `oneship.<max_rotations_count>.log`.
fn rotate_log_files(max_rotations_count: usize) {
    let logs_dir = get_logs_dir();
    let main_log = logs_dir.join("oneship.log");
    if max_rotations_count == 0 {
        let _ = fs::remove_file(&main_log);
        return;
    }
    let _ = fs::remove_file(logs_dir.join(format!("oneship.{}.log", max_rotations_count)));
    for i in (1..max_rotations_count).rev() {
        let from = logs_dir.join(format!("oneship.{}.log", i));
        if from.exists() {
            let _ = fs::rename(from, logs_dir.join(format!("oneship.{}.log", i + 1)));
        }
    }
    let _ = fs::rename(&main_log, logs_dir.join("oneship.1.log"));
}

fn open_log_file() -> Option<LogFileSink> {
    let main_log = get_logs_dir().join("oneship.log");
    let file = OpenOptions::new().create(true).append(true).open(&main_log).ok()?;
    let size_bytes = file.metadata().map(|m| m.len()).unwrap_or(0);
    Some(LogFileSink { file, size_bytes })
}

/// Appends one line to `oneship.log`, rotating first when the tracked size reached the limit.
fn write_file_line(line: &str, settings: &LoggerSettings) {
    let mut sink = LOG_FILE.lock().unwrap_or_else(PoisonError::into_inner);
    if sink.as_ref().is_some_and(|s| s.size_bytes >= settings.max_file_size_bytes) {
        *sink = None;
        rotate_log_files(settings.max_rotations_count);
    }
    if sink.is_none() {
        *sink = open_log_file();
    }
    if let Some(s) = sink.as_mut() {
        if s.file.write_all(line.as_bytes()).is_ok() {
            s.size_bytes += line.len() as u64;
        }
    }
}

fn format_line(level: &str, module: &str, origin: &str, message: &str) -> String {
    let timestamp_seconds = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("[{}] [{}] [{}] [{}] {}\n", timestamp_seconds, level, module, origin, message)
}

/// Records one entry in both sinks when `level` reaches `system.log_level`.
///
/// Unknown level names are always recorded (as given) so a malformed caller is never silenced.
pub fn log_with_origin(level: &str, module: &str, origin: &str, message: &str) {
    let settings = current_settings();
    if level_rank(level).is_some_and(|rank| rank < settings.min_level_rank) {
        return;
    }
    let level_upper = level.to_ascii_uppercase();
    write_file_line(&format_line(&level_upper, module, origin, message), &settings);
    if FILE_SINK_ONLY.load(Ordering::Relaxed) {
        return;
    }

    let queued = QueuedLog {
        level: level_upper,
        source: module.to_string(),
        origin: origin.to_string(),
        message: message.to_string(),
    };
    match get_log_sender().try_send(queued) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) => {
            DROPPED_DB_LOGS_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        // The worker thread is gone: say so in the file sink, which does not go through the channel.
        Err(TrySendError::Disconnected(_)) => {
            write_file_line(&format_line("ERROR", "logger", "system", "SQLite logger thread is gone; entry not stored in system_logs"), &settings);
        }
    }
}

/// Writes a structured log entry with origin `"system"` to the non-blocking logger channel.
pub fn log(level: &str, module: &str, message: &str) {
    log_with_origin(level, module, "system", message);
}

/// Records the failure of a best-effort side effect at `ERROR` instead of discarding the
/// `Result`. Use where the caller cannot meaningfully recover but the failure must stay
/// traceable (journal writes, callback prompts, cleanup).
pub fn log_failure<T, E: std::fmt::Display>(module: &str, operation: &str, result: Result<T, E>) {
    if let Err(e) = result {
        log("ERROR", module, &format!("{} failed: {}", operation, e));
    }
}

/// Daemon command `write_client_log`: the frontend's entry point into the logger. Rejects level
/// names outside DEBUG/INFO/WARN/ERROR.
pub fn write_client_log(level: String, module: String, message: String) -> Result<(), LoggerError> {
    if level_rank(&level).is_none() {
        log("WARN", "logger", &format!("write_client_log: rejected unknown level '{}' from module '{}'", level, module));
        return Err(LoggerError::UnknownLevel(level));
    }
    log(&level, &module, &message);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::level_rank;

    #[test]
    fn level_rank_orders_levels_and_rejects_unknown_names() {
        assert!(level_rank("debug") < level_rank("INFO"));
        assert!(level_rank("INFO") < level_rank("Warn"));
        assert!(level_rank("WARN") < level_rank("ERROR"));
        assert_eq!(level_rank("TRACE"), None);
    }
}
