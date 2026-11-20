# Manifesto: ONE-SHIP

## Vision
ONE-SHIP is a **Multidisciplinary Maker's Toolbox and Agentic Control Center** for Linux workstations (Arch/Omarchy, Ubuntu, Fedora Workstation, and Linux Mint). Physical and systems engineering span multiple tightly coupled worlds—software, embedded firmware, USB benchtop instrumentation, 3D CAD, PCB/EDA design, UI design, binary reverse engineering, and project management—that are traditionally fragmented across disconnected software silos. ONE-SHIP bridges these worlds under a single roof, acting as a silent process orchestrator, a hardware/software telemetry hub, and the central nervous system for autonomous AI agents.

## What it IS
*   **A Maker's Multidisciplinary Workstation:** Unifies code and living specifications (via an integrated CodeMirror 6 editor), 3D CAD (`SolidCraft`), PCB/EDA (`PCBCraft`), UI design (`One-Ship Designer`), USB hardware instrumentation (oscilloscopes, logic analyzers, MCU toolchains), and reverse engineering (`REA`) into one cohesive cockpit.
*   **A Multi-Agent Orchestrator:** Manages isolated project workspaces, supervised natively by the daemon, where specialized AI agents (personas) collaborate, delegate tasks, and report outcomes.
*   **A Subprocess & Service Manager:** Deterministically launches, stops, adopts, and monitors persistent dev servers, one-shot tasks, and interactive shells.
*   **An MCP Bus Provider:** Exposes a PID-attested HTTP Model Context Protocol (MCP) server so agents can interact with the host machine, project state, semantic knowledge (RAG), and each other.
*   **A Continuous Profiling & Diagnostic Engine:** Captures real-time process-tree telemetry into DuckDB, wraps services in flamegraph profilers (`samply`, `py-spy`), and extracts semantic errors from `stderr` (with cognitive LLM consumption of profiling and diagnostic streams tracked as an ongoing evolution).
*   **A Three-Layered Architecture:** Composed of a persistent core Daemon (the sole owner of processes and databases), an HTTP MCP/RPC Bus, and a decoupled Tauri/Vue GUI (the human lens).

## What it is NOT
*   Not a traditional manual-coding-only IDE built around hand-typing boilerplate.
*   Not a raw terminal multiplexer where agent context and service logs are lost in scrollback.
*   Not a blind log sink.
*   Not a monolithic app tied to a single UI window lifecycle (closing the window leaves the daemon, agents, and services running).
