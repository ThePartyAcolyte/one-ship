# System Logging & Status Terminal

## Purpose

Define ONE-SHIP's bounded cyclic file logger, the SQLite `system_logs` sink and its retention, and the rolling status terminal that replaces intrusive toasts.

## Scope

In scope: `oneship.log` rotation and format, log levels, logger settings, the `system_logs` queue/worker/pruning, client log entry point, the 5-line status terminal.

Out of scope: process supervision ([Project Registry, Scripts & Service Orchestration](01-project-registry-and-service-orchestration.md)).

## Model / Design

### 1. Cyclic Rotating File Logger (`oneship.log`)

Prevents unbounded log accumulation while letting AI coding assistants diagnose errors quickly.

- **Location:** `~/.config/one-ship/logs/oneship.log`.
- **Cap size:** `system.log_max_size_bytes` (default 512000). The file handle stays open and rotation is decided from the tracked written size, not a `stat` per line.
- **Rotations:** `system.log_max_rotations_count` generational backups (default 2: `oneship.1.log`, `oneship.2.log`).
- **Minimum level:** `system.log_level` (default `INFO`) gates both this file and the `system_logs` table; entries below it are dropped before any I/O. `DEBUG` gives exhaustive diagnostics.
- **Settings cache:** logger settings are re-resolved only when the `config.toml` mtime changes.
- **Format** `[timestamp_seconds] [LEVEL] [module] [origin] message`:

  ```text
  [1772654400] [INFO] [vcs] [one-ship] Dispatched instruction for 'one-ship': "Auto-Commit"
  [1772654410] [ERROR] [supervisor] [system] Failed to spawn agent process: command not found
  ```
- **Levels:** `DEBUG`, `INFO`, `WARN`, `ERROR`. `write_client_log` rejects any other level. `logger::log_with_origin` tags entries with a project or subsystem origin (`system_logs.origin`), defaulting to `"system"`.

### 2. SQLite `system_logs` Sink & Retention

- Entries at or above `system.log_level` are queued on a bounded channel (4096) and committed by one worker thread in transactions of up to 64 rows.
- A full channel never blocks the caller: the entry is still written to `oneship.log` but not queued for SQLite. Dropped entries are counted; after each batch the worker writes `WARN ... N entries dropped from system_logs` to `oneship.log` and resets the count. If the worker thread is gone, each entry writes an `ERROR` line to the file instead. A failed batch insert or prune is reported in the file too, never through the channel.
- The worker prunes `system_logs` to the newest `system.log_db_max_rows_count` rows (default 50000) every `system.log_db_prune_interval_seconds` (default 300); `init_database` prunes once at startup.
- The database runs with `auto_vacuum = INCREMENTAL`; each prune is followed by `PRAGMA incremental_vacuum`. Databases created before this setting are rebuilt once with `VACUUM` at startup, after the prune. Rationale (audit F18): a 3.9 GB file with ~2.6 GB of free pages caused by 5.6 M DEBUG rows in 32 h.

### 3. Rolling Status Terminal

In place of floating modal alerts or toasts:

- **Location:** fixed bottom-left overlay (`fixed bottom-4 left-4`).
- **Capacity:** 5-line FIFO buffer.
- **Behavior:** older messages roll off silently as new events arrive (`[VOICE] armed`, `[VCS] dispatched`, `[AGENT] agent working`).

## Contracts

| Item | Definition |
| :--- | :--- |
| `write_client_log(level, module, message)` | Frontend entry point; unknown level → `LoggerError::UnknownLevel`. |
| `get_system_logs` | Cursor paging (`before_id` / `after_id`) with module/level filtering. |
| `get_system_log_sources` | Distinct source discovery. |
| Config keys | `system.log_level` (`INFO`), `system.log_max_size_bytes` (512000), `system.log_max_rotations_count` (2), `system.log_db_max_rows_count` (50000), `system.log_db_prune_interval_seconds` (300). |
| Channel / batch constants | Queue 4096; batch ≤ 64 rows. |

## Implementation Status

| Area | Status |
| :--- | :--- |
| File logger, DB sink, pruning, incremental vacuum (`logger.rs`, `db.rs`, `config.rs`) | Implemented |
| 5-line status terminal | Implemented per legacy spec; not re-verified |

## Related Specs

- [Project Registry, Scripts & Service Orchestration](01-project-registry-and-service-orchestration.md)
- [Git Workspace, Branches & Snapshots Cockpit](02-git-workspace-and-branches.md)
- [Project Category Index](README.md)

Supersedes legacy: `09_vcs_and_logging_architecture.md` (logging and status terminal portions)
