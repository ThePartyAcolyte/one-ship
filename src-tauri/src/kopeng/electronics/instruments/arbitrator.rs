use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use crate::kopeng::electronics::errors::ElectronicsError;

static OSCILLOSCOPE_LOCK_EPOCH_MS: AtomicU64 = AtomicU64::new(0);
static LOGIC_ANALYZER_LOCK_EPOCH_MS: AtomicU64 = AtomicU64::new(0);

const DEFAULT_LOCK_TIMEOUT_MS: u64 = 5000;

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Arbitrates exclusive hardware access to USB test instruments.
pub struct HardwareLockArbitrator;

impl HardwareLockArbitrator {
    /// Attempts to acquire an exclusive lock on the oscilloscope.
    pub fn acquire_oscilloscope_lock() -> Result<HardwareLockGuard, ElectronicsError> {
        Self::acquire_lock(&OSCILLOSCOPE_LOCK_EPOCH_MS, "Oscilloscope (Owon VDS1022)")
    }

    /// Attempts to acquire an exclusive lock on the logic analyzer.
    pub fn acquire_logic_analyzer_lock() -> Result<HardwareLockGuard, ElectronicsError> {
        Self::acquire_lock(&LOGIC_ANALYZER_LOCK_EPOCH_MS, "Logic Analyzer (24MHz 8-Ch)")
    }

    fn acquire_lock(lock: &'static AtomicU64, device_name: &'static str) -> Result<HardwareLockGuard, ElectronicsError> {
        let now = now_epoch_ms().max(1);
        for _ in 0..16 {
            let prev = lock.load(Ordering::SeqCst);
            if prev > 0 && now.saturating_sub(prev) < DEFAULT_LOCK_TIMEOUT_MS {
                return Err(ElectronicsError::SerialError {
                    port: device_name.to_string(),
                    details: format!(
                        "Device is busy; another acquisition or agent task is currently in progress (active lease expires in {}ms)",
                        DEFAULT_LOCK_TIMEOUT_MS.saturating_sub(now.saturating_sub(prev))
                    ),
                });
            }
            if lock.compare_exchange(prev, now, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                return Ok(HardwareLockGuard { lock });
            }
        }
        Err(ElectronicsError::SerialError {
            port: device_name.to_string(),
            details: "Device lock contention; retry acquisition".to_string(),
        })
    }
}

/// RAII guard that releases the exclusive USB instrument lock on drop.
pub struct HardwareLockGuard {
    lock: &'static AtomicU64,
}

impl Drop for HardwareLockGuard {
    fn drop(&mut self) {
        self.lock.store(0, Ordering::SeqCst);
    }
}
