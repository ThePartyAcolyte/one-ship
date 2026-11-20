use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Platform version stamped into `rula.toml` (`project.oneship_version`). The canonical
/// `oneship-core/SKILL.md` frontmatter must carry the same version
/// (`tests::core_skill_version_matches_platform_version`).
pub const CURRENT_PLATFORM_VERSION: &str = "0.2.7";

/// Compile-time fallback path to the repository's `templates/` tree when running uninstalled
/// from a developer checkout or `cargo test`.
pub(crate) const DEV_TEMPLATES_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../templates");

/// Skills symlinked into every project; they cannot be disabled, removed or overwritten.
pub const PLATFORM_BASE_SKILLS: &[&str] = &[
    "oneship-core",
    "oneship-delegation",
    "oneship-devlog",
    "oneship-snapshots",
    "oneship-techdebt",
    "oneship-artifacts",
];

/// Skills symlinked on demand (initialization, flag resolution, REA) and removable again.
pub(crate) const SPECIALIZED_SKILLS: &[&str] = &[
    "oneship-docstyle-sdd",
    "oneship-docstyle-lean",
    "oneship-docstyle-contract",
    "oneship-docstyles-crafter",
    "oneship-initializer",
    "oneship-docstyles",
    "oneship-archetypes",
    "oneship-profiler-crafter",
    "oneship-rea",
    "oneship-desigma",
    "oneship-pcbcraft",
    "oneship-project-mgmt",
];

/// Errors of skill provisioning, project skill management and prompt template loading.
/// Serialized as its message so Tauri commands return it directly.
#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("Canonical templates path not found: {0}")]
    TemplatesNotFound(String),
    #[error("Unknown specialized skill: {0}")]
    UnknownSkill(String),
    #[error("Unknown prompt template: {0}")]
    UnknownTemplate(String),
    #[error("Invalid skill name '{0}'")]
    InvalidName(String),
    #[error("Platform base skill '{0}' cannot be disabled, removed or overwritten")]
    ProtectedSkill(String),
    #[error("Skill '{0}' not found in project")]
    NotFound(String),
    #[error("Project directory not found: {0}")]
    ProjectNotFound(String),
    #[error("Home directory could not be resolved")]
    HomeUnavailable,
    #[error("{0} is not a JSON object")]
    NotAnObject(String),
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{context}: {source}")]
    Json {
        context: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("{0}")]
    Remote(String),
    #[error(transparent)]
    Manifest(#[from] crate::kopeng::manifest::ManifestError),
    #[error(transparent)]
    Migration(#[from] crate::kopeng::migration::MigrationError),
}

impl crate::api::ApiError for SkillError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::UnknownSkill(_) | Self::InvalidName(_) => StatusCode::BAD_REQUEST,
            Self::NotFound(_) | Self::ProjectNotFound(_) => StatusCode::NOT_FOUND,
            Self::ProtectedSkill(_) | Self::NotAnObject(_) | Self::Json { .. } => StatusCode::CONFLICT,
            Self::Remote(_) => StatusCode::BAD_GATEWAY,
            Self::TemplatesNotFound(_) | Self::UnknownTemplate(_) | Self::HomeUnavailable | Self::Io { .. } => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            Self::Manifest(e) => e.status(),
            Self::Migration(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::TemplatesNotFound(_) => "SkillError::TemplatesNotFound",
            Self::UnknownSkill(_) => "SkillError::UnknownSkill",
            Self::UnknownTemplate(_) => "SkillError::UnknownTemplate",
            Self::InvalidName(_) => "SkillError::InvalidName",
            Self::ProtectedSkill(_) => "SkillError::ProtectedSkill",
            Self::NotFound(_) => "SkillError::NotFound",
            Self::ProjectNotFound(_) => "SkillError::ProjectNotFound",
            Self::HomeUnavailable => "SkillError::HomeUnavailable",
            Self::NotAnObject(_) => "SkillError::NotAnObject",
            Self::Io { .. } => "SkillError::Io",
            Self::Json { .. } => "SkillError::Json",
            Self::Remote(_) => "SkillError::Remote",
            Self::Manifest(e) => e.code(),
            Self::Migration(e) => e.code(),
        }
    }
}

/// Arguments of the skill commands that name one skill of a project.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ProjectSkillArgs {
    pub project_path: String,
    pub skill_name: String,
}

/// Arguments of `toggle_project_skill`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ToggleProjectSkillArgs {
    pub project_path: String,
    pub skill_name: String,
    pub enabled: bool,
}

/// Arguments of `search_marketplace_skills`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SearchMarketplaceSkillsArgs {
    pub query: String,
    pub limit_count: usize,
}

/// Arguments of `install_project_skill`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct InstallProjectSkillArgs {
    pub project_path: String,
    pub skill_name: String,
    pub source_url: String,
}

fn io_err(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> SkillError {
    let context = context.into();
    move |source| SkillError::Io { context, source }
}

fn json_err(context: impl Into<String>) -> impl FnOnce(serde_json::Error) -> SkillError {
    let context = context.into();
    move |source| SkillError::Json { context, source }
}

fn require_project_root(project_path: &str) -> Result<&Path, SkillError> {
    let root = Path::new(project_path);
    if !root.is_dir() {
        return Err(SkillError::ProjectNotFound(project_path.to_string()));
    }
    Ok(root)
}

/// Skill names are single path components (`[A-Za-z0-9._-]`, no leading dot), so they can never
/// address anything outside `.agents/skills` or `.agents/.skills-disabled`.
fn validate_skill_name(name: &str) -> Result<(), SkillError> {
    let is_valid = !name.is_empty()
        && !name.starts_with('.')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if !is_valid {
        return Err(SkillError::InvalidName(name.to_string()));
    }
    Ok(())
}

/// Removes a file or symlink at `path`; absence is not an error.
fn remove_file_if_present(path: &Path) -> Result<(), SkillError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_err(format!("Failed to remove {}", path.display()))(e)),
    }
}

fn resolve_templates_root_with(
    override_dir: Option<&str>,
    exe_dir: Option<&Path>,
    xdg_data_home: Option<&Path>,
    home_dir: Option<&Path>,
) -> Result<PathBuf, SkillError> {
    if let Some(raw) = override_dir.map(str::trim).filter(|s| !s.is_empty()) {
        let p = Path::new(raw);
        let canon = p
            .canonicalize()
            .map_err(|e| SkillError::TemplatesNotFound(format!("ONESHIP_TEMPLATES_DIR='{}' ({})", raw, e)))?;
        if !canon.is_dir() {
            return Err(SkillError::TemplatesNotFound(format!(
                "ONESHIP_TEMPLATES_DIR='{}' is not a directory",
                canon.display()
            )));
        }
        return Ok(canon);
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(exe) = exe_dir {
        if let Some(prefix) = exe.parent() {
            candidates.push(prefix.join("share").join("one-ship").join("templates"));
        }
        candidates.push(exe.join("templates"));
    }
    if let Some(xdg) = xdg_data_home.filter(|p| !p.as_os_str().is_empty()) {
        candidates.push(xdg.join("one-ship").join("templates"));
    } else if let Some(home) = home_dir {
        candidates.push(home.join(".local").join("share").join("one-ship").join("templates"));
    }
    candidates.push(PathBuf::from("/usr/local/share/one-ship/templates"));
    candidates.push(PathBuf::from("/usr/share/one-ship/templates"));
    candidates.push(PathBuf::from(DEV_TEMPLATES_ROOT));

    for candidate in &candidates {
        if candidate.is_dir() {
            if let Ok(canon) = candidate.canonicalize() {
                return Ok(canon);
            }
        }
    }

    let searched = candidates
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(SkillError::TemplatesNotFound(format!("searched: {}", searched)))
}

/// Resolves the relocatable canonical `templates/` root directory (`ONESHIP_TEMPLATES_DIR`,
/// FHS install prefix, XDG data directory, `/usr/share`, or dev checkout).
pub(crate) fn resolve_templates_root() -> Result<PathBuf, SkillError> {
    let override_val = std::env::var("ONESHIP_TEMPLATES_DIR").ok();
    let exe_path = std::env::current_exe().ok();
    let exe_dir = exe_path.as_deref().and_then(Path::parent);
    let xdg_data = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from);
    let home = dirs::home_dir();
    resolve_templates_root_with(
        override_val.as_deref(),
        exe_dir,
        xdg_data.as_deref(),
        home.as_deref(),
    )
}

/// Canonical `templates/<sub>` directory.
pub(crate) fn canonical_templates_dir(sub: &str) -> Result<PathBuf, SkillError> {
    let dir = resolve_templates_root()?.join(sub);
    dir.canonicalize()
        .map_err(|e| SkillError::TemplatesNotFound(format!("{} ({})", dir.display(), e)))
}

/// Value of the `version:` frontmatter line of a SKILL.md, without quotes.
fn frontmatter_version(content: &str) -> Option<&str> {
    content.lines().find_map(|line| {
        line.trim()
            .strip_prefix("version:")
            .map(|v| v.trim().trim_matches('"').trim_matches('\''))
    })
}


#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct SkillStatus {
    pub is_installed: bool,
    pub installed_version: Option<String>,
    pub current_version: String,
    pub is_outdated: bool,
    #[serde(default)]
    pub is_symlinked: bool,
    #[serde(default)]
    pub schema_version: Option<String>,
    #[serde(default)]
    pub needs_migration: bool,
}

/// Replaces whatever exists at `target_link` with a symlink to the canonical `source_dir`.
fn link_platform_directory(source_dir: &Path, target_link: &Path) -> Result<(), SkillError> {
    if !source_dir.is_dir() {
        return Err(SkillError::TemplatesNotFound(source_dir.display().to_string()));
    }

    match fs::symlink_metadata(target_link) {
        // `symlink_metadata` does not follow links: `is_dir` is true only for a real directory.
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(target_link)
            .map_err(io_err(format!("Failed to remove directory {}", target_link.display())))?,
        Ok(_) => remove_file_if_present(target_link)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io_err(format!("Failed to inspect {}", target_link.display()))(e)),
    }

    if let Some(parent) = target_link.parent() {
        fs::create_dir_all(parent).map_err(io_err(format!("Failed to create {}", parent.display())))?;
    }

    std::os::unix::fs::symlink(source_dir, target_link).map_err(io_err(format!(
        "Failed to create symlink {} -> {}",
        target_link.display(),
        source_dir.display()
    )))
}

pub fn get_project_skill_status(project_path: &Path) -> SkillStatus {
    let current_ver = CURRENT_PLATFORM_VERSION.to_string();
    let skills_root = project_path.join(".agents").join("skills");
    let core_skill_dir = skills_root.join("oneship-core");
    let core_skill_file = core_skill_dir.join("SKILL.md");

    let mut all_base_symlinked = true;
    let mut all_base_present = true;
    for skill in PLATFORM_BASE_SKILLS {
        let p = skills_root.join(skill);
        if !p.exists() {
            all_base_present = false;
            all_base_symlinked = false;
        } else if !p.is_symlink() {
            all_base_symlinked = false;
        }
    }

    let mut is_installed = false;
    let mut installed_version = None;

    if core_skill_file.exists() {
        is_installed = true;
        let content = fs::read_to_string(&core_skill_file).unwrap_or_default();
        installed_version = frontmatter_version(&content).map(str::to_string);
    }

    // Check rula.toml for oneship_version
    let rula_file = project_path.join("rula.toml");
    let mut schema_version = None;
    if rula_file.exists() {
        if let Ok(content) = fs::read_to_string(&rula_file) {
            // A document, not a single value: `str::parse::<toml::Value>` rejects every rula.toml.
            if let Ok(value) = toml::from_str::<toml::Value>(&content) {
                if let Some(ver) = value.get("project").and_then(|p| p.get("oneship_version")).and_then(|v| v.as_str()) {
                    schema_version = Some(ver.to_string());
                }
            }
        }
    }

    // Check AGENTS.md governance block
    let agents_md_path = project_path.join("AGENTS.md");
    let governance_outdated = if agents_md_path.exists() {
        let content = fs::read_to_string(&agents_md_path).unwrap_or_default();
        !content.contains("<!-- ONE_SHIP_MANAGED_START -->")
    } else {
        false
    };

    // Check runner configs and symlinks
    let claude_mcp_file = project_path.join(".mcp.json");
    let opencode_json_file = project_path.join("opencode.json");
    let claude_symlink_ok = match fs::symlink_metadata(project_path.join("CLAUDE.md")) {
        Ok(meta) => meta.file_type().is_symlink(),
        Err(_) => false,
    };
    let missing_runner_configs = is_installed && (!claude_mcp_file.exists() || !opencode_json_file.exists() || !claude_symlink_ok);

    let is_symlinked = all_base_present && all_base_symlinked;

    let skill_outdated = match &installed_version {
        Some(ver) => ver != &current_ver || !is_symlinked || missing_runner_configs || governance_outdated,
        None => is_installed,
    };

    let schema_outdated = match &schema_version {
        Some(ver) => ver != &current_ver,
        None => rula_file.exists(),
    };

    let is_outdated = skill_outdated || schema_outdated;
    let needs_migration = schema_outdated;

    crate::logger::log(
        "DEBUG",
        "skill",
        &format!(
            "Evaluated skill status for {}: installed={}, version={:?}, current={}, symlinked={}, outdated={}, needs_migration={}",
            project_path.display(), is_installed, installed_version, current_ver, is_symlinked, is_outdated, needs_migration
        ),
    );

    SkillStatus {
        is_installed,
        installed_version,
        current_version: current_ver,
        is_outdated,
        is_symlinked,
        schema_version,
        needs_migration,
    }
}

/// Reads a JSON config file that must hold an object; a missing file yields `default`.
/// A corrupt or non-object file is an error: it is never overwritten, since it may hold the
/// user's own entries.
fn read_json_object(
    path: &Path,
    default: serde_json::Map<String, serde_json::Value>,
) -> Result<serde_json::Map<String, serde_json::Value>, SkillError> {
    if !path.exists() {
        return Ok(default);
    }
    let raw = fs::read_to_string(path).map_err(io_err(format!("Failed to read {}", path.display())))?;
    match serde_json::from_str(&raw).map_err(json_err(format!("Failed to parse {}", path.display())))? {
        serde_json::Value::Object(map) => Ok(map),
        _ => Err(SkillError::NotAnObject(path.display().to_string())),
    }
}

fn write_json_object(path: &Path, map: serde_json::Map<String, serde_json::Value>) -> Result<(), SkillError> {
    let content = serde_json::to_string_pretty(&serde_json::Value::Object(map))
        .map_err(json_err(format!("Failed to serialize {}", path.display())))?;
    fs::write(path, content).map_err(io_err(format!("Failed to write {}", path.display())))
}

/// Returns the object under `key` in `map`, creating it when absent.
fn child_object<'a>(
    map: &'a mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    path: &Path,
) -> Result<&'a mut serde_json::Map<String, serde_json::Value>, SkillError> {
    map.entry(key)
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| SkillError::NotAnObject(format!("{} key '{}'", path.display(), key)))
}

/// Writes the runner MCP configs (`opencode.json`, `.mcp.json`), the `CLAUDE.md -> AGENTS.md`
/// symlink and the global `agy` MCP instructions for a project (AGENTS.md §7 pre-boot).
pub fn inject_project_mcp_plugin(project_path: &Path) -> Result<(), SkillError> {
    // Zero Backward Compatibility: clean up obsolete .agents/plugins/one-ship directory & mcp_config.json
    let legacy_plugin_dir = project_path.join(".agents").join("plugins").join("one-ship");
    if legacy_plugin_dir.exists() {
        fs::remove_dir_all(&legacy_plugin_dir)
            .map_err(io_err(format!("Failed to remove {}", legacy_plugin_dir.display())))?;
    }
    remove_file_if_present(&project_path.join(".agents").join("mcp_config.json"))?;

    let mcp_port = crate::config::mcp_daemon_port();
    let mcp_url = format!("http://localhost:{}/mcp", mcp_port);
    let rea_enabled = crate::kopeng::rea::is_project_rea_enabled(project_path);
    let rea_gateway_url = crate::kopeng::rea::build_project_rea_gateway_url(
        mcp_port,
        &project_path.to_string_lossy(),
    );
    let desigma_enabled = crate::kopeng::desigma::is_project_desigma_enabled(project_path);
    let desigma_gateway_url = crate::kopeng::desigma::build_project_desigma_gateway_url(
        mcp_port,
        &project_path.to_string_lossy(),
    );
    let pcbcraft_enabled = crate::kopeng::pcbcraft::is_project_pcbcraft_enabled(project_path);
    let pcbcraft_gateway_url = crate::kopeng::pcbcraft::build_project_pcbcraft_gateway_url(
        mcp_port,
        &project_path.to_string_lossy(),
    );

    // 1. Inject opencode.json for OpenCode runner discovery
    let opencode_json_path = project_path.join("opencode.json");
    let mut opencode_default = serde_json::Map::new();
    opencode_default.insert("$schema".to_string(), serde_json::json!("https://opencode.ai/config.json"));
    let mut opencode_config = read_json_object(&opencode_json_path, opencode_default)?;
    let mcp_obj = child_object(&mut opencode_config, "mcp", &opencode_json_path)?;
    mcp_obj.insert(
        "one-ship".to_string(),
        serde_json::json!({ "type": "remote", "url": mcp_url, "enabled": true }),
    );
    if rea_enabled {
        mcp_obj.insert(
            "rea".to_string(),
            serde_json::json!({ "type": "remote", "url": rea_gateway_url, "enabled": true }),
        );
    } else {
        mcp_obj.remove("rea");
    }
    if desigma_enabled {
        mcp_obj.insert(
            "desigma".to_string(),
            serde_json::json!({ "type": "remote", "url": desigma_gateway_url, "enabled": true }),
        );
    } else {
        mcp_obj.remove("desigma");
    }
    if pcbcraft_enabled {
        mcp_obj.insert(
            "pcbcraft".to_string(),
            serde_json::json!({ "type": "remote", "url": pcbcraft_gateway_url, "enabled": true }),
        );
    } else {
        mcp_obj.remove("pcbcraft");
    }
    write_json_object(&opencode_json_path, opencode_config)?;

    // 2. Inject .mcp.json for Claude Code runner discovery
    let claude_mcp_path = project_path.join(".mcp.json");
    let mut claude_mcp_config = read_json_object(&claude_mcp_path, serde_json::Map::new())?;
    let servers = child_object(&mut claude_mcp_config, "mcpServers", &claude_mcp_path)?;
    servers.insert("one-ship".to_string(), serde_json::json!({ "type": "http", "url": mcp_url }));
    if rea_enabled {
        servers.insert("rea".to_string(), serde_json::json!({ "type": "http", "url": rea_gateway_url }));
    } else {
        servers.remove("rea");
    }
    if desigma_enabled {
        servers.insert("desigma".to_string(), serde_json::json!({ "type": "http", "url": desigma_gateway_url }));
    } else {
        servers.remove("desigma");
    }
    if pcbcraft_enabled {
        servers.insert("pcbcraft".to_string(), serde_json::json!({ "type": "http", "url": pcbcraft_gateway_url }));
    } else {
        servers.remove("pcbcraft");
    }
    write_json_object(&claude_mcp_path, claude_mcp_config)?;

    // 3. Ensure CLAUDE.md is a relative symlink pointing to AGENTS.md for native Claude Code governance
    let claude_md_path = project_path.join("CLAUDE.md");
    if project_path.join("AGENTS.md").exists() {
        let is_linked = fs::read_link(&claude_md_path).is_ok_and(|target| target == Path::new("AGENTS.md"));
        if !is_linked {
            // Keep the project's own CLAUDE.md so de-integration can restore it (manifest::purge).
            let backup = crate::kopeng::manifest::backups_dir(project_path).join("CLAUDE.md.orig");
            if claude_md_path.exists() && !backup.exists() {
                if let Some(dir) = backup.parent() {
                    fs::create_dir_all(dir).map_err(io_err(format!("Failed to create {}", dir.display())))?;
                }
                fs::copy(&claude_md_path, &backup)
                    .map_err(io_err(format!("Failed to back up {}", claude_md_path.display())))?;
            }
            remove_file_if_present(&claude_md_path)?;
            std::os::unix::fs::symlink("AGENTS.md", &claude_md_path)
                .map_err(io_err(format!("Failed to create symlink {} -> AGENTS.md", claude_md_path.display())))?;
        }
    }

    // 4. Provision ~/.gemini/antigravity-cli/mcp/one-ship/instructions.md with neutral platform descriptor
    let home = dirs::home_dir().ok_or(SkillError::HomeUnavailable)?;
    let mcp_agy_dir = home.join(".gemini").join("antigravity-cli").join("mcp").join("one-ship");
    fs::create_dir_all(&mcp_agy_dir).map_err(io_err(format!("Failed to create {}", mcp_agy_dir.display())))?;
    let instructions_content = "# ONE-SHIP Platform Tools\n\n\
Tools provided by the local ONE-SHIP orchestration daemon. Behavioral guidelines and workflow protocols are governed per-project in AGENTS.md.\n";
    fs::write(mcp_agy_dir.join("instructions.md"), instructions_content)
        .map_err(io_err("Failed to write agy MCP instructions"))?;

    crate::logger::log(
        "INFO",
        "skill",
        &format!(
            "Injected runner MCP configs (.mcp.json, opencode.json) and CLAUDE.md symlink into {}",
            project_path.display()
        ),
    );

    Ok(())
}

/// Full platform sync of a project: symlinks the base skills (and re-links specialized skills
/// already present), refreshes the AGENTS.md managed block, writes the runner MCP configs and
/// stamps `rula.toml` with `CURRENT_PLATFORM_VERSION`. Any step failing fails the whole sync.
/// Daemon command `inject_platform_skill`.
pub fn inject_platform_skill(project_path: String) -> Result<String, SkillError> {
    crate::logger::log(
        "DEBUG",
        "skill",
        &format!("Synchronizing Base Platform Skills for {}", project_path),
    );
    let p = require_project_root(&project_path)?;
    let skills_root = p.join(".agents").join("skills");
    fs::create_dir_all(&skills_root).map_err(io_err("Failed to create .agents/skills directory"))?;

    let templates_dir = canonical_templates_dir("skills")?;
    for skill_name in PLATFORM_BASE_SKILLS {
        link_platform_directory(&templates_dir.join(skill_name), &skills_root.join(skill_name))?;
    }

    // Re-link specialized skills already present in the project (including dangling links).
    for spec_name in SPECIALIZED_SKILLS {
        let target = skills_root.join(spec_name);
        if fs::symlink_metadata(&target).is_ok() {
            link_platform_directory(&templates_dir.join(spec_name), &target)?;
        }
    }

    // Refresh AGENTS.md with modern ONE-SHIP governance (stripping any legacy block)
    crate::kopeng::manifest::prepare_project_agents_md(p)?;

    // Inject project-scoped MCP runner configs and CLAUDE.md symlink
    inject_project_mcp_plugin(p)?;

    let current_ver = CURRENT_PLATFORM_VERSION;

    // If rula.toml exists, synchronize oneship_version with platform version
    if p.join("rula.toml").exists() {
        crate::kopeng::migration::set_project_version(p, current_ver)?;
    }

    crate::logger::log(
        "INFO",
        "skill",
        &format!(
            "Injected/symlinked Base Platform Skills (v{}) into {}",
            current_ver, project_path
        ),
    );

    Ok(format!(
        "Base Platform Skills v{} linked into .agents/skills/oneship-*/",
        current_ver
    ))
}

fn require_specialized_skill(skill_name: &str, project_path: &str) -> Result<(), SkillError> {
    if SPECIALIZED_SKILLS.contains(&skill_name) {
        return Ok(());
    }
    crate::logger::log(
        "WARN",
        "skill",
        &format!("Rejected unknown specialized skill '{}' for {}", skill_name, project_path),
    );
    Err(SkillError::UnknownSkill(skill_name.to_string()))
}

/// Symlinks one specialized skill from the canonical templates into the project.
pub fn inject_specialized_skill(project_path: String, skill_name: String) -> Result<String, SkillError> {
    let p = require_project_root(&project_path)?;
    require_specialized_skill(&skill_name, &project_path)?;

    let src = canonical_templates_dir("skills")?.join(&skill_name);
    link_platform_directory(&src, &p.join(".agents").join("skills").join(&skill_name))?;

    crate::logger::log(
        "INFO",
        "skill",
        &format!("Injected specialized skill '{}' into {}", skill_name, project_path),
    );

    Ok(format!("Specialized skill '{}' injected into {}", skill_name, project_path))
}

/// Loads `templates/prompts/<template_name>.md` and substitutes each `{{KEY}}` with its value.
pub async fn load_prompt_template(template_name: &str, vars: &[(&str, &str)]) -> Result<String, SkillError> {
    if template_name.is_empty() || !template_name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(SkillError::UnknownTemplate(template_name.to_string()));
    }
    let prompts_dir = canonical_templates_dir("prompts")?;
    let path = prompts_dir.join(format!("{}.md", template_name));
    let raw_content = match tokio::fs::read_to_string(&path).await {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(SkillError::UnknownTemplate(format!("{} ({})", template_name, path.display())));
        }
        Err(e) => return Err(io_err(format!("Failed to read {}", path.display()))(e)),
    };
    crate::logger::log(
        "DEBUG",
        "skill",
        &format!("Loaded prompt template '{}' ({} bytes)", template_name, raw_content.len()),
    );

    let mut result = raw_content;
    for (k, v) in vars {
        let needle = format!("{{{{{}}}}}", k);
        result = result.replace(&needle, v);
    }
    Ok(result)
}

/// Removes an on-demand specialized skill link from the project; absence is not an error.
pub fn cleanup_ephemeral_skill(project_path: String, skill_name: String) -> Result<String, SkillError> {
    let p = require_project_root(&project_path)?;
    require_specialized_skill(&skill_name, &project_path)?;
    let skill_dir = p.join(".agents").join("skills").join(&skill_name);
    match fs::symlink_metadata(&skill_dir) {
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(&skill_dir)
            .map_err(io_err(format!("Failed to remove {}", skill_dir.display())))?,
        Ok(_) => remove_file_if_present(&skill_dir)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(format!("Ephemeral skill '{}' was not present", skill_name));
        }
        Err(e) => return Err(io_err(format!("Failed to inspect {}", skill_dir.display()))(e)),
    }
    crate::logger::log("INFO", "skill", &format!("Cleaned up ephemeral skill '{}' in {}", skill_name, project_path));
    Ok(format!("Cleaned up ephemeral skill '{}'", skill_name))
}

#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct ProjectSkillRecord {
    pub name: String,
    pub description: String,
    pub is_enabled: bool,
    pub is_platform_base: bool,
    pub token_estimate_count: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct MarketplaceSkillItem {
    pub id: String,
    pub name: String,
    pub author: String,
    pub description: String,
    #[ts(type = "number")]
    pub stars_count: u64,
    pub github_url: String,
    pub skill_url: Option<String>,
}

/// Returns `(name, description)` from a SKILL.md frontmatter; `dir_name` is the name when the
/// frontmatter declares none.
fn parse_skill_yaml_frontmatter(file_path: &Path, dir_name: &str) -> Result<(String, String), SkillError> {
    let content = fs::read_to_string(file_path).map_err(io_err(format!("Failed to read {}", file_path.display())))?;

    let mut in_frontmatter = false;
    let mut name = dir_name.to_string();
    let mut description = String::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            if !in_frontmatter {
                in_frontmatter = true;
                continue;
            } else {
                break;
            }
        }
        if in_frontmatter {
            if trimmed.starts_with("name:") {
                name = trimmed
                    .trim_start_matches("name:")
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string();
            } else if trimmed.starts_with("description:") {
                description = trimmed
                    .trim_start_matches("description:")
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string();
            }
        }
    }

    Ok((name, description))
}

/// Appends one record per `<dir>/<skill>/SKILL.md`; a missing `dir` contributes nothing.
fn scan_skill_dir(dir: &Path, is_enabled: bool, records: &mut Vec<ProjectSkillRecord>) -> Result<(), SkillError> {
    if !dir.exists() {
        return Ok(());
    }
    let entries = fs::read_dir(dir).map_err(io_err(format!("Failed to list {}", dir.display())))?;
    for entry in entries {
        let entry = entry.map_err(io_err(format!("Failed to list {}", dir.display())))?;
        let path = entry.path();
        let dir_name = entry.file_name().to_string_lossy().to_string();
        let skill_file = path.join("SKILL.md");
        if dir_name.starts_with('.') || !path.is_dir() || !skill_file.exists() {
            continue;
        }
        let (name, desc) = parse_skill_yaml_frontmatter(&skill_file, &dir_name)?;
        let is_base = is_enabled
            && (PLATFORM_BASE_SKILLS.contains(&name.as_str()) || PLATFORM_BASE_SKILLS.contains(&dir_name.as_str()));
        let token_estimate = (desc.len() + name.len()) / 4;
        records.push(ProjectSkillRecord {
            name: if name.is_empty() { dir_name } else { name },
            description: desc,
            is_enabled,
            is_platform_base: is_base,
            token_estimate_count: token_estimate,
        });
    }
    Ok(())
}

pub fn list_project_skills(project_path: &Path) -> Result<Vec<ProjectSkillRecord>, SkillError> {
    let mut records = Vec::new();
    let agents_dir = project_path.join(".agents");
    scan_skill_dir(&agents_dir.join("skills"), true, &mut records)?;
    scan_skill_dir(&agents_dir.join(".skills-disabled"), false, &mut records)?;

    // Sort: Platform base skills first, then alphabetical
    records.sort_by(|a, b| {
        match (a.is_platform_base, b.is_platform_base) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        }
    });

    Ok(records)
}

pub fn toggle_project_skill(project_path: &Path, skill_name: &str, enabled: bool) -> Result<bool, SkillError> {
    validate_skill_name(skill_name)?;
    if PLATFORM_BASE_SKILLS.contains(&skill_name) && !enabled {
        return Err(SkillError::ProtectedSkill(skill_name.to_string()));
    }

    let agents_dir = project_path.join(".agents");
    let active_dir = agents_dir.join("skills").join(skill_name);
    let disabled_dir = agents_dir.join(".skills-disabled").join(skill_name);

    let (from, to, verb) = if enabled {
        (&disabled_dir, &active_dir, "Enabled")
    } else {
        (&active_dir, &disabled_dir, "Disabled")
    };
    if !from.exists() {
        return Ok(enabled);
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(io_err(format!("Failed to create {}", parent.display())))?;
    }
    fs::rename(from, to).map_err(io_err(format!("Failed to move skill '{}'", skill_name)))?;
    crate::logger::log("INFO", "skill", &format!("{} skill '{}' in {}", verb, skill_name, project_path.display()));

    Ok(enabled)
}

pub fn remove_project_skill(project_path: &Path, skill_name: &str) -> Result<bool, SkillError> {
    validate_skill_name(skill_name)?;
    if PLATFORM_BASE_SKILLS.contains(&skill_name) {
        return Err(SkillError::ProtectedSkill(skill_name.to_string()));
    }

    let agents_dir = project_path.join(".agents");
    let active_dir = agents_dir.join("skills").join(skill_name);
    let disabled_dir = agents_dir.join(".skills-disabled").join(skill_name);

    let mut removed = false;
    for dir in [&active_dir, &disabled_dir] {
        if dir.exists() {
            fs::remove_dir_all(dir).map_err(io_err(format!("Failed to remove {}", dir.display())))?;
            removed = true;
        }
    }

    if removed {
        crate::logger::log("INFO", "skill", &format!("Removed skill '{}' from {}", skill_name, project_path.display()));
    }

    Ok(removed)
}

pub fn get_skill_content(project_path: &Path, skill_name: &str) -> Result<String, SkillError> {
    validate_skill_name(skill_name)?;
    let agents_dir = project_path.join(".agents");
    let active_file = agents_dir.join("skills").join(skill_name).join("SKILL.md");
    let disabled_file = agents_dir.join(".skills-disabled").join(skill_name).join("SKILL.md");

    let file = if active_file.exists() {
        active_file
    } else if disabled_file.exists() {
        disabled_file
    } else {
        return Err(SkillError::NotFound(skill_name.to_string()));
    };
    fs::read_to_string(&file).map_err(io_err(format!("Failed to read {}", file.display())))
}

const SKILLSMP_SEARCH_URL: &str = "https://skillsmp.com/api/v1/skills/search";

pub async fn search_skillsmp_registry(query: &str, limit_opt: Option<usize>) -> Result<Vec<MarketplaceSkillItem>, SkillError> {
    let clean_query = query.trim();
    if clean_query.is_empty() {
        return Ok(Vec::new());
    }

    // `-G --data-urlencode` builds `?q=<encoded>`, so the query cannot inject URL parameters.
    let output = tokio::process::Command::new("curl")
        .args(["-s", "-f", "-m", "10", "-G", "--data-urlencode"])
        .arg(format!("q={}", clean_query))
        .arg(SKILLSMP_SEARCH_URL)
        .output()
        .await
        .map_err(io_err("Failed to run curl for SkillsMP search"))?;

    if !output.status.success() {
        crate::logger::log(
            "WARN",
            "skill",
            &format!("SkillsMP search failed with exit code {:?}", output.status.code()),
        );
        return Err(SkillError::Remote(format!("SkillsMP request failed with curl exit code {:?}", output.status.code())));
    }

    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(json_err("Failed to parse SkillsMP response"))?;

    let max_count = limit_opt.unwrap_or(10);
    let mut results = Vec::new();

    if let Some(skills_arr) = parsed.get("data").and_then(|d| d.get("skills")).and_then(|s| s.as_array()) {
        for item in skills_arr.iter().take(max_count) {
            let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let author = item.get("author").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let description = item.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let github_url = item.get("githubUrl").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let skill_url = item.get("skillUrl").and_then(|v| v.as_str()).map(|s| s.to_string());
            let stars_count = item.get("stars").and_then(|v| v.as_u64()).unwrap_or(0);

            if !name.is_empty() {
                results.push(MarketplaceSkillItem {
                    id,
                    name,
                    author,
                    description,
                    stars_count,
                    github_url,
                    skill_url,
                });
            }
        }
    }

    crate::logger::log("DEBUG", "skill", &format!("SkillsMP search for '{}' returned {} items", clean_query, results.len()));
    Ok(results)
}

pub fn convert_github_url_to_raw(source_url: &str) -> String {
    let trimmed = source_url.trim();
    if trimmed.contains("raw.githubusercontent.com") {
        return trimmed.to_string();
    }

    if let Some(stripped) = trimmed.strip_prefix("https://github.com/") {
        let parts: Vec<&str> = stripped.split('/').collect();
        if parts.len() >= 4 && parts[2] == "tree" {
            let owner = parts[0];
            let repo = parts[1];
            let branch = parts[3];
            let rest = parts[4..].join("/");
            if rest.ends_with(".md") {
                return format!("https://raw.githubusercontent.com/{}/{}/{}/{}", owner, repo, branch, rest);
            } else {
                return format!("https://raw.githubusercontent.com/{}/{}/{}/{}/SKILL.md", owner, repo, branch, rest);
            }
        } else if parts.len() == 2 {
            return format!("https://raw.githubusercontent.com/{}/{}/main/SKILL.md", parts[0], parts[1]);
        }
    }

    trimmed.to_string()
}

pub async fn install_project_skill(project_path: &Path, skill_name: &str, source_url: &str) -> Result<String, SkillError> {
    let mut clean_name = skill_name
        .trim()
        .replace(' ', "-")
        .to_lowercase();

    if clean_name.is_empty() {
        let stripped = source_url.trim()
            .trim_start_matches("https://")
            .trim_start_matches("http://");
        let segments: Vec<&str> = stripped.split('/').collect();
        if let Some(first_seg) = segments.first() {
            let host_parts: Vec<&str> = first_seg.split('.').collect();
            if host_parts.len() >= 2 {
                let candidate = if host_parts[0] == "docs" && host_parts.len() >= 3 {
                    host_parts[1]
                } else {
                    host_parts[0]
                };
                clean_name = candidate.to_lowercase();
            } else {
                clean_name = first_seg.to_lowercase();
            }
        }
        if clean_name.is_empty() {
            clean_name = "custom-skill".to_string();
        }
    }

    clean_name = clean_name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
    if clean_name.is_empty() {
        clean_name = "custom-skill".to_string();
    }

    validate_skill_name(&clean_name)?;
    if PLATFORM_BASE_SKILLS.contains(&clean_name.as_str()) {
        return Err(SkillError::ProtectedSkill(clean_name));
    }

    let raw_url = convert_github_url_to_raw(source_url);
    crate::logger::log(
        "INFO",
        "skill",
        &format!("Downloading skill '{}' from '{}' (raw: '{}')", clean_name, source_url, raw_url),
    );

    let output = tokio::process::Command::new("curl")
        .args(["-s", "-L", "-f", "-m", "20", &raw_url])
        .output()
        .await
        .map_err(io_err("Failed to run curl for skill download"))?;

    if !output.status.success() {
        crate::logger::log(
            "ERROR",
            "skill",
            &format!("Skill download failed with exit code {:?} for url '{}'", output.status.code(), raw_url),
        );
        return Err(SkillError::Remote(format!("Skill download failed with curl exit code {:?}", output.status.code())));
    }

    let content = String::from_utf8_lossy(&output.stdout).to_string();
    let trimmed_content = content.trim();
    if trimmed_content.is_empty() {
        return Err(SkillError::Remote(format!("Downloaded skill file is empty: {}", raw_url)));
    }

    let target_dir = project_path.join(".agents").join("skills").join(&clean_name);
    tokio::fs::create_dir_all(&target_dir).await.map_err(io_err("Failed to create skill directory"))?;

    let has_frontmatter = trimmed_content.starts_with("---") && trimmed_content[3..].contains("---");

    if has_frontmatter {
        tokio::fs::write(target_dir.join("SKILL.md"), &content).await.map_err(io_err("Failed to write SKILL.md"))?;
    } else {
        if content.len() > 4000 {
            let ref_dir = target_dir.join("references");
            tokio::fs::create_dir_all(&ref_dir).await.map_err(io_err("Failed to create references directory"))?;
            tokio::fs::write(ref_dir.join("documentation.txt"), &content).await.map_err(io_err("Failed to write references/documentation.txt"))?;

            let skill_md_content = format!(
                "---\nname: {}\ndescription: >-\n  Technical reference, API documentation, and usage guidelines for {}.\n---\n\n# {} Documentation & Guidelines\n\nThis skill provides technical documentation and LLM reference for `{}` ingested from `{}`.\n\n## References\n- Complete technical documentation is located in [references/documentation.txt](./references/documentation.txt). Read on-demand when implementing or debugging features.\n",
                clean_name, clean_name, clean_name, clean_name, source_url
            );
            tokio::fs::write(target_dir.join("SKILL.md"), skill_md_content).await.map_err(io_err("Failed to write SKILL.md"))?;
        } else {
            let skill_md_content = format!(
                "---\nname: {}\ndescription: >-\n  Technical documentation and usage guidelines for {}.\n---\n\n{}\n",
                clean_name, clean_name, content
            );
            tokio::fs::write(target_dir.join("SKILL.md"), skill_md_content).await.map_err(io_err("Failed to write SKILL.md"))?;
        }
    }

    // A disabled copy with the same name would shadow the fresh install on the next enable.
    let disabled_dir = project_path.join(".agents").join(".skills-disabled").join(&clean_name);
    let has_disabled_copy = tokio::fs::try_exists(&disabled_dir)
        .await
        .map_err(io_err(format!("Failed to inspect {}", disabled_dir.display())))?;
    if has_disabled_copy {
        tokio::fs::remove_dir_all(&disabled_dir)
            .await
            .map_err(io_err(format!("Failed to remove disabled copy {}", disabled_dir.display())))?;
    }

    crate::logger::log(
        "INFO",
        "skill",
        &format!("Successfully installed skill '{}' into {}", clean_name, target_dir.display()),
    );

    Ok(format!("Successfully installed skill '{}'", clean_name))
}

/// Daemon command `get_project_skills`.
pub fn get_project_skills(project_path: String) -> Result<Vec<ProjectSkillRecord>, SkillError> {
    list_project_skills(require_project_root(&project_path)?)
}

/// Daemon command `toggle_project_skill`.
pub fn toggle_project_skill_command(args: ToggleProjectSkillArgs) -> Result<bool, SkillError> {
    toggle_project_skill(require_project_root(&args.project_path)?, &args.skill_name, args.enabled)
}

/// Daemon command `remove_project_skill`.
pub fn remove_project_skill_command(args: ProjectSkillArgs) -> Result<bool, SkillError> {
    remove_project_skill(require_project_root(&args.project_path)?, &args.skill_name)
}

/// Daemon command `get_project_skill_content`.
pub fn get_project_skill_content(args: ProjectSkillArgs) -> Result<String, SkillError> {
    get_skill_content(require_project_root(&args.project_path)?, &args.skill_name)
}

/// Daemon command `search_marketplace_skills`.
pub async fn search_marketplace_skills(args: SearchMarketplaceSkillsArgs) -> Result<Vec<MarketplaceSkillItem>, SkillError> {
    search_skillsmp_registry(&args.query, Some(args.limit_count)).await
}

/// Daemon command `install_project_skill`.
pub async fn install_project_skill_command(args: InstallProjectSkillArgs) -> Result<String, SkillError> {
    let root = require_project_root(&args.project_path)?;
    install_project_skill(root, &args.skill_name, &args.source_url).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_skill_version_matches_platform_version() {
        let core = canonical_templates_dir("skills").unwrap().join("oneship-core").join("SKILL.md");
        let content = fs::read_to_string(core).unwrap();
        assert_eq!(frontmatter_version(&content), Some(CURRENT_PLATFORM_VERSION));
    }

    #[test]
    fn every_declared_skill_has_a_canonical_template() {
        let skills_dir = canonical_templates_dir("skills").unwrap();
        for name in PLATFORM_BASE_SKILLS.iter().chain(SPECIALIZED_SKILLS) {
            assert!(skills_dir.join(name).join("SKILL.md").is_file(), "missing template for {}", name);
        }
    }

    #[test]
    fn skill_names_cannot_escape_the_skills_directory() {
        for bad in ["", "..", "../src", "a/b", ".hidden", "x\\y"] {
            assert!(validate_skill_name(bad).is_err(), "accepted '{}'", bad);
        }
        assert!(validate_skill_name("oneship-core").is_ok());
    }

    #[test]
    fn explicit_templates_override_fails_loudly_when_missing() {
        let err = resolve_templates_root_with(
            Some("/nonexistent/oneship/templates_xyz"),
            None,
            None,
            None,
        );
        assert!(matches!(err, Err(SkillError::TemplatesNotFound(_))));
    }
}

