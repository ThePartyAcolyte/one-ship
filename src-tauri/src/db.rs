use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use std::sync::OnceLock;
use thiserror::Error;

/// Failures of the SQLite persistence layer (`oneship.db`).
///
/// Every `db_*` function returns this type. Tauri commands convert it to `String` at the IPC
/// boundary through the `From` impl below; other modules propagate it with `?` until their own
/// error types land (AGENTS.md §9.3).
#[derive(Debug, Error)]
pub enum DatabaseError {
    /// `init_database` has not run in this process.
    #[error("Database pool not initialized")]
    PoolUninitialized,

    /// `init_database` ran twice in the same process.
    #[error("Database pool already initialized")]
    PoolAlreadyInitialized,

    /// The r2d2 pool could not be built or could not hand out a connection.
    #[error("Database pool error: {0}")]
    Pool(#[from] r2d2::Error),

    /// A statement failed; `operation` names what was being done.
    #[error("Failed to {operation}: {source}")]
    Query {
        operation: String,
        #[source]
        source: rusqlite::Error,
    },

    /// A JSON column could not be encoded or decoded.
    #[error("Failed to {operation}: {source}")]
    Serialization {
        operation: String,
        #[source]
        source: serde_json::Error,
    },

    /// A row that the caller requires does not exist.
    #[error("{entity} '{id}' not found")]
    NotFound { entity: &'static str, id: String },

    /// The schema or stored data violates an invariant (e.g. a database newer than this build).
    #[error("Invalid database state: {0}")]
    InvalidState(String),

    /// The caller's arguments are rejected (empty search, unknown status, cross-agent access).
    /// The message is shown verbatim to the UI and to agents.
    #[error("{0}")]
    InvalidRequest(String),
}

impl crate::api::ApiError for DatabaseError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::NotFound { .. } => StatusCode::NOT_FOUND,
            Self::InvalidRequest(_) => StatusCode::BAD_REQUEST,
            Self::PoolUninitialized
            | Self::PoolAlreadyInitialized
            | Self::Pool(_)
            | Self::Query { .. }
            | Self::Serialization { .. }
            | Self::InvalidState(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::PoolUninitialized => "DatabaseError::PoolUninitialized",
            Self::PoolAlreadyInitialized => "DatabaseError::PoolAlreadyInitialized",
            Self::Pool(_) => "DatabaseError::Pool",
            Self::Query { .. } => "DatabaseError::Query",
            Self::Serialization { .. } => "DatabaseError::Serialization",
            Self::NotFound { .. } => "DatabaseError::NotFound",
            Self::InvalidState(_) => "DatabaseError::InvalidState",
            Self::InvalidRequest(_) => "DatabaseError::InvalidRequest",
        }
    }
}

/// Attaches an operation name to rusqlite results and records the failure at `ERROR`, so call
/// sites need neither `map_err` boilerplate nor their own error log (AGENTS.md §9.5, §12).
pub trait DbResultExt<T> {
    fn db_context(self, op: &str) -> Result<T, DatabaseError>;
    /// Same as [`DbResultExt::db_context`] without the `ERROR` log. Only for the logger's own
    /// sink (`db_insert_logs_batch`, `db_prune_system_logs`), where logging would feed back
    /// into the failing insert path.
    fn db_context_unlogged(self, op: &str) -> Result<T, DatabaseError>;
}

impl<T> DbResultExt<T> for rusqlite::Result<T> {
    fn db_context(self, op: &str) -> Result<T, DatabaseError> {
        self.map_err(|e| {
            crate::logger::log("ERROR", "db", &format!("Failed to {}: {}", op, e));
            DatabaseError::Query { operation: op.to_string(), source: e }
        })
    }

    fn db_context_unlogged(self, op: &str) -> Result<T, DatabaseError> {
        self.map_err(|e| DatabaseError::Query { operation: op.to_string(), source: e })
    }
}

/// Collects mapped rows, failing on the first row error instead of silently skipping it.
fn collect_rows<T>(rows: impl Iterator<Item = rusqlite::Result<T>>, op: &str) -> Result<Vec<T>, DatabaseError> {
    rows.collect::<rusqlite::Result<Vec<T>>>().db_context(op)
}

static DB_POOL: OnceLock<Pool<SqliteConnectionManager>> = OnceLock::new();

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProjectRecord {
    pub id: usize,
    pub path: String,
    pub name: String,
    pub description: Option<String>,
    pub is_favorite: bool,
    pub is_archived: bool,
    pub hidden_tabs: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct TaskExecutionRecord {
    #[ts(type = "number")]
    pub id: i64,
    #[ts(type = "number")]
    pub project_id: i64,
    pub task_id: String,
    pub task_type: String, // 'persistent', 'task_cli', 'agent_task'
    pub command: String,
    pub started_at: String,
    #[ts(type = "number | null")]
    pub duration_ms: Option<i64>,
    pub exit_code: Option<i32>,
    pub status: String,    // 'running', 'completed', 'failed'
    pub summary: Option<String>,
    pub details_markdown: Option<String>,
    pub stdout_log: Option<String>,
}

#[cfg(not(test))]
fn get_db_path() -> PathBuf {
    let config_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    let app_dir = config_dir.join("one-ship");
    if !app_dir.exists() {
        let _ = fs::create_dir_all(&app_dir);
    }
    app_dir.join("oneship.db")
}

/// Test builds never touch `~/.config/one-ship/oneship.db`: every test process gets its own
/// throwaway database under the system temp directory (audit F17).
#[cfg(test)]
fn get_db_path() -> PathBuf {
    crate::config::test_process_dir().join("oneship.db")
}

/// Hands out a pooled connection; pool exhaustion is logged at `ERROR`.
pub fn get_connection() -> Result<r2d2::PooledConnection<SqliteConnectionManager>, DatabaseError> {
    get_connection_unlogged().inspect_err(|e| crate::logger::log("ERROR", "db", &format!("get_connection: {}", e)))
}

/// [`get_connection`] without the failure log, for the logger's own sink.
fn get_connection_unlogged() -> Result<r2d2::PooledConnection<SqliteConnectionManager>, DatabaseError> {
    let pool = DB_POOL.get().ok_or(DatabaseError::PoolUninitialized)?;
    Ok(pool.get()?)
}

/// Version of the conversation tables defined by Spec 30, stored in `PRAGMA user_version`.
const CONVERSATION_SCHEMA_VERSION: i64 = 1;

/// Resets the conversation tables when the database predates [`CONVERSATION_SCHEMA_VERSION`].
///
/// Existing conversation data is not migrated (Spec 30, decision D7): the whole database is first
/// copied to `oneship.pre-spec30-<epoch>.db` with `VACUUM INTO`, then the conversation tables are
/// dropped so the caller recreates them with the new layout. A fresh database only gets its
/// version stamped. Idempotent: later calls see the stamped version and do nothing.
fn reset_conversation_schema_if_outdated(conn: &rusqlite::Connection, db_path: &std::path::Path) -> Result<(), DatabaseError> {
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .db_context("read database user_version")?;
    if version > CONVERSATION_SCHEMA_VERSION {
        // A newer build already migrated this database; running older code against it silently
        // corrupts or drops data (observed 2026-10-09: a stale daemon queried dropped columns).
        return Err(DatabaseError::InvalidState(format!(
            "database schema version {} is newer than this build supports ({}); rebuild and restart this binary",
            version, CONVERSATION_SCHEMA_VERSION
        )));
    }
    if version == CONVERSATION_SCHEMA_VERSION {
        return Ok(());
    }

    let had_conversations = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'agent_conversations'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .db_context("probe for legacy conversation table")?
        > 0;

    if had_conversations {
        let backup_path = db_path.with_file_name(format!("oneship.pre-spec30-{}.db", now_epoch_secs()));
        let escaped = backup_path.to_string_lossy().replace('\'', "''");
        conn.execute_batch(&format!("VACUUM INTO '{}'", escaped))
            .db_context("back up database before conversation reset")?;
        crate::logger::log("INFO", "db", &format!("reset_conversation_schema: database backed up to {:?}", backup_path));
        // Children first: foreign keys are enforced.
        for table in ["conversation_search", "agent_session_metrics", "agent_chat_turns", "conversation_native_sessions", "agent_conversations"] {
            conn.execute_batch(&format!("DROP TABLE IF EXISTS {}", table))
                .db_context(&format!("drop legacy table {}", table))?;
        }
        crate::logger::log("INFO", "db", "reset_conversation_schema: legacy conversation tables dropped");
    }

    conn.execute_batch(&format!("PRAGMA user_version = {}", CONVERSATION_SCHEMA_VERSION))
        .db_context("stamp database user_version")?;
    Ok(())
}

/// Column list of `agent_session_journal` (Minutes), shared by creation and the Phase D rebuild.
const JOURNAL_COLUMNS_DDL: &str = "
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            agent_id TEXT NOT NULL,
            conversation_id TEXT REFERENCES agent_conversations(id) ON DELETE SET NULL,
            agy_conversation_id TEXT,
            title TEXT NOT NULL,
            summary TEXT NOT NULL,
            decisions TEXT,
            files_touched TEXT,
            technical_debt TEXT,
            tags TEXT,
            context_pct REAL,
            total_tokens INTEGER,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            embedding BLOB";

/// True when `table` has a column named `column`.
fn column_exists(conn: &rusqlite::Connection, table: &str, column: &str) -> Result<bool, DatabaseError> {
    conn.query_row(
        "SELECT count(*) FROM pragma_table_info(?1) WHERE name = ?2",
        params![table, column],
        |row| row.get::<_, i64>(0),
    )
    .map(|n| n > 0)
    .db_context(&format!("probe column {}.{}", table, column))
}

/// Adds `column` to `table` when absent (idempotent additive schema step).
fn ensure_column(conn: &rusqlite::Connection, table: &str, column: &str, decl: &str) -> Result<(), DatabaseError> {
    if column_exists(conn, table, column)? {
        return Ok(());
    }
    conn.execute_batch(&format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, decl))
        .db_context(&format!("add column {}.{}", table, column))?;
    crate::logger::log("INFO", "db", &format!("ensure_column: added {}.{}", table, column));
    Ok(())
}

/// Rebuilds `agent_session_journal` once so Minutes can be keyed by Conversation (Spec 30 Phase D).
///
/// Pre-Phase-D rows are kept (21 devlog entries at the time of writing) with a NULL
/// `conversation_id`; `agy_conversation_id` becomes nullable. Runs in one transaction and is a
/// no-op when the column already exists.
fn migrate_journal_to_conversation_key(conn: &mut rusqlite::Connection) -> Result<(), DatabaseError> {
    if column_exists(conn, "agent_session_journal", "conversation_id")? {
        return Ok(());
    }
    let tx = conn.transaction().db_context("begin journal rebuild")?;
    tx.execute_batch(&format!(
        "CREATE TABLE agent_session_journal_v2 ({});
         INSERT INTO agent_session_journal_v2
            (id, project_id, agent_id, agy_conversation_id, title, summary, decisions, files_touched,
             technical_debt, tags, context_pct, total_tokens, created_at, updated_at, embedding)
         SELECT id, project_id, agent_id, agy_conversation_id, title, summary, decisions, files_touched,
             technical_debt, tags, context_pct, total_tokens, created_at, updated_at, embedding
         FROM agent_session_journal;
         DROP TABLE agent_session_journal;
         ALTER TABLE agent_session_journal_v2 RENAME TO agent_session_journal;",
        JOURNAL_COLUMNS_DDL
    ))
    .db_context("rebuild agent_session_journal")?;
    tx.commit().db_context("commit journal rebuild")?;
    crate::logger::log("INFO", "db", "migrate_journal_to_conversation_key: journal rebuilt with conversation_id");
    Ok(())
}

pub fn init_database() -> Result<(), DatabaseError> {
    let db_path = get_db_path();
    crate::logger::log("DEBUG", "db", &format!("init_database: initializing SQLite database at {:?}", db_path));
    let manager = SqliteConnectionManager::file(&db_path).with_init(|c| {
        c.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;"
        )
    });
    
    let max_pool_size = crate::config::resolve_effective_usize(None, None, "telemetry", "db_connection_pool_max_count");
    let pool = Pool::builder()
        .max_size(max_pool_size as u32)
        .build(manager)?;

    DB_POOL.set(pool).map_err(|_| DatabaseError::PoolAlreadyInitialized)?;

    let mut conn = get_connection()?;

    // 1. Projects table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS projects (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            path TEXT UNIQUE NOT NULL,
            name TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );",
        [],
    ).db_context("create projects table")?;
    ensure_column(&conn, "projects", "is_favorite", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_column(&conn, "projects", "is_archived", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_column(&conn, "projects", "hidden_tabs", "TEXT NOT NULL DEFAULT '[]'")?;
    ensure_column(&conn, "projects", "description", "TEXT")?;
    ensure_column(&conn, "projects", "overview_layout_json", "TEXT")?;

    // 2. System logs table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS system_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
            level TEXT NOT NULL,
            source TEXT NOT NULL,
            origin TEXT NOT NULL,
            message TEXT NOT NULL
        );",
        [],
    ).db_context("create system_logs table")?;

    // 3. Task executions table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS task_executions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL,
            task_id TEXT NOT NULL,
            task_type TEXT NOT NULL,
            command TEXT NOT NULL,
            started_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            duration_ms INTEGER,
            exit_code INTEGER,
            status TEXT NOT NULL,
            summary TEXT,
            details_markdown TEXT,
            stdout_log TEXT,
            FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
        );",
        [],
    ).db_context("create task_executions table")?;

    // 4. Agent delegations table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS agent_delegations (
            task_id TEXT PRIMARY KEY,
            originator_agent TEXT NOT NULL,
            target_agent TEXT NOT NULL,
            instruction TEXT NOT NULL,
            status TEXT NOT NULL,
            dispatched_at INTEGER NOT NULL,
            completed_at INTEGER,
            result_summary TEXT,
            originator_project TEXT,
            target_project TEXT
        );",
        [],
    ).db_context("create agent_delegations table")?;
    ensure_column(&conn, "agent_delegations", "originator_project", "TEXT")?;
    ensure_column(&conn, "agent_delegations", "target_project", "TEXT")?;
    ensure_column(&conn, "agent_delegations", "conversation_id", "TEXT")?;
    ensure_column(&conn, "agent_delegations", "originator_agent_uid", "TEXT")?;
    ensure_column(&conn, "agent_delegations", "target_agent_uid", "TEXT")?;

    // 4. Project Initializations Table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS project_initializations (
            project_path TEXT PRIMARY KEY,
            status TEXT NOT NULL,
            mode TEXT NOT NULL,
            current_phase TEXT,
            last_message TEXT,
            git_checkpoint TEXT,
            agy_conversation_id TEXT,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );",
        [],
    ).db_context("create project_initializations table")?;
    ensure_column(&conn, "project_initializations", "agy_conversation_id", "TEXT")?;

    // 5. Audit Journal Table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS audit_journal (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL,
            timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
            originator TEXT NOT NULL,
            target TEXT NOT NULL,
            action_type TEXT NOT NULL,
            payload TEXT NOT NULL,
            metadata TEXT,
            FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
        );",
        [],
    ).db_context("create audit_journal table")?;

    // 6. Snapshots table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS snapshots (
            id TEXT PRIMARY KEY,
            ref_path TEXT NOT NULL UNIQUE,
            project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            conversation_id TEXT NOT NULL,
            agent_id TEXT NOT NULL,
            label TEXT NOT NULL,
            files_changed TEXT,
            created_at TEXT NOT NULL
        );",
        [],
    ).db_context("create snapshots table")?;
    ensure_column(&conn, "snapshots", "domain", "TEXT NOT NULL DEFAULT 'code'")?;
    ensure_column(&conn, "snapshots", "asset_key", "TEXT")?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_snapshots_domain_asset ON snapshots(project_id, domain, asset_key, created_at)",
        [],
    ).db_context("create snapshots domain index")?;

    // 7. Accounts table (Multi-Account Management — preserved across DB resets)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS accounts (
            id TEXT PRIMARY KEY,
            label TEXT NOT NULL DEFAULT '',
            email TEXT NOT NULL UNIQUE,
            is_active INTEGER NOT NULL DEFAULT 0,
            enc_token BLOB NOT NULL,
            added_at TEXT NOT NULL,
            provider TEXT NOT NULL DEFAULT 'google'
        );",
        [],
    ).db_context("create accounts table")?;
    ensure_column(&conn, "accounts", "provider", "TEXT NOT NULL DEFAULT 'google'")?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_accounts_provider ON accounts(provider)", [])
        .db_context("create accounts provider index")?;


    // 9. Pending Approvals Table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS pending_approvals (
            task_id TEXT PRIMARY KEY,
            project_path TEXT NOT NULL,
            phase TEXT NOT NULL,
            payload_markdown TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            status TEXT DEFAULT 'pending'
        );",
        [],
    ).db_context("create pending_approvals table")?;

    // 10. Agent Conversations (Spec 30). Schema version 1 replaced the pre-Spec-30 layout; older
    // databases get their conversation data backed up and reset (no migration, decision D7).
    reset_conversation_schema_if_outdated(&conn, &db_path)?;

    // runner/model/account_id/effort are overrides: NULL inherits from the persona (Spec 30 §7).
    conn.execute(
        "CREATE TABLE IF NOT EXISTS agent_conversations (
            id TEXT PRIMARY KEY,
            project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            agent_id TEXT NOT NULL,
            topic TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'archived')),
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            artifacts_touched TEXT,
            runner TEXT,
            model TEXT,
            account_id TEXT,
            effort TEXT,
            closed_at_ms INTEGER,
            close_reason TEXT CHECK (close_reason IS NULL OR close_reason IN ('resolved', 'cleaned', 'superseded')),
            summary TEXT,
            reopened_from TEXT
        );",
        [],
    ).db_context("create agent_conversations table")?;
    ensure_column(&conn, "agent_conversations", "summary", "TEXT")?;
    ensure_column(&conn, "agent_conversations", "reopened_from", "TEXT")?;
    ensure_column(&conn, "agent_conversations", "agent_uid", "TEXT")?;

    // Full-text index over topic, summary and Minutes (Spec 30 §8); one row per Conversation.
    conn.execute(
        "CREATE VIRTUAL TABLE IF NOT EXISTS conversation_search USING fts5(
            conversation_id UNINDEXED, project_id UNINDEXED, agent_id UNINDEXED, content
        );",
        [],
    ).db_context("create conversation_search table")?;

    // Native Sessions: runner-level contexts owned by a Conversation (Spec 30 §3).
    conn.execute(
        "CREATE TABLE IF NOT EXISTS conversation_native_sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            conversation_id TEXT NOT NULL REFERENCES agent_conversations(id) ON DELETE CASCADE,
            runner TEXT NOT NULL,
            model TEXT,
            account_id TEXT,
            native_session_id TEXT,
            started_at_ms INTEGER NOT NULL,
            ended_at_ms INTEGER,
            end_reason TEXT CHECK (end_reason IS NULL OR end_reason IN ('exit', 'runner_switch', 'invalidated', 'cleaned')),
            seeded_from_brief INTEGER NOT NULL DEFAULT 0
        );",
        [],
    ).db_context("create conversation_native_sessions table")?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_native_sessions_conv ON conversation_native_sessions(conversation_id, id DESC);",
        [],
    ).db_context("create native sessions index")?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS agent_personas (
            agent_uid TEXT PRIMARY KEY,
            project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            display_name TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            UNIQUE (project_id, display_name)
        );",
        [],
    ).db_context("create agent_personas table")?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS agent_instances (
            instance_id TEXT PRIMARY KEY,
            scope TEXT NOT NULL CHECK (scope IN ('project', 'system')),
            project_id INTEGER REFERENCES projects(id) ON DELETE CASCADE,
            agent_uid TEXT REFERENCES agent_personas(agent_uid) ON DELETE CASCADE,
            runner TEXT NOT NULL,
            pid INTEGER NOT NULL,
            spawned_at_ms INTEGER NOT NULL,
            terminated_at_ms INTEGER,
            CHECK (
                (scope = 'project' AND project_id IS NOT NULL AND agent_uid IS NOT NULL)
                OR (scope = 'system' AND project_id IS NULL AND agent_uid IS NULL)
            )
        );",
        [],
    ).db_context("create agent_instances table")?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_agent_instances_pid ON agent_instances(pid);",
        [],
    ).db_context("create agent_instances pid index")?;

    // 11. Complete Turn Audit Trail
    conn.execute(
        "CREATE TABLE IF NOT EXISTS agent_chat_turns (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            conversation_id TEXT NOT NULL REFERENCES agent_conversations(id) ON DELETE CASCADE,
            project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            agent_id TEXT NOT NULL,
            agy_conversation_id TEXT,
            role TEXT NOT NULL CHECK (role IN ('user', 'agent', 'system')),
            dialogue TEXT NOT NULL,
            artifact_type TEXT,
            artifact_lang TEXT,
            artifact_label TEXT,
            artifact_file_path TEXT,
            artifact_content TEXT,
            is_ephemeral INTEGER NOT NULL DEFAULT 0,
            terminal_snapshot TEXT,
            timestamp INTEGER NOT NULL,
            approval_payload TEXT,
            native_session_row_id INTEGER REFERENCES conversation_native_sessions(id) ON DELETE SET NULL
        );",
        [],
    ).db_context("create agent_chat_turns table")?;

    // 12. Canonical Devlog Journal (Minutes). Keyed by the ONE-SHIP Conversation (Spec 30 §3);
    // `agy_conversation_id` is only a native reference. `conversation_id` is NULL solely for rows
    // recorded before Phase D.
    conn.execute(&format!("CREATE TABLE IF NOT EXISTS agent_session_journal ({})", JOURNAL_COLUMNS_DDL), [])
        .db_context("create agent_session_journal table")?;
    migrate_journal_to_conversation_key(&mut conn)?;
    backfill_conversation_search(&conn)?;

    // 13. Relational Project Tasks
    conn.execute(
        "CREATE TABLE IF NOT EXISTS project_todos (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            agent_id TEXT,
            agy_conversation_id TEXT,
            title TEXT NOT NULL,
            description TEXT,
            category TEXT NOT NULL DEFAULT 'feature',
            status TEXT NOT NULL DEFAULT 'pending',
            priority INTEGER NOT NULL DEFAULT 2,
            created_at INTEGER NOT NULL,
            completed_at INTEGER
        );",
        [],
    ).db_context("create project_todos table")?;
    ensure_column(&conn, "project_todos", "conversation_id", "TEXT")?;

    // 14. Relational Technical Debt Ledger
    conn.execute(
        "CREATE TABLE IF NOT EXISTS project_technical_debt (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            agy_conversation_id TEXT,
            title TEXT NOT NULL,
            category TEXT NOT NULL DEFAULT 'architectural',
            impact_summary TEXT NOT NULL,
            remediation_plan TEXT,
            status TEXT NOT NULL DEFAULT 'open',
            severity TEXT NOT NULL DEFAULT 'medium',
            identified_at INTEGER NOT NULL,
            resolved_at INTEGER
        );",
        [],
    ).db_context("create project_technical_debt table")?;
    ensure_column(&conn, "project_technical_debt", "conversation_id", "TEXT")?;

    // 15. Project Initialization Flags
    conn.execute(
        "CREATE TABLE IF NOT EXISTS project_initialization_flags (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            flag_type TEXT NOT NULL,
            title TEXT NOT NULL,
            description TEXT,
            recommended_option TEXT,
            options_json TEXT,
            metadata_json TEXT,
            status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending', 'resolved', 'dismissed')),
            created_at INTEGER NOT NULL,
            resolved_at INTEGER
        );",
        [],
    ).db_context("create project_initialization_flags table")?;
    ensure_column(&conn, "project_initialization_flags", "raised_by_agent_uid", "TEXT")?;
    ensure_column(&conn, "project_initialization_flags", "resolved_by_agent_uid", "TEXT")?;

    // 15a. Hybrid Knowledge Graph Edges (Spec 22 §5.7): causal & transversal relations.
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS knowledge_edges (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            source_id TEXT NOT NULL,
            source_entity_type TEXT NOT NULL,
            source_title TEXT NOT NULL,
            source_file_path TEXT,
            source_line_start INTEGER,
            target_id TEXT NOT NULL,
            target_entity_type TEXT NOT NULL,
            target_title TEXT NOT NULL,
            target_file_path TEXT,
            target_line_start INTEGER,
            edge_type TEXT NOT NULL,
            origin TEXT NOT NULL DEFAULT 'explicit_ref',
            weight REAL NOT NULL DEFAULT 1.0,
            updated_at_epoch_seconds INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_knowledge_edges_source ON knowledge_edges(project_id, source_id);
        CREATE INDEX IF NOT EXISTS idx_knowledge_edges_target ON knowledge_edges(project_id, target_id);",
    )
    .db_context("create knowledge_edges table and indexes")?;

    let needs_causal_knowledge_reset: bool = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'journal_file_hunks'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .db_context("probe journal_file_hunks table presence")?
        == 0;

    if needs_causal_knowledge_reset {
        conn.execute("DELETE FROM knowledge_edges", [])
            .db_context("reset legacy knowledge_edges for causal v2")?;
        crate::logger::log(
            "INFO",
            "db",
            "init_database: reset legacy knowledge_edges for clean causal knowledge v2 reindexation",
        );
    }

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS journal_file_hunks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            journal_id INTEGER NOT NULL REFERENCES agent_session_journal(id) ON DELETE CASCADE,
            file_path TEXT NOT NULL,
            line_start INTEGER NOT NULL DEFAULT 0,
            line_end INTEGER NOT NULL DEFAULT 0,
            change_kind TEXT NOT NULL DEFAULT 'modified',
            source_kind TEXT NOT NULL DEFAULT 'git_commit',
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_journal_file_hunks_lookup
            ON journal_file_hunks(project_id, file_path, line_start, line_end);
        CREATE INDEX IF NOT EXISTS idx_journal_file_hunks_journal
            ON journal_file_hunks(journal_id);

        CREATE TABLE IF NOT EXISTS project_code_symbols (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            file_path TEXT NOT NULL,
            file_sha256 TEXT NOT NULL,
            symbol_name TEXT NOT NULL,
            short_name TEXT NOT NULL,
            title TEXT NOT NULL,
            line_start INTEGER NOT NULL,
            line_end INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_project_code_symbols_proj_file
            ON project_code_symbols(project_id, file_path);
        CREATE INDEX IF NOT EXISTS idx_project_code_symbols_proj_short
            ON project_code_symbols(project_id, short_name);",
    )
    .db_context("create journal_file_hunks and project_code_symbols tables")?;

    // 15b. Project Management Entities (Spec 34): decoupled from Journal (`project_todos`).
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS project_milestones (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            title TEXT NOT NULL,
            description TEXT,
            status TEXT NOT NULL DEFAULT 'planned',
            weight_percent REAL NOT NULL DEFAULT 0.0,
            hill_position_percent INTEGER NOT NULL DEFAULT 0,
            appetite_days INTEGER,
            planned_start_epoch_seconds INTEGER,
            planned_due_epoch_seconds INTEGER,
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at_seconds INTEGER NOT NULL,
            updated_at_seconds INTEGER NOT NULL,
            completed_at_seconds INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_project_milestones_proj ON project_milestones(project_id, sort_order, id);
        CREATE TABLE IF NOT EXISTS project_task_stages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            slug TEXT NOT NULL,
            label TEXT NOT NULL,
            sort_order INTEGER NOT NULL DEFAULT 0,
            is_collapsed INTEGER NOT NULL DEFAULT 0,
            is_terminal INTEGER NOT NULL DEFAULT 0,
            created_at_seconds INTEGER NOT NULL,
            updated_at_seconds INTEGER NOT NULL,
            UNIQUE(project_id, slug)
        );
        CREATE TABLE IF NOT EXISTS project_tasks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            title TEXT NOT NULL,
            description TEXT,
            domain TEXT NOT NULL DEFAULT 'code',
            status TEXT NOT NULL DEFAULT 'backlog',
            priority INTEGER NOT NULL DEFAULT 2,
            start_epoch_seconds INTEGER,
            due_epoch_seconds INTEGER,
            github_repo TEXT,
            github_issue_number INTEGER,
            github_pr_number INTEGER,
            branch_name TEXT,
            created_at_seconds INTEGER NOT NULL,
            updated_at_seconds INTEGER NOT NULL,
            completed_at_seconds INTEGER
        );
        CREATE TABLE IF NOT EXISTS project_task_subtasks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id INTEGER NOT NULL REFERENCES project_tasks(id) ON DELETE CASCADE,
            title TEXT NOT NULL,
            is_completed INTEGER NOT NULL DEFAULT 0,
            sort_order INTEGER NOT NULL DEFAULT 0,
            linked_todo_id INTEGER REFERENCES project_todos(id) ON DELETE SET NULL,
            created_at_seconds INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS project_task_links (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id INTEGER NOT NULL REFERENCES project_tasks(id) ON DELETE CASCADE,
            link_type TEXT NOT NULL CHECK(link_type IN ('todo', 'journal')),
            target_id INTEGER NOT NULL,
            created_at_seconds INTEGER NOT NULL,
            UNIQUE(task_id, link_type, target_id)
        );"
    ).db_context("create project management tables")?;
    ensure_column(&conn, "project_tasks", "milestone_id", "INTEGER REFERENCES project_milestones(id) ON DELETE SET NULL")?;
    ensure_column(&conn, "project_tasks", "weight_points", "INTEGER NOT NULL DEFAULT 1")?;
    ensure_column(&conn, "agent_session_journal", "milestone_id", "INTEGER REFERENCES project_milestones(id) ON DELETE SET NULL")?;
    ensure_column(&conn, "agent_session_journal", "runner", "TEXT")?;
    ensure_column(&conn, "agent_session_journal", "model", "TEXT")?;
    ensure_column(&conn, "agent_session_journal", "cost_dollars", "REAL")?;
    ensure_column(&conn, "agent_session_journal", "commit_hash", "TEXT")?;
    ensure_column(&conn, "agent_session_journal", "snapshot_ref", "TEXT")?;
    ensure_column(&conn, "project_todos", "file_path", "TEXT")?;
    ensure_column(&conn, "project_todos", "line_start", "INTEGER")?;
    ensure_column(&conn, "project_todos", "line_end", "INTEGER")?;
    ensure_column(&conn, "project_todos", "journal_id", "INTEGER REFERENCES agent_session_journal(id) ON DELETE SET NULL")?;
    ensure_column(&conn, "project_technical_debt", "milestone_id", "INTEGER REFERENCES project_milestones(id) ON DELETE SET NULL")?;
    ensure_column(&conn, "project_technical_debt", "file_path", "TEXT")?;
    ensure_column(&conn, "project_technical_debt", "line_start", "INTEGER")?;
    ensure_column(&conn, "project_technical_debt", "line_end", "INTEGER")?;
    ensure_column(&conn, "project_technical_debt", "journal_id", "INTEGER REFERENCES agent_session_journal(id) ON DELETE SET NULL")?;
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_project_tasks_milestone ON project_tasks(project_id, milestone_id);
         CREATE INDEX IF NOT EXISTS idx_session_journal_milestone ON agent_session_journal(project_id, milestone_id);
         CREATE INDEX IF NOT EXISTS idx_tech_debt_milestone ON project_technical_debt(project_id, milestone_id);"
    ).db_context("create milestone attribution indexes")?;

    // 16. The profiler session tables, which nothing writes since runs are `supervised_processes`
    // rows (Spec 25 §3.4, 2026-11-06).
    conn.execute_batch(
        "DROP TABLE IF EXISTS profiler_timeline_buckets;
         DROP TABLE IF EXISTS profiler_hotspots;
         DROP TABLE IF EXISTS profiler_runs;",
    ).db_context("drop the profiler session tables")?;

    // 17. Agent Session Metrics
    conn.execute(
        "CREATE TABLE IF NOT EXISTS agent_session_metrics (
            agy_conversation_id TEXT PRIMARY KEY,
            conversation_id TEXT NOT NULL REFERENCES agent_conversations(id) ON DELETE CASCADE,
            project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            agent_id TEXT NOT NULL,
            provider TEXT NOT NULL DEFAULT 'Agy',
            model TEXT,
            context_used_pct REAL NOT NULL DEFAULT 0.0,
            fresh_tokens_count INTEGER NOT NULL DEFAULT 0,
            cache_read_tokens_count INTEGER NOT NULL DEFAULT 0,
            cache_creation_tokens_count INTEGER NOT NULL DEFAULT 0,
            total_tokens_count INTEGER NOT NULL DEFAULT 0,
            cache_hit_pct REAL NOT NULL DEFAULT 0.0,
            five_hour_used_pct REAL,
            five_hour_resets_at TEXT,
            five_hour_reset_epoch_seconds INTEGER,
            weekly_used_pct REAL,
            weekly_resets_at TEXT,
            weekly_reset_epoch_seconds INTEGER,
            updated_at INTEGER NOT NULL
        );",
        [],
    ).db_context("create agent_session_metrics table")?;
    ensure_column(&conn, "agent_session_metrics", "runner", "TEXT")?;
    ensure_column(&conn, "agent_session_metrics", "cost_dollars", "REAL")?;
    ensure_column(&conn, "agent_session_metrics", "max_context_tokens_count", "INTEGER")?;

    // 18. Multi-Account Rolling Quota Ledger
    conn.execute(
        "CREATE TABLE IF NOT EXISTS account_quotas (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
            provider TEXT NOT NULL DEFAULT 'Agy',
            model TEXT NOT NULL DEFAULT 'default',
            five_hour_remaining_pct REAL,
            five_hour_resets_at TEXT,
            five_hour_reset_epoch_seconds INTEGER,
            five_hour_resets_at_local TEXT,
            weekly_remaining_pct REAL,
            weekly_resets_at TEXT,
            weekly_reset_epoch_seconds INTEGER,
            weekly_resets_at_local TEXT,
            last_pinged_at INTEGER,
            warmup_enabled INTEGER NOT NULL DEFAULT 1,
            status TEXT NOT NULL DEFAULT 'active',
            updated_at INTEGER NOT NULL,
            UNIQUE(account_id, provider, model)
        );",
        [],
    ).db_context("create account_quotas table")?;
    ensure_column(&conn, "account_quotas", "five_hour_resets_at_local", "TEXT")?;
    ensure_column(&conn, "account_quotas", "weekly_resets_at_local", "TEXT")?;

    // 19. Document & Code Review Comments
    conn.execute(
        "CREATE TABLE IF NOT EXISTS document_comments (
            id TEXT PRIMARY KEY,
            project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            file_path TEXT NOT NULL,
            start_line INTEGER NOT NULL,
            end_line INTEGER NOT NULL,
            selected_text TEXT NOT NULL,
            comment_text TEXT NOT NULL,
            author TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'open',
            created_at INTEGER NOT NULL,
            resolved_at INTEGER,
            resolved_by TEXT
        );",
        [],
    ).db_context("create document_comments table")?;

    // 20. Supervised processes ledger (scripts, services, tasks, shells)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS supervised_processes (
            run_id TEXT PRIMARY KEY,
            project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            target_id TEXT NOT NULL,
            target_type TEXT NOT NULL CHECK (target_type IN ('service', 'task', 'shell')),
            command TEXT NOT NULL,
            args_json TEXT NOT NULL DEFAULT '[]',
            cwd TEXT,
            pid INTEGER NOT NULL,
            pgid INTEGER NOT NULL,
            env_identifier TEXT NOT NULL,
            status TEXT NOT NULL CHECK (status IN (
                'starting', 'working', 'completed', 'failed', 'stopped', 'orphaned_killed', 'crashed'
            )),
            exit_code INTEGER,
            spawned_at_ms INTEGER NOT NULL,
            terminated_at_ms INTEGER,
            failure_reason TEXT,
            profiler_engine_id TEXT,
            profile_artifact_path TEXT
        );",
        [],
    ).db_context("create supervised_processes table")?;
    ensure_column(&conn, "supervised_processes", "profiler_engine_id", "TEXT")?;
    ensure_column(&conn, "supervised_processes", "profile_artifact_path", "TEXT")?;

    // 21. UI State Machine & Session Persistence (Spec 35)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS ui_global_state (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            active_project_id INTEGER REFERENCES projects(id) ON DELETE SET NULL,
            chat_layout_mode TEXT NOT NULL DEFAULT 'docked' CHECK (chat_layout_mode IN ('docked', 'floating')),
            docked_chat_width_vw REAL NOT NULL DEFAULT 28.0,
            revision INTEGER NOT NULL DEFAULT 1,
            updated_at_seconds INTEGER NOT NULL
        );",
        [],
    ).db_context("create ui_global_state table")?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS ui_project_state (
            project_id INTEGER PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
            active_node_path TEXT NOT NULL DEFAULT 'overview',
            slot_values_json TEXT NOT NULL DEFAULT '{}',
            chat_panel_state TEXT NOT NULL DEFAULT 'closed' CHECK (chat_panel_state IN ('closed', 'open', 'minimized')),
            active_agent_id TEXT,
            active_conversation_id TEXT,
            updated_at_seconds INTEGER NOT NULL
        );",
        [],
    ).db_context("create ui_project_state table")?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS forge_inbox_items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            provider TEXT NOT NULL CHECK (provider IN ('github', 'gitlab', 'unknown')),
            repo TEXT NOT NULL,
            item_kind TEXT NOT NULL CHECK (item_kind IN ('issue', 'pr')),
            item_number INTEGER NOT NULL,
            provider_node_id TEXT,
            last_seen_updated_at TEXT,
            is_read INTEGER NOT NULL DEFAULT 0,
            triage_flag TEXT CHECK (triage_flag IN ('flagged', 'muted') OR triage_flag IS NULL),
            updated_epoch_seconds INTEGER NOT NULL
        );",
        [],
    ).db_context("create forge_inbox_items table")?;

    // 21b. Voice Orchestrator Harness Persistence (Spec 40)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS voice_sessions (
            id TEXT PRIMARY KEY,
            status TEXT NOT NULL CHECK (status IN ('active', 'archived')),
            active_project_id INTEGER REFERENCES projects(id) ON DELETE SET NULL,
            current_objective TEXT,
            last_delegated_project_id INTEGER,
            last_delegated_agent_id TEXT,
            resumption_handle TEXT,
            resumption_valid_until_epoch_seconds INTEGER,
            rolling_summary TEXT NOT NULL DEFAULT '',
            created_epoch_seconds INTEGER NOT NULL,
            updated_epoch_seconds INTEGER NOT NULL
        );",
        [],
    ).db_context("create voice_sessions table")?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS voice_scratchpad_items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL REFERENCES voice_sessions(id) ON DELETE CASCADE,
            project_id INTEGER REFERENCES projects(id) ON DELETE CASCADE,
            title TEXT NOT NULL,
            status TEXT NOT NULL CHECK (status IN ('pending', 'in_progress', 'done')),
            note TEXT,
            created_epoch_seconds INTEGER NOT NULL,
            updated_epoch_seconds INTEGER NOT NULL
        );",
        [],
    ).db_context("create voice_scratchpad_items table")?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS voice_turn_ledger (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL REFERENCES voice_sessions(id) ON DELETE CASCADE,
            project_id INTEGER,
            user_transcript TEXT,
            actions_summary TEXT,
            assistant_transcript TEXT,
            created_epoch_seconds INTEGER NOT NULL
        );",
        [],
    ).db_context("create voice_turn_ledger table")?;

    // 22. Database Indexes
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_system_logs_origin_id ON system_logs(origin, id DESC);
         CREATE INDEX IF NOT EXISTS idx_system_logs_timestamp ON system_logs(timestamp DESC);
         CREATE INDEX IF NOT EXISTS idx_task_executions_proj ON task_executions(project_id, id DESC);
         CREATE INDEX IF NOT EXISTS idx_delegations_target_status ON agent_delegations(target_agent, status);
         CREATE INDEX IF NOT EXISTS idx_audit_journal_proj_time ON audit_journal(project_id, timestamp DESC);
         CREATE INDEX IF NOT EXISTS idx_audit_journal_agents ON audit_journal(originator, target);
         CREATE INDEX IF NOT EXISTS idx_snapshots_project ON snapshots(project_id);
         CREATE INDEX IF NOT EXISTS idx_snapshots_conversation ON snapshots(conversation_id);
         CREATE INDEX IF NOT EXISTS idx_accounts_email ON accounts(email);
         CREATE INDEX IF NOT EXISTS idx_accounts_is_active ON accounts(is_active);
         CREATE INDEX IF NOT EXISTS idx_agent_convs_agent ON agent_conversations(project_id, agent_id, updated_at DESC);
         CREATE INDEX IF NOT EXISTS idx_chat_turns_conv ON agent_chat_turns(conversation_id, id DESC);
         CREATE INDEX IF NOT EXISTS idx_chat_turns_agy ON agent_chat_turns(agy_conversation_id);
         CREATE INDEX IF NOT EXISTS idx_session_journal_proj ON agent_session_journal(project_id, agent_id, created_at DESC);
         CREATE INDEX IF NOT EXISTS idx_session_journal_agy ON agent_session_journal(agy_conversation_id);
         CREATE INDEX IF NOT EXISTS idx_session_journal_conv ON agent_session_journal(conversation_id, id DESC);
         CREATE INDEX IF NOT EXISTS idx_doc_comments_proj_file ON document_comments(project_id, file_path, status);
         CREATE INDEX IF NOT EXISTS idx_doc_comments_status ON document_comments(status);
         CREATE INDEX IF NOT EXISTS idx_todos_project_status ON project_todos(project_id, status, priority);
         CREATE INDEX IF NOT EXISTS idx_todos_conversation ON project_todos(conversation_id);
         CREATE INDEX IF NOT EXISTS idx_tech_debt_proj ON project_technical_debt(project_id, status, category);
         CREATE INDEX IF NOT EXISTS idx_tech_debt_conversation ON project_technical_debt(conversation_id);
         CREATE INDEX IF NOT EXISTS idx_init_flags_proj ON project_initialization_flags(project_id, status);
         CREATE INDEX IF NOT EXISTS idx_delegations_conversation ON agent_delegations(conversation_id);
         CREATE UNIQUE INDEX IF NOT EXISTS idx_forge_inbox_unique ON forge_inbox_items(project_id, provider, repo, item_kind, item_number);
         CREATE INDEX IF NOT EXISTS idx_forge_inbox_proj ON forge_inbox_items(project_id, provider, is_read, triage_flag);
         CREATE INDEX IF NOT EXISTS idx_conversations_agent_uid ON agent_conversations(agent_uid);
         CREATE INDEX IF NOT EXISTS idx_supervised_processes_runs ON supervised_processes(project_id, target_id, spawned_at_ms DESC);
         CREATE INDEX IF NOT EXISTS idx_session_metrics_conv ON agent_session_metrics(conversation_id);
         CREATE INDEX IF NOT EXISTS idx_session_metrics_proj ON agent_session_metrics(project_id, agent_id);
         CREATE INDEX IF NOT EXISTS idx_account_quotas_epoch ON account_quotas(weekly_reset_epoch_seconds, five_hour_reset_epoch_seconds);
         CREATE INDEX IF NOT EXISTS idx_supervised_processes_lookup ON supervised_processes(project_id, target_id, status);
         CREATE INDEX IF NOT EXISTS idx_supervised_processes_pid ON supervised_processes(pid);
         CREATE INDEX IF NOT EXISTS idx_supervised_processes_unclosed ON supervised_processes(status) WHERE terminated_at_ms IS NULL;
         CREATE INDEX IF NOT EXISTS idx_project_task_stages_proj_order ON project_task_stages(project_id, sort_order, id);
         CREATE INDEX IF NOT EXISTS idx_project_tasks_proj_status ON project_tasks(project_id, status, priority);
         CREATE INDEX IF NOT EXISTS idx_project_tasks_dates ON project_tasks(project_id, due_epoch_seconds);
         CREATE INDEX IF NOT EXISTS idx_project_task_subtasks_task ON project_task_subtasks(task_id, sort_order, id);
         CREATE INDEX IF NOT EXISTS idx_project_task_links_task ON project_task_links(task_id, link_type);
         CREATE INDEX IF NOT EXISTS idx_voice_sessions_status ON voice_sessions(status, updated_epoch_seconds DESC);
         CREATE INDEX IF NOT EXISTS idx_voice_scratchpad_session ON voice_scratchpad_items(session_id, status, id);
         CREATE INDEX IF NOT EXISTS idx_voice_turn_ledger_session ON voice_turn_ledger(session_id, id DESC);"
    ).db_context("create database indexes")?;

    backfill_agent_uids(&conn)?;

    // 21. Rolling log pruning (the logger worker repeats it periodically), then the one-time
    // switch to incremental auto_vacuum. Both are maintenance: a failure (e.g. the other ONE-SHIP
    // process holding a lock during VACUUM) is logged and retried on the next start.
    let log_db_max_rows_count = crate::config::resolve_effective_usize(
        None, None, "system", "log_db_max_rows_count",
    );
    match prune_system_logs_on(&conn, log_db_max_rows_count) {
        Ok(deleted_rows_count) => crate::logger::log("INFO", "db", &format!(
            "init_database: pruned {} system_logs rows (retaining {})", deleted_rows_count, log_db_max_rows_count
        )),
        Err(e) => crate::logger::log("ERROR", "db", &format!("init_database: {}", e)),
    }
    if let Err(e) = ensure_incremental_auto_vacuum(&conn) {
        crate::logger::log("WARN", "db", &format!("init_database: {}; retrying on next start", e));
    }

    crate::logger::log("INFO", "db", &format!("init_database: SQLite database initialized successfully at {:?}", db_path));
    Ok(())
}

// --- Project Operations ---

const PROJECT_COLUMNS: &str = "id, path, name, description, is_favorite, is_archived, hidden_tabs";

fn project_from_row(row: &rusqlite::Row) -> rusqlite::Result<ProjectRecord> {
    let hidden_tabs_raw: String = row.get(6).unwrap_or_else(|_| "[]".to_string());
    let hidden_tabs: Vec<String> = serde_json::from_str(&hidden_tabs_raw).unwrap_or_default();
    let description: Option<String> = row
        .get::<_, Option<String>>(3)
        .unwrap_or(None)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Ok(ProjectRecord {
        id: row.get::<_, i64>(0)? as usize,
        path: row.get(1)?,
        name: row.get(2)?,
        description,
        is_favorite: row.get::<_, i32>(4).unwrap_or(0) != 0,
        is_archived: row.get::<_, i32>(5).unwrap_or(0) != 0,
        hidden_tabs,
    })
}

pub fn db_get_projects() -> Result<Vec<ProjectRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", "db_get_projects: fetching all registered projects");
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(&format!("SELECT {} FROM projects ORDER BY is_favorite DESC, name COLLATE NOCASE ASC", PROJECT_COLUMNS))
        .db_context("prepare list projects")?;
    let rows = stmt.query_map([], project_from_row).db_context("query projects")?;
    let list = collect_rows(rows, "read project row")?;
    crate::logger::log("DEBUG", "db", &format!("db_get_projects: returned {} projects", list.len()));
    Ok(list)
}

pub fn db_get_project_by_path(path: &str) -> Result<Option<ProjectRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_project_by_path: path={}", path));
    let conn = get_connection()?;
    conn.query_row(
        &format!("SELECT {} FROM projects WHERE path = ?1", PROJECT_COLUMNS),
        [path],
        project_from_row,
    )
    .optional()
    .db_context("get project by path")
}

pub fn db_get_project_by_id(id: usize) -> Result<Option<ProjectRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_project_by_id: id={}", id));
    let conn = get_connection()?;
    conn.query_row(
        &format!("SELECT {} FROM projects WHERE id = ?1", PROJECT_COLUMNS),
        [id as i64],
        project_from_row,
    )
    .optional()
    .db_context("get project by id")
}

pub fn notify_projects_updated() {
    let sup = crate::supervisor::get_supervisor();
    match crate::registry::get_projects() {
        Ok(projects) => sup.emit_event(crate::events::DaemonEvent::projects_updated(&projects)),
        Err(e) => crate::logger::log("ERROR", "db", &format!("notify_projects_updated: cannot load projects: {}", e)),
    }
}

pub fn db_add_project(path: &str, name: &str) -> Result<ProjectRecord, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_add_project: adding project '{}' at '{}'", name, path));
    let conn = get_connection()?;
    conn.execute(
        "INSERT INTO projects (path, name, is_favorite, is_archived, hidden_tabs) VALUES (?1, ?2, 0, 0, '[]')",
        params![path, name],
    ).db_context("insert project")?;

    let id = conn.last_insert_rowid() as usize;
    crate::logger::log("INFO", "db", &format!("db_add_project: successfully added project '{}' (id={}) at '{}'", name, id, path));
    notify_projects_updated();
    Ok(ProjectRecord {
        id,
        path: path.to_string(),
        name: name.to_string(),
        description: None,
        is_favorite: false,
        is_archived: false,
        hidden_tabs: Vec::new(),
    })
}

pub fn db_update_project_description(
    project_id: usize,
    description: Option<&str>,
) -> Result<Option<String>, DatabaseError> {
    let cleaned = description
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let conn = get_connection()?;
    conn.execute(
        "UPDATE projects SET description = ?1 WHERE id = ?2",
        params![cleaned, project_id as i64],
    )
    .db_context("update project description")?;
    notify_projects_updated();
    Ok(cleaned)
}

pub fn db_update_project_name(
    project_id: usize,
    name: &str,
) -> Result<String, DatabaseError> {
    let cleaned = name.trim().to_string();
    let conn = get_connection()?;
    conn.execute(
        "UPDATE projects SET name = ?1 WHERE id = ?2",
        params![cleaned, project_id as i64],
    )
    .db_context("update project name")?;
    notify_projects_updated();
    Ok(cleaned)
}

pub fn db_toggle_project_favorite(
    project_id: usize,
    is_favorite: bool,
) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_toggle_project_favorite: project_id={}, is_fav={}", project_id, is_favorite));
    {
        let conn = get_connection()?;
        let fav_int = if is_favorite { 1 } else { 0 };
        conn.execute(
            "UPDATE projects SET is_favorite = ?1 WHERE id = ?2",
            params![fav_int, project_id as i64],
        ).db_context("update project favorite")?;
    }
    crate::logger::log("INFO", "db", &format!("db_toggle_project_favorite: updated project_id={} is_fav={}", project_id, is_favorite));
    notify_projects_updated();
    Ok(())
}

/// Persists whether a project is archived (hidden from active project lists).
pub fn db_toggle_project_archived(
    project_id: usize,
    is_archived: bool,
) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_toggle_project_archived: project_id={}, is_archived={}", project_id, is_archived));
    {
        let conn = get_connection()?;
        let archived_int = if is_archived { 1 } else { 0 };
        conn.execute(
            "UPDATE projects SET is_archived = ?1 WHERE id = ?2",
            params![archived_int, project_id as i64],
        ).db_context("update project archived")?;
    }
    crate::logger::log("INFO", "db", &format!("db_toggle_project_archived: updated project_id={} is_archived={}", project_id, is_archived));
    notify_projects_updated();
    Ok(())
}

pub fn db_toggle_project_tab_visibility(
    project_id: usize,
    tab: &str,
    visible: bool,
) -> Result<Vec<String>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_toggle_project_tab_visibility: project_id={}, tab={}, visible={}", project_id, tab, visible));
    let updated_tabs = {
        let conn = get_connection()?;
        let current_json: String = conn.query_row(
            "SELECT hidden_tabs FROM projects WHERE id = ?1",
            [project_id as i64],
            |row| row.get(0),
        ).optional().db_context("get current hidden_tabs")?
        .unwrap_or_else(|| "[]".to_string());

        let mut tabs: Vec<String> = serde_json::from_str(&current_json).unwrap_or_default();
        if visible {
            tabs.retain(|t| t != tab);
        } else if !tabs.iter().any(|t| t == tab) {
            tabs.push(tab.to_string());
        }

        let new_json = serde_json::to_string(&tabs).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "UPDATE projects SET hidden_tabs = ?1 WHERE id = ?2",
            params![new_json, project_id as i64],
        ).db_context("update project hidden_tabs")?;

        tabs
    };
    crate::logger::log("INFO", "db", &format!("db_toggle_project_tab_visibility: updated project_id={} tab={} visible={} new_hidden={:?}", project_id, tab, visible, updated_tabs));
    notify_projects_updated();
    Ok(updated_tabs)
}

pub fn db_remove_project(id: usize) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_remove_project: deleting project id {}", id));
    let mut conn = get_connection()?;
    let pid = id as i64;
    // Every table that references `projects(id)` cascades (foreign_keys = ON), except these two
    // which carry a bare project_id and `project_initializations`, keyed by path.
    // One transaction: a failure removes nothing.
    let tx = conn.transaction().db_context("begin project removal")?;
    tx.execute(
        "DELETE FROM project_initializations WHERE project_path = (SELECT path FROM projects WHERE id = ?1)",
        params![pid],
    )
    .db_context("delete project initialization state")?;
    tx.execute("DELETE FROM task_executions WHERE project_id = ?1", params![pid])
        .db_context("delete project task executions")?;
    tx.execute("DELETE FROM audit_journal WHERE project_id = ?1", params![pid])
        .db_context("delete project audit journal")?;
    tx.execute("DELETE FROM projects WHERE id = ?1", params![pid])
        .db_context("delete project")?;
    tx.commit().db_context("commit project removal")?;

    crate::logger::log("INFO", "db", &format!("db_remove_project: successfully removed project id {}", id));
    notify_projects_updated();
    Ok(())
}

// --- System Logs Operations ---

/// Inserts one micro-batch of log entries in a single transaction (all or nothing).
///
/// Called only by the logger worker thread. It must not log: a failure here would queue more
/// inserts into the failing sink, so errors are returned for the worker to write to the file sink.
pub fn db_insert_logs_batch(logs: &[crate::logger::QueuedLog]) -> Result<(), DatabaseError> {
    if logs.is_empty() {
        return Ok(());
    }
    let mut conn = get_connection_unlogged()?;
    let tx = conn.transaction().db_context_unlogged("begin log batch")?;
    {
        let mut stmt = tx
            .prepare_cached("INSERT INTO system_logs (level, source, origin, message) VALUES (?1, ?2, ?3, ?4)")
            .db_context_unlogged("prepare log batch insert")?;
        for log in logs {
            stmt.execute(params![&log.level, &log.source, &log.origin, &log.message])
                .db_context_unlogged("insert log row")?;
        }
    }
    tx.commit().db_context_unlogged("commit log batch")
}

/// Deletes all but the newest `max_rows_count` rows of `system_logs`, then returns the freed
/// pages to the filesystem (`PRAGMA incremental_vacuum`, effective once `auto_vacuum` is
/// INCREMENTAL, see [`ensure_incremental_auto_vacuum`]). Returns the number of deleted rows.
///
/// Called by `init_database` and periodically by the logger worker; it must not log (same
/// reason as [`db_insert_logs_batch`]).
pub fn db_prune_system_logs(max_rows_count: usize) -> Result<usize, DatabaseError> {
    let conn = get_connection_unlogged()?;
    prune_system_logs_on(&conn, max_rows_count)
}

fn prune_system_logs_on(conn: &rusqlite::Connection, max_rows_count: usize) -> Result<usize, DatabaseError> {
    // The OFFSET subquery walks the primary key from the newest row; NULL (fewer rows) deletes nothing.
    let deleted_rows_count = conn
        .execute(
            "DELETE FROM system_logs WHERE id <= (SELECT id FROM system_logs ORDER BY id DESC LIMIT 1 OFFSET ?1)",
            params![max_rows_count as i64],
        )
        .db_context_unlogged("prune system_logs")?;
    if deleted_rows_count > 0 {
        conn.execute_batch("PRAGMA incremental_vacuum;")
            .db_context_unlogged("incremental vacuum after log prune")?;
    }
    Ok(deleted_rows_count)
}

/// One-time switch of the database to `auto_vacuum = INCREMENTAL`.
///
/// SQLite only applies a new `auto_vacuum` mode during `VACUUM`, so databases created before
/// this setting are rebuilt once (audit F18: 3.9 GB file with ~2.6 GB of free pages after log
/// pruning). Later starts read mode 2 and return immediately. Call after the log prune so the
/// rebuild copies only live data.
fn ensure_incremental_auto_vacuum(conn: &rusqlite::Connection) -> Result<(), DatabaseError> {
    const AUTO_VACUUM_INCREMENTAL_MODE: i64 = 2;
    let mode: i64 = conn
        .query_row("PRAGMA auto_vacuum", [], |row| row.get(0))
        .db_context("read auto_vacuum mode")?;
    if mode == AUTO_VACUUM_INCREMENTAL_MODE {
        return Ok(());
    }
    let page_count_before: i64 = conn
        .query_row("PRAGMA page_count", [], |row| row.get(0))
        .db_context("read page_count")?;
    crate::logger::log("INFO", "db", &format!(
        "ensure_incremental_auto_vacuum: switching auto_vacuum {} -> INCREMENTAL; rebuilding {} pages",
        mode, page_count_before
    ));
    conn.execute_batch("PRAGMA auto_vacuum = INCREMENTAL; VACUUM;")
        .db_context("rebuild database with incremental auto_vacuum")?;
    let page_count_after: i64 = conn
        .query_row("PRAGMA page_count", [], |row| row.get(0))
        .db_context("read page_count after rebuild")?;
    crate::logger::log("INFO", "db", &format!(
        "ensure_incremental_auto_vacuum: rebuild complete ({} -> {} pages)",
        page_count_before, page_count_after
    ));
    Ok(())
}

// --- Task Execution Operations ---

pub fn db_insert_task_execution(
    project_id: i64,
    task_id: &str,
    task_type: &str,
    command: &str,
) -> Result<i64, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_insert_task_execution: project_id={}, task_id={}, task_type={}, command={}", project_id, task_id, task_type, command));
    let conn = get_connection()?;
    conn.execute(
        "INSERT INTO task_executions (project_id, task_id, task_type, command, status)
         VALUES (?1, ?2, ?3, ?4, 'running')",
        params![project_id, task_id, task_type, command],
    ).db_context("insert task execution")?;

    let id = conn.last_insert_rowid();
    crate::logger::log("INFO", "db", &format!("db_insert_task_execution: created task execution id {} for task_id {}", id, task_id));
    Ok(id)
}

/// Closes a `running` task execution. `exit_code` is `None` when the process never started.
pub fn db_finish_task_execution(
    execution_id: i64,
    duration_ms: i64,
    exit_code: Option<i32>,
    status: &str,
    summary: Option<&str>,
    details: Option<&str>,
    stdout_log: Option<&str>,
) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_finish_task_execution: execution_id={}, status={}, exit_code={:?}, duration_ms={}", execution_id, status, exit_code, duration_ms));
    let conn = get_connection()?;
    conn.execute(
        "UPDATE task_executions
         SET duration_ms = ?1, exit_code = ?2, status = ?3, summary = ?4, details_markdown = ?5, stdout_log = ?6
         WHERE id = ?7",
        params![duration_ms, exit_code, status, summary, details, stdout_log, execution_id],
    ).db_context("update task execution")?;

    crate::logger::log("INFO", "db", &format!("db_finish_task_execution: finished execution_id {} with status '{}'", execution_id, status));
    Ok(())
}

const TASK_EXECUTION_COLUMNS: &str =
    "id, project_id, task_id, task_type, command, started_at, duration_ms, exit_code, status, summary, details_markdown, stdout_log";

fn task_execution_from_row(row: &rusqlite::Row) -> rusqlite::Result<TaskExecutionRecord> {
    Ok(TaskExecutionRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        task_id: row.get(2)?,
        task_type: row.get(3)?,
        command: row.get(4)?,
        started_at: row.get(5)?,
        duration_ms: row.get(6)?,
        exit_code: row.get(7)?,
        status: row.get(8)?,
        summary: row.get(9)?,
        details_markdown: row.get(10)?,
        stdout_log: row.get(11)?,
    })
}

pub fn db_get_task_executions(project_id: i64, limit_count: usize) -> Result<Vec<TaskExecutionRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_task_executions: project_id={}, limit_count={}", project_id, limit_count));
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {} FROM task_executions WHERE project_id = ?1 ORDER BY id DESC LIMIT ?2",
            TASK_EXECUTION_COLUMNS
        ))
        .db_context("prepare list task executions")?;
    let rows = stmt
        .query_map(params![project_id, limit_count as i64], task_execution_from_row)
        .db_context("query task executions")?;
    let list = collect_rows(rows, "read task execution row")?;
    crate::logger::log("DEBUG", "db", &format!("db_get_task_executions: returned {} records for project_id {}", list.len(), project_id));
    Ok(list)
}

pub fn db_get_task_report(execution_id: i64) -> Result<Option<TaskExecutionRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_task_report: execution_id={}", execution_id));
    let conn = get_connection()?;
    let report = conn
        .query_row(
            &format!("SELECT {} FROM task_executions WHERE id = ?1", TASK_EXECUTION_COLUMNS),
            params![execution_id],
            task_execution_from_row,
        )
        .optional()
        .db_context("get task report")?;
    crate::logger::log("DEBUG", "db", &format!("db_get_task_report: execution_id={}, found={}", execution_id, report.is_some()));
    Ok(report)
}

/// Arguments of `get_task_executions`: the newest `limit_count` executions of a project.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct GetTaskExecutionsArgs {
    #[ts(type = "number")]
    pub project_id: i64,
    pub limit_count: usize,
}

/// Arguments of `get_task_report`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct GetTaskReportArgs {
    #[ts(type = "number")]
    pub execution_id: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct SystemLogRecord {
    #[ts(type = "number")]
    pub id: i64,
    pub timestamp: String,
    pub level: String,
    pub source: String,
    pub origin: String,
    pub message: String,
}

/// Arguments of `get_system_logs`. Paging is by cursor (AGENTS.md §12.4): `before_id` loads
/// older entries, `after_id` newer ones; with neither, the newest `limit_count` entries.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct GetSystemLogsArgs {
    pub origin: Option<String>,
    pub source: Option<String>,
    pub search_query: Option<String>,
    pub level: Option<String>,
    pub limit_count: usize,
    #[ts(type = "number | null")]
    pub before_id: Option<i64>,
    #[ts(type = "number | null")]
    pub after_id: Option<i64>,
}

/// Daemon command `get_system_logs`.
pub fn get_system_logs(args: GetSystemLogsArgs) -> Result<Vec<SystemLogRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!(
        "get_system_logs: origin={:?}, source={:?}, level={:?}, has_search_query={}, limit={}, before_id={:?}, after_id={:?}",
        args.origin, args.source, args.level, args.search_query.is_some(), args.limit_count, args.before_id, args.after_id
    ));
    let GetSystemLogsArgs { origin, source, search_query, level, limit_count, before_id, after_id } = args;
    let conn = get_connection()?;
    let mut query = String::from("SELECT id, timestamp, level, source, origin, message FROM system_logs WHERE 1=1");
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(orig) = origin {
        if !orig.trim().is_empty() {
            query.push_str(&format!(" AND origin = ?{}", params_vec.len() + 1));
            params_vec.push(Box::new(orig));
        }
    }

    if let Some(src) = source {
        if !src.trim().is_empty() {
            query.push_str(&format!(" AND source = ?{}", params_vec.len() + 1));
            params_vec.push(Box::new(src));
        }
    }

    if let Some(lvl) = level {
        if !lvl.trim().is_empty() {
            query.push_str(&format!(" AND level = ?{}", params_vec.len() + 1));
            params_vec.push(Box::new(lvl));
        }
    }

    if let Some(sq) = search_query {
        if !sq.trim().is_empty() {
            let search_term = format!("%{}%", sq);
            query.push_str(&format!(" AND (message LIKE ?{} OR source LIKE ?{})", params_vec.len() + 1, params_vec.len() + 1));
            params_vec.push(Box::new(search_term));
        }
    }

    if let Some(bid) = before_id {
        query.push_str(&format!(" AND id < ?{}", params_vec.len() + 1));
        params_vec.push(Box::new(bid));
    }

    if let Some(aid) = after_id {
        query.push_str(&format!(" AND id > ?{}", params_vec.len() + 1));
        params_vec.push(Box::new(aid));
        query.push_str(&format!(" ORDER BY id ASC LIMIT ?{}", params_vec.len() + 1));
        params_vec.push(Box::new(limit_count as i64));
    } else {
        query.push_str(&format!(" ORDER BY id DESC LIMIT ?{}", params_vec.len() + 1));
        params_vec.push(Box::new(limit_count as i64));
    }

    let mut stmt = conn.prepare(&query).db_context("prepare system log query")?;

    // Borrow params_vec elements as &dyn ToSql
    let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();

    let rows = stmt.query_map(rusqlite::params_from_iter(params_refs), |row| {
        Ok(SystemLogRecord {
            id: row.get(0)?,
            timestamp: row.get(1)?,
            level: row.get(2)?,
            source: row.get(3)?,
            origin: row.get(4)?,
            message: row.get(5)?,
        })
    }).db_context("query system logs")?;
    let list = collect_rows(rows, "read system log row")?;

    // Newest first unless paging forward with `after_id` (cursor-based paging, AGENTS.md §12.4).
    crate::logger::log("DEBUG", "db", &format!("get_system_logs: returning {} logs", list.len()));
    Ok(list)
}

/// Daemon command `clear_system_logs`: empties `system_logs` and resets its id sequence.
pub fn clear_system_logs() -> Result<(), DatabaseError> {
    crate::logger::log("INFO", "db", "clear_system_logs: clearing all system logs");
    let conn = get_connection()?;
    conn.execute("DELETE FROM system_logs;", []).db_context("clear system_logs")?;
    conn.execute("DELETE FROM sqlite_sequence WHERE name = 'system_logs';", [])
        .db_context("reset system_logs sequence")?;
    Ok(())
}

/// Daemon command `get_system_log_sources`: lists distinct log subsystem sources present in `system_logs`.
pub fn get_system_log_sources() -> Result<Vec<String>, DatabaseError> {
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare("SELECT DISTINCT source FROM system_logs WHERE source IS NOT NULL AND source != '' ORDER BY source ASC")
        .db_context("prepare system log sources query")?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .db_context("query system log sources")?;
    collect_rows(rows, "read system log source")
}


#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct ProjectInitState {
    pub project_path: String,
    pub status: String,
    pub mode: String,
    pub current_phase: Option<String>,
    pub last_message: Option<String>,
    pub git_checkpoint: Option<String>,
    /// The agy internal conversation ID captured from the stream-json `init` event.
    /// Persisted so initialization sessions can be resumed across restarts.
    pub agy_conversation_id: Option<String>,
}

pub fn db_get_init_state(project_path: &str) -> Result<Option<ProjectInitState>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_init_state: project_path={}", project_path));
    let conn = get_connection()?;
    conn.query_row(
        "SELECT project_path, status, mode, current_phase, last_message, git_checkpoint, agy_conversation_id \
         FROM project_initializations WHERE project_path = ?1",
        [project_path],
        |row| {
            Ok(ProjectInitState {
                project_path: row.get(0)?,
                status: row.get(1)?,
                mode: row.get(2)?,
                current_phase: row.get(3)?,
                last_message: row.get(4)?,
                git_checkpoint: row.get(5)?,
                agy_conversation_id: row.get(6)?,
            })
        },
    )
    .optional()
    .db_context("get project init state")
}

/// Stores or updates the initialization state for a project.
/// The `agy_conversation_id` column is written only if `Some`; existing value is preserved otherwise.
pub fn db_upsert_init_state(state: &ProjectInitState) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_upsert_init_state: project_path={}, status={}, mode={}", state.project_path, state.status, state.mode));
    let conn = get_connection()?;
    conn.execute(
        "INSERT INTO project_initializations \
             (project_path, status, mode, current_phase, last_message, git_checkpoint, agy_conversation_id, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, CURRENT_TIMESTAMP)
         ON CONFLICT(project_path) DO UPDATE SET
            status               = excluded.status,
            mode                 = excluded.mode,
            current_phase        = excluded.current_phase,
            last_message         = excluded.last_message,
            git_checkpoint       = excluded.git_checkpoint,
            agy_conversation_id  = COALESCE(excluded.agy_conversation_id, agy_conversation_id),
            updated_at           = CURRENT_TIMESTAMP",
        (
            &state.project_path,
            &state.status,
            &state.mode,
            &state.current_phase,
            &state.last_message,
            &state.git_checkpoint,
            &state.agy_conversation_id,
        ),
    ).db_context("upsert init state")?;

    crate::logger::log("INFO", "db", &format!("db_upsert_init_state: successfully updated init state for '{}' (status={})", state.project_path, state.status));
    Ok(())
}

pub fn db_delete_init_state(project_path: &str) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_delete_init_state: project_path={}", project_path));
    let conn = get_connection()?;
    conn.execute("DELETE FROM project_initializations WHERE project_path = ?1", [project_path])
        .db_context("delete init state")?;

    crate::logger::log("INFO", "db", &format!("db_delete_init_state: deleted init state for '{}'", project_path));
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct DelegationRecord {
    pub task_id: String,
    pub originator_agent: String,
    pub target_agent: String,
    pub instruction: String,
    pub status: String,
    pub dispatched_at: u64,
    pub completed_at: Option<u64>,
    pub result_summary: Option<String>,
    pub originator_project: Option<String>,
    pub target_project: Option<String>,
    pub conversation_id: Option<String>,
    pub originator_agent_uid: Option<String>,
    pub target_agent_uid: Option<String>,
}

pub fn db_insert_delegation(record: &DelegationRecord) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_insert_delegation: task_id={}, originator={}, target={}", record.task_id, record.originator_agent, record.target_agent));
    let conn = get_connection()?;
    conn.execute(
        "INSERT INTO agent_delegations (task_id, originator_agent, target_agent, instruction, status, dispatched_at, originator_project, target_project, conversation_id, originator_agent_uid, target_agent_uid)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            record.task_id,
            record.originator_agent,
            record.target_agent,
            record.instruction,
            record.status,
            record.dispatched_at as i64,
            record.originator_project,
            record.target_project,
            record.conversation_id,
            record.originator_agent_uid,
            record.target_agent_uid,
        ],
    ).db_context("insert delegation")?;
    crate::logger::log("INFO", "db", &format!("db_insert_delegation: dispatched task '{}' from '{}' to '{}'", record.task_id, record.originator_agent, record.target_agent));
    Ok(())
}

pub fn db_get_conversation_delegations(conversation_id: &str) -> Result<Vec<DelegationRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_conversation_delegations: conversation_id={}", conversation_id));
    let conn = get_connection()?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM agent_delegations WHERE conversation_id = ?1 ORDER BY dispatched_at DESC",
        DELEGATION_COLUMNS
    )).db_context("prepare conversation delegations query")?;
    let rows = stmt.query_map(params![conversation_id], delegation_from_row).db_context("query conversation delegations")?;
    let list = collect_rows(rows, "read delegation row")?;
    Ok(list)
}

/// Whether a registered agent process is bound to a project persona or is the
/// project-less orchestrator `system` agent.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentInstanceScope {
    Project,
    System,
}

impl AgentInstanceScope {
    fn as_db_str(self) -> &'static str {
        match self {
            AgentInstanceScope::Project => "project",
            AgentInstanceScope::System => "system",
        }
    }

    fn from_db_str(raw: &str) -> Result<Self, DatabaseError> {
        match raw {
            "project" => Ok(AgentInstanceScope::Project),
            "system" => Ok(AgentInstanceScope::System),
            other => Err(DatabaseError::InvalidState(format!("unknown agent_instances.scope '{}'", other))),
        }
    }
}

/// One live (or terminated) agent process registered by the supervisor at spawn.
///
/// Invariants: `scope == Project` implies `project_id` and `agent_uid` are `Some`;
/// `scope == System` implies both are `None` (enforced by a SQL CHECK constraint).
/// `terminated_at_ms` is `None` while the process is alive. Rows are written by the
/// supervisor only; the MCP daemon reads them to attest caller identity.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentInstanceRecord {
    pub instance_id: String,
    pub scope: AgentInstanceScope,
    pub project_id: Option<i64>,
    pub agent_uid: Option<String>,
    pub runner: String,
    pub pid: u32,
    pub spawned_at_ms: u64,
    pub terminated_at_ms: Option<u64>,
}

pub(crate) fn db_get_or_create_agent_persona_conn(conn: &rusqlite::Connection, project_id: i64, display_name: &str) -> Result<String, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_or_create_agent_persona_conn: project_id={}, display_name={}", project_id, display_name));
    let candidate_uid = uuid::Uuid::new_v4().to_string();
    let created_at_ms = crate::supervisor::epoch_now_ms() as i64;
    conn.execute(
        "INSERT OR IGNORE INTO agent_personas (agent_uid, project_id, display_name, created_at_ms) VALUES (?1, ?2, ?3, ?4)",
        params![candidate_uid, project_id, display_name, created_at_ms],
    ).db_context("insert agent persona")?;
    let agent_uid: String = conn.query_row(
        "SELECT agent_uid FROM agent_personas WHERE project_id = ?1 AND display_name = ?2",
        params![project_id, display_name],
        |row| row.get(0),
    ).db_context("select agent persona")?;
    if agent_uid == candidate_uid {
        crate::logger::log("INFO", "db", &format!("db_get_or_create_agent_persona_conn: created persona '{}' uid={} in project {}", display_name, agent_uid, project_id));
    }
    Ok(agent_uid)
}

/// Returns the stable `agent_uid` for a persona, creating it on first use.
///
/// The `(project_id, display_name)` pair is unique, so concurrent callers converge on
/// the same uid. The display name is a label only; the uid is the internal identity.
pub fn db_get_or_create_agent_persona(project_id: i64, display_name: &str) -> Result<String, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_or_create_agent_persona: project_id={}, display_name={}", project_id, display_name));
    let conn = get_connection()?;
    db_get_or_create_agent_persona_conn(&conn, project_id, display_name)
}

fn backfill_agent_uids(conn: &rusqlite::Connection) -> Result<(), DatabaseError> {
    // 1. Backfill agent_conversations
    let mut stmt = conn.prepare("SELECT DISTINCT project_id, agent_id FROM agent_conversations WHERE agent_uid IS NULL")
        .db_context("prepare backfill agent_conversations")?;
    let pairs: Vec<(i64, String)> = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .db_context("query distinct conversation personas")?
        .collect::<Result<Vec<_>, _>>()
        .db_context("collect distinct conversation personas")?;
    for (pid, aid) in pairs {
        let uid = db_get_or_create_agent_persona_conn(conn, pid, &aid)?;
        conn.execute(
            "UPDATE agent_conversations SET agent_uid = ?1 WHERE project_id = ?2 AND agent_id = ?3 AND agent_uid IS NULL",
            params![uid, pid, aid],
        ).db_context("backfill agent_conversations agent_uid")?;
    }

    // 2. Backfill agent_delegations
    let mut stmt = conn.prepare("SELECT task_id, originator_agent, target_agent, originator_project, target_project FROM agent_delegations WHERE originator_agent_uid IS NULL OR target_agent_uid IS NULL")
        .db_context("prepare backfill agent_delegations")?;
    struct DelRow {
        task_id: String,
        orig_agent: String,
        target_agent: String,
        orig_project: Option<String>,
        target_project: Option<String>,
    }
    let dels: Vec<DelRow> = stmt.query_map([], |row| {
        Ok(DelRow {
            task_id: row.get(0)?,
            orig_agent: row.get(1)?,
            target_agent: row.get(2)?,
            orig_project: row.get(3)?,
            target_project: row.get(4)?,
        })
    })
    .db_context("query delegations for backfill")?
    .collect::<Result<Vec<_>, _>>()
    .db_context("collect delegations for backfill")?;

    for d in dels {
        let orig_uid = if let Some(path) = &d.orig_project {
            if let Ok(Some(pid)) = conn.query_row("SELECT id FROM projects WHERE path = ?1", [path], |r| r.get::<_, i64>(0)).optional() {
                db_get_or_create_agent_persona_conn(conn, pid, &d.orig_agent).ok()
            } else { None }
        } else { None };

        let targ_uid = if let Some(path) = &d.target_project {
            if let Ok(Some(pid)) = conn.query_row("SELECT id FROM projects WHERE path = ?1", [path], |r| r.get::<_, i64>(0)).optional() {
                db_get_or_create_agent_persona_conn(conn, pid, &d.target_agent).ok()
            } else { None }
        } else { None };

        if orig_uid.is_some() || targ_uid.is_some() {
            conn.execute(
                "UPDATE agent_delegations SET originator_agent_uid = COALESCE(originator_agent_uid, ?1), target_agent_uid = COALESCE(target_agent_uid, ?2) WHERE task_id = ?3",
                params![orig_uid, targ_uid, d.task_id],
            ).db_context("update delegation uids")?;
        }
    }
    Ok(())
}

/// Returns the display label of a persona, or `None` if the uid is unknown.
pub fn db_get_agent_persona_display_name(agent_uid: &str) -> Result<Option<String>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_agent_persona_display_name: agent_uid={}", agent_uid));
    let conn = get_connection()?;
    let mut stmt = conn.prepare("SELECT display_name FROM agent_personas WHERE agent_uid = ?1")
        .db_context("prepare get persona display name")?;
    let mut rows = stmt.query(params![agent_uid]).db_context("query persona display name")?;
    match rows.next().db_context("read persona display name row")? {
        Some(row) => Ok(Some(row.get(0).db_context("read persona display name")?)),
        None => Ok(None),
    }
}

/// Registers a freshly spawned agent process. Fails if `instance_id` already exists.
pub fn db_insert_agent_instance(record: &AgentInstanceRecord) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!(
        "db_insert_agent_instance: instance_id={}, scope={:?}, project_id={:?}, pid={}, runner={}",
        record.instance_id, record.scope, record.project_id, record.pid, record.runner
    ));
    let conn = get_connection()?;
    conn.execute(
        "INSERT INTO agent_instances (instance_id, scope, project_id, agent_uid, runner, pid, spawned_at_ms, terminated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)",
        params![
            record.instance_id,
            record.scope.as_db_str(),
            record.project_id,
            record.agent_uid,
            record.runner,
            record.pid as i64,
            record.spawned_at_ms as i64,
        ],
    ).db_context("insert agent instance")?;
    Ok(())
}

/// Looks up one agent instance by its opaque id. Returns `None` if unknown.
pub fn db_get_agent_instance(instance_id: &str) -> Result<Option<AgentInstanceRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_agent_instance: instance_id={}", instance_id));
    let conn = get_connection()?;
    let mut stmt = conn.prepare(
        "SELECT instance_id, scope, project_id, agent_uid, runner, pid, spawned_at_ms, terminated_at_ms
         FROM agent_instances WHERE instance_id = ?1",
    ).db_context("prepare get agent instance")?;
    let mut rows = stmt.query(params![instance_id]).db_context("query agent instance")?;
    let row = match rows.next().db_context("read agent instance row")? {
        Some(row) => row,
        None => return Ok(None),
    };
    let scope_raw: String = row.get(1).db_context("read agent instance scope")?;
    let pid_raw: i64 = row.get(5).db_context("read agent instance pid")?;
    let spawned_raw: i64 = row.get(6).db_context("read agent instance spawned_at_ms")?;
    let terminated_raw: Option<i64> = row.get(7).db_context("read agent instance terminated_at_ms")?;
    Ok(Some(AgentInstanceRecord {
        instance_id: row.get(0).db_context("read agent instance id")?,
        scope: AgentInstanceScope::from_db_str(&scope_raw)?,
        project_id: row.get(2).db_context("read agent instance project_id")?,
        agent_uid: row.get(3).db_context("read agent instance agent_uid")?,
        runner: row.get(4).db_context("read agent instance runner")?,
        pid: pid_raw as u32,
        spawned_at_ms: spawned_raw as u64,
        terminated_at_ms: terminated_raw.map(|v| v as u64),
    }))
}

/// Marks an agent instance as terminated. Idempotent: an already-terminated row keeps
/// its original `terminated_at_ms`.
pub fn db_terminate_agent_instance(instance_id: &str, terminated_at_ms: u64) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_terminate_agent_instance: instance_id={}", instance_id));
    let conn = get_connection()?;
    let changed_count = conn.execute(
        "UPDATE agent_instances SET terminated_at_ms = ?2 WHERE instance_id = ?1 AND terminated_at_ms IS NULL",
        params![instance_id, terminated_at_ms as i64],
    ).db_context("terminate agent instance")?;
    if changed_count > 0 {
        crate::logger::log("INFO", "db", &format!("db_terminate_agent_instance: instance {} terminated", instance_id));
    }
    Ok(())
}

/// Lists every instance whose `terminated_at_ms` is still NULL.
pub fn db_list_alive_agent_instances() -> Result<Vec<AgentInstanceRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", "db_list_alive_agent_instances");
    let conn = get_connection()?;
    let mut stmt = conn.prepare("SELECT instance_id FROM agent_instances WHERE terminated_at_ms IS NULL")
        .db_context("prepare list alive agent instances")?;
    let ids: Vec<String> = stmt.query_map([], |row| row.get(0))
        .db_context("query alive agent instances")?
        .collect::<rusqlite::Result<Vec<String>>>()
        .db_context("read alive agent instance ids")?;
    let mut records = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(record) = db_get_agent_instance(&id)? {
            records.push(record);
        }
    }
    Ok(records)
}

// --- Supervised Process Persistence ---

/// Persistent record of a managed process (service, task, or interactive shell) in `supervised_processes`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SupervisedProcessRecord {
    pub run_id: String,
    pub project_id: i64,
    pub target_id: String,
    pub target_type: String,
    pub command: String,
    pub args_json: String,
    pub cwd: Option<String>,
    pub pid: u32,
    pub pgid: u32,
    pub env_identifier: String,
    pub status: String,
    pub exit_code: Option<i32>,
    pub spawned_at_ms: u64,
    pub terminated_at_ms: Option<u64>,
    pub failure_reason: Option<String>,
    /// Profiler engine the run was wrapped by (`rula.toml` `profiler`), if any.
    pub profiler_engine_id: Option<String>,
    /// Project-relative engine output file of the run (`scratchpad/profiling/<run_id>.<ext>`).
    pub profile_artifact_path: Option<String>,
}

/// Records a newly spawned managed process in SQLite.
pub fn db_record_process_spawn(record: &SupervisedProcessRecord) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_record_process_spawn: run_id={} target={}:{} pid={}", record.run_id, record.target_type, record.target_id, record.pid));
    let conn = get_connection()?;
    conn.execute(
        "INSERT INTO supervised_processes (
            run_id, project_id, target_id, target_type, command, args_json, cwd, pid, pgid, env_identifier, status, exit_code, spawned_at_ms, terminated_at_ms, failure_reason,
            profiler_engine_id, profile_artifact_path
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        params![
            record.run_id,
            record.project_id,
            record.target_id,
            record.target_type,
            record.command,
            record.args_json,
            record.cwd,
            record.pid as i64,
            record.pgid as i64,
            record.env_identifier,
            record.status,
            record.exit_code,
            record.spawned_at_ms as i64,
            record.terminated_at_ms.map(|v| v as i64),
            record.failure_reason,
            record.profiler_engine_id,
            record.profile_artifact_path,
        ],
    ).db_context("insert supervised process")?;
    crate::logger::log("INFO", "db", &format!("db_record_process_spawn: recorded process '{}' (PID {})", record.run_id, record.pid));
    Ok(())
}

/// Records how a managed process ended. A run terminates once: the first call wins and later ones
/// (e.g. `stop_process` after the watcher recorded the real exit code) change nothing.
pub fn db_record_process_termination(
    run_id: &str,
    status: &str,
    exit_code: Option<i32>,
    terminated_at_ms: u64,
    failure_reason: Option<&str>,
) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_record_process_termination: run_id={} status={}", run_id, status));
    let conn = get_connection()?;
    let changed = conn.execute(
        "UPDATE supervised_processes SET status = ?2, exit_code = ?3, terminated_at_ms = ?4, failure_reason = ?5
         WHERE run_id = ?1 AND terminated_at_ms IS NULL",
        params![run_id, status, exit_code, terminated_at_ms as i64, failure_reason],
    ).db_context("update supervised process termination")?;
    if changed > 0 {
        crate::logger::log("INFO", "db", &format!("db_record_process_termination: process '{}' transitioned to '{}'", run_id, status));
    }
    Ok(())
}

/// Lists all supervised process rows whose `terminated_at_ms` is NULL and status is 'starting' or 'working'.
pub fn db_list_unclosed_supervised_processes() -> Result<Vec<SupervisedProcessRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", "db_list_unclosed_supervised_processes");
    let conn = get_connection()?;
    let mut stmt = conn.prepare(
        "SELECT run_id, project_id, target_id, target_type, command, args_json, cwd, pid, pgid, env_identifier, status, exit_code, spawned_at_ms, terminated_at_ms, failure_reason,
                profiler_engine_id, profile_artifact_path
         FROM supervised_processes
         WHERE status IN ('starting', 'working') AND terminated_at_ms IS NULL"
    ).db_context("prepare list unclosed supervised processes")?;
    
    let rows = stmt.query_map([], |row| {
        Ok(SupervisedProcessRecord {
            run_id: row.get(0)?,
            project_id: row.get(1)?,
            target_id: row.get(2)?,
            target_type: row.get(3)?,
            command: row.get(4)?,
            args_json: row.get(5)?,
            cwd: row.get(6)?,
            pid: row.get::<_, i64>(7)? as u32,
            pgid: row.get::<_, i64>(8)? as u32,
            env_identifier: row.get(9)?,
            status: row.get(10)?,
            exit_code: row.get(11)?,
            spawned_at_ms: row.get::<_, i64>(12)? as u64,
            terminated_at_ms: row.get::<_, Option<i64>>(13)?.map(|v| v as u64),
            failure_reason: row.get(14)?,
            profiler_engine_id: row.get(15)?,
            profile_artifact_path: row.get(16)?,
        })
    }).db_context("query unclosed supervised processes")?;
    
    let mut list = Vec::new();
    for item in rows {
        list.push(item.db_context("read unclosed supervised process row")?);
    }
    Ok(list)
}

const DELEGATION_COLUMNS: &str =
    "task_id, originator_agent, target_agent, instruction, status, dispatched_at, completed_at, result_summary, originator_project, target_project, conversation_id, originator_agent_uid, target_agent_uid";

fn delegation_from_row(row: &rusqlite::Row) -> rusqlite::Result<DelegationRecord> {
    Ok(DelegationRecord {
        task_id: row.get(0)?,
        originator_agent: row.get(1)?,
        target_agent: row.get(2)?,
        instruction: row.get(3)?,
        status: row.get(4)?,
        dispatched_at: row.get::<_, i64>(5)? as u64,
        completed_at: row.get::<_, Option<i64>>(6)?.map(|v| v as u64),
        result_summary: row.get(7)?,
        originator_project: row.get(8)?,
        target_project: row.get(9)?,
        conversation_id: row.get(10)?,
        originator_agent_uid: row.get(11)?,
        target_agent_uid: row.get(12)?,
    })
}

pub fn db_get_pending_delegation_by_task(task_id: &str) -> Result<Option<DelegationRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_pending_delegation_by_task: task_id={}", task_id));
    let conn = get_connection()?;
    let rec = conn
        .query_row(
            &format!("SELECT {} FROM agent_delegations WHERE task_id = ?1 AND status = 'pending'", DELEGATION_COLUMNS),
            [task_id],
            delegation_from_row,
        )
        .optional()
        .db_context("get pending delegation by task")?;
    crate::logger::log("DEBUG", "db", &format!("db_get_pending_delegation_by_task: task_id={}, found={}", task_id, rec.is_some()));
    Ok(rec)
}

/// Newest pending delegation addressed to `target_agent` in the project at `target_project_path`.
///
/// The project is mandatory and matched by exact path: a delegation is never attributed to a
/// same-named agent of another project (AGENTS.md §1.11).
pub fn db_get_pending_delegation_by_target(target_agent: &str, target_project_path: &str) -> Result<Option<DelegationRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_pending_delegation_by_target: target_agent={}, target_project={}", target_agent, target_project_path));
    let conn = get_connection()?;
    let rec = conn
        .query_row(
            &format!(
                "SELECT {} FROM agent_delegations \
                 WHERE LOWER(target_agent) = LOWER(?1) AND target_project = ?2 AND status = 'pending' \
                 ORDER BY dispatched_at DESC LIMIT 1",
                DELEGATION_COLUMNS
            ),
            params![target_agent, target_project_path],
            delegation_from_row,
        )
        .optional()
        .db_context("get pending delegation by target")?;
    crate::logger::log("DEBUG", "db", &format!("db_get_pending_delegation_by_target: target_agent={}, found={}", target_agent, rec.is_some()));
    Ok(rec)
}

pub fn db_update_delegation_status(task_id: &str, status: &str, completed_at: Option<u64>, result_summary: Option<String>) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_update_delegation_status: task_id={}, status={}", task_id, status));
    let conn = get_connection()?;
    conn.execute(
        "UPDATE agent_delegations SET status = ?1, completed_at = ?2, result_summary = ?3 WHERE task_id = ?4",
        params![
            status,
            completed_at.map(|v| v as i64),
            result_summary,
            task_id
        ],
    ).db_context("update delegation status")?;
    crate::logger::log("INFO", "db", &format!("db_update_delegation_status: updated task '{}' status to '{}'", task_id, status));
    Ok(())
}

// --- Audit Journal Operations ---

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AuditJournalRecord {
    pub id: i64,
    pub project_id: i64,
    pub timestamp: String,
    pub originator: String,
    pub target: String,
    pub action_type: String,
    pub payload: String,
    pub metadata: Option<String>,
}

pub fn db_insert_audit_journal(
    project_id: i64,
    originator: &str,
    target: &str,
    action_type: &str,
    payload: &str,
    metadata: Option<&str>,
) -> Result<i64, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_insert_audit_journal: project_id={}, originator={}, target={}, action_type={}", project_id, originator, target, action_type));
    let conn = get_connection()?;
    conn.execute(
        "INSERT INTO audit_journal (project_id, originator, target, action_type, payload, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![project_id, originator, target, action_type, payload, metadata],
    ).db_context("insert audit journal entry")?;
    let id = conn.last_insert_rowid();
    crate::logger::log("INFO", "db", &format!("db_insert_audit_journal: inserted audit journal id {} (action_type={})", id, action_type));
    Ok(id)
}

pub fn db_get_audit_trail(
    project_id: i64,
    limit_count: usize,
    date_filter: Option<&str>,
) -> Result<Vec<AuditJournalRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_audit_trail: project_id={}, limit={}, date_filter={:?}", project_id, limit_count, date_filter));
    let conn = get_connection()?;
    let mut query = String::from(
        "SELECT id, project_id, timestamp, originator, target, action_type, payload, metadata 
         FROM audit_journal 
         WHERE project_id = ?1"
    );

    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(project_id)];

    if let Some(df) = date_filter {
        if !df.trim().is_empty() {
            query.push_str(&format!(" AND date(timestamp) = date(?{})", params_vec.len() + 1));
            params_vec.push(Box::new(df.to_string()));
        }
    }

    query.push_str(&format!(" ORDER BY id DESC LIMIT ?{}", params_vec.len() + 1));
    params_vec.push(Box::new(limit_count as i64));

    let mut stmt = conn.prepare(&query).db_context("prepare audit trail query")?;
    let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();

    let rows = stmt.query_map(rusqlite::params_from_iter(params_refs), |row| {
        Ok(AuditJournalRecord {
            id: row.get(0)?,
            project_id: row.get(1)?,
            timestamp: row.get(2)?,
            originator: row.get(3)?,
            target: row.get(4)?,
            action_type: row.get(5)?,
            payload: row.get(6)?,
            metadata: row.get(7)?,
        })
    }).db_context("query audit trail")?;
    let list = collect_rows(rows, "read audit trail row")?;
    crate::logger::log("DEBUG", "db", &format!("db_get_audit_trail: returned {} records for project_id {}", list.len(), project_id));
    Ok(list)
}

// --- Snapshot Operations ---

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SnapshotRecord {
    pub id: String,
    pub ref_path: String,
    pub project_id: i64,
    pub conversation_id: String,
    pub agent_id: String,
    pub label: String,
    pub files_changed: Option<String>,
    pub created_at: String,
    pub domain: String,
    pub asset_key: Option<String>,
}

pub fn db_insert_snapshot(record: &SnapshotRecord) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_insert_snapshot: id={}, project_id={}, domain={}, asset={:?}, label={}, ref_path={}", record.id, record.project_id, record.domain, record.asset_key, record.label, record.ref_path));
    let conn = get_connection()?;
    conn.execute(
        "INSERT INTO snapshots (id, ref_path, project_id, conversation_id, agent_id, label, files_changed, created_at, domain, asset_key)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            record.id,
            record.ref_path,
            record.project_id,
            record.conversation_id,
            record.agent_id,
            record.label,
            record.files_changed,
            record.created_at,
            record.domain,
            record.asset_key,
        ],
    ).db_context("insert snapshot record")?;
    crate::logger::log("INFO", "db", &format!("db_insert_snapshot: inserted snapshot '{}' ({})", record.id, record.label));
    Ok(())
}

pub fn db_list_snapshots(
    project_id: i64,
    conversation_id: Option<&str>,
    agent_id: Option<&str>,
    limit: usize,
) -> Result<Vec<SnapshotRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_list_snapshots: project_id={}, conversation_id={:?}, agent_id={:?}, limit={}", project_id, conversation_id, agent_id, limit));
    let conn = get_connection()?;
    let mut query = format!(
        "SELECT {}
         FROM snapshots
         WHERE project_id = ?1",
        SNAPSHOT_COLUMNS
    );

    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(project_id)];

    if let Some(cid) = conversation_id {
        if !cid.trim().is_empty() {
            query.push_str(&format!(" AND conversation_id = ?{}", params_vec.len() + 1));
            params_vec.push(Box::new(cid.to_string()));
        }
    }

    if let Some(aid) = agent_id {
        if !aid.trim().is_empty() {
            query.push_str(&format!(" AND agent_id = ?{}", params_vec.len() + 1));
            params_vec.push(Box::new(aid.to_string()));
        }
    }

    query.push_str(&format!(" ORDER BY created_at DESC, id DESC LIMIT ?{}", params_vec.len() + 1));
    params_vec.push(Box::new(limit as i64));

    let mut stmt = conn.prepare(&query).db_context("prepare snapshot list query")?;
    let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();

    let rows = stmt
        .query_map(rusqlite::params_from_iter(params_refs), snapshot_from_row)
        .db_context("query snapshots")?;
    let list = collect_rows(rows, "read snapshot row")?;
    crate::logger::log("DEBUG", "db", &format!("db_list_snapshots: returning {} snapshots for project_id {}", list.len(), project_id));
    Ok(list)
}

pub fn db_get_conversation_snapshots(conversation_id: &str) -> Result<Vec<SnapshotRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_conversation_snapshots: conversation_id={}", conversation_id));
    let conn = get_connection()?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM snapshots WHERE conversation_id = ?1 ORDER BY created_at DESC",
        SNAPSHOT_COLUMNS
    )).db_context("prepare conversation snapshots query")?;
    let rows = stmt.query_map(params![conversation_id], snapshot_from_row).db_context("query conversation snapshots")?;
    let list = collect_rows(rows, "read snapshot row")?;
    Ok(list)
}

const SNAPSHOT_COLUMNS: &str = "id, ref_path, project_id, conversation_id, agent_id, label, files_changed, created_at, domain, asset_key";

fn snapshot_from_row(row: &rusqlite::Row) -> rusqlite::Result<SnapshotRecord> {
    Ok(SnapshotRecord {
        id: row.get(0)?,
        ref_path: row.get(1)?,
        project_id: row.get(2)?,
        conversation_id: row.get(3)?,
        agent_id: row.get(4)?,
        label: row.get(5)?,
        files_changed: row.get(6)?,
        created_at: row.get(7)?,
        domain: row.get(8)?,
        asset_key: row.get(9)?,
    })
}

/// Lists snapshots for a `(project_id, domain, asset_key)` group ordered oldest first for retention pruning.
pub fn db_list_domain_asset_snapshots_oldest_first(
    project_id: i64,
    domain: &str,
    asset_key: Option<&str>,
) -> Result<Vec<SnapshotRecord>, DatabaseError> {
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {} FROM snapshots
             WHERE project_id = ?1 AND domain = ?2 AND COALESCE(asset_key, '') = COALESCE(?3, '')
             ORDER BY created_at ASC, id ASC",
            SNAPSHOT_COLUMNS
        ))
        .db_context("prepare domain asset snapshots query")?;
    let rows = stmt
        .query_map(params![project_id, domain, asset_key], snapshot_from_row)
        .db_context("query domain asset snapshots")?;
    collect_rows(rows, "read domain snapshot row")
}

/// Snapshots of a project whose `id` or `ref_path` equals `id_or_ref` exactly (at most two, so
/// the caller can tell a unique match from an ambiguous short id).
pub fn db_find_snapshots_by_id_or_ref(project_id: i64, id_or_ref: &str) -> Result<Vec<SnapshotRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_find_snapshots_by_id_or_ref: project_id={}, id_or_ref={}", project_id, id_or_ref));
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {} FROM snapshots WHERE project_id = ?1 AND (id = ?2 OR ref_path = ?2) LIMIT 2",
            SNAPSHOT_COLUMNS
        ))
        .db_context("prepare find snapshot")?;
    let rows = stmt.query_map(params![project_id, id_or_ref], snapshot_from_row).db_context("find snapshot")?;
    collect_rows(rows, "read snapshot row")
}

pub fn db_delete_snapshot(ref_path: &str) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_delete_snapshot: ref_path={}", ref_path));
    let conn = get_connection()?;
    conn.execute("DELETE FROM snapshots WHERE ref_path = ?1", params![ref_path])
        .db_context("delete snapshot")?;
    crate::logger::log("INFO", "db", &format!("db_delete_snapshot: deleted snapshot at ref_path '{}'", ref_path));
    Ok(())
}

pub fn db_count_snapshots(project_id: i64) -> Result<usize, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_count_snapshots: project_id={}", project_id));
    let conn = get_connection()?;
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM snapshots WHERE project_id = ?1", params![project_id], |row| row.get(0))
        .db_context("count snapshots")?;
    crate::logger::log("DEBUG", "db", &format!("db_count_snapshots: project_id={}, count={}", project_id, count));
    Ok(count as usize)
}

pub fn db_get_oldest_snapshot_conversations(project_id: i64, limit_count: usize) -> Result<Vec<String>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_oldest_snapshot_conversations: project_id={}, limit={}", project_id, limit_count));
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare("SELECT conversation_id FROM snapshots WHERE project_id = ?1 GROUP BY conversation_id ORDER BY MIN(created_at) ASC LIMIT ?2")
        .db_context("prepare oldest snapshot conversations")?;
    let rows = stmt
        .query_map(params![project_id, limit_count as i64], |row| row.get(0))
        .db_context("query oldest snapshot conversations")?;
    let list = collect_rows(rows, "read snapshot conversation row")?;
    crate::logger::log("DEBUG", "db", &format!("db_get_oldest_snapshot_conversations: returned {} conversations", list.len()));
    Ok(list)
}

// --- Account & Pending Resume Operations ---

fn default_account_provider() -> String {
    "google".to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AccountRecord {
    pub id: String,
    pub label: String,
    pub email: String,
    pub is_active: bool,
    pub enc_token: Vec<u8>,
    pub added_at: String,
    #[serde(default = "default_account_provider")]
    pub provider: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct AccountSummary {
    pub id: String,
    pub label: String,
    pub email: String,
    pub is_active: bool,
    pub added_at: String,
    #[serde(default = "default_account_provider")]
    pub provider: String,
}


pub fn db_insert_account(
    id: &str,
    label: &str,
    email: &str,
    is_active: bool,
    enc_token: &[u8],
    added_at: &str,
) -> Result<(), DatabaseError> {
    db_insert_account_with_provider(id, label, email, is_active, enc_token, added_at, "google")
}

pub fn db_insert_account_with_provider(
    id: &str,
    label: &str,
    email: &str,
    is_active: bool,
    enc_token: &[u8],
    added_at: &str,
    provider: &str,
) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_insert_account: id={}, email={}, provider={}, is_active={}", id, email, provider, is_active));
    let conn = get_connection()?;
    conn.execute(
        "INSERT INTO accounts (id, label, email, is_active, enc_token, added_at, provider)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(email) DO UPDATE SET
            label = excluded.label,
            enc_token = excluded.enc_token,
            provider = excluded.provider",
        params![
            id,
            label,
            email,
            if is_active { 1 } else { 0 },
            enc_token,
            added_at,
            provider,
        ],
    ).db_context("insert account")?;
    crate::logger::log("INFO", "db", &format!("db_insert_account: saved account '{}' ({})", email, provider));
    Ok(())
}

const ACCOUNT_COLUMNS: &str = "id, label, email, is_active, enc_token, added_at, provider";

fn account_from_row(row: &rusqlite::Row) -> rusqlite::Result<AccountRecord> {
    Ok(AccountRecord {
        id: row.get(0)?,
        label: row.get(1)?,
        email: row.get(2)?,
        is_active: row.get::<_, i64>(3)? == 1,
        enc_token: row.get(4)?,
        added_at: row.get(5)?,
        provider: row.get(6)?,
    })
}

/// Runs a single-row account query; `None` when no account matches.
fn query_optional_account(where_clause: &str, params: impl rusqlite::Params, op: &str) -> Result<Option<AccountRecord>, DatabaseError> {
    let conn = get_connection()?;
    conn.query_row(
        &format!("SELECT {} FROM accounts WHERE {} LIMIT 1", ACCOUNT_COLUMNS, where_clause),
        params,
        account_from_row,
    )
    .optional()
    .db_context(op)
}

pub fn db_get_accounts() -> Result<Vec<AccountRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", "db_get_accounts: fetching accounts");
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(&format!("SELECT {} FROM accounts ORDER BY added_at ASC", ACCOUNT_COLUMNS))
        .db_context("prepare list accounts")?;
    let rows = stmt.query_map([], account_from_row).db_context("query accounts")?;
    let list = collect_rows(rows, "read account row")?;
    crate::logger::log("DEBUG", "db", &format!("db_get_accounts: returning {} accounts", list.len()));
    Ok(list)
}

pub fn db_get_account_summaries() -> Result<Vec<AccountSummary>, DatabaseError> {
    crate::logger::log("DEBUG", "db", "db_get_account_summaries: fetching account summaries");
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare("SELECT id, label, email, is_active, added_at, provider FROM accounts ORDER BY added_at ASC")
        .db_context("prepare list account summaries")?;
    let rows = stmt
        .query_map([], |row| {
            Ok(AccountSummary {
                id: row.get(0)?,
                label: row.get(1)?,
                email: row.get(2)?,
                is_active: row.get::<_, i64>(3)? == 1,
                added_at: row.get(4)?,
                provider: row.get(5)?,
            })
        })
        .db_context("query account summaries")?;
    let list = collect_rows(rows, "read account summary row")?;
    crate::logger::log("DEBUG", "db", &format!("db_get_account_summaries: returning {} accounts", list.len()));
    Ok(list)
}

pub fn db_get_account_by_id(id: &str) -> Result<Option<AccountRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_account_by_id: id={}", id));
    query_optional_account("id = ?1", params![id], "get account by id")
}

pub fn db_get_account_by_email(email: &str) -> Result<Option<AccountRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_account_by_email: email={}", email));
    query_optional_account("email = ?1", params![email], "get account by email")
}

pub fn db_get_active_account_by_provider(provider: &str) -> Result<Option<AccountRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_active_account_by_provider: provider={}", provider));
    query_optional_account("is_active = 1 AND provider = ?1", params![provider], "get active account by provider")
}

/// Makes `target_id` the only active account of its provider, atomically.
pub fn db_set_active_account(target_id: &str) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_set_active_account: target_id={}", target_id));
    let mut conn = get_connection()?;
    let tx = conn.transaction().db_context("begin active account switch")?;
    let provider: String = tx
        .query_row("SELECT provider FROM accounts WHERE id = ?1", params![target_id], |row| row.get(0))
        .optional()
        .db_context("read account provider")?
        .ok_or_else(|| DatabaseError::NotFound { entity: "account", id: target_id.to_string() })?;
    tx.execute("UPDATE accounts SET is_active = 0 WHERE provider = ?1", params![provider])
        .db_context("reset active accounts for provider")?;
    tx.execute("UPDATE accounts SET is_active = 1 WHERE id = ?1", params![target_id])
        .db_context("set active account")?;
    tx.commit().db_context("commit active account switch")?;
    crate::logger::log("INFO", "db", &format!("db_set_active_account: activated account {} for provider {}", target_id, provider));
    Ok(())
}

pub fn db_update_account_label(id: &str, label: &str) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_update_account_label: id={}, label={}", id, label));
    let conn = get_connection()?;
    conn.execute("UPDATE accounts SET label = ?1 WHERE id = ?2", params![label, id])
        .db_context("update account label")?;
    crate::logger::log("INFO", "db", &format!("db_update_account_label: updated label for account {}", id));
    Ok(())
}

pub fn db_update_account_token(email: &str, enc_token: &[u8]) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_update_account_token: email={}", email));
    let conn = get_connection()?;
    conn.execute("UPDATE accounts SET enc_token = ?1 WHERE email = ?2", params![enc_token, email])
        .db_context("update account token")?;
    crate::logger::log("INFO", "db", &format!("db_update_account_token: updated token for {}", email));
    Ok(())
}

pub fn db_delete_account(id: &str) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_delete_account: id={}", id));
    let conn = get_connection()?;
    conn.execute("DELETE FROM accounts WHERE id = ?1", params![id])
        .db_context("delete account")?;
    crate::logger::log("INFO", "db", &format!("db_delete_account: deleted account {}", id));
    Ok(())
}


#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct PendingApprovalRecord {
    pub task_id: String,
    pub project_path: String,
    pub phase: String,
    pub payload_markdown: String,
    pub created_at: Option<String>,
    pub status: String,
}

pub fn db_insert_pending_approval(rec: &PendingApprovalRecord) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_insert_pending_approval: task_id={}, project_path={}, phase={}", rec.task_id, rec.project_path, rec.phase));
    let conn = get_connection()?;
    conn.execute(
        "INSERT INTO pending_approvals (task_id, project_path, phase, payload_markdown, status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP)
         ON CONFLICT(task_id) DO UPDATE SET
            project_path=excluded.project_path,
            phase=excluded.phase,
            payload_markdown=excluded.payload_markdown,
            status=excluded.status",
        (
            &rec.task_id,
            &rec.project_path,
            &rec.phase,
            &rec.payload_markdown,
            &rec.status,
        ),
    ).db_context("insert pending approval")?;
    crate::logger::log("INFO", "db", &format!("db_insert_pending_approval: recorded pending approval for task_id {}", rec.task_id));
    Ok(())
}

pub fn db_get_pending_approvals() -> Result<Vec<PendingApprovalRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", "db_get_pending_approvals: fetching pending approvals");
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(
            "SELECT task_id, project_path, phase, payload_markdown, created_at, status
             FROM pending_approvals WHERE status = 'pending' ORDER BY created_at DESC",
        )
        .db_context("prepare list pending approvals")?;
    let rows = stmt
        .query_map([], |row| {
            Ok(PendingApprovalRecord {
                task_id: row.get(0)?,
                project_path: row.get(1)?,
                phase: row.get(2)?,
                payload_markdown: row.get(3)?,
                created_at: row.get(4)?,
                status: row.get(5)?,
            })
        })
        .db_context("query pending approvals")?;
    let list = collect_rows(rows, "read pending approval row")?;
    crate::logger::log("DEBUG", "db", &format!("db_get_pending_approvals: returning {} approvals", list.len()));
    Ok(list)
}

pub fn db_delete_pending_approval(task_id: &str) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_delete_pending_approval: task_id={}", task_id));
    let conn = get_connection()?;
    conn.execute("DELETE FROM pending_approvals WHERE task_id = ?1", [task_id])
        .db_context("delete pending approval")?;
    crate::logger::log("INFO", "db", &format!("db_delete_pending_approval: deleted pending approval for task_id {}", task_id));
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Agent Topical Conversations & Turn Audit
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct AgentConversationRecord {
    pub id: String,
    #[ts(type = "number")]
    pub project_id: i64,
    pub agent_id: String,
    pub agent_uid: Option<String>,
    pub topic: String,
    pub status: String,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
    pub artifacts_touched: Option<String>,
    /// Override chosen for this Conversation; `None` inherits the persona's runner (Spec 30 §7).
    pub runner: Option<String>,
    pub model: Option<String>,
    pub account_id: Option<String>,
    pub effort: Option<String>,
    /// Set when the Conversation was closed (`status = 'archived'` by closure, not by a topic switch).
    #[ts(type = "number | null")]
    pub closed_at_ms: Option<i64>,
    /// `resolved`, `cleaned` or `superseded` when closed.
    pub close_reason: Option<String>,
    /// Latest runner-reported id among this Conversation's Native Sessions, if any.
    pub native_session_id: Option<String>,
    /// Agent-authored summary written through `record_session_minutes` (Spec 30 §6); `None` until recorded.
    pub summary: Option<String>,
    /// Id of the Conversation this one reopened (Spec 30 §8); `None` for ordinary Conversations.
    pub reopened_from: Option<String>,
}

/// `SELECT` prefix shared by every conversation query (aliased `c`); append a `WHERE` clause.
/// The Native Session id is the most recent one the runner reported for the Conversation.
const CONVERSATION_SELECT: &str = "SELECT c.id, c.project_id, c.agent_id, c.topic, c.status, c.created_at, c.updated_at, \
    c.artifacts_touched, c.runner, c.model, c.account_id, c.effort, c.closed_at_ms, c.close_reason, \
    (SELECT ns.native_session_id FROM conversation_native_sessions ns \
       WHERE ns.conversation_id = c.id AND ns.native_session_id IS NOT NULL ORDER BY ns.id DESC LIMIT 1), \
    c.summary, c.reopened_from, c.agent_uid \
    FROM agent_conversations c";

/// Maps a row produced by [`CONVERSATION_SELECT`].
fn conversation_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentConversationRecord> {
    Ok(AgentConversationRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        agent_id: row.get(2)?,
        topic: row.get(3)?,
        status: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        artifacts_touched: row.get(7)?,
        runner: row.get(8)?,
        model: row.get(9)?,
        account_id: row.get(10)?,
        effort: row.get(11)?,
        closed_at_ms: row.get(12)?,
        close_reason: row.get(13)?,
        native_session_id: row.get(14)?,
        summary: row.get(15)?,
        reopened_from: row.get(16)?,
        agent_uid: row.get(17)?,
    })
}

/// Returns the Conversation with this id, or `None` when it does not exist.
pub fn db_get_conversation_by_id(conversation_id: &str) -> Result<Option<AgentConversationRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_conversation_by_id: conversation_id={}", conversation_id));
    let conn = get_connection()?;
    conn.query_row(
        &format!("{} WHERE c.id = ?1", CONVERSATION_SELECT),
        params![conversation_id],
        conversation_from_row,
    )
    .optional()
    .db_context("query conversation by id")
}

/// Counts the user and agent turns of a Conversation (system turns such as the Readiness Stub excluded).
pub fn db_count_dialogue_turns(conversation_id: &str) -> Result<i64, DatabaseError> {
    let conn = get_connection()?;
    conn.query_row(
        "SELECT count(*) FROM agent_chat_turns WHERE conversation_id = ?1 AND role IN ('user', 'agent')",
        params![conversation_id],
        |row| row.get(0),
    )
    .db_context("count dialogue turns")
}

/// Minutes recorded for a Conversation, newest first, at most `limit` rows.
pub fn db_get_conversation_session_journals(conversation_id: &str, limit: usize) -> Result<Vec<SessionJournalRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_conversation_session_journals: conversation_id={}, limit={}", conversation_id, limit));
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, agent_id, agy_conversation_id, title, summary,
                    decisions, files_touched, technical_debt, tags, created_at, updated_at,
                    context_pct, total_tokens, conversation_id, milestone_id,
                    runner, model, cost_dollars, commit_hash, snapshot_ref
             FROM agent_session_journal
             WHERE conversation_id = ?1
             ORDER BY id DESC LIMIT ?2",
        )
        .db_context("prepare conversation journals query")?;
    let rows = stmt
        .query_map(params![conversation_id, limit as i64], |row| {
            Ok(SessionJournalRecord {
                id: Some(row.get(0)?),
                project_id: row.get(1)?,
                agent_id: row.get(2)?,
                agy_conversation_id: row.get(3)?,
                title: row.get(4)?,
                summary: row.get(5)?,
                decisions: row.get(6)?,
                files_touched: row.get(7)?,
                technical_debt: row.get(8)?,
                tags: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
                context_pct: row.get(12)?,
                total_tokens: row.get(13)?,
                conversation_id: row.get(14)?,
                milestone_id: row.get(15)?,
                runner: row.get(16)?,
                model: row.get(17)?,
                cost_dollars: row.get(18)?,
                commit_hash: row.get(19)?,
                snapshot_ref: row.get(20)?,
            })
        })
        .db_context("query conversation journals")?;
    rows.collect::<rusqlite::Result<Vec<_>>>().db_context("read conversation journals")
}

/// Marks a Native Session as started from a Brief rather than a resume (Spec 30 §3).
pub fn db_mark_native_session_seeded(row_id: i64) -> Result<(), DatabaseError> {
    let conn = get_connection()?;
    conn.execute("UPDATE conversation_native_sessions SET seeded_from_brief = 1 WHERE id = ?1", params![row_id])
        .db_context("mark native session seeded")?;
    Ok(())
}

/// Stores the agent-authored summary of a Conversation (Spec 30 §6), replacing any earlier one.
///
/// Fails when the Conversation does not exist so a summary is never silently dropped.
pub fn db_set_conversation_summary(conversation_id: &str, summary: &str) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_set_conversation_summary: conversation_id={}, summary_bytes={}", conversation_id, summary.len()));
    let conn = get_connection()?;
    let changed = conn
        .execute(
            "UPDATE agent_conversations SET summary = ?1, updated_at = ?2 WHERE id = ?3",
            params![summary, now_epoch_secs(), conversation_id],
        )
        .db_context("set conversation summary")?;
    if changed == 0 {
        return Err(DatabaseError::NotFound { entity: "Conversation", id: conversation_id.to_string() });
    }
    reindex_conversation(&conn, conversation_id).db_context("reindex conversation after summary")?;
    Ok(())
}

/// Rebuilds the full-text row of one Conversation from its topic, summary and Minutes (Spec 30 §8).
///
/// The table holds one row per Conversation, so a reindex is delete + insert. Callers invoke it
/// after every write that changes searchable text.
fn reindex_conversation(conn: &rusqlite::Connection, conversation_id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM conversation_search WHERE conversation_id = ?1", params![conversation_id])?;
    conn.execute(
        "INSERT INTO conversation_search (conversation_id, project_id, agent_id, content)
         SELECT c.id, c.project_id, c.agent_id,
                c.topic || ' ' || coalesce(c.summary, '') || ' ' || coalesce(
                    (SELECT group_concat(j.title || ' ' || j.summary || ' ' || coalesce(j.decisions, '') || ' ' || coalesce(j.tags, ''), ' ')
                       FROM agent_session_journal j WHERE j.conversation_id = c.id), '')
         FROM agent_conversations c WHERE c.id = ?1",
        params![conversation_id],
    )?;
    Ok(())
}

/// Indexes every Conversation when the full-text table is empty (first start after it was added).
fn backfill_conversation_search(conn: &rusqlite::Connection) -> Result<(), DatabaseError> {
    let indexed: i64 = conn
        .query_row("SELECT count(*) FROM conversation_search", [], |row| row.get(0))
        .db_context("count indexed conversations")?;
    if indexed > 0 {
        return Ok(());
    }
    let ids: Vec<String> = conn
        .prepare("SELECT id FROM agent_conversations")
        .db_context("prepare conversation id scan")?
        .query_map([], |row| row.get(0))
        .db_context("scan conversation ids")?
        .collect::<rusqlite::Result<_>>()
        .db_context("read conversation ids")?;
    for id in &ids {
        reindex_conversation(conn, id).db_context("backfill conversation index")?;
    }
    if !ids.is_empty() {
        crate::logger::log("INFO", "db", &format!("backfill_conversation_search: indexed {} conversations", ids.len()));
    }
    Ok(())
}

/// Turns free text into an FTS5 query: every word becomes a quoted term, terms are OR-ed and
/// ranked by relevance. Quoting keeps user input from being parsed as FTS5 syntax.
fn fts_query_from_text(text: &str) -> Option<String> {
    let terms: Vec<String> = text
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" OR "))
    }
}

/// Searches an agent's own Conversations by topic, summary and Minutes, best match first.
///
/// `status` is `active`, `archived` or `None` for both. The search is scoped to one project and
/// agent; `limit` bounds the result size. An empty query is an error, not "everything".
pub fn db_search_conversations(
    project_id: i64,
    agent_id: &str,
    query: &str,
    status: Option<&str>,
    limit: usize,
) -> Result<Vec<AgentConversationRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_search_conversations: project_id={}, agent_id={}, query_len={}, status={:?}, limit={}", project_id, agent_id, query.len(), status, limit));
    let fts = fts_query_from_text(query)
        .ok_or_else(|| DatabaseError::InvalidRequest("Search query has no searchable words".to_string()))?;
    if let Some(s) = status {
        if s != "active" && s != "archived" {
            return Err(DatabaseError::InvalidRequest(format!("Unknown conversation status '{}': use 'active' or 'archived'", s)));
        }
    }
    let conn = get_connection()?;
    search_conversations_on(&conn, project_id, agent_id, &fts, status, limit)
}

/// Searches conversations across a project (optionally filtered by `agent_id`) via FTS5.
pub fn db_search_project_conversations(
    project_id: i64,
    agent_id: Option<&str>,
    query: &str,
    status: Option<&str>,
    limit: usize,
) -> Result<Vec<AgentConversationRecord>, DatabaseError> {
    if let Some(aid) = agent_id.map(str::trim).filter(|s| !s.is_empty()) {
        return db_search_conversations(project_id, aid, query, status, limit);
    }
    let fts = fts_query_from_text(query)
        .ok_or_else(|| DatabaseError::InvalidRequest("Search query has no searchable words".to_string()))?;
    if let Some(s) = status {
        if s != "active" && s != "archived" {
            return Err(DatabaseError::InvalidRequest(format!("Unknown conversation status '{}': use 'active' or 'archived'", s)));
        }
    }
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(&format!(
            "{} JOIN conversation_search ON conversation_search.conversation_id = c.id \
             WHERE conversation_search MATCH ?1 AND c.project_id = ?2 \
               AND (?3 IS NULL OR c.status = ?3) \
             ORDER BY conversation_search.rank LIMIT ?4",
            CONVERSATION_SELECT
        ))
        .db_context("prepare project conversation search")?;
    let rows = stmt
        .query_map(params![fts, project_id, status, limit as i64], conversation_from_row)
        .db_context("run project conversation search")?;
    rows.collect::<rusqlite::Result<Vec<_>>>().db_context("read project conversation search")
}

/// Runs the scoped full-text query on `conn` (split out so it can be tested on a temporary database).
fn search_conversations_on(
    conn: &rusqlite::Connection,
    project_id: i64,
    agent_id: &str,
    fts: &str,
    status: Option<&str>,
    limit: usize,
) -> Result<Vec<AgentConversationRecord>, DatabaseError> {
    let mut stmt = conn
        .prepare(&format!(
            "{} JOIN conversation_search ON conversation_search.conversation_id = c.id \
             WHERE conversation_search MATCH ?1 AND c.project_id = ?2 AND c.agent_id = ?3 \
               AND (?4 IS NULL OR c.status = ?4) \
             ORDER BY conversation_search.rank LIMIT ?5",
            CONVERSATION_SELECT
        ))
        .db_context("prepare conversation search")?;
    let rows = stmt
        .query_map(params![fts, project_id, agent_id, status, limit as i64], conversation_from_row)
        .db_context("run conversation search")?;
    rows.collect::<rusqlite::Result<Vec<_>>>().db_context("read conversation search")
}

/// Lists the most recent Conversations of an agent in a project, optionally filtered by `status`
/// (`"active"` or `"archived"`), bounded by `limit` and ordered by `updated_at DESC`.
pub fn db_list_recent_conversations(
    project_id: i64,
    agent_id: &str,
    status: Option<&str>,
    limit: usize,
) -> Result<Vec<AgentConversationRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_list_recent_conversations: project_id={}, agent_id={}, status={:?}, limit={}", project_id, agent_id, status, limit));
    if let Some(s) = status {
        if s != "active" && s != "archived" {
            return Err(DatabaseError::InvalidRequest(format!("Unknown conversation status '{}': use 'active' or 'archived'", s)));
        }
    }
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(&format!(
            "{} WHERE c.project_id = ?1 AND c.agent_id = ?2 \
               AND (?3 IS NULL OR c.status = ?3) \
             ORDER BY c.updated_at DESC LIMIT ?4",
            CONVERSATION_SELECT
        ))
        .db_context("prepare recent conversations query")?;
    let rows = stmt
        .query_map(params![project_id, agent_id, status, limit as i64], conversation_from_row)
        .db_context("run recent conversations query")?;
    rows.collect::<rusqlite::Result<Vec<_>>>().db_context("read recent conversations")
}

/// Opens a new active Conversation that reopens `source_conversation_id` (Spec 30 §8).
///
/// The source must belong to the same project and agent, must not be the active Conversation, and
/// stays untouched. The Conversation in progress is closed as `cleaned`; the new one takes its
/// overrides (the configuration the live process runs with) and records `reopened_from`.
pub fn db_reopen_conversation(
    project_id: i64,
    agent_id: &str,
    source_conversation_id: &str,
) -> Result<AgentConversationRecord, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_reopen_conversation: project_id={}, agent_id={}, source={}", project_id, agent_id, source_conversation_id));
    let source = db_get_conversation_by_id(source_conversation_id)?
        .ok_or_else(|| DatabaseError::NotFound { entity: "Conversation", id: source_conversation_id.to_string() })?;
    if source.project_id != project_id || !source.agent_id.eq_ignore_ascii_case(agent_id) {
        return Err(DatabaseError::InvalidRequest(format!(
            "Conversation '{}' does not belong to agent '{}' of this project", source_conversation_id, agent_id
        )));
    }
    let current = db_get_or_create_active_conversation(project_id, agent_id, None, None, None, None, None)?;
    if current.id == source.id {
        return Err(DatabaseError::InvalidRequest(format!("Conversation '{}' is already the active one", source.id)));
    }
    let reopened = db_create_agent_conversation(
        project_id,
        agent_id,
        &source.topic,
        current.runner.as_deref(),
        current.model.as_deref(),
        current.account_id.as_deref(),
        current.effort.as_deref(),
    )?;
    let conn = get_connection()?;
    conn.execute(
        "UPDATE agent_conversations SET reopened_from = ?1 WHERE id = ?2",
        params![source.id, reopened.id],
    )
    .db_context("link reopened conversation")?;
    crate::logger::log("INFO", "db", &format!("db_reopen_conversation: {} reopened as {}", source.id, reopened.id));
    Ok(AgentConversationRecord { reopened_from: Some(source.id), ..reopened })
}

#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct ChatTurnRecord {
    #[ts(type = "number | null")]
    pub id: Option<i64>,
    pub conversation_id: String,
    #[ts(type = "number")]
    pub project_id: i64,
    pub agent_id: String,
    pub agy_conversation_id: Option<String>,
    pub role: String,
    pub dialogue: String,
    pub artifact_type: Option<String>,
    pub artifact_lang: Option<String>,
    pub artifact_label: Option<String>,
    pub artifact_file_path: Option<String>,
    pub artifact_content: Option<String>,
    pub is_ephemeral: bool,
    pub terminal_snapshot: Option<String>,
    #[ts(type = "number")]
    pub timestamp: i64,
    /// Populated only by `db_get_agent_turns_continuous`; null in standard conversation queries.
    /// Contains the `topic` from the `agent_conversations` record this turn belongs to.
    pub topic: Option<String>,
    pub approval_payload: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct SessionJournalRecord {
    #[ts(type = "number | null")]
    pub id: Option<i64>,
    #[ts(type = "number")]
    pub project_id: i64,
    pub agent_id: String,
    /// ONE-SHIP Conversation these Minutes belong to; `None` only for rows predating Spec 30 Phase D.
    pub conversation_id: Option<String>,
    /// Native reference (an `agy` session id) when the runner reported one.
    pub agy_conversation_id: Option<String>,
    pub title: String,
    pub summary: String,
    pub decisions: Option<String>,
    pub files_touched: Option<String>,
    pub technical_debt: Option<String>,
    pub tags: Option<String>,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
    pub context_pct: Option<f64>,
    #[ts(type = "number | null")]
    pub total_tokens: Option<i64>,
    #[ts(type = "number | null")]
    pub milestone_id: Option<i64>,
    pub runner: Option<String>,
    pub model: Option<String>,
    pub cost_dollars: Option<f64>,
    /// Git commit hash created automatically when `vcs.minutes_checkpoint_mode = "commit"`.
    pub commit_hash: Option<String>,
    /// Git snapshot ref created automatically when `vcs.minutes_checkpoint_mode = "snapshot"`.
    pub snapshot_ref: Option<String>,
}

/// Aggregated consumption per `(runner, model)` across recorded session minutes.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct RunnerModelUsageSummary {
    pub runner: String,
    pub model: String,
    #[ts(type = "number")]
    pub total_tokens_count: i64,
    pub total_cost_dollars: f64,
    #[ts(type = "number")]
    pub minutes_count: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct AgentSessionMetricsRecord {
    pub agy_conversation_id: String,
    pub conversation_id: String,
    #[ts(type = "number")]
    pub project_id: i64,
    pub agent_id: String,
    pub provider: String,
    pub model: Option<String>,
    pub context_used_pct: f64,
    #[ts(type = "number")]
    pub fresh_tokens_count: i64,
    #[ts(type = "number")]
    pub cache_read_tokens_count: i64,
    #[ts(type = "number")]
    pub cache_creation_tokens_count: i64,
    #[ts(type = "number")]
    pub total_tokens_count: i64,
    pub cache_hit_pct: f64,
    pub five_hour_used_pct: Option<f64>,
    pub five_hour_resets_at: Option<String>,
    #[ts(type = "number | null")]
    pub five_hour_reset_epoch_seconds: Option<i64>,
    pub weekly_used_pct: Option<f64>,
    pub weekly_resets_at: Option<String>,
    #[ts(type = "number | null")]
    pub weekly_reset_epoch_seconds: Option<i64>,
    #[ts(type = "number")]
    pub updated_at: i64,
    pub runner: Option<String>,
    pub cost_dollars: Option<f64>,
    #[ts(type = "number | null")]
    pub max_context_tokens_count: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct AccountQuotaRecord {
    #[ts(type = "number | null")]
    pub id: Option<i64>,
    pub account_id: String,
    pub provider: String,
    pub model: String,
    pub five_hour_remaining_pct: Option<f64>,
    pub five_hour_resets_at: Option<String>,
    #[ts(type = "number | null")]
    pub five_hour_reset_epoch_seconds: Option<i64>,
    pub five_hour_resets_at_local: Option<String>,
    pub weekly_remaining_pct: Option<f64>,
    pub weekly_resets_at: Option<String>,
    #[ts(type = "number | null")]
    pub weekly_reset_epoch_seconds: Option<i64>,
    pub weekly_resets_at_local: Option<String>,
    #[ts(type = "number | null")]
    pub last_pinged_at: Option<i64>,
    pub warmup_enabled: bool,
    pub status: String,
    #[ts(type = "number")]
    pub updated_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct ProjectTodoRecord {
    #[ts(type = "number | null")]
    pub id: Option<i64>,
    #[ts(type = "number")]
    pub project_id: i64,
    pub agent_id: Option<String>,
    pub agy_conversation_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub category: String,
    pub status: String,
    pub priority: i32,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number | null")]
    pub completed_at: Option<i64>,
    pub conversation_id: Option<String>,
    pub file_path: Option<String>,
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
    #[ts(type = "number | null")]
    pub journal_id: Option<i64>,
}

/// Customizable Kanban workflow stage belonging to a single project (Spec 34).
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct ProjectTaskStageRecord {
    #[ts(type = "number")]
    pub id: i64,
    #[ts(type = "number")]
    pub project_id: i64,
    pub slug: String,
    pub label: String,
    pub sort_order: i32,
    pub is_collapsed: bool,
    pub is_terminal: bool,
    #[ts(type = "number")]
    pub created_at_seconds: i64,
    #[ts(type = "number")]
    pub updated_at_seconds: i64,
}

/// Result of attempting to delete a [`ProjectTaskStageRecord`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteStageOutcome {
    Deleted { project_id: i64 },
    HasTasks { tasks_count: usize },
    LastStage,
}

/// Strategic project milestone (Spec 34 §2.5 / §7) supporting `linear`, `shape_up`, and `weighted` modes.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct ProjectMilestoneRecord {
    #[ts(type = "number")]
    pub id: i64,
    #[ts(type = "number")]
    pub project_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub weight_percent: f64,
    pub hill_position_percent: i32,
    pub appetite_days: Option<i32>,
    #[ts(type = "number | null")]
    pub planned_start_epoch_seconds: Option<i64>,
    #[ts(type = "number | null")]
    pub planned_due_epoch_seconds: Option<i64>,
    pub sort_order: i32,
    #[ts(type = "number")]
    pub created_at_seconds: i64,
    #[ts(type = "number")]
    pub updated_at_seconds: i64,
    #[ts(type = "number | null")]
    pub completed_at_seconds: Option<i64>,
}

/// High-level multidisciplinary project planning task (Spec 34), decoupled from `ProjectTodoRecord`.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct ProjectTaskRecord {
    #[ts(type = "number")]
    pub id: i64,
    #[ts(type = "number")]
    pub project_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub domain: String,
    pub status: String,
    pub priority: i32,
    #[ts(type = "number | null")]
    pub milestone_id: Option<i64>,
    pub weight_points: i32,
    #[ts(type = "number | null")]
    pub start_epoch_seconds: Option<i64>,
    #[ts(type = "number | null")]
    pub due_epoch_seconds: Option<i64>,
    pub github_repo: Option<String>,
    #[ts(type = "number | null")]
    pub github_issue_number: Option<i64>,
    #[ts(type = "number | null")]
    pub github_pr_number: Option<i64>,
    pub branch_name: Option<String>,
    #[ts(type = "number")]
    pub created_at_seconds: i64,
    #[ts(type = "number")]
    pub updated_at_seconds: i64,
    #[ts(type = "number | null")]
    pub completed_at_seconds: Option<i64>,
}

/// Internal checklist step inside a [`ProjectTaskRecord`].
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct ProjectTaskSubtaskRecord {
    #[ts(type = "number")]
    pub id: i64,
    #[ts(type = "number")]
    pub task_id: i64,
    pub title: String,
    pub is_completed: bool,
    pub sort_order: i32,
    #[ts(type = "number | null")]
    pub linked_todo_id: Option<i64>,
    #[ts(type = "number")]
    pub created_at_seconds: i64,
}

/// Explicit relational bridge between a [`ProjectTaskRecord`] and a `Journal` entity (`todo` or `journal`).
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct ProjectTaskLinkRecord {
    #[ts(type = "number")]
    pub id: i64,
    #[ts(type = "number")]
    pub task_id: i64,
    pub link_type: String,
    #[ts(type = "number")]
    pub target_id: i64,
    #[ts(type = "number")]
    pub created_at_seconds: i64,
}

/// Hydrated [`ProjectTaskRecord`] bundled with its subtasks and `Journal` links.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct ProjectTaskBundle {
    pub task: ProjectTaskRecord,
    pub subtasks: Vec<ProjectTaskSubtaskRecord>,
    pub links: Vec<ProjectTaskLinkRecord>,
}

fn default_tech_debt_category() -> String {
    "architectural".to_string()
}

/// An initialization flag to raise; [`db_insert_init_flag`] stores it `pending`, created now.
#[derive(Clone, Debug)]
pub struct NewInitFlag {
    pub project_id: i64,
    pub flag_type: String,
    pub title: String,
    pub description: Option<String>,
    pub recommended_option: Option<String>,
    pub options_json: Option<String>,
    pub metadata_json: Option<String>,
    pub raised_by_agent_uid: Option<String>,
}

/// A stored initialization flag.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct ProjectInitFlagRecord {
    #[ts(type = "number")]
    pub id: i64,
    #[ts(type = "number")]
    pub project_id: i64,
    pub flag_type: String,
    pub title: String,
    pub description: Option<String>,
    pub recommended_option: Option<String>,
    pub options_json: Option<String>,
    pub metadata_json: Option<String>,
    pub status: String,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number | null")]
    pub resolved_at: Option<i64>,
    pub raised_by_agent_uid: Option<String>,
    pub resolved_by_agent_uid: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct ProjectTechnicalDebtRecord {
    #[ts(type = "number | null")]
    pub id: Option<i64>,
    #[ts(type = "number")]
    pub project_id: i64,
    pub agy_conversation_id: Option<String>,
    pub title: String,
    #[serde(default = "default_tech_debt_category")]
    pub category: String,
    pub impact_summary: String,
    pub remediation_plan: Option<String>,
    pub status: String,
    pub severity: String,
    #[ts(type = "number")]
    pub identified_at: i64,
    #[ts(type = "number | null")]
    pub resolved_at: Option<i64>,
    pub conversation_id: Option<String>,
    #[ts(type = "number | null")]
    pub milestone_id: Option<i64>,
    pub file_path: Option<String>,
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
    #[ts(type = "number | null")]
    pub journal_id: Option<i64>,
}

/// One run of a supervised service or task: a `supervised_processes` row as the run history shows
/// it. The run is also the unit of the continuous profile (`duck::get_run_profile`, Spec 25 §3.4).
#[derive(Serialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct TargetRunRecord {
    pub run_id: String,
    #[ts(type = "number")]
    pub project_id: i64,
    /// `service` or `task`.
    pub target_type: String,
    pub target_id: String,
    pub status: String,
    pub exit_code: Option<i32>,
    #[ts(type = "number")]
    pub spawned_at_ms: i64,
    #[ts(type = "number | null")]
    pub terminated_at_ms: Option<i64>,
    pub failure_reason: Option<String>,
    pub profiler_engine_id: Option<String>,
    pub profile_artifact_path: Option<String>,
}

pub fn now_epoch_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn db_get_or_create_active_conversation(
    project_id: i64,
    agent_id: &str,
    default_topic: Option<&str>,
    runner: Option<&str>,
    model: Option<&str>,
    account_id: Option<&str>,
    effort: Option<&str>,
) -> Result<AgentConversationRecord, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_or_create_active_conversation: project_id={}, agent_id={}, default_topic={:?}, runner={:?}, effort={:?}", project_id, agent_id, default_topic, runner, effort));
    let conn = get_connection()?;
    let mut stmt = conn.prepare(&format!(
        "{} WHERE c.project_id = ?1 AND c.agent_id = ?2 AND c.status = 'active' ORDER BY c.updated_at DESC LIMIT 1",
        CONVERSATION_SELECT
    )).db_context("prepare active conversation query")?;

    let found = stmt
        .query_row(params![project_id, agent_id], conversation_from_row)
        .optional()
        .db_context("query active conversation")?;

    if let Some(conv) = found {
        crate::logger::log("DEBUG", "db", &format!("db_get_or_create_active_conversation: found active conversation id={}", conv.id));
        return Ok(conv);
    }

    // None active -> create fresh default conversation
    let topic_name = default_topic.unwrap_or("General Workspace");
    crate::logger::log("INFO", "db", &format!("db_get_or_create_active_conversation: no active conversation found, creating default with topic={}", topic_name));
    db_create_agent_conversation(project_id, agent_id, topic_name, runner, model, account_id, effort)
}

pub fn db_get_agent_conversations(
    project_id: i64,
    agent_id: &str,
) -> Result<Vec<AgentConversationRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_agent_conversations: project_id={}, agent_id={}", project_id, agent_id));
    let conn = get_connection()?;
    let mut stmt = conn.prepare(&format!(
        "{} WHERE c.project_id = ?1 AND c.agent_id = ?2 ORDER BY c.updated_at DESC",
        CONVERSATION_SELECT
    )).db_context("prepare conversation list query")?;

    let list = stmt
        .query_map(params![project_id, agent_id], conversation_from_row)
        .db_context("query conversation list")?
        .collect::<rusqlite::Result<Vec<AgentConversationRecord>>>()
        .db_context("read conversation list")?;
    crate::logger::log("DEBUG", "db", &format!("db_get_agent_conversations: returning {} conversations", list.len()));
    Ok(list)
}

pub fn db_create_agent_conversation(
    project_id: i64,
    agent_id: &str,
    topic: &str,
    runner: Option<&str>,
    model: Option<&str>,
    account_id: Option<&str>,
    effort: Option<&str>,
) -> Result<AgentConversationRecord, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_create_agent_conversation: project_id={}, agent_id={}, topic={}, runner={:?}, model={:?}, effort={:?}", project_id, agent_id, topic, runner, model, effort));
    let conn = get_connection()?;
    let now = now_epoch_secs();
    let short_uuid: String = uuid::Uuid::new_v4().to_string().chars().take(8).collect();
    let conv_id = format!("conv-{}-{}-{}", project_id, agent_id, short_uuid);

    // Starting a new Conversation closes the one in progress (the user cleaned the context).
    close_active_conversations(&conn, project_id, agent_id, "cleaned").db_context("close previous conversation")?;

    let agent_uid = db_get_or_create_agent_persona_conn(&conn, project_id, agent_id)?;

    // Overrides are stored exactly as given: None stays NULL and inherits the persona.
    conn.execute(
        "INSERT INTO agent_conversations (id, project_id, agent_id, agent_uid, topic, status, created_at, updated_at, artifacts_touched, runner, model, account_id, effort)
         VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?6, '[]', ?7, ?8, ?9, ?10)",
        params![conv_id, project_id, agent_id, agent_uid, topic, now, runner, model, account_id, effort],
    ).db_context("create conversation")?;
    reindex_conversation(&conn, &conv_id).db_context("index new conversation")?;

    crate::logger::log("INFO", "db", &format!("db_create_agent_conversation: created conversation id={} for agent={} (uid={}, runner override: {:?}, effort: {:?})", conv_id, agent_id, agent_uid, runner, effort));
    Ok(AgentConversationRecord {
        id: conv_id,
        project_id,
        agent_id: agent_id.to_string(),
        agent_uid: Some(agent_uid),
        topic: topic.to_string(),
        status: "active".to_string(),
        created_at: now,
        updated_at: now,
        artifacts_touched: Some("[]".to_string()),
        runner: runner.map(String::from),
        model: model.map(String::from),
        account_id: account_id.map(String::from),
        effort: effort.map(String::from),
        closed_at_ms: None,
        close_reason: None,
        native_session_id: None,
        summary: None,
        reopened_from: None,
    })
}

/// Maps a Conversation close reason to the end reason of its Native Sessions.
fn native_end_reason_for(close_reason: &str) -> &'static str {
    match close_reason {
        "superseded" => "runner_switch",
        "cleaned" => "cleaned",
        _ => "exit",
    }
}

/// Closes every active Conversation of an agent: `status = 'archived'`, `closed_at_ms`,
/// `close_reason`, and ends their open Native Sessions. Returns the ids that were closed.
fn close_active_conversations(
    conn: &rusqlite::Connection,
    project_id: i64,
    agent_id: &str,
    close_reason: &str,
) -> rusqlite::Result<Vec<String>> {
    debug_assert!(matches!(close_reason, "resolved" | "cleaned" | "superseded"), "unknown close reason");
    let mut stmt = conn.prepare(
        "SELECT id FROM agent_conversations WHERE project_id = ?1 AND agent_id = ?2 AND status = 'active'",
    )?;
    let ids = stmt
        .query_map(params![project_id, agent_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    let now_ms = now_epoch_secs() * 1000;
    for id in &ids {
        end_open_native_sessions(conn, id, native_end_reason_for(close_reason), now_ms)?;
        conn.execute(
            "UPDATE agent_conversations SET status = 'archived', closed_at_ms = ?1, close_reason = ?2 WHERE id = ?3",
            params![now_ms, close_reason, id],
        )?;
    }
    Ok(ids)
}

/// Closes the agent's active Conversation (see [`close_active_conversations`]). Used when the
/// runner changes: the context of one runner is never silently carried into another.
pub fn db_close_active_conversation(project_id: i64, agent_id: &str, close_reason: &str) -> Result<Vec<String>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_close_active_conversation: project_id={}, agent_id={}, reason={}", project_id, agent_id, close_reason));
    let conn = get_connection()?;
    let closed = close_active_conversations(&conn, project_id, agent_id, close_reason).db_context("close active conversation")?;
    if !closed.is_empty() {
        crate::logger::log("INFO", "db", &format!("db_close_active_conversation: closed {:?} ({})", closed, close_reason));
    }
    Ok(closed)
}

fn end_open_native_sessions(
    conn: &rusqlite::Connection,
    conversation_id: &str,
    end_reason: &str,
    now_ms: i64,
) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE conversation_native_sessions SET ended_at_ms = ?1, end_reason = ?2 WHERE conversation_id = ?3 AND ended_at_ms IS NULL",
        params![now_ms, end_reason, conversation_id],
    )
}

/// Opens a Native Session for a Conversation about to run on `runner`. Any session still open
/// belongs to a process that is gone, so it is ended with reason `exit` first. Returns the row id.
pub fn db_open_native_session(
    conversation_id: &str,
    runner: &str,
    model: Option<&str>,
    account_id: Option<&str>,
) -> Result<i64, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_open_native_session: conversation_id={}, runner={}", conversation_id, runner));
    let mut conn = get_connection()?;
    let now_ms = now_epoch_secs() * 1000;
    let tx = conn.transaction().db_context("begin native session open")?;
    end_open_native_sessions(&tx, conversation_id, "exit", now_ms).db_context("end previous native sessions")?;
    tx.execute(
        "INSERT INTO conversation_native_sessions (conversation_id, runner, model, account_id, started_at_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![conversation_id, runner, model, account_id, now_ms],
    ).db_context("insert native session")?;
    let row_id = tx.last_insert_rowid();
    tx.commit().db_context("commit native session open")?;
    crate::logger::log("INFO", "db", &format!("db_open_native_session: opened native session {} for conversation {} ({})", row_id, conversation_id, runner));
    Ok(row_id)
}

/// Returns the Conversation's open Native Session, opening one on `runner` if none is open.
/// For agents spawned outside `launch_agent` (initializer, flag resolvers) that still report a
/// runner session id.
pub fn db_ensure_open_native_session(conversation_id: &str, runner: &str) -> Result<i64, DatabaseError> {
    let existing: Option<i64> = {
        let conn = get_connection()?;
        conn.query_row(
            "SELECT id FROM conversation_native_sessions WHERE conversation_id = ?1 AND ended_at_ms IS NULL ORDER BY id DESC LIMIT 1",
            params![conversation_id],
            |row| row.get(0),
        )
        .optional()
        .db_context("look up open native session")?
    };
    match existing {
        Some(row_id) => Ok(row_id),
        None => db_open_native_session(conversation_id, runner, None, None),
    }
}

/// Ends one Native Session (for example when its process failed to start).
pub fn db_end_native_session(row_id: i64, end_reason: &str) -> Result<(), DatabaseError> {
    let conn = get_connection()?;
    conn.execute(
        "UPDATE conversation_native_sessions SET ended_at_ms = ?1, end_reason = ?2 WHERE id = ?3 AND ended_at_ms IS NULL",
        params![now_epoch_secs() * 1000, end_reason, row_id],
    ).db_context("end native session")?;
    Ok(())
}

pub fn db_switch_active_conversation(
    project_id: i64,
    agent_id: &str,
    conversation_id: &str,
) -> Result<AgentConversationRecord, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_switch_active_conversation: project_id={}, agent_id={}, conversation_id={}", project_id, agent_id, conversation_id));
    let mut conn = get_connection()?;
    let now = now_epoch_secs();

    // Archive the agent's other Conversations and activate the target in one transaction.
    let tx = conn.transaction().db_context("begin conversation switch")?;
    tx.execute(
        "UPDATE agent_conversations SET status = 'archived' WHERE project_id = ?1 AND agent_id = ?2",
        params![project_id, agent_id],
    ).db_context("archive other conversations")?;
    let activated = tx.execute(
        "UPDATE agent_conversations SET status = 'active', updated_at = ?1 WHERE id = ?2 AND project_id = ?3 AND agent_id = ?4",
        params![now, conversation_id, project_id, agent_id],
    ).db_context("switch active conversation")?;
    if activated == 0 {
        return Err(DatabaseError::NotFound { entity: "Conversation", id: conversation_id.to_string() });
    }
    tx.commit().db_context("commit conversation switch")?;

    let rec = conn
        .query_row(&format!("{} WHERE c.id = ?1", CONVERSATION_SELECT), params![conversation_id], conversation_from_row)
        .db_context("read switched conversation")?;
    crate::logger::log("INFO", "db", &format!("db_switch_active_conversation: switched active conversation to {}", conversation_id));
    Ok(rec)
}

/// Records the identifier the runner reported for its session on the Conversation's open Native
/// Session. Fails if no Native Session is open: `launch_agent` opens one before spawning.
pub fn db_record_native_session_id(
    conversation_id: &str,
    native_session_id: &str,
) -> Result<(), DatabaseError> {
    let conn = get_connection()?;
    let updated = conn.execute(
        "UPDATE conversation_native_sessions SET native_session_id = ?1
         WHERE id = (SELECT id FROM conversation_native_sessions WHERE conversation_id = ?2 AND ended_at_ms IS NULL ORDER BY id DESC LIMIT 1)",
        params![native_session_id, conversation_id],
    ).db_context("record native session id")?;
    if updated == 0 {
        return Err(DatabaseError::InvalidState(format!(
            "Conversation '{}' has no open native session to record '{}' on", conversation_id, native_session_id
        )));
    }
    conn.execute(
        "UPDATE agent_conversations SET updated_at = ?1 WHERE id = ?2",
        params![now_epoch_secs(), conversation_id],
    ).db_context("touch conversation")?;
    crate::logger::log("INFO", "db", &format!("db_record_native_session_id: conversation '{}' -> native session '{}'", conversation_id, native_session_id));
    Ok(())
}

pub fn db_update_conversation_config(
    conversation_id: &str,
    runner: Option<&str>,
    model: Option<&str>,
    account_id: Option<&str>,
    effort: Option<&str>,
) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_update_conversation_config: conversation_id={}, runner={:?}, model={:?}, account_id={:?}, effort={:?}", conversation_id, runner, model, account_id, effort));
    let conn = get_connection()?;
    let now = now_epoch_secs();

    let mut query = String::from("UPDATE agent_conversations SET updated_at = ?1");
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(now)];

    if let Some(r) = runner {
        query.push_str(&format!(", runner = ?{}", params_vec.len() + 1));
        params_vec.push(Box::new(r.to_string()));
    }
    if let Some(m) = model {
        query.push_str(&format!(", model = ?{}", params_vec.len() + 1));
        params_vec.push(Box::new(m.to_string()));
    }
    if let Some(a) = account_id {
        query.push_str(&format!(", account_id = ?{}", params_vec.len() + 1));
        params_vec.push(Box::new(a.to_string()));
    }
    if let Some(e) = effort {
        query.push_str(&format!(", effort = ?{}", params_vec.len() + 1));
        params_vec.push(Box::new(e.to_string()));
    }

    query.push_str(&format!(" WHERE id = ?{}", params_vec.len() + 1));
    params_vec.push(Box::new(conversation_id.to_string()));

    let params_slice: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();
    conn.execute(&query, params_slice.as_slice()).db_context("update conversation config")?;

    crate::logger::log("INFO", "db", &format!("db_update_conversation_config: updated config for conversation_id={}", conversation_id));
    Ok(())
}

pub fn db_insert_chat_turn(turn: &ChatTurnRecord) -> Result<i64, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_insert_chat_turn: conversation_id={}, project_id={}, agent_id={}, role={}", turn.conversation_id, turn.project_id, turn.agent_id, turn.role));
    let conn = get_connection()?;
    let now = if turn.timestamp > 0 { turn.timestamp } else { now_epoch_secs() };

    // The turn is attributed to the Conversation's open Native Session, if any.
    let native_session_row_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM conversation_native_sessions WHERE conversation_id = ?1 AND ended_at_ms IS NULL ORDER BY id DESC LIMIT 1",
            params![&turn.conversation_id],
            |row| row.get(0),
        )
        .optional()
        .db_context("look up open native session")?;

    conn.execute(
        "INSERT INTO agent_chat_turns (
            conversation_id, project_id, agent_id, agy_conversation_id, role, dialogue,
            artifact_type, artifact_lang, artifact_label, artifact_file_path, artifact_content,
            is_ephemeral, terminal_snapshot, timestamp, approval_payload, native_session_row_id
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            &turn.conversation_id,
            turn.project_id,
            &turn.agent_id,
            &turn.agy_conversation_id,
            &turn.role,
            &turn.dialogue,
            &turn.artifact_type,
            &turn.artifact_lang,
            &turn.artifact_label,
            &turn.artifact_file_path,
            &turn.artifact_content,
            if turn.is_ephemeral { 1 } else { 0 },
            &turn.terminal_snapshot,
            now,
            &turn.approval_payload,
            native_session_row_id
        ],
    ).db_context("insert chat turn")?;

    let row_id = conn.last_insert_rowid();

    // The turn is stored; bookkeeping failures below are logged and do not undo it.
    crate::logger::log_failure("db", "touch conversation updated_at", conn.execute(
        "UPDATE agent_conversations SET updated_at = ?1 WHERE id = ?2",
        params![now, &turn.conversation_id],
    ).db_context("touch conversation updated_at"));
    if let Some(ref fp) = turn.artifact_file_path {
        crate::logger::log_failure("db", "record artifact touched", db_record_artifact_touched(&turn.conversation_id, fp));
    }

    crate::logger::log("INFO", "db", &format!("db_insert_chat_turn: inserted chat turn id={} for conversation_id={}", row_id, turn.conversation_id));
    Ok(row_id)
}

pub fn db_update_turn_approval_status(
    turn_id: i64,
    status: &str,
    feedback: Option<&str>,
) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_update_turn_approval_status: turn_id={}, status={}", turn_id, status));
    let conn = get_connection()?;

    let existing: Option<String> = conn
        .query_row("SELECT approval_payload FROM agent_chat_turns WHERE id = ?1", params![turn_id], |row| row.get(0))
        .optional()
        .db_context("read turn approval payload")?
        .ok_or_else(|| DatabaseError::NotFound { entity: "Chat turn", id: turn_id.to_string() })?;
    let invalid_payload = || DatabaseError::InvalidRequest(format!("Turn {} has no valid approval payload to update", turn_id));
    let payload_str = existing.ok_or_else(invalid_payload)?;
    let mut val: serde_json::Value = serde_json::from_str(&payload_str).map_err(|_| invalid_payload())?;
    let obj = val.as_object_mut().ok_or_else(invalid_payload)?;
    obj.insert("status".to_string(), serde_json::Value::String(status.to_string()));
    obj.insert("resolvedAt".to_string(), serde_json::json!(now_epoch_secs()));
    if let Some(fb) = feedback {
        obj.insert("feedback".to_string(), serde_json::Value::String(fb.to_string()));
    }
    let updated_json = serde_json::to_string(&val)
        .map_err(|e| DatabaseError::Serialization { operation: "encode approval payload".to_string(), source: e })?;
    conn.execute(
        "UPDATE agent_chat_turns SET approval_payload = ?1 WHERE id = ?2",
        params![&updated_json, turn_id],
    ).db_context("update turn approval payload")?;
    crate::logger::log("INFO", "db", &format!("db_update_turn_approval_status: turn {} updated to status '{}'", turn_id, status));
    Ok(())
}

pub fn db_get_conversation_turns(
    conversation_id: &str,
    limit: usize,
    before_id: Option<i64>,
) -> Result<Vec<ChatTurnRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_conversation_turns: conversation_id={}, limit={}, before_id={:?}", conversation_id, limit, before_id));
    let conn = get_connection()?;
    // `before_id = None` pages from the newest turn (i64::MAX is above every rowid).
    let mut stmt = conn
        .prepare(
            "SELECT id, conversation_id, project_id, agent_id, agy_conversation_id, role, dialogue,
                    artifact_type, artifact_lang, artifact_label, artifact_file_path, artifact_content,
                    is_ephemeral, terminal_snapshot, timestamp, approval_payload
             FROM agent_chat_turns
             WHERE conversation_id = ?1 AND id < ?2
             ORDER BY id DESC LIMIT ?3",
        )
        .db_context("prepare conversation turns query")?;
    let rows = stmt
        .query_map(params![conversation_id, before_id.unwrap_or(i64::MAX), limit as i64], |row| {
            Ok(ChatTurnRecord {
                id: Some(row.get(0)?),
                conversation_id: row.get(1)?,
                project_id: row.get(2)?,
                agent_id: row.get(3)?,
                agy_conversation_id: row.get(4)?,
                role: row.get(5)?,
                dialogue: row.get(6)?,
                artifact_type: row.get(7)?,
                artifact_lang: row.get(8)?,
                artifact_label: row.get(9)?,
                artifact_file_path: row.get(10)?,
                artifact_content: row.get(11)?,
                is_ephemeral: row.get::<_, i64>(12)? != 0,
                terminal_snapshot: row.get(13)?,
                timestamp: row.get(14)?,
                topic: None,
                approval_payload: row.get(15)?,
            })
        })
        .db_context("query conversation turns")?;
    let mut turns = collect_rows(rows, "read conversation turn row")?;

    // Reverse to chronological order (oldest first in list)
    turns.reverse();
    crate::logger::log("DEBUG", "db", &format!("db_get_conversation_turns: returning {} turns", turns.len()));
    Ok(turns)
}

/// Retrieves a continuous cross-session turn feed for a given agent in a project,
/// joining each turn with the `topic` of its parent `agent_conversations` record.
/// Used to render a single chronological chat history with thematic session separators.
pub fn db_get_agent_turns_continuous(
    project_id: i64,
    agent_id: &str,
    limit: usize,
    before_id: Option<i64>,
) -> Result<Vec<ChatTurnRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_agent_turns_continuous: project_id={}, agent_id={}, limit={}, before_id={:?}", project_id, agent_id, limit, before_id));
    let conn = get_connection()?;

    let base_select = "SELECT t.id, t.conversation_id, t.project_id, t.agent_id, t.agy_conversation_id, \
                              t.role, t.dialogue, t.artifact_type, t.artifact_lang, t.artifact_label, \
                              t.artifact_file_path, t.artifact_content, t.is_ephemeral, t.terminal_snapshot, \
                              t.timestamp, COALESCE(c.topic, ''), t.approval_payload \
                       FROM agent_chat_turns t \
                       LEFT JOIN agent_conversations c ON c.id = t.conversation_id";

    let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<ChatTurnRecord> {
        Ok(ChatTurnRecord {
            id: Some(row.get(0)?),
            conversation_id: row.get(1)?,
            project_id: row.get(2)?,
            agent_id: row.get(3)?,
            agy_conversation_id: row.get(4)?,
            role: row.get(5)?,
            dialogue: row.get(6)?,
            artifact_type: row.get(7)?,
            artifact_lang: row.get(8)?,
            artifact_label: row.get(9)?,
            artifact_file_path: row.get(10)?,
            artifact_content: row.get(11)?,
            is_ephemeral: row.get::<_, i64>(12)? != 0,
            terminal_snapshot: row.get(13)?,
            timestamp: row.get(14)?,
            topic: {
                let t: String = row.get(15)?;
                if t.is_empty() { None } else { Some(t) }
            },
            approval_payload: row.get(16)?,
        })
    };

    // `before_id = None` pages from the newest turn (i64::MAX is above every rowid).
    let query = format!("{} WHERE t.project_id = ?1 AND t.agent_id = ?2 AND t.id < ?3 ORDER BY t.id DESC LIMIT ?4", base_select);
    let mut stmt = conn.prepare(&query).db_context("prepare continuous turns query")?;
    let rows = stmt
        .query_map(params![project_id, agent_id, before_id.unwrap_or(i64::MAX), limit as i64], map_row)
        .db_context("query continuous turns")?;
    let mut turns = collect_rows(rows, "read continuous turn row")?;

    turns.reverse();
    crate::logger::log("DEBUG", "db", &format!("db_get_agent_turns_continuous: returning {} turns", turns.len()));
    Ok(turns)
}

pub fn db_get_active_agent_session(
    project_id: i64,
    agent_id: &str,
    expected_runner: &str,
) -> Result<Option<(String, usize)>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_active_agent_session: project_id={}, agent_id={}, expected_runner={}", project_id, agent_id, expected_runner));
    let conn = get_connection()?;

    // The active Conversation's most recent Native Session on the expected runner that reported an
    // id is resumable. A different runner finds nothing and archives nothing (Spec 30 §4): the
    // caller decides whether to close the Conversation.
    let resumable: Option<(String, String)> = conn
        .query_row(
            "SELECT c.id, ns.native_session_id
             FROM agent_conversations c
             JOIN conversation_native_sessions ns ON ns.conversation_id = c.id
             WHERE c.project_id = ?1 AND c.agent_id = ?2 AND c.status = 'active'
               AND lower(ns.runner) = lower(?3)
               AND ns.native_session_id IS NOT NULL AND ns.native_session_id <> ''
             ORDER BY ns.id DESC LIMIT 1",
            params![project_id, agent_id, expected_runner],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .db_context("query resumable native session")?;

    if let Some((conv_id, sess_id)) = resumable {
        let total_turns: i64 = conn
            .query_row(
                "SELECT count(*) FROM agent_chat_turns WHERE conversation_id = ?1",
                params![&conv_id],
                |row| row.get(0),
            )
            .db_context("count conversation turns")?;
        crate::logger::log("DEBUG", "db", &format!("db_get_active_agent_session: found session id={}, total_turns={}", sess_id, total_turns));
        return Ok(Some((sess_id, total_turns as usize)));
    }

    crate::logger::log("DEBUG", "db", &format!("db_get_active_agent_session: no active compatible session found for project_id={}, agent_id={}, expected_runner={}", project_id, agent_id, expected_runner));
    Ok(None)
}

pub fn db_insert_session_journal(rec: &SessionJournalRecord) -> Result<i64, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_insert_session_journal: project_id={}, agent_id={}, title={}", rec.project_id, rec.agent_id, rec.title));
    let conn = get_connection()?;
    let now = if rec.created_at > 0 { rec.created_at } else { now_epoch_secs() };
    conn.execute(
        "INSERT INTO agent_session_journal (
            project_id, agent_id, agy_conversation_id, title, summary,
            decisions, files_touched, technical_debt, tags, created_at, updated_at,
            context_pct, total_tokens, conversation_id, milestone_id,
            runner, model, cost_dollars, commit_hash, snapshot_ref
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
        params![
            rec.project_id,
            &rec.agent_id,
            &rec.agy_conversation_id,
            &rec.title,
            &rec.summary,
            &rec.decisions,
            &rec.files_touched,
            &rec.technical_debt,
            &rec.tags,
            now,
            now,
            rec.context_pct,
            rec.total_tokens,
            &rec.conversation_id,
            rec.milestone_id,
            &rec.runner,
            &rec.model,
            rec.cost_dollars,
            &rec.commit_hash,
            &rec.snapshot_ref
        ],
    ).db_context("insert session journal")?;
    let row_id = conn.last_insert_rowid();
    if let Some(conversation_id) = &rec.conversation_id {
        reindex_conversation(&conn, conversation_id).db_context("reindex conversation after minutes")?;
    }
    crate::logger::log("INFO", "db", &format!("db_insert_session_journal: inserted session journal id={} for project_id={}", row_id, rec.project_id));
    Ok(row_id)
}

/// Attaches the automatic Git commit hash to an inserted session journal entry.
pub fn db_set_session_journal_commit_hash(journal_id: i64, commit_hash: &str) -> Result<(), DatabaseError> {
    let conn = get_connection()?;
    conn.execute(
        "UPDATE agent_session_journal SET commit_hash = ?1, updated_at = ?2 WHERE id = ?3",
        params![commit_hash, now_epoch_secs(), journal_id],
    )
    .db_context("update session journal commit_hash")?;
    Ok(())
}

/// Attaches the automatic Git snapshot ref to an inserted session journal entry.
pub fn db_set_session_journal_snapshot_ref(journal_id: i64, snapshot_ref: &str) -> Result<(), DatabaseError> {
    let conn = get_connection()?;
    conn.execute(
        "UPDATE agent_session_journal SET snapshot_ref = ?1, updated_at = ?2 WHERE id = ?3",
        params![snapshot_ref, now_epoch_secs(), journal_id],
    )
    .db_context("update session journal snapshot_ref")?;
    Ok(())
}

/// Populates `files_touched` on an inserted session journal entry when derived deterministically from Git.
pub fn db_set_session_journal_files_touched(journal_id: i64, files_touched: &str) -> Result<(), DatabaseError> {
    let conn = get_connection()?;
    conn.execute(
        "UPDATE agent_session_journal SET files_touched = ?1, updated_at = ?2 WHERE id = ?3",
        params![files_touched, now_epoch_secs(), journal_id],
    )
    .db_context("update session journal files_touched")?;
    Ok(())
}

/// Deletes a session journal entry (used to roll back when an automatic Git commit hook fails).
pub fn db_delete_session_journal(journal_id: i64, conversation_id: Option<&str>) -> Result<(), DatabaseError> {
    let conn = get_connection()?;
    conn.execute("DELETE FROM agent_session_journal WHERE id = ?1", params![journal_id])
        .db_context("delete rolled-back session journal")?;
    if let Some(conv_id) = conversation_id {
        reindex_conversation(&conn, conv_id).db_context("reindex conversation after journal rollback")?;
    }
    Ok(())
}

/// Number of session journal entries of a project.
pub fn db_count_project_session_journals(project_id: i64) -> Result<usize, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_count_project_session_journals: project_id={}", project_id));
    let conn = get_connection()?;
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM agent_session_journal WHERE project_id = ?1", params![project_id], |row| row.get(0))
        .db_context("count project session journals")?;
    Ok(count as usize)
}

pub fn db_get_project_session_journals(
    project_id: i64,
    agent_id: Option<&str>,
    limit: usize,
) -> Result<Vec<SessionJournalRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_project_session_journals: project_id={}, agent_id={:?}, limit={}", project_id, agent_id, limit));
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, agent_id, agy_conversation_id, title, summary,
                    decisions, files_touched, technical_debt, tags, created_at, updated_at,
                    context_pct, total_tokens, conversation_id, milestone_id,
                    runner, model, cost_dollars, commit_hash, snapshot_ref
             FROM agent_session_journal
             WHERE project_id = ?1 AND (?2 IS NULL OR agent_id = ?2)
             ORDER BY id DESC LIMIT ?3",
        )
        .db_context("prepare project session journals query")?;
    let rows = stmt
        .query_map(params![project_id, agent_id, limit as i64], |row| {
            Ok(SessionJournalRecord {
                id: Some(row.get(0)?),
                project_id: row.get(1)?,
                agent_id: row.get(2)?,
                agy_conversation_id: row.get(3)?,
                title: row.get(4)?,
                summary: row.get(5)?,
                decisions: row.get(6)?,
                files_touched: row.get(7)?,
                technical_debt: row.get(8)?,
                tags: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
                context_pct: row.get(12)?,
                total_tokens: row.get(13)?,
                conversation_id: row.get(14)?,
                milestone_id: row.get(15)?,
                runner: row.get(16)?,
                model: row.get(17)?,
                cost_dollars: row.get(18)?,
                commit_hash: row.get(19)?,
                snapshot_ref: row.get(20)?,
            })
        })
        .db_context("query project session journals")?;
    let list = collect_rows(rows, "read session journal row")?;
    crate::logger::log("DEBUG", "db", &format!("db_get_project_session_journals: returning {} records", list.len()));
    Ok(list)
}

pub fn db_insert_todo(rec: &ProjectTodoRecord) -> Result<i64, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_insert_todo: project_id={}, title={}, priority={}", rec.project_id, rec.title, rec.priority));
    let conn = get_connection()?;
    let now = if rec.created_at > 0 { rec.created_at } else { now_epoch_secs() };
    conn.execute(
        "INSERT INTO project_todos (
            project_id, agent_id, agy_conversation_id, title, description, 
            category, status, priority, created_at, completed_at, conversation_id,
            file_path, line_start, line_end, journal_id
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            rec.project_id,
            &rec.agent_id,
            &rec.agy_conversation_id,
            &rec.title,
            &rec.description,
            &rec.category,
            &rec.status,
            rec.priority,
            now,
            rec.completed_at,
            &rec.conversation_id,
            &rec.file_path,
            rec.line_start.map(i64::from),
            rec.line_end.map(i64::from),
            rec.journal_id,
        ],
    ).db_context("insert project todo")?;
    let row_id = conn.last_insert_rowid();
    crate::logger::log("INFO", "db", &format!("db_insert_todo: inserted todo id={} for project_id={}", row_id, rec.project_id));
    Ok(row_id)
}

fn todo_from_row(row: &rusqlite::Row) -> rusqlite::Result<ProjectTodoRecord> {
    let line_start_opt: Option<i64> = row.get(13)?;
    let line_end_opt: Option<i64> = row.get(14)?;
    Ok(ProjectTodoRecord {
        id: Some(row.get(0)?),
        project_id: row.get(1)?,
        agent_id: row.get(2)?,
        agy_conversation_id: row.get(3)?,
        title: row.get(4)?,
        description: row.get(5)?,
        category: row.get(6)?,
        status: row.get(7)?,
        priority: row.get(8)?,
        created_at: row.get(9)?,
        completed_at: row.get(10)?,
        conversation_id: row.get(11)?,
        file_path: row.get(12)?,
        line_start: line_start_opt.and_then(|n| u32::try_from(n).ok()),
        line_end: line_end_opt.and_then(|n| u32::try_from(n).ok()),
        journal_id: row.get(15)?,
    })
}

pub fn db_get_project_todos(
    project_id: i64,
    status_filter: Option<&str>,
) -> Result<Vec<ProjectTodoRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_project_todos: project_id={}, status_filter={:?}", project_id, status_filter));
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, agent_id, agy_conversation_id, title, description,
                    category, status, priority, created_at, completed_at, conversation_id,
                    file_path, line_start, line_end, journal_id
             FROM project_todos
             WHERE project_id = ?1 AND (?2 IS NULL OR status = ?2)
             ORDER BY priority ASC, id DESC",
        )
        .db_context("prepare project todos query")?;
    let rows = stmt
        .query_map(params![project_id, status_filter], todo_from_row)
        .db_context("query project todos")?;
    let list = collect_rows(rows, "read project todo row")?;
    crate::logger::log("DEBUG", "db", &format!("db_get_project_todos: returning {} todos", list.len()));
    Ok(list)
}

pub fn db_get_conversation_todos(
    conversation_id: &str,
) -> Result<Vec<ProjectTodoRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_conversation_todos: conversation_id={}", conversation_id));
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, agent_id, agy_conversation_id, title, description,
                    category, status, priority, created_at, completed_at, conversation_id,
                    file_path, line_start, line_end, journal_id
             FROM project_todos
             WHERE conversation_id = ?1
             ORDER BY priority ASC, id DESC",
        )
        .db_context("prepare conversation todos query")?;
    let rows = stmt
        .query_map(params![conversation_id], todo_from_row)
        .db_context("query conversation todos")?;
    let list = collect_rows(rows, "read project todo row")?;
    crate::logger::log("DEBUG", "db", &format!("db_get_conversation_todos: returning {} todos", list.len()));
    Ok(list)
}

pub fn db_update_todo_status(
    todo_id: i64,
    status: &str,
) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_update_todo_status: todo_id={}, status={}", todo_id, status));
    let conn = get_connection()?;
    let now = now_epoch_secs();
    let completed_at = if status == "completed" { Some(now) } else { None };
    let rows_affected = conn.execute(
        "UPDATE project_todos SET status = ?1, completed_at = ?2 WHERE id = ?3",
        params![status, completed_at, todo_id],
    ).db_context("update todo status")?;
    if rows_affected == 0 {
        return Err(DatabaseError::NotFound { entity: "Project todo", id: todo_id.to_string() });
    }
    crate::logger::log("INFO", "db", &format!("db_update_todo_status: updated todo_id={} to status={}", todo_id, status));
    Ok(())
}

/// Updates a todo item's status strictly scoped to `project_id`.
pub fn db_update_todo_status_scoped(
    project_id: i64,
    todo_id: i64,
    status: &str,
) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_update_todo_status_scoped: project_id={}, todo_id={}, status={}", project_id, todo_id, status));
    let conn = get_connection()?;
    let now = now_epoch_secs();
    let completed_at = if status == "completed" { Some(now) } else { None };
    let rows_affected = conn.execute(
        "UPDATE project_todos SET status = ?1, completed_at = ?2 WHERE id = ?3 AND project_id = ?4",
        params![status, completed_at, todo_id, project_id],
    ).db_context("update scoped todo status")?;
    if rows_affected == 0 {
        return Err(DatabaseError::NotFound { entity: "Project todo", id: todo_id.to_string() });
    }
    crate::logger::log("INFO", "db", &format!("db_update_todo_status_scoped: updated todo_id={} in project_id={} to status={}", todo_id, project_id, status));
    Ok(())
}

pub fn db_insert_technical_debt(rec: &ProjectTechnicalDebtRecord) -> Result<i64, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_insert_technical_debt: project_id={}, title={}, severity={}", rec.project_id, rec.title, rec.severity));
    let conn = get_connection()?;
    let now = if rec.identified_at > 0 { rec.identified_at } else { now_epoch_secs() };
    conn.execute(
        "INSERT INTO project_technical_debt (
            project_id, agy_conversation_id, title, category, impact_summary, 
            remediation_plan, status, severity, identified_at, resolved_at, conversation_id, milestone_id,
            file_path, line_start, line_end, journal_id
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            rec.project_id,
            &rec.agy_conversation_id,
            &rec.title,
            &rec.category,
            &rec.impact_summary,
            &rec.remediation_plan,
            &rec.status,
            &rec.severity,
            now,
            rec.resolved_at,
            &rec.conversation_id,
            rec.milestone_id,
            &rec.file_path,
            rec.line_start.map(i64::from),
            rec.line_end.map(i64::from),
            rec.journal_id,
        ],
    ).db_context("insert technical debt")?;
    let row_id = conn.last_insert_rowid();
    crate::logger::log("INFO", "db", &format!("db_insert_technical_debt: inserted tech debt id={} for project_id={}", row_id, rec.project_id));
    Ok(row_id)
}

fn tech_debt_from_row(row: &rusqlite::Row) -> rusqlite::Result<ProjectTechnicalDebtRecord> {
    let line_start_opt: Option<i64> = row.get(14)?;
    let line_end_opt: Option<i64> = row.get(15)?;
    Ok(ProjectTechnicalDebtRecord {
        id: Some(row.get(0)?),
        project_id: row.get(1)?,
        agy_conversation_id: row.get(2)?,
        title: row.get(3)?,
        category: row.get(4)?,
        impact_summary: row.get(5)?,
        remediation_plan: row.get(6)?,
        status: row.get(7)?,
        severity: row.get(8)?,
        identified_at: row.get(9)?,
        resolved_at: row.get(10)?,
        conversation_id: row.get(11)?,
        milestone_id: row.get(12)?,
        file_path: row.get(13)?,
        line_start: line_start_opt.and_then(|n| u32::try_from(n).ok()),
        line_end: line_end_opt.and_then(|n| u32::try_from(n).ok()),
        journal_id: row.get(16)?,
    })
}

pub fn db_get_project_technical_debt(
    project_id: i64,
    status_filter: Option<&str>,
) -> Result<Vec<ProjectTechnicalDebtRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_project_technical_debt: project_id={}, status_filter={:?}", project_id, status_filter));
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, agy_conversation_id, title, category, impact_summary,
                    remediation_plan, status, severity, identified_at, resolved_at, conversation_id, milestone_id,
                    file_path, line_start, line_end, journal_id
             FROM project_technical_debt
             WHERE project_id = ?1 AND (?2 IS NULL OR status = ?2)
             ORDER BY id DESC",
        )
        .db_context("prepare project technical debt query")?;
    let rows = stmt
        .query_map(params![project_id, status_filter], tech_debt_from_row)
        .db_context("query project technical debt")?;
    let list = collect_rows(rows, "read project technical debt row")?;
    crate::logger::log("DEBUG", "db", &format!("db_get_project_technical_debt: returning {} items", list.len()));
    Ok(list)
}

pub fn db_get_conversation_technical_debt(
    conversation_id: &str,
) -> Result<Vec<ProjectTechnicalDebtRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_conversation_technical_debt: conversation_id={}", conversation_id));
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, agy_conversation_id, title, category, impact_summary,
                    remediation_plan, status, severity, identified_at, resolved_at, conversation_id, milestone_id,
                    file_path, line_start, line_end, journal_id
             FROM project_technical_debt
             WHERE conversation_id = ?1
             ORDER BY id DESC",
        )
        .db_context("prepare conversation technical debt query")?;
    let rows = stmt
        .query_map(params![conversation_id], tech_debt_from_row)
        .db_context("query conversation technical debt")?;
    let list = collect_rows(rows, "read project technical debt row")?;
    crate::logger::log("DEBUG", "db", &format!("db_get_conversation_technical_debt: returning {} items", list.len()));
    Ok(list)
}

pub fn db_resolve_technical_debt(debt_id: i64) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_resolve_technical_debt: debt_id={}", debt_id));
    let conn = get_connection()?;
    let now = now_epoch_secs();
    let rows_affected = conn.execute(
        "UPDATE project_technical_debt SET status = 'resolved', resolved_at = ?1 WHERE id = ?2",
        params![now, debt_id],
    ).db_context("resolve technical debt")?;
    if rows_affected == 0 {
        return Err(DatabaseError::NotFound { entity: "Technical debt item", id: debt_id.to_string() });
    }
    crate::logger::log("INFO", "db", &format!("db_resolve_technical_debt: resolved tech debt id={}", debt_id));
    Ok(())
}

/// Resolves a technical debt item strictly scoped to `project_id`.
pub fn db_resolve_technical_debt_scoped(project_id: i64, debt_id: i64) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_resolve_technical_debt_scoped: project_id={}, debt_id={}", project_id, debt_id));
    let conn = get_connection()?;
    let now = now_epoch_secs();
    let rows_affected = conn.execute(
        "UPDATE project_technical_debt SET status = 'resolved', resolved_at = ?1 WHERE id = ?2 AND project_id = ?3",
        params![now, debt_id, project_id],
    ).db_context("resolve scoped technical debt")?;
    if rows_affected == 0 {
        return Err(DatabaseError::NotFound { entity: "Technical debt item", id: debt_id.to_string() });
    }
    crate::logger::log("INFO", "db", &format!("db_resolve_technical_debt_scoped: resolved debt_id={} in project_id={}", debt_id, project_id));
    Ok(())
}

// ============================================================================
// Project Management Stages, Tasks, Subtasks & Relational Links (Spec 34)
// ============================================================================

const DEFAULT_PROJECT_STAGES: &[(&str, &str, bool)] = &[
    ("backlog", "Backlog", false),
    ("planned", "Planned", false),
    ("in_progress", "In Progress", false),
    ("review", "Review", false),
    ("done", "Done", true),
    ("blocked", "Blocked", false),
];

fn project_task_stage_from_row(row: &rusqlite::Row) -> rusqlite::Result<ProjectTaskStageRecord> {
    let collapsed_int: i64 = row.get(5)?;
    let terminal_int: i64 = row.get(6)?;
    Ok(ProjectTaskStageRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        slug: row.get(2)?,
        label: row.get(3)?,
        sort_order: row.get(4)?,
        is_collapsed: collapsed_int != 0,
        is_terminal: terminal_int != 0,
        created_at_seconds: row.get(7)?,
        updated_at_seconds: row.get(8)?,
    })
}

fn ensure_project_stages_on(conn: &mut rusqlite::Connection, project_id: i64) -> Result<(), DatabaseError> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM project_task_stages WHERE project_id = ?1",
            params![project_id],
            |r| r.get(0),
        )
        .db_context("count project task stages")?;
    if count > 0 {
        return Ok(());
    }

    let now = now_epoch_secs();
    let tx = conn.transaction().db_context("begin default stage seeding")?;
    {
        let mut stmt = tx
            .prepare(
                "INSERT OR IGNORE INTO project_task_stages (
                    project_id, slug, label, sort_order, is_collapsed, is_terminal,
                    created_at_seconds, updated_at_seconds
                 ) VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, ?7)",
            )
            .db_context("prepare default stage insert")?;
        for (idx, (slug, label, is_terminal)) in DEFAULT_PROJECT_STAGES.iter().enumerate() {
            stmt.execute(params![
                project_id,
                *slug,
                *label,
                idx as i32,
                if *is_terminal { 1 } else { 0 },
                now,
                now,
            ])
            .db_context("seed default project stage")?;
        }
    }
    tx.commit().db_context("commit default stage seeding")?;
    Ok(())
}

fn is_stage_terminal_on(conn: &rusqlite::Connection, project_id: i64, slug: &str) -> Result<bool, DatabaseError> {
    let terminal_int: Option<i64> = conn
        .query_row(
            "SELECT is_terminal FROM project_task_stages WHERE project_id = ?1 AND slug = ?2",
            params![project_id, slug],
            |r| r.get(0),
        )
        .optional()
        .db_context("lookup stage terminal flag")?;
    Ok(terminal_int.unwrap_or_else(|| if slug == "done" { 1 } else { 0 }) != 0)
}

fn stage_slug_from_label(label: &str) -> String {
    let mut slug = String::new();
    let mut prev_underscore = false;
    for ch in label.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_underscore = false;
        } else if !prev_underscore && !slug.is_empty() {
            slug.push('_');
            prev_underscore = true;
        }
        if slug.len() >= 40 {
            break;
        }
    }
    let cleaned = slug.trim_matches('_').to_string();
    if cleaned.is_empty() {
        "stage".to_string()
    } else {
        cleaned
    }
}

/// Lists all ordered Kanban stages for `project_id`, seeding defaults lazily if none exist.
pub fn db_list_project_stages(project_id: i64) -> Result<Vec<ProjectTaskStageRecord>, DatabaseError> {
    let mut conn = get_connection()?;
    ensure_project_stages_on(&mut conn, project_id)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, slug, label, sort_order, is_collapsed, is_terminal,
                    created_at_seconds, updated_at_seconds
             FROM project_task_stages
             WHERE project_id = ?1
             ORDER BY sort_order ASC, id ASC",
        )
        .db_context("prepare list project stages")?;
    let rows = stmt
        .query_map(params![project_id], project_task_stage_from_row)
        .db_context("query project stages")?;
    collect_rows(rows, "read project task stage row")
}

/// Returns `true` if `slug` is a registered Kanban stage of `project_id` (seeding defaults if needed).
pub fn db_project_stage_exists(project_id: i64, slug: &str) -> Result<bool, DatabaseError> {
    let mut conn = get_connection()?;
    ensure_project_stages_on(&mut conn, project_id)?;
    let found: Option<i64> = conn
        .query_row(
            "SELECT id FROM project_task_stages WHERE project_id = ?1 AND slug = ?2",
            params![project_id, slug],
            |r| r.get(0),
        )
        .optional()
        .db_context("check project stage existence")?;
    Ok(found.is_some())
}

/// Resolves `preferred` if registered on `project_id`, otherwise the first stage by `sort_order`.
pub fn db_resolve_default_stage_slug(project_id: i64, preferred: Option<&str>) -> Result<String, DatabaseError> {
    let stages = db_list_project_stages(project_id)?;
    if let Some(pref) = preferred {
        if let Some(s) = stages.iter().find(|st| st.slug == pref) {
            return Ok(s.slug.clone());
        }
    }
    Ok(stages
        .into_iter()
        .next()
        .map(|s| s.slug)
        .unwrap_or_else(|| "backlog".to_string()))
}

/// Creates a new custom Kanban stage at the end of `project_id`'s board.
pub fn db_insert_project_stage(
    project_id: i64,
    label: &str,
    is_terminal: bool,
) -> Result<ProjectTaskStageRecord, DatabaseError> {
    let mut conn = get_connection()?;
    ensure_project_stages_on(&mut conn, project_id)?;
    let base_slug = stage_slug_from_label(label);
    let mut candidate_slug = base_slug.clone();
    let mut suffix: usize = 2;
    while suffix <= 1000 {
        let exists: Option<i64> = conn
            .query_row(
                "SELECT id FROM project_task_stages WHERE project_id = ?1 AND slug = ?2",
                params![project_id, &candidate_slug],
                |r| r.get(0),
            )
            .optional()
            .db_context("check unique stage slug")?;
        if exists.is_none() {
            break;
        }
        candidate_slug = format!("{}_{}", base_slug, suffix);
        suffix += 1;
    }

    let next_order: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM project_task_stages WHERE project_id = ?1",
            params![project_id],
            |r| r.get(0),
        )
        .db_context("compute next stage sort_order")?;

    let now = now_epoch_secs();
    conn.execute(
        "INSERT INTO project_task_stages (
            project_id, slug, label, sort_order, is_collapsed, is_terminal,
            created_at_seconds, updated_at_seconds
         ) VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, ?7)",
        params![
            project_id,
            &candidate_slug,
            label,
            next_order,
            if is_terminal { 1 } else { 0 },
            now,
            now,
        ],
    )
    .db_context("insert project task stage")?;
    let id = conn.last_insert_rowid();
    conn.query_row(
        "SELECT id, project_id, slug, label, sort_order, is_collapsed, is_terminal,
                created_at_seconds, updated_at_seconds
         FROM project_task_stages WHERE id = ?1",
        params![id],
        project_task_stage_from_row,
    )
    .db_context("fetch created project task stage")
}

/// Updates a stage's label, collapsed flag, and terminal flag (synchronizing task completion timestamps).
pub fn db_update_project_stage(
    stage_id: i64,
    label: &str,
    is_collapsed: bool,
    is_terminal: bool,
) -> Result<Option<i64>, DatabaseError> {
    let mut conn = get_connection()?;
    let existing: Option<(i64, String, i64)> = conn
        .query_row(
            "SELECT project_id, slug, is_terminal FROM project_task_stages WHERE id = ?1",
            params![stage_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .db_context("lookup project stage for update")?;
    let Some((project_id, slug, prev_terminal)) = existing else {
        return Ok(None);
    };

    let now = now_epoch_secs();
    let tx = conn.transaction().db_context("begin stage update")?;
    tx.execute(
        "UPDATE project_task_stages
         SET label = ?1, is_collapsed = ?2, is_terminal = ?3, updated_at_seconds = ?4
         WHERE id = ?5",
        params![
            label,
            if is_collapsed { 1 } else { 0 },
            if is_terminal { 1 } else { 0 },
            now,
            stage_id,
        ],
    )
    .db_context("update project task stage")?;

    let was_terminal = prev_terminal != 0;
    if was_terminal != is_terminal {
        if is_terminal {
            tx.execute(
                "UPDATE project_tasks
                 SET completed_at_seconds = COALESCE(completed_at_seconds, ?1), updated_at_seconds = ?1
                 WHERE project_id = ?2 AND status = ?3",
                params![now, project_id, &slug],
            )
            .db_context("mark stage tasks completed")?;
        } else {
            tx.execute(
                "UPDATE project_tasks
                 SET completed_at_seconds = NULL, updated_at_seconds = ?1
                 WHERE project_id = ?2 AND status = ?3",
                params![now, project_id, &slug],
            )
            .db_context("clear stage tasks completion")?;
        }
    }
    tx.commit().db_context("commit stage update")?;
    Ok(Some(project_id))
}

/// Reorders all Kanban stages for `project_id` inside a single SQLite transaction.
pub fn db_reorder_project_stages(project_id: i64, ordered_stage_ids: &[i64]) -> Result<(), DatabaseError> {
    let mut conn = get_connection()?;
    ensure_project_stages_on(&mut conn, project_id)?;
    let now = now_epoch_secs();
    let tx = conn.transaction().db_context("begin stage reorder")?;
    {
        let mut stmt = tx
            .prepare(
                "UPDATE project_task_stages
                 SET sort_order = ?1, updated_at_seconds = ?2
                 WHERE id = ?3 AND project_id = ?4",
            )
            .db_context("prepare stage reorder")?;
        for (idx, stage_id) in ordered_stage_ids.iter().enumerate() {
            let changed = stmt
                .execute(params![idx as i32, now, *stage_id, project_id])
                .db_context("update stage sort_order")?;
            if changed == 0 {
                return Err(DatabaseError::InvalidRequest(format!(
                    "Stage #{} does not belong to project {}",
                    stage_id, project_id
                )));
            }
        }
    }
    tx.commit().db_context("commit stage reorder")?;
    Ok(())
}

/// Deletes a stage, guarding against deleting the last remaining stage or non-empty stages without reassignment.
pub fn db_delete_project_stage(
    stage_id: i64,
    reassign_to_stage_id: Option<i64>,
) -> Result<Option<DeleteStageOutcome>, DatabaseError> {
    let mut conn = get_connection()?;
    let target_row: Option<(i64, String)> = conn
        .query_row(
            "SELECT project_id, slug FROM project_task_stages WHERE id = ?1",
            params![stage_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .db_context("lookup project stage for delete")?;
    let Some((project_id, slug)) = target_row else {
        return Ok(None);
    };

    let stage_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM project_task_stages WHERE project_id = ?1",
            params![project_id],
            |r| r.get(0),
        )
        .db_context("count remaining project stages")?;
    if stage_count <= 1 {
        return Ok(Some(DeleteStageOutcome::LastStage));
    }

    let tasks_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM project_tasks WHERE project_id = ?1 AND status = ?2",
            params![project_id, &slug],
            |r| r.get(0),
        )
        .db_context("count tasks in stage before delete")?;

    let tx = conn.transaction().db_context("begin stage delete")?;
    if tasks_count > 0 {
        let Some(dest_id) = reassign_to_stage_id else {
            return Ok(Some(DeleteStageOutcome::HasTasks {
                tasks_count: tasks_count as usize,
            }));
        };
        if dest_id == stage_id {
            return Err(DatabaseError::InvalidRequest(
                "Cannot reassign tasks to the stage being deleted".to_string(),
            ));
        }
        let dest_row: Option<(i64, String, i64)> = tx
            .query_row(
                "SELECT project_id, slug, is_terminal FROM project_task_stages WHERE id = ?1",
                params![dest_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .db_context("lookup destination stage for reassignment")?;
        let Some((dest_proj, dest_slug, dest_terminal)) = dest_row else {
            return Err(DatabaseError::InvalidRequest(format!(
                "Reassignment stage #{} not found",
                dest_id
            )));
        };
        if dest_proj != project_id {
            return Err(DatabaseError::InvalidRequest(
                "Cross-project stage reassignment rejected".to_string(),
            ));
        }

        let now = now_epoch_secs();
        if dest_terminal != 0 {
            tx.execute(
                "UPDATE project_tasks
                 SET status = ?1, completed_at_seconds = COALESCE(completed_at_seconds, ?2), updated_at_seconds = ?2
                 WHERE project_id = ?3 AND status = ?4",
                params![&dest_slug, now, project_id, &slug],
            )
            .db_context("reassign stage tasks to terminal stage")?;
        } else {
            tx.execute(
                "UPDATE project_tasks
                 SET status = ?1, completed_at_seconds = NULL, updated_at_seconds = ?2
                 WHERE project_id = ?3 AND status = ?4",
                params![&dest_slug, now, project_id, &slug],
            )
            .db_context("reassign stage tasks")?;
        }
    }

    tx.execute("DELETE FROM project_task_stages WHERE id = ?1", params![stage_id])
        .db_context("delete project task stage")?;
    tx.commit().db_context("commit stage delete")?;
    Ok(Some(DeleteStageOutcome::Deleted { project_id }))
}

fn project_task_from_row(row: &rusqlite::Row) -> rusqlite::Result<ProjectTaskRecord> {
    Ok(ProjectTaskRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        title: row.get(2)?,
        description: row.get(3)?,
        domain: row.get(4)?,
        status: row.get(5)?,
        priority: row.get(6)?,
        milestone_id: row.get(7)?,
        weight_points: row.get(8)?,
        start_epoch_seconds: row.get(9)?,
        due_epoch_seconds: row.get(10)?,
        github_repo: row.get(11)?,
        github_issue_number: row.get(12)?,
        github_pr_number: row.get(13)?,
        branch_name: row.get(14)?,
        created_at_seconds: row.get(15)?,
        updated_at_seconds: row.get(16)?,
        completed_at_seconds: row.get(17)?,
    })
}

fn project_task_subtask_from_row(row: &rusqlite::Row) -> rusqlite::Result<ProjectTaskSubtaskRecord> {
    let completed_int: i64 = row.get(3)?;
    Ok(ProjectTaskSubtaskRecord {
        id: row.get(0)?,
        task_id: row.get(1)?,
        title: row.get(2)?,
        is_completed: completed_int != 0,
        sort_order: row.get(4)?,
        linked_todo_id: row.get(5)?,
        created_at_seconds: row.get(6)?,
    })
}

fn project_task_link_from_row(row: &rusqlite::Row) -> rusqlite::Result<ProjectTaskLinkRecord> {
    Ok(ProjectTaskLinkRecord {
        id: row.get(0)?,
        task_id: row.get(1)?,
        link_type: row.get(2)?,
        target_id: row.get(3)?,
        created_at_seconds: row.get(4)?,
    })
}

/// Inserts a new [`ProjectTaskRecord`] and returns its row ID.
#[allow(clippy::too_many_arguments)]
pub fn db_insert_project_task(
    project_id: i64,
    title: &str,
    description: Option<&str>,
    domain: &str,
    status: &str,
    priority: i32,
    milestone_id: Option<i64>,
    weight_points: i32,
    start_epoch_seconds: Option<i64>,
    due_epoch_seconds: Option<i64>,
    github_repo: Option<&str>,
    github_issue_number: Option<i64>,
    github_pr_number: Option<i64>,
    branch_name: Option<&str>,
) -> Result<i64, DatabaseError> {
    let mut conn = get_connection()?;
    ensure_project_stages_on(&mut conn, project_id)?;
    let now = now_epoch_secs();
    let is_terminal = is_stage_terminal_on(&conn, project_id, status)?;
    let completed_at = if is_terminal { Some(now) } else { None };
    conn.execute(
        "INSERT INTO project_tasks (
            project_id, title, description, domain, status, priority,
            milestone_id, weight_points,
            start_epoch_seconds, due_epoch_seconds,
            github_repo, github_issue_number, github_pr_number, branch_name,
            created_at_seconds, updated_at_seconds, completed_at_seconds
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        params![
            project_id,
            title,
            description,
            domain,
            status,
            priority,
            milestone_id,
            weight_points.max(1),
            start_epoch_seconds,
            due_epoch_seconds,
            github_repo,
            github_issue_number,
            github_pr_number,
            branch_name,
            now,
            now,
            completed_at,
        ],
    ).db_context("insert project task")?;
    Ok(conn.last_insert_rowid())
}

/// Lists all [`ProjectTaskBundle`] records for `project_id` in 3 bounded queries (tasks, subtasks, links).
pub fn db_list_project_tasks(
    project_id: i64,
    status_filter: Option<&str>,
) -> Result<Vec<ProjectTaskBundle>, DatabaseError> {
    let conn = get_connection()?;
    let mut task_stmt = conn
        .prepare(
            "SELECT id, project_id, title, description, domain, status, priority,
                    milestone_id, weight_points,
                    start_epoch_seconds, due_epoch_seconds,
                    github_repo, github_issue_number, github_pr_number, branch_name,
                    created_at_seconds, updated_at_seconds, completed_at_seconds
             FROM project_tasks
             WHERE project_id = ?1 AND (?2 IS NULL OR status = ?2)
             ORDER BY priority ASC, COALESCE(due_epoch_seconds, 9223372036854775807) ASC, id DESC",
        )
        .db_context("prepare project tasks query")?;
    let task_rows = task_stmt
        .query_map(params![project_id, status_filter], project_task_from_row)
        .db_context("query project tasks")?;
    let tasks = collect_rows(task_rows, "read project task row")?;
    if tasks.is_empty() {
        return Ok(Vec::new());
    }

    let mut sub_stmt = conn
        .prepare(
            "SELECT s.id, s.task_id, s.title, s.is_completed, s.sort_order, s.linked_todo_id, s.created_at_seconds
             FROM project_task_subtasks s
             INNER JOIN project_tasks t ON t.id = s.task_id
             WHERE t.project_id = ?1
             ORDER BY s.sort_order ASC, s.id ASC",
        )
        .db_context("prepare project task subtasks query")?;
    let sub_rows = sub_stmt
        .query_map(params![project_id], project_task_subtask_from_row)
        .db_context("query project task subtasks")?;
    let subtasks = collect_rows(sub_rows, "read project task subtask row")?;

    let mut link_stmt = conn
        .prepare(
            "SELECT l.id, l.task_id, l.link_type, l.target_id, l.created_at_seconds
             FROM project_task_links l
             INNER JOIN project_tasks t ON t.id = l.task_id
             WHERE t.project_id = ?1
             ORDER BY l.id ASC",
        )
        .db_context("prepare project task links query")?;
    let link_rows = link_stmt
        .query_map(params![project_id], project_task_link_from_row)
        .db_context("query project task links")?;
    let links = collect_rows(link_rows, "read project task link row")?;

    let mut subtasks_by_task: std::collections::HashMap<i64, Vec<ProjectTaskSubtaskRecord>> =
        std::collections::HashMap::new();
    for s in subtasks {
        subtasks_by_task.entry(s.task_id).or_default().push(s);
    }
    let mut links_by_task: std::collections::HashMap<i64, Vec<ProjectTaskLinkRecord>> =
        std::collections::HashMap::new();
    for l in links {
        links_by_task.entry(l.task_id).or_default().push(l);
    }

    let mut bundles = Vec::with_capacity(tasks.len());
    for task in tasks {
        let tid = task.id;
        bundles.push(ProjectTaskBundle {
            task,
            subtasks: subtasks_by_task.remove(&tid).unwrap_or_default(),
            links: links_by_task.remove(&tid).unwrap_or_default(),
        });
    }
    Ok(bundles)
}

/// Fetches a single [`ProjectTaskRecord`] by ID.
pub fn db_get_project_task(task_id: i64) -> Result<Option<ProjectTaskRecord>, DatabaseError> {
    let conn = get_connection()?;
    conn.query_row(
        "SELECT id, project_id, title, description, domain, status, priority,
                milestone_id, weight_points,
                start_epoch_seconds, due_epoch_seconds,
                github_repo, github_issue_number, github_pr_number, branch_name,
                created_at_seconds, updated_at_seconds, completed_at_seconds
         FROM project_tasks WHERE id = ?1",
        params![task_id],
        project_task_from_row,
    )
    .optional()
    .db_context("get project task by id")
}

/// Updates an existing [`ProjectTaskRecord`] in place and returns its `project_id`.
#[allow(clippy::too_many_arguments)]
pub fn db_update_project_task(
    task_id: i64,
    title: &str,
    description: Option<&str>,
    domain: &str,
    status: &str,
    priority: i32,
    milestone_id: Option<i64>,
    weight_points: i32,
    start_epoch_seconds: Option<i64>,
    due_epoch_seconds: Option<i64>,
    branch_name: Option<&str>,
) -> Result<Option<i64>, DatabaseError> {
    let mut conn = get_connection()?;
    let existing: Option<(i64, Option<i64>)> = conn
        .query_row(
            "SELECT project_id, completed_at_seconds FROM project_tasks WHERE id = ?1",
            params![task_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .db_context("lookup project task before update")?;
    let Some((project_id, prev_completed_at)) = existing else {
        return Ok(None);
    };

    ensure_project_stages_on(&mut conn, project_id)?;
    let now = now_epoch_secs();
    let is_terminal = is_stage_terminal_on(&conn, project_id, status)?;
    let completed_at = if is_terminal {
        Some(prev_completed_at.unwrap_or(now))
    } else {
        None
    };

    conn.execute(
        "UPDATE project_tasks SET
            title = ?1,
            description = ?2,
            domain = ?3,
            status = ?4,
            priority = ?5,
            milestone_id = ?6,
            weight_points = ?7,
            start_epoch_seconds = ?8,
            due_epoch_seconds = ?9,
            branch_name = ?10,
            updated_at_seconds = ?11,
            completed_at_seconds = ?12
         WHERE id = ?13",
        params![
            title,
            description,
            domain,
            status,
            priority,
            milestone_id,
            weight_points.max(1),
            start_epoch_seconds,
            due_epoch_seconds,
            branch_name,
            now,
            completed_at,
            task_id,
        ],
    ).db_context("update project task")?;
    Ok(Some(project_id))
}

/// Deletes a [`ProjectTaskRecord`] (cascading subtasks and links) and returns its `project_id` if present.
pub fn db_delete_project_task(task_id: i64) -> Result<Option<i64>, DatabaseError> {
    let conn = get_connection()?;
    let project_id: Option<i64> = conn
        .query_row(
            "SELECT project_id FROM project_tasks WHERE id = ?1",
            params![task_id],
            |r| r.get(0),
        )
        .optional()
        .db_context("lookup project task before delete")?;
    if project_id.is_some() {
        conn.execute("DELETE FROM project_tasks WHERE id = ?1", params![task_id])
            .db_context("delete project task")?;
    }
    Ok(project_id)
}

/// Adds a subtask to `task_id` and returns `(subtask_id, project_id)` when `task_id` exists.
pub fn db_insert_task_subtask(
    task_id: i64,
    title: &str,
    linked_todo_id: Option<i64>,
) -> Result<Option<(i64, i64)>, DatabaseError> {
    let conn = get_connection()?;
    let project_id: Option<i64> = conn
        .query_row(
            "SELECT project_id FROM project_tasks WHERE id = ?1",
            params![task_id],
            |r| r.get(0),
        )
        .optional()
        .db_context("lookup parent task for subtask")?;
    let Some(project_id) = project_id else {
        return Ok(None);
    };

    let next_order: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(sort_order) + 1, 0) FROM project_task_subtasks WHERE task_id = ?1",
            params![task_id],
            |r| r.get(0),
        )
        .db_context("compute subtask sort order")?;
    let now = now_epoch_secs();
    conn.execute(
        "INSERT INTO project_task_subtasks (task_id, title, is_completed, sort_order, linked_todo_id, created_at_seconds)
         VALUES (?1, ?2, 0, ?3, ?4, ?5)",
        params![task_id, title, next_order, linked_todo_id, now],
    ).db_context("insert project task subtask")?;
    let sub_id = conn.last_insert_rowid();
    conn.execute(
        "UPDATE project_tasks SET updated_at_seconds = ?1 WHERE id = ?2",
        params![now, task_id],
    ).db_context("touch project task updated_at")?;
    Ok(Some((sub_id, project_id)))
}

/// Toggles completion of a subtask and returns the parent `project_id` if found.
pub fn db_set_task_subtask_completed(
    subtask_id: i64,
    is_completed: bool,
) -> Result<Option<i64>, DatabaseError> {
    let conn = get_connection()?;
    let row: Option<(i64, i64)> = conn
        .query_row(
            "SELECT s.task_id, t.project_id
             FROM project_task_subtasks s
             INNER JOIN project_tasks t ON t.id = s.task_id
             WHERE s.id = ?1",
            params![subtask_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .db_context("lookup subtask before toggle")?;
    let Some((task_id, project_id)) = row else {
        return Ok(None);
    };
    let now = now_epoch_secs();
    conn.execute(
        "UPDATE project_task_subtasks SET is_completed = ?1 WHERE id = ?2",
        params![if is_completed { 1 } else { 0 }, subtask_id],
    ).db_context("update subtask completion")?;
    conn.execute(
        "UPDATE project_tasks SET updated_at_seconds = ?1 WHERE id = ?2",
        params![now, task_id],
    ).db_context("touch parent task updated_at")?;
    Ok(Some(project_id))
}

/// Deletes a subtask and returns the parent `project_id` if found.
pub fn db_delete_task_subtask(subtask_id: i64) -> Result<Option<i64>, DatabaseError> {
    let conn = get_connection()?;
    let row: Option<(i64, i64)> = conn
        .query_row(
            "SELECT s.task_id, t.project_id
             FROM project_task_subtasks s
             INNER JOIN project_tasks t ON t.id = s.task_id
             WHERE s.id = ?1",
            params![subtask_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .db_context("lookup subtask before delete")?;
    let Some((task_id, project_id)) = row else {
        return Ok(None);
    };
    conn.execute("DELETE FROM project_task_subtasks WHERE id = ?1", params![subtask_id])
        .db_context("delete subtask")?;
    conn.execute(
        "UPDATE project_tasks SET updated_at_seconds = ?1 WHERE id = ?2",
        params![now_epoch_secs(), task_id],
    ).db_context("touch parent task updated_at")?;
    Ok(Some(project_id))
}

/// Fetches a single [`ProjectTaskStageRecord`] by `stage_id`.
pub fn db_get_project_stage(stage_id: i64) -> Result<Option<ProjectTaskStageRecord>, DatabaseError> {
    let conn = get_connection()?;
    conn.query_row(
        "SELECT id, project_id, slug, label, sort_order, is_collapsed, is_terminal,
                created_at_seconds, updated_at_seconds
         FROM project_task_stages WHERE id = ?1",
        params![stage_id],
        project_task_stage_from_row,
    )
    .optional()
    .db_context("get project task stage by id")
}

/// Resolves the owning `project_id` of a [`ProjectTaskSubtaskRecord`].
pub fn db_get_task_subtask_project_id(subtask_id: i64) -> Result<Option<i64>, DatabaseError> {
    let conn = get_connection()?;
    conn.query_row(
        "SELECT t.project_id
         FROM project_task_subtasks s
         INNER JOIN project_tasks t ON t.id = s.task_id
         WHERE s.id = ?1",
        params![subtask_id],
        |r| r.get(0),
    )
    .optional()
    .db_context("lookup subtask parent project_id")
}

fn project_milestone_from_row(row: &rusqlite::Row) -> rusqlite::Result<ProjectMilestoneRecord> {
    Ok(ProjectMilestoneRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        title: row.get(2)?,
        description: row.get(3)?,
        status: row.get(4)?,
        weight_percent: row.get(5)?,
        hill_position_percent: row.get(6)?,
        appetite_days: row.get(7)?,
        planned_start_epoch_seconds: row.get(8)?,
        planned_due_epoch_seconds: row.get(9)?,
        sort_order: row.get(10)?,
        created_at_seconds: row.get(11)?,
        updated_at_seconds: row.get(12)?,
        completed_at_seconds: row.get(13)?,
    })
}

/// Inserts a new [`ProjectMilestoneRecord`] and returns its row ID.
#[allow(clippy::too_many_arguments)]
/// Outcome of a project-scoped milestone update or delete inside an atomic transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MilestoneScopedOutcome {
    Applied,
    NotFound,
    ProjectMismatch { actual_project_id: i64 },
}

/// Inserts a new [`ProjectMilestoneRecord`] inside an explicit transaction (atomic `sort_order` + `INSERT`).
#[allow(clippy::too_many_arguments)]
pub fn db_insert_project_milestone(
    project_id: i64,
    title: &str,
    description: Option<&str>,
    status: &str,
    weight_percent: f64,
    hill_position_percent: i32,
    appetite_days: Option<i32>,
    planned_start_epoch_seconds: Option<i64>,
    planned_due_epoch_seconds: Option<i64>,
    sort_order: Option<i32>,
) -> Result<i64, DatabaseError> {
    let mut conn = get_connection()?;
    let tx = conn
        .transaction()
        .db_context("begin insert project milestone transaction")?;
    let now = now_epoch_secs();
    let next_order: i32 = match sort_order {
        Some(v) => v,
        None => tx
            .query_row(
                "SELECT COALESCE(MAX(sort_order) + 1, 0) FROM project_milestones WHERE project_id = ?1",
                params![project_id],
                |r| r.get(0),
            )
            .db_context("compute next milestone sort_order")?,
    };
    let completed_at = if status == "completed" { Some(now) } else { None };
    tx.execute(
        "INSERT INTO project_milestones (
            project_id, title, description, status, weight_percent,
            hill_position_percent, appetite_days,
            planned_start_epoch_seconds, planned_due_epoch_seconds,
            sort_order, created_at_seconds, updated_at_seconds, completed_at_seconds
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            project_id,
            title,
            description,
            status,
            weight_percent.max(0.0),
            hill_position_percent.clamp(0, 100),
            appetite_days,
            planned_start_epoch_seconds,
            planned_due_epoch_seconds,
            next_order,
            now,
            now,
            completed_at,
        ],
    )
    .db_context("insert project milestone")?;
    let id = tx.last_insert_rowid();
    tx.commit()
        .db_context("commit insert project milestone")?;
    Ok(id)
}

/// Lists all [`ProjectMilestoneRecord`] rows for `project_id` ordered by `sort_order ASC, id ASC`.
pub fn db_list_project_milestones(project_id: i64) -> Result<Vec<ProjectMilestoneRecord>, DatabaseError> {
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, title, description, status, weight_percent,
                    hill_position_percent, appetite_days,
                    planned_start_epoch_seconds, planned_due_epoch_seconds,
                    sort_order, created_at_seconds, updated_at_seconds, completed_at_seconds
             FROM project_milestones
             WHERE project_id = ?1
             ORDER BY sort_order ASC, id ASC",
        )
        .db_context("prepare list project milestones")?;
    let rows = stmt
        .query_map(params![project_id], project_milestone_from_row)
        .db_context("query project milestones")?;
    collect_rows(rows, "read project milestone row")
}

/// Fetches a single [`ProjectMilestoneRecord`] by ID.
pub fn db_get_project_milestone(milestone_id: i64) -> Result<Option<ProjectMilestoneRecord>, DatabaseError> {
    let conn = get_connection()?;
    conn.query_row(
        "SELECT id, project_id, title, description, status, weight_percent,
                hill_position_percent, appetite_days,
                planned_start_epoch_seconds, planned_due_epoch_seconds,
                sort_order, created_at_seconds, updated_at_seconds, completed_at_seconds
         FROM project_milestones WHERE id = ?1",
        params![milestone_id],
        project_milestone_from_row,
    )
    .optional()
    .db_context("get project milestone by id")
}

/// Updates an existing [`ProjectMilestoneRecord`] inside an explicit transaction after verifying ownership.
#[allow(clippy::too_many_arguments)]
pub fn db_update_project_milestone(
    milestone_id: i64,
    expected_project_id: i64,
    title: &str,
    description: Option<&str>,
    status: &str,
    weight_percent: f64,
    hill_position_percent: i32,
    appetite_days: Option<i32>,
    planned_start_epoch_seconds: Option<i64>,
    planned_due_epoch_seconds: Option<i64>,
    sort_order: i32,
) -> Result<MilestoneScopedOutcome, DatabaseError> {
    let mut conn = get_connection()?;
    let tx = conn
        .transaction()
        .db_context("begin update project milestone transaction")?;
    let existing: Option<(i64, Option<i64>)> = tx
        .query_row(
            "SELECT project_id, completed_at_seconds FROM project_milestones WHERE id = ?1",
            params![milestone_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .db_context("lookup project milestone before update")?;
    let Some((project_id, prev_completed_at)) = existing else {
        return Ok(MilestoneScopedOutcome::NotFound);
    };
    if project_id != expected_project_id {
        return Ok(MilestoneScopedOutcome::ProjectMismatch {
            actual_project_id: project_id,
        });
    }
    let now = now_epoch_secs();
    let completed_at = if status == "completed" {
        Some(prev_completed_at.unwrap_or(now))
    } else {
        None
    };
    tx.execute(
        "UPDATE project_milestones SET
            title = ?1,
            description = ?2,
            status = ?3,
            weight_percent = ?4,
            hill_position_percent = ?5,
            appetite_days = ?6,
            planned_start_epoch_seconds = ?7,
            planned_due_epoch_seconds = ?8,
            sort_order = ?9,
            updated_at_seconds = ?10,
            completed_at_seconds = ?11
         WHERE id = ?12 AND project_id = ?13",
        params![
            title,
            description,
            status,
            weight_percent.max(0.0),
            hill_position_percent.clamp(0, 100),
            appetite_days,
            planned_start_epoch_seconds,
            planned_due_epoch_seconds,
            sort_order,
            now,
            completed_at,
            milestone_id,
            expected_project_id,
        ],
    )
    .db_context("update project milestone")?;
    tx.commit()
        .db_context("commit update project milestone")?;
    Ok(MilestoneScopedOutcome::Applied)
}

/// Deletes a [`ProjectMilestoneRecord`] inside an explicit transaction after verifying ownership
/// (tasks/journal/tech_debt keep their rows with `milestone_id = NULL`).
pub fn db_delete_project_milestone(
    milestone_id: i64,
    expected_project_id: i64,
) -> Result<MilestoneScopedOutcome, DatabaseError> {
    let mut conn = get_connection()?;
    let tx = conn
        .transaction()
        .db_context("begin delete project milestone transaction")?;
    let project_id: Option<i64> = tx
        .query_row(
            "SELECT project_id FROM project_milestones WHERE id = ?1",
            params![milestone_id],
            |r| r.get(0),
        )
        .optional()
        .db_context("lookup project milestone before delete")?;
    let Some(actual_project_id) = project_id else {
        return Ok(MilestoneScopedOutcome::NotFound);
    };
    if actual_project_id != expected_project_id {
        return Ok(MilestoneScopedOutcome::ProjectMismatch { actual_project_id });
    }
    tx.execute(
        "DELETE FROM project_milestones WHERE id = ?1 AND project_id = ?2",
        params![milestone_id, expected_project_id],
    )
    .db_context("delete project milestone")?;
    tx.commit()
        .db_context("commit delete project milestone")?;
    Ok(MilestoneScopedOutcome::Applied)
}

/// Queries all `Journal` evidence aggregated for `milestone_id`:
/// 1. Direct & transitive `agent_session_journal` entries (`j.milestone_id = ?2` OR linked via `project_task_links`).
/// 2. Distinct `project_todos` linked via `project_task_links` or `project_task_subtasks.linked_todo_id` on tasks in this milestone.
/// 3. `project_technical_debt` rows with `milestone_id = ?2`.
pub fn db_get_milestone_linked_records(
    project_id: i64,
    milestone_id: i64,
) -> Result<(Vec<SessionJournalRecord>, Vec<ProjectTodoRecord>, Vec<ProjectTechnicalDebtRecord>), DatabaseError> {
    let conn = get_connection()?;

    let mut journal_stmt = conn
        .prepare(
            "SELECT DISTINCT j.id, j.project_id, j.agent_id, j.agy_conversation_id, j.title, j.summary,
                    j.decisions, j.files_touched, j.technical_debt, j.tags, j.created_at, j.updated_at,
                    j.context_pct, j.total_tokens, j.conversation_id, j.milestone_id,
                    j.runner, j.model, j.cost_dollars, j.commit_hash, j.snapshot_ref
             FROM agent_session_journal j
             WHERE j.project_id = ?1 AND (
                 j.milestone_id = ?2
                 OR j.id IN (
                     SELECT l.target_id
                     FROM project_task_links l
                     INNER JOIN project_tasks t ON t.id = l.task_id
                     WHERE t.project_id = ?1 AND t.milestone_id = ?2 AND l.link_type = 'journal'
                 )
             )
             ORDER BY j.created_at DESC, j.id DESC",
        )
        .db_context("prepare milestone journal aggregation")?;
    let journal_rows = journal_stmt
        .query_map(params![project_id, milestone_id], |row| {
            Ok(SessionJournalRecord {
                id: Some(row.get(0)?),
                project_id: row.get(1)?,
                agent_id: row.get(2)?,
                agy_conversation_id: row.get(3)?,
                title: row.get(4)?,
                summary: row.get(5)?,
                decisions: row.get(6)?,
                files_touched: row.get(7)?,
                technical_debt: row.get(8)?,
                tags: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
                context_pct: row.get(12)?,
                total_tokens: row.get(13)?,
                conversation_id: row.get(14)?,
                milestone_id: row.get(15)?,
                runner: row.get(16)?,
                model: row.get(17)?,
                cost_dollars: row.get(18)?,
                commit_hash: row.get(19)?,
                snapshot_ref: row.get(20)?,
            })
        })
        .db_context("query milestone journal aggregation")?;
    let journals = collect_rows(journal_rows, "read milestone journal row")?;

    let mut todo_stmt = conn
        .prepare(
            "SELECT DISTINCT td.id, td.project_id, td.agent_id, td.agy_conversation_id, td.title,
                    td.description, td.category, td.status, td.priority, td.created_at, td.completed_at, td.conversation_id,
                    td.file_path, td.line_start, td.line_end, td.journal_id
             FROM project_todos td
             WHERE td.project_id = ?1 AND (
                 td.id IN (
                     SELECT l.target_id
                     FROM project_task_links l
                     INNER JOIN project_tasks t ON t.id = l.task_id
                     WHERE t.project_id = ?1 AND t.milestone_id = ?2 AND l.link_type = 'todo'
                 )
                 OR td.id IN (
                     SELECT s.linked_todo_id
                     FROM project_task_subtasks s
                     INNER JOIN project_tasks t ON t.id = s.task_id
                     WHERE t.project_id = ?1 AND t.milestone_id = ?2 AND s.linked_todo_id IS NOT NULL
                 )
             )
             ORDER BY td.priority ASC, td.id DESC",
        )
        .db_context("prepare milestone todo aggregation")?;
    let todo_rows = todo_stmt
        .query_map(params![project_id, milestone_id], todo_from_row)
        .db_context("query milestone todo aggregation")?;
    let todos = collect_rows(todo_rows, "read milestone todo row")?;

    let mut debt_stmt = conn
        .prepare(
            "SELECT id, project_id, agy_conversation_id, title, category, impact_summary,
                    remediation_plan, status, severity, identified_at, resolved_at, conversation_id, milestone_id,
                    file_path, line_start, line_end, journal_id
             FROM project_technical_debt
             WHERE project_id = ?1 AND milestone_id = ?2
             ORDER BY id DESC",
        )
        .db_context("prepare milestone tech debt query")?;
    let debt_rows = debt_stmt
        .query_map(params![project_id, milestone_id], tech_debt_from_row)
        .db_context("query milestone tech debt")?;
    let debts = collect_rows(debt_rows, "read milestone tech debt row")?;

    Ok((journals, todos, debts))
}

/// Aggregates `agent_session_journal.total_tokens` at the project level and per milestone
/// (direct `milestone_id` or transitive via `project_task_links`). Scaffolding for future
/// per-project/milestone token budget tracking.
pub fn db_sum_milestone_tokens(
    project_id: i64,
) -> Result<(i64, std::collections::HashMap<i64, i64>), DatabaseError> {
    let conn = get_connection()?;
    let project_total: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(total_tokens), 0)
             FROM agent_session_journal
             WHERE project_id = ?1",
            params![project_id],
            |r| r.get(0),
        )
        .db_context("sum project journal tokens")?;

    let mut stmt = conn
        .prepare(
            "SELECT m_id, COALESCE(SUM(tokens), 0)
             FROM (
                 SELECT DISTINCT j.id AS j_id, j.milestone_id AS m_id, COALESCE(j.total_tokens, 0) AS tokens
                 FROM agent_session_journal j
                 WHERE j.project_id = ?1 AND j.milestone_id IS NOT NULL
                 UNION
                 SELECT DISTINCT j.id AS j_id, t.milestone_id AS m_id, COALESCE(j.total_tokens, 0) AS tokens
                 FROM agent_session_journal j
                 INNER JOIN project_task_links l ON l.target_id = j.id AND l.link_type = 'journal'
                 INNER JOIN project_tasks t ON t.id = l.task_id
                 WHERE j.project_id = ?1 AND t.project_id = ?1 AND t.milestone_id IS NOT NULL
             )
             GROUP BY m_id",
        )
        .db_context("prepare milestone token sum query")?;
    let rows = stmt
        .query_map(params![project_id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
        .db_context("query milestone token sums")?;
    let mut by_milestone = std::collections::HashMap::new();
    for pair in collect_rows(rows, "read milestone token sum")? {
        by_milestone.insert(pair.0, pair.1);
    }
    Ok((project_total, by_milestone))
}

/// Sums tokens and USD cost already recorded in `agent_session_journal` for `conversation_id`,
/// used to compute per-minute deltas when an agent records multiple minutes in one conversation.
pub fn db_sum_conversation_journal_usage(conversation_id: &str) -> Result<(i64, f64), DatabaseError> {
    let conn = get_connection()?;
    conn.query_row(
        "SELECT COALESCE(SUM(total_tokens), 0), COALESCE(SUM(cost_dollars), 0.0)
         FROM agent_session_journal
         WHERE conversation_id = ?1",
        params![conversation_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .db_context("sum conversation journal usage")
}

/// Aggregates recorded session-minute consumption grouped by `(runner, model)` for a project.
pub fn db_sum_project_usage_by_runner_model(project_id: i64) -> Result<Vec<RunnerModelUsageSummary>, DatabaseError> {
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(
            "SELECT COALESCE(NULLIF(trim(runner), ''), 'unknown') AS r_name,
                    COALESCE(NULLIF(trim(model), ''), 'unknown') AS m_name,
                    COALESCE(SUM(total_tokens), 0) AS tokens_sum,
                    COALESCE(SUM(cost_dollars), 0.0) AS cost_sum,
                    COUNT(*) AS minutes_cnt
             FROM agent_session_journal
             WHERE project_id = ?1
               AND (COALESCE(total_tokens, 0) > 0 OR COALESCE(cost_dollars, 0.0) > 0.0)
             GROUP BY r_name, m_name
             ORDER BY cost_sum DESC, tokens_sum DESC",
        )
        .db_context("prepare project usage by runner/model query")?;
    let rows = stmt
        .query_map(params![project_id], |r| {
            Ok(RunnerModelUsageSummary {
                runner: r.get(0)?,
                model: r.get(1)?,
                total_tokens_count: r.get(2)?,
                total_cost_dollars: r.get(3)?,
                minutes_count: r.get(4)?,
            })
        })
        .db_context("query project usage by runner/model")?;
    collect_rows(rows, "read project usage by runner/model")
}

/// Links a `project_task` to a `Journal` record (`todo` or `journal`), verifying both belong to the same project.
pub fn db_link_task_relation(
    task_id: i64,
    link_type: &str,
    target_id: i64,
) -> Result<i64, DatabaseError> {
    let conn = get_connection()?;
    let task_project_id: i64 = conn
        .query_row(
            "SELECT project_id FROM project_tasks WHERE id = ?1",
            params![task_id],
            |r| r.get(0),
        )
        .db_context("lookup task for relation link")?;

    let target_project_id: i64 = match link_type {
        "todo" => conn
            .query_row(
                "SELECT project_id FROM project_todos WHERE id = ?1",
                params![target_id],
                |r| r.get(0),
            )
            .db_context("lookup target todo for relation link")?,
        "journal" => conn
            .query_row(
                "SELECT project_id FROM agent_session_journal WHERE id = ?1",
                params![target_id],
                |r| r.get(0),
            )
            .db_context("lookup target journal entry for relation link")?,
        other => {
            return Err(DatabaseError::InvalidRequest(format!(
                "Invalid link_type '{}': expected 'todo' or 'journal'",
                other
            )));
        }
    };

    if task_project_id != target_project_id {
        return Err(DatabaseError::InvalidRequest(format!(
            "Cross-project task link rejected: task belongs to project {}, target {} #{} belongs to project {}",
            task_project_id, link_type, target_id, target_project_id
        )));
    }

    let now = now_epoch_secs();
    conn.execute(
        "INSERT OR IGNORE INTO project_task_links (task_id, link_type, target_id, created_at_seconds)
         VALUES (?1, ?2, ?3, ?4)",
        params![task_id, link_type, target_id, now],
    ).db_context("insert project task link")?;
    conn.execute(
        "UPDATE project_tasks SET updated_at_seconds = ?1 WHERE id = ?2",
        params![now, task_id],
    ).db_context("touch project task updated_at")?;
    Ok(task_project_id)
}

/// Removes a relation link between a `project_task` and a `Journal` entity.
pub fn db_unlink_task_relation(
    task_id: i64,
    link_type: &str,
    target_id: i64,
) -> Result<Option<i64>, DatabaseError> {
    let conn = get_connection()?;
    let project_id: Option<i64> = conn
        .query_row(
            "SELECT project_id FROM project_tasks WHERE id = ?1",
            params![task_id],
            |r| r.get(0),
        )
        .optional()
        .db_context("lookup task for relation unlink")?;
    if project_id.is_some() {
        conn.execute(
            "DELETE FROM project_task_links WHERE task_id = ?1 AND link_type = ?2 AND target_id = ?3",
            params![task_id, link_type, target_id],
        ).db_context("delete project task link")?;
        conn.execute(
            "UPDATE project_tasks SET updated_at_seconds = ?1 WHERE id = ?2",
            params![now_epoch_secs(), task_id],
        ).db_context("touch project task updated_at")?;
    }
    Ok(project_id)
}

const TARGET_RUN_COLUMNS: &str = "run_id, project_id, target_type, target_id, status, exit_code, spawned_at_ms,
    terminated_at_ms, failure_reason, profiler_engine_id, profile_artifact_path";

fn target_run_from_row(row: &rusqlite::Row) -> rusqlite::Result<TargetRunRecord> {
    Ok(TargetRunRecord {
        run_id: row.get(0)?,
        project_id: row.get(1)?,
        target_type: row.get(2)?,
        target_id: row.get(3)?,
        status: row.get(4)?,
        exit_code: row.get(5)?,
        spawned_at_ms: row.get(6)?,
        terminated_at_ms: row.get(7)?,
        failure_reason: row.get(8)?,
        profiler_engine_id: row.get(9)?,
        profile_artifact_path: row.get(10)?,
    })
}

/// The newest `limit_count` service and task runs of a project (or of one target), newest first.
pub fn db_get_target_runs(project_id: i64, target_id: Option<&str>, limit_count: u32) -> Result<Vec<TargetRunRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_target_runs: project_id={}, target_id={:?}, limit_count={}", project_id, target_id, limit_count));
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {} FROM supervised_processes
             WHERE project_id = ?1 AND target_type IN ('service', 'task') AND (?2 IS NULL OR target_id = ?2)
             ORDER BY spawned_at_ms DESC
             LIMIT ?3",
            TARGET_RUN_COLUMNS
        ))
        .db_context("prepare target runs query")?;
    let rows = stmt
        .query_map(params![project_id, target_id, limit_count], target_run_from_row)
        .db_context("query target runs")?;
    let list = collect_rows(rows, "read target run row")?;
    crate::logger::log("DEBUG", "db", &format!("db_get_target_runs: returning {} runs", list.len()));
    Ok(list)
}

/// One service or task run by id.
pub fn db_get_target_run(run_id: &str) -> Result<Option<TargetRunRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_target_run: run_id={}", run_id));
    let conn = get_connection()?;
    conn.query_row(
        &format!(
            "SELECT {} FROM supervised_processes WHERE run_id = ?1 AND target_type IN ('service', 'task')",
            TARGET_RUN_COLUMNS
        ),
        params![run_id],
        target_run_from_row,
    )
    .optional()
    .db_context("query target run")
}

/// Adds `file_path` to the Conversation's `artifacts_touched` JSON list (no duplicates).
pub fn db_record_artifact_touched(conversation_id: &str, file_path: &str) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_record_artifact_touched: conversation_id={}, file_path={}", conversation_id, file_path));
    let conn = get_connection()?;
    let current_raw: Option<String> = conn
        .query_row("SELECT artifacts_touched FROM agent_conversations WHERE id = ?1", params![conversation_id], |r| r.get(0))
        .optional()
        .db_context("read artifacts_touched")?
        .ok_or_else(|| DatabaseError::NotFound { entity: "Conversation", id: conversation_id.to_string() })?;

    let mut paths: Vec<String> = match current_raw {
        Some(raw) => serde_json::from_str(&raw)
            .map_err(|e| DatabaseError::Serialization { operation: "decode artifacts_touched".to_string(), source: e })?,
        None => Vec::new(),
    };
    if paths.iter().any(|p| p == file_path) {
        return Ok(());
    }
    paths.push(file_path.to_string());
    let updated = serde_json::to_string(&paths)
        .map_err(|e| DatabaseError::Serialization { operation: "encode artifacts_touched".to_string(), source: e })?;
    conn.execute(
        "UPDATE agent_conversations SET artifacts_touched = ?1 WHERE id = ?2",
        params![updated, conversation_id],
    ).db_context("update artifacts_touched")?;
    crate::logger::log("INFO", "db", &format!("db_record_artifact_touched: recorded touched artifact {} for conversation_id={}", file_path, conversation_id));
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Spec 23: Agent Session Metrics & Multi-Account Quota CRUD
// ─────────────────────────────────────────────────────────────────────────────

pub fn db_upsert_session_metrics(rec: &AgentSessionMetricsRecord) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_upsert_session_metrics: agy_conversation_id={}, project_id={}", rec.agy_conversation_id, rec.project_id));
    let conn = get_connection()?;
    conn.execute(
        "INSERT INTO agent_session_metrics (
            agy_conversation_id, conversation_id, project_id, agent_id,
            provider, model, context_used_pct, fresh_tokens_count, cache_read_tokens_count,
            cache_creation_tokens_count, total_tokens_count, cache_hit_pct,
            five_hour_used_pct, five_hour_resets_at, five_hour_reset_epoch_seconds,
            weekly_used_pct, weekly_resets_at, weekly_reset_epoch_seconds, updated_at,
            runner, cost_dollars, max_context_tokens_count
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)
        ON CONFLICT(agy_conversation_id) DO UPDATE SET
            context_used_pct = excluded.context_used_pct,
            fresh_tokens_count = excluded.fresh_tokens_count,
            cache_read_tokens_count = excluded.cache_read_tokens_count,
            cache_creation_tokens_count = excluded.cache_creation_tokens_count,
            total_tokens_count = excluded.total_tokens_count,
            cache_hit_pct = excluded.cache_hit_pct,
            five_hour_used_pct = excluded.five_hour_used_pct,
            five_hour_resets_at = excluded.five_hour_resets_at,
            five_hour_reset_epoch_seconds = excluded.five_hour_reset_epoch_seconds,
            weekly_used_pct = excluded.weekly_used_pct,
            weekly_resets_at = excluded.weekly_resets_at,
            weekly_reset_epoch_seconds = excluded.weekly_reset_epoch_seconds,
            model = coalesce(excluded.model, agent_session_metrics.model),
            provider = excluded.provider,
            updated_at = excluded.updated_at,
            runner = coalesce(excluded.runner, agent_session_metrics.runner),
            cost_dollars = coalesce(excluded.cost_dollars, agent_session_metrics.cost_dollars),
            max_context_tokens_count = coalesce(excluded.max_context_tokens_count, agent_session_metrics.max_context_tokens_count)",
        params![
            rec.agy_conversation_id,
            rec.conversation_id,
            rec.project_id,
            rec.agent_id,
            rec.provider,
            rec.model,
            rec.context_used_pct,
            rec.fresh_tokens_count,
            rec.cache_read_tokens_count,
            rec.cache_creation_tokens_count,
            rec.total_tokens_count,
            rec.cache_hit_pct,
            rec.five_hour_used_pct,
            rec.five_hour_resets_at,
            rec.five_hour_reset_epoch_seconds,
            rec.weekly_used_pct,
            rec.weekly_resets_at,
            rec.weekly_reset_epoch_seconds,
            rec.updated_at,
            rec.runner,
            rec.cost_dollars,
            rec.max_context_tokens_count,
        ],
    ).db_context("upsert agent_session_metrics")?;
    crate::logger::log("INFO", "db", &format!("db_upsert_session_metrics: upserted session metrics for agy_conversation_id={}", rec.agy_conversation_id));
    Ok(())
}

/// Persists live runner usage (tokens, cost, context window) for the active Conversation of an agent.
pub fn db_upsert_live_session_usage(
    project_path: &str,
    agent_id: &str,
    runner: &str,
    model: Option<&str>,
    tokens_used_count: u64,
    max_context_tokens_count: Option<u64>,
    cost_dollars: Option<f64>,
) -> Result<(), DatabaseError> {
    let Some(project) = db_get_project_by_path(project_path)? else {
        return Ok(());
    };
    let project_id = project.id as i64;
    let conv = db_get_or_create_active_conversation(
        project_id,
        agent_id,
        None,
        Some(runner),
        model,
        None,
        None,
    )?;
    let native_id = db_latest_native_session_for_conversation(&conv.id)?;
    let key = native_id.unwrap_or_else(|| conv.id.clone());
    let context_used_pct = max_context_tokens_count
        .filter(|&w| w > 0)
        .map(|w| (tokens_used_count as f64 / w as f64 * 100.0).clamp(0.0, 100.0))
        .unwrap_or(0.0);
    let provider = match runner {
        "opencode" => "deepseek",
        "claude" => "claude",
        _ => "antigravity",
    };
    let now = now_epoch_secs();
    let conn = get_connection()?;
    conn.execute(
        "INSERT INTO agent_session_metrics (
            agy_conversation_id, conversation_id, project_id, agent_id,
            provider, model, context_used_pct, fresh_tokens_count, cache_read_tokens_count,
            cache_creation_tokens_count, total_tokens_count, cache_hit_pct,
            updated_at, runner, cost_dollars, max_context_tokens_count
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, 0, 0, ?8, 0.0, ?9, ?10, ?11, ?12)
        ON CONFLICT(agy_conversation_id) DO UPDATE SET
            provider = excluded.provider,
            model = coalesce(excluded.model, agent_session_metrics.model),
            context_used_pct = CASE WHEN excluded.context_used_pct > 0.0 THEN excluded.context_used_pct ELSE agent_session_metrics.context_used_pct END,
            total_tokens_count = CASE WHEN excluded.total_tokens_count > 0 THEN excluded.total_tokens_count ELSE agent_session_metrics.total_tokens_count END,
            updated_at = excluded.updated_at,
            runner = excluded.runner,
            cost_dollars = coalesce(excluded.cost_dollars, agent_session_metrics.cost_dollars),
            max_context_tokens_count = coalesce(excluded.max_context_tokens_count, agent_session_metrics.max_context_tokens_count)",
        params![
            key,
            conv.id,
            project_id,
            agent_id,
            provider,
            model,
            context_used_pct,
            tokens_used_count as i64,
            now,
            runner,
            cost_dollars,
            max_context_tokens_count.map(|v| v as i64),
        ],
    )
    .db_context("upsert live session usage")?;
    Ok(())
}

/// Newest native conversation id recorded for a ONE-SHIP Conversation.
pub fn db_latest_native_session_for_conversation(conversation_id: &str) -> Result<Option<String>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_latest_native_session_for_conversation: conversation_id={}", conversation_id));
    let conn = get_connection()?;
    let from_native: Option<String> = conn.query_row(
        "SELECT native_session_id FROM conversation_native_sessions
         WHERE conversation_id = ?1 AND native_session_id IS NOT NULL AND native_session_id != ''
         ORDER BY id DESC LIMIT 1",
        params![conversation_id],
        |r| r.get(0),
    )
    .optional()
    .db_context("latest native session from conversation_native_sessions")?;

    if from_native.is_some() {
        return Ok(from_native);
    }

    conn.query_row(
        "SELECT agy_conversation_id FROM agent_chat_turns
         WHERE conversation_id = ?1 AND agy_conversation_id IS NOT NULL AND agy_conversation_id != ''
         ORDER BY timestamp DESC LIMIT 1",
        params![conversation_id],
        |r| r.get(0),
    )
    .optional()
    .db_context("latest native session of conversation")
}

/// Newest native conversation id recorded for one agent of one project.
pub fn db_latest_native_session_for_agent(project_id: i64, agent_id: &str) -> Result<Option<String>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_latest_native_session_for_agent: project_id={}, agent_id={}", project_id, agent_id));
    let conn = get_connection()?;
    let from_native: Option<String> = conn.query_row(
        "SELECT ns.native_session_id FROM conversation_native_sessions ns
         JOIN agent_conversations c ON c.id = ns.conversation_id
         WHERE c.project_id = ?1 AND c.agent_id = ?2 AND ns.native_session_id IS NOT NULL AND ns.native_session_id != ''
         ORDER BY ns.id DESC LIMIT 1",
        params![project_id, agent_id],
        |r| r.get(0),
    )
    .optional()
    .db_context("latest native session from conversation_native_sessions")?;

    if from_native.is_some() {
        return Ok(from_native);
    }

    conn.query_row(
        "SELECT agy_conversation_id FROM agent_chat_turns
         WHERE project_id = ?1 AND agent_id = ?2 AND agy_conversation_id IS NOT NULL AND agy_conversation_id != ''
         ORDER BY timestamp DESC LIMIT 1",
        params![project_id, agent_id],
        |r| r.get(0),
    )
    .optional()
    .db_context("latest native session of agent")
}

pub fn db_get_session_metrics(agy_conversation_id: &str) -> Result<Option<AgentSessionMetricsRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_session_metrics: agy_conversation_id={}", agy_conversation_id));
    let conn = get_connection()?;
    let mut stmt = conn.prepare(
        "SELECT agy_conversation_id, conversation_id, project_id, agent_id,
                provider, model, context_used_pct, fresh_tokens_count, cache_read_tokens_count,
                cache_creation_tokens_count, total_tokens_count, cache_hit_pct,
                five_hour_used_pct, five_hour_resets_at, five_hour_reset_epoch_seconds,
                weekly_used_pct, weekly_resets_at, weekly_reset_epoch_seconds, updated_at,
                runner, cost_dollars, max_context_tokens_count
         FROM agent_session_metrics WHERE agy_conversation_id = ?1"
    ).db_context("prepare session metrics query")?;

    stmt.query_row(params![agy_conversation_id], |r| {
        Ok(AgentSessionMetricsRecord {
            agy_conversation_id: r.get(0)?,
            conversation_id: r.get(1)?,
            project_id: r.get(2)?,
            agent_id: r.get(3)?,
            provider: r.get(4)?,
            model: r.get(5)?,
            context_used_pct: r.get(6)?,
            fresh_tokens_count: r.get(7)?,
            cache_read_tokens_count: r.get(8)?,
            cache_creation_tokens_count: r.get(9)?,
            total_tokens_count: r.get(10)?,
            cache_hit_pct: r.get(11)?,
            five_hour_used_pct: r.get(12)?,
            five_hour_resets_at: r.get(13)?,
            five_hour_reset_epoch_seconds: r.get(14)?,
            weekly_used_pct: r.get(15)?,
            weekly_resets_at: r.get(16)?,
            weekly_reset_epoch_seconds: r.get(17)?,
            updated_at: r.get(18)?,
            runner: r.get(19)?,
            cost_dollars: r.get(20)?,
            max_context_tokens_count: r.get(21)?,
        })
    })
    .optional()
    .db_context("query session metrics")
}

/// Looks up the most recently updated `agent_session_metrics` row for a ONE-SHIP `conversation_id`.
pub fn db_get_session_metrics_by_conversation(conversation_id: &str) -> Result<Option<AgentSessionMetricsRecord>, DatabaseError> {
    let conn = get_connection()?;
    let mut stmt = conn.prepare(
        "SELECT agy_conversation_id, conversation_id, project_id, agent_id,
                provider, model, context_used_pct, fresh_tokens_count, cache_read_tokens_count,
                cache_creation_tokens_count, total_tokens_count, cache_hit_pct,
                five_hour_used_pct, five_hour_resets_at, five_hour_reset_epoch_seconds,
                weekly_used_pct, weekly_resets_at, weekly_reset_epoch_seconds, updated_at,
                runner, cost_dollars, max_context_tokens_count
         FROM agent_session_metrics WHERE conversation_id = ?1
         ORDER BY updated_at DESC LIMIT 1"
    ).db_context("prepare session metrics by conversation query")?;

    stmt.query_row(params![conversation_id], |r| {
        Ok(AgentSessionMetricsRecord {
            agy_conversation_id: r.get(0)?,
            conversation_id: r.get(1)?,
            project_id: r.get(2)?,
            agent_id: r.get(3)?,
            provider: r.get(4)?,
            model: r.get(5)?,
            context_used_pct: r.get(6)?,
            fresh_tokens_count: r.get(7)?,
            cache_read_tokens_count: r.get(8)?,
            cache_creation_tokens_count: r.get(9)?,
            total_tokens_count: r.get(10)?,
            cache_hit_pct: r.get(11)?,
            five_hour_used_pct: r.get(12)?,
            five_hour_resets_at: r.get(13)?,
            five_hour_reset_epoch_seconds: r.get(14)?,
            weekly_used_pct: r.get(15)?,
            weekly_resets_at: r.get(16)?,
            weekly_reset_epoch_seconds: r.get(17)?,
            updated_at: r.get(18)?,
            runner: r.get(19)?,
            cost_dollars: r.get(20)?,
            max_context_tokens_count: r.get(21)?,
        })
    })
    .optional()
    .db_context("query session metrics by conversation")
}

pub fn db_upsert_account_quota(rec: &AccountQuotaRecord) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_upsert_account_quota: account_id={}, provider={}, model={}", rec.account_id, rec.provider, rec.model));
    let conn = get_connection()?;
    conn.execute(
        "INSERT INTO account_quotas (
            account_id, provider, model, five_hour_remaining_pct, five_hour_resets_at,
            five_hour_reset_epoch_seconds, five_hour_resets_at_local, weekly_remaining_pct, weekly_resets_at,
            weekly_reset_epoch_seconds, weekly_resets_at_local, last_pinged_at, warmup_enabled, status, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
        ON CONFLICT(account_id, provider, model) DO UPDATE SET
            five_hour_remaining_pct = excluded.five_hour_remaining_pct,
            five_hour_resets_at = excluded.five_hour_resets_at,
            five_hour_reset_epoch_seconds = excluded.five_hour_reset_epoch_seconds,
            five_hour_resets_at_local = excluded.five_hour_resets_at_local,
            weekly_remaining_pct = excluded.weekly_remaining_pct,
            weekly_resets_at = excluded.weekly_resets_at,
            weekly_reset_epoch_seconds = excluded.weekly_reset_epoch_seconds,
            weekly_resets_at_local = excluded.weekly_resets_at_local,
            last_pinged_at = coalesce(excluded.last_pinged_at, account_quotas.last_pinged_at),
            warmup_enabled = account_quotas.warmup_enabled,
            status = excluded.status,
            updated_at = excluded.updated_at",
        params![
            rec.account_id,
            rec.provider,
            rec.model,
            rec.five_hour_remaining_pct,
            rec.five_hour_resets_at,
            rec.five_hour_reset_epoch_seconds,
            rec.five_hour_resets_at_local,
            rec.weekly_remaining_pct,
            rec.weekly_resets_at,
            rec.weekly_reset_epoch_seconds,
            rec.weekly_resets_at_local,
            rec.last_pinged_at,
            if rec.warmup_enabled { 1 } else { 0 },
            rec.status,
            rec.updated_at,
        ],
    ).db_context("upsert account_quotas")?;
    crate::logger::log("DEBUG", "db", &format!("db_upsert_account_quota: upserted quota for account_id={}", rec.account_id));
    Ok(())
}

const ACCOUNT_QUOTA_COLUMNS: &str = "id, account_id, provider, model, five_hour_remaining_pct,
    five_hour_resets_at, five_hour_reset_epoch_seconds, five_hour_resets_at_local,
    weekly_remaining_pct, weekly_resets_at, weekly_reset_epoch_seconds,
    weekly_resets_at_local, last_pinged_at, warmup_enabled, status, updated_at";

fn account_quota_from_row(r: &rusqlite::Row) -> rusqlite::Result<AccountQuotaRecord> {
    Ok(AccountQuotaRecord {
        id: Some(r.get(0)?),
        account_id: r.get(1)?,
        provider: r.get(2)?,
        model: r.get(3)?,
        five_hour_remaining_pct: r.get(4)?,
        five_hour_resets_at: r.get(5)?,
        five_hour_reset_epoch_seconds: r.get(6)?,
        five_hour_resets_at_local: r.get(7)?,
        weekly_remaining_pct: r.get(8)?,
        weekly_resets_at: r.get(9)?,
        weekly_reset_epoch_seconds: r.get(10)?,
        weekly_resets_at_local: r.get(11)?,
        last_pinged_at: r.get(12)?,
        warmup_enabled: r.get::<_, i64>(13)? == 1,
        status: r.get(14)?,
        updated_at: r.get(15)?,
    })
}

pub fn db_get_account_quotas() -> Result<Vec<AccountQuotaRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", "db_get_account_quotas: fetching all quotas");
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(&format!("SELECT {} FROM account_quotas ORDER BY updated_at DESC", ACCOUNT_QUOTA_COLUMNS))
        .db_context("prepare account quotas query")?;
    let rows = stmt.query_map([], account_quota_from_row).db_context("query account quotas")?;
    let list = collect_rows(rows, "read account quota row")?;
    crate::logger::log("DEBUG", "db", &format!("db_get_account_quotas: returning {} quotas", list.len()));
    Ok(list)
}

pub fn db_get_account_quota(account_id: &str, provider: &str, model: &str) -> Result<Option<AccountQuotaRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_account_quota: account_id={}, provider={}, model={}", account_id, provider, model));
    let conn = get_connection()?;
    conn.query_row(
        &format!("SELECT {} FROM account_quotas WHERE account_id = ?1 AND provider = ?2 AND model = ?3", ACCOUNT_QUOTA_COLUMNS),
        params![account_id, provider, model],
        account_quota_from_row,
    )
    .optional()
    .db_context("query account quota")
}

pub fn db_toggle_account_warmup(account_id: &str, enabled: bool) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_toggle_account_warmup: account_id={}, enabled={}", account_id, enabled));
    let conn = get_connection()?;
    let val = if enabled { 1 } else { 0 };
    conn.execute(
        "UPDATE account_quotas SET warmup_enabled = ?1 WHERE account_id = ?2",
        params![val, account_id],
    ).db_context("toggle account warmup")?;
    crate::logger::log("INFO", "db", &format!("db_toggle_account_warmup: toggled warmup for account_id={} to {}", account_id, enabled));
    Ok(())
}

pub fn db_update_account_quota_ping(account_id: &str, now: i64) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_update_account_quota_ping: account_id={}, now={}", account_id, now));
    let conn = get_connection()?;
    conn.execute(
        "UPDATE account_quotas SET last_pinged_at = ?1, updated_at = ?1 WHERE account_id = ?2",
        params![now, account_id],
    ).db_context("update last_pinged_at")?;
    crate::logger::log("INFO", "db", &format!("db_update_account_quota_ping: updated ping timestamp for account_id={}", account_id));
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Document & Code Review Comments (Spec 20 Phase 3: Inline feedback loop)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct DocumentCommentRecord {
    pub id: String,
    #[ts(type = "number")]
    pub project_id: i64,
    pub file_path: String,
    #[ts(type = "number")]
    pub start_line: i64,
    #[ts(type = "number")]
    pub end_line: i64,
    pub selected_text: String,
    pub comment_text: String,
    pub author: String,
    pub status: String,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number | null")]
    pub resolved_at: Option<i64>,
    pub resolved_by: Option<String>,
}

pub fn db_insert_document_comment(record: &DocumentCommentRecord) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_insert_document_comment: id={}, project_id={}, file_path={}:{}", record.id, record.project_id, record.file_path, record.start_line));
    let conn = get_connection()?;
    conn.execute(
        "INSERT INTO document_comments (
            id, project_id, file_path, start_line, end_line, selected_text,
            comment_text, author, status, created_at, resolved_at, resolved_by
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            record.id,
            record.project_id,
            record.file_path,
            record.start_line,
            record.end_line,
            record.selected_text,
            record.comment_text,
            record.author,
            record.status,
            record.created_at,
            record.resolved_at,
            record.resolved_by,
        ],
    ).db_context("insert document comment")?;
    crate::logger::log("INFO", "db", &format!("db_insert_document_comment: inserted document comment id={}", record.id));
    Ok(())
}

pub fn db_list_document_comments(
    project_id: i64,
    file_path: Option<&str>,
    status: Option<&str>,
) -> Result<Vec<DocumentCommentRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_list_document_comments: project_id={}, file_path={:?}, status={:?}", project_id, file_path, status));
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, file_path, start_line, end_line, selected_text, comment_text, author, status, created_at, resolved_at, resolved_by
             FROM document_comments
             WHERE project_id = ?1 AND (?2 IS NULL OR file_path = ?2) AND (?3 IS NULL OR status = ?3)
             ORDER BY file_path ASC, start_line ASC, created_at ASC",
        )
        .db_context("prepare document comments query")?;

    let rows = stmt.query_map(params![project_id, file_path, status], |r| {
        Ok(DocumentCommentRecord {
            id: r.get(0)?,
            project_id: r.get(1)?,
            file_path: r.get(2)?,
            start_line: r.get(3)?,
            end_line: r.get(4)?,
            selected_text: r.get(5)?,
            comment_text: r.get(6)?,
            author: r.get(7)?,
            status: r.get(8)?,
            created_at: r.get(9)?,
            resolved_at: r.get(10)?,
            resolved_by: r.get(11)?,
        })
    }).db_context("query document comments")?;
    let list = collect_rows(rows, "read document comment row")?;
    crate::logger::log("DEBUG", "db", &format!("db_list_document_comments: returning {} comments", list.len()));
    Ok(list)
}

/// Returns open (`status = 'open'`) comment counts keyed by normalized `file_path` for a project.
pub fn db_get_open_comment_counts_by_file(
    project_id: i64,
) -> Result<std::collections::HashMap<String, u32>, DatabaseError> {
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(
            "SELECT file_path, COUNT(*)
             FROM document_comments
             WHERE project_id = ?1 AND status = 'open'
             GROUP BY file_path",
        )
        .db_context("prepare open comment counts query")?;
    let rows = stmt
        .query_map(params![project_id], |r| {
            let file_path: String = r.get(0)?;
            let count: i64 = r.get(1)?;
            let safe_count = if count <= 0 {
                0
            } else {
                u32::try_from(count).map_or(u32::MAX, |v| v)
            };
            Ok((file_path, safe_count))
        })
        .db_context("query open comment counts")?;
    let pairs = collect_rows(rows, "read open comment count row")?;
    Ok(pairs.into_iter().collect())
}

pub fn db_resolve_document_comment(
    comment_id: &str,
    resolved_by: Option<&str>,
    now: i64,
) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_resolve_document_comment: comment_id={}, resolved_by={:?}", comment_id, resolved_by));
    let conn = get_connection()?;
    let rows_affected = conn.execute(
        "UPDATE document_comments SET status = 'resolved', resolved_at = ?1, resolved_by = ?2 WHERE id = ?3",
        params![now, resolved_by, comment_id],
    ).db_context("resolve document comment")?;

    if rows_affected == 0 {
        crate::logger::log("WARN", "db", &format!("db_resolve_document_comment: document comment '{}' not found", comment_id));
        return Err(DatabaseError::NotFound { entity: "Document comment", id: comment_id.to_string() });
    }
    crate::logger::log("INFO", "db", &format!("db_resolve_document_comment: resolved comment_id={}", comment_id));
    Ok(())
}

/// Resolves a document comment strictly scoped to `project_id`.
pub fn db_resolve_document_comment_scoped(
    project_id: i64,
    comment_id: &str,
    resolved_by: Option<&str>,
    now: i64,
) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_resolve_document_comment_scoped: project_id={}, comment_id={}", project_id, comment_id));
    let conn = get_connection()?;
    let rows_affected = conn.execute(
        "UPDATE document_comments SET status = 'resolved', resolved_at = ?1, resolved_by = ?2 WHERE id = ?3 AND project_id = ?4",
        params![now, resolved_by, comment_id, project_id],
    ).db_context("resolve scoped document comment")?;
    if rows_affected == 0 {
        return Err(DatabaseError::NotFound { entity: "Document comment", id: comment_id.to_string() });
    }
    crate::logger::log("INFO", "db", &format!("db_resolve_document_comment_scoped: resolved comment_id={} in project_id={}", comment_id, project_id));
    Ok(())
}

pub fn db_delete_document_comment(comment_id: &str) -> Result<(), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_delete_document_comment: comment_id={}", comment_id));
    let conn = get_connection()?;
    conn.execute("DELETE FROM document_comments WHERE id = ?1", params![comment_id])
        .db_context("delete document comment")?;
    crate::logger::log("INFO", "db", &format!("db_delete_document_comment: deleted comment_id={}", comment_id));
    Ok(())
}

// --- Project Initialization Flags ---

pub fn db_insert_init_flag(record: &NewInitFlag) -> Result<i64, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_insert_init_flag: project_id={}, flag_type={}, title={}", record.project_id, record.flag_type, record.title));
    let conn = get_connection()?;
    conn.execute(
        "INSERT INTO project_initialization_flags (project_id, flag_type, title, description, recommended_option, options_json, metadata_json, status, created_at, resolved_at, raised_by_agent_uid, resolved_by_agent_uid)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8, NULL, ?9, NULL)",
        params![
            record.project_id,
            record.flag_type,
            record.title,
            record.description,
            record.recommended_option,
            record.options_json,
            record.metadata_json,
            now_epoch_secs(),
            record.raised_by_agent_uid,
        ],
    ).db_context("insert init flag")?;

    let row_id = conn.last_insert_rowid();
    crate::logger::log("INFO", "db", &format!("db_insert_init_flag: inserted init flag id={} for project_id={}", row_id, record.project_id));

    let sup = crate::supervisor::get_supervisor();
    sup.emit_event(crate::events::DaemonEvent::init_flag_raised(&crate::events::InitFlagRaisedPayload {
        flag_id: row_id,
        project_id: record.project_id,
        flag_type: record.flag_type.clone(),
    }));

    Ok(row_id)
}

pub fn db_get_init_flags(project_id: i64, status: Option<&str>) -> Result<Vec<ProjectInitFlagRecord>, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_init_flags: project_id={}, status={:?}", project_id, status));
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, flag_type, title, description, recommended_option, options_json, metadata_json, status, created_at, resolved_at, raised_by_agent_uid, resolved_by_agent_uid
             FROM project_initialization_flags
             WHERE project_id = ?1 AND (?2 IS NULL OR status = ?2)
             ORDER BY created_at DESC",
        )
        .db_context("prepare init flags query")?;

    let rows = stmt.query_map(params![project_id, status], init_flag_from_row).db_context("query init flags")?;
    let list = collect_rows(rows, "read init flag row")?;
    crate::logger::log("DEBUG", "db", &format!("db_get_init_flags: returning {} flags", list.len()));
    Ok(list)
}

/// The flag with this id; an unknown id is `DatabaseError::NotFound`.
pub fn db_get_init_flag(id: i64) -> Result<ProjectInitFlagRecord, DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_get_init_flag: id={}", id));
    let conn = get_connection()?;
    conn.query_row(
        "SELECT id, project_id, flag_type, title, description, recommended_option, options_json, metadata_json, status, created_at, resolved_at, raised_by_agent_uid, resolved_by_agent_uid
         FROM project_initialization_flags
         WHERE id = ?1",
        params![id],
        init_flag_from_row,
    )
    .optional()
    .db_context("read init flag")?
    .ok_or_else(|| DatabaseError::NotFound { entity: "Initialization flag", id: id.to_string() })
}

fn init_flag_from_row(row: &rusqlite::Row) -> rusqlite::Result<ProjectInitFlagRecord> {
    Ok(ProjectInitFlagRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        flag_type: row.get(2)?,
        title: row.get(3)?,
        description: row.get(4)?,
        recommended_option: row.get(5)?,
        options_json: row.get(6)?,
        metadata_json: row.get(7)?,
        status: row.get(8)?,
        created_at: row.get(9)?,
        resolved_at: row.get(10)?,
        raised_by_agent_uid: row.get(11)?,
        resolved_by_agent_uid: row.get(12)?,
    })
}

/// Marks a flag resolved/dismissed and returns `(flag_type, project_id, project_path)`.
/// An unknown flag id is an error, not a silent no-op.
pub fn db_resolve_init_flag(id: i64, status: &str, resolved_by_agent_uid: Option<&str>) -> Result<(String, i64, String), DatabaseError> {
    crate::logger::log("DEBUG", "db", &format!("db_resolve_init_flag: id={}, status={}, resolved_by={:?}", id, status, resolved_by_agent_uid));
    let conn = get_connection()?;
    let flag_meta: (String, i64, String) = conn
        .query_row(
            "SELECT f.flag_type, f.project_id, COALESCE(p.path, '')
             FROM project_initialization_flags f
             LEFT JOIN projects p ON f.project_id = p.id
             WHERE f.id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .db_context("read init flag")?
        .ok_or_else(|| DatabaseError::NotFound { entity: "Initialization flag", id: id.to_string() })?;

    conn.execute(
        "UPDATE project_initialization_flags SET status = ?1, resolved_at = ?2, resolved_by_agent_uid = ?3 WHERE id = ?4",
        params![status, now_epoch_secs(), resolved_by_agent_uid, id],
    ).db_context("resolve init flag")?;
    drop(conn);
    crate::logger::log("INFO", "db", &format!("db_resolve_init_flag: flag id {} marked as {} (by {:?})", id, status, resolved_by_agent_uid));

    let (ref flag_type, project_id, ref project_path) = flag_meta;

    crate::supervisor::get_supervisor().emit_event(crate::events::DaemonEvent::init_flag_resolved(&crate::events::InitFlagResolvedPayload {
        flag_id: id,
        status: status.to_string(),
        project_id,
        project_path: project_path.clone(),
        flag_type: flag_type.clone(),
        agent_id: None,
    }));

    Ok(flag_meta)
}

/// Arguments of `get_project_init_flags`; `status` filters (`pending`, `resolved`, `dismissed`).
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct GetProjectInitFlagsArgs {
    #[ts(type = "number")]
    pub project_id: i64,
    pub status: Option<String>,
}

/// Arguments of `resolve_project_init_flag`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ResolveProjectInitFlagArgs {
    #[ts(type = "number")]
    pub id: i64,
    pub status: String,
}

/// Persisted global UI state row (`ui_global_state`, Spec 35).
#[derive(Clone, Debug)]
pub struct DbUiGlobalState {
    pub active_project_id: Option<i64>,
    pub chat_layout_mode: String,
    pub docked_chat_width_vw: f64,
    pub revision: i64,
    pub updated_at_seconds: i64,
}

/// Persisted per-project UI state row (`ui_project_state`, Spec 35).
#[derive(Clone, Debug)]
pub struct DbUiProjectState {
    pub project_id: i64,
    pub active_node_path: String,
    pub slot_values_json: String,
    pub chat_panel_state: String,
    pub active_agent_id: Option<String>,
    pub active_conversation_id: Option<String>,
    pub updated_at_seconds: i64,
}

/// Loads the singleton global UI state row (`id = 1`), returning `None` if not yet persisted.
pub fn db_get_ui_global_state() -> Result<Option<DbUiGlobalState>, DatabaseError> {
    let conn = get_connection()?;
    conn.query_row(
        "SELECT active_project_id, chat_layout_mode, docked_chat_width_vw, revision, updated_at_seconds
         FROM ui_global_state WHERE id = 1",
        [],
        |row| {
            Ok(DbUiGlobalState {
                active_project_id: row.get(0)?,
                chat_layout_mode: row.get(1)?,
                docked_chat_width_vw: row.get(2)?,
                revision: row.get(3)?,
                updated_at_seconds: row.get(4)?,
            })
        },
    )
    .optional()
    .db_context("read ui_global_state")
}

/// Loads all persisted per-project UI state rows (`ui_project_state`).
pub fn db_get_all_ui_project_states() -> Result<Vec<DbUiProjectState>, DatabaseError> {
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(
            "SELECT project_id, active_node_path, slot_values_json, chat_panel_state,
                    active_agent_id, active_conversation_id, updated_at_seconds
             FROM ui_project_state",
        )
        .db_context("prepare ui_project_state query")?;
    let rows = stmt
        .query_map([], |row| {
            Ok(DbUiProjectState {
                project_id: row.get(0)?,
                active_node_path: row.get(1)?,
                slot_values_json: row.get(2)?,
                chat_panel_state: row.get(3)?,
                active_agent_id: row.get(4)?,
                active_conversation_id: row.get(5)?,
                updated_at_seconds: row.get(6)?,
            })
        })
        .db_context("query ui_project_state")?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.db_context("map ui_project_state row")?);
    }
    Ok(out)
}

/// Atomically persists global UI state and an optional updated project UI state in a single transaction.
pub fn db_save_ui_state_atomic(
    global: &DbUiGlobalState,
    project: Option<&DbUiProjectState>,
) -> Result<(), DatabaseError> {
    let mut conn = get_connection()?;
    let tx = conn.transaction().db_context("begin ui_state transaction")?;

    tx.execute(
        "INSERT INTO ui_global_state (id, active_project_id, chat_layout_mode, docked_chat_width_vw, revision, updated_at_seconds)
         VALUES (1, ?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET
            active_project_id = excluded.active_project_id,
            chat_layout_mode = excluded.chat_layout_mode,
            docked_chat_width_vw = excluded.docked_chat_width_vw,
            revision = excluded.revision,
            updated_at_seconds = excluded.updated_at_seconds",
        rusqlite::params![
            global.active_project_id,
            global.chat_layout_mode,
            global.docked_chat_width_vw,
            global.revision,
            global.updated_at_seconds,
        ],
    )
    .db_context("upsert ui_global_state")?;

    if let Some(proj) = project {
        tx.execute(
            "INSERT INTO ui_project_state (
                project_id, active_node_path, slot_values_json, chat_panel_state,
                active_agent_id, active_conversation_id, updated_at_seconds
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(project_id) DO UPDATE SET
                active_node_path = excluded.active_node_path,
                slot_values_json = excluded.slot_values_json,
                chat_panel_state = excluded.chat_panel_state,
                active_agent_id = excluded.active_agent_id,
                active_conversation_id = excluded.active_conversation_id,
                updated_at_seconds = excluded.updated_at_seconds",
            rusqlite::params![
                proj.project_id,
                proj.active_node_path,
                proj.slot_values_json,
                proj.chat_panel_state,
                proj.active_agent_id,
                proj.active_conversation_id,
                proj.updated_at_seconds,
            ],
        )
        .db_context("upsert ui_project_state")?;
    }

    tx.commit().db_context("commit ui_state transaction")?;
    Ok(())
}

/// Persisted Forge inbox read/triage state row (Spec 38 / Spec 39 §9).
#[derive(Debug, Clone)]
pub struct ForgeInboxDbRow {
    pub project_id: i64,
    pub provider: String,
    pub repo: String,
    pub item_kind: String,
    pub item_number: i64,
    pub provider_node_id: Option<String>,
    pub last_seen_updated_at: Option<String>,
    pub is_read: bool,
    pub triage_flag: Option<String>,
    pub updated_epoch_seconds: i64,
}

/// Lists persisted Forge inbox rows for `project_id`, optionally filtered by `provider`.
pub fn db_list_forge_inbox_items(
    project_id: i64,
    provider: Option<&str>,
) -> Result<Vec<ForgeInboxDbRow>, DatabaseError> {
    let conn = get_connection()?;
    let sql = if provider.is_some() {
        "SELECT project_id, provider, repo, item_kind, item_number, provider_node_id,
                last_seen_updated_at, is_read, triage_flag, updated_epoch_seconds
         FROM forge_inbox_items
         WHERE project_id = ?1 AND provider = ?2
         ORDER BY updated_epoch_seconds DESC, item_number DESC"
    } else {
        "SELECT project_id, provider, repo, item_kind, item_number, provider_node_id,
                last_seen_updated_at, is_read, triage_flag, updated_epoch_seconds
         FROM forge_inbox_items
         WHERE project_id = ?1
         ORDER BY updated_epoch_seconds DESC, item_number DESC"
    };
    let mut stmt = conn.prepare(sql).db_context("prepare db_list_forge_inbox_items")?;
    let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<ForgeInboxDbRow> {
        Ok(ForgeInboxDbRow {
            project_id: row.get(0)?,
            provider: row.get(1)?,
            repo: row.get(2)?,
            item_kind: row.get(3)?,
            item_number: row.get(4)?,
            provider_node_id: row.get(5)?,
            last_seen_updated_at: row.get(6)?,
            is_read: row.get::<_, i64>(7)? != 0,
            triage_flag: row.get(8)?,
            updated_epoch_seconds: row.get(9)?,
        })
    };
    let rows = if let Some(p) = provider {
        stmt.query_map(rusqlite::params![project_id, p], map_row)
            .db_context("query forge_inbox_items with provider")?
            .collect::<Result<Vec<_>, _>>()
            .db_context("collect forge_inbox_items")?
    } else {
        stmt.query_map(rusqlite::params![project_id], map_row)
            .db_context("query forge_inbox_items")?
            .collect::<Result<Vec<_>, _>>()
            .db_context("collect forge_inbox_items")?
    };
    Ok(rows)
}

/// Upserts the read/unread state of a Forge inbox item.
pub fn db_upsert_forge_inbox_read(
    project_id: i64,
    provider: &str,
    repo: &str,
    item_kind: &str,
    item_number: i64,
    read: bool,
    last_seen_updated_at: Option<&str>,
    updated_epoch_seconds: i64,
) -> Result<(), DatabaseError> {
    let conn = get_connection()?;
    conn.execute(
        "INSERT INTO forge_inbox_items (
            project_id, provider, repo, item_kind, item_number,
            last_seen_updated_at, is_read, updated_epoch_seconds
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(project_id, provider, repo, item_kind, item_number) DO UPDATE SET
            is_read = excluded.is_read,
            last_seen_updated_at = COALESCE(excluded.last_seen_updated_at, forge_inbox_items.last_seen_updated_at),
            updated_epoch_seconds = excluded.updated_epoch_seconds",
        rusqlite::params![
            project_id,
            provider,
            repo,
            item_kind,
            item_number,
            last_seen_updated_at,
            if read { 1i64 } else { 0i64 },
            updated_epoch_seconds
        ],
    )
    .db_context("upsert forge_inbox_items read state")?;
    Ok(())
}

/// Upserts the triage flag (`flagged`, `muted`, or `None`) of a Forge inbox item.
pub fn db_upsert_forge_inbox_flag(
    project_id: i64,
    provider: &str,
    repo: &str,
    item_kind: &str,
    item_number: i64,
    triage_flag: Option<&str>,
    updated_epoch_seconds: i64,
) -> Result<(), DatabaseError> {
    let conn = get_connection()?;
    conn.execute(
        "INSERT INTO forge_inbox_items (
            project_id, provider, repo, item_kind, item_number,
            is_read, triage_flag, updated_epoch_seconds
         ) VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7)
         ON CONFLICT(project_id, provider, repo, item_kind, item_number) DO UPDATE SET
            triage_flag = excluded.triage_flag,
            updated_epoch_seconds = excluded.updated_epoch_seconds",
        rusqlite::params![
            project_id,
            provider,
            repo,
            item_kind,
            item_number,
            triage_flag,
            updated_epoch_seconds
        ],
    )
    .db_context("upsert forge_inbox_items triage flag")?;
    Ok(())
}

/// Reads the persisted Overview Dashboard widget layout JSON for `project_id`, if any.
pub fn db_get_project_overview_layout(project_id: i64) -> Result<Option<String>, DatabaseError> {
    let conn = get_connection()?;
    conn.query_row(
        "SELECT overview_layout_json FROM projects WHERE id = ?1",
        params![project_id],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .map(|opt| opt.flatten())
    .db_context("query projects.overview_layout_json")
}

/// Persists the Overview Dashboard widget layout JSON for `project_id`.
pub fn db_save_project_overview_layout(project_id: i64, layout_json: &str) -> Result<(), DatabaseError> {
    let conn = get_connection()?;
    conn.execute(
        "UPDATE projects SET overview_layout_json = ?1 WHERE id = ?2",
        params![layout_json, project_id],
    )
    .db_context("update projects.overview_layout_json")?;
    Ok(())
}

/// Atomically replaces all knowledge graph edges for `project_id` inside a single SQLite transaction (Spec 22 §5.7).
pub fn db_replace_project_knowledge_edges(
    project_id: i64,
    edges: &[crate::semantic::types::KnowledgeEdgeRecord],
) -> Result<(), DatabaseError> {
    let mut conn = get_connection()?;
    let tx = conn
        .transaction()
        .db_context("begin knowledge_edges replacement transaction")?;

    tx.execute(
        "DELETE FROM knowledge_edges WHERE project_id = ?1",
        params![project_id],
    )
    .db_context("delete existing knowledge_edges for project")?;

    if !edges.is_empty() {
        let now_seconds = chrono::Utc::now().timestamp();
        let mut stmt = tx
            .prepare(
                "INSERT INTO knowledge_edges (
                    project_id, source_id, source_entity_type, source_title, source_file_path, source_line_start,
                    target_id, target_entity_type, target_title, target_file_path, target_line_start,
                    edge_type, origin, weight, updated_at_epoch_seconds
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            )
            .db_context("prepare knowledge_edges batch insert")?;

        for edge in edges {
            stmt.execute(params![
                edge.project_id,
                edge.source_id,
                edge.source_entity_type,
                edge.source_title,
                edge.source_file_path,
                edge.source_line_start.map(i64::from),
                edge.target_id,
                edge.target_entity_type,
                edge.target_title,
                edge.target_file_path,
                edge.target_line_start.map(i64::from),
                edge.edge_type,
                edge.origin,
                edge.weight,
                now_seconds,
            ])
            .db_context("insert knowledge_edge row")?;
        }
    }

    tx.commit()
        .db_context("commit knowledge_edges replacement transaction")?;
    Ok(())
}

/// Queries 1-hop outgoing and incoming knowledge graph edges for a set of seed entity IDs (Spec 22 §5.7).
pub fn db_get_knowledge_relations_for_ids(
    project_id: Option<i64>,
    seed_ids: &[String],
    max_per_seed_count: usize,
    min_score_ratio: f64,
) -> Result<std::collections::HashMap<String, Vec<crate::semantic::types::KnowledgeRelation>>, DatabaseError> {
    let mut map: std::collections::HashMap<String, Vec<crate::semantic::types::KnowledgeRelation>> =
        std::collections::HashMap::new();
    if seed_ids.is_empty() || max_per_seed_count == 0 {
        return Ok(map);
    }

    let conn = get_connection()?;

    let mut out_stmt = conn
        .prepare(
            "SELECT edge_type, target_id, target_entity_type, target_title, target_file_path, target_line_start, weight
             FROM knowledge_edges
             WHERE source_id = ?1 AND (?2 IS NULL OR project_id = ?2) AND weight >= ?3
             ORDER BY weight DESC, id ASC
             LIMIT ?4",
        )
        .db_context("prepare outgoing knowledge_edges query")?;

    let mut in_stmt = conn
        .prepare(
            "SELECT edge_type, source_id, source_entity_type, source_title, source_file_path, source_line_start, weight
             FROM knowledge_edges
             WHERE target_id = ?1 AND (?2 IS NULL OR project_id = ?2) AND weight >= ?3
             ORDER BY weight DESC, id ASC
             LIMIT ?4",
        )
        .db_context("prepare incoming knowledge_edges query")?;

    let limit_i64 = max_per_seed_count as i64;

    for seed_id in seed_ids {
        let mut relations = Vec::new();
        let mut seen_peers = std::collections::HashSet::new();

        let out_rows = out_stmt
            .query_map(params![seed_id, project_id, min_score_ratio, limit_i64], |row| {
                let edge_type: String = row.get(0)?;
                let peer_id: String = row.get(1)?;
                let peer_entity_type: String = row.get(2)?;
                let peer_title: String = row.get(3)?;
                let peer_file_path: Option<String> = row.get(4)?;
                let peer_line_start: Option<i64> = row.get(5)?;
                let weight: f64 = row.get(6)?;
                Ok(crate::semantic::types::KnowledgeRelation {
                    direction: "outgoing".to_string(),
                    edge_type,
                    peer_id,
                    peer_entity_type,
                    peer_title,
                    peer_file_path,
                    peer_line_start: peer_line_start.and_then(|n| u32::try_from(n).ok()),
                    weight: weight as f32,
                })
            })
            .db_context("execute outgoing knowledge_edges query")?;

        for rel in out_rows.flatten() {
            let dedup_key = format!("out::{}::{}", rel.edge_type, rel.peer_id);
            if seen_peers.insert(dedup_key) {
                relations.push(rel);
            }
        }

        let in_rows = in_stmt
            .query_map(params![seed_id, project_id, min_score_ratio, limit_i64], |row| {
                let edge_type: String = row.get(0)?;
                let peer_id: String = row.get(1)?;
                let peer_entity_type: String = row.get(2)?;
                let peer_title: String = row.get(3)?;
                let peer_file_path: Option<String> = row.get(4)?;
                let peer_line_start: Option<i64> = row.get(5)?;
                let weight: f64 = row.get(6)?;
                Ok(crate::semantic::types::KnowledgeRelation {
                    direction: "incoming".to_string(),
                    edge_type,
                    peer_id,
                    peer_entity_type,
                    peer_title,
                    peer_file_path,
                    peer_line_start: peer_line_start.and_then(|n| u32::try_from(n).ok()),
                    weight: weight as f32,
                })
            })
            .db_context("execute incoming knowledge_edges query")?;

        for rel in in_rows.flatten() {
            let dedup_key = format!("in::{}::{}", rel.edge_type, rel.peer_id);
            if seen_peers.insert(dedup_key) {
                relations.push(rel);
            }
        }

        relations.sort_by(|a, b| b.weight.total_cmp(&a.weight));
        relations.truncate(max_per_seed_count);

        if !relations.is_empty() {
            map.insert(seed_id.clone(), relations);
        }
    }

    Ok(map)
}

/// Returns all stored knowledge graph edges for `project_id`, ordered by descending weight.
pub fn db_get_project_knowledge_edges(
    project_id: i64,
) -> Result<Vec<crate::semantic::types::KnowledgeGraphEdge>, DatabaseError> {
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(
            "SELECT source_id, source_entity_type, source_title, source_file_path, source_line_start,
                    target_id, target_entity_type, target_title, target_file_path, target_line_start,
                    edge_type, origin, weight
             FROM knowledge_edges
             WHERE project_id = ?1
             ORDER BY weight DESC, id ASC",
        )
        .db_context("prepare db_get_project_knowledge_edges query")?;

    let rows = stmt
        .query_map(params![project_id], |row| {
            let s_line: Option<i64> = row.get(4)?;
            let t_line: Option<i64> = row.get(9)?;
            let w: f64 = row.get(12)?;
            Ok(crate::semantic::types::KnowledgeGraphEdge {
                source_id: row.get(0)?,
                source_entity_type: row.get(1)?,
                source_title: row.get(2)?,
                source_file_path: row.get(3)?,
                source_line_start: s_line.and_then(|n| u32::try_from(n).ok()),
                target_id: row.get(5)?,
                target_entity_type: row.get(6)?,
                target_title: row.get(7)?,
                target_file_path: row.get(8)?,
                target_line_start: t_line.and_then(|n| u32::try_from(n).ok()),
                edge_type: row.get(10)?,
                origin: row.get(11)?,
                weight: w as f32,
            })
        })
        .db_context("execute db_get_project_knowledge_edges query")?;

    Ok(rows.flatten().collect())
}

/// Persisted line-level hunk attributing a session journal minute to a workspace file.
#[derive(Debug, Clone)]
pub struct JournalFileHunkRecord {
    pub project_id: i64,
    pub journal_id: i64,
    pub file_path: String,
    pub line_start: u32,
    pub line_end: u32,
    pub change_kind: String,
    pub source_kind: String,
}

/// Lightweight deterministic Tree-sitter AST symbol entry persisted in SQLite (`project_code_symbols`).
#[derive(Debug, Clone)]
pub struct CodeSymbolRecord {
    pub project_id: i64,
    pub file_path: String,
    pub file_sha256: String,
    pub symbol_name: String,
    pub short_name: String,
    pub title: String,
    pub line_start: u32,
    pub line_end: u32,
}

/// Inserts file/line hunks for `journal_id` inside a single SQLite transaction.
pub fn db_insert_journal_file_hunks(
    project_id: i64,
    journal_id: i64,
    hunks: &[crate::kopeng::git::DiffFileHunk],
    source_kind: &str,
) -> Result<(), DatabaseError> {
    if hunks.is_empty() {
        return Ok(());
    }
    let mut conn = get_connection()?;
    let tx = conn.transaction().db_context("begin journal_file_hunks insert")?;
    let now = now_epoch_secs();
    {
        let mut stmt = tx
            .prepare(
                "INSERT INTO journal_file_hunks (
                    project_id, journal_id, file_path, line_start, line_end, change_kind, source_kind, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .db_context("prepare journal_file_hunks insert")?;
        for h in hunks {
            let norm = h.file_path.trim().trim_start_matches("./");
            if norm.is_empty() {
                continue;
            }
            stmt.execute(params![
                project_id,
                journal_id,
                norm,
                i64::from(h.line_start),
                i64::from(h.line_end),
                &h.change_kind,
                source_kind,
                now,
            ])
            .db_context("insert journal_file_hunks row")?;
        }
    }
    tx.commit().db_context("commit journal_file_hunks insert")?;
    Ok(())
}

/// Returns the set of `journal_id` values in `project_id` that already have entries in `journal_file_hunks`.
pub fn db_get_journal_ids_with_hunks(project_id: i64) -> Result<std::collections::HashSet<i64>, DatabaseError> {
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare("SELECT DISTINCT journal_id FROM journal_file_hunks WHERE project_id = ?1")
        .db_context("prepare db_get_journal_ids_with_hunks")?;
    let rows = stmt
        .query_map(params![project_id], |row| row.get::<_, i64>(0))
        .db_context("query db_get_journal_ids_with_hunks")?;
    let mut set = std::collections::HashSet::new();
    for r in rows {
        set.insert(r.db_context("decode journal_id from hunks")?);
    }
    Ok(set)
}

/// Returns all causal file/line hunks recorded for `project_id`.
pub fn db_get_journal_hunks_for_project(project_id: i64) -> Result<Vec<JournalFileHunkRecord>, DatabaseError> {
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(
            "SELECT project_id, journal_id, file_path, line_start, line_end, change_kind, source_kind
             FROM journal_file_hunks
             WHERE project_id = ?1
             ORDER BY journal_id DESC, id ASC",
        )
        .db_context("prepare db_get_journal_hunks_for_project")?;
    let rows = stmt
        .query_map(params![project_id], |row| {
            let ls: i64 = row.get(3)?;
            let le: i64 = row.get(4)?;
            Ok(JournalFileHunkRecord {
                project_id: row.get(0)?,
                journal_id: row.get(1)?,
                file_path: row.get(2)?,
                line_start: u32::try_from(ls).unwrap_or(0),
                line_end: u32::try_from(le).unwrap_or(0),
                change_kind: row.get(5)?,
                source_kind: row.get(6)?,
            })
        })
        .db_context("query db_get_journal_hunks_for_project")?;
    collect_rows(rows, "decode journal_file_hunk row")
}

/// Returns causal session minutes touching `rel_path`, prioritizing hunks that overlap `target_line`.
pub fn db_get_causal_minutes_for_file_line(
    project_id: i64,
    rel_path: &str,
    target_line: u32,
    max_results_count: usize,
) -> Result<Vec<crate::semantic::types::FileCausalMinute>, DatabaseError> {
    if max_results_count == 0 {
        return Ok(Vec::new());
    }
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(
            "SELECT j.id, j.agent_id, j.title, j.summary, j.decisions, j.commit_hash, j.snapshot_ref,
                    h.line_start, h.line_end, h.change_kind, h.source_kind, j.created_at,
                    CASE
                        WHEN h.line_start > 0 AND ?3 >= h.line_start AND ?3 <= h.line_end THEN 1
                        ELSE 0
                    END AS exact_match
             FROM journal_file_hunks h
             INNER JOIN agent_session_journal j ON j.id = h.journal_id
             WHERE h.project_id = ?1 AND h.file_path = ?2
             ORDER BY exact_match DESC, j.created_at DESC, h.id ASC
             LIMIT ?4",
        )
        .db_context("prepare db_get_causal_minutes_for_file_line")?;

    let rows = stmt
        .query_map(
            params![project_id, rel_path, i64::from(target_line), (max_results_count * 3) as i64],
            |row| {
                let ls: i64 = row.get(7)?;
                let le: i64 = row.get(8)?;
                let exact_int: i64 = row.get(12)?;
                Ok(crate::semantic::types::FileCausalMinute {
                    journal_id: row.get(0)?,
                    agent_id: row.get(1)?,
                    title: row.get(2)?,
                    summary: row.get(3)?,
                    decisions: row.get(4)?,
                    commit_hash: row.get(5)?,
                    snapshot_ref: row.get(6)?,
                    line_start: u32::try_from(ls).unwrap_or(0),
                    line_end: u32::try_from(le).unwrap_or(0),
                    change_kind: row.get(9)?,
                    source_kind: row.get(10)?,
                    exact_line_match: exact_int == 1,
                    created_at: row.get(11)?,
                })
            },
        )
        .db_context("execute db_get_causal_minutes_for_file_line")?;

    let mut deduped = Vec::new();
    let mut seen_journals = std::collections::HashSet::new();
    for r in rows {
        let item = r.db_context("decode causal minute row")?;
        if seen_journals.insert(item.journal_id) {
            deduped.push(item);
            if deduped.len() >= max_results_count {
                break;
            }
        }
    }
    Ok(deduped)
}

/// Returns technical debt and TODOs anchored directly to `rel_path` or referencing its `file::<pid>::<rel_path>` node.
pub fn db_get_anchored_items_for_file(
    project_id: i64,
    rel_path: &str,
    target_line: u32,
) -> Result<Vec<crate::semantic::types::FileAnchoredItem>, DatabaseError> {
    let conn = get_connection()?;
    let mut items = Vec::new();
    let mut seen_keys = std::collections::HashSet::new();
    let file_node_id = format!("file::{}::{}", project_id, rel_path);

    let mut debt_stmt = conn
        .prepare(
            "SELECT id, title, impact_summary, remediation_plan, status, severity, file_path, line_start, line_end
             FROM project_technical_debt
             WHERE project_id = ?1 AND file_path = ?2
             ORDER BY CASE WHEN status = 'open' THEN 0 ELSE 1 END, id DESC",
        )
        .db_context("prepare anchored tech debt query")?;
    for row_res in debt_stmt
        .query_map(params![project_id, rel_path], |row| {
            let id: i64 = row.get(0)?;
            let impact: String = row.get(2)?;
            let rem: Option<String> = row.get(3)?;
            let details = match rem {
                Some(r) if !r.trim().is_empty() => format!("{}\nRemediation: {}", impact, r),
                _ => impact,
            };
            let ls_opt: Option<i64> = row.get(7)?;
            let le_opt: Option<i64> = row.get(8)?;
            let line_start = ls_opt.and_then(|n| u32::try_from(n).ok());
            let line_end = le_opt.and_then(|n| u32::try_from(n).ok());
            let exact = match (line_start, line_end) {
                (Some(s), Some(e)) if s > 0 => target_line >= s && target_line <= e.max(s),
                (Some(s), None) if s > 0 => target_line == s,
                _ => false,
            };
            Ok(crate::semantic::types::FileAnchoredItem {
                entity_type: "tech_debt".to_string(),
                entity_id: id,
                title: row.get(1)?,
                details,
                status: row.get(4)?,
                severity_or_priority: row.get(5)?,
                file_path: row.get(6)?,
                line_start,
                line_end,
                exact_line_match: exact,
                link_reason: if line_start.is_some_and(|n| n > 0) {
                    "line_anchor".to_string()
                } else {
                    "file_anchor".to_string()
                },
            })
        })
        .db_context("query anchored tech debt")?
    {
        let item = row_res.db_context("decode anchored tech debt")?;
        if seen_keys.insert(format!("tech_debt::{}", item.entity_id)) {
            items.push(item);
        }
    }

    let mut todo_stmt = conn
        .prepare(
            "SELECT id, title, description, status, priority, file_path, line_start, line_end
             FROM project_todos
             WHERE project_id = ?1 AND file_path = ?2
             ORDER BY CASE WHEN status IN ('pending', 'in_progress') THEN 0 ELSE 1 END, priority ASC, id DESC",
        )
        .db_context("prepare anchored todos query")?;
    for row_res in todo_stmt
        .query_map(params![project_id, rel_path], |row| {
            let id: i64 = row.get(0)?;
            let desc: Option<String> = row.get(2)?;
            let prio: i32 = row.get(4)?;
            let ls_opt: Option<i64> = row.get(6)?;
            let le_opt: Option<i64> = row.get(7)?;
            let line_start = ls_opt.and_then(|n| u32::try_from(n).ok());
            let line_end = le_opt.and_then(|n| u32::try_from(n).ok());
            let exact = match (line_start, line_end) {
                (Some(s), Some(e)) if s > 0 => target_line >= s && target_line <= e.max(s),
                (Some(s), None) if s > 0 => target_line == s,
                _ => false,
            };
            Ok(crate::semantic::types::FileAnchoredItem {
                entity_type: "tasks".to_string(),
                entity_id: id,
                title: row.get(1)?,
                details: desc.unwrap_or_default(),
                status: row.get(3)?,
                severity_or_priority: format!("P{}", prio),
                file_path: row.get(5)?,
                line_start,
                line_end,
                exact_line_match: exact,
                link_reason: if line_start.is_some_and(|n| n > 0) {
                    "line_anchor".to_string()
                } else {
                    "file_anchor".to_string()
                },
            })
        })
        .db_context("query anchored todos")?
    {
        let item = row_res.db_context("decode anchored todo")?;
        if seen_keys.insert(format!("tasks::{}", item.entity_id)) {
            items.push(item);
        }
    }

    // Also include explicit_ref edges targeting `file::<pid>::<rel_path>` from tech_debt / tasks
    let mut edge_stmt = conn
        .prepare(
            "SELECT source_id, source_entity_type, source_title
             FROM knowledge_edges
             WHERE project_id = ?1 AND target_id = ?2 AND source_entity_type IN ('tech_debt', 'tasks')
             ORDER BY weight DESC, id ASC",
        )
        .db_context("prepare explicit file edge debt/todo query")?;
    for row_res in edge_stmt
        .query_map(params![project_id, file_node_id], |row| {
            let sid: String = row.get(0)?;
            let etype: String = row.get(1)?;
            let title: String = row.get(2)?;
            let eid = sid.rsplit("::").next().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
            Ok((sid, etype, eid, title))
        })
        .db_context("query explicit file edge debt/todo")?
    {
        let (_sid, etype, eid, title) = row_res.db_context("decode explicit file edge debt/todo")?;
        if eid <= 0 || !seen_keys.insert(format!("{}::{}", etype, eid)) {
            continue;
        }
        if etype == "tech_debt" {
            if let Ok(Some(d)) = conn
                .query_row(
                    "SELECT impact_summary, status, severity FROM project_technical_debt WHERE id = ?1 AND project_id = ?2",
                    params![eid, project_id],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
                )
                .optional()
            {
                items.push(crate::semantic::types::FileAnchoredItem {
                    entity_type: etype,
                    entity_id: eid,
                    title,
                    details: d.0,
                    status: d.1,
                    severity_or_priority: d.2,
                    file_path: Some(rel_path.to_string()),
                    line_start: None,
                    line_end: None,
                    exact_line_match: false,
                    link_reason: "explicit_ref".to_string(),
                });
            }
        } else if etype == "tasks" {
            if let Ok(Some(t)) = conn
                .query_row(
                    "SELECT description, status, priority FROM project_todos WHERE id = ?1 AND project_id = ?2",
                    params![eid, project_id],
                    |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?, r.get::<_, i32>(2)?)),
                )
                .optional()
            {
                items.push(crate::semantic::types::FileAnchoredItem {
                    entity_type: etype,
                    entity_id: eid,
                    title,
                    details: t.0.unwrap_or_default(),
                    status: t.1,
                    severity_or_priority: format!("P{}", t.2),
                    file_path: Some(rel_path.to_string()),
                    line_start: None,
                    line_end: None,
                    exact_line_match: false,
                    link_reason: "explicit_ref".to_string(),
                });
            }
        }
    }

    items.sort_by(|a, b| b.exact_line_match.cmp(&a.exact_line_match));
    Ok(items)
}

/// Returns related `specs` linked directly to `file::<pid>::<rel_path>` or linked via causal `minute_journal_ids`.
pub fn db_get_related_docs_for_file(
    project_id: i64,
    rel_path: &str,
    minute_entries: &[(i64, String)],
    max_results_count: usize,
) -> Result<Vec<crate::semantic::types::FileRelatedDoc>, DatabaseError> {
    if max_results_count == 0 {
        return Ok(Vec::new());
    }
    let conn = get_connection()?;
    let mut docs = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();
    let file_node_id = format!("file::{}::{}", project_id, rel_path);

    // 1. Specs that explicitly reference this file (`specs --(governs/explicit_ref)--> file`)
    let mut file_stmt = conn
        .prepare(
            "SELECT source_id, source_title, source_file_path, source_line_start, weight
             FROM knowledge_edges
             WHERE project_id = ?1 AND target_id = ?2 AND source_entity_type = 'specs'
             ORDER BY weight DESC, id ASC
             LIMIT ?3",
        )
        .db_context("prepare explicit specs->file query")?;
    for row_res in file_stmt
        .query_map(params![project_id, file_node_id, max_results_count as i64], |row| {
            let line_opt: Option<i64> = row.get(3)?;
            let w: f64 = row.get(4)?;
            let fp_opt: Option<String> = row.get(2)?;
            Ok(crate::semantic::types::FileRelatedDoc {
                id: row.get(0)?,
                title: row.get(1)?,
                content_preview: String::new(),
                file_path: fp_opt.unwrap_or_default(),
                line_start: line_opt.and_then(|n| u32::try_from(n).ok()),
                line_end: None,
                weight: w as f32,
                link_reason: "explicit_ref".to_string(),
                via_minute_title: None,
            })
        })
        .db_context("query explicit specs->file")?
    {
        let d = row_res.db_context("decode explicit specs->file row")?;
        if !d.file_path.is_empty() && seen_ids.insert(d.id.clone()) {
            docs.push(d);
        }
    }

    // 2. Specs linked to the causal minutes that touched this file/line
    let mut min_out_stmt = conn
        .prepare(
            "SELECT target_id, target_title, target_file_path, target_line_start, weight
             FROM knowledge_edges
             WHERE project_id = ?1 AND source_id = ?2 AND target_entity_type = 'specs'
             ORDER BY weight DESC, id ASC
             LIMIT 6",
        )
        .db_context("prepare minute->specs query")?;

    for (jid, jtitle) in minute_entries {
        let j_node_id = format!("journal::{}::{}", project_id, jid);
        for row_res in min_out_stmt
            .query_map(params![project_id, j_node_id], |row| {
                let line_opt: Option<i64> = row.get(3)?;
                let w: f64 = row.get(4)?;
                let fp_opt: Option<String> = row.get(2)?;
                Ok(crate::semantic::types::FileRelatedDoc {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    content_preview: String::new(),
                    file_path: fp_opt.unwrap_or_default(),
                    line_start: line_opt.and_then(|n| u32::try_from(n).ok()),
                    line_end: None,
                    weight: w as f32,
                    link_reason: "via_minute".to_string(),
                    via_minute_title: Some(jtitle.clone()),
                })
            })
            .db_context("query minute->specs")?
        {
            let d = row_res.db_context("decode minute->specs row")?;
            if !d.file_path.is_empty() && seen_ids.insert(d.id.clone()) {
                docs.push(d);
            }
        }
    }

    docs.sort_by(|a, b| b.weight.total_cmp(&a.weight));
    docs.truncate(max_results_count);
    Ok(docs)
}

/// Returns `file_path -> file_sha256` for deterministic code symbols stored in SQLite.
pub fn db_get_project_code_symbol_hashes(
    project_id: i64,
) -> Result<std::collections::HashMap<String, String>, DatabaseError> {
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare("SELECT DISTINCT file_path, file_sha256 FROM project_code_symbols WHERE project_id = ?1")
        .db_context("prepare db_get_project_code_symbol_hashes")?;
    let rows = stmt
        .query_map(params![project_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .db_context("query db_get_project_code_symbol_hashes")?;
    let mut map = std::collections::HashMap::new();
    for r in rows {
        let (fp, hash) = r.db_context("decode symbol file hash")?;
        map.insert(fp, hash);
    }
    Ok(map)
}

/// Transactionally updates AST symbols for changed files and removes deleted files from `project_code_symbols`.
pub fn db_sync_project_code_symbols(
    project_id: i64,
    changed_files: &[String],
    symbols: &[CodeSymbolRecord],
    deleted_files: &[String],
) -> Result<(), DatabaseError> {
    if changed_files.is_empty() && deleted_files.is_empty() {
        return Ok(());
    }
    let mut conn = get_connection()?;
    let tx = conn.transaction().db_context("begin project_code_symbols sync")?;
    {
        let mut del_stmt = tx
            .prepare("DELETE FROM project_code_symbols WHERE project_id = ?1 AND file_path = ?2")
            .db_context("prepare delete file code symbols")?;
        for fp in changed_files.iter().chain(deleted_files.iter()) {
            del_stmt
                .execute(params![project_id, fp])
                .db_context("delete file code symbols")?;
        }
    }
    if !symbols.is_empty() {
        let mut ins_stmt = tx
            .prepare(
                "INSERT INTO project_code_symbols (
                    project_id, file_path, file_sha256, symbol_name, short_name, title, line_start, line_end
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .db_context("prepare insert project_code_symbols")?;
        for s in symbols {
            ins_stmt
                .execute(params![
                    s.project_id,
                    &s.file_path,
                    &s.file_sha256,
                    &s.symbol_name,
                    &s.short_name,
                    &s.title,
                    i64::from(s.line_start),
                    i64::from(s.line_end),
                ])
                .db_context("insert project_code_symbol")?;
        }
    }
    tx.commit().db_context("commit project_code_symbols sync")?;
    Ok(())
}

/// Counts deterministic AST symbols indexed for `project_id`.
pub fn db_count_project_code_symbols(project_id: i64) -> Result<usize, DatabaseError> {
    let conn = get_connection()?;
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM project_code_symbols WHERE project_id = ?1",
            params![project_id],
            |row| row.get(0),
        )
        .db_context("count project_code_symbols")?;
    Ok(usize::try_from(count).unwrap_or(0))
}

/// Lists deterministic AST symbols for a project (optionally filtered to a single project or all projects).
pub fn db_list_project_code_symbols(
    project_id: Option<i64>,
) -> Result<Vec<CodeSymbolRecord>, DatabaseError> {
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(
            "SELECT project_id, file_path, file_sha256, symbol_name, short_name, title, line_start, line_end
             FROM project_code_symbols
             WHERE (?1 IS NULL OR project_id = ?1)
             ORDER BY file_path ASC, line_start ASC",
        )
        .db_context("prepare db_list_project_code_symbols")?;
    let rows = stmt
        .query_map(params![project_id], |row| {
            let ls: i64 = row.get(6)?;
            let le: i64 = row.get(7)?;
            Ok(CodeSymbolRecord {
                project_id: row.get(0)?,
                file_path: row.get(1)?,
                file_sha256: row.get(2)?,
                symbol_name: row.get(3)?,
                short_name: row.get(4)?,
                title: row.get(5)?,
                line_start: u32::try_from(ls).unwrap_or(1),
                line_end: u32::try_from(le).unwrap_or(1),
            })
        })
        .db_context("query db_list_project_code_symbols")?;
    collect_rows(rows, "decode CodeSymbolRecord")
}

/// Resolves the narrowest enclosing AST symbol covering `target_line` in `(project_id, rel_path)`.
pub fn db_get_file_enclosing_symbol(
    project_id: i64,
    rel_path: &str,
    target_line: u32,
) -> Result<Option<CodeSymbolRecord>, DatabaseError> {
    let conn = get_connection()?;
    conn.query_row(
        "SELECT project_id, file_path, file_sha256, symbol_name, short_name, title, line_start, line_end
         FROM project_code_symbols
         WHERE project_id = ?1 AND file_path = ?2 AND line_start <= ?3 AND line_end >= ?3
         ORDER BY (line_end - line_start) ASC, line_start ASC
         LIMIT 1",
        params![project_id, rel_path, i64::from(target_line)],
        |row| {
            let ls: i64 = row.get(6)?;
            let le: i64 = row.get(7)?;
            Ok(CodeSymbolRecord {
                project_id: row.get(0)?,
                file_path: row.get(1)?,
                file_sha256: row.get(2)?,
                symbol_name: row.get(3)?,
                short_name: row.get(4)?,
                title: row.get(5)?,
                line_start: u32::try_from(ls).unwrap_or(1),
                line_end: u32::try_from(le).unwrap_or(1),
            })
        },
    )
    .optional()
    .db_context("query enclosing file code symbol")
}

/// Returns the total number of knowledge graph edges stored for `project_id`.
pub fn db_count_project_knowledge_edges(project_id: i64) -> Result<usize, DatabaseError> {
    let conn = get_connection()?;
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM knowledge_edges WHERE project_id = ?1",
            params![project_id],
            |row| row.get(0),
        )
        .db_context("count project knowledge_edges")?;
    Ok(usize::try_from(count).unwrap_or(0))
}

/// Returns aggregated edge statistics and sample edges from `knowledge_edges` for semantic auditing.
pub fn db_audit_project_knowledge_edges(
    project_id: i64,
    sample_limit_count: usize,
) -> Result<serde_json::Value, DatabaseError> {
    let conn = get_connection()?;

    let mut group_stmt = conn
        .prepare(
            "SELECT edge_type, origin, COUNT(*), MIN(weight), MAX(weight), AVG(weight)
             FROM knowledge_edges
             WHERE project_id = ?1
             GROUP BY edge_type, origin
             ORDER BY COUNT(*) DESC",
        )
        .db_context("prepare knowledge_edges audit summary query")?;

    let groups: Vec<serde_json::Value> = group_stmt
        .query_map(params![project_id], |row| {
            let edge_type: String = row.get(0)?;
            let origin: String = row.get(1)?;
            let count: i64 = row.get(2)?;
            let min_w: f64 = row.get(3)?;
            let max_w: f64 = row.get(4)?;
            let avg_w: f64 = row.get(5)?;
            Ok(serde_json::json!({
                "edgeType": edge_type,
                "origin": origin,
                "count": count,
                "minWeight": (min_w * 1000.0).round() / 1000.0,
                "maxWeight": (max_w * 1000.0).round() / 1000.0,
                "avgWeight": (avg_w * 1000.0).round() / 1000.0,
            }))
        })
        .db_context("execute knowledge_edges audit summary query")?
        .flatten()
        .collect();

    let mut sample_stmt = conn
        .prepare(
            "SELECT edge_type, origin, weight, source_entity_type, source_title, source_file_path,
                    target_entity_type, target_title, target_file_path, target_line_start
             FROM knowledge_edges
             WHERE project_id = ?1
             ORDER BY CASE WHEN origin = 'semantic' THEN 0 ELSE 1 END, weight DESC, id ASC
             LIMIT ?2",
        )
        .db_context("prepare knowledge_edges sample query")?;

    let samples: Vec<serde_json::Value> = sample_stmt
        .query_map(params![project_id, sample_limit_count as i64], |row| {
            let edge_type: String = row.get(0)?;
            let origin: String = row.get(1)?;
            let weight: f64 = row.get(2)?;
            let source_type: String = row.get(3)?;
            let source_title: String = row.get(4)?;
            let source_file: Option<String> = row.get(5)?;
            let target_type: String = row.get(6)?;
            let target_title: String = row.get(7)?;
            let target_file: Option<String> = row.get(8)?;
            let target_line: Option<i64> = row.get(9)?;
            Ok(serde_json::json!({
                "edgeType": edge_type,
                "origin": origin,
                "scorePercent": (weight * 1000.0).round() / 10.0,
                "source": {
                    "type": source_type,
                    "title": source_title,
                    "file": source_file,
                },
                "target": {
                    "type": target_type,
                    "title": target_title,
                    "file": target_file,
                    "lineStart": target_line,
                }
            }))
        })
        .db_context("execute knowledge_edges sample query")?
        .flatten()
        .collect();

    Ok(serde_json::json!({
        "byTypeAndOrigin": groups,
        "sampleEdges": samples,
    }))
}

// ============================================================================
// Voice Orchestrator Harness Persistence (Spec 40)
// ============================================================================

/// Persisted global working session row (`voice_sessions`, Spec 40).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbVoiceSession {
    pub id: String,
    pub status: String,
    pub active_project_id: Option<i64>,
    pub current_objective: Option<String>,
    pub last_delegated_project_id: Option<i64>,
    pub last_delegated_agent_id: Option<String>,
    pub resumption_handle: Option<String>,
    pub resumption_valid_until_epoch_seconds: Option<i64>,
    pub rolling_summary: String,
    pub created_epoch_seconds: i64,
    pub updated_epoch_seconds: i64,
}

/// Persisted working scratchpad item (`voice_scratchpad_items`, Spec 40).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbVoiceScratchpadItem {
    pub id: i64,
    pub session_id: String,
    pub project_id: Option<i64>,
    pub title: String,
    pub status: String,
    pub note: Option<String>,
    pub created_epoch_seconds: i64,
    pub updated_epoch_seconds: i64,
}

/// Persisted recent turn ledger entry (`voice_turn_ledger`, Spec 40).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbVoiceTurnLedgerEntry {
    pub id: i64,
    pub session_id: String,
    pub project_id: Option<i64>,
    pub user_transcript: Option<String>,
    pub actions_summary: Option<String>,
    pub assistant_transcript: Option<String>,
    pub created_epoch_seconds: i64,
}

fn voice_session_from_row(row: &rusqlite::Row) -> rusqlite::Result<DbVoiceSession> {
    Ok(DbVoiceSession {
        id: row.get(0)?,
        status: row.get(1)?,
        active_project_id: row.get(2)?,
        current_objective: row.get(3)?,
        last_delegated_project_id: row.get(4)?,
        last_delegated_agent_id: row.get(5)?,
        resumption_handle: row.get(6)?,
        resumption_valid_until_epoch_seconds: row.get(7)?,
        rolling_summary: row.get(8)?,
        created_epoch_seconds: row.get(9)?,
        updated_epoch_seconds: row.get(10)?,
    })
}

fn voice_scratchpad_from_row(row: &rusqlite::Row) -> rusqlite::Result<DbVoiceScratchpadItem> {
    Ok(DbVoiceScratchpadItem {
        id: row.get(0)?,
        session_id: row.get(1)?,
        project_id: row.get(2)?,
        title: row.get(3)?,
        status: row.get(4)?,
        note: row.get(5)?,
        created_epoch_seconds: row.get(6)?,
        updated_epoch_seconds: row.get(7)?,
    })
}

fn voice_turn_from_row(row: &rusqlite::Row) -> rusqlite::Result<DbVoiceTurnLedgerEntry> {
    Ok(DbVoiceTurnLedgerEntry {
        id: row.get(0)?,
        session_id: row.get(1)?,
        project_id: row.get(2)?,
        user_transcript: row.get(3)?,
        actions_summary: row.get(4)?,
        assistant_transcript: row.get(5)?,
        created_epoch_seconds: row.get(6)?,
    })
}

fn truncate_str_chars(s: &str, max_chars: usize) -> String {
    let trimmed = s.trim();
    let count = trimmed.chars().count();
    if count <= max_chars {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Folds pruned turn ledger entries into a bounded deterministic rolling summary.
pub(crate) fn fold_voice_turns_into_summary(
    existing_summary: &str,
    folded_entries: &[DbVoiceTurnLedgerEntry],
    max_summary_chars: usize,
) -> String {
    let mut lines: Vec<String> = existing_summary
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();

    for entry in folded_entries {
        let mut parts = Vec::with_capacity(3);
        if let Some(pid) = entry.project_id {
            parts.push(format!("p{pid}"));
        }
        if let Some(ref u) = entry.user_transcript {
            let u = u.trim();
            if !u.is_empty() {
                parts.push(format!("u:{}", truncate_str_chars(u, 90)));
            }
        }
        if let Some(ref act) = entry.actions_summary {
            let act = act.trim();
            if !act.is_empty() {
                parts.push(format!("act:{}", truncate_str_chars(act, 90)));
            }
        }
        if let Some(ref a) = entry.assistant_transcript {
            let a = a.trim();
            if !a.is_empty() {
                parts.push(format!("ai:{}", truncate_str_chars(a, 70)));
            }
        }
        if !parts.is_empty() {
            lines.push(format!("- {}", parts.join(" | ")));
        }
    }

    let mut joined = lines.join("\n");
    while joined.chars().count() > max_summary_chars && lines.len() > 1 {
        lines.remove(0);
        joined = lines.join("\n");
    }
    if joined.chars().count() > max_summary_chars {
        joined = truncate_str_chars(&joined, max_summary_chars);
    }
    joined
}

/// Loads the active global voice session, creating one transactionally if none exists.
pub fn db_get_or_create_active_voice_session(now_epoch: i64) -> Result<DbVoiceSession, DatabaseError> {
    let mut conn = get_connection()?;
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .db_context("begin voice session init tx")?;
    let existing = tx
        .query_row(
            "SELECT id, status, active_project_id, current_objective,
                    last_delegated_project_id, last_delegated_agent_id,
                    resumption_handle, resumption_valid_until_epoch_seconds,
                    rolling_summary, created_epoch_seconds, updated_epoch_seconds
             FROM voice_sessions
             WHERE status = 'active'
             ORDER BY updated_epoch_seconds DESC
             LIMIT 1",
            [],
            voice_session_from_row,
        )
        .optional()
        .db_context("query active voice session")?;

    if let Some(sess) = existing {
        tx.commit().db_context("commit voice session read tx")?;
        return Ok(sess);
    }

    let id = uuid::Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO voice_sessions (
            id, status, active_project_id, current_objective,
            last_delegated_project_id, last_delegated_agent_id,
            resumption_handle, resumption_valid_until_epoch_seconds,
            rolling_summary, created_epoch_seconds, updated_epoch_seconds
         ) VALUES (?1, 'active', NULL, NULL, NULL, NULL, NULL, NULL, '', ?2, ?2)",
        params![id, now_epoch],
    )
    .db_context("insert initial voice session")?;
    tx.commit().db_context("commit voice session create tx")?;

    Ok(DbVoiceSession {
        id,
        status: "active".to_string(),
        active_project_id: None,
        current_objective: None,
        last_delegated_project_id: None,
        last_delegated_agent_id: None,
        resumption_handle: None,
        resumption_valid_until_epoch_seconds: None,
        rolling_summary: String::new(),
        created_epoch_seconds: now_epoch,
        updated_epoch_seconds: now_epoch,
    })
}

/// Archives any active voice sessions and creates a fresh working session atomically.
pub fn db_archive_and_create_voice_session(now_epoch: i64) -> Result<DbVoiceSession, DatabaseError> {
    let mut conn = get_connection()?;
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .db_context("begin reset voice session tx")?;
    tx.execute(
        "UPDATE voice_sessions SET status = 'archived', updated_epoch_seconds = ?1 WHERE status = 'active'",
        params![now_epoch],
    )
    .db_context("archive active voice sessions")?;
    let id = uuid::Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO voice_sessions (
            id, status, active_project_id, current_objective,
            last_delegated_project_id, last_delegated_agent_id,
            resumption_handle, resumption_valid_until_epoch_seconds,
            rolling_summary, created_epoch_seconds, updated_epoch_seconds
         ) VALUES (?1, 'active', NULL, NULL, NULL, NULL, NULL, NULL, '', ?2, ?2)",
        params![id, now_epoch],
    )
    .db_context("insert reset voice session")?;
    tx.commit().db_context("commit reset voice session tx")?;

    Ok(DbVoiceSession {
        id,
        status: "active".to_string(),
        active_project_id: None,
        current_objective: None,
        last_delegated_project_id: None,
        last_delegated_agent_id: None,
        resumption_handle: None,
        resumption_valid_until_epoch_seconds: None,
        rolling_summary: String::new(),
        created_epoch_seconds: now_epoch,
        updated_epoch_seconds: now_epoch,
    })
}

/// Persists a Gemini Live `sessionResumptionUpdate` handle (or clears it when `handle` is `None`).
pub fn db_update_voice_session_resumption(
    session_id: &str,
    handle: Option<&str>,
    valid_until_epoch: Option<i64>,
    now_epoch: i64,
) -> Result<(), DatabaseError> {
    let conn = get_connection()?;
    conn.execute(
        "UPDATE voice_sessions
         SET resumption_handle = ?1,
             resumption_valid_until_epoch_seconds = ?2,
             updated_epoch_seconds = ?3
         WHERE id = ?4",
        params![handle, valid_until_epoch, now_epoch, session_id],
    )
    .db_context("update voice session resumption handle")?;
    Ok(())
}

/// Updates working session focus fields (pass `Some(...)` to overwrite or `None` to keep current).
pub fn db_update_voice_session_focus(
    session_id: &str,
    active_project_id: Option<i64>,
    current_objective: Option<Option<&str>>,
    last_delegated: Option<(i64, &str)>,
    now_epoch: i64,
) -> Result<(), DatabaseError> {
    let mut conn = get_connection()?;
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .db_context("begin voice focus update tx")?;
    if let Some(pid) = active_project_id {
        tx.execute(
            "UPDATE voice_sessions SET active_project_id = ?1, updated_epoch_seconds = ?2 WHERE id = ?3",
            params![pid, now_epoch, session_id],
        )
        .db_context("update voice active_project_id")?;
    }
    if let Some(obj_opt) = current_objective {
        tx.execute(
            "UPDATE voice_sessions SET current_objective = ?1, updated_epoch_seconds = ?2 WHERE id = ?3",
            params![obj_opt, now_epoch, session_id],
        )
        .db_context("update voice current_objective")?;
    }
    if let Some((del_pid, del_agent)) = last_delegated {
        tx.execute(
            "UPDATE voice_sessions
             SET last_delegated_project_id = ?1,
                 last_delegated_agent_id = ?2,
                 updated_epoch_seconds = ?3
             WHERE id = ?4",
            params![del_pid, del_agent, now_epoch, session_id],
        )
        .db_context("update voice last delegation")?;
    }
    tx.commit().db_context("commit voice focus update tx")?;
    Ok(())
}

/// Lists all scratchpad items for `session_id` ordered by status priority and insertion order.
pub fn db_list_voice_scratchpad(session_id: &str) -> Result<Vec<DbVoiceScratchpadItem>, DatabaseError> {
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, project_id, title, status, note, created_epoch_seconds, updated_epoch_seconds
             FROM voice_scratchpad_items
             WHERE session_id = ?1
             ORDER BY CASE status WHEN 'in_progress' THEN 0 WHEN 'pending' THEN 1 ELSE 2 END, id ASC",
        )
        .db_context("prepare voice scratchpad list")?;
    let rows = stmt
        .query_map(params![session_id], voice_scratchpad_from_row)
        .db_context("query voice scratchpad")?;
    collect_rows(rows, "read voice scratchpad row")
}

/// Inserts a scratchpad item inside a transaction, enforcing `max_items`.
pub fn db_insert_voice_scratchpad_item(
    session_id: &str,
    project_id: Option<i64>,
    title: &str,
    note: Option<&str>,
    max_items: usize,
    now_epoch: i64,
) -> Result<DbVoiceScratchpadItem, DatabaseError> {
    let clean_title = title.trim();
    if clean_title.is_empty() {
        return Err(DatabaseError::InvalidRequest("Scratchpad item title cannot be empty".to_string()));
    }
    let mut conn = get_connection()?;
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .db_context("begin insert voice scratchpad tx")?;
    let count: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM voice_scratchpad_items WHERE session_id = ?1",
            params![session_id],
            |r| r.get(0),
        )
        .db_context("count voice scratchpad items")?;
    if count as usize >= max_items {
        return Err(DatabaseError::InvalidRequest(format!(
            "Scratchpad limit reached ({max_items} items). Complete or clear existing items first."
        )));
    }
    tx.execute(
        "INSERT INTO voice_scratchpad_items (
            session_id, project_id, title, status, note, created_epoch_seconds, updated_epoch_seconds
         ) VALUES (?1, ?2, ?3, 'pending', ?4, ?5, ?5)",
        params![session_id, project_id, clean_title, note, now_epoch],
    )
    .db_context("insert voice scratchpad item")?;
    let id = tx.last_insert_rowid();
    tx.commit().db_context("commit insert voice scratchpad tx")?;

    Ok(DbVoiceScratchpadItem {
        id,
        session_id: session_id.to_string(),
        project_id,
        title: clean_title.to_string(),
        status: "pending".to_string(),
        note: note.map(str::to_string),
        created_epoch_seconds: now_epoch,
        updated_epoch_seconds: now_epoch,
    })
}

/// Updates a scratchpad item scoped strictly to `session_id`.
pub fn db_update_voice_scratchpad_item(
    session_id: &str,
    item_id: i64,
    status: Option<&str>,
    title: Option<&str>,
    note: Option<Option<&str>>,
    now_epoch: i64,
) -> Result<DbVoiceScratchpadItem, DatabaseError> {
    if let Some(st) = status {
        if !matches!(st, "pending" | "in_progress" | "done") {
            return Err(DatabaseError::InvalidRequest(format!(
                "Invalid scratchpad status '{st}': expected 'pending', 'in_progress', or 'done'"
            )));
        }
    }
    let mut conn = get_connection()?;
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .db_context("begin update voice scratchpad tx")?;
    let existing = tx
        .query_row(
            "SELECT id, session_id, project_id, title, status, note, created_epoch_seconds, updated_epoch_seconds
             FROM voice_scratchpad_items
             WHERE id = ?1 AND session_id = ?2",
            params![item_id, session_id],
            voice_scratchpad_from_row,
        )
        .optional()
        .db_context("lookup voice scratchpad item")?
        .ok_or(DatabaseError::NotFound {
            entity: "Voice scratchpad item",
            id: item_id.to_string(),
        })?;

    let new_title = match title.map(str::trim) {
        Some("") => {
            return Err(DatabaseError::InvalidRequest(
                "Scratchpad item title cannot be empty".to_string(),
            ))
        }
        Some(t) => t.to_string(),
        None => existing.title,
    };
    let new_status = status.map(str::to_string).unwrap_or(existing.status);
    let new_note = match note {
        Some(opt) => opt.map(str::to_string),
        None => existing.note,
    };

    tx.execute(
        "UPDATE voice_scratchpad_items
         SET title = ?1, status = ?2, note = ?3, updated_epoch_seconds = ?4
         WHERE id = ?5 AND session_id = ?6",
        params![new_title, new_status, new_note, now_epoch, item_id, session_id],
    )
    .db_context("update voice scratchpad item")?;
    tx.commit().db_context("commit update voice scratchpad tx")?;

    Ok(DbVoiceScratchpadItem {
        id: existing.id,
        session_id: existing.session_id,
        project_id: existing.project_id,
        title: new_title,
        status: new_status,
        note: new_note,
        created_epoch_seconds: existing.created_epoch_seconds,
        updated_epoch_seconds: now_epoch,
    })
}

/// Deletes a single scratchpad item scoped strictly to `session_id`.
pub fn db_delete_voice_scratchpad_item(session_id: &str, item_id: i64) -> Result<(), DatabaseError> {
    let conn = get_connection()?;
    let affected = conn
        .execute(
            "DELETE FROM voice_scratchpad_items WHERE id = ?1 AND session_id = ?2",
            params![item_id, session_id],
        )
        .db_context("delete voice scratchpad item")?;
    if affected == 0 {
        return Err(DatabaseError::NotFound {
            entity: "Voice scratchpad item",
            id: item_id.to_string(),
        });
    }
    Ok(())
}

/// Clears either completed (`done_only = true`) or all scratchpad items for `session_id`.
pub fn db_clear_voice_scratchpad(session_id: &str, done_only: bool) -> Result<usize, DatabaseError> {
    let conn = get_connection()?;
    let sql = if done_only {
        "DELETE FROM voice_scratchpad_items WHERE session_id = ?1 AND status = 'done'"
    } else {
        "DELETE FROM voice_scratchpad_items WHERE session_id = ?1"
    };
    let deleted = conn
        .execute(sql, params![session_id])
        .db_context("clear voice scratchpad")?;
    Ok(deleted)
}

/// Lists the most recent `limit` turn ledger entries in chronological order (oldest -> newest).
pub fn db_list_voice_turn_ledger(
    session_id: &str,
    limit: usize,
) -> Result<Vec<DbVoiceTurnLedgerEntry>, DatabaseError> {
    let conn = get_connection()?;
    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, project_id, user_transcript, actions_summary, assistant_transcript, created_epoch_seconds
             FROM voice_turn_ledger
             WHERE session_id = ?1
             ORDER BY id DESC
             LIMIT ?2",
        )
        .db_context("prepare voice turn ledger list")?;
    let rows = stmt
        .query_map(params![session_id, limit as i64], voice_turn_from_row)
        .db_context("query voice turn ledger")?;
    let mut entries = collect_rows(rows, "read voice turn ledger row")?;
    entries.reverse();
    Ok(entries)
}

/// Appends a turn entry and deterministically folds overflow rows beyond `max_entries` into `rolling_summary`
/// inside one atomic SQLite transaction.
#[allow(clippy::too_many_arguments)]
pub fn db_append_voice_turn_and_fold(
    session_id: &str,
    project_id: Option<i64>,
    user_transcript: Option<&str>,
    actions_summary: Option<&str>,
    assistant_transcript: Option<&str>,
    max_entries: usize,
    max_summary_chars: usize,
    now_epoch: i64,
) -> Result<(), DatabaseError> {
    let mut conn = get_connection()?;
    append_voice_turn_and_fold_on(
        &mut conn,
        session_id,
        project_id,
        user_transcript,
        actions_summary,
        assistant_transcript,
        max_entries,
        max_summary_chars,
        now_epoch,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn append_voice_turn_and_fold_on(
    conn: &mut rusqlite::Connection,
    session_id: &str,
    project_id: Option<i64>,
    user_transcript: Option<&str>,
    actions_summary: Option<&str>,
    assistant_transcript: Option<&str>,
    max_entries: usize,
    max_summary_chars: usize,
    now_epoch: i64,
) -> Result<(), DatabaseError> {
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .db_context("begin voice turn append tx")?;
    tx.execute(
        "INSERT INTO voice_turn_ledger (
            session_id, project_id, user_transcript, actions_summary, assistant_transcript, created_epoch_seconds
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            session_id,
            project_id,
            user_transcript,
            actions_summary,
            assistant_transcript,
            now_epoch,
        ],
    )
    .db_context("insert voice turn ledger entry")?;

    let mut all_rows: Vec<DbVoiceTurnLedgerEntry> = {
        let mut stmt = tx
            .prepare(
                "SELECT id, session_id, project_id, user_transcript, actions_summary, assistant_transcript, created_epoch_seconds
                 FROM voice_turn_ledger
                 WHERE session_id = ?1
                 ORDER BY id ASC",
            )
            .db_context("prepare voice turn overflow scan")?;
        let mapped = stmt
            .query_map(params![session_id], voice_turn_from_row)
            .db_context("scan voice turn overflow")?;
        collect_rows(mapped, "read voice turn overflow row")?
    };

    if all_rows.len() > max_entries {
        let overflow_count = all_rows.len() - max_entries;
        let overflow: Vec<DbVoiceTurnLedgerEntry> = all_rows.drain(..overflow_count).collect();
        let current_summary: String = tx
            .query_row(
                "SELECT rolling_summary FROM voice_sessions WHERE id = ?1",
                params![session_id],
                |r| r.get(0),
            )
            .db_context("read voice rolling summary")?;
        let updated_summary = fold_voice_turns_into_summary(&current_summary, &overflow, max_summary_chars);
        let cutoff_id = overflow.last().map(|e| e.id).unwrap_or(0);
        tx.execute(
            "DELETE FROM voice_turn_ledger WHERE session_id = ?1 AND id <= ?2",
            params![session_id, cutoff_id],
        )
        .db_context("prune folded voice turns")?;
        tx.execute(
            "UPDATE voice_sessions SET rolling_summary = ?1, updated_epoch_seconds = ?2 WHERE id = ?3",
            params![updated_summary, now_epoch, session_id],
        )
        .db_context("update voice rolling summary")?;
    } else {
        tx.execute(
            "UPDATE voice_sessions SET updated_epoch_seconds = ?1 WHERE id = ?2",
            params![now_epoch, session_id],
        )
        .db_context("touch voice session updated_at")?;
    }

    tx.commit().db_context("commit voice turn append tx")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestProjectGuard(usize);
    impl Drop for TestProjectGuard {
        fn drop(&mut self) {
            let _ = db_remove_project(self.0);
        }
    }

    #[test]
    fn test_init_flags_crud_with_options() {
        init_database().expect("init_database should succeed");
        let proj = db_add_project("/tmp/test_init_flag_proj", "test_init_flag_proj")
            .expect("add test project to the per-process test database");
        let project_id = proj.id as i64;
        let _guard = TestProjectGuard(proj.id);

        let rec = NewInitFlag {
            project_id,
            flag_type: "docs".to_string(),
            title: "Test Documentation Flag".to_string(),
            description: Some("Missing spec doc".to_string()),
            recommended_option: Some("spec-driven".to_string()),
            options_json: Some("[\"spec-driven\",\"fast-iterative\"]".to_string()),
            metadata_json: Some("{\"key\":\"value\"}".to_string()),
            raised_by_agent_uid: None,
        };
        let flag_id = db_insert_init_flag(&rec).expect("insert init flag should succeed");
        assert!(flag_id > 0);

        let flags = db_get_init_flags(project_id, Some("pending")).expect("get init flags should succeed");
        let found = flags.iter().find(|f| f.id == flag_id);
        assert!(found.is_some());
        let f = found.unwrap();
        assert_eq!(f.flag_type, "docs");
        assert_eq!(f.title, "Test Documentation Flag");
        assert_eq!(f.recommended_option.as_deref(), Some("spec-driven"));
        assert_eq!(f.options_json.as_deref(), Some("[\"spec-driven\",\"fast-iterative\"]"));

        db_resolve_init_flag(flag_id, "resolved", None).expect("resolve flag should succeed");
        let flags_after = db_get_init_flags(project_id, Some("resolved")).expect("get resolved flags");
        assert!(flags_after.iter().any(|f| f.id == flag_id && f.status == "resolved"));
    }

    /// Temporary directory removed on drop; keeps the schema tests away from the real database.
    struct TempDir(std::path::PathBuf);
    impl TempDir {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("oneship-db-test-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&dir).expect("create temp dir");
            TempDir(dir)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn table_exists(conn: &rusqlite::Connection, name: &str) -> bool {
        conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![name],
            |row| row.get::<_, i64>(0),
        )
        .expect("probe table")
            > 0
    }

    #[test]
    fn legacy_conversation_tables_are_backed_up_then_dropped_once() {
        let dir = TempDir::new();
        let db_path = dir.0.join("oneship.db");
        let conn = rusqlite::Connection::open(&db_path).expect("open db");
        conn.execute_batch(
            "CREATE TABLE agent_conversations (id TEXT PRIMARY KEY);
             INSERT INTO agent_conversations VALUES ('legacy-conv');
             CREATE TABLE agent_chat_turns (id INTEGER PRIMARY KEY);",
        )
        .expect("seed legacy tables");

        reset_conversation_schema_if_outdated(&conn, &db_path).expect("reset");

        assert!(!table_exists(&conn, "agent_conversations"));
        assert!(!table_exists(&conn, "agent_chat_turns"));
        let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0)).expect("version");
        assert_eq!(version, CONVERSATION_SCHEMA_VERSION);
        let backups: Vec<_> = fs::read_dir(&dir.0)
            .expect("read dir")
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("oneship.pre-spec30-"))
            .collect();
        assert_eq!(backups.len(), 1, "exactly one backup expected");
        let backup = rusqlite::Connection::open(backups[0].path()).expect("open backup");
        let preserved: i64 = backup
            .query_row("SELECT count(*) FROM agent_conversations", [], |row| row.get(0))
            .expect("backup keeps legacy rows");
        assert_eq!(preserved, 1);

        // A second call must not touch tables created afterwards.
        conn.execute_batch("CREATE TABLE agent_conversations (id TEXT PRIMARY KEY)").expect("recreate");
        reset_conversation_schema_if_outdated(&conn, &db_path).expect("second reset");
        assert!(table_exists(&conn, "agent_conversations"));
    }

    #[test]
    fn journal_rebuild_keeps_rows_and_is_idempotent() {
        let dir = TempDir::new();
        let db_path = dir.0.join("oneship.db");
        let mut conn = rusqlite::Connection::open(&db_path).expect("open db");
        conn.execute_batch(
            "CREATE TABLE projects (id INTEGER PRIMARY KEY);
             INSERT INTO projects VALUES (1);
             CREATE TABLE agent_conversations (id TEXT PRIMARY KEY);
             CREATE TABLE agent_session_journal (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id INTEGER NOT NULL,
                agent_id TEXT NOT NULL,
                agy_conversation_id TEXT NOT NULL,
                title TEXT NOT NULL,
                summary TEXT NOT NULL,
                decisions TEXT, files_touched TEXT, technical_debt TEXT, tags TEXT,
                context_pct REAL, total_tokens INTEGER,
                created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, embedding BLOB
             );
             INSERT INTO agent_session_journal (project_id, agent_id, agy_conversation_id, title, summary, created_at, updated_at)
             VALUES (1, 'frontend', 'agy-1', 'Old minutes', 'kept', 10, 10);",
        )
        .expect("seed old journal");

        migrate_journal_to_conversation_key(&mut conn).expect("rebuild");
        migrate_journal_to_conversation_key(&mut conn).expect("second rebuild is a no-op");

        let (title, conv, agy): (String, Option<String>, Option<String>) = conn
            .query_row(
                "SELECT title, conversation_id, agy_conversation_id FROM agent_session_journal",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("row survives");
        assert_eq!(title, "Old minutes");
        assert_eq!(conv, None);
        assert_eq!(agy.as_deref(), Some("agy-1"));
        // New rows may omit the native reference.
        conn.execute(
            "INSERT INTO agent_session_journal (project_id, agent_id, title, summary, created_at, updated_at) VALUES (1, 'frontend', 't', 's', 1, 1)",
            [],
        )
        .expect("agy_conversation_id is nullable after the rebuild");
    }

    fn search_fixture() -> (TempDir, rusqlite::Connection) {
        let dir = TempDir::new();
        let conn = rusqlite::Connection::open(dir.0.join("oneship.db")).expect("open db");
        conn.execute_batch(
            "CREATE TABLE agent_conversations (
                id TEXT PRIMARY KEY, project_id INTEGER NOT NULL, agent_id TEXT NOT NULL, topic TEXT NOT NULL,
                status TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, artifacts_touched TEXT,
                runner TEXT, model TEXT, account_id TEXT, effort TEXT, closed_at_ms INTEGER, close_reason TEXT,
                summary TEXT, reopened_from TEXT, agent_uid TEXT);
             CREATE TABLE conversation_native_sessions (
                id INTEGER PRIMARY KEY, conversation_id TEXT, native_session_id TEXT);
             CREATE TABLE agent_session_journal (
                id INTEGER PRIMARY KEY, conversation_id TEXT, title TEXT, summary TEXT, decisions TEXT, tags TEXT);
             CREATE VIRTUAL TABLE conversation_search USING fts5(
                conversation_id UNINDEXED, project_id UNINDEXED, agent_id UNINDEXED, content);
             INSERT INTO agent_conversations (id, project_id, agent_id, topic, status, created_at, updated_at, summary) VALUES
                ('c-db', 1, 'frontend', 'Database migration', 'archived', 1, 1, 'Moved the journal to conversation keys'),
                ('c-ui', 1, 'frontend', 'Button styling', 'active', 2, 2, NULL),
                ('c-other-agent', 1, 'backend', 'Database indexes', 'archived', 3, 3, NULL),
                ('c-other-project', 2, 'frontend', 'Database cleanup', 'archived', 4, 4, NULL);
             INSERT INTO agent_session_journal (conversation_id, title, summary, decisions, tags)
                VALUES ('c-ui', 'Palette work', 'adjusted tokens', 'prefer semantic variables', 'css,theme');",
        )
        .expect("seed fixture");
        for id in ["c-db", "c-ui", "c-other-agent", "c-other-project"] {
            reindex_conversation(&conn, id).expect("index");
        }
        (dir, conn)
    }

    #[test]
    fn conversation_search_is_scoped_ranked_and_filterable() {
        let (_dir, conn) = search_fixture();
        let q = fts_query_from_text("resume the database topic").expect("query");

        let hits = search_conversations_on(&conn, 1, "frontend", &q, None, 10).expect("search");
        let ids: Vec<&str> = hits.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["c-db"], "other agents, other projects and unrelated topics are excluded");

        // Searchable through Minutes and tags, not only the topic.
        let q = fts_query_from_text("semantic theme").expect("query");
        let hits = search_conversations_on(&conn, 1, "frontend", &q, None, 10).expect("search minutes");
        assert_eq!(hits.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), vec!["c-ui"]);

        // Status filter and result bound.
        let q = fts_query_from_text("database button").expect("query");
        assert_eq!(search_conversations_on(&conn, 1, "frontend", &q, Some("active"), 10).expect("active").len(), 1);
        assert_eq!(search_conversations_on(&conn, 1, "frontend", &q, None, 1).expect("bounded").len(), 1);
    }

    #[test]
    fn fts_query_quotes_user_input_and_rejects_empty_text() {
        assert_eq!(fts_query_from_text("a \"b\" c-d").as_deref(), Some("\"a\" OR \"b\" OR \"c-d\""));
        assert_eq!(fts_query_from_text("  !!  "), None);
    }

    #[test]
    fn log_prune_keeps_only_the_newest_rows() {
        let conn = rusqlite::Connection::open_in_memory().expect("open memory db");
        conn.execute_batch(
            "CREATE TABLE system_logs (id INTEGER PRIMARY KEY AUTOINCREMENT, level TEXT, source TEXT, origin TEXT, message TEXT);",
        )
        .expect("create table");
        for i in 0..10 {
            conn.execute("INSERT INTO system_logs (level, source, origin, message) VALUES ('INFO', 't', 's', ?1)", params![i.to_string()])
                .expect("seed row");
        }

        assert_eq!(prune_system_logs_on(&conn, 3).expect("prune"), 7);
        let kept: Vec<String> = conn
            .prepare("SELECT message FROM system_logs ORDER BY id")
            .expect("prepare")
            .query_map([], |row| row.get(0))
            .expect("query")
            .map(|r| r.expect("row"))
            .collect();
        assert_eq!(kept, vec!["7", "8", "9"]);
        assert_eq!(prune_system_logs_on(&conn, 3).expect("second prune"), 0, "fewer rows than the bound deletes nothing");
    }

    #[test]
    fn fresh_database_is_only_stamped() {
        let dir = TempDir::new();
        let db_path = dir.0.join("oneship.db");
        let conn = rusqlite::Connection::open(&db_path).expect("open db");

        reset_conversation_schema_if_outdated(&conn, &db_path).expect("reset");

        let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0)).expect("version");
        assert_eq!(version, CONVERSATION_SCHEMA_VERSION);
        let backups = fs::read_dir(&dir.0)
            .expect("read dir")
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("oneship.pre-spec30-"))
            .count();
        assert_eq!(backups, 0, "nothing to back up on a fresh database");
    }

    #[test]
    fn closing_a_conversation_ends_its_native_sessions_with_a_mapped_reason() {
        let conn = rusqlite::Connection::open_in_memory().expect("open memory db");
        conn.execute_batch(
            "CREATE TABLE agent_conversations (
                 id TEXT PRIMARY KEY, project_id INTEGER, agent_id TEXT, status TEXT,
                 closed_at_ms INTEGER, close_reason TEXT);
             CREATE TABLE conversation_native_sessions (
                 id INTEGER PRIMARY KEY AUTOINCREMENT, conversation_id TEXT,
                 ended_at_ms INTEGER, end_reason TEXT);
             INSERT INTO agent_conversations VALUES ('c1', 1, 'frontend', 'active', NULL, NULL);
             INSERT INTO agent_conversations VALUES ('c2', 1, 'backend', 'active', NULL, NULL);
             INSERT INTO conversation_native_sessions (conversation_id) VALUES ('c1');
             INSERT INTO conversation_native_sessions (conversation_id) VALUES ('c2');",
        )
        .expect("seed");

        let closed = close_active_conversations(&conn, 1, "frontend", "superseded").expect("close");

        assert_eq!(closed, vec!["c1".to_string()]);
        let (status, reason, closed_at): (String, String, Option<i64>) = conn
            .query_row(
                "SELECT status, close_reason, closed_at_ms FROM agent_conversations WHERE id = 'c1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("c1");
        assert_eq!((status.as_str(), reason.as_str()), ("archived", "superseded"));
        assert!(closed_at.is_some());
        let end_reason: String = conn
            .query_row("SELECT end_reason FROM conversation_native_sessions WHERE conversation_id = 'c1'", [], |row| row.get(0))
            .expect("session end reason");
        assert_eq!(end_reason, "runner_switch");
        // Another agent's conversation and session are untouched.
        let other_status: String = conn
            .query_row("SELECT status FROM agent_conversations WHERE id = 'c2'", [], |row| row.get(0))
            .expect("c2");
        assert_eq!(other_status, "active");
        let other_open: i64 = conn
            .query_row("SELECT count(*) FROM conversation_native_sessions WHERE conversation_id = 'c2' AND ended_at_ms IS NULL", [], |row| row.get(0))
            .expect("c2 sessions");
        assert_eq!(other_open, 1);
    }

    #[test]
    fn voice_turn_ledger_folds_overflow_deterministically_into_summary() {
        let mut conn = rusqlite::Connection::open_in_memory().expect("open memory db");
        conn.execute_batch(
            "CREATE TABLE voice_sessions (
                id TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                active_project_id INTEGER,
                current_objective TEXT,
                last_delegated_project_id INTEGER,
                last_delegated_agent_id TEXT,
                resumption_handle TEXT,
                resumption_valid_until_epoch_seconds INTEGER,
                rolling_summary TEXT NOT NULL DEFAULT '',
                created_epoch_seconds INTEGER NOT NULL,
                updated_epoch_seconds INTEGER NOT NULL
            );
            CREATE TABLE voice_turn_ledger (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                project_id INTEGER,
                user_transcript TEXT,
                actions_summary TEXT,
                assistant_transcript TEXT,
                created_epoch_seconds INTEGER NOT NULL
            );
            INSERT INTO voice_sessions VALUES ('s1', 'active', 1, NULL, NULL, NULL, NULL, NULL, '', 100, 100);",
        )
        .expect("seed voice tables");

        for i in 1..=5 {
            append_voice_turn_and_fold_on(
                &mut conn,
                "s1",
                Some(1),
                Some(&format!("turn {i} intent")),
                Some(&format!("project.list_tasks -> ok_{i}")),
                Some(&format!("reply {i}")),
                2,
                500,
                100 + i,
            )
            .expect("append turn");
        }

        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM voice_turn_ledger WHERE session_id = 's1'", [], |r| r.get(0))
            .expect("count remaining");
        assert_eq!(remaining, 2);

        let summary: String = conn
            .query_row("SELECT rolling_summary FROM voice_sessions WHERE id = 's1'", [], |r| r.get(0))
            .expect("read summary");
        assert!(summary.contains("turn 1 intent"));
        assert!(summary.contains("turn 3 intent"));
        assert!(!summary.contains("turn 4 intent"));
        assert!(summary.chars().count() <= 500);
    }
}
