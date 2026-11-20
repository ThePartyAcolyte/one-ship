use std::collections::{HashMap, HashSet};
use std::io::ErrorKind;
use std::path::Path;
use serde::{Deserialize, Serialize};

const RUN_ID_ENV_PREFIX: &[u8] = b"ONESHIP_RUN_ID=";
/// `st` column value of a listening socket in `/proc/net/tcp*`.
const TCP_LISTEN_STATE: &str = "0A";

/// Failure of a `/proc` inspection or of a port release.
#[derive(Debug, thiserror::Error)]
pub enum PortInspectError {
    #[error("Cannot read {path}: {source}")]
    ProcRead {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("Malformed {path}: {details}")]
    Malformed { path: String, details: String },
    /// A socket listens on the port but its owner's descriptors are not readable (another user).
    #[error("Port {port} is held by a process whose owner cannot be inspected")]
    OwnerNotVisible { port: u16 },
    /// The occupant belongs to the ONE-SHIP process or its process group.
    #[error("Port {port} is held by ONE-SHIP itself (PID {pid}, PGID {pgid}); refusing to terminate it")]
    ProtectedProcess { port: u16, pid: u32, pgid: u32 },
    #[error("Cannot send SIGKILL to {target}: {source}")]
    Signal {
        target: String,
        #[source]
        source: std::io::Error,
    },
    #[error("Port {port} is still occupied {timeout_ms}ms after SIGKILL to PID {pid}")]
    ReleaseTimeout { port: u16, pid: u32, timeout_ms: u64 },
    #[error("Background task failed: {0}")]
    Task(String),
}

fn proc_read_err(path: &Path) -> impl FnOnce(std::io::Error) -> PortInspectError + '_ {
    move |source| PortInspectError::ProcRead { path: path.display().to_string(), source }
}

/// Detailed occupant information for an occupied network port.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortOccupant {
    pub port: u16,
    pub pid: u32,
    pub pgid: u32,
    pub cmdline: String,
    pub oneship_run_id: Option<String>,
}

/// Inodes of the TCP sockets listening on `target_port` (`/proc/net/tcp` and `/proc/net/tcp6`).
///
/// `/proc/net/tcp6` is absent when the kernel has IPv6 disabled; that means no IPv6 sockets.
fn listening_socket_inodes(target_port: u16) -> Result<HashSet<u64>, PortInspectError> {
    let port_suffix = format!(":{:04X}", target_port);
    let mut inodes = HashSet::new();
    for net_path in ["/proc/net/tcp", "/proc/net/tcp6"] {
        let path = Path::new(net_path);
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) if e.kind() == ErrorKind::NotFound && net_path.ends_with('6') => continue,
            Err(e) => return Err(proc_read_err(path)(e)),
        };
        // Columns: sl local_address rem_address st tx_queue:rx_queue tr:tm->when retrnsmt uid timeout inode
        for line in content.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 10 {
                return Err(PortInspectError::Malformed { path: net_path.into(), details: format!("short line '{}'", line) });
            }
            if parts[3] != TCP_LISTEN_STATE || !parts[1].ends_with(&port_suffix) {
                continue;
            }
            let inode = parts[9].parse::<u64>().map_err(|_| PortInspectError::Malformed {
                path: net_path.into(),
                details: format!("inode '{}'", parts[9]),
            })?;
            inodes.insert(inode);
        }
    }
    Ok(inodes)
}

/// Whether one of `pid`'s file descriptors is a socket in `inodes`.
///
/// `None` when the descriptors cannot be read: the process exited during the scan or belongs to
/// another user.
fn owns_socket(proc_entry: &Path, inodes: &HashSet<u64>) -> Option<bool> {
    let fds = std::fs::read_dir(proc_entry.join("fd")).ok()?;
    let owns = fds.flatten().any(|fd| {
        std::fs::read_link(fd.path())
            .ok()
            .and_then(|target| {
                let target = target.to_string_lossy();
                target.strip_prefix("socket:[")?.strip_suffix(']')?.parse::<u64>().ok()
            })
            .is_some_and(|inode| inodes.contains(&inode))
    });
    Some(owns)
}

/// Discovers the process listening on `target_port` from `/proc/net/tcp*` socket inodes and
/// `/proc/<pid>/fd/*` descriptors.
///
/// `Ok(None)` means no socket listens on the port. A listening socket whose owner cannot be
/// inspected is `OwnerNotVisible`, never reported as a free port.
pub fn find_port_listener(target_port: u16) -> Result<Option<PortOccupant>, PortInspectError> {
    let inodes = listening_socket_inodes(target_port)?;
    if inodes.is_empty() {
        return Ok(None);
    }

    let proc_dir = Path::new("/proc");
    let entries = std::fs::read_dir(proc_dir).map_err(proc_read_err(proc_dir))?;
    for entry in entries.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        if owns_socket(&entry.path(), &inodes) != Some(true) {
            continue;
        }
        let cmdline_path = entry.path().join("cmdline");
        let cmdline_bytes = std::fs::read(&cmdline_path).map_err(proc_read_err(&cmdline_path))?;
        let cmdline = String::from_utf8_lossy(&cmdline_bytes).replace('\0', " ").trim().to_string();
        return Ok(Some(PortOccupant {
            port: target_port,
            pid,
            pgid: parse_proc_pgid(pid)?,
            cmdline,
            oneship_run_id: read_proc_run_id(pid)?,
        }));
    }
    Err(PortInspectError::OwnerNotVisible { port: target_port })
}

/// `ONESHIP_RUN_ID` value in a NUL-separated `/proc/<pid>/environ` buffer.
fn run_id_in_environ(bytes: &[u8]) -> Option<String> {
    bytes
        .split(|&b| b == 0)
        .filter_map(|chunk| chunk.strip_prefix(RUN_ID_ENV_PREFIX))
        .find(|value| !value.is_empty())
        .map(|value| String::from_utf8_lossy(value).into_owned())
}

/// Single-pass scan of `/proc`, grouping the PIDs whose environment carries `ONESHIP_RUN_ID`.
///
/// Processes whose environment is unreadable (exited during the scan, or another user's) are
/// skipped: ONE-SHIP only spawns processes as the current user.
pub fn scan_proc_for_oneship_run_ids() -> Result<HashMap<String, Vec<u32>>, PortInspectError> {
    let proc_dir = Path::new("/proc");
    let entries = std::fs::read_dir(proc_dir).map_err(proc_read_err(proc_dir))?;
    let mut map: HashMap<String, Vec<u32>> = HashMap::new();
    for entry in entries.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(bytes) = std::fs::read(entry.path().join("environ")) else {
            continue;
        };
        if let Some(run_id) = run_id_in_environ(&bytes) {
            map.entry(run_id).or_default().push(pid);
        }
    }
    Ok(map)
}

/// Reads the `ONESHIP_RUN_ID` environment variable of `pid`.
pub fn read_proc_run_id(pid: u32) -> Result<Option<String>, PortInspectError> {
    let path = format!("/proc/{}/environ", pid);
    let bytes = std::fs::read(&path).map_err(proc_read_err(Path::new(&path)))?;
    Ok(run_id_in_environ(&bytes))
}

/// Process group ID (`pgrp`, the third field after the command name) from `/proc/<pid>/stat`.
pub fn parse_proc_pgid(pid: u32) -> Result<u32, PortInspectError> {
    let path = format!("/proc/{}/stat", pid);
    let content = std::fs::read_to_string(&path).map_err(proc_read_err(Path::new(&path)))?;
    // The command name may contain spaces and parentheses; fields resume after the last ')'.
    content
        .rfind(')')
        .and_then(|rparen| content[rparen + 1..].split_whitespace().nth(2))
        .and_then(|field| field.parse::<u32>().ok())
        .ok_or(PortInspectError::Malformed { path, details: "no pgrp field".into() })
}

/// Sends SIGKILL to `target` (a PID, or a negated PGID for the whole group). A target that no
/// longer exists (`ESRCH`) is already terminated.
fn send_sigkill(target: i32, label: String) -> Result<(), PortInspectError> {
    debug_assert!(target != 0, "kill(0) would signal ONE-SHIP's own process group");
    // SAFETY: kill(2) has no memory-safety preconditions.
    if unsafe { libc::kill(target, libc::SIGKILL) } == 0 {
        return Ok(());
    }
    let source = std::io::Error::last_os_error();
    if source.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }
    Err(PortInspectError::Signal { target: label, source })
}

/// Terminates a port occupant and its process group with SIGKILL, then polls until the port is
/// free (bounded by `timeout_ms`, step `poll_step_ms`).
///
/// Refuses an occupant in ONE-SHIP's own process or process group.
pub async fn terminate_port_occupant(occupant: &PortOccupant, timeout_ms: u64, poll_step_ms: u64) -> Result<(), PortInspectError> {
    debug_assert!(occupant.pid > 0 && occupant.pgid > 0, "PortOccupant PID/PGID must be non-zero");
    debug_assert!(poll_step_ms > 0, "poll_step_ms must be non-zero");
    let own_pid = std::process::id();
    // SAFETY: getpgrp(2) has no preconditions and cannot fail.
    let own_pgid = unsafe { libc::getpgrp() } as u32;
    if occupant.pid == own_pid || occupant.pgid == own_pgid {
        return Err(PortInspectError::ProtectedProcess { port: occupant.port, pid: occupant.pid, pgid: occupant.pgid });
    }

    crate::logger::log("WARN", "port_inspect", &format!(
        "Terminating port occupant on port {} (PID {}, PGID {}, cmd: '{}', run_id: {:?})",
        occupant.port, occupant.pid, occupant.pgid, occupant.cmdline, occupant.oneship_run_id
    ));
    send_sigkill(-(occupant.pgid as i32), format!("process group {}", occupant.pgid))?;
    send_sigkill(occupant.pid as i32, format!("PID {}", occupant.pid))?;

    let max_attempts_count = (timeout_ms / poll_step_ms).max(1);
    for _ in 0..max_attempts_count {
        tokio::time::sleep(tokio::time::Duration::from_millis(poll_step_ms)).await;
        let port = occupant.port;
        let listener = tokio::task::spawn_blocking(move || find_port_listener(port))
            .await
            .map_err(|e| PortInspectError::Task(e.to_string()))?;
        match listener {
            Ok(None) => {
                crate::logger::log("INFO", "port_inspect", &format!(
                    "Freed port {} after terminating occupant PID {}",
                    occupant.port, occupant.pid
                ));
                return Ok(());
            }
            // The socket can outlive the killed process for a moment.
            Ok(Some(_)) | Err(PortInspectError::OwnerNotVisible { .. }) => {}
            Err(e) => return Err(e),
        }
    }

    Err(PortInspectError::ReleaseTimeout { port: occupant.port, pid: occupant.pid, timeout_ms })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_proc_for_oneship_run_ids_reads_proc() {
        assert!(scan_proc_for_oneship_run_ids().is_ok());
    }

    #[test]
    fn parse_proc_pgid_of_current_process() {
        let pgid = parse_proc_pgid(std::process::id()).expect("current process must have a readable stat");
        assert_eq!(pgid, unsafe { libc::getpgrp() } as u32);
    }

    #[test]
    fn run_id_is_read_from_environ_buffer() {
        assert_eq!(run_id_in_environ(b"A=1\0ONESHIP_RUN_ID=svc_web_1\0B=2\0").as_deref(), Some("svc_web_1"));
        assert_eq!(run_id_in_environ(b"ONESHIP_RUN_ID=\0A=1\0"), None);
        assert_eq!(run_id_in_environ(b"A=1\0"), None);
    }

    #[tokio::test]
    async fn own_process_group_is_never_terminated() {
        let occupant = PortOccupant {
            port: 1,
            pid: std::process::id(),
            pgid: unsafe { libc::getpgrp() } as u32,
            cmdline: "oneship".into(),
            oneship_run_id: None,
        };
        let err = terminate_port_occupant(&occupant, 100, 50).await.unwrap_err();
        assert!(matches!(err, PortInspectError::ProtectedProcess { .. }));
    }
}
