use duckdb::{params, DuckdbConnectionManager};
use r2d2::Pool;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Failure of the telemetry store (`oneship_telemetry.duckdb`), owned by the daemon (Spec 25 §3.3).
#[derive(Debug, thiserror::Error)]
pub enum TelemetryError {
    #[error("DuckDB pool not initialized")]
    PoolUninitialized,
    #[error("DuckDB connection pool: {0}")]
    Pool(#[from] r2d2::Error),
    #[error("Failed to {operation}: {source}")]
    Query { operation: String, source: duckdb::Error },
    #[error("Configuration directory is unavailable")]
    ConfigDirUnavailable,
    #[error("{context}: {source}")]
    Io { context: String, source: std::io::Error },
}

impl crate::api::ApiError for TelemetryError {
    fn status(&self) -> axum::http::StatusCode {
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    }

    fn code(&self) -> &'static str {
        match self {
            Self::PoolUninitialized => "TelemetryError::PoolUninitialized",
            Self::Pool(_) => "TelemetryError::Pool",
            Self::Query { .. } => "TelemetryError::Query",
            Self::ConfigDirUnavailable => "TelemetryError::ConfigDirUnavailable",
            Self::Io { .. } => "TelemetryError::Io",
        }
    }
}

/// Attaches an operation name to DuckDB results and records the failure at `ERROR`
/// (the DuckDB counterpart of `db::DbResultExt`, AGENTS.md §9.5).
pub trait DuckResultExt<T> {
    fn duck_context(self, op: &str) -> Result<T, TelemetryError>;
}

impl<T> DuckResultExt<T> for duckdb::Result<T> {
    fn duck_context(self, op: &str) -> Result<T, TelemetryError> {
        self.map_err(|e| {
            crate::logger::log("ERROR", "duckdb", &format!("Failed to {}: {}", op, e));
            TelemetryError::Query { operation: op.to_string(), source: e }
        })
    }
}

static DUCK_POOL: OnceLock<Pool<DuckdbConnectionManager>> = OnceLock::new();

/// Raw samples are kept without pruning (Spec 25 §3.2). The last four columns were added on
/// 2026-11-06 and are NULL in older rows.
const PROCESS_SAMPLES_SCHEMA_SQL: &str = "CREATE TABLE IF NOT EXISTS process_samples (
        sampled_at_ms BIGINT,
        project_id BIGINT,
        target_type VARCHAR,
        target_id VARCHAR,
        run_id VARCHAR,
        pid UINTEGER,
        process_name VARCHAR,
        cmdline VARCHAR,
        cpu_usage_percent FLOAT,
        rss_memory_bytes UBIGINT,
        rss_memory_mbytes FLOAT,
        virtual_memory_bytes UBIGINT,
        threads_count UINTEGER,
        disk_read_bytes UBIGINT,
        disk_written_bytes UBIGINT
    );
    ALTER TABLE process_samples ADD COLUMN IF NOT EXISTS virtual_memory_bytes UBIGINT;
    ALTER TABLE process_samples ADD COLUMN IF NOT EXISTS threads_count UINTEGER;
    ALTER TABLE process_samples ADD COLUMN IF NOT EXISTS disk_read_bytes UBIGINT;
    ALTER TABLE process_samples ADD COLUMN IF NOT EXISTS disk_written_bytes UBIGINT;
    DROP INDEX IF EXISTS idx_samples_target_run;
    CREATE INDEX IF NOT EXISTS idx_samples_run ON process_samples (run_id);";

/// One process of a run's tree at one sampling tick (`process_samples` row).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProcessSample {
    pub sampled_at_ms: i64,
    pub project_id: i64,
    /// `service` or `task`.
    pub target_type: String,
    pub target_id: String,
    pub run_id: String,
    pub pid: u32,
    pub process_name: String,
    pub cmdline: String,
    pub cpu_usage_percent: f32,
    pub rss_memory_bytes: u64,
    pub rss_memory_mbytes: f64,
    pub virtual_memory_bytes: u64,
    /// Threads of the process (`None` where the platform does not report them).
    pub threads_count: Option<u32>,
    /// Bytes read from disk since the previous tick.
    pub disk_read_bytes: u64,
    /// Bytes written to disk since the previous tick.
    pub disk_written_bytes: u64,
}

/// One process name of a run, aggregated per tick across its instances.
#[derive(Serialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct RunProcessProfile {
    pub process_name: String,
    /// Ticks in which the process was present.
    #[ts(type = "number")]
    pub ticks_count: i64,
    /// Most instances of the process alive in one tick.
    #[ts(type = "number")]
    pub peak_instances_count: i64,
    pub avg_cpu_percent: f64,
    pub peak_cpu_percent: f64,
    pub avg_rss_memory_mbytes: f64,
    pub peak_rss_memory_mbytes: f64,
}

/// The continuous profile of one whole run. Every metric is computed over ticks, each tick being
/// the sum of the run's process tree at that instant.
#[derive(Serialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct RunProfile {
    pub run_id: String,
    #[ts(type = "number")]
    pub ticks_count: i64,
    #[ts(type = "number")]
    pub first_sample_at_ms: i64,
    #[ts(type = "number")]
    pub last_sample_at_ms: i64,
    pub sampled_seconds: f64,
    pub avg_cpu_percent: f64,
    pub p50_cpu_percent: f64,
    pub p95_cpu_percent: f64,
    pub peak_cpu_percent: f64,
    pub avg_rss_memory_mbytes: f64,
    pub peak_rss_memory_mbytes: f64,
    #[ts(type = "number")]
    pub peak_processes_count: i64,
    /// `None` for runs sampled before threads were recorded.
    #[ts(type = "number | null")]
    pub peak_threads_count: Option<i64>,
    /// `None` for runs sampled before disk I/O was recorded.
    pub disk_read_mbytes: Option<f64>,
    pub disk_written_mbytes: Option<f64>,
    /// Process names ordered by average CPU, at most `profiler.process_breakdown_max_items_count`.
    pub processes: Vec<RunProcessProfile>,
}

/// Default for `profiler.process_breakdown_max_items_count` (registered in `config.rs`).
pub const DEFAULT_PROCESS_BREAKDOWN_MAX_ITEMS_COUNT: u64 = 20;

const BYTES_PER_MBYTE: f64 = 1_048_576.0;

/// `~/.config/one-ship/oneship_telemetry.duckdb`, creating the directory if needed.
fn get_duckdb_path() -> Result<PathBuf, TelemetryError> {
    let app_dir = dirs::config_dir().ok_or(TelemetryError::ConfigDirUnavailable)?.join("one-ship");
    fs::create_dir_all(&app_dir).map_err(|source| TelemetryError::Io { context: format!("Cannot create {}", app_dir.display()), source })?;
    Ok(app_dir.join("oneship_telemetry.duckdb"))
}

/// Returns `true` if the DuckDB telemetry connection pool has been initialized.
pub fn is_duckdb_available() -> bool {
    DUCK_POOL.get().is_some()
}

/// Checks out a pooled DuckDB connection from the global telemetry pool.
pub fn get_duck_connection() -> Result<r2d2::PooledConnection<DuckdbConnectionManager>, TelemetryError> {
    let pool = DUCK_POOL.get().ok_or(TelemetryError::PoolUninitialized)?;
    Ok(pool.get()?)
}

/// Opens a pool on `manager` and creates or extends the `process_samples` schema.
fn open_pool(manager: DuckdbConnectionManager) -> Result<Pool<DuckdbConnectionManager>, TelemetryError> {
    let pool = Pool::builder().max_size(4).build(manager)?;
    let conn = pool.get()?;
    conn.execute_batch(PROCESS_SAMPLES_SCHEMA_SQL).duck_context("create the process_samples schema")?;
    Ok(pool)
}

/// Opens the telemetry database. Only the MCP daemon calls this: it is the sole owner of
/// `oneship_telemetry.duckdb` and of the continuous profiler (Spec 25 §3.3). A locked file means
/// another daemon is running, which is an error, never a reason to fall back to memory.
pub fn init_duckdb() -> Result<(), TelemetryError> {
    if DUCK_POOL.get().is_some() {
        return Ok(());
    }
    crate::logger::log("DEBUG", "duckdb", "init_duckdb: initializing DuckDB telemetry database");
    let db_path = get_duckdb_path()?;
    let manager = DuckdbConnectionManager::file(&db_path)
        .duck_context("open the telemetry database (locked by another daemon?)")?;
    let pool = open_pool(manager)?;
    // A concurrent initializer won the race; its pool is the one in use.
    let _ = DUCK_POOL.set(pool);
    crate::logger::log("INFO", "duckdb", &format!("DuckDB initialized at {:?}", db_path));
    Ok(())
}

/// In-memory telemetry database for unit tests only; production never runs without the file.
#[cfg(test)]
pub fn init_duckdb_memory() -> Result<(), TelemetryError> {
    if DUCK_POOL.get().is_some() {
        return Ok(());
    }
    let manager = DuckdbConnectionManager::memory().duck_context("open an in-memory DuckDB")?;
    let _ = DUCK_POOL.set(open_pool(manager)?);
    Ok(())
}

/// Appends a batch of `ProcessSample` rows into the columnar `process_samples` DuckDB table.
pub fn insert_process_samples(samples: &[ProcessSample]) -> Result<(), TelemetryError> {
    if samples.is_empty() {
        return Ok(());
    }
    crate::logger::log("DEBUG", "duckdb", &format!("insert_process_samples: appending {} samples", samples.len()));
    let conn = get_duck_connection()?;
    let mut appender = conn.appender("process_samples").duck_context("create the process_samples appender")?;
    for s in samples {
        appender.append_row(params![
            s.sampled_at_ms,
            s.project_id,
            &s.target_type,
            &s.target_id,
            &s.run_id,
            s.pid,
            &s.process_name,
            &s.cmdline,
            s.cpu_usage_percent,
            s.rss_memory_bytes,
            s.rss_memory_mbytes,
            s.virtual_memory_bytes,
            s.threads_count,
            s.disk_read_bytes,
            s.disk_written_bytes,
        ]).duck_context("append a process sample")?;
    }
    appender.flush().duck_context("flush the process_samples appender")?;
    crate::logger::log("DEBUG", "duckdb", &format!("insert_process_samples: flushed {} samples", samples.len()));
    Ok(())
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// Per-tick totals of a run: the run's whole process tree summed at each sampling instant.
const RUN_TICKS_CTE: &str = "WITH ticks AS (
        SELECT sampled_at_ms,
            SUM(cpu_usage_percent) AS cpu,
            CAST(SUM(rss_memory_bytes) AS DOUBLE) AS rss,
            CAST(SUM(threads_count) AS BIGINT) AS threads,
            COUNT(*) AS processes,
            CAST(SUM(disk_read_bytes) AS DOUBLE) AS disk_read,
            CAST(SUM(disk_written_bytes) AS DOUBLE) AS disk_written
        FROM process_samples
        WHERE run_id = ?
        GROUP BY sampled_at_ms
    )";

fn query_run_processes(conn: &duckdb::Connection, run_id: &str, max_items_count: u64) -> Result<Vec<RunProcessProfile>, TelemetryError> {
    let sql = "WITH per_tick AS (
            SELECT sampled_at_ms, process_name,
                SUM(cpu_usage_percent) AS cpu,
                CAST(SUM(rss_memory_bytes) AS DOUBLE) AS rss,
                COUNT(*) AS instances
            FROM process_samples
            WHERE run_id = ?
            GROUP BY sampled_at_ms, process_name
        )
        SELECT process_name, COUNT(*), MAX(instances), AVG(cpu), MAX(cpu), AVG(rss), MAX(rss)
        FROM per_tick
        GROUP BY process_name
        ORDER BY AVG(cpu) DESC, process_name
        LIMIT ?";
    let mut stmt = conn.prepare(sql).duck_context("prepare the run processes query")?;
    let mut rows = stmt.query(params![run_id, max_items_count]).duck_context("run the run processes query")?;
    let mut processes = Vec::with_capacity(max_items_count as usize);
    while let Some(row) = rows.next().duck_context("read a run process row")? {
        processes.push(RunProcessProfile {
            process_name: row.get(0).duck_context("read the process name")?,
            ticks_count: row.get(1).duck_context("read the process ticks")?,
            peak_instances_count: row.get(2).duck_context("read the process instances")?,
            avg_cpu_percent: round1(row.get(3).duck_context("read the process average CPU")?),
            peak_cpu_percent: round1(row.get(4).duck_context("read the process peak CPU")?),
            avg_rss_memory_mbytes: round1(row.get::<_, f64>(5).duck_context("read the process average RSS")? / BYTES_PER_MBYTE),
            peak_rss_memory_mbytes: round1(row.get::<_, f64>(6).duck_context("read the process peak RSS")? / BYTES_PER_MBYTE),
        });
    }
    Ok(processes)
}

/// The profile of one whole run from its samples; `None` while the run has none (a run shorter
/// than one sampling interval never gets any). Blocking.
pub fn get_run_profile(run_id: &str) -> Result<Option<RunProfile>, TelemetryError> {
    crate::logger::log("DEBUG", "duckdb", &format!("get_run_profile: run_id={}", run_id));
    let conn = get_duck_connection()?;
    let sql = format!(
        "{} SELECT COUNT(*), MIN(sampled_at_ms), MAX(sampled_at_ms),
            AVG(cpu), QUANTILE_CONT(cpu, 0.50), QUANTILE_CONT(cpu, 0.95), MAX(cpu),
            AVG(rss), MAX(rss), MAX(processes), MAX(threads), SUM(disk_read), SUM(disk_written)
        FROM ticks",
        RUN_TICKS_CTE
    );
    let mut stmt = conn.prepare(&sql).duck_context("prepare the run profile query")?;
    let mut rows = stmt.query(params![run_id]).duck_context("run the run profile query")?;
    let Some(row) = rows.next().duck_context("read the run profile row")? else {
        return Ok(None);
    };
    // Every other column is NULL when the run has no sample.
    let ticks_count: i64 = row.get(0).duck_context("read the tick count")?;
    if ticks_count == 0 {
        crate::logger::log("DEBUG", "duckdb", &format!("get_run_profile: no samples for run '{}'", run_id));
        return Ok(None);
    }
    let first_sample_at_ms: i64 = row.get(1).duck_context("read the first sample time")?;
    let last_sample_at_ms: i64 = row.get(2).duck_context("read the last sample time")?;
    debug_assert!(last_sample_at_ms >= first_sample_at_ms, "last sample precedes the first");
    let max_items_count = crate::config::resolve_effective_u64(
        None, None, "profiler", "process_breakdown_max_items_count",
    );
    let profile = RunProfile {
        run_id: run_id.to_string(),
        ticks_count,
        first_sample_at_ms,
        last_sample_at_ms,
        sampled_seconds: round1((last_sample_at_ms - first_sample_at_ms) as f64 / 1000.0),
        avg_cpu_percent: round1(row.get(3).duck_context("read the average CPU")?),
        p50_cpu_percent: round1(row.get(4).duck_context("read the p50 CPU")?),
        p95_cpu_percent: round1(row.get(5).duck_context("read the p95 CPU")?),
        peak_cpu_percent: round1(row.get(6).duck_context("read the peak CPU")?),
        avg_rss_memory_mbytes: round1(row.get::<_, f64>(7).duck_context("read the average RSS")? / BYTES_PER_MBYTE),
        peak_rss_memory_mbytes: round1(row.get::<_, f64>(8).duck_context("read the peak RSS")? / BYTES_PER_MBYTE),
        peak_processes_count: row.get(9).duck_context("read the peak process count")?,
        peak_threads_count: row.get(10).duck_context("read the peak thread count")?,
        disk_read_mbytes: row.get::<_, Option<f64>>(11).duck_context("read the disk reads")?.map(|b| round1(b / BYTES_PER_MBYTE)),
        disk_written_mbytes: row.get::<_, Option<f64>>(12).duck_context("read the disk writes")?.map(|b| round1(b / BYTES_PER_MBYTE)),
        processes: query_run_processes(&conn, run_id, max_items_count)?,
    };
    crate::logger::log("DEBUG", "duckdb", &format!("get_run_profile: run '{}' has {} ticks", run_id, ticks_count));
    Ok(Some(profile))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(run_id: &str, sampled_at_ms: i64, pid: u32, process_name: &str, cpu: f32, rss_mbytes: u64) -> ProcessSample {
        ProcessSample {
            sampled_at_ms,
            project_id: 1,
            target_type: "service".to_string(),
            target_id: "api".to_string(),
            run_id: run_id.to_string(),
            pid,
            process_name: process_name.to_string(),
            cmdline: format!("{} run", process_name),
            cpu_usage_percent: cpu,
            rss_memory_bytes: rss_mbytes * 1_048_576,
            rss_memory_mbytes: rss_mbytes as f64,
            virtual_memory_bytes: rss_mbytes * 4 * 1_048_576,
            threads_count: Some(4),
            disk_read_bytes: 1_048_576,
            disk_written_bytes: 0,
        }
    }

    #[test]
    fn run_profile_sums_the_process_tree_per_tick() {
        init_duckdb_memory().expect("in-memory DuckDB");
        let run = "svc_api_profile_test";
        insert_process_samples(&[
            // Tick 1: two processes (30% + 10%, 100 + 50 MB).
            sample(run, 1_000, 101, "node", 30.0, 100),
            sample(run, 1_000, 102, "esbuild", 10.0, 50),
            // Tick 2: one process.
            sample(run, 3_000, 101, "node", 50.0, 200),
            sample("svc_other_run", 3_000, 900, "node", 99.0, 999),
        ]).expect("insert");

        let p = get_run_profile(run).expect("profile").expect("samples present");
        assert_eq!(p.ticks_count, 2);
        assert_eq!(p.sampled_seconds, 2.0);
        assert_eq!(p.avg_cpu_percent, 45.0);
        assert_eq!(p.peak_cpu_percent, 50.0);
        assert_eq!(p.peak_rss_memory_mbytes, 200.0);
        assert_eq!(p.avg_rss_memory_mbytes, 175.0);
        assert_eq!(p.peak_processes_count, 2);
        assert_eq!(p.peak_threads_count, Some(8));
        assert_eq!(p.disk_read_mbytes, Some(3.0));
        assert_eq!(p.processes.first().map(|h| h.process_name.as_str()), Some("node"));
        assert_eq!(p.processes.first().map(|h| h.ticks_count), Some(2));

        assert!(get_run_profile("no_such_run").expect("profile").is_none());
    }
}
