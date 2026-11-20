# Agents Specifications

Specifications covering autonomous agent process supervision, conversation continuity, MCP dialogue, multi-account credential management, quota observability, snapshots, on-demand skills, and persona portraits.

## Reading Order

1. [Native Process Supervisor](01-native-process-supervisor.md) — Asynchronous Tokio supervision in `oneship-daemon`, trimodal headless runner protocols (`agy`, `opencode`, `claude`), process group isolation, boot-time adoption, and port conflict guards.
2. [Thread, Conversation, and Runner-Neutral Context Continuity](02-thread-conversation-and-continuity.md) — Separation of Threads, Conversations, and Native Sessions; SQLite Brief assembly; configuration precedence; FTS5 search and Reopen.
3. [MCP Dialogue Channel and Chat Overlay](03-mcp-dialogue-and-chat-overlay.md) — `post_agent_response` and `request_approval` MCP contracts, turn-end compliance enforcement, composite-key frontend state, and `FloatingChatOverlay` / `AgentTaskbar` UI.
4. [Multi-Account Management](04-multi-account-management.md) — Multi-provider (`google`, `deepseek`, `claude`) discovery, machine-bound AES-256-GCM token encryption, keyring watching, `Notify` launch handshakes, and model-pool rotation.
5. [Quota Observability and Token Economy](05-quota-observability-and-token-economy.md) — Statusline and transcript token/cache metrics, context health thresholds, rolling 5h/7d quota windows, and PTY-isolated warmup probes.
6. [Agent Session Snapshots](06-agent-session-snapshots.md) — Event-driven, domain-scoped (`code`, `pcbcraft`, `solidcraft`, `desigma`) Git safety snapshots under `refs/one-ship/snapshots/`.
7. [On-Demand Skills and Skill Manager](07-on-demand-skills.md) — Project-scoped `.agents/skills/` and `.agents/.skills-disabled/` JIT lifecycle, MCP skill tools, and persona skill directives.
8. [Agent Pixel Portrait DNA](08-agent-pixel-portrait-dna.md) — Deterministic `64×96` layered pixel portrait DNA format (`Bxx:Ex:Hxx:BDxx:Axx`) and Canvas compositor.
