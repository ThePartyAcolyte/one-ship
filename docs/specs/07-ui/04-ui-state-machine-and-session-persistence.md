# UI State Machine and Session Persistence

## Purpose

Defines the daemon-owned UI state machine, declarative workspace surface tree registry, agent chat panel lifecycle, SQLite persistence schema, and bidirectional synchronization protocol between `oneship-daemon` and the Vue/Tauri frontend. Decoupling workspace navigation from the agent chat lifecycle enables exact session recovery across restarts and deterministic control by external automation such as the Global Voice Controller.

## Scope

In scope: the two-layer hybrid state model (`GlobalUiState`, `AgentChatState`, and `UiSurfaceRegistry`), the agent chat finite state machine, transition actions (`UiAction`), strict validation vs. boot-time sanitization, `oneship.db` schema, daemon IPC/SSE contracts, and frontend projection rules (including the unsaved-change guard).

Out of scope: visual panel geometry and layout recipes ([Workspace Chrome and Panel Contracts](03-workspace-chrome-and-panel-contracts.md)), per-widget overview aggregation ([Project Settings and Overview Dashboard](05-project-settings-and-overview-dashboard.md)), and the underlying agent conversation engine.

## Model / Design

### Architectural Principles

1. **Single Source of Truth in Daemon:** `oneship-daemon` owns the canonical UI state in memory and persists it to SQLite (`oneship.db`). The Vue/Tauri frontend acts strictly as a reactive projection of the daemon snapshot and dispatches explicit transition actions.
2. **Exact Session Recovery:** Closing and reopening the desktop interface restores the exact active project, workspace tab/subtab path, subsystem slot values (e.g., open file, explorer scope, active artifact), and agent chat panel state (`closed` / `open` / `minimized`, active agent, active conversation).
3. **External Control Readiness:** State ownership and transition validation in the daemon allow external controllers (such as the Global Voice Controller) to inspect valid navigation paths and surface actions for any project and execute deterministic UI transitions.
4. **Removal of Legacy `activeWorkspace`:** The obsolete `'dashboard' | 'workspace'` switcher is removed; ONE-SHIP operates exclusively in the project workspace with a global project selector.
5. **Unsaved-Change Guard (Frontend):** Navigation transitions that leave an editable panel with dirty state are intercepted locally (`src/utils/navigationGuard.ts`) before the projected state changes. The panel offers `Save & Continue` / `Discard & Continue` / `Cancel` and only then dispatches the transition; daemon snapshots and `navigateTo` targets are not partially applied when the guard blocks (see [Workspace Chrome and Panel Contracts](03-workspace-chrome-and-panel-contracts.md)).

### Two-Layer Hybrid State Model

To avoid hardcoding an ever-growing union of tabs and per-tab fields in the core state machine, UI state is split into a **Fixed Typed Core** and a **Declarative Surface Tree Registry**:

```text
┌─────────────────────────────────────────────────────────────────────────┐
│ CANONICAL UI STATE (Owned by oneship-daemon, persisted in oneship.db)   │
│                                                                         │
│  1. Fixed Typed Core                                                    │
│     ├── GlobalUiState                                                   │
│     │     ├── active_project_id: Option<i64>                            │
│     │     ├── chat_layout_mode: 'docked' | 'floating'                   │
│     │     └── docked_chat_width_vw: f32  (clamped [16.0, 60.0])         │
│     └── AgentChatState (per project_id)                                 │
│           ├── panel_state: 'closed' | 'open' | 'minimized'              │
│           ├── active_agent_id: Option<String>                           │
│           └── active_conversation_id: Option<String>                    │
│                                                                         │
│  2. Declarative Surface Tree (per project_id, validated vs Registry)    │
│     ├── active_node_path: String   (e.g. "electronics/oscilloscope")    │
│     └── slot_values: Map<String, Option<String>>                        │
│           (e.g. "files.explorer_scope" -> "docs")                       │
└─────────────────────────────────────────────────────────────────────────┘
```

#### Fixed Typed Core

- **`GlobalUiState`:**
  - `active_project_id`: `Option<i64>` (`NULL` only when zero projects are registered).
  - `chat_layout_mode`: `ChatLayoutMode` (`Docked` | `Floating`).
  - `docked_chat_width_vw`: floating-point percentage of viewport width; clamped to `[16.0, 60.0]`, default `28.0`.
- **`AgentChatState` (scoped per `project_id`):**
  - Switching between projects preserves each project's independent chat panel state, active agent, and active conversation.
  - Switching workspace tabs/subtabs within a project does **not** alter `AgentChatState`.

#### Declarative Surface Tree Registry (`UiSurfaceNode`)

Each workspace subsystem declares its own navigation hierarchy, state slots, and surface actions via a static/registrable `UiSurfaceNode` descriptor in the daemon:

```text
UiSurfaceNode
├── id: String                        // Segment identifier (e.g. "devlog", "todos")
├── label: String                     // Human- and assistant-readable name
├── kind: SurfaceNodeKind             // Tab | Subtab | View
├── required_capability: Option<String> // Capability key (e.g. "has_git", "has_electronics_enabled")
├── default_child_id: Option<String>  // Default child when navigating to parent path
├── children: Vec<UiSurfaceNode>      // Arbitrary depth (tab -> subtab -> subview)
├── state_slots: Vec<UiSlotDef>       // Typed local state keys owned by this subsystem
└── actions: Vec<UiSurfaceActionDef>  // Surface-scoped actions for external discovery
```

- **`UiSlotDef`:** Declares `key` (namespaced as `<subsystem>.<slot>`, e.g. `files.selected_document_path`, `files.explorer_scope`, `files.explorer_path`, `dialogue.active_artifact_id`, `project.last_tasks_node`), `value_kind` (`String | Enum(Vec<String>)`), and `default_value`.
- **Automatic Discovery:** Adding a new tab, subtab, or subsystem slot requires only registering its `UiSurfaceNode` descriptor in that subsystem. Persistence, `ts-rs` API transport, capability gating, and external controller discovery work automatically without modifying the core state machine.

### Agent Chat Panel State Machine

```text
                   OpenAgentChat(agent_id, conv_id?)
      ┌────────────────────────────────────────────────────────┐
      │                                                        ▼
  [CLOSED] ◄──────────────────────────────────────────────── [OPEN]
      ▲                 CloseAgentChat()                     │ ▲
      │                                                      │ │
      │      MinimizeAgentChat()                             ▼ │ RestoreAgentChat()
      └────────────────────────────────────────────────── [MINIMIZED]
                          CloseAgentChat()
```

- **`OpenAgentChat`:** Transitions `CLOSED | MINIMIZED | OPEN` -> `OPEN`, sets `active_agent_id`, and optionally binds `active_conversation_id`. If another agent was active, it replaces `active_agent_id` atomically.
- **`MinimizeAgentChat`:** Valid only from `OPEN` -> `MINIMIZED`. Preserves `active_agent_id` and `active_conversation_id`.
- **`RestoreAgentChat`:** Valid only from `MINIMIZED` -> `OPEN`. Preserves `active_agent_id` and `active_conversation_id`.
- **`CloseAgentChat`:** Transitions `OPEN | MINIMIZED` -> `CLOSED` and clears `active_agent_id` and `active_conversation_id`.

### Transition Actions and Invariants (Zero Silent Fallbacks)

All UI state changes pass through `dispatch_ui_action(action: UiAction)`:

1. `SelectProject { project_id: i64 }`
2. `NavigateToNode { project_id: i64, node_path: String }`
3. `SetSlotValue { project_id: i64, key: String, value: Option<String> }`
4. `OpenAgentChat { project_id: i64, agent_id: String, conversation_id: Option<String> }`
5. `MinimizeAgentChat { project_id: i64 }`
6. `RestoreAgentChat { project_id: i64 }`
7. `CloseAgentChat { project_id: i64 }`
8. `SetChatLayout { mode: ChatLayoutMode, docked_chat_width_vw: Option<f64> }`

#### Strict Validation vs. Boot-Time Sanitization

- **Explicit action validation (Tiger Style):**
  - `NavigateToNode` walks the `UiSurfaceNode` tree along `node_path`, verifies every segment's `required_capability` against the target project's capabilities, and resolves `default_child_id` recursively if a parent path is given.
  - An invalid `project_id`, unknown `node_path`, unmet `required_capability`, hidden tab, unknown `slot_values` key, invalid enum slot value, or empty `agent_id` returns a strongly-typed `UiStateError` immediately. The daemon never silently redirects an explicit command to a different project or tab.
- **Boot-time recovery sanitization (`get_ui_state`):**
  - When restoring persisted state from SQLite on startup, if a previously saved project was deleted, a plugin capability was disabled, or an ephemeral agent no longer exists, the daemon sanitizes **only the stale field** to its safe baseline (`active_node_path = "overview"`, `chat_panel_state = Closed`) and emits a structured `WARN` log entry.

### Frontend Projection Rules

- `workspaceLayoutStore.ts` acts as a reactive projection of `UiStateSnapshot` and `UiSurfaceRegistry`.
- Subsystem components (e.g. `ForgeCollabPanel.vue`, `ProjectElectronicsTab.vue`, `ProjectSkillsTab.vue`, Journal) read and write their active subtab via a shared composable (`src/composables/useUiSurfaceNode.ts`) instead of local `ref()` state.
- **Project surface nodes and `project.last_tasks_node` slot:** `ProjectManagementTab.vue` binds directly to the real daemon surface nodes `project/milestones`, `project/board`, `project/schedule`, `project/gantt`, and `project/forge` (with `required_capability = has_git`, children `issues`, `prs`, `releases`). Switching between `Tasks` and `Forge` persists and restores the last Tasks view via the `project.last_tasks_node` (`milestones | board | schedule | gantt`, default `board`) slot on `project`. Persisted legacy paths (`project/timeline`, `project/github/*`) are migrated once during DB hydration (`UiStateManager::hydrate_from_db_locked`).
- **Conversation hydration:** When `UiStateSnapshot` restores a project with `chat_panel_state` in `'open' | 'minimized'` and `active_agent_id` set, the frontend triggers `loadActiveConversation` for `(project.path, active_agent_id, active_conversation_id)` so conversation history is immediately hydrated.

## Contracts

### SQLite Schema (`oneship.db`)

All state mutations execute within a single SQLite transaction, incrementing `ui_global_state.revision` atomically.

#### `ui_global_state` (Single-Row Table, `id = 1`)

| Column | Type / Constraints |
| :--- | :--- |
| `id` | `INTEGER PRIMARY KEY CHECK (id = 1)` |
| `active_project_id` | `INTEGER REFERENCES projects(id) ON DELETE SET NULL` |
| `chat_layout_mode` | `TEXT NOT NULL DEFAULT 'docked' CHECK (chat_layout_mode IN ('docked', 'floating'))` |
| `docked_chat_width_vw` | `REAL NOT NULL DEFAULT 28.0` |
| `revision` | `INTEGER NOT NULL DEFAULT 1` |
| `updated_at_seconds` | `INTEGER NOT NULL` |

#### `ui_project_state`

| Column | Type / Constraints |
| :--- | :--- |
| `project_id` | `INTEGER PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE` |
| `active_node_path` | `TEXT NOT NULL DEFAULT 'overview'` |
| `slot_values_json` | `TEXT NOT NULL DEFAULT '{}'` |
| `chat_panel_state` | `TEXT NOT NULL DEFAULT 'closed' CHECK (chat_panel_state IN ('closed', 'open', 'minimized'))` |
| `active_agent_id` | `TEXT` (nullable) |
| `active_conversation_id` | `TEXT` (nullable) |
| `updated_at_seconds` | `INTEGER NOT NULL` |

### Daemon Commands and Events (`src-tauri/src/api.rs`, `events.rs`, `src/types/daemonApi.ts`)

| Interface | Signature / Payload | Behavior |
| :--- | :--- | :--- |
| `get_ui_registry` | `(NoArgs) -> UiSurfaceRegistry` | Returns the full `UiSurfaceNode` tree, slot definitions, and surface actions declared by all subsystems. |
| `get_ui_state` | `(NoArgs) -> UiStateSnapshot` | Returns `{ global: UiGlobalState, projects: HashMap<String, UiProjectState>, revision: u64 }`. |
| `dispatch_ui_action` | `(UiAction) -> UiStateSnapshot` | Validates, persists atomically, increments `revision`, broadcasts `ui-state-updated`, and returns the updated snapshot. |
| SSE event `/api/events` | `ui-state-updated(UiStateSnapshot)` | Broadcast on every accepted `dispatch_ui_action`. Frontend ignores snapshots where `snapshot.revision <= localRevision`. |

### Error Contract (`UiStateError`)

Typed `thiserror` variants implementing `ApiError`: `ProjectNotFound` (404), `UnknownNodePath` / `UnknownSlot` / `InvalidSlotValue` / `EmptyAgentId` (400), `CapabilityNotEnabled` / `TabHidden` / `InvalidChatTransition` (409), and `Database` / `Hydration` / `Serialization` (500).

### Registered Surface Tree (`build_surface_registry`)

Top-level `Tab` nodes in `src-tauri/src/ui_state.rs`:

- `settings` (`Settings`) — actions: `get_project_config_schema`, `set_project_config_value`
- `overview` (`Overview`)
- `project` (`Project`, `has_project_management_enabled`, default `board`) — children: `milestones`, `board`, `schedule`, `gantt`, `forge` (`has_git`, default `issues`, children `issues`, `prs`, `releases`); slot: `project.last_tasks_node` (`milestones | board | schedule | gantt`); project and forge actions
- `files` (`Explorer`) — slots: `files.explorer_scope`, `files.explorer_path`, `files.selected_document_path`; actions: `search_files`, `semantic_search`, `list_directory`, `open_file`, `set_scope`
- `knowledge` (`Graph`, `has_knowledge_graph_enabled`)
- `electronics` (`Electronics`, `has_electronics_enabled`, default `firmware`) — children: `firmware`, `oscilloscope` (slots: `electronics.oscilloscope_ch1_enabled`, `ch2_enabled`, `timebase_us`, `trigger_source`, `trigger_slope`, `trigger_sweep`), `logic`
- `design` (`Design`, `has_desigma_enabled`)
- `cad` (`CAD`, `has_solidcraft_enabled`)
- `pcb` (`PCB`, `has_pcbcraft_enabled`)
- `preview` (`Preview`, `has_preview_url`)
- `profiler` (`Profiling`, `has_profiler_enabled`)
- `rea` (`RE Lab`, `has_rea_enabled`)
- `terminal` (`Scripts`)
- `git` (`Git & Snapshots`, `has_git`)
- `devlog` (`Journal`, default `devlog`) — children: `devlog`, `todos`, `debt`
- `skills` (`Skills`, default `installed`) — children: `installed`, `explore`

## Implementation Status

Verified against the repository:

- `src-tauri/src/ui_state.rs` implements `UiStateManager`, `UiAction`, `UiSurfaceRegistry`, `UiSurfaceActionDef`, `UiStateError`, `build_surface_registry`, `hydrate_from_db_locked` (with legacy `project/timeline` and `project/github/*` migration tests), `get_ui_registry`, `get_ui_state`, and `dispatch_ui_action`: **Implemented**.
- `src-tauri/src/db.rs` defines `ui_global_state` and `ui_project_state` tables and `db_save_ui_state_atomic`: **Implemented**.
- `src-tauri/src/events.rs` and `src/utils/daemonEvents.ts` use the hyphenated event name `ui-state-updated` (note: legacy spec prose wrote `ui_state_updated`): **Implemented**.
- `src/stores/workspaceLayoutStore.ts`, `src/composables/useUiSurfaceNode.ts`, and `src/utils/navigationGuard.ts` implement reactive projection, monotonic `localRevision` filtering, and dirty-state guarding: **Implemented** (note: `workspaceLayoutStore.ts` retains `localStorage` initialization defaults for `dockedChatWidthVw` and `chatLayoutMode` prior to snapshot hydration).
- `src-tauri/src/ui_state.rs` uses `f64` for `docked_chat_width_vw` (legacy spec prose wrote `f32`): **Implemented**.

## Related Specs

- [Workspace Chrome and Panel Contracts](03-workspace-chrome-and-panel-contracts.md)
- [Project Settings and Overview Dashboard](05-project-settings-and-overview-dashboard.md)
- [Configuration Schema and Cascade](../01-architecture/02-configuration-schema-and-cascade.md)

Supersedes legacy: `35_ui_state_machine_and_session_persistence.md`
