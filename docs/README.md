# ONE-SHIP Architecture & Technical Specifications

This directory contains the living specifications, architectural manifestos, and domain foundations that govern the design and execution of **ONE-SHIP**.

Code in this repository is the materialization of these specifications. When behaviors evolve, specifications are updated in the same work cycle.

---

## 🏛️ Domain Foundations & Manifestos

- [00 Manifesto](00_manifesto.md) — Core vision as a multidisciplinary maker's toolbox and agentic control center.
- [01 Ubiquitous Language](01_ubiquitous_language.md) — Canonical vocabulary and terminology across all layers.
- [02 Architecture Stack](02_architecture_stack.md) — Three-layer decoupled architecture (Daemon, HTTP MCP/RPC Bus, GUI) and tri-database storage (SQLite, DuckDB, LanceDB/BGE-M3).
- [03 Belta Dictionary](03_lang_belta_dictionary.md) — Domain lexicon and Belter Creole reference (parked until post-completion packaging).
- [Architectural & Code Audit Reports](audit/audit_00_index.md) — Active `2026-11-16` general audit, discrepancy matrix, and step-by-step remediation plan (with historical audits in `audit/archived/`).

---

## 📐 System Specifications (`docs/specs/`)

The 31 living specifications below define the subsystems of ONE-SHIP, organized into 5 architectural pillars:

### 1. Core Architecture & Cascading Configuration
- [Spec 01: Service Orchestrator](specs/01_service_orchestrator.md) — Deterministic lifecycle control for project services, tasks, and shells.
- [Spec 06: Plugin Architecture (Kopeng)](specs/06_plugin_architecture.md) — Modular domain subsystem architecture and extensible capabilities.
- [Spec 10: Configuration Schema & Cascade](specs/10_configuration_schema_and_cascade.md) — 4-level cascading configuration (`rula.toml` ↔ system defaults) across 16 plugin schemas.
- [Spec 24: Project Versioning & Migration Engine](specs/24_project_versioning_and_migration_engine.md) — Database schema migrations, flag resolution, and project lifecycle upgrades.

### 2. Autonomous Agent Runtime & MCP Protocol
- [Spec 08: Native Process Supervisor](specs/08_native_process_supervisor.md) — Headless multi-turn agent execution (`agy`, `claude`, `opencode` with DeepSeek) over `stream-json` and ACP.
- [Spec 12: Streamable HTTP MCP Daemon](specs/12_streamable_http_mcp_daemon.md) — High-throughput JSON-RPC agent tools and daemon RPC API with PID-attested security.
- [Spec 40: Unified Capability Core & Voice Harness](specs/40_unified_capability_core_and_voice_harness.md) — Domain capability registry, semantic/knowledge graph retrieval, and voice controller tool bridge.
- [Spec 41: Linux Packaging, Multi-Distro & Host Capabilities](specs/41_linux_packaging_and_host_capabilities.md) — Relocatable runtime paths, Linux multi-distro portability, dynamic feature gating, and installer wizard roadmap.
- [Spec 18: MCP Dialogue Channel & Chat Overlay](specs/18_mcp_dialogue_channel_and_chat_overlay.md) — Docked and floating interactive overlay for bidirectional agent-user dialogue.
- [Spec 19: Agent Quota & Context Observability](specs/19_agent_quota_and_context_observability.md) — Real-time token usage and context window consumption tracking.
- [Spec 29: On-Demand Skill Manager & MCP Protocol](specs/29_on_demand_skill_manager_and_mcp_protocol.md) — Dynamic skill injection and lifecycle symlinking.
- [Spec 30: Conversation & Context Continuity](specs/30_agent_thread_conversation_and_context_continuity.md) — Persistent thread state, Native Sessions, and runner-neutral Briefs.

### 3. Telemetry, Profiling & Session Graphs
- [Spec 09: VCS and Logging Architecture](specs/09_vcs_and_logging_architecture.md) — Dual-signification Git status, branch management, and bounded log capturing.
- [Spec 16: Agent Session Snapshot System](specs/16_agent_session_snapshot_system.md) — Point-in-time hidden Git ref snapshots (`refs/one-ship/snapshots/`) and recovery.
- [Spec 22: Session Lifecycle & Devlog Graph](specs/22_session_lifecycle_and_devlog_graph.md) — Development journals, session timelines, and LanceDB/BGE-M3 semantic search (RAG).
- [Spec 23: Token Economy & Quota Kickstart](specs/23_token_economy_and_quota_kickstart.md) — Token economy and quota runway kickstart probes.
- [Spec 25: Universal Telemetry & Profiling Engine](specs/25_universal_telemetry_and_profiling_engine.md) — Continuous process-tree performance profiling using DuckDB columnar analytics and flamegraph engines.

### 4. System, Hardware, CAD/PCB & OSS Integrations
- [Spec 04: Dictation & NLP (OpenDictate)](specs/04_dictation_and_nlp.md) — Asynchronous voice transcription integration over D-Bus.
- [Spec 05: Dictate IPC Protocol](specs/05_dictate_ipc_protocol.md) — Freedesktop D-Bus session bus protocol for local voice input.
- [Spec 11: Remote GitHub & GitLab Integration](specs/11_remote_github_integration.md) — GitHub (`gh`) and GitLab (`glab`) CLI remote repository workflows.
- [Spec 17: Multi-Account Management](specs/17_multi_account_management.md) — Secure multi-credential storage via system keyring and AES-256-GCM.
- [Spec 26: Greenfield Project Creation & Bootstrapping](specs/26_greenfield_project_creation_and_conversational_bootstrapping.md) — Autonomous spec-first project scaffolding.
- [Spec 26b: REA Reverse Engineering Integration](specs/26_rea_reverse_engineering_integration.md) — REA engine gateway for binary and firmware reverse engineering with a multi-project FIFO job queue.
- [Spec 31: Electronics Workstation & Instrumentation](specs/31_electronics_workstation_and_instrumentation_engine.md) — USB oscilloscope (`Owon VDS1022`), logic analyzer (`fx2lafw`), and MCU firmware toolchains.
- [Spec 32: One-Ship Designer Integration](specs/32_oneship_designer_integration.md) — Native UI/vector design editor (`desigma` fork) and `/mcp/desigma` gateway.
- [Spec 33: CAD & PCB Integration](specs/33_cad_and_pcb_integration.md) — 3D parametric CAD (`SolidCraft`) and EDA/PCB layout (`PCBCraft`) workstation integration.
- [Spec 34: Project Management & OSS Collaboration](specs/34_project_management_and_oss_collaboration.md) — Multidisciplinary Kanban board, Timeline milestones, and GitHub Fork/Upstream OSS workflows.

### 5. User Interface & Artifacts
- [Spec 07: i18n Translation Strategy](specs/07_i18n_translation_strategy.md) — Internationalization and dialect dictionary support (parked until packaging).
- [Spec 13: Design System & UI Components](specs/13_design_system_and_ui_components.md) — Brutalist typography, custom tooltips (`AppTooltip`), and strict UI component standards.
- [Spec 14: Cinturonero Translator Protocol](specs/14_cinturonero_translator_protocol.md) — Protocol for the Cinturonero (Belta) interface language (parked until packaging).
- [Spec 20: Live Artifacts & Project Scratchpad](specs/20_live_artifacts_and_project_scratchpad.md) — Codebase explorer, CodeMirror 6 reader/editor, and live agent artifact previews.
- [Spec 21: Agent Pixel Portrait DNA](specs/21_agent_pixel_portrait_dna.md) — Procedural layered visual identification for agent personas.

*(Note: [Spec 02: Diagnostic Laboratory](specs/02_diagnostic_laboratory.md) and [Spec 03: Tray Hub Interface](specs/03_tray_hub_interface.md) are deferred post-0.2.x concepts; tray controls are provided natively by the Omarchy bar widget in `integrations/omarchy-bar/`).*
