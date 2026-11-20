# Spec 20: Live Artifacts, Project Scratchpad & Documentation Workspace
**Document Version:** 0.3.0
**Domain:** Knowledge Management, Document Engine, Live Artifacts, Scratchpad Lifecycle
**Maintainer:** ONE-SHIP / Omarchy Development Team

---

## 1. Overview & Motivation

In previous builds of ONE-SHIP, agent artifacts (plans, code snippets, diffs, analysis tables) were treated as transient text payloads encapsulated inside single-turn MCP `AgentResponse` messages and saved in chat history. This paradigm suffered from major operational limitations:
1. **Redundant Content Reproduction:** Modifying an artifact required the agent to rewrite the entire text document into subsequent turn payloads, incurring token overhead and cluttering the chat log.
2. **Disconnected from Project Reality:** Artifacts did not exist as real, inspectable, or version-controlled files in the developer's filesystem.
3. **No Central Knowledge Exploration:** Developers had no way to explore existing project specifications, guides, or scratchpad drafts from within the dialogue overlay without manually opening an external editor.

Spec 20 elevates artifacts into **first-class, file-backed live documents** residing directly inside the project directory, automatically refreshed on disk modification, and managed across two distinct lifecycle zones: `docs/` and `scratchpad/`.

```
┌────────────────────────────────────────────────────────────────────────┐
│                        PROJECT WORKSPACE SYSTEM                        │
│                                                                        │
│   docs/                                scratchpad/                     │
│   (Stable, permanent specs,            (Ephemeral working drafts,      │
│    ADRs, architectural guides)          conversation-scoped research)  │
│         ▲                                    ▲                         │
│         │ read / write                       │ read / write            │
│   ┌─────┴────────────────────────────────────┴─────┐                   │
│   │              AI Agent in Pane / agy            │                   │
│   │  - Writes plan to disk                         │                   │
│   │  - Invokes `post_agent_response` with path     │                   │
│   │  - Incremental edits via file tools            │                   │
│   └──────────────────────┬─────────────────────────┘                   │
│                          │ MCP post_agent_response                     │
│                          ▼                                             │
│   ┌────────────────────────────────────────────────┐                   │
│   │         ONE-SHIP Daemon & Watcher           │                   │
│   │  - Binds live artifact to project relative path│                   │
│   │  - Emits file modification refresh events      │                   │
│   └──────────────────────┬─────────────────────────┘                   │
│                          │ Tauri Event / IPC                           │
│                          ▼                                             │
│   ┌────────────────────────────────────────────────┐                   │
│   │      Artifact & Document Panel (Column 3)      │                   │
│   │  - Mode 1: [Live Artifact] (auto-refreshing)   │                   │
│   │  - Mode 2: [Browse Docs] (docs/ & scratchpad/) │                   │
│   │  - Rich Markdown / Code / Multi-format reader  │                   │
│   └────────────────────────────────────────────────┘                   │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Project Directory Architecture

Each tracked project contains two dedicated directories:

### 2.1 `docs/` (Stable Knowledge Base)
- **Purpose:** Permanent project specifications, architecture documents, runbooks, schemas, and developer manuals.
- **Git Strategy:** Fully versioned in the project repository.
- **Scaffolding Lifecycle:** During Phase 1 initialization, `docs/` is **NOT** eagerly or automatically created. If documentation is absent or incomplete, the initializer raises an initialization flag (`flag_type = 'docs'`). The `docs/` hierarchy is scaffolded in Phase 2 only after the developer selects their desired Docstyle (e.g., SDD, Lean, Contract-First, Test-Anchored) in a dedicated resolution session.
- **Access Model:** Read/write for agents (when requested) and humans.
- **UI Exposure:** Browsable tree in the Artifact Panel under `docs/`.

### 2.2 `scratchpad/` (Ephemeral Agent Workspace)
- **Purpose:** Transient working space for agents: iterative plans, draft breakdowns, debug session notes, profiler flamegraph exports, and intermediate research summaries.
- **Lifecycle & Retention Policy:**
  - Auto-created at project root upon first agent execution (`ensure_project_scratchpad_dir`).
  - Recommended inclusion in `.gitignore` to avoid polluting repository commit history.
  - Covered by ONE-SHIP's hidden Git safety snapshots (Spec 16), allowing rollback and auditability without dirtying git status.
  - **File Explorer & Optional Auto-Pruning:** Because `scratchpad/` is a standard directory visible directly in the workspace `Explorer` tab (`ProjectWorkspace.vue`), developers can select, rename, or delete scratchpad files using standard explorer actions without needing a dedicated purge button. Additionally, an optional configuration field (`vcs.scratchpad_retention_days`, default `0` = disabled) controls automatic age-based pruning when enabled.

---

## 3. Agent Operational Protocol

### 3.1 Initial Prompt Injection (`supervisor.rs`)
Agents are instructed in their system prompt:
1. When generating extensive plans, architecture specs, tables, or working notes, write them directly to a file on disk:
   - `scratchpad/<name>.md` for working drafts and temporary scratchpad notes.
   - `docs/<name>.md` for permanent project specifications.
2. In `post_agent_response`, pass the relative path in the `file_path` field (e.g. `"file_path": "scratchpad/migration_plan.md"`) or inline companion content in `artifact`.
3. Subsequent adjustments are made via standard file edit tools (`replace_file_content`, `write_to_file`). The agent does not re-dump full document text into conversational turns.

### 3.2 Backend Binding & Live Refresh (`project-files-changed`)
1. When `post_agent_response` receives a `file_path`, it resolves the canonical path inside the project root.
2. A per-project `notify` filesystem watcher (`ensure_project_files_watcher` in `kopeng/documents.rs`) emits the `project-files-changed` daemon SSE event on disk modifications, updating open documents in place and detecting disk vs. unsaved-draft conflicts.

---

## 4. UI Design: Column 3 (`ArtifactPanel.vue`)

### 4.1 Unified Reader Header
The header of Column 3 provides mode toggling between the unified reading surface and terminal logging:
- `[Reader]`: Unified, full-width reader canvas displaying the active document or artifact.
- `[Terminal Log]`: Displays live rolling output of the active agent's supervised process, with auto-polling, error filter toggling (`All` vs `Errors Only`), and quick copy action.

In Reader mode, a dedicated selector button displays the current document/artifact title, type icon, category badge, and toggles the Three-Tier Navigator Menu.

### 4.2 Three-Tier Categorized Artifact & Document Navigator
The navigator overlay acts as a centralized switcher with integrated search filtering:
1. **Active Scratchpads (Temporary):** Scratchpad files (`scratchpad/*.md`) generated or touched in the active conversation topic.
2. **Project Docs (`docs/` only - Infinite Recursive Tree):** Permanent repository documentation rendered via recursive tree components (`DocTreeNode.vue`), supporting arbitrarily deep directory nesting (e.g., `docs/specs/subcategories/...`) with expand/collapse states and `[● MODIFIED]` badges.
3. **Ephemeral Artifacts (History):** In-memory markdown tables, execution plans, diffs, or code snippets created during conversation turns, tagged with `[EPHEMERAL]`.

Selecting any item across any of the three tiers immediately loads the document into the single full-width reading canvas and dismisses the navigator overlay.

### 4.3 Multi-Format Reader
- **Single Canvas Surface:** Eradicates redundant lateral split viewers; the reader spans the entire width of Column 3.
- **Markdown (`.md`):** Formatted Markdown renderer supporting headers, bullet lists, markdown tables, blockquotes, and code blocks.
- **Code / Text / Diffs (`.rs`, `.ts`, `.json`, `.diff`, etc.):** Clean monospaced view with line numbers and copy-to-clipboard action.
- **Auto-Refresh:** Automatically reloads modified content from disk via background polling.

---

## 5. Discrete Topical Conversations & Turn Audit Trail

### 5.1 Architecture & Context Hygiene
To prevent context degradation and token bloat from accumulated messages across multiple unrelated tasks, conversations are partitioned into discrete topical sessions per agent in SQLite (`agent_conversations` and `agent_chat_turns` tables).

```
┌────────────────────────────────────────────────────────┐
│               SQLite Turn Persistence Model            │
│                                                        │
│  agent_conversations (id, project_id, agent_id, topic, │
│                       status, artifacts_touched)       │
│         ▲                                              │
│         │ 1:N CASCADE                                  │
│  agent_chat_turns (id, role, dialogue, artifact_*,     │
│                    is_ephemeral, terminal_snapshot,    │
│                    timestamp)                          │
└────────────────────────────────────────────────────────┘
```

### 5.2 MCP Primitive: `switch_conversation_topic`
Agents can programmatically transition or resume discrete conversations without contaminating active context. An optional transition `summary` is stored as a system turn in the target topic.

### 5.3 Full Turn Audit & Terminal Snapshotting
Every agent turn samples the latest 60 lines of the agent's supervisor output buffer (`get_terminal_output`), persisting an immutable audit trail in `agent_chat_turns.terminal_snapshot`.

---

---

## 6. Inline Document Commenting & Agent Feedback Loop (Phase 3)

### 6.1 Database Persistence & Data Schema
Document review comments are persisted in the centralized SQLite database under the `document_comments` table:

```sql
CREATE TABLE IF NOT EXISTS document_comments (
    id TEXT PRIMARY KEY,
    project_id INTEGER NOT NULL,
    file_path TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    selected_text TEXT NOT NULL,
    comment_text TEXT NOT NULL,
    author TEXT NOT NULL DEFAULT 'user',
    status TEXT NOT NULL DEFAULT 'open', -- 'open' | 'resolved' | 'dismissed'
    created_at INTEGER NOT NULL,
    resolved_at INTEGER,
    resolved_by TEXT,
    FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_doc_comments_file ON document_comments(project_id, file_path, status);
```

### 6.2 Reader Selection, Line Calculation & Floating Popover
1. In `ArtifactPanel.vue`, `handleTextSelection` intercepts mouse selection events within the document reader body.
2. The exact line range `L<start>-L<end>` is calculated based on character offset and newline counts within `displayContent.content`.
3. A floating brutalist popover anchors to the selection coordinates, providing:
   - Line range header badge.
   - Quoted text snippet preview.
   - Multi-line comment textarea (`AppTextarea`).
   - OpenDictate Voice Mic button triggering `reserve_voice_capture` and listening to `opendictate_session_finished` over D-Bus to append transcribed speech.
   - `[Save]` and `[Cancel]` buttons.
4. Saving dispatches the daemon command `create_document_comment` and opens the document's slide-over Comments Drawer.

### 6.3 Structured Review Context Packaging for Chat
1. The developer attaches open review files to the chat conversation via the `[+ Attach to Chat]` topbar action.
2. Attached files are rendered in the **Attached Review Files Tray** in `ConversationPanel.vue`.
3. When the user sends a prompt, the message payload is formatted into an atomic structured review block:
   ```markdown
   [REVIEW CONTEXT: N FILE(S) ATTACHED]

   ### File: <rel_path>
   - [Comment #<id>] Lines L<start>-L<end>: "<selected_snippet>"
     Feedback: "<comment_text>"

   ---
   User Note: <user_input_text>
   ```
4. Attached files are automatically detached after prompt dispatch.

### 6.4 Autonomous Agent Resolution via MCP
Agents inspect and resolve review feedback using specialized MCP tools:
- `list_document_comments`: Queries open or resolved comments for a specific project file.
- `resolve_document_comment`: Marks comment as resolved, emitting the `DocumentCommentResolved` SSE event to update UI indicators in real-time.

---

## 7. Workspace File & Directory Operations in Explorer (Phase 4)

### 7.1 Scope Selector: `Docs` vs `Codebase`
The Column 3 file explorer supports two operational scopes:
- `Docs`: Scans strictly `docs/` and `scratchpad/`.
- `Codebase`: Recursively scans the entire project repository, ignoring build artifacts, virtual environments, and VCS internal directories (`.git`, `target`, `node_modules`, `dist`, `.gemini`, `brain`, `package-lock.json`, `Cargo.lock`, etc.).

### 7.2 Path Sandboxing & Security
All file and directory operations enforce strict path sandboxing via `sanitize_project_rel_path`:
- Rejects relative path traversal sequences (`..`).
- Resolves and canonicalizes target paths.
- Guarantees that no file creation, copy, cut, or deletion escapes the project root directory.

### 7.3 Multi-Item Selection & Clipboard Operations
- **Multi-Select Mode:** Activated via `[Select]` toolbar button, rendering selection checkboxes (`CheckSquare` / `Square`) alongside each file and directory row.
- **Docked Action Bar:** Appears when 1+ items are selected, providing item count, `[Select All]`, `[Clear]`, `[Copy]`, `[Cut]`, and `[Delete]`.
- **Clipboard (`explorerClipboard`):** Tracks selected paths and operation type (`copy` vs `cut`).
- **Paste Action:** `[📋 Paste (N)]` toolbar button appears when items exist in clipboard, copying or moving items into the active `explorerPath`.
- **Modals:**
  - `Create Folder Modal`: Creates directories in the active folder via `create_project_directory`.
  - `Batch Delete Modal`: Displays item preview and permanent deletion warning before invoking `delete_project_items`.

---

## 8. In-Place Document & Code Editor (Phase 5)

### 8.1 Dual-Mode Surface Workflow
The Column 3 Reader & Editor surface operates in two continuous modes:
1. **View / Review Mode (Default):**
   - **Markdown (`.md`):** Formatted GFM HTML rendering with syntax-highlighted code blocks and mouse-drag selection hooked into the inline review commenting popover.
   - **Code & Config (`.rs`, `.ts`, `.vue`, `.json`, `.toml`, `.py`, `.c`/`.cpp`, `.go`, `.sh`, `.yaml`, `.sql`, `.xml`, etc.):** Readonly CodeMirror 6 surface with line numbers gutter, syntax linting, symbol navigation, and right-click context menu.
2. **Edit Mode (`[Edit]` / `Ctrl+E`):**
   - Multi-file open tabs (`openDocuments`) in the top bar with horizontal wheel scroll, relative paths (root files without prefix; `.` as symbolic root in breadcrumbs), dirty dot indicator, and per-tab mode persistence.
   - Read/Edit mode toggle positioned on the right side of the action toolbar alongside the **Line Wrap / Horizontal Scroll** toggle (`WrapText`).
   - Interactive full-height CodeMirror 6 canvas (`AppCodeEditor.vue`).
   - Brutalist theme styling using Spec 13 surface and semantic variables (`--ui-bg-*`, `--accent-color`, `--border-color`, `--status-*`).
   - Multi-language grammar support: Lezer parsers (`@codemirror/lang-rust`, `javascript`/`typescript`, `python`, `cpp`, `html`, `css`, `json`, `markdown`), dedicated SFC parsers (`@codemirror/lang-vue`, `@replit/codemirror-lang-svelte`, `@fazelstudio/codemirror-lang-astro`), plus `@codemirror/legacy-modes` stream modes (`toml`, `shell`, `yaml`, `go`, `dockerfile`, `sql`, `xml`, `ruby`, `properties`, `diff`).
   - Real-time local syntax validation via `@codemirror/lint` (`lintGutter` + `linter` using full-document `language.parser.parse(text)` when incremental `syntaxTreeAvailable` is incomplete, plus strict `JSON.parse` and offset deduplication), surfaced in the editor footer (`N SYNTAX ERR`).
   - Docked inline side drawers (Outline, Graph Insights with inline chunk/file preview, and Comments) laid out side-by-side with the editor so code is never obscured.
   - Non-disruptive right-click context menu (`AppContextMenu`) for **Ask Agent**, **Refactor with Agent**, **Add Review Comment**, **Graph Insights**, and **Go to Definition** (`F12` / `Ctrl+Click`), plus Tree-sitter/Markdown hierarchical **Outline** tree (`Ctrl+Shift+O`) with collapsible sublevels, subtle right-aligned line numbers, and active cursor tracking.
   - Save via `[Save]` topbar action or `Ctrl+S` / `Cmd+S` keyboard shortcut.
   - Dirty tracking with indicator `●` and unsaved changes discard action.

### 8.2 Sandboxed Document Saving (`save_project_document`)
- Direct atomic writes to disk via the daemon command `save_project_document`.
- Strict path canonicalization and isolation verification via `sanitize_project_rel_path`.
- Real-time synchronization with active artifact cache and file trees.

---

## 9. Implementation Checklist

- [x] **Spec & Documentation:** Spec 20 created, maintained, and synchronized.
- [x] **Phase 1: Foundation:**
  - [x] Auto-create `scratchpad/` directory on project init / launch.
  - [x] Backend daemon commands: `get_project_documents`, `read_project_document`.
  - [x] Injected agent prompt guidance for `scratchpad/` and `docs/`.
  - [x] Rich Markdown formatting in `ArtifactPanel.vue`.
  - [x] Triple-mode header switcher (`Live Artifact`, `Docs & Scratchpad`, `Terminal Log`).
  - [x] Three-tier artifact selector (*This Conversation*, *Project Docs* with `[● MODIFIED]`, *Ephemeral Artifacts*).
  - [x] SQLite conversation persistence and turn audit trail (`agent_conversations`, `agent_chat_turns`).
  - [x] MCP primitive `switch_conversation_topic` with UI event dispatch.
  - [x] Pagination history loader (`loadOlderMessages`).
  - [x] Live file refresh polling and terminal buffer polling.
- [x] **Phase 2: Scratchpad Lifecycle & Retention:**
  - [x] `scratchpad/` managed directly through standard File Explorer CRUD (`ProjectWorkspace.vue`) without redundant special-purpose purge buttons.
  - [x] Optional `vcs.scratchpad_retention_days` configuration setting (default `0` = disabled).
- [x] **Phase 3: Inline Document Commenting & Voice Review Loop:**
  - [x] SQLite `document_comments` table and CRUD IPC commands.
  - [x] Selection highlight detection & line calculation (`L<start>-L<end>`) in reader canvas.
  - [x] Floating comment popover with OpenDictate D-Bus voice dictation integration.
  - [x] Active file Comments Drawer with resolve and delete actions.
  - [x] Structured review context prompt packaging in `ConversationPanel.vue`.
  - [x] Agent MCP tools `list_document_comments` and `resolve_document_comment`.
- [x] **Phase 4: Explorer File & Folder Operations:**
  - [x] Codebase vs Docs scope switcher with ignore filters.
  - [x] Multi-item select mode with checkboxes and docked action bar.
  - [x] Folder creation modal and `create_project_directory` command.
  - [x] Clipboard copy, cut, and paste (`copy_project_items`, `move_project_items`).
  - [x] Batch deletion modal and `delete_project_items` command with path sandboxing.
- [x] **Phase 5: In-Place Document & Code Editor:**
  - [x] Backend daemon command `save_project_document` with sandboxing.
  - [x] CodeMirror 6 integration (`AppCodeEditor.vue`) with dynamic syntax highlighting (`@codemirror/lang-*` + `@codemirror/legacy-modes`).
  - [x] Real-time syntax diagnostics (`@codemirror/lint` over Lezer AST & JSON) and cursor/diagnostic footer indicators.
  - [x] Right-click editor context menu (`AppContextMenu`) for non-disruptive review commenting, Ask Agent, Refactor with Agent, Graph Insights, and Go to Definition.
  - [x] Tree-sitter Outline drawer (`get_file_symbols`, `Ctrl+Shift+O`) and indexed Symbol Definition navigation (`resolve_symbol_definition`, `F12` / `Ctrl+Click`).
  - [x] Brutalist dark theme integration for CodeMirror.
  - [x] View/Edit mode switcher in Reader topbar (`[Edit]` / `[View]`, `Ctrl+E`).
  - [x] Dirty state tracking (`●`), `[Save]` (`Ctrl+S`), and `[Discard]` actions.
  - [x] Line numbers gutter in Read Mode for monospaced code files.

