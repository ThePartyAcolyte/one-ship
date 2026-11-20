# Devlog: Omarchy Polkit Engine & VCS Diagnostic Debt

**Date:** 2026-09-05  
**Topic:** Implementing backend engine for privileged tool installation via Omarchy's native Polkit (`pkexec`) and formalizing multi-provider VCS technical debt.

---

## 1. Problem Statement & Research
To support one-click installation of missing system binaries (e.g. `git`, `gh`, `glab`) in Phase 0 without forcing the user to open a terminal manually, we evaluated Omarchy's privilege escalation mechanisms.

Omarchy's official specification (`/usr/share/omarchy/default/agents/skills/omarchy/SKILL.md`) specifies:
- Use `sudo` when an interactive terminal TTY is present.
- Use `pkexec` when the caller cannot interact with a terminal (e.g. graphical desktop applications or background agents).

Empirical verification in the desktop environment (`WAYLAND_DISPLAY=wayland-1 DISPLAY=:0 pkexec whoami`) confirmed that Omarchy's native Quickshell Polkit agent (`omarchy.polkit` / `PolkitAgent.qml`) presents a themed password/fingerprint modal and cleanly completes privileged execution.

## 2. Backend Implementation (`src-tauri/src/kopeng/diagnostic.rs`)
1. Implemented `resolve_pacman_package(tool_id: &str) -> Option<&'static str>` mapping canonical tool identifiers to Arch Linux packages:
   - `"git"` -> `"git"`
   - `"gh"` / `"github-cli"` -> `"github-cli"`
   - `"glab"` / `"gitlab-cli"` -> `"glab"`
2. Implemented `install_missing_tool(tool_id: String)` and batch `install_missing_tools(tool_ids: Vec<String>)`:
   - Dispatches via `tauri::async_runtime::spawn_blocking`.
   - Invokes `pkexec pacman -S --noconfirm <pkgs...>`. Batch execution ensures a single Polkit prompt when installing multiple tools simultaneously.
3. Registered both commands in `tauri::generate_handler!` inside `src-tauri/src/lib.rs`.

## 3. Frontend UX Deferral & Technical Debt Register
Per user decision, frontend UI presentation is deferred pending resolution of architectural design questions:
- **Placement:** Whether missing tool alerts and installation buttons belong in the floating `DiagnosticBubble`, in a dedicated first-run Onboarding view, or inline within contextual action modals (`ProjectInitModal`, remote connection).
- **Multi-Provider VCS Coexistence:** How to handle users with mixed repositories (some on GitHub using `gh`, others on GitLab using `glab`), avoiding false "degraded system health" states when an unneeded CLI is missing.
- Documented in `TECHNICAL_DEBT.md` under **Item 9: UX Flow for Privileged Tool Installation & Multi-Provider VCS Diagnostics (GitHub vs GitLab vs Mixed)**.
