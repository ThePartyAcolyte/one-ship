# Devlog: Herder Agent Execution Engine (`kopeng::herdr`)
**Date:** 2026-09-02  
**Topic:** Herder Workspace Integration, Dedicated `git` Pane Lifecycle, Autonomous Execution (`--dangerously-skip-permissions`), and System Prompt Injection

## Overview
Implemented the backend bridge between Kapawu and the local running Herder daemon (`herdr 0.8.2` on `/home/butcherwutcher/.config/herdr/herdr.sock`). The engine maps projects to their respective Herder workspaces, manages a dedicated `"git"` tab/pane, bootstraps `agy` autonomously with `--dangerously-skip-permissions`, and conditionally injects the Git specialist system prompt.

## Key Changes
1. **Backend Module (`src-tauri/src/kopeng/herdr.rs`)**:
   - `get_project_herdr_state`: Scans `herdr pane list` and `herdr workspace list` to identify workspace linkage and searches for a tab labeled `"git"`.
   - `dispatch_git_instruction`:
     - Creates workspace via `herdr workspace create` if unmapped.
     - Creates dedicated tab via `herdr tab create --label git --no-focus` if absent.
     - Bootstraps agent with `herdr agent start git --kind agy --pane <pane_id> -- --dangerously-skip-permissions`.
     - Injects specialized Git System Prompt only on fresh initialization, sending raw instruction on subsequent runs.
     - Dispatches prompt via `herdr agent prompt <pane_id> "<payload>"`.
2. **Frontend UI Integration (`src/App.vue`)**:
   - Wired Project Card toolbar button `<BrainCircuit />` to live Herder status (working = pulsing accent green, idle = sky blue, linked/none = dimmed).
   - Hooked up both the VCS modal `[ Dispatch ]` button and the split-button instant dispatch (`»`) to `dispatch_git_instruction`.
   - Connected OpenDictate voice capture completions to automatically dispatch voice instructions to the dedicated Git agent.
