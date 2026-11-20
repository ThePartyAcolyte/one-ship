# Architecture Specifications

Core platform architecture specifications for ONE-SHIP: modular domain plugins, hierarchical configuration, platform skill distribution, the Streamable HTTP MCP daemon, operator RPC and lifecycle management, project versioning/migration, and Linux packaging/host capabilities.

## Reading Order

1. [Plugin Architecture (Kopeng)](01-plugin-architecture.md) — Modular domain subsystems (`kopeng/`), plain-English `rula.toml` namespacing, typed UI events, and external `manifest.toml` plugin discovery.
2. [Configuration Schema and Cascade](02-configuration-schema-and-cascade.md) — Four-level configuration cascade, runner/model/effort pair resolution, 17 plugin schemas, and `ProjectConfigPanel.vue` contract.
3. [Platform Skill Injection](03-platform-skill-injection.md) — Single template source (`templates/`), relocatable `TEMPLATES_ROOT`, base vs. specialized skills, and `SkillStatus` versioning.
4. [Streamable HTTP MCP Daemon](04-streamable-http-mcp-daemon.md) — Axum HTTP MCP transport (`POST /mcp`), 59-tool partitioning, SQLite multi-agent delegation ledger, and Linux `/proc` process-attested caller identity.
5. [Daemon Operator API and Lifecycle](05-daemon-operator-api-and-lifecycle.md) — Mandatory daemon model, `/api/events` SSE bus, fail-closed `/api/*` operator guard, `POST /api/v1/call/<command>` RPC contract, systemd dev launcher, Omarchy bar widget, and WebKitGTK renderer modes.
6. [Project Versioning and Migration](06-project-versioning-and-migration.md) — `rula.toml` `0.2.x` version invariant, Tier 1 deterministic migration, Phase 1 initialization lifecycle, and Phase 2 on-demand flag resolution.
7. [Linux Packaging and Host Capabilities](07-linux-packaging-and-host-capabilities.md) — Multi-distro Linux portability, relocatable assets, runtime `HostCapabilities` feature gating, unified Omarchy bar telemetry, and the interactive `./install.sh` wizard.
