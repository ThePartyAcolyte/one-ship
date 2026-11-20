# ONE-SHIP Technical Specifications

This directory holds the 40 living technical specifications for **ONE-SHIP**, organized into 7 subsystem categories so contributors and external readers can navigate the architecture cleanly on GitHub.

> [!NOTE]
> Looking for the foundational vision, glossary, or stack overview? See [`docs/README.md`](../README.md), the [Manifesto](../00_manifesto.md), [Ubiquitous Language](../01_ubiquitous_language.md), [Architecture Stack](../02_architecture_stack.md), and [Engineering & Design Rules](../DESIGN_RULES.md). Historical pre-reorganization specs (`01_*`–`41_*`) are archived in [`docs/legacy/`](../legacy/README.md) with a complete [Mapping Table](../legacy/MAPPING.md).

---

## Categories Overview

| # | Category | Focus | Specs |
| :-: | :--- | :--- | :-: |
| `01` | [Architecture](01-architecture/README.md) | Plugin model, 4-level config cascade, skill injection, HTTP MCP daemon, operator RPC API, project migrations, Linux packaging & `./install.sh` | 7 |
| `02` | [Agents](02-agents/README.md) | Process supervisor, thread/conversation continuity, chat overlay & dialogue, multi-account auth, quota/tokens, snapshots, on-demand skills, portraits | 8 |
| `03` | [Projects](03-projects/README.md) | Service orchestration, Git cockpit, cyclic logging, remote repos, Forge abstraction & inbox, milestones/Kanban, artifacts, greenfield init | 9 |
| `04` | [Observability](04-observability/README.md) | Session lifecycle, SQLite devlog, LanceDB/BGE-M3/Tree-sitter causal knowledge graph, DuckDB continuous profiling & flamegraphs | 2 |
| `05` | [Voice](05-voice/README.md) | Native Gemini Live dictation into focused fields, Gemini Live global voice controller ("Ship Brain"), Unified Capability Core | 3 |
| `06` | [Hardware & CAD](06-hardware-and-cad/README.md) | USB electronics workstation (oscilloscope/LA/MCU/SPICE), One-Ship Designer (`desigma`), SolidCraft & PCBCraft, REA reverse engineering | 4 |
| `07` | [UI & Design System](07-ui/README.md) | Design tokens, `src/components/ui/` library, workspace chrome, daemon UI state machine, settings & overview, Lang Belta i18n, deferred tools | 7 |

---

## Current Platform & Packaging Baseline

ONE-SHIP targets native **Linux workstations** (sandboxes such as Flatpak and Snap are excluded by design due to `/proc/<pid>` MCP caller attestation, `systemd --user` supervision, and USB/serial access):

- **Non-Dev Production Installer ([`install.sh`](../../install.sh)):** Builds release binaries (`one-ship`, `oneship-daemon`), installs `one-ship-launch` with WebKitGTK startup retry (`WEBKIT_DMABUF_RENDERER_FORCE_SHM=1`), relocatable `templates/`, `.desktop` entry, and `oneship-daemon.service`, and adapts window chrome + AppIndicator/SNI system tray behavior dynamically for floating desktops (`!is_hyprland_session()`).
- **Distribution Test Matrix:** Primary development runs on **Arch / Omarchy (Hyprland)**; non-dev installation and floating-DE behavior are currently under active testing on **Ubuntu (GNOME)**, with verification planned for **Fedora Workstation** and **Linux Mint (Cinnamon)**. See [Linux Packaging and Host Capabilities](01-architecture/07-linux-packaging-and-host-capabilities.md).

---

## Recommended Reading Paths

- **New to ONE-SHIP architecture?**
  1. [Architecture Stack](../02_architecture_stack.md)
  2. [Plugin Architecture (Kopeng)](01-architecture/01-plugin-architecture.md)
  3. [Configuration Schema and Cascade](01-architecture/02-configuration-schema-and-cascade.md)
  4. [Streamable HTTP MCP Daemon](01-architecture/04-streamable-http-mcp-daemon.md)
  5. [Daemon Operator API and Lifecycle](01-architecture/05-daemon-operator-api-and-lifecycle.md)
  6. [Linux Packaging and Host Capabilities](01-architecture/07-linux-packaging-and-host-capabilities.md)

- **Autonomous agents, sessions & MCP tools:**
  1. [Native Process Supervisor](02-agents/01-native-process-supervisor.md)
  2. [Thread, Conversation, and Continuity](02-agents/02-thread-conversation-and-continuity.md)
  3. [MCP Dialogue Channel and Chat Overlay](02-agents/03-mcp-dialogue-and-chat-overlay.md)
  4. [Multi-Account Management](02-agents/04-multi-account-management.md)
  5. [Session Lifecycle, Database Devlog & Knowledge Graph](04-observability/01-session-lifecycle-and-devlog-graph.md)

- **Voice orchestration & capabilities:**
  1. [Native Gemini Dictation](05-voice/02-dictation-and-agent-chat.md)
  2. [Global Voice Controller](05-voice/03-global-voice-controller.md)
  3. [Unified Capability Core and Voice Orchestrator Harness](05-voice/04-unified-capability-core-and-voice-harness.md)

- **Contributing to the UI:**
  1. [Design System Foundations](07-ui/01-design-system-foundations.md)
  2. [UI Component Library and Composition Recipes](07-ui/02-ui-component-library-and-recipes.md)
  3. [Workspace Chrome and Panel Contracts](07-ui/03-workspace-chrome-and-panel-contracts.md)
  4. [UI State Machine and Session Persistence](07-ui/04-ui-state-machine-and-session-persistence.md)

---

## Full Specification Index

### [01 — Architecture](01-architecture/README.md)
- [Plugin Architecture (Kopeng)](01-architecture/01-plugin-architecture.md)
- [Configuration Schema and Cascade](01-architecture/02-configuration-schema-and-cascade.md)
- [Platform Skill Injection](01-architecture/03-platform-skill-injection.md)
- [Streamable HTTP MCP Daemon](01-architecture/04-streamable-http-mcp-daemon.md)
- [Daemon Operator API and Lifecycle](01-architecture/05-daemon-operator-api-and-lifecycle.md)
- [Project Versioning and Migration](01-architecture/06-project-versioning-and-migration.md)
- [Linux Packaging and Host Capabilities](01-architecture/07-linux-packaging-and-host-capabilities.md)

### [02 — Agents](02-agents/README.md)
- [Native Process Supervisor](02-agents/01-native-process-supervisor.md)
- [Thread, Conversation, and Continuity](02-agents/02-thread-conversation-and-continuity.md)
- [MCP Dialogue Channel and Chat Overlay](02-agents/03-mcp-dialogue-and-chat-overlay.md)
- [Multi-Account Management](02-agents/04-multi-account-management.md)
- [Quota Observability and Token Economy](02-agents/05-quota-observability-and-token-economy.md)
- [Agent Session Snapshots](02-agents/06-agent-session-snapshots.md)
- [On-Demand Skills and Skill Manager](02-agents/07-on-demand-skills.md)
- [Agent Pixel Portrait DNA](02-agents/08-agent-pixel-portrait-dna.md)

### [03 — Projects & Workspace](03-projects/README.md)
- [Project Registry, Scripts & Service Orchestration](03-projects/01-project-registry-and-service-orchestration.md)
- [Git Workspace, Branches & Snapshots Cockpit](03-projects/02-git-workspace-and-branches.md)
- [System Logging & Status Terminal](03-projects/03-system-logging-and-status-terminal.md)
- [Remote Repositories & Diagnostics](03-projects/04-remote-repositories-and-diagnostics.md)
- [Forge Provider Abstraction](03-projects/05-forge-provider-abstraction.md)
- [Forge Inbox & Triage](03-projects/06-forge-inbox-and-triage.md)
- [Project Management & Milestones](03-projects/07-project-management-and-milestones.md)
- [Live Artifacts & Scratchpad](03-projects/08-live-artifacts-and-scratchpad.md)
- [Greenfield Project Creation](03-projects/10-greenfield-project-creation.md)

### [04 — Observability](04-observability/README.md)
- [Session Lifecycle, Database Devlog & Knowledge Graph](04-observability/01-session-lifecycle-and-devlog-graph.md)
- [Universal Telemetry, Continuous Profiling & Profiler Engines](04-observability/02-universal-telemetry-and-profiling.md)

### [05 — Voice & Capability Orchestration](05-voice/README.md)
- [Native Gemini Dictation](05-voice/02-dictation-and-agent-chat.md)
- [Global Voice Controller](05-voice/03-global-voice-controller.md)
- [Unified Capability Core and Voice Orchestrator Harness](05-voice/04-unified-capability-core-and-voice-harness.md)

### [06 — Hardware, CAD, Design & Reverse Engineering](06-hardware-and-cad/README.md)
- [Electronics Workstation and Instrumentation Engine](06-hardware-and-cad/01-electronics-workstation-and-instrumentation.md)
- [One-Ship Designer Integration](06-hardware-and-cad/02-oneship-designer-integration.md)
- [CAD and PCB Integration](06-hardware-and-cad/03-cad-and-pcb-integration.md)
- [REA Reverse Engineering Engine and Multi-Project Gateway](06-hardware-and-cad/04-rea-reverse-engineering-integration.md)

### [07 — UI & Design System](07-ui/README.md)
- [Design System Foundations](07-ui/01-design-system-foundations.md)
- [UI Component Library and Composition Recipes](07-ui/02-ui-component-library-and-recipes.md)
- [Workspace Chrome and Panel Contracts](07-ui/03-workspace-chrome-and-panel-contracts.md)
- [UI State Machine and Session Persistence](07-ui/04-ui-state-machine-and-session-persistence.md)
- [Project Settings and Overview Dashboard](07-ui/05-project-settings-and-overview-dashboard.md)
- [Localization and Belta Language Strategy](07-ui/06-localization-and-belta-language.md)
- [Diagnostic Laboratory and Tray Hub Interface](07-ui/07-diagnostic-laboratory-and-tray-hub.md)

---

## Specification Conventions

Every specification in this tree follows a uniform structure:

1. **Purpose & Scope** — What problem the subsystem solves and explicit boundaries.
2. **Model / Design** — Architecture, invariants, state machines, and data flows.
3. **Contracts** — Canonical API commands, MCP tools, SSE events, schemas, and `rula.toml` / `config.toml` keys.
4. **Implementation Status** — Verified code reality (`Implemented`, `Partial`, or `Roadmap`).
5. **Related Specs & Provenance** — Relative cross-links and `Supersedes legacy:` lineage.
