# 2026-09-20: Project Scripts & Terminal Hub Refactor

## Context & Objectives
The workspace Terminals tab previously aggregated agent processes alongside system processes in a read-only polling grid. Because agent terminals are already natively visualised within the conversational chat overlay and artifact viewer, having them in the workspace terminal grid created redundant noise. Furthermore, the previous terminal viewer was purely unidirectional (no standard input capabilities) and lacked ANSI colour sequence rendering.

This session refactored the workspace terminal interface into a dedicated **Project Scripts & Terminal Hub** (Option 3):
1. Separated execution contexts by dedicating the workspace section exclusively to managing project scripts (persistent services, deterministic tasks, and manual interactive sessions).
2. Added bidirectional communication capabilities to send interactive inputs and commands via `send_terminal_input`.
3. Created an interactive shell launcher (`launch_interactive_shell`) and a native host terminal launcher (`open_system_terminal`) respecting the user's `preferred_terminal` (e.g. Ghostty) configured in global settings.
4. Built a high-performance, robust ANSI-to-HTML parser (`src/utils/ansi.ts`) supporting 16-colour, bright, and 256-colour palettes mapped cleanly to brutalist theme variables.

## Architectural Changes

### 1. Backend Extensions (`src-tauri/src/supervisor.rs` & `src-tauri/src/lib.rs`)
- **Suffix matching in `send_input`:** Updated `ProcessSupervisor::send_input` to resolve target processes both by exact key (`${project_path}:svc:${service_id}`) and by suffix match (`svc:${service_id}` or `${service_id}`).
- **`send_terminal_input` command:** Added Tauri command to dispatch standard input strings down the managed child process's stdin channel.
- **`launch_interactive_shell` command:** Added Tauri command to spawn a managed interactive shell (`/bin/bash -i` or `$SHELL`) scoped directly to the project's root path.
- **`open_system_terminal` command:** Added Tauri command to spawn the host's preferred terminal emulator (reading `[system].preferred_terminal` from `config.toml`, defaulting to Ghostty) with proper working directory flags (`--working-directory` for Ghostty/Alacritty, `--directory` for Kitty, `-D` for Foot, etc.).
- Registered all new commands in the application's `invoke_handler`.

### 2. Frontend ANSI Engine (`src/utils/ansi.ts`)
- Implemented `parseAnsiToHtml` with safe HTML entity escaping (`&`, `<`, `>`, `"`, `'`) prior to tokenizing SGR sequences.
- Supports 16 standard and bright foreground/background colours, bold, dim, italic, underline, and extended 256-colour approximations.
- Strips unhandled cursor navigation and screen clear control codes to eliminate raw escape sequence debris.

### 3. Dedicated Terminal Workspace (`src/components/workspace/ProjectTerminalGrid.vue`)
- **Left Explorer Rail:**
  - Fast search filter for scripts and commands.
  - SESSIONS section: Interactive bash shell (`bash -i`) with start/interrupt controls, plus supervisor log fallback.
  - PERSISTENT SERVICES section: Status dot, start/stop toggles, restart button, parameter configuration modal (`ScriptLaunchModal`), and 15-second CPU/RAM profiler trigger.
  - DETERMINISTIC TASKS section: Run button, exit code badge, execution duration in milliseconds, and parameter modal.
  - Top action buttons: `Open in Ghostty` (or preferred terminal) and `+ Shell`.
- **Right Console Viewport:**
  - Process metadata header (status badge, PID, effective command).
  - Controls: `ALL` vs `ERRORS` filter, autoscroll lock toggle (`AUTO`), clear console display, and clipboard copy.
  - Native terminal viewport with dark brutalist background (`bg-[#09090b]`) and ANSI colour rendering.
  - Monospace interactive stdin bar at the bottom with prompt indicator (`project $`), command history navigation (Up/Down arrow keys), and `SIGINT` (Ctrl+C) interrupt support.

## Verification
- Rust compilation: `cargo check --manifest-path src-tauri/Cargo.toml` passed with exit code 0.
- Frontend type verification: `npx vue-tsc --noEmit` passed with exit code 0.
- Production build: `npm run build` completed successfully with Vite packaging in 5.10s.
