> [!WARNING]
> **LEGACY SPECIFICATION:** Superseded by [07-ui/04-ui-state-machine-and-session-persistence.md](../../specs/07-ui/04-ui-state-machine-and-session-persistence.md). Preserved for historical provenance only.

# Spec 35: UI State Machine, Daemon Synchronization & Session Persistence
**Document Version:** 0.2.3  
**Domain:** Declarative UI Surface Tree, Agent Chat Panel Lifecycle, Bidirectional Daemon-Frontend Sync, Exact Session Recovery  
**Maintainer:** ONE-SHIP / Omarchy Development Team

---

## 1. Architectural Purpose & Scope

ONE-SHIP decouples **Workspace Navigation** (declarative surface tree) from **Agent Chat Lifecycle** (finite state machine) and moves the canonical UI state into `oneship-daemon`:

1. **Single Source of Truth in Daemon (Spec 12 §7.2):** The daemon owns the canonical UI state in memory and persists it to SQLite (`oneship.db`). The Vue/Tauri frontend acts strictly as a reactive projection of the daemon snapshot and dispatches explicit transition actions.
2. **Exact Session Recovery:** Closing and reopening the desktop interface restores the exact active project, workspace tab/subtab path, subsystem slot values (e.g., open file, explorer scope, active artifact), and agent chat panel state (`closed` / `open` / `minimized`, active agent, active conversation).
3. **External Control Readiness:** Moving state ownership and transition validation into the daemon allows external controllers (such as a future Global Voice Controller) to inspect valid navigation paths for any project and execute deterministic UI transitions via `/api/events`.
4. **Removal of Legacy `activeWorkspace`:** The obsolete `'dashboard' | 'workspace'` switcher is removed; ONE-SHIP operates exclusively in the project workspace with a global project selector.
5. **Unsaved-Change Guard (Frontend):** Navigation transitions that leave an editable panel with dirty state are intercepted locally (`src/utils/navigationGuard.ts`) before the projected state changes. The panel offers Save/Discard/Cancel and only then dispatches the transition; daemon snapshots and `navigateTo` targets are not partially applied when the guard blocks. See Spec 13 §3.13.

---

## 2. Two-Layer Hybrid State Model

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

### 2.1 Fixed Typed Core

* **`GlobalUiState`:**
  * `active_project_id`: `Option<i64>` (`NULL` only when zero projects are registered).
  * `chat_layout_mode`: `ChatLayoutMode` (`Docked` | `Floating`).
  * `docked_chat_width_vw`: `f32` (percentage of viewport width; clamped to `[16.0, 60.0]`, default `28.0`).
* **`AgentChatState` (scoped per `project_id`):**
  * Switching between projects preserves each project's independent chat panel state, active agent, and active conversation.
  * Switching workspace tabs/subtabs within a project does **not** alter `AgentChatState`.

### 2.2 Declarative Surface Tree Registry (`UiSurfaceNode`)

Each workspace subsystem declares its own navigation hierarchy and state slots via a static/registrable `UiSurfaceNode` descriptor in the daemon:

```text
UiSurfaceNode
├── id: String                        // Segment identifier (e.g. "devlog", "todos")
├── label: String                     // Human- and assistant-readable name
├── kind: SurfaceNodeKind             // Tab | Subtab | View
├── required_capability: Option<String> // Capability key (e.g. "has_git", "has_electronics_enabled")
├── default_child_id: Option<String>  // Default child when navigating to parent path
├── children: Vec<UiSurfaceNode>      // Arbitrary depth (tab -> subtab -> subview)
└── state_slots: Vec<UiSlotDef>       // Typed local state keys owned by this subsystem
```

* **`UiSlotDef`:** Declares `key` (namespaced as `<subsystem>.<slot>`, e.g. `files.selected_document_path`, `files.explorer_scope`, `files.explorer_path`, `dialogue.active_artifact_id`), `value_kind` (`String | Enum(Vec<String>)`), and `default_value`.
* **Automatic Discovery:** Adding a new tab, subtab, or subsystem slot requires only registering its `UiSurfaceNode` descriptor in that subsystem. Persistence, `ts-rs` API transport, capability gating, and external controller discovery work automatically without modifying the core state machine.

---

## 3. Agent Chat Panel State Machine

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

* **`OpenAgentChat`:** Transitions `CLOSED | MINIMIZED | OPEN` -> `OPEN`, sets `active_agent_id`, and optionally binds `active_conversation_id`. If another agent was active, it replaces `active_agent_id` atomically.
* **`MinimizeAgentChat`:** Valid only from `OPEN` -> `MINIMIZED`. Preserves `active_agent_id` and `active_conversation_id`.
* **`RestoreAgentChat`:** Valid only from `MINIMIZED` -> `OPEN`. Preserves `active_agent_id` and `active_conversation_id`.
* **`CloseAgentChat`:** Transitions `OPEN | MINIMIZED` -> `CLOSED` and clears `active_agent_id` and `active_conversation_id`.

---

## 4. Transition Actions & Invariants (Zero Silent Fallbacks)

All UI state changes pass through `dispatch_ui_action(action: UiAction)`:

1. `SelectProject { project_id: i64 }`
2. `NavigateToNode { project_id: i64, node_path: String }`
3. `SetSlotValue { project_id: i64, key: String, value: Option<String> }`
4. `OpenAgentChat { project_id: i64, agent_id: String, conversation_id: Option<String> }`
5. `MinimizeAgentChat { project_id: i64 }`
6. `RestoreAgentChat { project_id: i64 }`
7. `CloseAgentChat { project_id: i64 }`
8. `SetChatLayout { mode: ChatLayoutMode, docked_chat_width_vw: Option<f32> }`

### 4.1 Strict Validation vs. Boot-Time Sanitization

* **Explicit Action Validation (Tiger Style):**
  * `NavigateToNode` walks the `UiSurfaceNode` tree along `node_path`, verifies every segment's `required_capability` against the target project's capabilities, and resolves `default_child_id` recursively if a parent path is given.
  * An invalid `project_id`, unknown `node_path`, unmet `required_capability`, unknown `slot_values` key, or unknown `agent_id` returns a strongly-typed `UiStateError` immediately. The daemon never silently redirects an explicit command to a different project or tab.
* **Boot-Time Recovery Sanitization (`get_ui_state`):**
  * When restoring persisted state from SQLite on startup, if a previously saved project was deleted, a plugin capability was disabled, or an ephemeral agent no longer exists, the daemon sanitizes **only the stale field** to its safe baseline (`active_node_path = "overview"`, `chat_panel_state = Closed`) and emits a structured `WARN` log entry.

---

## 5. Database Schema (`oneship.db`)

### 5.1 `ui_global_state` (Single-Row Table, `id = 1`)
* `id`: `INTEGER PRIMARY KEY CHECK (id = 1)`
* `active_project_id`: `INTEGER REFERENCES projects(id) ON DELETE SET NULL`
* `chat_layout_mode`: `TEXT NOT NULL DEFAULT 'docked'` (`'docked'` | `'floating'`)
* `docked_chat_width_vw`: `REAL NOT NULL DEFAULT 28.0`
* `revision`: `INTEGER NOT NULL DEFAULT 1`
* `updated_at_seconds`: `INTEGER NOT NULL`

### 5.2 `ui_project_state`
* `project_id`: `INTEGER PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE`
* `active_node_path`: `TEXT NOT NULL DEFAULT 'overview'`
* `slot_values_json`: `TEXT NOT NULL DEFAULT '{}'`
* `chat_panel_state`: `TEXT NOT NULL DEFAULT 'closed'` (`'closed'` | `'open'` | `'minimized'`)
* `active_agent_id`: `TEXT` (nullable)
* `active_conversation_id`: `TEXT` (nullable)
* `updated_at_seconds`: `INTEGER NOT NULL`

All state mutations execute within a single SQLite transaction, incrementing `ui_global_state.revision` atomically.

---

## 6. Daemon IPC & Event Protocol (Spec 12 §7.2)

### 6.1 Daemon Commands (`src-tauri/src/api.rs` & `src/types/daemonApi.ts`)
* `get_ui_registry(NoArgs) -> UiSurfaceRegistry`: Returns the full `UiSurfaceNode` tree and slot definitions declared by all subsystems.
* `get_ui_state(NoArgs) -> UiStateSnapshot`: Returns `{ global: UiGlobalState, projects: HashMap<i64, UiProjectState>, revision: u64 }`.
* `dispatch_ui_action(UiAction) -> UiStateSnapshot`: Validates, persists, increments `revision`, broadcasts `ui_state_updated`, and returns the updated snapshot.

### 6.2 SSE Event (`/api/events`, `src-tauri/src/events.rs`)
* `ui_state_updated(UiStateSnapshot)`: Broadcast on every accepted `dispatch_ui_action`. The frontend ignores snapshots with `snapshot.revision <= local_revision` to guarantee monotonic ordering under concurrent user clicks and external controller actions.

### 6.3 Frontend Projection Rules
* `workspaceLayoutStore.ts` becomes a reactive projection of `UiStateSnapshot` and `UiSurfaceRegistry`.
* Subsystem components (e.g., `GithubCollabPanel.vue`, `ProjectElectronicsTab.vue`, `ProjectSkillsTab.vue`, `Journal`) read and write their active subtab via a shared composable (`useUiSurfaceNode(subsystemId)`) instead of local `ref()` state.
* **Project Surface Nodes & `project.last_tasks_node` Slot:** `ProjectManagementTab.vue` binds directly to the real daemon surface nodes `project/board`, `project/schedule`, `project/gantt`, and `project/forge` (with `required_capability = has_git`, children `issues`, `prs`, `releases`). Switching between `Tasks` and `Forge` persists and restores the last Tasks view via the `project.last_tasks_node` (`board | schedule | gantt`, default `board`) slot on `project`. Persisted legacy paths (`project/timeline`, `project/github/*`) are migrated once during DB hydration (`UiStateManager::hydrate_from_db_locked`).
* **Planned removal:** Spec 39 defines the real `project/schedule`, `project/gantt` and `project/forge` nodes, the persisted-path migration and the `project.last_tasks_node` slot. When that lands, the frontend-local mapping and the `localStorage` key are deleted; this bullet then documents only the transition.
* When `UiStateSnapshot` restores a project with `chat_panel_state` in `'open' | 'minimized'` and `active_agent_id` set, the frontend triggers `loadActiveConversation` for `(project.path, active_agent_id, active_conversation_id)` so conversation history is immediately hydrated.
