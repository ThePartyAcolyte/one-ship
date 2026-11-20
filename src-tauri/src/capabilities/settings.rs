//! Project Configuration Capability Provider (`domain = "settings"`, Spec 40).
//!
//! Exposes declarative project configuration schema discovery and validated `rula.toml` field
//! mutations without permissive silent JSON fallbacks.

use std::borrow::Cow;
use serde_json::{json, Value};

use crate::config::{get_project_config_tree, set_project_config_field};

use super::{
    require_non_empty_str, run_blocking, BoxFuture, CapabilityContext, CapabilityDef,
    CapabilityError, CapabilityProvider, CapabilityRisk, CapabilityTarget, CapabilityVisibility,
};

pub static PROVIDER: CapabilityProvider = CapabilityProvider {
    domain_id: "settings",
    label: "Project Settings",
    required_capability: None,
    linked_surface_path: Some("settings"),
    static_actions,
    dynamic_actions: |_| Vec::new(),
    execute,
};

fn cap(
    id: &'static str,
    description: &'static str,
    parameters_schema: Value,
    risk: CapabilityRisk,
) -> CapabilityDef {
    CapabilityDef {
        id: Cow::Borrowed(id),
        mcp_name: None,
        description: Cow::Borrowed(description),
        parameters_schema,
        visibility: CapabilityVisibility::Shared,
        risk,
        target: CapabilityTarget::Backend,
        voice_max_items: None,
    }
}

fn static_actions() -> Vec<CapabilityDef> {
    vec![
        cap(
            "get_project_config_schema",
            "Returns the declarative project settings schema and effective resolved configuration values (from config.toml + rula.toml) for this project.",
            json!({ "type": "OBJECT", "properties": {} }),
            CapabilityRisk::ReadOnly,
        ),
        cap(
            "set_project_config_value",
            "Validates and sets a project-scoped [plugins.<plugin_id>].<field_key> value in rula.toml. Provide either 'value' or valid JSON in 'value_json' (for string values, quote as JSON e.g. '\"firmware\"').",
            json!({
                "type": "OBJECT",
                "properties": {
                    "plugin_id": { "type": "STRING", "description": "Plugin section ID returned by get_project_config_schema" },
                    "field_key": { "type": "STRING", "description": "Setting key returned by get_project_config_schema" },
                    "value_json": { "type": "STRING", "description": "Strict JSON literal (e.g. 'true', '42', or '\"string\"')" }
                },
                "required": ["plugin_id", "field_key", "value_json"]
            }),
            CapabilityRisk::Mutating,
        ),
    ]
}

fn parse_config_value(params: &Value) -> Result<Value, CapabilityError> {
    if let Some(raw_val) = params.get("value_json") {
        let Some(raw) = raw_val.as_str() else {
            return Err(CapabilityError::InvalidParams(
                "'value_json' must be a valid JSON-encoded string".to_string(),
            ));
        };
        serde_json::from_str::<Value>(raw).map_err(|e| {
            CapabilityError::InvalidParams(format!(
                "Invalid JSON in 'value_json' (quote strings as JSON literals, e.g. '\"text\"'): {e}"
            ))
        })
    } else if let Some(val) = params.get("value") {
        Ok(val.clone())
    } else {
        Err(CapabilityError::InvalidParams(
            "Missing configuration value: provide 'value_json' or 'value'".to_string(),
        ))
    }
}

fn execute<'a>(
    ctx: CapabilityContext,
    action_id: &'a str,
    params: Value,
) -> BoxFuture<'a, Result<Value, CapabilityError>> {
    Box::pin(async move {
        let project_path = ctx.project.path.clone();

        match action_id {
            "get_project_config_schema" => {
                run_blocking(move || Ok(get_project_config_tree(&project_path))).await
            }
            "set_project_config_value" => {
                let plugin_id = require_non_empty_str(&params, "plugin_id")?;
                let field_key = require_non_empty_str(&params, "field_key")?;
                let value = parse_config_value(&params)?;
                run_blocking(move || {
                    Ok(set_project_config_field(
                        &project_path,
                        &plugin_id,
                        &field_key,
                        value,
                    )?)
                })
                .await
            }
            other => Err(CapabilityError::UnknownAction {
                domain: "settings".to_string(),
                action_id: other.to_string(),
            }),
        }
    })
}
