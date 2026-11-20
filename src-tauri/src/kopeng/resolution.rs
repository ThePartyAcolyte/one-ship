//! Effective runner, model and effort of an agent session (Spec 10 §5).
//!
//! The runner manifests (`templates/runners/<id>.toml`) are the only source of the
//! runner → models → efforts relation. A pair always comes whole from one source — an explicit
//! choice, the agent's `[agents.<id>]` table, or a global pair of `[plugins.orchestrator]` — and
//! is validated against the manifest. Nothing is substituted: an unknown runner or model, a model
//! of another runner or an unsupported effort is an error.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::config::ConfigError;

/// Resolved `(runner, model, effort)` execution triple for an agent invocation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ts_rs::TS)]
#[ts(export)]
pub struct EffectiveAgentConfig {
    pub runner: String,
    pub model: String,
    /// `None` only for a model whose manifest declares no efforts.
    pub effort: Option<String>,
}

/// Keys of a runner/model/effort pair in `[plugins.<plugin>]`.
#[derive(Debug, Clone, Copy)]
pub struct AgentPairKeys {
    pub plugin: &'static str,
    pub runner: &'static str,
    pub model: &'static str,
    pub effort: &'static str,
}

/// Pair of the project initializer.
pub const INIT_AGENT_KEYS: AgentPairKeys = AgentPairKeys { plugin: "orchestrator", runner: "init_runner", model: "init_model", effort: "init_effort" };
/// Pair of platform agents without a persona (flag resolution).
pub const DEFAULT_AGENT_KEYS: AgentPairKeys = AgentPairKeys { plugin: "orchestrator", runner: "agent_runner", model: "default_model", effort: "default_effort" };
/// Pair of the Git & VCS platform agent and commit-message generator.
pub const GIT_AGENT_KEYS: AgentPairKeys = AgentPairKeys { plugin: "vcs", runner: "git_runner", model: "git_model", effort: "git_effort" };

/// A pair chosen explicitly by the caller (e.g. the init modal).
#[derive(Debug, Clone)]
pub struct ExplicitAgentConfig {
    pub runner: String,
    pub model: String,
    pub effort: Option<String>,
}

impl ExplicitAgentConfig {
    /// `None` when neither runner nor model was chosen; one without the other is an error.
    pub fn from_parts(
        context: &str,
        runner: Option<String>,
        model: Option<String>,
        effort: Option<String>,
    ) -> Result<Option<Self>, ConfigError> {
        let non_empty = |v: Option<String>| v.filter(|s| !s.trim().is_empty());
        match (non_empty(runner), non_empty(model)) {
            (Some(runner), Some(model)) => Ok(Some(Self { runner, model, effort: non_empty(effort) })),
            (None, None) => Ok(None),
            _ => Err(ConfigError::IncompletePair { context: context.to_string() }),
        }
    }
}

/// Validates a pair from one source against its runner's manifest; a missing effort takes the
/// model's declared `default_effort`.
pub fn finalize_agent_config(
    context: &str,
    runner: &str,
    model: Option<&str>,
    effort: Option<&str>,
) -> Result<EffectiveAgentConfig, ConfigError> {
    let model = model
        .filter(|m| !m.trim().is_empty())
        .ok_or_else(|| ConfigError::MissingModel { context: context.to_string(), runner: runner.to_string() })?;
    let effort = crate::config::validate_agent_pair(runner, model, effort.filter(|e| !e.trim().is_empty()))?;
    Ok(EffectiveAgentConfig { runner: runner.to_string(), model: model.to_string(), effort })
}

/// Resolves the pair of an agent session from the first source that has one:
/// 1. `explicit` (the caller's choice);
/// 2. `[agents.<agent_id>]` of the project's `rula.toml` (read strictly; only the
///    `config-resolver`, which repairs an invalid file, reads it leniently);
/// 3. the global pair `keys` of `[plugins.orchestrator]` (project overrides global).
pub fn resolve_effective_agent_config(
    project_path: Option<&str>,
    agent_id: Option<&str>,
    explicit: Option<ExplicitAgentConfig>,
    keys: &AgentPairKeys,
) -> Result<EffectiveAgentConfig, ConfigError> {
    if let Some(e) = explicit {
        return finalize_agent_config("explicit agent configuration", &e.runner, Some(&e.model), e.effort.as_deref());
    }

    if let (Some(path), Some(id)) = (project_path, agent_id) {
        let rula = if id == crate::kopeng::flag_resolver::CONFIG_AGENT_ID {
            crate::config::load_project_rula(Path::new(path))
        } else {
            crate::config::read_project_rula(Path::new(path))?
        };
        if let Some(agent) = rula.as_ref().and_then(|r| r.get("agents")?.get(id)) {
            let field = |key: &str| agent.get(key).and_then(|v| v.as_str());
            let context = format!("agents.{}", id);
            let runner = field("runner").ok_or_else(|| ConfigError::IncompletePair { context: context.clone() })?;
            return finalize_agent_config(&context, runner, field("model"), field("effort"));
        }
    }

    let value = |key: &str| crate::config::resolve_effective_string(project_path, None, keys.plugin, key);
    let runner = value(keys.runner);
    let model = value(keys.model);
    let effort = value(keys.effort);
    finalize_agent_config(&format!("plugins.{}.{}", keys.plugin, keys.model), &runner, Some(&model), Some(&effort))
}

/// A configured global pair the UI preselects (e.g. the init modal starts from `init`).
#[derive(Debug, Clone, Copy, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ConfiguredAgentPair {
    Init,
    Default,
}

/// Arguments of the daemon command `get_configured_agent_pair`.
#[derive(Debug, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct GetConfiguredAgentPairArgs {
    pub pair: ConfiguredAgentPair,
}

/// Daemon command `get_configured_agent_pair`: the global pair as configured, validated against
/// the runner manifest. An invalid configuration is an error, never replaced by another pair.
pub async fn get_configured_agent_pair(pair: ConfiguredAgentPair) -> Result<EffectiveAgentConfig, ConfigError> {
    crate::logger::log("DEBUG", "resolution", &format!("get_configured_agent_pair: {:?}", pair));
    match pair {
        ConfiguredAgentPair::Init => resolve_effective_agent_config(None, None, None, &INIT_AGENT_KEYS),
        ConfiguredAgentPair::Default => resolve_effective_agent_config(None, None, None, &DEFAULT_AGENT_KEYS),
    }
}

/// Markdown catalog of every runner manifest — runners, their models and each model's efforts —
/// injected into agents that write `[agents.<id>]` tables, so they take pairs only from the
/// manifests instead of choosing models themselves.
pub fn runner_catalog_markdown() -> Result<String, ConfigError> {
    let mut lines = Vec::new();
    for runner in crate::config::list_agent_runners()? {
        lines.push(format!("- runner `{}` ({}):", runner.id, runner.name));
        for model in &runner.models {
            let efforts = match &model.default_effort {
                Some(default) => format!("effort one of {} (default `{}`)", model.effort_levels.iter().map(|e| format!("`{}`", e)).collect::<Vec<_>>().join(", "), default),
                None => "no effort setting (omit `effort`)".to_string(),
            };
            lines.push(format!("  - model `{}` ({}): {}", model.id, model.label, efforts));
        }
    }
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn explicit(runner: &str, model: &str, effort: Option<&str>) -> Option<ExplicitAgentConfig> {
        Some(ExplicitAgentConfig { runner: runner.into(), model: model.into(), effort: effort.map(String::from) })
    }

    #[test]
    fn global_pairs_come_from_the_configuration() {
        // Test builds use a per-process config.toml written with the defaults.
        let cfg = resolve_effective_agent_config(None, None, None, &DEFAULT_AGENT_KEYS).unwrap();
        assert_eq!(cfg, EffectiveAgentConfig { runner: "agy".into(), model: "gemini-3.8-flash".into(), effort: Some("high".into()) });
        let init = resolve_effective_agent_config(None, None, None, &INIT_AGENT_KEYS).unwrap();
        assert_eq!(init.model, "gemini-3.8-flash");
    }

    #[test]
    fn an_explicit_pair_of_the_manifest_is_used_and_a_missing_effort_takes_the_model_default() {
        let cfg = resolve_effective_agent_config(None, None, explicit("opencode", "deepseek/deepseek-v4-pro", Some("max")), &DEFAULT_AGENT_KEYS).unwrap();
        assert_eq!(cfg.effort.as_deref(), Some("max"));
        let cfg = resolve_effective_agent_config(None, None, explicit("claude", "claude-opus-5-5", None), &DEFAULT_AGENT_KEYS).unwrap();
        assert_eq!(cfg.effort.as_deref(), Some("medium"));
        let cfg = resolve_effective_agent_config(None, None, explicit("agy", "gpt-oss-120b-medium", None), &DEFAULT_AGENT_KEYS).unwrap();
        assert_eq!(cfg.effort, None);
    }

    #[test]
    fn a_model_of_another_runner_or_an_unsupported_effort_is_an_error_not_a_substitution() {
        let cross = resolve_effective_agent_config(None, None, explicit("agy", "deepseek/deepseek-v4-pro", None), &DEFAULT_AGENT_KEYS);
        assert!(matches!(cross, Err(ConfigError::UnknownModel { .. })));
        let effort = resolve_effective_agent_config(None, None, explicit("opencode", "deepseek/deepseek-flash", Some("medium")), &DEFAULT_AGENT_KEYS);
        assert!(matches!(effort, Err(ConfigError::InvalidEffort { .. })));
        let no_levels = resolve_effective_agent_config(None, None, explicit("agy", "gpt-oss-120b-medium", Some("high")), &DEFAULT_AGENT_KEYS);
        assert!(matches!(no_levels, Err(ConfigError::InvalidEffort { .. })));
        let runner = resolve_effective_agent_config(None, None, explicit("cursor", "x", None), &DEFAULT_AGENT_KEYS);
        assert!(matches!(runner, Err(ConfigError::UnknownRunner { .. })));
    }

    #[test]
    fn an_explicit_runner_without_model_is_incomplete() {
        assert!(matches!(
            ExplicitAgentConfig::from_parts("init", Some("agy".into()), None, None),
            Err(ConfigError::IncompletePair { .. })
        ));
        assert!(ExplicitAgentConfig::from_parts("init", None, Some(" ".into()), None).unwrap().is_none());
    }
}
