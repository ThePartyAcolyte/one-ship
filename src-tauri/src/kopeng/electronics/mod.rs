pub mod errors;
pub mod discovery;
pub mod toolchain;
pub mod firmware;
pub mod instruments;

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use crate::kopeng::electronics::errors::ElectronicsError;
use crate::kopeng::electronics::discovery::{scan_connected_microcontrollers, DiscoveredMcuPort};
use crate::kopeng::electronics::toolchain::{check_electronics_toolchain, ElectronicsToolStatus};
use crate::kopeng::electronics::firmware::list_firmware_environments;
use crate::kopeng::electronics::instruments::driver::DiscoveredInstrument;

/// Resolves the configured firmware directory (`[plugins.electronics].firmware_dir`) safely within `project_path`.
pub fn resolve_electronics_firmware_dir(project_path: &Path) -> Result<(PathBuf, String), ElectronicsError> {
    crate::config::resolve_project_subdir(project_path, "electronics", "firmware_dir")
        .map_err(|e| ElectronicsError::InvalidSetting {
            key: "electronics.firmware_dir",
            details: e.to_string(),
        })
}

/// Aggregated electronics status for a project workspace.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProjectElectronicsStatus {
    pub is_enabled: bool,
    pub has_platformio: bool,
    /// Active firmware language (`platformio` or `micropython`); `None` when no firmware project exists.
    pub firmware_language: Option<String>,
    pub environments: Vec<String>,
    pub connected_mcus: Vec<DiscoveredMcuPort>,
    pub connected_instruments: Vec<DiscoveredInstrument>,
    pub toolchains: Vec<ElectronicsToolStatus>,
}

/// Resolves the project firmware language from `rula.toml`, then from the firmware directory layout.
pub(crate) fn detect_firmware_language(project_path: &Path, fw_dir: &Path) -> Option<String> {
    if let Ok(Some(rula_val)) = crate::config::read_project_rula(project_path) {
        if let Some(lang) = rula_val
            .get("plugins")
            .and_then(|p| p.get("electronics"))
            .and_then(|e| e.get("firmware_language"))
            .and_then(|v| v.as_str())
        {
            return Some(lang.to_string());
        }
    }
    if fw_dir.join("platformio.ini").exists() {
        return Some("platformio".to_string());
    }
    if fw_dir.join("main.py").exists() {
        return Some("micropython".to_string());
    }
    None
}

/// Checks whether the electronics plugin is enabled in rula.toml or detected via platformio.ini in the configured firmware_dir.
pub fn is_project_electronics_enabled(project_path: &Path) -> bool {
    if let Ok(Some(rula_val)) = crate::config::read_project_rula(project_path) {
        if let Some(enabled) = rula_val
            .get("plugins")
            .and_then(|p| p.get("electronics"))
            .and_then(|e| e.get("enabled"))
            .and_then(|v| v.as_bool())
        {
            return enabled;
        }
    }
    resolve_electronics_firmware_dir(project_path)
        .map(|(fw_dir, _)| fw_dir.join("platformio.ini").exists())
        .unwrap_or(false)
}

/// Arguments for commands requiring only a project path.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ProjectElectronicsPathArgs {
    pub project_path: String,
}

/// Arguments for `toggle_project_electronics_plugin`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ToggleProjectElectronicsPluginArgs {
    pub project_path: String,
    pub enabled: bool,
}

/// Arguments for `build_project_firmware`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct BuildProjectFirmwareArgs {
    pub project_path: String,
    pub environment: Option<String>,
}

/// Arguments for `flash_project_firmware`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct FlashProjectFirmwareArgs {
    pub project_path: String,
    pub environment: Option<String>,
    pub upload_port: Option<String>,
}

/// Returns aggregated electronics status for a registered project.
pub async fn get_project_electronics_status(
    project_path: String,
) -> Result<ProjectElectronicsStatus, ElectronicsError> {
    let p_path = Path::new(&project_path);
    if !p_path.exists() {
        return Err(ElectronicsError::UnregisteredProject(project_path));
    }

    let is_enabled = is_project_electronics_enabled(p_path);
    let (fw_dir, _) = resolve_electronics_firmware_dir(p_path)?;
    let has_platformio = fw_dir.join("platformio.ini").exists();
    let firmware_language = detect_firmware_language(p_path, &fw_dir);
    let environments = list_firmware_environments(p_path).await;
    let connected_mcus = match tokio::task::spawn_blocking(scan_connected_microcontrollers).await {
        Ok(Ok(list)) => list,
        Ok(Err(e)) => {
            crate::logger::log("WARN", "electronics", &format!("scan_connected_microcontrollers failed: {}", e));
            Vec::new()
        }
        Err(e) => {
            crate::logger::log("WARN", "electronics", &format!("scan_connected_microcontrollers task failed: {}", e));
            Vec::new()
        }
    };
    let connected_instruments = match instruments::scan_connected_instruments().await {
        Ok(list) => list,
        Err(e) => {
            crate::logger::log("WARN", "electronics", &format!("scan_connected_instruments failed: {}", e));
            Vec::new()
        }
    };
    let toolchains = match check_electronics_toolchain().await {
        Ok(list) => list,
        Err(e) => {
            crate::logger::log("WARN", "electronics", &format!("check_electronics_toolchain failed: {}", e));
            Vec::new()
        }
    };

    Ok(ProjectElectronicsStatus {
        is_enabled,
        has_platformio,
        firmware_language,
        environments,
        connected_mcus,
        connected_instruments,
        toolchains,
    })
}

/// Toggles the electronics plugin inside rula.toml.
pub async fn toggle_project_electronics_plugin(
    project_path: String,
    enabled: bool,
) -> Result<ProjectElectronicsStatus, ElectronicsError> {
    let p_path = Path::new(&project_path);
    if !p_path.exists() {
        return Err(ElectronicsError::UnregisteredProject(project_path));
    }

    let rula_path = p_path.join("rula.toml");
    let mut rula_val = crate::config::read_project_rula(p_path)
        .map_err(|e| ElectronicsError::InvalidSetting {
            key: "rula.toml",
            details: format!("cannot parse rula.toml: {}", e),
        })?
        .ok_or_else(|| ElectronicsError::InvalidSetting {
            key: "rula.toml",
            details: "project is not initialized (missing rula.toml)".to_string(),
        })?;

    if let Some(root_table) = rula_val.as_table_mut() {
        let plugins_entry = root_table
            .entry("plugins".to_string())
            .or_insert_with(|| toml::Value::Table(Default::default()));
        if let Some(plugins_table) = plugins_entry.as_table_mut() {
            let elec_entry = plugins_table
                .entry("electronics".to_string())
                .or_insert_with(|| toml::Value::Table(Default::default()));
            if let Some(elec_table) = elec_entry.as_table_mut() {
                elec_table.insert("enabled".to_string(), toml::Value::Boolean(enabled));
            }
        }
    }

    let serialized_toml = toml::to_string_pretty(&rula_val).map_err(|e| ElectronicsError::InvalidSetting {
        key: "enabled",
        details: format!("cannot serialize rula.toml: {}", e),
    })?;

    tokio::fs::write(&rula_path, serialized_toml)
        .await
        .map_err(|e| ElectronicsError::Io(format!("Cannot write {}: {}", rula_path.display(), e)))?;

    get_project_electronics_status(project_path).await
}

/// Lists all connected microcontrollers and serial adapters.
pub async fn list_connected_mcus() -> Result<Vec<DiscoveredMcuPort>, ElectronicsError> {
    scan_connected_microcontrollers()
}
