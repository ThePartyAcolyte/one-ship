//! Process-wide microphone capture lease shared by the Global Voice Controller and native
//! dictation. `pw-record` is an exclusive resource from the user's point of view: two captures
//! at once produce two competing transcripts, so the second owner receives an explicit error.

use std::sync::{Mutex, PoisonError};

/// Error returned when another subsystem already owns the microphone.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("Microphone is already in use by {owner}")]
    Busy { owner: &'static str },
}

/// RAII lease over the microphone. Dropping it releases the owner automatically on every exit
/// path (normal end, error, cancellation or panic).
#[derive(Debug)]
pub struct CaptureLease {
    owner: &'static str,
}

static CAPTURE_OWNER: Mutex<Option<&'static str>> = Mutex::new(None);

/// Acquires the microphone for `owner`, or reports the current owner.
pub fn try_acquire(owner: &'static str) -> Result<CaptureLease, CaptureError> {
    let mut current = CAPTURE_OWNER
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some(existing) = *current {
        return Err(CaptureError::Busy { owner: existing });
    }
    *current = Some(owner);
    Ok(CaptureLease { owner })
}

impl Drop for CaptureLease {
    fn drop(&mut self) {
        let mut current = CAPTURE_OWNER
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if *current == Some(self.owner) {
            *current = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lease_is_exclusive_and_released_on_drop() {
        let first = try_acquire("test-a").expect("first lease");
        let err = try_acquire("test-b").expect_err("second lease must be rejected");
        assert!(matches!(err, CaptureError::Busy { owner: "test-a" }));
        drop(first);
        let second = try_acquire("test-b").expect("lease after release");
        drop(second);
    }
}
