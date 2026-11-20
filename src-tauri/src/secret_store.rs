//! OS secret storage for daemon-only credentials (the Gemini API key).
//!
//! Linux uses the FreeDesktop Secret Service through `secret-tool`, the same mechanism already
//! used for Antigravity OAuth credentials. The [`SecretKind`] abstraction keeps callers free of
//! platform details so a future macOS Keychain / Windows Credential Manager backend can replace
//! this implementation without touching configuration or API code.

use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;
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
    #[error("Secret Service tool 'secret-tool' is not available: {0}")]
    ToolUnavailable(String),
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
            Self::ToolUnavailable(_) | Self::Operation { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Self::Timeout { .. } => StatusCode::GATEWAY_TIMEOUT,
            Self::Empty => StatusCode::BAD_REQUEST,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::ToolUnavailable(_) => "SecretStoreError::ToolUnavailable",
            Self::Operation { .. } => "SecretStoreError::Operation",
            Self::Timeout { .. } => "SecretStoreError::Timeout",
            Self::Empty => "SecretStoreError::Empty",
        }
    }
}

/// Whether `secret-tool` exists on the host. The daemon still reports a typed error if the
/// binary exists but the Secret Service is unavailable.
pub fn is_available() -> bool {
    crate::desktop::resolve_executable("secret-tool").is_some()
}

fn command_error(operation: &'static str, error: std::io::Error) -> SecretStoreError {
    if error.kind() == std::io::ErrorKind::NotFound {
        SecretStoreError::ToolUnavailable(error.to_string())
    } else {
        SecretStoreError::Operation {
            operation,
            details: error.to_string(),
        }
    }
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

/// Reads a secret. `Ok(None)` means the item does not exist (normal before configuration).
pub async fn get(kind: SecretKind) -> Result<Option<String>, SecretStoreError> {
    run_with_timeout("lookup", async {
        let output = Command::new("secret-tool")
            .args(["lookup", "service", "one-ship", "username", kind.username()])
            .output()
            .await
            .map_err(|e| command_error("lookup", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if stdout.is_empty() {
            if output.status.success() || output.stderr.is_empty() {
                return Ok(None);
            }
            return Err(SecretStoreError::Operation {
                operation: "lookup",
                details: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }
        Ok(Some(stdout))
    })
    .await
}

/// Stores or replaces a secret.
pub async fn set(kind: SecretKind, value: &str) -> Result<(), SecretStoreError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(SecretStoreError::Empty);
    }
    let payload = trimmed.to_string();
    run_with_timeout("store", async move {
        let mut child = Command::new("secret-tool")
            .args([
                "store",
                "--label",
                kind.label(),
                "service",
                "one-ship",
                "username",
                kind.username(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| command_error("store", e))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(payload.as_bytes())
                .await
                .map_err(|e| SecretStoreError::Operation {
                    operation: "store",
                    details: format!("cannot pipe secret: {e}"),
                })?;
        }

        let output = child.wait_with_output().await.map_err(|e| {
            SecretStoreError::Operation {
                operation: "store",
                details: e.to_string(),
            }
        })?;
        if !output.status.success() {
            return Err(SecretStoreError::Operation {
                operation: "store",
                details: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }
        Ok(())
    })
    .await
}

/// Removes a secret. A missing item is not an error.
pub async fn clear(kind: SecretKind) -> Result<(), SecretStoreError> {
    run_with_timeout("clear", async move {
        let output = Command::new("secret-tool")
            .args(["clear", "service", "one-ship", "username", kind.username()])
            .output()
            .await
            .map_err(|e| command_error("clear", e))?;
        if output.status.success() || output.stderr.is_empty() {
            return Ok(());
        }
        Err(SecretStoreError::Operation {
            operation: "clear",
            details: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    })
    .await
}
