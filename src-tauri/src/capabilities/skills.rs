//! Project Skills Capability Provider (`domain = "skills"`, Spec 40).
//!
//! Unifies installed project skill listing, SkillsMP ecosystem search, and project skill
//! activation/deactivation/installation/removal across MCP and Global Voice Controller.

use std::borrow::Cow;
use std::path::Path;
use serde_json::{json, Value};

use crate::kopeng::skill::{
    install_project_skill, list_project_skills, remove_project_skill, search_skillsmp_registry,
    toggle_project_skill,
};

use super::{
    require_non_empty_str, run_blocking, BoxFuture, CapabilityContext, CapabilityDef,
    CapabilityError, CapabilityProvider, CapabilityRisk, CapabilityTarget, CapabilityVisibility,
};

const MAX_SKILL_SEARCH_LIMIT_COUNT: usize = 25;

pub static PROVIDER: CapabilityProvider = CapabilityProvider {
    domain_id: "skills",
    label: "Project Skills",
    required_capability: None,
    linked_surface_path: Some("skills"),
    static_actions,
    dynamic_actions: |_| Vec::new(),
    execute,
};

fn cap(
    id: &'static str,
    mcp_name: Option<&'static str>,
    description: &'static str,
    parameters_schema: Value,
    risk: CapabilityRisk,
    voice_max_items: Option<usize>,
) -> CapabilityDef {
    CapabilityDef {
        id: Cow::Borrowed(id),
        mcp_name: mcp_name.map(Cow::Borrowed),
        description: Cow::Borrowed(description),
        parameters_schema,
        visibility: CapabilityVisibility::Shared,
        risk,
        target: CapabilityTarget::Backend,
        voice_max_items,
    }
}

fn static_actions() -> Vec<CapabilityDef> {
    vec![
        cap(
            "list_skills",
            None,
            "Lists all skills symlinked or installed in the project with active/disabled and platform-base status.",
            json!({ "type": "OBJECT", "properties": {} }),
            CapabilityRisk::ReadOnly,
            Some(25),
        ),
        cap(
            "search_registry",
            Some("search_project_skills"),
            "Searches the SkillsMP ecosystem registry for specialized skills matching query.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "query": { "type": "STRING", "description": "Technology, framework, or domain keyword to search" },
                    "limit": { "type": "INTEGER", "description": "Max registry results (1-25)" }
                },
                "required": ["query"]
            }),
            CapabilityRisk::ReadOnly,
            Some(10),
        ),
        cap(
            "manage_skill",
            Some("manage_project_skill"),
            "Lists, enables, disables, installs (from source_url), or removes non-platform skills in this project.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "action": { "type": "STRING", "enum": ["list", "enable", "disable", "install", "remove"], "description": "Skill management operation" },
                    "skill_name": { "type": "STRING", "description": "Skill directory name (required for enable/disable/install/remove)" },
                    "source_url": { "type": "STRING", "description": "GitHub/SkillsMP source URL (required for install)" }
                },
                "required": ["action"]
            }),
            CapabilityRisk::Mutating,
            None,
        ),
    ]
}

fn execute<'a>(
    ctx: CapabilityContext,
    action_id: &'a str,
    params: Value,
) -> BoxFuture<'a, Result<Value, CapabilityError>> {
    Box::pin(async move {
        let project_name = ctx.project.name.clone();
        let project_path = ctx.project.path.clone();

        match action_id {
            "list_skills" => {
                let skills = run_blocking(move || {
                    Ok(list_project_skills(Path::new(&project_path))?)
                })
                .await?;
                Ok(json!({ "project_name": project_name, "skills": skills }))
            }
            "search_registry" => {
                let query = require_non_empty_str(&params, "query")?;
                let limit = match params.get("limit") {
                    None | Some(Value::Null) => None,
                    Some(v) => match v.as_u64() {
                        Some(n) if n > 0 => Some((n as usize).clamp(1, MAX_SKILL_SEARCH_LIMIT_COUNT)),
                        _ => {
                            return Err(CapabilityError::InvalidParams(
                                "'limit' must be a positive integer".to_string(),
                            ));
                        }
                    },
                };
                let items = search_skillsmp_registry(&query, limit).await?;
                Ok(json!({ "query": query, "items": items }))
            }
            "manage_skill" => {
                let action = require_non_empty_str(&params, "action")?;
                match action.as_str() {
                    "list" => {
                        let skills = run_blocking(move || {
                            Ok(list_project_skills(Path::new(&project_path))?)
                        })
                        .await?;
                        Ok(json!({
                            "action": "list",
                            "project_name": project_name,
                            "skills": skills
                        }))
                    }
                    "enable" => {
                        let skill_name = require_non_empty_str(&params, "skill_name")?;
                        let name_for_msg = skill_name.clone();
                        run_blocking(move || {
                            Ok(toggle_project_skill(
                                Path::new(&project_path),
                                &skill_name,
                                true,
                            )?)
                        })
                        .await?;
                        Ok(json!({
                            "ok": true,
                            "action": "enable",
                            "skill_name": name_for_msg,
                            "message": format!("Skill '{name_for_msg}' enabled successfully in project '{project_name}'.")
                        }))
                    }
                    "disable" => {
                        let skill_name = require_non_empty_str(&params, "skill_name")?;
                        let name_for_msg = skill_name.clone();
                        run_blocking(move || {
                            Ok(toggle_project_skill(
                                Path::new(&project_path),
                                &skill_name,
                                false,
                            )?)
                        })
                        .await?;
                        Ok(json!({
                            "ok": true,
                            "action": "disable",
                            "skill_name": name_for_msg,
                            "message": format!("Skill '{name_for_msg}' disabled successfully in project '{project_name}'.")
                        }))
                    }
                    "remove" => {
                        let skill_name = require_non_empty_str(&params, "skill_name")?;
                        let name_for_msg = skill_name.clone();
                        let removed = run_blocking(move || {
                            Ok(remove_project_skill(Path::new(&project_path), &skill_name)?)
                        })
                        .await?;
                        if removed {
                            Ok(json!({
                                "ok": true,
                                "action": "remove",
                                "skill_name": name_for_msg,
                                "message": format!("Skill '{name_for_msg}' removed successfully from project '{project_name}'.")
                            }))
                        } else {
                            Err(CapabilityError::InvalidParams(format!(
                                "Skill '{name_for_msg}' was not found in project '{project_name}'"
                            )))
                        }
                    }
                    "install" => {
                        let skill_name = require_non_empty_str(&params, "skill_name")?;
                        let source_url = require_non_empty_str(&params, "source_url")?;
                        let msg =
                            install_project_skill(Path::new(&project_path), &skill_name, &source_url)
                                .await?;
                        Ok(json!({
                            "ok": true,
                            "action": "install",
                            "skill_name": skill_name,
                            "message": format!("{msg} in project '{project_name}'. It is now active in .agents/skills/{skill_name}.")
                        }))
                    }
                    other => Err(CapabilityError::InvalidParams(format!(
                        "Unknown skill action '{other}'; expected list, enable, disable, install, or remove"
                    ))),
                }
            }
            other => Err(CapabilityError::UnknownAction {
                domain: "skills".to_string(),
                action_id: other.to_string(),
            }),
        }
    })
}
