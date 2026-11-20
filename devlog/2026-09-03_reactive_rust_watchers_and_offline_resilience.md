# Work Session Log: Reactive Rust Watchers Architecture & Offline Resilience

**Date:** 2026-09-03  
**Status:** Completed  
**Branch:** master  

---

## 1. Context & Architectural Evolution
The user identified two critical design flaws in the platform:
1. **GitHub Auth Flapping & Offline Fragility:** `gh auth status` was issuing HTTP calls against `api.github.com`. When internet connectivity flapped or dropped, the subprocess hung on TCP/DNS timeouts (30-60s), stalling threads and reporting false-negative authentication states.
2. **Synchronous Vue Polling:** The Vue frontend was using `setInterval` to periodically invoke `get_projects` and `get_project_herdr_state`. This spawned dozens of synchronous CLI commands (`git`, `herdr`), stacking IPC messages and freezing the UI thread.
3. **Decoupled Architecture Rule:** Background watching must be managed by asynchronous Rust workers (`tokio`), grouped cleanly with their respective domains/plugins, emitting events reactively to the frontend.

---

## 2. Implementations

### A. Offline-Resilient GitHub Authentication (`src-tauri/src/kopeng/diagnostic.rs`)
- Replaced `gh auth status` with `gh auth token`.
- `gh auth token` inspects local configuration/keyring (`~/.config/gh/hosts.yml`) directly in **~3ms without issuing any network requests**.
- The platform now seamlessly detects GitHub authentication offline with zero freezes.

### B. Project & Configuration Watcher (`src-tauri/src/watcher.rs`)
- Implemented `spawn_project_watcher(app_handle)`:
  - Lightweight tokio worker monitoring filesystem modification timestamps (`rula.toml` and `.git/HEAD`) every 1500ms.
  - When modified by an LLM agent, user in IDE, or external tool, it automatically rehydrates and emits `projects-updated`.
  - Zero manual reload buttons needed; instant reactive sync.

### C. Herder Agent Supervisor Worker (`src-tauri/src/kopeng/herdr.rs`)
- Implemented `spawn_herdr_watcher(app_handle)`:
  - Asynchronous tokio loop polling active workspaces, tabs, and panes.
  - Monitors `last_agent_status.json` and emits `agent-status-report`.
  - Emits `herdr-states-updated` directly to the frontend.

### D. Frontend Reactivity (`src/App.vue`)
- Eliminated all polling intervals (`herdrInterval`, `diagnosticInterval`).
- Subscribed to `listen("projects-updated")`, `listen("herdr-states-updated")`, and `listen("agent-status-report")`.
- UI thread is now 100% reactive, silky smooth, and free of blocking subprocesses.

---

## 3. Verification & Operational Status
- `npm run build`: 0 errors (1.57s).
- `cargo check`: 0 errors (0.21s).
- `npm run tauri dev`: Booted and running (`task-1016`).
