# ONE-SHIP Architecture & Technical Documentation

This directory contains the living specifications, architectural manifestos, and domain foundations that govern the design and execution of **ONE-SHIP**.

Code in this repository is the materialization of these specifications. When behaviors evolve, specifications are updated in the same work cycle.

---

## Domain Foundations & Manifestos

- [00 Manifesto](00_manifesto.md) — Core vision as a multidisciplinary maker's toolbox and agentic control center.
- [01 Ubiquitous Language](01_ubiquitous_language.md) — Canonical vocabulary and terminology across all layers.
- [02 Architecture Stack](02_architecture_stack.md) — Three-layer decoupled architecture (Daemon, HTTP MCP/RPC Bus, GUI) and tri-database storage (SQLite, DuckDB, LanceDB/BGE-M3).
- [03 Belta Dictionary](03_lang_belta_dictionary.md) — Domain lexicon and Belter Creole reference (parked until post-completion packaging).
- [Engineering & Design Rules](DESIGN_RULES.md) — Detailed Tiger Style, concurrency, UI, logging, and onboarding standards.

---

## System Specifications ([`docs/specs/`](specs/README.md))

All 40 living specifications are organized into 7 subsystem categories:

### 1. [Architecture](specs/01-architecture/README.md)
- [Plugin Architecture (Kopeng)](specs/01-architecture/01-plugin-architecture.md) — Modular domain subsystems (`kopeng/`), plain-English `rula.toml` namespacing, and declarative plugin discovery.
- [Configuration Schema and Cascade](specs/01-architecture/02-configuration-schema-and-cascade.md) — 4-level configuration cascade (`rula.toml` <-> system defaults), runner resolution, and 18 plugin schemas.
- [Platform Skill Injection](specs/01-architecture/03-platform-skill-injection.md) — Single-source `templates/` skill catalog, relocatable `TEMPLATES_ROOT`, and `SkillStatus` versioning.
- [Streamable HTTP MCP Daemon](specs/01-architecture/04-streamable-http-mcp-daemon.md) — Axum HTTP MCP bus (`POST /mcp`), 59-tool partitioning, delegation ledger, and `/proc` caller attestation.
- [Daemon Operator API and Lifecycle](specs/01-architecture/05-daemon-operator-api-and-lifecycle.md) — Mandatory daemon model, `/api/events` SSE bus, fail-closed `/api/*` guard, RPC commands, and systemd units.
- [Project Versioning and Migration](specs/01-architecture/06-project-versioning-and-migration.md) — `rula.toml` `0.2.x` invariant, deterministic Tier 1 migrations, and Phase 1/2 initialization flags.
- [Linux Packaging and Host Capabilities](specs/01-architecture/07-linux-packaging-and-host-capabilities.md) — Multi-distro Linux portability, runtime `HostCapabilities` feature gating, Omarchy bar widget, and `./install.sh`.

### 2. [Agents](specs/02-agents/README.md)
- [Native Process Supervisor](specs/02-agents/01-native-process-supervisor.md) — Headless multi-turn agent execution (`agy`, `opencode`, `claude`) over `stream-json` and ACP.
- [Thread, Conversation, and Continuity](specs/02-agents/02-thread-conversation-and-continuity.md) — Separation of Threads, Conversations, and Native Sessions; SQLite Briefs; FTS5 search and reopen.
- [MCP Dialogue Channel and Chat Overlay](specs/02-agents/03-mcp-dialogue-and-chat-overlay.md) — `post_agent_response` / `request_approval` MCP contracts and `FloatingChatOverlay` / `AgentTaskbar` UI.
- [Multi-Account Management](specs/02-agents/04-multi-account-management.md) — Multi-provider (`google`, `deepseek`, `claude`) account discovery, AES-256-GCM keyring tokens, and model-pool rotation.
- [Quota Observability and Token Economy](specs/02-agents/05-quota-observability-and-token-economy.md) — Real-time token/cache/context window tracking, 5h/7d quota windows, and PTY warmup probes.
- [Agent Session Snapshots](specs/02-agents/06-agent-session-snapshots.md) — Event-driven, domain-scoped hidden Git ref snapshots (`refs/one-ship/snapshots/`).
- [On-Demand Skills and Skill Manager](specs/02-agents/07-on-demand-skills.md) — Project-scoped `.agents/skills/` JIT toggling and MCP skill lifecycle tools.
- [Agent Pixel Portrait DNA](specs/02-agents/08-agent-pixel-portrait-dna.md) — Deterministic `64x96` layered pixel portrait DNA format and Canvas compositor.

### 3. [Projects & Workspace](specs/03-projects/README.md)
- [Project Registry, Scripts & Service Orchestration](specs/03-projects/01-project-registry-and-service-orchestration.md) — SQLite registry, `rula.toml` script supervision, `ONESHIP_RUN_ID` boot reconciliation, and terminal hub.
- [Git Workspace, Branches & Snapshots Cockpit](specs/03-projects/02-git-workspace-and-branches.md) — Dual-signification Git status model, dynamic presets, diffs, and branch lifecycle.
- [System Logging & Status Terminal](specs/03-projects/03-system-logging-and-status-terminal.md) — Cyclic rotating `oneship.log`, bounded SQLite `system_logs` sink, and 5-line rolling status terminal.
- [Remote Repositories & Diagnostics](specs/03-projects/04-remote-repositories-and-diagnostics.md) — Zero-credential `gh` / `glab` remote workflows, `GitRemote` classification, host diagnostics, and `pkexec` install.
- [Forge Provider Abstraction](specs/03-projects/05-forge-provider-abstraction.md) — Provider-neutral `ForgeProvider` contract, project surface hierarchy, and GitHub/GitLab adapters.
- [Forge Inbox & Triage](specs/03-projects/06-forge-inbox-and-triage.md) — Persistent `forge_inbox_items` read/unread/updated state and triage flags (`flagged`, `muted`).
- [Project Management & Milestones](specs/03-projects/07-project-management-and-milestones.md) — Opt-in Kanban stages, Schedule/Gantt geometry, `Fork <-> Upstream` OSS flow, and milestones.
- [Live Artifacts & Scratchpad](specs/03-projects/08-live-artifacts-and-scratchpad.md) — File-backed `docs/` and `scratchpad/` lifecycle, inline comments, sandboxed explorer, and CodeMirror 6 editor.
- [Greenfield Project Creation](specs/03-projects/10-greenfield-project-creation.md) — Dual-path `AddProjectModal` and spec-first `greenfield_initializer` conversational bootstrapping.

### 4. [Observability](specs/04-observability/README.md)
- [Session Lifecycle, Database Devlog & Knowledge Graph](specs/04-observability/01-session-lifecycle-and-devlog-graph.md) — Development journals, causal knowledge graph, and LanceDB/BGE-M3/Tree-sitter semantic search.
- [Universal Telemetry, Continuous Profiling & Profiler Engines](specs/04-observability/02-universal-telemetry-and-profiling.md) — Continuous process-tree sampling into DuckDB and declarative `manifest.toml` profiler wrappers.

### 5. [Voice & Capability Orchestration](specs/05-voice/README.md)
- [Native Gemini Dictation](specs/05-voice/02-dictation-and-agent-chat.md) — Gemini Live speech-to-text into focused fields, revision-safe transcript reconciliation, contextual `Alt+V`, VAD rollover, reconnect replay, and auto-send.
- [Global Voice Controller](specs/05-voice/03-global-voice-controller.md) — Push-to-Talk Gemini Live conversational orchestrator ("Ship Brain") and PipeWire audio bridge.
- [Unified Capability Core and Voice Orchestrator Harness](specs/05-voice/04-unified-capability-core-and-voice-harness.md) — Navigation-agnostic `CapabilityProvider` registry, persistent voice sessions, and 65K TPM budget controls.

### 6. [Hardware, CAD, Design & Reverse Engineering](specs/06-hardware-and-cad/README.md)
- [Electronics Workstation and Instrumentation Engine](specs/06-hardware-and-cad/01-electronics-workstation-and-instrumentation.md) — Native USB oscilloscope (`Owon VDS1022`), logic analyzer (`fx2lafw`), MCU toolchains, and SPICE roadmap.
- [One-Ship Designer Integration](specs/06-hardware-and-cad/02-oneship-designer-integration.md) — Sibling `desigma` UI/vector editor fork and `/mcp/desigma` gateway.
- [CAD and PCB Integration](specs/06-hardware-and-cad/03-cad-and-pcb-integration.md) — 3D parametric CAD (`SolidCraft`) and EDA/PCB layout (`PCBCraft`) workstation integration.
- [REA Reverse Engineering Engine and Multi-Project Gateway](specs/06-hardware-and-cad/04-rea-reverse-engineering-integration.md) — Per-project supervised `rea` HTTP engine, `/mcp/rea` FIFO heavy-job queue, and RE Lab explorer.

### 7. [UI & Design System](specs/07-ui/README.md)
- [Design System Foundations](specs/07-ui/01-design-system-foundations.md) — Theme CSS variables, surface hierarchy, spacing/typography scales, and mandatory agent design contract.
- [UI Component Library and Composition Recipes](specs/07-ui/02-ui-component-library-and-recipes.md) — Standardized `src/components/ui/` primitives (`AppTooltip`, `AppSelect`, `AppInput`, etc.) and layout recipes.
- [Workspace Chrome and Panel Contracts](specs/07-ui/03-workspace-chrome-and-panel-contracts.md) — Shell geometry, scrollbar/WebKitGTK blur invariants, Settings modal, and per-tab rules.
- [UI State Machine and Session Persistence](specs/07-ui/04-ui-state-machine-and-session-persistence.md) — Daemon-owned UI state, declarative `UiSurfaceRegistry`, chat FSM, and SSE projection.
- [Project Settings and Overview Dashboard](specs/07-ui/05-project-settings-and-overview-dashboard.md) — Fixed `settings` tab, editable metadata, and 4-column plugin-driven `overview` widget grid.
- [Localization and Belta Language Strategy](specs/07-ui/06-localization-and-belta-language.md) — English-first policy and deferred 3-tier Lang Belta / Cinturonero translation protocol.
- [Diagnostic Laboratory and Tray Hub Interface](specs/07-ui/07-diagnostic-laboratory-and-tray-hub.md) — Deferred post-`0.2.x` roadmap concepts for LLM crash curation, telemetry, and tray hub.

---

## Historical Specs Archive

Pre-reorganization numbered specifications (`01_service_orchestrator.md` through `41_linux_packaging_and_host_capabilities.md`) are preserved untouched for provenance in [`docs/legacy/`](legacy/README.md). See [`docs/legacy/MAPPING.md`](legacy/MAPPING.md) for the exact mapping to the current specification tree.
