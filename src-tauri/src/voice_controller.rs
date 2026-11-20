//! Global Voice Controller powered by `models/gemini-3.8-live-extended-thinking` (Spec 36).
//!
//! Bridges Push-to-Talk PCM16 audio from the ONE-SHIP webview (`/api/v1/voice/stream`)
//! to Google's `BidiGenerateContent` Live API WebSocket, keeping the API key isolated
//! in `oneship-daemon` and executing non-blocking UI & agent orchestration tools.

use std::process::Stdio;
use std::sync::{Arc, OnceLock, RwLock};
use axum::extract::ws::{Message as AxumWsMessage, WebSocket, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::Response;
use base64::Engine as _;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, Mutex, Notify};
use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;
use ts_rs::TS;

use crate::api::ApiError;
use crate::config::get_voice_controller_config;
use crate::conversation::{create_agent_conversation, CreateAgentConversationArgs};
use crate::events::DaemonEvent;
use crate::kopeng::skill::get_project_skills;
use crate::registry::get_projects;
use crate::supervisor::{interrupt_agent, launch_agent, prompt_agent, stop_agent};
use crate::ui_state::{dispatch_ui_action, get_ui_registry, get_ui_state, UiAction};

const GEMINI_LIVE_WS_BASE: &str =
    "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent";
const PW_PLAY_PREROLL_SILENCE_BYTES: usize = 4800; // 100 ms at 24 kHz, 16-bit mono
const VOICE_TEST_TIMEOUT_SECONDS: u64 = 15;
const OUTBOUND_QUEUE_CAPACITY_COUNT: usize = 128;

/// Strongly-typed errors for the Global Voice Controller.
#[derive(Debug, thiserror::Error)]
pub enum VoiceControllerError {
    #[error("Voice controller is disabled in global configuration")]
    Disabled,

    #[error("Google AI Studio API key is not configured in [plugins.voice_controller]")]
    MissingApiKey,

    #[error("Invalid voice controller parameter: {0}")]
    InvalidConfig(String),

    #[error("Project {0} not found")]
    ProjectNotFound(i64),

    #[error("Tool error: {0}")]
    Tool(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Gemini Live WebSocket connection failed: {0}")]
    WebSocketConnect(String),

    #[error("Internal task error: {0}")]
    Task(String),
}

impl ApiError for VoiceControllerError {
    fn status(&self) -> StatusCode {
        match self {
            Self::Disabled | Self::MissingApiKey => StatusCode::PRECONDITION_FAILED,
            Self::InvalidConfig(_) | Self::Tool(_) => {
                StatusCode::BAD_REQUEST
            }
            Self::ProjectNotFound(_) => StatusCode::NOT_FOUND,
            Self::WebSocketConnect(_) => StatusCode::BAD_GATEWAY,
            Self::Io(_) | Self::Task(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::Disabled => "VoiceControllerError::Disabled",
            Self::MissingApiKey => "VoiceControllerError::MissingApiKey",
            Self::InvalidConfig(_) => "VoiceControllerError::InvalidConfig",
            Self::ProjectNotFound(_) => "VoiceControllerError::ProjectNotFound",
            Self::Tool(_) => "VoiceControllerError::Tool",
            Self::Io(_) => "VoiceControllerError::Io",
            Self::WebSocketConnect(_) => "VoiceControllerError::WebSocketConnect",
            Self::Task(_) => "VoiceControllerError::Task",
        }
    }
}

fn validate_project_id(project_id: i64) -> Result<usize, VoiceControllerError> {
    if project_id <= 0 {
        return Err(VoiceControllerError::InvalidConfig(format!(
            "project_id must be a positive integer, got {project_id}"
        )));
    }
    usize::try_from(project_id).map_err(|_| {
        VoiceControllerError::InvalidConfig(format!("project_id {project_id} exceeds usize range"))
    })
}

/// Runtime state of the Global Voice Controller.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum VoiceControllerState {
    Idle,
    Connecting,
    Listening,
    Thinking,
    Speaking,
    Error,
}

/// Status payload returned by `get_voice_controller_status` and broadcast as `voice-controller-state`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct VoiceControllerStatus {
    pub configured: bool,
    pub enabled: bool,
    pub state: VoiceControllerState,
    pub thinking_level: String,
    pub last_error: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ClientControlMessage {
    action: String,
}

struct ManagerInner {
    state: VoiceControllerState,
    last_error: Option<String>,
}

/// Thread-safe, poison-resistant coordinator for the Global Voice Controller.
pub struct VoiceControllerManager {
    inner: RwLock<ManagerInner>,
}

static VOICE_CONTROLLER_MANAGER: OnceLock<VoiceControllerManager> = OnceLock::new();

/// Returns the global [`VoiceControllerManager`] singleton.
pub fn get_voice_controller_manager() -> &'static VoiceControllerManager {
    VOICE_CONTROLLER_MANAGER.get_or_init(|| VoiceControllerManager {
        inner: RwLock::new(ManagerInner {
            state: VoiceControllerState::Idle,
            last_error: None,
        }),
    })
}

impl VoiceControllerManager {
    fn read_inner(&self) -> std::sync::RwLockReadGuard<'_, ManagerInner> {
        match self.inner.read() {
            Ok(g) => g,
            Err(poisoned) => {
                crate::logger::log("WARN", "voice_controller", "Recovering poisoned read lock");
                poisoned.into_inner()
            }
        }
    }

    fn write_inner(&self) -> std::sync::RwLockWriteGuard<'_, ManagerInner> {
        match self.inner.write() {
            Ok(g) => g,
            Err(poisoned) => {
                crate::logger::log("WARN", "voice_controller", "Recovering poisoned write lock");
                poisoned.into_inner()
            }
        }
    }

    /// Returns current configuration and runtime status without exposing the API key.
    pub fn status(&self) -> VoiceControllerStatus {
        let cfg = get_voice_controller_config();
        let inner = self.read_inner();
        VoiceControllerStatus {
            configured: cfg.gemini_api_key.is_some(),
            enabled: cfg.enabled,
            state: inner.state,
            thinking_level: cfg.thinking_level,
            last_error: inner.last_error.clone(),
        }
    }

    fn set_state(&self, state: VoiceControllerState, last_error: Option<String>) {
        {
            let mut inner = self.write_inner();
            inner.state = state;
            inner.last_error = last_error;
        }
        let snapshot = self.status();
        crate::supervisor::get_supervisor()
            .emit_event(DaemonEvent::voice_controller_state(&snapshot));
    }
}

/// Daemon command handler for `get_voice_controller_status`.
pub fn get_voice_controller_status() -> VoiceControllerStatus {
    get_voice_controller_manager().status()
}

/// Arguments for `test_voice_controller_voice`.
#[derive(Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TestVoiceArgs {
    pub voice_name: String,
    pub model: Option<String>,
    pub persona_style: Option<String>,
}

/// Daemon command `test_voice_controller_voice`: opens an ephemeral Gemini Live session with the
/// selected voice, sends a short greeting prompt, and plays the response via `pw-play`.
pub async fn test_voice_controller_voice(args: TestVoiceArgs) -> Result<(), VoiceControllerError> {
    let cfg = get_voice_controller_config();
    let api_key = cfg.gemini_api_key.clone().ok_or(VoiceControllerError::MissingApiKey)?;
    let model = match args.model {
        Some(m) => {
            if matches!(
                m.as_str(),
                "gemini-3.8-live" | "gemini-3.8-live-extended-thinking"
            ) {
                m
            } else {
                return Err(VoiceControllerError::InvalidConfig(format!(
                    "Unsupported voice controller model '{m}'"
                )));
            }
        }
        None => cfg.model.clone(),
    };
    let persona_style = match args.persona_style {
        Some(p) => {
            if matches!(p.as_str(), "belta" | "neutral") {
                p
            } else {
                return Err(VoiceControllerError::InvalidConfig(format!(
                    "Unsupported persona_style '{p}'"
                )));
            }
        }
        None => cfg.persona_style.clone(),
    };
    let voice_name = if crate::config::GEMINI_LIVE_VOICES.contains(&args.voice_name.as_str()) {
        args.voice_name
    } else {
        return Err(VoiceControllerError::InvalidConfig(format!(
            "Unsupported voice_name '{}'",
            args.voice_name
        )));
    };

    let ws_url = format!("{GEMINI_LIVE_WS_BASE}?key={api_key}");
    let (mut gemini_ws, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .map_err(|e| VoiceControllerError::WebSocketConnect(e.to_string()))?;

    let mut test_cfg = cfg.clone();
    test_cfg.model = model.clone();
    test_cfg.persona_style = persona_style.clone();
    test_cfg.voice_name = voice_name.clone();
    let setup_payload = build_gemini_setup_payload(&test_cfg, None, None);
    gemini_ws
        .send(TungsteniteMessage::Text(setup_payload.to_string().into()))
        .await
        .map_err(|e| VoiceControllerError::WebSocketConnect(e.to_string()))?;

    let mut sink = PwPlaySink::default();
    sink.pre_warm();

    let test_prompt = if persona_style == "belta" {
        "Say a brief 1-sentence ship-bridge voice check greeting as the ONE-SHIP Ship Brain."
    } else {
        "Say a brief 1-sentence voice check greeting as the ONE-SHIP assistant."
    };

    let mut prompt_sent = false;
    let run_result = tokio::time::timeout(
        std::time::Duration::from_secs(VOICE_TEST_TIMEOUT_SECONDS),
        async {
            while let Some(msg_res) = gemini_ws.next().await {
                let msg = msg_res.map_err(|e| VoiceControllerError::WebSocketConnect(e.to_string()))?;
                let text_str = match msg {
                    TungsteniteMessage::Text(t) => t.to_string(),
                    TungsteniteMessage::Binary(b) => match String::from_utf8(b.to_vec()) {
                        Ok(s) => s,
                        Err(_) => continue,
                    },
                    TungsteniteMessage::Close(_) => break,
                    _ => continue,
                };
                let Ok(val) = serde_json::from_str::<Value>(&text_str) else {
                    continue;
                };
                if let Some(err_obj) = val.get("error") {
                    return Err(VoiceControllerError::WebSocketConnect(err_obj.to_string()));
                }
                if val.get("setupComplete").is_some() && !prompt_sent {
                    prompt_sent = true;
                    let turn = json!({
                        "clientContent": {
                            "turns": [{
                                "role": "user",
                                "parts": [{ "text": test_prompt }]
                            }],
                            "turnComplete": true
                        }
                    });
                    gemini_ws
                        .send(TungsteniteMessage::Text(turn.to_string().into()))
                        .await
                        .map_err(|e| VoiceControllerError::WebSocketConnect(e.to_string()))?;
                    continue;
                }
                if let Some(server_content) = val.get("serverContent") {
                    if let Some(parts) = server_content
                        .get("modelTurn")
                        .and_then(|m| m.get("parts"))
                        .and_then(|p| p.as_array())
                    {
                        for part in parts {
                            if let Some(inline) = part.get("inlineData") {
                                let mime = inline.get("mimeType").and_then(|m| m.as_str()).unwrap_or("");
                                if mime.starts_with("audio/pcm") {
                                    if let Some(data_b64) = inline.get("data").and_then(|d| d.as_str()) {
                                        if let Ok(pcm_bytes) =
                                            base64::engine::general_purpose::STANDARD.decode(data_b64)
                                        {
                                            sink.push_pcm(&pcm_bytes);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if server_content.get("turnComplete").and_then(|v| v.as_bool()) == Some(true)
                        && val.get("interactionStatus").is_none()
                    {
                        break;
                    }
                }
                if val.get("interactionStatus").and_then(|v| v.as_str()) == Some("IDLE") {
                    break;
                }
            }
            Ok::<(), VoiceControllerError>(())
        },
    )
    .await;

    let _ = gemini_ws.close(None).await;
    sink.drain_and_finish().await;

    match run_result {
        Ok(res) => res,
        Err(_) => Err(VoiceControllerError::Task(
            "Voice test timed out waiting for Gemini Live response".to_string(),
        )),
    }
}

/// Axum WebSocket upgrade handler for `/api/v1/voice/stream`.
pub async fn ws_stream_handler(ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(handle_voice_socket)
}

const GEMINI_SETUP_TIMEOUT_SECONDS: u64 = 10;

type GeminiWsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn try_open_gemini_setup(
    ws_url: &str,
    setup_payload: Value,
) -> Result<GeminiWsStream, VoiceControllerError> {
    let (mut ws, _) = tokio_tungstenite::connect_async(ws_url)
        .await
        .map_err(|e| VoiceControllerError::WebSocketConnect(e.to_string()))?;
    let setup_text = setup_payload.to_string();
    crate::logger::log(
        "INFO",
        "voice_controller",
        &format!("Sending Gemini Live setup payload ({} bytes)", setup_text.len()),
    );
    ws.send(TungsteniteMessage::Text(setup_text.into()))
        .await
        .map_err(|e| VoiceControllerError::WebSocketConnect(e.to_string()))?;

    let wait_res = tokio::time::timeout(
        std::time::Duration::from_secs(GEMINI_SETUP_TIMEOUT_SECONDS),
        async {
            while let Some(msg_res) = ws.next().await {
                let msg = msg_res
                    .map_err(|e| VoiceControllerError::WebSocketConnect(e.to_string()))?;
                let text_str = match msg {
                    TungsteniteMessage::Text(t) => t.to_string(),
                    TungsteniteMessage::Binary(b) => match String::from_utf8(b.to_vec()) {
                        Ok(s) => s,
                        Err(_) => continue,
                    },
                    TungsteniteMessage::Close(frame) => {
                        return Err(VoiceControllerError::WebSocketConnect(format!(
                            "Gemini Live closed before setupComplete: {frame:?}"
                        )));
                    }
                    _ => continue,
                };
                let Ok(val) = serde_json::from_str::<Value>(&text_str) else {
                    continue;
                };
                if let Some(err_obj) = val.get("error") {
                    return Err(VoiceControllerError::WebSocketConnect(err_obj.to_string()));
                }
                if val.get("setupComplete").is_some() {
                    return Ok(ws);
                }
            }
            Err(VoiceControllerError::WebSocketConnect(
                "Gemini Live WebSocket ended before setupComplete".to_string(),
            ))
        },
    )
    .await;

    match wait_res {
        Ok(res) => res,
        Err(_) => Err(VoiceControllerError::WebSocketConnect(
            "Timed out waiting for Gemini Live setupComplete".to_string(),
        )),
    }
}

async fn open_gemini_live_and_await_setup(
    ws_url: &str,
    cfg: &crate::config::VoiceControllerConfig,
    session_id: &str,
    resumption_handle: Option<&str>,
    brief: Option<&str>,
) -> Result<GeminiWsStream, VoiceControllerError> {
    if let Some(handle) = resumption_handle {
        let payload = build_gemini_setup_payload(cfg, Some(handle), brief);
        match try_open_gemini_setup(ws_url, payload).await {
            Ok(ws) => {
                crate::logger::log(
                    "INFO",
                    "voice_controller",
                    &format!("Connected to Gemini Live (harness session={session_id}, resumption=true)"),
                );
                return Ok(ws);
            }
            Err(err) => {
                crate::logger::log(
                    "WARN",
                    "voice_controller",
                    &format!(
                        "Resumption handle rejected before setupComplete ({err}); clearing handle and falling back to Context Brief"
                    ),
                );
                if let Err(clear_err) =
                    crate::voice_harness::save_resumption_handle(session_id.to_string(), None).await
                {
                    crate::logger::log(
                        "WARN",
                        "voice_controller",
                        &format!("Failed to clear expired resumption handle: {clear_err}"),
                    );
                }
            }
        }
    }

    let fresh_payload = build_gemini_setup_payload(cfg, None, brief);
    let ws = try_open_gemini_setup(ws_url, fresh_payload).await?;
    crate::logger::log(
        "INFO",
        "voice_controller",
        &format!("Connected to Gemini Live (harness session={session_id}, resumption=false, brief_hydrated={})", brief.is_some()),
    );
    Ok(ws)
}

async fn handle_voice_socket(local_ws: WebSocket) {
    let mgr = get_voice_controller_manager();
    let cfg = get_voice_controller_config();
    crate::logger::log("INFO", "voice_controller", "Local WebSocket /api/v1/voice/stream connected");

    if !cfg.enabled {
        mgr.set_state(
            VoiceControllerState::Error,
            Some(VoiceControllerError::Disabled.to_string()),
        );
        return;
    }

    let Some(api_key) = cfg.gemini_api_key.clone() else {
        mgr.set_state(
            VoiceControllerState::Error,
            Some(VoiceControllerError::MissingApiKey.to_string()),
        );
        return;
    };

    mgr.set_state(VoiceControllerState::Connecting, None);

    let session = match crate::voice_harness::load_or_init_session().await {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("Failed to load voice harness session: {e}");
            crate::logger::log("ERROR", "voice_controller", &msg);
            mgr.set_state(VoiceControllerState::Error, Some(msg));
            return;
        }
    };
    let brief = match crate::voice_harness::compile_session_brief(session.id.clone(), &cfg).await {
        Ok(b) => Some(b),
        Err(e) => {
            crate::logger::log(
                "WARN",
                "voice_controller",
                &format!("Failed to compile voice session brief: {e}"),
            );
            None
        }
    };
    let resumption_handle = crate::voice_harness::active_resumption_handle(&session).map(str::to_string);
    let active_session_id = Arc::new(Mutex::new(session.id.clone()));

    let ws_url = format!("{GEMINI_LIVE_WS_BASE}?key={api_key}");
    let gemini_ws = match open_gemini_live_and_await_setup(
        &ws_url,
        &cfg,
        &session.id,
        resumption_handle.as_deref(),
        brief.as_deref(),
    )
    .await
    {
        Ok(ws) => ws,
        Err(e) => {
            let msg = e.to_string();
            crate::logger::log("ERROR", "voice_controller", &msg);
            mgr.set_state(VoiceControllerState::Error, Some(msg));
            return;
        }
    };

    let (mut local_tx, mut local_rx) = local_ws.split();
    let (mut gemini_tx, mut gemini_rx) = gemini_ws.split();

    let (gemini_out_tx, mut gemini_out_rx) =
        mpsc::channel::<TungsteniteMessage>(OUTBOUND_QUEUE_CAPACITY_COUNT);
    let (local_out_tx, mut local_out_rx) =
        mpsc::channel::<AxumWsMessage>(OUTBOUND_QUEUE_CAPACITY_COUNT);

    let gemini_writer = tokio::spawn(async move {
        while let Some(msg) = gemini_out_rx.recv().await {
            if gemini_tx.send(msg).await.is_err() {
                break;
            }
        }
    });

    let local_writer = tokio::spawn(async move {
        while let Some(msg) = local_out_rx.recv().await {
            if local_tx.send(msg).await.is_err() {
                break;
            }
        }
    });

    let paused_mpris = Arc::new(Mutex::new(Vec::<String>::new()));
    let pw_record_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>> =
        Arc::new(Mutex::new(None));
    let pw_play_sink = Arc::new(Mutex::new(PwPlaySink::default()));
    let setup_ready = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let setup_notify = Arc::new(Notify::new());
    let last_activity_epoch = Arc::new(std::sync::atomic::AtomicI64::new(
        chrono::Utc::now().timestamp(),
    ));

    #[derive(Default)]
    struct TurnAccumulator {
        project_id: Option<i64>,
        user_parts: Vec<String>,
        actions: Vec<String>,
        assistant_parts: Vec<String>,
    }

    let turn_acc = Arc::new(Mutex::new(TurnAccumulator::default()));

    let gemini_out_for_local = gemini_out_tx.clone();
    let local_out_for_pw = local_out_tx.clone();
    let paused_for_local = Arc::clone(&paused_mpris);
    let pw_handle_for_local = Arc::clone(&pw_record_handle);
    let pw_play_for_local = Arc::clone(&pw_play_sink);
    let setup_ready_for_local = Arc::clone(&setup_ready);
    let setup_notify_for_local = Arc::clone(&setup_notify);
    let activity_for_local = Arc::clone(&last_activity_epoch);
    let local_reader = tokio::spawn(async move {
        while let Some(Ok(msg)) = local_rx.next().await {
            match msg {
                AxumWsMessage::Text(txt) => {
                    if let Ok(ctrl) = serde_json::from_str::<ClientControlMessage>(&txt) {
                        activity_for_local.store(
                            chrono::Utc::now().timestamp(),
                            std::sync::atomic::Ordering::Release,
                        );
                        match ctrl.action.as_str() {
                            "ptt_start" => {
                                abort_pw_record(&pw_handle_for_local).await;
                                pw_play_for_local.lock().await.stop();
                                if !setup_ready_for_local.load(std::sync::atomic::Ordering::Acquire) {
                                    let _ = tokio::time::timeout(
                                        std::time::Duration::from_secs(5),
                                        setup_notify_for_local.notified(),
                                    )
                                    .await;
                                }
                                let newly_paused = pause_playing_mpris_players().await;
                                if !newly_paused.is_empty() {
                                    let mut guard = paused_for_local.lock().await;
                                    for name in newly_paused {
                                        if !guard.contains(&name) {
                                            guard.push(name);
                                        }
                                    }
                                }
                                // Stay in Connecting until pw-record delivers its first PCM frame so UI
                                // Listening state reflects actual active PipeWire capture.
                                let lease = match crate::audio_capture::try_acquire("voice_controller") {
                                    Ok(lease) => lease,
                                    Err(crate::audio_capture::CaptureError::Busy { owner }) => {
                                        let message = format!("Microphone is busy: {owner}");
                                        crate::logger::log("WARN", "voice_controller", &message);
                                        mgr.set_state(VoiceControllerState::Error, Some(message));
                                        continue;
                                    }
                                };
                                mgr.set_state(VoiceControllerState::Connecting, None);
                                let handle = spawn_pw_record_task(
                                    gemini_out_for_local.clone(),
                                    local_out_for_pw.clone(),
                                    mgr,
                                    lease,
                                );
                                *pw_handle_for_local.lock().await = Some(handle);
                            }
                            "ptt_stop" => {
                                abort_pw_record(&pw_handle_for_local).await;
                                pw_play_for_local.lock().await.pre_warm();
                                mgr.set_state(VoiceControllerState::Thinking, None);
                                let frame = json!({ "realtimeInput": { "activityEnd": {} } });
                                let _ = gemini_out_for_local
                                    .send(TungsteniteMessage::Text(frame.to_string().into()))
                                    .await;
                            }
                            "ptt_cancel" | "disconnect" => {
                                abort_pw_record(&pw_handle_for_local).await;
                                pw_play_for_local.lock().await.stop();
                                resume_mpris_players(&paused_for_local).await;
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                AxumWsMessage::Close(_) => break,
                _ => {}
            }
        }
    });

    let gemini_out_for_tools = gemini_out_tx.clone();
    let paused_for_gemini = Arc::clone(&paused_mpris);
    let pw_play_for_gemini = Arc::clone(&pw_play_sink);
    let session_id_for_gemini = Arc::clone(&active_session_id);
    let turn_acc_for_gemini = Arc::clone(&turn_acc);
    let activity_for_gemini = Arc::clone(&last_activity_epoch);
    let cfg_for_gemini = cfg.clone();

    async fn flush_turn_accumulator(
        acc: &Mutex<TurnAccumulator>,
        session_id_slot: &Mutex<String>,
        cfg: &crate::config::VoiceControllerConfig,
    ) {
        let (pid, user_text, actions_text, ai_text) = {
            let mut guard = acc.lock().await;
            let pid = guard.project_id.take();
            let u = guard.user_parts.join(" ").trim().to_string();
            let act = guard.actions.join("; ").trim().to_string();
            let a = guard.assistant_parts.join(" ").trim().to_string();
            guard.user_parts.clear();
            guard.actions.clear();
            guard.assistant_parts.clear();
            (pid, u, act, a)
        };
        if user_text.is_empty() && actions_text.is_empty() && ai_text.is_empty() {
            return;
        }
        let sid = session_id_slot.lock().await.clone();
        crate::voice_harness::record_turn(
            sid,
            pid,
            (!user_text.is_empty()).then_some(user_text),
            (!actions_text.is_empty()).then_some(actions_text),
            (!ai_text.is_empty()).then_some(ai_text),
            cfg,
        )
        .await;
    }

    let gemini_reader = tokio::spawn(async move {
        while let Some(msg_res) = gemini_rx.next().await {
            let msg = match msg_res {
                Ok(m) => m,
                Err(e) => {
                    crate::logger::log("ERROR", "voice_controller", &format!("Gemini Live WebSocket read error: {e}"));
                    mgr.set_state(VoiceControllerState::Error, Some(e.to_string()));
                    break;
                }
            };
            let text_str = match msg {
                TungsteniteMessage::Text(t) => t.to_string(),
                TungsteniteMessage::Binary(b) => match String::from_utf8(b.to_vec()) {
                    Ok(s) => s,
                    Err(_) => continue,
                },
                TungsteniteMessage::Close(frame) => {
                    crate::logger::log("WARN", "voice_controller", &format!("Gemini Live WebSocket closed: {:?}", frame));
                    if let Some(ref f) = frame {
                        if f.code != tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Normal {
                            mgr.set_state(
                                VoiceControllerState::Error,
                                Some(format!("Gemini Live closed ({}): {}", f.code, f.reason)),
                            );
                        }
                        let reason = f.reason.to_lowercase();
                        if reason.contains("not found")
                            || reason.contains("expired")
                            || reason.contains("inactivity")
                        {
                            let sid = session_id_for_gemini.lock().await.clone();
                            if let Err(e) = crate::voice_harness::save_resumption_handle(sid, None).await {
                                crate::logger::log(
                                    "WARN",
                                    "voice_controller",
                                    &format!("Failed to clear invalidated resumption handle: {e}"),
                                );
                            }
                        }
                    }
                    break;
                }
                _ => continue,
            };

            let Ok(val) = serde_json::from_str::<Value>(&text_str) else {
                crate::logger::log(
                    "WARN",
                    "voice_controller",
                    &format!("Non-JSON Gemini Live frame: {}", &text_str[..text_str.len().min(200)]),
                );
                continue;
            };

            activity_for_gemini.store(
                chrono::Utc::now().timestamp(),
                std::sync::atomic::Ordering::Release,
            );

            if let Some(obj) = val.as_object() {
                let unknown_keys: Vec<&str> = obj
                    .keys()
                    .map(|k| k.as_str())
                    .filter(|k| {
                        !matches!(
                            *k,
                            "error"
                                | "setupComplete"
                                | "serverContent"
                                | "interactionStatus"
                                | "toolCall"
                                | "toolCallCancellation"
                                | "goAway"
                                | "usageMetadata"
                                | "sessionResumptionUpdate"
                                | "voiceActivity"
                        )
                    })
                    .collect();
                if !unknown_keys.is_empty() {
                    crate::logger::log(
                        "INFO",
                        "voice_controller",
                        &format!("Gemini frame unknown keys={unknown_keys:?} payload={val}"),
                    );
                }
            }

            if let Some(err_obj) = val.get("error") {
                let msg = format!("Gemini Live error: {err_obj}");
                crate::logger::log("ERROR", "voice_controller", &msg);
                mgr.set_state(VoiceControllerState::Error, Some(msg));
                continue;
            }

            if let Some(usage) = val.get("usageMetadata") {
                let prompt_tokens = usage.get("promptTokenCount").and_then(|v| v.as_u64());
                let response_tokens = usage.get("responseTokenCount").and_then(|v| v.as_u64());
                let total_tokens = usage.get("totalTokenCount").and_then(|v| v.as_u64());
                crate::logger::log(
                    "INFO",
                    "voice_controller",
                    &format!(
                        "Gemini usageMetadata: prompt={prompt_tokens:?} response={response_tokens:?} total={total_tokens:?} details={usage}"
                    ),
                );
            }

            if let Some(resumption_update) = val.get("sessionResumptionUpdate") {
                let resumable = resumption_update
                    .get("resumable")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let new_handle = resumption_update
                    .get("newHandle")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                if resumable {
                    if let Some(handle) = new_handle {
                        let sid = session_id_for_gemini.lock().await.clone();
                        if let Err(e) = crate::voice_harness::save_resumption_handle(sid, Some(handle)).await {
                            crate::logger::log(
                                "WARN",
                                "voice_controller",
                                &format!("Failed to persist sessionResumptionUpdate handle: {e}"),
                            );
                        }
                    }
                }
            }

            if val.get("setupComplete").is_some() {
                crate::logger::log("INFO", "voice_controller", "Gemini Live setupComplete received");
                setup_ready.store(true, std::sync::atomic::Ordering::Release);
                setup_notify.notify_waiters();
                if mgr.status().state == VoiceControllerState::Connecting {
                    mgr.set_state(VoiceControllerState::Idle, None);
                }
                continue;
            }

            if let Some(server_content) = val.get("serverContent") {
                if server_content.get("interrupted").and_then(|v| v.as_bool()) == Some(true) {
                    pw_play_for_gemini.lock().await.stop();
                    let _ = local_out_tx
                        .try_send(AxumWsMessage::Text(json!({ "event": "interrupted" }).to_string().into()));
                    mgr.set_state(VoiceControllerState::Idle, None);
                }

                if let Some(in_tr) = server_content
                    .get("inputTranscription")
                    .and_then(|t| t.get("text"))
                    .and_then(|v| v.as_str())
                {
                    let trimmed = in_tr.trim();
                    if !trimmed.is_empty() {
                        crate::logger::log("INFO", "voice_controller", &format!("User (transcribed): {trimmed}"));
                        turn_acc_for_gemini.lock().await.user_parts.push(trimmed.to_string());
                    }
                }

                if let Some(out_tr) = server_content
                    .get("outputTranscription")
                    .and_then(|t| t.get("text"))
                    .and_then(|v| v.as_str())
                {
                    let trimmed = out_tr.trim();
                    if !trimmed.is_empty() {
                        crate::logger::log("INFO", "voice_controller", &format!("Gemini (spoken): {trimmed}"));
                        turn_acc_for_gemini.lock().await.assistant_parts.push(trimmed.to_string());
                    }
                }

                if let Some(parts) = server_content
                    .get("modelTurn")
                    .and_then(|m| m.get("parts"))
                    .and_then(|p| p.as_array())
                {
                    for part in parts {
                        if let Some(inline) = part.get("inlineData") {
                            let mime = inline.get("mimeType").and_then(|m| m.as_str()).unwrap_or("");
                            if mime.starts_with("audio/pcm") {
                                if let Some(data_b64) = inline.get("data").and_then(|d| d.as_str()) {
                                    if let Ok(pcm_bytes) =
                                        base64::engine::general_purpose::STANDARD.decode(data_b64)
                                    {
                                        mgr.set_state(VoiceControllerState::Speaking, None);
                                        let samples = downsample_pcm16(&pcm_bytes, 48);
                                        let _ = local_out_tx.try_send(AxumWsMessage::Text(
                                            json!({ "event": "output_waveform", "samples": samples })
                                                .to_string()
                                                .into(),
                                        ));
                                        pw_play_for_gemini.lock().await.push_pcm(&pcm_bytes);
                                    }
                                }
                            }
                        } else {
                            crate::logger::log(
                                "INFO",
                                "voice_controller",
                                &format!("Gemini modelTurn non-audio part: {part}"),
                            );
                        }
                    }
                }

                if server_content.get("generationComplete").and_then(|v| v.as_bool()) == Some(true) {
                    crate::logger::log("INFO", "voice_controller", "Gemini generationComplete=true");
                }

                if server_content.get("turnComplete").and_then(|v| v.as_bool()) == Some(true) {
                    crate::logger::log("INFO", "voice_controller", "Gemini turnComplete=true");
                    flush_turn_accumulator(
                        &turn_acc_for_gemini,
                        &session_id_for_gemini,
                        &cfg_for_gemini,
                    )
                    .await;
                    if val.get("interactionStatus").is_none() {
                        mgr.set_state(VoiceControllerState::Idle, None);
                        resume_mpris_players(&paused_for_gemini).await;
                    }
                }
            }

            if let Some(status) = val.get("interactionStatus").and_then(|v| v.as_str()) {
                crate::logger::log("INFO", "voice_controller", &format!("Gemini interactionStatus={status}"));
                if status == "IDLE" {
                    flush_turn_accumulator(
                        &turn_acc_for_gemini,
                        &session_id_for_gemini,
                        &cfg_for_gemini,
                    )
                    .await;
                    mgr.set_state(VoiceControllerState::Idle, None);
                    resume_mpris_players(&paused_for_gemini).await;
                } else if status == "IN_PROGRESS" && mgr.status().state == VoiceControllerState::Idle {
                    mgr.set_state(VoiceControllerState::Thinking, None);
                }
            }

            if let Some(cancel) = val.get("toolCallCancellation") {
                crate::logger::log("WARN", "voice_controller", &format!("Gemini toolCallCancellation: {cancel}"));
            }

            if let Some(go_away) = val.get("goAway") {
                crate::logger::log("WARN", "voice_controller", &format!("Gemini goAway: {go_away}"));
            }

            if let Some(tool_calls) = val
                .get("toolCall")
                .and_then(|tc| tc.get("functionCalls"))
                .and_then(|fc| fc.as_array())
            {
                mgr.set_state(VoiceControllerState::Thinking, None);
                let mut responses = Vec::with_capacity(tool_calls.len());
                for call in tool_calls {
                    let id = call.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let name = call.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let args = call.get("args").cloned().unwrap_or_else(|| json!({}));
                    crate::logger::log(
                        "INFO",
                        "voice_controller",
                        &format!("Executing toolCall '{name}' args={args}"),
                    );

                    let call_project_id = args.get("project_id").and_then(|v| v.as_i64());
                    let current_sid = session_id_for_gemini.lock().await.clone();
                    let (raw_result, maybe_new_sid) =
                        if name == "harness_scratchpad" || name == "harness_session_reset" {
                            crate::voice_harness::execute_harness_tool(
                                current_sid,
                                &name,
                                args.clone(),
                                &cfg_for_gemini,
                            )
                            .await
                        } else {
                            (
                                execute_core_tool(&current_sid, &name, args.clone()).await,
                                None,
                            )
                        };
                    if let Some(new_sid) = maybe_new_sid {
                        *session_id_for_gemini.lock().await = new_sid;
                    }

                    let raw_text = raw_result.to_string();
                    let raw_bytes = raw_text.len();
                    if raw_bytes > cfg_for_gemini.voice_tool_response_max_bytes {
                        crate::logger::log(
                            "WARN",
                            "voice_controller",
                            &format!(
                                "Tool '{name}' response ({raw_bytes} bytes) exceeded warning threshold ({} bytes); sending unclipped payload",
                                cfg_for_gemini.voice_tool_response_max_bytes
                            ),
                        );
                    }
                    let outcome_tag = if raw_result.get("error").is_some() {
                        "err"
                    } else {
                        "ok"
                    };
                    {
                        let mut acc = turn_acc_for_gemini.lock().await;
                        if acc.project_id.is_none() {
                            acc.project_id = call_project_id;
                        }
                        let detail = if name == "capability_execute" {
                            let dom = args.get("domain").and_then(|v| v.as_str()).unwrap_or("");
                            let act = args.get("action_id").and_then(|v| v.as_str()).unwrap_or("");
                            format!("{dom}.{act}->{outcome_tag}")
                        } else {
                            format!("{name}->{outcome_tag}")
                        };
                        acc.actions.push(detail);
                    }

                    crate::logger::log(
                        "INFO",
                        "voice_controller",
                        &format!("Tool '{name}' result ({raw_bytes} bytes)={raw_text}"),
                    );
                    responses.push(json!({
                        "id": id,
                        "name": name,
                        "response": raw_result
                    }));
                }
                let tool_resp = json!({
                    "toolResponse": {
                        "functionResponses": responses
                    }
                });
                let tool_resp_text = tool_resp.to_string();
                crate::logger::log(
                    "INFO",
                    "voice_controller",
                    &format!("Sending Gemini toolResponse batch ({} bytes)", tool_resp_text.len()),
                );
                if let Err(e) = gemini_out_for_tools
                    .send(TungsteniteMessage::Text(tool_resp_text.into()))
                    .await
                {
                    crate::logger::log(
                        "ERROR",
                        "voice_controller",
                        &format!("Failed to enqueue toolResponse to Gemini Live writer: {e}"),
                    );
                }
            }
        }
    });

    let idle_timeout_secs = cfg.idle_disconnect_seconds as i64;
    let activity_for_idle = Arc::clone(&last_activity_epoch);
    let idle_watcher = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            let now = chrono::Utc::now().timestamp();
            let last = activity_for_idle.load(std::sync::atomic::Ordering::Acquire);
            if mgr.status().state == VoiceControllerState::Idle && (now - last) >= idle_timeout_secs {
                crate::logger::log(
                    "INFO",
                    "voice_controller",
                    &format!("Hibernating idle Gemini Live WebSocket after {idle_timeout_secs}s (working session preserved)"),
                );
                break;
            }
        }
    });

    tokio::select! {
        _ = local_reader => {},
        _ = gemini_reader => {},
        _ = idle_watcher => {},
    }

    flush_turn_accumulator(&turn_acc, &active_session_id, &cfg).await;
    abort_pw_record(&pw_record_handle).await;
    pw_play_sink.lock().await.stop();
    resume_mpris_players(&paused_mpris).await;
    gemini_writer.abort();
    local_writer.abort();
    mgr.set_state(VoiceControllerState::Idle, None);
}

fn downsample_pcm16(bytes: &[u8], points_count: usize) -> Vec<f32> {
    let sample_count = bytes.len() / 2;
    if sample_count == 0 || points_count == 0 {
        return vec![0.0; points_count];
    }
    let step = (sample_count / points_count).max(1);
    let mut samples = Vec::with_capacity(points_count);
    for i in 0..points_count {
        let idx = (i * step).min(sample_count - 1);
        let raw = i16::from_le_bytes([bytes[idx * 2], bytes[idx * 2 + 1]]);
        samples.push((raw as f32) / 32768.0);
    }
    samples
}

#[derive(Default)]
struct PwPlaySink {
    tx: Option<mpsc::Sender<Vec<u8>>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl PwPlaySink {
    fn ensure_started(&mut self) -> Option<&mpsc::Sender<Vec<u8>>> {
        let needs_spawn = self.tx.as_ref().is_none_or(|t| t.is_closed());
        if needs_spawn {
            self.stop();
            let (tx, mut rx) = mpsc::channel::<Vec<u8>>(OUTBOUND_QUEUE_CAPACITY_COUNT);
            let handle = tokio::spawn(async move {
                let mut child = match tokio::process::Command::new("pw-play")
                    .args(["--raw", "--rate", "24000", "--channels", "1", "--format", "s16", "-"])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .kill_on_drop(true)
                    .spawn()
                {
                    Ok(c) => c,
                    Err(e) => {
                        crate::logger::log("ERROR", "voice_controller", &format!("Failed to spawn pw-play: {e}"));
                        return;
                    }
                };
                let Some(mut stdin) = child.stdin.take() else {
                    return;
                };
                while let Some(chunk) = rx.recv().await {
                    if stdin.write_all(&chunk).await.is_err() {
                        break;
                    }
                }
                drop(stdin);
                let _ = child.wait().await;
            });
            let _ = tx.try_send(vec![0u8; PW_PLAY_PREROLL_SILENCE_BYTES]);
            self.tx = Some(tx);
            self.task = Some(handle);
        }
        self.tx.as_ref()
    }

    fn pre_warm(&mut self) {
        let _ = self.ensure_started();
    }

    fn push_pcm(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        if let Some(tx) = self.ensure_started() {
            if tx.try_send(bytes.to_vec()).is_err() {
                crate::logger::log("WARN", "voice_controller", "Dropped pw-play audio chunk (queue full)");
            }
        }
    }

    async fn drain_and_finish(&mut self) {
        self.tx = None;
        if let Some(handle) = self.task.take() {
            let _ = handle.await;
        }
    }

    fn stop(&mut self) {
        self.tx = None;
        if let Some(handle) = self.task.take() {
            handle.abort();
        }
    }
}

async fn abort_pw_record(handle_slot: &Mutex<Option<tokio::task::JoinHandle<()>>>) {
    if let Some(handle) = handle_slot.lock().await.take() {
        handle.abort();
    }
}

/// Spawns `pw-record` (16 kHz PCM16 mono) and streams chunks to Gemini + waveform samples to UI.
/// Delays `activityStart` and `Listening` state until the first PCM frame arrives from PipeWire.
fn spawn_pw_record_task(
    gemini_out_tx: mpsc::Sender<TungsteniteMessage>,
    local_out_tx: mpsc::Sender<AxumWsMessage>,
    mgr: &'static VoiceControllerManager,
    lease: crate::audio_capture::CaptureLease,
) -> tokio::task::JoinHandle<()> {
    const CHUNK_BYTES_COUNT: usize = 1600; // 50 ms at 16 kHz, 16-bit mono
    const WAVEFORM_POINTS_COUNT: usize = 48;

    tokio::spawn(async move {
        let _lease = lease;
        let mut child = match tokio::process::Command::new("pw-record")
            .args(["--raw", "--rate", "16000", "--channels", "1", "--format", "s16", "-"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                crate::logger::log("ERROR", "voice_controller", &format!("Failed to spawn pw-record: {e}"));
                mgr.set_state(VoiceControllerState::Error, Some(format!("Failed to spawn pw-record: {e}")));
                return;
            }
        };

        let Some(mut stdout) = child.stdout.take() else {
            return;
        };

        let mut buf = vec![0u8; CHUNK_BYTES_COUNT];
        let mut started = false;
        loop {
            match stdout.read_exact(&mut buf).await {
                Ok(_) => {
                    if !started {
                        started = true;
                        let start_frame = json!({ "realtimeInput": { "activityStart": {} } });
                        if gemini_out_tx
                            .try_send(TungsteniteMessage::Text(start_frame.to_string().into()))
                            .is_err()
                        {
                            break;
                        }
                        mgr.set_state(VoiceControllerState::Listening, None);
                    }
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);
                    let frame = json!({
                        "realtimeInput": {
                            "audio": {
                                "mimeType": "audio/pcm;rate=16000",
                                "data": b64
                            }
                        }
                    });
                    match gemini_out_tx.try_send(TungsteniteMessage::Text(frame.to_string().into())) {
                        Ok(()) => {}
                        Err(mpsc::error::TrySendError::Full(_)) => {
                            crate::logger::log("WARN", "voice_controller", "Dropped outbound pw-record frame (queue full)");
                        }
                        Err(mpsc::error::TrySendError::Closed(_)) => {
                            crate::logger::log("INFO", "voice_controller", "Stopping pw-record: Gemini outbound channel closed");
                            break;
                        }
                    }

                    // Downsample 16-bit PCM chunk to 48 normalized [-1, 1] points for the UI oscilloscope.
                    let sample_count = CHUNK_BYTES_COUNT / 2;
                    let step = (sample_count / WAVEFORM_POINTS_COUNT).max(1);
                    let mut samples = Vec::with_capacity(WAVEFORM_POINTS_COUNT);
                    for i in 0..WAVEFORM_POINTS_COUNT {
                        let idx = (i * step).min(sample_count - 1);
                        let raw = i16::from_le_bytes([buf[idx * 2], buf[idx * 2 + 1]]);
                        samples.push((raw as f32) / 32768.0);
                    }
                    let wf_msg = json!({ "event": "input_waveform", "samples": samples });
                    let _ = local_out_tx.try_send(AxumWsMessage::Text(wf_msg.to_string().into()));
                }
                Err(_) => break,
            }
        }
        let _ = child.kill().await;
    })
}

/// Deterministic parser for GVariant string tuples emitted by `gdbus`.
fn parse_gvariant_tuple(line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() && chars[i] != '(' {
        i += 1;
    }
    if i >= chars.len() {
        return args;
    }
    i += 1;
    while i < chars.len() {
        while i < chars.len() && (chars[i].is_whitespace() || chars[i] == ',') {
            i += 1;
        }
        if i >= chars.len() || chars[i] == ')' {
            break;
        }
        let quote = chars[i];
        if quote == '\'' || quote == '"' {
            i += 1;
            let mut value = String::new();
            while i < chars.len() {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 1;
                    match chars[i] {
                        'n' => value.push('\n'),
                        'r' => value.push('\r'),
                        't' => value.push('\t'),
                        '\\' => value.push('\\'),
                        '\'' => value.push('\''),
                        '"' => value.push('"'),
                        other => {
                            value.push('\\');
                            value.push(other);
                        }
                    }
                } else if chars[i] == quote {
                    i += 1;
                    break;
                } else {
                    value.push(chars[i]);
                }
                i += 1;
            }
            args.push(value);
        } else {
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

/// Finds all active `org.mpris.MediaPlayer2.*` bus names currently `'Playing'` and pauses them.
async fn pause_playing_mpris_players() -> Vec<String> {
    let Ok(out) = tokio::process::Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.DBus",
            "--object-path",
            "/org/freedesktop/DBus",
            "--method",
            "org.freedesktop.DBus.ListNames",
        ])
        .output()
        .await
    else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let names = parse_gvariant_tuple(&text);
    let mut paused = Vec::new();

    for name in names {
        if !name.starts_with("org.mpris.MediaPlayer2.") {
            continue;
        }
        let Ok(status_out) = tokio::process::Command::new("gdbus")
            .args([
                "call",
                "--session",
                "--dest",
                &name,
                "--object-path",
                "/org/mpris/MediaPlayer2",
                "--method",
                "org.freedesktop.DBus.Properties.Get",
                "org.mpris.MediaPlayer2.Player",
                "PlaybackStatus",
            ])
            .output()
            .await
        else {
            continue;
        };
        let status_text = String::from_utf8_lossy(&status_out.stdout);
        if status_text.contains("Playing") {
            let _ = tokio::process::Command::new("gdbus")
                .args([
                    "call",
                    "--session",
                    "--dest",
                    &name,
                    "--object-path",
                    "/org/mpris/MediaPlayer2",
                    "--method",
                    "org.mpris.MediaPlayer2.Player.Pause",
                ])
                .output()
                .await;
            paused.push(name);
        }
    }
    paused
}

/// Resumes any MPRIS media players that were paused by [`pause_playing_mpris_players`].
async fn resume_mpris_players(paused_slot: &Mutex<Vec<String>>) {
    let names: Vec<String> = {
        let mut guard = paused_slot.lock().await;
        std::mem::take(&mut *guard)
    };
    for name in names {
        let _ = tokio::process::Command::new("gdbus")
            .args([
                "call",
                "--session",
                "--dest",
                &name,
                "--object-path",
                "/org/mpris/MediaPlayer2",
                "--method",
                "org.mpris.MediaPlayer2.Player.Play",
            ])
            .output()
            .await;
    }
}

fn non_blocking_decl(name: &str, description: &str, parameters: Option<Value>) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("name".into(), json!(name));
    obj.insert("description".into(), json!(description));
    obj.insert("behavior".into(), json!("NON_BLOCKING"));
    if let Some(params) = parameters {
        obj.insert("parameters".into(), params);
    }
    Value::Object(obj)
}

fn build_gemini_setup_payload(
    cfg: &crate::config::VoiceControllerConfig,
    resumption_handle: Option<&str>,
    context_brief: Option<&str>,
) -> Value {
    let base_prompt = if cfg.persona_style == "belta" {
        "You are the global ONE-SHIP Ship Brain voice controller, speaking like a sharp Belter (Beltalowda) ship-bridge AI. \
You control the ONE-SHIP desktop workspace across all projects and route instructions to specialist agents (kopeng). \
Style & Verbosity: Be very concise (1-2 short sentences), direct, technical, with dry Belter ship-bridge grit. \
Match the user's base language (Spanish or English), weaving in Belta Creole terms naturally while keeping file paths, code symbols, and error details exact. \
Belta Phonetic Guide (pronounce smoothly): \
bosmang [bohs-mahng] (boss/user), kopeng [koh-peng] (mate/agent), beltalowda [bel-tah-low-dah] (crew), \
oye [oy-eh] (hey), sasa ke? [sah-sah keh] (understood?/confirm?), gowork [goh-werk] (launch/start), \
kwit [kwit] (stop/kill), mera [meh-rah] (inspect/check), kowlting gonya gut [kowl-ting gohn-yah goot] (all nominal), \
pashang [pah-shahng] (failure/bug), taki [tah-kee] (thanks), im [eem] (it/he/she), mi [mee] (I). \
Rules: Always call ui_get_state first if you do not know the active project_id or enabled domains. \
Navigation (ui_navigate) is strictly visual: NEVER navigate just to read or execute capabilities in another domain (call capability_discover + capability_execute directly). \
Use harness_scratchpad to track ongoing objectives and working checklist items across pauses. \
Never guess a project or agent when ambiguous; ask briefly ('Sasa ke? Which project, bosmang?')."
    } else {
        "You are the global ONE-SHIP Ship Brain voice controller. \
You control the ONE-SHIP desktop workspace across all projects and route user instructions to specialist agents. \
Keep spoken replies concise (1-2 sentences) and technical, matching the user's language. \
Always call ui_get_state first if you do not know the active project_id or enabled domains. \
Navigation (ui_navigate) is strictly visual: NEVER navigate just to read or execute capabilities in another domain (call capability_discover + capability_execute directly). \
Use harness_scratchpad to track ongoing objectives and working checklist items across pauses. \
Never guess a project or agent when ambiguous; ask briefly for clarification."
    };

    let system_prompt = match context_brief.map(str::trim).filter(|s| !s.is_empty()) {
        Some(brief) => format!("{base_prompt}\n\n{brief}"),
        None => base_prompt.to_string(),
    };

    let declarations = vec![
        non_blocking_decl(
            "ui_get_state",
            "Returns the active project, open tab path, chat panel state, registered projects, and enabled capability domains per project.",
            None,
        ),
        non_blocking_decl(
            "ui_select_project",
            "Switches the active workspace project by project_id.",
            Some(json!({
                "type": "OBJECT",
                "properties": { "project_id": { "type": "INTEGER" } },
                "required": ["project_id"]
            })),
        ),
        non_blocking_decl(
            "ui_navigate",
            "Visually switches the workspace tab/subtab shown to the user (e.g. 'overview', 'project/board', 'devlog/todos', 'files', 'project/forge/prs'). Never required for capability_discover or capability_execute.",
            Some(json!({
                "type": "OBJECT",
                "properties": {
                    "project_id": { "type": "INTEGER" },
                    "node_path": { "type": "STRING" }
                },
                "required": ["project_id", "node_path"]
            })),
        ),
        non_blocking_decl(
            "ui_open_agent_chat",
            "Opens an agent's chat panel in the specified project.",
            Some(json!({
                "type": "OBJECT",
                "properties": {
                    "project_id": { "type": "INTEGER" },
                    "agent_id": { "type": "STRING" }
                },
                "required": ["project_id", "agent_id"]
            })),
        ),
        non_blocking_decl(
            "ui_minimize_agent_chat",
            "Minimizes the currently open agent chat panel for the project.",
            Some(json!({
                "type": "OBJECT",
                "properties": { "project_id": { "type": "INTEGER" } },
                "required": ["project_id"]
            })),
        ),
        non_blocking_decl(
            "ui_restore_agent_chat",
            "Restores a minimized agent chat panel for the project.",
            Some(json!({
                "type": "OBJECT",
                "properties": { "project_id": { "type": "INTEGER" } },
                "required": ["project_id"]
            })),
        ),
        non_blocking_decl(
            "ui_close_agent_chat",
            "Closes the agent chat panel for the project.",
            Some(json!({
                "type": "OBJECT",
                "properties": { "project_id": { "type": "INTEGER" } },
                "required": ["project_id"]
            })),
        ),
        non_blocking_decl(
            "agent_prompt",
            "Sends an instruction or message to a specific project agent.",
            Some(json!({
                "type": "OBJECT",
                "properties": {
                    "project_id": { "type": "INTEGER" },
                    "agent_id": { "type": "STRING" },
                    "prompt": { "type": "STRING" }
                },
                "required": ["project_id", "agent_id", "prompt"]
            })),
        ),
        non_blocking_decl(
            "agent_launch",
            "Starts or wakes a specialist agent in a project.",
            Some(json!({
                "type": "OBJECT",
                "properties": {
                    "project_id": { "type": "INTEGER" },
                    "agent_id": { "type": "STRING" }
                },
                "required": ["project_id", "agent_id"]
            })),
        ),
        non_blocking_decl(
            "agent_stop",
            "Stops an active agent in a project. Confirm with the user if the agent is actively working.",
            Some(json!({
                "type": "OBJECT",
                "properties": {
                    "project_id": { "type": "INTEGER" },
                    "agent_id": { "type": "STRING" }
                },
                "required": ["project_id", "agent_id"]
            })),
        ),
        non_blocking_decl(
            "agent_interrupt",
            "Interrupts an active agent turn with an optional corrective prompt.",
            Some(json!({
                "type": "OBJECT",
                "properties": {
                    "project_id": { "type": "INTEGER" },
                    "agent_id": { "type": "STRING" },
                    "corrective_prompt": { "type": "STRING" }
                },
                "required": ["project_id", "agent_id"]
            })),
        ),
        non_blocking_decl(
            "agent_new_conversation",
            "Starts a fresh conversation thread for a project agent.",
            Some(json!({
                "type": "OBJECT",
                "properties": {
                    "project_id": { "type": "INTEGER" },
                    "agent_id": { "type": "STRING" },
                    "topic": { "type": "STRING" }
                },
                "required": ["project_id", "agent_id"]
            })),
        ),
        non_blocking_decl(
            "agent_compact_context",
            "Sends the context compaction directive to a project agent.",
            Some(json!({
                "type": "OBJECT",
                "properties": {
                    "project_id": { "type": "INTEGER" },
                    "agent_id": { "type": "STRING" }
                },
                "required": ["project_id", "agent_id"]
            })),
        ),
        non_blocking_decl(
            "project_search_skills",
            "Searches installed skills in a project matching a query.",
            Some(json!({
                "type": "OBJECT",
                "properties": {
                    "project_id": { "type": "INTEGER" },
                    "query": { "type": "STRING" }
                },
                "required": ["project_id", "query"]
            })),
        ),
        non_blocking_decl(
            "harness_scratchpad",
            "Manages your persistent working session memory: list items, set_objective, add, update (status: 'pending'|'in_progress'|'done'), remove, or clear.",
            Some(json!({
                "type": "OBJECT",
                "properties": {
                    "action": { "type": "STRING", "enum": ["list", "set_objective", "add", "update", "remove", "clear"] },
                    "objective": { "type": "STRING" },
                    "project_id": { "type": "INTEGER" },
                    "item_id": { "type": "INTEGER" },
                    "title": { "type": "STRING" },
                    "status": { "type": "STRING", "enum": ["pending", "in_progress", "done"] },
                    "note": { "type": "STRING" },
                    "done_only": { "type": "BOOLEAN" }
                },
                "required": ["action"]
            })),
        ),
        non_blocking_decl(
            "harness_session_reset",
            "Archives the current working voice session and starts a clean session (only when the user explicitly requests starting fresh).",
            None,
        ),
        non_blocking_decl(
            "capability_discover",
            "Discovers available actions and schemas for any domain on project_id without navigating (e.g. 'project', 'journal', 'forge', 'files', 'settings', 'git', 'scripts', 'logs', 'electronics', 'profiler', 'skills').",
            Some(json!({
                "type": "OBJECT",
                "properties": {
                    "project_id": { "type": "INTEGER" },
                    "domain": { "type": "STRING" }
                },
                "required": ["project_id", "domain"]
            })),
        ),
        non_blocking_decl(
            "capability_execute",
            "Executes a domain action discovered via capability_discover on project_id without changing the active UI tab.",
            Some(json!({
                "type": "OBJECT",
                "properties": {
                    "project_id": { "type": "INTEGER" },
                    "domain": { "type": "STRING" },
                    "action_id": { "type": "STRING" },
                    "params_json": { "type": "STRING" }
                },
                "required": ["project_id", "domain", "action_id"]
            })),
        ),
    ];

    let model_resource = if cfg.model.starts_with("models/") {
        cfg.model.clone()
    } else {
        format!("models/{}", cfg.model)
    };

    let mut generation_config = json!({
        "responseModalities": ["AUDIO"],
        "speechConfig": {
            "voiceConfig": {
                "prebuiltVoiceConfig": {
                    "voiceName": cfg.voice_name
                }
            }
        }
    });
    if cfg.model == "gemini-3.8-live-extended-thinking" {
        if let Some(obj) = generation_config.as_object_mut() {
            obj.insert(
                "thinkingConfig".into(),
                json!({ "thinkingLevel": cfg.thinking_level }),
            );
        }
    }

    let session_resumption = match resumption_handle {
        Some(h) if !h.trim().is_empty() => json!({ "handle": h }),
        _ => json!({}),
    };

    json!({
        "setup": {
            "model": model_resource,
            "generationConfig": generation_config,
            "contextWindowCompression": {
                "triggerTokens": cfg.context_compression_trigger_tokens.to_string(),
                "slidingWindow": {
                    "targetTokens": cfg.context_compression_target_tokens.to_string()
                }
            },
            "sessionResumption": session_resumption,
            "realtimeInputConfig": {
                "automaticActivityDetection": {
                    "disabled": true
                }
            },
            "inputAudioTranscription": {},
            "outputAudioTranscription": {},
            "systemInstruction": {
                "parts": [{ "text": system_prompt }]
            },
            "tools": [{
                "functionDeclarations": declarations
            }]
        }
    })
}

async fn execute_core_tool(session_id: &str, name: &str, args: Value) -> Value {
    match name {
        "ui_get_state" => {
            let res = tokio::task::spawn_blocking(|| {
                let state = get_ui_state().map_err(|e| VoiceControllerError::Tool(e.to_string()))?;
                let projects =
                    get_projects().map_err(|e| VoiceControllerError::Tool(e.to_string()))?;
                let registry = get_ui_registry();
                let caller = crate::capabilities::CapabilityCaller::Voice;
                let proj_summaries: Vec<Value> = projects
                    .into_iter()
                    .map(|p| {
                        let domains = crate::capabilities::list_enabled_domains(&p, &caller);
                        json!({
                            "id": p.id,
                            "name": p.name,
                            "description": p.description,
                            "path": p.path,
                            "has_git": p.has_git,
                            "has_electronics_enabled": p.has_electronics_enabled,
                            "has_project_management_enabled": p.has_project_management_enabled,
                            "hidden_tabs": p.hidden_tabs,
                            "available_domains": domains
                        })
                    })
                    .collect();
                let tab_ids: Vec<String> = registry.nodes.into_iter().map(|n| n.id).collect();
                Ok::<Value, VoiceControllerError>(json!({
                    "ui_state": state,
                    "projects": proj_summaries,
                    "available_tabs": tab_ids
                }))
            })
            .await;
            flatten_blocking_tool_result(res)
        }
        "ui_select_project" => {
            let Some(project_id) = args.get("project_id").and_then(|v| v.as_i64()) else {
                return json!({ "error": "Missing integer project_id" });
            };
            if let Err(e) = validate_project_id(project_id) {
                return json!({ "error": e.to_string() });
            }
            let res = tokio::task::spawn_blocking(move || {
                dispatch_ui_action(UiAction::SelectProject { project_id })
                    .map(|s| json!({ "ok": true, "revision": s.revision }))
                    .map_err(|e| VoiceControllerError::Tool(e.to_string()))
            })
            .await;
            let out = flatten_blocking_tool_result(res);
            if out.get("ok").and_then(|v| v.as_bool()) == Some(true) {
                crate::voice_harness::record_active_project(session_id.to_string(), project_id).await;
            }
            out
        }
        "ui_navigate" => {
            let Some(project_id) = args.get("project_id").and_then(|v| v.as_i64()) else {
                return json!({ "error": "Missing integer project_id" });
            };
            if let Err(e) = validate_project_id(project_id) {
                return json!({ "error": e.to_string() });
            }
            let Some(node_path) = args.get("node_path").and_then(|v| v.as_str()).map(str::to_string) else {
                return json!({ "error": "Missing string node_path" });
            };
            let res = tokio::task::spawn_blocking(move || {
                dispatch_ui_action(UiAction::NavigateToNode {
                    project_id,
                    node_path,
                })
                .map(|s| json!({ "ok": true, "revision": s.revision }))
                .map_err(|e| VoiceControllerError::Tool(e.to_string()))
            })
            .await;
            let out = flatten_blocking_tool_result(res);
            if out.get("ok").and_then(|v| v.as_bool()) == Some(true) {
                crate::voice_harness::record_active_project(session_id.to_string(), project_id).await;
            }
            out
        }
        "ui_open_agent_chat" => {
            let Some(project_id) = args.get("project_id").and_then(|v| v.as_i64()) else {
                return json!({ "error": "Missing integer project_id" });
            };
            if let Err(e) = validate_project_id(project_id) {
                return json!({ "error": e.to_string() });
            }
            let Some(agent_id) = args.get("agent_id").and_then(|v| v.as_str()).map(str::to_string) else {
                return json!({ "error": "Missing string agent_id" });
            };
            let res = tokio::task::spawn_blocking(move || {
                dispatch_ui_action(UiAction::OpenAgentChat {
                    project_id,
                    agent_id,
                    conversation_id: None,
                })
                .map(|s| json!({ "ok": true, "revision": s.revision }))
                .map_err(|e| VoiceControllerError::Tool(e.to_string()))
            })
            .await;
            flatten_blocking_tool_result(res)
        }
        "ui_minimize_agent_chat" => {
            let Some(project_id) = args.get("project_id").and_then(|v| v.as_i64()) else {
                return json!({ "error": "Missing integer project_id" });
            };
            if let Err(e) = validate_project_id(project_id) {
                return json!({ "error": e.to_string() });
            }
            let res = tokio::task::spawn_blocking(move || {
                dispatch_ui_action(UiAction::MinimizeAgentChat { project_id })
                    .map(|s| json!({ "ok": true, "revision": s.revision }))
                    .map_err(|e| VoiceControllerError::Tool(e.to_string()))
            })
            .await;
            flatten_blocking_tool_result(res)
        }
        "ui_restore_agent_chat" => {
            let Some(project_id) = args.get("project_id").and_then(|v| v.as_i64()) else {
                return json!({ "error": "Missing integer project_id" });
            };
            if let Err(e) = validate_project_id(project_id) {
                return json!({ "error": e.to_string() });
            }
            let res = tokio::task::spawn_blocking(move || {
                dispatch_ui_action(UiAction::RestoreAgentChat { project_id })
                    .map(|s| json!({ "ok": true, "revision": s.revision }))
                    .map_err(|e| VoiceControllerError::Tool(e.to_string()))
            })
            .await;
            flatten_blocking_tool_result(res)
        }
        "ui_close_agent_chat" => {
            let Some(project_id) = args.get("project_id").and_then(|v| v.as_i64()) else {
                return json!({ "error": "Missing integer project_id" });
            };
            if let Err(e) = validate_project_id(project_id) {
                return json!({ "error": e.to_string() });
            }
            let res = tokio::task::spawn_blocking(move || {
                dispatch_ui_action(UiAction::CloseAgentChat { project_id })
                    .map(|s| json!({ "ok": true, "revision": s.revision }))
                    .map_err(|e| VoiceControllerError::Tool(e.to_string()))
            })
            .await;
            flatten_blocking_tool_result(res)
        }
        "agent_prompt" => {
            let Some(project_id) = args.get("project_id").and_then(|v| v.as_i64()) else {
                return json!({ "error": "Missing integer project_id" });
            };
            let Some(agent_id) = args.get("agent_id").and_then(|v| v.as_str()).map(str::to_string) else {
                return json!({ "error": "Missing string agent_id" });
            };
            let Some(prompt) = args.get("prompt").and_then(|v| v.as_str()).map(str::to_string) else {
                return json!({ "error": "Missing string prompt" });
            };
            let (project_path, _) = match resolve_project_info(project_id).await {
                Ok(info) => info,
                Err(e) => return json!({ "error": e.to_string() }),
            };
            match prompt_agent(Some(project_path), agent_id.clone(), prompt).await {
                Ok(msg) => {
                    crate::voice_harness::record_delegation(
                        session_id.to_string(),
                        project_id,
                        agent_id,
                    )
                    .await;
                    json!({ "ok": true, "result": msg })
                }
                Err(e) => json!({ "error": e.to_string() }),
            }
        }
        "agent_launch" => {
            let Some(project_id) = args.get("project_id").and_then(|v| v.as_i64()) else {
                return json!({ "error": "Missing integer project_id" });
            };
            let Some(agent_id) = args.get("agent_id").and_then(|v| v.as_str()).map(str::to_string) else {
                return json!({ "error": "Missing string agent_id" });
            };
            let (project_path, project_name) = match resolve_project_info(project_id).await {
                Ok(info) => info,
                Err(e) => return json!({ "error": e.to_string() }),
            };
            match launch_agent(project_path, project_name, agent_id.clone(), None, None, None, None, None).await {
                Ok(msg) => {
                    crate::voice_harness::record_delegation(
                        session_id.to_string(),
                        project_id,
                        agent_id,
                    )
                    .await;
                    json!({ "ok": true, "result": msg })
                }
                Err(e) => json!({ "error": e.to_string() }),
            }
        }
        "agent_stop" => {
            let Some(project_id) = args.get("project_id").and_then(|v| v.as_i64()) else {
                return json!({ "error": "Missing integer project_id" });
            };
            let Some(agent_id) = args.get("agent_id").and_then(|v| v.as_str()).map(str::to_string) else {
                return json!({ "error": "Missing string agent_id" });
            };
            let (project_path, _) = match resolve_project_info(project_id).await {
                Ok(info) => info,
                Err(e) => return json!({ "error": e.to_string() }),
            };
            match stop_agent(project_path, agent_id).await {
                Ok(msg) => json!({ "ok": true, "result": msg }),
                Err(e) => json!({ "error": e.to_string() }),
            }
        }
        "agent_interrupt" => {
            let Some(project_id) = args.get("project_id").and_then(|v| v.as_i64()) else {
                return json!({ "error": "Missing integer project_id" });
            };
            let Some(agent_id) = args.get("agent_id").and_then(|v| v.as_str()).map(str::to_string) else {
                return json!({ "error": "Missing string agent_id" });
            };
            let corrective_prompt = args
                .get("corrective_prompt")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let (project_path, _) = match resolve_project_info(project_id).await {
                Ok(info) => info,
                Err(e) => return json!({ "error": e.to_string() }),
            };
            match interrupt_agent(project_path, agent_id, corrective_prompt).await {
                Ok(msg) => json!({ "ok": true, "result": msg }),
                Err(e) => json!({ "error": e.to_string() }),
            }
        }
        "agent_new_conversation" => {
            let Some(project_id) = args.get("project_id").and_then(|v| v.as_i64()) else {
                return json!({ "error": "Missing integer project_id" });
            };
            let Some(agent_id) = args.get("agent_id").and_then(|v| v.as_str()).map(str::to_string) else {
                return json!({ "error": "Missing string agent_id" });
            };
            let topic = args
                .get("topic")
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("New Session")
                .to_string();
            let (project_path, _) = match resolve_project_info(project_id).await {
                Ok(info) => info,
                Err(e) => return json!({ "error": e.to_string() }),
            };
            let res = tokio::task::spawn_blocking(move || {
                create_agent_conversation(CreateAgentConversationArgs {
                    project_path,
                    agent_id,
                    topic,
                    runner: None,
                    model: None,
                    account_id: None,
                    effort: None,
                })
                .map(|c| json!({ "ok": true, "conversation_id": c.id, "topic": c.topic }))
                .map_err(|e| VoiceControllerError::Tool(e.to_string()))
            })
            .await;
            flatten_blocking_tool_result(res)
        }
        "agent_compact_context" => {
            let Some(project_id) = args.get("project_id").and_then(|v| v.as_i64()) else {
                return json!({ "error": "Missing integer project_id" });
            };
            let Some(agent_id) = args.get("agent_id").and_then(|v| v.as_str()).map(str::to_string) else {
                return json!({ "error": "Missing string agent_id" });
            };
            let (project_path, _) = match resolve_project_info(project_id).await {
                Ok(info) => info,
                Err(e) => return json!({ "error": e.to_string() }),
            };
            let directive = "[SYSTEM DIRECTIVE: CONTEXT COMPACTION]\nPlease summarize our progress, current technical state, and open items into a concise milestone record. Once recorded, retain only active tasks in memory to compact the context footprint.".to_string();
            match prompt_agent(Some(project_path), agent_id, directive).await {
                Ok(msg) => json!({ "ok": true, "result": msg }),
                Err(e) => json!({ "error": e.to_string() }),
            }
        }
        "project_search_skills" => {
            let Some(project_id) = args.get("project_id").and_then(|v| v.as_i64()) else {
                return json!({ "error": "Missing integer project_id" });
            };
            let query = args
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_lowercase();
            let (project_path, _) = match resolve_project_info(project_id).await {
                Ok(info) => info,
                Err(e) => return json!({ "error": e.to_string() }),
            };
            let res = tokio::task::spawn_blocking(move || {
                let skills = get_project_skills(project_path)
                    .map_err(|e| VoiceControllerError::Tool(e.to_string()))?;
                let matches: Vec<Value> = skills
                    .into_iter()
                    .filter(|s| {
                        query.is_empty()
                            || s.name.to_lowercase().contains(&query)
                            || s.description.to_lowercase().contains(&query)
                    })
                    .take(25)
                    .map(|s| {
                        json!({
                            "name": s.name,
                            "description": s.description,
                            "is_enabled": s.is_enabled,
                            "is_platform_base": s.is_platform_base
                        })
                    })
                    .collect();
                Ok::<Value, VoiceControllerError>(json!({ "skills": matches }))
            })
            .await;
            flatten_blocking_tool_result(res)
        }
        "capability_discover" => {
            let Some(project_id) = args.get("project_id").and_then(|v| v.as_i64()) else {
                return json!({ "error": "Missing integer project_id" });
            };
            let Some(domain) = args.get("domain").and_then(|v| v.as_str()).map(str::to_string) else {
                return json!({ "error": "Missing string domain" });
            };
            match crate::capabilities::discover_domain(
                project_id,
                &domain,
                &crate::capabilities::CapabilityCaller::Voice,
            )
            .await
            {
                Ok(v) => v,
                Err(e) => json!({ "error": e.to_string() }),
            }
        }
        "capability_execute" => {
            let Some(project_id) = args.get("project_id").and_then(|v| v.as_i64()) else {
                return json!({ "error": "Missing integer project_id" });
            };
            if let Err(e) = validate_project_id(project_id) {
                return json!({ "error": e.to_string() });
            }
            let Some(domain) = args.get("domain").and_then(|v| v.as_str()).map(str::to_string) else {
                return json!({ "error": "Missing string domain" });
            };
            let Some(action_id) = args.get("action_id").and_then(|v| v.as_str()).map(str::to_string) else {
                return json!({ "error": "Missing string action_id" });
            };
            let params = match args.get("params_json") {
                None | Some(Value::Null) => json!({}),
                Some(Value::String(raw_json)) => {
                    if raw_json.trim().is_empty() {
                        json!({})
                    } else {
                        match serde_json::from_str::<Value>(raw_json) {
                            Ok(v) => v,
                            Err(e) => {
                                return json!({
                                    "error": format!("Invalid JSON in params_json: {e}")
                                });
                            }
                        }
                    }
                }
                Some(_) => {
                    return json!({ "error": "params_json must be a JSON-encoded string" });
                }
            };
            match crate::capabilities::execute_capability(
                project_id,
                &domain,
                &action_id,
                params,
                crate::capabilities::CapabilityCaller::Voice,
            )
            .await
            {
                Ok(v) => v,
                Err(e) => json!({ "error": e.to_string() }),
            }
        }
        unknown => json!({ "error": format!("Unknown tool '{unknown}'") }),
    }
}

async fn resolve_project_info(project_id: i64) -> Result<(String, String), VoiceControllerError> {
    let project_id_usize = validate_project_id(project_id)?;
    tokio::task::spawn_blocking(move || {
        get_projects()
            .map_err(|e| VoiceControllerError::Task(e.to_string()))?
            .into_iter()
            .find(|p| p.id == project_id_usize)
            .map(|p| (p.path, p.name))
            .ok_or(VoiceControllerError::ProjectNotFound(project_id))
    })
    .await
    .map_err(|e| VoiceControllerError::Task(e.to_string()))?
}

fn flatten_blocking_tool_result(
    res: Result<Result<Value, VoiceControllerError>, tokio::task::JoinError>,
) -> Value {
    match res {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => json!({ "error": e.to_string() }),
        Err(e) => json!({ "error": e.to_string() }),
    }
}
