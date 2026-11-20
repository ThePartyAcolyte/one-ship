//! Electronics & Dynamic Hardware Capabilities Provider (`domain = "electronics"`, Spec 40).
//!
//! Derives oscilloscope capability schemas dynamically from `OscilloscopeDescriptor` and backs
//! channel/timebase/trigger state with persistent `ui_state` slots plus direct daemon captures,
//! allowing instrument configuration and capture even when `OscilloscopeView` is unmounted.

use std::borrow::Cow;
use serde_json::{json, Value};

use crate::events::{DaemonEvent, UiSurfaceCommandPayload};
use crate::hydration::HydratedProject;
use crate::kopeng::electronics::instruments::driver::{
    OscilloscopeChannelConfig, OscilloscopeDescriptor, OscilloscopeTriggerConfig,
};
use crate::kopeng::electronics::instruments::owon_vds1022::descriptor as owon_descriptor;
use crate::kopeng::electronics::instruments::{
    capture_oscilloscope, scan_connected_instruments, CaptureOscilloscopeArgs,
};
use crate::ui_state::{dispatch_ui_action, get_ui_state, UiAction};

use super::{
    require_positive_i64, run_blocking, BoxFuture, CapabilityContext, CapabilityDef,
    CapabilityError, CapabilityProvider, CapabilityRisk, CapabilityTarget, CapabilityVisibility,
};

pub static PROVIDER: CapabilityProvider = CapabilityProvider {
    domain_id: "electronics",
    label: "Electronics & Oscilloscope",
    required_capability: Some("has_electronics_enabled"),
    linked_surface_path: Some("electronics/oscilloscope"),
    static_actions,
    dynamic_actions,
    execute,
};

fn cap(
    id: &'static str,
    description: impl Into<Cow<'static, str>>,
    parameters_schema: Value,
    risk: CapabilityRisk,
    target: CapabilityTarget,
) -> CapabilityDef {
    CapabilityDef {
        id: Cow::Borrowed(id),
        mcp_name: None,
        description: description.into(),
        parameters_schema,
        visibility: CapabilityVisibility::Shared,
        risk,
        target,
        voice_max_items: None,
    }
}

fn static_actions() -> Vec<CapabilityDef> {
    vec![cap(
        "scan_instruments",
        "Scans USB buses for connected oscilloscope and logic analyzer instruments.",
        json!({ "type": "OBJECT", "properties": {} }),
        CapabilityRisk::ReadOnly,
        CapabilityTarget::Backend,
    )]
}

fn dynamic_actions(_project: &HydratedProject) -> Vec<CapabilityDef> {
    let desc = owon_descriptor();
    let timebase_list: Vec<String> = desc.timebase_steps_us.iter().map(|v| v.to_string()).collect();
    let volts_list: Vec<String> = desc
        .volts_per_div_options_mv
        .iter()
        .map(|v| v.to_string())
        .collect();

    vec![
        cap(
            "get_oscilloscope_descriptor",
            Cow::Owned(format!(
                "Returns hardware specifications for '{}' (channels: {}, timebase: [{}] us, volts/div: [{}] mV) and the current persisted oscilloscope slots.",
                desc.name,
                desc.channel_names.join(", "),
                timebase_list.join(", "),
                volts_list.join(", "),
            )),
            json!({ "type": "OBJECT", "properties": {} }),
            CapabilityRisk::ReadOnly,
            CapabilityTarget::Backend,
        ),
        cap(
            "set_channel_enabled",
            Cow::Owned(format!(
                "Enables or disables oscilloscope channel 1..={} ({}) and persists the slot state.",
                desc.channel_count,
                desc.channel_names.join(", ")
            )),
            json!({
                "type": "OBJECT",
                "properties": {
                    "channel": { "type": "INTEGER", "description": format!("Analog channel index (1..={})", desc.channel_count) },
                    "enabled": { "type": "BOOLEAN", "description": "True to show and acquire channel, false to mute it" }
                },
                "required": ["channel", "enabled"]
            }),
            CapabilityRisk::Mutating,
            CapabilityTarget::FrontendCommand {
                node_path: "electronics/oscilloscope",
            },
        ),
        cap(
            "set_timebase",
            Cow::Owned(format!(
                "Sets oscilloscope horizontal timebase in microseconds per division. Supported steps: [{}].",
                timebase_list.join(", ")
            )),
            json!({
                "type": "OBJECT",
                "properties": {
                    "timebase_us": { "type": "NUMBER", "description": "Horizontal time per grid division in microseconds (must match supported driver step)" }
                },
                "required": ["timebase_us"]
            }),
            CapabilityRisk::Mutating,
            CapabilityTarget::FrontendSlot {
                slot_key: "electronics.oscilloscope_timebase_us",
            },
        ),
        cap(
            "set_trigger",
            Cow::Owned(format!(
                "Configures oscilloscope trigger parameters (source: {:?}, slope: {:?}, sweep: {:?}) and persists them in ui_state slots.",
                desc.trigger_sources, desc.trigger_slopes, desc.trigger_modes
            )),
            json!({
                "type": "OBJECT",
                "properties": {
                    "source": { "type": "STRING", "enum": desc.trigger_sources, "description": "Channel or external input used for trigger detection" },
                    "slope": { "type": "STRING", "enum": desc.trigger_slopes, "description": "Signal edge direction that fires trigger (rising | falling)" },
                    "sweep": { "type": "STRING", "enum": desc.trigger_modes, "description": "Trigger mode: auto (free-run if untriggered), normal (wait), or single" }
                }
            }),
            CapabilityRisk::Mutating,
            CapabilityTarget::FrontendCommand {
                node_path: "electronics/oscilloscope",
            },
        ),
        cap(
            "capture_single",
            Cow::Owned(format!(
                "Acquires a single oscilloscope capture from '{}' using persisted channel/timebase/trigger slots and returns scalar electrical metrics (Vpp, Vrms, Vmin, Vmax, Vavg, frequency).",
                desc.name
            )),
            json!({ "type": "OBJECT", "properties": {} }),
            CapabilityRisk::Mutating,
            CapabilityTarget::Backend,
        ),
    ]
}

fn normalize_slot_titlecase(raw: &str) -> String {
    let mut chars = raw.trim().chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let head: String = first.to_uppercase().collect();
            head + &chars.as_str().to_lowercase()
        }
    }
}

struct PersistedScopeConfig {
    ch1_enabled: bool,
    ch2_enabled: bool,
    timebase_us: f64,
    trigger_source: String,
    trigger_slope: String,
    trigger_sweep: String,
}

fn read_scope_slots(project_id: i64) -> Result<PersistedScopeConfig, CapabilityError> {
    let snapshot = get_ui_state()?;
    let slots = snapshot
        .projects
        .get(&project_id.to_string())
        .map(|p| &p.slot_values);

    let read_bool_slot = |key: &str, default: bool| -> Result<bool, CapabilityError> {
        match slots.and_then(|s| s.get(key)).and_then(|v| v.as_deref()) {
            None => Ok(default),
            Some("true") => Ok(true),
            Some("false") => Ok(false),
            Some(other) => Err(CapabilityError::InvalidParams(format!(
                "Invalid persisted boolean slot '{key}' value '{other}'"
            ))),
        }
    };

    let ch1_enabled = read_bool_slot("electronics.oscilloscope_ch1_enabled", true)?;
    let ch2_enabled = read_bool_slot("electronics.oscilloscope_ch2_enabled", true)?;
    let timebase_us = match slots
        .and_then(|s| s.get("electronics.oscilloscope_timebase_us"))
        .and_then(|v| v.as_deref())
    {
        None => 200.0,
        Some(raw) => raw.parse::<f64>().map_err(|_| {
            CapabilityError::InvalidParams(format!(
                "Invalid persisted timebase slot value '{raw}'"
            ))
        })?,
    };
    let trigger_source = match slots
        .and_then(|s| s.get("electronics.oscilloscope_trigger_source"))
        .and_then(|v| v.as_deref())
    {
        Some(s) => s.to_string(),
        None => "CH1".to_string(),
    };
    let trigger_slope = match slots
        .and_then(|s| s.get("electronics.oscilloscope_trigger_slope"))
        .and_then(|v| v.as_deref())
    {
        Some(s) => s.to_lowercase(),
        None => "rising".to_string(),
    };
    let trigger_sweep = match slots
        .and_then(|s| s.get("electronics.oscilloscope_trigger_sweep"))
        .and_then(|v| v.as_deref())
    {
        Some(s) => s.to_lowercase(),
        None => "auto".to_string(),
    };

    Ok(PersistedScopeConfig {
        ch1_enabled,
        ch2_enabled,
        timebase_us,
        trigger_source,
        trigger_slope,
        trigger_sweep,
    })
}

fn validate_timebase_step(desc: &OscilloscopeDescriptor, value: f64) -> Result<String, CapabilityError> {
    if !value.is_finite() {
        return Err(CapabilityError::InvalidParams(
            "'timebase_us' must be a finite number".to_string(),
        ));
    }
    let matched = desc
        .timebase_steps_us
        .iter()
        .find(|&&step| (step - value).abs() <= 1e-6);
    match matched {
        Some(step) => Ok(step.to_string()),
        None => Err(CapabilityError::InvalidParams(format!(
            "Unsupported timebase_us {value}; supported steps are {:?}",
            desc.timebase_steps_us
        ))),
    }
}

fn emit_oscilloscope_command(project_id: i64, action_id: &str, params: Value) {
    crate::supervisor::get_supervisor().emit_event(DaemonEvent::ui_surface_command(
        &UiSurfaceCommandPayload {
            project_id,
            node_path: "electronics/oscilloscope".to_string(),
            action_id: action_id.to_string(),
            params,
        },
    ));
}

fn execute<'a>(
    ctx: CapabilityContext,
    action_id: &'a str,
    params: Value,
) -> BoxFuture<'a, Result<Value, CapabilityError>> {
    Box::pin(async move {
        let project_id = ctx.project_id();
        let desc = owon_descriptor();

        match action_id {
            "scan_instruments" => {
                let instruments = scan_connected_instruments().await?;
                Ok(json!({ "instruments": instruments }))
            }
            "get_oscilloscope_descriptor" => {
                let slots = run_blocking(move || read_scope_slots(project_id)).await?;
                Ok(json!({
                    "descriptor": desc,
                    "current_slots": {
                        "ch1_enabled": slots.ch1_enabled,
                        "ch2_enabled": slots.ch2_enabled,
                        "timebase_us": slots.timebase_us,
                        "trigger_source": slots.trigger_source,
                        "trigger_slope": slots.trigger_slope,
                        "trigger_sweep": slots.trigger_sweep,
                    }
                }))
            }
            "set_channel_enabled" => {
                let channel = require_positive_i64(&params, "channel")?;
                if channel > i64::from(desc.channel_count) {
                    return Err(CapabilityError::InvalidParams(format!(
                        "Channel {channel} out of range (1..={})",
                        desc.channel_count
                    )));
                }
                let Some(enabled) = params.get("enabled").and_then(|v| v.as_bool()) else {
                    return Err(CapabilityError::InvalidParams(
                        "Missing boolean parameter 'enabled'".to_string(),
                    ));
                };
                let slot_key = format!("electronics.oscilloscope_ch{channel}_enabled");
                let slot_val = if enabled { "true" } else { "false" }.to_string();
                let snapshot = run_blocking(move || {
                    Ok(dispatch_ui_action(UiAction::SetSlotValue {
                        project_id,
                        key: slot_key,
                        value: Some(slot_val),
                    })?)
                })
                .await?;
                emit_oscilloscope_command(
                    project_id,
                    "set_channel_enabled",
                    json!({ "channel": channel, "enabled": enabled }),
                );
                Ok(json!({
                    "ok": true,
                    "channel": channel,
                    "enabled": enabled,
                    "revision": snapshot.revision
                }))
            }
            "set_timebase" => {
                let Some(tb_val) = params.get("timebase_us").and_then(|v| v.as_f64()) else {
                    return Err(CapabilityError::InvalidParams(
                        "Missing numeric parameter 'timebase_us'".to_string(),
                    ));
                };
                let step_str = validate_timebase_step(&desc, tb_val)?;
                let slot_val = step_str.clone();
                let snapshot = run_blocking(move || {
                    Ok(dispatch_ui_action(UiAction::SetSlotValue {
                        project_id,
                        key: "electronics.oscilloscope_timebase_us".to_string(),
                        value: Some(slot_val),
                    })?)
                })
                .await?;
                emit_oscilloscope_command(
                    project_id,
                    "set_timebase",
                    json!({ "timebase_us": step_str }),
                );
                Ok(json!({
                    "ok": true,
                    "timebase_us": tb_val,
                    "revision": snapshot.revision
                }))
            }
            "set_trigger" => {
                let mut updates: Vec<(String, String)> = Vec::new();
                let mut cmd_payload = serde_json::Map::new();

                if let Some(val) = params.get("source") {
                    let Some(src) = val.as_str().map(str::trim) else {
                        return Err(CapabilityError::InvalidParams(
                            "'source' must be a string".to_string(),
                        ));
                    };
                    if !desc.trigger_sources.iter().any(|s| s == src) {
                        return Err(CapabilityError::InvalidParams(format!(
                            "Invalid trigger source '{src}'; expected one of {:?}",
                            desc.trigger_sources
                        )));
                    }
                    updates.push((
                        "electronics.oscilloscope_trigger_source".to_string(),
                        src.to_string(),
                    ));
                    cmd_payload.insert("source".to_string(), json!(src));
                }

                if let Some(val) = params.get("slope") {
                    let Some(raw) = val.as_str().map(str::trim) else {
                        return Err(CapabilityError::InvalidParams(
                            "'slope' must be a string".to_string(),
                        ));
                    };
                    let lower = raw.to_lowercase();
                    if !desc.trigger_slopes.iter().any(|s| s == &lower) {
                        return Err(CapabilityError::InvalidParams(format!(
                            "Invalid trigger slope '{raw}'; expected one of {:?}",
                            desc.trigger_slopes
                        )));
                    }
                    let slot_label = normalize_slot_titlecase(&lower);
                    updates.push((
                        "electronics.oscilloscope_trigger_slope".to_string(),
                        slot_label.clone(),
                    ));
                    cmd_payload.insert("slope".to_string(), json!(slot_label));
                }

                if let Some(val) = params.get("sweep") {
                    let Some(raw) = val.as_str().map(str::trim) else {
                        return Err(CapabilityError::InvalidParams(
                            "'sweep' must be a string".to_string(),
                        ));
                    };
                    let lower = raw.to_lowercase();
                    if !desc.trigger_modes.iter().any(|m| m == &lower) {
                        return Err(CapabilityError::InvalidParams(format!(
                            "Invalid trigger sweep '{raw}'; expected one of {:?}",
                            desc.trigger_modes
                        )));
                    }
                    let slot_label = normalize_slot_titlecase(&lower);
                    updates.push((
                        "electronics.oscilloscope_trigger_sweep".to_string(),
                        slot_label.clone(),
                    ));
                    cmd_payload.insert("sweep".to_string(), json!(slot_label));
                }

                if updates.is_empty() {
                    return Err(CapabilityError::InvalidParams(
                        "Provide at least one of 'source', 'slope', or 'sweep'".to_string(),
                    ));
                }

                let updated_updates = updates.clone();
                let revision = run_blocking(move || {
                    let mut rev = 0u64;
                    for (key, value) in updated_updates {
                        let snap = dispatch_ui_action(UiAction::SetSlotValue {
                            project_id,
                            key,
                            value: Some(value),
                        })?;
                        rev = snap.revision;
                    }
                    Ok(rev)
                })
                .await?;
                emit_oscilloscope_command(project_id, "set_trigger", Value::Object(cmd_payload.clone()));
                Ok(json!({
                    "ok": true,
                    "revision": revision,
                    "updated": Value::Object(cmd_payload)
                }))
            }
            "capture_single" => {
                let slots = run_blocking(move || read_scope_slots(project_id)).await?;
                let res = capture_oscilloscope(CaptureOscilloscopeArgs {
                    ch1: Some(OscilloscopeChannelConfig {
                        enabled: slots.ch1_enabled,
                        ..Default::default()
                    }),
                    ch2: Some(OscilloscopeChannelConfig {
                        enabled: slots.ch2_enabled,
                        ..Default::default()
                    }),
                    trigger: Some(OscilloscopeTriggerConfig {
                        source: slots.trigger_source.clone(),
                        slope: slots.trigger_slope.clone(),
                        sweep_mode: slots.trigger_sweep.clone(),
                        level_mv: 0,
                    }),
                    timebase_div_us: slots.timebase_us,
                })
                .await?;

                Ok(json!({
                    "ch1_enabled": slots.ch1_enabled,
                    "ch2_enabled": slots.ch2_enabled,
                    "time_per_div_us": res.time_per_div_us,
                    "sample_rate_hz": res.sample_rate_hz,
                    "samples_count": res.samples_count,
                    "duration_ms": res.duration_ms,
                    "ch1_metrics": res.ch1_metrics,
                    "ch2_metrics": res.ch2_metrics,
                }))
            }
            other => Err(CapabilityError::UnknownAction {
                domain: "electronics".to_string(),
                action_id: other.to_string(),
            }),
        }
    })
}
