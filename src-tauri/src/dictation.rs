//! Native Gemini Live dictation.
//!
//! Captures PCM through `pw-record`, streams it to `models/gemini-3.5-transcribe-live` over the
//! BidiGenerateContent WebSocket, and emits cumulative interim text plus a final transcript over
//! the daemon SSE bus. Recorded audio lives in a per-session PCM file so a dropped Live session
//! can be resumed by replaying the unconfirmed tail at real-time pace; nothing is discarded on a
//! network failure.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};

use base64::Engine as _;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot, watch, Mutex};
use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;
use tokio_util::sync::CancellationToken;

const GEMINI_LIVE_WS_BASE: &str =
    "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent";
const DICTATION_MODEL: &str = "models/gemini-3.5-transcribe-live";
const AUDIO_SAMPLE_RATE_HZ: u32 = 16_000;
const AUDIO_CHANNELS_COUNT: u8 = 1;
const AUDIO_FRAME_DURATION_MS: u64 = 50;
const AUDIO_FRAME_BYTES_COUNT: usize = 1_600; // 50 ms at 16 kHz, 16-bit mono
const OUTBOUND_QUEUE_CAPACITY_COUNT: usize = 256;
const SERVER_EVENT_QUEUE_CAPACITY_COUNT: usize = 128;
const SETUP_TIMEOUT_SECONDS: u64 = 10;
const HEALTH_TICK_MS: u64 = 250;
const RECONNECT_BACKOFF_BASE_MS: u64 = 250;
const RECONNECT_BACKOFF_MAX_MS: u64 = 4_000;

/// How the transcription model formats the text. `Smart` is native model formatting/cleanup,
/// not a second LLM pass; `Verbatim` keeps the literal acoustic transcript.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum DictationMode {
    Smart,
    Verbatim,
}

impl DictationMode {
    fn live_value(self) -> &'static str {
        match self {
            Self::Smart => "SMART",
            Self::Verbatim => "VERBATIM",
        }
    }
}

/// Runtime state of the native dictation subsystem.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DictationState {
    Idle,
    Connecting,
    Listening,
    Processing,
    Error,
}

/// Final transcript outcome.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DictationOutcome {
    Ok,
    Empty,
}

/// Returned by `start_dictation`.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DictationSessionInfo {
    pub session_id: String,
    pub mode: DictationMode,
    pub model: String,
}

/// Non-secret status returned by `get_dictation_status` and `dictation-state`.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DictationStatus {
    pub configured: bool,
    pub enabled: bool,
    pub available: bool,
    pub keyring_available: bool,
    pub mode: DictationMode,
    pub auto_send: bool,
    pub state: DictationState,
    pub last_error: Option<String>,
    pub recovery_file_path: Option<String>,
}

/// Payload of `dictation-state`.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DictationStatePayload {
    pub session_id: Option<String>,
    pub state: DictationState,
    pub last_error: Option<String>,
    pub recovery_file_path: Option<String>,
}

/// Payload of `dictation-interim-text`.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DictationInterimPayload {
    pub session_id: String,
    pub text: String,
}

/// Payload of `dictation-finished`.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DictationFinishedPayload {
    pub session_id: String,
    pub text: String,
    pub outcome: DictationOutcome,
    pub recovery_file_path: Option<String>,
}

/// Payload of `dictation-cancelled`.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DictationCancelledPayload {
    pub session_id: String,
}

/// Arguments of `stop_dictation` and `cancel_dictation`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct DictationSessionArgs {
    pub session_id: String,
}

/// Result returned by `stop_dictation`.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DictationResult {
    pub session_id: String,
    pub text: String,
    pub outcome: DictationOutcome,
    pub recovery_file_path: Option<String>,
}

/// Typed dictation failure. Every variant carries a stable API status and code.
#[derive(Debug, thiserror::Error)]
pub enum DictationError {
    #[error("Native dictation is disabled")]
    Disabled,
    #[error("Configure the Google AI Studio API key in Settings > Global Voice Controller")]
    MissingApiKey,
    #[error("OS keyring is unavailable: {0}")]
    KeyringUnavailable(String),
    #[error("A dictation session is already active")]
    Busy,
    #[error("Microphone is busy: {0}")]
    MicrophoneBusy(String),
    #[error("Audio capture failed: {0}")]
    AudioCapture(String),
    #[error("Gemini Live connection failed: {0}")]
    WebSocketConnect(String),
    #[error("Gemini Live API error ({status}): {message}")]
    Api { status: u16, message: String },
    #[error("Dictation session not found: {0}")]
    NotFound(String),
    #[error("Dictation I/O failed: {0}")]
    Io(String),
    #[error("Dictation task failed: {0}")]
    Task(String),
    #[error("Dictation cancelled")]
    Cancelled,
    #[error(
        "Live transcription could not resume after {attempts} attempts. The recorded audio was preserved at {path}"
    )]
    RecoveryExhausted { attempts: u32, path: String },
}

impl crate::api::ApiError for DictationError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::Disabled | Self::MissingApiKey => StatusCode::PRECONDITION_FAILED,
            Self::KeyringUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Busy | Self::MicrophoneBusy(_) => StatusCode::CONFLICT,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Cancelled => StatusCode::BAD_REQUEST,
            Self::Api { status, .. } => match *status {
                429 => StatusCode::TOO_MANY_REQUESTS,
                401 => StatusCode::UNAUTHORIZED,
                503 => StatusCode::SERVICE_UNAVAILABLE,
                _ => StatusCode::BAD_GATEWAY,
            },
            Self::AudioCapture(_) | Self::Io(_) | Self::Task(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::WebSocketConnect(_) => StatusCode::BAD_GATEWAY,
            Self::RecoveryExhausted { .. } => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::Disabled => "DictationError::Disabled",
            Self::MissingApiKey => "DictationError::MissingApiKey",
            Self::KeyringUnavailable(_) => "DictationError::KeyringUnavailable",
            Self::Busy => "DictationError::Busy",
            Self::MicrophoneBusy(_) => "DictationError::MicrophoneBusy",
            Self::AudioCapture(_) => "DictationError::AudioCapture",
            Self::WebSocketConnect(_) => "DictationError::WebSocketConnect",
            Self::Api { .. } => "DictationError::Api",
            Self::NotFound(_) => "DictationError::NotFound",
            Self::Io(_) => "DictationError::Io",
            Self::Task(_) => "DictationError::Task",
            Self::Cancelled => "DictationError::Cancelled",
            Self::RecoveryExhausted { .. } => "DictationError::RecoveryExhausted",
        }
    }
}

struct SessionHandle {
    stop: CancellationToken,
    cancel: CancellationToken,
    result: Option<oneshot::Receiver<Result<DictationResult, DictationError>>>,
}

struct ManagerInner {
    state: DictationState,
    last_error: Option<String>,
    recovery_file_path: Option<String>,
    active_session_id: Option<String>,
}

struct DictationManager {
    inner: RwLock<ManagerInner>,
    sessions: Mutex<HashMap<String, SessionHandle>>,
}

static DICTATION_MANAGER: OnceLock<DictationManager> = OnceLock::new();

fn manager() -> &'static DictationManager {
    DICTATION_MANAGER.get_or_init(|| DictationManager {
        inner: RwLock::new(ManagerInner {
            state: DictationState::Idle,
            last_error: None,
            recovery_file_path: None,
            active_session_id: None,
        }),
        sessions: Mutex::new(HashMap::new()),
    })
}

fn read_inner() -> std::sync::RwLockReadGuard<'static, ManagerInner> {
    manager()
        .inner
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write_inner() -> std::sync::RwLockWriteGuard<'static, ManagerInner> {
    manager()
        .inner
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn set_state(
    session_id: Option<String>,
    state: DictationState,
    last_error: Option<String>,
    recovery_file_path: Option<String>,
) {
    {
        let mut inner = write_inner();
        inner.state = state;
        inner.last_error = last_error.clone();
        inner.recovery_file_path = recovery_file_path.clone();
        if let Some(id) = session_id.clone() {
            inner.active_session_id = Some(id);
        }
        if state == DictationState::Idle || state == DictationState::Error {
            inner.active_session_id = None;
        }
    }
    let payload = DictationStatePayload {
        session_id,
        state,
        last_error,
        recovery_file_path,
    };
    crate::supervisor::get_supervisor()
        .emit_event(crate::events::DaemonEvent::dictation_state(&payload));
}

fn emit_interim(session_id: &str, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    crate::supervisor::get_supervisor().emit_event(
        crate::events::DaemonEvent::dictation_interim_text(&DictationInterimPayload {
            session_id: session_id.to_string(),
            text: text.to_string(),
        }),
    );
}

fn emit_finished(payload: &DictationFinishedPayload) {
    crate::supervisor::get_supervisor()
        .emit_event(crate::events::DaemonEvent::dictation_finished(payload));
}

fn emit_cancelled(session_id: &str) {
    crate::supervisor::get_supervisor().emit_event(
        crate::events::DaemonEvent::dictation_cancelled(&DictationCancelledPayload {
            session_id: session_id.to_string(),
        }),
    );
}

fn audio_available() -> bool {
    crate::desktop::resolve_executable("pw-record").is_some()
}

/// Daemon command `get_dictation_status`.
pub fn get_dictation_status() -> DictationStatus {
    let cfg = crate::config::get_dictation_config();
    let inner = read_inner();
    DictationStatus {
        configured: crate::config::get_gemini_api_key().is_some(),
        enabled: cfg.enabled,
        available: audio_available(),
        keyring_available: crate::secret_store::is_available(),
        mode: cfg.mode,
        auto_send: cfg.auto_send,
        state: inner.state,
        last_error: inner.last_error.clone(),
        recovery_file_path: inner.recovery_file_path.clone(),
    }
}

/// Daemon command `start_dictation`.
pub async fn start_dictation() -> Result<DictationSessionInfo, DictationError> {
    let cfg = crate::config::get_dictation_config();
    if !cfg.enabled {
        return Err(DictationError::Disabled);
    }
    if !audio_available() {
        return Err(DictationError::AudioCapture(
            "pw-record is not installed (PipeWire utilities are required)".to_string(),
        ));
    }
    let api_key = crate::config::get_gemini_api_key().ok_or_else(|| {
        match crate::config::gemini_api_key_error() {
            Some(error) => DictationError::KeyringUnavailable(error),
            None => DictationError::MissingApiKey,
        }
    })?;

    {
        let sessions = manager().sessions.lock().await;
        if !sessions.is_empty() {
            return Err(DictationError::Busy);
        }
    }

    let lease = crate::audio_capture::try_acquire("dictation").map_err(|e| match e {
        crate::audio_capture::CaptureError::Busy { owner } => {
            DictationError::MicrophoneBusy(owner.to_string())
        }
    })?;

    let session_id = uuid::Uuid::new_v4().to_string();
    let stop = CancellationToken::new();
    let cancel = CancellationToken::new();
    let (result_tx, result_rx) = oneshot::channel();

    {
        let mut sessions = manager().sessions.lock().await;
        sessions.insert(
            session_id.clone(),
            SessionHandle {
                stop: stop.clone(),
                cancel: cancel.clone(),
                result: Some(result_rx),
            },
        );
    }

    set_state(
        Some(session_id.clone()),
        DictationState::Connecting,
        None,
        None,
    );

    let info = DictationSessionInfo {
        session_id: session_id.clone(),
        mode: cfg.mode,
        model: DICTATION_MODEL.trim_start_matches("models/").to_string(),
    };

    let cfg_for_task = cfg.clone();
    let api_key_for_task = api_key.clone();
    tokio::spawn(async move {
        run_session(
            session_id,
            cfg_for_task,
            api_key_for_task,
            lease,
            stop,
            cancel,
            result_tx,
        )
        .await;
    });

    crate::logger::log(
        "INFO",
        "dictation",
        &format!(
            "Native dictation started (mode={:?}, model={})",
            cfg.mode, DICTATION_MODEL
        ),
    );

    Ok(info)
}

/// Daemon command `stop_dictation`: asks the active session to finalize and waits for its result.
pub async fn stop_dictation(args: DictationSessionArgs) -> Result<DictationResult, DictationError> {
    let mut handle = {
        let mut sessions = manager().sessions.lock().await;
        sessions
            .remove(&args.session_id)
            .ok_or_else(|| DictationError::NotFound(args.session_id.clone()))?
    };

    set_state(
        Some(args.session_id.clone()),
        DictationState::Processing,
        None,
        None,
    );
    handle.stop.cancel();

    let Some(result_rx) = handle.result.take() else {
        return Err(DictationError::Task(
            "dictation session has no result channel".to_string(),
        ));
    };

    match result_rx.await {
        Ok(result) => result,
        Err(_) => Err(DictationError::Task(
            "dictation task ended without returning a result".to_string(),
        )),
    }
}

/// Daemon command `cancel_dictation`: discards the session and its recorded audio.
pub async fn cancel_dictation(args: DictationSessionArgs) -> Result<(), DictationError> {
    let handle = {
        let mut sessions = manager().sessions.lock().await;
        sessions
            .remove(&args.session_id)
            .ok_or_else(|| DictationError::NotFound(args.session_id.clone()))?
    };
    handle.cancel.cancel();
    set_state(None, DictationState::Idle, None, None);
    Ok(())
}

/// Cancels every dictation session (daemon shutdown).
pub async fn shutdown() {
    let handles = {
        let mut sessions = manager().sessions.lock().await;
        sessions.drain().map(|(_, handle)| handle).collect::<Vec<_>>()
    };
    for handle in handles {
        handle.cancel.cancel();
    }
    set_state(None, DictationState::Idle, None, None);
}

fn state_dir() -> Result<PathBuf, DictationError> {
    let base = dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .ok_or_else(|| DictationError::Io("state directory is unavailable".to_string()))?;
    Ok(base.join("one-ship").join("dictation"))
}

async fn prepare_audio_path(session_id: &str) -> Result<PathBuf, DictationError> {
    let dir = state_dir()?;
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| DictationError::Io(format!("cannot create {}: {e}", dir.display())))?;
    let path = dir.join(format!("{session_id}.pcm"));
    tokio::fs::File::create(&path)
        .await
        .map_err(|e| DictationError::Io(format!("cannot create {}: {e}", path.display())))?;
    Ok(path)
}

/// Removes unrecovered PCM files older than the configured retention.
pub async fn cleanup_stale_sessions() {
    let retention = crate::config::get_dictation_config().stale_audio_retention_hours;
    let Ok(dir) = state_dir() else {
        return;
    };
    let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
        return;
    };
    let cutoff = match std::time::SystemTime::now()
        .checked_sub(Duration::from_secs(retention.saturating_mul(3600)))
    {
        Some(cutoff) => cutoff,
        None => std::time::SystemTime::UNIX_EPOCH,
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("pcm") {
            continue;
        }
        let Ok(metadata) = entry.metadata().await else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if modified < cutoff {
            crate::logger::log(
                "INFO",
                "dictation",
                &format!("Removing stale dictation audio {}", path.display()),
            );
            if let Err(e) = tokio::fs::remove_file(&path).await {
                crate::logger::log(
                    "WARN",
                    "dictation",
                    &format!("Could not remove stale audio {}: {e}", path.display()),
                );
            }
        }
    }
}

async fn run_session(
    session_id: String,
    cfg: crate::config::DictationConfig,
    api_key: String,
    lease: crate::audio_capture::CaptureLease,
    stop: CancellationToken,
    cancel: CancellationToken,
    result_tx: oneshot::Sender<Result<DictationResult, DictationError>>,
) {
    let audio_path = match prepare_audio_path(&session_id).await {
        Ok(path) => path,
        Err(e) => {
            let message = e.to_string();
            set_state(
                Some(session_id.clone()),
                DictationState::Error,
                Some(message.clone()),
                None,
            );
            if result_tx.send(Err(e)).is_err() {
                crate::logger::log(
                    "WARN",
                    "dictation",
                    "Stop caller dropped before receiving the audio-path error",
                );
            }
            remove_session(&session_id).await;
            return;
        }
    };

    let (audio_tx, mut audio_rx) = watch::channel(AudioStatus::default());
    let capture = match spawn_capture(audio_path.clone(), audio_tx, lease).await {
        Ok(capture) => capture,
        Err(e) => {
            let message = e.to_string();
            set_state(
                Some(session_id.clone()),
                DictationState::Error,
                Some(message.clone()),
                None,
            );
            if result_tx.send(Err(e)).is_err() {
                crate::logger::log(
                    "WARN",
                    "dictation",
                    "Stop caller dropped before receiving the start error",
                );
            }
            remove_session(&session_id).await;
            return;
        }
    };

    let result = drive_session(
        &session_id,
        &cfg,
        &api_key,
        &audio_path,
        &mut audio_rx,
        &stop,
        &cancel,
        &capture.cancel,
    )
    .await;

    capture.cancel.cancel();
    if let Err(e) = capture.handle.await {
        crate::logger::log(
            "WARN",
            "dictation",
            &format!("Audio capture task ended unexpectedly: {e}"),
        );
    }

    match &result {
        Ok(dictation) => {
            if dictation.recovery_file_path.is_none() {
                if let Err(e) = tokio::fs::remove_file(&audio_path).await {
                    crate::logger::log(
                        "WARN",
                        "dictation",
                        &format!("Could not remove {}: {e}", audio_path.display()),
                    );
                }
            }
            set_state(
                None,
                DictationState::Idle,
                None,
                dictation.recovery_file_path.clone(),
            );
            emit_finished(&DictationFinishedPayload {
                session_id: session_id.clone(),
                text: dictation.text.clone(),
                outcome: dictation.outcome,
                recovery_file_path: dictation.recovery_file_path.clone(),
            });
        }
        Err(DictationError::Cancelled) => {
            if let Err(e) = tokio::fs::remove_file(&audio_path).await {
                crate::logger::log(
                    "WARN",
                    "dictation",
                    &format!("Could not remove cancelled audio {}: {e}", audio_path.display()),
                );
            }
            set_state(None, DictationState::Idle, None, None);
            emit_cancelled(&session_id);
        }
        Err(e) => {
            let path = audio_path.display().to_string();
            let message = e.to_string();
            crate::logger::log(
                "ERROR",
                "dictation",
                &format!("Dictation session {session_id} failed: {message} (audio kept at {path})"),
            );
            set_state(
                None,
                DictationState::Error,
                Some(message),
                Some(path),
            );
        }
    }

    if result_tx.send(result).is_err() {
        crate::logger::log(
            "DEBUG",
            "dictation",
            "Stop caller dropped before receiving the final dictation result",
        );
    }
    remove_session(&session_id).await;
}

async fn remove_session(session_id: &str) {
    let mut sessions = manager().sessions.lock().await;
    sessions.remove(session_id);
}

struct CaptureTask {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

#[derive(Clone, Debug, Default)]
struct AudioStatus {
    total_bytes: u64,
    rms: f32,
    capture_ended: bool,
    error: Option<String>,
}

async fn spawn_capture(
    path: PathBuf,
    status_tx: watch::Sender<AudioStatus>,
    lease: crate::audio_capture::CaptureLease,
) -> Result<CaptureTask, DictationError> {
    let cancel = CancellationToken::new();
    let cancel_for_task = cancel.clone();
    let handle = tokio::spawn(async move {
        let _lease = lease;
        let result = capture_loop(path, status_tx.clone(), cancel_for_task).await;
        let mut status = status_tx.borrow().clone();
        status.capture_ended = true;
        status.error = match &result {
            Ok(()) => None,
            Err(e) => {
                crate::logger::log("ERROR", "dictation", &format!("Audio capture failed: {e}"));
                Some(e.to_string())
            }
        };
        if status_tx.send(status).is_err() {
            crate::logger::log(
                "DEBUG",
                "dictation",
                "Audio status receiver closed before capture finished",
            );
        }
    });
    Ok(CaptureTask { cancel, handle })
}

async fn capture_loop(
    path: PathBuf,
    status_tx: watch::Sender<AudioStatus>,
    cancel: CancellationToken,
) -> Result<(), DictationError> {
    if crate::desktop::resolve_executable("pw-record").is_none() {
        return Err(DictationError::AudioCapture(
            "pw-record is not installed (PipeWire utilities are required)".to_string(),
        ));
    }

    let rate = AUDIO_SAMPLE_RATE_HZ.to_string();
    let channels = AUDIO_CHANNELS_COUNT.to_string();
    let mut child = tokio::process::Command::new("pw-record")
        .args([
            "--raw",
            "--rate",
            rate.as_str(),
            "--channels",
            channels.as_str(),
            "--format",
            "s16",
            "-",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| DictationError::AudioCapture(format!("cannot spawn pw-record: {e}")))?;

    let Some(mut stdout) = child.stdout.take() else {
        return Err(DictationError::AudioCapture(
            "pw-record produced no stdout pipe".to_string(),
        ));
    };
    let mut file = tokio::fs::File::create(&path)
        .await
        .map_err(|e| DictationError::Io(format!("cannot create {}: {e}", path.display())))?;
    let mut buffer = vec![0u8; AUDIO_FRAME_BYTES_COUNT];
    let mut total_bytes = 0u64;

    loop {
        let read = tokio::select! {
            _ = cancel.cancelled() => break,
            read = stdout.read(&mut buffer) => read,
        };
        let read = read.map_err(|e| DictationError::AudioCapture(e.to_string()))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .await
            .map_err(|e| DictationError::Io(format!("cannot append dictation audio: {e}")))?;
        total_bytes = total_bytes.saturating_add(read as u64);
        let status = AudioStatus {
            total_bytes,
            rms: normalized_rms(&buffer[..read]),
            capture_ended: false,
            error: None,
        };
        if status_tx.send(status).is_err() {
            break;
        }
    }

    if let Err(e) = file.flush().await {
        crate::logger::log("WARN", "dictation", &format!("Could not flush dictation audio: {e}"));
    }
    if let Err(e) = child.kill().await {
        crate::logger::log("WARN", "dictation", &format!("Could not stop pw-record: {e}"));
    }
    Ok(())
}

fn normalized_rms(bytes: &[u8]) -> f32 {
    if bytes.len() < 2 {
        return 0.0;
    }
    let mut sum_squares = 0.0f64;
    let mut count = 0u32;
    for pair in bytes.chunks_exact(2) {
        let sample = i16::from_le_bytes([pair[0], pair[1]]) as f64 / 32768.0;
        sum_squares += sample * sample;
        count += 1;
    }
    if count == 0 {
        return 0.0;
    }
    (sum_squares / count as f64).sqrt() as f32
}

#[derive(Debug)]
enum SegmentOutcome {
    Finalize,
    Rollover,
    Recover(String),
    Cancelled,
}

#[derive(Debug, PartialEq, Eq)]
enum Finalization {
    Completed,
    TimedOut,
    Disconnected,
}

struct TranscriptState {
    committed_text: String,
    current_turn: String,
    interim_text: String,
    committed_offset: u64,
    session_sent_offset: u64,
    latest_total_bytes: u64,
    last_text_at: Instant,
    last_speech_at: Instant,
    silence_since: Option<Instant>,
    session_started_at: Instant,
    last_recovery_at: Instant,
    recovery_attempts: u32,
    stopping: bool,
    capture_ended: bool,
    listening: bool,
}

impl TranscriptState {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            committed_text: String::new(),
            current_turn: String::new(),
            interim_text: String::new(),
            committed_offset: 0,
            session_sent_offset: 0,
            latest_total_bytes: 0,
            last_text_at: now,
            last_speech_at: now,
            silence_since: None,
            session_started_at: now,
            last_recovery_at: now - Duration::from_secs(3_600),
            recovery_attempts: 0,
            stopping: false,
            capture_ended: false,
            listening: false,
        }
    }

    fn full_text(&self) -> String {
        let authoritative = join_segments(&self.committed_text, &self.current_turn);
        join_segments(&authoritative, &self.interim_text)
    }

    fn commit_current_turn(&mut self) -> String {
        self.committed_text = join_segments(&self.committed_text, &self.current_turn);
        self.current_turn.clear();
        self.interim_text.clear();
        self.full_text()
    }

    /// Applies an authoritative `inputTranscription` event. A full-session revision replaces the
    /// committed transcript; a segment-sized update is merged into the current turn.
    fn apply_transcription(&mut self, text: &str, finished: bool) -> String {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            if is_cumulative_revision(&self.committed_text, trimmed) {
                self.committed_text = trimmed.to_string();
                self.current_turn.clear();
            } else {
                self.current_turn = merge_turn_update(&self.current_turn, trimmed);
            }
        }
        self.interim_text.clear();
        if finished {
            self.committed_text = join_segments(&self.committed_text, &self.current_turn);
            self.current_turn.clear();
        }
        self.full_text()
    }
}

async fn drive_session(
    session_id: &str,
    cfg: &crate::config::DictationConfig,
    api_key: &str,
    audio_path: &Path,
    audio_rx: &mut watch::Receiver<AudioStatus>,
    stop: &CancellationToken,
    cancel: &CancellationToken,
    capture_cancel: &CancellationToken,
) -> Result<DictationResult, DictationError> {
    let mut state = TranscriptState::new();
    let mut conn = connect_with_retries(api_key, cfg, cancel).await?;
    let mut read_file = open_audio_reader(audio_path, 0).await?;

    loop {
        let outcome = run_segment(
            session_id,
            cfg,
            &mut state,
            &mut conn,
            &mut read_file,
            audio_rx,
            stop,
            cancel,
            capture_cancel,
        )
        .await?;

        match outcome {
            SegmentOutcome::Cancelled => return Err(DictationError::Cancelled),
            SegmentOutcome::Finalize => {
                let finalization = finalize_segment(
                    &mut state,
                    &mut conn,
                    cfg.finalize_timeout_seconds,
                )
                .await;
                match finalization {
                    Finalization::Completed => {
                        let text = state.commit_current_turn();
                        return Ok(DictationResult {
                            session_id: session_id.to_string(),
                            text,
                            outcome: outcome_for(&state.committed_text),
                            recovery_file_path: None,
                        });
                    }
                    Finalization::TimedOut => {
                        let text = state.full_text();
                        let outcome = outcome_for(&text);
                        crate::logger::log(
                            "WARN",
                            "dictation",
                            "Final Live transcription timed out; returning the accumulated transcript and preserving the audio file",
                        );
                        return Ok(DictationResult {
                            session_id: session_id.to_string(),
                            text,
                            outcome,
                            recovery_file_path: Some(audio_path.display().to_string()),
                        });
                    }
                    Finalization::Disconnected => {
                        if state.recovery_attempts >= cfg.reconnect_attempts_count {
                            return Err(DictationError::RecoveryExhausted {
                                attempts: state.recovery_attempts,
                                path: audio_path.display().to_string(),
                            });
                        }
                        state.recovery_attempts += 1;
                        state.last_recovery_at = Instant::now();
                        state.current_turn.clear();
                        state.interim_text.clear();
                        state.session_sent_offset = state.committed_offset;
                        conn.close().await;
                        reconnect_backoff(state.recovery_attempts).await;
                        conn = connect_with_retries(api_key, cfg, cancel).await?;
                        read_file = open_audio_reader(audio_path, state.session_sent_offset).await?;
                        state.listening = false;
                    }
                }
            }
            SegmentOutcome::Rollover => {
                let finalization = finalize_segment(
                    &mut state,
                    &mut conn,
                    cfg.rollover_finalize_timeout_seconds,
                )
                .await;
                if finalization == Finalization::Completed {
                    state.commit_current_turn();
                    state.committed_offset = state.session_sent_offset;
                } else {
                    state.current_turn.clear();
                    state.interim_text.clear();
                    state.session_sent_offset = state.committed_offset;
                }
                conn.close().await;
                conn = connect_with_retries(api_key, cfg, cancel).await?;
                read_file = open_audio_reader(audio_path, state.session_sent_offset).await?;
                state.session_started_at = Instant::now();
                state.last_text_at = Instant::now();
                state.last_speech_at = Instant::now();
                state.silence_since = None;
                state.recovery_attempts = 0;
                state.listening = false;
            }
            SegmentOutcome::Recover(reason) => {
                if state.recovery_attempts >= cfg.reconnect_attempts_count {
                    return Err(DictationError::RecoveryExhausted {
                        attempts: state.recovery_attempts,
                        path: audio_path.display().to_string(),
                    });
                }
                state.recovery_attempts += 1;
                state.last_recovery_at = Instant::now();
                crate::logger::log(
                    "WARN",
                    "dictation",
                    &format!(
                        "Live session interrupted ({reason}); reconnecting and replaying unconfirmed audio from byte {}",
                        state.committed_offset
                    ),
                );
                state.current_turn.clear();
                state.interim_text.clear();
                state.session_sent_offset = state.committed_offset;
                conn.close().await;
                reconnect_backoff(state.recovery_attempts).await;
                conn = connect_with_retries(api_key, cfg, cancel).await?;
                read_file = open_audio_reader(audio_path, state.session_sent_offset).await?;
                state.session_started_at = Instant::now();
                state.last_text_at = Instant::now();
                state.last_speech_at = Instant::now();
                state.silence_since = None;
                state.listening = false;
            }
        }
    }
}

fn outcome_for(text: &str) -> DictationOutcome {
    if text.trim().is_empty() {
        DictationOutcome::Empty
    } else {
        DictationOutcome::Ok
    }
}

async fn reconnect_backoff(attempt: u32) {
    let exponent = attempt.saturating_sub(1).min(4);
    let delay_ms = (RECONNECT_BACKOFF_BASE_MS << exponent).min(RECONNECT_BACKOFF_MAX_MS);
    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
}

async fn open_audio_reader(path: &Path, offset: u64) -> Result<tokio::fs::File, DictationError> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| DictationError::Io(format!("cannot open {}: {e}", path.display())))?;
    if offset > 0 {
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|e| DictationError::Io(format!("cannot seek dictation audio: {e}")))?;
    }
    Ok(file)
}

async fn run_segment(
    session_id: &str,
    cfg: &crate::config::DictationConfig,
    state: &mut TranscriptState,
    conn: &mut LiveConnection,
    read_file: &mut tokio::fs::File,
    audio_rx: &mut watch::Receiver<AudioStatus>,
    stop: &CancellationToken,
    cancel: &CancellationToken,
    capture_cancel: &CancellationToken,
) -> Result<SegmentOutcome, DictationError> {
    state.session_started_at = Instant::now();
    state.last_text_at = Instant::now();
    state.last_speech_at = Instant::now();
    state.silence_since = None;
    state.listening = false;

    send_json(
        &conn.out_tx,
        json!({ "realtimeInput": { "activityStart": {} } }),
    )
    .await?;
    let out_tx = conn.out_tx.clone();
    let server_rx = &mut conn.server_rx;

    let mut send_tick = tokio::time::interval(Duration::from_millis(AUDIO_FRAME_DURATION_MS));
    send_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut health_tick = tokio::time::interval(Duration::from_millis(HEALTH_TICK_MS));
    health_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            biased;

            _ = cancel.cancelled() => return Ok(SegmentOutcome::Cancelled),

            _ = stop.cancelled(), if !state.stopping => {
                state.stopping = true;
                capture_cancel.cancel();
            }

            changed = audio_rx.changed() => {
                if changed.is_err() {
                    state.capture_ended = true;
                } else {
                    let status = audio_rx.borrow_and_update().clone();
                    state.latest_total_bytes = status.total_bytes;
                    if let Some(error) = status.error {
                        return Err(DictationError::AudioCapture(error));
                    }
                    if status.capture_ended {
                        state.capture_ended = true;
                    }
                    if status.rms >= cfg.speech_energy_threshold {
                        state.last_speech_at = Instant::now();
                        state.silence_since = None;
                    } else if state.silence_since.is_none() {
                        state.silence_since = Some(Instant::now());
                    }
                }
            }

            event = server_rx.recv() => {
                match event {
                    Some(ServerEvent::Interim(text)) => {
                        // Interim hypotheses are display-only. Committing them mixes speculative
                        // revisions into the authoritative transcript and produces duplicated
                        // full-session text when the API later re-emits a corrected revision.
                        state.interim_text = text;
                        state.last_text_at = Instant::now();
                        emit_interim(session_id, &state.full_text());
                    }
                    Some(ServerEvent::Transcription { text, finished }) => {
                        let full = state.apply_transcription(&text, finished);
                        state.last_text_at = Instant::now();
                        emit_interim(session_id, &full);
                    }
                    Some(ServerEvent::TurnComplete) => {
                        let full = state.commit_current_turn();
                        state.committed_offset = state.session_sent_offset;
                        state.recovery_attempts = 0;
                        emit_interim(session_id, &full);
                    }
                    Some(ServerEvent::GoAway) => {
                        return Ok(SegmentOutcome::Rollover);
                    }
                    Some(ServerEvent::Error(message)) => {
                        return Ok(SegmentOutcome::Recover(message));
                    }
                    Some(ServerEvent::Closed) | None => {
                        return Ok(SegmentOutcome::Recover("websocket closed".to_string()));
                    }
                }
            }

            _ = send_tick.tick() => {
                if state.session_sent_offset < state.latest_total_bytes {
                    let available = (state.latest_total_bytes - state.session_sent_offset) as usize;
                    let len = available.min(AUDIO_FRAME_BYTES_COUNT);
                    match send_pcm_chunk(&out_tx, read_file, len).await {
                        Ok(sent) => {
                            state.session_sent_offset = state.session_sent_offset.saturating_add(sent as u64);
                            if !state.listening {
                                state.listening = true;
                                set_state(
                                    Some(session_id.to_string()),
                                    DictationState::Listening,
                                    None,
                                    None,
                                );
                            }
                        }
                        Err(DictationError::Task(_)) => {
                            return Ok(SegmentOutcome::Recover("outbound queue full".to_string()));
                        }
                        Err(e) => return Err(e),
                    }
                }
            }

            _ = health_tick.tick() => {
                let now = Instant::now();

                if state.capture_ended
                    && state.session_sent_offset >= state.latest_total_bytes
                {
                    return Ok(SegmentOutcome::Finalize);
                }

                if !state.stopping {
                    let age = now.duration_since(state.session_started_at).as_secs();
                    let silence_ms = match state.silence_since {
                        Some(start) => now.duration_since(start).as_millis() as u64,
                        None => 0,
                    };
                    if age >= cfg.rollover_min_session_seconds
                        && silence_ms >= cfg.rollover_silence_ms
                        && state.session_sent_offset >= state.latest_total_bytes
                    {
                        return Ok(SegmentOutcome::Rollover);
                    }

                    let speech_recent =
                        now.duration_since(state.last_speech_at) <= Duration::from_secs(3);
                    let last_text_age = now.duration_since(state.last_text_at).as_secs();
                    let since_recovery =
                        now.duration_since(state.last_recovery_at).as_secs();
                    if speech_recent
                        && last_text_age >= cfg.watchdog_silence_seconds
                        && since_recovery >= cfg.recovery_min_interval_seconds
                    {
                        return Ok(SegmentOutcome::Recover("speech_stall".to_string()));
                    }
                }
            }
        }
    }
}

async fn finalize_segment(
    state: &mut TranscriptState,
    conn: &mut LiveConnection,
    timeout_seconds: u64,
) -> Finalization {
    if let Err(e) = send_json(
        &conn.out_tx,
        json!({ "realtimeInput": { "activityEnd": {} } }),
    )
    .await
    {
        crate::logger::log("WARN", "dictation", &format!("Could not send activityEnd: {e}"));
    }
    if let Err(e) = send_json(
        &conn.out_tx,
        json!({ "realtimeInput": { "audioStreamEnd": true } }),
    )
    .await
    {
        crate::logger::log("WARN", "dictation", &format!("Could not send audioStreamEnd: {e}"));
    }

    let server_rx = &mut conn.server_rx;
    let deadline = Instant::now() + Duration::from_secs(timeout_seconds);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            crate::logger::log(
                "WARN",
                "dictation",
                &format!(
                    "Live session did not confirm its final segment within {timeout_seconds}s"
                ),
            );
            return Finalization::TimedOut;
        }

        tokio::select! {
            event = server_rx.recv() => {
                match event {
                    Some(ServerEvent::Interim(text)) => {
                        state.interim_text = text;
                    }
                    Some(ServerEvent::Transcription { text, finished }) => {
                        state.apply_transcription(&text, finished);
                    }
                    Some(ServerEvent::TurnComplete) => {
                        state.commit_current_turn();
                        state.committed_offset = state.session_sent_offset;
                        return Finalization::Completed;
                    }
                    Some(ServerEvent::GoAway) => {}
                    Some(ServerEvent::Error(message)) => {
                        crate::logger::log(
                            "WARN",
                            "dictation",
                            &format!("Live session failed during finalization: {message}"),
                        );
                        return Finalization::Disconnected;
                    }
                    Some(ServerEvent::Closed) | None => return Finalization::Disconnected,
                }
            }
            _ = tokio::time::sleep(remaining) => {
                crate::logger::log(
                    "WARN",
                    "dictation",
                    &format!(
                        "Live session did not confirm its final segment within {timeout_seconds}s"
                    ),
                );
                return Finalization::TimedOut;
            }
        }
    }
}

async fn send_pcm_chunk(
    out_tx: &mpsc::Sender<TungsteniteMessage>,
    read_file: &mut tokio::fs::File,
    len: usize,
) -> Result<usize, DictationError> {
    if len == 0 {
        return Ok(0);
    }
    let mut bytes = vec![0u8; len];
    read_file
        .read_exact(&mut bytes)
        .await
        .map_err(|e| DictationError::Io(format!("cannot read dictation audio: {e}")))?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let frame = json!({
        "realtimeInput": {
            "audio": {
                "mimeType": "audio/pcm;rate=16000",
                "data": encoded
            }
        }
    });
    match out_tx.try_send(TungsteniteMessage::Text(frame.to_string().into())) {
        Ok(()) => Ok(len),
        Err(mpsc::error::TrySendError::Full(_)) => Err(DictationError::Task(
            "outbound queue full".to_string(),
        )),
        Err(mpsc::error::TrySendError::Closed(_)) => Err(DictationError::WebSocketConnect(
            "outbound channel closed".to_string(),
        )),
    }
}

async fn send_json(
    out_tx: &mpsc::Sender<TungsteniteMessage>,
    value: Value,
) -> Result<(), DictationError> {
    out_tx
        .send(TungsteniteMessage::Text(value.to_string().into()))
        .await
        .map_err(|_| DictationError::WebSocketConnect("outbound channel closed".to_string()))
}

#[derive(Debug)]
enum ServerEvent {
    Interim(String),
    Transcription { text: String, finished: bool },
    TurnComplete,
    GoAway,
    Error(String),
    Closed,
}

struct LiveConnection {
    out_tx: mpsc::Sender<TungsteniteMessage>,
    server_rx: mpsc::Receiver<ServerEvent>,
    reader: tokio::task::JoinHandle<()>,
    writer: tokio::task::JoinHandle<()>,
}

impl LiveConnection {
    async fn close(self) {
        self.reader.abort();
        self.writer.abort();
    }
}

async fn connect_with_retries(
    api_key: &str,
    cfg: &crate::config::DictationConfig,
    cancel: &CancellationToken,
) -> Result<LiveConnection, DictationError> {
    let attempts = cfg.reconnect_attempts_count.max(1);
    let mut last_error = None;
    for attempt in 1..=attempts {
        if cancel.is_cancelled() {
            return Err(DictationError::Cancelled);
        }
        match open_live_connection(api_key, cfg).await {
            Ok(connection) => {
                if attempt > 1 {
                    crate::logger::log(
                        "INFO",
                        "dictation",
                        &format!("Gemini Live connected after {attempt} attempts"),
                    );
                }
                return Ok(connection);
            }
            Err(e) => {
                crate::logger::log(
                    "WARN",
                    "dictation",
                    &format!("Gemini Live connection attempt {attempt}/{attempts} failed: {e}"),
                );
                last_error = Some(e);
                if attempt < attempts {
                    let delay_ms = (RECONNECT_BACKOFF_BASE_MS << (attempt - 1).min(4))
                        .min(RECONNECT_BACKOFF_MAX_MS);
                    tokio::select! {
                        _ = cancel.cancelled() => return Err(DictationError::Cancelled),
                        _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => {}
                    }
                }
            }
        }
    }
    match last_error {
        Some(error) => Err(error),
        None => Err(DictationError::WebSocketConnect(
            "Gemini Live connection failed".to_string(),
        )),
    }
}

async fn open_live_connection(
    api_key: &str,
    cfg: &crate::config::DictationConfig,
) -> Result<LiveConnection, DictationError> {
    let url = format!("{GEMINI_LIVE_WS_BASE}?key={api_key}");
    let (ws, _) = tokio::time::timeout(
        Duration::from_secs(SETUP_TIMEOUT_SECONDS),
        tokio_tungstenite::connect_async(&url),
    )
    .await
    .map_err(|_| {
        DictationError::WebSocketConnect("timed out opening the Gemini Live WebSocket".to_string())
    })?
    .map_err(|e| DictationError::WebSocketConnect(e.to_string()))?;

    let (mut sink, mut stream) = ws.split();
    let setup = build_setup_payload(cfg);
    sink.send(TungsteniteMessage::Text(setup.to_string().into()))
        .await
        .map_err(|e| DictationError::WebSocketConnect(e.to_string()))?;

    tokio::time::timeout(Duration::from_secs(SETUP_TIMEOUT_SECONDS), async {
        while let Some(message) = stream.next().await {
            let message = message.map_err(|e| DictationError::WebSocketConnect(e.to_string()))?;
            let text = match message {
                TungsteniteMessage::Text(text) => text.to_string(),
                TungsteniteMessage::Binary(bytes) => String::from_utf8_lossy(&bytes).to_string(),
                TungsteniteMessage::Close(frame) => {
                    return Err(DictationError::WebSocketConnect(format!(
                        "Gemini Live closed before setupComplete: {frame:?}"
                    )));
                }
                _ => continue,
            };
            let Ok(value) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            if let Some(error) = value.get("error") {
                return Err(classify_api_error(&error.to_string()));
            }
            if value.get("setupComplete").is_some() {
                return Ok(());
            }
        }
        Err(DictationError::WebSocketConnect(
            "Gemini Live WebSocket ended before setupComplete".to_string(),
        ))
    })
    .await
    .map_err(|_| {
        DictationError::WebSocketConnect("timed out waiting for Gemini Live setupComplete".to_string())
    })??;

    let (out_tx, mut out_rx) = mpsc::channel::<TungsteniteMessage>(OUTBOUND_QUEUE_CAPACITY_COUNT);
    let (server_tx, server_rx) =
        mpsc::channel::<ServerEvent>(SERVER_EVENT_QUEUE_CAPACITY_COUNT);

    let writer = tokio::spawn(async move {
        while let Some(message) = out_rx.recv().await {
            if sink.send(message).await.is_err() {
                break;
            }
        }
    });

    let reader = tokio::spawn(async move {
        while let Some(message) = stream.next().await {
            let events = match message {
                Ok(TungsteniteMessage::Text(text)) => parse_server_events(&text),
                Ok(TungsteniteMessage::Binary(bytes)) => {
                    parse_server_events(&String::from_utf8_lossy(&bytes))
                }
                Ok(TungsteniteMessage::Close(frame)) => {
                    if server_tx
                        .send(ServerEvent::Error(format!("websocket closed: {frame:?}")))
                        .await
                        .is_err()
                    {
                        return;
                    }
                    break;
                }
                Ok(_) => continue,
                Err(e) => {
                    if server_tx.send(ServerEvent::Error(e.to_string())).await.is_err() {
                        return;
                    }
                    break;
                }
            };
            for event in events {
                if server_tx.send(event).await.is_err() {
                    return;
                }
            }
        }
        if server_tx.send(ServerEvent::Closed).await.is_err() {
            return;
        }
    });

    Ok(LiveConnection {
        out_tx,
        server_rx,
        reader,
        writer,
    })
}

fn build_setup_payload(cfg: &crate::config::DictationConfig) -> Value {
    json!({
        "setup": {
            "model": DICTATION_MODEL,
            "generationConfig": {
                "responseModalities": ["TEXT"]
            },
            "inputAudioTranscription": {
                "mode": cfg.mode.live_value()
            },
            "realtimeInputConfig": {
                "automaticActivityDetection": {
                    "disabled": true
                }
            }
        }
    })
}

fn classify_api_error(message: &str) -> DictationError {
    let lower = message.to_lowercase();
    let status = if lower.contains("429")
        || lower.contains("quota")
        || lower.contains("resource_exhausted")
        || lower.contains("resourceexhausted")
    {
        429
    } else if lower.contains("401")
        || lower.contains("403")
        || lower.contains("api key")
        || lower.contains("unauthenticated")
        || lower.contains("permission_denied")
        || lower.contains("permissiondenied")
    {
        401
    } else if lower.contains("500")
        || lower.contains("502")
        || lower.contains("503")
        || lower.contains("504")
        || lower.contains("unavailable")
    {
        503
    } else {
        502
    };
    DictationError::Api {
        status,
        message: message.to_string(),
    }
}

fn parse_server_events(text: &str) -> Vec<ServerEvent> {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let mut events = Vec::new();
    if let Some(error) = value.get("error") {
        events.push(ServerEvent::Error(error.to_string()));
        return events;
    }
    if value.get("goAway").is_some() {
        events.push(ServerEvent::GoAway);
    }
    if let Some(content) = value.get("serverContent") {
        if let Some(interim) = content
            .get("interimInputTranscription")
            .and_then(|v| v.get("text"))
            .and_then(|v| v.as_str())
        {
            if !interim.trim().is_empty() {
                events.push(ServerEvent::Interim(interim.to_string()));
            }
        }
        if let Some(transcription) = content.get("inputTranscription") {
            let text = match transcription.get("text").and_then(|v| v.as_str()) {
                Some(value) => value.to_string(),
                None => String::new(),
            };
            let finished = match transcription.get("finished").and_then(|v| v.as_bool()) {
                Some(value) => value,
                None => false,
            };
            if !text.trim().is_empty() || finished {
                events.push(ServerEvent::Transcription { text, finished });
            }
        }
        if let Some(turn_complete) = content.get("turnComplete").and_then(|v| v.as_bool()) {
            if turn_complete {
                events.push(ServerEvent::TurnComplete);
            }
        }
    }
    events
}

fn normalize_for_compare(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn comparable_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|word| {
            word.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect()
}

fn word_coverage(base_words: &[String], candidate_words: &[String]) -> f64 {
    if base_words.is_empty() {
        return 0.0;
    }
    let mut remaining: HashMap<&str, usize> = HashMap::with_capacity(base_words.len());
    for word in base_words {
        *remaining.entry(word.as_str()).or_insert(0) += 1;
    }
    let mut matched = 0usize;
    for word in candidate_words {
        if let Some(count) = remaining.get_mut(word.as_str()) {
            if *count > 0 {
                *count -= 1;
                matched += 1;
            }
        }
    }
    matched as f64 / base_words.len() as f64
}

fn is_cumulative_revision(base: &str, candidate: &str) -> bool {
    let base_words = comparable_words(base);
    let candidate_words = comparable_words(candidate);
    if base_words.is_empty() || candidate_words.is_empty() {
        return false;
    }
    let normalized_base = base_words.join(" ");
    let normalized_candidate = candidate_words.join(" ");
    if normalized_candidate.starts_with(&normalized_base) {
        return true;
    }

    // The Live transcriber periodically re-emits the complete transcript with corrections,
    // including corrections near the beginning. A prefix check alone misses those revisions and
    // would append the whole transcript again. High word coverage plus a comparable length is the
    // reliable signal that `candidate` supersedes `base` instead of extending it.
    if candidate_words.len() >= base_words.len() * 8 / 10
        && word_coverage(&base_words, &candidate_words) >= 0.70
    {
        return true;
    }

    if base_words.len() >= 5 && candidate_words.len() >= (base_words.len() * 8 / 10) {
        if base_words.iter().take(5).eq(candidate_words.iter().take(5)) {
            return true;
        }
        let prefix_len = base_words.len().min(candidate_words.len()).min(10);
        let matches = base_words
            .iter()
            .take(prefix_len)
            .zip(candidate_words.iter())
            .filter(|(a, b)| a == b)
            .count();
        if matches >= 4.max(prefix_len * 8 / 10) {
            return true;
        }
    }
    false
}

fn join_segments(base: &str, addition: &str) -> String {
    let base = base.trim();
    let addition = addition.trim();
    if addition.is_empty() {
        return base.to_string();
    }
    if base.is_empty() {
        return addition.to_string();
    }
    if is_cumulative_revision(base, addition) {
        return addition.to_string();
    }
    if is_cumulative_revision(addition, base) {
        return base.to_string();
    }

    let normalized_base = normalize_for_compare(base);
    let normalized_addition = normalize_for_compare(addition);
    if !normalized_addition.is_empty() && normalized_base.ends_with(&normalized_addition) {
        return base.to_string();
    }

    let base_words: Vec<&str> = base.split_whitespace().collect();
    let addition_words: Vec<&str> = addition.split_whitespace().collect();
    let max_overlap = base_words.len().min(addition_words.len()).min(30);
    for overlap in (1..=max_overlap).rev() {
        let tail: Vec<String> = base_words[base_words.len() - overlap..]
            .iter()
            .map(|word| {
                word.trim_matches(|c: char| !c.is_alphanumeric())
                    .to_lowercase()
            })
            .collect();
        let head: Vec<String> = addition_words[..overlap]
            .iter()
            .map(|word| {
                word.trim_matches(|c: char| !c.is_alphanumeric())
                    .to_lowercase()
            })
            .collect();
        if tail == head {
            let remaining = addition_words[overlap..].join(" ");
            if remaining.is_empty() {
                return base.to_string();
            }
            return format!("{base} {remaining}");
        }
    }

    format!("{base} {addition}")
}

fn merge_turn_update(current_turn: &str, incoming: &str) -> String {
    let current = current_turn.trim();
    let incoming = incoming.trim();
    if incoming.is_empty() {
        return current.to_string();
    }
    if current.is_empty() {
        return incoming.to_string();
    }
    let normalized_current = normalize_for_compare(current);
    let normalized_incoming = normalize_for_compare(incoming);
    if normalized_incoming.starts_with(&normalized_current) {
        return incoming.to_string();
    }
    if normalized_current.starts_with(&normalized_incoming) {
        return if incoming.len() >= current.len() * 6 / 10 {
            incoming.to_string()
        } else {
            current.to_string()
        };
    }
    join_segments(current, incoming)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> crate::config::DictationConfig {
        crate::config::DictationConfig {
            enabled: true,
            mode: DictationMode::Smart,
            auto_send: true,
            speech_energy_threshold: 0.03,
            rollover_min_session_seconds: 90,
            rollover_silence_ms: 800,
            rollover_finalize_timeout_seconds: 5,
            watchdog_silence_seconds: 10,
            finalize_timeout_seconds: 120,
            reconnect_attempts_count: 5,
            recovery_min_interval_seconds: 5,
            stale_audio_retention_hours: 24,
        }
    }

    #[test]
    fn setup_payload_uses_transcribe_live_and_requested_mode() {
        let mut cfg = test_config();
        let smart = build_setup_payload(&cfg);
        assert_eq!(smart["setup"]["model"], DICTATION_MODEL);
        assert_eq!(
            smart["setup"]["generationConfig"]["responseModalities"][0],
            "TEXT"
        );
        assert_eq!(
            smart["setup"]["inputAudioTranscription"]["mode"],
            "SMART"
        );
        assert_eq!(
            smart["setup"]["realtimeInputConfig"]["automaticActivityDetection"]["disabled"],
            true
        );

        cfg.mode = DictationMode::Verbatim;
        let verbatim = build_setup_payload(&cfg);
        assert_eq!(
            verbatim["setup"]["inputAudioTranscription"]["mode"],
            "VERBATIM"
        );
    }

    #[test]
    fn cumulative_turn_updates_do_not_duplicate() {
        let mut state = TranscriptState::new();
        state.current_turn = merge_turn_update(&state.current_turn, "Hola");
        assert_eq!(state.current_turn, "Hola");
        state.current_turn = merge_turn_update(&state.current_turn, "Hola mundo");
        assert_eq!(state.current_turn, "Hola mundo");
        state.current_turn = merge_turn_update(&state.current_turn, "Hola mundo cruel.");
        let full = state.commit_current_turn();
        assert_eq!(full, "Hola mundo cruel.");
        assert!(state.current_turn.is_empty());
    }

    #[test]
    fn overlapping_and_repeated_segments_are_deduplicated() {
        let mut state = TranscriptState::new();
        state.committed_text = join_segments(
            &state.committed_text,
            "Recientemente hemos tenido problemas.",
        );
        state.committed_text = join_segments(
            &state.committed_text,
            "Recientemente hemos tenido problemas.",
        );
        assert_eq!(
            state.committed_text,
            "Recientemente hemos tenido problemas."
        );
        state.committed_text = join_segments(
            &state.committed_text,
            "tenido problemas con la calidad.",
        );
        assert_eq!(
            state.committed_text,
            "Recientemente hemos tenido problemas. con la calidad."
        );
    }

    #[test]
    fn cumulative_smart_revisions_replace_instead_of_appending() {
        let first = "Recientemente estuvimos trabajando en una estandarización visual. Así que quedan cosas.";
        let second = "Recientemente estuvimos trabajando en una estandarización visual. Aún quedan cosas. Hay dos mejoras.";
        let mut state = TranscriptState::new();
        state.committed_text = join_segments(&state.committed_text, first);
        state.committed_text = join_segments(&state.committed_text, second);
        assert_eq!(state.committed_text, second);
    }

    #[test]
    fn full_session_revisions_with_early_corrections_replace_base_text() {
        let first = "2400 Ya este es un Ya esta es una prueba. de dictado eh en tiempo real usando el motor local de dictado Adiós";
        let revised = "Ya, esta es una prueba de dictado eh en tiempo real usando el motor local de dictado. Adiós OpenDictate. Te extrañaremos. Ajá, tenemos audio doblado.";
        let mut state = TranscriptState::new();
        state.committed_text = join_segments(&state.committed_text, first);
        state.committed_text = join_segments(&state.committed_text, revised);
        assert_eq!(state.committed_text, revised);
    }

    #[test]
    fn authoritative_full_session_revisions_replace_committed_text() {
        let first = "2400 Ya este es un Ya esta es una prueba. de dictado eh en tiempo real usando el motor local de dictado Adiós";
        let revised = "Ya, esta es una prueba de dictado eh en tiempo real usando el motor local de dictado. Adiós OpenDictate. Te extrañaremos. Ajá, tenemos audio doblado.";
        let mut state = TranscriptState::new();
        state.apply_transcription(first, false);
        let full = state.apply_transcription(revised, true);
        assert_eq!(full, revised);
        assert_eq!(state.committed_text, revised);
    }

    #[test]
    fn interim_hypotheses_are_display_only_and_never_committed() {
        let mut state = TranscriptState::new();
        state.committed_text = "Primera frase.".to_string();
        state.interim_text = "Segunda frase provisional.".to_string();
        assert!(state.full_text().contains("Segunda frase provisional."));
        let committed = state.commit_current_turn();
        assert_eq!(committed, "Primera frase.");
        assert!(state.interim_text.is_empty());
    }

    #[test]
    fn new_utterances_without_shared_content_are_appended() {
        let first = "Hola mundo.";
        let next = "Comprar pan mañana.";
        let mut state = TranscriptState::new();
        state.committed_text = join_segments(&state.committed_text, first);
        state.committed_text = join_segments(&state.committed_text, next);
        assert_eq!(state.committed_text, "Hola mundo. Comprar pan mañana.");
    }

    #[test]
    fn api_errors_are_classified_for_the_frontend() {
        assert!(matches!(
            classify_api_error("429 RESOURCE_EXHAUSTED"),
            DictationError::Api { status: 429, .. }
        ));
        assert!(matches!(
            classify_api_error("API key not valid"),
            DictationError::Api { status: 401, .. }
        ));
        assert!(matches!(
            classify_api_error("503 UNAVAILABLE"),
            DictationError::Api { status: 503, .. }
        ));
    }
}
