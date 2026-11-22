//! Actions on the user's desktop session (Hyprland window management and detached processes).
//! The daemon unit carries the session environment (`WAYLAND_DISPLAY`, spec 12 §7.2).

use std::collections::HashSet;
use serde_json::Value;

/// Command that starts `program` as a desktop window in its own transient systemd scope
/// (`systemd-run --user --scope`), outside the daemon's cgroup: stopping or restarting the daemon
/// unit kills what is left in its cgroup, and must never close the user's windows. Falls back to
/// a detached Unix process group when `systemd-run` or a user D-Bus session is unavailable.
pub fn desktop_command(program: &str) -> std::process::Command {
    let has_systemd_user = resolve_executable("systemd-run").is_some()
        && std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some_and(|v| !v.is_empty());
    if has_systemd_user {
        let mut cmd = std::process::Command::new("systemd-run");
        cmd.args(["--user", "--scope", "--collect", "--quiet", "--", program]);
        return cmd;
    }
    let mut cmd = std::process::Command::new(program);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd
}

/// Returns `true` when running inside a live Hyprland session with `hyprctl` available.
pub fn is_hyprland_session() -> bool {
    std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some_and(|v| !v.is_empty())
        && resolve_executable("hyprctl").is_some()
}

/// Spawns `cmd` without supervising it; a thread waits for its exit so it never stays a zombie
/// of the daemon, and logs a failed exit.
pub fn spawn_detached(mut cmd: std::process::Command, label: &str) -> std::io::Result<()> {
    let mut child = cmd.spawn()?;
    let label = label.to_string();
    std::thread::spawn(move || match child.wait() {
        Ok(status) if !status.success() => {
            crate::logger::log("WARN", "desktop", &format!("{} exited with {}", label, status));
        }
        Ok(_) => {}
        Err(e) => crate::logger::log("WARN", "desktop", &format!("Cannot wait for {}: {}", label, e)),
    });
    Ok(())
}

/// Matched Hyprland client window metadata.
#[derive(Debug, Clone)]
pub struct HyprlandWindowMatch {
    pub address: String,
    pub pid: u32,
    pub workspace_id: Option<i64>,
}

/// Checks whether a host process PID belongs to `project_path` by inspecting `/proc/<pid>/cwd`
/// and `/proc/<pid>/cmdline`.
fn proc_pid_matches_project(pid: u32, project_path: &str) -> bool {
    let proc_dir = std::path::PathBuf::from(format!("/proc/{}", pid));
    if let Ok(cwd) = std::fs::read_link(proc_dir.join("cwd")) {
        let cwd_str = cwd.to_string_lossy();
        if cwd_str == project_path || cwd_str.starts_with(&format!("{}/", project_path)) {
            return true;
        }
    }
    if let Ok(cmdline_bytes) = std::fs::read(proc_dir.join("cmdline")) {
        let cmdline = String::from_utf8_lossy(&cmdline_bytes);
        if cmdline.contains(project_path) {
            return true;
        }
    }
    false
}

/// Finds the lowest unoccupied Hyprland workspace ID (starting after the active workspace
/// and wrapping around 1..=99) so external GUI tools open in a clean, contiguous workspace.
pub async fn find_empty_hyprland_workspace() -> Option<i64> {
    if !is_hyprland_session() {
        return None;
    }
    let active_out = tokio::process::Command::new("hyprctl")
        .args(["activeworkspace", "-j"])
        .output()
        .await
        .ok()?;
    if !active_out.status.success() {
        return None;
    }
    let active_json: Value = serde_json::from_slice(&active_out.stdout).ok()?;
    let active_id = active_json.get("id").and_then(|v| v.as_i64()).unwrap_or(1);

    let ws_out = tokio::process::Command::new("hyprctl")
        .args(["workspaces", "-j"])
        .output()
        .await
        .ok()?;
    if !ws_out.status.success() {
        return None;
    }
    let workspaces: Vec<Value> = serde_json::from_slice(&ws_out.stdout).ok()?;
    let occupied: HashSet<i64> = workspaces
        .iter()
        .filter_map(|w| w.get("id").and_then(|v| v.as_i64()))
        .collect();

    for candidate in (active_id + 1)..=99 {
        if !occupied.contains(&candidate) {
            return Some(candidate);
        }
    }
    for candidate in 1..active_id {
        if !occupied.contains(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Finds an open Hyprland window whose `class` or `initialClass` matches `class_names`
/// AND whose process `/proc/<pid>/cwd` or `/proc/<pid>/cmdline` belongs to `project_path`.
pub async fn find_hyprland_window_for_project(
    class_names: &[&str],
    project_path: &str,
) -> Option<HyprlandWindowMatch> {
    if !is_hyprland_session() {
        return None;
    }
    let out = tokio::process::Command::new("hyprctl")
        .args(["clients", "-j"])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let clients: Vec<Value> = serde_json::from_slice(&out.stdout).ok()?;
    let mut candidates = Vec::new();
    for c in clients {
        let cls = c.get("class").and_then(|v| v.as_str()).unwrap_or("");
        let init_cls = c.get("initialClass").and_then(|v| v.as_str()).unwrap_or("");
        let matches = class_names.iter().any(|target| {
            cls.eq_ignore_ascii_case(target) || init_cls.eq_ignore_ascii_case(target)
        });
        if matches {
            if let (Some(addr), Some(pid)) = (
                c.get("address").and_then(|v| v.as_str()).map(str::to_string),
                c.get("pid").and_then(|v| v.as_u64()).map(|p| p as u32),
            ) {
                let ws_id = c.get("workspace").and_then(|w| w.get("id")).and_then(|v| v.as_i64());
                candidates.push(HyprlandWindowMatch {
                    address: addr,
                    pid,
                    workspace_id: ws_id,
                });
            }
        }
    }
    if candidates.is_empty() {
        return None;
    }

    let proj = project_path.to_string();
    tokio::task::spawn_blocking(move || {
        candidates
            .into_iter()
            .find(|win| proc_pid_matches_project(win.pid, &proj))
    })
    .await
    .ok()
    .flatten()
}

/// Switches to the window's workspace (if known) and focuses it by Hyprland window address.
pub async fn focus_hyprland_window(win: &HyprlandWindowMatch) {
    if !is_hyprland_session() {
        return;
    }
    if let Some(ws_id) = win.workspace_id {
        let _ = tokio::process::Command::new("hyprctl")
            .args(["dispatch", "workspace", &ws_id.to_string()])
            .output()
            .await;
    }
    let _ = tokio::process::Command::new("hyprctl")
        .args(["dispatch", "focuswindow", &format!("address:{}", win.address)])
        .output()
        .await;
}

/// Polls Hyprland briefly until the spawned editor registers a window (matching either `target_pid`
/// or `class_names` + `project_path`), moves it to `workspace_id`, and focuses it.
pub async fn wait_and_move_hyprland_window(
    target_pid: u32,
    class_names: &[&str],
    project_path: &str,
    workspace_id: i64,
) {
    if !is_hyprland_session() {
        return;
    }
    const MAX_ATTEMPTS_COUNT: usize = 100;
    const POLL_STEP_MS: u64 = 100;

    for _ in 0..MAX_ATTEMPTS_COUNT {
        if let Ok(out) = tokio::process::Command::new("hyprctl")
            .args(["clients", "-j"])
            .output()
            .await
        {
            if out.status.success() {
                if let Ok(clients) = serde_json::from_slice::<Vec<Value>>(&out.stdout) {
                    let direct = clients.iter().find(|c| {
                        c.get("pid").and_then(|v| v.as_u64()) == Some(u64::from(target_pid))
                    });
                    if let Some(win) = direct {
                        if let Some(addr) = win.get("address").and_then(|v| v.as_str()) {
                            let _ = tokio::process::Command::new("hyprctl")
                                .args([
                                    "dispatch",
                                    "movetoworkspace",
                                    &format!("{},address:{}", workspace_id, addr),
                                ])
                                .output()
                                .await;
                            let _ = tokio::process::Command::new("hyprctl")
                                .args(["dispatch", "focuswindow", &format!("address:{}", addr)])
                                .output()
                                .await;
                            return;
                        }
                    }
                }
            }
        }

        if let Some(win) = find_hyprland_window_for_project(class_names, project_path).await {
            let _ = tokio::process::Command::new("hyprctl")
                .args([
                    "dispatch",
                    "movetoworkspace",
                    &format!("{},address:{}", workspace_id, win.address),
                ])
                .output()
                .await;
            let _ = tokio::process::Command::new("hyprctl")
                .args(["dispatch", "focuswindow", &format!("address:{}", win.address)])
                .output()
                .await;
            return;
        }

        tokio::time::sleep(std::time::Duration::from_millis(POLL_STEP_MS)).await;
    }
}

fn is_executable_file(path: &std::path::Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        (meta.permissions().mode() & 0o111) != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Standard user-local binary directories checked after `PATH` so tools installed while the
/// daemon is running (e.g. `agy`, `opencode`, `claude`, `rea`) are detected without restarting.
const EXTRA_USER_BIN_DIRS: &[&str] = &[
    ".local/bin",
    ".cargo/bin",
    ".opencode/bin",
    ".antigravity/bin",
    ".npm-global/bin",
];

/// Resolves an executable configured as either an explicit path (containing a separator) or a
/// bare command name searched across `PATH` and standard user-local bin directories.
/// Returns `None` if empty or not found/executable.
pub fn resolve_executable(configured: &str) -> Option<std::path::PathBuf> {
    let trimmed = configured.trim();
    if trimmed.is_empty() {
        return None;
    }
    let candidate = std::path::Path::new(trimmed);
    if candidate.components().count() > 1 || candidate.is_absolute() {
        return is_executable_file(candidate).then(|| candidate.to_path_buf());
    }
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let joined = dir.join(trimmed);
            if is_executable_file(&joined) {
                return Some(joined);
            }
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home_path = std::path::PathBuf::from(home);
        for rel in EXTRA_USER_BIN_DIRS {
            let joined = home_path.join(rel).join(trimmed);
            if is_executable_file(&joined) {
                return Some(joined);
            }
        }
    }
    None
}

/// Standard fallback terminal emulators probed when `system.preferred_terminal` is unavailable.
const FALLBACK_TERMINALS: &[&str] = &[
    "ghostty",
    "kitty",
    "alacritty",
    "foot",
    "wezterm",
    "ptyxis",
    "kgx",
    "gnome-terminal",
    "konsole",
    "xfce4-terminal",
    "x-terminal-emulator",
    "xterm",
];

/// Resolves the terminal emulator to launch: `configured` if present on `PATH`, otherwise the
/// first available standard Linux terminal emulator. Logs a warning on fallback.
pub fn resolve_preferred_terminal(configured: &str) -> Option<(String, std::path::PathBuf)> {
    let trimmed = configured.trim();
    if let Some(path) = resolve_executable(trimmed) {
        let name = std::path::Path::new(trimmed)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(trimmed)
            .to_string();
        return Some((name, path));
    }
    for candidate in FALLBACK_TERMINALS {
        if *candidate == trimmed {
            continue;
        }
        if let Some(path) = resolve_executable(candidate) {
            crate::logger::log(
                "WARN",
                "desktop",
                &format!("Preferred terminal '{}' not found; falling back to '{}'", trimmed, candidate),
            );
            return Some(((*candidate).to_string(), path));
        }
    }
    None
}

/// Builds the terminal-specific arguments required to execute `command` inside `term_name`.
pub fn build_terminal_exec_args<'a>(term_name: &str, command: &'a str) -> Vec<&'a str> {
    match term_name {
        "wezterm" => vec!["start", "--", command],
        "gnome-terminal" | "ptyxis" | "kgx" => vec!["--", command],
        "foot" => vec![command],
        _ => vec!["-e", command],
    }
}

/// Builds the terminal-specific arguments required to execute a shell `script` via `sh -c`.
pub fn build_terminal_shell_args<'a>(term_name: &str, script: &'a str) -> Vec<&'a str> {
    match term_name {
        "wezterm" => vec!["start", "--", "sh", "-c", script],
        "gnome-terminal" | "ptyxis" | "kgx" => vec!["--", "sh", "-c", script],
        "foot" => vec!["sh", "-c", script],
        _ => vec!["-e", "sh", "-c", script],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_executable_finds_path_and_rejects_missing() {
        assert!(resolve_executable("").is_none());
        assert!(resolve_executable("   ").is_none());
        assert!(resolve_executable("/nonexistent/oneship/binary_xyz").is_none());
        assert!(resolve_executable("oneship-nonexistent-cmd-xyz").is_none());
        assert!(resolve_executable("sh").is_some());
    }

    #[test]
    fn terminal_exec_args_match_emulator_conventions() {
        assert_eq!(build_terminal_exec_args("wezterm", "agy"), vec!["start", "--", "agy"]);
        assert_eq!(build_terminal_exec_args("gnome-terminal", "agy"), vec!["--", "agy"]);
        assert_eq!(build_terminal_exec_args("foot", "agy"), vec!["agy"]);
        assert_eq!(build_terminal_exec_args("ghostty", "agy"), vec!["-e", "agy"]);
        assert_eq!(build_terminal_exec_args("konsole", "agy"), vec!["-e", "agy"]);
    }
}
