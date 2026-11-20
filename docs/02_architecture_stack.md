# Architecture and Tech Stack

## Core Stack
ONE-SHIP implements a decoupled three-layer architecture tailored for a single-machine Arch Linux (Omarchy / Hyprland on Wayland) maker workstation:

*   **Backend (`oneship-daemon`):** Rust (Tokio, Axum, `thiserror`, `ts-rs`, `rusb`, `serialport`). A persistent background daemon that is the **sole owner of child processes and storage engines**. It manages subprocess supervision, file watching, state hydration, hardware USB/serial probing, and continuous telemetry without memory leaks, broadcasting real-time events via `tokio::sync::broadcast`.
*   **Tri-Database Embedded Storage Layer:**
    1.  **SQLite (`~/.config/one-ship/oneship.db`):** Relational state in WAL mode with `r2d2` connection pooling (`projects`, `agent_conversations`, `agent_chat_turns`, `agent_session_journal`, `project_todos`, `project_technical_debt`, `project_tasks`, `supervised_processes`, `snapshots`, `accounts`, `system_logs`).
    2.  **DuckDB (`~/.config/one-ship/oneship_telemetry.duckdb`):** Embedded columnar analytics engine storing high-frequency process-tree CPU, RSS memory, thread, and disk I/O samples for every supervised service and task run (Spec 25).
    3.  **LanceDB (`~/.config/one-ship/vectors.lance/` + Local `BGE-M3` INT8 ONNX):** Embedded vector store and local 1024-dimensional embedding engine (`fastembed` / `ort`) powering offline-first transversal semantic search (RAG) across source code, living specifications, session minutes, tasks, and technical debt (Spec 22).
*   **Agent & Service Orchestrator (Native Process Supervisor):** Rust (`supervisor.rs`, Spec 08). Spawns services, tasks, and headless multi-turn agent runners in isolated process groups with bounded output ring buffers. Supported agent harnesses are strictly those installed and used on the workstation:
    *   **Antigravity (`agy`)** over `stream-json`
    *   **Claude Code (`claude`)** over `stream-json`
    *   **OpenCode (`opencode`)** over ACP, configured with **DeepSeek**
*   **MCP & Operator HTTP Bus:** Rust (Axum on port `8765`, Spec 12). Exposes PID-attested Model Context Protocol endpoints (`/mcp`, `/mcp/rea`, `/mcp/desigma`, `/mcp/pcbcraft`) to managed agent subprocesses, alongside the operator RPC endpoint (`/api/call`, attested via `/proc/<pid>/exe`) and the Server-Sent Events stream (`GET /api/events`) consumed by the GUI.
*   **Frontend (The Hub):** Tauri v2 + Vue 3 + TypeScript + Pinia + Tailwind CSS + **CodeMirror 6** (`AppCodeEditor.vue`). A thin, reactive GUI shell that invokes daemon commands via `daemonCall` (typed end-to-end with `ts-rs`) and subscribes to `/api/events` SSE. It opens no database and supervises no processes; closing the window leaves the daemon and all running agents/services untouched.

## Multidisciplinary Maker Subsystems (`src-tauri/src/kopeng/`)
The backend is organized around modular domain subsystems under `kopeng`:
*   **Agent Personas, Skills & Snapshots:** Dynamic persona configuration (`persona.rs`), on-demand skill symlinking (`skill.rs`), and hidden Git ref snapshots (`snapshots.rs`, `refs/one-ship/snapshots/`).
*   **VCS & OSS Collaboration:** Local Git & branch management (`git.rs`), GitHub/GitLab remote management (`github.rs`, `gitlab.rs`), and Kanban/Timeline + Fork/Upstream OSS workflows (`project_mgmt.rs`, Spec 34).
*   **Electronics & Instrumentation Lab:** USB oscilloscope driver (`owon_vds1022.rs`), logic analyzer capture (`fx2lafw.rs` / `sigrok-cli`), and MCU firmware toolchains (`firmware.rs`, `toolchain.rs`, Spec 31).
*   **CAD, PCB & UI Design Bridges:** Headless preview and MCP/CLI bridges for 3D CAD (`solidcraft.rs`, Spec 33), EDA/PCB (`pcbcraft.rs`, Spec 33), and UI vector design (`desigma.rs` / `oneship-designer`, Spec 32).
*   **Reverse Engineering Gateway:** Multi-project FIFO job queue and daemon lifecycle manager for `rea` (`rea.rs`, Spec 26b).
*   **Voice Bus:** Native Freedesktop D-Bus integration with **OpenDictate** (`opendictate.rs`, Specs 04 & 05) for local speech-to-text streaming.
