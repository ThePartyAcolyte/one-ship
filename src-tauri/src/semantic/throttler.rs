//! Adaptive idle throttler governing background embedding workloads via host telemetry and supervisor state.
//!
//! Enforces memory safety gates, CPU load limits, and internal task concurrency boundaries
//! to prevent background indexing from degrading active internal tasks or host responsiveness.

use std::sync::Mutex;
use std::time::Instant;
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

/// Persistent host hardware monitor preserving CPU sampling history across evaluations.
static SYSTEM_MONITOR: Mutex<Option<(System, Instant)>> = Mutex::new(None);

fn min_background_free_memory_bytes() -> u64 {
    let mb = crate::config::resolve_effective_u64(
        None,
        None,
        "semantic",
        "throttler_min_free_mem_mbytes",
    )
    .max(64);
    mb.saturating_mul(1024 * 1024)
}

fn min_manual_free_memory_bytes() -> u64 {
    let mb = crate::config::resolve_effective_u64(
        None,
        None,
        "semantic",
        "throttler_critical_free_mem_mbytes",
    )
    .max(64);
    mb.saturating_mul(1024 * 1024)
}

fn max_background_cpu_percent() -> f32 {
    crate::config::resolve_effective_u64(None, None, "semantic", "throttler_max_cpu_percent")
        .clamp(5, 100) as f32
}

/// Evaluates whether current workstation telemetry and internal supervisor load permit background vector indexing.
pub fn can_run_background_indexing() -> bool {
    // 1. Internal ONE-SHIP Process Concurrency Gate
    // If the native Process Supervisor is executing internal tasks (builds, tests, linters), yield.
    if has_active_internal_tasks() {
        crate::logger::log(
            "INFO",
            "semantic",
            "Background indexing paused: internal ONE-SHIP task execution actively running",
        );
        return false;
    }

    let min_available_memory_bytes = min_background_free_memory_bytes();
    let max_cpu_usage_percent = max_background_cpu_percent();

    // 2. Host Telemetry Gates (RAM & CPU only, no OS process inspection)
    let mut guard = SYSTEM_MONITOR.lock().unwrap_or_else(|p| p.into_inner());
    let (sys, last_tick) = guard.get_or_insert_with(|| {
        let mut s = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::nothing().with_cpu_usage())
                .with_memory(MemoryRefreshKind::nothing().with_ram()),
        );
        s.refresh_memory();
        s.refresh_cpu_usage();
        (s, Instant::now())
    });

    sys.refresh_memory();
    if last_tick.elapsed().as_millis() >= 200 {
        sys.refresh_cpu_usage();
        *last_tick = Instant::now();
    }

    let available_ram_bytes = sys.available_memory();
    if available_ram_bytes < min_available_memory_bytes {
        crate::logger::log(
            "INFO",
            "semantic",
            &format!(
                "Background indexing throttled: available host RAM {} MB < threshold {} MB",
                available_ram_bytes / (1024 * 1024),
                min_available_memory_bytes / (1024 * 1024)
            ),
        );
        return false;
    }

    let global_cpu_percent = sys.global_cpu_usage();
    if global_cpu_percent > max_cpu_usage_percent {
        crate::logger::log(
            "INFO",
            "semantic",
            &format!(
                "Background indexing throttled: global host CPU {:.1}% > threshold {:.1}%",
                global_cpu_percent, max_cpu_usage_percent
            ),
        );
        return false;
    }

    true
}

/// Evaluates whether the system has sufficient memory safety to execute an explicit manual indexation request.
pub fn can_run_manual_indexing() -> bool {
    let min_manual_memory_bytes = min_manual_free_memory_bytes();
    let mut guard = SYSTEM_MONITOR.lock().unwrap_or_else(|p| p.into_inner());
    let (sys, _) = guard.get_or_insert_with(|| {
        let mut s = System::new_with_specifics(
            RefreshKind::nothing().with_memory(MemoryRefreshKind::nothing().with_ram()),
        );
        s.refresh_memory();
        (s, Instant::now())
    });
    sys.refresh_memory();

    let available_ram_bytes = sys.available_memory();
    if available_ram_bytes < min_manual_memory_bytes {
        crate::logger::log(
            "WARN",
            "semantic",
            &format!(
                "Manual indexing aborted: available host RAM {} MB < critical safety limit {} MB",
                available_ram_bytes / (1024 * 1024),
                min_manual_memory_bytes / (1024 * 1024)
            ),
        );
        return false;
    }

    true
}

/// Checks whether any internal tasks (builds, tests, migrations) managed by ONE-SHIP are currently executing.
fn has_active_internal_tasks() -> bool {
    let supervisor = crate::supervisor::get_supervisor();
    let runs = supervisor.running_runs();
    runs.iter().any(|r| r.target_type == crate::supervisor::RunTargetType::Task)
}
