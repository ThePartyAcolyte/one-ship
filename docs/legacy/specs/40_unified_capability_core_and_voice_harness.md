> [!WARNING]
> **LEGACY SPECIFICATION:** Superseded by [05-voice/04-unified-capability-core-and-voice-harness.md](../../specs/05-voice/04-unified-capability-core-and-voice-harness.md). Preserved for historical provenance only.

# Spec 40: Unified Capability Core & Voice Orchestrator Harness

**Document Version:** 0.2.1  
**Status:** DRAFT (Approved for Spec Authoring; Implementation Staged: Base + Pilot first)  
**Domain:** Unified Domain Capability Registry, Navigation-Decoupled Voice Orchestration, Persistent Voice Working Session Harness, 65K TPM Context Budget Control  
**Maintainer:** ONE-SHIP / Omarchy Development Team

---

## 1. Architectural Purpose & Scope

ONE-SHIP exposes overlapping workspace capabilities to two distinct AI consumers:

1. **Internal Coding & Specialist Agents (MCP Server — Spec 12):** Subprocesses running inside registered projects via `mcp.rs`, authenticated by OS-level process attestation (`CallerContext` via `/proc` + `ONESHIP_AGENT_INSTANCE`).
2. **Global Voice Controller (Ship Brain — Spec 36):** A project-independent spoken orchestrator connected over `BidiGenerateContent` (`voice_controller.rs`).

Prior to this specification, these two surfaces diverged and suffered from three structural defects:

1. **Parallel Duplication & Ownership Drift:** `mcp.rs` and `voice_controller.rs` independently re-implemented argument parsing and domain calls over `kopeng/*` and `db.rs`. This caused feature drift (`mcp.rs` remained on legacy `github_*` while `voice_controller.rs` used provider-neutral `kopeng::forge`; `devlog`/Journal existed only in MCP) and ownership gaps (e.g. `voice_controller.rs` `delete_task` / `delete_stage` / `*_subtask` accepted bare IDs without verifying `project_id` ownership).
2. **Perceived Coupling Between Navigation and Tool Access:** `UiSurfaceRegistry` (`ui_state.rs`) bundled UI navigation nodes with `UiSurfaceActionDef`, `ui_get_state` exposed only top-level `available_tabs`, 14 of 19 surface nodes declared zero actions, and `Frontend` actions (`OscilloscopeView.vue`) silently dropped `ui-surface-command` events when the target view was not mounted while returning `ok: true`.
3. **Stateless Voice Transport ("Black-Box" Amnesia & 65K TPM Risk):** `voice_controller.rs` treated each `/api/v1/voice/stream` WebSocket as an ephemeral scratch session. Closing the GUI, hitting `goAway`, or pausing work between voice commands wiped all conversational focus, transcripts, and pending operational items. Meanwhile, `setup` omitted `contextWindowCompression` and `sessionResumption`, leaving raw `toolResponse` JSON payloads to accumulate in the Live API input context against the 65,000 TPM ceiling.

This specification defines a unified two-layer architecture to resolve all three defects:

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                   1. VOICE ORCHESTRATOR HARNESS (Daemon)                    │
│     Persistent working session, scratchpad & TPM budget in oneship.db       │
│                                                                             │
│  ├── VoiceSessionStore (SQLite): working session, scratchpad, turn ledger   │
│  ├── Context Budget Manager: Context Brief hydration, response compaction,  │
│  │   contextWindowCompression, sessionResumption, idle-disconnect/reconnect │
│  └── Tier 1 Core Tools: ui_* (visual only), agent_*, harness_*              │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                    2. UNIFIED CAPABILITY CORE (Daemon)                      │
│   Inversion of Control: each Kopeng module & plugin declares its actions    │
│                                                                             │
│  CapabilityProvider trait / descriptors:                                    │
│  ├── Static domain actions + dynamic plugin descriptors (e.g. oscilloscope  │
│  │   drivers, profiler manifest.toml recipes, rula.toml scripts)            │
│  ├── Mandatory project_id resolution & cross-project ownership enforcement  │
│  ├── Visibility policy: Shared | McpOnly | VoiceOnly                        │
│  └── Safety policy: ReadOnly | Mutating | ConfirmRequired                   │
└──────────────────────────┬───────────────────────────────┬──────────────────┘
                           │                               │
                           ▼                               ▼
               [Voice Harness Tier 2]            [MCP Server (mcp.rs)]
               capability_discover(...)          tools/list + tools/call
               capability_execute(...)           (Shared + McpOnly under
               (Navigation-independent)           CallerContext /proc guard)
```

---

## 2. Core Invariants (Non-Negotiable)

1. **Navigation Is Strictly Visual (`ui_navigate`):** Navigating to a tab or subtab changes what the user sees on screen (`UiStateManager`, Spec 35). It is **never** a prerequisite for discovering or executing any backend domain capability. Cross-domain queries (e.g. checking `journal` minutes/TODOs while the user is viewing `project/board`) must execute directly in a single turn without switching tabs.
2. **Inversion of Control per Subsystem (`Kopeng`):** Capabilities are declared by the domain modules and plugin drivers that own them, not hardcoded inside `ui_state.rs` or `voice_controller.rs`. Hardware/recipe plugins (such as `OscilloscopeDescriptor` in `kopeng/electronics/instruments/driver.rs`) project dynamic capabilities from their live/configured descriptors.
3. **Single Source of Truth & Strict `project_id` Ownership (Tiger Style):** Shared domain handlers live once in the Capability Core. Every project-scoped action must validate `project_id > 0`, resolve the registered project explicitly, and verify entity ownership (`task.project_id == project_id`, `stage.project_id == project_id`, etc.) before mutating state.
4. **Zero Silent Fallbacks:** Invalid parameters, malformed `params_json`, missing projects, or unmounted `Frontend` targets must return structured errors (`CapabilityError` / `VoiceControllerError` implementing `api::ApiError`), never `json!({})` or false `{"ok": true}`.
5. **Privilege Separation (`Voice` vs. `MCP`):**
   * The Global Voice Controller is an **orchestrator and workspace operator**, not a code-authoring subprocess. Subprocess-only tools (`post_agent_response`, `request_approval`, `record_session_minutes`, `delegate_agent_task`, etc.) are `McpOnly`.
   * Workspace UI and agent lifecycle controls (`ui_*`, `agent_launch`, `agent_prompt`, `agent_stop`, `harness_*`) are `VoiceOnly`.
6. **Pre-1.0 Clean Replacement:** Once a subsystem is migrated to the Capability Core, legacy duplicated wrappers in `voice_controller.rs` or `mcp.rs` are deleted outright (zero compatibility shims).

---

## 3. Unified Capability Core (`src-tauri/src/capabilities/`)

### 3.1 Domain & Provider Model

A domain capability module registers a `CapabilityProvider` descriptor containing:

* **Domain ID (`&'static str`):** Canonical domain identifier (e.g. `"project"`, `"forge"`, `"journal"`, `"files"`, `"git"`, `"scripts"`, `"electronics"`, `"settings"`, `"skills"`, `"profiler"`, `"desigma"`, `"cad"`, `"pcb"`, `"rea"`, `"overview"`).
* **Required Capability (`Option<&'static str>`):** Optional project capability key matching `HydratedProject` (e.g. `"has_project_management_enabled"`, `"has_git"`, `"has_electronics_enabled"`), so disabled subsystems are neither advertised nor executable for that project.
* **Linked Surface Path (`Option<&'static str>`):** Optional default `UiSurfaceNode` path (e.g. `"project"`, `"devlog"`, `"electronics/oscilloscope"`) used purely as a visual hint when the voice controller or UI wants to optionally show a panel.
* **Static Capabilities (`Vec<CapabilityDef>`):** Fixed actions owned by the domain.
* **Dynamic Capabilities (`fn dynamic_capabilities(&HydratedProject) -> Vec<CapabilityDef>`):** Runtime-derived actions for extensible plugins (e.g. connected instrument descriptors via `OscilloscopeDescriptor`, `rula.toml` scripts, or `manifest.toml` profiler recipes).

### 3.2 `CapabilityDef` Schema

```rust
pub enum CapabilityVisibility {
    /// Exposed to both MCP coding agents and the Global Voice Controller.
    Shared,
    /// Exposed exclusively to managed MCP subprocesses (CallerContext).
    McpOnly,
    /// Exposed exclusively to the Global Voice Controller.
    VoiceOnly,
}

pub enum CapabilityRisk {
    /// Pure read / query; safe without verbal confirmation.
    ReadOnly,
    /// Creates or updates state reversibly.
    Mutating,
    /// Destructive or irreversible (e.g. delete_task, restore_snapshot, agent_stop);
    /// Voice Controller must obtain explicit user confirmation before invoking.
    ConfirmRequired,
}

pub enum CapabilityTarget {
    /// Executed directly inside oneship-daemon (async / spawn_blocking).
    Backend,
    /// Mutates a persisted UiSlotDef and/or requires an active desktop UI handler.
    FrontendSlot { slot_key: &'static str },
    /// Dispatches a live UI command; must verify active/mounted listener or fail explicitly.
    FrontendCommand { node_path: &'static str },
}
```

Each `CapabilityDef` declares:
* `id`: unique action name within the domain (e.g. `"list_tasks"`, `"query_session_journal"`).
* `mcp_name`: optional explicit MCP tool name when exposed via `tools/list` (e.g. `"manage_project_task"` or `"forge_list_issues"`).
* `description`: concise model-facing description (kept compact for Gemini Live TPM budget).
* `parameters_schema`: JSON Schema object.
* `visibility`: `Shared | McpOnly | VoiceOnly`.
* `risk`: `ReadOnly | Mutating | ConfirmRequired`.
* `target`: `Backend | FrontendSlot | FrontendCommand`.
* `voice_max_items`: optional item/byte cap applied automatically when projecting responses to the Voice Harness.

### 3.3 Visibility Policy Matrix

| Category | Visibility | Rationale |
| :--- | :--- | :--- |
| Subprocess Dialogue & Approvals (`post_agent_response`, `request_approval`, `request_user_approval`) | `McpOnly` | Voice controller speaks directly to the operator over audio; it is not a managed subprocess. |
| Code-Session Accounting (`record_session_minutes`, `switch_conversation_topic`, `complete_project_initialization`, `complete_project_removal`, `delegate_agent_task`) | `McpOnly` | Coding agents author formal implementation minutes and run project lifecycle workflows. Voice delegates via `agent_prompt`. |
| UI Presentation & Chat Panel Control (`ui_get_state`, `ui_select_project`, `ui_navigate`, `ui_*_agent_chat`) | `VoiceOnly` | Coding agents must never hijack the user's active tab or workspace focus. |
| Agent Lifecycle & Voice Harness (`agent_launch`, `agent_stop`, `agent_interrupt`, `agent_prompt`, `agent_new_conversation`, `agent_compact_context`, `harness_*`) | `VoiceOnly` | Operator-level orchestration and working memory control. |
| Workspace Config & Light File Nav (`settings.*`, `files.search_files`, `files.list_directory`, `files.open_file`) | `VoiceOnly` | Coding agents already have native filesystem tools and should not alter global/project `rula.toml` silently. |
| Domain Knowledge & Operations (`project.*`, `forge.*`, `journal.*` read/triage, `knowledge.*` hybrid search/AST/causal graph, `git`/snapshots, `scripts`/logs/audit, `electronics`, `profiler`, `skills`) | `Shared` | Both internal agents and the operator via voice need consistent, project-scoped access to the same truth. |

> [!NOTE]
> **Frontend Action Reliability Rule:** Actions targeting `FrontendCommand` (`oscilloscope.*`, `files.open_file`) must no longer fire-and-forget into an unmounted Vue component. State-bearing UI actions should either persist through `UiSlotDef` (`FrontendSlot`) backed by Pinia watchers, or track active view readiness / return an explicit `CapabilityError::ViewNotMounted` (or require explicit `ui_navigate` first when live canvas interaction is mandatory).

---

## 4. Voice Orchestrator Harness (`VoiceSessionHarness`)

### 4.1 Purpose: Persistent Working Session Beyond Ephemeral Sockets

In `BidiGenerateContent`, WebSocket connections are inherently volatile (idle pauses between user tasks, `goAway` rotation ~10 minutes, GUI reloads, or daemon restarts). The daemon therefore owns a persistent **Voice Working Session** decoupled from individual WebSocket connections.

By default, ONE-SHIP maintains **one active global working session** (matching the cross-project nature of the Global Voice Controller), where scratchpad items and turn entries are tagged with `project_id` whenever scoped to a specific project.

### 4.2 SQLite Persistence (`oneship.db`)

1. **`voice_sessions`:**
   * `id TEXT PRIMARY KEY` (UUID)
   * `status TEXT NOT NULL CHECK (status IN ('active', 'archived'))`
   * `active_project_id INTEGER REFERENCES projects(id) ON DELETE SET NULL`
   * `current_objective TEXT` (short operator-stated goal)
   * `last_delegated_project_id INTEGER`
   * `last_delegated_agent_id TEXT`
   * `resumption_handle TEXT` (latest `sessionResumptionUpdate.newHandle` from Gemini Live)
   * `resumption_valid_until_epoch_seconds INTEGER`
   * `rolling_summary TEXT NOT NULL DEFAULT ''` (deterministic compact summary of pruned turns)
   * `created_epoch_seconds INTEGER NOT NULL`
   * `updated_epoch_seconds INTEGER NOT NULL`
2. **`voice_scratchpad_items`:**
   * `id INTEGER PRIMARY KEY AUTOINCREMENT`
   * `session_id TEXT NOT NULL REFERENCES voice_sessions(id) ON DELETE CASCADE`
   * `project_id INTEGER REFERENCES projects(id) ON DELETE CASCADE` (nullable for cross-project notes)
   * `title TEXT NOT NULL`
   * `status TEXT NOT NULL CHECK (status IN ('pending', 'in_progress', 'done'))`
   * `note TEXT`
   * `created_epoch_seconds INTEGER NOT NULL`
   * `updated_epoch_seconds INTEGER NOT NULL`
3. **`voice_turn_ledger`:**
   * `id INTEGER PRIMARY KEY AUTOINCREMENT`
   * `session_id TEXT NOT NULL REFERENCES voice_sessions(id) ON DELETE CASCADE`
   * `project_id INTEGER`
   * `user_transcript TEXT`
   * `actions_summary TEXT` (compact JSON/text summary of capabilities executed & outcomes)
   * `assistant_transcript TEXT`
   * `created_epoch_seconds INTEGER NOT NULL`

All multi-step updates (e.g. appending a turn + pruning overflow into `rolling_summary`) execute inside an explicit SQLite `conn.transaction()?` ... `tx.commit()?`.

### 4.3 65K TPM Budget & Context Window Strategy

Because Gemini Live counts **accumulated input context** (system instruction + tool declarations + audio input + prior `toolResponse` JSONs) against the **65,000 TPM** ceiling on every turn, the harness enforces four complementary controls:

1. **Server-Side `contextWindowCompression` in `setup`:**
   * Enable `contextWindowCompression: { slidingWindow: { targetTokens: ... }, triggerTokens: ... }` configured via `[plugins.voice_controller]` in `config.rs` so Gemini truncates stale raw audio/tool history automatically before reaching session or TPM ceilings.
2. **Server-Side `sessionResumption` + Deterministic Brief Fallback:**
   * Send `sessionResumption: { handle }` when a non-expired `resumption_handle` exists in `voice_sessions`, and persist every `sessionResumptionUpdate` frame received from Gemini.
   * When starting a fresh Live socket (no handle, expired handle, or `resumption` rejected), the harness compiles a **Deterministic Context Brief** (~400–700 tokens max) appended to `systemInstruction`:
     * Active `project_id` / name and `current_objective`
     * Last delegated `(project_id, agent_id)`
     * Active `voice_scratchpad_items` (`pending` / `in_progress`, capped)
     * `rolling_summary` + last `N` entries from `voice_turn_ledger`
   * No secondary LLM call is required for summarization during compaction; overflow entries from `voice_turn_ledger` are folded deterministically (timestamp, project, intent, action IDs + short result) into `rolling_summary` (bounded by `max_rolling_summary_chars`).
3. **Two-Tier Compact Tool Surface (`setup.tools` <= ~3,000 tokens):**
   * Tier 1 fixed declarations stay minimal:
     * Navigation & UI (visual only): `ui_get_state`, `ui_select_project`, `ui_navigate`, `ui_open_agent_chat`, `ui_minimize_agent_chat`, `ui_restore_agent_chat`, `ui_close_agent_chat`
     * Agent Orchestration: `agent_launch`, `agent_stop`, `agent_interrupt`, `agent_prompt`, `agent_new_conversation`, `agent_compact_context`
     * Harness Working Memory: `harness_scratchpad` (`list | add | update | remove | clear`, plus `set_objective`), `harness_session_reset`
     * Navigation-Agnostic Capability Meta-Tools: `capability_discover(project_id, domain)` and `capability_execute(project_id, domain, action_id, params_json)`
   * `ui_get_state` returns the active UI snapshot, registered projects, and **the list of enabled capability domains per project** (respecting `HydratedProject` capabilities and `hidden_tabs`), so the model can discover any domain directly without calling `ui_navigate`.
4. **Unclipped Discovery, Explicit Pagination & Token Telemetry (`usageMetadata`):**
   * `capability_discover` returns every action and parameter description for the requested domain without silent truncation (projected for Voice as `{ id, description, risk, parameters }`).
   * List capabilities accept explicit `offset` and `limit` pagination parameters and return `{ total_count, offset, limit, has_more }` metadata rather than silently dropping records or clipping strings.
   * `voice_controller` logs `setup` payload bytes, every `toolResponse` byte size, explicit `WARN` diagnostics when exceeding `voice_tool_response_max_bytes` (without altering payloads), WebSocket close/error frames, and Gemini Live `usageMetadata` (`promptTokenCount`, `responseTokenCount`, `totalTokenCount`, modality details).

### 4.4 WebSocket Lifecycle & Idle Hibernation

* Holding an idle Live WebSocket indefinitely risks silent `goAway` drops and wasted state.
* After `idle_disconnect_seconds` without PTT activity or spoken output, the daemon closes the upstream Gemini WebSocket cleanly while keeping the `voice_sessions` record `active` in SQLite.
* On the next `ptt_start`, the harness lazily reconnects (using `resumption_handle` if valid, or rehydrating via the compiled Context Brief) before transitioning to `Listening`.

---

## 5. Configuration Schema (`config.rs`)

Add the following `global_only` fields to `[plugins.voice_controller]` in `get_all_plugin_schemas()` (`src-tauri/src/config.rs`):

| Key | Type | Default | Purpose |
| :--- | :--- | :---: | :--- |
| `idle_disconnect_seconds` | `integer` | `180` | Idle seconds before closing upstream Gemini Live WebSocket (working session stays persisted in SQLite). |
| `turn_ledger_max_entries_count` | `integer` | `12` | Active recent turns kept verbatim before folding oldest entries into `rolling_summary`. |
| `scratchpad_max_items_count` | `integer` | `20` | Maximum active items allowed in `voice_scratchpad_items` per working session. |
| `context_compression_trigger_tokens` | `integer` | `32000` | Gemini Live `contextWindowCompression.triggerTokens`. |
| `context_compression_target_tokens` | `integer` | `16000` | Gemini Live `contextWindowCompression.slidingWindow.targetTokens`. |
| `voice_tool_response_max_bytes` | `integer` | `6144` | Non-truncating `WARN` log threshold in bytes for serialized `toolResponse` payloads sent to Gemini Live. |

Invalid types or out-of-range values emit explicit `WARN` logs and typed errors per `AGENTS.md` §2.

---

## 6. Staged Implementation & Rollout Plan

Implementation proceeds in strictly scoped stages so the core harness can be verified before migrating all subsystems:

### Stage 1: Base Infrastructure + Pilot Subsystem (`project` + `journal` read)
1. **Persistence & Config:**
   * Add `voice_sessions`, `voice_scratchpad_items`, and `voice_turn_ledger` tables + transactional helpers in `db.rs`.
   * Add harness configuration knobs in `config.rs`.
2. **Voice Orchestrator Harness (`voice_harness.rs` / `voice_controller.rs`):**
   * Implement `VoiceSessionHarness` (Brief compiler, scratchpad CRUD, turn ledger + deterministic rolling summary folding, response compactor).
   * Wire `sessionResumption`, `contextWindowCompression`, and `sessionResumptionUpdate` persistence in `voice_controller.rs`.
   * Replace `surface_get_capabilities` / `surface_execute_action` with navigation-agnostic `capability_discover` / `capability_execute` + `harness_scratchpad` / `harness_session_reset`, and enrich `ui_get_state` with per-project enabled capability domains.
3. **Capability Core (`src-tauri/src/capabilities/`):**
   * Define `CapabilityProvider`, `CapabilityRegistry`, `CapabilityDef`, `CapabilityError` (`ApiError`), and strict `project_id` validation.
   * **Pilot Providers:**
     * `project` (`project_mgmt`: stages, tasks, subtasks, milestones, task relations — closing existing `project_id` ownership gaps on `delete_task`, `delete_stage`, `*_subtask`).
     * `journal` (`db`: read-only `query_session_journal`, `list_todos`, `list_technical_debt`, `search_conversations` for Voice + `Shared`; `record_session_minutes` remains `McpOnly`).
   * Wire `mcp.rs` to dispatch the migrated pilot `project` / `journal` tools through the shared Capability Core while preserving `CallerContext` attestation.
4. **Verification Gate:**
   * Unit tests: cross-project `project_id` ownership rejection, `VoiceSessionHarness` brief token/byte bounds, deterministic rolling-summary folding, response truncation, and `api_commands_match_the_frontend_map`.
   * Runtime verification (user-tested): while focused on `project/board`, ask the Global Voice Controller to check `journal` / `todos` for completed work, update a task/scratchpad item, pause/reconnect, and verify continuity without unintended tab navigation.

### Stage 2: Incremental Subsystem Migration (Post-Pilot)
Once Stage 1 is validated:
1. **Stage 2A — Forge, Files & Settings (`forge`, `files`, `settings`) [IMPLEMENTED]:**
   * `capabilities/forge.rs` (`domain = "forge"`, `linked_surface_path = Some("project/forge")`) unifies GitHub & GitLab collaboration + inbox triage (`get_context`, `list_issues`, `get_issue`, `list_prs`, `get_pr`, `list_releases`, `list_inbox`, `mark_item_read`, `set_item_flag`, `get_unread_counts`) and replaces legacy `github_*` / `propose_github_draft_pr` MCP tools with provider-neutral `forge_*` / `propose_forge_draft_pr` tools in `mcp.rs` and `templates/skills/oneship-project-mgmt/SKILL.md`.
   * `capabilities/files.rs` (`domain = "files"`) unifies `search_files`, `list_directory`, `read_file_excerpt`, slot-backed `set_scope`, and slot+event `open_file` (with `offset`/`limit` pagination on directory/file listings).
   * `capabilities/knowledge.rs` (`domain = "knowledge"`, `linked_surface_path = Some("knowledge")`) unifies the refactored hybrid semantic + causal graph engine: `search_knowledge` (BGE-M3 prose search across `specs`, `journal`, `tasks`, `tech_debt` + deterministic Tree-sitter AST code lookup + 1-hop causal relations), `resolve_symbol_definition` (zero-ONNX SQLite `project_code_symbols`), `get_file_symbols`, `get_file_line_insights` (causal `minutes` from `journal_file_hunks` + anchored `debt_and_todos` + `related_docs`), `get_indexing_status`, `index_project`, and `rebuild_graph`.
   * `mcp.rs` dispatches both `search_knowledge` (with `max_relations_per_result` and `min_relation_score_percent`) and `get_file_line_insights` through `call_mcp_capability`.
   * `capabilities/settings.rs` (`domain = "settings"`) unifies `get_project_config_schema` and strict-JSON `set_project_config_value`.
   * Removed all duplicated `settings`, `project/forge`, and `files` arms from `voice_controller.rs::execute_surface_action`.
2. **Stage 2B — Git, Snapshots, Scripts & Logs (`git`, `scripts`, `logs`) [IMPLEMENTED]:**
   * `capabilities/git.rs` (`domain = "git"`, `required_capability = Some("has_git")`, `linked_surface_path = Some("git")`) exposes `get_status`, `list_commits`, `get_working_tree_diff`, `list_branches`, `list_snapshots`, `create_snapshot`, and `restore_snapshot` (`ConfirmRequired`).
   * `capabilities/scripts.rs` (`domain = "scripts"`, `linked_surface_path = Some("overview")`) exposes `list_scripts`, `start_task`, `query_task_status`, `cancel_task` (`ConfirmRequired`), `manage_service`, and `inspect_terminal` with strict `project_id` ownership verification.
   * `capabilities/logs.rs` (`domain = "logs"`) exposes read-only `query_system_logs` (`project` | `system` scope), `query_project_audit`, and `query_agents`.
   * Routed all 12 corresponding MCP tools (`start_project_task`, `query_project_task`, `cancel_project_task`, `list_project_scripts`, `manage_project_script`, `inspect_terminal_output`, `query_system_logs`, `query_project_agents`, `query_project_audit`, `create_snapshot`, `list_snapshots`, `restore_snapshot`) through `call_mcp_capability` in `mcp.rs`.
3. **Stage 2C — Dynamic Hardware/Plugin Subsystems (`electronics`, `profiler`, `skills`) & Legacy Cleanup [IMPLEMENTED]:**
   * `capabilities/electronics.rs` (`domain = "electronics"`, `required_capability = Some("has_electronics_enabled")`, `linked_surface_path = Some("electronics/oscilloscope")`) builds `dynamic_actions` from `OscilloscopeDescriptor` (`owon_vds1022::descriptor()`), persists oscilloscope configuration in `ui_state` slots (`electronics.oscilloscope_*`), syncs with `workspaceLayoutStore` / `OscilloscopeView.vue`, and executes direct backend hardware captures via `capture_oscilloscope`.
   * `capabilities/profiler.rs` (`domain = "profiler"`, `required_capability = Some("has_profiler_enabled")`) and `capabilities/skills.rs` (`domain = "skills"`), plus `journal.rs::record_technical_debt`, unify the remaining profiler, skills, TODO, technical-debt, and conversation-search MCP handlers through `call_mcp_capability`.
   * Completely deleted `execute_surface_action` and all legacy surface-node fallbacks from `voice_controller.rs`.
   * **Knowledge & Causal Graph Completion [IMPLEMENTED]:** Migrated `mcp.rs::search_knowledge` onto `call_mcp_capability`, eliminated `execute_search_knowledge`, exposed `get_file_line_insights` to MCP & Voice, and consolidated hybrid search / AST symbol lookup / causal insights / index management inside `capabilities/knowledge.rs`.
4. **Cleanup:** Retire residual `SYSTEM_AGENT_ID` / `SystemAgentHasNoProject` assumptions where superseded by the Global Voice Controller (Spec 36 / Spec 12 alignment).

---

## 7. References

* [Spec 06: ONE-SHIP Plugin Architecture](06_plugin_architecture.md)
* [Spec 12: Streamable HTTP MCP Daemon](12_streamable_http_mcp_daemon.md)
* [Spec 35: UI State Machine, Daemon Synchronization & Session Persistence](35_ui_state_machine_and_session_persistence.md)
* [Spec 36: Global Voice Controller](36_global_voice_controller.md)
* [Spec 39: Forge Provider Abstraction & Project Surface](39_forge_provider_and_project_surface_handoff.md)
