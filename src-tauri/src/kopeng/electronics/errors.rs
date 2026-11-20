use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;
use crate::api::ApiError;

/// Strongly-typed domain error hierarchy for the Electronics & Embedded Lab subsystem.
#[derive(Debug, Error, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "details")]
#[ts(export)]
pub enum ElectronicsError {
    #[error("Device not found on port '{port}'")]
    DeviceNotFound { port: String },

    #[error("Electronics toolchain binary '{tool}' is missing: {hint}")]
    ToolchainMissing { tool: String, hint: String },

    #[error("Serial port communication error on '{port}': {details}")]
    SerialError { port: String, details: String },

    #[error("Firmware build failed for environment '{environment}': {details}")]
    BuildFailed { environment: String, details: String },

    #[error("Firmware upload/flash failed on '{port}': {details}")]
    FlashFailed { port: String, details: String },

    #[error("Invalid configuration key '{key}': {details}")]
    InvalidSetting { key: &'static str, details: String },

    #[error("I/O error: {0}")]
    Io(String),

    #[error("Firmware scaffolding failed: {0}")]
    FirmwareScaffoldFailed(String),

    #[error("Project '{0}' is not registered")]
    UnregisteredProject(String),
}

impl ApiError for ElectronicsError {
    fn status(&self) -> StatusCode {
        match self {
            Self::DeviceNotFound { .. } => StatusCode::NOT_FOUND,
            Self::ToolchainMissing { .. } => StatusCode::PRECONDITION_FAILED,
            Self::SerialError { .. } => StatusCode::BAD_GATEWAY,
            Self::BuildFailed { .. } | Self::FlashFailed { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Self::InvalidSetting { .. } => StatusCode::BAD_REQUEST,
            Self::Io(_) | Self::FirmwareScaffoldFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::UnregisteredProject(_) => StatusCode::NOT_FOUND,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::DeviceNotFound { .. } => "electronics_device_not_found",
            Self::ToolchainMissing { .. } => "electronics_toolchain_missing",
            Self::SerialError { .. } => "electronics_serial_error",
            Self::BuildFailed { .. } => "electronics_build_failed",
            Self::FlashFailed { .. } => "electronics_flash_failed",
            Self::InvalidSetting { .. } => "electronics_invalid_setting",
            Self::Io(_) => "electronics_io_error",
            Self::FirmwareScaffoldFailed(_) => "electronics_scaffold_failed",
            Self::UnregisteredProject(_) => "electronics_unregistered_project",
        }
    }
}
