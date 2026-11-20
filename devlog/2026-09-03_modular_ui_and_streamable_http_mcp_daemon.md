# Work Session Log: Vue Modularization, Streamable HTTP MCP Daemon & Settings Persistence

**Date:** 2026-09-03  
**Status:** Completed  
**Branch:** master  

---

## 1. Objectives & Context
During this session, three major architectural initiatives were executed:
1. **Monolith Modularization (Phase 5):** Decompose the monolithic `src/App.vue` (~1,820 lines) into dedicated domain components under `src/components/`.
2. **Streamable HTTP MCP Daemon (Phase 6):** Eliminate brittle `stdio` child processes, adopting a persistent, async Streamable HTTP daemon on `http://127.0.0.1:8765/mcp` inspired by `odoo-mcp`, enforcing a strict **Zero Backward Compatibility & Zero Dead Code Policy**.
3. **Settings Persistence & Scoped Schema Reorganization:** Resolve bidirectional configuration loading bugs in `rula.toml`, decouple system settings from process orchestration, and expose dynamic port configuration.

---

## 2. Key Accomplishments

### A. Vue Monolith Modularization & Tailwind CSS v4 Fix
- **Extracted 13 Domain Components:**
  - `src/components/AppHeader.vue`: Top telemetry metrics and locale switcher.
  - `src/components/ProjectCard/`:
    - `ProjectCard.vue`: Root container assembling header, herder, and services.
    - `CardHeader.vue`: Project title, dual-signification git status, action buttons, and contextual dropdown.
    - `HerderSection.vue`: Workspace ID, status badges, and interactive agent pills.
    - `ServicesSection.vue`: Brutalist service fleet cards and process controllers.
  - `src/components/modals/`:
    - `SettingsModal.vue`: Master-detail scoped configuration editor with provenance tags.
    - `VcsActionModal.vue`: Agentic VCS instruction dispatcher with `rula.toml` preset manager.
    - `GithubRemoteModal.vue`: Interactive remote search, repo creation, and linking dialog.
    - `ConfirmDeleteModal.vue`: Project purge confirmation dialog.
  - `src/components/floating/`:
    - `DiagnosticBubble.vue`: Checklist popup and warning trigger.
    - `VoiceFab.vue`: OpenDictate mode switch.
    - `RollingStatusLog.vue`: 5-line terminal log feed.
    - `SettingsFab.vue`: Global settings trigger.
- **Tailwind CSS v4 Fix:** Restored `@import "tailwindcss";` in `src/App.vue`, recovering the full CSS stylesheet (bundle expanded from 0.26 kB to 27.35 kB).
- Reduced `src/App.vue` from 1,818 lines to 607 lines.

### B. Streamable HTTP MCP Daemon & Governance Enforcement
- **Governance Update:** Formally added the **Zero Backward Compatibility & Zero Dead Code Policy** to `AGENTS.md`.
- **Pure Async Daemon (`src-tauri/src/mcp.rs`):**
  - Integrated `axum` and `tower-http`.
  - Bound to `http://127.0.0.1:<port>/mcp` via `tauri::async_runtime::spawn` in `src-tauri/src/lib.rs`.
  - Implemented headless execution via `wang-kapawu --mcp-daemon`.
  - Completely removed the legacy synchronous `stdin.lines()` loop.
- **Tool Partitioning:**
  - Meta-tools: `list_available_workflows`, `activate_workflows`.
  - Core tools: `report_task_status`, `query_workspace_state`.
  - Domain tools: `delegate_agent_task`.
- **Client Migration:** Pointed Antigravity's `~/.gemini/config/mcp_config.json` directly to `serverUrl: "http://localhost:8765/mcp"`.
- **Closed-Loop Callback Verification:** Documented and verified `delegations.json` correlation ledger ensuring target agents prompt originator agents upon completion.

### C. Settings Persistence & VCS UI Polish
- **Bidirectional Scope Bug Fix:** Updated `resolve_effective_field` in `src-tauri/src/config.rs` to read from both `rula.get("plugins")` and root `rula.get("system")` / `rula.get(plugin_id)`. Changes to `auto_approve` and other flags in `rula.toml` now persist and reflect immediately on reopen with `[project]` provenance.
- **Schema Reorganization:**
  - Partitioned `system` (`mcp_port`) and `orchestrator` (`agent_runner`, `auto_approve`).
  - Dynamic port binding in `mcp::start_http_daemon()` resolving `system.mcp_port` from configuration.
- **VCS Action Button Styling:** Removed `border-transparent opacity-50` when git status is clean; added a 1px solid border matching the 3-dots menu button for persistent interactive affordance.

---

## 3. Verification & Metrics
- `cargo check --manifest-path src-tauri/Cargo.toml`: 0 warnings, 0 errors.
- `npm run build`: Bundled in 4.61s with 0 errors.
- Live `curl` JSON-RPC test: Received complete tool catalog from local port 8765.
