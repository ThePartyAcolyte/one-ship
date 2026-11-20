use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use std::process::{Command, Output};
use ts_rs::TS;

use crate::api::ApiError;

/// Supported Linux system package managers for optional host tool installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxPackageManager {
    Pacman,
    Apt,
    Dnf,
    Zypper,
}

impl LinuxPackageManager {
    pub fn id(self) -> &'static str {
        match self {
            Self::Pacman => "pacman",
            Self::Apt => "apt-get",
            Self::Dnf => "dnf",
            Self::Zypper => "zypper",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Pacman => "pacman",
            Self::Apt => "apt",
            Self::Dnf => "dnf",
            Self::Zypper => "zypper",
        }
    }

    pub fn install_prefix(self) -> &'static str {
        match self {
            Self::Pacman => "pkexec pacman -S --noconfirm",
            Self::Apt => "pkexec apt-get install -y",
            Self::Dnf => "pkexec dnf install -y",
            Self::Zypper => "pkexec zypper --non-interactive install",
        }
    }

    fn install_args(self) -> &'static [&'static str] {
        match self {
            Self::Pacman => &["pacman", "-S", "--noconfirm"],
            Self::Apt => &["apt-get", "install", "-y"],
            Self::Dnf => &["dnf", "install", "-y"],
            Self::Zypper => &["zypper", "--non-interactive", "install"],
        }
    }
}

/// Errors of the system diagnostics and the privileged tool installer.
#[derive(Debug, thiserror::Error)]
pub enum DiagnosticError {
    #[error("Cannot run {program}: {source}")]
    Spawn { program: String, source: std::io::Error },
    #[error("Unrecognized tool '{0}': no known package mapping")]
    UnknownTool(String),
    #[error("Tool '{tool}' cannot be auto-installed on {distro}; install it manually")]
    ToolUnavailableInDistro { tool: String, distro: String },
    #[error("No supported Linux package manager (pacman, apt, dnf, zypper) found on PATH")]
    UnsupportedPackageManager,
    #[error("No tools specified for installation")]
    NoTools,
    #[error("Installation of {packages} failed: {details}")]
    Install { packages: String, details: String },
    #[error("Background task failed: {0}")]
    Task(String),
}

impl ApiError for DiagnosticError {
    fn status(&self) -> StatusCode {
        match self {
            Self::UnknownTool(_) | Self::ToolUnavailableInDistro { .. } | Self::NoTools => StatusCode::BAD_REQUEST,
            Self::UnsupportedPackageManager => StatusCode::FAILED_DEPENDENCY,
            Self::Install { .. } => StatusCode::BAD_GATEWAY,
            Self::Spawn { .. } | Self::Task(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::Spawn { .. } => "DiagnosticError::Spawn",
            Self::UnknownTool(_) => "DiagnosticError::UnknownTool",
            Self::ToolUnavailableInDistro { .. } => "DiagnosticError::ToolUnavailableInDistro",
            Self::UnsupportedPackageManager => "DiagnosticError::UnsupportedPackageManager",
            Self::NoTools => "DiagnosticError::NoTools",
            Self::Install { .. } => "DiagnosticError::Install",
            Self::Task(_) => "DiagnosticError::Task",
        }
    }
}

/// Availability of the host tools ONE-SHIP integrates with, shown in the header's diagnostics popover.
#[derive(Serialize, TS)]
#[ts(export)]
pub struct DiagnosticState {
    pub git_installed: bool,
    pub gh_installed: bool,
    pub gh_authenticated: bool,
    pub glab_installed: bool,
    pub glab_authenticated: bool,
    pub opendictate_available: bool,
    pub package_manager: Option<String>,
    pub can_auto_install_tools: bool,
}

/// Availability snapshot of a single optional external domain integration.
#[derive(Serialize, Clone, Debug, TS)]
#[ts(export)]
pub struct IntegrationCapability {
    pub plugin_id: String,
    pub label: String,
    pub available: bool,
    pub reason: String,
    pub required_binaries: Vec<String>,
    pub configured_paths: Vec<String>,
}

/// Host capability snapshot used to dynamically gate optional workspace tabs, settings, and runners.
#[derive(Serialize, Clone, Debug, TS)]
#[ts(export)]
pub struct HostCapabilities {
    pub installed_runners: Vec<String>,
    pub integrations: Vec<IntegrationCapability>,
    pub opendictate_available: bool,
    pub gdbus_available: bool,
    pub omarchy_available: bool,
    pub hyprland_available: bool,
    pub clipboard_available: bool,
    pub package_manager: Option<String>,
}

/// Runs `program args`. `Ok(None)` when the program is not installed (`NotFound`); any other
/// spawn failure is an error, never reported as a missing tool.
fn run_tool(program: &str, args: &[&str]) -> Result<Option<Output>, DiagnosticError> {
    match Command::new(program).args(args).output() {
        Ok(out) => Ok(Some(out)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(DiagnosticError::Spawn { program: program.to_string(), source: e }),
    }
}

fn succeeds(output: &Option<Output>) -> bool {
    output.as_ref().is_some_and(|o| o.status.success())
}

fn collect_diagnostics() -> Result<DiagnosticState, DiagnosticError> {
    let git_installed = succeeds(&run_tool("git", &["--version"])?);
    let gh_installed = succeeds(&run_tool("gh", &["--version"])?);

    // `gh auth token` reads the local token store: no network call.
    let gh_authenticated = gh_installed
        && run_tool("gh", &["auth", "token"])?.is_some_and(|o| o.status.success() && !o.stdout.is_empty());

    let glab_installed = succeeds(&run_tool("glab", &["--version"])?);

    // `glab auth status` checks whether authentication is configured.
    let glab_authenticated = glab_installed
        && succeeds(&run_tool("glab", &["auth", "status"])?);

    // The OpenDictate service answers `GetStatus` on the session bus; a missing `gdbus` means unavailable.
    let timeout_seconds = crate::config::resolve_effective_u64(None, None, "system", "dbus_call_timeout_seconds");
    let opendictate_available = succeeds(&run_tool("gdbus", &[
        "call",
        "--session",
        "--timeout",
        &timeout_seconds.to_string(),
        "--dest",
        "org.kirulab.OpenDictate",
        "--object-path",
        "/org/kirulab/OpenDictate",
        "--method",
        "org.kirulab.OpenDictate.GetStatus",
    ])?);

    let pm = detect_host_package_manager();
    Ok(DiagnosticState {
        git_installed,
        gh_installed,
        gh_authenticated,
        glab_installed,
        glab_authenticated,
        opendictate_available,
        package_manager: pm.map(|p| p.display_name().to_string()),
        can_auto_install_tools: pm.is_some() && crate::desktop::resolve_executable("pkexec").is_some(),
    })
}

/// Daemon command `run_diagnostics`.
pub async fn run_diagnostics() -> Result<DiagnosticState, DiagnosticError> {
    crate::logger::log("DEBUG", "diagnostic", "run_diagnostics");
    let state = tokio::task::spawn_blocking(collect_diagnostics)
        .await
        .map_err(|e| DiagnosticError::Task(e.to_string()))??;
    crate::logger::log("DEBUG", "diagnostic", &format!(
        "run_diagnostics: git={}, gh={}, gh_auth={}, glab={}, glab_auth={}, opendictate={}",
        state.git_installed, state.gh_installed, state.gh_authenticated,
        state.glab_installed, state.glab_authenticated, state.opendictate_available
    ));
    Ok(state)
}

fn check_opendictate_dbus_available() -> bool {
    if crate::desktop::resolve_executable("gdbus").is_none() {
        return false;
    }
    let timeout_seconds = crate::config::resolve_effective_u64(None, None, "system", "dbus_call_timeout_seconds");
    run_tool("gdbus", &[
        "call",
        "--session",
        "--timeout",
        &timeout_seconds.to_string(),
        "--dest",
        "org.kirulab.OpenDictate",
        "--object-path",
        "/org/kirulab/OpenDictate",
        "--method",
        "org.kirulab.OpenDictate.GetStatus",
    ])
    .is_ok_and(|out| succeeds(&out))
}

fn integration_entry(
    plugin_id: &str,
    label: &str,
    required_binaries: &[&str],
    configured_paths: Vec<String>,
    available: bool,
    ready_reason: &str,
    missing_reason: &str,
) -> IntegrationCapability {
    IntegrationCapability {
        plugin_id: plugin_id.to_string(),
        label: label.to_string(),
        available,
        reason: if available {
            ready_reason.to_string()
        } else {
            missing_reason.to_string()
        },
        required_binaries: required_binaries.iter().map(|s| (*s).to_string()).collect(),
        configured_paths,
    }
}

fn collect_host_capabilities() -> Result<HostCapabilities, DiagnosticError> {
    let mut installed_runners = Vec::new();
    match crate::config::list_agent_runners() {
        Ok(schemas) => {
            for schema in schemas {
                if crate::desktop::resolve_executable(&schema.binary).is_some() {
                    installed_runners.push(schema.id);
                }
            }
        }
        Err(e) => {
            crate::logger::log("WARN", "diagnostic", &format!("Failed to list runner manifests for capability probe: {}", e));
        }
    }

    let rea_ok = crate::desktop::resolve_executable("rea").is_some();
    let electronics_bins = ["pio", "probe-rs", "esptool.py", "esptool", "arduino-cli", "sigrok-cli", "mpremote"];
    let electronics_found: Vec<&str> = electronics_bins
        .iter()
        .copied()
        .filter(|b| crate::desktop::resolve_executable(b).is_some())
        .collect();
    let electronics_ok = !electronics_found.is_empty();

    let solid_app = crate::config::resolve_effective_string(None, None, "solidcraft", "app_binary_path");
    let solid_cli = crate::config::resolve_effective_string(None, None, "solidcraft", "cli_binary_path");
    let solid_ok = crate::desktop::resolve_executable(&solid_app).is_some()
        || crate::desktop::resolve_executable(&solid_cli).is_some();

    let pcb_app = crate::config::resolve_effective_string(None, None, "pcbcraft", "app_binary_path");
    let pcb_cli = crate::config::resolve_effective_string(None, None, "pcbcraft", "cli_binary_path");
    let pcb_ok = crate::desktop::resolve_executable(&pcb_app).is_some()
        || crate::desktop::resolve_executable(&pcb_cli).is_some();

    let desigma_app = crate::config::resolve_effective_string(None, None, "desigma", "app_binary_path");
    let desigma_mcp = crate::config::resolve_effective_string(None, None, "desigma", "mcp_binary_path");
    let desigma_ok = crate::desktop::resolve_executable(&desigma_app).is_some()
        || crate::desktop::resolve_executable(&desigma_mcp).is_some();

    let voice_ctrl_ok = crate::desktop::resolve_executable("pw-record").is_some()
        && crate::desktop::resolve_executable("pw-play").is_some();

    let integrations = vec![
        integration_entry(
            "rea",
            "RE Lab (REA)",
            &["rea"],
            Vec::new(),
            rea_ok,
            "rea CLI detected on PATH",
            "Missing `rea` binary (install rea-agents)",
        ),
        integration_entry(
            "electronics",
            "Electronics Workbench",
            &["pio", "probe-rs", "esptool", "arduino-cli", "sigrok-cli", "mpremote"],
            Vec::new(),
            electronics_ok,
            &format!("Detected: {}", electronics_found.join(", ")),
            "No embedded/instrumentation CLI detected (pio, probe-rs, esptool, arduino-cli, sigrok-cli, mpremote)",
        ),
        integration_entry(
            "solidcraft",
            "SolidCraft 3D CAD",
            &["solidcraft", "solidcraft-cli"],
            vec![solid_app.clone(), solid_cli.clone()],
            solid_ok,
            "SolidCraft binary available",
            &format!("Not found: '{}' / '{}'", solid_app, solid_cli),
        ),
        integration_entry(
            "pcbcraft",
            "PCBCraft EDA / PCB",
            &["pcbcraft", "pcbcraft-cli"],
            vec![pcb_app.clone(), pcb_cli.clone()],
            pcb_ok,
            "PCBCraft binary available",
            &format!("Not found: '{}' / '{}'", pcb_app, pcb_cli),
        ),
        integration_entry(
            "desigma",
            "One-Ship Designer (Desigma)",
            &["oneship-designer", "desigma-mcp"],
            vec![desigma_app.clone(), desigma_mcp.clone()],
            desigma_ok,
            "Desigma binary available",
            &format!("Not found: '{}' / '{}'", desigma_app, desigma_mcp),
        ),
        integration_entry(
            "voice_controller",
            "Global Voice Controller",
            &["pw-record", "pw-play"],
            Vec::new(),
            voice_ctrl_ok,
            "PipeWire audio utilities (pw-record / pw-play) available",
            "Missing PipeWire audio tools (pw-record / pw-play)",
        ),
    ];

    let gdbus_available = crate::desktop::resolve_executable("gdbus").is_some();
    let opendictate_available = check_opendictate_dbus_available();
    let omarchy_available = crate::theme::is_omarchy_available();
    let hyprland_available = crate::desktop::is_hyprland_session();
    let clipboard_available = crate::desktop::resolve_executable("wl-paste").is_some()
        || crate::desktop::resolve_executable("xclip").is_some();
    let package_manager = detect_host_package_manager().map(|p| p.display_name().to_string());

    Ok(HostCapabilities {
        installed_runners,
        integrations,
        opendictate_available,
        gdbus_available,
        omarchy_available,
        hyprland_available,
        clipboard_available,
        package_manager,
    })
}

/// Daemon command `get_host_capabilities`: returns host binary, runner, audio, and desktop
/// capabilities so the UI only exposes available integrations.
pub async fn get_host_capabilities() -> Result<HostCapabilities, DiagnosticError> {
    crate::logger::log("DEBUG", "diagnostic", "get_host_capabilities");
    tokio::task::spawn_blocking(collect_host_capabilities)
        .await
        .map_err(|e| DiagnosticError::Task(e.to_string()))?
}

/// Parses `/etc/os-release` (`ID` / `ID_LIKE`) into the corresponding Linux package manager.
pub fn detect_package_manager_from_os_release(content: &str) -> Option<LinuxPackageManager> {
    let mut id_tokens: Vec<String> = Vec::new();
    let mut like_tokens: Vec<String> = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(val) = trimmed.strip_prefix("ID=") {
            let clean = val.trim_matches('"').trim_matches('\'').to_ascii_lowercase();
            id_tokens.extend(clean.split_whitespace().map(str::to_string));
        } else if let Some(val) = trimmed.strip_prefix("ID_LIKE=") {
            let clean = val.trim_matches('"').trim_matches('\'').to_ascii_lowercase();
            like_tokens.extend(clean.split_whitespace().map(str::to_string));
        }
    }
    for token in id_tokens.iter().chain(like_tokens.iter()) {
        match token.as_str() {
            "arch" | "manjaro" | "endeavouros" | "cachyos" | "garuda" | "omarchy" => {
                return Some(LinuxPackageManager::Pacman);
            }
            "debian" | "ubuntu" | "linuxmint" | "pop" | "elementary" | "kali" | "zorin" => {
                return Some(LinuxPackageManager::Apt);
            }
            "fedora" | "rhel" | "centos" | "rocky" | "almalinux" | "nobara" => {
                return Some(LinuxPackageManager::Dnf);
            }
            "opensuse" | "opensuse-tumbleweed" | "opensuse-leap" | "suse" | "sles" => {
                return Some(LinuxPackageManager::Zypper);
            }
            _ => {}
        }
    }
    None
}

/// Detects the active Linux distribution's package manager from `/etc/os-release` and `PATH`.
pub fn detect_host_package_manager() -> Option<LinuxPackageManager> {
    if let Ok(os_release) = std::fs::read_to_string("/etc/os-release") {
        if let Some(pm) = detect_package_manager_from_os_release(&os_release) {
            if crate::desktop::resolve_executable(pm.id()).is_some() {
                return Some(pm);
            }
        }
    }
    [
        LinuxPackageManager::Pacman,
        LinuxPackageManager::Apt,
        LinuxPackageManager::Dnf,
        LinuxPackageManager::Zypper,
    ]
    .into_iter()
    .find(|pm| crate::desktop::resolve_executable(pm.id()).is_some())
}

fn is_known_tool_id(tool_id: &str) -> bool {
    matches!(
        tool_id.to_ascii_lowercase().as_str(),
        "git"
            | "gh"
            | "github-cli"
            | "glab"
            | "gitlab-cli"
            | "platformio"
            | "platformio-core"
            | "pio"
            | "sigrok-cli"
            | "sigrok-firmware-fx2lafw"
            | "esptool"
            | "arduino-cli"
            | "probe-rs"
            | "probe-rs-tools"
            | "ghidra"
            | "radare2"
            | "r2"
            | "binutils"
            | "objdump"
            | "readelf"
            | "binwalk"
            | "jadx"
            | "squashfs-tools"
            | "unsquashfs"
            | "samply"
    )
}

fn resolve_distro_packages(pm: LinuxPackageManager, tool_id: &str) -> Option<&'static [&'static str]> {
    match (pm, tool_id.to_ascii_lowercase().as_str()) {
        (_, "git") => Some(&["git"]),
        (LinuxPackageManager::Pacman, "gh" | "github-cli") => Some(&["github-cli"]),
        (LinuxPackageManager::Apt | LinuxPackageManager::Dnf | LinuxPackageManager::Zypper, "gh" | "github-cli") => {
            Some(&["gh"])
        }
        (_, "glab" | "gitlab-cli") => Some(&["glab"]),
        (LinuxPackageManager::Pacman, "platformio" | "platformio-core" | "pio") => Some(&["platformio-core"]),
        (LinuxPackageManager::Apt | LinuxPackageManager::Dnf | LinuxPackageManager::Zypper, "platformio" | "platformio-core" | "pio") => {
            None
        }
        (_, "sigrok-cli") => Some(&["sigrok-cli"]),
        (_, "sigrok-firmware-fx2lafw") => Some(&["sigrok-firmware-fx2lafw"]),
        (_, "esptool") => Some(&["esptool"]),
        (LinuxPackageManager::Pacman, "arduino-cli") => Some(&["arduino-cli"]),
        (LinuxPackageManager::Apt | LinuxPackageManager::Dnf | LinuxPackageManager::Zypper, "arduino-cli") => None,
        (LinuxPackageManager::Pacman, "probe-rs" | "probe-rs-tools") => Some(&["probe-rs"]),
        (LinuxPackageManager::Apt | LinuxPackageManager::Dnf | LinuxPackageManager::Zypper, "probe-rs" | "probe-rs-tools") => {
            None
        }
        (LinuxPackageManager::Pacman, "ghidra") => Some(&["ghidra"]),
        (LinuxPackageManager::Apt | LinuxPackageManager::Dnf | LinuxPackageManager::Zypper, "ghidra") => None,
        (_, "radare2" | "r2") => Some(&["radare2"]),
        (_, "binutils" | "objdump" | "readelf") => Some(&["binutils"]),
        (_, "binwalk") => Some(&["binwalk"]),
        (LinuxPackageManager::Pacman, "jadx") => Some(&["jadx"]),
        (LinuxPackageManager::Apt | LinuxPackageManager::Dnf | LinuxPackageManager::Zypper, "jadx") => None,
        (_, "squashfs-tools" | "unsquashfs") => Some(&["squashfs-tools"]),
        (LinuxPackageManager::Pacman, "samply") => Some(&["samply"]),
        (LinuxPackageManager::Apt | LinuxPackageManager::Dnf | LinuxPackageManager::Zypper, "samply") => None,
        _ => None,
    }
}

/// Returns `(install_hint, auto_installable)` for `tool_id` on the detected host package manager.
pub fn resolve_tool_install_hint(tool_id: &str, fallback_hint: &str) -> (String, bool) {
    if let Some(pm) = detect_host_package_manager() {
        if let Some(pkgs) = resolve_distro_packages(pm, tool_id) {
            let can_pkexec = crate::desktop::resolve_executable("pkexec").is_some();
            return (format!("{} {}", pm.install_prefix(), pkgs.join(" ")), can_pkexec);
        }
    }
    (fallback_hint.to_string(), false)
}

/// Packages for `tool_ids` on `pm`, deduplicated; an empty list, unknown tool, or unmapped
/// distro package returns a typed error.
fn resolve_packages_for_pm(pm: LinuxPackageManager, tool_ids: &[String]) -> Result<Vec<&'static str>, DiagnosticError> {
    if tool_ids.is_empty() {
        return Err(DiagnosticError::NoTools);
    }
    let mut pkgs: Vec<&'static str> = Vec::with_capacity(tool_ids.len());
    for id in tool_ids {
        if !is_known_tool_id(id) {
            return Err(DiagnosticError::UnknownTool(id.clone()));
        }
        let mapped = resolve_distro_packages(pm, id).ok_or_else(|| DiagnosticError::ToolUnavailableInDistro {
            tool: id.clone(),
            distro: pm.display_name().to_string(),
        })?;
        for pkg in mapped {
            if !pkgs.contains(pkg) {
                pkgs.push(pkg);
            }
        }
    }
    Ok(pkgs)
}

/// Arguments of the daemon command `install_missing_tools`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct InstallMissingToolsArgs {
    pub tool_ids: Vec<String>,
}

/// Daemon command `install_missing_tools`: installs the tools using the detected Linux package
/// manager via `pkexec` (Polkit asks the user for consent).
pub async fn install_missing_tools(tool_ids: Vec<String>) -> Result<(), DiagnosticError> {
    crate::logger::log("DEBUG", "diagnostic", &format!("install_missing_tools: tool_ids={:?}", tool_ids));
    let pm = detect_host_package_manager().ok_or(DiagnosticError::UnsupportedPackageManager)?;
    let pkgs = resolve_packages_for_pm(pm, &tool_ids)?;
    let pkgs_joined = pkgs.join(" ");
    crate::logger::log(
        "INFO",
        "diagnostic",
        &format!("Requesting privileged {} installation via pkexec for packages: {}", pm.display_name(), pkgs_joined),
    );

    tokio::task::spawn_blocking(move || {
        let output = Command::new("pkexec")
            .args(pm.install_args())
            .args(&pkgs)
            .output()
            .map_err(|e| DiagnosticError::Spawn { program: "pkexec".to_string(), source: e })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let details = if stderr.is_empty() { String::from_utf8_lossy(&output.stdout).trim().to_string() } else { stderr };
            crate::logger::log("ERROR", "diagnostic", &format!("Failed to install packages '{}': {}", pkgs_joined, details));
            return Err(DiagnosticError::Install { packages: pkgs_joined, details });
        }
        crate::logger::log("INFO", "diagnostic", &format!("Successfully installed packages: {}", pkgs_joined));
        Ok(())
    })
    .await
    .map_err(|e| DiagnosticError::Task(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packages_are_deduplicated_and_unknown_or_empty_requests_rejected() {
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            resolve_packages_for_pm(LinuxPackageManager::Pacman, &ids(&["gh", "github-cli", "git", "glab"])).unwrap(),
            vec!["github-cli", "git", "glab"]
        );
        assert_eq!(
            resolve_packages_for_pm(LinuxPackageManager::Apt, &ids(&["gh", "github-cli", "git", "glab"])).unwrap(),
            vec!["gh", "git", "glab"]
        );
        assert_eq!(
            resolve_packages_for_pm(LinuxPackageManager::Pacman, &ids(&["ghidra", "radare2", "binutils", "binwalk", "jadx", "squashfs-tools"])).unwrap(),
            vec!["ghidra", "radare2", "binutils", "binwalk", "jadx", "squashfs-tools"]
        );
        assert!(matches!(
            resolve_packages_for_pm(LinuxPackageManager::Apt, &ids(&["ghidra"])),
            Err(DiagnosticError::ToolUnavailableInDistro { .. })
        ));
        assert!(matches!(resolve_packages_for_pm(LinuxPackageManager::Pacman, &ids(&[])), Err(DiagnosticError::NoTools)));
        assert!(matches!(resolve_packages_for_pm(LinuxPackageManager::Pacman, &ids(&["git", "mcp"])), Err(DiagnosticError::UnknownTool(_))));
    }

    #[test]
    fn os_release_detects_major_linux_distros() {
        assert_eq!(
            detect_package_manager_from_os_release("ID=ubuntu\nID_LIKE=debian\n"),
            Some(LinuxPackageManager::Apt)
        );
        assert_eq!(
            detect_package_manager_from_os_release("ID=fedora\n"),
            Some(LinuxPackageManager::Dnf)
        );
        assert_eq!(
            detect_package_manager_from_os_release("ID=\"opensuse-tumbleweed\"\nID_LIKE=\"opensuse suse\"\n"),
            Some(LinuxPackageManager::Zypper)
        );
        assert_eq!(
            detect_package_manager_from_os_release("ID=omarchy\nID_LIKE=arch\n"),
            Some(LinuxPackageManager::Pacman)
        );
    }

    #[test]
    fn a_missing_program_is_absent_not_an_error() {
        assert!(run_tool("oneship-nonexistent-tool-xyz", &[]).unwrap().is_none());
    }
}
