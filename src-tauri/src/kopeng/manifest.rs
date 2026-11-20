use std::fs;
use std::path::{Path, PathBuf};

/// Files managed by ONE-SHIP inside a registered project root.
pub const ONESHIP_MANAGED_FILES: &[&str] = &[
    "rula.toml",
];

/// Directories managed by ONE-SHIP inside a registered project root.
pub const ONESHIP_MANAGED_DIRECTORIES: &[&str] = &[
    ".one-ship",
];

/// Opening HTML marker delimiting the managed governance block in `AGENTS.md`.
pub const MANAGED_BLOCK_START: &str = "<!-- ONE_SHIP_MANAGED_START -->";
/// Closing HTML marker delimiting the managed governance block in `AGENTS.md`.
pub const MANAGED_BLOCK_END: &str = "<!-- ONE_SHIP_MANAGED_END -->";

/// MCP server entries ONE-SHIP writes into the runner configs (`skill::inject_project_mcp_plugin`).
const ONESHIP_MCP_SERVER_KEYS: &[&str] = &["one-ship", "rea", "desigma", "pcbcraft"];

/// Canonical governance block injected into `AGENTS.md` for every onboarded project.
pub const ONE_SHIP_MANAGED_BLOCK: &str = r#"<!-- ONE_SHIP_MANAGED_START -->
# ONE-SHIP Agent Governance & Communication Protocol

## 1. Terminal stdout Silence & MCP Response Channel
- Your terminal pane is strictly an internal execution and reasoning log.
- NEVER output final conversational dialogue or chat text directly to terminal stdout.
- You MUST deliver every user-directed response by calling the MCP tool `post_agent_response`.

## 2. Response Structure: Spoken Dialogue vs Technical Artifact
- `dialogue` (required string):
  - Natural, spoken-style language suitable for Text-To-Speech (TTS).
  - Strictly NO markdown tables, NO code blocks (no triple backticks), NO raw URLs, NO ASCII diagrams.
  - Conversational, concise, and direct.
- `artifact` (optional string):
  - Relative file path (`scratchpad/<name>.md`, `docs/<name>.md`) or raw technical payloads (unified diffs, code snippets, markdown tables, execution plans, JSON schemas).
  - If the turn requires no code or structured data (e.g. status updates, answers to conceptual questions), set `artifact` to `null` or omit it.
- `artifact_type` (optional string): `"code" | "diff" | "markdown" | "json" | "plan"`.
- `artifact_lang` (optional string): Syntax highlighting identifier (e.g. `"rust"`, `"typescript"`).
- `approval` (optional object): Embedded confirmation request `{ title: string, description: string, options?: string[] }`.

## 3. Mandatory User Authorization & Proposals
- Before executing any unrequested file modification, destructive terminal command, or architectural change, you MUST present a proposal and request human approval via the embedded `approval` object in `post_agent_response`.

## 4. Token Economy & Lazy Documentation Ingestion
- DO NOT preload or eagerly read large documentation files on initialization.
- Inspect files and documentation on-demand using read tools ONLY when directly relevant to the current user prompt.

## 5. Database-First Development Journals & Tasks
- Do NOT create or maintain flat markdown log files (`devlog/`, `TODO.md`, `TECHNICAL_DEBT.md`) on disk.
- MANDATORY: Before concluding any turn that modifies code, implements a feature, fixes a bug, or introduces architectural decisions, you MUST record your work via `record_session_minutes`.
- Inspect past milestones and project history via `query_session_journal`.
- Manage active project tasks via the MCP tool `manage_todo_item`.
- Record technical debt and bottlenecks via the MCP tool `record_technical_debt`.

## 6. Project Script & Service Lifecycle Protocol
- NEVER execute project build, test, lint, or service commands via raw bash/terminal commands if they are registered in the project's `rula.toml` catalog.
- Inspect project capabilities using `list_project_scripts` or `discover_project`.
- Execute finite verification tasks (e.g. `test`, `build`, `lint`) exclusively via `start_project_task(task_id="...", wait=true)`.
- Manage persistent dev servers and watchers exclusively via `manage_project_script(script_id="...", action="start"|"stop"|"restart")`.

## 7. Native MCP Tool Invocations Only (Strict Script Execution Ban)
- All ONE-SHIP tools (`post_agent_response`, `request_approval`, `record_session_minutes`, `query_session_journal`, `start_project_task`, `manage_project_script`, `list_project_scripts`, `manage_todo_item`, `record_technical_debt`, `resolve_initialization_flag`) are MCP tools provided by the `one-ship` MCP server.
- ALWAYS invoke these tools using your agent environment's native MCP tool calling facility (`call_mcp_tool` or direct tool wrappers).
- Writing Python scripts, bash curl commands, or shell scripts to make raw HTTP requests to `http://localhost:8765/mcp` is **STRICTLY PROHIBITED**.
- Call `post_agent_response` EXACTLY ONCE at the conclusion of your turn when all scanning and staging are complete. NEVER make dummy, ping, or probe calls.

## 8. Host Platform Protection & Zero Introspection Policy
- STRICTLY FORBIDDEN: Agents must NEVER inspect, query, read, or manipulate ONE-SHIP internal files, directories, or databases:
  * Platform configuration, logs, and state directories: `~/.config/one-ship/` (including `oneship.db`, `oneship_telemetry.duckdb`, `config.toml`, and runtime logs).
  * Executing database queries against the platform database (e.g. `sqlite3 ~/.config/one-ship/oneship.db ...` is strictly prohibited).
  * Inspecting or grepping ONE-SHIP source code repositories (`one-ship/src-tauri/`, `one-ship/src/`).
- All interactions with the platform (services, tasks, journals, todos, tech debt, initialization) MUST be executed exclusively through the official ONE-SHIP MCP tools. If an MCP tool returns an error, explain the issue in dialogue; never attempt to bypass MCP tools by querying or editing the platform database directly.
<!-- ONE_SHIP_MANAGED_END -->"#;

/// Failure of a project integration or de-integration step (AGENTS.md, artifacts, knowledge export).
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("{context}: {source}")]
    Io { context: String, source: std::io::Error },
    #[error("Invalid {path}: {reason}")]
    InvalidFile { path: String, reason: String },
    #[error("Project directory '{0}' does not exist")]
    ProjectDirMissing(String),
    #[error("ONE-SHIP artifacts could not be fully removed: {}", .failures.join("; "))]
    PurgeIncomplete { failures: Vec<String> },
    #[error("Background task failed: {0}")]
    Task(String),
    #[error(transparent)]
    Registry(#[from] crate::registry::RegistryError),
    #[error(transparent)]
    Database(#[from] crate::db::DatabaseError),
}

impl crate::api::ApiError for ManifestError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::InvalidFile { .. } | Self::PurgeIncomplete { .. } => StatusCode::CONFLICT,
            Self::ProjectDirMissing(_) => StatusCode::NOT_FOUND,
            Self::Io { .. } | Self::Task(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Registry(e) => e.status(),
            Self::Database(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::Io { .. } => "ManifestError::Io",
            Self::InvalidFile { .. } => "ManifestError::InvalidFile",
            Self::ProjectDirMissing(_) => "ManifestError::ProjectDirMissing",
            Self::PurgeIncomplete { .. } => "ManifestError::PurgeIncomplete",
            Self::Task(_) => "ManifestError::Task",
            Self::Registry(e) => e.code(),
            Self::Database(e) => e.code(),
        }
    }
}

fn io_err(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> ManifestError {
    let context = context.into();
    move |source| ManifestError::Io { context, source }
}

/// `.one-ship/backups/` of a project: originals of files ONE-SHIP replaced (AGENTS.md, CLAUDE.md).
pub(crate) fn backups_dir(project_path: &Path) -> PathBuf {
    project_path.join(".one-ship").join("backups")
}

/// Whether anything (file, directory or symlink, dangling included) exists at `path`.
fn entry_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// All files, directories and skill links owned by ONE-SHIP in a project.
///
/// Static entries, every platform and specialized skill link, and the persona/workflow documents
/// that `rula.toml` declares under `.agents/personas/` or `.agents/workflows/`. An unreadable or
/// invalid `rula.toml` is an error.
pub fn resolve_oneship_artifacts(project_path: &Path) -> Result<Vec<PathBuf>, ManifestError> {
    let mut artifacts: Vec<PathBuf> = ONESHIP_MANAGED_FILES
        .iter()
        .chain(ONESHIP_MANAGED_DIRECTORIES)
        .map(|rel| project_path.join(rel))
        .chain(
            crate::kopeng::skill::PLATFORM_BASE_SKILLS
                .iter()
                .chain(crate::kopeng::skill::SPECIALIZED_SKILLS)
                .map(|name| project_path.join(".agents").join("skills").join(name)),
        )
        .filter(|p| entry_exists(p))
        .collect();

    let rula_file = project_path.join("rula.toml");
    let content = match fs::read_to_string(&rula_file) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(artifacts),
        Err(e) => return Err(io_err(format!("Cannot read {}", rula_file.display()))(e)),
    };
    let value: toml::Value = toml::from_str(&content)
        .map_err(|e: toml::de::Error| ManifestError::InvalidFile { path: rula_file.display().to_string(), reason: e.to_string() })?;
    let Some(agents) = value.get("agents").and_then(|a| a.as_table()) else {
        return Ok(artifacts);
    };
    for agent in agents.values() {
        let docs = agent.get("context_docs").and_then(|d| d.as_array()).into_iter().flatten();
        for doc in docs.filter_map(|d| d.as_str()) {
            if !(doc.starts_with(".agents/personas/") || doc.starts_with(".agents/workflows/")) || doc.contains("..") {
                continue;
            }
            let p = project_path.join(doc);
            if entry_exists(&p) && !artifacts.contains(&p) {
                artifacts.push(p);
            }
        }
    }
    Ok(artifacts)
}

/// Prepares AGENTS.md in the project root by safely wrapping or updating the ONE-SHIP
/// managed governance block while preserving any pre-existing project rules.
///
/// The first time, a pre-existing AGENTS.md is backed up to `.one-ship/backups/AGENTS.md.orig`
/// (or `AGENTS.md.created` records that ONE-SHIP created it) so de-integration can revert it.
pub fn prepare_project_agents_md(project_path: &Path) -> Result<(), ManifestError> {
    let agents_md_path = project_path.join("AGENTS.md");
    let backup_dir = backups_dir(project_path);
    fs::create_dir_all(&backup_dir).map_err(io_err(format!("Cannot create {}", backup_dir.display())))?;

    let orig_backup = backup_dir.join("AGENTS.md.orig");
    let created_marker = backup_dir.join("AGENTS.md.created");

    if agents_md_path.exists() {
        let existing_content = fs::read_to_string(&agents_md_path).map_err(io_err("Cannot read AGENTS.md"))?;
        if !orig_backup.exists() && !created_marker.exists() {
            fs::write(&orig_backup, &existing_content).map_err(io_err("Cannot write the AGENTS.md backup"))?;
        }
        let cleaned = strip_managed_blocks(&existing_content);
        let new_content = format!("{}\n\n{}", ONE_SHIP_MANAGED_BLOCK, cleaned.trim_start());
        fs::write(&agents_md_path, new_content).map_err(io_err("Cannot update AGENTS.md"))?;
    } else {
        fs::write(&created_marker, "created_by_one_ship").map_err(io_err("Cannot write the AGENTS.md created marker"))?;
        let baseline_content = format!(
            "{}\n\n# Project Guidelines\n\n## Architecture Overview\n<!-- Document core architectural boundaries and stack conventions here -->\n\n## Development & Verification\n<!-- Document key build, test, and lint commands here -->\n",
            ONE_SHIP_MANAGED_BLOCK
        );
        fs::write(&agents_md_path, baseline_content).map_err(io_err("Cannot create AGENTS.md"))?;
    }

    crate::logger::log("INFO", "manifest", &format!("Prepared AGENTS.md with ONE-SHIP protocol for {}", project_path.display()));
    Ok(())
}

/// `content` without any `MANAGED_BLOCK_START … MANAGED_BLOCK_END` block.
fn strip_managed_blocks(content: &str) -> String {
    let mut result = content.to_string();
    let max_blocks_count = content.matches(MANAGED_BLOCK_START).count();
    for _ in 0..max_blocks_count {
        let (Some(start_idx), Some(end_idx)) = (result.find(MANAGED_BLOCK_START), result.find(MANAGED_BLOCK_END)) else {
            break;
        };
        if start_idx > end_idx {
            break;
        }
        let actual_end = end_idx + MANAGED_BLOCK_END.len();
        let mut next = result[..start_idx].to_string();
        next.push_str(result[actual_end..].trim_start_matches('\n'));
        result = next;
    }
    result
}

/// Collects the outcome of one de-integration step.
struct PurgeLog {
    removed: Vec<String>,
    failures: Vec<String>,
}

impl PurgeLog {
    fn record(&mut self, what: &Path, result: Result<(), ManifestError>) {
        match result {
            Ok(()) => self.removed.push(what.display().to_string()),
            Err(e) => self.failures.push(e.to_string()),
        }
    }
}

/// Reads a file; `Ok(None)` when it does not exist.
fn read_optional(path: &Path) -> Result<Option<String>, ManifestError> {
    match fs::read_to_string(path) {
        Ok(c) => Ok(Some(c)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_err(format!("Cannot read {}", path.display()))(e)),
    }
}

fn write_file(path: &Path, content: &str) -> Result<(), ManifestError> {
    fs::write(path, content).map_err(io_err(format!("Cannot write {}", path.display())))
}

fn remove_file(path: &Path) -> Result<(), ManifestError> {
    fs::remove_file(path).map_err(io_err(format!("Cannot remove {}", path.display())))
}

/// Restores AGENTS.md to its pre-ONE-SHIP state: removed if ONE-SHIP created it, the original
/// restored if one was backed up, else the managed block stripped.
fn revert_agents_md(project_path: &Path) -> Result<(), ManifestError> {
    let backup_dir = backups_dir(project_path);
    let agents_md_path = project_path.join("AGENTS.md");
    if backup_dir.join("AGENTS.md.created").exists() {
        return if entry_exists(&agents_md_path) { remove_file(&agents_md_path) } else { Ok(()) };
    }
    if let Some(original) = read_optional(&backup_dir.join("AGENTS.md.orig"))? {
        return write_file(&agents_md_path, &original);
    }
    let Some(content) = read_optional(&agents_md_path)? else {
        return Ok(());
    };
    let cleaned = strip_managed_blocks(&content);
    if cleaned == content {
        return Ok(());
    }
    write_file(&agents_md_path, cleaned.trim())
}

/// Removes the `CLAUDE.md -> AGENTS.md` link and restores a `CLAUDE.md` that it replaced.
fn revert_claude_md(project_path: &Path) -> Result<(), ManifestError> {
    let claude_md = project_path.join("CLAUDE.md");
    if fs::read_link(&claude_md).is_ok_and(|target| target == Path::new("AGENTS.md")) {
        remove_file(&claude_md)?;
    }
    if let Some(original) = read_optional(&backups_dir(project_path).join("CLAUDE.md.orig"))? {
        write_file(&claude_md, &original)?;
    }
    Ok(())
}

/// Removes the ONE-SHIP MCP server entries from a runner config; deletes the file when nothing
/// but ONE-SHIP's entries (and `ignored_keys`) remains.
fn revert_runner_config(path: &Path, servers_key: &str, ignored_keys: &[&str]) -> Result<(), ManifestError> {
    let Some(content) = read_optional(path)? else {
        return Ok(());
    };
    let invalid = |reason: String| ManifestError::InvalidFile { path: path.display().to_string(), reason };
    let mut config: serde_json::Value = serde_json::from_str(&content).map_err(|e| invalid(e.to_string()))?;
    let root = config.as_object_mut().ok_or_else(|| invalid("not a JSON object".to_string()))?;
    if let Some(servers) = root.get_mut(servers_key).and_then(|s| s.as_object_mut()) {
        for key in ONESHIP_MCP_SERVER_KEYS {
            servers.remove(*key);
        }
        if servers.is_empty() {
            root.remove(servers_key);
        }
    }
    if root.keys().all(|k| ignored_keys.contains(&k.as_str())) {
        return remove_file(path);
    }
    let serialized = serde_json::to_string_pretty(&config).map_err(|e| invalid(e.to_string()))?;
    write_file(path, &serialized)
}

fn remove_entry(path: &Path) -> Result<(), ManifestError> {
    let meta = fs::symlink_metadata(path).map_err(io_err(format!("Cannot inspect {}", path.display())))?;
    if meta.is_dir() {
        fs::remove_dir_all(path).map_err(io_err(format!("Cannot remove {}", path.display())))
    } else {
        remove_file(path)
    }
}

/// Removes `dir` if it exists and is empty.
fn remove_dir_if_empty(dir: &Path) -> Result<(), ManifestError> {
    let mut entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(io_err(format!("Cannot list {}", dir.display()))(e)),
    };
    if entries.next().is_some() {
        return Ok(());
    }
    fs::remove_dir(dir).map_err(io_err(format!("Cannot remove {}", dir.display())))
}

/// Removes everything ONE-SHIP installed in a project and restores what it replaced
/// (AGENTS.md, CLAUDE.md, runner MCP configs), then the empty `.agents/` directories.
///
/// Every step runs; any failure makes the result `PurgeIncomplete` listing each failure.
pub fn purge_oneship_artifacts(project_path: &Path) -> Result<Vec<String>, ManifestError> {
    crate::logger::log("DEBUG", "manifest", &format!("purge_oneship_artifacts: {}", project_path.display()));
    let mut log = PurgeLog { removed: Vec::new(), failures: Vec::new() };

    // 1. Restore replaced files first: their backups live in `.one-ship/`, removed below.
    log.record(&project_path.join("AGENTS.md"), revert_agents_md(project_path));
    log.record(&project_path.join("CLAUDE.md"), revert_claude_md(project_path));
    log.record(&project_path.join(".mcp.json"), revert_runner_config(&project_path.join(".mcp.json"), "mcpServers", &[]));
    log.record(&project_path.join("opencode.json"), revert_runner_config(&project_path.join("opencode.json"), "mcp", &["$schema"]));

    // 2. Managed files, directories and skill links.
    match resolve_oneship_artifacts(project_path) {
        Ok(artifacts) => {
            for item in artifacts {
                log.record(&item, remove_entry(&item));
            }
        }
        Err(e) => log.failures.push(e.to_string()),
    }

    // 3. Empty `.agents/` directories.
    let agents_dir = project_path.join(".agents");
    for dir in ["skills", "personas", "workflows"].iter().map(|s| agents_dir.join(s)).chain([agents_dir.clone()]) {
        if let Err(e) = remove_dir_if_empty(&dir) {
            log.failures.push(e.to_string());
        }
    }

    if !log.failures.is_empty() {
        crate::logger::log("ERROR", "manifest", &format!("Purge of {} incomplete: {}", project_path.display(), log.failures.join("; ")));
        return Err(ManifestError::PurgeIncomplete { failures: log.failures });
    }
    crate::logger::log("INFO", "manifest", &format!("Purged {} ONE-SHIP artifacts from {}", log.removed.len(), project_path.display()));
    Ok(log.removed)
}

fn write_export(path: &Path, content: &str) -> Result<(), ManifestError> {
    fs::write(path, content).map_err(io_err(format!("Cannot write {}", path.display())))?;
    crate::logger::log("INFO", "manifest", &format!("Exported {}", path.display()));
    Ok(())
}

/// Exports project knowledge (Technical Debt, Tasks/Todos, Session Journal) from SQLite into
/// `TECHNICAL_DEBT.md`, `TODO.md` and `SESSION_JOURNAL.md` before project removal.
///
/// The database is the source of truth while the project is in ONE-SHIP (these files are
/// imported at onboarding), so existing files are overwritten. Target: `docs/` when it exists,
/// else the project root. A ledger with no records writes no file. Any failure is an error.
pub fn export_project_knowledge_to_markdown(project_path: &Path, project_id: i64) -> Result<(), ManifestError> {
    if !project_path.is_dir() {
        return Err(ManifestError::ProjectDirMissing(project_path.display().to_string()));
    }
    let docs_dir = project_path.join("docs");
    let target_dir = if docs_dir.is_dir() { docs_dir } else { project_path.to_path_buf() };

    // 1. Technical Debt
    let debts = crate::db::db_get_project_technical_debt(project_id, None)?;
    if !debts.is_empty() {
        let mut md = String::from("# Technical Debt Registry\n\n");
        md.push_str("> Exported from ONE-SHIP project ledger.\n\n");
        md.push_str("| ID | Category | Severity | Title | Impact Summary | Remediation Plan | Status |\n");
        md.push_str("|:---|:---|:---|:---|:---|:---|:---|\n");
        for d in debts {
            let id_str = d.id.map(|i| i.to_string()).unwrap_or_else(|| "-".to_string());
            let title = d.title.replace('|', "\\|");
            let impact = d.impact_summary.replace('|', "\\|").replace('\n', " ");
            let plan = d.remediation_plan.as_deref().unwrap_or_default().replace('|', "\\|").replace('\n', " ");
            md.push_str(&format!(
                "| {} | `{}` | `{}` | {} | {} | {} | `{}` |\n",
                id_str, d.category, d.severity, title, impact, plan, d.status
            ));
        }
        write_export(&target_dir.join("TECHNICAL_DEBT.md"), &md)?;
    }

    // 2. Todos / Tasks
    let todos = crate::db::db_get_project_todos(project_id, None)?;
    if !todos.is_empty() {
        let mut md = String::from("# Project Tasks & TODOs\n\n");
        md.push_str("> Exported from ONE-SHIP project task tracker.\n\n");
        for t in todos {
            let check = if t.status == "completed" { "[x]" } else { "[ ]" };
            let cat = if t.category.is_empty() { "general" } else { &t.category };
            md.push_str(&format!("- {} **[P{}] [{}]** {}\n", check, t.priority, cat, t.title));
            for l in t.description.as_deref().unwrap_or_default().trim().lines() {
                md.push_str(&format!("  {}\n", l));
            }
        }
        write_export(&target_dir.join("TODO.md"), &md)?;
    }

    // 3. Session Journal (every entry)
    let journal_count = crate::db::db_count_project_session_journals(project_id)?;
    let journals = crate::db::db_get_project_session_journals(project_id, None, journal_count)?;
    if !journals.is_empty() {
        let mut md = String::from("# Project Session Journal & Milestones\n\n");
        md.push_str("> Exported from ONE-SHIP agent session journal.\n\n");
        for j in journals {
            let title = if j.title.trim().is_empty() { "Session Milestone" } else { j.title.trim() };
            md.push_str(&format!("## {}\n", title));
            md.push_str(&format!("- **Agent**: `{}`\n", j.agent_id));
            md.push_str(&format!("- **Timestamp**: {}\n\n", j.created_at));
            if !j.summary.trim().is_empty() {
                md.push_str(&format!("### Summary\n{}\n\n", j.summary.trim()));
            }
            let decisions = j.decisions.as_deref().unwrap_or_default().trim();
            if !decisions.is_empty() {
                md.push_str(&format!("### Decisions & Milestones\n{}\n\n", decisions));
            }
            let files = j.files_touched.as_deref().unwrap_or_default().trim();
            if !files.is_empty() {
                md.push_str(&format!("### Files Touched\n{}\n\n", files));
            }
            md.push_str("---\n\n");
        }
        write_export(&target_dir.join("SESSION_JOURNAL.md"), &md)?;
    }

    Ok(())
}

/// Reverts a project initialization: purges ONE-SHIP artifacts and deletes the init state.
/// The project must be registered. Daemon command `rollback_project_init`.
pub async fn rollback_project_init(project_path: String) -> Result<String, ManifestError> {
    crate::logger::log("DEBUG", "manifest", &format!("rollback_project_init: {}", project_path));
    tokio::task::spawn_blocking(move || {
        let project = crate::registry::find_project_by_path(&project_path)?;
        purge_oneship_artifacts(Path::new(&project.path))?;
        crate::db::db_delete_init_state(&project.path)?;
        crate::logger::log("INFO", "manifest", &format!("Rolled back initialization for {}", project.path));
        Ok(format!("Rolled back initialization artifacts for {}", project.path))
    })
    .await
    .map_err(|e| ManifestError::Task(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purge_restores_replaced_files_and_runner_configs() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        fs::write(p.join("AGENTS.md"), "# Own rules\n").unwrap();
        fs::write(p.join("CLAUDE.md"), "own claude notes").unwrap();
        fs::create_dir_all(backups_dir(p)).unwrap();
        fs::copy(p.join("CLAUDE.md"), backups_dir(p).join("CLAUDE.md.orig")).unwrap();
        fs::remove_file(p.join("CLAUDE.md")).unwrap();
        std::os::unix::fs::symlink("AGENTS.md", p.join("CLAUDE.md")).unwrap();
        prepare_project_agents_md(p).unwrap();
        fs::write(p.join(".mcp.json"), r#"{"mcpServers":{"one-ship":{"type":"http"},"other":{"type":"http"}}}"#).unwrap();
        fs::write(p.join("opencode.json"), r#"{"$schema":"x","mcp":{"one-ship":{"type":"remote"}}}"#).unwrap();
        fs::create_dir_all(p.join(".agents/skills")).unwrap();
        std::os::unix::fs::symlink("/nonexistent/oneship-core", p.join(".agents/skills/oneship-core")).unwrap();
        fs::write(p.join("rula.toml"), "[project]\nname = \"t\"\n").unwrap();

        purge_oneship_artifacts(p).unwrap();

        assert_eq!(fs::read_to_string(p.join("AGENTS.md")).unwrap(), "# Own rules\n");
        assert_eq!(fs::read_to_string(p.join("CLAUDE.md")).unwrap(), "own claude notes");
        let mcp: serde_json::Value = serde_json::from_str(&fs::read_to_string(p.join(".mcp.json")).unwrap()).unwrap();
        assert!(mcp["mcpServers"].get("one-ship").is_none() && mcp["mcpServers"].get("other").is_some());
        assert!(!p.join("opencode.json").exists());
        assert!(!entry_exists(&p.join(".agents")));
        assert!(!p.join(".one-ship").exists());
        assert!(!p.join("rula.toml").exists());
    }

    #[test]
    fn purge_reports_an_invalid_runner_config() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join(".mcp.json"), "{not json").unwrap();
        assert!(matches!(purge_oneship_artifacts(tmp.path()), Err(ManifestError::PurgeIncomplete { .. })));
    }

    #[test]
    fn managed_blocks_are_stripped() {
        let content = format!("{}\n\nown\n{}\nx\n{}", ONE_SHIP_MANAGED_BLOCK, MANAGED_BLOCK_START, MANAGED_BLOCK_END);
        assert_eq!(strip_managed_blocks(&content).trim(), "own");
    }
}
