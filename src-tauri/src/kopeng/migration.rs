use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};
use crate::kopeng::skill::{inject_platform_skill, CURRENT_PLATFORM_VERSION};

/// Version of a project whose `rula.toml` has no `[project].oneship_version` (pre-versioning).
pub const UNVERSIONED_PLATFORM_VERSION: &str = "0.2.1";

/// Failure of a project version read/stamp or a platform migration.
#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("{context}: {source}")]
    Io { context: String, source: std::io::Error },
    #[error("Invalid {path}: {reason}")]
    InvalidRula { path: String, reason: String },
    #[error("Project '{0}' has no rula.toml; its platform version cannot be recorded")]
    MissingRula(String),
    #[error("Platform sync failed: {0}")]
    Skill(Box<crate::kopeng::skill::SkillError>),
    #[error(transparent)]
    Snapshot(#[from] crate::kopeng::snapshots::SnapshotError),
    #[error("Background task failed: {0}")]
    Task(String),
    #[error(transparent)]
    Registry(#[from] crate::registry::RegistryError),
}

impl From<crate::kopeng::skill::SkillError> for MigrationError {
    fn from(e: crate::kopeng::skill::SkillError) -> Self {
        Self::Skill(Box::new(e))
    }
}

impl crate::api::ApiError for MigrationError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::InvalidRula { .. } | Self::MissingRula(_) => StatusCode::CONFLICT,
            Self::Io { .. } | Self::Task(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Skill(e) => e.status(),
            Self::Snapshot(e) => e.status(),
            Self::Registry(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::Io { .. } => "MigrationError::Io",
            Self::InvalidRula { .. } => "MigrationError::InvalidRula",
            Self::MissingRula(_) => "MigrationError::MissingRula",
            Self::Task(_) => "MigrationError::Task",
            Self::Skill(e) => e.code(),
            Self::Snapshot(e) => e.code(),
            Self::Registry(e) => e.code(),
        }
    }
}

/// Summary returned after migrating a project's ONE-SHIP configuration/skill artifacts to the current version.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct MigrationResult {
    pub from_version: String,
    pub to_version: String,
    pub applied_steps: Vec<String>,
    pub git_checkpoint: Option<String>,
    pub message: String,
}

fn read_rula(project_path: &Path) -> Result<Option<(String, toml::Value)>, MigrationError> {
    let rula_file = project_path.join("rula.toml");
    let content = match fs::read_to_string(&rula_file) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(MigrationError::Io { context: format!("Cannot read {}", rula_file.display()), source: e }),
    };
    let value = toml::from_str::<toml::Value>(&content)
        .map_err(|e| MigrationError::InvalidRula { path: rula_file.display().to_string(), reason: e.to_string() })?;
    Ok(Some((content, value)))
}

fn version_of(value: &toml::Value) -> Option<String> {
    value.get("project")?.get("oneship_version")?.as_str().map(str::to_string)
}

/// `[project].oneship_version` of a project; `UNVERSIONED_PLATFORM_VERSION` when the key or the
/// file is absent. An unreadable or invalid `rula.toml` is an error.
pub fn get_project_version(project_path: &Path) -> Result<String, MigrationError> {
    Ok(read_rula(project_path)?
        .and_then(|(_, value)| version_of(&value))
        .unwrap_or_else(|| UNVERSIONED_PLATFORM_VERSION.to_string()))
}

/// Table header name of a TOML line (`[project]`, `[ project ] # c` → `project`); `None` otherwise.
fn table_header(line: &str) -> Option<&str> {
    let code = line.split('#').next().unwrap_or_default().trim();
    let inner = code.strip_prefix('[')?.strip_suffix(']')?;
    (!inner.starts_with('[')).then(|| inner.trim())
}

fn is_version_key(line: &str) -> bool {
    line.split_once('=').is_some_and(|(key, _)| key.trim() == "oneship_version")
}

/// Text of `content` with `[project].oneship_version = "<version>"` set, keeping every other line
/// (comments and layout included). A missing `[project]` table is appended at the end.
fn stamp_version(content: &str, version: &str) -> String {
    let version_line = format!("oneship_version = \"{}\"", version);
    let mut lines: Vec<String> = Vec::with_capacity(content.lines().count() + 2);
    let mut in_project = false;
    let mut written = false;
    for line in content.lines() {
        if let Some(name) = table_header(line) {
            if in_project && !written {
                lines.push(version_line.clone());
                written = true;
            }
            in_project = name == "project";
        } else if in_project && is_version_key(line) {
            if !written {
                lines.push(version_line.clone());
                written = true;
            }
            continue;
        }
        lines.push(line.to_string());
    }
    if in_project && !written {
        lines.push(version_line.clone());
        written = true;
    }
    if !written {
        lines.push(String::new());
        lines.push("[project]".to_string());
        lines.push(version_line);
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// Records `version` as `[project].oneship_version` in `rula.toml`. A project without
/// `rula.toml` (not initialized yet) is left unchanged. The edited file is verified to parse and
/// to read back `version` before it is written.
pub fn set_project_version(project_path: &Path, version: &str) -> Result<(), MigrationError> {
    let Some((content, _)) = read_rula(project_path)? else {
        return Ok(());
    };
    let rula_file = project_path.join("rula.toml");
    let updated = stamp_version(&content, version);
    let parsed = toml::from_str::<toml::Value>(&updated)
        .map_err(|e| MigrationError::InvalidRula { path: rula_file.display().to_string(), reason: format!("version stamp produced invalid TOML: {}", e) })?;
    if version_of(&parsed).as_deref() != Some(version) {
        return Err(MigrationError::InvalidRula { path: rula_file.display().to_string(), reason: "version stamp did not apply".to_string() });
    }
    fs::write(&rula_file, updated).map_err(|e| MigrationError::Io { context: format!("Cannot write {}", rula_file.display()), source: e })
}

/// Brings a project to `CURRENT_PLATFORM_VERSION`: migrates legacy `[plugins.snapshots]` config
/// tables in `rula.toml` into `[plugins.vcs]`, re-syncs platform skills, the AGENTS.md block
/// and runner configs and stamps the version (`skill::inject_platform_skill`), then verifies the
/// recorded version. HEAD is reported as a checkpoint; nothing is committed.
pub fn run_deterministic_migration(project_path: &Path) -> Result<MigrationResult, MigrationError> {
    let p_str = project_path.to_string_lossy().to_string();
    let from_version = get_project_version(project_path)?;
    let target_version = CURRENT_PLATFORM_VERSION.to_string();
    let Some((rula_content, _)) = read_rula(project_path)? else {
        return Err(MigrationError::MissingRula(p_str));
    };

    let git_checkpoint = if project_path.join(".git").exists() {
        crate::kopeng::snapshots::head_commit(project_path)?
    } else {
        None
    };

    let mut applied_steps = Vec::new();
    let rula_file = project_path.join("rula.toml");
    match crate::config::migrate_toml_snapshots_to_vcs(&rula_content) {
        Ok(Some(migrated_rula)) => {
            fs::write(&rula_file, &migrated_rula).map_err(|e| MigrationError::Io {
                context: format!("Cannot write migrated {}", rula_file.display()),
                source: e,
            })?;
            applied_steps.push("migrated [plugins.snapshots] into [plugins.vcs] in rula.toml".to_string());
            crate::logger::log("INFO", "migration", &format!("Migrated [plugins.snapshots] into [plugins.vcs] in {}", rula_file.display()));
        }
        Ok(None) => {}
        Err(reason) => {
            return Err(MigrationError::InvalidRula {
                path: rula_file.display().to_string(),
                reason,
            });
        }
    }

    crate::logger::log("DEBUG", "migration", &format!("Syncing platform assets of '{}' ({} -> {})", p_str, from_version, target_version));
    inject_platform_skill(p_str.clone())?;

    let recorded = get_project_version(project_path)?;
    if recorded != target_version {
        return Err(MigrationError::InvalidRula {
            path: rula_file.display().to_string(),
            reason: format!("version reads '{}' after migration, expected '{}'", recorded, target_version),
        });
    }

    applied_steps.extend([
        "platform skills re-linked".to_string(),
        "AGENTS.md managed block refreshed".to_string(),
        "runner MCP configs and CLAUDE.md link refreshed".to_string(),
        format!("rula.toml oneship_version {} -> {}", from_version, target_version),
    ]);
    crate::logger::log("INFO", "migration", &format!("Migrated project {} from v{} to v{}", p_str, from_version, target_version));
    Ok(MigrationResult {
        from_version,
        to_version: target_version,
        applied_steps,
        git_checkpoint,
        message: format!("Upgraded project to platform version {}", CURRENT_PLATFORM_VERSION),
    })
}

/// Daemon command `migrate_project_deterministic`: migrates a registered project to the current
/// platform version (blocking pool).
pub async fn migrate_project_deterministic(project_path: String) -> Result<MigrationResult, MigrationError> {
    tokio::task::spawn_blocking(move || {
        let project = crate::registry::find_project_by_path(&project_path)?;
        run_deterministic_migration(Path::new(&project.path))
    })
    .await
    .map_err(|e| MigrationError::Task(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_stamps_the_current_version_and_migrates_snapshots_plugin() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("rula.toml"),
            "[project]\nname = \"temp\"\noneship_version = \"0.2.6\"\n\n[plugins.snapshots]\nenabled = true\nauto_snapshot_on_minutes = false\nmax_per_project_count = 30\n",
        )
        .unwrap();

        let m = run_deterministic_migration(tmp.path()).unwrap();

        assert_eq!(m.from_version, "0.2.6");
        assert_eq!(m.to_version, CURRENT_PLATFORM_VERSION);
        assert_eq!(get_project_version(tmp.path()).unwrap(), CURRENT_PLATFORM_VERSION);
        assert!(m.applied_steps.iter().any(|s| s.contains("migrated [plugins.snapshots] into [plugins.vcs]")));
        let rula: toml::Value = toml::from_str(&fs::read_to_string(tmp.path().join("rula.toml")).unwrap()).unwrap();
        assert!(rula["plugins"].get("snapshots").is_none());
        assert_eq!(rula["plugins"]["vcs"]["snapshots_enabled"].as_bool(), Some(true));
        assert_eq!(rula["plugins"]["vcs"]["minutes_checkpoint_mode"].as_str(), Some("off"));
        assert_eq!(rula["plugins"]["vcs"]["max_per_project_count"].as_integer(), Some(30));
    }

    #[test]
    fn version_stamp_keeps_root_keys_and_comments() {
        let content = "name = \"root\"\n\n[project] # main\nid = \"x\"\noneship_version_note = \"keep\"\n\n[scripts]\na = 1\n";
        let stamped = stamp_version(content, "9.9.9");
        let v: toml::Value = toml::from_str(&stamped).unwrap();
        assert_eq!(v["name"].as_str(), Some("root"));
        assert_eq!(v["project"]["oneship_version"].as_str(), Some("9.9.9"));
        assert_eq!(v["project"]["oneship_version_note"].as_str(), Some("keep"));
        assert!(stamped.contains("[project] # main"));

        let no_table = stamp_version("name = \"root\"\n", "9.9.9");
        let v: toml::Value = toml::from_str(&no_table).unwrap();
        assert_eq!(v["name"].as_str(), Some("root"));
        assert_eq!(v["project"]["oneship_version"].as_str(), Some("9.9.9"));
    }

    #[test]
    fn invalid_rula_is_reported() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("rula.toml"), "[project\n").unwrap();
        assert!(matches!(get_project_version(tmp.path()), Err(MigrationError::InvalidRula { .. })));
    }
}
