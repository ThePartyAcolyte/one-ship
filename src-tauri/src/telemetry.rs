use serde::{Deserialize, Serialize};
use sysinfo::{Disks, Networks, Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use std::path::Path;
use std::collections::{HashMap, HashSet};
use tokio_util::sync::CancellationToken;
use tokio::sync::broadcast;
use crate::events::DaemonEvent;
use crate::supervisor::RunningRun;

/// Aggregated real-time hardware telemetry snapshot of the host machine.
/// Broadcast periodically to update topbar metrics and system status widgets.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[ts(export)]
pub struct SystemMetrics {
    /// Formatted CPU usage string (e.g. "12.4%").
    pub cpu: String,
    /// Formatted RAM usage string (e.g. "4.2 / 16.0 GB").
    pub ram: String,
    /// Formatted Swap usage string.
    pub swap: String,
    /// Formatted Disk usage string.
    pub disk: String,
    /// Formatted Network I/O rate string.
    pub net: String,
    /// Per-core CPU utilization percentages.
    pub cpu_cores: Vec<f32>,
    /// Global host CPU utilization percentage.
    pub cpu_usage_percent: f32,
    /// Physical memory usage percentage.
    pub ram_percent: f32,
    /// Physical memory currently allocated in gigabytes.
    pub ram_used_gbytes: f64,
    /// Total physical memory installed in gigabytes.
    pub ram_total_gbytes: f64,
    /// Swap space usage percentage.
    pub swap_percent: f32,
    /// Swap space currently allocated in gigabytes.
    pub swap_used_gbytes: f64,
    /// Total swap space configured in gigabytes.
    pub swap_total_gbytes: f64,
    /// Root disk usage percentage.
    pub disk_percent: f32,
    /// Disk space used in gigabytes.
    pub disk_used_gbytes: f64,
    /// Total disk capacity in gigabytes.
    pub disk_total_gbytes: f64,
    /// Network inbound transfer rate in kilobytes per second.
    pub net_rx_kbytes_per_sec: f64,
    /// Network outbound transfer rate in kilobytes per second.
    pub net_tx_kbytes_per_sec: f64,
}

/// Runtime telemetry captured for an active supervised service or background worker process tree.
#[derive(Serialize, Deserialize, Clone, Debug, Default, ts_rs::TS)]
#[ts(export)]
pub struct ServiceMetrics {
    /// Database identifier of the parent project.
    #[ts(type = "number")]
    pub project_id: i64,
    /// Identifier of the service script (e.g. "frontend", "api").
    pub service_id: String,
    /// Total CPU utilization across all descendant processes in the tree.
    pub cpu_usage_percent: f32,
    /// Formatted CPU usage string.
    pub cpu_str: String,
    /// Resident Set Size (RSS) memory consumption in bytes.
    #[ts(type = "number")]
    pub rss_memory_bytes: u64,
    /// Resident Set Size (RSS) memory consumption in megabytes.
    pub rss_memory_mbytes: f64,
    /// Formatted memory consumption string (e.g. "124.5 MB").
    pub memory_str: String,
    /// Number of active processes in the process group tree.
    #[ts(type = "number")]
    pub processes_count: usize,
    /// Name of the descendant process consuming the largest amount of memory.
    pub top_process: Option<String>,
}

fn get_telemetry_interval_ms() -> u64 {
    crate::config::resolve_effective_u64(None, None, "telemetry", "telemetry_interval_ms")
}

fn get_profiler_sample_interval_ms() -> u64 {
    crate::config::resolve_effective_u64(None, None, "profiler", "sample_interval_ms")
}

/// What the samplers read from each process. sysinfo leaves the command line out by default, and
/// the task list is needed to count threads.
fn process_refresh_kind() -> ProcessRefreshKind {
    ProcessRefreshKind::nothing()
        .with_memory()
        .with_cpu()
        .with_disk_usage()
        .with_cmd(UpdateKind::OnlyIfNotSet)
        .with_tasks()
}

/// Parent → children index of the host's processes. On Linux sysinfo also lists every thread as a
/// process whose parent is its owning process; threads are left out, so a tree holds processes only
/// and their memory is counted once.
pub(crate) struct ProcessForest {
    children: HashMap<Pid, Vec<Pid>>,
}

impl ProcessForest {
    pub(crate) fn new(sys: &System) -> Self {
        let mut children: HashMap<Pid, Vec<Pid>> = HashMap::with_capacity(sys.processes().len());
        for (pid, process) in sys.processes() {
            if process.thread_kind().is_some() {
                continue;
            }
            if let Some(parent) = process.parent() {
                children.entry(parent).or_default().push(*pid);
            }
        }
        Self { children }
    }

    /// `root` and all its descendants that are present in `sys`.
    pub(crate) fn tree(&self, root: u32, sys: &System) -> Vec<Pid> {
        let mut visited = HashSet::new();
        let mut stack = vec![Pid::from_u32(root)];
        let mut tree = Vec::new();
        while let Some(pid) = stack.pop() {
            if !visited.insert(pid) {
                continue;
            }
            if sys.process(pid).is_some() {
                tree.push(pid);
            }
            if let Some(kids) = self.children.get(&pid) {
                stack.extend(kids.iter().copied());
            }
        }
        tree
    }
}

/// Project id of every registered project path, for the runs being sampled.
fn project_ids_by_path() -> Result<HashMap<String, i64>, crate::registry::RegistryError> {
    Ok(crate::registry::list_projects()?.into_iter().map(|p| (p.path, p.id as i64)).collect())
}

/// Service tree metrics of the running services, keyed `<project_id>:<service_id>`. Blocking.
fn sample_running_services_telemetry(sys: &System, runs: &[RunningRun]) -> Result<HashMap<String, ServiceMetrics>, crate::registry::RegistryError> {
    let services: Vec<&RunningRun> = runs.iter().filter(|r| r.target_type == crate::supervisor::RunTargetType::Service).collect();
    let mut metrics_map = HashMap::with_capacity(services.len());
    if services.is_empty() {
        return Ok(metrics_map);
    }
    let project_ids = project_ids_by_path()?;
    let forest = ProcessForest::new(sys);

    for run in services {
        let Some(&project_id) = project_ids.get(&run.project_path) else {
            continue;
        };
        let mut total_cpu = 0.0f32;
        let mut total_mem_bytes = 0u64;
        let mut top_proc_name = None;
        let mut top_proc_mem_bytes = 0u64;
        let tree = forest.tree(run.pid, sys);
        for process in tree.iter().filter_map(|pid| sys.process(*pid)) {
            total_cpu += process.cpu_usage();
            let mem = process.memory();
            total_mem_bytes += mem;
            if mem > top_proc_mem_bytes {
                top_proc_mem_bytes = mem;
                top_proc_name = Some(process.name().to_string_lossy().to_string());
            }
        }
        if tree.is_empty() {
            continue;
        }
        let mem_mbytes = total_mem_bytes as f64 / 1_048_576.0;
        let memory_str = if mem_mbytes >= 1024.0 {
            format!("{:.2} GB", mem_mbytes / 1024.0)
        } else {
            format!("{:.1} MB", mem_mbytes)
        };
        metrics_map.insert(format!("{}:{}", project_id, run.target_id), ServiceMetrics {
            project_id,
            service_id: run.target_id.clone(),
            cpu_usage_percent: total_cpu,
            cpu_str: format!("{:.1}%", total_cpu),
            rss_memory_bytes: total_mem_bytes,
            rss_memory_mbytes: (mem_mbytes * 10.0).round() / 10.0,
            memory_str,
            processes_count: tree.len(),
            top_process: top_proc_name,
        });
    }

    Ok(metrics_map)
}

fn sample_host_metrics(sys: &mut System, networks: &mut Networks, disks: &mut Disks, poll_interval_ms: u64) -> SystemMetrics {
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, process_refresh_kind());
    networks.refresh(true);
    disks.refresh(true);

    let cpus = sys.cpus();
    let cpu_cores: Vec<f32> = cpus.iter().map(|c| c.cpu_usage()).collect();
    let cpu_usage_percent: f32 = if !cpus.is_empty() {
        cpu_cores.iter().sum::<f32>() / cpus.len() as f32
    } else {
        0.0
    };
    let cpu_str = format!("{:.1}%", cpu_usage_percent);

    let total_ram_gb = sys.total_memory() as f64 / 1_073_741_824.0;
    let used_ram_gb = sys.used_memory() as f64 / 1_073_741_824.0;
    let ram_percent = if total_ram_gb > 0.0 { (used_ram_gb / total_ram_gb * 100.0) as f32 } else { 0.0 };
    let ram_str = format!("{:.1}GB / {:.1}GB", used_ram_gb, total_ram_gb);

    let total_swap_gb = sys.total_swap() as f64 / 1_073_741_824.0;
    let used_swap_gb = sys.used_swap() as f64 / 1_073_741_824.0;
    let swap_percent = if total_swap_gb > 0.0 { (used_swap_gb / total_swap_gb * 100.0) as f32 } else { 0.0 };
    let swap_str = format!("{:.1}GB / {:.1}GB", used_swap_gb, total_swap_gb);

    let mut target_disk = disks.list().first();
    for disk in disks.list() {
        if disk.mount_point() == Path::new("/") {
            target_disk = Some(disk);
            break;
        }
    }
    let (disk_used_gb, disk_total_gb, disk_percent, disk_str) = if let Some(d) = target_disk {
        let total_gb = d.total_space() as f64 / 1_073_741_824.0;
        let available_gb = d.available_space() as f64 / 1_073_741_824.0;
        let used_gb = (total_gb - available_gb).max(0.0);
        let pct = if total_gb > 0.0 { (used_gb / total_gb * 100.0) as f32 } else { 0.0 };
        (used_gb, total_gb, pct, format!("{:.1}GB / {:.1}GB", used_gb, total_gb))
    } else {
        (0.0, 0.0, 0.0, "-- / --".to_string())
    };

    let mut tx_bytes = 0;
    let mut rx_bytes = 0;
    for data in networks.list().values() {
        tx_bytes += data.transmitted();
        rx_bytes += data.received();
    }

    let step_seconds = (poll_interval_ms as f64) / 1000.0;
    let tx_mbs = (tx_bytes as f64 / step_seconds) / 1_048_576.0;
    let rx_mbs = (rx_bytes as f64 / step_seconds) / 1_048_576.0;

    let format_speed = |mbs: f64| -> String {
        if mbs < 1.0 {
            format!("{:.0} KB/s", mbs * 1024.0)
        } else {
            format!("{:.1} MB/s", mbs)
        }
    };

    SystemMetrics {
        cpu: cpu_str,
        ram: ram_str,
        swap: swap_str,
        disk: disk_str,
        net: format!("↓ {} ↑ {}", format_speed(rx_mbs), format_speed(tx_mbs)),
        cpu_cores,
        cpu_usage_percent,
        ram_percent,
        ram_used_gbytes: used_ram_gb,
        ram_total_gbytes: total_ram_gb,
        swap_percent,
        swap_used_gbytes: used_swap_gb,
        swap_total_gbytes: total_swap_gb,
        disk_percent,
        disk_used_gbytes: disk_used_gb,
        disk_total_gbytes: disk_total_gb,
        net_rx_kbytes_per_sec: rx_mbs * 1024.0,
        net_tx_kbytes_per_sec: tx_mbs * 1024.0,
    }
}

/// State carried between ticks of the host telemetry loop (moved into the blocking pool per tick).
struct HostSampler {
    sys: System,
    networks: Networks,
    disks: Disks,
}

/// Spawns the background host hardware telemetry sampler loop emitting `system-telemetry` events.
pub fn spawn_telemetry_loop(
    tx: broadcast::Sender<DaemonEvent>,
    cancel: CancellationToken,
) {
    crate::logger::log("INFO", "telemetry", "Starting host hardware telemetry sampler loop");
    tauri::async_runtime::spawn(async move {
        let poll_interval_ms = get_telemetry_interval_ms();
        let initial = tokio::task::spawn_blocking(|| {
            let mut sys = System::new_all();
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            sys.refresh_processes_specifics(ProcessesToUpdate::All, true, process_refresh_kind());
            HostSampler { sys, networks: Networks::new_with_refreshed_list(), disks: Disks::new_with_refreshed_list() }
        }).await;
        let mut sampler = match initial {
            Ok(state) => Some(state),
            Err(e) => {
                crate::logger::log("ERROR", "telemetry", &format!("Host telemetry initialization panicked: {}; loop not started", e));
                return;
            }
        };

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    crate::logger::log("INFO", "telemetry", "Host hardware telemetry loop cancelled, shutting down");
                    break;
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(poll_interval_ms)) => {}
            }

            let Some(mut state) = sampler.take() else {
                break;
            };
            let runs = crate::supervisor::get_supervisor().running_runs();
            let tick = tokio::task::spawn_blocking(move || {
                let metrics = sample_host_metrics(&mut state.sys, &mut state.networks, &mut state.disks, poll_interval_ms);
                let services = sample_running_services_telemetry(&state.sys, &runs);
                (state, metrics, services)
            }).await;
            let (state, metrics, services) = match tick {
                Ok(result) => result,
                Err(e) => {
                    crate::logger::log("ERROR", "telemetry", &format!("Host telemetry tick panicked: {}; loop stopped", e));
                    break;
                }
            };
            sampler = Some(state);

            crate::logger::log("DEBUG", "telemetry", &format!("Host telemetry tick: CPU {} ({} cores, {:.1}%), RAM {}, Swap {}, Disk {}, Net {}", metrics.cpu, metrics.cpu_cores.len(), metrics.cpu_usage_percent, metrics.ram, metrics.swap, metrics.disk, metrics.net));
            let _ = tx.send(DaemonEvent::system_telemetry(&metrics));

            let services_telemetry = match services {
                Ok(m) => m,
                Err(e) => {
                    crate::logger::log("ERROR", "telemetry", &format!("Service telemetry tick skipped: {}", e));
                    continue;
                }
            };
            if !services_telemetry.is_empty() {
                crate::logger::log("DEBUG", "telemetry", &format!("Service telemetry sampled {} active service trees", services_telemetry.len()));
                let _ = tx.send(DaemonEvent::services_telemetry(&services_telemetry));
            }
        }
    });
}

/// Failure of one continuous profiler tick.
#[derive(Debug, thiserror::Error)]
enum ProfilerTickError {
    #[error(transparent)]
    Registry(#[from] crate::registry::RegistryError),
    #[error(transparent)]
    Telemetry(#[from] crate::duck::TelemetryError),
}

/// One profiler tick: the process tree of every running service and task run into DuckDB.
/// Returns the number of samples written. Blocking.
fn record_run_samples(sys: &mut System, runs: &[RunningRun]) -> Result<usize, ProfilerTickError> {
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, process_refresh_kind());
    let project_ids = project_ids_by_path()?;
    let forest = ProcessForest::new(sys);
    let sampled_at_ms = crate::supervisor::epoch_now_ms() as i64;
    let mut samples = Vec::with_capacity(runs.len() * 4);

    for run in runs {
        let Some(&project_id) = project_ids.get(&run.project_path) else {
            crate::logger::log("WARN", "telemetry", &format!("Run '{}' belongs to unregistered project '{}'; not sampled", run.run_id, run.project_path));
            continue;
        };
        for pid in forest.tree(run.pid, sys) {
            let Some(process) = sys.process(pid) else {
                continue;
            };
            let memory_bytes = process.memory();
            let disk = process.disk_usage();
            samples.push(crate::duck::ProcessSample {
                sampled_at_ms,
                project_id,
                target_type: run.target_type.as_str().to_string(),
                target_id: run.target_id.clone(),
                run_id: run.run_id.clone(),
                pid: pid.as_u32(),
                process_name: process.name().to_string_lossy().to_string(),
                cmdline: process.cmd().iter().map(|s| s.to_string_lossy()).collect::<Vec<_>>().join(" "),
                cpu_usage_percent: process.cpu_usage(),
                rss_memory_bytes: memory_bytes,
                rss_memory_mbytes: ((memory_bytes as f64 / 1_048_576.0) * 10.0).round() / 10.0,
                virtual_memory_bytes: process.virtual_memory(),
                // sysinfo lists the threads other than the main one.
                threads_count: process.tasks().map(|tasks| tasks.len() as u32 + 1),
                disk_read_bytes: disk.read_bytes,
                disk_written_bytes: disk.written_bytes,
            });
        }
    }

    crate::duck::insert_process_samples(&samples)?;
    Ok(samples.len())
}

/// The continuous profiler (Spec 25 §3.1): every `profiler.sample_interval_ms` it samples the
/// process tree of every running service and task run into DuckDB. Samples are never pruned.
pub fn spawn_continuous_profiler_loop(cancel: CancellationToken) {
    crate::logger::log("INFO", "telemetry", "Starting continuous profiler loop");
    tauri::async_runtime::spawn(async move {
        let sample_interval_ms = get_profiler_sample_interval_ms();
        let mut sys = Some(System::new());

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    crate::logger::log("INFO", "telemetry", "Continuous profiler loop cancelled, shutting down");
                    break;
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(sample_interval_ms)) => {}
            }

            let runs = crate::supervisor::get_supervisor().running_runs();
            if runs.is_empty() || !crate::duck::is_duckdb_available() {
                continue;
            }
            let Some(mut state) = sys.take() else {
                break;
            };
            let tick = tokio::task::spawn_blocking(move || {
                let written = record_run_samples(&mut state, &runs);
                (state, written)
            }).await;
            match tick {
                Ok((state, written)) => {
                    sys = Some(state);
                    match written {
                        Ok(count) => crate::logger::log("DEBUG", "telemetry", &format!("Continuous profiler tick: recorded {} process samples", count)),
                        Err(e) => crate::logger::log("ERROR", "telemetry", &format!("Continuous profiler tick failed: {}", e)),
                    }
                }
                Err(e) => {
                    crate::logger::log("ERROR", "telemetry", &format!("Continuous profiler tick panicked: {}; loop stopped", e));
                    break;
                }
            }
        }
    });
}
