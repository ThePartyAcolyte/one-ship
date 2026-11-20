//! Profiler engines and the run history (Spec 25).
//!
//! Profiling is continuous and bound to every run of a service or task: the daemon samples each
//! run's process tree into DuckDB (`telemetry.rs`), and a service that declares an engine in
//! `rula.toml` (`profiler = "<id>"`) is launched wrapped by that engine, whose output file is linked
//! to the run. A run is a row of `supervised_processes`; there is no separate profiling session.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Placeholder in `command_template` replaced by the run's command line.
pub const TOKEN_ORIGINAL_COMMAND: &str = "{original_command}";
/// Placeholder in `command_template` replaced by the engine output file of the run.
pub const TOKEN_OUTPUT_FILE: &str = "{output_file}";
/// Placeholder in `command_template` replaced by `execution.defaults.sampling_rate_hz`.
pub const TOKEN_SAMPLING_RATE_HZ: &str = "{sampling_rate_hz}";
/// Placeholder in `command_template` replaced by `execution.defaults.format`.
pub const TOKEN_FORMAT: &str = "{format}";

/// Project-relative directory holding the engine output files, named `<run_id>.<visual_extension>`.
pub const PROFILING_OUTPUT_DIR: &str = "scratchpad/profiling";

/// `[plugin]` section of a profiler manifest.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ProfilerPluginSection {
    pub id: String,
    pub name: String,
    pub version: String,
    /// Always `"profiler"`; checked by `validate_manifest`.
    #[serde(rename = "type")]
    pub plugin_type: String,
    pub author: Option<String>,
    pub description: String,
}

/// `[detection]` section: what tells the initializer agent that a project fits this engine.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct ProfilerDetection {
    #[serde(default)]
    pub file_triggers: Vec<String>,
    #[serde(default)]
    pub binary_triggers: Vec<String>,
}

/// `[requirements]` section; `check_command` exiting 0 means the engine is installed.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ProfilerRequirements {
    pub check_command: String,
    pub install_hint: Option<String>,
    pub install: Option<ProfilerInstallSection>,
}

/// `[requirements.install]`: install recipes per package manager, shown to the user.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct ProfilerInstallSection {
    pub arch: Option<String>,
    pub debian: Option<String>,
    pub pip: Option<String>,
    pub cargo: Option<String>,
    pub npm: Option<String>,
    pub script: Option<String>,
    #[serde(default)]
    pub requires_sudo: bool,
}

/// `[execution]` section.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ProfilerExecution {
    pub defaults: ProfilerExecutionDefaults,
    pub modes: ProfilerExecutionModes,
}

/// `[execution.defaults]`: values substituted into the command template.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ProfilerExecutionDefaults {
    pub sampling_rate_hz: u32,
    pub format: String,
}

/// `[execution.modes]`. The wrapper is the only mode: the engine starts and stops with the run.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ProfilerExecutionModes {
    pub wrapper: ProfilerWrapperMode,
}

/// `[execution.modes.wrapper]`: the command line that launches the run under the engine.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ProfilerWrapperMode {
    pub command_template: String,
}

/// Viewer able to open an engine output file.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProfilerViewerType {
    Flamegraph,
    Speedscope,
    FirefoxProfiler,
    Text,
}

/// `[artifacts]` section: the engine output file.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ProfilerArtifacts {
    pub visual_extension: String,
    pub viewer_type: ProfilerViewerType,
}

/// A profiler engine manifest (`<plugins dir>/<dir>/manifest.toml`, Spec 25 §4.2). Parsed
/// strictly: an unknown key or a missing section is a fault, never ignored.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ProfilerManifest {
    pub plugin: ProfilerPluginSection,
    #[serde(default)]
    pub detection: ProfilerDetection,
    pub requirements: ProfilerRequirements,
    pub execution: ProfilerExecution,
    pub artifacts: ProfilerArtifacts,
}

impl ProfilerManifest {
    /// The wrapped command line of a run: `original_command` launched under this engine, writing
    /// its output to `output_file`. Both values must already be shell-quoted.
    pub fn wrap_command(&self, original_command: &str, output_file: &str) -> String {
        self.execution.modes.wrapper.command_template
            .replace(TOKEN_ORIGINAL_COMMAND, original_command)
            .replace(TOKEN_OUTPUT_FILE, output_file)
            .replace(TOKEN_SAMPLING_RATE_HZ, &self.execution.defaults.sampling_rate_hz.to_string())
            .replace(TOKEN_FORMAT, &self.execution.defaults.format)
    }

    /// Project-relative path of the engine output file of `run_id`.
    pub fn output_rel_path(&self, run_id: &str) -> String {
        format!("{}/{}.{}", PROFILING_OUTPUT_DIR, run_id, self.artifacts.visual_extension)
    }
}

/// Where a manifest was found. A project manifest shadows a global one with the same id, which
/// shadows a built-in one.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProfilerScope {
    Project,
    Global,
    BuiltIn,
}

/// Why a manifest file was rejected.
#[derive(Debug, thiserror::Error)]
pub enum ManifestFault {
    #[error("cannot read the file: {0}")]
    Read(std::io::Error),
    #[error("invalid manifest: {0}")]
    Parse(toml::de::Error),
    #[error("plugin.type is '{0}', expected 'profiler'")]
    WrongType(String),
    #[error("execution.modes.wrapper.command_template lacks the {0} token")]
    MissingToken(&'static str),
}

/// A manifest file that could not be used, listed next to the engines so it is visible.
#[derive(Serialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct ProfilerManifestFault {
    pub manifest_path: String,
    pub error: String,
}

/// A usable manifest and where it was found (`manifest_path` is `None` for a built-in).
#[derive(Clone, Debug)]
pub struct DiscoveredProfiler {
    pub manifest: ProfilerManifest,
    pub scope: ProfilerScope,
    pub manifest_path: Option<PathBuf>,
}

/// Result of scanning the plugin directories of a project.
#[derive(Debug, Default)]
pub struct ProfilerDiscovery {
    pub profilers: Vec<DiscoveredProfiler>,
    pub faults: Vec<ProfilerManifestFault>,
}

/// Failure of a profiler or run history operation.
#[derive(Debug, thiserror::Error)]
pub enum ProfilerError {
    #[error("Configuration directory is unavailable")]
    ConfigDirUnavailable,
    #[error("Cannot read the profiler directory {path}: {source}")]
    ReadDir { path: String, source: std::io::Error },
    #[error("Invalid configuration in {path}: {reason}")]
    InvalidConfig { path: String, reason: String },
    #[error("{context}: {source}")]
    Io { context: String, source: std::io::Error },
    #[error("Run '{0}' does not exist")]
    RunNotFound(String),
    #[error("Run '{0}' has no profile samples")]
    NoSamples(String),
    #[error(transparent)]
    Registry(#[from] crate::registry::RegistryError),
    #[error(transparent)]
    Database(#[from] crate::db::DatabaseError),
    #[error(transparent)]
    Telemetry(#[from] crate::duck::TelemetryError),
}

impl crate::api::ApiError for ProfilerError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::ConfigDirUnavailable | Self::ReadDir { .. } | Self::Io { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            Self::InvalidConfig { .. } => StatusCode::BAD_REQUEST,
            Self::RunNotFound(_) => StatusCode::NOT_FOUND,
            Self::NoSamples(_) => StatusCode::CONFLICT,
            Self::Registry(e) => e.status(),
            Self::Database(e) => e.status(),
            Self::Telemetry(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::ConfigDirUnavailable => "ProfilerError::ConfigDirUnavailable",
            Self::ReadDir { .. } => "ProfilerError::ReadDir",
            Self::InvalidConfig { .. } => "ProfilerError::InvalidConfig",
            Self::Io { .. } => "ProfilerError::Io",
            Self::RunNotFound(_) => "ProfilerError::RunNotFound",
            Self::NoSamples(_) => "ProfilerError::NoSamples",
            Self::Registry(e) => e.code(),
            Self::Database(e) => e.code(),
            Self::Telemetry(e) => e.code(),
        }
    }
}

/// `~/.config/one-ship/plugins`.
pub fn get_global_plugins_dir() -> Result<PathBuf, ProfilerError> {
    let config_dir = dirs::config_dir().ok_or(ProfilerError::ConfigDirUnavailable)?;
    Ok(config_dir.join("one-ship").join("plugins"))
}

/// `<project>/.one-ship/plugins`.
pub fn get_project_plugins_dir(project_path: &Path) -> PathBuf {
    project_path.join(".one-ship").join("plugins")
}

fn validate_manifest(manifest: &ProfilerManifest) -> Result<(), ManifestFault> {
    if manifest.plugin.plugin_type != "profiler" {
        return Err(ManifestFault::WrongType(manifest.plugin.plugin_type.clone()));
    }
    let template = &manifest.execution.modes.wrapper.command_template;
    for token in [TOKEN_ORIGINAL_COMMAND, TOKEN_OUTPUT_FILE] {
        if !template.contains(token) {
            return Err(ManifestFault::MissingToken(token));
        }
    }
    Ok(())
}

fn parse_manifest_file(file_path: &Path) -> Result<ProfilerManifest, ManifestFault> {
    let content = fs::read_to_string(file_path).map_err(ManifestFault::Read)?;
    let manifest: ProfilerManifest = toml::from_str(&content).map_err(ManifestFault::Parse)?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

/// Reads every `<dir>/<plugin>/manifest.toml`. A missing `dir` has no manifests; an unreadable one
/// is an error; a manifest that does not parse or validate becomes a fault.
fn scan_dir_for_manifests(dir: &Path, scope: ProfilerScope, discovery: &mut ProfilerDiscovery) -> Result<(), ProfilerError> {
    let read_dir_err = |source| ProfilerError::ReadDir { path: dir.display().to_string(), source };
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(read_dir_err(e)),
    };
    for entry in entries {
        let manifest_path = entry.map_err(read_dir_err)?.path().join("manifest.toml");
        if !manifest_path.is_file() {
            continue;
        }
        match parse_manifest_file(&manifest_path) {
            Ok(manifest) => discovery.profilers.push(DiscoveredProfiler { manifest, scope, manifest_path: Some(manifest_path) }),
            Err(fault) => {
                crate::logger::log("WARN", "profiler", &format!("Rejected profiler manifest {}: {}", manifest_path.display(), fault));
                discovery.faults.push(ProfilerManifestFault { manifest_path: manifest_path.display().to_string(), error: fault.to_string() });
            }
        }
    }
    Ok(())
}

/// Engines shipped in the daemon binary.
pub fn get_builtin_manifests() -> Vec<ProfilerManifest> {
    vec![ProfilerManifest {
        plugin: ProfilerPluginSection {
            id: "samply".to_string(),
            name: "Samply Universal Profiler".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            plugin_type: "profiler".to_string(),
            author: Some("ONE-SHIP Core".to_string()),
            description: "Sampling profiler for native binaries, Node.js and Rust, viewed in the Firefox Profiler".to_string(),
        },
        detection: ProfilerDetection {
            file_triggers: vec!["Cargo.toml".to_string(), "*.rs".to_string(), "*.c".to_string(), "*.cpp".to_string()],
            binary_triggers: vec!["cargo".to_string(), "rustc".to_string()],
        },
        requirements: ProfilerRequirements {
            check_command: "samply --version".to_string(),
            install_hint: Some("pacman -S samply (or: cargo install samply)".to_string()),
            install: Some(ProfilerInstallSection {
                arch: Some("pacman -S samply".to_string()),
                cargo: Some("cargo install samply".to_string()),
                ..Default::default()
            }),
        },
        execution: ProfilerExecution {
            defaults: ProfilerExecutionDefaults { sampling_rate_hz: 1000, format: "firefox_profiler".to_string() },
            modes: ProfilerExecutionModes {
                wrapper: ProfilerWrapperMode {
                    // `--save-only`: write the file and exit instead of serving the profile.
                    command_template: "samply record --save-only --rate {sampling_rate_hz} --output {output_file} -- {original_command}".to_string(),
                },
            },
        },
        artifacts: ProfilerArtifacts { visual_extension: "json.gz".to_string(), viewer_type: ProfilerViewerType::FirefoxProfiler },
    }]
}

/// Manifests visible to a project: project, then global, then built-in; the first id wins.
/// Blocking (reads the plugin directories).
pub fn discover_profilers(project_path: &Path) -> Result<ProfilerDiscovery, ProfilerError> {
    crate::logger::log("DEBUG", "profiler", &format!("discover_profilers: project='{}'", project_path.display()));
    let mut discovery = ProfilerDiscovery::default();
    scan_dir_for_manifests(&get_project_plugins_dir(project_path), ProfilerScope::Project, &mut discovery)?;
    scan_dir_for_manifests(&get_global_plugins_dir()?, ProfilerScope::Global, &mut discovery)?;
    discovery.profilers.extend(get_builtin_manifests().into_iter().map(|manifest| DiscoveredProfiler {
        manifest,
        scope: ProfilerScope::BuiltIn,
        manifest_path: None,
    }));

    let mut seen_ids = std::collections::HashSet::new();
    discovery.profilers.retain(|p| seen_ids.insert(p.manifest.plugin.id.clone()));
    crate::logger::log("DEBUG", "profiler", &format!(
        "discover_profilers: {} engines, {} faults", discovery.profilers.len(), discovery.faults.len()
    ));
    Ok(discovery)
}

/// True when `check_command` exits 0. Blocking (runs the command).
pub fn check_tool_installed(check_command: &str) -> bool {
    let installed = Command::new("sh")
        .arg("-c")
        .arg(check_command)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    crate::logger::log("DEBUG", "profiler", &format!("check_tool_installed: '{}' -> {}", check_command, installed));
    installed
}

/// An engine as the Profiler tab lists it.
#[derive(Serialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct ProfilerManifestSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub scope: ProfilerScope,
    pub manifest_path: Option<String>,
    pub is_installed: bool,
    pub file_triggers: Vec<String>,
    pub install_hint: Option<String>,
    pub requires_sudo: bool,
    pub viewer_type: ProfilerViewerType,
    pub visual_extension: String,
}

/// Result of `list_available_profilers`: the usable engines and the rejected manifests.
#[derive(Serialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct ProfilerCatalog {
    pub engines: Vec<ProfilerManifestSummary>,
    pub faults: Vec<ProfilerManifestFault>,
}

/// Daemon command `list_available_profilers`. Blocking (reads the plugin directories and runs each
/// `check_command`).
pub fn list_available_profilers(project_path: String) -> Result<ProfilerCatalog, ProfilerError> {
    let discovery = discover_profilers(Path::new(&project_path))?;
    let engines = discovery.profilers.into_iter().map(|p| {
        let m = p.manifest;
        ProfilerManifestSummary {
            is_installed: check_tool_installed(&m.requirements.check_command),
            requires_sudo: m.requirements.install.as_ref().is_some_and(|i| i.requires_sudo),
            id: m.plugin.id,
            name: m.plugin.name,
            version: m.plugin.version,
            description: m.plugin.description,
            scope: p.scope,
            manifest_path: p.manifest_path.map(|path| path.display().to_string()),
            file_triggers: m.detection.file_triggers,
            install_hint: m.requirements.install_hint,
            viewer_type: m.artifacts.viewer_type,
            visual_extension: m.artifacts.visual_extension,
        }
    }).collect();
    Ok(ProfilerCatalog { engines, faults: discovery.faults })
}

/// Arguments of `get_target_runs`: the newest runs of a project, or of one of its targets.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct GetTargetRunsArgs {
    pub project_path: String,
    pub target_id: Option<String>,
    pub limit_count: u32,
}

/// Daemon command `get_target_runs`: the run history, newest first.
pub fn get_target_runs(args: GetTargetRunsArgs) -> Result<Vec<crate::db::TargetRunRecord>, ProfilerError> {
    let project = crate::registry::find_project_by_path(&args.project_path)?;
    Ok(crate::db::db_get_target_runs(project.id as i64, args.target_id.as_deref(), args.limit_count)?)
}

/// Arguments of `get_run_profile`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct RunIdArgs {
    pub run_id: String,
}

/// Arguments of the daemon command `toggle_project_profiler_plugin`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ToggleProjectProfilerPluginArgs {
    pub project_path: String,
    pub enabled: bool,
}

/// Daemon command `toggle_project_profiler_plugin`.
pub async fn toggle_project_profiler_plugin(
    project_path: String,
    enabled: bool,
) -> Result<bool, ProfilerError> {
    let project = crate::registry::find_project_by_path(&project_path)?;
    let p_path = Path::new(&project.path);

    let rula_path = p_path.join("rula.toml");
    let mut rula_val = crate::config::read_project_rula(p_path)
        .map_err(|e| ProfilerError::InvalidConfig {
            path: rula_path.display().to_string(),
            reason: e.to_string(),
        })?
        .ok_or_else(|| ProfilerError::InvalidConfig {
            path: rula_path.display().to_string(),
            reason: format!("{} does not exist; initialize the project first", rula_path.display()),
        })?;

    if let Some(root_table) = rula_val.as_table_mut() {
        let plugins_entry = root_table
            .entry("plugins".to_string())
            .or_insert_with(|| toml::Value::Table(Default::default()));
        if let Some(plugins_table) = plugins_entry.as_table_mut() {
            let profiler_entry = plugins_table
                .entry("profiler".to_string())
                .or_insert_with(|| toml::Value::Table(Default::default()));
            if let Some(profiler_table) = profiler_entry.as_table_mut() {
                profiler_table.insert("enabled".to_string(), toml::Value::Boolean(enabled));
            }
        }
    }

    let serialized_toml = toml::to_string_pretty(&rula_val).map_err(|e| ProfilerError::InvalidConfig {
        path: rula_path.display().to_string(),
        reason: format!("cannot serialize rula.toml: {}", e),
    })?;
    tokio::fs::write(&rula_path, serialized_toml)
        .await
        .map_err(|e| ProfilerError::Io {
            context: format!("Cannot write {}", rula_path.display()),
            source: e,
        })?;

    crate::db::notify_projects_updated();
    crate::logger::log(
        "INFO",
        "profiler",
        &format!("toggle_project_profiler_plugin: project='{}', enabled={}", project_path, enabled),
    );

    Ok(enabled)
}

/// Daemon command `get_run_profile`: the profile of one whole run, `None` while it has no samples.
pub fn get_run_profile(args: RunIdArgs) -> Result<Option<crate::duck::RunProfile>, ProfilerError> {
    crate::db::db_get_target_run(&args.run_id)?.ok_or_else(|| ProfilerError::RunNotFound(args.run_id.clone()))?;
    Ok(crate::duck::get_run_profile(&args.run_id)?)
}

/// Arguments of `compare_runs`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CompareRunsArgs {
    pub run_before_id: String,
    pub run_after_id: String,
}

/// `after - before` of the headline metrics of two runs.
#[derive(Serialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct RunMetricDeltas {
    pub sampled_seconds: f64,
    pub avg_cpu_percent: f64,
    pub p95_cpu_percent: f64,
    pub peak_cpu_percent: f64,
    pub avg_rss_memory_mbytes: f64,
    pub peak_rss_memory_mbytes: f64,
}

/// Result of `compare_runs`. Per-process differences are read from the two `processes` lists.
#[derive(Serialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct RunComparison {
    pub before: crate::duck::RunProfile,
    pub after: crate::duck::RunProfile,
    pub deltas: RunMetricDeltas,
}

fn profile_with_samples(run_id: &str) -> Result<crate::duck::RunProfile, ProfilerError> {
    crate::db::db_get_target_run(run_id)?.ok_or_else(|| ProfilerError::RunNotFound(run_id.to_string()))?;
    crate::duck::get_run_profile(run_id)?.ok_or_else(|| ProfilerError::NoSamples(run_id.to_string()))
}

/// Daemon command `compare_runs`: two runs side by side from their DuckDB samples.
pub fn compare_runs(args: CompareRunsArgs) -> Result<RunComparison, ProfilerError> {
    crate::logger::log("DEBUG", "profiler", &format!("compare_runs: before='{}', after='{}'", args.run_before_id, args.run_after_id));
    let before = profile_with_samples(&args.run_before_id)?;
    let after = profile_with_samples(&args.run_after_id)?;
    let deltas = RunMetricDeltas {
        sampled_seconds: after.sampled_seconds - before.sampled_seconds,
        avg_cpu_percent: after.avg_cpu_percent - before.avg_cpu_percent,
        p95_cpu_percent: after.p95_cpu_percent - before.p95_cpu_percent,
        peak_cpu_percent: after.peak_cpu_percent - before.peak_cpu_percent,
        avg_rss_memory_mbytes: after.avg_rss_memory_mbytes - before.avg_rss_memory_mbytes,
        peak_rss_memory_mbytes: after.peak_rss_memory_mbytes - before.peak_rss_memory_mbytes,
    };
    Ok(RunComparison { before, after, deltas })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"
[plugin]
id = "py-spy"
name = "py-spy"
version = "0.4.2"
type = "profiler"
description = "Python sampling profiler"

[requirements]
check_command = "py-spy --version"

[execution.defaults]
sampling_rate_hz = 100
format = "flamegraph"

[execution.modes.wrapper]
command_template = "py-spy record --rate {sampling_rate_hz} --format {format} --output {output_file} -- {original_command}"

[artifacts]
visual_extension = "svg"
viewer_type = "flamegraph"
"#;

    fn write_manifest(root: &Path, dir: &str, content: &str) -> PathBuf {
        let plugin_dir = root.join(dir);
        fs::create_dir_all(&plugin_dir).expect("plugin dir");
        let path = plugin_dir.join("manifest.toml");
        fs::write(&path, content).expect("manifest");
        path
    }

    #[test]
    fn a_valid_manifest_wraps_the_command() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = write_manifest(dir.path(), "profiler-pyspy", MANIFEST);
        let manifest = parse_manifest_file(&path).expect("valid manifest");
        assert_eq!(
            manifest.wrap_command("'uvicorn' 'app:main'", "'/p/scratchpad/profiling/svc_api_1.svg'"),
            "py-spy record --rate 100 --format flamegraph --output '/p/scratchpad/profiling/svc_api_1.svg' -- 'uvicorn' 'app:main'"
        );
        assert_eq!(manifest.output_rel_path("svc_api_1"), "scratchpad/profiling/svc_api_1.svg");
    }

    #[test]
    fn faulty_manifests_are_listed_instead_of_dropped() {
        let dir = tempfile::tempdir().expect("temp dir");
        write_manifest(dir.path(), "good", MANIFEST);
        write_manifest(dir.path(), "unknown-key", &MANIFEST.replace("[artifacts]", "[execution.modes.attach]\ncommand_template = \"x\"\n\n[artifacts]"));
        write_manifest(dir.path(), "old-token", &MANIFEST.replace("sampling_rate_hz = 100", "sampling_rate = 100"));
        write_manifest(dir.path(), "no-output", &MANIFEST.replace("--output {output_file} ", ""));

        let mut discovery = ProfilerDiscovery::default();
        scan_dir_for_manifests(dir.path(), ProfilerScope::Global, &mut discovery).expect("scan");

        assert_eq!(discovery.profilers.len(), 1);
        assert_eq!(discovery.profilers[0].manifest.plugin.id, "py-spy");
        assert_eq!(discovery.faults.len(), 3);
        assert!(discovery.faults.iter().any(|f| f.error.contains("{output_file}")));
    }

    #[test]
    fn a_missing_plugins_dir_has_no_manifests() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut discovery = ProfilerDiscovery::default();
        scan_dir_for_manifests(&dir.path().join("absent"), ProfilerScope::Project, &mut discovery).expect("scan");
        assert!(discovery.profilers.is_empty() && discovery.faults.is_empty());
    }

    #[test]
    fn builtin_manifests_are_valid() {
        for manifest in get_builtin_manifests() {
            validate_manifest(&manifest).expect("built-in manifest");
        }
    }
}
