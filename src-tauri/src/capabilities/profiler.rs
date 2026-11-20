//! Performance Profiler Capability Provider (`domain = "profiler"`, Spec 40).
//!
//! Exposes execution run listing, flamegraph/sample profile inspection, and comparative
//! regression diffs with strict `project_id` ownership verification.

use std::borrow::Cow;
use serde_json::{json, Value};

use crate::db::{db_get_target_run, db_get_target_runs};
use crate::kopeng::profiler::{compare_runs, get_run_profile, CompareRunsArgs, RunIdArgs};

use super::{
    require_non_empty_str, run_blocking, BoxFuture, CapabilityContext, CapabilityDef,
    CapabilityError, CapabilityProvider, CapabilityRisk, CapabilityTarget, CapabilityVisibility,
};

const DEFAULT_PROFILER_RUNS_LIMIT: u32 = 20;
const MAX_PROFILER_RUNS_LIMIT: u32 = 50;

pub static PROVIDER: CapabilityProvider = CapabilityProvider {
    domain_id: "profiler",
    label: "Performance Profiler",
    required_capability: Some("has_profiler_enabled"),
    linked_surface_path: Some("profiler"),
    static_actions,
    dynamic_actions: |_| Vec::new(),
    execute,
};

fn cap(
    id: &'static str,
    mcp_name: Option<&'static str>,
    description: &'static str,
    parameters_schema: Value,
    voice_max_items: Option<usize>,
) -> CapabilityDef {
    CapabilityDef {
        id: Cow::Borrowed(id),
        mcp_name: mcp_name.map(Cow::Borrowed),
        description: Cow::Borrowed(description),
        parameters_schema,
        visibility: CapabilityVisibility::Shared,
        risk: CapabilityRisk::ReadOnly,
        target: CapabilityTarget::Backend,
        voice_max_items,
    }
}

fn static_actions() -> Vec<CapabilityDef> {
    vec![
        cap(
            "list_runs",
            Some("list_profiler_runs"),
            "Lists recorded profiler execution runs for this project, optionally filtered by target_id and limit_count.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "target_id": { "type": "STRING", "description": "Optional profiler target binary/script ID" },
                    "limit_count": { "type": "INTEGER", "description": "Max recent runs to return (1-50, default 20)" }
                }
            }),
            Some(15),
        ),
        cap(
            "get_run_profile",
            Some("get_run_profile"),
            "Retrieves hotspot samples, metrics, and metadata for a specific profiler run_id belonging to this project.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "run_id": { "type": "STRING", "description": "Execution run ID from list_runs" }
                },
                "required": ["run_id"]
            }),
            None,
        ),
        cap(
            "compare_runs",
            Some("compare_profiler_runs"),
            "Compares two profiler runs (run_before_id vs run_after_id) belonging to this project to highlight regressions or improvements.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "run_before_id": { "type": "STRING", "description": "Baseline run ID for comparison" },
                    "run_after_id": { "type": "STRING", "description": "Candidate run ID to compare against baseline" }
                },
                "required": ["run_before_id", "run_after_id"]
            }),
            None,
        ),
    ]
}

fn ensure_run_owned(project_id: i64, run_id: &str) -> Result<(), CapabilityError> {
    let Some(rec) = db_get_target_run(run_id)? else {
        return Err(CapabilityError::InvalidParams(format!(
            "Profiler run '{run_id}' does not exist"
        )));
    };
    if rec.project_id != project_id {
        return Err(CapabilityError::InvalidParams(format!(
            "Profiler run '{run_id}' does not belong to project {project_id}"
        )));
    }
    Ok(())
}

fn execute<'a>(
    ctx: CapabilityContext,
    action_id: &'a str,
    params: Value,
) -> BoxFuture<'a, Result<Value, CapabilityError>> {
    Box::pin(async move {
        let project_id = ctx.project_id();

        match action_id {
            "list_runs" => {
                let target_id = match params.get("target_id") {
                    None | Some(Value::Null) => None,
                    Some(v) => v
                        .as_str()
                        .map(|s| (!s.trim().is_empty()).then(|| s.trim().to_string()))
                        .ok_or_else(|| {
                            CapabilityError::InvalidParams(
                                "'target_id' must be a string".to_string(),
                            )
                        })?,
                };
                let limit_count = match params.get("limit_count") {
                    None | Some(Value::Null) => DEFAULT_PROFILER_RUNS_LIMIT,
                    Some(v) => match v.as_u64().map(u32::try_from) {
                        Some(Ok(n)) if n > 0 => n.clamp(1, MAX_PROFILER_RUNS_LIMIT),
                        _ => {
                            return Err(CapabilityError::InvalidParams(
                                "'limit_count' must be a positive integer".to_string(),
                            ));
                        }
                    },
                };
                let runs = run_blocking(move || {
                    Ok(db_get_target_runs(
                        project_id,
                        target_id.as_deref(),
                        limit_count,
                    )?)
                })
                .await?;
                Ok(json!({ "runs": runs }))
            }
            "get_run_profile" => {
                let run_id = require_non_empty_str(&params, "run_id")?;
                let profile = run_blocking(move || {
                    ensure_run_owned(project_id, &run_id)?;
                    Ok(get_run_profile(RunIdArgs { run_id })?)
                })
                .await?;
                Ok(serde_json::to_value(profile)
                    .map_err(|e| CapabilityError::Task(e.to_string()))?)
            }
            "compare_runs" => {
                let run_before_id = require_non_empty_str(&params, "run_before_id")?;
                let run_after_id = require_non_empty_str(&params, "run_after_id")?;
                let diff = run_blocking(move || {
                    ensure_run_owned(project_id, &run_before_id)?;
                    ensure_run_owned(project_id, &run_after_id)?;
                    Ok(compare_runs(CompareRunsArgs {
                        run_before_id,
                        run_after_id,
                    })?)
                })
                .await?;
                Ok(serde_json::to_value(diff).map_err(|e| CapabilityError::Task(e.to_string()))?)
            }
            other => Err(CapabilityError::UnknownAction {
                domain: "profiler".to_string(),
                action_id: other.to_string(),
            }),
        }
    })
}
