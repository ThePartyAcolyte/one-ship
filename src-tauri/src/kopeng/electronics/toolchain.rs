use serde::{Deserialize, Serialize};
use ts_rs::TS;
use crate::kopeng::electronics::errors::ElectronicsError;

/// Status and metadata for an installed electronics toolchain binary.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ElectronicsToolStatus {
    pub id: String,
    pub name: String,
    pub binary: String,
    pub category: String,
    pub is_installed: bool,
    pub version: Option<String>,
    pub install_hint: String,
    pub auto_installable: bool,
}

/// Probes whether a CLI binary exists and returns `(installed, version_string)`.
pub async fn probe_command_version(bin: &str, args: &[&str]) -> (bool, Option<String>) {
    let Ok(output) = tokio::process::Command::new(bin).args(args).output().await else {
        return (false, None);
    };
    if !output.status.success() && output.stdout.is_empty() && output.stderr.is_empty() {
        return (false, None);
    }
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let first_line = combined
        .lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .map(|l| l.chars().take(80).collect::<String>());
    (true, first_line)
}

/// Checks the presence and versions of installed microcontroller and electronics toolchains.
pub async fn check_electronics_toolchain() -> Result<Vec<ElectronicsToolStatus>, ElectronicsError> {
    let mut tools = Vec::with_capacity(6);

    // 1. PlatformIO Core
    let (has_pio, pio_ver) = probe_command_version("pio", &["--version"]).await;
    let (pio_hint, pio_auto) = crate::kopeng::diagnostic::resolve_tool_install_hint(
        "platformio",
        "pipx install platformio (or: python -m pip install --user platformio)",
    );
    tools.push(ElectronicsToolStatus {
        id: "platformio".to_string(),
        name: "PlatformIO Core (CLI)".to_string(),
        binary: "pio".to_string(),
        category: "firmware".to_string(),
        is_installed: has_pio,
        version: pio_ver,
        install_hint: pio_hint,
        auto_installable: pio_auto,
    });

    // 2. probe-rs
    let (has_probe, probe_ver) = probe_command_version("probe-rs", &["--version"]).await;
    let (probe_hint, probe_auto) = crate::kopeng::diagnostic::resolve_tool_install_hint(
        "probe-rs",
        "cargo install probe-rs-tools",
    );
    tools.push(ElectronicsToolStatus {
        id: "probe-rs".to_string(),
        name: "probe-rs (Native Rust Toolchain)".to_string(),
        binary: "probe-rs".to_string(),
        category: "firmware".to_string(),
        is_installed: has_probe,
        version: probe_ver,
        install_hint: probe_hint,
        auto_installable: probe_auto,
    });

    // 3. esptool
    let (has_esp, esp_ver) = probe_command_version("esptool.py", &["version"]).await;
    let (has_esp_alt, esp_ver_alt) = if !has_esp {
        probe_command_version("esptool", &["version"]).await
    } else {
        (has_esp, esp_ver)
    };
    let (esp_hint, esp_auto) = crate::kopeng::diagnostic::resolve_tool_install_hint(
        "esptool",
        "pipx install esptool",
    );
    tools.push(ElectronicsToolStatus {
        id: "esptool".to_string(),
        name: "Espressif esptool (ESP32/ESP8266)".to_string(),
        binary: "esptool.py".to_string(),
        category: "flasher".to_string(),
        is_installed: has_esp_alt,
        version: esp_ver_alt,
        install_hint: esp_hint,
        auto_installable: esp_auto,
    });

    // 4. arduino-cli
    let (has_arduino, arduino_ver) = probe_command_version("arduino-cli", &["version"]).await;
    let (arduino_hint, arduino_auto) = crate::kopeng::diagnostic::resolve_tool_install_hint(
        "arduino-cli",
        "Install arduino-cli via https://arduino.github.io/arduino-cli/",
    );
    tools.push(ElectronicsToolStatus {
        id: "arduino-cli".to_string(),
        name: "Arduino CLI".to_string(),
        binary: "arduino-cli".to_string(),
        category: "firmware".to_string(),
        is_installed: has_arduino,
        version: arduino_ver,
        install_hint: arduino_hint,
        auto_installable: arduino_auto,
    });

    // 5. sigrok-cli
    let (has_sigrok, sigrok_ver) = probe_command_version("sigrok-cli", &["--version"]).await;
    let (sigrok_hint, sigrok_auto) = crate::kopeng::diagnostic::resolve_tool_install_hint(
        "sigrok-cli",
        "Install sigrok-cli and sigrok-firmware-fx2lafw via your package manager",
    );
    tools.push(ElectronicsToolStatus {
        id: "sigrok-cli".to_string(),
        name: "sigrok-cli (Logic Analyzers)".to_string(),
        binary: "sigrok-cli".to_string(),
        category: "instrumentation".to_string(),
        is_installed: has_sigrok,
        version: sigrok_ver,
        install_hint: sigrok_hint,
        auto_installable: sigrok_auto,
    });

    // 6. mpremote (MicroPython file transfer / REPL)
    let (has_mpremote, mpremote_ver) = probe_command_version("mpremote", &["--version"]).await;
    tools.push(ElectronicsToolStatus {
        id: "mpremote".to_string(),
        name: "mpremote (MicroPython)".to_string(),
        binary: "mpremote".to_string(),
        category: "firmware".to_string(),
        is_installed: has_mpremote,
        version: mpremote_ver,
        install_hint: "python -m pip install --user mpremote".to_string(),
        auto_installable: false,
    });

    Ok(tools)
}
