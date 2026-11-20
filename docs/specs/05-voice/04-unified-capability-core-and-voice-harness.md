# Unified Capability Core and Voice Orchestrator Harness

## Purpose

Defines the two-layer daemon architecture that unifies workspace capabilities across ONE-SHIP's two AI consumers—managed coding/specialist subprocesses via MCP (`mcp.rs`) and the Global Voice Controller (`voice_controller.rs`)—while providing a persistent, TPM-budgeted working session harness for voice orchestration:

1. **Eliminates parallel duplication and ownership drift:** shared domain handlers live once in the Unified Capability Core instead of being re-implemented separately in `mcp.rs` and `voice_controller.rs`, enforcing uniform `project_id` ownership verification.
2. **Decouples domain tool access from UI navigation:** `ui_navigate` changes only what the operator sees; cross-domain queries and actions execute directly without switching tabs or mounting views.
3. **Eliminates voice session amnesia and 65K TPM overflow:** `VoiceSessionHarness` persists working objectives, scratchpad items, turn history, and Gemini `resumption_handle` state in `oneship.db`, applying `contextWindowCompression`, `sessionResumption`, deterministic Context Brief hydration, and response telemetry.

| Property | Value |
| :--- | :--- |
| Document version | 0.2.1 |
| Domain | Unified domain capability registry, navigation-decoupled voice orchestration, persistent voice working session harness, 65K TPM context budget control |
| Maintainer | ONE-SHIP / Omarchy development team |

## Scope

In scope:

* Core invariants governing navigation decoupling, Inversion of Control, `project_id` ownership, error handling, privilege separation (`Shared` / `McpOnly` / `VoiceOnly`), and pre-1.0 shim deletion.
* `CapabilityProvider`, `CapabilityDef`, `CapabilityVisibility`, `CapabilityRisk`, `CapabilityTarget`, `CapabilityError`, visibility policy matrix, and frontend action reliability rules (`src-tauri/src/capabilities/`).
* `VoiceSessionHarness` (`src-tauri/src/voice_harness.rs`), SQLite tables (`voice_sessions`, `voice_scratchpad_items`, `voice_turn_ledger`), 65K TPM budget controls, Tier 1 harness/capability tools, unclipped discovery, explicit pagination, and WebSocket idle hibernation.
* `[plugins.voice_controller]` harness configuration keys (`src-tauri/src/config.rs`).
* Staged migration architecture across domain subsystems (`project`, `journal`, `forge`, `files`, `knowledge`, `settings`, `git`, `scripts`, `logs`, `electronics`, `profiler`, `skills`).

Out of scope:

* PipeWire audio transport, push-to-talk keybindings, and voice persona/model configuration (see [Global Voice Controller](03-global-voice-controller.md)).
* Native Gemini Live dictation in agent chat inputs and focused fields (see [Native Gemini Dictation](02-dictation-and-agent-chat.md)).
* Internal MCP HTTP transport and `/proc` subprocess attestation internals (covered in the Streamable HTTP MCP Daemon specification).

## Model / Design

### Two-Layer Architecture

ONE-SHIP exposes overlapping workspace capabilities to two distinct AI consumers:

1. **Internal coding and specialist agents (MCP Server):** subprocesses running inside registered projects via `mcp.rs`, authenticated by OS-level process attestation (`CallerContext` via `/proc` + `ONESHIP_AGENT_INSTANCE`).
2. **Global Voice Controller (Ship Brain):** a project-independent spoken orchestrator connected over `BidiGenerateContent` (`voice_controller.rs`).

Prior to this architecture, the two surfaces suffered from: (1) parallel duplication and ownership drift (`mcp.rs` still on legacy `github_*`, `devlog`/Journal missing from voice, bare IDs accepted on `delete_task`/`delete_stage`/`*_subtask` without `project_id` ownership verification), (2) perceived coupling between UI navigation and tool access (`UiSurfaceRegistry` bundling navigation nodes with `UiSurfaceActionDef`, `ui_get_state` exposing only top-level `available_tabs`, 14 of 19 surface nodes declaring zero actions, and `Frontend` actions silently dropping `ui-surface-command` when unmounted while returning `ok: true`), and (3) stateless voice transport ("black-box" amnesia on GUI close, `goAway`, or idle pauses, plus missing `contextWindowCompression` and `sessionResumption` against the 65,000 TPM ceiling).

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

### Core Invariants (Non-Negotiable)

1. **Navigation is strictly visual (`ui_navigate`):** navigating to a tab or subtab changes what the user sees on screen (`UiStateManager`). It is **never** a prerequisite for discovering or executing any backend domain capability. Cross-domain queries (e.g., checking `journal` minutes/TODOs while viewing `project/board`) execute directly in a single turn without switching tabs.
2. **Inversion of Control per subsystem (Kopeng):** capabilities are declared by the domain modules and plugin drivers that own them, not hardcoded inside `ui_state.rs` or `voice_controller.rs`. Hardware/recipe plugins (such as `OscilloscopeDescriptor` in `kopeng/electronics/instruments/driver.rs`) project dynamic capabilities from their live/configured descriptors.
3. **Single source of truth and strict `project_id` ownership (Tiger Style):** shared domain handlers live once in the Capability Core. Every project-scoped action must validate `project_id > 0`, resolve the registered project explicitly, and verify entity ownership (`task.project_id == project_id`, `stage.project_id == project_id`, etc.) before mutating state.
4. **Zero silent fallbacks:** invalid parameters, malformed `params_json`, missing projects, or unmounted `Frontend` targets must return structured errors (`CapabilityError` / `VoiceControllerError` implementing `api::ApiError`), never `json!({})` or false `{"ok": true}`.
5. **Privilege separation (`Voice` vs. `MCP`):**
   * The Global Voice Controller is an **orchestrator and workspace operator**, not a code-authoring subprocess. Subprocess-only tools (`post_agent_response`, `request_approval`, `record_session_minutes`, `delegate_agent_task`, etc.) are `McpOnly`.
   * Workspace UI and agent lifecycle controls (`ui_*`, `agent_launch`, `agent_prompt`, `agent_stop`, `harness_*`) are `VoiceOnly`.
6. **Pre-1.0 clean replacement:** once a subsystem is migrated to the Capability Core, legacy duplicated wrappers in `voice_controller.rs` or `mcp.rs` are deleted outright (zero compatibility shims).

### Unified Capability Core (`src-tauri/src/capabilities/`)

#### Domain and Provider Model

Each domain capability module registers a `CapabilityProvider` descriptor containing:

* **Domain ID (`&'static str`):** canonical domain identifier (e.g., `"project"`, `"forge"`, `"journal"`, `"files"`, `"knowledge"`, `"settings"`, `"git"`, `"scripts"`, `"logs"`, `"electronics"`, `"profiler"`, `"skills"`, as well as future workstation domains such as `"desigma"`, `"cad"`, `"pcb"`, `"rea"`, `"overview"`).
* **Required Capability (`Option<&'static str>`):** optional project capability key matching `HydratedProject` (e.g., `"has_project_management_enabled"`, `"has_git"`, `"has_electronics_enabled"`), so disabled subsystems are neither advertised nor executable for that project. UI tab visibility (`hidden_tabs`) governs `ui_navigate` only, never backend domain capabilities.
* **Linked Surface Path (`Option<&'static str>`):** optional default `UiSurfaceNode` path (e.g., `"project"`, `"devlog"`, `"electronics/oscilloscope"`) used purely as a visual hint when the voice controller or UI wants to optionally show a panel.
* **Static Capabilities (`fn() -> Vec<CapabilityDef>`):** fixed actions owned by the domain.
* **Dynamic Capabilities (`fn(&HydratedProject) -> Vec<CapabilityDef>`):** runtime-derived actions for extensible plugins (e.g., connected instrument descriptors via `OscilloscopeDescriptor`, `rula.toml` scripts, or `manifest.toml` profiler recipes).

#### `CapabilityDef` Schema

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

* `id`: unique action name within the domain (e.g., `"list_tasks"`, `"query_session_journal"`).
* `mcp_name`: optional explicit MCP tool name when exposed via `tools/list` (e.g., `"manage_project_task"` or `"forge_list_issues"`).
* `description`: concise model-facing description (kept compact for Gemini Live TPM budget).
* `parameters_schema`: JSON Schema object.
* `visibility`: `Shared | McpOnly | VoiceOnly`.
* `risk`: `ReadOnly | Mutating | ConfirmRequired`.
* `target`: `Backend | FrontendSlot | FrontendCommand`.
* `voice_max_items`: optional item/byte cap applied automatically when projecting responses to the Voice Harness.

#### Visibility Policy Matrix

| Category | Visibility | Rationale |
| :--- | :--- | :--- |
| Subprocess Dialogue & Approvals (`post_agent_response`, `request_approval`, `request_user_approval`) | `McpOnly` | Voice controller speaks directly to the operator over audio; it is not a managed subprocess. |
| Code-Session Accounting (`record_session_minutes`, `switch_conversation_topic`, `complete_project_initialization`, `complete_project_removal`, `delegate_agent_task`) | `McpOnly` | Coding agents author formal implementation minutes and run project lifecycle workflows. Voice delegates via `agent_prompt`. |
| UI Presentation & Chat Panel Control (`ui_get_state`, `ui_select_project`, `ui_navigate`, `ui_*_agent_chat`) | `VoiceOnly` | Coding agents must never hijack the user's active tab or workspace focus. |
| Agent Lifecycle & Voice Harness (`agent_launch`, `agent_stop`, `agent_interrupt`, `agent_prompt`, `agent_new_conversation`, `agent_compact_context`, `harness_*`) | `VoiceOnly` | Operator-level orchestration and working memory control. |
| Workspace Config & Light File Nav (`settings.*`, `files.search_files`, `files.list_directory`, `files.open_file`) | `VoiceOnly` | Coding agents already have native filesystem tools and should not alter global/project `rula.toml` silently. |
| Domain Knowledge & Operations (`project.*`, `forge.*`, `journal.*` read/triage, `knowledge.*` hybrid search/AST/causal graph, `git`/snapshots, `scripts`/logs/audit, `electronics`, `profiler`, `skills`) | `Shared` | Both internal agents and the operator via voice need consistent, project-scoped access to the same truth. |

> [!NOTE]
> **Frontend Action Reliability Rule:** Actions targeting `FrontendCommand` (`oscilloscope.*`, `files.open_file`) must not fire-and-forget into an unmounted Vue component. State-bearing UI actions should either persist through `UiSlotDef` (`FrontendSlot`) backed by Pinia watchers, or track active view readiness and return an explicit `CapabilityError::ViewNotMounted` (or require explicit `ui_navigate` first when live canvas interaction is mandatory).

### Voice Orchestrator Harness (`VoiceSessionHarness`)

#### Persistent Working Session

In `BidiGenerateContent`, WebSocket connections are inherently volatile (idle pauses between user tasks, `goAway` rotation ~10 minutes, GUI reloads, or daemon restarts). The daemon therefore owns a persistent **Voice Working Session** decoupled from individual WebSocket connections.

By default, ONE-SHIP maintains **one active global working session** (matching the cross-project nature of the Global Voice Controller), where scratchpad items and turn entries are tagged with `project_id` whenever scoped to a specific project.

#### 65K TPM Budget and Context Window Strategy

Because Gemini Live counts **accumulated input context** (system instruction + tool declarations + audio input + prior `toolResponse` JSONs) against the **65,000 TPM** ceiling on every turn, the harness enforces four complementary controls:

1. **Server-side `contextWindowCompression` in `setup`:**
   * Enables `contextWindowCompression: { triggerTokens, slidingWindow: { targetTokens } }` configured via `[plugins.voice_controller]` in `config.rs` so Gemini truncates stale raw audio/tool history automatically before reaching session or TPM ceilings.
2. **Server-side `sessionResumption` + Deterministic Brief Fallback:**
   * Sends `sessionResumption: { handle }` when a non-expired `resumption_handle` exists in `voice_sessions` (`RESUMPTION_HANDLE_TTL_SECONDS = 600`), and persists every `sessionResumptionUpdate` frame (`resumable: true`, `newHandle`) received from Gemini. If a handle is rejected before `setupComplete` or closed with `not found` / `expired` / `inactivity`, it is cleared and falls back cleanly.
   * When starting a fresh Live socket (no handle, expired handle, or `resumption` rejected), the harness compiles a **Deterministic Context Brief** (~400–700 tokens max, capped at `MAX_BRIEF_CHARS = 2400`) appended to `systemInstruction`:
     * Active `project_id` / name and `current_objective`
     * Last delegated `(project_id, agent_id)`
     * Active `voice_scratchpad_items` (`pending` / `in_progress`, capped at 8 items in the brief)
     * `rolling_summary` + last `N` entries from `voice_turn_ledger` (up to `min(turn_ledger_max_entries_count, 8)` queried, up to 5 rendered)
   * No secondary LLM call is required for summarization during compaction; overflow entries from `voice_turn_ledger` are folded deterministically (timestamp, project, intent, action IDs + short result) into `rolling_summary` (bounded by `MAX_ROLLING_SUMMARY_CHARS = 1600`).
3. **Two-Tier Compact Tool Surface (`setup.tools` <= ~3,000 tokens):**
   * Tier 1 fixed declarations stay minimal:
     * Navigation and UI (visual only): `ui_get_state`, `ui_select_project`, `ui_navigate`, `ui_open_agent_chat`, `ui_minimize_agent_chat`, `ui_restore_agent_chat`, `ui_close_agent_chat`
     * Agent orchestration: `agent_launch`, `agent_stop`, `agent_interrupt`, `agent_prompt`, `agent_new_conversation`, `agent_compact_context`, `project_search_skills`
     * Harness working memory: `harness_scratchpad` (`list | set_objective | add | update | remove | clear`), `harness_session_reset`
     * Navigation-agnostic capability meta-tools: `capability_discover(project_id, domain)` and `capability_execute(project_id, domain, action_id, params_json)`
   * `ui_get_state` returns the active UI snapshot, registered projects, and **the list of enabled capability domains per project** (`available_domains`, respecting `HydratedProject` capabilities independent of `hidden_tabs`), so the model can discover any domain directly without calling `ui_navigate`.
4. **Unclipped Discovery, Explicit Pagination, and Token Telemetry (`usageMetadata`):**
   * `capability_discover` returns every action and parameter description for the requested domain without silent truncation (projected for Voice as `{ id, description, risk, parameters }`).
   * List capabilities accept explicit `offset` and `limit` pagination parameters and return `{ <list_key>, total_count, offset, limit, has_more }` metadata rather than silently dropping records or clipping strings.
   * `voice_controller` logs `setup` payload bytes, every `toolResponse` byte size, explicit `WARN` diagnostics when exceeding `voice_tool_response_max_bytes` (without altering payloads), WebSocket close/error frames, and Gemini Live `usageMetadata` (`promptTokenCount`, `responseTokenCount`, `totalTokenCount`, modality details).

#### WebSocket Lifecycle and Idle Hibernation

* Holding an idle Live WebSocket indefinitely risks silent `goAway` drops and wasted state.
* After `idle_disconnect_seconds` without PTT activity or spoken output while in `Idle`, the daemon closes the upstream Gemini WebSocket cleanly while keeping the `voice_sessions` record `active` in SQLite.
* On the next `ptt_start`, the harness lazily reconnects (using `resumption_handle` if valid, or rehydrating via the compiled Context Brief) before transitioning to `Listening`.

### Subsystem Migration Scope

Subsystem migration onto the Unified Capability Core proceeds in scoped stages:

1. **Stage 1 — Base Infrastructure & Pilot (`project`, `journal` read):**
   * `voice_sessions`, `voice_scratchpad_items`, and `voice_turn_ledger` tables + transactional helpers in `db.rs`; harness config in `config.rs`; `VoiceSessionHarness` in `voice_harness.rs`.
   * Capability Core (`capabilities/mod.rs`) with pilot providers `project` (stages, tasks, subtasks, milestones, task relations — closing `project_id` ownership gaps on `delete_task`, `delete_stage`, `*_subtask`) and `journal` (read-only `query_session_journal`, `list_todos`, `list_technical_debt`, `search_conversations` for Voice + `Shared`; `record_session_minutes` remains `McpOnly`), plus MCP dispatch through `call_mcp_capability`.
2. **Stage 2A — Forge, Files, Knowledge & Settings (`forge`, `files`, `knowledge`, `settings`):**
   * `capabilities/forge.rs` (`domain = "forge"`, `linked_surface_path = Some("project/forge")`): provider-neutral GitHub & GitLab collaboration + inbox triage (`get_context`, `list_issues`, `get_issue`, `list_prs`, `get_pr`, `list_releases`, `list_inbox`, `mark_item_read`, `set_item_flag`, `get_unread_counts`), replacing legacy `github_*` / `propose_github_draft_pr` MCP tools with `forge_*` / `propose_forge_draft_pr`.
   * `capabilities/files.rs` (`domain = "files"`): `search_files`, `list_directory`, `read_file_excerpt`, slot-backed `set_scope`, and slot+event `open_file` (with `offset`/`limit` pagination).
   * `capabilities/knowledge.rs` (`domain = "knowledge"`, `linked_surface_path = Some("knowledge")`): hybrid semantic + causal graph engine (`search_knowledge` across `specs`, `journal`, `tasks`, `tech_debt` + Tree-sitter AST lookup + 1-hop causal relations, `resolve_symbol_definition`, `get_file_symbols`, `get_file_line_insights`, `get_indexing_status`, `index_project`, `rebuild_graph`), with `mcp.rs` dispatching `search_knowledge` and `get_file_line_insights` via `call_mcp_capability`.
   * `capabilities/settings.rs` (`domain = "settings"`): `get_project_config_schema` and strict-JSON `set_project_config_value`.
3. **Stage 2B — Git, Snapshots, Scripts & Logs (`git`, `scripts`, `logs`):**
   * `capabilities/git.rs` (`domain = "git"`, `required_capability = Some("has_git")`, `linked_surface_path = Some("git")`): `get_status`, `list_commits`, `get_working_tree_diff`, `list_branches`, `list_snapshots`, `create_snapshot`, `restore_snapshot` (`ConfirmRequired`).
   * `capabilities/scripts.rs` (`domain = "scripts"`, `linked_surface_path = Some("overview")`): `list_scripts`, `start_task`, `query_task_status`, `cancel_task` (`ConfirmRequired`), `manage_service`, `inspect_terminal` with strict `project_id` ownership verification.
   * `capabilities/logs.rs` (`domain = "logs"`): read-only `query_system_logs` (`project` | `system` scope), `query_project_audit`, `query_agents`.
   * Routes all 12 corresponding MCP tools (`start_project_task`, `query_project_task`, `cancel_project_task`, `list_project_scripts`, `manage_project_script`, `inspect_terminal_output`, `query_system_logs`, `query_project_agents`, `query_project_audit`, `create_snapshot`, `list_snapshots`, `restore_snapshot`) through `call_mcp_capability`.
4. **Stage 2C — Dynamic Hardware/Plugin Subsystems (`electronics`, `profiler`, `skills`) & Legacy Cleanup:**
   * `capabilities/electronics.rs` (`domain = "electronics"`, `required_capability = Some("has_electronics_enabled")`, `linked_surface_path = Some("electronics/oscilloscope")`): dynamic actions from `OscilloscopeDescriptor` (`owon_vds1022::descriptor()`), persists oscilloscope configuration in `ui_state` slots (`electronics.oscilloscope_*`), syncs with `workspaceLayoutStore` / `OscilloscopeView.vue`, and executes direct backend captures (`capture_single`).
   * `capabilities/profiler.rs` (`domain = "profiler"`, `required_capability = Some("has_profiler_enabled")`) and `capabilities/skills.rs` (`domain = "skills"`), plus `journal.rs::record_technical_debt`, unify profiler, skills, TODO, technical-debt, and conversation-search MCP handlers through `call_mcp_capability`.
   * Deletes `execute_surface_action` and legacy surface-node fallbacks from `voice_controller.rs`, and retires residual `SYSTEM_AGENT_ID` / `SystemAgentHasNoProject` assumptions where superseded by the Global Voice Controller.

## Contracts

### SQLite Persistence Schema (`oneship.db`)

All multi-step updates (such as appending a turn and pruning overflow into `rolling_summary`) execute inside an explicit `conn.transaction()?` ... `tx.commit()?`.

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

### Tier 1 Harness and Capability Tools (`NON_BLOCKING`)

| Tool | Arguments | Purpose |
| :--- | :--- | :--- |
| `harness_scratchpad` | `action` (`"list" \| "set_objective" \| "add" \| "update" \| "remove" \| "clear"`), `objective?`, `project_id?`, `item_id?`, `title?`, `status?` (`"pending" \| "in_progress" \| "done"`), `note?`, `done_only?` | Reads or mutates persistent working session objective and scratchpad items. |
| `harness_session_reset` | None | Archives the active `voice_sessions` row and starts a clean session (only on explicit operator request). |
| `capability_discover` | `project_id` (`INTEGER`), `domain` (`STRING`) | Returns unclipped `{ project_id, domain, label, linked_surface_path, action_count, actions }` projected for `Voice` as `{ id, description, risk, parameters }`. |
| `capability_execute` | `project_id` (`INTEGER`), `domain` (`STRING`), `action_id` (`STRING`), `params_json?` (`STRING`) | Executes any capability allowed for `CapabilityCaller::Voice` on `project_id` without changing UI tabs. |

### Pagination Envelope and Typed Errors (`CapabilityError`)

* **Pagination envelope (`paginate_items`):** `{ <list_key>: [...], total_count: usize, offset: usize, limit: Option<usize>, has_more: bool }`.
* **`CapabilityError` (`ApiError` HTTP status mapping):**
  * `400 Bad Request`: `InvalidProjectId`, `InvalidParams`
  * `404 Not Found`: `ProjectNotFound`, `UnknownDomain`, `UnknownAction`, `NotFound`
  * `403 Forbidden`: `ForbiddenForCaller`, `OwnershipMismatch`
  * `409 Conflict`: `CapabilityNotEnabled`, `Conflict`
  * `500 Internal Server Error`: `Database`, `ProjectMgmt`, `Registry`, `Task`, plus delegated status/codes from domain errors (`Forge`, `Config`, `Semantic`, `UiState`, `Git`, `Snapshot`, `Runner`, `Script`, `Orchestrator`, `Supervisor`, `Electronics`, `Profiler`, `Skill`, `Io`).

### Configuration Schema (`[plugins.voice_controller]`, `scope = "global_only"`)

| Key | Type | Default | Purpose |
| :--- | :--- | :---: | :--- |
| `idle_disconnect_seconds` | `number` (`integer`) | `180` | Idle seconds before closing upstream Gemini Live WebSocket (working session stays persisted in SQLite). |
| `turn_ledger_max_entries_count` | `number` (`integer`) | `12` | Active recent turns kept verbatim before folding oldest entries into `rolling_summary`. |
| `scratchpad_max_items_count` | `number` (`integer`) | `20` | Maximum active items allowed in `voice_scratchpad_items` per working session. |
| `context_compression_trigger_tokens` | `number` (`integer`) | `32000` | Gemini Live `contextWindowCompression.triggerTokens`. |
| `context_compression_target_tokens` | `number` (`integer`) | `16000` | Gemini Live `contextWindowCompression.slidingWindow.targetTokens`. |
| `voice_tool_response_max_bytes` | `number` (`integer`) | `6144` | Non-truncating `WARN` log threshold in bytes for serialized `toolResponse` payloads sent to Gemini Live. |

Invalid types or out-of-range values emit explicit `WARN` logs and typed errors.

## Implementation Status

| Item | Status | Evidence |
| :--- | :--- | :--- |
| SQLite tables (`voice_sessions`, `voice_scratchpad_items`, `voice_turn_ledger`) & transactional folding | Implemented | `src-tauri/src/db.rs` |
| `VoiceSessionHarness` (Brief compiler, scratchpad CRUD, session reset, TTL resumption handle) | Implemented | `src-tauri/src/voice_harness.rs` |
| `contextWindowCompression`, `sessionResumption`, `usageMetadata` logging, idle hibernation, `capability_*` & `harness_*` tools | Implemented | `src-tauri/src/voice_controller.rs` |
| `[plugins.voice_controller]` harness config keys (`global_only`) | Implemented | `src-tauri/src/config.rs` |
| Unified Capability Core (`CapabilityProvider`, `CapabilityDef`, `CapabilityError`, `paginate_items`) | Implemented | `src-tauri/src/capabilities/mod.rs` |
| 12 migrated domain providers (`project`, `journal`, `forge`, `files`, `knowledge`, `settings`, `git`, `scripts`, `logs`, `electronics`, `profiler`, `skills`) & MCP `call_mcp_capability` dispatch | Implemented | `src-tauri/src/capabilities/*.rs`, `src-tauri/src/mcp.rs` |
| Deletion of `execute_surface_action` and legacy surface-node fallbacks from `voice_controller.rs` | Implemented | `src-tauri/src/voice_controller.rs` |
| `FrontendCommand` active-view readiness check (`CapabilityError::ViewNotMounted`) | Partial / Roadmap (`electronics.rs` and `files.rs` emit `ui_surface_command` without checking view mount state; `ViewNotMounted` is not yet in `CapabilityError`) | `src-tauri/src/capabilities/mod.rs`, `src-tauri/src/capabilities/electronics.rs`, `src-tauri/src/capabilities/files.rs` |
| Additional domain providers (`desigma`, `cad`, `pcb`, `rea`, `overview`) | Roadmap (not registered in `PROVIDERS`) | `src-tauri/src/capabilities/mod.rs` |

## Related Specs

* [Global Voice Controller](03-global-voice-controller.md)
* [Dictation and Agent Chat Integration](02-dictation-and-agent-chat.md)
* [Plugin Architecture (Kopeng)](../01-architecture/01-plugin-architecture.md)
* [Configuration Schema and Cascade](../01-architecture/02-configuration-schema-and-cascade.md)
* [README](README.md)

Supersedes legacy: `40_unified_capability_core_and_voice_harness.md`
