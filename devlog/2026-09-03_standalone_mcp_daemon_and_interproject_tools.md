# Devlog: Standalone MCP Subprocess Daemon & Inter-Project Delegation

**Date:** 2026-09-03  
**Author:** bosmang (Lead Systems Architect)  
**Status:** Completed & Validated

## 1. Objective
Decouple the Wang-Kapawu Model Context Protocol (MCP) server from the Tauri GUI application lifecycle, ensuring 100% continuous uptime across frontend and backend reloads. Expand the MCP toolset with inter-project discovery, persona inspection, and lazy auto-bootstrapping for unlaunched agents.

## 2. Architectural Changes
1. **Dedicated Binary Target (`kapawu-mcp`)**:
   - Configured `src-tauri/Cargo.toml` with separate binary targets for `wang-kapawu` (GUI) and `kapawu-mcp` (headless daemon).
   - Created entry point in `src-tauri/src/bin/mcp.rs` that initializes SQLite WAL database schema and starts the Tokio/Axum HTTP daemon.
   - Added `"mcp"` script to `package.json`.
2. **Graceful Port Yielding**:
   - Modified `start_http_daemon()` in `src-tauri/src/mcp.rs`: if port 8765 is already bound by the standalone daemon, the GUI's in-process daemon logs an info message and yields without error.
3. **Inter-Project MCP Tools**:
   - `list_projects`: Queries SQLite repository and returns all registered projects, workspace IDs, and agent personas with statuses.
   - `query_project_agents`: Introspects agent personas, configurations, and running statuses for a specific project by name or path.
4. **Auto-Bootstrapping Unlaunched Agents**:
   - Enhanced `execute_delegate_agent_task`: If the target agent is not active in Herder, it searches registered projects to locate the persona, launches it via `launch_herdr_agent()`, waits for initialization, and dispatches the prompt.

## 3. Verification & Results
- Frontend build: `npm run build` completed with 0 errors.
- Backend compilation: `cargo check` and `cargo build --bin kapawu-mcp` completed with 0 errors.
- Live MCP verification: Tested `list_projects` and `query_project_agents` over HTTP JSON-RPC on `http://localhost:8765/mcp`, confirming multi-project state introspection.
