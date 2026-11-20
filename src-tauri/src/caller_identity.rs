//! Process-attested MCP caller identity (spec 12 §6).
//!
//! The MCP endpoint is global and shared by every agent, so the caller is identified
//! from the operating system instead of from anything the LLM or the client config
//! declares: TCP peer address -> owning PID -> `ONESHIP_AGENT_INSTANCE` in that
//! process environment -> row in `agent_instances`. Every failure is a typed
//! [`CallerError`]; there is no CWD, name, first-project or suffix fallback.
//!
//! All functions here perform blocking `/proc` and SQLite I/O. Async callers MUST invoke
//! [`resolve_caller`] through `tokio::task::spawn_blocking`.

use std::fs;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;

use thiserror::Error;

use crate::db::{self, AgentInstanceScope, DatabaseError};

/// Environment variable injected by the supervisor into every managed agent process.
pub const AGENT_INSTANCE_ENV_KEY: &str = "ONESHIP_AGENT_INSTANCE";

/// Reasons a caller could not be attributed to a managed agent instance.
#[derive(Debug, Error)]
pub enum CallerError {
    /// No socket row or owning process was found for the peer address.
    #[error("Caller socket {peer} could not be resolved to a process")]
    SocketUnresolved { peer: SocketAddr },

    /// The peer address is not IPv4 loopback, the only transport the daemon serves.
    #[error("Caller address {peer} is not IPv4 loopback")]
    UnsupportedPeerAddress { peer: SocketAddr },

    /// The owning process exited before `/proc` could be read.
    #[error("Caller process {pid} exited before its identity could be read")]
    ProcessGone { pid: u32 },

    /// The process carries no `ONESHIP_AGENT_INSTANCE`: an external, unmanaged session.
    #[error("Caller process {pid} is not a ONE-SHIP managed agent")]
    UnmanagedCaller { pid: u32 },

    /// The identity is not present in `agent_instances`.
    #[error("Agent instance '{instance_id}' is not registered")]
    InstanceUnknown { instance_id: String },

    /// The instance row exists but the agent process was already terminated.
    #[error("Agent instance '{instance_id}' is terminated")]
    InstanceTerminated { instance_id: String },

    /// The calling PID is neither the registered PID nor one of its descendants.
    #[error("Caller process {caller_pid} does not belong to instance '{instance_id}' (registered pid {registered_pid})")]
    IdentityMismatch { instance_id: String, caller_pid: u32, registered_pid: u32 },

    /// A registered project-scoped instance references a project or persona that no longer exists.
    #[error("Agent instance '{instance_id}' references a missing {entity}")]
    DanglingReference { instance_id: String, entity: &'static str },

    /// A `/proc` read failed for a reason other than the process being gone.
    #[error("Failed to read {context}: {source}")]
    ProcIo {
        context: &'static str,
        #[source]
        source: std::io::Error,
    },

    /// An operator API caller that does not descend from the ONE-SHIP app binary.
    #[error("Caller process {pid} does not descend from the ONE-SHIP app ({app_exe})")]
    NotFromApp { pid: u32, app_exe: String },

    /// The HTTP layer supplied no peer address, so the caller cannot be located.
    #[error("Request carried no peer address")]
    PeerAddressMissing,

    /// The blocking resolver task panicked or was cancelled.
    #[error("Caller resolver task failed: {details}")]
    ResolverTaskFailed { details: String },

    /// Registry lookup failed.
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

/// Verified identity of the agent process behind one MCP request.
///
/// Built per request by [`resolve_caller`]; it is the only source of project, agent
/// and runner attribution for tool handlers. `project_*` and `agent_*` fields are
/// `Some` exactly when `scope == Project`.
#[derive(Debug, Clone)]
pub struct CallerContext {
    pub instance_id: String,
    pub scope: AgentInstanceScope,
    pub project_id: Option<i64>,
    pub project_path: Option<String>,
    pub agent_uid: Option<String>,
    pub agent_display_name: Option<String>,
    pub runner: String,
    pub pid: u32,
}

/// Formats an IPv4 socket address the way `/proc/net/tcp` prints it (`0100007F:1F49`).
///
/// The kernel prints the raw network-order address as a host-order `u32`; this is
/// little-endian on the target machine (x86-64 Linux).
fn proc_net_tcp_addr(ip: std::net::Ipv4Addr, port: u16) -> String {
    format!("{:08X}:{:04X}", u32::from_le_bytes(ip.octets()), port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_descendant_check_accepts_the_app_and_rejects_other_trees_as_not_from_app() {
        let own_pid = std::process::id();
        let own_exe = std::env::current_exe().unwrap();
        assert!(require_app_descendant(own_pid, &own_exe, 16).is_ok());
        // Walking past ancestors whose exe is unreadable (e.g. the compositor) still ends in NotFromApp.
        let foreign = Path::new("/nonexistent/one-ship");
        assert!(matches!(require_app_descendant(own_pid, foreign, 64), Err(CallerError::NotFromApp { .. })));
    }

    #[test]
    fn proc_net_tcp_addr_matches_kernel_format() {
        assert_eq!(proc_net_tcp_addr(std::net::Ipv4Addr::LOCALHOST, 8765), "0100007F:223D");
    }

    #[test]
    fn non_loopback_peer_is_rejected() {
        let peer: SocketAddr = "192.168.1.5:40000".parse().unwrap();
        assert!(matches!(
            find_client_socket_inode(peer, 8765),
            Err(CallerError::UnsupportedPeerAddress { .. })
        ));
    }

    #[test]
    fn current_process_is_its_own_descendant_and_walks_to_parent() {
        let me = std::process::id();
        assert!(is_self_or_descendant(me, me, 0).unwrap());
        let parent = read_parent_pid(me).unwrap();
        assert!(is_self_or_descendant(me, parent, 1).unwrap());
        assert!(!is_self_or_descendant(me, parent, 0).unwrap());
    }

    #[test]
    fn process_without_identity_env_is_unmanaged() {
        std::env::remove_var(AGENT_INSTANCE_ENV_KEY);
        assert_eq!(read_instance_id_from_environ(std::process::id()).unwrap(), None);
    }
}

/// Finds the inode of the client-side socket whose local address is `peer` and whose
/// remote address is `127.0.0.1:server_port`.
fn find_client_socket_inode(peer: SocketAddr, server_port: u16) -> Result<u64, CallerError> {
    let peer_ip = match peer.ip() {
        IpAddr::V4(ip) if ip.is_loopback() => ip,
        _ => return Err(CallerError::UnsupportedPeerAddress { peer }),
    };
    let local_needle = proc_net_tcp_addr(peer_ip, peer.port());
    let remote_needle = proc_net_tcp_addr(std::net::Ipv4Addr::LOCALHOST, server_port);

    let content = fs::read_to_string("/proc/net/tcp")
        .map_err(|source| CallerError::ProcIo { context: "/proc/net/tcp", source })?;
    for line in content.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 10 || parts[1] != local_needle || parts[2] != remote_needle {
            continue;
        }
        let inode: u64 = parts[9].parse().unwrap_or(0);
        if inode > 0 {
            return Ok(inode);
        }
    }
    Err(CallerError::SocketUnresolved { peer })
}

/// Finds a PID holding `socket:[inode]` among its open file descriptors.
fn find_pid_holding_socket(inode: u64, peer: SocketAddr) -> Result<u32, CallerError> {
    let needle = format!("socket:[{}]", inode);
    let proc_entries = fs::read_dir("/proc")
        .map_err(|source| CallerError::ProcIo { context: "/proc", source })?;
    for entry in proc_entries.flatten() {
        let pid: u32 = match entry.file_name().to_string_lossy().parse() {
            Ok(pid) => pid,
            Err(_) => continue,
        };
        // Processes may exit or deny access while scanning; those are simply not the owner.
        let fds = match fs::read_dir(entry.path().join("fd")) {
            Ok(fds) => fds,
            Err(_) => continue,
        };
        let holds_socket = fds
            .flatten()
            .any(|fd| fs::read_link(fd.path()).map(|l| l.to_string_lossy() == needle).unwrap_or(false));
        if holds_socket {
            return Ok(pid);
        }
    }
    Err(CallerError::SocketUnresolved { peer })
}

/// Reads `ONESHIP_AGENT_INSTANCE` from `/proc/<pid>/environ`; `None` if the process has none.
fn read_instance_id_from_environ(pid: u32) -> Result<Option<String>, CallerError> {
    let raw = match fs::read(Path::new("/proc").join(pid.to_string()).join("environ")) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(CallerError::ProcessGone { pid }),
        Err(source) => return Err(CallerError::ProcIo { context: "process environ", source }),
    };
    let prefix = format!("{}=", AGENT_INSTANCE_ENV_KEY);
    for entry in raw.split(|b| *b == 0) {
        let text = String::from_utf8_lossy(entry);
        if let Some(value) = text.strip_prefix(&prefix) {
            return Ok(Some(value.to_string()));
        }
    }
    Ok(None)
}

/// Reads the parent PID from `/proc/<pid>/stat`.
fn read_parent_pid(pid: u32) -> Result<u32, CallerError> {
    let stat = match fs::read_to_string(Path::new("/proc").join(pid.to_string()).join("stat")) {
        Ok(stat) => stat,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(CallerError::ProcessGone { pid }),
        Err(source) => return Err(CallerError::ProcIo { context: "process stat", source }),
    };
    // The comm field may contain spaces and parentheses; fields resume after the last ')'.
    let after_comm = stat.rsplit_once(')').map(|(_, rest)| rest).unwrap_or("");
    let mut fields = after_comm.split_whitespace();
    let _state = fields.next();
    fields
        .next()
        .and_then(|ppid| ppid.parse().ok())
        .ok_or(CallerError::ProcessGone { pid })
}

/// True if `pid == ancestor_pid` or `ancestor_pid` is reachable by walking parents,
/// bounded by `max_depth_count` hops.
fn is_self_or_descendant(pid: u32, ancestor_pid: u32, max_depth_count: usize) -> Result<bool, CallerError> {
    let mut current = pid;
    for _ in 0..=max_depth_count {
        if current == ancestor_pid {
            return Ok(true);
        }
        if current <= 1 {
            return Ok(false);
        }
        current = read_parent_pid(current)?;
    }
    Ok(false)
}

/// Executable of `pid` (`/proc/<pid>/exe`). A binary replaced on disk while running (e.g. a
/// rebuild during `tauri dev`) reads as `<path> (deleted)`; the suffix is stripped.
///
/// `Ok(None)` when the link is not readable (`EACCES`): the process is not dumpable for this
/// user (another user's process, or one with capabilities such as the compositor). The ONE-SHIP
/// app runs as this user without capabilities, so its exe is always readable; an unreadable
/// exe is therefore never the app.
fn read_process_exe(pid: u32) -> Result<Option<std::path::PathBuf>, CallerError> {
    match fs::read_link(Path::new("/proc").join(pid.to_string()).join("exe")) {
        Ok(path) => {
            let text = path.to_string_lossy();
            Ok(Some(std::path::PathBuf::from(text.strip_suffix(" (deleted)").unwrap_or(&text))))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(CallerError::ProcessGone { pid }),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Ok(None),
        Err(source) => Err(CallerError::ProcIo { context: "process exe", source }),
    }
}

/// File name of the ONE-SHIP desktop app binary (Cargo `[[bin]]`), built next to `oneship-daemon`.
pub const APP_BINARY_NAME: &str = "one-ship";

/// Path of the app binary that may call the operator API: `APP_BINARY_NAME` in the directory of
/// the running daemon executable (both are built into the same Cargo target directory).
pub fn operator_app_exe() -> Result<std::path::PathBuf, CallerError> {
    let daemon_exe = std::env::current_exe().map_err(|source| CallerError::ProcIo { context: "daemon exe", source })?;
    Ok(daemon_exe.with_file_name(APP_BINARY_NAME))
}

/// Requires `pid` (the process holding an operator API socket) to be `app_exe` or one of its
/// descendants, walking at most `max_depth_count` parents. The webview's HTTP requests come
/// from WebKit's network process, a child of the app; an agent that strips its
/// `ONESHIP_AGENT_INSTANCE` still does not descend from the app. Blocking `/proc` I/O.
pub fn require_app_descendant(pid: u32, app_exe: &Path, max_depth_count: usize) -> Result<(), CallerError> {
    let mut current = pid;
    for _ in 0..=max_depth_count {
        if current <= 1 {
            break;
        }
        if read_process_exe(current)?.is_some_and(|exe| exe == app_exe) {
            return Ok(());
        }
        current = read_parent_pid(current)?;
    }
    Err(CallerError::NotFromApp { pid, app_exe: app_exe.display().to_string() })
}

/// Closes registry rows whose agent process no longer exists.
///
/// A row is alive only if its registered PID still carries its own
/// `ONESHIP_AGENT_INSTANCE`; PID reuse by an unrelated process therefore cannot keep a
/// stale row open. Rows owned by another running supervisor owner (app vs. standalone
/// daemon) are left untouched. Returns the number of rows closed.
pub fn close_stale_agent_instances() -> Result<usize, CallerError> {
    let mut closed_count = 0;
    for record in db::db_list_alive_agent_instances()? {
        let still_running = matches!(
            read_instance_id_from_environ(record.pid),
            Ok(Some(ref id)) if *id == record.instance_id
        );
        if still_running {
            continue;
        }
        db::db_terminate_agent_instance(&record.instance_id, crate::supervisor::epoch_now_ms())?;
        closed_count += 1;
    }
    crate::logger::log("INFO", "caller_identity", &format!("close_stale_agent_instances: closed {} stale instances", closed_count));
    Ok(closed_count)
}

/// Resolves the managed agent behind an MCP request.
///
/// `peer` is the TCP peer address from `ConnectInfo`, `server_port` the MCP listening
/// port. Blocking; call through `spawn_blocking`.
pub fn resolve_caller(peer: SocketAddr, server_port: u16) -> Result<CallerContext, CallerError> {
    crate::logger::log("DEBUG", "caller_identity", &format!("resolve_caller: peer={}", peer));
    let inode = find_client_socket_inode(peer, server_port)?;
    let pid = find_pid_holding_socket(inode, peer)?;
    let instance_id = read_instance_id_from_environ(pid)?
        .ok_or(CallerError::UnmanagedCaller { pid })?;

    let record = db::db_get_agent_instance(&instance_id)?
        .ok_or_else(|| CallerError::InstanceUnknown { instance_id: instance_id.clone() })?;
    if record.terminated_at_ms.is_some() {
        return Err(CallerError::InstanceTerminated { instance_id });
    }

    let max_depth_count = crate::config::resolve_effective_usize(
        None, None, "supervisor", "identity_ancestor_walk_max_depth_count",
    );
    if !is_self_or_descendant(pid, record.pid, max_depth_count)? {
        return Err(CallerError::IdentityMismatch { instance_id, caller_pid: pid, registered_pid: record.pid });
    }

    let (project_path, agent_display_name) = match record.scope {
        AgentInstanceScope::System => (None, None),
        AgentInstanceScope::Project => {
            let project_id = record.project_id
                .ok_or_else(|| CallerError::DanglingReference { instance_id: instance_id.clone(), entity: "project" })?;
            let agent_uid = record.agent_uid.as_deref()
                .ok_or_else(|| CallerError::DanglingReference { instance_id: instance_id.clone(), entity: "persona" })?;
            let project = db::db_get_projects()?
                .into_iter()
                .find(|p| p.id as i64 == project_id)
                .ok_or_else(|| CallerError::DanglingReference { instance_id: instance_id.clone(), entity: "project" })?;
            let display_name = db::db_get_agent_persona_display_name(agent_uid)?
                .ok_or_else(|| CallerError::DanglingReference { instance_id: instance_id.clone(), entity: "persona" })?;
            (Some(project.path), Some(display_name))
        }
    };

    crate::logger::log("DEBUG", "caller_identity", &format!(
        "resolve_caller: peer={} -> instance={} scope={:?} pid={}", peer, instance_id, record.scope, pid
    ));
    Ok(CallerContext {
        instance_id,
        scope: record.scope,
        project_id: record.project_id,
        project_path,
        agent_uid: record.agent_uid,
        agent_display_name,
        runner: record.runner,
        pid,
    })
}
