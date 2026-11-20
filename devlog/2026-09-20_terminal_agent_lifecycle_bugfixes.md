# 2026-09-20: Terminal & Agent Lifecycle Bug Fixes

## Context & Objectives
Code audit session identified three confirmed bugs in `supervisor.rs` affecting:
1. Services and task processes leaking into the agent list (Chat Overlay confusion).
2. System/git agent operating in the wrong working directory (parent of project instead of project root).
3. Redundant `cmd.current_dir()` call in `open_system_terminal`.

## Root Cause Analysis

### Bug 1 — Dynamic Discovery contaminating the agent list
`compute_project_state()` Step 5 iterated all `ManagedProcess` entries keyed under the
project prefix (`<project_path>:*`) and injected any unrecognized process as an `AgentState`.
This included `svc:shell`, `svc:<service_id>`, and `tsk:<task_id>:<exec_id>` — Terminal Hub
processes that should never appear in the agent list or trigger the Chat Overlay.

Symptom: interactive shell appeared as an "agent" in the Overview, and clicking it opened the
Chat Overlay instead of the Terminal Hub. `/sbin/nologin` appearing in the agent list when
`$SHELL` resolved to an unexpected binary was also caused by this.

### Bug 2 — resolve_system_working_dir returning parent of project
When exactly one project was registered, the function returned `p.parent()` (one level above
the project path). With `project = ~/Projects/dev/wang-kapawu`, the resolved dir was
`~/Projects/dev`. The git agent spawned via `launch_system_agent_cmd()` / `dispatch_git_instruction`
operated in this parent directory, causing it to `ls` the workspace root instead of the project.
The agent session never completed meaningfully.

### Bug 3 — Duplicate current_dir in open_system_terminal
`cmd.current_dir(p)` was called once inside the `_` match arm (correct) and again unconditionally
after the match (redundant). For named terminals (ghostty, kitty, etc.) the `--working-directory`
flag is the canonical mechanism; the extra `current_dir` was harmless but incorrect dead code.

## Changes Applied

### `src-tauri/src/supervisor.rs`

**Fix 1 — Dynamic Discovery guard (`compute_project_state`, Step 5):**
Added guard before injecting discovered processes as agents:
```rust
if agent_id.starts_with("svc:") || agent_id.starts_with("tsk:") {
    continue;
}
```
Services (`svc:shell`, `svc:dev-server`) and task executions (`tsk:build:42`) are now strictly
excluded from the agent list. They remain fully managed by the Terminal Hub (spawn_service /
spawn_task) and their output is accessible via `read_terminal_output`.

**Fix 2 — resolve_system_working_dir uses project path directly:**
Removed the `p.parent()` branch. The function now returns `p.to_path_buf()` (the project path
itself) when a single project is registered. This ensures git agent and system agent always start
with `cwd = project_path`.

**Fix 3 — Removed redundant `cmd.current_dir(p)` after match block in `open_system_terminal`:**
Named terminals use their CLI flags for working directory. The `current_dir` inside the `_` arm
handles unknown terminals. The post-match call was removed.

**Fix 4 — Multi-shell backend parameter in `launch_interactive_shell`:**
`launch_interactive_shell` now accepts an optional `shell_id: Option<String>`. If provided,
it spawns the service with that specific key (e.g. `shell_<timestamp>`), allowing N independent
named shell instances per project without collisions.

### `src/components/workspace/ProjectTerminalGrid.vue`

**Fix 5 — Multi-shell Registry & UI:**
Replaced the single hardcoded `shell` target with a dynamic `shellSessions` list.
Users can spawn N interactive shells (`Shell 1`, `Shell 2`, ...), switch between them,
and terminate individual sessions cleanly with a close action.

**Fix 6 — Reactive Event Subscription for Live Output (Bug #7):**
Integrated Tauri `listen` for `task-output-chunk` and `agent-step-update` events, appending
live stdout/stderr to the output buffer in real-time. Polling interval was increased from
1500ms to 3000ms as a lightweight heartbeat fallback.

## Verification
- `cargo check --manifest-path src-tauri/Cargo.toml` → Exit code 0.
- `npx vue-tsc --noEmit` → Exit code 0.

## Impact
- Shell sessions no longer appear in the agent list or open the Chat Overlay.
- Terminal Hub supports N independent interactive shell tabs.
- Real-time terminal output streaming without polling lag.
- Git agent now operates inside the project directory when dispatching instructions.
- `open_system_terminal` cleanup.
