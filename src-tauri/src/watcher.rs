use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tokio_util::sync::CancellationToken;
use tokio::sync::broadcast;
use crate::events::DaemonEvent;

fn get_watcher_debounce_ms() -> u64 {
    crate::config::resolve_effective_u64(None, None, "supervisor", "watcher_debounce_ms")
}

fn get_keyring_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("keyrings").join("Default_keyring.keyring"))
}

/// Registers the account currently in the OS keyring and broadcasts the account list.
///
/// Blocking keyring and SQLite work runs on the blocking pool. `new-account-detected` is
/// emitted only when `announce_new` is set (keyring changes, not the startup sync).
async fn sync_keyring_account(tx: &broadcast::Sender<DaemonEvent>, announce_new: bool) {
    let synced = tokio::task::spawn_blocking(|| {
        let registered = crate::kopeng::accounts::check_and_register_keyring_account()?;
        let summaries = crate::db::db_get_account_summaries()?;
        Ok::<_, crate::kopeng::accounts::AccountsError>(registered.map(|r| (r, summaries)))
    })
    .await;
    let ((summary, is_new), summaries) = match synced {
        Ok(Ok(Some(found))) => found,
        Ok(Ok(None)) => return,
        Ok(Err(e)) => {
            crate::logger::log("ERROR", "accounts", &format!("Keyring account sync failed: {}", e));
            return;
        }
        Err(e) => {
            crate::logger::log("ERROR", "accounts", &format!("Keyring account sync task failed: {}", e));
            return;
        }
    };
    crate::logger::log("INFO", "accounts", &format!("Keyring account synced: {}, is_new: {}", summary.email, is_new));
    if announce_new && is_new {
        let _ = tx.send(DaemonEvent::new_account_detected(&summary));
    }
    let _ = tx.send(DaemonEvent::accounts_updated(&summaries));
}

/// Polls registered projects for `rula.toml` / `.git/HEAD` changes and the keyring for account
/// changes, emitting `projects-updated` / `accounts-updated` on the daemon event bus.
///
/// Each tick reads only the project registry and file mtimes. Full hydration (`git status`,
/// rula parsing, skill status) runs on the blocking pool and only when a change was detected.
pub fn spawn_project_watcher(
    tx: broadcast::Sender<DaemonEvent>,
    cancel: CancellationToken,
) {
    crate::logger::log("INFO", "watcher", "Starting filesystem project and keyring watcher loop");
    tauri::async_runtime::spawn(async move {
        let mut known_mtimes: HashMap<usize, (Option<SystemTime>, Option<SystemTime>)> = HashMap::new();
        let mut known_project_ids: Vec<usize> = Vec::new();
        let mut known_keyring_mtime: Option<SystemTime> = None;

        sync_keyring_account(&tx, false).await;

        loop {
            let debounce_ms = get_watcher_debounce_ms();
            tokio::select! {
                _ = cancel.cancelled() => {
                    crate::logger::log("INFO", "watcher", "Project watcher loop cancelled, shutting down");
                    break;
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(debounce_ms)) => {}
            }

            if let Some(keyring_path) = get_keyring_path() {
                let current_keyring_mtime = tokio::fs::metadata(&keyring_path).await.ok().and_then(|m| m.modified().ok());
                if current_keyring_mtime != known_keyring_mtime {
                    known_keyring_mtime = current_keyring_mtime;
                    crate::logger::log("INFO", "watcher", "Keyring modification detected");
                    sync_keyring_account(&tx, true).await;
                }
            }

            let registry = match tokio::task::spawn_blocking(crate::db::db_get_projects).await {
                Ok(Ok(projects)) => projects,
                Ok(Err(e)) => {
                    crate::logger::log("ERROR", "watcher", &format!("Project watcher tick skipped: {}", e));
                    continue;
                }
                Err(e) => {
                    crate::logger::log("ERROR", "watcher", &format!("Project watcher registry task failed: {}", e));
                    continue;
                }
            };
            let current_ids: Vec<usize> = registry.iter().map(|p| p.id).collect();
            crate::logger::log("DEBUG", "watcher", &format!("Project watcher tick: inspecting {} registered projects", registry.len()));

            let mut should_emit = false;
            if current_ids != known_project_ids {
                crate::logger::log("INFO", "watcher", &format!("Project registry membership change detected ({} -> {} projects). Emitting update.", known_project_ids.len(), current_ids.len()));
                should_emit = true;
                known_mtimes.retain(|id, _| current_ids.contains(id));
                known_project_ids = current_ids;
            }

            for p in &registry {
                let p_path = Path::new(&p.path);
                let rula_mtime = tokio::fs::metadata(p_path.join("rula.toml")).await.ok().and_then(|m| m.modified().ok());
                let git_mtime = tokio::fs::metadata(p_path.join(".git").join("HEAD")).await.ok().and_then(|m| m.modified().ok());
                let current = (rula_mtime, git_mtime);
                let previous = known_mtimes.insert(p.id, current);
                if previous.is_some_and(|prev| prev != current) {
                    crate::logger::log("INFO", "watcher", &format!("Filesystem change detected in project '{}' (id: {}). Emitting projects-updated.", p.name, p.id));
                    should_emit = true;
                }
                // Validate rula.toml when the project is first seen and whenever it changes.
                if previous.is_none_or(|prev| prev.0 != rula_mtime) {
                    let (project_id, project_path) = (p.id as i64, p_path.to_path_buf());
                    match tokio::task::spawn_blocking(move || crate::config::sync_project_config_flag(project_id, &project_path)).await {
                        Ok(result) => crate::logger::log_failure("watcher", "sync invalid_config flag", result),
                        Err(e) => crate::logger::log("ERROR", "watcher", &format!("Config validation task failed: {}", e)),
                    }
                }
            }

            if !should_emit {
                continue;
            }
            match tokio::task::spawn_blocking(crate::registry::get_projects).await {
                Ok(Ok(hydrated)) => {
                    crate::logger::log("DEBUG", "watcher", &format!("Emitting projects-updated event for {} projects", hydrated.len()));
                    let _ = tx.send(DaemonEvent::projects_updated(&hydrated));
                }
                Ok(Err(e)) => crate::logger::log("ERROR", "watcher", &format!("Cannot load projects for projects-updated: {}", e)),
                Err(e) => crate::logger::log("ERROR", "watcher", &format!("Project hydration task failed: {}", e)),
            }
        }
    });
}
