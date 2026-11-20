//! Bounded persistent disk cache for expensive headless CAD/PCB previews and evaluations.
//!
//! Stores serialized payloads in `~/.cache/one-ship/previews/<namespace>/<sha256>.json`
//! keyed by canonical source path and render parameters, validated against source `modified_ms`.
//! Enforces strict size and entry count caps (Tiger Style) using async I/O only.

use std::path::{Path, PathBuf};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MAX_DISK_CACHE_ENTRY_BYTES: usize = 24 * 1024 * 1024;
const MAX_DISK_CACHE_FILES_COUNT: usize = 32;

#[derive(Serialize, Deserialize)]
struct DiskCacheEnvelope<T> {
    modified_ms: u128,
    payload: T,
}

fn cache_namespace_dir(namespace: &str) -> Option<PathBuf> {
    let base = dirs::cache_dir().or_else(|| dirs::home_dir().map(|h| h.join(".cache")))?;
    Some(base.join("one-ship").join("previews").join(namespace))
}

fn cache_entry_path(namespace: &str, key: &str) -> Option<PathBuf> {
    let dir = cache_namespace_dir(namespace)?;
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{:02x}", b);
    }
    Some(dir.join(format!("{}.json", hex)))
}

fn prune_oldest_entries_sync(dir: &Path, max_files_count: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let modified = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .unwrap_or(std::time::UNIX_EPOCH);
        files.push((path, modified));
    }
    if files.len() <= max_files_count {
        return;
    }
    files.sort_by_key(|(_, mtime)| *mtime);
    let remove_count = files.len() - max_files_count;
    for (path, _) in files.into_iter().take(remove_count) {
        let _ = std::fs::remove_file(path);
    }
}

/// Reads a cached payload from disk if it exists, fits within size bounds, and matches `expected_modified_ms`.
pub async fn read_cached<T: DeserializeOwned>(
    namespace: &str,
    key: &str,
    expected_modified_ms: u128,
) -> Option<T> {
    if expected_modified_ms == 0 {
        return None;
    }
    let path = cache_entry_path(namespace, key)?;
    let meta = tokio::fs::metadata(&path).await.ok()?;
    if meta.len() as usize > MAX_DISK_CACHE_ENTRY_BYTES {
        let _ = tokio::fs::remove_file(&path).await;
        return None;
    }
    let bytes = tokio::fs::read(&path).await.ok()?;
    let env: DiskCacheEnvelope<T> = serde_json::from_slice(&bytes).ok()?;
    if env.modified_ms != expected_modified_ms {
        return None;
    }
    Some(env.payload)
}

/// Persists a payload to disk asynchronously with bounded entry size and LRU/mtime directory pruning.
pub async fn write_cached<T: Serialize>(
    namespace: &str,
    key: &str,
    modified_ms: u128,
    payload: &T,
) {
    if modified_ms == 0 {
        return;
    }
    let Some(dir) = cache_namespace_dir(namespace) else {
        return;
    };
    let Some(path) = cache_entry_path(namespace, key) else {
        return;
    };
    let env = DiskCacheEnvelope { modified_ms, payload };
    let Ok(bytes) = serde_json::to_vec(&env) else {
        return;
    };
    if bytes.len() > MAX_DISK_CACHE_ENTRY_BYTES {
        return;
    }
    if tokio::fs::create_dir_all(&dir).await.is_err() {
        return;
    }
    if tokio::fs::write(&path, bytes).await.is_ok() {
        let _ = tokio::task::spawn_blocking(move || {
            prune_oldest_entries_sync(&dir, MAX_DISK_CACHE_FILES_COUNT);
        })
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_hashes_to_deterministic_hex_path() {
        let p1 = cache_entry_path("test", "a|iso|1280").unwrap();
        let p2 = cache_entry_path("test", "a|iso|1280").unwrap();
        let p3 = cache_entry_path("test", "a|top|1280").unwrap();
        assert_eq!(p1, p2);
        assert_ne!(p1, p3);
        assert!(p1.to_string_lossy().ends_with(".json"));
    }

    #[test]
    fn prune_oldest_entries_keeps_bounded_count() {
        let dir = tempfile::tempdir().unwrap();
        for idx in 0..5 {
            std::fs::write(dir.path().join(format!("item_{}.json", idx)), b"{}").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        prune_oldest_entries_sync(dir.path(), 3);
        let remaining = std::fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(remaining, 3);
    }
}
