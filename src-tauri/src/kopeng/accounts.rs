use std::process::Command;
use std::sync::OnceLock;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use hkdf::Hkdf;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use uuid::Uuid;

use crate::db::{
    db_delete_account, db_get_account_by_email, db_get_account_by_id,
    db_get_account_summaries, db_get_accounts,
    db_get_active_account_by_provider, db_insert_account,
    db_insert_account_with_provider, db_set_active_account,
    db_update_account_label, db_update_account_token,
};
pub use crate::db::{AccountRecord, AccountSummary};
use crate::db::DatabaseError;

/// Failures of multi-account management: token encryption, the OS keyring (`secret-tool`),
/// runner credential files and account bookkeeping. Tauri commands convert it to `String`.
#[derive(Debug, thiserror::Error)]
pub enum AccountsError {
    /// `~/.gemini/installation_id`, the key-derivation input, is missing or empty.
    #[error("Machine installation id unavailable at {path}: {details}")]
    InstallationIdUnavailable { path: String, details: String },

    /// Key derivation, encryption or authenticated decryption failed.
    #[error("Token {operation} failed: {details}")]
    Crypto { operation: &'static str, details: String },

    /// A decrypted or stored credential payload has the wrong shape.
    #[error("Invalid credential payload: {0}")]
    InvalidPayload(String),

    /// `secret-tool` could not run or reported a failure.
    #[error("Keyring {operation} failed: {details}")]
    Keyring { operation: &'static str, details: String },

    /// The account id is not registered.
    #[error("Account '{0}' not found")]
    AccountNotFound(String),

    /// The caller asked for something the account rules forbid (empty token, removing the active account).
    #[error("{0}")]
    InvalidRequest(String),

    /// The provider stored for an account is not one ONE-SHIP knows how to activate.
    #[error("Account '{account_id}' has unsupported provider '{provider}'")]
    UnsupportedProvider { account_id: String, provider: String },

    /// A runner credential file could not be read or written.
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    /// An external account service (e.g. the DeepSeek balance API) failed.
    #[error("{service} request failed: {details}")]
    Remote { service: &'static str, details: String },

    #[error(transparent)]
    Database(#[from] DatabaseError),
}

impl crate::api::ApiError for AccountsError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::AccountNotFound(_) => StatusCode::NOT_FOUND,
            Self::InvalidRequest(_) | Self::UnsupportedProvider { .. } => StatusCode::BAD_REQUEST,
            Self::Remote { .. } => StatusCode::BAD_GATEWAY,
            Self::InstallationIdUnavailable { .. }
            | Self::Crypto { .. }
            | Self::InvalidPayload(_)
            | Self::Keyring { .. }
            | Self::Io { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Database(e) => e.status(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::InstallationIdUnavailable { .. } => "AccountsError::InstallationIdUnavailable",
            Self::Crypto { .. } => "AccountsError::Crypto",
            Self::InvalidPayload(_) => "AccountsError::InvalidPayload",
            Self::Keyring { .. } => "AccountsError::Keyring",
            Self::AccountNotFound(_) => "AccountsError::AccountNotFound",
            Self::InvalidRequest(_) => "AccountsError::InvalidRequest",
            Self::UnsupportedProvider { .. } => "AccountsError::UnsupportedProvider",
            Self::Io { .. } => "AccountsError::Io",
            Self::Remote { .. } => "AccountsError::Remote",
            Self::Database(e) => e.code(),
        }
    }
}

static KEYRING_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

/// Global centralized async mutex protecting the OS keyring from concurrent mutations.
pub fn keyring_lock() -> &'static tokio::sync::Mutex<()> {
    KEYRING_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

#[derive(Serialize, Deserialize, Debug, Clone, ts_rs::TS)]
#[ts(export)]
pub struct AgySession {
    pub pid: u32,
    pub agent_key: Option<String>,
    pub conversation_id: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
}

/// Stable machine identifier used as key-derivation input: `~/.gemini/installation_id`.
///
/// There is deliberately no second source (such as `/etc/machine-id`): a different input would
/// derive a different key and silently make every stored token undecryptable.
pub fn get_installation_id() -> Result<String, AccountsError> {
    let path = dirs::home_dir()
        .ok_or_else(|| AccountsError::InstallationIdUnavailable { path: "~/.gemini/installation_id".to_string(), details: "home directory is unknown".to_string() })?
        .join(".gemini")
        .join("installation_id");
    let unavailable = |details: String| AccountsError::InstallationIdUnavailable { path: path.display().to_string(), details };
    let content = std::fs::read_to_string(&path).map_err(|e| unavailable(e.to_string()))?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err(unavailable("file is empty".to_string()));
    }
    Ok(trimmed.to_string())
}

pub const CURRENT_ACCOUNT_SALT: &[u8] = b"one-ship-accounts-v1";

/// Derives a 256-bit symmetric encryption key using HKDF-SHA256 from the machine installation ID.
pub fn derive_master_key() -> Result<[u8; 32], AccountsError> {
    let ikm = get_installation_id()?;
    derive_key_from_ikm(&ikm)
}

/// Derives key from input key material with a specific salt.
pub fn derive_key_with_salt(ikm: &str, salt: &[u8]) -> Result<[u8; 32], AccountsError> {
    let info = b"account-token-encryption";
    let hk = Hkdf::<Sha256>::new(Some(salt), ikm.as_bytes());
    let mut okm = [0u8; 32];
    hk.expand(info, &mut okm)
        .map_err(|e| AccountsError::Crypto { operation: "key derivation", details: format!("{:?}", e) })?;
    Ok(okm)
}

/// Internal helper for HKDF key derivation from any input key material using canonical salt.
pub fn derive_key_from_ikm(ikm: &str) -> Result<[u8; 32], AccountsError> {
    derive_key_with_salt(ikm, CURRENT_ACCOUNT_SALT)
}

/// Encrypts a refresh token using AES-256-GCM. Returns (12-byte nonce || ciphertext + auth tag).
pub fn encrypt_refresh_token(refresh_token: &str) -> Result<Vec<u8>, AccountsError> {
    let key_bytes = derive_master_key()?;
    encrypt_refresh_token_with_key(refresh_token, &key_bytes)
}

pub fn encrypt_refresh_token_with_key(refresh_token: &str, key_bytes: &[u8; 32]) -> Result<Vec<u8>, AccountsError> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key_bytes));
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let payload = serde_json::json!({
        "refresh_token": refresh_token
    }).to_string();

    let ciphertext = cipher
        .encrypt(nonce, payload.as_bytes())
        .map_err(|e| AccountsError::Crypto { operation: "encryption", details: format!("{:?}", e) })?;

    let mut result = Vec::with_capacity(12 + ciphertext.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&ciphertext);
    Ok(result)
}

/// Decrypts encrypted token bytes (12-byte nonce || ciphertext + auth tag) back to the refresh token string.
pub fn decrypt_refresh_token(encrypted_data: &[u8]) -> Result<String, AccountsError> {
    let key_bytes = derive_master_key()?;
    decrypt_refresh_token_with_key(encrypted_data, &key_bytes)
}

pub fn decrypt_refresh_token_with_key(encrypted_data: &[u8], key_bytes: &[u8; 32]) -> Result<String, AccountsError> {
    if encrypted_data.len() < 12 {
        return Err(AccountsError::InvalidPayload("encrypted token is shorter than its 12-byte nonce".to_string()));
    }
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key_bytes));
    let (nonce_bytes, ciphertext) = encrypted_data.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| AccountsError::Crypto { operation: "authenticated decryption", details: format!("{:?}", e) })?;

    let json_val: Value = serde_json::from_slice(&plaintext)
        .map_err(|e| AccountsError::InvalidPayload(format!("decrypted token is not JSON: {}", e)))?;

    json_val
        .get("refresh_token")
        .or_else(|| json_val.get("api_key"))
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| AccountsError::InvalidPayload("decrypted token has no refresh_token/api_key".to_string()))
}

/// Builds the canonical keyring JSON payload expected by `agy` and ONE-SHIP account sync.
pub fn build_keyring_payload(refresh_token: &str, email: &str) -> Value {
    serde_json::json!({
        "token": {
            "access_token": "",
            "token_type": "Bearer",
            "refresh_token": refresh_token,
            "expiry": "2000-01-01T00:00:00Z"
        },
        "auth_method": "consumer",
        "id_token_email": email
    })
}

/// Extracts the non-empty `id_token_email` claim from a keyring JSON payload.
pub fn extract_email_from_keyring_token(token_val: &Value) -> Option<String> {
    token_val
        .get("id_token_email")
        .and_then(|e| e.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// Registers or updates an account in the local SQLite database.
pub fn save_account(email: &str, refresh_token: &str, is_active: bool) -> Result<String, AccountsError> {
    let enc_token = encrypt_refresh_token(refresh_token)?;
    let now = chrono_or_simple_iso();

    if let Some(existing) = db_get_account_by_email(email)? {
        db_update_account_token(email, &enc_token)?;
        if is_active && !existing.is_active {
            db_set_active_account(&existing.id)?;
        }
        Ok(existing.id)
    } else {
        let account_id = Uuid::new_v4().to_string();
        db_insert_account(&account_id, email, email, is_active, &enc_token, &now)?;
        Ok(account_id)
    }
}

/// Scans the Linux `/proc` filesystem to detect running `agy` sessions and extract their
/// active conversation UUID.
pub fn get_active_agy_sessions() -> Vec<AgySession> {
    let mut sessions = Vec::new();

    let proc_dir = match std::fs::read_dir("/proc") {
        Ok(d) => d,
        Err(_) => return sessions,
    };

    for entry in proc_dir.flatten() {
        let file_name = entry.file_name();
        let pid_str = match file_name.to_str() {
            Some(s) => s,
            None => continue,
        };

        let pid: u32 = match pid_str.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        let path = entry.path();

        // 1. Check cmdline
        let cmdline_path = path.join("cmdline");
        let cmdline_bytes = match std::fs::read(&cmdline_path) {
            Ok(b) if !b.is_empty() => b,
            _ => continue,
        };

        let args: Vec<String> = cmdline_bytes
            .split(|&b| b == 0)
            .filter(|chunk| !chunk.is_empty())
            .map(|chunk| String::from_utf8_lossy(chunk).to_string())
            .collect();

        if args.is_empty() {
            continue;
        }

        // Check if executable basename is "agy"
        let exe_path = &args[0];
        let is_agy = std::path::Path::new(exe_path)
            .file_name()
            .and_then(|f| f.to_str())
            .map(|f| f == "agy")
            .unwrap_or(false)
            || exe_path == "agy"
            || args.iter().any(|a| a == "agy" || a.ends_with("/agy"));

        if !is_agy {
            continue;
        }

        // 2. Extract conversation_id from cmdline args, or fallback to inspecting /proc/<pid>/fd symlinks
        let mut conversation_id = None;
        let mut i = 0;
        while i < args.len() {
            if args[i] == "--conversation" || args[i] == "-c" {
                if i + 1 < args.len() {
                    conversation_id = Some(args[i + 1].clone());
                    break;
                }
            } else if let Some(stripped) = args[i].strip_prefix("--conversation=") {
                conversation_id = Some(stripped.to_string());
                break;
            }
            i += 1;
        }

        if conversation_id.is_none() {
            let fd_dir = path.join("fd");
            if let Ok(fd_entries) = std::fs::read_dir(fd_dir) {
                for fd_entry in fd_entries.flatten() {
                    if let Ok(target) = std::fs::read_link(fd_entry.path()) {
                        let target_str = target.to_string_lossy();
                        if target_str.contains("/antigravity-cli/conversations/") && target_str.ends_with(".db") {
                            if let Some(stem) = target.file_stem().and_then(|s| s.to_str()) {
                                if !stem.is_empty() {
                                    conversation_id = Some(stem.to_string());
                                    break;
                                }
                            }
                        } else if target_str.contains("/antigravity-cli/presence/") && target_str.ends_with(".lock") {
                            if let Some(stem) = target.file_stem().and_then(|s| s.to_str()) {
                                if !stem.is_empty() {
                                    conversation_id = Some(stem.to_string());
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        // 3. Match agent_key by PID from the native ProcessSupervisor
        let agent_key = crate::supervisor::get_supervisor().get_agent_key_by_pid(pid);

        let cwd = std::fs::read_link(path.join("cwd"))
            .ok()
            .and_then(|p| p.to_str().map(|s| s.to_string()));

        sessions.push(AgySession {
            pid,
            agent_key,
            conversation_id,
            cwd,
        });
    }

    sessions
}


/// Reads the raw credentials JSON blob currently stored in the system keyring.
///
/// `Ok(None)` means nothing is stored (no `agy` login yet), a normal state. An unreachable
/// Secret Service, or a stored blob that is not JSON, is an error.
pub fn read_keyring_token() -> Result<Option<Value>, AccountsError> {
    let raw = crate::secret_store::run_sync(async {
        crate::secret_store::lookup_pair("gemini", "antigravity").await
    })
    .map_err(|e| AccountsError::Keyring {
        operation: "lookup",
        details: e.to_string(),
    })?;

    let Some(stdout_str) = raw else {
        return Ok(None);
    };

    serde_json::from_str(&stdout_str)
        .map(Some)
        .map_err(|e| AccountsError::InvalidPayload(format!("keyring token is not JSON: {}", e)))
}

/// Stores an updated credentials JSON blob into the system keyring.
pub fn write_keyring_token(payload: &Value) -> Result<(), AccountsError> {
    let payload_str = payload.to_string();
    crate::secret_store::run_sync(async move {
        crate::secret_store::store_pair(
            "Password for 'antigravity' on 'gemini'",
            "gemini",
            "antigravity",
            &payload_str,
        )
        .await
    })
    .map_err(|e| AccountsError::Keyring {
        operation: "store",
        details: e.to_string(),
    })
}

/// Clears the Antigravity entry from the system keyring.
pub fn clear_keyring_token() -> Result<(), AccountsError> {
    crate::secret_store::run_sync(async {
        crate::secret_store::clear_pair("gemini", "antigravity").await
    })
    .map_err(|e| AccountsError::Keyring {
        operation: "clear",
        details: e.to_string(),
    })
}

/// Ensures that the active account registered in SQLite is synchronized to the OS keyring.
/// If the keyring is missing, empty, or points to a different account, this function
/// decrypts and writes the active account credentials into the keyring.
pub fn ensure_active_account_in_keyring() -> Result<(), AccountsError> {
    let accounts = db_get_accounts()?;
    let active = match accounts.into_iter().find(|a| a.is_active && (a.provider == "google" || a.provider.is_empty())) {
        Some(a) => a,
        None => return Ok(()), // No registered active Google account
    };

    let current_keyring = read_keyring_token()?;
    let current_email = current_keyring.as_ref().and_then(extract_email_from_keyring_token);

    if let Some(ref email) = current_email {
        if email == &active.email {
            let has_refresh = current_keyring.as_ref()
                .and_then(|t| t.get("token"))
                .and_then(|t| t.get("refresh_token"))
                .and_then(|r| r.as_str())
                .map(|r| !r.is_empty())
                .unwrap_or(false);
            if has_refresh {
                return Ok(());
            }
        }
    }

    let refresh_token = decrypt_refresh_token(&active.enc_token)?;
    let payload = build_keyring_payload(&refresh_token, &active.email);

    write_keyring_token(&payload)?;
    crate::logger::log("INFO", "accounts", &format!("Synchronized active account '{}' to OS keyring", active.email));
    Ok(())
}

/// Checks the current keyring state, updates the active account or registers a newly detected account.
/// Returns Ok(Some((AccountSummary, is_new))) if a registered account was synced or discovered.
pub fn check_and_register_keyring_account() -> Result<Option<(AccountSummary, bool)>, AccountsError> {
    let Some(token_val) = read_keyring_token()? else {
        return Ok(None);
    };

    let refresh_token = match token_val
        .get("token")
        .and_then(|t| t.get("refresh_token"))
        .and_then(|r| r.as_str()) {
            Some(r) => r,
            None => return Ok(None),
        };

    let email = match extract_email_from_keyring_token(&token_val) {
        Some(e) => e,
        None => return Ok(None),
    };

    let existing = db_get_account_by_email(&email)?;
    let is_new = existing.is_none();

    let account_id = save_account(&email, refresh_token, is_new)?;
    if is_new {
        crate::logger::log(
            "INFO",
            "accounts",
            &format!("OAuth login detected: registered new account '{}'", email),
        );
    } else {
        crate::logger::log(
            "DEBUG",
            "accounts",
            &format!("Synced existing account credentials for '{}'", email),
        );
    }

    // Reflect the actual DB state rather than assuming is_active=true.
    let is_active = if is_new {
        existing.is_none() // first account set active by save_account; others stay false
    } else {
        existing.as_ref().map(|r| r.is_active).unwrap_or(false)
    };

    let summary = AccountSummary {
        id: account_id,
        label: email.clone(),
        email,
        is_active,
        added_at: chrono_or_simple_iso(),
        provider: "google".to_string(),
    };

    Ok(Some((summary, is_new)))
}

/// Check for active agy sessions running in the system or scoped to a project.
pub fn check_active_presence_sessions(project_path: Option<&str>) -> Result<Vec<AgySession>, AccountsError> {
    let all_sessions = get_active_agy_sessions();
    if let Some(proj_path) = project_path {
        if !proj_path.trim().is_empty() {
            let state = crate::supervisor::get_supervisor().compute_project_state(proj_path, "");
            let active_pids: Vec<u32> = state.agents.iter().filter_map(|a| a.pid).collect();
            let canonical_proj = std::fs::canonicalize(proj_path).unwrap_or_else(|_| std::path::PathBuf::from(proj_path));

            let filtered: Vec<AgySession> = all_sessions.into_iter().filter(|s| {
                if active_pids.contains(&s.pid) {
                    return true;
                }
                if let Some(ref c) = s.cwd {
                    let s_path = std::fs::canonicalize(c).unwrap_or_else(|_| std::path::PathBuf::from(c));
                    if s_path == canonical_proj {
                        return true;
                    }
                }
                false
            }).collect();
            return Ok(filtered);
        }
    }
    Ok(all_sessions)
}

/// Executes account switch sequence:
/// 1. If `relaunch_active` is true:
///    - Identifies active agents running in ProcessSupervisor.
///    - Gracefully stops them via ProcessSupervisor::stop_process.
/// 2. Swaps credentials in the system keyring and SQLite under keyring lock.
/// 3. If `relaunch_active` is true:
///    - Relaunches stopped agents via ProcessSupervisor::launch_agent,
///      which resumes active conversation IDs under the new credentials.
pub async fn switch_account(target_account_id: &str, relaunch_active: bool) -> Result<(), AccountsError> {
    let sup = crate::supervisor::get_supervisor();
    crate::logger::log(
        "DEBUG",
        "accounts",
        &format!(
            "Initiating account switch to '{}' (relaunch_active: {})",
            target_account_id, relaunch_active
        ),
    );

    // 1. Identify active agent processes if relaunch requested
    let active_agents: Vec<(String, String, String)> = if relaunch_active {
        let procs = sup.list_active_processes();
        let mut targets = Vec::new();
        for p in procs {
            if let (Some(k), Some(path), Some(aid)) = (
                p.get("key").and_then(|v| v.as_str()),
                p.get("project_path").and_then(|v| v.as_str()),
                p.get("id").and_then(|v| v.as_str()),
            ) {
                if !k.contains(":svc:") && !k.contains(":tsk:") {
                    targets.push((k.to_string(), path.to_string(), aid.to_string()));
                }
            }
        }
        targets
    } else {
        Vec::new()
    };

    // 2. Stop active agents if relaunch is requested
    if relaunch_active && !active_agents.is_empty() {
        crate::logger::log(
            "DEBUG",
            "accounts",
            &format!("Stopping {} active agents before switching account", active_agents.len()),
        );
        for (key, _, _) in &active_agents {
            crate::logger::log_failure("accounts", "stop agent before account switch", sup.stop_process(key).await);
        }
    }

    // 3. Swap credentials and set DB active account
    {
        let target_id = target_account_id.to_string();
        let target_record = tokio::task::spawn_blocking(move || db_get_account_by_id(&target_id))
            .await
            .map_err(|e| AccountsError::Io { context: "db_get_account_by_id task".to_string(), source: std::io::Error::other(e.to_string()) })??
            .ok_or_else(|| AccountsError::AccountNotFound(target_account_id.to_string()))?;

        if target_record.provider == "google" || target_record.provider.is_empty() {
            let _guard = keyring_lock().lock().await;
            let acc_id = target_account_id.to_string();
            let email = target_record.email.clone();
            let enc = target_record.enc_token.clone();
            tokio::task::spawn_blocking(move || -> Result<(), AccountsError> {
                let refresh_token = decrypt_refresh_token(&enc)?;
                let payload = build_keyring_payload(&refresh_token, &email);
                write_keyring_token(&payload)?;
                db_set_active_account(&acc_id)?;
                Ok(())
            })
            .await
            .map_err(|e| AccountsError::Io { context: "switch Google account task".to_string(), source: std::io::Error::other(e.to_string()) })??;

            crate::logger::log("INFO", "accounts", &format!("Switched to Google account: {}", target_record.email));
        } else if target_record.provider == "deepseek" {
            let acc_id = target_account_id.to_string();
            let enc = target_record.enc_token.clone();
            tokio::task::spawn_blocking(move || -> Result<(), AccountsError> {
                let api_key = decrypt_refresh_token(&enc)?;
                db_set_active_account(&acc_id)?;
                sync_opencode_auth_json(&api_key)?;
                Ok(())
            })
            .await
            .map_err(|e| AccountsError::Io { context: "switch DeepSeek account task".to_string(), source: std::io::Error::other(e.to_string()) })??;

            crate::logger::log("INFO", "accounts", &format!("Switched to DeepSeek account: {}", target_record.label));
        } else if target_record.provider == "claude" || target_record.provider == "anthropic" {
            let acc_id = target_account_id.to_string();
            tokio::task::spawn_blocking(move || db_set_active_account(&acc_id))
                .await
                .map_err(|e| AccountsError::Io { context: "switch Claude account task".to_string(), source: std::io::Error::other(e.to_string()) })??;
            crate::logger::log("INFO", "accounts", &format!("Switched to Claude account: {}", target_record.label));
        } else {
            return Err(AccountsError::UnsupportedProvider {
                account_id: target_account_id.to_string(),
                provider: target_record.provider.clone(),
            });
        }
    }

    // 4. Relaunch agents under new credentials
    if relaunch_active && !active_agents.is_empty() {
        crate::logger::log(
            "DEBUG",
            "accounts",
            &format!("Relaunching {} active agents under new credentials", active_agents.len()),
        );
        let projects = crate::db::db_get_projects()?;
        for (_, project_path, agent_id) in active_agents {
            let Some(project) = projects.iter().find(|p| p.path == project_path) else {
                crate::logger::log("ERROR", "accounts", &format!("Cannot relaunch '{}': '{}' is no longer a registered project", agent_id, project_path));
                continue;
            };
            crate::logger::log_failure(
                "accounts",
                "relaunch agent after account switch",
                crate::supervisor::launch_agent(project_path.clone(), project.name.clone(), agent_id, None, None, None, None, None).await,
            );
        }
    }

    Ok(())
}

/// Initiates adding an account by spawning an isolated `system.preferred_terminal` window running
/// `agy`. Protects existing credentials by backing them up in memory, clearing the keyring for
/// OAuth login, and restoring previous credentials if login times out. Daemon command.
pub async fn initiate_add_account() -> Result<(), AccountsError> {
    let _guard = keyring_lock().lock().await;
    let original_token = tokio::task::spawn_blocking(|| -> Result<Option<serde_json::Value>, AccountsError> {
        let orig = read_keyring_token()?;
        clear_keyring_token()?;
        Ok(orig)
    })
    .await
    .map_err(|e| AccountsError::Io { context: "prepare keyring for add account".to_string(), source: std::io::Error::other(e.to_string()) })??;

    let configured_term = crate::config::resolve_effective_string(None, None, "system", "preferred_terminal");
    let Some((term_name, term_path)) = crate::desktop::resolve_preferred_terminal(&configured_term) else {
        if let Some(ref orig) = original_token {
            crate::logger::log_failure("accounts", "restore keyring token", write_keyring_token(orig));
        }
        return Err(AccountsError::Io {
            context: format!("No supported terminal emulator found on PATH (configured '{}')", configured_term),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "terminal emulator not found"),
        });
    };
    crate::logger::log(
        "INFO",
        "accounts",
        &format!("Initiating OAuth add account flow using terminal '{}' ({})", term_name, term_path.display()),
    );
    let term_str = term_path.to_string_lossy().into_owned();
    let mut cmd = crate::desktop::desktop_command(&term_str);
    cmd.args(crate::desktop::build_terminal_exec_args(&term_name, "agy"));

    let mut child = cmd
        .spawn()
        .map_err(|e| {
            // Restore token if spawn failed
            if let Some(ref orig) = original_token {
                crate::logger::log_failure("accounts", "restore keyring token", write_keyring_token(orig));
            }
            AccountsError::Io { context: format!("Failed to spawn terminal '{}'", term_name), source: e }
        })?;

    let child_pid = child.id();

    // Spawn detached monitoring thread
    std::thread::spawn(move || {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(300); // 5 minutes
        let mut account_added = false;

        while start.elapsed() < timeout {
            std::thread::sleep(std::time::Duration::from_secs(1));

            if let Ok(Some((_summary, is_new))) = check_and_register_keyring_account() {
                if is_new {
                    account_added = true;
                    break;
                }
            }

            // Check if child exited
            if let Ok(Some(_)) = child.try_wait() {
                break;
            }
        }

        // Kill terminal process
        let _ = Command::new("kill")
            .args(["-TERM", &child_pid.to_string()])
            .output();

        // Restore original active token so active session context remains predictable
        if let Some(ref orig) = original_token {
            crate::logger::log_failure("accounts", "restore keyring token", write_keyring_token(orig));
        }

        if account_added {
            crate::logger::log("INFO", "accounts", "New account added and persisted successfully");
        } else {
            crate::logger::log("WARN", "accounts", "Add account flow completed without new account or timed out");
        }
    });

    Ok(())
}

/// Removes a registered account from SQLite. Cannot delete the currently active account
/// or the only remaining account for that provider.
pub fn remove_account(account_id: &str) -> Result<(), AccountsError> {
    let account = db_get_account_by_id(account_id)?
        .ok_or_else(|| AccountsError::AccountNotFound(account_id.to_string()))?;

    if account.is_active {
        return Err(AccountsError::InvalidRequest("Cannot remove the currently active account. Switch to another account first.".to_string()));
    }

    let all_accounts = db_get_accounts()?;
    let same_provider_accounts: Vec<_> = all_accounts.into_iter().filter(|a| a.provider == account.provider).collect();
    if same_provider_accounts.len() <= 1 {
        return Err(AccountsError::InvalidRequest(format!("Cannot remove the only registered {} account.", account.provider)));
    }

    db_delete_account(account_id)?;
    crate::logger::log("INFO", "accounts", &format!("Removed account: {}", account.email));
    Ok(())
}

/// Updates the display label for an account.
pub fn rename_account(account_id: &str, label: &str) -> Result<(), AccountsError> {
    db_update_account_label(account_id, label)?;
    crate::logger::log("INFO", "accounts", &format!("Updated account {} label to '{}'", account_id, label));
    Ok(())
}

/// Discovers DeepSeek API key from ~/.local/share/opencode/auth.json if present.
pub fn discover_opencode_deepseek_key() -> Option<String> {
    let home = dirs::home_dir()?;
    let auth_path = home.join(".local/share/opencode/auth.json");
    if !auth_path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(&auth_path).ok()?;
    let val: serde_json::Value = serde_json::from_str(&content).ok()?;
    val.get("deepseek")
        .and_then(|d| d.get("key"))
        .and_then(|k| k.as_str())
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
}

/// Discovers Claude Code OAuth account from ~/.claude/.credentials.json if present.
pub fn discover_claude_account() -> Option<(String, String, String)> {
    let home = dirs::home_dir()?;
    let creds_path = home.join(".claude/.credentials.json");
    if !creds_path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(&creds_path).ok()?;
    let root: serde_json::Value = serde_json::from_str(&content).ok()?;
    let oauth = root.get("claudeAiOauth")?;
    let token = oauth.get("accessToken").or_else(|| oauth.get("refreshToken")).and_then(|v| v.as_str())?;
    let sub_type = oauth.get("subscriptionType").and_then(|v| v.as_str()).unwrap_or("pro");

    let mut email = "claude.ai@user".to_string();
    let claude_json_path = home.join(".claude.json");
    if claude_json_path.exists() {
        if let Ok(c_content) = std::fs::read_to_string(&claude_json_path) {
            if let Ok(c_val) = serde_json::from_str::<serde_json::Value>(&c_content) {
                if let Some(e) = c_val.get("email").and_then(|v| v.as_str()) {
                    email = e.to_string();
                } else if let Some(e) = c_val.get("user").and_then(|u| u.get("email")).and_then(|v| v.as_str()) {
                    email = e.to_string();
                }
            }
        }
    }
    if email == "claude.ai@user" {
        if let Ok(output) = std::process::Command::new("claude").args(["auth", "status", "--json"]).output() {
            if output.status.success() {
                if let Ok(st_val) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                    if let Some(e) = st_val.get("email").and_then(|v| v.as_str()) {
                        email = e.to_string();
                    }
                }
            }
        }
    }
    let label = format!("Claude {} ({})", sub_type.to_uppercase(), email);
    Some((email, label, token.to_string()))
}

/// Registers or updates a Claude Code account in SQLite.
pub fn save_claude_account(email: &str, token: &str, label: &str) -> Result<String, AccountsError> {
    let clean_token = token.trim();
    if clean_token.is_empty() {
        return Err(AccountsError::InvalidRequest("Token cannot be empty".to_string()));
    }
    let enc_token = encrypt_refresh_token(clean_token)?;
    let now = chrono_or_simple_iso();
    let accounts = db_get_accounts()?;
    let existing = accounts.into_iter().find(|a| a.provider == "claude" || (a.email == email && a.provider == "claude"));
    let account_id = if let Some(acc) = existing {
        crate::db::db_update_account_token(&acc.email, &enc_token)?;
        acc.id
    } else {
        let new_id = uuid::Uuid::new_v4().to_string();
        db_insert_account_with_provider(
            &new_id,
            label,
            email,
            false,
            &enc_token,
            &now,
            "claude",
        )?;
        new_id
    };
    Ok(account_id)
}

/// Synchronizes a DeepSeek API key into ~/.local/share/opencode/auth.json, preserving the other
/// providers stored there. An unreadable or non-object file is an error: overwriting it would
/// destroy the other providers' credentials.
pub fn sync_opencode_auth_json(api_key: &str) -> Result<(), AccountsError> {
    let home = dirs::home_dir().ok_or_else(|| AccountsError::InvalidRequest("Failed to resolve home directory".to_string()))?;
    let opencode_dir = home.join(".local/share/opencode");
    std::fs::create_dir_all(&opencode_dir)
        .map_err(|e| AccountsError::Io { context: format!("Failed to create {}", opencode_dir.display()), source: e })?;
    let auth_path = opencode_dir.join("auth.json");
    let mut root: serde_json::Value = if auth_path.exists() {
        let content = std::fs::read_to_string(&auth_path)
            .map_err(|e| AccountsError::Io { context: format!("Failed to read {}", auth_path.display()), source: e })?;
        serde_json::from_str(&content)
            .map_err(|e| AccountsError::InvalidPayload(format!("{} is not valid JSON: {}", auth_path.display(), e)))?
    } else {
        serde_json::json!({})
    };

    let map = root
        .as_object_mut()
        .ok_or_else(|| AccountsError::InvalidPayload(format!("{} is not a JSON object", auth_path.display())))?;
    map.insert(
        "deepseek".to_string(),
        serde_json::json!({
            "type": "api",
            "key": api_key.trim()
        }),
    );

    let json_bytes = serde_json::to_string_pretty(&root)
        .map_err(|e| AccountsError::InvalidPayload(format!("cannot serialize auth.json: {}", e)))?;
    std::fs::write(&auth_path, json_bytes)
        .map_err(|e| AccountsError::Io { context: format!("Failed to write {}", auth_path.display()), source: e })?;

    Ok(())
}

/// API key of the active DeepSeek account. SQLite is the only source: an environment variable or
/// a runner file never substitutes a missing or undecryptable account.
pub fn get_active_deepseek_key() -> Result<Option<String>, AccountsError> {
    let Some(acc) = db_get_active_account_by_provider("deepseek")? else {
        return Ok(None);
    };
    let key = decrypt_refresh_token(&acc.enc_token)?;
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(AccountsError::InvalidPayload(format!("DeepSeek account '{}' stores an empty API key", acc.id)));
    }
    Ok(Some(trimmed.to_string()))
}

/// Registers or updates a DeepSeek API key account in SQLite.
pub fn save_deepseek_key(api_key: &str, email: Option<&str>, label: Option<&str>) -> Result<String, AccountsError> {
    let clean_key = api_key.trim();
    if clean_key.is_empty() {
        return Err(AccountsError::InvalidRequest("API key cannot be empty".to_string()));
    }
    let enc_key = encrypt_refresh_token(clean_key)?;
    let now = chrono_or_simple_iso();
    let account_label = label.unwrap_or("DeepSeek Key").trim().to_string();

    let accounts = db_get_accounts()?;
    let target_email = email.map(|s| s.trim()).filter(|s| !s.is_empty());
    let existing = accounts.into_iter().find(|a| {
        a.provider == "deepseek" && (
            (target_email.is_some() && Some(a.email.as_str()) == target_email)
            || a.label == account_label
        )
    });

    let account_id = if let Some(acc) = existing {
        crate::db::db_update_account_token(&acc.email, &enc_key)?;
        db_set_active_account(&acc.id)?;
        acc.id
    } else {
        let new_id = Uuid::new_v4().to_string();
        let eff_email = target_email
            .map(String::from)
            .unwrap_or_else(|| format!("deepseek:{}@api", &new_id[0..8]));
        db_insert_account_with_provider(
            &new_id,
            &account_label,
            &eff_email,
            true,
            &enc_key,
            &now,
            "deepseek",
        )?;
        db_set_active_account(&new_id)?;
        new_id
    };

    sync_opencode_auth_json(clean_key)?;

    crate::logger::log(
        "INFO",
        "accounts",
        &format!("Saved DeepSeek API key for account '{}' (id: {})", account_label, account_id),
    );

    Ok(account_id)
}

#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct DeepSeekBalanceInfo {
    pub currency: String,
    pub total_balance: String,
    pub granted_balance: String,
    pub topped_up_balance: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct DeepSeekBalanceResponse {
    pub is_available: bool,
    pub balance_infos: Vec<DeepSeekBalanceInfo>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Queries real-time wallet balance from https://api.deepseek.com/user/balance.
pub async fn query_deepseek_balance() -> Result<DeepSeekBalanceResponse, AccountsError> {
    let api_key = match get_active_deepseek_key()? {
        Some(k) => k,
        None => return Ok(DeepSeekBalanceResponse {
            is_available: false,
            balance_infos: Vec::new(),
            error: Some("No DeepSeek API key configured".to_string()),
        }),
    };

    let output = tokio::process::Command::new("curl")
        .args(&[
            "-s",
            "-m", "8",
            "https://api.deepseek.com/user/balance",
            "-H", &format!("Authorization: Bearer {}", api_key.trim()),
        ])
        .output()
        .await
        .map_err(|e| AccountsError::Remote { service: "DeepSeek balance", details: format!("cannot run curl: {}", e) })?;

    if !output.status.success() {
        crate::logger::log(
            "WARN",
            "accounts",
            &format!("DeepSeek balance query failed with exit code {:?}", output.status.code()),
        );
        return Err(AccountsError::Remote { service: "DeepSeek balance", details: format!("curl exit code {:?}", output.status.code()) });
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let parsed: DeepSeekBalanceResponse = serde_json::from_str(&stdout_str)
        .map_err(|e| AccountsError::Remote { service: "DeepSeek balance", details: format!("unexpected response: {} (raw: {})", e, stdout_str) })?;

    crate::logger::log("DEBUG", "accounts", "DeepSeek wallet balance queried successfully");

    Ok(parsed)
}

/// Helper to generate ISO 8601 string without extra chrono crate requirement.
fn chrono_or_simple_iso() -> String {
    let now = std::time::SystemTime::now();
    let dur = now.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = dur.as_secs();
    format!("{secs}")
}

// --- Daemon commands ---

/// Registered accounts, after importing credentials found in runner files. Daemon command
/// (SQLite and file reads: dispatched on the blocking pool).
pub fn list_accounts() -> Result<Vec<AccountSummary>, AccountsError> {
    // Import credentials found in runner files into SQLite (registration, not a lookup fallback:
    // after import SQLite is the only source).
    let mut summaries = db_get_account_summaries()?;
    let has_active_google = summaries.iter().any(|a| a.is_active && (a.provider == "google" || a.provider.is_empty()));
    if !has_active_google && check_and_register_keyring_account()?.is_some() {
        summaries = db_get_account_summaries()?;
    }

    if !summaries.iter().any(|a| a.provider == "deepseek") {
        if let Some(key) = discover_opencode_deepseek_key() {
            crate::logger::log("INFO", "accounts", "Auto-discovered DeepSeek API key from OpenCode config");
            save_deepseek_key(&key, None, Some("OpenCode Key"))?;
            summaries = db_get_account_summaries()?;
        }
    }

    if !summaries.iter().any(|a| a.provider == "claude" || a.provider == "anthropic") {
        if let Some((email, label, token)) = discover_claude_account() {
            crate::logger::log("INFO", "accounts", &format!("Auto-discovered Claude Code account '{}'", label));
            save_claude_account(&email, &token, &label)?;
            summaries = db_get_account_summaries()?;
        }
    }

    Ok(summaries)
}

/// Arguments of the daemon command `save_deepseek_api_key`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SaveDeepseekApiKeyArgs {
    pub api_key: String,
    pub email: Option<String>,
    pub label: Option<String>,
}

/// Stores a DeepSeek API key as an account and announces `accounts-updated`. Daemon command.
pub fn save_deepseek_api_key(args: SaveDeepseekApiKeyArgs) -> Result<String, AccountsError> {
    let id = save_deepseek_key(&args.api_key, args.email.as_deref(), args.label.as_deref())?;
    let summaries = crate::db::db_get_account_summaries()?;
    crate::supervisor::get_supervisor().emit_event(crate::events::DaemonEvent::accounts_updated(&summaries));
    Ok(id)
}

/// Arguments of the daemon command `switch_account`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SwitchAccountArgs {
    pub account_id: String,
    /// Restart the agents that run with the previous account.
    pub relaunch_active: bool,
}

/// Activates an account and announces `account-switched` and `accounts-updated`. Daemon command.
pub async fn switch_account_command(account_id: String, relaunch_active: bool) -> Result<(), AccountsError> {
    switch_account(&account_id, relaunch_active).await?;
    let summaries = crate::db::db_get_account_summaries()?;
    let email = summaries.iter().find(|a| a.id == account_id).map(|a| a.email.clone()).ok_or_else(|| AccountsError::AccountNotFound(account_id.clone()))?;
    let sup = crate::supervisor::get_supervisor();
    sup.emit_event(crate::events::DaemonEvent::account_switched(&crate::events::AccountSwitchedPayload {
        account_id,
        email,
    }));
    sup.emit_event(crate::events::DaemonEvent::accounts_updated(&summaries));
    Ok(())
}

/// Arguments of the daemon command `remove_account`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct AccountIdArgs {
    pub account_id: String,
}

/// Arguments of the daemon command `rename_account`.
#[derive(Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct RenameAccountArgs {
    pub account_id: String,
    pub label: String,
}

/// Agents running with an agy session (warned about before an account switch). Daemon command.
pub fn check_active_agent_sessions() -> Result<Vec<AgySession>, AccountsError> {
    check_active_presence_sessions(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_key_deterministic() {
        let ikm = "test-machine-uuid-1234";
        let key1 = derive_key_from_ikm(ikm).expect("derive 1");
        let key2 = derive_key_from_ikm(ikm).expect("derive 2");
        assert_eq!(key1, key2);
        assert_eq!(key1.len(), 32);

        let other_key = derive_key_from_ikm("other-machine").expect("derive other");
        assert_ne!(key1, other_key);
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let ikm = "test-installation-id-5678";
        let key = derive_key_from_ikm(ikm).expect("key derivation");

        let refresh_token = "1//04_random_oauth_refresh_token_abcdef1234567890";
        let encrypted = encrypt_refresh_token_with_key(refresh_token, &key).expect("encryption");
        assert!(encrypted.len() > 12);

        let decrypted = decrypt_refresh_token_with_key(&encrypted, &key).expect("decryption");
        assert_eq!(decrypted, refresh_token);
    }

    #[test]
    fn test_keyring_email_extraction() {
        let payload = build_keyring_payload("1//04_refresh", "developer@example.com");
        assert_eq!(
            extract_email_from_keyring_token(&payload),
            Some("developer@example.com".to_string())
        );

        let empty = serde_json::json!({ "id_token_email": "  " });
        assert_eq!(extract_email_from_keyring_token(&empty), None);
    }

    #[test]
    fn test_active_agy_sessions_scan() {
        let sessions = get_active_agy_sessions();
        println!("Detected active agy sessions: {:?}", sessions);
    }

    #[test]
    fn test_deepseek_key_encryption_roundtrip() {
        let ikm = "test-machine-uuid-deepseek";
        let key = derive_key_from_ikm(ikm).expect("key derivation");

        let api_key = "sk-0123456789abcdef0123456789abcdef";
        let encrypted = encrypt_refresh_token_with_key(api_key, &key).expect("encryption");
        assert!(encrypted.len() > 12);

        let decrypted = decrypt_refresh_token_with_key(&encrypted, &key).expect("decryption");
        assert_eq!(decrypted, api_key);
    }
}
