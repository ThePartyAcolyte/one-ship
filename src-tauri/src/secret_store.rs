//! OS secret storage for daemon-only credentials (the Gemini API key and Antigravity OAuth).
//!
//! Linux uses the FreeDesktop Secret Service (`org.freedesktop.secrets`) over D-Bus via the
//! `secret-service` crate, eliminating any dependency on external CLI tools (`secret-tool`).
//! The [`SecretKind`] abstraction keeps callers free of platform details so a future macOS
//! Keychain / Windows Credential Manager backend can replace this implementation without touching
//! configuration or API code.

use std::collections::HashMap;
use std::time::Duration;

use secret_service::{EncryptionType, SecretService};
use tokio::time::timeout;

/// A secret owned by ONE-SHIP.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretKind {
    GeminiApiKey,
}

impl SecretKind {
    fn username(self) -> &'static str {
        match self {
            Self::GeminiApiKey => "gemini_api_key",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::GeminiApiKey => "ONE-SHIP Gemini API Key",
        }
    }
}

/// Failure of a Secret Service read, write or clear operation.
#[derive(Debug, thiserror::Error)]
pub enum SecretStoreError {
    #[error("OS Secret Service is unavailable: {0}")]
    ServiceUnavailable(String),
    #[error("Secret Service operation '{operation}' failed: {details}")]
    Operation {
        operation: &'static str,
        details: String,
    },
    #[error("Secret Service operation '{operation}' timed out after {seconds}s")]
    Timeout {
        operation: &'static str,
        seconds: u64,
    },
    #[error("The secret value cannot be empty")]
    Empty,
}

impl crate::api::ApiError for SecretStoreError {
    fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::ServiceUnavailable(_) | Self::Operation { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Self::Timeout { .. } => StatusCode::GATEWAY_TIMEOUT,
            Self::Empty => StatusCode::BAD_REQUEST,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::ServiceUnavailable(_) => "SecretStoreError::ServiceUnavailable",
            Self::Operation { .. } => "SecretStoreError::Operation",
            Self::Timeout { .. } => "SecretStoreError::Timeout",
            Self::Empty => "SecretStoreError::Empty",
        }
    }
}

async fn connect_service() -> Result<SecretService<'static>, SecretStoreError> {
    SecretService::connect(EncryptionType::Dh)
        .await
        .map_err(|e| SecretStoreError::ServiceUnavailable(e.to_string()))
}

/// Runs an async Secret Service future from synchronous code on an isolated current-thread Tokio
/// runtime so callers never panic regardless of whether they run on a Tokio worker, blocking pool,
/// or OS thread.
pub fn run_sync<F, T>(future: F) -> Result<T, SecretStoreError>
where
    F: std::future::Future<Output = Result<T, SecretStoreError>> + Send + 'static,
    T: Send + 'static,
{
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| SecretStoreError::Operation {
                operation: "runtime",
                details: e.to_string(),
            })?;
        rt.block_on(future)
    })
    .join()
    .map_err(|_| SecretStoreError::Operation {
        operation: "runtime",
        details: "secret service worker thread panicked".to_string(),
    })?
}

/// Whether the FreeDesktop Secret Service (`org.freedesktop.secrets`) is reachable on D-Bus.
pub fn is_available() -> bool {
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none() {
        return false;
    }
    run_sync(async {
        match timeout(Duration::from_secs(2), connect_service()).await {
            Ok(Ok(_)) => Ok(true),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(SecretStoreError::Timeout {
                operation: "connect",
                seconds: 2,
            }),
        }
    })
    .unwrap_or(false)
}

async fn run_with_timeout<F, T>(
    operation: &'static str,
    future: F,
) -> Result<T, SecretStoreError>
where
    F: std::future::Future<Output = Result<T, SecretStoreError>>,
{
    let seconds = crate::config::keyring_timeout_seconds();
    match timeout(Duration::from_secs(seconds), future).await {
        Ok(result) => result,
        Err(_) => Err(SecretStoreError::Timeout { operation, seconds }),
    }
}

fn map_op_err(operation: &'static str, err: secret_service::Error) -> SecretStoreError {
    SecretStoreError::Operation {
        operation,
        details: err.to_string(),
    }
}

/// Reads a secret matching `service` and `username` attributes. `Ok(None)` means no item exists.
pub async fn lookup_pair(service: &str, username: &str) -> Result<Option<String>, SecretStoreError> {
    let service = service.to_string();
    let username = username.to_string();
    run_with_timeout("lookup", async move {
        let ss = connect_service().await?;
        let attrs = HashMap::from([("service", service.as_str()), ("username", username.as_str())]);
        let items = ss
            .search_items(attrs)
            .await
            .map_err(|e| map_op_err("lookup", e))?;

        let item = match items.unlocked.first() {
            Some(item) => item,
            None => match items.locked.first() {
                Some(locked_item) => {
                    locked_item
                        .unlock()
                        .await
                        .map_err(|e| map_op_err("unlock", e))?;
                    locked_item
                }
                None => return Ok(None),
            },
        };

        let bytes = item
            .get_secret()
            .await
            .map_err(|e| map_op_err("lookup", e))?;
        let text = String::from_utf8_lossy(&bytes).trim().to_string();
        if text.is_empty() {
            return Ok(None);
        }
        Ok(Some(text))
    })
    .await
}

/// Stores or replaces a secret matching `service` and `username` attributes.
pub async fn store_pair(
    label: &str,
    service: &str,
    username: &str,
    value: &str,
) -> Result<(), SecretStoreError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(SecretStoreError::Empty);
    }
    let label = label.to_string();
    let service = service.to_string();
    let username = username.to_string();
    let payload = trimmed.to_string();
    run_with_timeout("store", async move {
        let ss = connect_service().await?;
        let collection = ss
            .get_any_collection()
            .await
            .map_err(|e| map_op_err("store", e))?;
        if collection
            .is_locked()
            .await
            .map_err(|e| map_op_err("store", e))?
        {
            collection
                .unlock()
                .await
                .map_err(|e| map_op_err("unlock", e))?;
        }
        let attrs = HashMap::from([("service", service.as_str()), ("username", username.as_str())]);
        collection
            .create_item(&label, attrs, payload.as_bytes(), true, "text/plain")
            .await
            .map_err(|e| map_op_err("store", e))?;
        Ok(())
    })
    .await
}

/// Removes all secrets matching `service` and `username` attributes. Missing items are not errors.
pub async fn clear_pair(service: &str, username: &str) -> Result<(), SecretStoreError> {
    let service = service.to_string();
    let username = username.to_string();
    run_with_timeout("clear", async move {
        let ss = connect_service().await?;
        let attrs = HashMap::from([("service", service.as_str()), ("username", username.as_str())]);
        let items = ss
            .search_items(attrs)
            .await
            .map_err(|e| map_op_err("clear", e))?;

        for item in items.unlocked {
            item.delete().await.map_err(|e| map_op_err("clear", e))?;
        }
        for item in items.locked {
            item.unlock()
                .await
                .map_err(|e| map_op_err("unlock", e))?;
            item.delete().await.map_err(|e| map_op_err("clear", e))?;
        }
        Ok(())
    })
    .await
}

/// Reads a ONE-SHIP secret. `Ok(None)` means the item does not exist (normal before configuration).
pub async fn get(kind: SecretKind) -> Result<Option<String>, SecretStoreError> {
    lookup_pair("one-ship", kind.username()).await
}

/// Stores or replaces a ONE-SHIP secret.
pub async fn set(kind: SecretKind, value: &str) -> Result<(), SecretStoreError> {
    store_pair(kind.label(), "one-ship", kind.username(), value).await
}

/// Removes a ONE-SHIP secret. A missing item is not an error.
pub async fn clear(kind: SecretKind) -> Result<(), SecretStoreError> {
    clear_pair("one-ship", kind.username()).await
}
