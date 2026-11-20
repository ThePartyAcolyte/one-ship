use std::process::{Command, Stdio};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncBufReadExt;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use crate::events::DaemonEvent;

/// Parameters requested by client when arming an OpenDictate speech session.
/// Arguments of the daemon command `reserve_voice_capture`.
#[derive(Deserialize, Clone, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct VoiceReservationRequest {
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub project_id: Option<usize>,
    pub project_name: Option<String>,
    pub app_name: Option<String>,
    pub accent_color: Option<String>,
    pub hide_bubble: Option<bool>,
}

/// Structured outcome returned when registering a capture session.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct VoiceReservationResult {
    pub success: bool,
    pub service_available: bool,
    pub session_id: String,
    pub agent_id: Option<String>,
    pub message: String,
}

/// Payload of `opendictate_session_started`.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct SessionStartedPayload {
    pub session_id: String,
}

/// Payload of `opendictate_interim_text`.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct InterimTextPayload {
    pub session_id: String,
    pub text: String,
}

/// Payload of `opendictate_session_finished`.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct SessionFinishedPayload {
    pub session_id: String,
    pub text: String,
    pub raw_text: String,
    pub status: String,
    pub auto_send: bool,
}

/// Payload of `opendictate_session_cancelled` and `opendictate_session_released`.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct SessionCancelledPayload {
    pub session_id: String,
}

/// Payload of `opendictate_session_reserved`.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct SessionReservedPayload {
    pub session_id: String,
    pub app_name: String,
    pub accent_color: String,
}

/// Deterministic parser for GVariant string tuples emitted by gdbus monitor.
/// Extracts arguments inside parenthetical expressions, handling single quotes,
/// double quotes, and standard escape characters without relying on fragile string splitting.
pub fn parse_gvariant_tuple(line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    // Scan forward to the opening parenthesis of the tuple
    while i < chars.len() && chars[i] != '(' {
        i += 1;
    }
    if i >= chars.len() {
        return args;
    }
    i += 1; // Advance past '('

    while i < chars.len() {
        // Skip whitespace and argument delimiters
        while i < chars.len() && (chars[i].is_whitespace() || chars[i] == ',') {
            i += 1;
        }
        if i >= chars.len() || chars[i] == ')' {
            break;
        }

        let quote = chars[i];
        if quote == '\'' || quote == '"' {
            i += 1;
            let mut arg_val = String::new();
            while i < chars.len() {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 1;
                    match chars[i] {
                        'n' => arg_val.push('\n'),
                        'r' => arg_val.push('\r'),
                        't' => arg_val.push('\t'),
                        '\\' => arg_val.push('\\'),
                        '\'' => arg_val.push('\''),
                        '"' => arg_val.push('"'),
                        other => {
                            arg_val.push('\\');
                            arg_val.push(other);
                        }
                    }
                } else if chars[i] == quote {
                    i += 1;
                    break;
                } else {
                    arg_val.push(chars[i]);
                }
                i += 1;
            }
            args.push(arg_val);
        } else {
            // Unquoted token (number, boolean, or identifier)
            let mut token = String::new();
            while i < chars.len() && chars[i] != ',' && chars[i] != ')' && !chars[i].is_whitespace() {
                token.push(chars[i]);
                i += 1;
            }
            args.push(token);
        }
    }
    args
}

fn opendictate_state_candidates() -> Vec<std::path::PathBuf> {
    let mut paths = Vec::with_capacity(2);
    if let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        if !runtime_dir.is_empty() {
            paths.push(std::path::PathBuf::from(runtime_dir).join("opendictate_state.json"));
        }
    }
    paths.push(std::path::PathBuf::from("/tmp/opendictate_state.json"));
    paths
}

fn parse_autosend_from_json(state_content: &str) -> bool {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(state_content) {
        if let Some(enabled) = val.get("autosend_enabled").and_then(|v| v.as_bool()) {
            return enabled;
        }
        if let Some(enabled) = val.get("config").and_then(|c| c.get("auto_send")).and_then(|v| v.as_bool()) {
            return enabled;
        }
    }
    false
}

/// Reads `$XDG_RUNTIME_DIR/opendictate_state.json` (or `/tmp/opendictate_state.json`) to
/// asynchronously detect if OpenDictate has auto_send enabled.
pub async fn check_opendictate_autosend() -> bool {
    for path in opendictate_state_candidates() {
        if let Ok(state_content) = tokio::fs::read_to_string(&path).await {
            return parse_autosend_from_json(&state_content);
        }
    }
    false
}

/// Synchronously reads `$XDG_RUNTIME_DIR/opendictate_state.json` (or `/tmp/opendictate_state.json`)
/// to detect if OpenDictate has auto_send enabled.
pub fn check_opendictate_autosend_sync() -> bool {
    for path in opendictate_state_candidates() {
        if let Ok(state_content) = std::fs::read_to_string(&path) {
            return parse_autosend_from_json(&state_content);
        }
    }
    false
}

/// Spawns a background listener for org.kirulab.OpenDictate D-Bus signals via gdbus monitor.
pub fn spawn_dbus_monitor(tx: broadcast::Sender<DaemonEvent>, cancel: CancellationToken) {
    if crate::desktop::resolve_executable("gdbus").is_none()
        || std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none_or(|v| v.is_empty())
    {
        crate::logger::log(
            "INFO",
            "opendictate",
            "gdbus or DBUS_SESSION_BUS_ADDRESS unavailable; skipping OpenDictate D-Bus monitor",
        );
        return;
    }
    tauri::async_runtime::spawn(async move {
        crate::logger::log("DEBUG", "opendictate", "Starting persistent gdbus monitor for org.kirulab.OpenDictate...");

        loop {
            if cancel.is_cancelled() {
                break;
            }

            let mut child = match tokio::process::Command::new("gdbus")
                .arg("monitor")
                .arg("--session")
                .arg("--dest")
                .arg("org.kirulab.OpenDictate")
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    crate::logger::log("WARN", "opendictate", &format!("Failed to start gdbus monitor process: {}. Retrying in 15s...", e));
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        _ = tokio::time::sleep(tokio::time::Duration::from_secs(15)) => continue,
                    }
                }
            };

            if let Some(stdout) = child.stdout.take() {
                let mut reader = tokio::io::BufReader::new(stdout).lines();
                loop {
                    tokio::select! {
                        _ = cancel.cancelled() => {
                            let _ = child.kill().await;
                            return;
                        }
                        line_res = reader.next_line() => {
                            match line_res {
                                Ok(Some(line)) => {
                                    if !line.contains("org.kirulab.OpenDictate") {
                                        continue;
                                    }

                                    if line.contains(".SessionStarted") {
                                        let args = parse_gvariant_tuple(&line);
                                        if let Some(session_id) = args.first() {
                                            crate::logger::log("INFO", "opendictate", &format!("D-Bus SessionStarted: {}", session_id));
                                            let _ = tx.send(DaemonEvent::opendictate_session_started(&SessionStartedPayload {
                                                session_id: session_id.clone(),
                                            }));
                                        }
                                    } else if line.contains(".InterimText") {
                                        let args = parse_gvariant_tuple(&line);
                                        if args.len() >= 2 {
                                            let session_id = args[0].clone();
                                            let text = args[1].clone();
                                            crate::logger::log("DEBUG", "opendictate", &format!("D-Bus InterimText: '{}' ({})", text, session_id));
                                            let _ = tx.send(DaemonEvent::opendictate_interim_text(&InterimTextPayload { session_id, text }));
                                        }
                                    } else if line.contains(".SessionFinished") {
                                        let args = parse_gvariant_tuple(&line);
                                        if !args.is_empty() {
                                            let session_id = args[0].clone();
                                            let raw_text = args.get(1).cloned().unwrap_or_default();
                                            let processed_text = args.get(2).cloned().unwrap_or_default();
                                            let status = args.get(3).cloned().unwrap_or_else(|| "ok".to_string());
                                            
                                            // Prefer LLM-processed text if non-empty, otherwise use raw transcript
                                            let final_text = if !processed_text.trim().is_empty() {
                                                processed_text.clone()
                                            } else {
                                                raw_text.clone()
                                            };

                                            let auto_send = check_opendictate_autosend().await;
                                            crate::logger::log("INFO", "opendictate", &format!("D-Bus SessionFinished: '{}' ({}) status={} auto_send={}", final_text, session_id, status, auto_send));
                                            let _ = tx.send(DaemonEvent::opendictate_session_finished(&SessionFinishedPayload {
                                                session_id,
                                                text: final_text,
                                                raw_text,
                                                status,
                                                auto_send,
                                            }));
                                        }
                                    } else if line.contains(".SessionCancelled") {
                                        let args = parse_gvariant_tuple(&line);
                                        if let Some(session_id) = args.first() {
                                            crate::logger::log("INFO", "opendictate", &format!("D-Bus SessionCancelled: {}", session_id));
                                            let _ = tx.send(DaemonEvent::opendictate_session_cancelled(&SessionCancelledPayload {
                                                session_id: session_id.clone(),
                                            }));
                                        }
                                    } else if line.contains(".SessionReservationReleased") {
                                        let args = parse_gvariant_tuple(&line);
                                        if let Some(session_id) = args.first() {
                                            crate::logger::log("INFO", "opendictate", &format!("D-Bus SessionReservationReleased: {}", session_id));
                                            let _ = tx.send(DaemonEvent::opendictate_session_released(&SessionCancelledPayload {
                                                session_id: session_id.clone(),
                                            }));
                                        }
                                    } else if line.contains(".SessionReserved") {
                                        let args = parse_gvariant_tuple(&line);
                                        if args.len() >= 3 {
                                            let session_id = args[0].clone();
                                            let app_name = args[1].clone();
                                            let accent_color = args[2].clone();
                                            crate::logger::log("INFO", "opendictate", &format!("D-Bus SessionReserved: sid={} app={} color={}", session_id, app_name, accent_color));
                                            let _ = tx.send(DaemonEvent::opendictate_session_reserved(&SessionReservedPayload {
                                                session_id,
                                                app_name,
                                                accent_color,
                                            }));
                                        }
                                    }
                                }
                                Ok(None) | Err(_) => {
                                    crate::logger::log("WARN", "opendictate", "gdbus monitor stream closed. Reconnecting...");
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(15)) => continue,
            }
        }
    });
}

/// Daemon command `reserve_voice_capture`: arms an OpenDictate session for this client.
pub fn reserve_voice_capture(request: VoiceReservationRequest) -> VoiceReservationResult {
    let agent_id = request.agent_id.clone().unwrap_or_else(|| "chat".to_string());
    let session_id = request.session_id.unwrap_or_else(|| {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        format!("oneship-{}-{}", agent_id, ts)
    });

    if crate::desktop::resolve_executable("gdbus").is_none() {
        return VoiceReservationResult {
            success: false,
            service_available: false,
            session_id,
            agent_id: Some(agent_id),
            message: "OpenDictate unavailable: gdbus not installed on host".into(),
        };
    }

    let app_name = request.app_name.unwrap_or_else(|| {
        format!("ONE-SHIP // {}", agent_id)
    });

    let accent_color = request.accent_color.unwrap_or_else(|| "#00ff9d".to_string());
    let hide_bubble = request.hide_bubble.unwrap_or(true);

    let context_arg = format!(
        "{{'session_id': <'{}'>, 'app_name': <'{}'>, 'accent_color': <'{}'>, 'hide_bubble': <{}>}}",
        session_id, app_name, accent_color, hide_bubble
    );

    crate::logger::log(
        "INFO",
        "opendictate",
        &format!("Arming reservation via D-Bus: session={} app='{}' hide_bubble={}", session_id, app_name, hide_bubble),
    );

    let gdbus_call = Command::new("gdbus")
        .arg("call")
        .arg("--session")
        .arg("--dest")
        .arg("org.kirulab.OpenDictate")
        .arg("--object-path")
        .arg("/org/kirulab/OpenDictate")
        .arg("--method")
        .arg("org.kirulab.OpenDictate.ReserveCaptureSession")
        .arg(&context_arg)
        .output();

    match gdbus_call {
        Ok(out) if out.status.success() => {
            let stdout_str = String::from_utf8_lossy(&out.stdout);
            let args = parse_gvariant_tuple(&stdout_str);
            let returned_sid = args.first().cloned().unwrap_or_default();
            if returned_sid.trim().is_empty() {
                crate::logger::log(
                    "WARN",
                    "opendictate",
                    "OpenDictate returned empty session_id (daemon busy or recording in progress)",
                );
                VoiceReservationResult {
                    success: false,
                    service_available: true,
                    session_id: String::new(),
                    agent_id: Some(agent_id),
                    message: "OpenDictate is busy or actively recording speech".into(),
                }
            } else {
                VoiceReservationResult {
                    success: true,
                    service_available: true,
                    session_id: returned_sid,
                    agent_id: Some(agent_id),
                    message: "Reserved successfully in OpenDictate".into(),
                }
            }
        }
        Ok(out) => {
            let stderr_str = String::from_utf8_lossy(&out.stderr);
            crate::logger::log(
                "DEBUG",
                "opendictate",
                &format!("OpenDictate ReserveCaptureSession unavailable/error: {}", stderr_str.trim()),
            );
            VoiceReservationResult {
                success: false,
                service_available: false,
                session_id,
                agent_id: Some(agent_id),
                message: format!("OpenDictate unavailable: {}", stderr_str.trim()),
            }
        }
        Err(e) => {
            crate::logger::log(
                "WARN",
                "opendictate",
                &format!("Failed to execute gdbus call for OpenDictate: {}", e),
            );
            VoiceReservationResult {
                success: false,
                service_available: false,
                session_id,
                agent_id: Some(agent_id),
                message: format!("Failed to invoke gdbus: {}", e),
            }
        }
    }
}

/// Arguments of the daemon command `cancel_voice_capture`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CancelVoiceCaptureArgs {
    pub session_id: String,
}

/// Daemon command `cancel_voice_capture`: releases a reservation; `true` when OpenDictate
/// acknowledged it.
pub fn cancel_voice_capture(session_id: String) -> bool {
    crate::logger::log("INFO", "opendictate", &format!("Releasing OpenDictate reservation: {}", session_id));
    let arg = format!("'{}'", session_id);
    let out = Command::new("gdbus")
        .arg("call")
        .arg("--session")
        .arg("--dest")
        .arg("org.kirulab.OpenDictate")
        .arg("--object-path")
        .arg("/org/kirulab/OpenDictate")
        .arg("--method")
        .arg("org.kirulab.OpenDictate.ReleaseReservedSession")
        .arg(&arg)
        .output();

    match out {
        Ok(o) if o.status.success() => true,
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr);
            crate::logger::log("WARN", "opendictate", &format!("ReleaseReservedSession error: {}", err.trim()));
            false
        }
        Err(e) => {
            crate::logger::log("WARN", "opendictate", &format!("Failed to run ReleaseReservedSession: {}", e));
            false
        }
    }
}

/// Real-time operational state queried from OpenDictate D-Bus interface.
#[derive(Serialize, Deserialize, Clone, Debug, Default, ts_rs::TS)]
#[ts(export)]
pub struct OpenDictateStatus {
    pub state: String,
    pub is_busy: bool,
    pub active_dbus_session: Option<String>,
    pub reserved_session: Option<String>,
    pub is_available: bool,
    pub autosend_enabled: bool,
}

/// Daemon command `get_opendictate_status`, read over D-Bus.
pub fn get_opendictate_status() -> OpenDictateStatus {
    crate::logger::log("DEBUG", "opendictate", "Querying OpenDictate runtime status via D-Bus");
    let autosend = check_opendictate_autosend_sync();
    if crate::desktop::resolve_executable("gdbus").is_none() {
        return OpenDictateStatus {
            is_available: false,
            autosend_enabled: autosend,
            ..Default::default()
        };
    }
    let out = Command::new("gdbus")
        .arg("call")
        .arg("--session")
        .arg("--dest")
        .arg("org.kirulab.OpenDictate")
        .arg("--object-path")
        .arg("/org/kirulab/OpenDictate")
        .arg("--method")
        .arg("org.kirulab.OpenDictate.GetStatus")
        .output();

    match out {
        Ok(o) if o.status.success() => {
            let stdout_str = String::from_utf8_lossy(&o.stdout);
            let args = parse_gvariant_tuple(&stdout_str);
            if let Some(json_str) = args.first() {
                if let Ok(mut status) = serde_json::from_str::<OpenDictateStatus>(json_str) {
                    status.is_available = true;
                    if !status.autosend_enabled {
                        status.autosend_enabled = autosend;
                    }
                    return status;
                }
            }
            OpenDictateStatus {
                is_available: true,
                autosend_enabled: autosend,
                ..Default::default()
            }
        }
        _ => OpenDictateStatus {
            is_available: false,
            autosend_enabled: autosend,
            ..Default::default()
        },
    }
}

