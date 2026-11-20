# Work Session Log: Strict Working Directory Enforcement for Herder Workspaces and Agent Panes

**Date:** 2026-09-03  
**Status:** Completed  
**Branch:** master  

---

## 1. Objectives & Context
Resolve an issue where launching an agent in a non-primary project workspace (e.g., `odoo-mcp`) launched the agent inside the daemon's working directory (`wang-kapawu`) instead of the target project directory.

---

## 2. Key Accomplishments

### A. Tri-Layer Working Directory Enforcement (`src-tauri/src/kopeng/herdr.rs`)
1. **Workspace Level:** Added `--cwd <project_path>` to `herdr workspace create`.
2. **Tab Level:** Added `--cwd <project_path>` to `herdr tab create` in `launch_herdr_agent`.
3. **Active Shell Level:** Injected an explicit `herdr pane run <pane_id> "cd '<project_path>'"` command immediately prior to `herdr agent start`. This ensures that even reused initial tabs (`t1`) are strictly shifted to the target project directory before the agent binary (`agy`, `claude`, etc.) takes over the pane.

---

## 3. Verification
- Backend check (`cargo check`) succeeded with 0 errors.
- Dev server (`task-75`) recompiled and is running live.
